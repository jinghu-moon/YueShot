use super::{geometry::Geometry, *};
#[path = "updates.rs"]
mod updates;
pub use updates::*;
#[path = "cache.rs"]
mod cache;
pub use cache::*;
use gpui::{
    App, Bounds, Edges, FocusHandle, FollowMode, InteractiveElement as _, IntoElement as _,
    ListAlignment, ListOffset, ListState, ListViewportLayout, ParentElement as _, Point,
    ScrollHandle, Size, Styled as _, Window, div, px, size,
};
use gpui::{Context, Task};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Weak,
    time::Instant,
};

/// Local axis slots belonging to one actual scroll owner. Sharing this value
/// permits two independent axes, never two active controllers for the same axis.
#[derive(Clone, Default)]
pub struct VirtualizerBinding(Rc<RefCell<[Weak<()>; 2]>>);
impl VirtualizerBinding {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Immutable input from the actual owner, in positive logical coordinates.
#[derive(Clone)]
pub struct VirtualizerViewport {
    binding: VirtualizerBinding,
    revision: u64,
    input_revision: u64,
    offset: f64,
    extent: Pixels,
    cross: Pixels,
    origin: f64,
    scroll_extent: Option<f64>,
    frozen_prefix: usize,
    leading_occlusion: Pixels,
}
impl VirtualizerViewport {
    pub fn new(
        binding: &VirtualizerBinding,
        revision: u64,
        offset: f64,
        extent: Pixels,
        cross: Pixels,
    ) -> Self {
        Self {
            binding: binding.clone(),
            revision,
            input_revision: revision,
            offset,
            extent,
            cross,
            origin: 0.0,
            scroll_extent: None,
            frozen_prefix: 0,
            leading_occlusion: px(0.),
        }
    }
    // Frame-only Grid constraints; never rewrite the caller's layout padding.
    pub(super) fn with_frozen_prefix(mut self, count: usize, extent: Pixels) -> Self {
        self.frozen_prefix = count;
        self.leading_occlusion = extent;
        self
    }
    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }
    pub fn input_revision(mut self, revision: u64) -> Self {
        self.input_revision = revision;
        self
    }
    /// List start in its host's content coordinates (Attached composition).
    pub fn with_content_origin(mut self, origin: f64) -> Self {
        self.origin = origin;
        self
    }
    /// Actual host content extent, including regions before/after this list.
    pub fn with_scroll_extent(mut self, extent: f64) -> Self {
        self.scroll_extent = Some(extent);
        self
    }
    pub fn content_origin(&self) -> f64 {
        self.origin
    }
    pub fn offset(&self) -> f64 {
        self.offset
    }
    pub fn extent(&self) -> Pixels {
        self.extent
    }
    pub fn cross_extent(&self) -> Pixels {
        self.cross
    }
    pub(super) fn shares_host(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.binding.0, &other.binding.0)
    }
    pub fn input_revision_value(&self) -> u64 {
        self.input_revision
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }
}

/// Only working items are materialized; fixed snapshots remain O(1) in N.
#[derive(Clone, Debug)]
pub struct VirtualizerItem {
    index: usize,
    key: ElementId,
    start: f64,
    extent: Pixels,
    measured: bool,
}

/// Source identity for immutable content preparation, deliberately without frame
/// geometry. Reactive layout belongs in the returned view's ordinary Render.
#[derive(Clone, Debug)]
pub struct VirtualizerItemIdentity {
    index: usize,
    key: ElementId,
}
impl VirtualizerItemIdentity {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn key(&self) -> &ElementId {
        &self.key
    }
}
impl VirtualizerItem {
    pub fn identity(&self) -> VirtualizerItemIdentity {
        VirtualizerItemIdentity {
            index: self.index,
            key: self.key.clone(),
        }
    }
    pub(super) fn from_geometry(
        index: usize,
        key: ElementId,
        start: f64,
        extent: Pixels,
        measured: bool,
    ) -> Self {
        Self {
            index,
            key,
            start,
            extent,
            measured,
        }
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn key(&self) -> &ElementId {
        &self.key
    }
    pub fn start(&self) -> f64 {
        self.start
    }
    pub fn extent(&self) -> Pixels {
        self.extent
    }
    pub fn is_measured(&self) -> bool {
        self.measured
    }
}

#[derive(Clone)]
pub struct VirtualizerSnapshot {
    owner: Weak<()>,
    binding_revision: u64,
    generation: u64,
    geometry_revision: u64,
    content_revision: u64,
    viewport: VirtualizerViewport,
    axis: Axis,
    source: Rc<dyn VirtualizerSource>,
    geometry: Geometry,
    visible: Range<usize>,
    rendered: Range<usize>,
    retained: Rc<[ElementId]>,
    padding: f64,
    total: f64,
    gap: f64,
}
impl VirtualizerSnapshot {
    pub(super) fn is_frozen(&self, index: usize) -> bool {
        index < self.viewport.frozen_prefix
    }
    pub fn axis(&self) -> Axis {
        self.axis
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn geometry_revision(&self) -> u64 {
        self.geometry_revision
    }
    pub fn viewport_revision(&self) -> u64 {
        self.viewport.revision
    }
    pub fn visible_range(&self) -> Range<usize> {
        self.visible.clone()
    }
    pub fn rendered_range(&self) -> Range<usize> {
        self.rendered.clone()
    }
    pub fn retained_keys(&self) -> &[ElementId] {
        &self.retained
    }
    /// Sorted, unique union. No intervening rows are materialized for retained items.
    pub fn working_indices(&self) -> Vec<usize> {
        if self.retained.is_empty() && self.viewport.frozen_prefix == 0 {
            return self.rendered.clone().collect();
        }
        let mut indices: Vec<_> = self.rendered.clone().collect();
        if self.viewport.frozen_prefix > 0 {
            let range = self
                .geometry
                .range(-self.padding, self.viewport.extent.into());
            indices.extend(range.start..range.end.min(self.viewport.frozen_prefix));
        }
        indices.extend(
            self.retained
                .iter()
                .filter_map(|key| self.source.index_of(key)),
        );
        indices.sort_unstable();
        indices.dedup();
        indices
    }
    pub fn total_extent(&self) -> f64 {
        self.total
    }
    pub fn is_estimated(&self) -> bool {
        self.geometry.estimated()
    }
    pub fn viewport(&self) -> &VirtualizerViewport {
        &self.viewport
    }
    pub fn len(&self) -> usize {
        self.source.len()
    }
    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }
    /// Frozen geometry remains readable after a newer transaction. Mutable
    /// operations validate this snapshot separately and reject stale versions.
    pub fn item(&self, index: usize) -> Result<VirtualizerItem, VirtualizerError> {
        let (start, extent, measured) = self
            .geometry
            .item(index)
            .ok_or(VirtualizerError::MissingKey)?;
        self.item_from_geometry(index, start, extent, measured)
    }
    fn item_from_geometry(
        &self,
        index: usize,
        start: f64,
        extent: f64,
        measured: bool,
    ) -> Result<VirtualizerItem, VirtualizerError> {
        let key = self.source.key_at(index);
        if self.source.index_of(&key) != Some(index) {
            return Err(VirtualizerError::InvalidSource);
        }
        let extent = if matches!(self.geometry, Geometry::Measured(_)) && index + 1 < self.len() {
            (extent - self.gap).max(0.01)
        } else {
            extent
        };
        Ok(VirtualizerItem {
            index,
            key,
            start: start + self.padding,
            extent: extent.into(),
            measured,
        })
    }
    /// Sequential frozen geometry; Known seeks once then advances through leaf
    /// blocks. Invalid ranges fail before iteration, invalid domain keys per item.
    pub fn items(
        &self,
        range: Range<usize>,
    ) -> Result<
        impl Iterator<Item = Result<VirtualizerItem, VirtualizerError>> + '_,
        VirtualizerError,
    > {
        if range.start > range.end || range.end > self.len() {
            return Err(VirtualizerError::InvalidChange);
        }
        Ok(range.clone().zip(self.geometry.items(range)).map(
            move |(ix, (start, extent, measured))| {
                self.item_from_geometry(ix, start, extent, measured)
            },
        ))
    }
    /// Sorted working items. Remote retained items seek directly; intervening
    /// rows are neither iterated nor materialized.
    pub fn working_items(
        &self,
    ) -> impl Iterator<Item = Result<VirtualizerItem, VirtualizerError>> + '_ {
        // The ordinary contiguous window needs neither an index allocation nor
        // sorting. Sparse retained items keep the same sorted-union contract.
        let contiguous = self.retained.is_empty() && self.viewport.frozen_prefix == 0;
        let mut indexes = if contiguous {
            Vec::new()
        } else {
            self.working_indices()
        }
        .into_iter()
        .peekable();
        let mut run = self
            .items(if contiguous {
                self.rendered.clone()
            } else {
                0..0
            })
            .expect("validated working range");
        std::iter::from_fn(move || {
            loop {
                if let Some(item) = run.next() {
                    return Some(item);
                }
                let start = indexes.next()?;
                let mut end = start + 1;
                while indexes.peek() == Some(&end) {
                    indexes.next();
                    end += 1;
                }
                run = self.items(start..end).expect("validated working indices");
            }
        })
    }
    pub fn item_for_key(&self, key: &ElementId) -> Result<VirtualizerItem, VirtualizerError> {
        self.item(
            self.source
                .index_of(key)
                .ok_or(VirtualizerError::MissingKey)?,
        )
    }
}

/// An absolute proposal, never an accumulating delta. Only its axis component
/// is meaningful; the host merges axes, clamps and confirms the actual result.
#[derive(Clone)]
pub struct VirtualizerScrollRequest {
    owner: Weak<()>,
    binding_revision: u64,
    id: u64,
    generation: u64,
    geometry_revision: u64,
    viewport_revision: u64,
    input_revision: u64,
    axis: Axis,
    offset: f64,
    settled: bool,
}
impl VirtualizerScrollRequest {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn axis(&self) -> Axis {
        self.axis
    }
    pub fn offset(&self) -> f64 {
        self.offset
    }
    pub fn is_settled(&self) -> bool {
        self.settled
    }
}

struct Command {
    id: u64,
    target: CommandTarget,
    options: VirtualizerScrollOptions,
    progress: super::command::CommandProgress,
    from: f64,
    time_progressed: bool,
    measured_motion: Option<Box<super::command::MeasuredMotion>>,
}
enum CommandTarget {
    Key(ElementId),
    Offset(f64),
}

/// Retained one-axis behavior. Store as Entity<VirtualizerState>; the source,
/// geometry, native scroll owner and revisions are never mirrored in Yue.
pub struct VirtualizerState {
    axis: Axis,
    source: Rc<dyn VirtualizerSource>,
    sizing: VirtualizerSizing,
    layout: VirtualizerLayout,
    geometry: Geometry,
    measured: Option<ListState>,
    scroll: Option<ScrollHandle>,
    token: Rc<()>,
    binding: Option<VirtualizerBinding>,
    attachment: VirtualizerAttachment,
    binding_revision: u64,
    generation: u64,
    geometry_revision: u64,
    content_revision: u64,
    // Piecewise-constant epochs, allocated only by explicit content invalidation.
    // Invalidating a million consecutive rows needs two boundaries, not N keys.
    content_revisions: BTreeMap<usize, u64>,
    content_base_revision: u64,
    viewport: Option<VirtualizerViewport>,
    anchor: Option<(ElementId, VirtualizerAnchorPosition)>,
    pending_anchor: Option<f64>,
    // A measured source/layout transaction changes the native prefix under its
    // logical scroll index. That offset change is geometry, not user input.
    owned_geometry_offset: Option<f64>,
    command: Option<Command>,
    serial: u64,
    outstanding: Option<VirtualizerScrollRequest>,
    accepted: Option<u64>,
    focused: HashMap<ElementId, FocusHandle>,
    retained: Rc<[ElementId]>,
    explicit_retained: Rc<[ElementId]>,
    range_extractor: Option<VirtualizerRangeExtractor>,
    events: Vec<VirtualizerEvent>,
    last_range: Option<(u64, Range<usize>, Range<usize>)>,
    mount: Option<(gpui::WindowId, gpui::GlobalElementId)>,
    focus_fallback: Option<FocusHandle>,
    removed_focus: Vec<FocusHandle>,
    scroll_end_task: Option<Task<()>>,
    scroll_session: u64,
    observed_invalid_sizes: usize,
    reported_invalid_size: bool,
    reported_budget_exhaustion: bool,
    scrollbar_activity: Rc<Cell<u64>>,
    observed_scrollbar_activity: u64,
    scrolling: bool,
    direction: Option<VirtualizerScrollDirection>,
    last_scroll_at: Option<Instant>,
    clock: Option<Instant>,
    // Native Measured owns its own follow state. This slot serves arithmetic backends only.
    follow_tail: Option<bool>,
}
impl gpui::EventEmitter<VirtualizerEvent> for VirtualizerState {}

impl VirtualizerState {
    pub fn new(
        axis: Axis,
        source: Rc<dyn VirtualizerSource>,
        sizing: VirtualizerSizing,
    ) -> Result<Self, VirtualizerError> {
        Self::create(
            axis,
            source,
            sizing,
            VirtualizerAttachment::Owned,
            &VirtualizerBinding::new(),
        )
    }
    pub fn new_attached(
        axis: Axis,
        source: Rc<dyn VirtualizerSource>,
        sizing: VirtualizerSizing,
        binding: &VirtualizerBinding,
    ) -> Result<Self, VirtualizerError> {
        Self::create(
            axis,
            source,
            sizing,
            VirtualizerAttachment::Attached,
            binding,
        )
    }
    fn create(
        axis: Axis,
        source: Rc<dyn VirtualizerSource>,
        sizing: VirtualizerSizing,
        attachment: VirtualizerAttachment,
        binding: &VirtualizerBinding,
    ) -> Result<Self, VirtualizerError> {
        // Capabilities are checked before reading/allocating N-sized metadata.
        VirtualizerCapabilities::for_configuration(axis, sizing.kind())?;
        validate_sizing(&sizing, source.len())?;
        if binding.0.borrow()[axis_slot(axis)].upgrade().is_some() {
            return Err(VirtualizerError::BindingConflict);
        }
        let measured = match sizing.0 {
            Sizing::Measured(estimate) => {
                let list = ListState::new_with_uniform_height(
                    if sizing.1.is_some() { 0 } else { source.len() },
                    ListAlignment::Top,
                    px(0.),
                    estimate,
                );
                if sizing.1.is_some() {
                    let invalid = Cell::new(false);
                    list.remap_items(
                        source.len(),
                        |_| None,
                        |ix| {
                            let size = sizing.estimate_at(ix);
                            if valid_positive(size) {
                                size
                            } else {
                                invalid.set(true);
                                estimate
                            }
                        },
                    );
                    if invalid.get() {
                        return Err(VirtualizerError::InvalidGeometry("item estimate"));
                    }
                }
                list.set_layout_budget(1024);
                Some(list)
            }
            _ => None,
        };
        let geometry = make_geometry(
            &sizing,
            source.len(),
            &VirtualizerLayout::default(),
            measured.as_ref(),
        );
        let token = Rc::new(());
        binding.0.borrow_mut()[axis_slot(axis)] = Rc::downgrade(&token);
        let scroll = if attachment == VirtualizerAttachment::Owned && measured.is_none() {
            Some(ScrollHandle::new())
        } else {
            None
        };
        Ok(Self {
            axis,
            source,
            sizing,
            layout: VirtualizerLayout::default(),
            geometry,
            measured,
            scroll,
            token,
            binding: Some(binding.clone()),
            attachment,
            binding_revision: 1,
            generation: 1,
            geometry_revision: 1,
            content_revision: 0,
            content_revisions: BTreeMap::new(),
            content_base_revision: 0,
            viewport: None,
            anchor: None,
            pending_anchor: None,
            owned_geometry_offset: None,
            command: None,
            serial: 0,
            outstanding: None,
            accepted: None,
            focused: HashMap::new(),
            retained: Rc::from([]),
            explicit_retained: Rc::from([]),
            range_extractor: None,
            events: Vec::new(),
            last_range: None,
            mount: None,
            focus_fallback: None,
            removed_focus: Vec::new(),
            scroll_end_task: None,
            scroll_session: 0,
            observed_invalid_sizes: 0,
            reported_invalid_size: false,
            reported_budget_exhaustion: false,
            scrollbar_activity: Rc::new(Cell::new(0)),
            observed_scrollbar_activity: 0,
            scrolling: false,
            direction: None,
            last_scroll_at: None,
            clock: None,
            follow_tail: None,
        })
    }
    pub fn axis(&self) -> Axis {
        self.axis
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn capabilities(&self) -> VirtualizerCapabilities {
        VirtualizerCapabilities {
            axis: self.axis,
            sizing: self.sizing.kind(),
        }
    }
    pub fn attachment(&self) -> VirtualizerAttachment {
        self.attachment
    }
    pub fn binding(&self) -> Result<&VirtualizerBinding, VirtualizerError> {
        self.binding.as_ref().ok_or(VirtualizerError::Detached)
    }
    pub fn viewport(&self) -> Option<&VirtualizerViewport> {
        self.viewport.as_ref()
    }
    pub fn total_extent(&self) -> f64 {
        self.geometry.total()
            + f64::from(self.layout.padding_start)
            + f64::from(self.layout.padding_end)
    }
    pub fn sizing_kind(&self) -> VirtualizerSizingKind {
        self.sizing.kind()
    }
    pub fn take_events(&mut self) -> Vec<VirtualizerEvent> {
        std::mem::take(&mut self.events)
    }
    pub fn is_scrolling(&self) -> bool {
        self.scrolling
    }
    pub fn scroll_direction(&self) -> Option<VirtualizerScrollDirection> {
        self.direction
    }
    pub fn has_scroll_command(&self) -> bool {
        self.command.is_some()
    }

    /// Attached owners may poll this from their existing idle/frame mechanism.
    /// Does not request frames or install another input handler.
    pub fn poll_scroll_idle(&mut self, now: Instant) {
        if self
            .last_scroll_at
            .is_some_and(|last| now.saturating_duration_since(last) >= Duration::from_millis(120))
        {
            self.end_scrolling();
        }
    }
    fn observe_offset(&mut self, offset: f64, now: Instant) {
        if let Some(old) = &self.viewport {
            let delta = offset - old.offset;
            if delta.abs() > 0.01 {
                let direction = Some(if delta > 0.0 {
                    VirtualizerScrollDirection::Forward
                } else {
                    VirtualizerScrollDirection::Backward
                });
                if !self.scrolling || self.direction != direction {
                    self.events.push(VirtualizerEvent::ScrollStateChanged {
                        is_scrolling: true,
                        direction,
                    });
                }
                self.scrolling = true;
                self.direction = direction;
                self.last_scroll_at = Some(now);
            }
        }
    }
    fn end_scrolling(&mut self) {
        if self.scrolling {
            self.scrolling = false;
            self.direction = None;
            self.last_scroll_at = None;
            self.events.push(VirtualizerEvent::ScrollStateChanged {
                is_scrolling: false,
                direction: None,
            });
            self.events.push(VirtualizerEvent::ScrollEnd);
        }
    }

    pub fn detach(&mut self) {
        self.scroll_end_task = None;
        self.end_scrolling();
        if let Some(binding) = self.binding.take() {
            binding.0.borrow_mut()[axis_slot(self.axis)] = Weak::new();
        }
        self.binding_revision += 1;
        self.outstanding = None;
        self.viewport = None;
        self.mount = None;
        self.finish_command(VirtualizerCommandOutcome::Cancelled);
    }
    /// Rebind an explicitly detached state, preserving its fixed axis/backend.
    pub fn bind(&mut self, binding: &VirtualizerBinding) -> Result<(), VirtualizerError> {
        if self.binding.is_some() {
            return Err(VirtualizerError::AlreadyBound);
        }
        let mut slots = binding.0.borrow_mut();
        if slots[axis_slot(self.axis)].upgrade().is_some() {
            return Err(VirtualizerError::BindingConflict);
        }
        slots[axis_slot(self.axis)] = Rc::downgrade(&self.token);
        self.binding = Some(binding.clone());
        self.binding_revision += 1;
        Ok(())
    }
    pub(super) fn frozen_extent(&self, count: usize) -> f64 {
        if count == 0 || self.source.is_empty() {
            return 0.0;
        }
        if count >= self.source.len() {
            return self.total_extent();
        }
        self.geometry.item(count).unwrap().0 + f64::from(self.layout.padding_start)
    }
    pub fn set_layout(&mut self, layout: VirtualizerLayout) -> Result<(), VirtualizerError> {
        if ![
            layout.gap,
            layout.padding_start,
            layout.padding_end,
            layout.overscan,
            layout.scroll_padding_start,
            layout.scroll_padding_end,
        ]
        .into_iter()
        .all(valid_nonnegative)
            || !layout.valid_overscan()
            || layout.measurement_budget == 0
        {
            return Err(VirtualizerError::InvalidGeometry("layout"));
        }
        if let Sizing::Measured(estimate) = self.sizing.0
            && !valid_positive(estimate + layout.gap)
        {
            return Err(VirtualizerError::InvalidGeometry("estimate plus gap"));
        }
        // The measured tree owns content heights, while the viewport adapter
        // supplies gap/padding slots. No second measured height array is kept.
        let geometry_changed = self.layout.gap != layout.gap
            || self.layout.padding_start != layout.padding_start
            || self.layout.padding_end != layout.padding_end
            || self.layout.revision != layout.revision;
        if self.layout.scroll_padding_start != layout.scroll_padding_start
            || self.layout.scroll_padding_end != layout.scroll_padding_end
        {
            // The target changed, not the measured geometry. Existing request
            // identity validation rejects this proposal until prepare runs again.
            self.outstanding = None;
        }
        if let Some(list) = &self.measured {
            list.set_overdraw(layout.overscan);
            list.set_layout_budget(layout.measurement_budget);
        }
        if geometry_changed {
            if let Some(list) = &self.measured {
                if self.layout.gap != layout.gap {
                    let old_gap = self.layout.gap;
                    list.remap_items(
                        self.source.len(),
                        |ix| {
                            let delta = if ix + 1 < self.source.len() {
                                layout.gap - old_gap
                            } else {
                                px(0.)
                            };
                            Some((ix, delta))
                        },
                        |ix| {
                            self.sizing.estimate_at(ix)
                                + if ix + 1 < self.source.len() {
                                    layout.gap
                                } else {
                                    px(0.)
                                }
                        },
                    );
                }
                if self.layout.revision != layout.revision {
                    list.remeasure_items(0..self.source.len());
                }
            }
            self.layout = layout;
            self.geometry = make_geometry(
                &self.sizing,
                self.source.len(),
                &self.layout,
                self.measured.as_ref(),
            );
            self.changed_geometry();
            self.restore_anchor();
        } else {
            self.layout = layout;
        }
        Ok(())
    }
    pub fn replace_source(
        &mut self,
        source: Rc<dyn VirtualizerSource>,
        sizing: VirtualizerSizing,
        change: VirtualizerChange,
    ) -> Result<(), VirtualizerError> {
        if sizing.kind() != self.sizing.kind() {
            return Err(VirtualizerError::Unsupported(
                "reconstruct state to change sizing mode",
            ));
        }
        validate_sizing(&sizing, source.len())?;
        if let Sizing::Measured(estimate) = sizing.0
            && !valid_positive(estimate + self.layout.gap)
        {
            return Err(VirtualizerError::InvalidGeometry("estimate plus gap"));
        }
        match &change {
            VirtualizerChange::Splice { old, inserted }
                if old.start > old.end
                    || old.end > self.source.len()
                    || (self.source.len() - old.len()).checked_add(*inserted)
                        != Some(source.len()) =>
            {
                return Err(VirtualizerError::InvalidChange);
            }
            VirtualizerChange::Reorder if source.len() != self.source.len() => {
                return Err(VirtualizerError::InvalidChange);
            }
            _ => {}
        }
        // Splice promises an unchanged prefix/suffix, including their hints.
        // Generic replacement evaluates all hints; both validate before mutation.
        let measured_hints = if sizing.1.is_some() {
            let range = match &change {
                VirtualizerChange::Splice { old, inserted } => old.start..old.start + inserted,
                _ => 0..source.len(),
            };
            let values: Vec<_> = range.map(|ix| sizing.estimate_at(ix)).collect();
            if !values
                .iter()
                .copied()
                .all(|size| valid_positive(size) && valid_positive(size + self.layout.gap))
            {
                return Err(VirtualizerError::InvalidGeometry("item estimate"));
            }
            Some(values)
        } else {
            None
        };
        let preserve = !matches!(
            change,
            VirtualizerChange::Replace {
                preserve_anchor: false
            }
        );
        self.removed_focus.extend(
            self.focused
                .iter()
                .filter(|(key, _)| source.index_of(key).is_none())
                .map(|(_, handle)| handle.clone()),
        );
        // Missing anchors choose the old successor then predecessor. This cold
        // transaction may search a deletion; stable scrolling never scans N.
        if preserve
            && let Some((key, _)) = &self.anchor
            && source.index_of(key).is_none()
        {
            let old = self.source.index_of(key).unwrap_or(0);
            self.anchor = (old..self.source.len())
                .chain((0..old).rev())
                .find_map(|index| {
                    let key = self.source.key_at(index);
                    source
                        .index_of(&key)
                        .map(|_| (key, VirtualizerAnchorPosition::Within(0.0)))
                });
        }
        if let Some(list) = &self.measured {
            let Sizing::Measured(estimate) = sizing.0 else {
                unreachable!()
            };
            let hints = &measured_hints;
            let gap = self.layout.gap;
            let old_count = self.source.len();
            let old_geometry = list.geometry();
            let same_estimate = matches!(self.sizing.0, Sizing::Measured(old) if old == estimate);
            list.set_default_item_height(estimate + gap);
            // Splices retain the native prefix/suffix, including per-item hints.
            // Other transactions remap native entries by stable key/content revision.
            if let VirtualizerChange::Splice { old, inserted } = &change {
                let last = old_count.checked_sub(1).and_then(|ix| {
                    old_geometry
                        .item(ix)
                        .map(|item| (self.source.key_at(ix), item))
                });
                list.splice_with_size_hints(
                    old.clone(),
                    (0..*inserted).map(|local| {
                        let ix = old.start + local;
                        hints.as_ref().map_or(estimate, |hints| hints[local])
                            + if ix + 1 < source.len() { gap } else { px(0.) }
                    }),
                );
                if gap > px(0.) {
                    if let Some((key, item)) = &last
                        && let Some(ix) = source.index_of(key)
                        && ix + 1 < source.len()
                    {
                        list.set_item_sizes(
                            &[(ix, size(px(0.), item.extent() + gap))],
                            item.is_measured(),
                        );
                    }
                    if let Some(ix) = source.len().checked_sub(1)
                        && !(old.start..old.start + inserted).contains(&ix)
                        && last
                            .as_ref()
                            .is_none_or(|(key, _)| *key != source.key_at(ix))
                    {
                        let item = list.geometry().item(ix).unwrap();
                        list.set_item_sizes(
                            &[(ix, size(px(0.), (item.extent() - gap).max(px(0.01))))],
                            item.is_measured(),
                        );
                    }
                }
            } else {
                list.remap_items(
                    source.len(),
                    |ix| {
                        let old_ix = self.source.index_of(&source.key_at(ix))?;
                        if self.source.revision_at(old_ix) != source.revision_at(ix) {
                            return None;
                        }
                        if (!same_estimate || hints.is_some())
                            && !old_geometry.item(old_ix)?.is_measured()
                        {
                            return None;
                        }
                        let old_gap = if old_ix + 1 < old_count { gap } else { px(0.) };
                        let new_gap = if ix + 1 < source.len() { gap } else { px(0.) };
                        Some((old_ix, new_gap - old_gap))
                    },
                    |ix| {
                        hints.as_ref().map_or(estimate, |hints| hints[ix])
                            + if ix + 1 < source.len() { gap } else { px(0.) }
                    },
                );
            }
        }
        self.source = source;
        self.sizing = sizing;
        self.generation += 1;
        // Dropping an index domain must not restore an old prepared-view stamp.
        // Pay one working-set refresh here, not on every subsequent append.
        if !self.content_revisions.is_empty() {
            self.content_revision += 1;
            self.content_base_revision = self.content_revision;
            self.content_revisions.clear();
        }
        self.reported_invalid_size = false;
        self.reported_budget_exhaustion = false;
        self.geometry = make_geometry(
            &self.sizing,
            self.source.len(),
            &self.layout,
            self.measured.as_ref(),
        );
        self.changed_geometry();
        let outcome = if self
            .command
            .as_ref()
            .is_some_and(|command| matches!(&command.target, CommandTarget::Key(key) if self.source.index_of(key).is_none()))
        {
            VirtualizerCommandOutcome::Missing
        } else {
            VirtualizerCommandOutcome::Cancelled
        };
        self.finish_command(outcome);
        if preserve {
            self.restore_anchor();
        } else {
            self.set_follow_tail(false)?;
            self.anchor = None;
            self.pending_anchor = Some(0.0);
        }
        self.focused
            .retain(|key, _| self.source.index_of(key).is_some());
        for (key, focus) in &self.focused {
            if let (Some(list), Some(index)) = (&self.measured, self.source.index_of(key)) {
                list.set_item_focus_handle(index, Some(focus.clone()));
            }
        }
        Ok(())
    }
    /// Expires this controller's live content and feedback. For persistent
    /// measurement-cache invalidation also advance source.revision_at.
    pub fn invalidate_items(&mut self, range: Range<usize>) -> Result<(), VirtualizerError> {
        if range.start > range.end || range.end > self.source.len() {
            return Err(VirtualizerError::InvalidChange);
        }
        if range.is_empty() {
            return Ok(());
        }
        let after = self.item_content_revision(range.end);
        self.content_revision += 1;
        let boundaries: Vec<_> = self
            .content_revisions
            .range(range.start..=range.end)
            .map(|(&ix, _)| ix)
            .collect();
        for ix in boundaries {
            self.content_revisions.remove(&ix);
        }
        self.content_revisions
            .insert(range.start, self.content_revision);
        if range.end < self.source.len() {
            self.content_revisions.insert(range.end, after);
        }
        if let Some(list) = &self.measured {
            list.remeasure_items(range);
            self.geometry = Geometry::Measured(list.geometry());
            self.changed_geometry();
            self.restore_anchor();
        }
        Ok(())
    }
    /// Late content/resource callbacks can validate their captured revision
    /// before invalidating. Measurement itself remains synchronous native work.
    pub fn invalidate_items_from_snapshot(
        &mut self,
        snapshot: &VirtualizerSnapshot,
        range: Range<usize>,
    ) -> Result<(), VirtualizerError> {
        self.validate_snapshot(snapshot)?;
        self.invalidate_items(range)
    }
    pub fn set_focus_fallback(&mut self, handle: Option<FocusHandle>) {
        self.focus_fallback = handle;
    }
    pub fn set_retained_keys(&mut self, keys: Vec<ElementId>) -> Result<(), VirtualizerError> {
        if keys.iter().any(|key| self.source.index_of(key).is_none()) {
            return Err(VirtualizerError::MissingKey);
        }
        let mut keys = keys;
        keys.sort_by_key(|key| self.source.index_of(key));
        keys.dedup();
        self.explicit_retained = keys.into();
        Ok(())
    }
    pub fn set_range_extractor(&mut self, extractor: Option<VirtualizerRangeExtractor>) {
        self.range_extractor = extractor;
    }
    pub fn set_focus_handle(
        &mut self,
        key: ElementId,
        handle: Option<FocusHandle>,
    ) -> Result<(), VirtualizerError> {
        let index = self
            .source
            .index_of(&key)
            .ok_or(VirtualizerError::MissingKey)?;
        if handle.as_ref().is_some_and(|handle| {
            self.focused
                .iter()
                .any(|(other, focus)| *other != key && focus == handle)
        }) {
            return Err(VirtualizerError::InvalidSource);
        }
        if let Some(list) = &self.measured {
            list.set_item_focus_handle(index, handle.clone());
        }
        match handle {
            Some(handle) => {
                self.focused.insert(key, handle);
            }
            None => {
                self.focused.remove(&key);
            }
        }
        Ok(())
    }
    pub fn update_focus(&mut self, window: &mut Window, cx: &mut App) {
        let repair_focus = self
            .removed_focus
            .iter()
            .any(|handle| handle.contains_focused(window, cx));
        self.removed_focus.clear();
        if repair_focus {
            if let Some(handle) = &self.focus_fallback {
                handle.focus(window, cx);
            } else {
                window.blur(cx);
            }
        }
        let keys: Vec<_> = self
            .focused
            .iter()
            .filter(|(_, handle)| handle.contains_focused(window, cx))
            .map(|(key, _)| key.clone())
            .collect();
        if keys.as_slice() != self.retained.as_ref() {
            self.retained = keys.into();
        }
    }
    pub fn set_follow_tail(&mut self, follow: bool) -> Result<(), VirtualizerError> {
        if let Some(list) = &self.measured {
            list.set_follow_mode(if follow {
                FollowMode::Tail
            } else {
                FollowMode::Normal
            });
        } else {
            self.follow_tail = follow.then_some(true);
        }
        Ok(())
    }
    pub fn is_following_tail(&self) -> bool {
        self.measured
            .as_ref()
            .map_or(self.follow_tail == Some(true), ListState::is_following_tail)
    }
    pub fn scroll_to_key(
        &mut self,
        key: ElementId,
        options: VirtualizerScrollOptions,
    ) -> Result<u64, VirtualizerError> {
        if self.source.index_of(&key).is_none() {
            return Err(VirtualizerError::MissingKey);
        }
        self.start_command(CommandTarget::Key(key), options)
    }
    pub fn scroll_to_offset(
        &mut self,
        offset: f64,
        options: VirtualizerScrollOptions,
    ) -> Result<u64, VirtualizerError> {
        if !offset.is_finite() || offset < 0.0 {
            return Err(VirtualizerError::InvalidGeometry("scroll offset"));
        }
        self.start_command(CommandTarget::Offset(offset), options)
    }
    pub fn scroll_by(
        &mut self,
        delta: f64,
        options: VirtualizerScrollOptions,
    ) -> Result<u64, VirtualizerError> {
        if !delta.is_finite() {
            return Err(VirtualizerError::InvalidGeometry("scroll delta"));
        }
        let offset = self
            .viewport
            .as_ref()
            .ok_or(VirtualizerError::InvalidGeometry(
                "scroll_by requires viewport",
            ))?
            .offset;
        self.scroll_to_offset((offset + delta).max(0.0), options)
    }
    fn start_command(
        &mut self,
        target: CommandTarget,
        options: VirtualizerScrollOptions,
    ) -> Result<u64, VirtualizerError> {
        if let VirtualizerMotion::Smooth {
            duration,
            max_distance,
        }
        | VirtualizerMotion::JumpThenSmooth {
            duration,
            max_distance,
        } = options.motion
        {
            if duration.is_zero() || !valid_positive(max_distance) {
                return Err(VirtualizerError::InvalidGeometry("smooth bounds"));
            }
        }
        if self.measured.is_none()
            && matches!(options.motion, VirtualizerMotion::JumpThenSmooth { .. })
        {
            return Err(VirtualizerError::Unsupported(
                "jump-then-smooth requires measured list",
            ));
        }
        self.finish_command(VirtualizerCommandOutcome::Cancelled);
        if let Some(list) = &self.measured {
            list.pause_following_tail();
        }
        if self.follow_tail.is_some() {
            self.follow_tail = Some(false);
        }
        self.serial += 1;
        self.command = Some(Command {
            id: self.serial,
            target,
            options,
            progress: super::command::CommandProgress::default(),
            from: self.viewport.as_ref().map_or(0.0, |v| v.offset),
            time_progressed: false,
            measured_motion: (self.measured.is_some()
                && !matches!(options.motion, VirtualizerMotion::Instant))
            .then(|| Box::default()),
        });
        self.outstanding = None;
        Ok(self.serial)
    }
    pub fn cancel_scroll(&mut self) {
        self.outstanding = None;
        self.finish_command(VirtualizerCommandOutcome::Cancelled);
    }
    pub(super) fn settle_motion(&mut self) {
        if let Some(command) = &mut self.command {
            command.options.reduced_motion = true;
        }
    }
    /// The actual input owner calls this after native user scrolling. This is
    /// observation only: it neither handles a wheel delta nor writes an offset.
    /// The single cancellable idle task reports one semantic ScrollEnd.
    pub fn user_scroll_activity(&mut self, cx: &mut Context<Self>) {
        self.cancel_scroll();
        self.pending_anchor = None;
        if !self.scrolling {
            self.scrolling = true;
            self.events.push(VirtualizerEvent::ScrollStateChanged {
                is_scrolling: true,
                direction: self.direction,
            });
            cx.notify();
        }
        self.last_scroll_at = Some(cx.background_executor().now());
        self.arm_scroll_idle(cx);
    }
    /// Entity-backed Attached owners call after submitting the observed viewport;
    /// pure owners may instead poll_scroll_idle using their own clock.
    pub fn arm_scroll_idle(&mut self, cx: &mut Context<Self>) {
        if !self.scrolling || self.scroll_end_task.is_some() {
            return;
        }
        self.scroll_session += 1;
        let session = self.scroll_session;
        let binding_revision = self.binding_revision;
        self.scroll_end_task = Some(cx.spawn(async move |state, cx| {
            let mut delay = Duration::from_millis(120);
            loop {
                cx.background_executor().timer(delay).await;
                let next = state
                    .update(cx, |state, cx| {
                        if state.binding.is_none()
                            || state.binding_revision != binding_revision
                            || state.scroll_session != session
                        {
                            return None;
                        }
                        let elapsed = state.last_scroll_at.map(|last| {
                            cx.background_executor()
                                .now()
                                .saturating_duration_since(last)
                        });
                        if state.scrolling
                            && let Some(elapsed) = elapsed
                            && elapsed < Duration::from_millis(120)
                        {
                            return Some(Duration::from_millis(120) - elapsed);
                        }
                        state.end_scrolling();
                        state.scroll_end_task = None;
                        for event in state.take_events() {
                            cx.emit(event);
                        }
                        cx.notify();
                        None
                    })
                    .ok()
                    .flatten();
                let Some(next) = next else {
                    break;
                };
                delay = next;
            }
        }));
    }
    pub(super) fn observe_scrollbar_activity(&mut self, cx: &mut Context<Self>) {
        let revision = self.scrollbar_activity.get();
        if revision != self.observed_scrollbar_activity {
            self.observed_scrollbar_activity = revision;
            self.user_scroll_activity(cx);
        }
    }

    /// Pure headless geometry query. Does not notify, render or alter offset.
    pub fn snapshot(
        &self,
        viewport: &VirtualizerViewport,
    ) -> Result<VirtualizerSnapshot, VirtualizerError> {
        self.validate_viewport(viewport)?;
        let padding = viewport.origin + f64::from(self.layout.padding_start);
        let offset = viewport.offset - padding + f64::from(viewport.leading_occlusion);
        let extent = f64::from(viewport.extent - viewport.leading_occlusion).max(0.);
        let has_area = extent > 0.0 && viewport.cross > gpui::px(0.);
        let range = |offset: f64, extent: f64| {
            let mut range = self.geometry.range(offset, extent);
            range.start = range
                .start
                .max(viewport.frozen_prefix.min(self.source.len()));
            range.end = range.end.max(range.start);
            // Native measured slots include the trailing gap; visibility is
            // about content, just as it is on the Fixed and Known paths.
            if matches!(self.geometry, Geometry::Measured(_))
                && !range.is_empty()
                && let Some((start, extent, _)) = self.geometry.item(range.start)
                && start + extent
                    - if range.start + 1 < self.source.len() {
                        f64::from(self.layout.gap)
                    } else {
                        0.0
                    }
                    <= offset
            {
                range.start += 1;
                range.end = range.end.max(range.start);
            }
            range
        };
        let visible = if has_area {
            range(offset, extent)
        } else {
            0..0
        };
        let (before, after) = self.layout.overscan_edges(self.scroll_direction());
        let rendered = if has_area {
            range(
                offset - f64::from(before),
                extent + f64::from(before + after),
            )
        } else {
            0..0
        };
        let retained = if self.explicit_retained.is_empty() && self.range_extractor.is_none() {
            self.retained.clone()
        } else {
            let mut keys = self.retained.to_vec();
            keys.extend(
                self.explicit_retained
                    .iter()
                    .filter(|key| self.source.index_of(key).is_some())
                    .cloned(),
            );
            if let Some(extractor) = &self.range_extractor {
                for ix in extractor(&VirtualizerRange {
                    visible: visible.clone(),
                    rendered: rendered.clone(),
                    count: self.source.len(),
                }) {
                    if ix >= self.source.len() {
                        return Err(VirtualizerError::InvalidChange);
                    }
                    keys.push(self.source.key_at(ix));
                }
            }
            keys.sort_by_key(|key| self.source.index_of(key));
            keys.dedup();
            keys.into()
        };
        Ok(VirtualizerSnapshot {
            owner: Rc::downgrade(&self.token),
            binding_revision: self.binding_revision,
            generation: self.generation,
            geometry_revision: self.geometry_revision,
            content_revision: self.content_revision,
            viewport: viewport.clone(),
            axis: self.axis,
            source: self.source.clone(),
            geometry: self.geometry.clone(),
            visible,
            rendered,
            retained,
            padding,
            total: self.total_extent(),
            gap: self.layout.gap.into(),
        })
    }
    /// Synchronous batch calculation. User input cancels obsolete programmatic
    /// motion; geometry corrections are returned before painting, not as events.
    pub fn prepare(
        &mut self,
        viewport: &VirtualizerViewport,
        now: Instant,
    ) -> Result<(VirtualizerSnapshot, Option<VirtualizerScrollRequest>), VirtualizerError> {
        self.validate_viewport(viewport)?;
        self.validate_current_viewport(viewport)?;
        let time_progressed = self.clock.is_some_and(|previous| now > previous);
        self.clock = Some(now);
        self.observe_offset(viewport.offset, now);
        self.poll_scroll_idle(now);
        if self.layout.reverse_overscan.is_some()
            && let Some(list) = &self.measured
        {
            let (before, after) = self.layout.overscan_edges(self.scroll_direction());
            list.set_overdraw_edges(before, after);
        }
        if self.viewport.as_ref().is_some_and(|old| {
            old.input_revision != viewport.input_revision
                && (old.offset - viewport.offset).abs() > 0.01
        }) {
            self.cancel_scroll();
            self.pending_anchor = None;
            if let Some(list) = &self.measured {
                list.pause_following_tail();
            }
            if self.follow_tail.is_some() {
                self.follow_tail = Some(false);
            }
        }
        self.viewport = Some(viewport.clone());
        if let Some(command) = &mut self.command {
            let exceeded = if let Some(motion) = &command.measured_motion {
                command
                    .progress
                    .advance_with_motion(now, motion.is_animating(now))
            } else {
                command.progress.advance(now)
            };
            if self.measured.is_some() && exceeded {
                self.finish_command(VirtualizerCommandOutcome::NoProgress);
            }
        }
        let snapshot = self.snapshot(viewport)?;
        if let Some(Command {
            target: CommandTarget::Key(key),
            ..
        }) = &self.command
            && let Ok(item) = snapshot.item_for_key(key)
            && ((item.index < viewport.frozen_prefix
                && (item.start >= f64::from(viewport.extent)
                    || item.start + f64::from(item.extent) <= 0.))
                || (item.index >= viewport.frozen_prefix
                    && viewport.frozen_prefix > 0
                    && viewport.leading_occlusion >= viewport.extent))
        {
            self.finish_command(VirtualizerCommandOutcome::NoProgress);
        }
        let mut target = if self.follow_tail == Some(true) {
            Some(self.scroll_limit(viewport))
        } else {
            self.pending_anchor
        };
        let mut settled = true;
        let mut motion_progressed = false;
        if let Some(command) = &self.command {
            let destination = match &command.target {
                CommandTarget::Key(key) => snapshot.item_for_key(key).map(|item| {
                    let offset = alignment_offset(
                        &item,
                        viewport,
                        command.options.align,
                        f64::from(self.layout.scroll_padding_start),
                        f64::from(self.layout.scroll_padding_end),
                    );
                    (offset, item.measured)
                }),
                CommandTarget::Offset(offset) => Ok((*offset, true)),
            };
            match destination {
                Ok((destination, measured)) => {
                    let destination = destination.clamp(0.0, self.scroll_limit(viewport));
                    target = Some(destination);
                    if command.measured_motion.is_some() {
                        let options = command.options;
                        let padding = viewport.origin + f64::from(self.layout.padding_start);
                        let start = viewport.offset.min(destination) - padding;
                        let end =
                            viewport.offset.max(destination) + f64::from(viewport.extent) - padding;
                        let range = self.geometry.range(start, (end - start).max(0.));
                        let known = viewport.extent > gpui::px(0.)
                            && range.len() <= self.layout.measurement_budget
                            && self.geometry.items(range).all(|(_, _, measured)| measured);
                        let (offset, complete, advanced) = self
                            .command
                            .as_mut()
                            .unwrap()
                            .measured_motion
                            .as_mut()
                            .unwrap()
                            .sample(
                                options,
                                now,
                                super::command::MotionGeometry {
                                    current: viewport.offset,
                                    destination,
                                    extent: viewport.extent.into(),
                                    known,
                                    revision: self.geometry_revision,
                                },
                            );
                        target = Some(offset);
                        settled = complete;
                        motion_progressed = advanced;
                    } else if self.capabilities().supports_smooth() {
                        let (offset, complete) = super::command::sample_offset(
                            command.options,
                            now.saturating_duration_since(command.progress.started().unwrap()),
                            command.from,
                            destination,
                            viewport.offset,
                            viewport.extent.into(),
                        );
                        target = Some(offset);
                        settled = complete;
                        motion_progressed = !complete && time_progressed;
                    }
                    settled &= measured;
                }
                Err(_) => {
                    self.finish_command(VirtualizerCommandOutcome::Missing);
                    target = None;
                }
            }
        }
        if let Some(command) = &mut self.command {
            command.time_progressed = motion_progressed;
        }
        let target = target.map(|target| target.clamp(0.0, self.scroll_limit(viewport)));
        let request = if let Some(target) = target {
            self.serial += 1;
            let request = VirtualizerScrollRequest {
                owner: Rc::downgrade(&self.token),
                binding_revision: self.binding_revision,
                id: self.serial,
                generation: self.generation,
                geometry_revision: self.geometry_revision,
                viewport_revision: viewport.revision,
                input_revision: viewport.input_revision,
                axis: self.axis,
                offset: target,
                settled,
            };
            self.outstanding = Some(request.clone());
            Some(request)
        } else {
            None
        };
        Ok((snapshot, request))
    }
    pub fn validate_snapshot(
        &self,
        snapshot: &VirtualizerSnapshot,
    ) -> Result<(), VirtualizerError> {
        self.validate_viewport(&snapshot.viewport)?;
        if !snapshot.owner.ptr_eq(&Rc::downgrade(&self.token))
            || snapshot.binding_revision != self.binding_revision
            || snapshot.generation != self.generation
            || snapshot.geometry_revision != self.geometry_revision
            || snapshot.content_revision != self.content_revision
            || self.viewport.as_ref().is_some_and(|current| {
                current.revision > snapshot.viewport.revision
                    || current.input_revision > snapshot.viewport.input_revision
            })
        {
            return Err(VirtualizerError::StaleSnapshot);
        }
        Ok(())
    }
    pub fn validate_request(
        &self,
        request: &VirtualizerScrollRequest,
        viewport: &VirtualizerViewport,
    ) -> Result<(), VirtualizerError> {
        self.validate_viewport(viewport)?;
        self.validate_current_viewport(viewport)
            .map_err(|_| VirtualizerError::StaleRequest)?;
        if self.accepted == Some(request.id) {
            return Err(VirtualizerError::RequestAlreadyAccepted);
        }
        if !request.owner.ptr_eq(&Rc::downgrade(&self.token))
            || request.binding_revision != self.binding_revision
            || request.generation != self.generation
            || request.geometry_revision != self.geometry_revision
            || request.viewport_revision != viewport.revision
            || request.input_revision != viewport.input_revision
            || self
                .outstanding
                .as_ref()
                .is_none_or(|current| current.id != request.id)
        {
            return Err(VirtualizerError::StaleRequest);
        }
        Ok(())
    }
    pub fn confirm(
        &mut self,
        request: &VirtualizerScrollRequest,
        applied_offset: f64,
        viewport: &VirtualizerViewport,
    ) -> Result<(), VirtualizerError> {
        self.validate_request(request, viewport)?;
        if !applied_offset.is_finite() || applied_offset < 0.0 {
            return Err(VirtualizerError::InvalidGeometry("applied offset"));
        }
        self.accepted = Some(request.id);
        self.outstanding = None;
        self.pending_anchor = None;
        if let Some(command) = &mut self.command {
            let settled = request.settled && (applied_offset - request.offset).abs() <= 0.5;
            // A small legitimate timed sample is not an unresponsive host.
            // After motion completes, actual target acceptance still governs.
            let stalled = command.progress.confirm(
                applied_offset,
                settled
                    || command.time_progressed
                        && !request.settled
                        && (applied_offset - request.offset).abs() <= 0.01,
            );
            if settled {
                self.finish_command(VirtualizerCommandOutcome::Settled);
            } else if stalled {
                self.finish_command(VirtualizerCommandOutcome::NoProgress);
            }
        }
        self.commit(&viewport.clone().with_offset(applied_offset))?;
        Ok(())
    }
    pub fn commit(&mut self, viewport: &VirtualizerViewport) -> Result<(), VirtualizerError> {
        self.validate_current_viewport(viewport)?;
        let snapshot = self.snapshot(viewport)?;
        self.owned_geometry_offset = None;
        let range = (
            self.generation,
            snapshot.visible.clone(),
            snapshot.rendered.clone(),
        );
        if self.last_range.as_ref() != Some(&range) {
            self.events
                .retain(|event| !matches!(event, VirtualizerEvent::RangeChanged { .. }));
            self.events.push(VirtualizerEvent::RangeChanged {
                generation: self.generation,
                visible: snapshot.visible.clone(),
                rendered: snapshot.rendered.clone(),
            });
            self.last_range = Some(range);
        }
        self.observe_offset(viewport.offset, self.clock.unwrap_or_else(Instant::now));
        self.viewport = Some(viewport.clone());
        if self.follow_tail.is_some() {
            self.follow_tail = Some((viewport.offset - self.scroll_limit(viewport)).abs() <= 1.0);
        }
        // The list does not own the host's header/footer reading position.
        // This tests the start edge, not item visibility: padding and gaps
        // inside this region remain valid reading positions.
        if self.source.is_empty()
            || viewport.frozen_prefix > 0 && viewport.leading_occlusion >= viewport.extent
            || self.attachment == VirtualizerAttachment::Attached
                && (viewport.offset < viewport.origin
                    || viewport.offset >= viewport.origin + self.total_extent())
        {
            self.anchor = None;
            self.pending_anchor = None;
        } else {
            // A frozen strip does not scroll. Preserve the reading position at
            // the body's clear leading edge, not an item hidden behind it.
            let reading_offset = viewport.offset + f64::from(viewport.leading_occlusion);
            let mut index = self.geometry.index_at(
                (reading_offset - viewport.origin - f64::from(self.layout.padding_start)).max(0.0),
            );
            let mut item = snapshot.item(index)?;
            // Attach inter-item space to the following item, so the distance
            // remains independent of the previous item's content size.
            if reading_offset >= item.start + f64::from(item.extent)
                && index + 1 < self.source.len()
            {
                index += 1;
                item = snapshot.item(index)?;
            }
            let relative = reading_offset - item.start;
            let position = if relative < 0.0 {
                VirtualizerAnchorPosition::Before(-relative)
            } else if relative > f64::from(item.extent) {
                VirtualizerAnchorPosition::After(relative - f64::from(item.extent))
            } else {
                VirtualizerAnchorPosition::Within(relative)
            };
            self.anchor = Some((item.key, position));
        }
        Ok(())
    }
    /// Actual native layout for Measured Attached. No Element or hidden list is
    /// mounted, and this call cannot install input listeners or mutate the host.
    pub fn layout_measured(
        &mut self,
        snapshot: &VirtualizerSnapshot,
        bounds: Bounds<Pixels>,
        mut render: impl FnMut(usize, &mut Window, &mut App) -> gpui::AnyElement + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<ListViewportLayout, VirtualizerError> {
        self.validate_snapshot(snapshot)?;
        let list = self
            .measured
            .as_ref()
            .ok_or(VirtualizerError::WrongBackend)?;
        if self.layout.reverse_overscan.is_some() {
            let (before, after) = self.layout.overscan_edges(self.scroll_direction());
            list.set_overdraw_edges(before, after);
        }
        let source = self.source.clone();
        let gap = self.layout.gap;
        let mut padding = self.native_padding();
        if self.axis == Axis::Vertical {
            padding.top += Pixels::from(snapshot.viewport.origin);
        } else {
            padding.left += Pixels::from(snapshot.viewport.origin);
        }
        // A list inside a larger host must not clamp a footer's scroll position
        // back to its own end. Clip the measuring viewport to the list's region.
        let remaining =
            (snapshot.viewport.origin + self.total_extent() - snapshot.viewport.offset).max(0.0);
        let mut bounds = bounds;
        if self.axis == Axis::Vertical {
            bounds.size.height = bounds.size.height.min(Pixels::from(remaining));
        } else {
            bounds.size.width = bounds.size.width.min(Pixels::from(remaining));
        }
        list.set_retained_items(
            snapshot
                .retained_keys()
                .iter()
                .filter_map(|key| self.source.index_of(key))
                .collect(),
        );
        let axis = self.axis;
        let batch = list.layout_for_precise_viewport_on_axis(
            axis,
            bounds,
            snapshot.viewport.offset,
            padding,
            move |index, w, cx| {
                let row = div().id(source.key_at(index));
                let mut row = if axis == Axis::Vertical {
                    row.w_full()
                } else {
                    row.h_full()
                }
                .child(render(index, w, cx));
                if index + 1 < source.len() {
                    row = if axis == Axis::Vertical {
                        row.pb(gap)
                    } else {
                        row.pr(gap)
                    };
                }
                row.into_any_element()
            },
            window,
            cx,
        );
        self.sync_measured_geometry();
        if (batch.precise_offset() - snapshot.viewport.offset).abs() > 0.01 {
            self.pending_anchor = Some(batch.precise_offset());
        }
        Ok(batch)
    }
    pub fn scrollbar_handle(&self) -> Result<VirtualizerScrollbarHandle, VirtualizerError> {
        if self.attachment != VirtualizerAttachment::Owned {
            return Err(VirtualizerError::Unsupported(
                "attached viewport owns its scrollbar",
            ));
        }
        Ok(VirtualizerScrollbarHandle {
            scroll: self.scroll.clone(),
            measured: self.measured.clone(),
            activity: self.scrollbar_activity.clone(),
            axis: self.axis,
        })
    }
    pub fn native_scroll_handle(&self) -> Result<&ScrollHandle, VirtualizerError> {
        if self.attachment != VirtualizerAttachment::Owned || self.measured.is_some() {
            return Err(VirtualizerError::WrongBackend);
        }
        self.scroll.as_ref().ok_or(VirtualizerError::WrongBackend)
    }
    fn scroll_limit(&self, viewport: &VirtualizerViewport) -> f64 {
        (viewport
            .scroll_extent
            .unwrap_or(viewport.origin + self.total_extent())
            - f64::from(viewport.extent))
        .max(0.0)
    }
    fn validate_viewport(&self, viewport: &VirtualizerViewport) -> Result<(), VirtualizerError> {
        let binding = self.binding()?;
        if !Rc::ptr_eq(&binding.0, &viewport.binding.0) {
            return Err(VirtualizerError::BindingConflict);
        }
        if !viewport.offset.is_finite()
            || viewport.offset < 0.0
            || !viewport.origin.is_finite()
            || viewport.origin < 0.0
            || viewport
                .scroll_extent
                .is_some_and(|extent| !extent.is_finite() || extent < 0.0)
            || !valid_nonnegative(viewport.leading_occlusion)
            || viewport.leading_occlusion > viewport.extent
            || !valid_nonnegative(viewport.extent)
            || !valid_nonnegative(viewport.cross)
        {
            return Err(VirtualizerError::InvalidGeometry("viewport"));
        }
        Ok(())
    }
    fn validate_current_viewport(
        &self,
        viewport: &VirtualizerViewport,
    ) -> Result<(), VirtualizerError> {
        if self.viewport.as_ref().is_some_and(|current| {
            current.revision > viewport.revision || current.input_revision > viewport.input_revision
        }) {
            return Err(VirtualizerError::StaleSnapshot);
        }
        Ok(())
    }
    fn changed_geometry(&mut self) {
        self.geometry_revision += 1;
        self.outstanding = None;
        self.owned_geometry_offset = self.measured.as_ref().map(|list| -list.precise_scroll_offset().y);
    }
    fn restore_anchor(&mut self) {
        self.pending_anchor = self.anchor.as_ref().and_then(|(key, position)| {
            self.source.index_of(key).and_then(|index| {
                // Recompute clearance from current geometry: resizing frozen
                // tracks moves the body edge as well as its content. A key
                // reordered into the stationary prefix no longer anchors it.
                let clearance = if let Some(viewport) = &self.viewport
                    && viewport.frozen_prefix > 0
                {
                    if index < viewport.frozen_prefix {
                        return None;
                    }
                    let clearance = self.frozen_extent(viewport.frozen_prefix);
                    if clearance >= f64::from(viewport.extent) {
                        return None;
                    }
                    clearance
                } else {
                    0.0
                };
                self.geometry.item(index).map(|(start, extent, _)| {
                    let extent = extent
                        - if self.measured.is_some() && index + 1 < self.source.len() {
                            f64::from(self.layout.gap)
                        } else {
                            0.0
                        };
                    let before = if index == 0 {
                        self.layout.padding_start
                    } else {
                        self.layout.gap
                    };
                    let after = if index + 1 == self.source.len() {
                        self.layout.padding_end
                    } else {
                        self.layout.gap
                    };
                    start
                        + self.viewport.as_ref().map_or(0.0, |v| v.origin)
                        + f64::from(self.layout.padding_start)
                        + position.resolve(extent, f64::from(before), f64::from(after))
                        - clearance
                })
            })
        });
    }
    fn finish_command(&mut self, outcome: VirtualizerCommandOutcome) {
        if let Some(command) = self.command.take() {
            self.events.push(VirtualizerEvent::CommandFinished {
                id: command.id,
                outcome,
            });
        }
    }
    pub(super) fn claim_mount(
        &mut self,
        window: gpui::WindowId,
        id: &gpui::GlobalElementId,
    ) -> Result<(), VirtualizerError> {
        if self.attachment != VirtualizerAttachment::Owned {
            return Err(VirtualizerError::Unsupported(
                "use snapshots for attached presentation",
            ));
        }
        if self
            .mount
            .as_ref()
            .is_some_and(|current| current.0 != window || &current.1 != id)
        {
            return Err(VirtualizerError::AlreadyBound);
        }
        self.mount = Some((window, id.clone()));
        Ok(())
    }
    pub(super) fn shares_binding(&self, other: &Self) -> bool {
        self.binding
            .as_ref()
            .zip(other.binding.as_ref())
            .is_some_and(|(a, b)| Rc::ptr_eq(&a.0, &b.0))
    }
    pub(super) fn claim_attached_mount(
        &mut self,
        window: gpui::WindowId,
        id: &gpui::GlobalElementId,
    ) -> Result<(), VirtualizerError> {
        if self.attachment != VirtualizerAttachment::Attached {
            return Err(VirtualizerError::WrongBackend);
        }
        if self
            .mount
            .as_ref()
            .is_some_and(|old| old.0 != window || &old.1 != id)
        {
            return Err(VirtualizerError::AlreadyBound);
        }
        self.mount = Some((window, id.clone()));
        Ok(())
    }
    pub(super) fn native_measured(&self) -> Option<&ListState> {
        self.measured.as_ref()
    }
    pub(super) fn sync_measured_geometry(&mut self) {
        if let Some(list) = &self.measured {
            let invalid_sizes = list.invalid_size_count();
            let invalid = invalid_sizes > self.observed_invalid_sizes;
            if invalid && !self.reported_invalid_size {
                self.events.push(VirtualizerEvent::Diagnostic(
                    "invalid measured extent; using estimate",
                ));
            }
            self.observed_invalid_sizes = invalid_sizes;
            self.reported_invalid_size = invalid;
            let exhausted = list.last_layout_budget_exhausted();
            if exhausted && !self.reported_budget_exhaustion {
                self.events.push(VirtualizerEvent::Diagnostic(
                    "native measurement budget exhausted",
                ));
            }
            self.reported_budget_exhaustion = exhausted;
            let next = list.geometry();
            if !matches!(&self.geometry, Geometry::Measured(old) if old.revision() == next.revision())
            {
                self.geometry = Geometry::Measured(next);
                self.changed_geometry();
            }
        }
    }
    pub(super) fn layout_values(&self) -> VirtualizerLayout {
        self.layout
    }
    pub(super) fn source_for_render(&self) -> Rc<dyn VirtualizerSource> {
        self.source.clone()
    }
    pub(super) fn item_content_revision(&self, ix: usize) -> u64 {
        self.content_revisions
            .range(..=ix)
            .next_back()
            .map_or(self.content_base_revision, |(_, &revision)| revision)
    }
    pub(super) fn native_padding(&self) -> Edges<Pixels> {
        if self.axis == Axis::Vertical {
            Edges {
                top: self.layout.padding_start,
                bottom: self.layout.padding_end,
                left: px(0.),
                right: px(0.),
            }
        } else {
            Edges {
                left: self.layout.padding_start,
                right: self.layout.padding_end,
                top: px(0.),
                bottom: px(0.),
            }
        }
    }
    pub(super) fn release_mount(&mut self, revision: u64, cx: &mut Context<Self>) {
        if self.binding_revision == revision {
            self.scroll_end_task = None;
            self.end_scrolling();
            self.cancel_scroll();
            self.mount = None;
            self.outstanding = None;
            self.binding_revision += 1;
            self.viewport = None;
            for event in self.take_events() {
                cx.emit(event);
            }
        }
    }
    pub(super) fn binding_revision(&self) -> u64 {
        self.binding_revision
    }
    pub(super) fn owned_viewport(
        &self,
        bounds: Bounds<Pixels>,
    ) -> Result<VirtualizerViewport, VirtualizerError> {
        let offset = if let Some(list) = &self.measured {
            -list.precise_scroll_offset().y
        } else if self.axis == Axis::Vertical {
            -self.scroll.as_ref().unwrap().precise_offset().y
        } else {
            -self.scroll.as_ref().unwrap().precise_offset().x
        };
        let extent = if self.axis == Axis::Vertical {
            bounds.size.height
        } else {
            bounds.size.width
        };
        let cross = if self.axis == Axis::Vertical {
            bounds.size.width
        } else {
            bounds.size.height
        };
        let revision = self
            .viewport
            .as_ref()
            .map_or(1, |viewport| viewport.revision + 1);
        let input_revision = self.viewport.as_ref().map_or(0, |viewport| {
            viewport.input_revision
                + u64::from(!self.is_following_tail()
                    && (self.owned_geometry_offset.unwrap_or(viewport.offset) - offset).abs() > 0.01)
        });
        Ok(
            VirtualizerViewport::new(self.binding()?, revision, offset.max(0.0), extent, cross)
                .input_revision(input_revision),
        )
    }
    pub(super) fn apply_owned_offset(&self, offset: f64) {
        if let Some(list) = &self.measured {
            let index = self
                .geometry
                .index_at((offset - f64::from(self.layout.padding_start)).max(0.0));
            let start = self.geometry.item(index).map_or(0.0, |item| item.0);
            list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: Pixels::from((offset - start).max(0.0)),
            });
        } else {
            let scroll = self.scroll.as_ref().unwrap();
            let mut point = scroll.precise_offset();
            if self.axis == Axis::Vertical {
                point.y = -offset;
            } else {
                point.x = -offset;
            }
            scroll.set_precise_offset(point);
        }
    }
}

impl Drop for VirtualizerState {
    fn drop(&mut self) {
        if let Some(binding) = &self.binding {
            binding.0.borrow_mut()[axis_slot(self.axis)] = Weak::new();
        }
    }
}

/// A projection of the existing native owner, suitable for Base Scrollbar.
/// Clones share handles; this type owns no additional scrolling state.
#[derive(Clone)]
pub struct VirtualizerScrollbarHandle {
    scroll: Option<ScrollHandle>,
    measured: Option<ListState>,
    activity: Rc<Cell<u64>>,
    axis: Axis,
}
impl crate::ScrollbarHandle for VirtualizerScrollbarHandle {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.measured.as_ref().map_or_else(
            || self.scroll.as_ref().unwrap().bounds(),
            |list| {
                let bounds = list.viewport_bounds();
                if self.axis == Axis::Vertical {
                    bounds
                } else {
                    Bounds::new(bounds.origin, size(bounds.size.height, bounds.size.width))
                }
            },
        )
    }
    fn offset(&self) -> Point<Pixels> {
        self.measured.as_ref().map_or_else(
            || self.scroll.as_ref().unwrap().offset(),
            |list| {
                let offset = list.scroll_px_offset_for_scrollbar();
                if self.axis == Axis::Vertical {
                    offset
                } else {
                    gpui::point(offset.y, offset.x)
                }
            },
        )
    }
    fn set_offset(&self, offset: Point<Pixels>) {
        self.activity.set(self.activity.get().wrapping_add(1));
        if let Some(list) = &self.measured {
            list.set_offset_from_scrollbar(if self.axis == Axis::Vertical {
                offset
            } else {
                gpui::point(offset.y, offset.x)
            });
        } else {
            self.scroll.as_ref().unwrap().set_offset(offset);
        }
    }
    fn content_size(&self) -> Size<Pixels> {
        self.measured.as_ref().map_or_else(
            || self.scroll.as_ref().unwrap().content_size(),
            |list| {
                let cross = list.viewport_bounds().size.width;
                let main = list.max_offset_for_scrollbar().y + list.viewport_bounds().size.height;
                if self.axis == Axis::Vertical {
                    size(cross, main)
                } else {
                    size(main, cross)
                }
            },
        )
    }
    fn start_drag(&self) {
        if let Some(list) = &self.measured {
            list.scrollbar_drag_started();
        }
    }
    fn end_drag(&self) {
        self.activity.set(self.activity.get().wrapping_add(1));
        if let Some(list) = &self.measured {
            list.scrollbar_drag_ended();
        }
    }
}

fn axis_slot(axis: Axis) -> usize {
    if axis == Axis::Horizontal { 0 } else { 1 }
}
pub(super) fn validate_sizing(
    sizing: &VirtualizerSizing,
    count: usize,
) -> Result<(), VirtualizerError> {
    let valid = match &sizing.0 {
        Sizing::Fixed(extent) | Sizing::Measured(extent) => valid_positive(*extent),
        Sizing::Known(extents) => extents.len() == count && extents.is_valid(),
    };
    if valid {
        Ok(())
    } else {
        Err(VirtualizerError::InvalidGeometry("item extents/count"))
    }
}
fn make_geometry(
    sizing: &VirtualizerSizing,
    count: usize,
    layout: &VirtualizerLayout,
    measured: Option<&ListState>,
) -> Geometry {
    match &sizing.0 {
        Sizing::Fixed(extent) => Geometry::Fixed {
            count,
            extent: (*extent).into(),
            gap: layout.gap.into(),
        },
        Sizing::Known(extents) => {
            let mut tree = extents.clone();
            tree.gap = layout.gap.into();
            Geometry::Known(tree)
        }
        Sizing::Measured(_) => Geometry::Measured(measured.unwrap().geometry()),
    }
}
pub(super) fn alignment_offset(
    item: &VirtualizerItem,
    viewport: &VirtualizerViewport,
    align: VirtualizerAlign,
    inset_start: f64,
    inset_end: f64,
) -> f64 {
    if item.index < viewport.frozen_prefix {
        return viewport.offset;
    }
    let inset_start = inset_start + f64::from(viewport.leading_occlusion);
    let extent = (f64::from(viewport.extent) - inset_start - inset_end).max(0.0);
    let start = viewport.offset + inset_start;
    let end = item.start + f64::from(item.extent);
    match align {
        VirtualizerAlign::Start => item.start - inset_start,
        VirtualizerAlign::Center => {
            item.start + (f64::from(item.extent) - extent) / 2.0 - inset_start
        }
        VirtualizerAlign::End => end - extent - inset_start,
        VirtualizerAlign::Nearest if item.start <= start && end >= start + extent => {
            viewport.offset
        }
        VirtualizerAlign::Nearest if item.start < start => item.start - inset_start,
        VirtualizerAlign::Nearest if end > start + extent => end - extent - inset_start,
        VirtualizerAlign::Nearest => viewport.offset,
    }
}
