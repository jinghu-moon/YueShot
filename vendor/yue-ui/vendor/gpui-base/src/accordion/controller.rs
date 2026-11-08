//! Controlled disclosure groups. No layout, styling or animation policy.
use gpui::{App, Entity, FocusHandle, KeyDownEvent, SharedString, Window};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollapseMode {
    #[default]
    Multiple,
    Accordion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollapseChangeSource {
    Pointer,
    Keyboard,
    Accessibility,
}

#[derive(Clone, Debug)]
pub struct CollapseChangeRequest {
    key: SharedString,
    expanded: bool,
    expanded_keys: Vec<SharedString>,
    source: CollapseChangeSource,
}
impl CollapseChangeRequest {
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }
    pub fn expanded_keys(&self) -> &[SharedString] {
        &self.expanded_keys
    }
    pub fn source(&self) -> CollapseChangeSource {
        self.source
    }
}

#[derive(Clone, Debug)]
pub struct AccordionDescriptor {
    key: SharedString,
    disabled: bool,
}
impl AccordionDescriptor {
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            disabled: false,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn key(&self) -> &SharedString {
        &self.key
    }
}

pub struct AccordionSnapshot {
    items: Vec<AccordionDescriptor>,
    expanded: Vec<SharedString>,
    mode: CollapseMode,
    disabled: bool,
}
impl AccordionSnapshot {
    pub fn new(
        items: impl IntoIterator<Item = AccordionDescriptor>,
        expanded: impl IntoIterator<Item = SharedString>,
    ) -> Self {
        Self {
            items: items.into_iter().collect(),
            expanded: expanded.into_iter().collect(),
            mode: CollapseMode::Multiple,
            disabled: false,
        }
    }
    pub fn mode(mut self, mode: CollapseMode) -> Self {
        self.mode = mode;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

type ChangeHandler = Rc<dyn Fn(&CollapseChangeRequest, &mut Window, &mut App)>;
struct Handles {
    trigger: FocusHandle,
    panel: FocusHandle,
}

/// Owns focus identities and a projection of the owner's last committed value.
/// Requests never mutate that value, including when an owner rejects a request.
pub struct AccordionController {
    snapshot: AccordionSnapshot,
    handles: HashMap<SharedString, Handles>,
    boundary: FocusHandle,
    on_change: Option<ChangeHandler>,
    physical_activation: Option<SharedString>,
}
impl AccordionController {
    pub fn new(cx: &mut App) -> Self {
        Self {
            snapshot: AccordionSnapshot::new([], []),
            handles: HashMap::new(),
            boundary: cx.focus_handle().tab_stop(false),
            on_change: None,
            physical_activation: None,
        }
    }
    pub fn sync(&mut self, mut snapshot: AccordionSnapshot, window: &mut Window, cx: &mut App) {
        let mut keys = HashSet::new();
        for item in &snapshot.items {
            assert!(
                !item.key.trim().is_empty() && keys.insert(item.key.clone()),
                "Collapse requires unique nonempty keys"
            );
            self.handles
                .entry(item.key.clone())
                .or_insert_with(|| Handles {
                    trigger: cx.focus_handle(),
                    panel: cx.focus_handle().tab_stop(false),
                });
        }
        snapshot.expanded = snapshot
            .items
            .iter()
            .filter(|i| snapshot.expanded.contains(&i.key))
            .map(|i| i.key.clone())
            .take(if snapshot.mode == CollapseMode::Accordion {
                1
            } else {
                usize::MAX
            })
            .collect();
        let enabled = |key: &SharedString| {
            !snapshot.disabled && snapshot.items.iter().any(|i| &i.key == key && !i.disabled)
        };
        // Inspect old handles before retiring removed panels. Only repair the
        // subtree that actually holds focus; another clicked header wins.
        let lost = self.snapshot.items.iter().find(|item| {
            self.handles.get(&item.key).is_some_and(|h| {
                (h.trigger.is_focused(window) && !enabled(&item.key))
                    || (h.panel.contains_focused(window, cx)
                        && (!snapshot.expanded.contains(&item.key) || !keys.contains(&item.key)))
            })
        });
        if let Some(item) = lost {
            let ix = self
                .snapshot
                .items
                .iter()
                .position(|i| i.key == item.key)
                .unwrap();
            let target = enabled(&item.key)
                .then_some(&item.key)
                .or_else(|| {
                    self.snapshot.items[ix + 1..]
                        .iter()
                        .chain(self.snapshot.items[..ix].iter().rev())
                        .find(|i| enabled(&i.key))
                        .map(|i| &i.key)
                })
                .or_else(|| {
                    snapshot
                        .items
                        .iter()
                        .find(|i| enabled(&i.key))
                        .map(|i| &i.key)
                });
            target
                .and_then(|key| self.handles.get(key).map(|h| &h.trigger))
                .unwrap_or(&self.boundary)
                .focus(window, cx);
        }
        self.snapshot = snapshot;
        self.handles.retain(|key, _| keys.contains(key));
        if self.physical_activation.as_ref().is_some_and(|key| {
            self.is_item_disabled(key)
                || !self.focus_handle(key).is_some_and(|h| h.is_focused(window))
        }) {
            self.physical_activation = None;
        }
    }
    pub fn on_change(
        &mut self,
        handler: impl Fn(&CollapseChangeRequest, &mut Window, &mut App) + 'static,
    ) {
        self.on_change = Some(Rc::new(handler));
    }
    pub fn clear_handler(&mut self) {
        self.on_change = None;
    }
    pub fn boundary_focus(&self) -> FocusHandle {
        self.boundary.clone()
    }
    pub fn focus_handle(&self, key: &SharedString) -> Option<FocusHandle> {
        self.handles.get(key).map(|h| h.trigger.clone())
    }
    pub fn panel_focus(&self, key: &SharedString) -> Option<FocusHandle> {
        self.handles.get(key).map(|h| h.panel.clone())
    }
    pub fn expanded_keys(&self) -> &[SharedString] {
        &self.snapshot.expanded
    }
    pub fn is_expanded(&self, key: &SharedString) -> bool {
        self.snapshot.expanded.contains(key)
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
    /// GPUI routes both physical key releases and native accessibility actions
    /// through the click handler. A preceding focused keydown distinguishes the
    /// physical path without duplicating Button's Enter/Space implementation.
    pub fn click(
        entity: &Entity<Self>,
        key: &SharedString,
        event: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        let physical = entity.update(cx, |s, _| {
            s.physical_activation.take().as_ref() == Some(key)
        });
        let source = match event {
            gpui::ClickEvent::Mouse(_) | gpui::ClickEvent::Touch(_) => {
                CollapseChangeSource::Pointer
            }
            gpui::ClickEvent::Keyboard(_) if physical => CollapseChangeSource::Keyboard,
            _ => CollapseChangeSource::Accessibility,
        };
        Self::activate(entity, key, source, window, cx);
    }
    pub fn activate(
        entity: &Entity<Self>,
        key: &SharedString,
        source: CollapseChangeSource,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = entity.read(cx);
        if state.is_item_disabled(key) {
            return;
        }
        let expanded = !state.is_expanded(key);
        let expanded_keys = state
            .snapshot
            .items
            .iter()
            .filter(|i| {
                if &i.key == key {
                    expanded
                } else {
                    state.snapshot.mode == CollapseMode::Multiple && state.is_expanded(&i.key)
                }
            })
            .map(|i| i.key.clone())
            .collect();
        let request = CollapseChangeRequest {
            key: key.clone(),
            expanded,
            expanded_keys,
            source,
        };
        let handler = state.on_change.clone();
        let focus = state.focus_handle(key);
        if let Some(focus) = focus {
            focus.focus(window, cx);
        }
        // Do not hold an Entity borrow while the owner synchronously commits.
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
        if command == "enter" || command == "space" {
            entity.update(cx, |s, _| {
                s.physical_activation = (!event.keystroke.modifiers.modified()).then(|| key.clone())
            });
            // Let Button synthesize its single native click on key release.
            // Stop ancestor shortcuts without preventing that native default.
            cx.stop_propagation();
        } else if matches!(command, "down" | "up" | "home" | "end") {
            let keys: Vec<_> = state
                .snapshot
                .items
                .iter()
                .filter(|i| !i.disabled)
                .map(|i| &i.key)
                .collect();
            let ix = keys.iter().position(|k| *k == key).unwrap_or(0);
            let next = match command {
                "home" => 0,
                "end" => keys.len() - 1,
                "down" => (ix + 1) % keys.len(),
                _ => (ix + keys.len() - 1) % keys.len(),
            };
            if let Some(focus) = state.focus_handle(keys[next]) {
                focus.focus(window, cx);
            }
            entity.update(cx, |s, _| s.physical_activation = None);
            cx.stop_propagation();
        } else {
            entity.update(cx, |s, _| s.physical_activation = None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _};
    use std::cell::RefCell;

    struct Probe {
        state: Entity<AccordionController>,
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            crate::AccordionTrigger::new("native-group-trigger")
                .in_group(&self.state, "a")
                .size(gpui::px(40.))
                .child("A")
        }
    }

    #[gpui::test]
    fn native_accessibility_click_requests_once_and_handler_can_commit_reentrantly(
        cx: &mut gpui::TestAppContext,
    ) {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let state = cx.new(|cx| AccordionController::new(cx));
        let (_, cx) = cx.add_window_view({
            let state = state.clone();
            move |_, _| Probe { state }
        });
        cx.update(|w, cx| {
            state.update(cx, |s, cx| {
                s.sync(
                    AccordionSnapshot::new(
                        [AccordionDescriptor::new("a"), AccordionDescriptor::new("b")],
                        [],
                    ),
                    w,
                    cx,
                )
            });
            let commit = state.clone();
            state.update(cx, |s, _| {
                s.on_change({
                    let requests = requests.clone();
                    move |request, w, cx| {
                        requests.borrow_mut().push(request.clone());
                        commit.update(cx, |s, cx| {
                            s.sync(
                                AccordionSnapshot::new(
                                    [AccordionDescriptor::new("a"), AccordionDescriptor::new("b")],
                                    request.expanded_keys().to_vec(),
                                ),
                                w,
                                cx,
                            )
                        });
                    }
                })
            });
            w.draw(cx).clear(cx);
            AccordionController::click(&state, &"a".into(), &gpui::ClickEvent::default(), w, cx);
            assert!(state.read(cx).is_expanded(&"a".into()));
            AccordionController::activate(
                &state,
                &"b".into(),
                CollapseChangeSource::Accessibility,
                w,
                cx,
            );
            assert_eq!(
                state.read(cx).expanded_keys(),
                &[SharedString::from("a"), SharedString::from("b")]
            );
        });
        assert_eq!(requests.borrow().len(), 2);
        assert!(
            requests
                .borrow()
                .iter()
                .all(|r| r.source() == CollapseChangeSource::Accessibility)
        );
    }
}
