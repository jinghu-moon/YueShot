use super::*;
use crate::tokens::{duration, easing, mix_rgba};
use gpui_kit::base::{Button, Transition, transition};

#[derive(Default)]
struct Feedback {
    hovered: bool,
}

/// Only presentation is local; Base owns activation, focus and disabled input.
#[derive(IntoElement)]
pub(super) struct TextAction {
    pub theme: YueTheme,
    pub label: SharedString,
    pub description: Option<SharedString>,
    pub icon: Option<Icon>,
    pub focus: FocusHandle,
    pub disabled: bool,
    pub current: bool,
    pub expanded: Option<bool>,
    pub handler: Option<ClickHandler>,
    pub max_width: DefiniteLength,
    pub overflow: Option<Decoration>,
}
impl RenderOnce for TextAction {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let enabled = self.handler.is_some() && !self.disabled;
        let feedback =
            window.use_keyed_state("breadcrumb-feedback", cx, |_, _| Feedback::default());
        if !enabled {
            feedback.update(cx, |f, _| f.hovered = false);
        }
        let hover = enabled && (feedback.read(cx).hovered || self.expanded == Some(true));
        let progress = transition(
            "breadcrumb-hover",
            if hover { 1. } else { 0. },
            Transition::new(duration(theme, keys::BREADCRUMB_DURATION))
                .ease(easing(theme, keys::BREADCRUMB_EASE)),
            window,
            cx,
        );
        let rest = rgba(
            theme,
            if self.disabled {
                keys::BREADCRUMB_COLOR_DISABLED
            } else if self.current {
                keys::BREADCRUMB_COLOR_CURRENT
            } else {
                keys::BREADCRUMB_COLOR
            },
        );
        let foreground = if enabled {
            mix_rgba(rest, rgba(theme, keys::BREADCRUMB_COLOR_HOVER), progress)
        } else {
            rest
        };
        let mut background = rgba(theme, keys::BREADCRUMB_BACKGROUND_HOVER);
        background.a *= progress;
        let mut content = div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(rems(theme, keys::BREADCRUMB_GAP));
        if let Some(icon) = self.icon {
            content = content.child(
                icon.size(rems(theme, keys::BREADCRUMB_ICON_SIZE))
                    .color_value(foreground),
            );
        }
        content = content.child(if let Some(decoration) = self.overflow {
            decoration_element(decoration, theme, foreground)
        } else {
            let mut text =
                crate::YueOverflowTooltip::new("breadcrumb-text", theme, self.label.clone())
                    .max_width(self.max_width);
            if enabled {
                text = text.track_focus(&self.focus);
            }
            text.into_any_element()
        });
        if self.handler.is_none() && !self.disabled {
            return div()
                .id("breadcrumb-action")
                .test_support()
                .relative()
                .flex()
                .items_center()
                .min_w_0()
                .min_h(rems(theme, keys::BREADCRUMB_HEIGHT))
                .text_color(foreground)
                .text_size(rems(theme, keys::BREADCRUMB_FONT_SIZE))
                .line_height(rems(theme, keys::BREADCRUMB_LINE_HEIGHT))
                .role(Role::Caption)
                .aria_label(self.label)
                .when_some(self.description, |d, text| d.aria_description(text))
                .child(content)
                .into_any_element();
        }
        let mut action = Button::new("breadcrumb-action")
            .track_focus(&self.focus)
            .disabled(self.disabled)
            .tab_stop(enabled)
            .accessibility_label(self.label)
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .min_h(rems(theme, keys::BREADCRUMB_HEIGHT))
            .rounded(rems(theme, keys::BREADCRUMB_BORDER_RADIUS))
            .text_color(foreground)
            .text_size(rems(theme, keys::BREADCRUMB_FONT_SIZE))
            .line_height(rems(theme, keys::BREADCRUMB_LINE_HEIGHT))
            .bg(background)
            .cursor_default()
            .when_some(self.description, |d, text| d.aria_description(text))
            .when_some(self.expanded, |d, expanded| d.aria_expanded(expanded))
            .on_hover(
                window.listener_for(&feedback, move |state, hovered, window, cx| {
                    let hovered = enabled && *hovered;
                    if state.hovered != hovered {
                        state.hovered = hovered;
                        cx.notify();
                        window.refresh();
                    }
                }),
            )
            .child(content);
        if let Some(handler) = self.handler {
            action = action.on_click(move |event, w, cx| handler(event, w, cx));
        } else {
            action = action.role(Role::Caption);
        }
        if enabled && self.focus.is_focused(window) && window.last_input_was_keyboard() {
            let outside = rems(theme, keys::BREADCRUMB_FOCUS_RING_WIDTH)
                + rems(theme, keys::BREADCRUMB_FOCUS_RING_OFFSET);
            action = action.child(
                div()
                    .absolute()
                    .top(-outside)
                    .bottom(-outside)
                    .left(-outside)
                    .right(-outside)
                    .rounded(rems(theme, keys::BREADCRUMB_BORDER_RADIUS))
                    .border(rems(theme, keys::BREADCRUMB_FOCUS_RING_WIDTH))
                    .border_color(rgba(theme, keys::BREADCRUMB_FOCUS_RING_COLOR)),
            );
        }
        action.into_any_element()
    }
}

pub(super) fn decoration_element(
    decoration: Decoration,
    theme: YueTheme,
    color: gpui_kit::Rgba,
) -> AnyElement {
    match decoration {
        Decoration::Text(text) => div().child(text).into_any_element(),
        Decoration::Icon(icon) => icon
            .size(rems(theme, keys::BREADCRUMB_ICON_SIZE))
            .color_value(color)
            .into_any_element(),
    }
}
