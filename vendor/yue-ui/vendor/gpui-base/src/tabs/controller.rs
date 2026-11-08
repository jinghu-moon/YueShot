//! Controlled tab-list navigation and scroll geometry; no presentation policy.
use crate::AxisExt as _;
use gpui::{
    App, Axis, Bounds, Entity, FocusHandle, KeyDownEvent, Pixels, ScrollHandle, SharedString,
    Window, px,
};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TabsActivation {
    #[default]
    Automatic,
    Manual,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabsChangeSource {
    Pointer,
    Keyboard,
    Accessibility,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TabsScrollAlign {
    #[default]
    Auto,
    Start,
    Center,
    End,
}

#[derive(Clone, Debug)]
pub struct TabDescriptor {
    key: SharedString,
    disabled: bool,
    closable: bool,
}
impl TabDescriptor {
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            disabled: false,
            closable: false,
        }
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }
    pub fn closable(mut self, value: bool) -> Self {
        self.closable = value;
        self
    }
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn is_closable(&self) -> bool {
        self.closable
    }
}
#[derive(Clone, Debug)]
pub struct TabsChangeRequest {
    key: SharedString,
    source: TabsChangeSource,
}
impl TabsChangeRequest {
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn source(&self) -> TabsChangeSource {
        self.source
    }
}
#[derive(Clone, Debug)]
pub struct TabsCloseRequest {
    key: SharedString,
    next: Option<SharedString>,
}
impl TabsCloseRequest {
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    /// Candidate based on the current snapshot; the owner must revalidate after an asynchronous decision.
    pub fn next_key(&self) -> Option<&SharedString> {
        self.next.as_ref()
    }
}

pub struct TabsSnapshot {
    items: Vec<TabDescriptor>,
    selected: Option<SharedString>,
    axis: Axis,
    disabled: bool,
    activation: TabsActivation,
}
impl TabsSnapshot {
    pub fn new(
        items: impl IntoIterator<Item = TabDescriptor>,
        selected: Option<SharedString>,
    ) -> Self {
        Self {
            items: items.into_iter().collect(),
            selected,
            axis: Axis::Horizontal,
            disabled: false,
            activation: TabsActivation::Automatic,
        }
    }
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn activation(mut self, activation: TabsActivation) -> Self {
        self.activation = activation;
        self
    }
}
type ChangeHandler = Rc<dyn Fn(&TabsChangeRequest, &mut Window, &mut App)>;
type CloseHandler = Rc<dyn Fn(&TabsCloseRequest, &mut Window, &mut App)>;

/// Retained interaction state. `sync` receives the owner's committed selection;
/// activation only requests a change and never edits that snapshot.
pub struct TabsController {
    snapshot: TabsSnapshot,
    handles: HashMap<SharedString, FocusHandle>,
    focused: Option<SharedString>,
    visited: HashSet<SharedString>,
    panel: FocusHandle,
    fallback: Option<FocusHandle>,
    scroll: ScrollHandle,
    cells: Vec<(SharedString, Bounds<Pixels>)>,
    viewport: Pixels,
    extent: Pixels,
    reveal: Option<(SharedString, TabsScrollAlign)>,
    scroll_target: Option<Pixels>,
    on_change: Option<ChangeHandler>,
    on_close: Option<CloseHandler>,
}
impl TabsController {
    pub fn new(cx: &mut App) -> Self {
        Self {
            snapshot: TabsSnapshot::new([], None),
            handles: HashMap::new(),
            focused: None,
            visited: HashSet::new(),
            panel: cx.focus_handle().tab_stop(false),
            fallback: None,
            scroll: ScrollHandle::new(),
            cells: Vec::new(),
            viewport: px(0.),
            extent: px(0.),
            reveal: None,
            scroll_target: None,
            on_change: None,
            on_close: None,
        }
    }
    pub fn sync(&mut self, snapshot: TabsSnapshot, window: &mut Window, cx: &mut App) {
        let mut keys = HashSet::new();
        for item in &snapshot.items {
            assert!(
                !item.key.trim().is_empty() && keys.insert(item.key.clone()),
                "Tabs requires unique nonempty keys"
            );
            self.handles
                .entry(item.key.clone())
                .or_insert_with(|| cx.focus_handle());
        }
        let actual_focus = self
            .handles
            .iter()
            .find(|(_, h)| h.is_focused(window))
            .map(|(key, _)| key.clone());
        let selected = snapshot
            .selected
            .as_ref()
            .filter(|key| keys.contains(*key))
            .cloned();
        let selected_changed = selected != self.selected_key().cloned();
        let old_items = &self.snapshot.items;
        let old_focus_ix = actual_focus
            .as_ref()
            .and_then(|key| old_items.iter().position(|i| &i.key == key));
        let enabled = |key: &SharedString| {
            !snapshot.disabled && snapshot.items.iter().any(|i| &i.key == key && !i.disabled)
        };
        let candidate = selected
            .as_ref()
            .filter(|key| enabled(key))
            .cloned()
            .or_else(|| self.focused.as_ref().filter(|key| enabled(key)).cloned())
            .or_else(|| {
                snapshot
                    .items
                    .iter()
                    .find(|i| !snapshot.disabled && !i.disabled)
                    .map(|i| i.key.clone())
            });
        let lost_focus = actual_focus.as_ref().is_some_and(|key| !enabled(key));
        if lost_focus || (selected_changed && self.panel.contains_focused(window, cx)) {
            let neighbor = old_focus_ix.and_then(|ix| {
                old_items[ix + 1..]
                    .iter()
                    .chain(old_items[..ix].iter().rev())
                    .find(|i| enabled(&i.key))
                    .map(|i| i.key.clone())
            });
            let next = if lost_focus {
                neighbor.or(candidate.clone())
            } else {
                candidate.clone()
            };
            if let Some(handle) = next
                .as_ref()
                .and_then(|key| self.handles.get(key))
                .or(self.fallback.as_ref())
            {
                handle.focus(window, cx);
            } else if self.panel.contains_focused(window, cx) || actual_focus.is_some() {
                window.blur(cx);
            }
            self.focused = next;
        } else {
            self.focused = actual_focus.filter(|key| enabled(key)).or(candidate);
        }
        let shape_changed = self.snapshot.axis != snapshot.axis
            || self
                .snapshot
                .items
                .iter()
                .map(|i| (&i.key, i.disabled, i.closable))
                .collect::<Vec<_>>()
                != snapshot
                    .items
                    .iter()
                    .map(|i| (&i.key, i.disabled, i.closable))
                    .collect::<Vec<_>>();
        self.snapshot = snapshot;
        self.handles.retain(|key, _| keys.contains(key));
        self.visited.retain(|key| keys.contains(key));
        if let Some(key) = selected {
            self.visited.insert(key.clone());
            if selected_changed || shape_changed {
                let reveal_key = if selected_changed {
                    key
                } else {
                    self.focused.clone().unwrap_or(key)
                };
                self.reveal = Some((reveal_key, TabsScrollAlign::Auto));
            }
        }
        if shape_changed {
            self.cells.clear();
        }
    }
    pub fn on_change(
        &mut self,
        handler: impl Fn(&TabsChangeRequest, &mut Window, &mut App) + 'static,
    ) {
        self.on_change = Some(Rc::new(handler));
    }
    pub fn on_close(
        &mut self,
        handler: impl Fn(&TabsCloseRequest, &mut Window, &mut App) + 'static,
    ) {
        self.on_close = Some(Rc::new(handler));
    }
    pub fn clear_handlers(&mut self) {
        self.on_change = None;
        self.on_close = None;
    }
    pub fn fallback_focus(&mut self, handle: Option<FocusHandle>) {
        self.fallback = handle;
    }
    pub fn panel_focus(&self) -> FocusHandle {
        self.panel.clone()
    }
    pub fn scroll_handle(&self) -> ScrollHandle {
        self.scroll.clone()
    }
    pub fn focus_handle(&self, key: &SharedString) -> Option<FocusHandle> {
        self.handles.get(key).cloned()
    }
    pub fn focused_key(&self) -> Option<&SharedString> {
        self.focused.as_ref()
    }
    pub fn selected_key(&self) -> Option<&SharedString> {
        self.snapshot
            .selected
            .as_ref()
            .filter(|key| self.snapshot.items.iter().any(|i| &i.key == *key))
    }
    pub fn axis(&self) -> Axis {
        self.snapshot.axis
    }
    pub fn is_disabled(&self) -> bool {
        self.snapshot.disabled
    }
    pub fn is_visited(&self, key: &SharedString) -> bool {
        self.visited.contains(key)
    }
    pub fn is_item_disabled(&self, key: &SharedString) -> bool {
        self.snapshot.disabled
            || self
                .snapshot
                .items
                .iter()
                .find(|i| &i.key == key)
                .is_none_or(|i| i.disabled)
    }
    pub fn is_tab_stop(&self, key: &SharedString) -> bool {
        !self.is_item_disabled(key) && self.focused.as_ref() == Some(key)
    }
    pub fn request_reveal(&mut self, key: SharedString, align: TabsScrollAlign) {
        if self.snapshot.items.iter().any(|item| item.key == key) {
            self.reveal = Some((key, align));
        }
    }
    pub fn next_after_close(&self, key: &SharedString) -> Option<SharedString> {
        let ix = self.snapshot.items.iter().position(|i| &i.key == key)?;
        self.snapshot.items[ix + 1..]
            .iter()
            .chain(self.snapshot.items[..ix].iter().rev())
            .find(|i| !i.disabled)
            .map(|i| i.key.clone())
    }
    pub fn activate(
        entity: &Entity<Self>,
        key: &SharedString,
        source: TabsChangeSource,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus, change) = entity.update(cx, |state, cx| {
            if state.is_item_disabled(key) {
                return (None, None);
            }
            state.focused = Some(key.clone());
            state.request_reveal(key.clone(), TabsScrollAlign::Auto);
            cx.notify();
            let change = (state.selected_key() != Some(key))
                .then(|| state.on_change.clone())
                .flatten();
            (state.focus_handle(key), change)
        });
        if let Some(focus) = focus {
            focus.focus(window, cx);
        }
        if let Some(handler) = change {
            handler(
                &TabsChangeRequest {
                    key: key.clone(),
                    source,
                },
                window,
                cx,
            );
        }
    }
    pub fn close(entity: &Entity<Self>, key: &SharedString, window: &mut Window, cx: &mut App) {
        let state = entity.read(cx);
        if state.is_item_disabled(key)
            || !state
                .snapshot
                .items
                .iter()
                .any(|i| &i.key == key && i.closable)
        {
            return;
        }
        let request = TabsCloseRequest {
            key: key.clone(),
            next: state.next_after_close(key),
        };
        let handler = state.on_close.clone();
        if let Some(handler) = handler {
            handler(&request, window, cx);
        }
    }
    pub fn handle_key(
        entity: &Entity<Self>,
        key: &SharedString,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = entity.read(cx);
        if state.is_item_disabled(key)
            || !state
                .focus_handle(key)
                .is_some_and(|h| h.is_focused(window))
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.alt
        {
            return;
        }
        let command = event.keystroke.key.as_str();
        let direction = match (state.axis(), command) {
            (_, "home") => Some(0),
            (_, "end") => Some(1),
            (Axis::Horizontal, "right") | (Axis::Vertical, "down") => Some(2),
            (Axis::Horizontal, "left") | (Axis::Vertical, "up") => Some(3),
            _ => None,
        };
        if let Some(direction) = direction {
            let keys = state
                .snapshot
                .items
                .iter()
                .filter(|i| !i.disabled)
                .map(|i| i.key.clone())
                .collect::<Vec<_>>();
            if keys.is_empty() {
                return;
            }
            let ix = keys.iter().position(|k| k == key).unwrap_or(0);
            let next = keys[match direction {
                0 => 0,
                1 => keys.len() - 1,
                2 => (ix + 1) % keys.len(),
                _ => (ix + keys.len() - 1) % keys.len(),
            }]
            .clone();
            let automatic = state.snapshot.activation == TabsActivation::Automatic;
            if automatic {
                Self::activate(entity, &next, TabsChangeSource::Keyboard, window, cx);
            } else {
                let focus = entity.update(cx, |state, cx| {
                    state.focused = Some(next.clone());
                    state.request_reveal(next.clone(), TabsScrollAlign::Auto);
                    cx.notify();
                    state.focus_handle(&next)
                });
                if let Some(focus) = focus {
                    focus.focus(window, cx);
                }
            }
            cx.stop_propagation();
        } else if command == "enter" || command == "space" {
            if !event.is_held {
                Self::activate(entity, key, TabsChangeSource::Keyboard, window, cx);
            }
            cx.stop_propagation();
        } else if command == "delete"
            && state
                .snapshot
                .items
                .iter()
                .any(|i| &i.key == key && i.closable)
        {
            if !event.is_held {
                Self::close(entity, key, window, cx);
            }
            cx.stop_propagation();
        }
    }
    /// Content-coordinate cells and the viewport's main-axis length.
    pub fn record_layout(&mut self, cells: Vec<(SharedString, Bounds<Pixels>)>, viewport: Pixels) {
        // Compare content geometry, never screen position: ordinary wheel motion
        // must not continually pull the owner-selected item back into view.
        let geometry_changed = (self.viewport - viewport).abs() > px(0.5)
            || self.cells.len() != cells.len()
            || self.cells.iter().zip(&cells).any(|((ka, a), (kb, b))| {
                ka != kb
                    || (a.origin.x - b.origin.x).abs() > px(0.5)
                    || (a.origin.y - b.origin.y).abs() > px(0.5)
                    || (a.size.width - b.size.width).abs() > px(0.5)
                    || (a.size.height - b.size.height).abs() > px(0.5)
            });
        if geometry_changed && self.reveal.is_none() && self.scroll_target.is_none() {
            if let Some(key) = self
                .focused
                .as_ref()
                .or_else(|| self.selected_key())
                .cloned()
            {
                self.request_reveal(key, TabsScrollAlign::Auto);
            }
        }
        self.extent = cells
            .iter()
            .map(|(_, b)| {
                if self.axis().is_horizontal() {
                    b.right()
                } else {
                    b.bottom()
                }
            })
            .fold(px(0.), Pixels::max);
        self.cells = cells;
        self.viewport = viewport;
    }
    pub fn viewport(&self) -> Pixels {
        self.viewport
    }
    pub fn extent(&self) -> Pixels {
        self.extent
    }
    pub fn offset(&self) -> Pixels {
        -(if self.axis().is_horizontal() {
            self.scroll.offset().x
        } else {
            self.scroll.offset().y
        })
    }
    pub fn is_overflowing(&self) -> bool {
        self.extent > self.viewport + px(0.5)
    }
    pub fn page(&mut self, forward: bool) {
        let overlap = self
            .cells
            .first()
            .map(|(_, b)| {
                if self.axis().is_horizontal() {
                    b.size.width
                } else {
                    b.size.height
                }
            })
            .unwrap_or(px(0.));
        let step = (self.viewport - overlap.min(self.viewport / 2.)).max(px(1.));
        let current = self.offset();
        let maximum = (self.extent - self.viewport).max(px(0.));
        let desired = (current + if forward { step } else { -step }).clamp(px(0.), maximum);
        let start = |b: &Bounds<Pixels>| {
            if self.axis().is_horizontal() {
                b.left()
            } else {
                b.top()
            }
        };
        let end = |b: &Bounds<Pixels>| {
            if self.axis().is_horizontal() {
                b.right()
            } else {
                b.bottom()
            }
        };
        let oversized = self.cells.iter().any(|(_, b)| {
            end(b) - start(b) > self.viewport
                && if forward {
                    start(b) <= current && end(b) > current + self.viewport
                } else {
                    start(b) < current && end(b) >= current
                }
        });
        let target = if oversized {
            // A single wide item still advances through its own visible slices.
            (current
                + if forward {
                    self.viewport
                } else {
                    -self.viewport
                })
            .clamp(px(0.), maximum)
        } else if desired == px(0.) || desired == maximum {
            desired
        } else {
            self.cells
                .iter()
                .map(|(_, b)| start(b).clamp(px(0.), maximum))
                .filter(|p| if forward { *p > current } else { *p < current })
                .min_by(|a, b| {
                    (*a - desired)
                        .abs()
                        .partial_cmp(&(*b - desired).abs())
                        .unwrap()
                })
                .unwrap_or(desired)
        };
        self.scroll_target = Some(target);
    }
    pub fn take_scroll_target(&mut self) -> Option<Pixels> {
        if let Some((key, align)) = &self.reveal {
            if let Some((_, b)) = self.cells.iter().find(|(k, _)| k == key) {
                let (start, end) = if self.axis().is_horizontal() {
                    (b.left(), b.right())
                } else {
                    (b.top(), b.bottom())
                };
                let current = self.offset();
                let target = match align {
                    TabsScrollAlign::Start => start,
                    TabsScrollAlign::Center => (start + end - self.viewport) / 2.,
                    TabsScrollAlign::End => end - self.viewport,
                    TabsScrollAlign::Auto if end - start > self.viewport || start < current => {
                        start
                    }
                    TabsScrollAlign::Auto if end > current + self.viewport => end - self.viewport,
                    TabsScrollAlign::Auto => current,
                };
                self.scroll_target =
                    Some(target.clamp(px(0.), (self.extent - self.viewport).max(px(0.))));
                self.reveal = None;
            }
        }
        self.scroll_target.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn neighbors_skip_disabled_and_selection_remains_controlled(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let mut state = TabsController::new(cx);
            state.snapshot = TabsSnapshot::new(
                [
                    TabDescriptor::new("a"),
                    TabDescriptor::new("b").disabled(true),
                    TabDescriptor::new("c"),
                ],
                Some("a".into()),
            );
            assert_eq!(state.next_after_close(&"a".into()).as_deref(), Some("c"));
            assert_eq!(state.next_after_close(&"c".into()).as_deref(), Some("a"));
            assert_eq!(state.selected_key().map(|k| k.as_ref()), Some("a"));
        });
    }

    #[gpui::test]
    fn scroll_pages_align_to_cells_and_oversized_items_advance_in_slices(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            for axis in [Axis::Horizontal, Axis::Vertical] {
                let mut s = TabsController::new(cx);
                s.snapshot =
                    TabsSnapshot::new((0..5).map(|ix| TabDescriptor::new(format!("{ix}"))), None)
                        .axis(axis);
                let cells = (0..5)
                    .map(|ix| {
                        let (x, y, width, height) = if axis.is_horizontal() {
                            (ix as f32 * 100., 0., 100., 32.)
                        } else {
                            (0., ix as f32 * 100., 100., 100.)
                        };
                        (
                            format!("{ix}").into(),
                            Bounds::new(
                                gpui::point(px(x), px(y)),
                                gpui::size(px(width), px(height)),
                            ),
                        )
                    })
                    .collect();
                s.record_layout(cells, px(250.));
                s.page(true);
                assert_eq!(s.take_scroll_target(), Some(px(100.)));
                s.scroll.set_offset(if axis.is_horizontal() {
                    gpui::point(px(-100.), px(0.))
                } else {
                    gpui::point(px(0.), px(-100.))
                });
                s.page(true);
                assert_eq!(s.take_scroll_target(), Some(px(250.)));
            }
            let mut s = TabsController::new(cx);
            s.snapshot = TabsSnapshot::new([TabDescriptor::new("wide")], None);
            s.record_layout(
                vec![(
                    "wide".into(),
                    Bounds::new(gpui::point(px(0.), px(0.)), gpui::size(px(600.), px(32.))),
                )],
                px(200.),
            );
            s.scroll.set_offset(gpui::point(px(-300.), px(0.)));
            s.page(false);
            assert_eq!(s.take_scroll_target(), Some(px(100.)));
            s.page(true);
            assert_eq!(s.take_scroll_target(), Some(px(400.)));
        });
    }
}
