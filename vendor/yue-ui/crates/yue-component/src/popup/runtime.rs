use super::{anchor::AnchorSnapshot, arrow::ArrowGeometry};
use gpui_kit::base::{
    ModalScope, POPUP_PRIORITY, PresencePhase, ResolvedPosition, Root, RootPlugin,
};
use gpui_kit::{
    AnyElement, App, Bounds, Context, DispatchPhase, ElementId, EntityId, FocusHandle, Global,
    IntoElement, KeyDownEvent, MouseDownEvent, ParentElement as _, Pixels, Point, Render,
    Styled as _, Subscription, Window, WindowId, canvas, div,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PopupId {
    owner: EntityId,
    local: ElementId,
}
impl PopupId {
    pub(crate) fn new(owner: EntityId, local: impl Into<ElementId>) -> Self {
        Self {
            owner,
            local: local.into(),
        }
    }
    pub(super) fn local(&self) -> ElementId {
        self.local.clone()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PopupDismissRoute {
    #[default]
    None,
    Owner,
    // The second consumer (Tooltip) uses the same infrastructure without focus.
    #[allow(dead_code)]
    Runtime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopupDismissReason {
    Escape,
    OutsidePress,
    AnchorInvalid,
    ParentClosed,
    WindowInactive,
    ScopeInvalid,
    Unplaceable,
}

#[derive(Clone)]
pub(crate) enum PopupDismissSource {
    Key(KeyDownEvent),
    Pointer(MouseDownEvent),
    Lifecycle,
}

#[derive(Clone)]
pub(crate) struct PopupDismissRequest {
    id: PopupId,
    reason: PopupDismissReason,
    source: PopupDismissSource,
}
impl PopupDismissRequest {
    #[allow(dead_code)] // Internal event seam; the menu only needs the close request.
    pub(crate) fn id(&self) -> &PopupId {
        &self.id
    }
    #[allow(dead_code)]
    pub(crate) fn reason(&self) -> PopupDismissReason {
        self.reason
    }
    #[allow(dead_code)]
    pub(crate) fn source(&self) -> &PopupDismissSource {
        &self.source
    }
    #[allow(dead_code)]
    pub(crate) fn key_event(&self) -> Option<&KeyDownEvent> {
        if let PopupDismissSource::Key(event) = &self.source {
            Some(event)
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub(crate) fn pointer_event(&self) -> Option<&MouseDownEvent> {
        if let PopupDismissSource::Pointer(event) = &self.source {
            Some(event)
        } else {
            None
        }
    }
}
pub(super) type DismissHandler = Rc<dyn Fn(PopupDismissRequest, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopupValidity {
    Unmeasured,
    Ready,
    AnchorInvalid,
    ParentClosed,
    NoSpace,
    Protected,
    ArrowDoesNotFit,
    UnsafeMotion,
}

/// Published only after positioning. Geometry, arrow, registry and hit-testing
/// all consume this one sample; an invalid sample never leaves an old hitbox.
#[derive(Clone, Copy)]
pub(crate) struct PopupSnapshot {
    phase: PresencePhase,
    validity: PopupValidity,
    anchor: Option<AnchorSnapshot>,
    position: Option<ResolvedPosition>,
    visual: Option<Bounds<Pixels>>,
    arrow: Option<ArrowGeometry>,
    revision: u64,
}
impl Default for PopupSnapshot {
    fn default() -> Self {
        Self {
            phase: PresencePhase::Absent,
            validity: PopupValidity::Unmeasured,
            anchor: None,
            position: None,
            visual: None,
            arrow: None,
            revision: 0,
        }
    }
}
impl PopupSnapshot {
    pub(crate) fn position(&self) -> Option<ResolvedPosition> {
        (self.validity == PopupValidity::Ready)
            .then_some(self.position)
            .flatten()
    }
    #[allow(dead_code)] // Geometry observation seam for the next controller.
    pub(crate) fn anchor_bounds(&self) -> Option<Bounds<Pixels>> {
        (self.validity == PopupValidity::Ready)
            .then_some(self.anchor)
            .flatten()
            .map(|a| a.bounds)
    }
    #[allow(dead_code)]
    pub(crate) fn visual_bounds(&self) -> Option<Bounds<Pixels>> {
        (self.validity == PopupValidity::Ready)
            .then_some(self.visual)
            .flatten()
    }
    pub(super) fn last_position(&self) -> Option<ResolvedPosition> {
        self.position
    }
    pub(super) fn last_visual(&self) -> Option<Bounds<Pixels>> {
        self.visual
    }
    pub(crate) fn validity(&self) -> PopupValidity {
        self.validity
    }
    #[allow(dead_code)]
    pub(crate) fn phase(&self) -> PresencePhase {
        self.phase
    }
    #[allow(dead_code)]
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn arrow(&self) -> Option<ArrowGeometry> {
        (self.validity == PopupValidity::Ready)
            .then_some(self.arrow)
            .flatten()
    }
}

#[derive(Clone, Default)]
pub(crate) struct PopupSnapshotHandle(Rc<RefCell<PopupSnapshot>>);
impl PopupSnapshotHandle {
    pub(crate) fn read(&self) -> PopupSnapshot {
        *self.0.borrow()
    }
    pub(super) fn invalidate(&self, phase: PresencePhase, validity: PopupValidity) {
        let mut sample = self.0.borrow_mut();
        sample.phase = phase;
        sample.validity = validity;
        // Retain the last displayed geometry privately for the next path proof.
        // Public readers return no geometry while the sample is invalid.
    }
    pub(super) fn publish(
        &self,
        phase: PresencePhase,
        anchor: AnchorSnapshot,
        position: ResolvedPosition,
        visual: Bounds<Pixels>,
        arrow: Option<ArrowGeometry>,
    ) {
        let previous = self.read();
        let changed = previous.anchor != Some(anchor)
            || previous.position != Some(position)
            || previous.visual != Some(visual)
            || previous.arrow != arrow;
        *self.0.borrow_mut() = PopupSnapshot {
            phase,
            validity: PopupValidity::Ready,
            anchor: Some(anchor),
            position: Some(position),
            visual: Some(visual),
            arrow,
            revision: previous.revision + u64::from(changed),
        };
    }
}

struct Entry {
    lease: Weak<Registration>,
    generation: u64,
    parent: Option<PopupId>,
    order: u64,
    present: bool,
    route: PopupDismissRoute,
    escape: bool,
    outside: bool,
    exclude: Vec<super::PopupAnchorHandle>,
    scope: Option<FocusHandle>,
    modal: Option<ModalScope>,
    handler: Option<DismissHandler>,
}
#[derive(Default)]
struct RuntimeState {
    entries: HashMap<PopupId, Entry>,
    serial: u64,
    press_routed: bool,
}

#[derive(Clone, Default)]
pub(crate) struct PopupRuntime(Rc<RefCell<RuntimeState>>);
#[derive(Default)]
struct WindowRuntimes(HashMap<WindowId, Weak<RefCell<RuntimeState>>>);
impl Global for WindowRuntimes {}

/// A registration lives with the actual host element, including cached trees.
/// Its generation prevents the old tree's drop from removing its replacement.
pub(super) struct Registration {
    runtime: Weak<RefCell<RuntimeState>>,
    id: PopupId,
    generation: u64,
    snapshot: PopupSnapshotHandle,
    invalidation: Cell<Option<PopupDismissReason>>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.upgrade() {
            let mut runtime = runtime.borrow_mut();
            if runtime
                .entries
                .get(&self.id)
                .is_some_and(|entry| entry.generation == self.generation)
            {
                runtime.entries.remove(&self.id);
            }
        }
    }
}

pub(super) struct RegistrationSpec {
    pub(super) id: PopupId,
    pub(super) parent: Option<PopupId>,
    pub(super) present: bool,
    pub(super) route: PopupDismissRoute,
    pub(super) escape: bool,
    pub(super) outside: bool,
    pub(super) exclude: Vec<super::PopupAnchorHandle>,
    pub(super) scope: Option<FocusHandle>,
    pub(super) modal: Option<ModalScope>,
    pub(super) handler: Option<DismissHandler>,
}

impl PopupRuntime {
    pub(crate) fn for_window(window: &Window, cx: &mut App) -> Self {
        if cx.try_global::<WindowRuntimes>().is_none() {
            cx.set_global(WindowRuntimes::default());
        }
        let windows = &mut cx.global_mut::<WindowRuntimes>().0;
        windows.retain(|_, state| state.strong_count() > 0);
        if let Some(state) = windows
            .get(&window.window_handle().window_id())
            .and_then(Weak::upgrade)
        {
            return Self(state);
        }
        let runtime = Self::default();
        windows.insert(
            window.window_handle().window_id(),
            Rc::downgrade(&runtime.0),
        );
        runtime
    }

    pub(super) fn register(&self, spec: RegistrationSpec) -> Rc<Registration> {
        let mut state = self.0.borrow_mut();
        let previous = state.entries.get(&spec.id).and_then(|e| e.lease.upgrade());
        let snapshot = previous
            .as_ref()
            .map_or_else(PopupSnapshotHandle::default, |r| r.snapshot.clone());
        let invalidation = spec
            .present
            .then(|| previous.as_ref().and_then(|r| r.invalidation.get()))
            .flatten();
        let parent_valid = spec.parent.as_ref().is_none_or(|parent| {
            parent != &spec.id
                && state.entries.contains_key(parent)
                && !descends_from(&state, parent, &spec.id)
        });
        if !parent_valid {
            snapshot.invalidate(PresencePhase::Absent, PopupValidity::ParentClosed);
        }
        if invalidation.is_some() {
            snapshot.invalidate(PresencePhase::Absent, PopupValidity::AnchorInvalid);
        }
        state.serial += 1;
        let order = state
            .entries
            .get(&spec.id)
            .map_or(state.serial, |e| e.order);
        let registration = Rc::new(Registration {
            runtime: Rc::downgrade(&self.0),
            id: spec.id.clone(),
            generation: state.serial,
            snapshot,
            invalidation: Cell::new(invalidation),
        });
        state.entries.insert(
            spec.id,
            Entry {
                lease: Rc::downgrade(&registration),
                generation: registration.generation,
                parent: spec.parent,
                order,
                present: spec.present && parent_valid && invalidation.is_none(),
                route: spec.route,
                escape: spec.escape,
                outside: spec.outside,
                exclude: spec.exclude,
                scope: spec.scope,
                modal: spec.modal,
                handler: spec.handler,
            },
        );
        drop(state);
        registration
    }

    pub(crate) fn contains_or_excluded(&self, id: &PopupId, point: Point<Pixels>) -> bool {
        let state = self.0.borrow();
        state.entries.iter().any(|(candidate, entry)| {
            (candidate == id || descends_from(&state, candidate, id))
                && effective(&state, candidate)
                && (entry
                    .lease
                    .upgrade()
                    .and_then(|r| r.snapshot.read().position())
                    .is_some_and(|p| p.bounds.contains(&point))
                    || entry.exclude.iter().any(|anchor| {
                        anchor
                            .bounds()
                            .is_some_and(|bounds| bounds.contains(&point))
                    }))
        })
    }

    pub(crate) fn owner_press_suppressed(&self) -> bool {
        self.0.borrow().press_routed
    }
    /// Transient surfaces arbitrate focus before lower presentation layers.
    pub(crate) fn has_present_surface(&self) -> bool {
        let state = self.0.borrow();
        state.entries.keys().any(|id| effective(&state, id))
    }
    pub(crate) fn mark_owner_press_routed(&self) {
        self.0.borrow_mut().press_routed = true;
    }
    pub(crate) fn highest_priority(&self) -> usize {
        let state = self.0.borrow();
        state
            .entries
            .iter()
            .map(|(id, e)| {
                e.modal
                    .as_ref()
                    .map_or(POPUP_PRIORITY, |s| s.priority() + 1)
                    + depth(&state, id)
            })
            .max()
            .unwrap_or(POPUP_PRIORITY)
    }
    /// A modal transition ends the previous transient interaction session.
    /// Release registry borrows before invoking potentially reentrant owners.
    pub(crate) fn invalidate_modal(&self, window: &mut Window, cx: &mut App) {
        let leases: Vec<_> = self
            .0
            .borrow()
            .entries
            .iter()
            .filter_map(|(id, e)| e.lease.upgrade().map(|lease| (id.clone(), lease)))
            .collect();
        for (id, lease) in leases {
            self.deactivate(&id);
            lease.invalidate(PopupDismissReason::ScopeInvalid, window, cx);
        }
    }
    /// The deepest registered surface under a trigger, for a semantic child
    /// whose host remains in the window's presentation plugin.
    pub(crate) fn parent_at(&self, point: Point<Pixels>) -> Option<PopupId> {
        let state = self.0.borrow();
        state
            .entries
            .iter()
            .filter(|(id, entry)| {
                effective(&state, id)
                    && entry
                        .lease
                        .upgrade()
                        .and_then(|r| r.snapshot.read().position())
                        .is_some_and(|p| p.bounds.contains(&point))
            })
            .max_by_key(|(id, entry)| (depth(&state, id), entry.order))
            .map(|(id, _)| id.clone())
    }

    pub(super) fn deactivate(&self, id: &PopupId) {
        let mut state = self.0.borrow_mut();
        for (candidate, entry) in &mut state.entries {
            if candidate == id {
                entry.present = false;
            }
        }
    }

    fn top(&self) -> Option<(PopupId, PopupDismissRoute)> {
        let state = self.0.borrow();
        state
            .entries
            .iter()
            .filter(|(id, _)| effective(&state, id))
            .max_by_key(|(id, entry)| (depth(&state, id), entry.order))
            .map(|(id, entry)| (id.clone(), entry.route))
    }

    fn request(
        &self,
        id: &PopupId,
        reason: PopupDismissReason,
        source: PopupDismissSource,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let handler = self
            .0
            .borrow()
            .entries
            .get(id)
            .and_then(|entry| entry.handler.clone());
        if let Some(handler) = handler {
            handler(
                PopupDismissRequest {
                    id: id.clone(),
                    reason,
                    source,
                },
                window,
                cx,
            );
            true
        } else {
            false
        }
    }

    fn route_key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
        if event.keystroke.key != "escape" {
            return;
        }
        if let Some((id, PopupDismissRoute::Runtime)) = self.top() {
            let enabled = self.0.borrow().entries.get(&id).is_some_and(|e| e.escape);
            if enabled
                && self.request(
                    &id,
                    PopupDismissReason::Escape,
                    PopupDismissSource::Key(event.clone()),
                    window,
                    cx,
                )
            {
                cx.stop_propagation();
            }
        }
    }

    fn route_press(&self, event: &MouseDownEvent, window: &mut Window, cx: &mut App) {
        self.0.borrow_mut().press_routed = false;
        if let Some((id, PopupDismissRoute::Runtime)) = self.top() {
            let enabled = self.0.borrow().entries.get(&id).is_some_and(|e| e.outside);
            if enabled && !self.contains_or_excluded(&id, event.position) {
                self.0.borrow_mut().press_routed = self.request(
                    &id,
                    PopupDismissReason::OutsidePress,
                    PopupDismissSource::Pointer(event.clone()),
                    window,
                    cx,
                );
            }
        }
    }

    fn invalidate_scopes(&self, window: &mut Window, cx: &mut App) {
        let trap = gpui_kit::base::active_focus_trap(window, cx);
        let invalid: Vec<_> = self
            .0
            .borrow()
            .entries
            .iter()
            .filter_map(|(id, entry)| {
                let reason = if !window.is_window_active() {
                    Some(PopupDismissReason::WindowInactive)
                } else if let (Some(trap), Some(scope)) = (&trap, &entry.scope) {
                    (!trap.contains(scope, window)).then_some(PopupDismissReason::ScopeInvalid)
                } else {
                    None
                };
                reason.and_then(|reason| {
                    entry
                        .lease
                        .upgrade()
                        .map(|lease| (id.clone(), lease, reason))
                })
            })
            .collect();
        for (id, lease, reason) in invalid {
            self.deactivate(&id);
            lease.invalidate(reason, window, cx);
        }
    }
}

fn effective(state: &RuntimeState, id: &PopupId) -> bool {
    let mut current = Some(id);
    let mut remaining = state.entries.len() + 1;
    while let Some(id) = current {
        if remaining == 0 {
            return false;
        }
        remaining -= 1;
        let Some(entry) = state.entries.get(id) else {
            return false;
        };
        let Some(lease) = entry.lease.upgrade() else {
            return false;
        };
        if !entry.present || lease.snapshot.read().validity() != PopupValidity::Ready {
            return false;
        }
        current = entry.parent.as_ref();
    }
    true
}
fn descends_from(state: &RuntimeState, child: &PopupId, parent: &PopupId) -> bool {
    let mut current = state.entries.get(child).and_then(|e| e.parent.as_ref());
    for _ in 0..state.entries.len() {
        let Some(id) = current else {
            break;
        };
        if id == parent {
            return true;
        }
        current = state.entries.get(id).and_then(|e| e.parent.as_ref());
    }
    false
}
fn depth(state: &RuntimeState, id: &PopupId) -> usize {
    let mut count = 0;
    let mut current = state.entries.get(id).and_then(|e| e.parent.as_ref());
    while let Some(id) = current {
        if count >= state.entries.len() {
            break;
        }
        count += 1;
        current = state.entries.get(id).and_then(|e| e.parent.as_ref());
    }
    count
}

impl Registration {
    pub(super) fn snapshot(&self) -> PopupSnapshotHandle {
        self.snapshot.clone()
    }
    pub(super) fn is_parent_present(&self) -> bool {
        let Some(state) = self.runtime.upgrade() else {
            return false;
        };
        let state = state.borrow();
        state.entries.get(&self.id).is_some_and(|e| {
            e.parent
                .as_ref()
                .is_none_or(|parent| effective(&state, parent))
        })
    }
    pub(super) fn priority(&self) -> usize {
        self.runtime.upgrade().map_or(POPUP_PRIORITY, |state| {
            let state = state.borrow();
            state
                .entries
                .get(&self.id)
                .and_then(|e| e.modal.as_ref())
                .map_or(POPUP_PRIORITY, |s| s.priority() + 1)
                + depth(&state, &self.id)
        })
    }
    pub(super) fn validate(&self) {
        self.invalidation.set(None);
    }
    pub(super) fn is_invalidated(&self) -> bool {
        self.invalidation.get().is_some()
    }
    pub(super) fn invalidate(&self, reason: PopupDismissReason, window: &mut Window, cx: &mut App) {
        if self.invalidation.replace(Some(reason)) == Some(reason) {
            return;
        }
        if let Some(state) = self.runtime.upgrade() {
            PopupRuntime(state).request(
                &self.id,
                reason,
                PopupDismissSource::Lifecycle,
                window,
                cx,
            );
        }
    }
    #[cfg(all(test, feature = "headless-tests"))]
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
}

struct PopupPlugin {
    runtime: PopupRuntime,
    _activation: Subscription,
    _keys: Subscription,
}
impl Render for PopupPlugin {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
impl RootPlugin for PopupPlugin {
    fn decorate(
        &self,
        surface: AnyElement,
        _: &Root,
        _: &mut Window,
        _: &mut App,
    ) -> impl IntoElement {
        let mouse = self.runtime.clone();
        // Window listener observes outside presses without adding a hitbox that
        // could unhover a trigger under a pass-through surface.
        let listener = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let runtime = mouse.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        runtime.route_press(event, window, cx);
                    }
                });
            },
        )
        .absolute()
        .inset_0();
        div().size_full().child(listener).child(surface)
    }
}
pub(super) fn init(cx: &mut App) {
    Root::register_plugin(cx, |window, cx| {
        let runtime = PopupRuntime::for_window(window, cx);
        let activation = cx
            .observe_window_activation(window, |this: &mut PopupPlugin, window, cx| {
                this.runtime.invalidate_scopes(window, cx)
            });
        // A tooltip never takes focus. A window with no focused control has no
        // focused dispatch path, so Escape must enter the same runtime router
        // before dispatch. Filter by this window; owner-routed menus retain
        // their own keyboard behavior.
        let keys = runtime.clone();
        let window_id = window.window_handle().window_id();
        let key_subscription = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle().window_id() == window_id {
                keys.route_key(
                    &KeyDownEvent {
                        keystroke: event.keystroke.clone(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
            }
        });
        PopupPlugin {
            runtime,
            _activation: activation,
            _keys: key_subscription,
        }
    });
}
