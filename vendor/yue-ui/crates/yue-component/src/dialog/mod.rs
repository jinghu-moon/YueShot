//! Modal dialog contract and builders.

mod element;

use crate::YueTheme;

use gpui_kit::{
    AnyElement, App, DefiniteLength, ElementId, Entity, FocusHandle, IntoElement, ParentElement,
    SharedString, Window,
};

use std::rc::Rc;

mod layer;

mod model;

mod state;

pub use model::*;

pub use state::{DialogSubmitRequest, DialogTicket, YueDialogState};

pub(crate) fn init(cx: &mut App) {
    layer::init(cx);
}

/// Keep this declaration mounted even while closed; retain its state in the host.
///
/// ```no_run
/// use yue_component::{YueDialog, YueDialogState, YueTheme, ThemeMode, Accent};
/// use yue_component::gpui_kit::{AppContext as _, Context};
/// fn declaration(cx: &mut Context<()>) {
///     let state = cx.new(YueDialogState::new);
///     let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
///     let _dialog = YueDialog::new("save", theme, &state)
///         .title("保存项目").confirm_label("保存");
/// }
/// ```
#[derive(IntoElement)]
pub struct YueDialog {
    id: ElementId,
    theme: YueTheme,
    inherited: bool,
    state: Entity<YueDialogState>,
    title: SharedString,
    label: Option<SharedString>,
    description: Option<SharedString>,
    header: Option<AnyElement>,
    drag_handle: Option<AnyElement>,
    header_visible: bool,
    footer: DialogFooter,
    footer_content: Option<AnyElement>,
    children: Vec<AnyElement>,
    confirm_label: SharedString,
    cancel_label: SharedString,
    close_label: SharedString,
    closable: bool,
    overlay: bool,
    alert: bool,
    enter: Option<bool>,
    size: DialogSize,
    width: Option<DefiniteLength>,
    status: Option<DialogStatus>,
    draggable: bool,
    behavior: state::Behavior,
}

impl YueDialog {
    pub fn new(id: impl Into<ElementId>, theme: YueTheme, state: &Entity<YueDialogState>) -> Self {
        Self {
            id: id.into(),
            theme,
            inherited: false,
            state: state.clone(),
            title: "".into(),
            label: None,
            description: None,
            header: None,
            drag_handle: None,
            header_visible: true,
            footer: DialogFooter::Standard,
            footer_content: None,
            children: vec![],
            confirm_label: "OK".into(),
            cancel_label: "Cancel".into(),
            close_label: "Close".into(),
            closable: true,
            overlay: true,
            alert: false,
            enter: None,
            size: DialogSize::default(),
            width: None,
            status: None,
            draggable: false,
            behavior: state::Behavior::default(),
        }
    }
    pub fn alert(
        id: impl Into<ElementId>,
        theme: YueTheme,
        state: &Entity<YueDialogState>,
    ) -> Self {
        let mut result = Self::new(id, theme, state);
        result.alert = true;
        result.behavior.overlay = false;
        result
    }
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }
    pub fn header(mut self, content: impl IntoElement) -> Self {
        self.header = Some(content.into_any_element());
        self
    }
    pub fn header_visible(mut self, value: bool) -> Self {
        self.header_visible = value;
        self
    }
    /// An explicit, non-interactive drag region beside a custom header.
    pub fn drag_handle(mut self, content: impl IntoElement) -> Self {
        self.drag_handle = Some(content.into_any_element());
        self
    }
    pub fn footer(mut self, value: DialogFooter) -> Self {
        self.footer = value;
        self
    }
    pub fn footer_content(mut self, content: impl IntoElement) -> Self {
        self.footer_content = Some(content.into_any_element());
        self
    }
    pub fn confirm_label(mut self, text: impl Into<SharedString>) -> Self {
        self.confirm_label = text.into();
        self
    }
    pub fn cancel_label(mut self, text: impl Into<SharedString>) -> Self {
        self.cancel_label = text.into();
        self
    }
    pub fn close_label(mut self, text: impl Into<SharedString>) -> Self {
        self.close_label = text.into();
        self
    }
    pub fn closable(mut self, value: bool) -> Self {
        self.closable = value;
        self
    }
    pub fn overlay(mut self, value: bool) -> Self {
        self.overlay = value;
        self
    }
    pub fn close_on_escape(mut self, value: bool) -> Self {
        self.behavior.escape = value;
        self
    }
    pub fn close_on_overlay_click(mut self, value: bool) -> Self {
        self.behavior.overlay = value;
        self
    }
    pub fn confirm_disabled(mut self, value: bool) -> Self {
        self.behavior.confirm_disabled = value;
        self
    }
    pub fn confirm_on_enter(mut self, value: bool) -> Self {
        self.enter = Some(value);
        self
    }
    pub fn size(mut self, value: DialogSize) -> Self {
        self.size = value;
        self
    }
    pub fn width(mut self, value: impl Into<DefiniteLength>) -> Self {
        self.width = Some(value.into());
        self
    }
    pub fn status(mut self, value: DialogStatus) -> Self {
        self.status = Some(value);
        self
    }
    pub fn draggable(mut self, value: bool) -> Self {
        self.draggable = value;
        self
    }
    pub fn initial_focus(mut self, focus: &FocusHandle) -> Self {
        self.behavior.initial_focus = Some(focus.clone());
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.inherited = false;
        self
    }
    pub fn use_default_theme(mut self) -> Self {
        self.inherited = true;
        self
    }
    pub fn on_confirm(
        mut self,
        handler: impl Fn(&DialogSubmitRequest, &mut Window, &mut App) -> DialogDecision + 'static,
    ) -> Self {
        self.behavior.confirm = Some(Rc::new(handler));
        self
    }
    pub fn on_close_request(
        mut self,
        handler: impl Fn(DialogCloseReason, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.behavior.guard = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for YueDialog {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}

#[cfg(all(test, feature = "headless-tests"))]
mod tests;
