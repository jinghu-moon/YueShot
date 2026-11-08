use super::{
    DialogChange, DialogCloseReason, DialogDecision, DialogOpenError, DialogOutcome,
    YueDialogEvent,
    layer::{DialogLayer, Entry},
};
use gpui_kit::base::{ModalScope, active_modal_scope};
use gpui_kit::{
    App, Context, ElementId, Entity, EntityId, EventEmitter, FocusHandle, Subscription, WeakEntity,
    WeakFocusHandle, Window, WindowId,
};
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

pub(super) type ConfirmHandler =
    Rc<dyn Fn(&DialogSubmitRequest, &mut Window, &mut App) -> DialogDecision>;
pub(super) type CloseGuard = Rc<dyn Fn(DialogCloseReason, &mut Window, &mut App) -> bool>;
#[derive(Clone)]
pub(super) struct Behavior {
    pub escape: bool,
    pub overlay: bool,
    pub confirm_disabled: bool,
    pub initial_focus: Option<FocusHandle>,
    pub confirm: Option<ConfirmHandler>,
    pub guard: Option<CloseGuard>,
}
impl Default for Behavior {
    fn default() -> Self {
        Self {
            escape: true,
            overlay: true,
            confirm_disabled: false,
            initial_focus: None,
            confirm: None,
            guard: None,
        }
    }
}

/// One terminal capability for one submission. Completion never keeps its owner alive.
#[derive(Clone)]
pub struct DialogTicket {
    state: WeakEntity<YueDialogState>,
    window: WindowId,
    session: u64,
    attempt: u64,
    consumed: Rc<Cell<bool>>,
}
impl DialogTicket {
    pub fn complete(&self, outcome: DialogOutcome, w: &mut Window, cx: &mut App) -> bool {
        if self.window != w.window_handle().window_id() || self.consumed.replace(true) {
            return false;
        }
        self.state
            .update(cx, |s, cx| {
                if s.session != self.session || s.pending != Some(self.attempt) || !s.open {
                    return false;
                }
                s.pending = None;
                s.refresh(w, cx);
                if outcome == DialogOutcome::Close {
                    s.request_close(DialogCloseReason::Confirm, w, cx);
                }
                true
            })
            .unwrap_or(false)
    }
}
#[derive(Clone)]
pub struct DialogSubmitRequest {
    ticket: DialogTicket,
}
impl DialogSubmitRequest {
    pub fn ticket(&self) -> DialogTicket {
        self.ticket.clone()
    }
}

/// Retained modal behavior; the host owns its form data and asynchronous task.
pub struct YueDialogState {
    pub(super) focus: FocusHandle,
    pub(super) cancel_focus: FocusHandle,
    pub(super) behavior: Behavior,
    pub(super) open: bool,
    pub(super) visible: bool,
    pub(super) session: u64,
    pub(super) opened: bool,
    pub(super) scope: Option<ModalScope>,
    pub(super) drag: gpui_kit::base::ConstrainedDrag,
    pub(super) mount: Weak<()>,
    pub(super) mount_id: Option<ElementId>,
    pub(super) owner: Option<EntityId>,
    window: Option<WindowId>,
    pending: Option<u64>,
    attempt: u64,
    checking_close: bool,
    focus_pending: bool,
    close_reason: DialogCloseReason,
    previous: Option<WeakFocusHandle>,
    restore_from: Option<WeakFocusHandle>,
    _focus_guard: Option<Subscription>,
}
impl YueDialogState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            behavior: Behavior::default(),
            open: false,
            visible: false,
            session: 0,
            opened: false,
            scope: None,
            drag: Default::default(),
            mount: Weak::new(),
            mount_id: None,
            owner: None,
            window: None,
            pending: None,
            attempt: 0,
            checking_close: false,
            focus_pending: false,
            close_reason: DialogCloseReason::Programmatic,
            previous: None,
            restore_from: None,
            _focus_guard: None,
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn open(&mut self, w: &mut Window, cx: &mut Context<Self>) -> Result<(), DialogOpenError> {
        self.open_with(None, w, cx)
    }
    pub fn open_child(
        &mut self,
        parent: &Entity<Self>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), DialogOpenError> {
        if parent.entity_id() == cx.entity_id() || !parent.read(cx).is_open() {
            return Err(DialogOpenError::InvalidParent);
        }
        self.open_with(Some(parent.entity_id()), w, cx)
    }
    fn open_with(
        &mut self,
        parent: Option<EntityId>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), DialogOpenError> {
        let id = w.window_handle().window_id();
        if self.window.is_some_and(|own| own != id) {
            return Err(DialogOpenError::WrongWindow);
        }
        if self.mount.strong_count() == 0 {
            return Err(DialogOpenError::NotMounted);
        }
        let layer = DialogLayer::for_window(w, cx).ok_or(DialogOpenError::MissingRoot)?;
        if self.open {
            return Ok(());
        }
        let priority = layer.update(cx, |l, cx| l.priority(w, cx));
        let scope = ModalScope::new(cx.entity_id(), self.focus.clone(), priority);
        let state = cx.entity().downgrade();
        layer.update(cx, |l, cx| {
            l.admit(
                Entry {
                    state,
                    scope: scope.clone(),
                    lease: self.mount.clone(),
                },
                parent,
                w,
                cx,
            )
        })?;
        self.session += 1;
        self.pending = None;
        self.checking_close = false;
        self.opened = false;
        self.focus_pending = true;
        if !self.visible {
            self.previous = w.focused(cx).map(|f| f.downgrade());
            self.drag.reset();
        }
        self.scope = Some(scope);
        self.open = true;
        self.visible = true;
        let window = w.window_handle();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                crate::popup::PopupRuntime::for_window(w, cx).invalidate_modal(w, cx);
            });
        });
        cx.emit(YueDialogEvent::OpenChanged(DialogChange {
            open: true,
            reason: None,
        }));
        self.refresh(w, cx);
        Ok(())
    }
    pub(super) fn schedule_initial_focus(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_pending || !self.open {
            return;
        }
        self.focus_pending = false;
        let session = self.session;
        let weak = cx.entity().downgrade();
        // Schedule from the first visible render, not from open(): a native
        // frame callback runs before drawing, so open() still sees the old
        // dispatch tree and would incorrectly reject the new input handles.
        w.on_next_frame(move |w, cx| {
            let _ = weak.update(cx, |s, cx| {
                if s.open && s.session == session && s.is_top(w, cx) {
                    let focus = s
                        .behavior
                        .initial_focus
                        .as_ref()
                        .filter(|f| {
                            s.focus.contains(f, w) && gpui_kit::base::Root::is_focus_available(f, w)
                        })
                        .cloned()
                        .or_else(|| {
                            s.scope.as_ref().and_then(|scope| {
                                gpui_kit::base::preferred_modal_focus(scope, w, cx)
                            })
                        })
                        .unwrap_or_else(|| s.focus.clone());
                    focus.focus(w, cx);
                }
            });
        });
    }
    pub fn close(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        self.request_close(DialogCloseReason::Programmatic, w, cx)
    }
    pub fn cancel(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        self.request_close(DialogCloseReason::CancelButton, w, cx)
    }
    pub fn confirm(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.open
            || self.pending.is_some()
            || self.checking_close
            || self.behavior.confirm_disabled
            || !self.is_top(w, cx)
        {
            return false;
        }
        self.attempt += 1;
        let attempt = self.attempt;
        self.pending = Some(attempt);
        let ticket = DialogTicket {
            state: cx.entity().downgrade(),
            window: w.window_handle().window_id(),
            session: self.session,
            attempt,
            consumed: Rc::new(Cell::new(false)),
        };
        let handler = self.behavior.confirm.clone();
        let window = w.window_handle();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                if ticket.consumed.get() {
                    return;
                }
                let valid = ticket.state.upgrade().is_some_and(|s| {
                    let s = s.read(cx);
                    s.open && s.session == ticket.session && s.pending == Some(attempt)
                });
                if !valid {
                    return;
                }
                let decision = handler.map_or(DialogDecision::Close, |h| {
                    h(
                        &DialogSubmitRequest {
                            ticket: ticket.clone(),
                        },
                        w,
                        cx,
                    )
                });
                match decision {
                    DialogDecision::Close => {
                        ticket.complete(DialogOutcome::Close, w, cx);
                    }
                    DialogDecision::KeepOpen => {
                        ticket.complete(DialogOutcome::KeepOpen, w, cx);
                    }
                    DialogDecision::Pending => {}
                }
            });
        });
        self.refresh(w, cx);
        true
    }
    pub(super) fn request_close(
        &mut self,
        reason: DialogCloseReason,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.open
            || self.pending.is_some()
            || self.checking_close
            || !self.is_top(w, cx)
            || (reason == DialogCloseReason::Escape && !self.behavior.escape)
            || (reason == DialogCloseReason::OverlayPress && !self.behavior.overlay)
        {
            return false;
        }
        self.checking_close = true;
        let guard = self.behavior.guard.clone();
        let session = self.session;
        let weak = cx.entity().downgrade();
        let window = w.window_handle();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, w, cx| {
                let allowed = guard.is_none_or(|g| g(reason, w, cx));
                let _ = weak.update(cx, |s, cx| {
                    if s.session != session || !s.open {
                        return;
                    }
                    s.checking_close = false;
                    if allowed && s.is_top(w, cx) {
                        s.open = false;
                        s.restore_from = s
                            .focus
                            .contains_focused(w, cx)
                            .then(|| w.focused(cx).map(|f| f.downgrade()))
                            .flatten();
                        s.drag.cancel();
                        s.pending = None;
                        s.close_reason = reason;
                        let id = cx.entity_id();
                        if let Some(layer) = DialogLayer::for_window(w, cx) {
                            layer.update(cx, |l, cx| l.freeze(id, w, cx));
                        }
                        cx.emit(YueDialogEvent::OpenChanged(DialogChange {
                            open: false,
                            reason: Some(reason),
                        }));
                    }
                    s.refresh(w, cx);
                });
                if allowed {
                    crate::popup::PopupRuntime::for_window(w, cx).invalidate_modal(w, cx);
                }
            });
        });
        true
    }
    pub(super) fn is_top(&self, w: &Window, cx: &App) -> bool {
        self.scope
            .as_ref()
            .zip(active_modal_scope(w, cx))
            .is_some_and(|(s, a)| s.owner() == a.owner())
    }
    pub(super) fn mounted(
        &mut self,
        id: ElementId,
        lease: &Rc<()>,
        owner: EntityId,
        behavior: Behavior,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        assert!(
            self.mount
                .upgrade()
                .is_none_or(|existing| Rc::ptr_eq(&existing, lease)
                    && self.owner == Some(owner)
                    && self.mount_id.as_ref() == Some(&id)
                    && self.window == Some(w.window_handle().window_id())),
            "mount one dialog state in one window under one stable id"
        );
        self.mount = Rc::downgrade(lease);
        self.mount_id = Some(id);
        self.window = Some(w.window_handle().window_id());
        self.owner = Some(owner);
        self.behavior = behavior;
        if !self.open && self.visible && self.focus.contains_focused(w, cx) {
            self.restore_from = w.focused(cx).map(|f| f.downgrade());
        }
        if self._focus_guard.is_none() {
            self._focus_guard = Some(cx.on_focus_out(&self.focus.clone(), w, |s, _, w, cx| {
                if s.open && s.is_top(w, cx) && !s.focus.contains_focused(w, cx) {
                    s.focus.focus(w, cx);
                }
            }));
        }
    }
    pub(super) fn settled(
        &mut self,
        session: u64,
        present: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session != session || self.open != present || !self.visible {
            return;
        }
        if present {
            if !self.opened {
                self.opened = true;
                cx.emit(YueDialogEvent::Opened);
            }
        } else {
            self.visible = false;
            let id = cx.entity_id();
            if let Some(layer) = DialogLayer::for_window(w, cx) {
                layer.update(cx, |l, cx| l.retire(id, w, cx));
            }
            if self.focus.contains_focused(w, cx)
                || w.focused(cx).is_none()
                || self
                    .restore_from
                    .as_ref()
                    .is_some_and(|f| w.focused(cx).as_ref().is_some_and(|current| f == current))
            {
                if let Some(previous) = self
                    .previous
                    .take()
                    .and_then(|f| f.upgrade())
                    .filter(|f| gpui_kit::base::Root::is_focus_available(f, w))
                {
                    previous.focus(w, cx);
                } else if let Some(scope) = active_modal_scope(w, cx) {
                    scope.focus().focus(w, cx);
                    gpui_kit::base::Root::focus_next(w, cx);
                } else {
                    w.blur(cx);
                    gpui_kit::base::Root::focus_next(w, cx);
                }
            }
            self.restore_from = None;
            cx.emit(YueDialogEvent::Closed(DialogChange {
                open: false,
                reason: Some(self.close_reason),
            }));
            self.refresh(w, cx);
        }
    }
    pub(super) fn unmount(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.session += 1;
        self.open = false;
        self.visible = false;
        self.pending = None;
        self.drag.cancel();
        let id = cx.entity_id();
        if let Some(layer) = DialogLayer::for_window(w, cx) {
            layer.update(cx, |l, cx| l.retire(id, w, cx));
        }
        self._focus_guard = None;
        if self.focus.contains_focused(w, cx)
            || w.focused(cx).is_none()
            || w.focused(cx)
                .is_some_and(|f| !gpui_kit::base::Root::is_focus_available(&f, w))
        {
            if let Some(previous) = self
                .previous
                .take()
                .and_then(|f| f.upgrade())
                .filter(|f| gpui_kit::base::Root::is_focus_available(f, w))
            {
                previous.focus(w, cx);
            } else {
                w.blur(cx);
                gpui_kit::base::Root::focus_next(w, cx);
            }
        }
        w.refresh();
    }
    pub(super) fn refresh(&self, w: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
        if let Some(owner) = self.owner {
            App::notify(cx, owner);
        }
        w.refresh();
    }
}
impl EventEmitter<YueDialogEvent> for YueDialogState {}
