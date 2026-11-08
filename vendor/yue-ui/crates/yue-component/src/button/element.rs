//! Element composition and component-local presentation.

use super::*;

use gpui_kit::{
    AnyElement, App, Axis, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    RenderOnce, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _,
};

use crate::tokens::{
    duration as token_duration, easing as token_ease, mix_rgba, number as token_number,
    rems as token_rems, rgba as token_rgba, token,
};

use crate::YueTheme;

use yue_component_icons::Icon;

pub(super) fn token_color(theme: YueTheme, name: impl crate::theme::TokenSource) -> u32 {
    token(theme, name)
        .color()
        .unwrap_or_else(|| panic!("Yue token {name} is not an opaque color"))
}

/// The share of `color-mix(in srgb, currentColor calc(N*100%), transparent)`.
fn current_color_wash(value: &str) -> Option<f32> {
    let amount = value
        .strip_prefix("color-mix(in srgb, currentColor calc(")?
        .strip_suffix("*100%), transparent)")?
        .parse::<f32>()
        .ok()?;
    (0.0..=1.0).contains(&amount).then_some(amount)
}

impl RenderOnce for YueButton {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let accessibility_label =
            self.accessibility_label
                .clone()
                .unwrap_or_else(|| match &self.content {
                    ButtonContent::Text(label) | ButtonContent::IconText { label, .. } => {
                        label.clone()
                    }
                    ButtonContent::Icon {
                        accessibility_label,
                        ..
                    } => accessibility_label.clone(),
                });
        let icon_only =
            matches!(&self.content, ButtonContent::Icon { .. }) && !self.dropdown_indicator;
        let (background_token, hover_token, foreground_token, border_token) = match self.variant {
            ButtonVariant::Primary => (
                crate::theme::keys::BUTTON_PRIMARY_BACKGROUND,
                crate::theme::keys::BUTTON_PRIMARY_BACKGROUND_HOVER,
                crate::theme::keys::BUTTON_PRIMARY_COLOR,
                crate::theme::keys::BUTTON_PRIMARY_BORDER_COLOR,
            ),
            ButtonVariant::Default => (
                crate::theme::keys::BUTTON_DEFAULT_BACKGROUND,
                crate::theme::keys::BUTTON_DEFAULT_BACKGROUND_HOVER,
                crate::theme::keys::BUTTON_DEFAULT_COLOR,
                crate::theme::keys::BUTTON_DEFAULT_BORDER_COLOR,
            ),
            ButtonVariant::Outline => (
                crate::theme::keys::BUTTON_OUTLINE_BACKGROUND,
                crate::theme::keys::BUTTON_SECONDARY_BACKGROUND_HOVER,
                crate::theme::keys::BUTTON_SECONDARY_COLOR,
                crate::theme::keys::BUTTON_SECONDARY_BORDER_COLOR,
            ),
            ButtonVariant::Ghost => (
                crate::theme::keys::BUTTON_GHOST_BACKGROUND,
                crate::theme::keys::BUTTON_GHOST_BACKGROUND_HOVER,
                crate::theme::keys::BUTTON_GHOST_COLOR,
                crate::theme::keys::BUTTON_GHOST_BORDER_COLOR,
            ),
            ButtonVariant::Text => (
                crate::theme::keys::BUTTON_SUBTLE_BACKGROUND,
                crate::theme::keys::BUTTON_SUBTLE_BACKGROUND_HOVER,
                crate::theme::keys::BUTTON_LINK_COLOR,
                crate::theme::keys::BUTTON_GHOST_BORDER_COLOR,
            ),
            ButtonVariant::Danger => (
                crate::theme::keys::BUTTON_DANGER_BACKGROUND,
                crate::theme::keys::BUTTON_DANGER_BACKGROUND_HOVER,
                crate::theme::keys::BUTTON_DANGER_COLOR,
                crate::theme::keys::BUTTON_DANGER_BORDER_COLOR,
            ),
        };
        let foreground = token_color(self.theme, foreground_token);
        let background = token_rgba(self.theme, background_token);
        let hover = token(self.theme, hover_token);
        // Only a `currentColor` wash is left for the button to resolve, since
        // the label colour is its own; any other unreadable value is a broken
        // token and must not quietly become a tint.
        let hover_background = hover.rgba().unwrap_or_else(|| {
            let amount = current_color_wash(hover.value())
                .unwrap_or_else(|| panic!("Yue token {} is not a supported color", hover.name()));
            mix_rgba(gpui_kit::rgba(0), gpui_kit::rgb(foreground), amount)
        });
        let border = if self.disabled || self.loading {
            token_rgba(self.theme, crate::theme::keys::BUTTON_DISABLED_BORDER_COLOR)
        } else {
            token_rgba(self.theme, border_token)
        };
        let button_duration = token_duration(self.theme, crate::theme::keys::BUTTON_DURATION);
        let content_color = if self.disabled || self.loading {
            token_rgba(self.theme, crate::theme::keys::BUTTON_DISABLED_COLOR)
        } else {
            gpui_kit::rgb(foreground)
        };
        let disabled_background =
            token_rgba(self.theme, crate::theme::keys::BUTTON_DISABLED_BACKGROUND);
        let disabled = self.disabled || self.loading;
        let focus_handle = self.focus_handle.clone().unwrap_or_else(|| {
            window
                .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone()
        });
        let ring_visible =
            !disabled && focus_handle.is_focused(window) && window.last_input_was_keyboard();
        let (hovered, pressed, motion_state) = if disabled {
            (false, false, None)
        } else {
            let state =
                window.use_keyed_state((self.id.clone(), "yue-button-motion"), cx, |_, _| {
                    ButtonMotion::default()
                });
            let current = state.read(cx);
            (current.hovered, current.pressed, Some(state))
        };
        let hover_progress = gpui_kit::base::transition(
            (self.id.clone(), "yue-button-hover"),
            if hovered { 1.0 } else { 0.0 },
            gpui_kit::base::Transition::new(button_duration)
                .ease(token_ease(self.theme, crate::theme::keys::BUTTON_EASE)),
            window,
            cx,
        );
        let press_progress = gpui_kit::base::transition(
            (self.id.clone(), "yue-button-press"),
            if pressed { 1.0 } else { 0.0 },
            gpui_kit::base::Transition::new(button_duration)
                .ease(token_ease(self.theme, crate::theme::keys::BUTTON_EASE)),
            window,
            cx,
        );
        let animated_background = mix_rgba(background, hover_background, hover_progress);
        let press_inset =
            token_rems(self.theme, crate::theme::keys::BUTTON_BORDER_WIDTH) * press_progress;
        // A disclosure's chevron turns half a circle towards the panel it opened,
        // on the same clock the button's own hover and press ramps use. Only the
        // icon that faces the disclosure turns: a leading icon belongs to the
        // action, not to the panel.
        let expand_turn = self
            .expanded
            .filter(|_| {
                self.dropdown_indicator || icon_only || self.icon_position == IconPosition::Trailing
            })
            .map(|open| {
                gpui_kit::base::transition(
                    (self.id.clone(), "yue-button-expand"),
                    if open { 0.5 } else { 0.0 },
                    gpui_kit::base::Transition::new(button_duration)
                        .ease(token_ease(self.theme, crate::theme::keys::BUTTON_EASE)),
                    window,
                    cx,
                )
            });
        let expand = |icon: Icon| -> Icon {
            match expand_turn {
                Some(turn) => icon.rotation(turn),
                None => icon,
            }
        };

        let mut button = gpui_kit::base::Button::new(self.id.clone())
            .track_focus(&focus_handle)
            .disabled(disabled)
            .accessibility_label(accessibility_label)
            .flex()
            .items_center()
            .justify_center()
            .text_size(token_rems(
                self.theme,
                match self.size {
                    ButtonSize::Small => crate::theme::keys::BUTTON_FONT_SIZE_SM,
                    ButtonSize::Medium => crate::theme::keys::BUTTON_FONT_SIZE_MD,
                    ButtonSize::Large => crate::theme::keys::BUTTON_FONT_SIZE_LG,
                },
            ))
            .line_height(token_rems(
                self.theme,
                crate::theme::keys::BUTTON_LINE_HEIGHT,
            ))
            .font_weight(gpui_kit::FontWeight(token_number(
                self.theme,
                crate::theme::keys::BUTTON_FONT_WEIGHT,
            )))
            .bg(gpui_kit::transparent_black())
            .border(token_rems(
                self.theme,
                crate::theme::keys::BUTTON_BORDER_WIDTH,
            ))
            .border_color(gpui_kit::transparent_black())
            .text_color(content_color)
            .px(token_rems(
                self.theme,
                crate::theme::keys::BUTTON_PADDING_INLINE_FLUSH,
            ))
            .when(!disabled, |button| button.cursor_pointer());

        if let Some(expanded) = self.expanded {
            button = button.aria_expanded(expanded);
        }

        if let Some(state) = motion_state {
            let hover = window.listener_for(&state, |state, hovered, window, cx| {
                if state.hovered != *hovered {
                    state.hovered = *hovered;
                    cx.notify();
                    window.refresh();
                }
            });
            let down = window.listener_for(&state, |state, _, window, cx| {
                if !state.pressed {
                    state.pressed = true;
                    cx.notify();
                    window.refresh();
                }
            });
            let up = window.listener_for(&state, |state, _, window, cx| {
                if state.pressed {
                    state.pressed = false;
                    cx.notify();
                    window.refresh();
                }
            });
            let up_out = window.listener_for(&state, |state, _, window, cx| {
                if state.pressed {
                    state.pressed = false;
                    state.hovered = false;
                    cx.notify();
                    window.refresh();
                }
            });
            button = button
                .on_hover(hover)
                .on_mouse_down(MouseButton::Left, down)
                .on_mouse_up(MouseButton::Left, up)
                .on_mouse_up_out(MouseButton::Left, up_out);
        }

        let (height_token, padding_token, icon_token) = match self.size {
            ButtonSize::Small => (
                crate::theme::keys::BUTTON_HEIGHT_SM,
                crate::theme::keys::BUTTON_PADDING_INLINE_SM,
                crate::theme::keys::BUTTON_ICON_SIZE_SM,
            ),
            ButtonSize::Medium => (
                crate::theme::keys::BUTTON_HEIGHT_MD,
                crate::theme::keys::BUTTON_PADDING_INLINE_MD,
                crate::theme::keys::BUTTON_ICON_SIZE_MD,
            ),
            ButtonSize::Large => (
                crate::theme::keys::BUTTON_HEIGHT_LG,
                crate::theme::keys::BUTTON_PADDING_INLINE_LG,
                crate::theme::keys::BUTTON_ICON_SIZE_LG,
            ),
        };
        let height = token_rems(self.theme, height_token);
        let horizontal_padding = token_rems(self.theme, padding_token);
        let icon_size = self
            .icon_size_override
            .unwrap_or_else(|| token_rems(self.theme, icon_token));
        let button_gap = token_rems(self.theme, crate::theme::keys::BUTTON_GAP);
        button = button.h(height).when(self.block, |button| button.w_full());
        if icon_only {
            button = button.w(height);
        }

        let surface = div()
            .absolute()
            .top(press_inset)
            .right(press_inset)
            .bottom(press_inset)
            .left(press_inset)
            .bg(if disabled {
                disabled_background
            } else {
                animated_background
            })
            .border(token_rems(
                self.theme,
                crate::theme::keys::BUTTON_BORDER_WIDTH,
            ))
            .border_color(border);
        let surface = match self.shape {
            ButtonShape::RoundedRectangle => surface.rounded(token_rems(
                self.theme,
                crate::theme::keys::BUTTON_BORDER_RADIUS,
            )),
            ButtonShape::RoundedSquare => {
                surface.rounded(token_rems(self.theme, crate::theme::keys::RADIUS_MD))
            }
            ButtonShape::Pill => surface.rounded(token_rems(
                self.theme,
                crate::theme::keys::BUTTON_BORDER_RADIUS_FULL,
            )),
            ButtonShape::Circle => surface.rounded(token_rems(
                self.theme,
                crate::theme::keys::BUTTON_BORDER_RADIUS_FULL,
            )),
        };
        let render_content = || -> AnyElement {
            match &self.content {
                ButtonContent::Text(label) => div()
                    .text_color(content_color)
                    .child(label.clone())
                    .into_any_element(),
                ButtonContent::Icon { icon, .. } => {
                    let icon = icon.size(icon_size).color_value(content_color);
                    if self.dropdown_indicator {
                        div()
                            .flex()
                            .items_center()
                            .gap(button_gap)
                            .child(icon)
                            .child(expand(
                                Icon::new(crate::outline::ChevronDown)
                                    .size(icon_size * 0.75)
                                    .color_value(content_color),
                            ))
                            .into_any_element()
                    } else {
                        div().child(expand(icon)).into_any_element()
                    }
                }
                ButtonContent::IconText { icon, label } => {
                    let icon = expand(icon.size(icon_size).color_value(content_color));
                    let row = div()
                        .flex()
                        .items_center()
                        .gap(button_gap)
                        .text_color(content_color);
                    match self.icon_position {
                        IconPosition::Leading => {
                            row.child(icon).child(label.clone()).into_any_element()
                        }
                        IconPosition::Trailing => {
                            row.child(label.clone()).child(icon).into_any_element()
                        }
                    }
                }
            }
        };
        let sized_content = div()
            .child(render_content())
            .when(!icon_only, |content| content.px(horizontal_padding))
            .opacity(0.0);
        let visible_content = if self.loading {
            crate::loading::spinner_glyph(
                self.theme,
                icon_size,
                token_rems(self.theme, crate::theme::keys::SPINNER_BORDER_WIDTH),
                content_color,
                token_duration(self.theme, crate::theme::keys::BUTTON_SPINNER_DURATION),
                true,
            )
        } else {
            render_content()
        };
        let content = div()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .items_center()
            .justify_center()
            .gap(button_gap)
            .text_size(token_rems(
                self.theme,
                match self.size {
                    ButtonSize::Small => crate::theme::keys::BUTTON_FONT_SIZE_SM,
                    ButtonSize::Medium => crate::theme::keys::BUTTON_FONT_SIZE_MD,
                    ButtonSize::Large => crate::theme::keys::BUTTON_FONT_SIZE_LG,
                },
            ))
            .text_color(content_color)
            .child(visible_content);
        button = button.child(sized_content).child(surface).child(content);
        // Focus is paint and never changes intrinsic width or sibling positions.
        if ring_visible {
            let radius = match self.shape {
                ButtonShape::RoundedRectangle => crate::theme::keys::BUTTON_BORDER_RADIUS,
                ButtonShape::RoundedSquare => crate::theme::keys::RADIUS_MD,
                ButtonShape::Pill | ButtonShape::Circle => {
                    crate::theme::keys::BUTTON_BORDER_RADIUS_FULL
                }
            };
            let outer_border = token_rems(self.theme, crate::theme::keys::BUTTON_BORDER_WIDTH);
            button = button.child(
                div()
                    .absolute()
                    .top(-outer_border)
                    .right(-outer_border)
                    .bottom(-outer_border)
                    .left(-outer_border)
                    .border(token_rems(
                        self.theme,
                        crate::theme::keys::BUTTON_FOCUS_RING_WIDTH,
                    ))
                    .border_color(token_rgba(
                        self.theme,
                        crate::theme::keys::BUTTON_FOCUS_RING_COLOR,
                    ))
                    .rounded(token_rems(self.theme, radius)),
            );
        }
        if let Some(handler) = self.on_click {
            button = button.on_click(move |event, window, cx| handler(event, window, cx));
        }
        if let Some(text) = self.tooltip {
            if self.description.is_none() && !text.trim().is_empty() {
                button = button.aria_description(text.clone());
            }
            let attachment = crate::tooltip::attach(
                self.id.clone(),
                crate::tooltip::Config::new(text, self.theme, self.tooltip_options)
                    .disabled(disabled),
                Some(focus_handle.clone()),
                window,
                cx,
            );
            button = button.relative().child(attachment);
        }
        button = self
            .layout
            .apply(button)
            .tab_index(self.tab_index)
            .tab_stop(self.tab_stop);
        if let Some(context) = self.key_context {
            button = button.key_context(context.as_ref());
        }
        if let Some(description) = self.description {
            button = button.aria_description(description);
        }
        button
    }
}

#[derive(Default)]
struct ButtonMotion {
    hovered: bool,
    pressed: bool,
}

impl RenderOnce for ButtonGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .when(self.axis == Axis::Vertical, |group| group.flex_col())
            .gap(self.gap)
            .children(self.children)
    }
}
