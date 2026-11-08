use super::{
    group::{self, Group, ProtectedState},
    surface, *,
};
use crate::{
    popup::{
        PopupAnchor, PopupAnchorHandle, PopupCandidate, PopupDismissRoute, PopupHost, PopupId,
        PopupPlacement, PopupSnapshotHandle, PopupSpec, union,
    },
    theme::keys,
    tokens::{duration, easing},
};
use gpui_kit::base::{Align, Placement, Root, RootPlugin, TooltipDefaults, Transition};
use gpui_kit::{
    Axis, Bounds, Context, DispatchPhase, Entity, EntityId, HitboxBehavior,
    InteractiveElement as _, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, Subscription, Task, WeakEntity, canvas, px,
};
use std::collections::HashMap;

pub(super) fn layer(window: &Window, cx: &App) -> Option<Entity<Layer>> {
    window.root::<Root>().flatten()?.read(cx).plugin::<Layer>()
}
#[cfg(all(test, feature = "headless-tests"))]
pub(super) fn snapshot(window: &Window, cx: &App) -> Option<crate::popup::PopupSnapshot> {
    layer(window, cx)?
        .read(cx)
        .current
        .as_ref()?
        .snapshot
        .as_ref()
        .map(|s| s.read())
}
pub(super) fn init(cx: &mut App) {
    Root::register_plugin(cx, |window, cx| {
        let activation = cx.observe_window_activation(window, |layer: &mut Layer, window, cx| {
            if !window.is_window_active() {
                layer.cancel_all(window, cx);
            }
        });
        Layer {
            entries: HashMap::new(),
            protected: HashMap::new(),
            current: None,
            pending: None,
            show_task: None,
            grace_task: None,
            auto_task: None,
            switch_task: None,
            epoch: 0,
            serial: 0,
            surface_hovered: false,
            runtime: crate::popup::PopupRuntime::for_window(window, cx),
            _activation: activation,
        }
    });
}
pub(super) fn availability_changed(signal: &Rc<Cell<bool>>, window: &mut Window, cx: &mut App) {
    let Some(layer) = layer(window, cx) else {
        return;
    };
    let owners = layer
        .read(cx)
        .entries
        .values()
        .filter_map(WeakEntity::upgrade)
        .filter(|s| {
            s.read(cx)
                .config
                .availability
                .as_ref()
                .is_some_and(|v| Rc::ptr_eq(v, signal))
        })
        .collect::<Vec<_>>();
    for owner in owners {
        if signal.get() {
            let (wants, focused) = owner.update(cx, |s, _| {
                s.suppressed = false;
                (s.wants(), s.focused)
            });
            if wants {
                layer.update(cx, |layer, cx| {
                    layer.enter(owner.downgrade(), focused, window, cx)
                });
            }
        } else {
            layer.update(cx, |layer, cx| {
                layer.close_owner(owner.entity_id(), true, window, cx)
            });
        }
    }
}
pub(super) struct TriggerState {
    config: Config,
    anchor: PopupAnchor,
    group: Option<Group>,
    focus: Option<FocusHandle>,
    modal: Option<gpui_kit::base::ModalScope>,
    hovered: bool,
    focused: bool,
    pressed: bool,
    internal_open: bool,
    suppressed: bool,
    requested: Option<bool>,
    layer: WeakEntity<Layer>,
    _subscriptions: Vec<Subscription>,
}
impl TriggerState {
    fn new(
        config: Config,
        focus: Option<FocusHandle>,
        layer: &Entity<Layer>,
        group: Option<Group>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(handle) = &focus {
            subscriptions.push(cx.on_focus(handle, window, |state, window, cx| {
                state.source(false, true, window, cx)
            }));
            subscriptions.push(cx.on_blur(handle, window, |state, window, cx| {
                state.source(false, false, window, cx)
            }));
        }
        let weak = layer.downgrade();
        let on_release = weak.clone();
        let id = cx.entity_id();
        let handle = window.window_handle();
        cx.on_release(move |_, cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = on_release.update(cx, |layer, cx| layer.remove(id, window, cx));
            });
        })
        .detach();
        let requested = config.controlled;
        Self {
            config,
            anchor: PopupAnchor::default(),
            group,
            focus,
            modal: gpui_kit::base::building_modal_scope(window, cx),
            hovered: false,
            focused: false,
            pressed: false,
            internal_open: false,
            suppressed: false,
            requested,
            layer: weak,
            _subscriptions: subscriptions,
        }
    }
    fn effective_open(&self) -> bool {
        self.config.controlled.unwrap_or(self.internal_open) && !self.suppressed
    }
    fn send(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        action: impl FnOnce(&mut Layer, &mut Window, &mut Context<Layer>) + 'static,
    ) {
        let layer = self.layer.clone();
        let handle = window.window_handle();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = layer.update(cx, |layer, cx| action(layer, window, cx));
            });
        });
    }
    fn eligible(&self, window: &Window, cx: &App) -> bool {
        let active = gpui_kit::base::active_modal_scope(window, cx);
        let modal_matches = active.as_ref().map(|s| s.owner())
            == self.modal.as_ref().map(|s| s.owner())
            && active.as_ref().is_none_or(|s| s.is_interactive());
        self.config.available()
            && modal_matches
            && !self.suppressed
            && group::bounds(&self.anchor.handle()).is_some()
            && window.is_window_active()
            && (active.is_some()
                || gpui_kit::base::active_focus_trap(window, cx).is_none_or(|trap| {
                    self.focus
                        .as_ref()
                        .is_some_and(|f| trap.contains(f, window))
                }))
    }
    fn wants(&self) -> bool {
        match self.config.mode {
            TooltipTrigger::HoverFocus => self.hovered || self.focused,
            TooltipTrigger::Hover => self.hovered,
            TooltipTrigger::Focus => self.focused,
            _ => false,
        }
    }
    fn request(&mut self, value: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.config.controlled.is_none() && self.internal_open == value {
            self.requested = Some(value);
            return;
        }
        if self.requested == Some(value) {
            return;
        }
        self.requested = Some(value);
        if self.config.controlled.is_none() {
            self.internal_open = value;
        }
        if let Some(handler) = self.config.handler.clone() {
            let window = window.window_handle();
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| handler(&value, window, cx));
            });
        }
    }
    fn source(&mut self, pointer: bool, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.wants();
        if pointer {
            if self.hovered == active {
                return;
            }
            self.hovered = active;
        } else {
            self.focused = active;
        }
        if !self.hovered && !self.focused {
            self.suppressed = false;
            self.requested = None;
        }
        let wants = self.wants();
        if wants && !self.suppressed && (!previous || (!pointer && active)) {
            let immediate = !pointer && active;
            let owner = cx.entity().downgrade();
            self.send(window, cx, move |layer, window, cx| {
                layer.enter(owner, immediate, window, cx)
            });
        } else if !wants && previous {
            let id = cx.entity_id();
            self.send(window, cx, move |layer, window, cx| {
                layer.leave(id, window, cx)
            });
        }
    }
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.config.disabled {
            return;
        }
        match self.config.mode {
            TooltipTrigger::Click | TooltipTrigger::ContextMenu => {
                self.suppressed = false;
                self.requested = None;
                let owner = cx.entity().downgrade();
                let open = self.effective_open();
                self.send(window, cx, move |layer, window, cx| {
                    if open {
                        layer.close_owner(owner.entity_id(), false, window, cx);
                    } else {
                        layer.enter(owner, true, window, cx);
                    }
                });
            }
            TooltipTrigger::HoverFocus | TooltipTrigger::Hover | TooltipTrigger::Focus => {
                if !self.config.options.dismiss_on_press {
                    return;
                }
                self.request(false, window, cx);
                self.suppressed = true;
                let id = cx.entity_id();
                self.send(window, cx, move |layer, window, cx| {
                    layer.close_owner(id, true, window, cx)
                });
            }
            TooltipTrigger::Manual => {}
        }
    }
}

pub(crate) fn attach(
    id: ElementId,
    config: Config,
    focus: Option<FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Some(layer) = layer(window, cx) else {
        #[cfg(debug_assertions)]
        {
            #[derive(Default)]
            struct MissingRoot;
            impl gpui_kit::Global for MissingRoot {}
            if cx.try_global::<MissingRoot>().is_none() {
                eprintln!(
                    "YueTooltip requires yue_component::init before creating a Base Root; keeping the trigger without a tooltip"
                );
                cx.set_global(MissingRoot);
            }
        }
        return div().absolute().inset_0().into_any_element();
    };
    let group = group::current(window, cx);
    let initial = config.clone();
    let initial_focus = focus.clone();
    let owner_layer = layer.clone();
    let state = window.use_keyed_state((id, "tooltip-state"), cx, move |window, cx| {
        TriggerState::new(initial, initial_focus, &owner_layer, group, window, cx)
    });
    layer.update(cx, |layer, _| {
        layer.entries.insert(state.entity_id(), state.downgrade());
    });
    let first_or_changed = state.update(cx, |s, cx| {
        let changed = !s.config.same_content(&config) || s.group != group;
        let was_controlled = s.config.controlled;
        s.config = config.clone();
        s.modal = gpui_kit::base::building_modal_scope(window, cx);
        s.group = group;
        if was_controlled != config.controlled {
            s.requested = config.controlled;
            if config.controlled == Some(false) || was_controlled == Some(false) {
                s.suppressed = false;
            }
        }
        if s.focus != focus {
            s._subscriptions.clear();
            s.focus = focus.clone();
            if let Some(handle) = &focus {
                s._subscriptions
                    .push(cx.on_focus(handle, window, |s, w, cx| s.source(false, true, w, cx)));
                s._subscriptions
                    .push(cx.on_blur(handle, window, |s, w, cx| s.source(false, false, w, cx)));
            }
        }
        let focused = focus.as_ref().is_some_and(|f| f.is_focused(window));
        if s.focused != focused {
            s.source(false, focused, window, cx);
        }
        changed
    });
    // Initial controlled values also synchronize, but owner rendering never
    // sends an open-change notification back to itself.
    let owner = state.downgrade();
    let update_layer = layer.downgrade();
    let handle = window.window_handle();
    cx.defer(move |cx| {
        let Some(owner) = owner.upgrade() else {
            return;
        };
        let _ = handle.update(cx, |_, window, cx| {
            let _ = update_layer.update(cx, |layer, cx| {
                layer.sync(owner, first_or_changed, window, cx);
            });
        });
    });
    let capture = state.read(cx).anchor.capture();
    let pointer = state.clone();
    let down = state.clone();
    let up = state.clone();
    let watch = canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let target = pointer.clone();
            let hover_hitbox = hitbox.clone();
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    let active = hover_hitbox.is_hovered(window);
                    target.update(cx, |s, cx| s.source(true, active, window, cx));
                }
            });
            let target = down.clone();
            let down_hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !down_hitbox.is_hovered(window) {
                    return;
                }
                target.update(cx, |s, cx| {
                    if event.button == MouseButton::Right
                        && s.config.mode == TooltipTrigger::ContextMenu
                    {
                        s.activate(window, cx);
                        cx.stop_propagation();
                    } else if event.button == MouseButton::Left {
                        if s.config.mode == TooltipTrigger::Click {
                            s.pressed = true;
                        } else if s.config.mode != TooltipTrigger::ContextMenu {
                            s.activate(window, cx);
                        }
                    }
                });
            });
            let target = up.clone();
            let up_hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                    return;
                }
                target.update(cx, |s, cx| {
                    let pressed = std::mem::take(&mut s.pressed);
                    if pressed && up_hitbox.is_hovered(window) {
                        s.activate(window, cx);
                    }
                });
            });
        },
    )
    .absolute()
    .inset_0();
    div()
        .id("tooltip-observer")
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .inset(config.options.anchor_inset)
                .child(capture),
        )
        .child(watch)
        .into_any_element()
}

#[derive(Clone)]
struct Displayed {
    owner: WeakEntity<TriggerState>,
    config: Config,
    anchor: PopupAnchorHandle,
    group: Option<Group>,
    present: bool,
    moving: bool,
    generation: u64,
    measure: surface::Measure,
    snapshot: Option<PopupSnapshotHandle>,
    target_anchor: Option<Bounds<Pixels>>,
    old_text: Option<(Config, surface::Measure, u64)>,
    preferred_side: Option<Placement>,
    parent: Option<PopupId>,
    scope: Option<FocusHandle>,
    environment: (gpui_kit::Size<Pixels>, Pixels),
}
pub(super) struct Layer {
    entries: HashMap<EntityId, WeakEntity<TriggerState>>,
    pub(super) protected: HashMap<EntityId, WeakEntity<ProtectedState>>,
    current: Option<Displayed>,
    pending: Option<WeakEntity<TriggerState>>,
    show_task: Option<Task<()>>,
    grace_task: Option<Task<()>>,
    auto_task: Option<Task<()>>,
    switch_task: Option<Task<()>>,
    epoch: u64,
    serial: u64,
    surface_hovered: bool,
    runtime: crate::popup::PopupRuntime,
    _activation: Subscription,
}
impl Layer {
    fn sync(
        &mut self,
        owner: Entity<TriggerState>,
        changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let controlled = owner.read(cx).config.controlled;
        let id = owner.entity_id();
        let current = self
            .current
            .as_ref()
            .is_some_and(|d| d.owner.entity_id() == id);
        if current && changed {
            let config = owner.read(cx).config.clone();
            if let Some(display) = &mut self.current {
                display.config = config;
                display.moving = false;
                display.old_text = None;
            }
            cx.notify();
        }
        let invalid = !owner.read(cx).config.available();
        if invalid {
            let open = owner.read(cx).effective_open();
            self.close_owner(id, true, window, cx);
            if open {
                owner.update(cx, |s, cx| {
                    s.request(false, window, cx);
                    s.suppressed = true;
                });
            }
        } else if controlled == Some(true)
            && !owner.read(cx).suppressed
            && !self
                .current
                .as_ref()
                .is_some_and(|d| d.owner.entity_id() == id && d.present)
        {
            self.enter(owner.downgrade(), true, window, cx);
        } else if current && controlled == Some(false) {
            // External synchronization is silent, including while exiting.
            if let Some(display) = self.current.as_mut().filter(|d| d.present) {
                display.present = false;
                display.moving = false;
                self.auto_task = None;
                self.grace_task = None;
                cx.notify();
            }
        }
    }
    fn enter(
        &mut self,
        owner: WeakEntity<TriggerState>,
        immediate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entity) = owner.upgrade() else {
            return;
        };
        if !entity.read(cx).eligible(window, cx) {
            return;
        }
        let id = entity.entity_id();
        self.grace_task = None;
        if self
            .current
            .as_ref()
            .is_some_and(|d| d.owner.entity_id() == id && d.present)
        {
            return;
        }
        if self.pending.as_ref().is_some_and(|p| p.entity_id() == id) {
            return;
        }
        let switching = self.switch_task.is_some();
        let pending_owner = switching
            .then(|| self.pending.as_ref().and_then(WeakEntity::upgrade))
            .flatten();
        self.epoch += 1;
        self.show_task = None;
        self.switch_task = None;
        let hot = self.current.as_ref().is_some_and(|d| {
            let mut previous = d.clone();
            if let Some(pending) = &pending_owner {
                previous.anchor = pending.read(cx).anchor.handle();
                previous.group = pending.read(cx).group;
            }
            (d.present || switching) && self.near(&previous, entity.read(cx), window, cx)
        });
        if let Some(pending) = pending_owner {
            pending.update(cx, |s, cx| {
                s.request(false, window, cx);
                s.suppressed = s.hovered || s.focused || s.config.controlled == Some(true);
            });
        }
        if immediate || hot {
            self.show(owner, hot, window, cx);
            return;
        }
        if let Some(old) = self.current.as_ref().map(|d| d.owner.entity_id()) {
            self.close_owner(old, false, window, cx);
        }
        let delay = entity
            .read(cx)
            .config
            .options
            .delay
            .unwrap_or(TooltipDefaults::global(cx).show_delay());
        self.pending = Some(owner.clone());
        let epoch = self.epoch;
        self.show_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |layer, window, cx| {
                if layer.epoch == epoch {
                    layer.show_task = None;
                    layer.pending = None;
                    layer.show(owner, false, window, cx);
                }
            });
        }));
    }
    fn show(
        &mut self,
        owner: WeakEntity<TriggerState>,
        hot: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entity) = owner.upgrade() else {
            return;
        };
        if !entity.read(cx).eligible(window, cx) {
            return;
        }
        entity.update(cx, |s, cx| s.request(true, window, cx));
        if !entity.read(cx).effective_open() {
            return;
        }
        let state = entity.read(cx);
        let config = state.config.clone();
        let anchor = state.anchor.handle();
        let group = state.group;
        let measure = surface::measure(&config, window);
        let parent = anchor
            .bounds()
            .and_then(|b| self.runtime.parent_at(b.center()));
        let scope = gpui_kit::base::active_focus_trap(window, cx);
        let preferred_side = self
            .current
            .as_ref()
            .filter(|_| hot)
            .and_then(|d| d.snapshot.as_ref())
            .and_then(|s| s.read().position())
            .and_then(|p| p.placement);
        let mut moving = hot
            && !cx.reduce_motion()
            && self.current.as_ref().is_some_and(|old| {
                old.measure.size == measure.size
                    && old.config.options.variant == config.options.variant
            });
        let target = Displayed {
            owner: owner.clone(),
            config: config.clone(),
            anchor: anchor.clone(),
            group,
            present: true,
            moving,
            generation: self.serial + 1,
            measure,
            snapshot: self.current.as_ref().and_then(|d| d.snapshot.clone()),
            target_anchor: anchor.bounds(),
            old_text: None,
            preferred_side,
            parent: parent.clone(),
            scope: scope.clone(),
            environment: (window.viewport_size(), window.rem_size()),
        };
        if moving {
            let previous = target.snapshot.as_ref().map(|s| s.read());
            moving = previous.as_ref().is_some_and(|previous| {
                self.specification(&target, window, cx).can_translate(
                    previous,
                    measure.size,
                    surface::length(config.theme, keys::TOOLTIP_MOVE_MAX_DISTANCE, window),
                    window,
                    cx,
                )
            });
        }
        if let Some(old) = self
            .current
            .as_ref()
            .map(|d| d.owner.clone())
            .filter(|o| o.entity_id() != owner.entity_id())
            && let Some(old) = old.upgrade()
        {
            old.update(cx, |s, cx| {
                s.request(false, window, cx);
                s.suppressed = s.hovered || s.focused || s.config.controlled == Some(true);
            });
        }
        if hot && !moving && !cx.reduce_motion() && self.current.is_some() {
            if let Some(old) = &mut self.current {
                old.present = false;
            }
            self.pending = Some(owner.clone());
            let epoch = self.epoch;
            let delay = duration(config.theme, keys::TOOLTIP_DURATION_EXIT);
            self.switch_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update_in(cx, |layer, window, cx| {
                    if layer.epoch == epoch {
                        layer.pending = None;
                        layer.switch_task = None;
                        layer.install(owner, preferred_side, window, cx);
                    }
                });
            }));
            cx.notify();
            return;
        }
        self.install_display(
            Displayed {
                owner,
                config,
                anchor: anchor.clone(),
                group,
                present: true,
                moving,
                generation: self.serial + 1,
                measure,
                snapshot: self.current.as_ref().and_then(|d| d.snapshot.clone()),
                target_anchor: anchor.bounds(),
                old_text: if moving {
                    self.current
                        .as_ref()
                        .map(|d| (d.config.clone(), d.measure, d.generation))
                } else {
                    None
                },
                preferred_side,
                parent,
                scope,
                environment: (window.viewport_size(), window.rem_size()),
            },
            window,
            cx,
        );
    }
    fn install(
        &mut self,
        owner: WeakEntity<TriggerState>,
        preferred_side: Option<Placement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entity) = owner.upgrade() else {
            return;
        };
        let s = entity.read(cx);
        if !s.eligible(window, cx) || !s.effective_open() {
            return;
        }
        let config = s.config.clone();
        let measure = surface::measure(&config, window);
        self.install_display(
            Displayed {
                owner,
                config,
                anchor: s.anchor.handle(),
                group: s.group,
                present: true,
                moving: false,
                generation: self.serial + 1,
                measure,
                snapshot: None,
                target_anchor: s.anchor.handle().bounds(),
                old_text: None,
                preferred_side,
                parent: s
                    .anchor
                    .handle()
                    .bounds()
                    .and_then(|b| self.runtime.parent_at(b.center())),
                scope: gpui_kit::base::active_focus_trap(window, cx),
                environment: (window.viewport_size(), window.rem_size()),
            },
            window,
            cx,
        );
    }
    fn install_display(&mut self, display: Displayed, window: &mut Window, cx: &mut Context<Self>) {
        self.serial += 1;
        self.pending = None;
        self.show_task = None;
        self.auto_task = None;
        self.surface_hovered = false;
        let timeout = display.config.options.auto_hide;
        let owner = display.owner.entity_id();
        let generation = display.generation;
        self.current = Some(display);
        cx.notify();
        if let Some(timeout) = timeout {
            self.auto_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(timeout).await;
                let _ = this.update_in(cx, |layer, window, cx| {
                    if layer
                        .current
                        .as_ref()
                        .is_some_and(|d| d.generation == generation)
                    {
                        layer.close_owner(owner, true, window, cx);
                    }
                });
            }));
        }
    }
    fn leave(&mut self, owner: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.entity_id() == owner)
        {
            // Leave/enter notifications for B→C arrive separately. Keep the
            // short handoff through the current fade so C can prove adjacency
            // to B; B's withdrawn open request prevents a late installation.
            if self.switch_task.is_none() {
                self.epoch += 1;
                self.pending = None;
                self.show_task = None;
            }
            if let Some(owner) = self.entries.get(&owner).and_then(WeakEntity::upgrade) {
                owner.update(cx, |s, cx| s.request(false, window, cx));
            }
        }
        if !self
            .current
            .as_ref()
            .is_some_and(|d| d.owner.entity_id() == owner && d.present)
            || self.grace_task.is_some()
            || self.surface_hovered
        {
            return;
        }
        let period = TooltipDefaults::global(cx).grace_period();
        let generation = self.current.as_ref().unwrap().generation;
        self.grace_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(period).await;
            let _ = this.update_in(cx, |layer, window, cx| {
                layer.grace_task = None;
                if layer
                    .current
                    .as_ref()
                    .is_some_and(|d| d.generation == generation)
                {
                    layer.close_owner(owner, false, window, cx);
                }
            });
        }));
    }
    fn close_owner(
        &mut self,
        owner: EntityId,
        hard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.entity_id() == owner)
        {
            if let Some(pending) = self.pending.as_ref().and_then(WeakEntity::upgrade) {
                pending.update(cx, |s, cx| {
                    s.request(false, window, cx);
                    if hard {
                        s.suppressed = true;
                    }
                });
            }
            self.pending = None;
            self.show_task = None;
            self.switch_task = None;
            self.epoch += 1;
        }
        let Some(display) = self
            .current
            .as_mut()
            .filter(|d| d.owner.entity_id() == owner)
        else {
            return;
        };
        if let Some(entity) = display.owner.upgrade() {
            entity.update(cx, |s, cx| {
                s.request(false, window, cx);
                if hard {
                    s.suppressed = true;
                }
            });
            if !hard && entity.read(cx).effective_open() {
                return;
            }
        }
        display.present = false;
        display.moving = false;
        self.auto_task = None;
        self.grace_task = None;
        cx.notify();
    }
    fn remove(&mut self, owner: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        self.entries.remove(&owner);
        self.close_owner(owner, true, window, cx);
    }
    fn cancel_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.pending.as_ref().map(|owner| owner.entity_id()) {
            self.close_owner(id, true, window, cx);
        }
        self.epoch += 1;
        self.pending = None;
        self.show_task = None;
        self.switch_task = None;
        self.grace_task = None;
        self.auto_task = None;
        if let Some(id) = self.current.as_ref().map(|d| d.owner.entity_id()) {
            self.close_owner(id, true, window, cx);
        }
        self.current = None;
        cx.notify();
    }
    fn pointer_moved(
        &mut self,
        point: gpui_kit::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hovered = self.current.as_ref().is_some_and(|d| {
            d.present
                && d.snapshot
                    .as_ref()
                    .and_then(|s| s.read().visual_bounds())
                    .is_some_and(|b| b.contains(&point))
        });
        if hovered {
            self.grace_task = None;
        } else if self.surface_hovered
            && let Some(owner) = self.current.as_ref().and_then(|d| d.owner.upgrade())
            && !owner.read(cx).wants()
        {
            self.surface_hovered = false;
            self.leave(owner.entity_id(), window, cx);
        }
        self.surface_hovered = hovered;
    }
    fn regions(
        &self,
        group: Option<Group>,
        own: &PopupAnchorHandle,
        cx: &App,
    ) -> Vec<PopupAnchorHandle> {
        let mut regions = vec![own.clone()];
        if let Some(group) = group {
            regions.extend(
                self.entries
                    .values()
                    .filter_map(|w| {
                        let s = w.upgrade()?;
                        let s = s.read(cx);
                        (s.group == Some(group)).then(|| s.anchor.handle())
                    })
                    .filter(|a| a.bounds().is_some()),
            );
            regions.extend(
                self.protected
                    .values()
                    .filter_map(|w| {
                        let s = w.upgrade()?;
                        let s = s.read(cx);
                        (s.group == Some(group)).then(|| s.anchor.handle())
                    })
                    .filter(|a| a.bounds().is_some()),
            );
        }
        regions
    }

    fn specification(
        &self,
        display: &Displayed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupSpec {
        let config = display.config.clone();
        let measure = display.measure;
        let anchor = display.anchor.clone();
        let group = display.group;
        let regions = self.regions(group, &anchor, cx);
        let clearance = surface::length(config.theme, keys::FOCUS_RING_WIDTH, window)
            + surface::length(config.theme, keys::FOCUS_RING_OFFSET, window);
        let gap = config.options.gap.map_or_else(
            || surface::length(config.theme, keys::TOOLTIP_GAP, window),
            |gap| window.rem_size() * gap.0,
        );
        let arrow_depth = if config.options.arrow {
            surface::length(config.theme, keys::TOOLTIP_ARROW_SIZE, window) / 2.
        } else {
            px(0.)
        };
        let requested = config.options.placement.unwrap_or(
            if group.is_some_and(|g| g.axis == Axis::Vertical) {
                TooltipPlacement::Right
            } else {
                TooltipPlacement::Top
            },
        );
        let side = placement(requested);
        let align = align(config.options.align);
        let preferred = display
            .snapshot
            .as_ref()
            .and_then(|s| s.read().position())
            .and_then(|p| p.placement)
            .filter(|_| display.moving || group.is_some())
            .or(display.preferred_side)
            .unwrap_or(side);
        let weak = cx.weak_entity();
        let owner_id = display.owner.entity_id();
        let candidate_anchor = anchor.clone();
        let candidate_regions = regions.clone();
        let availability = config.availability.clone();
        let mut spec = PopupSpec::new(
            PopupId::new(cx.entity_id(), "tooltip"),
            anchor.clone(),
            display.present,
            PopupPlacement::Side(side, align),
        )
        .tip_gap(gap + clearance)
        .margin(surface::length(
            config.theme,
            keys::TOOLTIP_VIEWPORT_MARGIN,
            window,
        ))
        .motion(surface::motion(&config, window))
        .visual_outsets(surface::outsets(&config, window))
        .dismiss_route(PopupDismissRoute::Runtime)
        .dismiss_unplaceable()
        .arrow_within_anchor()
        .dismiss_on(true, true)
        .exclude(anchor.clone())
        .protected_regions(regions.into_iter().map(|a| (a, clearance)))
        .on_dismiss(move |_, window, cx| {
            let _ = weak.update(cx, |layer, cx| {
                layer.close_owner(owner_id, true, window, cx)
            });
        })
        .candidates_with(move |size, _, _| {
            if availability.as_ref().is_some_and(|value| !value.get()) {
                return vec![];
            }
            let Some(trigger) = candidate_anchor.bounds() else {
                return vec![];
            };
            let rail = candidate_regions
                .iter()
                .filter_map(PopupAnchorHandle::bounds)
                .fold(trigger, union);
            let mut sides = vec![preferred, opposite(preferred)];
            if group.is_some() {
                sides.extend([
                    Placement::Top,
                    Placement::Bottom,
                    Placement::Right,
                    Placement::Left,
                ]);
            }
            let mut seen = Vec::new();
            sides
                .into_iter()
                .filter(|s| {
                    if seen.contains(s) {
                        false
                    } else {
                        seen.push(*s);
                        true
                    }
                })
                .map(|side| {
                    let align = effective_align(
                        side,
                        align,
                        measure.single_line,
                        size.width,
                        trigger.size.width,
                    );
                    let rail_gap = if group.is_none() {
                        px(0.)
                    } else {
                        match side {
                            Placement::Top => trigger.top() - rail.top(),
                            Placement::Bottom => rail.bottom() - trigger.bottom(),
                            Placement::Left => trigger.left() - rail.left(),
                            Placement::Right => rail.right() - trigger.right(),
                        }
                    };
                    PopupCandidate::new(side, align, gap + clearance + arrow_depth + rail_gap)
                })
                .collect()
        });
        if let Some(owner) = display.owner.upgrade() {
            spec = spec.modal_scope(owner.read(cx).modal.clone());
            if let Some(focus) = &owner.read(cx).focus {
                spec = spec.scope(focus.clone());
            }
        }
        if let Some(parent) = &display.parent {
            spec = spec.parent(parent.clone());
        }
        if let Some(arrow) = surface::arrow(&config, window) {
            spec = spec.arrow(arrow);
        }
        if display.moving {
            spec = spec.translate_to_anchor(
                Transition::new(duration(config.theme, keys::TOOLTIP_DURATION_MOVE))
                    .ease(easing(config.theme, keys::TOOLTIP_EASE_MOVE)),
                surface::length(config.theme, keys::TOOLTIP_MOVE_MAX_DISTANCE, window),
                display.target_anchor.unwrap_or_default(),
            );
        }
        spec
    }
    fn near(&self, from: &Displayed, to: &TriggerState, window: &Window, cx: &App) -> bool {
        let Some(group) = from.group.filter(|g| Some(*g) == to.group) else {
            return false;
        };
        let (Some(a), Some(b)) = (from.anchor.bounds(), to.anchor.handle().bounds()) else {
            return false;
        };
        if from.parent != self.runtime.parent_at(b.center())
            || from.scope != gpui_kit::base::active_focus_trap(window, cx)
        {
            return false;
        }
        let regions = self.regions(Some(group), &to.anchor.handle(), cx);
        adjacent(
            a,
            b,
            group.axis,
            &regions
                .iter()
                .filter_map(PopupAnchorHandle::bounds)
                .collect::<Vec<_>>(),
            surface::length(to.config.theme, keys::TOOLTIP_MOVE_MAX_GAP, window),
            surface::length(to.config.theme, keys::TOOLTIP_MOVE_MAX_DISTANCE, window),
        )
    }
}

pub(super) fn adjacent(
    a: Bounds<Pixels>,
    b: Bounds<Pixels>,
    axis: Axis,
    regions: &[Bounds<Pixels>],
    max_gap: Pixels,
    max_distance: Pixels,
) -> bool {
    let (a0, a1, b0, b1, ac0, ac1, bc0, bc1) = if axis == Axis::Horizontal {
        (
            a.left(),
            a.right(),
            b.left(),
            b.right(),
            a.top(),
            a.bottom(),
            b.top(),
            b.bottom(),
        )
    } else {
        (
            a.top(),
            a.bottom(),
            b.top(),
            b.bottom(),
            a.left(),
            a.right(),
            b.left(),
            b.right(),
        )
    };
    if (ac1.min(bc1) - ac0.max(bc0)) < (ac1 - ac0).min(bc1 - bc0) / 2. {
        return false;
    }
    if (a0.max(b0) - a1.min(b1)).max(px(0.)) > max_gap {
        return false;
    }
    let d = a.center() - b.center();
    if f32::from(d.x).hypot(f32::from(d.y)) > f32::from(max_distance) {
        return false;
    }
    let (lo, hi) = if axis == Axis::Horizontal {
        (
            a.center().x.min(b.center().x),
            a.center().x.max(b.center().x),
        )
    } else {
        (
            a.center().y.min(b.center().y),
            a.center().y.max(b.center().y),
        )
    };
    !regions.iter().any(|r| {
        let p = if axis == Axis::Horizontal {
            r.center().x
        } else {
            r.center().y
        };
        let (cross_start, cross_end) = if axis == Axis::Horizontal {
            (r.top(), r.bottom())
        } else {
            (r.left(), r.right())
        };
        p > lo
            && p < hi
            && *r != a
            && *r != b
            && cross_start < ac1.min(bc1)
            && cross_end > ac0.max(bc0)
    })
}

impl Render for Layer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.entries.retain(|_, w| w.upgrade().is_some());
        self.protected.retain(|_, w| w.upgrade().is_some());
        let Some(mut display) = self.current.clone() else {
            return div().into_any_element();
        };
        let Some(owner) = display.owner.upgrade() else {
            self.current = None;
            return div().into_any_element();
        };
        if !owner.read(cx).eligible(window, cx) && display.present {
            self.close_owner(owner.entity_id(), true, window, cx);
            self.current = None;
            return div().into_any_element();
        }
        let measure = surface::measure(&display.config, window);
        if display.measure.size != measure.size
            || display.environment != (window.viewport_size(), window.rem_size())
            || display.target_anchor != display.anchor.bounds()
        {
            display.moving = false;
            display.old_text = None;
            if let Some(current) = &mut self.current {
                current.moving = false;
                current.old_text = None;
                current.environment = (window.viewport_size(), window.rem_size());
            }
        }
        display.measure = measure;
        let config = display.config.clone();
        let measure = display.measure;
        let spec = self.specification(&display, window, cx);
        let text_transition = Transition::new(if display.old_text.is_some() {
            duration(config.theme, keys::TOOLTIP_DURATION_MOVE)
        } else {
            Duration::ZERO
        })
        .ease(easing(config.theme, keys::TOOLTIP_EASE_MOVE));
        let text = gpui_kit::base::Presence::new(
            (ElementId::Integer(display.generation), "tooltip-copy"),
            true,
        )
        .transition(text_transition)
        .sample(window, cx);
        let old_text = display
            .old_text
            .as_ref()
            .and_then(|(config, measure, generation)| {
                let sample = gpui_kit::base::Presence::new(
                    (ElementId::Integer(*generation), "tooltip-copy"),
                    false,
                )
                .transition(
                    Transition::new(duration(config.theme, keys::TOOLTIP_DURATION_MOVE))
                        .ease(easing(config.theme, keys::TOOLTIP_EASE_MOVE)),
                )
                .sample(window, cx);
                sample
                    .should_render()
                    .then(|| (config.clone(), *measure, sample.progress))
            });
        if display.old_text.is_some()
            && old_text.is_none()
            && let Some(d) = &mut self.current
        {
            d.old_text = None;
        }
        let mut sample = None;
        let surface = PopupHost::new(spec).mount(
            |presentation, snapshot, window, _| {
                sample = Some(snapshot);
                surface::content(
                    &config,
                    measure,
                    presentation.is_interactive(),
                    if display.old_text.is_some() {
                        text.progress
                    } else {
                        1.
                    },
                    old_text,
                    window,
                )
            },
            window,
            cx,
        );
        if let Some(d) = &mut self.current {
            d.snapshot = sample;
            d.measure = measure;
        }
        if surface.is_none() && !display.present {
            self.current = None;
        }
        div()
            .absolute()
            .inset_0()
            .children(surface)
            .into_any_element()
    }
}
impl RootPlugin for Layer {
    fn decorate(
        &self,
        surface: AnyElement,
        root: &Root,
        _: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let focused = self
            .entries
            .values()
            .filter_map(WeakEntity::upgrade)
            .filter(|s| s.read(cx).focus.is_some())
            .map(|s| s.downgrade())
            .collect::<Vec<_>>();
        let layer = root.plugin::<Layer>().map(|l| l.downgrade());
        let pointer = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture
                        && let Some(layer) = &layer
                    {
                        let _ = layer.update(cx, |layer, cx| {
                            layer.pointer_moved(event.position, window, cx)
                        });
                    }
                });
            },
        )
        .absolute()
        .inset_0();
        div()
            .relative()
            .size_full()
            .capture_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.is_held
                    || !matches!(event.keystroke.key.as_str(), "enter" | "space")
                    || event.keystroke.modifiers.modified()
                {
                    return;
                }
                for owner in &focused {
                    if let Some(owner) = owner.upgrade()
                        && owner
                            .read(cx)
                            .focus
                            .as_ref()
                            .is_some_and(|f| f.is_focused(window))
                    {
                        owner.update(cx, |s, cx| s.activate(window, cx));
                        break;
                    }
                }
            })
            .child(surface)
            .child(pointer)
    }
}
fn placement(p: TooltipPlacement) -> Placement {
    match p {
        TooltipPlacement::Top => Placement::Top,
        TooltipPlacement::Bottom => Placement::Bottom,
        TooltipPlacement::Left => Placement::Left,
        TooltipPlacement::Right => Placement::Right,
    }
}
fn align(a: TooltipAlign) -> Align {
    match a {
        TooltipAlign::Start => Align::Start,
        TooltipAlign::Center => Align::Center,
        TooltipAlign::End => Align::End,
    }
}
fn opposite(p: Placement) -> Placement {
    match p {
        Placement::Top => Placement::Bottom,
        Placement::Bottom => Placement::Top,
        Placement::Left => Placement::Right,
        Placement::Right => Placement::Left,
    }
}
pub(super) fn effective_align(
    side: Placement,
    requested: Align,
    single_line: bool,
    width: Pixels,
    trigger_width: Pixels,
) -> Align {
    if (matches!(side, Placement::Left | Placement::Right) && single_line)
        || (matches!(side, Placement::Top | Placement::Bottom) && width <= trigger_width)
    {
        Align::Center
    } else {
        requested
    }
}
