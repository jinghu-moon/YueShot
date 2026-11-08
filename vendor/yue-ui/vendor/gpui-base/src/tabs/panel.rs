use super::TabsController;
use crate::TestSupportExt as _;
use gpui::{
    AnyElement, App, Div, ElementId, Entity, InteractiveElement, Interactivity, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, Window, div,
};

/// A panel activation boundary. Inactive descendants are not attached to the
/// layout/dispatch/accessibility tree; opacity is never used as an input gate.
#[derive(IntoElement)]
pub struct TabPanel {
    base: Stateful<Div>,
    controller: Entity<TabsController>,
    key: SharedString,
    label: SharedString,
    content: Option<AnyElement>,
    tab_stop: bool,
}
impl TabPanel {
    pub fn new(
        id: impl Into<ElementId>,
        controller: &Entity<TabsController>,
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            base: div().id(id.into()),
            controller: controller.clone(),
            key: key.into(),
            label: label.into(),
            content: None,
            tab_stop: true,
        }
    }
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
    /// Disable the panel's own stop when its supplied contents already have a
    /// natural entry point. Descendant focus is otherwise unchanged.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
}
impl gpui::Styled for TabPanel {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
    }
}
impl InteractiveElement for TabPanel {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}
impl gpui::StatefulInteractiveElement for TabPanel {}
impl RenderOnce for TabPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.controller.read(cx);
        if state.selected_key() != Some(&self.key) {
            return self.base.into_any_element();
        }
        self.base
            .test_support()
            .role(Role::TabPanel)
            .aria_label(self.label)
            .track_focus(&state.panel_focus().tab_stop(self.tab_stop))
            .min_w_0()
            .min_h_0()
            .children(self.content)
            .into_any_element()
    }
}
