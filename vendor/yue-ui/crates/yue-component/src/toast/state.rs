use super::*;
use gpui_kit::base::{
    ModalScopeChanges, Root, ToastManager, ToastMotion, ToastOptions, ToastStackState,
    ToastTransitionStatus, active_modal_scope,
};
use gpui_kit::{
    App, Bounds, Context, Entity, EventEmitter, FocusHandle, Pixels, Subscription, Task,
    WeakEntity, WeakFocusHandle, Window, WindowId,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};

const HOVER: u8 = 1;
const FOCUS: u8 = 2;
const PRESSED: u8 = 4;
const INACTIVE: u8 = 8;
const MODAL: u8 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastPhase {
    Waiting,
    Starting,
    Present,
    Ending,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToastPlayback {
    #[default]
    Normal,
    Slow,
    Paused,
}
impl ToastPlayback {
    fn rate(self) -> f32 {
        match self {
            Self::Normal => 1.,
            Self::Slow => 0.25,
            Self::Paused => 0.,
        }
    }
}
/// Lifecycle events are emitted after the mutation, never from a card's render.
#[derive(Clone, Debug)]
pub enum ToastEvent {
    Presented {
        id: ToastId,
        generation: u64,
    },
    Dismissed {
        id: ToastId,
        generation: u64,
        reason: ToastDismissReason,
    },
    Removed {
        id: ToastId,
        generation: u64,
        reason: ToastDismissReason,
    },
}
#[derive(Clone, Debug)]
pub struct ToastSnapshot {
    pub id: ToastId,
    pub generation: u64,
    pub phase: ToastPhase,
    pub remaining: Option<Duration>,
    pub total: Option<Duration>,
    pub bounds: Option<Bounds<Pixels>>,
    pub reason: Option<ToastDismissReason>,
}

/// A weak, window-bound capability for one generation and one business operation.
#[derive(Clone)]
pub struct ToastTicket {
    pub(super) owner: WeakEntity<YueToasterState>,
    pub(super) window: WindowId,
    pub(super) id: ToastId,
    pub(super) generation: u64,
    pub(super) operation: u64,
}
impl ToastTicket {
    pub fn id(&self) -> &ToastId {
        &self.id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn update(&self, spec: ToastSpec, w: &mut Window, cx: &mut App) -> Result<(), ToastError> {
        if self.window != w.window_handle().window_id() {
            return Err(ToastError::WrongWindow);
        }
        self.owner
            .update(cx, |s, cx| s.update(self, spec, w, cx))
            .map_err(|_| ToastError::Unavailable)?
    }
    pub fn dismiss(&self, w: &mut Window, cx: &mut App) -> Result<bool, ToastError> {
        if self.window != w.window_handle().window_id() {
            return Err(ToastError::WrongWindow);
        }
        self.owner
            .update(cx, |s, cx| {
                s.dismiss(self, ToastDismissReason::Programmatic, w, cx)
            })
            .map_err(|_| ToastError::Unavailable)?
    }
    /// Invalidate earlier work before starting a retry. Closing a toast does not cancel a Future.
    pub fn begin_operation(&self, w: &mut Window, cx: &mut App) -> Result<Self, ToastError> {
        if self.window != w.window_handle().window_id() {
            return Err(ToastError::WrongWindow);
        }
        self.owner
            .update(cx, |s, cx| {
                s.validate_ticket(self, w)?;
                let record = s.record_mut(self.generation).ok_or(ToastError::Missing)?;
                record.operation += 1;
                let mut ticket = self.clone();
                ticket.operation = record.operation;
                cx.notify();
                Ok(ticket)
            })
            .map_err(|_| ToastError::Unavailable)?
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Visual {
    pub bounds: Option<Bounds<Pixels>>,
    pub opacity: f32,
    pub content: f32,
    pub foreground: bool,
    pub colors: Option<[gpui_kit::Rgba; 4]>,
    pub sampled_at: Option<Instant>,
}
impl Default for Visual {
    fn default() -> Self {
        Self {
            bounds: None,
            opacity: 1.,
            content: 1.,
            foreground: true,
            colors: None,
            sampled_at: None,
        }
    }
}
impl Visual {
    pub(super) fn unmount(&mut self) {
        self.bounds = None;
        self.opacity = 0.;
        self.content = 0.;
        self.foreground = false;
        self.sampled_at = None;
    }
}
pub(super) struct Record {
    pub id: ToastId,
    pub generation: u64,
    pub operation: u64,
    pub revision: u64,
    pub semantic_revision: u64,
    pub announcement_suppressed: bool,
    pub display_revision: u64,
    pub spec: ToastSpec,
    pub display: ToastSpec,
    pub previous: Option<ToastSpec>,
    pub previous_alpha: f32,
    pub changed_at: Instant,
    pub measured: Option<(Pixels, Pixels, f32)>, // natural height, constraint width, rem
    pub focus: FocusHandle,
    pub action_focus: FocusHandle,
    pub close_focus: FocusHandle,
    pub action_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub close_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub visual: Rc<RefCell<Visual>>,
    pub painted: Rc<RefCell<Option<ToastSpec>>>,
    pub exit: Option<Visual>,
    pub entry: Option<Visual>,
    pub superseded: bool,
    pub exit_spec: Option<ToastSpec>,
    pub reason: Option<ToastDismissReason>,
    pub delivered: Rc<Cell<bool>>,
    pub icon_slot: bool,
}
pub(super) struct Clock {
    real: Instant,
    virtual_time: Instant,
    playback: ToastPlayback,
}
impl Clock {
    fn new(now: Instant) -> Self {
        Self {
            real: now,
            virtual_time: now,
            playback: ToastPlayback::Normal,
        }
    }
    pub fn now(&self, now: Instant) -> Instant {
        self.virtual_time
            + now
                .saturating_duration_since(self.real)
                .mul_f32(self.playback.rate())
    }
    fn set(&mut self, now: Instant, playback: ToastPlayback) {
        self.virtual_time = self.now(now);
        self.real = now;
        self.playback = playback;
    }
}

/// One retained owner per Root/window. Applications acquire it with `for_window`.
pub struct YueToasterState {
    pub(super) owner: WeakEntity<Self>,
    pub(super) window: WindowId,
    pub(super) manager: ToastManager<u64, Record>,
    pub(super) waiting: VecDeque<Record>,
    pub(super) stack: ToastStackState,
    pub(super) options: ToasterOptions,
    pub(super) focus: FocusHandle,
    pub(super) previous_focus: Option<WeakFocusHandle>,
    pub(super) expanded: bool,
    pub(super) collapse_at: Option<Instant>,
    pub(super) clock: Clock,
    pub(super) grow_up: bool,
    pub(super) scroll: gpui_kit::ScrollHandle,
    pub(super) scroll_session: Option<(u64, bool)>,
    pub(super) signature: Option<(
        Pixels,
        f32,
        crate::ThemeMode,
        crate::Accent,
        crate::NeutralTone,
    )>,
    reasons: u8,
    closed: bool,
    serial: u64,
    scheduled: Option<Instant>,
    deadline: Option<Task<()>>,
    last_advanced: Instant,
    subscriptions: Vec<Subscription>,
}
impl EventEmitter<ToastEvent> for YueToasterState {}
impl YueToasterState {
    pub fn for_window(w: &Window, cx: &App) -> Result<Entity<Self>, ToastError> {
        w.root::<Root>()
            .flatten()
            .and_then(|r| r.read(cx).plugin::<Self>())
            .ok_or(ToastError::Unavailable)
    }
    pub(super) fn new(w: &mut Window, cx: &mut Context<Self>) -> Self {
        let now = cx.background_executor().now();
        let focus = cx.focus_handle();
        let mut state = Self {
            owner: cx.weak_entity(),
            closed: false,
            window: w.window_handle().window_id(),
            manager: ToastManager::new(ToastMotion::sonner())
                .durations(Duration::from_millis(300), Duration::from_millis(200)),
            waiting: VecDeque::new(),
            stack: ToastStackState::default(),
            options: ToasterOptions::default(),
            focus: focus.clone(),
            previous_focus: None,
            expanded: false,
            collapse_at: None,
            clock: Clock::new(now),
            grow_up: false,
            scroll: gpui_kit::ScrollHandle::new(),
            scroll_session: None,
            signature: None,
            reasons: if w.is_window_active() && w.is_visible() {
                0
            } else {
                INACTIVE
            },
            serial: 0,
            scheduled: None,
            deadline: None,
            last_advanced: now,
            subscriptions: vec![],
        };
        let owner = cx.weak_entity();
        let window_id = state.window;
        state.subscriptions.push(cx.on_window_closed(move |cx, id| {
            if id != window_id {
                return;
            }
            let _ = owner.update(cx, |s, cx| s.window_closed(cx));
        }));
        state
            .subscriptions
            .push(cx.observe_window_visibility(w, |s, _, w, cx| {
                s.sync(w, cx);
                cx.notify();
            }));
        state
            .subscriptions
            .push(cx.observe_window_activation(w, |s, w, cx| {
                s.sync(w, cx);
                cx.notify();
            }));
        state
            .subscriptions
            .push(cx.observe_global_in::<ModalScopeChanges>(w, |s, w, cx| {
                s.sync(w, cx);
                cx.notify();
            }));
        state
            .subscriptions
            .push(cx.on_focus_in(&focus, w, |s, w, cx| {
                s.set_reason(FOCUS, true, w, cx);
            }));
        state
            .subscriptions
            .push(cx.on_focus_out(&focus, w, |s, _, w, cx| {
                let inside = s.focus.contains_focused(w, cx);
                s.set_reason(FOCUS, inside, w, cx);
            }));
        state
    }
    fn window_closed(&mut self, cx: &mut Context<Self>) {
        self.closed = true;
        self.deadline = None;
        self.scheduled = None;
        let records =
            std::mem::replace(&mut self.manager, ToastManager::new(ToastMotion::sonner()));
        for (g, r, _) in records.iter() {
            if r.reason.is_none() {
                cx.emit(ToastEvent::Dismissed {
                    id: r.id.clone(),
                    generation: *g,
                    reason: ToastDismissReason::WindowClosed,
                });
            }
            cx.emit(ToastEvent::Removed {
                id: r.id.clone(),
                generation: *g,
                reason: r.reason.unwrap_or(ToastDismissReason::WindowClosed),
            });
        }
        while let Some(r) = self.waiting.pop_front() {
            self.emit_closed(&r, ToastDismissReason::WindowClosed, cx);
        }
        self.stack.clear();
        self.previous_focus = None;
        self.reasons = INACTIVE;
        cx.notify();
    }
    pub fn len(&self) -> usize {
        self.manager.len() + self.waiting.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn options(&self) -> &ToasterOptions {
        &self.options
    }
    pub fn is_paused(&self) -> bool {
        self.reasons != 0
    }
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }
    pub(super) fn hidden(&self) -> bool {
        self.reasons & (INACTIVE | MODAL) != 0
    }
    pub(super) fn now(&self, cx: &App) -> Instant {
        self.clock.now(cx.background_executor().now())
    }
    pub(super) fn animating(&self) -> bool {
        self.clock.playback != ToastPlayback::Paused
    }
    pub fn playback(&self) -> ToastPlayback {
        self.clock.playback
    }
    pub fn configure(
        &mut self,
        options: ToasterOptions,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ToastError> {
        self.check_window(w)?;
        if !options.validate() {
            return Err(ToastError::InvalidOptions);
        }
        if !self.is_empty() {
            return Err(ToastError::Occupied);
        }
        self.options = options;
        self.signature = None;
        cx.notify();
        Ok(())
    }
    pub fn snapshots(&self, cx: &App) -> Vec<ToastSnapshot> {
        let now = self.now(cx);
        self.waiting
            .iter()
            .map(|r| ToastSnapshot {
                id: r.id.clone(),
                generation: r.generation,
                phase: ToastPhase::Waiting,
                remaining: r.spec.lifetime(),
                total: r.spec.lifetime(),
                bounds: None,
                reason: None,
            })
            .chain(self.manager.iter().map(|(id, r, status)| {
                let reading = self.manager.reading_time_at(id, now, self.is_paused());
                ToastSnapshot {
                    id: r.id.clone(),
                    generation: *id,
                    phase: match status {
                        ToastTransitionStatus::Starting => ToastPhase::Starting,
                        ToastTransitionStatus::Present => ToastPhase::Present,
                        ToastTransitionStatus::Ending => ToastPhase::Ending,
                    },
                    remaining: reading.map(|t| t.0),
                    total: reading.map(|t| t.1),
                    bounds: r.visual.borrow().bounds,
                    reason: r.reason,
                }
            }))
            .collect()
    }
    pub fn push(
        &mut self,
        spec: ToastSpec,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<ToastTicket, ToastError> {
        self.check_window(w)?;
        spec.validate()?;
        self.sync(w, cx);
        self.advance(self.now(cx), w, cx);
        // Capacity includes exits. A new generation must never evict a live record.
        if self.len() >= self.options.capacity {
            return Err(ToastError::Capacity);
        }
        self.serial += 1;
        let generation = self.serial;
        let id = spec
            .id
            .clone()
            .unwrap_or_else(|| ToastId::new(format!("toast-{generation}")));
        let replaced = self
            .manager
            .iter()
            .filter(|(_, r, s)| r.id == id && *s != ToastTransitionStatus::Ending)
            .map(|(i, _, _)| *i)
            .collect::<Vec<_>>();
        for old in replaced {
            self.close_generation(old, ToastDismissReason::Programmatic, w, cx);
        }
        if let Some(ix) = self.waiting.iter().position(|r| r.id == id) {
            let old = self.waiting.remove(ix).unwrap();
            self.emit_closed(&old, ToastDismissReason::Programmatic, cx);
        }
        let now = self.now(cx);
        let previous_generation = self
            .manager
            .iter()
            .rev()
            .find(|(_, r, _)| r.id == id)
            .map(|(g, _, _)| *g);
        let mut entry = None;
        let mut previous = None;
        if let Some(g) = previous_generation {
            let elapsed = self
                .manager
                .transition_elapsed_at(&g, now)
                .unwrap_or_default();
            let t = crate::current_theme(cx);
            let exit_duration = crate::tokens::duration(t, crate::theme::keys::TOAST_DURATION_EXIT);
            let p = crate::MotionEase::Exit.easing(t)(
                (elapsed.as_secs_f32() / exit_duration.as_secs_f32()).clamp(0., 1.),
            );
            if let Some(old) = self.manager.get_mut(&g)
                && let Some(mut visual) = old.exit
            {
                let distance = super::card::length(
                    t,
                    if visual.foreground {
                        crate::theme::keys::SPACE_24
                    } else {
                        crate::theme::keys::SPACE_8
                    },
                    w,
                ) * p;
                if let Some(bounds) = &mut visual.bounds {
                    match self.options.position {
                        ToastPosition::Left => bounds.origin.x -= distance,
                        ToastPosition::Right => bounds.origin.x += distance,
                        p if p.bottom() => bounds.origin.y += distance,
                        _ => bounds.origin.y -= distance,
                    }
                }
                visual.opacity *= 1. - p;
                entry = Some(visual);
                previous = old.exit_spec.clone();
                old.superseded = true;
            }
        }
        self.waiting.push_back(Record {
            id: id.clone(),
            generation,
            operation: 0,
            revision: 0,
            semantic_revision: 0,
            announcement_suppressed: false,
            display_revision: 0,
            display: spec.clone(),
            icon_slot: spec.glyph().is_some(),
            spec,
            previous,
            previous_alpha: entry.map_or(1., |v| v.content),
            changed_at: now,
            measured: None,
            focus: cx.focus_handle(),
            action_focus: cx.focus_handle(),
            close_focus: cx.focus_handle(),
            action_bounds: Rc::new(Cell::new(None)),
            close_bounds: Rc::new(Cell::new(None)),
            visual: Rc::new(RefCell::new(Visual::default())),
            painted: Rc::new(RefCell::new(None)),
            exit: None,
            entry,
            superseded: false,
            exit_spec: None,
            reason: None,
            delivered: Rc::new(Cell::new(false)),
        });
        cx.notify();
        self.schedule(w, cx);
        Ok(ToastTicket {
            owner: cx.weak_entity(),
            window: self.window,
            id,
            generation,
            operation: 0,
        })
    }
    pub fn update(
        &mut self,
        ticket: &ToastTicket,
        spec: ToastSpec,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ToastError> {
        self.check_window(w)?;
        spec.validate()?;
        self.sync(w, cx);
        self.advance(self.now(cx), w, cx);
        self.validate_ticket(ticket, w)?;
        if spec.id.as_ref().is_some_and(|id| id != &ticket.id) {
            return Err(ToastError::Stale);
        }
        let r = self
            .record_mut(ticket.generation)
            .ok_or(ToastError::Missing)?;
        let reset = spec.timeout.or_else(|| {
            (r.spec.kind == ToastKind::Loading && spec.kind != ToastKind::Loading)
                .then(|| spec.lifetime())
        });
        let semantic = r.spec.spoken() != spec.spoken() || r.spec.announcement != spec.announcement;
        r.icon_slot |= spec.glyph().is_some();
        r.spec = spec;
        r.revision += 1;
        if semantic {
            r.delivered = Rc::new(Cell::new(false));
            r.semantic_revision += 1;
            r.announcement_suppressed = false;
        }
        // An updating action may temporarily fade out or disappear. Retain a real focus target.
        if r.focus.contains_focused(w, cx) {
            r.focus.focus(w, cx);
        }
        // Preserve the display/measurement until the replacement has a valid natural size.
        if self.manager.get(&ticket.generation).is_some() {
            // A small metadata reset API avoids cloning Record / the manager's private clocks.
            self.manager.reset_timeout(&ticket.generation, reset);
        }
        self.schedule(w, cx);
        cx.notify();
        Ok(())
    }
    pub fn dismiss(
        &mut self,
        ticket: &ToastTicket,
        reason: ToastDismissReason,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, ToastError> {
        self.check_window(w)?;
        if ticket.window != self.window {
            return Err(ToastError::WrongWindow);
        }
        if ticket.owner.entity_id() != self.owner.entity_id() {
            return Err(ToastError::Stale);
        }
        self.sync(w, cx);
        if self.manager.iter().any(|(g, r, s)| {
            *g == ticket.generation
                && r.operation == ticket.operation
                && s == ToastTransitionStatus::Ending
        }) {
            return Ok(false);
        }
        self.validate_ticket(ticket, w)?;
        if matches!(
            reason,
            ToastDismissReason::CloseButton | ToastDismissReason::Keyboard
        ) && self
            .waiting
            .iter()
            .find(|r| r.generation == ticket.generation)
            .or_else(|| self.manager.get(&ticket.generation))
            .is_some_and(|r| !r.spec.can_close())
        {
            return Ok(false);
        }
        Ok(self.close_generation(ticket.generation, reason, w, cx))
    }
    pub fn dismiss_all(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.check_window(w).is_err() {
            return;
        }
        self.sync(w, cx);
        while let Some(record) = self.waiting.pop_front() {
            self.emit_closed(&record, ToastDismissReason::Programmatic, cx);
        }
        let ids = self.manager.iter().map(|(g, _, _)| *g).collect::<Vec<_>>();
        for g in ids {
            self.close_generation(g, ToastDismissReason::Programmatic, w, cx);
        }
        self.schedule(w, cx);
        cx.notify();
    }
    pub fn set_playback(
        &mut self,
        playback: ToastPlayback,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync(w, cx);
        self.advance(self.now(cx), w, cx);
        self.clock.set(cx.background_executor().now(), playback);
        self.scheduled = None;
        self.schedule(w, cx);
        cx.notify();
    }
    pub fn step(&mut self, delta: Duration, w: &mut Window, cx: &mut Context<Self>) {
        if self.playback() != ToastPlayback::Paused {
            return;
        }
        self.clock.virtual_time += delta;
        self.advance(self.now(cx), w, cx);
        self.schedule(w, cx);
        cx.notify();
    }
    pub fn focus_notifications(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        self.sync(w, cx);
        if self.hidden() || crate::popup::PopupRuntime::for_window(w, cx).has_present_surface() {
            return false;
        }
        let target = self
            .manager
            .iter()
            .rev()
            .find(|(_, _, s)| *s != ToastTransitionStatus::Ending)
            .map(|(_, r, _)| r.focus.clone());
        if let Some(focus) = target {
            if !self.focus.contains_focused(w, cx) {
                self.previous_focus = w.focused(cx).map(|h| h.downgrade());
            }
            focus.focus(w, cx);
            self.set_reason(FOCUS, true, w, cx);
            return true;
        }
        false
    }
    pub(super) fn check_window(&self, w: &Window) -> Result<(), ToastError> {
        if self.closed {
            return Err(ToastError::Unavailable);
        }
        if self.window == w.window_handle().window_id() {
            Ok(())
        } else {
            Err(ToastError::WrongWindow)
        }
    }
    fn validate_ticket(&self, ticket: &ToastTicket, w: &Window) -> Result<(), ToastError> {
        self.check_window(w)?;
        if ticket.window != self.window {
            return Err(ToastError::WrongWindow);
        }
        if ticket.owner.entity_id() != self.owner.entity_id() {
            return Err(ToastError::Stale);
        }
        if self
            .manager
            .iter()
            .any(|(g, _, s)| *g == ticket.generation && s == ToastTransitionStatus::Ending)
        {
            return Err(ToastError::AlreadyClosing);
        }
        let r = self
            .waiting
            .iter()
            .find(|r| r.generation == ticket.generation)
            .or_else(|| self.manager.get(&ticket.generation))
            .ok_or(ToastError::Stale)?;
        if r.id != ticket.id || r.operation != ticket.operation {
            return Err(ToastError::Stale);
        }
        Ok(())
    }
    pub(super) fn actionable(&self, ticket: &ToastTicket, revision: u64, w: &Window) -> bool {
        !self.hidden()
            && self.validate_ticket(ticket, w).is_ok()
            && self
                .manager
                .get(&ticket.generation)
                .is_some_and(|r| r.revision == revision && r.spec.action.is_some())
    }
    pub(super) fn ticket(&self, r: &Record, cx: &Context<Self>) -> ToastTicket {
        ToastTicket {
            owner: cx.weak_entity(),
            window: self.window,
            id: r.id.clone(),
            generation: r.generation,
            operation: r.operation,
        }
    }
    pub(super) fn record_mut(&mut self, g: u64) -> Option<&mut Record> {
        if let Some(ix) = self.waiting.iter().position(|r| r.generation == g) {
            self.waiting.get_mut(ix)
        } else {
            self.manager.get_mut(&g)
        }
    }
    fn emit_closed(&self, r: &Record, reason: ToastDismissReason, cx: &mut Context<Self>) {
        cx.emit(ToastEvent::Dismissed {
            id: r.id.clone(),
            generation: r.generation,
            reason,
        });
        cx.emit(ToastEvent::Removed {
            id: r.id.clone(),
            generation: r.generation,
            reason,
        });
    }
    pub(super) fn close_generation(
        &mut self,
        g: u64,
        reason: ToastDismissReason,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(ix) = self.waiting.iter().position(|r| r.generation == g) {
            let r = self.waiting.remove(ix).unwrap();
            self.emit_closed(&r, reason, cx);
            cx.notify();
            return true;
        }
        let now = self.now(cx);
        if !self.manager.dismiss(&g, now) {
            return false;
        }
        if let Some(r) = self.manager.get_mut(&g) {
            r.exit = Some(*r.visual.borrow());
            r.exit_spec = r
                .painted
                .borrow()
                .clone()
                .or_else(|| Some(r.display.clone()));
            r.reason = Some(reason);
            cx.emit(ToastEvent::Dismissed {
                id: r.id.clone(),
                generation: g,
                reason,
            });
        }
        self.restore_after_close(g, w, cx);
        self.schedule(w, cx);
        cx.notify();
        true
    }
    fn restore_after_close(&mut self, g: u64, w: &mut Window, cx: &mut Context<Self>) {
        let focused = self
            .manager
            .get(&g)
            .is_some_and(|r| r.focus.contains_focused(w, cx));
        if !focused {
            return;
        }
        if let Some((_, r, _)) = self
            .manager
            .iter()
            .rev()
            .find(|(_, _, s)| *s != ToastTransitionStatus::Ending)
        {
            r.focus.focus(w, cx);
        } else {
            self.restore_focus(w, cx);
        }
    }
    pub(super) fn restore_focus(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = self
            .previous_focus
            .take()
            .and_then(|h| h.upgrade())
            .filter(|h| Root::is_focus_available(h, w))
        {
            handle.focus(w, cx);
        } else {
            w.blur(cx);
        }
    }
    pub(super) fn collapse_notifications(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.advance(self.now(cx), w, cx);
        self.reasons &= !(HOVER | FOCUS | PRESSED);
        self.restore_focus(w, cx);
        self.expanded = false;
        self.collapse_at = None;
        self.schedule(w, cx);
        cx.notify();
    }
    pub(super) fn hover(&mut self, value: bool, w: &mut Window, cx: &mut Context<Self>) {
        if value && self.hidden() {
            return;
        }
        self.set_reason(HOVER, value, w, cx);
    }
    pub(super) fn press(&mut self, value: bool, w: &mut Window, cx: &mut Context<Self>) {
        if value && self.hidden() {
            return;
        }
        self.set_reason(PRESSED, value, w, cx);
    }
    fn set_reason(&mut self, reason: u8, value: bool, w: &mut Window, cx: &mut Context<Self>) {
        let next = if value {
            self.reasons | reason
        } else {
            self.reasons & !reason
        };
        if next == self.reasons {
            return;
        }
        self.advance(self.now(cx), w, cx);
        self.reasons = next;
        self.expansion(cx);
        self.schedule(w, cx);
        cx.notify();
    }
    fn expansion(&mut self, cx: &App) {
        if self.reasons & (HOVER | FOCUS | PRESSED) != 0 {
            self.expanded = true;
            self.collapse_at = None;
        } else if self.expanded && self.collapse_at.is_none() {
            self.collapse_at = Some(
                self.now(cx)
                    + crate::tokens::duration(
                        crate::current_theme(cx),
                        crate::theme::keys::TOAST_COLLAPSE_DELAY,
                    ),
            );
        }
    }
    fn silence_delivered(&mut self) {
        let ids = self
            .manager
            .iter()
            .filter(|(_, r, _)| r.delivered.get())
            .map(|(g, _, _)| *g)
            .collect::<Vec<_>>();
        for g in ids {
            self.manager.get_mut(&g).unwrap().announcement_suppressed = true;
        }
    }
    pub(super) fn sync(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let modal = active_modal_scope(w, cx).is_some();
        let inactive = !w.is_window_active() || !w.is_visible();
        let old_modal = self.reasons & MODAL != 0;
        if modal != old_modal {
            let real = cx
                .try_global::<ModalScopeChanges>()
                .and_then(|c| c.changed_at(w))
                .unwrap_or_else(|| cx.background_executor().now());
            self.advance(self.clock.now(real).min(now), w, cx);
            if modal {
                self.silence_delivered();
                self.reasons |= MODAL;
                self.reasons &= !(HOVER | PRESSED);
                self.expanded = false;
                self.collapse_at = None;
            } else {
                self.reasons &= !MODAL;
            }
        }
        if inactive != (self.reasons & INACTIVE != 0) {
            self.advance(now, w, cx);
            if inactive {
                self.silence_delivered();
                self.reasons |= INACTIVE;
                self.reasons &= !(HOVER | PRESSED);
            } else {
                self.reasons &= !INACTIVE;
            }
        }
        let theme = crate::current_theme(cx);
        self.manager.set_durations(
            if cx.reduce_motion() {
                Duration::ZERO
            } else {
                crate::tokens::duration(theme, crate::theme::keys::TOAST_DURATION_ENTER)
            },
            if cx.reduce_motion() {
                Duration::ZERO
            } else {
                crate::tokens::duration(theme, crate::theme::keys::TOAST_DURATION_EXIT)
            },
        );
        self.schedule(w, cx);
    }
    pub(super) fn advance(&mut self, now: Instant, w: &mut Window, cx: &mut Context<Self>) {
        let now = now.max(self.last_advanced);
        self.last_advanced = now;
        let delta = self.manager.advance(now, self.is_paused());
        for g in delta.presented {
            if let Some(r) = self.manager.get(&g) {
                cx.emit(ToastEvent::Presented {
                    id: r.id.clone(),
                    generation: g,
                });
            }
        }
        for g in delta.ending {
            if let Some(r) = self.manager.get_mut(&g) {
                r.exit = Some(*r.visual.borrow());
                r.exit_spec = r
                    .painted
                    .borrow()
                    .clone()
                    .or_else(|| Some(r.display.clone()));
                r.reason = Some(ToastDismissReason::Timeout);
                cx.emit(ToastEvent::Dismissed {
                    id: r.id.clone(),
                    generation: g,
                    reason: ToastDismissReason::Timeout,
                });
            }
            self.restore_after_close(g, w, cx);
        }
        for (g, r) in delta.removed {
            let reason = r.reason.unwrap_or(ToastDismissReason::Timeout);
            if r.reason.is_none() {
                cx.emit(ToastEvent::Dismissed {
                    id: r.id.clone(),
                    generation: g,
                    reason,
                });
            }
            cx.emit(ToastEvent::Removed {
                id: r.id,
                generation: g,
                reason,
            });
        }
        let content_duration = crate::tokens::duration(
            crate::current_theme(cx),
            crate::theme::keys::TOAST_DURATION_CONTENT,
        );
        let revisions = self
            .manager
            .iter()
            .filter(|(_, r, _)| {
                r.previous.is_some()
                    && now.saturating_duration_since(r.changed_at) >= content_duration
            })
            .map(|(g, _, _)| *g)
            .collect::<Vec<_>>();
        for g in revisions {
            self.manager.get_mut(&g).unwrap().previous = None;
        }
        if self.collapse_at.is_some_and(|at| now >= at) {
            self.expanded = false;
            self.collapse_at = None;
        }
        if self.is_empty() {
            self.stack.clear();
            self.expanded = false;
            self.collapse_at = None;
            self.reasons &= !(HOVER | PRESSED);
        }
    }
    pub(super) fn schedule(&mut self, w: &Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let target = self
            .manager
            .next_deadline(now, self.is_paused())
            .into_iter()
            .chain(self.collapse_at)
            .chain(
                self.manager
                    .iter()
                    .filter(|(_, r, _)| r.previous.is_some())
                    .map(|(_, r, _)| {
                        r.changed_at
                            + crate::tokens::duration(
                                crate::current_theme(cx),
                                crate::theme::keys::TOAST_DURATION_CONTENT,
                            )
                    }),
            )
            .min();
        if self.scheduled == target {
            return;
        }
        self.deadline = None;
        self.scheduled = target;
        if self.clock.playback == ToastPlayback::Paused {
            return;
        }
        if let Some(target) = target {
            let delay = target
                .saturating_duration_since(now)
                .div_f32(self.clock.playback.rate());
            self.deadline = Some(cx.spawn_in(w, async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update_in(cx, |s, w, cx| {
                    s.scheduled = None;
                    s.sync(w, cx);
                    s.advance(s.now(cx), w, cx);
                    s.schedule(w, cx);
                    cx.notify();
                });
            }));
        }
    }
    pub(super) fn promote(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.hidden() {
            return;
        }
        let now = self.now(cx);
        let mut ix = 0;
        while ix < self.waiting.len() {
            if self.waiting[ix].measured.is_none() {
                ix += 1;
                continue;
            }
            let r = self.waiting.remove(ix).unwrap();
            let g = r.generation;
            let timeout = r.spec.lifetime();
            if self.manager.is_empty() && self.options.position.side() {
                self.grow_up =
                    super::layer::prefer_grow_up(&self.options, r.measured.unwrap().0, w, cx);
            }
            self.manager.push(g, r, ToastOptions { timeout }, now);
        }
        // A new foreground can push the focused card beyond the visible budget.
        // Transfer focus before that old card's dispatch subtree is removed.
        if self.focus.contains_focused(w, cx) {
            let visible = self
                .manager
                .iter()
                .rev()
                .filter(|(_, _, phase)| *phase != ToastTransitionStatus::Ending)
                .take(self.options.visible)
                .collect::<Vec<_>>();
            if !visible
                .iter()
                .any(|(_, r, _)| r.focus.contains_focused(w, cx))
                && let Some((_, front, _)) = visible.first()
            {
                front.focus.focus(w, cx);
            }
        }
        self.advance(now, w, cx);
        self.schedule(w, cx);
    }
}
