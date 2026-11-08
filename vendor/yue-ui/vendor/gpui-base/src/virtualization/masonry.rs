//! Stable-lane masonry. Each lane reuses the persistent extent tree; resize
//! changes only the affected lane. Repacking is an explicit cold transaction.
use super::*;
use gpui::{Pixels, ScrollHandle, Size, px};
use std::collections::{BinaryHeap, HashSet};
#[path = "masonry_element.rs"]
mod element;
pub use element::VirtualMasonry;
#[path = "masonry_index.rs"]
mod index;
use index::IndexVector;
#[path = "masonry_activity.rs"]
mod activity;
#[path = "masonry_cache.rs"]
mod cache;
pub use cache::{VirtualMasonryCache, VirtualMasonryCacheItem};

struct MasonryCommand {
    id: u64,
    target: MasonryTarget,
    align: VirtualizerAlign,
    progress: super::command::CommandProgress,
    motion: Option<Box<MasonryMotion>>,
}
// Optional timed state must not enlarge every controller's inline command slot.
// Instant and Measured requests need no allocation for this state.
struct MasonryMotion {
    options: VirtualizerScrollOptions,
    from: f64,
    elapsed: Duration,
    motion_settled: bool,
    time_progressed: bool,
}
enum MasonryTarget {
    Key(ElementId),
    Offset(f64),
}

#[derive(Clone)]
struct Lane {
    indexes: IndexVector<usize>,
    geometry: geometry::KnownGeometry,
}
#[derive(Clone, Copy)]
struct LaneEnd(f64, usize);
impl PartialEq for LaneEnd {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}
impl Eq for LaneEnd {}
impl Ord for LaneEnd {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| Ord::cmp(&other.1, &self.1))
    }
}
impl PartialOrd for LaneEnd {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

#[derive(Clone, Debug)]
pub struct VirtualMasonryItem {
    item: VirtualizerItem,
    lane: usize,
    cross_start: Pixels,
    cross_extent: Pixels,
}
impl VirtualMasonryItem {
    pub fn item(&self) -> &VirtualizerItem {
        &self.item
    }
    pub fn lane(&self) -> usize {
        self.lane
    }
    pub fn cross_start(&self) -> Pixels {
        self.cross_start
    }
    pub fn cross_extent(&self) -> Pixels {
        self.cross_extent
    }
}
#[derive(Clone)]
pub struct VirtualMasonrySnapshot {
    owner: std::rc::Weak<()>,
    revision: u64,
    source: Rc<dyn VirtualizerSource>,
    lanes: Vec<Lane>,
    slots: IndexVector<(usize, usize)>,
    offset: f64,
    viewport: Size<Pixels>,
    layout: VirtualizerLayout,
    column_gap: Pixels,
    direction: Option<VirtualizerScrollDirection>,
    total: f64,
    retained: Rc<[ElementId]>,
}
impl VirtualMasonrySnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn total_extent(&self) -> f64 {
        self.total
    }
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }
    pub fn offset(&self) -> f64 {
        self.offset
    }
    pub fn item(&self, ix: usize) -> Result<VirtualMasonryItem, VirtualizerError> {
        let &(lane, local) = self.slots.get(ix).ok_or(VirtualizerError::MissingKey)?;
        let (start, extent, measured) = self.lanes[lane]
            .geometry
            .item(local)
            .ok_or(VirtualizerError::MissingKey)?;
        let width = ((self.viewport.width - self.column_gap * self.lanes.len().saturating_sub(1))
            .max(px(0.)))
            / self.lanes.len() as f32;
        let key = self.source.key_at(ix);
        if self.source.index_of(&key) != Some(ix) {
            return Err(VirtualizerError::InvalidSource);
        }
        Ok(VirtualMasonryItem {
            item: VirtualizerItem::from_geometry(
                ix,
                key,
                start + f64::from(self.layout.padding_start),
                Pixels::from(extent),
                measured,
            ),
            lane,
            cross_start: (width + self.column_gap) * lane,
            cross_extent: width,
        })
    }
    pub fn working_indices(&self) -> Vec<usize> {
        if self.viewport.width <= self.column_gap * self.lanes.len().saturating_sub(1)
            || self.viewport.height <= px(0.)
        {
            return Vec::new();
        }
        let (before, after) = self.layout.overscan_edges(self.direction);
        let start = self.offset - f64::from(self.layout.padding_start) - f64::from(before);
        let extent = f64::from(self.viewport.height) + f64::from(before + after);
        let mut indexes = Vec::new();
        for lane in &self.lanes {
            indexes.extend(
                lane.geometry
                    .range(start, extent)
                    .map(|ix| lane.indexes[ix]),
            );
        }
        indexes.extend(
            self.retained
                .iter()
                .filter_map(|key| self.source.index_of(key)),
        );
        indexes.sort_unstable();
        indexes.dedup();
        indexes
    }
}

/// One vertical scroll owner. Item measurements have one owner: lane trees.
/// Source identity/selection/editing remain consumer-owned. Reflow does not
/// silently redistribute items during scrolling or asynchronous measurement.
pub struct VirtualMasonryState {
    source: Rc<dyn VirtualizerSource>,
    lanes: Vec<Lane>,
    slots: IndexVector<(usize, usize)>,
    dynamic: bool,
    layout: VirtualizerLayout,
    column_gap: Pixels,
    cross: Option<Pixels>,
    viewport: Option<Size<Pixels>>,
    anchor: Option<(ElementId, VirtualizerAnchorPosition, bool)>,
    pending: Option<MasonryCommand>,
    next_command_id: u64,
    events: Vec<VirtualizerEvent>,
    // Observation of the native owner, never a second writable scroll position.
    last_offset: Option<f64>,
    // None has no session; Some(false) is paused; Some(true) follows the tail.
    follow_tail: Option<bool>,
    follow_tail_input_pending: bool,
    scroll: ScrollHandle,
    token: Rc<()>,
    revision: u64,
    content_revision: u64,
    content_base_revision: u64,
    content_revisions: std::collections::BTreeMap<usize, u64>,
    retained: Rc<[ElementId]>,
    focus: std::collections::HashMap<ElementId, gpui::FocusHandle>,
    mount: Option<(gpui::WindowId, gpui::GlobalElementId)>,
    budget_reported: bool,
    invalid_reported: bool,
    focus_fallback: Option<gpui::FocusHandle>,
    removed_focus: Vec<gpui::FocusHandle>,
    activity: Option<Box<activity::MasonryActivity>>,
}
impl gpui::EventEmitter<VirtualizerEvent> for VirtualMasonryState {}

/// Masonry owns key/offset positioning, with no attached adapter. Smooth motion
/// requires explicit dimensions, as on the one-dimensional path.
/// Query these boundaries before exposing controls shared with a linear list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualMasonryCapabilities {
    smooth: bool,
}
impl VirtualMasonryCapabilities {
    pub fn axis(&self) -> Axis {
        Axis::Vertical
    }
    pub fn supports_attached(&self) -> bool {
        false
    }
    pub fn supports_follow_tail(&self) -> bool {
        true
    }
    pub fn supports_smooth(&self) -> bool {
        self.smooth
    }
    pub fn supports_scroll_to_offset(&self) -> bool {
        true
    }
}
impl VirtualMasonryState {
    pub fn capabilities(&self) -> VirtualMasonryCapabilities {
        VirtualMasonryCapabilities {
            smooth: !self.dynamic,
        }
    }
    pub fn new(
        source: Rc<dyn VirtualizerSource>,
        sizing: VirtualizerSizing,
        lane_count: usize,
    ) -> Result<Self, VirtualizerError> {
        if lane_count == 0 {
            return Err(VirtualizerError::InvalidGeometry("lane count"));
        }
        let extents = sizing_extents(&sizing, source.len(), 0..source.len())?;
        let dynamic = sizing.kind() == VirtualizerSizingKind::Measured;
        let (lanes, slots) = pack(&extents, lane_count, 0.0);
        Ok(Self {
            source,
            lanes,
            slots,
            dynamic,
            layout: VirtualizerLayout::default(),
            column_gap: px(0.),
            cross: None,
            viewport: None,
            anchor: None,
            pending: None,
            next_command_id: 0,
            events: Vec::new(),
            last_offset: None,
            follow_tail: None,
            follow_tail_input_pending: false,
            scroll: ScrollHandle::new(),
            token: Rc::new(()),
            revision: 1,
            content_revision: 0,
            content_base_revision: 0,
            content_revisions: Default::default(),
            retained: Rc::from([]),
            focus: std::collections::HashMap::new(),
            mount: None,
            budget_reported: false,
            invalid_reported: false,
            focus_fallback: None,
            removed_focus: Vec::new(),
            activity: None,
        })
    }
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }
    pub fn source(&self) -> &Rc<dyn VirtualizerSource> {
        &self.source
    }
    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }
    pub fn total_extent(&self) -> f64 {
        self.lanes
            .iter()
            .map(|lane| lane.geometry.total())
            .fold(0.0, f64::max)
            + f64::from(self.layout.padding_start)
            + f64::from(self.layout.padding_end)
    }
    pub fn set_layout(
        &mut self,
        layout: VirtualizerLayout,
        column_gap: Pixels,
    ) -> Result<(), VirtualizerError> {
        if self.layout == layout && self.column_gap == column_gap {
            return Ok(());
        }
        if ![
            layout.gap,
            layout.padding_start,
            layout.padding_end,
            layout.overscan,
            layout.scroll_padding_start,
            layout.scroll_padding_end,
            column_gap,
        ]
        .into_iter()
        .all(valid_nonnegative)
            || !layout.valid_overscan()
            || layout.measurement_budget == 0
        {
            return Err(VirtualizerError::InvalidGeometry("masonry layout"));
        }
        self.observe_input();
        self.capture_anchor();
        if layout.revision != self.layout.revision && self.dynamic {
            for lane in &mut self.lanes {
                lane.geometry.invalidate_measurements();
            }
        }
        self.layout = layout;
        self.column_gap = column_gap;
        for lane in &mut self.lanes {
            lane.geometry.gap = layout.gap.into();
        }
        self.revision += 1;
        self.restore_anchor();
        Ok(())
    }
    /// Cold reflow for a responsive column-count change, preserving a key anchor.
    pub fn reflow(&mut self, lane_count: usize) -> Result<(), VirtualizerError> {
        if lane_count == 0 {
            return Err(VirtualizerError::InvalidGeometry("lane count"));
        }
        self.observe_input();
        self.capture_anchor();
        let extents = self.source_extents();
        (self.lanes, self.slots) = pack(&extents, lane_count, self.layout.gap.into());
        self.cross = None;
        if self.dynamic {
            for lane in &mut self.lanes {
                lane.geometry.invalidate_measurements();
            }
        }
        self.revision += 1;
        self.restore_anchor();
        Ok(())
    }
    /// Structural transaction. Real sizes survive unchanged key/content pairs;
    /// insertion/removal/reorder repacks once, outside the scrolling path.
    pub fn replace_source(
        &mut self,
        source: Rc<dyn VirtualizerSource>,
        sizing: VirtualizerSizing,
    ) -> Result<(), VirtualizerError> {
        let mut extents = sizing_extents(&sizing, source.len(), 0..source.len())?;
        let dynamic = sizing.kind() == VirtualizerSizingKind::Measured;
        let previous = self.source_extents();
        for (ix, value) in extents.iter_mut().enumerate() {
            let old = self.source.index_of(&source.key_at(ix)).filter(|old| {
                dynamic
                    && self.source.revision_at(*old) == source.revision_at(ix)
                    && previous[*old].1
            });
            if let Some(old) = old {
                *value = previous[old];
            }
        }
        let (lanes, slots) = pack(&extents, self.lanes.len(), self.layout.gap.into());
        self.observe_input();
        self.follow_tail_input_pending = false;
        self.follow_tail = None;
        self.capture_anchor();
        self.source = source;
        self.lanes = lanes;
        self.slots = slots;
        self.dynamic = dynamic;
        if !self.content_revisions.is_empty() {
            self.content_revision += 1;
            self.content_base_revision = self.content_revision;
            self.content_revisions.clear();
        }
        self.removed_focus.extend(
            self.focus
                .iter()
                .filter(|(key, _)| self.source.index_of(key).is_none())
                .map(|(_, focus)| focus.clone()),
        );
        self.focus
            .retain(|key, _| self.source.index_of(key).is_some());
        self.retained = self
            .retained
            .iter()
            .filter(|key| self.source.index_of(key).is_some())
            .cloned()
            .collect();
        self.anchor = self
            .anchor
            .take()
            .filter(|(key, _, _)| self.source.index_of(key).is_some());
        if self.pending.as_ref().is_some_and(|command| {
            matches!(&command.target,
                MasonryTarget::Key(key) if self.source.index_of(key).is_none())
        }) {
            self.finish_command(VirtualizerCommandOutcome::Missing);
        }
        self.revision += 1;
        self.restore_anchor();
        Ok(())
    }
    /// Append with an explicitly unchanged prefix (keys, content and hints).
    /// Stable lanes and real measurements survive. Frozen snapshots share
    /// complete index blocks; only the block directory and tail leaves copy.
    pub fn append_source(
        &mut self,
        source: Rc<dyn VirtualizerSource>,
        sizing: VirtualizerSizing,
    ) -> Result<(), VirtualizerError> {
        let old_count = self.source.len();
        if source.len() < old_count
            || (sizing.kind() == VirtualizerSizingKind::Measured) != self.dynamic
        {
            return Err(VirtualizerError::InvalidChange);
        }
        let extents = sizing_extents(&sizing, source.len(), old_count..source.len())?;
        self.observe_input();
        self.capture_anchor();
        let mut heap: BinaryHeap<_> = self
            .lanes
            .iter()
            .enumerate()
            .map(|(ix, lane)| {
                LaneEnd(
                    lane.geometry.total()
                        + if lane.indexes.is_empty() {
                            0.0
                        } else {
                            self.layout.gap.into()
                        },
                    ix,
                )
            })
            .collect();
        let mut appended = vec![Vec::new(); self.lanes.len()];
        for (local, value) in extents.into_iter().enumerate() {
            let ix = old_count + local;
            let LaneEnd(end, lane_ix) = heap.pop().unwrap();
            let lane = &mut self.lanes[lane_ix];
            self.slots.push((lane_ix, lane.indexes.len()));
            lane.indexes.push(ix);
            appended[lane_ix].push(value);
            heap.push(LaneEnd(
                end + f64::from(value.0) + f64::from(self.layout.gap),
                lane_ix,
            ));
        }
        for (lane, values) in self.lanes.iter_mut().zip(appended) {
            lane.geometry.extend(values);
        }
        self.source = source;
        self.revision += 1;
        self.restore_anchor();
        Ok(())
    }

    /// Follow the native tail during append and size correction. User scrolling
    /// pauses the session away from the tail; returning to the tail resumes it.
    /// Passing `false` ends the session.
    pub fn set_follow_tail(&mut self, follow: bool) -> Result<(), VirtualizerError> {
        self.observe_input();
        self.follow_tail_input_pending = false;
        if follow {
            self.cancel_scroll();
            self.follow_tail = Some(true);
            self.restore_anchor();
        } else {
            self.follow_tail = None;
        }
        Ok(())
    }

    pub fn is_following_tail(&self) -> bool {
        self.follow_tail == Some(true)
    }
    fn source_extents(&self) -> Vec<(Pixels, bool)> {
        let mut extents = vec![(px(0.), false); self.source.len()];
        for lane in &self.lanes {
            for (&ix, (_, extent, measured)) in lane
                .indexes
                .iter()
                .zip(lane.geometry.items(0..lane.indexes.len()))
            {
                extents[ix] = (Pixels::from(extent), measured);
            }
        }
        extents
    }
    pub fn set_retained_keys(&mut self, keys: Vec<ElementId>) -> Result<(), VirtualizerError> {
        if keys.iter().any(|key| self.source.index_of(key).is_none()) {
            return Err(VirtualizerError::MissingKey);
        }
        self.retained = keys.into();
        self.revision += 1;
        Ok(())
    }
    pub fn set_focus_handle(
        &mut self,
        key: ElementId,
        focus: Option<gpui::FocusHandle>,
    ) -> Result<(), VirtualizerError> {
        if self.source.index_of(&key).is_none() {
            return Err(VirtualizerError::MissingKey);
        }
        if let Some(focus) = focus {
            if self
                .focus
                .iter()
                .any(|(other, handle)| *other != key && *handle == focus)
            {
                return Err(VirtualizerError::InvalidSource);
            }
            self.focus.insert(key, focus);
        } else {
            self.focus.remove(&key);
        }
        Ok(())
    }
    pub fn set_focus_fallback(&mut self, focus: Option<gpui::FocusHandle>) {
        self.focus_fallback = focus;
    }
    /// Explicit content invalidation keeps old extents as hints.
    pub fn invalidate_items(&mut self, keys: &[ElementId]) -> Result<(), VirtualizerError> {
        let indexes: Result<Vec<_>, _> = keys
            .iter()
            .map(|key| {
                self.source
                    .index_of(key)
                    .ok_or(VirtualizerError::MissingKey)
            })
            .collect();
        let mut indexes = indexes?;
        let mut by_lane = vec![Vec::new(); self.lanes.len()];
        let mut seen = HashSet::new();
        for &ix in &indexes {
            if !seen.insert(ix) {
                continue;
            }
            let (lane, local) = self.slots[ix];
            let (_, extent, measured) = self.lanes[lane].geometry.item(local).unwrap();
            if self.dynamic && measured {
                by_lane[lane].push((local, Pixels::from(extent)));
            }
        }
        for (lane, values) in self.lanes.iter_mut().zip(by_lane) {
            lane.geometry.update_measurements(&values, false);
        }
        // Explicit content identity changes even before its first measurement.
        if !indexes.is_empty() {
            indexes.sort_unstable();
            indexes.dedup();
            self.content_revision += 1;
            let mut at = 0;
            while at < indexes.len() {
                let start = indexes[at];
                let mut end = start + 1;
                at += 1;
                while at < indexes.len() && indexes[at] == end {
                    end += 1;
                    at += 1;
                }
                let after = self.item_content_revision(end);
                let boundaries: Vec<_> = self
                    .content_revisions
                    .range(start..=end)
                    .map(|(&ix, _)| ix)
                    .collect();
                for ix in boundaries {
                    self.content_revisions.remove(&ix);
                }
                self.content_revisions.insert(start, self.content_revision);
                if end < self.source.len() {
                    self.content_revisions.insert(end, after);
                }
            }
            self.revision += 1;
        }
        Ok(())
    }
    fn item_content_revision(&self, ix: usize) -> u64 {
        self.content_revisions
            .range(..=ix)
            .next_back()
            .map_or(self.content_base_revision, |(_, &revision)| revision)
    }
    pub fn snapshot(
        &self,
        offset: f64,
        viewport: Size<Pixels>,
    ) -> Result<VirtualMasonrySnapshot, VirtualizerError> {
        if !offset.is_finite()
            || offset < 0.0
            || !valid_nonnegative(viewport.width)
            || !valid_nonnegative(viewport.height)
        {
            return Err(VirtualizerError::InvalidGeometry("masonry viewport"));
        }
        Ok(VirtualMasonrySnapshot {
            owner: Rc::downgrade(&self.token),
            revision: self.revision,
            source: self.source.clone(),
            lanes: self.lanes.clone(),
            slots: self.slots.clone(),
            offset,
            viewport,
            layout: self.layout,
            column_gap: self.column_gap,
            direction: self.scroll_direction(),
            total: self.total_extent(),
            retained: self.retained.clone(),
        })
    }
    pub fn update_sizes(
        &mut self,
        updates: &[VirtualizerSizeUpdate],
    ) -> Result<(), VirtualizerError> {
        // A single asynchronous measurement needs neither duplicate sorting
        // nor a per-lane batch directory. Keep the same validation, no-op,
        // anchor and revision contracts as the multi-item transaction below.
        if let [update] = updates {
            return self.update_single_size(update);
        }
        let mut indexed = Vec::with_capacity(updates.len());
        for update in updates {
            let ix = self
                .source
                .index_of(update.key())
                .ok_or(VirtualizerError::MissingKey)?;
            if !valid_positive(update.extent()) {
                return Err(VirtualizerError::InvalidGeometry("masonry size update"));
            }
            indexed.push((ix, update.extent()));
        }
        indexed.sort_unstable_by_key(|(ix, _)| *ix);
        if indexed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(VirtualizerError::InvalidGeometry("masonry size update"));
        }
        indexed.retain(|(ix, extent)| {
            let (lane, local) = self.slots[*ix];
            let (_, old, measured) = self.lanes[lane].geometry.item(local).unwrap();
            old != f64::from(*extent) || !measured
        });
        if indexed.is_empty() {
            return Ok(());
        }
        let mut by_lane = vec![Vec::new(); self.lanes.len()];
        for (ix, extent) in indexed {
            let (lane, local) = self.slots[ix];
            by_lane[lane].push((local, extent));
        }
        self.capture_anchor();
        for (lane, values) in self.lanes.iter_mut().zip(by_lane) {
            lane.geometry.update_measurements(&values, true);
        }
        self.revision += 1;
        self.restore_anchor();
        Ok(())
    }
    // Keep the specialized branch out of the multi-item feedback body; larger
    // batches should retain the original hot-path code and allocation profile.
    #[inline(never)]
    fn update_single_size(
        &mut self,
        update: &VirtualizerSizeUpdate,
    ) -> Result<(), VirtualizerError> {
        let ix = self
            .source
            .index_of(update.key())
            .ok_or(VirtualizerError::MissingKey)?;
        if !valid_positive(update.extent()) {
            return Err(VirtualizerError::InvalidGeometry("masonry size update"));
        }
        let (lane, local) = self.slots[ix];
        let (_, old, measured) = self.lanes[lane].geometry.item(local).unwrap();
        if measured && old == f64::from(update.extent()) {
            return Ok(());
        }
        self.capture_anchor();
        self.lanes[lane]
            .geometry
            .update_measurements(&[(local, update.extent())], true);
        self.revision += 1;
        self.restore_anchor();
        Ok(())
    }
    pub fn update_sizes_from_snapshot(
        &mut self,
        snapshot: &VirtualMasonrySnapshot,
        updates: &[VirtualizerSizeUpdate],
    ) -> Result<(), VirtualizerError> {
        if !snapshot.owner.ptr_eq(&Rc::downgrade(&self.token)) || snapshot.revision != self.revision
        {
            return Err(VirtualizerError::StaleSnapshot);
        }
        self.update_sizes(updates)
    }
    /// Accept common scroll options or a legacy alignment. Fixed/Known sizes
    /// support bounded, short-distance smooth motion. Measured sizes, reduced
    /// motion and long distances safely position instantly, as on a linear list.
    pub fn scroll_to_key(
        &mut self,
        key: ElementId,
        options: impl Into<VirtualizerScrollOptions>,
    ) -> Result<u64, VirtualizerError> {
        if self.source.index_of(&key).is_none() {
            return Err(VirtualizerError::MissingKey);
        }
        self.start_command(MasonryTarget::Key(key), options.into())
    }
    /// Position the native owner at a nonnegative, finite content offset.
    /// It can be queued before mount and is clamped to current geometry during
    /// layout. Alignment/scroll padding apply only to key targets. Unknown
    /// offscreen sizes remain estimates; this is not a persistent tail pin.
    /// Callers notify their owning entity after a successful request.
    pub fn scroll_to_offset(
        &mut self,
        offset: f64,
        options: impl Into<VirtualizerScrollOptions>,
    ) -> Result<u64, VirtualizerError> {
        if !offset.is_finite() || offset < 0.0 {
            return Err(VirtualizerError::InvalidGeometry("scroll offset"));
        }
        self.start_command(MasonryTarget::Offset(offset), options.into())
    }
    fn start_command(
        &mut self,
        target: MasonryTarget,
        options: VirtualizerScrollOptions,
    ) -> Result<u64, VirtualizerError> {
        if matches!(options.motion, VirtualizerMotion::JumpThenSmooth { .. }) {
            return Err(VirtualizerError::Unsupported("masonry jump-then-smooth"));
        }
        if let VirtualizerMotion::Smooth {
            duration,
            max_distance,
        } = options.motion
        {
            if duration.is_zero() || !valid_positive(max_distance) {
                return Err(VirtualizerError::InvalidGeometry("smooth bounds"));
            }
        }
        self.observe_input();
        self.cancel_scroll();
        self.follow_tail_input_pending = false;
        self.next_command_id += 1;
        let id = self.next_command_id;
        self.pending = Some(MasonryCommand {
            id,
            target,
            align: options.align,
            progress: Default::default(),
            motion: (!self.dynamic
                && !options.reduced_motion
                && matches!(options.motion, VirtualizerMotion::Smooth { .. }))
            .then(|| {
                Box::new(MasonryMotion {
                    options,
                    from: (-self.scroll.precise_offset().y).max(0.0),
                    elapsed: Duration::ZERO,
                    motion_settled: false,
                    time_progressed: false,
                })
            }),
        });
        Ok(id)
    }
    /// Queued terminal results for pure owners. Entity presentation emits these
    /// once at its input/frame/unmount boundary.
    pub fn take_events(&mut self) -> Vec<VirtualizerEvent> {
        std::mem::take(&mut self.events)
    }
    fn finish_command(&mut self, outcome: VirtualizerCommandOutcome) {
        if let Some(command) = self.pending.take() {
            self.events.push(VirtualizerEvent::CommandFinished {
                id: command.id,
                outcome,
            });
        }
    }
    pub(super) fn emit_events(&mut self, cx: &mut gpui::Context<Self>) {
        for event in self.take_events() {
            cx.emit(event);
        }
    }
    pub(super) fn begin_command_frame(&mut self, now: std::time::Instant) {
        let exceeded = self.pending.as_mut().is_some_and(|command| {
            let elapsed = command.progress.started().map_or(Duration::ZERO, |started| now.saturating_duration_since(started));
            let animating = command.motion.as_mut().is_some_and(|motion| {
                let animating = !self.dynamic && !motion.options.reduced_motion
                    && matches!(motion.options.motion, VirtualizerMotion::Smooth { duration, .. } if elapsed < duration);
                motion.time_progressed = animating && elapsed > motion.elapsed;
                motion.elapsed = elapsed;
                animating
            });
            command.progress.advance_with_motion(now, animating)
        });
        if exceeded {
            self.finish_command(VirtualizerCommandOutcome::NoProgress);
        }
    }
    pub(super) fn settle_motion(&mut self) {
        if let Some(motion) = self
            .pending
            .as_mut()
            .and_then(|command| command.motion.as_mut())
        {
            motion.options.reduced_motion = true;
        }
    }
    pub(super) fn confirm_command(&mut self, complete: bool) {
        let offset = (-self.scroll.precise_offset().y).max(0.);
        let settled = self.pending.as_ref().is_some_and(|command| {
            complete
                && command
                    .motion
                    .as_ref()
                    .is_none_or(|motion| motion.motion_settled)
                && match &command.target {
                    MasonryTarget::Key(key) => self.source.index_of(key).is_some_and(|ix| {
                        let (lane, local) = self.slots[ix];
                        self.lanes[lane]
                            .geometry
                            .item(local)
                            .is_some_and(|(_, _, measured)| measured)
                    }),
                    MasonryTarget::Offset(target) => self.viewport.is_some_and(|viewport| {
                        let target =
                            target.min((self.total_extent() - f64::from(viewport.height)).max(0.));
                        (offset - target).abs() <= 0.01
                    }),
                }
        });
        if let Some(command) = &mut self.pending {
            // Small legitimate motion may be below the positional stall
            // tolerance at high refresh rates. Advancing animation time is
            // progress; frozen time and post-animation correction still stall.
            let stalled = command.progress.confirm(
                offset,
                settled
                    || command
                        .motion
                        .as_ref()
                        .is_some_and(|motion| motion.time_progressed && !motion.motion_settled),
            );
            if settled {
                self.finish_command(VirtualizerCommandOutcome::Settled);
            } else if stalled {
                self.finish_command(VirtualizerCommandOutcome::NoProgress);
            }
        }
    }
    /// Cancel an active command and pause a follow session before explicit
    /// keyboard/drag navigation. Native wheel activity is observed separately.
    pub fn cancel_scroll(&mut self) {
        self.follow_tail_input_pending = false;
        if self.follow_tail.is_some() {
            self.follow_tail = Some(false);
        }
        self.finish_command(VirtualizerCommandOutcome::Cancelled);
    }
    /// Observe native user activity before the scroll owner applies its delta.
    /// Final frame geometry then decides whether the reader left or reached tail.
    pub fn user_scroll_activity(&mut self) {
        self.record_input_direction(None);
        self.follow_tail_input_pending = self.follow_tail.is_some();
        self.finish_command(VirtualizerCommandOutcome::Cancelled);
    }
    pub(super) fn settle_follow_tail_input(&mut self) {
        if self.follow_tail_input_pending {
            self.follow_tail_input_pending = false;
            if self.follow_tail.is_some() {
                self.follow_tail = Some(self.viewport.is_some_and(|viewport| {
                    let tail = (self.total_extent() - f64::from(viewport.height)).max(0.);
                    (-self.scroll.precise_offset().y - tail).abs() <= 1.0
                }));
            }
        }
    }
    pub fn has_scroll_command(&self) -> bool {
        self.pending.is_some()
    }
    fn capture_anchor(&mut self) {
        if let Some(viewport) = self.viewport {
            let offset = (-self.scroll.precise_offset().y).max(0.0);
            let snapshot = self.snapshot(offset, viewport).unwrap();
            self.commit_anchor(&snapshot);
        }
    }
    pub(super) fn observe_input(&mut self) {
        let offset = (-self.scroll.precise_offset().y).max(0.0);
        if self.last_offset.is_some_and(|old| old != offset) {
            let direction = self
                .last_offset
                .filter(|old| (offset - old).abs() > 0.01)
                .map(|old| {
                    if offset > old {
                        VirtualizerScrollDirection::Forward
                    } else {
                        VirtualizerScrollDirection::Backward
                    }
                });
            self.record_input_direction(direction);
            self.cancel_scroll();
            // Native input supersedes the last layout anchor. Retaining it would
            // let the next measurement pass snap a user-displaced viewport back.
            self.anchor = None;
            if self.follow_tail.is_some() {
                self.follow_tail_input_pending = true;
                let tail = self.viewport.map_or(0.0, |viewport| {
                    (self.total_extent() - f64::from(viewport.height)).max(0.0)
                });
                self.follow_tail = Some((offset - tail).abs() <= 1.0);
            }
        }
        self.last_offset = Some(offset);
    }
    fn restore_anchor(&mut self) {
        if self.follow_tail == Some(true) {
            if let Some(viewport) = self.viewport {
                let offset = (self.total_extent() - f64::from(viewport.height)).max(0.);
                self.scroll.set_precise_offset(gpui::point(0.0, -offset));
                self.last_offset = Some(offset);
            }
            return;
        }
        if let Some((key, intra, measured)) = &self.anchor
            && let Some(ix) = self.source.index_of(key)
            && let Some(viewport) = self.viewport
        {
            let snapshot = self.snapshot(0.0, viewport).unwrap();
            let item = snapshot.item(ix).unwrap();
            let (lane, local) = self.slots[ix];
            let before = if local == 0 {
                f64::from(self.layout.padding_start)
            } else {
                f64::from(self.layout.gap)
            };
            let after = if local + 1 == self.lanes[lane].indexes.len() {
                self.total_extent() - item.item.start() - f64::from(item.item.extent())
            } else {
                f64::from(self.layout.gap)
            };
            // An unmeasured slot is not content the reader has actually seen.
            // Preserve native input within that estimate instead of snapping
            // back when the first real height is smaller than its hint.
            let relative = match intra {
                VirtualizerAnchorPosition::Within(distance) if !measured => *distance,
                _ => intra.resolve(f64::from(item.item.extent()), before, after),
            };
            let offset = (item.item.start() + relative).clamp(
                0.0,
                (self.total_extent() - f64::from(viewport.height)).max(0.0),
            );
            self.scroll.set_precise_offset(gpui::point(0.0, -offset));
            self.last_offset = Some(offset);
        }
    }
    pub(super) fn observe_viewport(&mut self, viewport: Size<Pixels>) {
        let width = (viewport.width - self.column_gap * self.lanes.len().saturating_sub(1))
            .max(px(0.))
            / self.lanes.len() as f32;
        if self.dynamic && self.cross.is_some_and(|old| old != width) {
            for lane in &mut self.lanes {
                lane.geometry.invalidate_measurements();
            }
            self.revision += 1;
        }
        let first = self.viewport.is_none();
        self.cross = Some(width);
        self.viewport = Some(viewport);
        if first && self.anchor.is_some() {
            self.restore_anchor();
        }
    }
    pub(super) fn resolve_offset(&mut self) -> f64 {
        let viewport = self.viewport.unwrap();
        let old = (-self.scroll.precise_offset().y).max(0.0);
        let mut offset = old;
        if self.follow_tail == Some(true) {
            offset = (self.total_extent() - f64::from(viewport.height)).max(0.);
        }
        if self.pending.as_ref().is_some_and(|command| {
            matches!(&command.target,
                MasonryTarget::Key(key) if self.source.index_of(key).is_none())
        }) {
            self.finish_command(VirtualizerCommandOutcome::Missing);
        }
        if let Some(command) = &self.pending {
            let destination = match &command.target {
                MasonryTarget::Offset(target) => *target,
                MasonryTarget::Key(key) => {
                    let ix = self.source.index_of(key).unwrap();
                    let snapshot = self.snapshot(old, viewport).unwrap();
                    let item = snapshot.item(ix).unwrap();
                    let v = VirtualizerViewport::new(
                        &VirtualizerBinding::new(),
                        0,
                        if !self.dynamic
                            && let Some(motion) = &command.motion
                        {
                            motion.from
                        } else {
                            old
                        },
                        viewport.height,
                        viewport.width,
                    );
                    state::alignment_offset(
                        item.item(),
                        &v,
                        command.align,
                        self.layout.scroll_padding_start.into(),
                        self.layout.scroll_padding_end.into(),
                    )
                }
            }
            .clamp(
                0.0,
                (self.total_extent() - f64::from(viewport.height)).max(0.0),
            );
            let (sample, settled) = if !self.dynamic
                && let Some(motion) = &command.motion
            {
                super::command::sample_offset(
                    motion.options,
                    motion.elapsed,
                    motion.from,
                    destination,
                    old,
                    viewport.height.into(),
                )
            } else {
                (destination, true)
            };
            offset = sample;
            if let Some(motion) = self
                .pending
                .as_mut()
                .and_then(|command| command.motion.as_mut())
            {
                motion.motion_settled = settled;
            }
        }
        offset = offset.clamp(
            0.0,
            (self.total_extent() - f64::from(viewport.height)).max(0.0),
        );
        self.scroll.set_precise_offset(gpui::point(0.0, -offset));
        self.last_offset = Some(offset);
        offset
    }
    pub(super) fn commit_anchor(&mut self, snapshot: &VirtualMasonrySnapshot) {
        self.commit_anchor_items(
            snapshot.offset,
            snapshot
                .working_indices()
                .into_iter()
                .filter_map(|ix| snapshot.item(ix).ok()),
        );
    }
    pub(super) fn commit_anchor_items(
        &mut self,
        offset: f64,
        items: impl IntoIterator<Item = VirtualMasonryItem>,
    ) {
        self.anchor = items
            .into_iter()
            .filter(|item| {
                item.item.start() <= offset
                    && item.item.start() + f64::from(item.item.extent()) > offset
            })
            .min_by(|a, b| {
                a.item
                    .start()
                    .total_cmp(&b.item.start())
                    .then_with(|| a.item.index().cmp(&b.item.index()))
            })
            .map(|item| {
                (
                    item.item.key().clone(),
                    VirtualizerAnchorPosition::Within(offset - item.item.start()),
                    item.item.is_measured(),
                )
            })
            .or_else(|| self.blank_region_anchor(offset));
    }
    fn blank_region_anchor(
        &self,
        offset: f64,
    ) -> Option<(ElementId, VirtualizerAnchorPosition, bool)> {
        let mut best: Option<(u8, f64, usize, VirtualizerAnchorPosition)> = None;
        for lane in &self.lanes {
            if lane.indexes.is_empty() {
                continue;
            }
            let mut local = lane
                .geometry
                .index_at(offset - f64::from(self.layout.padding_start));
            let (mut start, mut extent, _) = lane.geometry.item(local)?;
            start += f64::from(self.layout.padding_start);
            if start + extent <= offset && local + 1 < lane.indexes.len() {
                local += 1;
                (start, extent, _) = lane.geometry.item(local)?;
                start += f64::from(self.layout.padding_start);
            }
            let (rank, order, position) = if offset < start {
                (1, start, VirtualizerAnchorPosition::Before(start - offset))
            } else if offset < start + extent {
                (0, start, VirtualizerAnchorPosition::Within(offset - start))
            } else {
                (
                    2,
                    -(start + extent),
                    VirtualizerAnchorPosition::After(offset - start - extent),
                )
            };
            let ix = lane.indexes[local];
            if best
                .as_ref()
                .is_none_or(|(r, o, i, _)| (rank, order, ix) < (*r, *o, *i))
            {
                best = Some((rank, order, ix, position));
            }
        }
        best.map(|(_, _, ix, position)| {
            let (lane, local) = self.slots[ix];
            let measured = self.lanes[lane].geometry.item(local).unwrap().2;
            (self.source.key_at(ix), position, measured)
        })
    }
    pub(super) fn focus_keys(
        &mut self,
        w: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> Vec<ElementId> {
        if self
            .removed_focus
            .iter()
            .any(|focus| focus.contains_focused(w, cx))
        {
            if let Some(fallback) = &self.focus_fallback {
                fallback.focus(w, cx);
            } else {
                w.blur(cx);
            }
        }
        self.removed_focus.clear();
        self.focus
            .iter()
            .filter(|(_, focus)| focus.contains_focused(w, cx))
            .map(|(key, _)| key.clone())
            .collect()
    }
    pub(super) fn budget(&self) -> usize {
        self.layout.measurement_budget
    }
    pub(super) fn claim_mount(&mut self, w: &gpui::Window, id: &gpui::GlobalElementId) {
        let current = (w.window_handle().window_id(), id.clone());
        assert!(
            self.mount.as_ref().is_none_or(|old| *old == current),
            "one masonry mount per state"
        );
        self.mount = Some(current);
    }
    pub(super) fn release_mount(&mut self) {
        if self.mount.take().is_some() {
            // A surviving controller must reject resource/measurement feedback
            // captured by the removed presentation, including after remount.
            self.revision += 1;
            self.cancel_scroll();
            self.last_offset = None;
            self.end_scrolling();
        }
    }
}
fn sizing_extents(
    sizing: &VirtualizerSizing,
    count: usize,
    range: std::ops::Range<usize>,
) -> Result<Vec<(Pixels, bool)>, VirtualizerError> {
    state::validate_sizing(sizing, count)?;
    let values: Vec<_> = match &sizing.0 {
        Sizing::Known(tree) => tree
            .items(range)
            .map(|(_, extent, _)| (Pixels::from(extent), true))
            .collect(),
        Sizing::Fixed(extent) => range.map(|_| (*extent, true)).collect(),
        Sizing::Measured(_) => range.map(|ix| (sizing.estimate_at(ix), false)).collect(),
    };
    if !values.iter().all(|(extent, _)| valid_positive(*extent)) {
        return Err(VirtualizerError::InvalidGeometry("masonry extents"));
    }
    Ok(values)
}
fn pack(
    extents: &[(Pixels, bool)],
    lane_count: usize,
    gap: f64,
) -> (Vec<Lane>, IndexVector<(usize, usize)>) {
    let mut heap: BinaryHeap<_> = (0..lane_count).map(|lane| LaneEnd(0.0, lane)).collect();
    let mut indexes = vec![IndexVector::default(); lane_count];
    let mut sizes = vec![Vec::new(); lane_count];
    let mut slots = IndexVector::default();
    for (ix, (extent, measured)) in extents.iter().copied().enumerate() {
        let LaneEnd(end, lane) = heap.pop().unwrap();
        slots.push((lane, indexes[lane].len()));
        indexes[lane].push(ix);
        sizes[lane].push((extent, measured));
        heap.push(LaneEnd(end + f64::from(extent) + gap, lane));
    }
    let lanes = indexes
        .into_iter()
        .zip(sizes)
        .map(|(indexes, sizes)| Lane {
            indexes,
            geometry: geometry::KnownGeometry::with_measurements(sizes, gap),
        })
        .collect();
    (lanes, slots)
}
