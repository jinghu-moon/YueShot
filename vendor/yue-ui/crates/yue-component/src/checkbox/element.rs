//! Element composition and component-local presentation.

use super::*;

use super::behavior::*;

use super::paint::*;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt as _;

use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::{
    App, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, RenderOnce,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use crate::tokens::{duration, easing, mix_rgba, number, pixels, rems, rgba};

use crate::{GroupDirection, YueTheme};

pub(super) fn size_tokens(
    size: CheckboxSize,
) -> (
    crate::theme::TokenKey,
    crate::theme::TokenKey,
    crate::theme::TokenKey,
    crate::theme::TokenKey,
) {
    match size {
        CheckboxSize::Small => (
            crate::theme::keys::CHECKBOX_SIZE_SM,
            crate::theme::keys::CHECKBOX_HEIGHT_SM,
            crate::theme::keys::CHECKBOX_LABEL_FONT_SIZE_SM,
            crate::theme::keys::CHECKBOX_MARK_STROKE_WIDTH_SM,
        ),
        CheckboxSize::Medium => (
            crate::theme::keys::CHECKBOX_SIZE_MD,
            crate::theme::keys::CHECKBOX_HEIGHT_MD,
            crate::theme::keys::CHECKBOX_LABEL_FONT_SIZE_MD,
            crate::theme::keys::CHECKBOX_MARK_STROKE_WIDTH_MD,
        ),
        CheckboxSize::Large => (
            crate::theme::keys::CHECKBOX_SIZE_LG,
            crate::theme::keys::CHECKBOX_HEIGHT_LG,
            crate::theme::keys::CHECKBOX_LABEL_FONT_SIZE_LG,
            crate::theme::keys::CHECKBOX_MARK_STROKE_WIDTH_LG,
        ),
    }
}

/// The pointer states that drive the hover halo and the press shrink. Kept as one
/// piece of keyed state so a move or a release costs a single repaint.
#[derive(Clone, Copy, Default)]
struct Pointer {
    hovered: bool,
    pressed: bool,
}

impl RenderOnce for YueCheckbox {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let (box_token, height_token, font_token, stroke_token) = size_tokens(self.size);
        let box_size = pixels(self.theme, box_token);
        let border = pixels(self.theme, crate::theme::keys::CHECKBOX_BORDER_WIDTH);
        let radius = pixels(self.theme, crate::theme::keys::CHECKBOX_BORDER_RADIUS);
        let halo_width = pixels(self.theme, crate::theme::keys::CHECKBOX_HALO_WIDTH);
        let halo_color = rgba(self.theme, crate::theme::keys::CHECKBOX_HALO_COLOR);
        let ring_width = pixels(self.theme, crate::theme::keys::CHECKBOX_FOCUS_RING_WIDTH);
        let ring_offset = pixels(self.theme, crate::theme::keys::CHECKBOX_FOCUS_RING_OFFSET);
        let ring_color = rgba(self.theme, crate::theme::keys::CHECKBOX_FOCUS_RING_COLOR);
        let mark_stroke = pixels(self.theme, stroke_token);
        let chosen = self.state.chosen();
        let id = self.id.clone();

        // The ring belongs to the box, not to the row, so the focus handle is
        // Yue's own: the base control is told to track it and the ring reads it
        // back. `last_input_was_keyboard` is what GPUI's own `focus_visible`
        // condition uses, so a mouse click leaves the box unringed.
        let focus_handle = self.focus_handle.clone().unwrap_or_else(|| {
            window
                .use_keyed_state((id.clone(), "yue-checkbox-focus"), cx, |_, cx| {
                    cx.focus_handle()
                })
                .read(cx)
                .clone()
        });
        let ring_visible =
            !self.disabled && focus_handle.is_focused(window) && window.last_input_was_keyboard();

        let pointer = (!self.disabled).then(|| {
            window.use_keyed_state((id.clone(), "yue-checkbox-pointer"), cx, |_, _| {
                Pointer::default()
            })
        });
        let (hovered, pressed, pointer_state) = match pointer.as_ref() {
            None => (false, false, None),
            Some(state) => {
                let current = state.read(cx);
                (current.hovered, current.pressed, Some(state.clone()))
            }
        };

        let standard = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(theme, crate::theme::keys::CHECKBOX_DURATION))
                .ease(easing(theme, crate::theme::keys::CHECKBOX_EASE))
        };
        // The mark is written in only once the box has finished filling, and is
        // wiped off on the short rung before the fill has gone.
        let ink = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(theme, crate::theme::keys::CHECKBOX_DURATION))
                .delay(duration(theme, crate::theme::keys::CHECKBOX_DURATION_FAST))
                .ease(easing(theme, crate::theme::keys::CHECKBOX_EASE_ENTER))
        };
        let erase = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(
                theme,
                crate::theme::keys::CHECKBOX_DURATION_FAST,
            ))
            .ease(easing(theme, crate::theme::keys::CHECKBOX_EASE_EXIT))
        };
        // The box shrinks on the short rung, and the shrink is eased the same way
        // as the fill rather than on its own curve.
        let press = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(
                theme,
                crate::theme::keys::CHECKBOX_DURATION_FAST,
            ))
            .ease(easing(theme, crate::theme::keys::CHECKBOX_EASE))
        };

        let mut progress = |channel: &'static str, target: f32, policy| {
            gpui_kit::base::transition(
                (id.clone(), format!("yue-checkbox-{channel}")),
                target,
                policy,
                window,
                cx,
            )
            .clamp(0.0, 1.0)
        };
        let fill = progress("fill", f32::from(chosen), standard(self.theme));
        let emphasis = progress("border", f32::from(hovered || chosen), standard(self.theme));
        let halo = progress("halo", f32::from(hovered), standard(self.theme));
        let scale = gpui_kit::base::transition(
            (id.clone(), "yue-checkbox-press"),
            if pressed {
                number(self.theme, crate::theme::keys::CHECKBOX_PRESS_SCALE)
            } else {
                1.0
            },
            press(self.theme),
            window,
            cx,
        );
        let check_inked = gpui_kit::base::transition(
            (id.clone(), "yue-checkbox-check"),
            f32::from(self.state == CheckboxState::Checked),
            if self.state == CheckboxState::Checked {
                ink(self.theme)
            } else {
                erase(self.theme)
            },
            window,
            cx,
        )
        .clamp(0.0, 1.0);
        let dash_inked = gpui_kit::base::transition(
            (id.clone(), "yue-checkbox-dash"),
            f32::from(self.state == CheckboxState::Indeterminate),
            if self.state == CheckboxState::Indeterminate {
                ink(self.theme)
            } else {
                erase(self.theme)
            },
            window,
            cx,
        )
        .clamp(0.0, 1.0);

        // A disabled box is repainted rather than transitioned, the way the
        // reference's own disabled rule replaces the fill outright.
        let background = if self.disabled {
            rgba(self.theme, crate::theme::keys::CHECKBOX_DISABLED_BACKGROUND)
        } else {
            mix_rgba(
                rgba(self.theme, crate::theme::keys::CHECKBOX_BACKGROUND),
                rgba(self.theme, crate::theme::keys::CHECKBOX_BACKGROUND_CHECKED),
                fill,
            )
        };
        let border_color = if self.disabled {
            rgba(
                self.theme,
                crate::theme::keys::CHECKBOX_DISABLED_BORDER_COLOR,
            )
        } else {
            mix_rgba(
                rgba(self.theme, crate::theme::keys::CHECKBOX_BORDER_COLOR),
                rgba(
                    self.theme,
                    crate::theme::keys::CHECKBOX_BORDER_COLOR_CHECKED,
                ),
                emphasis,
            )
        };
        let mark_color = gpui_kit::Hsla::from(rgba(
            self.theme,
            if self.disabled {
                crate::theme::keys::CHECKBOX_DISABLED_MARK_COLOR
            } else {
                crate::theme::keys::CHECKBOX_MARK_COLOR
            },
        ));
        // The mark is painted on the box's content area, so its unit is that
        // area's share of the reference's 16-unit box. Its stroke is not: the
        // weight is the size's own pixel rung, shared with the switch's glyphs, so
        // a mark keeps that weight wherever the box scales it and only the press
        // shrinks it.
        let inner = box_size * scale;
        let content = inner - 2.0 * border * scale;
        let unit = content / MARK_BOX;
        let stroke = mark_stroke * scale;
        let margin = (box_size - inner) / 2.0;
        // `outline-offset` measures from the border box, and GPUI offsets an
        // absolute child from the content box, so the border is one leg of the
        // distance. Every part of the ring shrinks with the box it rings.
        let ring_inset = -scale * (ring_offset + ring_width + border);

        let box_layer = div()
            .absolute()
            .left(px(margin))
            .top(px(margin))
            .w(px(inner))
            .h(px(inner))
            .rounded(gpui_kit::rems(radius * scale / 16.0))
            .bg(background)
            .border(px(border * scale))
            .border_color(border_color)
            .when(halo > 0.0, |layer| {
                // The hover halo is a spread shadow rather than a laid-out border,
                // so hovering never moves the box or the label beside it.
                layer.shadow(vec![
                    gpui_kit::BoxShadow::new(
                        px(0.0),
                        px(0.0),
                        gpui_kit::Hsla::from(halo_color).opacity(halo * halo_color.a),
                    )
                    .spread_radius(px(halo_width * halo)),
                ])
            })
            .child(mark_layer(
                &CHECK_LINE,
                check_inked,
                unit,
                stroke,
                mark_color,
            ))
            .child(mark_layer(&DASH_LINE, dash_inked, unit, stroke, mark_color))
            .when(ring_visible, |layer| {
                layer.child(
                    div()
                        .absolute()
                        .top(px(ring_inset))
                        .left(px(ring_inset))
                        .right(px(ring_inset))
                        .bottom(px(ring_inset))
                        .border(px(ring_width * scale))
                        .border_color(ring_color)
                        .rounded(gpui_kit::rems(
                            (radius + ring_offset + ring_width) * scale / 16.0,
                        )),
                )
            });

        let mut checkbox = gpui_kit::base::Checkbox::new(id)
            .state(self.state.to_base())
            .disabled(self.disabled)
            .accessibility_label(self.accessible_name())
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .gap(rems(self.theme, crate::theme::keys::CHECKBOX_LABEL_GAP))
            .min_h(rems(self.theme, height_token))
            .text_size(rems(self.theme, font_token))
            .text_color(rgba(
                self.theme,
                if self.disabled {
                    crate::theme::keys::CHECKBOX_LABEL_COLOR_DISABLED
                } else {
                    crate::theme::keys::CHECKBOX_LABEL_COLOR
                },
            ))
            .when(!self.disabled, |checkbox| checkbox.cursor_pointer())
            // A fixed slot holds the box, so shrinking it on press moves nothing
            // else in the row — the reference's `transform: scale(.9)` never
            // reflows either.
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .w(px(box_size))
                    .h(px(box_size))
                    .child(box_layer),
            );
        if let Some(label) = self.label {
            checkbox = checkbox.child(label);
        }

        if let Some(state) = pointer_state {
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
            checkbox = checkbox
                .on_hover(hover)
                .on_mouse_down(MouseButton::Left, down)
                .on_mouse_up(MouseButton::Left, up)
                .on_mouse_up_out(MouseButton::Left, up_out);
        }
        if let Some(handler) = self.on_change {
            checkbox = checkbox.on_change(move |state, event, window, cx| {
                handler(CheckboxState::from_base(state), event, window, cx);
            });
        }
        self.layout.apply(checkbox)
    }
}

impl RenderOnce for YueCheckboxGroup {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let gap = rems(self.theme, crate::theme::keys::CHECKBOX_GROUP_GAP);
        let vertical = self.direction == GroupDirection::Vertical;
        let group_disabled = self.disabled;
        let has_header = self.select_all.is_some();
        let nothing_left_to_change = eligible_values(&self.options).is_empty();

        // Registered for UI queries like the radio group's container: the name and
        // the Group role live here, so a test can see the options sit inside a
        // container the tree can name.
        let mut group = div()
            .id((self.id.clone(), "yue-checkbox-group"))
            .test_support()
            .flex()
            // The container always stacks its own parts — caption, header, options
            // — so a horizontal group keeps them above the row rather than in it.
            // The option direction is the rows container's business below.
            .flex_col()
            .items_start()
            .gap(gap);
        // The name is carried by the container as well as by the visible caption,
        // so a screen reader announces the group before its options.
        group = group
            .role(gpui_kit::Role::Group)
            .when_some(self.label.clone(), |group, label| group.aria_label(label));

        group = group.when_some(self.label.clone(), |group, label| {
            group.child(
                div()
                    .text_size(rems(
                        self.theme,
                        crate::theme::keys::CHECKBOX_LABEL_FONT_SIZE_MD,
                    ))
                    .text_color(rgba(self.theme, crate::theme::keys::CHECKBOX_LABEL_COLOR))
                    .child(label),
            )
        });

        if let Some(header_label) = self.select_all {
            let state = select_all_state(&self.options, &self.selected);
            let mut header = YueCheckbox::new(
                (self.id.clone(), "yue-checkbox-select-all"),
                self.theme,
                state,
            )
            .size(self.size)
            // With nothing left to change, the header itself cannot be operated.
            .disabled(group_disabled || nothing_left_to_change)
            // The header's text is both its visible label and its accessible name,
            // so the row reads like any other option and still answers to that name.
            .label(header_label);
            if let Some(handler) = self.on_change.as_ref() {
                let handler = Rc::clone(handler);
                let selected = self.selected.clone();
                let options = self.options.clone();
                header = header.on_change(move |next, event, window, cx| {
                    handler(
                        apply_select_all(&options, &selected, next.chosen()),
                        event,
                        window,
                        cx,
                    );
                });
            }
            group = group.child(header);
        }

        let rows = self.options.iter().map(|option| {
            let state = if self.selected.contains(&option.value) {
                CheckboxState::Checked
            } else {
                CheckboxState::Unchecked
            };
            let mut checkbox =
                YueCheckbox::new((self.id.clone(), option.value.clone()), self.theme, state)
                    .size(self.size)
                    .disabled(group_disabled || option.disabled)
                    .label(option.label.clone());
            if let Some(handler) = self.on_change.as_ref() {
                let handler = Rc::clone(handler);
                let selected = self.selected.clone();
                let value = option.value.clone();
                checkbox = checkbox.on_change(move |next, event, window, cx| {
                    handler(apply_option(&selected, &value, next), event, window, cx);
                });
            }
            checkbox
        });
        // Options are the header's children, so a vertical group indents them
        // under it; a horizontal group has no such column to hang off.
        let indent = rems(self.theme, crate::theme::keys::CHECKBOX_GROUP_INDENT);
        self.layout.apply(
            group.child(
                div()
                    .flex()
                    .gap(gap)
                    .when(vertical, |rows| rows.flex_col().items_start())
                    .when(!vertical, |rows| rows.flex_row().flex_wrap())
                    .when(vertical && has_header, |rows| rows.pl(indent))
                    .children(rows),
            ),
        )
    }
}

use crate::icon::{CHECK_LINE, MARK_BOX};
