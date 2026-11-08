//! Row adaptation only: Base owns all scrolling, measurement and convergence.
use super::{ListGrid, ListGridGeometry, YueListItem, grid::valid};
use crate::{
    VirtualizerChange, VirtualizerError, VirtualizerLayout, VirtualizerScrollOptions,
    VirtualizerSizing, VirtualizerSource, VirtualizerState, YueTheme, YueVirtualList,
};
use gpui_kit::{
    App, Axis, Context, ElementId, Entity, FocusHandle, IntoElement, ParentElement as _,
    RenderOnce, Role, StyleRefinement, Styled, Window, div, px,
};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

#[derive(Clone)]
enum Sizing {
    Fixed(gpui_kit::Pixels),
    Known(Rc<[gpui_kit::Pixels]>),
    Measured(gpui_kit::Pixels),
}
/// Heights describe complete cards (including padding), excluding the grid gap.
/// Known sizes follow source order; replace them with the source after edits.
#[derive(Clone)]
pub struct ListGridSizing(Sizing);
impl ListGridSizing {
    pub fn fixed(height: gpui_kit::Pixels) -> Self {
        Self(Sizing::Fixed(height))
    }
    pub fn known(heights: impl Into<Rc<[gpui_kit::Pixels]>>) -> Self {
        Self(Sizing::Known(heights.into()))
    }
    pub fn measured(estimate: gpui_kit::Pixels) -> Self {
        Self(Sizing::Measured(estimate))
    }
    fn validate(&self, count: usize) -> Result<(), VirtualizerError> {
        let valid = match &self.0 {
            Sizing::Fixed(h) | Sizing::Measured(h) => valid(*h, true),
            Sizing::Known(heights) => {
                heights.len() == count && heights.iter().all(|h| valid(*h, true))
            }
        };
        if valid {
            Ok(())
        } else {
            Err(VirtualizerError::InvalidGeometry("list grid card heights"))
        }
    }
    fn rows(&self, source: &Rows) -> VirtualizerSizing {
        match &self.0 {
            Sizing::Fixed(h) => VirtualizerSizing::fixed(*h),
            Sizing::Measured(h) => VirtualizerSizing::measured(*h),
            Sizing::Known(heights) => VirtualizerSizing::known(
                heights
                    .chunks(source.columns)
                    .map(|row| row.iter().copied().fold(px(0.), |a, b| a.max(b)))
                    .collect::<Vec<_>>(),
            ),
        }
    }
    fn validate_gap(&self, gap: gpui_kit::Pixels) -> Result<(), VirtualizerError> {
        if let Sizing::Measured(estimate) = self.0
            && !valid(estimate + gap, true)
        {
            return Err(VirtualizerError::InvalidGeometry(
                "list grid estimate plus gap",
            ));
        }
        Ok(())
    }
}

struct Rows {
    items: Rc<dyn VirtualizerSource>,
    columns: usize,
    // Promote the old reading key inside its new row. Base can then restore its
    // own Within/Before/After anchor even when the key is no longer row-first.
    reading_key: Option<ElementId>,
}
impl Rows {
    fn range(&self, row: usize) -> std::ops::Range<usize> {
        let start = row * self.columns;
        start..start.saturating_add(self.columns).min(self.items.len())
    }
    fn row_key_for(&self, key: &ElementId) -> Option<ElementId> {
        self.items
            .index_of(key)
            .map(|ix| self.key_at(ix / self.columns))
    }
}
impl VirtualizerSource for Rows {
    fn len(&self) -> usize {
        self.items.len().div_ceil(self.columns)
    }
    fn key_at(&self, row: usize) -> ElementId {
        if let Some(key) = &self.reading_key
            && self
                .items
                .index_of(key)
                .is_some_and(|ix| ix / self.columns == row)
        {
            return key.clone();
        }
        self.items.key_at(row * self.columns)
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        let row = self.items.index_of(key)? / self.columns;
        (self.key_at(row) == *key).then_some(row)
    }
    fn revision_at(&self, row: usize) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for ix in self.range(row) {
            self.items.key_at(ix).hash(&mut hash);
            self.items.revision_at(ix).hash(&mut hash);
        }
        hash.finish()
    }
}

/// Retained card-to-row mapping. Construct once in the host, outside render.
/// Card data, editing entities and focus handles remain owned by the host.
pub struct ListGridState {
    rows: Rc<Rows>,
    base: Entity<VirtualizerState>,
    grid: ListGrid,
    sizing: ListGridSizing,
    geometry: ListGridGeometry,
    width: gpui_kit::Pixels,
    rem: gpui_kit::Pixels,
    revision: u64,
    focus: HashMap<ElementId, FocusHandle>,
    removed_focus: Vec<FocusHandle>,
    fallback: Option<FocusHandle>,
}
impl ListGridState {
    pub fn new(
        source: Rc<dyn VirtualizerSource>,
        grid: ListGrid,
        sizing: ListGridSizing,
        cx: &mut App,
    ) -> Result<Self, VirtualizerError> {
        validate_source(source.as_ref())?;
        sizing.validate(source.len())?;
        let geometry = grid.resolve(px(0.), px(16.))?;
        let rows = Rc::new(Rows {
            items: source,
            columns: usize::from(geometry.columns()),
            reading_key: None,
        });
        let mut base = VirtualizerState::new(Axis::Vertical, rows.clone(), sizing.rows(&rows))?;
        base.set_layout(VirtualizerLayout::default().gap(geometry.gap()))?;
        Ok(Self {
            rows,
            base: gpui_kit::AppContext::new(cx, |_| base),
            grid,
            sizing,
            geometry,
            width: px(0.),
            rem: px(16.),
            revision: 0,
            focus: HashMap::new(),
            removed_focus: Vec::new(),
            fallback: None,
        })
    }
    pub fn geometry(&self) -> ListGridGeometry {
        self.geometry
    }
    pub fn len(&self) -> usize {
        self.rows.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Access to the single Base scroll/measurement owner for commands and snapshots.
    /// Use this adapter's source/configuration methods to keep the mapping valid;
    /// replacing Base's source directly invalidates this adapter's row mapping.
    pub fn virtualizer(&self) -> &Entity<VirtualizerState> {
        &self.base
    }
    pub fn scroll_to_key(
        &mut self,
        key: &ElementId,
        options: VirtualizerScrollOptions,
        cx: &mut Context<Self>,
    ) -> Result<u64, VirtualizerError> {
        let row_key = self
            .rows
            .row_key_for(key)
            .ok_or(VirtualizerError::MissingKey)?;
        self.base.update(cx, |state, cx| {
            let result = state.scroll_to_key(row_key, options);
            cx.notify();
            result
        })
    }
    /// The handle may identify a card's focus scope or a real control inside it.
    /// Register host-owned scopes to retain focused cards across virtual unload
    /// and reflow. Private keyed control state alone cannot survive reparenting.
    pub fn set_focus_handle(
        &mut self,
        key: ElementId,
        handle: Option<FocusHandle>,
    ) -> Result<(), VirtualizerError> {
        if self.rows.items.index_of(&key).is_none() {
            return Err(VirtualizerError::MissingKey);
        }
        if handle.as_ref().is_some_and(|handle| {
            self.focus
                .iter()
                .any(|(other, current)| *other != key && current == handle)
        }) {
            return Err(VirtualizerError::InvalidSource);
        }
        if let Some(handle) = handle {
            self.focus.insert(key, handle);
        } else {
            self.focus.remove(&key);
        }
        Ok(())
    }
    pub fn set_focus_fallback(&mut self, handle: Option<FocusHandle>) {
        self.fallback = handle;
    }
    pub fn replace_source(
        &mut self,
        source: Rc<dyn VirtualizerSource>,
        sizing: ListGridSizing,
        cx: &mut Context<Self>,
    ) -> Result<(), VirtualizerError> {
        validate_source(source.as_ref())?;
        sizing.validate(source.len())?;
        self.rebuild(source, self.grid, sizing, self.width, self.rem, cx)?;
        Ok(())
    }
    pub fn set_grid(
        &mut self,
        grid: ListGrid,
        cx: &mut Context<Self>,
    ) -> Result<(), VirtualizerError> {
        self.resize(grid, self.width, self.rem, cx)?;
        Ok(())
    }
    fn resize(
        &mut self,
        grid: ListGrid,
        width: gpui_kit::Pixels,
        rem: gpui_kit::Pixels,
        cx: &mut Context<Self>,
    ) -> Result<(), VirtualizerError> {
        let geometry = grid.resolve(width, rem)?;
        self.sizing.validate_gap(geometry.gap())?;
        if grid == self.grid && width == self.width && rem == self.rem {
            return Ok(());
        }
        if geometry.columns() != self.geometry.columns() {
            return self.rebuild(
                self.rows.items.clone(),
                grid,
                self.sizing.clone(),
                width,
                rem,
                cx,
            );
        }
        // Width-only changes invalidate measurements without rebuilding the
        // domain mapping or remapping every source row.
        self.grid = grid;
        self.geometry = geometry;
        self.width = width;
        self.rem = rem;
        self.invalidate(cx)
    }
    /// Call for theme/density/size-affecting content changes under existing keys.
    /// Width and rem changes are invalidated automatically by the presentation.
    pub fn invalidate(&mut self, cx: &mut Context<Self>) -> Result<(), VirtualizerError> {
        self.revision = self.revision.wrapping_add(1);
        let gap = self.geometry.gap();
        let revision = self.revision;
        self.base.update(cx, |state, cx| {
            let result = state.set_layout(VirtualizerLayout::default().gap(gap).revision(revision));
            cx.notify();
            result
        })?;
        cx.notify();
        Ok(())
    }
    fn rebuild(
        &mut self,
        source: Rc<dyn VirtualizerSource>,
        grid: ListGrid,
        sizing: ListGridSizing,
        width: gpui_kit::Pixels,
        rem: gpui_kit::Pixels,
        cx: &mut Context<Self>,
    ) -> Result<(), VirtualizerError> {
        let geometry = grid.resolve(width, rem)?;
        sizing.validate_gap(geometry.gap())?;
        let mut anchor_removed = false;
        let reading_key = self.base.read(cx).viewport().and_then(|viewport| {
            let snapshot = self.base.read(cx).snapshot(viewport).ok()?;
            let mut row = snapshot.visible_range().start;
            let item = snapshot.item(row).ok()?;
            if viewport.offset() >= item.start() + f64::from(item.extent())
                && row + 1 < self.rows.len()
            {
                row += 1;
            }
            let key = self.rows.key_at(row);
            let index = self.rows.items.index_of(&key)?;
            anchor_removed = source.index_of(&key).is_none();
            (index..self.rows.items.len())
                .chain((0..index).rev())
                .find_map(|index| {
                    let key = self.rows.items.key_at(index);
                    source.index_of(&key).map(|_| key)
                })
        });
        let rows = Rc::new(Rows {
            items: source,
            columns: usize::from(geometry.columns()),
            reading_key,
        });
        let row_sizing = sizing.rows(&rows);
        self.base.update(cx, |state, _| {
            state.replace_source(
                rows.clone(),
                row_sizing,
                VirtualizerChange::Replace {
                    preserve_anchor: true,
                },
            )
        })?;
        self.removed_focus.extend(
            self.focus
                .iter()
                .filter(|(key, _)| rows.items.index_of(key).is_none())
                .map(|(_, handle)| handle.clone()),
        );
        self.focus
            .retain(|key, _| rows.items.index_of(key).is_some());
        self.rows = rows;
        self.grid = grid;
        self.sizing = sizing;
        self.geometry = geometry;
        self.width = width;
        self.rem = rem;
        self.invalidate(cx)?;
        // Base's missing-row fallback advances by old rows. At the card seam,
        // prefer the next surviving business item (possibly a sibling card).
        if anchor_removed && let Some(key) = self.rows.reading_key.clone() {
            self.scroll_to_key(&key, crate::VirtualizerAlign::Start.into(), cx)?;
        }
        Ok(())
    }
    fn update_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .removed_focus
            .drain(..)
            .any(|handle| handle.contains_focused(window, cx))
        {
            if let Some(handle) = &self.fallback {
                handle.focus(window, cx);
            } else {
                window.blur(cx);
            }
        }
        let keys = self
            .focus
            .iter()
            .filter(|(_, handle)| handle.contains_focused(window, cx))
            .filter_map(|(key, _)| self.rows.row_key_for(key))
            .collect();
        self.base
            .update(cx, |state, _| state.set_retained_keys(keys))
            .expect("validated focus keys");
    }
}
fn validate_source(source: &dyn VirtualizerSource) -> Result<(), VirtualizerError> {
    for ix in 0..source.len() {
        if source.index_of(&source.key_at(ix)) != Some(ix) {
            return Err(VirtualizerError::InvalidSource);
        }
    }
    Ok(())
}

type Renderer = Rc<dyn Fn(usize, &ElementId, &mut Window, &mut App) -> YueListItem>;
/// Explicit, finite-height virtual card list. Every Base item is one native
/// grid row; only working rows and registered focused cards are materialized.
#[derive(IntoElement)]
pub struct YueVirtualListGrid {
    id: ElementId,
    state: Entity<ListGridState>,
    render: Renderer,
    label: Option<gpui_kit::SharedString>,
    theme: Option<YueTheme>,
    scrollbar: bool,
    style: StyleRefinement,
}
impl YueVirtualListGrid {
    pub fn new(
        id: impl Into<ElementId>,
        state: &Entity<ListGridState>,
        render: impl Fn(usize, &ElementId, &mut Window, &mut App) -> YueListItem + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            render: Rc::new(render),
            label: None,
            theme: None,
            scrollbar: true,
            style: Default::default(),
        }
    }
    pub fn accessibility_label(mut self, label: impl Into<gpui_kit::SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn scrollbar(mut self, enabled: bool) -> Self {
        self.scrollbar = enabled;
        self
    }
}
impl Styled for YueVirtualListGrid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl RenderOnce for YueVirtualListGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let observed = self.state.clone();
        let _subscription = window.use_keyed_state((self.id.clone(), "observe"), cx, |_, cx| {
            cx.observe(&observed, |_, _, cx| cx.notify())
        });
        self.state
            .update(cx, |state, cx| state.update_focus(window, cx));
        let state = self.state.read(cx);
        let rows = state.rows.clone();
        let base = state.base.clone();
        let geometry = state.geometry;
        let measured = matches!(state.sizing.0, Sizing::Measured(_));
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let render = self.render;
        let mut list = YueVirtualList::new(self.id, &base, move |row, window, cx| {
            use gpui_kit::{InteractiveElement as _, base::TestSupportExt as _};
            let mut el = div()
                .id("list-grid-row")
                .test_support()
                .w_full()
                .grid()
                .grid_cols(geometry.columns())
                .gap(geometry.gap());
            if !measured {
                el = el.h(row.extent());
            }
            for ix in rows.range(row.index()) {
                let key = rows.items.key_at(ix);
                let mut item = render(ix, &key, window, cx);
                item.id = key;
                item.logical_index = ix;
                item.set_size = Some(rows.items.len());
                item.semantics = true;
                item.striped = false;
                item.theme = item.theme.or(Some(theme));
                el = el.child(item);
            }
            el
        })
        .theme(theme)
        .semantics(Some(Role::List), None)
        .scrollbar(self.scrollbar);
        if let Some(label) = self.label {
            list = list.accessibility_label(label);
        }
        use gpui_kit::Refineable as _;
        let state = self.state;
        // The observer is an overlay: it neither adds a card nor affects height.
        let observer = gpui_kit::canvas(
            move |bounds, window, cx| {
                let rem = window.rem_size();
                let current = state.read(cx);
                if current.width != bounds.size.width || current.rem != rem {
                    let width = bounds.size.width;
                    window.defer(cx, move |_, cx| {
                        state.update(cx, |state, cx| {
                            if state.width != width || state.rem != rem {
                                state
                                    .resize(state.grid, width, rem, cx)
                                    .expect("valid measured list grid bounds");
                            }
                        })
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let mut root = div().relative().size_full().min_w_0().min_h_0();
        root.style().refine(&self.style);
        root.child(list).child(observer)
    }
}
