//! Menu triggers, context regions and split-button composition.

use super::host::{HostSpec, MenuHost};
use super::*;

use std::rc::Rc;

use gpui_kit::{App, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div};

use crate::icon::YueIcon;

use crate::YueButton;

impl RenderOnce for YueDropdownMenu {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.shape.theme = crate::current_theme(cx);
        }
        let host = MenuHost::new(
            self.id,
            HostSpec {
                state: self.state,
                kind: MenuKind::Trigger,
                shape: self.shape,
                name: self.name,
                default_open: self.default_open,
                on_open_change: self.on_open_change,
            },
            &self.items,
            window,
            cx,
        );
        let (trigger, toggled) = match self.trigger {
            TriggerSpec::Action(build) => (
                build(host.is_open(), Rc::new(host.toggle()), window, cx),
                true,
            ),
            TriggerSpec::Button(button) => {
                let toggle = host.toggle();
                (
                    (*button)
                        .expanded(host.is_open())
                        .on_click(move |event, window, cx| toggle(event, window, cx))
                        .into_any_element(),
                    true,
                )
            }
            TriggerSpec::Custom(build) => (build(host.is_open(), window, cx), false),
        };
        host.assemble(trigger, toggled, self.items, window, cx)
    }
}

impl RenderOnce for YueContextMenu {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.shape.theme = crate::current_theme(cx);
        }
        let host = MenuHost::new(
            self.id,
            HostSpec {
                kind: MenuKind::Context,
                state: None,
                shape: self.shape,
                name: self.name,
                default_open: false,
                on_open_change: self.on_open_change,
            },
            &self.items,
            window,
            cx,
        );
        host.region(self.children, self.items, window, cx)
    }
}

impl RenderOnce for YueDropdownSplitButton {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        self.shape.theme = self.theme;
        let disabled = self.disabled;
        let arrow = YueButton::icon(
            (self.id.clone(), "yue-dropdown-arrow"),
            self.theme,
            YueIcon::ChevronDown,
            self.arrow_label.clone(),
        )
        .disabled(disabled);
        let menu = YueDropdownMenu {
            state: None,
            use_default_theme: false,
            id: self.id,
            shape: self.shape,
            name: self.name,
            trigger: TriggerSpec::Button(Box::new(arrow)),
            items: self.items,
            default_open: self.default_open,
            on_open_change: self.on_open_change,
        };
        // An inert arrow is also an inert shell: gpui's button blocks the
        // activation that would otherwise reach the host's own click handler.
        let action = self.action.theme(self.theme).disabled(disabled);
        div()
            .flex()
            .items_start()
            .child(action)
            .child(menu)
            .into_any_element()
    }
}
