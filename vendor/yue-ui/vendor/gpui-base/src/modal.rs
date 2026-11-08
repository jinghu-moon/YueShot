//! Window-scoped modal identity and native semantic boundaries. No visual policy.
use gpui::{
    App, BorrowAppContext as _, Bounds, Element, ElementId, EntityId, FocusHandle, Global,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Role, Window, WindowId,
    accesskit,
};
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

#[derive(Clone)]
pub struct ModalScope {
    owner: EntityId,
    focus: FocusHandle,
    priority: usize,
    interactive: bool,
}
impl ModalScope {
    pub fn new(owner: EntityId, focus: FocusHandle, priority: usize) -> Self {
        Self {
            owner,
            focus,
            priority,
            interactive: true,
        }
    }
    pub fn owner(&self) -> EntityId {
        self.owner
    }
    pub fn focus(&self) -> &FocusHandle {
        &self.focus
    }
    pub fn priority(&self) -> usize {
        self.priority
    }
    pub fn is_interactive(&self) -> bool {
        self.interactive
    }
    pub fn interactive(mut self, value: bool) -> Self {
        self.interactive = value;
        self
    }
}
#[derive(Default)]
struct Scopes {
    active: HashMap<WindowId, ModalScope>,
    building: HashMap<WindowId, ModalScope>,
    editors: HashMap<WindowId, HashMap<EntityId, Vec<Editor>>>,
}
struct Editor {
    focus: gpui::WeakFocusHandle,
    enabled: Rc<dyn Fn(&App) -> bool>,
}
impl Global for Scopes {}

/// Observable, window-indexed modal changes. Values are private; consumers read the timestamp.
#[derive(Default)]
pub struct ModalScopeChanges {
    changed_at: HashMap<WindowId, Instant>,
}
impl Global for ModalScopeChanges {}
impl ModalScopeChanges {
    pub fn changed_at(&self, window: &Window) -> Option<Instant> {
        self.changed_at
            .get(&window.window_handle().window_id())
            .copied()
    }
}

pub(crate) fn register_modal_editor(
    focus: &FocusHandle,
    enabled: Rc<dyn Fn(&App) -> bool>,
    w: &Window,
    cx: &mut App,
) {
    let Some(scope) = building_modal_scope(w, cx) else {
        return;
    };
    let entries = cx
        .global_mut::<Scopes>()
        .editors
        .entry(w.window_handle().window_id())
        .or_default()
        .entry(scope.owner())
        .or_default();
    entries.retain(|e| e.focus.upgrade().is_some());
    if let Some(entry) = entries.iter_mut().find(|e| &e.focus == focus) {
        entry.enabled = enabled;
    } else {
        entries.push(Editor {
            focus: focus.downgrade(),
            enabled,
        });
    }
}
/// First live editable control declared by this modal, in declaration order.
pub fn preferred_modal_focus(scope: &ModalScope, w: &Window, cx: &App) -> Option<FocusHandle> {
    cx.try_global::<Scopes>()?
        .editors
        .get(&w.window_handle().window_id())?
        .get(&scope.owner())?
        .iter()
        .filter(|e| (e.enabled)(cx))
        .filter_map(|e| e.focus.upgrade())
        .find(|f| scope.focus().contains(f, w) && crate::Root::is_focus_available(f, w))
}
/// Release window-only weak registrations when its root is destroyed.
pub fn release_modal_window(w: &Window, cx: &mut App) {
    if cx.try_global::<Scopes>().is_some() {
        let scopes = cx.global_mut::<Scopes>();
        let id = w.window_handle().window_id();
        scopes.active.remove(&id);
        scopes.building.remove(&id);
        scopes.editors.remove(&id);
        if cx.try_global::<ModalScopeChanges>().is_some() {
            cx.global_mut::<ModalScopeChanges>().changed_at.remove(&id);
        }
    }
}

pub fn active_modal_scope(window: &Window, cx: &App) -> Option<ModalScope> {
    cx.try_global::<Scopes>()
        .and_then(|s| s.active.get(&window.window_handle().window_id()).cloned())
}
/// The actual declaring surface, independent of pointer coordinates or current focus.
pub fn building_modal_scope(window: &Window, cx: &App) -> Option<ModalScope> {
    cx.try_global::<Scopes>()
        .and_then(|s| s.building.get(&window.window_handle().window_id()).cloned())
}
/// Called by the window's retained modal coordinator, including throughout exit.
pub fn set_active_modal_scope(scope: Option<ModalScope>, window: &Window, cx: &mut App) {
    if cx.try_global::<Scopes>().is_none() {
        cx.set_global(Scopes::default());
    }
    let id = window.window_handle().window_id();
    let previous = cx.global::<Scopes>().active.get(&id);
    let changed = previous.map(|s| (s.owner(), s.is_interactive(), s.priority()))
        != scope
            .as_ref()
            .map(|s| (s.owner(), s.is_interactive(), s.priority()));
    if cx.try_global::<ModalScopeChanges>().is_none() {
        cx.set_global(ModalScopeChanges::default());
    }
    if changed {
        let now = cx.background_executor().now();
        cx.update_global::<ModalScopeChanges, _>(|changes, _| {
            changes.changed_at.insert(id, now);
        });
    }
    let scopes = &mut cx.global_mut::<Scopes>().active;
    if let Some(scope) = scope {
        scopes.insert(id, scope);
    } else {
        scopes.remove(&id);
    }
}

/// Adds no layout, focus or input node. It forwards native semantics and stamps
/// the declaring scope while descendants build, including deferred descendants.
pub struct ModalBoundary<E> {
    inner: E,
    scope: Option<ModalScope>,
    background: bool,
    hidden: bool,
    busy: bool,
}
impl<E: Element> ModalBoundary<E> {
    pub fn surface(inner: E, scope: ModalScope) -> Self {
        Self {
            inner,
            scope: Some(scope),
            background: false,
            hidden: false,
            busy: false,
        }
    }
    pub fn background(inner: E) -> Self {
        Self {
            inner,
            scope: None,
            background: true,
            hidden: false,
            busy: false,
        }
    }
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    fn enter(&mut self, window: &Window, cx: &mut App) -> Option<ModalScope> {
        let active = active_modal_scope(window, cx);
        self.hidden = if self.background {
            active.is_some()
        } else {
            active
                .as_ref()
                .is_some_and(|a| self.scope.as_ref().is_some_and(|s| s.owner != a.owner))
        };
        if cx.try_global::<Scopes>().is_none() {
            cx.set_global(Scopes::default());
        }
        let scopes = &mut cx.global_mut::<Scopes>().building;
        let id = window.window_handle().window_id();
        if let Some(scope) = &self.scope {
            scopes.insert(id, scope.clone())
        } else {
            scopes.remove(&id)
        }
    }
    fn leave(previous: Option<ModalScope>, window: &Window, cx: &mut App) {
        let scopes = &mut cx.global_mut::<Scopes>().building;
        let id = window.window_handle().window_id();
        if let Some(scope) = previous {
            scopes.insert(id, scope);
        } else {
            scopes.remove(&id);
        }
    }
}
impl<E: Element> IntoElement for ModalBoundary<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl<E: Element> Element for ModalBoundary<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let previous = self.enter(w, cx);
        let result = self.inner.request_layout(id, inspector, w, cx);
        Self::leave(previous, w, cx);
        result
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        w: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let previous = self.enter(w, cx);
        let result = self.inner.prepaint(id, inspector, bounds, layout, w, cx);
        Self::leave(previous, w, cx);
        result
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, w, cx)
    }
    fn a11y_role(&self) -> Option<Role> {
        self.inner
            .a11y_role()
            .or(self.background.then_some(Role::GenericContainer))
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        if self.hidden {
            node.set_hidden();
        }
        if !self.background && !self.hidden {
            node.set_modal();
        }
        if self.busy {
            node.set_busy();
        }
    }
    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, InteractiveElement as _, Render, StatefulInteractiveElement as _, div};
    struct Probe;
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    #[gpui::test]
    fn native_boundaries_export_modal_busy_and_background_hidden(cx: &mut gpui::TestAppContext) {
        let (owner, cx) = cx.add_window_view(|_, _| Probe);
        cx.update(|w, cx| {
            let scope = ModalScope::new(owner.entity_id(), cx.focus_handle(), 1000);
            set_active_modal_scope(Some(scope.clone()), w, cx);
            let mut background = ModalBoundary::background(div().id("background"));
            let previous = background.enter(w, cx);
            let mut node = accesskit::Node::new(Role::GenericContainer);
            background.write_a11y_info(&mut node);
            assert!(node.is_hidden());
            ModalBoundary::<gpui::Div>::leave(previous, w, cx);
            let mut surface = ModalBoundary::surface(
                div()
                    .id("surface")
                    .role(Role::Dialog)
                    .aria_label("Edit project"),
                scope.clone(),
            )
            .busy(true);
            let previous = surface.enter(w, cx);
            assert_eq!(building_modal_scope(w, cx).unwrap().owner(), scope.owner());
            let mut node = accesskit::Node::new(surface.a11y_role().unwrap());
            surface.write_a11y_info(&mut node);
            assert_eq!(node.label(), Some("Edit project"));
            assert!(node.is_modal() && node.is_busy() && !node.is_hidden());
            ModalBoundary::<gpui::Div>::leave(previous, w, cx);
            assert!(building_modal_scope(w, cx).is_none());
            release_modal_window(w, cx);
            assert!(active_modal_scope(w, cx).is_none());
        });
    }
}
