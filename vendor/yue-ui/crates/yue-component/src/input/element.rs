//! Single-line input element composition.

use super::frame::*;
use super::*;

use gpui_kit::base::TestSupportExt as _;

use gpui_kit::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, Styled as _,
    TextAlign, Window, div, prelude::FluentBuilder as _,
};

use crate::{ButtonShape, ButtonSize, ButtonVariant, YueButton};

use yue_component_icons::Icon;

use crate::icon::YueIcon;

use crate::tokens::{rems, rgba as color};

impl RenderOnce for YueInput {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        use gpui_kit::base::input::InputEditorStyle;

        let (height, padding, font_size, icon_size) = match self.size {
            InputSize::Small => (
                crate::theme::keys::INPUT_HEIGHT_SM,
                crate::theme::keys::INPUT_PADDING_INLINE_SM,
                crate::theme::keys::INPUT_FONT_SIZE_SM,
                crate::theme::keys::INPUT_ICON_SIZE_SM,
            ),
            InputSize::Medium => (
                crate::theme::keys::INPUT_HEIGHT_MD,
                crate::theme::keys::INPUT_PADDING_INLINE_MD,
                crate::theme::keys::INPUT_FONT_SIZE_MD,
                crate::theme::keys::INPUT_ICON_SIZE_MD,
            ),
            InputSize::Large => (
                crate::theme::keys::INPUT_HEIGHT_LG,
                crate::theme::keys::INPUT_PADDING_INLINE_LG,
                crate::theme::keys::INPUT_FONT_SIZE_LG,
                crate::theme::keys::INPUT_ICON_SIZE_LG,
            ),
        };
        let visibility = self.password.then(|| {
            window.use_keyed_state(
                (self.id.clone(), "yue-input-password-visible"),
                cx,
                |_, _| false,
            )
        });
        let password_visible = visibility.as_ref().is_some_and(|state| *state.read(cx));
        let state = self.state.clone();
        state.update(cx, |input, cx| {
            input.set_disabled(self.disabled, cx);
            input.set_readonly(self.readonly, cx);
            input.set_text_align(
                match self.text_align {
                    InputTextAlign::Left => TextAlign::Left,
                    InputTextAlign::Center => TextAlign::Center,
                    InputTextAlign::Right => TextAlign::Right,
                },
                cx,
            );
            input.set_editor_style(InputEditorStyle {
                foreground: color(
                    self.theme,
                    if self.disabled {
                        crate::theme::keys::INPUT_COLOR_DISABLED
                    } else {
                        crate::theme::keys::INPUT_COLOR
                    },
                )
                .into(),
                muted_foreground: color(self.theme, crate::theme::keys::INPUT_PLACEHOLDER_COLOR)
                    .into(),
                caret: color(self.theme, crate::theme::keys::INPUT_COLOR).into(),
                selection: self.theme.selection(),
                ..Default::default()
            });
            if let Some(placeholder) = &self.placeholder
                && input.presentation().placeholder() != placeholder
            {
                input.set_placeholder(placeholder.clone(), window, cx);
            }
            let should_mask = self.password && !password_visible;
            if input.presentation().is_masked() != should_mask {
                input.set_masked(should_mask, window, cx);
            }
        });
        if let Some(limit) = self.max_length {
            truncate_to_limit(&state, limit, window, cx);
        }

        let presentation = state.read(cx).presentation();
        let focused = presentation.focus_handle().is_focused(window);
        let masked = presentation.is_masked();
        let border = border_token(self.status, self.disabled, focused);
        let height = rems(self.theme, height);
        let pad = rems(self.theme, padding);
        let gap = rems(self.theme, crate::theme::keys::INPUT_GAP);
        let text_size = rems(self.theme, font_size);
        let icon_size = rems(self.theme, icon_size);
        let disabled = self.disabled;
        let value = state.read(cx).value();
        let current_value_nonempty = !value.is_empty();
        let input = div()
            .flex_1()
            .min_w_0()
            .child(gpui_kit::base::input::Input::new(&state));
        let mut frame = div()
            .id(self.id.clone())
            .test_support()
            .flex()
            .items_center()
            .gap(gap)
            .h(height)
            .px(pad)
            .text_size(text_size)
            .bg(color(
                self.theme,
                if disabled {
                    crate::theme::keys::INPUT_BACKGROUND_DISABLED
                } else if self.readonly {
                    crate::theme::keys::INPUT_BACKGROUND_READONLY
                } else {
                    crate::theme::keys::INPUT_BACKGROUND
                },
            ))
            .rounded(rems(self.theme, crate::theme::keys::INPUT_BORDER_RADIUS))
            .border(if self.bordered {
                rems(self.theme, crate::theme::keys::INPUT_BORDER_WIDTH)
            } else {
                gpui_kit::rems(0.0)
            })
            .border_color(color(self.theme, border))
            .when(self.block, |this| this.w_full())
            .when_some(self.width, |this, width| this.w(width))
            .when(disabled, |this| this.opacity(0.72))
            .when(!self.bordered, |this| this.border_color(gpui_kit::rgba(0)))
            .when(!disabled && self.status == InputStatus::Normal, |this| {
                this.hover(|style| {
                    style.border_color(color(
                        self.theme,
                        crate::theme::keys::INPUT_BORDER_COLOR_HOVER,
                    ))
                })
            })
            .when(focused, |this| this.border_color(color(self.theme, border)))
            .when_some(self.leading, |this, leading| {
                let leading = match leading {
                    InputSlot::Element(element) => element,
                    InputSlot::Icon(icon) => icon
                        .size(icon_size)
                        .color_value(color(self.theme, crate::theme::keys::INPUT_AFFIX_COLOR))
                        .into_any_element(),
                };
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_color(color(self.theme, crate::theme::keys::INPUT_AFFIX_COLOR))
                        .child(leading),
                )
            })
            .child(input);
        if self.show_counter {
            frame = frame.child(counter_slot(
                self.theme,
                value.chars().count(),
                self.max_length,
            ));
        }
        if self.password {
            let reveal = state.clone();
            let visibility = visibility.expect("password visibility state exists");
            let mut toggle = if masked {
                YueButton::icon(
                    (self.id.clone(), "input-password-toggle"),
                    self.theme,
                    YueIcon::Visible,
                    "Show password",
                )
            } else {
                YueButton::icon(
                    (self.id.clone(), "input-password-toggle"),
                    self.theme,
                    YueIcon::Hidden,
                    "Hide password",
                )
            };
            toggle = toggle
                .size(match self.size {
                    InputSize::Small => ButtonSize::Small,
                    InputSize::Medium => ButtonSize::Medium,
                    InputSize::Large => ButtonSize::Large,
                })
                .shape(ButtonShape::Circle)
                .variant(ButtonVariant::Ghost)
                .on_click(move |_, window, cx| {
                    let should_mask = visibility.update(cx, |visible, _| {
                        *visible = !*visible;
                        !*visible
                    });
                    reveal.update(cx, |input, cx| input.set_masked(should_mask, window, cx));
                });
            frame = frame.child(toggle);
        }
        if self.clearable && current_value_nonempty && !disabled && !self.readonly {
            let clear = state.clone();
            frame = frame.child(
                gpui_kit::base::Button::new((self.id.clone(), "input-clear"))
                    .accessibility_label("Clear input")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(rems(self.theme, crate::theme::keys::INPUT_CLEAR_HIT_SIZE))
                    .rounded(rems(
                        self.theme,
                        crate::theme::keys::INPUT_CLEAR_BORDER_RADIUS,
                    ))
                    .bg(gpui_kit::rgba(0))
                    .text_color(color(self.theme, crate::theme::keys::INPUT_CLEAR_COLOR))
                    .cursor_pointer()
                    .hover(|style| {
                        style.text_color(color(
                            self.theme,
                            crate::theme::keys::INPUT_CLEAR_COLOR_HOVER,
                        ))
                    })
                    .on_click(move |_, window, cx| {
                        clear.update(cx, |input, cx| input.clean(window, cx))
                    })
                    .child(
                        Icon::new(YueIcon::Close)
                            .size(icon_size)
                            .color_value(color(self.theme, crate::theme::keys::INPUT_CLEAR_COLOR)),
                    ),
            );
        }
        let frame = accessible_frame(
            frame,
            FrameA11y {
                role: self.input_type.role(self.password),
                label: self.accessibility_label,
                description: self.accessibility_description,
                placeholder: self
                    .placeholder
                    .clone()
                    .or_else(|| Some(presentation.placeholder().clone()))
                    .filter(|text| !text.is_empty()),
                value: (!masked).then(|| value.clone()),
                editable: !disabled && !self.readonly,
            },
            &state,
            presentation.focus_handle(),
        );
        frame.when_some(self.trailing, |this, trailing| {
            let trailing = match trailing {
                InputSlot::Element(element) => element,
                InputSlot::Icon(icon) => icon
                    .size(icon_size)
                    .color_value(color(self.theme, crate::theme::keys::INPUT_AFFIX_COLOR))
                    .into_any_element(),
            };
            this.child(
                div()
                    .flex_shrink_0()
                    .text_color(color(self.theme, crate::theme::keys::INPUT_AFFIX_COLOR))
                    .child(trailing),
            )
        })
    }
}
