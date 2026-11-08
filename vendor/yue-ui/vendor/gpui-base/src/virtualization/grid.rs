//! Cell-level two-axis presentation. Both axes reuse VirtualizerState; the host
//! owns exactly one ScrollHandle and applies both proposals in one transaction.
use super::element::ViewStamp;
use super::*;
use crate::TestSupportExt as _;
use gpui::{
    AnyElement, App, AppContext, AvailableSpace, Bounds, ContentMask, Div, Element, Entity, GlobalElementId,
    Hitbox, InspectorElementId, InteractiveElement, IntoElement, LayoutId, ParentElement,
    ScrollHandle, Stateful, StatefulInteractiveElement, StyleRefinement, Styled, Window, div,
    point, px, size,
};

/// Stable sparse cell identity. Retaining two cells does not retain their
/// cross-product. Deleted row/column keys are ignored on the next frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VirtualGridCellKey {
    row: ElementId,
    column: ElementId,
}
impl VirtualGridCellKey {
    pub fn new(row: impl Into<ElementId>, column: impl Into<ElementId>) -> Self {
        Self {
            row: row.into(),
            column: column.into(),
        }
    }
    pub fn row(&self) -> &ElementId {
        &self.row
    }
    pub fn column(&self) -> &ElementId {
        &self.column
    }
}

#[derive(Clone)]
pub struct VirtualGridCell {
    row: VirtualizerItem,
    column: VirtualizerItem,
    frozen_row: bool,
    frozen_column: bool,
    span: Option<Rc<CellSpan>>,
}
struct CellSpan {
    rows: usize,
    columns: usize,
    extent: gpui::Size<Pixels>,
}
impl VirtualGridCell {
    /// Presentation rectangle, including inter-track gaps for explicit spans.
    /// row()/column() continue to describe the original origin tracks.
    pub fn extent(&self) -> gpui::Size<Pixels> {
        self.span.as_ref().map_or_else(
            || size(self.column.extent(), self.row.extent()),
            |s| s.extent,
        )
    }
    pub fn row_span(&self) -> usize {
        self.span.as_ref().map_or(1, |s| s.rows)
    }
    pub fn column_span(&self) -> usize {
        self.span.as_ref().map_or(1, |s| s.columns)
    }
    pub fn key(&self) -> VirtualGridCellKey {
        VirtualGridCellKey::new(self.row.key().clone(), self.column.key().clone())
    }
    /// Frame-local presentation flags; identity and preparation stay unchanged.
    pub fn is_frozen_row(&self) -> bool {
        self.frozen_row
    }
    pub fn is_frozen_column(&self) -> bool {
        self.frozen_column
    }

    pub fn row(&self) -> &VirtualizerItem {
        &self.row
    }
    pub fn column(&self) -> &VirtualizerItem {
        &self.column
    }
    pub fn identity(&self) -> VirtualGridCellIdentity {
        VirtualGridCellIdentity {
            row: self.row.identity(),
            column: self.column.identity(),
        }
    }
}
/// Content identity for preparation; geometry remains on the frame's cell.
#[derive(Clone)]
pub struct VirtualGridCellIdentity {
    row: VirtualizerItemIdentity,
    column: VirtualizerItemIdentity,
}
impl VirtualGridCellIdentity {
    pub fn row(&self) -> &VirtualizerItemIdentity {
        &self.row
    }
    pub fn column(&self) -> &VirtualizerItemIdentity {
        &self.column
    }
}
#[derive(Clone)]
pub struct VirtualGridSnapshot {
    rows: VirtualizerSnapshot,
    columns: VirtualizerSnapshot,
}
impl VirtualGridSnapshot {
    // Only sparse extras are buffered. The ordinary cross-product remains lazy.
    fn retained_cells(&self, keys: &[VirtualGridCellKey]) -> Vec<VirtualGridCell> {
        if keys.is_empty() {
            return Vec::new();
        }
        let rows = self.rows.working_indices();
        let columns = self.columns.working_indices();
        let mut cells: Vec<_> = keys
            .iter()
            .filter_map(|key| {
                let row = self.rows.item_for_key(key.row()).ok()?;
                let column = self.columns.item_for_key(key.column()).ok()?;
                if rows.binary_search(&row.index()).is_ok()
                    && columns.binary_search(&column.index()).is_ok()
                {
                    return None;
                }
                Some(VirtualGridCell {
                    span: None,
                    frozen_row: self.rows.is_frozen(row.index()),
                    frozen_column: self.columns.is_frozen(column.index()),
                    row,
                    column,
                })
            })
            .collect();
        cells.sort_unstable_by_key(cell_order);
        cells.dedup_by_key(|cell| (cell.row.index(), cell.column.index()));
        cells
    }
    pub fn new(
        rows: VirtualizerSnapshot,
        columns: VirtualizerSnapshot,
    ) -> Result<Self, VirtualizerError> {
        if rows.axis() != Axis::Vertical
            || columns.axis() != Axis::Horizontal
            || !rows.viewport().shares_host(columns.viewport())
            || rows.viewport_revision() != columns.viewport_revision()
            || rows.viewport().extent() != columns.viewport().cross_extent()
            || columns.viewport().extent() != rows.viewport().cross_extent()
        {
            return Err(VirtualizerError::InvalidGeometry("grid axes"));
        }
        Ok(Self { rows, columns })
    }
    pub fn rows(&self) -> &VirtualizerSnapshot {
        &self.rows
    }
    pub fn columns(&self) -> &VirtualizerSnapshot {
        &self.columns
    }
    pub fn cells(&self) -> impl Iterator<Item = VirtualGridCell> + '_ {
        // Cells cost O(R*C); source/geometry preparation costs only O(R+C).
        let mut rows = self.rows.working_items();
        let mut row = rows.next().map(|item| item.expect("valid grid row"));
        let columns = if row.is_some() {
            self.columns
                .working_items()
                .map(|item| item.expect("valid grid column"))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut column = 0;
        std::iter::from_fn(move || {
            if columns.is_empty() {
                return None;
            }
            if column == columns.len() {
                column = 0;
                row = rows.next().map(|item| item.expect("valid grid row"));
            }
            let row = row.as_ref()?;
            let cell = VirtualGridCell {
                span: None,
                frozen_row: self.rows.is_frozen(row.index()),
                frozen_column: self.columns.is_frozen(columns[column].index()),
                row: row.clone(),
                column: columns[column].clone(),
            };
            column += 1;
            Some(cell)
        })
    }
    // The four disjoint regions need stable stacking, not an R*C cell buffer
    // and sort. Each sorted axis splits once at its frozen prefix. Materialize
    // O(R+C) tracks and stream the same region/row/column order as before.
    fn partitioned_cells(&self) -> impl Iterator<Item = VirtualGridCell> + '_ {
        let rows: Vec<_> = self
            .rows
            .working_items()
            .map(|r| r.expect("valid grid row"))
            .collect();
        let columns: Vec<_> = self
            .columns
            .working_items()
            .map(|c| c.expect("valid grid column"))
            .collect();
        let row_split = rows.partition_point(|r| self.rows.is_frozen(r.index()));
        let column_split = columns.partition_point(|c| self.columns.is_frozen(c.index()));
        let mut region = 0;
        let mut row = row_split;
        let mut column = column_split;
        std::iter::from_fn(move || {
            loop {
                if region == 4 {
                    return None;
                }
                let row_range = if region & 1 == 0 {
                    row_split..rows.len()
                } else {
                    0..row_split
                };
                let column_range = if region & 2 == 0 {
                    column_split..columns.len()
                } else {
                    0..column_split
                };
                if column >= column_range.end {
                    column = column_range.start;
                    row += 1;
                }
                if row >= row_range.end || column_range.is_empty() {
                    region += 1;
                    row = if region & 1 == 0 { row_split } else { 0 };
                    column = if region & 2 == 0 { column_split } else { 0 };
                    continue;
                }
                let cell = VirtualGridCell {
                    span: None,
                    row: rows[row].clone(),
                    column: columns[column].clone(),
                    frozen_row: region & 1 != 0,
                    frozen_column: region & 2 != 0,
                };
                column += 1;
                return Some(cell);
            }
        })
    }
}

fn cell_order(cell: &VirtualGridCell) -> (usize, usize, usize) {
    (
        usize::from(cell.frozen_row) + 2 * usize::from(cell.frozen_column),
        cell.row.index(),
        cell.column.index(),
    )
}

#[derive(Default)]
struct GridOptions {
    retained: Vec<VirtualGridCellKey>,
    focused: Vec<(VirtualGridCellKey, gpui::FocusHandle)>,
    fallback: Option<gpui::FocusHandle>,
    spans: Option<super::grid_span::GridSpanStorage>,
    right_to_left: bool,
}
type Renderer = Rc<dyn Fn(VirtualGridCell, &mut Window, &mut App) -> AnyElement>;
type ViewFactory = Rc<dyn Fn(VirtualGridCellIdentity, &mut Window, &mut App) -> gpui::AnyView>;
type CellKey = (ElementId, ElementId);
#[derive(Default)]
struct CellViews {
    entries: std::collections::HashMap<CellKey, ([ViewStamp; 2], gpui::AnyView)>,
    touched: std::collections::HashSet<CellKey>,
}
pub struct VirtualGrid {
    id: ElementId,
    rows: Entity<VirtualizerState>,
    columns: Entity<VirtualizerState>,
    scroll: ScrollHandle,
    base: Stateful<Div>,
    render: Renderer,
    view_factory: Option<ViewFactory>,
    content_revision: u64,
    frozen_rows: usize,
    frozen_columns: usize,
    options: Option<Box<GridOptions>>,
}
impl VirtualGrid {
    /// Atomically update explicit row and column measurements for one Grid.
    ///
    /// Both axes are validated before either is changed. The axes must be the
    /// same Attached, Fixed/Known Grid pair; Fixed axes reject per-item
    /// updates, while Known axes apply their ordinary anchor-preserving batch
    /// semantics. This helper does not write the shared scroll position.
    pub fn update_sizes(
        rows: &Entity<VirtualizerState>,
        columns: &Entity<VirtualizerState>,
        row_updates: &[VirtualizerSizeUpdate],
        column_updates: &[VirtualizerSizeUpdate],
        cx: &mut App,
    ) -> Result<(), VirtualizerError> {
        let validation = cx.read_entity(rows, |rows, _| {
            cx.read_entity(columns, |columns, _| {
                Self::validate_axes(rows, columns)?;
                rows.validate_size_batch(row_updates, true)?;
                columns.validate_size_batch(column_updates, true)
            })
        });
        validation?;
        cx.update_entity(rows, |rows, cx| {
            rows.update_sizes(row_updates)?;
            cx.notify();
            Ok::<_, VirtualizerError>(())
        })?;
        cx.update_entity(columns, |columns, cx| {
            columns.update_sizes(column_updates)?;
            cx.notify();
            Ok::<_, VirtualizerError>(())
        })?;
        Ok(())
    }

    /// Snapshot-guarded form of [`Self::update_sizes`]. Both snapshots are
    /// checked before either axis is mutated, so a stale pair is a no-op.
    pub fn update_sizes_from_snapshots(
        rows: &Entity<VirtualizerState>,
        columns: &Entity<VirtualizerState>,
        row_snapshot: &VirtualizerSnapshot,
        column_snapshot: &VirtualizerSnapshot,
        row_updates: &[VirtualizerSizeUpdate],
        column_updates: &[VirtualizerSizeUpdate],
        cx: &mut App,
    ) -> Result<(), VirtualizerError> {
        let row_validation = cx.read_entity(rows, |rows, _| {
            rows.validate_snapshot(row_snapshot)?;
            rows.validate_size_batch(row_updates, true)
        });
        row_validation?;
        let column_validation = cx.read_entity(columns, |columns, _| {
            columns.validate_snapshot(column_snapshot)?;
            columns.validate_size_batch(column_updates, true)
        });
        column_validation?;
        Self::update_sizes(rows, columns, row_updates, column_updates, cx)
    }

    /// Preflight configuration without claiming a mount, querying the source,
    /// or writing a scroll position. Dynamic cell-derived row heights are not
    /// supported: both dimensions must be provided by the caller.
    pub fn validate_axes(
        rows: &VirtualizerState,
        columns: &VirtualizerState,
    ) -> Result<(), VirtualizerError> {
        if rows.axis() != Axis::Vertical || columns.axis() != Axis::Horizontal {
            return Err(VirtualizerError::InvalidGeometry("grid axes"));
        }
        if rows.attachment() != VirtualizerAttachment::Attached
            || columns.attachment() != VirtualizerAttachment::Attached
        {
            return Err(VirtualizerError::Unsupported("grid requires attached axes"));
        }
        if rows.sizing_kind() == VirtualizerSizingKind::Measured
            || columns.sizing_kind() == VirtualizerSizingKind::Measured
        {
            return Err(VirtualizerError::Unsupported(
                "grid requires explicit row/column dimensions",
            ));
        }
        if !rows.shares_binding(columns) {
            return Err(VirtualizerError::BindingConflict);
        }
        Ok(())
    }
    /// States must be Attached, share a binding, and use Fixed/Known sizes.
    /// Cells do not determine row/column sizes; callers supply explicit geometry.
    pub fn new<R: IntoElement>(
        id: impl Into<ElementId>,
        rows: &Entity<VirtualizerState>,
        columns: &Entity<VirtualizerState>,
        scroll: &ScrollHandle,
        render: impl Fn(VirtualGridCell, &mut Window, &mut App) -> R + 'static,
    ) -> Self {
        let id = id.into();
        let row_owner = rows.downgrade();
        let column_owner = columns.downgrade();
        let mut base = div()
            .id(id.clone())
            .size_full()
            .overflow_scroll()
            .track_scroll(scroll)
            .on_scroll_wheel(move |_, _, cx| {
                let _ = row_owner.update(cx, |s, cx| s.user_scroll_activity(cx));
                let _ = column_owner.update(cx, |s, cx| s.user_scroll_activity(cx));
            });
        base.style().allow_concurrent_scroll = Some(true);
        Self {
            id,
            rows: rows.clone(),
            columns: columns.clone(),
            scroll: scroll.clone(),
            base,
            render: Rc::new(move |cell, w, cx| render(cell, w, cx).into_any_element()),
            view_factory: None,
            content_revision: 0,
            frozen_rows: 0,
            frozen_columns: 0,
            options: None,
        }
    }
    /// Prepare one view per live content identity, independently of frame geometry.
    /// External factory inputs require content_revision or axis content invalidation.
    /// Child entity Render/layout/paint and its own notify contract remain unchanged.
    pub fn new_with_cell_views<V: Into<gpui::AnyView>>(
        id: impl Into<ElementId>,
        rows: &Entity<VirtualizerState>,
        columns: &Entity<VirtualizerState>,
        scroll: &ScrollHandle,
        factory: impl Fn(VirtualGridCellIdentity, &mut Window, &mut App) -> V + 'static,
    ) -> Self {
        let mut grid = Self::new(id, rows, columns, scroll, |_, _, _| div());
        grid.view_factory = Some(Rc::new(move |cell, w, cx| factory(cell, w, cx).into()));
        grid
    }
    /// Freeze the leading rows in current source order, clamped to source length.
    /// Oversized prefixes materialize only their visible portion and retained keys.
    pub fn frozen_rows(mut self, count: usize) -> Self {
        self.frozen_rows = count;
        self
    }
    /// Freeze leading columns using the same native scroll owner and cell IDs.
    pub fn frozen_columns(mut self, count: usize) -> Self {
        self.frozen_columns = count;
        self
    }
    pub fn content_revision(mut self, revision: u64) -> Self {
        self.content_revision = revision;
        self
    }
    /// Sparse union with visible/axis-retained/frozen cells, deduplicated by
    /// stable key pair. Does not grow either axis's retained working set.
    pub fn retained_cells(mut self, keys: Vec<VirtualGridCellKey>) -> Self {
        if !keys.is_empty() || self.options.is_some() {
            self.options.get_or_insert_with(Default::default).retained = keys;
        }
        self
    }
    /// Retain this cell only while its editor contains focus. Supply the same
    /// domain handle on each render; never register it separately on both axes.
    /// Removing a focused registration/key repairs focus before pruning views.
    pub fn cell_focus_handle(mut self, key: VirtualGridCellKey, handle: gpui::FocusHandle) -> Self {
        let options = self.options.get_or_insert_with(Default::default);
        assert!(
            !options
                .focused
                .iter()
                .any(|(k, h)| *k != key && *h == handle),
            "one focus handle cannot identify two cells"
        );
        options.focused.retain(|(k, _)| *k != key);
        options.focused.push((key, handle));
        self
    }
    pub fn focus_fallback(mut self, handle: gpui::FocusHandle) -> Self {
        self.options.get_or_insert_with(Default::default).fallback = Some(handle);
        self
    }
    /// Mirror column presentation and map the single native physical scroll
    /// owner to logical start/end offsets. Keys and source order stay unchanged.
    pub fn right_to_left(mut self, enabled: bool) -> Self {
        if enabled || self.options.is_some() {
            self.options
                .get_or_insert_with(Default::default)
                .right_to_left = enabled;
        }
        self
    }
    /// Validate sparse Fixed/Known spans before claiming a mount or mutating
    /// scroll state. Call after frozen_rows/columns, and rebuild each render
    /// from the current stable-key source transaction. Invalid input is atomic.
    pub fn spans(mut self, spans: &[VirtualGridSpan], cx: &App) -> Result<Self, VirtualizerError> {
        if !spans.is_empty() {
            let resolved = super::grid_span::GridSpans::new(
                self.rows.read(cx),
                self.columns.read(cx),
                spans,
                (self.frozen_rows, self.frozen_columns),
            )?;
            self.options.get_or_insert_with(Default::default).spans =
                Some(super::grid_span::GridSpanStorage::Owned(resolved));
        } else if let Some(options) = &mut self.options {
            options.spans = None;
        }
        Ok(self)
    }
    /// Apply retained immutable descriptors, reusing at most one validated
    /// resolution. Call after freezing. Retain the set outside render; replace
    /// it when descriptors change. Source transactions invalidate automatically.
    pub fn span_set(
        mut self,
        spans: &VirtualGridSpanSet,
        cx: &App,
    ) -> Result<Self, VirtualizerError> {
        if let Some(resolved) = spans.resolve(
            &self.rows,
            &self.columns,
            (self.frozen_rows, self.frozen_columns),
            cx,
        )? {
            self.options.get_or_insert_with(Default::default).spans =
                Some(super::grid_span::GridSpanStorage::Shared(resolved));
        } else if let Some(options) = &mut self.options {
            options.spans = None;
        }
        Ok(self)
    }
}
impl Styled for VirtualGrid {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl InteractiveElement for VirtualGrid {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}
impl StatefulInteractiveElement for VirtualGrid {}
impl IntoElement for VirtualGrid {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
pub struct VirtualGridFrame {
    cells: Vec<(ElementId, AnyElement)>,
    masks: Option<Vec<Bounds<Pixels>>>,
    views: Option<Rc<std::cell::RefCell<CellViews>>>,
    mount: Entity<GridMount>,
}
const MAX_RETAINED_GRID_BUFFER_CAPACITY: usize = 4096;
struct GridHorizontal {
    logical: f64,
    physical: f64,
}
struct GridMount {
    rows: gpui::WeakEntity<VirtualizerState>,
    columns: gpui::WeakEntity<VirtualizerState>,
    row_revision: u64,
    column_revision: u64,
    _subscriptions: Vec<gpui::Subscription>,
    views: Option<Rc<std::cell::RefCell<CellViews>>>,
    focused: Vec<(VirtualGridCellKey, gpui::FocusHandle)>,
    horizontal: Option<Box<GridHorizontal>>,
    cell_buffer: Vec<(ElementId, AnyElement)>,
    mask_buffer: Vec<Bounds<Pixels>>,
}
impl Element for VirtualGrid {
    type RequestLayoutState = VirtualGridFrame;
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn a11y_role(&self) -> Option<gpui::Role> {
        self.base.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) {
        self.base.write_a11y_info(node);
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, VirtualGridFrame) {
        let rows = self.rows.read(cx);
        let columns = self.columns.read(cx);
        Self::validate_axes(rows, columns).expect("invalid virtual grid configuration");
        let retain_masks = self.frozen_rows.min(rows.source_for_render().len()) > 0
            || self.frozen_columns.min(columns.source_for_render().len()) > 0;
        if let Some(spans) = self.options.as_ref().and_then(|o| o.spans.as_ref()) {
            assert_eq!(
                spans.generations,
                (rows.generation(), columns.generation()),
                "rebuild spans after source change"
            );
            assert_eq!(
                spans.frozen,
                (self.frozen_rows, self.frozen_columns),
                "configure freezing before spans"
            );
        }
        let global = id.expect("grid identity");
        for state in [&self.rows, &self.columns] {
            state
                .update(cx, |s, _| {
                    s.claim_attached_mount(w.window_handle().window_id(), global)
                })
                .unwrap();
        }
        let rows = self.rows.clone();
        let columns = self.columns.clone();
        let row_revision = rows.read(cx).binding_revision();
        let column_revision = columns.read(cx).binding_revision();
        let mount = w.use_keyed_state(
            (self.id.clone(), "grid-mount"),
            cx,
            move |_, cx: &mut gpui::Context<GridMount>| {
                let subscriptions = vec![
                    cx.observe(&rows, |_, _, cx| cx.notify()),
                    cx.observe(&columns, |_, _, cx| cx.notify()),
                    cx.on_release(|mount: &mut GridMount, cx| {
                        let _ = mount
                            .rows
                            .update(cx, |s, cx| s.release_mount(mount.row_revision, cx));
                        let _ = mount
                            .columns
                            .update(cx, |s, cx| s.release_mount(mount.column_revision, cx));
                    }),
                ];
                GridMount {
                    rows: rows.downgrade(),
                    columns: columns.downgrade(),
                    row_revision,
                    column_revision,
                    _subscriptions: subscriptions,
                    views: None,
                    focused: Vec::new(),
                    horizontal: None,
                    cell_buffer: Vec::new(),
                    mask_buffer: Vec::new(),
                }
            },
        );
        mount.update(cx, |m, cx| {
            let changed_owner = m.rows != self.rows
                || m.row_revision != row_revision
                || m.columns != self.columns
                || m.column_revision != column_revision;
            let row_source = self.rows.read(cx).source_for_render();
            let column_source = self.columns.read(cx).source_for_render();
            let live = |key: &VirtualGridCellKey| {
                row_source.index_of(key.row()).is_some()
                    && column_source.index_of(key.column()).is_some()
            };
            let next = self
                .options
                .as_ref()
                .map(|o| o.focused.as_slice())
                .unwrap_or(&[]);
            let repair = m.focused.iter().any(|(key, handle)| {
                (changed_owner
                    || !live(key)
                    || !next
                        .iter()
                        .any(|entry| entry == &(key.clone(), handle.clone())))
                    && handle.contains_focused(w, cx)
            });
            if repair {
                if let Some(fallback) = self.options.as_ref().and_then(|o| o.fallback.as_ref()) {
                    fallback.focus(w, cx);
                } else {
                    w.blur(cx);
                }
            }
            m.focused = next.iter().filter(|(key, _)| live(key)).cloned().collect();
            if m.rows != self.rows || m.row_revision != row_revision {
                m.views = None;
                let _ = m
                    .rows
                    .update(cx, |s, cx| s.release_mount(m.row_revision, cx));
                m.rows = self.rows.downgrade();
                m.row_revision = row_revision;
                m._subscriptions[0] = cx.observe(&self.rows, |_, _, cx| cx.notify());
            }
            if m.columns != self.columns || m.column_revision != column_revision {
                m.horizontal = None;
                m.views = None;
                let _ = m
                    .columns
                    .update(cx, |s, cx| s.release_mount(m.column_revision, cx));
                m.columns = self.columns.downgrade();
                m.column_revision = column_revision;
                m._subscriptions[1] = cx.observe(&self.columns, |_, _, cx| cx.notify());
            }
        });
        let views = mount.update(cx, |m, _| {
            if self.view_factory.is_some() {
                Some(m.views.get_or_insert_with(|| Rc::default()).clone())
            } else {
                m.views = None;
                None
            }
        });
        let (cells, masks) = mount.update(cx, |m, _| {
            (
                std::mem::take(&mut m.cell_buffer),
                retain_masks.then(|| std::mem::take(&mut m.mask_buffer)),
            )
        });
        if let (Some(factory), Some(views)) = (self.view_factory.clone(), views.clone()) {
            views.borrow_mut().touched.clear();
            let rows = self.rows.clone();
            let columns = self.columns.clone();
            let row_source = rows.read(cx).source_for_render();
            let column_source = columns.read(cx).source_for_render();
            let revision = self.content_revision;
            self.render = Rc::new(move |cell, w, cx| {
                let key = (cell.row.key().clone(), cell.column.key().clone());
                let stamp = [
                    (
                        row_source.revision_at(cell.row.index()),
                        revision,
                        rows.read(cx).item_content_revision(cell.row.index()),
                        cell.row.index(),
                    ),
                    (
                        column_source.revision_at(cell.column.index()),
                        revision,
                        columns.read(cx).item_content_revision(cell.column.index()),
                        cell.column.index(),
                    ),
                ];
                let existing = {
                    let mut cache = views.borrow_mut();
                    cache.touched.insert(key.clone());
                    cache
                        .entries
                        .get(&key)
                        .filter(|(old, _)| *old == stamp)
                        .map(|(_, view)| view.clone())
                };
                let view = existing.unwrap_or_else(|| {
                    let view = factory(cell.identity(), w, cx);
                    views
                        .borrow_mut()
                        .entries
                        .insert(key, (stamp, view.clone()));
                    view
                });
                view.into_any_element()
            });
        }
        let layout =
            self.base
                .interactivity()
                .request_layout(id, inspector, w, cx, |style, w, cx| {
                    w.request_layout(style, None, cx)
                });
        (
            layout,
            VirtualGridFrame {
                cells,
                masks,
                views,
                mount,
            },
        )
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut VirtualGridFrame,
        w: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        let clip = bounds.intersect(&w.content_mask().bounds);
        let visible = clip.size.width > px(0.)
            && clip.size.height > px(0.)
            && w.is_visible()
            && w.is_window_active();
        for state in [&self.rows, &self.columns] {
            state.update(cx, |s, cx| {
                s.update_focus(w, cx);
                if !visible || cx.reduce_motion() {
                    s.settle_motion();
                }
            });
        }
        let offset = self.scroll.precise_offset();
        let rtl = self.options.as_ref().is_some_and(|o| o.right_to_left);
        let maximum = (self.columns.read(cx).total_extent() - f64::from(bounds.size.width)).max(0.);
        let physical = (-offset.x).clamp(0., maximum);
        let logical_x = if rtl {
            if let Some(previous) = frame.mount.read(cx).horizontal.as_deref() {
                // Native clamping after resize is not user movement. Apply only
                // the delta beyond the previous physical position's new clamp.
                let expected = previous.physical.clamp(0., maximum);
                (previous.logical - (physical - expected)).clamp(0., maximum)
            } else {
                self.columns
                    .read(cx)
                    .viewport()
                    .map_or(0., |v| v.offset())
                    .clamp(0., maximum)
            }
        } else {
            frame
                .mount
                .read(cx)
                .horizontal
                .as_deref()
                .map_or(physical, |previous| previous.logical.clamp(0., maximum))
        };
        let now = cx.background_executor().now();
        let observe = |state: &VirtualizerState, offset: f64, main, cross| {
            let old = state.viewport();
            VirtualizerViewport::new(
                state.binding().unwrap(),
                old.map_or(1, |v| v.revision() + 1),
                offset.max(0.0),
                main,
                cross,
            )
            .input_revision(old.map_or(0, |v| {
                v.input_revision_value() + u64::from((v.offset() - offset).abs() > 0.01)
            }))
        };
        let revision = self
            .rows
            .read(cx)
            .viewport()
            .map_or(0, |v| v.revision())
            .max(self.columns.read(cx).viewport().map_or(0, |v| v.revision()))
            + 1;
        let mut rv = observe(
            self.rows.read(cx),
            -offset.y,
            bounds.size.height,
            bounds.size.width,
        );
        let mut cv = observe(
            self.columns.read(cx),
            logical_x,
            bounds.size.width,
            bounds.size.height,
        );
        let row_count = if self.frozen_rows == 0 {
            0
        } else {
            self.frozen_rows
                .min(self.rows.read(cx).source_for_render().len())
        };
        let column_count = if self.frozen_columns == 0 {
            0
        } else {
            self.frozen_columns
                .min(self.columns.read(cx).source_for_render().len())
        };
        let frozen_height = Pixels::from(
            self.rows
                .read(cx)
                .frozen_extent(row_count)
                .min(f64::from(bounds.size.height)),
        );
        let frozen_width = Pixels::from(
            self.columns
                .read(cx)
                .frozen_extent(column_count)
                .min(f64::from(bounds.size.width)),
        );
        rv = rv
            .with_revision(revision)
            .with_frozen_prefix(row_count, frozen_height);
        cv = cv
            .with_revision(revision)
            .with_frozen_prefix(column_count, frozen_width);
        let (rs, rr) = self.rows.update(cx, |s, _| s.prepare(&rv, now)).unwrap();
        let (cs, cr) = self.columns.update(cx, |s, _| s.prepare(&cv, now)).unwrap();
        let y = rr
            .as_ref()
            .map_or(rv.offset(), VirtualizerScrollRequest::offset)
            .clamp(
                0.0,
                (rs.total_extent() - f64::from(bounds.size.height)).max(0.0),
            );
        let x = cr
            .as_ref()
            .map_or(cv.offset(), VirtualizerScrollRequest::offset)
            .clamp(
                0.0,
                (cs.total_extent() - f64::from(bounds.size.width)).max(0.0),
            );
        let physical_x = if rtl { maximum - x } else { x };
        self.scroll.set_precise_offset(point(-physical_x, -y));
        if rtl || frame.mount.read(cx).horizontal.is_some() {
            frame.mount.update(cx, |m, _| {
                if rtl {
                    let state = m.horizontal.get_or_insert_with(|| {
                        Box::new(GridHorizontal {
                            logical: x,
                            physical: physical_x,
                        })
                    });
                    state.logical = x;
                    state.physical = physical_x;
                } else {
                    m.horizontal = None;
                }
            });
        }
        for (state, viewport, request, applied) in [
            (&self.rows, &rv, rr.as_ref(), y),
            (&self.columns, &cv, cr.as_ref(), x),
        ] {
            state.update(cx, |s, cx| {
                if let Some(request) = request {
                    s.confirm(request, applied, viewport).unwrap();
                } else {
                    s.commit(&viewport.clone().with_offset(applied)).unwrap();
                }
                s.arm_scroll_idle(cx);
                for event in s.take_events() {
                    cx.emit(event);
                }
            });
        }
        let rows = self.rows.read(cx).snapshot(&rv.with_offset(y)).unwrap();
        let columns = self.columns.read(cx).snapshot(&cv.with_offset(x)).unwrap();
        let content_size = size(
            Pixels::from(columns.total_extent()),
            Pixels::from(rows.total_extent()),
        );
        let snapshot = VirtualGridSnapshot::new(rows, columns).unwrap();
        let partitioned = row_count > 0 || column_count > 0;
        if partitioned && frame.masks.is_none() {
            frame.masks = Some(Vec::new());
        }
        if !partitioned {
            frame.masks = None;
        }
        let hitbox = self.base.interactivity().prepaint(
            id,
            inspector,
            bounds,
            content_size,
            w,
            cx,
            |_, _, hitbox, w, cx| {
                let regions = [
                    Bounds::new(
                        bounds.origin
                            + point(if rtl { px(0.) } else { frozen_width }, frozen_height),
                        bounds.size - size(frozen_width, frozen_height),
                    ),
                    Bounds::new(
                        bounds.origin + point(if rtl { px(0.) } else { frozen_width }, px(0.)),
                        size(bounds.size.width - frozen_width, frozen_height),
                    ),
                    Bounds::new(
                        bounds.origin
                            + point(
                                if rtl {
                                    bounds.size.width - frozen_width
                                } else {
                                    px(0.)
                                },
                                frozen_height,
                            ),
                        size(frozen_width, bounds.size.height - frozen_height),
                    ),
                    Bounds::new(
                        bounds.origin
                            + point(
                                if rtl {
                                    bounds.size.width - frozen_width
                                } else {
                                    px(0.)
                                },
                                px(0.),
                            ),
                        size(frozen_width, frozen_height),
                    ),
                ];
                let mut ordinary = (!partitioned).then(|| snapshot.cells());
                let mut ordered = partitioned.then(|| snapshot.partitioned_cells());
                let ordinary = std::iter::from_fn(|| {
                    ordinary
                        .as_mut()
                        .map_or_else(|| ordered.as_mut().and_then(|it| it.next()), |it| it.next())
                });
                let mut keys = self
                    .options
                    .as_ref()
                    .map(|o| o.retained.clone())
                    .unwrap_or_default();
                if let Some(options) = &self.options {
                    keys.extend(
                        options
                            .focused
                            .iter()
                            .filter(|(_, handle)| handle.contains_focused(w, cx))
                            .map(|(key, _)| key.clone()),
                    );
                }
                let spans = self.options.as_ref().and_then(|o| o.spans.as_ref());
                let mut extras = snapshot.retained_cells(&keys);
                let mut ordinary = ordinary
                    .filter(|cell| {
                        spans.is_none_or(|spans| {
                            spans
                                .covering(cell.row.index(), cell.column.index())
                                .is_none()
                        })
                    })
                    .peekable();
                if let Some(spans) = spans {
                    let rows = snapshot.rows.working_indices();
                    let columns = snapshot.columns.working_indices();
                    let mut needed = std::collections::BTreeSet::new();
                    extras.retain(|cell| {
                        if let Some(index) = spans.covering(cell.row.index(), cell.column.index()) {
                            needed.insert(index);
                            false
                        } else {
                            true
                        }
                    });
                    spans.visit_intersecting(&rows, &columns, |index| {
                        needed.insert(index);
                    });
                    for index in needed {
                        let span = &spans.entries[index];
                        let row = snapshot.rows.item(span.rows.start).unwrap();
                        let column = snapshot.columns.item(span.columns.start).unwrap();
                        let last_row = snapshot.rows.item(span.rows.end - 1).unwrap();
                        let last_column = snapshot.columns.item(span.columns.end - 1).unwrap();
                        extras.push(VirtualGridCell {
                            frozen_row: snapshot.rows.is_frozen(row.index()),
                            frozen_column: snapshot.columns.is_frozen(column.index()),
                            span: Some(Rc::new(CellSpan {
                                rows: span.rows.len(),
                                columns: span.columns.len(),
                                extent: size(
                                    Pixels::from(last_column.start() - column.start())
                                        + last_column.extent(),
                                    Pixels::from(last_row.start() - row.start())
                                        + last_row.extent(),
                                ),
                            })),
                            row,
                            column,
                        });
                    }
                    extras.sort_unstable_by_key(cell_order);
                }
                let mut extras = extras.into_iter().peekable();
                let cells = std::iter::from_fn(|| match (ordinary.peek(), extras.peek()) {
                    (Some(a), Some(b)) if cell_order(b) < cell_order(a) => extras.next(),
                    (Some(_), _) => ordinary.next(),
                    _ => extras.next(),
                });
                w.with_content_mask(Some(ContentMask { bounds: clip }), |w| {
                    for cell in cells {
                        let row_key = cell.row.key().clone();
                        let column_key = cell.column.key().clone();
                        let extent = cell.extent();
                        let column_offset = Pixels::from(
                            cell.column.start() - if cell.frozen_column { 0. } else { x },
                        );
                        let origin = bounds.origin
                            + point(
                                if rtl {
                                    bounds.size.width - column_offset - extent.width
                                } else {
                                    column_offset
                                },
                                Pixels::from(
                                    cell.row.start() - if cell.frozen_row { 0. } else { y },
                                ),
                            );
                        let region =
                            usize::from(cell.frozen_row) + 2 * usize::from(cell.frozen_column);
                        let mask = regions[region].intersect(&clip);
                        let build = |w: &mut Window, cx: &mut App| {
                            let mut element = div()
                                .id(column_key)
                                .w(extent.width)
                                .h(extent.height)
                                .child((self.render)(cell, w, cx))
                                .test_support()
                                .into_any_element();
                            w.with_id(row_key.clone(), |w| {
                                element.layout_as_root(extent.map(AvailableSpace::Definite), w, cx);
                                element.prepaint_at(origin, w, cx);
                            });
                            frame.cells.push((row_key, element));
                            if let Some(masks) = &mut frame.masks {
                                masks.push(mask);
                            }
                        };
                        if row_count > 0 || column_count > 0 {
                            w.with_content_mask(Some(ContentMask { bounds: mask }), |w| {
                                build(w, cx)
                            });
                        } else {
                            build(w, cx);
                        }
                    }
                });
                hitbox
            },
        );
        if let Some(views) = &frame.views {
            let mut cache = views.borrow_mut();
            let CellViews { entries, touched } = &mut *cache;
            entries.retain(|key, _| touched.contains(key));
        }
        if visible
            && (self.rows.read(cx).has_scroll_command()
                || self.columns.read(cx).has_scroll_command())
        {
            w.request_animation_frame();
        }
        hitbox
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut VirtualGridFrame,
        hitbox: &mut Option<Hitbox>,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.base.interactivity().paint(
            id,
            inspector,
            bounds,
            hitbox.as_ref(),
            w,
            cx,
            |_, w, cx| {
                w.with_content_mask(Some(ContentMask { bounds }), |w| {
                    if let Some(masks) = &frame.masks {
                        for ((row, element), mask) in frame.cells.iter_mut().zip(masks) {
                            w.with_content_mask(Some(ContentMask { bounds: *mask }), |w| {
                                w.with_id(row.clone(), |w| element.paint(w, cx));
                            });
                        }
                    } else {
                        for (row, element) in &mut frame.cells {
                            w.with_id(row.clone(), |w| element.paint(w, cx));
                        }
                    }
                });
            },
        );
        let mut cells = std::mem::take(&mut frame.cells);
        cells.clear();
        let mut masks = frame.masks.take();
        let has_masks = masks.is_some();
        let mut masks = masks.take().unwrap_or_default();
        masks.clear();
        frame.mount.update(cx, |mount, _| {
            mount.cell_buffer = if cells.capacity() <= MAX_RETAINED_GRID_BUFFER_CAPACITY {
                cells
            } else {
                Vec::new()
            };
            if has_masks && masks.capacity() <= MAX_RETAINED_GRID_BUFFER_CAPACITY {
                mount.mask_buffer = masks;
            } else if has_masks {
                mount.mask_buffer = Vec::new();
            }
        });
    }
}
