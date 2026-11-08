use super::{DialogOpenError, YueDialogState};
use gpui_kit::base::{ModalBoundary, ModalScope, Root, RootPlugin, set_active_modal_scope};
use gpui_kit::{
    AnyElement, App, Context, Entity, EntityId, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, WeakEntity, Window, div,
};
use std::rc::Weak;

pub(super) struct Entry {
    pub state: WeakEntity<YueDialogState>,
    pub scope: ModalScope,
    pub lease: Weak<()>,
}
pub(super) struct DialogLayer {
    stack: Vec<Entry>,
}
impl DialogLayer {
    pub fn for_window(window: &Window, cx: &App) -> Option<Entity<Self>> {
        window.root::<Root>().flatten()?.read(cx).plugin::<Self>()
    }
    pub fn is_top(&self, id: EntityId) -> bool {
        self.stack.last().is_some_and(|e| e.scope.owner() == id)
    }
    pub fn has_parent(&self) -> bool {
        self.stack.len() > 1
    }
    pub fn priority(&self, w: &Window, cx: &mut App) -> usize {
        let modal =
            crate::tokens::number(crate::current_theme(cx), crate::theme::keys::DIALOG_Z_INDEX)
                as usize;
        modal
            .max(self.stack.last().map_or(0, |e| e.scope.priority() + 1))
            .max(crate::popup::PopupRuntime::for_window(w, cx).highest_priority() + 1)
    }
    pub fn admit(
        &mut self,
        entry: Entry,
        parent: Option<EntityId>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), DialogOpenError> {
        let id = entry.scope.owner();
        if self.is_top(id) {
            self.stack.pop();
        } else if let Some(parent) = parent {
            if !self.is_top(parent) {
                return Err(DialogOpenError::InvalidParent);
            }
        } else if !self.stack.is_empty() {
            return Err(DialogOpenError::Occupied);
        }
        self.stack.push(entry);
        self.sync(w, cx);
        cx.notify();
        Ok(())
    }
    pub fn retire(&mut self, id: EntityId, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.stack.iter().position(|e| e.scope.owner() == id) {
            let descendants: Vec<_> = self.stack.drain(ix..).skip(1).map(|e| e.state).collect();
            let window = w.window_handle();
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, w, cx| {
                    for descendant in descendants {
                        let _ = descendant.update(cx, |s, cx| s.unmount(w, cx));
                    }
                });
            });
        }
        self.sync(w, cx);
        cx.notify();
    }
    pub fn freeze(&mut self, id: EntityId, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.stack.last_mut().filter(|e| e.scope.owner() == id) {
            entry.scope = entry.scope.clone().interactive(false);
        }
        self.sync(w, cx);
    }
    fn sync(&self, w: &mut Window, cx: &mut App) {
        set_active_modal_scope(self.stack.last().map(|e| e.scope.clone()), w, cx);
        w.refresh();
    }
}
impl Render for DialogLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
impl RootPlugin for DialogLayer {
    fn prepare(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self
            .stack
            .iter()
            .position(|e| e.lease.strong_count() == 0 || e.state.upgrade().is_none())
        {
            let id = self.stack[ix].scope.owner();
            self.retire(id, w, cx);
        }
        set_active_modal_scope(self.stack.last().map(|e| e.scope.clone()), w, cx);
    }
    fn decorate(
        &self,
        surface: AnyElement,
        _: &Root,
        _: &mut Window,
        _: &mut App,
    ) -> impl IntoElement {
        // Deferred semantic surfaces escape this background boundary. They keep
        // their original input dispatch ancestry, not background accessibility.
        ModalBoundary::background(div().id("modal-background").size_full().child(surface))
    }
}
pub(super) fn init(cx: &mut App) {
    Root::register_plugin(cx, |w, cx| {
        let handle = w.window_handle();
        cx.on_release(move |_, cx| {
            let _ = handle.update(cx, |_, w, cx| gpui_kit::base::release_modal_window(w, cx));
        })
        .detach();
        DialogLayer { stack: vec![] }
    });
}
