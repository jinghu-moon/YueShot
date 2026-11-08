//! Sparse explicit spans. Geometry never feeds back into track sizing.
use super::*;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ops::{Deref, Range},
};

/// Immutable sparse span descriptors with opt-in reuse of one validated
/// configuration. Retain this value in the consumer, rather than recreating it
/// in render. Clones share descriptors and the single bounded cache entry.
///
/// Only track indices are cached: current sizes, layout and cells remain
/// frame-local. Source identity/generation and frozen prefixes invalidate the
/// entry. The set retains no controller, window, source or scroll owner.
#[derive(Clone, Default)]
pub struct VirtualGridSpanSet(Option<Rc<SpanSetData>>);

struct SpanSetData {
    descriptors: Vec<VirtualGridSpan>,
    resolved: RefCell<Option<SpanResolution>>,
}
struct SpanResolution {
    owners: (gpui::EntityId, gpui::EntityId),
    spans: Rc<GridSpans>,
}
impl VirtualGridSpanSet {
    /// Takes ownership of descriptors; an empty set allocates no cache payload.
    /// Validation occurs when applied to a Grid's current axes.
    pub fn new(descriptors: Vec<VirtualGridSpan>) -> Self {
        if descriptors.is_empty() {
            Self::default()
        } else {
            Self(Some(Rc::new(SpanSetData {
                descriptors,
                resolved: RefCell::new(None),
            })))
        }
    }
    pub fn len(&self) -> usize {
        self.0.as_ref().map_or(0, |data| data.descriptors.len())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    pub(super) fn resolve(
        &self,
        rows: &gpui::Entity<VirtualizerState>,
        columns: &gpui::Entity<VirtualizerState>,
        frozen: (usize, usize),
        cx: &gpui::App,
    ) -> Result<Option<Rc<GridSpans>>, VirtualizerError> {
        let rs = rows.read(cx);
        let cs = columns.read(cx);
        VirtualGrid::validate_axes(rs, cs)?;
        let Some(data) = &self.0 else { return Ok(None) };
        let owners = (rows.entity_id(), columns.entity_id());
        if let Some(entry) = data.resolved.borrow().as_ref()
            && entry.owners == owners
            && entry.spans.generations == (rs.generation(), cs.generation())
            && entry.spans.frozen == frozen
        {
            return Ok(Some(entry.spans.clone()));
        }
        // Build completely before replacing the last good entry. A rejected
        // configuration cannot publish partially resolved rectangles.
        let spans = Rc::new(GridSpans::new(rs, cs, &data.descriptors, frozen)?);
        *data.resolved.borrow_mut() = Some(SpanResolution {
            owners,
            spans: spans.clone(),
        });
        Ok(Some(spans))
    }
}

// Existing .spans() keeps its owned value and allocation behavior. Only the
// explicit .span_set() path shares a retained resolution across frames.
pub(super) enum GridSpanStorage {
    Owned(GridSpans),
    Shared(Rc<GridSpans>),
}
impl Deref for GridSpanStorage {
    type Target = GridSpans;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(spans) => spans,
            Self::Shared(spans) => spans,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VirtualGridSpan {
    origin: VirtualGridCellKey,
    rows: usize,
    columns: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;
    use std::{hint::black_box, time::Instant};

    struct Keys;
    impl VirtualizerSource for Keys {
        fn len(&self) -> usize {
            100_000
        }
        fn key_at(&self, index: usize) -> ElementId {
            index.into()
        }
        fn index_of(&self, key: &ElementId) -> Option<usize> {
            match key {
                ElementId::Integer(i) if *i < 100_000 => Some(*i as usize),
                _ => None,
            }
        }
    }
    fn axes() -> [VirtualizerState; 2] {
        let binding = VirtualizerBinding::new();
        [Axis::Vertical, Axis::Horizontal].map(|axis| {
            VirtualizerState::new_attached(
                axis,
                Rc::new(Keys),
                VirtualizerSizing::fixed(px(20.)),
                &binding,
            )
            .unwrap()
        })
    }
    fn descriptors(count: usize, same_row: bool) -> Vec<VirtualGridSpan> {
        (0..count)
            .rev()
            .map(|i| {
                VirtualGridSpan::new(
                    VirtualGridCellKey::new(if same_row { 0 } else { i * 3 }, i * 3),
                    2,
                    2,
                )
            })
            .collect()
    }
    #[test]
    fn span_lookup_matches_independent_linear_rectangles_at_half_open_edges() {
        let [rows, columns] = axes();
        for same_row in [false, true] {
            for count in [0, 1, 100, 1000] {
                let spans =
                    GridSpans::new(&rows, &columns, &descriptors(count, same_row), (0, 0)).unwrap();
                let mut seed = 783_421u64;
                for step in 0..3000 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let column = ((seed >> 32) as usize) % (count * 3 + 3);
                    let row = if same_row {
                        step % 4
                    } else {
                        column + step % 3
                    };
                    let expected = spans.entries.iter().position(|span| {
                        span.rows.contains(&row) && span.columns.contains(&column)
                    });
                    assert_eq!(
                        spans.covering(row, column),
                        expected,
                        "count={count} row={row} column={column}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "manual isolated span validation and point-query CPU cost"]
    fn span_cost_probe() {
        let [rows, columns] = axes();
        for same_row in [false, true] {
            for count in [0, 100, 1000, 10_000] {
                let descriptors = descriptors(count, same_row);
                for sample in 0..5 {
                    let start = Instant::now();
                    for _ in 0..20 {
                        black_box(GridSpans::new(&rows, &columns, &descriptors, (0, 0)).unwrap());
                    }
                    println!(
                        "SPAN_COST,stage=config,same_row={same_row},count={count},sample={sample},iterations=20,ns={}",
                        start.elapsed().as_nanos()
                    );
                    let spans = GridSpans::new(&rows, &columns, &descriptors, (0, 0)).unwrap();
                    let start = Instant::now();
                    let mut checksum = 0;
                    for i in 0..2000 {
                        let column = (i * 397) % (count * 3 + 3);
                        let row = if same_row { i % 3 } else { column };
                        checksum += black_box(spans.covering(black_box(row), black_box(column)))
                            .map_or(0, |i| i + 1);
                    }
                    black_box(checksum);
                    println!(
                        "SPAN_COST,stage=query,same_row={same_row},count={count},sample={sample},iterations=2000,ns={}",
                        start.elapsed().as_nanos()
                    );
                }
            }
        }
    }

    #[test]
    fn sparse_span_visits_match_linear_oracle_without_filling_frozen_body_gaps() {
        let [rows, columns] = axes();
        for same_row in [false, true] {
            for count in [0, 1, 100, 1000] {
                let mut descriptors = descriptors(count, same_row);
                // Vary row ends as well as starts; equal-start column pruning
                // must not assume identical span heights.
                for (i, span) in descriptors.iter_mut().enumerate() {
                    if same_row {
                        span.rows = 1 + i % 17;
                    }
                }
                let spans = GridSpans::new(&rows, &columns, &descriptors, (0, 0)).unwrap();
                for step in 0..300 {
                    let mut rows = vec![0, step % 19, step * 7, step * 7 + 1];
                    rows.sort_unstable();
                    rows.dedup();
                    let mut columns = vec![0, step * 3, step * 3 + 1];
                    columns.sort_unstable();
                    columns.dedup();
                    for (rows, columns) in [
                        (&rows[..], &columns[..]),
                        (&[][..], &columns[..]),
                        (&rows[..], &[][..]),
                    ] {
                        let expected = spans
                            .entries
                            .iter()
                            .enumerate()
                            .filter(|(_, span)| {
                                rows.iter().any(|r| span.rows.contains(r))
                                    && columns.iter().any(|c| span.columns.contains(c))
                            })
                            .map(|(i, _)| i)
                            .collect::<Vec<_>>();
                        let mut actual = Vec::new();
                        spans.visit_intersecting(rows, columns, |i| actual.push(i));
                        assert_eq!(
                            actual, expected,
                            "count={count} same_row={same_row} step={step}"
                        );
                        for row in rows {
                            for column in columns {
                                assert_eq!(
                                    spans.covering(*row, *column),
                                    spans.entries.iter().position(|span| span.rows.contains(row)
                                        && span.columns.contains(column))
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
impl VirtualGridSpan {
    pub fn new(origin: VirtualGridCellKey, rows: usize, columns: usize) -> Self {
        Self {
            origin,
            rows,
            columns,
        }
    }
    pub fn origin(&self) -> &VirtualGridCellKey {
        &self.origin
    }
    pub fn row_span(&self) -> usize {
        self.rows
    }
    pub fn column_span(&self) -> usize {
        self.columns
    }
}

pub(super) struct ResolvedSpan {
    pub rows: Range<usize>,
    pub columns: Range<usize>,
}
pub(super) struct GridSpans {
    pub entries: Vec<ResolvedSpan>,
    pub generations: (u64, u64),
    pub frozen: (usize, usize),
    max_end: Vec<usize>,
}
impl GridSpans {
    pub fn new(
        rows: &VirtualizerState,
        columns: &VirtualizerState,
        descriptors: &[VirtualGridSpan],
        frozen: (usize, usize),
    ) -> Result<Self, VirtualizerError> {
        VirtualGrid::validate_axes(rows, columns)?;
        let rs = rows.source_for_render();
        let cs = columns.source_for_render();
        let mut entries = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            if descriptor.rows == 0 || descriptor.columns == 0 {
                return Err(VirtualizerError::InvalidGeometry("span count"));
            }
            let r = rs
                .index_of(descriptor.origin.row())
                .ok_or(VirtualizerError::MissingKey)?;
            let c = cs
                .index_of(descriptor.origin.column())
                .ok_or(VirtualizerError::MissingKey)?;
            let re = r
                .checked_add(descriptor.rows)
                .filter(|end| *end <= rs.len())
                .ok_or(VirtualizerError::InvalidGeometry("span bounds"))?;
            let ce = c
                .checked_add(descriptor.columns)
                .filter(|end| *end <= cs.len())
                .ok_or(VirtualizerError::InvalidGeometry("span bounds"))?;
            if r < frozen.0 && re > frozen.0 || c < frozen.1 && ce > frozen.1 {
                return Err(VirtualizerError::Unsupported(
                    "span crosses frozen boundary",
                ));
            }
            entries.push(ResolvedSpan {
                rows: r..re,
                columns: c..ce,
            });
        }
        entries.sort_unstable_by_key(|span| (span.rows.start, span.columns.start));
        // End events precede starts: adjacent rectangles do not overlap.
        let mut events = Vec::with_capacity(entries.len() * 2);
        for (i, span) in entries.iter().enumerate() {
            events.push((span.rows.start, true, i));
            events.push((span.rows.end, false, i));
        }
        events.sort_unstable();
        let mut active: BTreeMap<usize, usize> = BTreeMap::new();
        for (_, start, i) in events {
            let span = &entries[i];
            if !start {
                active.remove(&span.columns.start);
                continue;
            }
            if active
                .range(..span.columns.end)
                .next_back()
                .is_some_and(|(_, end)| *end > span.columns.start)
            {
                return Err(VirtualizerError::InvalidGeometry("overlapping spans"));
            }
            active.insert(span.columns.start, span.columns.end);
        }
        let mut result = Self {
            max_end: vec![0; entries.len().saturating_mul(4)],
            entries,
            generations: (rows.generation(), columns.generation()),
            frozen,
        };
        if !result.entries.is_empty() {
            result.build(1, 0, result.entries.len());
        }
        Ok(result)
    }
    fn build(&mut self, node: usize, lo: usize, hi: usize) -> usize {
        let end = if hi - lo == 1 {
            self.entries[lo].rows.end
        } else {
            let mid = (lo + hi) / 2;
            let a = self.build(node * 2, lo, mid);
            let b = self.build(node * 2 + 1, mid, hi);
            a.max(b)
        };
        self.max_end[node] = end;
        end
    }
    pub fn covering(&self, row: usize, column: usize) -> Option<usize> {
        if self.entries.is_empty() {
            None
        } else {
            self.find(1, 0, self.entries.len(), row, column)
        }
    }
    fn find(&self, node: usize, lo: usize, hi: usize, row: usize, column: usize) -> Option<usize> {
        if self.max_end[node] <= row || self.entries[lo].rows.start > row {
            return None;
        }
        // Equal row starts imply disjoint, column-sorted intervals after
        // overlap validation. Prune those subtrees without a second index.
        if self.entries[lo].rows.start == self.entries[hi - 1].rows.start
            && (self.entries[lo].columns.start > column
                || self.entries[hi - 1].columns.end <= column)
        {
            return None;
        }
        if hi - lo == 1 {
            return self.entries[lo].columns.contains(&column).then_some(lo);
        }
        let mid = (lo + hi) / 2;
        self.find(node * 2, lo, mid, row, column)
            .or_else(|| self.find(node * 2 + 1, mid, hi, row, column))
    }

    /// Visit only spans intersecting both sparse sorted track working sets.
    /// Reuses the row-max tree; no cell-area enumeration or per-frame index.
    pub fn visit_intersecting(
        &self,
        rows: &[usize],
        columns: &[usize],
        mut visit: impl FnMut(usize),
    ) {
        if !self.entries.is_empty() && !rows.is_empty() && !columns.is_empty() {
            self.visit(1, 0, self.entries.len(), rows, columns, &mut visit);
        }
    }
    fn visit(
        &self,
        node: usize,
        lo: usize,
        hi: usize,
        rows: &[usize],
        columns: &[usize],
        visit: &mut impl FnMut(usize),
    ) {
        let intersects = |start, end, indices: &[usize]| {
            indices
                .get(indices.partition_point(|i| *i < start))
                .is_some_and(|i| *i < end)
        };
        let first = &self.entries[lo];
        let last = &self.entries[hi - 1];
        if !intersects(first.rows.start, self.max_end[node], rows) {
            return;
        }
        if first.rows.start == last.rows.start
            && !intersects(first.columns.start, last.columns.end, columns)
        {
            return;
        }
        if hi - lo == 1 {
            if intersects(first.columns.start, first.columns.end, columns) {
                visit(lo);
            }
        } else {
            let mid = (lo + hi) / 2;
            self.visit(node * 2, lo, mid, rows, columns, visit);
            self.visit(node * 2 + 1, mid, hi, rows, columns, visit);
        }
    }
}
