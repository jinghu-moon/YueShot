use gpui::{ListGeometry, Pixels, uniform_list_range};
use std::ops::Range;
#[cfg(test)]
use std::rc::Rc;

/// The existing uniform arithmetic, known-size prefix sums, or native SumTree.
/// There is one geometry owner per backend, shared by presentation and queries.
#[derive(Clone)]
pub(super) enum Geometry {
    Fixed { count: usize, extent: f64, gap: f64 },
    Known(KnownGeometry),
    Measured(ListGeometry),
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;
    #[test]
    fn known_prefix_keeps_gap_outside_item_content_and_does_not_append_a_trailing_gap() {
        let geometry = Geometry::known(Rc::from([px(10.), px(30.), px(5.), px(50.)]), 7.);
        assert_eq!(geometry.total(), 116.);
        assert_eq!(geometry.item(2), Some((54., 5., true)));
        assert_eq!(geometry.range(11., 5.), 1..1);
        assert_eq!(geometry.range(17., 1.), 1..2);
        assert_eq!(geometry.range(54., 5.), 2..3);
    }
    #[test]
    fn fixed_geometry_uses_constant_metadata_and_f64_local_coordinates() {
        let geometry = Geometry::Fixed {
            count: 1_000_000_000,
            extent: 20.,
            gap: 1.,
        };
        assert_eq!(geometry.total(), 20_999_999_999.);
        assert_eq!(
            geometry.range(20_999_999_970., 20.),
            999_999_998..1_000_000_000
        );
    }
}

impl Geometry {
    #[cfg(test)]
    pub fn known(extents: Rc<[Pixels]>, gap: f64) -> Self {
        Self::Known(KnownGeometry::new(extents.iter().copied(), gap))
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Fixed { count, .. } => *count,
            Self::Known(tree) => tree.len(),
            Self::Measured(tree) => tree.len(),
        }
    }
    pub fn total(&self) -> f64 {
        match self {
            Self::Fixed { count, extent, gap } => {
                *count as f64 * extent + count.saturating_sub(1) as f64 * gap
            }
            Self::Known(tree) => tree.total(),
            Self::Measured(tree) => tree.precise_total_extent(),
        }
    }
    pub fn estimated(&self) -> bool {
        matches!(self, Self::Measured(tree) if tree.is_estimated())
    }
    pub fn item(&self, index: usize) -> Option<(f64, f64, bool)> {
        if index >= self.len() {
            return None;
        }
        Some(match self {
            Self::Fixed { extent, gap, .. } => (index as f64 * (extent + gap), *extent, true),
            Self::Known(tree) => tree.item(index)?,
            Self::Measured(tree) => {
                let item = tree.item(index)?;
                (
                    item.precise_start(),
                    item.extent().into(),
                    item.is_measured(),
                )
            }
        })
    }
    pub fn range(&self, start: f64, extent: f64) -> Range<usize> {
        if extent <= 0.0 || self.len() == 0 {
            return 0..0;
        }
        match self {
            Self::Fixed {
                count,
                extent: item_extent,
                gap,
            } => uniform_list_range(*count, *item_extent, *gap, start, extent),
            Self::Known(tree) => tree.range(start, extent),
            Self::Measured(tree) => tree.precise_range(start, extent),
        }
    }
    pub fn items(&self, range: Range<usize>) -> impl Iterator<Item = (f64, f64, bool)> + '_ {
        let mut known = match self {
            Self::Known(tree) => Some(tree.items(range.clone())),
            _ => None,
        };
        range.map_while(move |ix| match &mut known {
            Some(items) => items.next(),
            None => self.item(ix),
        })
    }
    pub fn index_at(&self, offset: f64) -> usize {
        match self {
            Self::Fixed { extent, gap, count } => {
                ((offset.max(0.0) / (extent + gap)).floor() as usize).min(count.saturating_sub(1))
            }
            Self::Known(tree) => tree.index_at(offset),
            Self::Measured(tree) => tree
                .precise_range(offset, 0.01)
                .start
                .min(tree.len().saturating_sub(1)),
        }
    }
}

/// Persistent extent index with compact leaf blocks. Actual f32 extents remain
/// independent of f64 offsets, so small extents beside huge values stay exact.
#[derive(Clone)]
pub(super) struct KnownGeometry {
    tree: sum_tree::SumTree<KnownBlock>,
    pub gap: f64,
    valid: bool,
    measurement_epoch: u64,
}

const BLOCK_CAPACITY: usize = 128;
#[derive(Clone)]
struct KnownBlock {
    values: std::sync::Arc<BlockValues>,
    measurement_epoch: u64,
}
impl KnownBlock {
    fn values_for_write(&mut self, epoch: u64) -> &mut BlockValues {
        let values = std::sync::Arc::make_mut(&mut self.values);
        if self.measurement_epoch != epoch {
            for value in &mut values.sizes[..values.len] {
                *value = -value.abs();
            }
            self.measurement_epoch = epoch;
        }
        values
    }
}
#[derive(Clone)]
struct BlockValues {
    // The sign bit records measured/estimated without another per-item array.
    sizes: [f32; BLOCK_CAPACITY],
    ends: [f64; BLOCK_CAPACITY],
    len: usize,
}
impl Default for BlockValues {
    fn default() -> Self {
        Self {
            sizes: [0.; BLOCK_CAPACITY],
            ends: [0.; BLOCK_CAPACITY],
            len: 0,
        }
    }
}
impl BlockValues {
    fn push(&mut self, extent: Pixels, measured: bool) {
        let value = f32::from(extent);
        self.sizes[self.len] = if measured { value } else { -value };
        self.ends[self.len] =
            self.len.checked_sub(1).map_or(0., |ix| self.ends[ix]) + f64::from(value);
        self.len += 1;
    }
    fn item(&self, ix: usize) -> (f64, f64, bool) {
        (
            ix.checked_sub(1).map_or(0., |ix| self.ends[ix]),
            f64::from(self.sizes[ix].abs()),
            self.sizes[ix].is_sign_positive(),
        )
    }
    fn bound(&self, offset: f64, gap: f64, right: bool) -> usize {
        let (mut low, mut high) = (0, self.len);
        while low < high {
            let mid = (low + high) / 2;
            let end = self.ends[mid] + (mid + 1) as f64 * gap;
            if end < offset || (right && end == offset) {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low
    }
}

#[derive(Clone, Default, Debug)]
struct ExtentSummary {
    count: usize,
    extent: f64,
}
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Count(usize);
struct Position {
    offset: f64,
    gap: f64,
}
impl sum_tree::Item for KnownBlock {
    type Summary = ExtentSummary;
    fn summary(&self, _: ()) -> ExtentSummary {
        ExtentSummary {
            count: self.values.len,
            extent: self.values.ends[self.values.len - 1],
        }
    }
}
impl sum_tree::ContextLessSummary for ExtentSummary {
    fn zero() -> Self {
        Self::default()
    }
    fn add_summary(&mut self, other: &Self) {
        self.count += other.count;
        self.extent += other.extent;
    }
}
impl<'a> sum_tree::Dimension<'a, ExtentSummary> for Count {
    fn zero(_: ()) -> Self {
        Self::default()
    }
    fn add_summary(&mut self, other: &'a ExtentSummary, _: ()) {
        self.0 += other.count;
    }
}
impl sum_tree::SeekTarget<'_, ExtentSummary, ExtentSummary> for Count {
    fn cmp(&self, other: &ExtentSummary, _: ()) -> std::cmp::Ordering {
        Ord::cmp(&self.0, &other.count)
    }
}
impl sum_tree::SeekTarget<'_, ExtentSummary, ExtentSummary> for Position {
    fn cmp(&self, other: &ExtentSummary, _: ()) -> std::cmp::Ordering {
        self.offset
            .partial_cmp(&(other.extent + other.count as f64 * self.gap))
            .unwrap()
    }
}
fn blocks(
    extents: impl IntoIterator<Item = (Pixels, bool)>,
    epoch: u64,
) -> impl Iterator<Item = KnownBlock> {
    let mut extents = extents.into_iter();
    std::iter::from_fn(move || {
        let (extent, measured) = extents.next()?;
        let mut block = BlockValues::default();
        block.push(extent, measured);
        for (extent, measured) in extents.by_ref().take(BLOCK_CAPACITY - 1) {
            block.push(extent, measured);
        }
        Some(KnownBlock {
            values: std::sync::Arc::new(block),
            measurement_epoch: epoch,
        })
    })
}
impl KnownGeometry {
    pub fn new(extents: impl IntoIterator<Item = Pixels>, gap: f64) -> Self {
        Self::with_measurements(extents.into_iter().map(|extent| (extent, true)), gap)
    }
    pub fn with_measurements(extents: impl IntoIterator<Item = (Pixels, bool)>, gap: f64) -> Self {
        let mut valid = true;
        let tree = sum_tree::SumTree::from_iter(
            blocks(
                extents.into_iter().map(|(extent, measured)| {
                    let positive = super::valid_positive(extent);
                    valid &= positive;
                    (if positive { extent } else { gpui::px(1.) }, measured)
                }),
                0,
            ),
            (),
        );
        Self {
            tree,
            gap,
            valid,
            measurement_epoch: 0,
        }
    }
    pub fn is_valid(&self) -> bool {
        self.valid
    }
    pub fn len(&self) -> usize {
        self.tree.summary().count
    }
    pub fn total(&self) -> f64 {
        self.tree.summary().extent + self.len().saturating_sub(1) as f64 * self.gap
    }
    pub fn item(&self, index: usize) -> Option<(f64, f64, bool)> {
        let mut cursor = self.tree.cursor::<ExtentSummary>(());
        cursor.seek(&Count(index), sum_tree::Bias::Right);
        let block = cursor.item()?;
        let (start, extent, measured) = block.values.item(index - cursor.start().count);
        Some((
            cursor.start().extent + start + index as f64 * self.gap,
            extent,
            measured && block.measurement_epoch == self.measurement_epoch,
        ))
    }
    pub fn items(&self, range: Range<usize>) -> impl Iterator<Item = (f64, f64, bool)> + '_ {
        let mut cursor = self.tree.cursor::<ExtentSummary>(());
        cursor.seek(&Count(range.start), sum_tree::Bias::Right);
        range.map_while(move |ix| {
            let block = cursor.item()?;
            let values = &block.values;
            let local = ix - cursor.start().count;
            let (start, extent, measured) = values.item(local);
            let value = (
                cursor.start().extent + start + ix as f64 * self.gap,
                extent,
                measured && block.measurement_epoch == self.measurement_epoch,
            );
            if local + 1 == values.len {
                cursor.next();
            }
            Some(value)
        })
    }
    pub fn extend(&mut self, extents: impl IntoIterator<Item = (Pixels, bool)>) {
        let mut extents = extents.into_iter().peekable();
        if extents.peek().is_none() {
            return;
        }
        if let Some(last_ix) = self.len().checked_sub(1) {
            let mut cursor = self.tree.cursor::<ExtentSummary>(());
            cursor.seek(&Count(last_ix), sum_tree::Bias::Right);
            let mut last = cursor.item().unwrap().clone();
            if last.values.len < BLOCK_CAPACITY {
                let block = last.values_for_write(self.measurement_epoch);
                for (extent, measured) in extents.by_ref().take(BLOCK_CAPACITY - block.len) {
                    block.push(extent, measured);
                }
                let start = cursor.start().count;
                drop(cursor);
                let mut cursor = self.tree.cursor::<ExtentSummary>(());
                let mut tree = cursor.slice(&Count(start), sum_tree::Bias::Right);
                tree.push(last, ());
                drop(cursor);
                self.tree = tree;
            }
        }
        self.tree
            .extend(blocks(extents, self.measurement_epoch), ());
    }
    pub fn invalidate_measurements(&mut self) {
        // Frozen geometry keeps its own epoch. Old sizes remain hints without
        // touching any leaf until a later measurement or tail append writes it.
        self.measurement_epoch += 1;
    }
    pub fn update_measurements(&mut self, updates: &[(usize, Pixels)], measured: bool) {
        if updates.is_empty() {
            return;
        }
        let mut updates = updates.to_vec();
        updates.sort_unstable_by_key(|(ix, _)| *ix);
        let mut cursor = self.tree.cursor::<ExtentSummary>(());
        let mut tree = sum_tree::SumTree::default();
        let mut at = 0;
        while at < updates.len() {
            tree.append(
                cursor.slice(&Count(updates[at].0), sum_tree::Bias::Right),
                (),
            );
            let start = cursor.start().count;
            let mut block = cursor.item().unwrap().clone();
            let values = block.values_for_write(self.measurement_epoch);
            let first = updates[at].0 - start;
            while at < updates.len() && updates[at].0 < start + values.len {
                let (ix, size) = updates[at];
                let size = f32::from(size);
                values.sizes[ix - start] = if measured { size } else { -size };
                at += 1;
            }
            let mut end = first.checked_sub(1).map_or(0., |ix| values.ends[ix]);
            for ix in first..values.len {
                end += f64::from(values.sizes[ix].abs());
                values.ends[ix] = end;
            }
            let next = start + values.len;
            tree.push(block, ());
            cursor.seek_forward(&Count(next), sum_tree::Bias::Right);
        }
        tree.append(cursor.suffix(), ());
        drop(cursor);
        self.tree = tree;
    }
    // Return the content end as well, avoiding a second tree seek for gaps.
    fn slot_at(&self, offset: f64, right: bool) -> (usize, f64) {
        let mut cursor = self.tree.cursor::<ExtentSummary>(());
        cursor.seek(
            &Position {
                offset,
                gap: self.gap,
            },
            if right {
                sum_tree::Bias::Right
            } else {
                sum_tree::Bias::Left
            },
        );
        let start = cursor.start();
        let Some(block) = cursor.item() else {
            return (start.count, f64::INFINITY);
        };
        let local = block.values.bound(
            offset - start.extent - start.count as f64 * self.gap,
            self.gap,
            right,
        );
        let ix = start.count + local;
        let end = if local < block.values.len {
            let (local_start, extent, _) = block.values.item(local);
            start.extent + local_start + ix as f64 * self.gap + extent
        } else {
            f64::INFINITY
        };
        (ix, end)
    }
    pub fn index_at(&self, offset: f64) -> usize {
        self.slot_at(offset.max(0.), true)
            .0
            .min(self.len().saturating_sub(1))
    }
    pub fn range(&self, start: f64, extent: f64) -> Range<usize> {
        if extent <= 0.0 || self.len() == 0 || start + extent <= 0.0 {
            return 0..0;
        }
        let (mut low, end) = self.slot_at(start.max(0.), true);
        if end <= start {
            low += 1;
        }
        let last = self.slot_at(start + extent, false).0;
        let high = (last + usize::from(last < self.len())).min(self.len());
        low..high.max(low)
    }
}
