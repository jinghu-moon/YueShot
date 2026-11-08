//! Element composition and component-local presentation.

use super::*;

use super::behavior::*;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt as _;

use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::{
    App, ElementId, FocusHandle, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use crate::tokens::{duration, easing, mix_rgba, number, pixels, rems, rgba};

use crate::{GroupDirection, YueTheme};

/// The row a segment lives in, the label it carries, and the inline padding that
/// gives it its width — the same three rungs the Indicator appearance reads,
/// resolved against the segmented control's own ladder.
pub(super) fn segment_tokens(
    size: RadioSize,
) -> (
    crate::theme::TokenKey,
    crate::theme::TokenKey,
    crate::theme::TokenKey,
) {
    match size {
        RadioSize::Small => (
            crate::theme::keys::RADIO_BUTTON_HEIGHT_SM,
            crate::theme::keys::RADIO_BUTTON_FONT_SIZE_SM,
            crate::theme::keys::RADIO_BUTTON_PADDING_INLINE_SM,
        ),
        RadioSize::Medium => (
            crate::theme::keys::RADIO_BUTTON_HEIGHT_MD,
            crate::theme::keys::RADIO_BUTTON_FONT_SIZE_MD,
            crate::theme::keys::RADIO_BUTTON_PADDING_INLINE_MD,
        ),
        RadioSize::Large => (
            crate::theme::keys::RADIO_BUTTON_HEIGHT_LG,
            crate::theme::keys::RADIO_BUTTON_FONT_SIZE_LG,
            crate::theme::keys::RADIO_BUTTON_PADDING_INLINE_LG,
        ),
    }
}

/// Where a segment's focus ring sits. `outline-offset` measures from the border
/// box and GPUI offsets an absolute child from the content box, so the segment's
/// own divider is one leg of the distance the ring travels inward.
pub(super) fn ring_inset(ring_offset: f32, border: f32) -> f32 {
    ring_offset - border
}

/// Paints a translucent overlay the way a browser paints a `color-mix()`
/// background over a fill: the result keeps the fill's own opacity and only its
/// channels move toward the overlay, so a hovered segment tints itself instead of
/// punching a hole through to whatever sits behind the row.
pub(super) fn tint(fill: gpui_kit::Rgba, overlay: gpui_kit::Rgba, progress: f32) -> gpui_kit::Rgba {
    fill.blend(gpui_kit::Rgba {
        a: overlay.a * progress,
        ..overlay
    })
}

/// The pointer states that drive the hover halo and the press shrink. Kept as one
/// piece of keyed state so a move or a release costs a single repaint.
#[derive(Clone, Copy, Default)]
struct Pointer {
    hovered: bool,
    pressed: bool,
}

/// What both appearances need before they style anything: the identity their
/// transitions key on, the name the node carries, and the pointer and focus facts
/// that decide which state gets drawn.
struct RadioFrame<'a> {
    id: ElementId,
    name: SharedString,
    focus: &'a FocusHandle,
    ring_visible: bool,
    hovered: bool,
    pressed: bool,
}

impl YueRadio {
    /// The option as Base sees it: its state, its name, and its place in the tab
    /// order and the set. Both appearances start here and add only drawing.
    fn base(&self, frame: &RadioFrame) -> gpui_kit::base::Radio {
        gpui_kit::base::Radio::new(frame.id.clone())
            .checked(self.checked)
            .disabled(self.disabled)
            .accessibility_label(frame.name.clone())
            .tab_stop(self.tab_stop)
            .tab_index(if self.tab_stop { 0 } else { -1 })
            .track_focus(frame.focus)
            .when(!self.disabled, |radio| radio.cursor_pointer())
            .when_some(self.position, |radio, (position, size)| {
                radio.set_position(position, size)
            })
    }

    /// The circle, with the label beside it.
    fn indicator(
        &self,
        frame: &RadioFrame,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::base::Radio {
        let id = frame.id.clone();
        let checked = self.checked;
        let (hovered, pressed, ring_visible) = (frame.hovered, frame.pressed, frame.ring_visible);
        let (box_token, dot_token, height_token, font_token) = size_tokens(self.size);
        let box_size = pixels(self.theme, box_token);
        let dot_size = pixels(self.theme, dot_token);
        let border = pixels(self.theme, crate::theme::keys::RADIO_BORDER_WIDTH);
        let radius = pixels(self.theme, crate::theme::keys::RADIO_BORDER_RADIUS);
        let halo_width = pixels(self.theme, crate::theme::keys::RADIO_HALO_WIDTH);
        let halo_color = rgba(self.theme, crate::theme::keys::RADIO_HALO_COLOR);
        let ring_width = pixels(self.theme, crate::theme::keys::RADIO_FOCUS_RING_WIDTH);
        let ring_offset = pixels(self.theme, crate::theme::keys::RADIO_FOCUS_RING_OFFSET);
        let ring_color = rgba(self.theme, crate::theme::keys::RADIO_FOCUS_RING_COLOR);

        let standard = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(theme, crate::theme::keys::RADIO_DURATION))
                .ease(easing(theme, crate::theme::keys::RADIO_EASE))
        };
        // The dot grows and collapses on the same rung the circle changes on; the
        // reference switches only its easing, entering with the ease-in curve and
        // leaving with the ease-out one.
        let dot_motion = |theme: YueTheme, growing| {
            gpui_kit::base::Transition::new(duration(theme, crate::theme::keys::RADIO_DURATION))
                .ease(easing(
                    theme,
                    if growing {
                        crate::theme::keys::RADIO_EASE_ENTER
                    } else {
                        crate::theme::keys::RADIO_EASE_EXIT
                    },
                ))
        };
        // The circle shrinks on the short rung, eased the same way as the fill.
        let press = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(
                theme,
                crate::theme::keys::RADIO_DURATION_FAST,
            ))
            .ease(easing(theme, crate::theme::keys::RADIO_EASE))
        };

        let mut progress = |channel: &'static str, target: f32, policy| {
            gpui_kit::base::transition(
                (id.clone(), format!("yue-radio-{channel}")),
                target,
                policy,
                window,
                cx,
            )
            .clamp(0.0, 1.0)
        };
        let fill = progress("fill", f32::from(checked), standard(self.theme));
        let emphasis = progress(
            "border",
            f32::from(hovered || checked),
            standard(self.theme),
        );
        let halo = progress("halo", f32::from(hovered), standard(self.theme));
        let dot = progress("dot", f32::from(checked), dot_motion(self.theme, checked));
        let scale = gpui_kit::base::transition(
            (id.clone(), "yue-radio-press"),
            if pressed {
                number(self.theme, crate::theme::keys::RADIO_PRESS_SCALE)
            } else {
                1.0
            },
            press(self.theme),
            window,
            cx,
        );

        // A disabled circle is repainted rather than transitioned, the way the
        // reference's disabled rule replaces the fill outright.
        let background = if self.disabled {
            rgba(self.theme, crate::theme::keys::RADIO_DISABLED_BACKGROUND)
        } else {
            mix_rgba(
                rgba(self.theme, crate::theme::keys::RADIO_BACKGROUND),
                rgba(self.theme, crate::theme::keys::RADIO_BACKGROUND_CHECKED),
                fill,
            )
        };
        let border_color = if self.disabled {
            rgba(self.theme, crate::theme::keys::RADIO_DISABLED_BORDER_COLOR)
        } else {
            mix_rgba(
                rgba(self.theme, crate::theme::keys::RADIO_BORDER_COLOR),
                rgba(self.theme, crate::theme::keys::RADIO_BORDER_COLOR_CHECKED),
                emphasis,
            )
        };
        let dot_color = rgba(
            self.theme,
            if self.disabled {
                crate::theme::keys::RADIO_DISABLED_DOT_COLOR
            } else {
                crate::theme::keys::RADIO_DOT_COLOR
            },
        );
        // Everything about the circle shrinks together with the press, the dot
        // included: the reference scales the element, and its `::after` rides the
        // same box.
        let inner = box_size * scale;
        let content = inner - 2.0 * border * scale;
        let margin = (box_size - inner) / 2.0;
        let dot_inset = (content - dot_size * scale) / 2.0;
        // `outline-offset` measures from the border box, and GPUI offsets an
        // absolute child from the content box, so the border is one leg of the
        // distance.
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
                // so hovering never moves the circle or the label beside it.
                layer.shadow(vec![
                    gpui_kit::BoxShadow::new(
                        px(0.0),
                        px(0.0),
                        gpui_kit::Hsla::from(halo_color).opacity(halo * halo_color.a),
                    )
                    .spread_radius(px(halo_width * halo)),
                ])
            })
            .when(dot > 0.0, |layer| {
                layer.child(
                    div()
                        .absolute()
                        .top(px(dot_inset))
                        .left(px(dot_inset))
                        .right(px(dot_inset))
                        .bottom(px(dot_inset))
                        .rounded(gpui_kit::rems(radius * scale / 16.0))
                        .bg(dot_color),
                )
            })
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

        self.base(frame)
            .flex()
            .items_center()
            .gap(rems(self.theme, crate::theme::keys::RADIO_LABEL_GAP))
            .min_h(rems(self.theme, height_token))
            .text_size(rems(self.theme, font_token))
            .text_color(rgba(
                self.theme,
                if self.disabled {
                    crate::theme::keys::RADIO_LABEL_COLOR_DISABLED
                } else {
                    crate::theme::keys::RADIO_LABEL_COLOR
                },
            ))
            // A fixed slot holds the circle, so shrinking it on press moves nothing
            // else in the row — the reference's `transform: scale(.9)` never reflows
            // either.
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .w(px(box_size))
                    .h(px(box_size))
                    .child(box_layer),
            )
            .when_some(self.label.clone(), |radio, label| radio.child(label))
    }

    /// One cell of the outlined row: the option's state as a fill and a label
    /// colour, no circle, and the same focus ring the circle draws — inset,
    /// because the row clips its ends and a ring outside a segment would be cut
    /// in half.
    fn segment_row(
        &self,
        frame: &RadioFrame,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::base::Radio {
        let id = frame.id.clone();
        let checked = self.checked;
        let (hovered, pressed, ring_visible) = (frame.hovered, frame.pressed, frame.ring_visible);
        let (height_token, font_token, padding_token) = segment_tokens(self.size);
        let border = pixels(self.theme, crate::theme::keys::RADIO_BUTTON_BORDER_WIDTH);
        let radius = pixels(self.theme, crate::theme::keys::RADIO_BUTTON_BORDER_RADIUS);
        let ring_width = pixels(self.theme, crate::theme::keys::RADIO_FOCUS_RING_WIDTH);
        let ring_offset = pixels(self.theme, crate::theme::keys::RADIO_FOCUS_RING_OFFSET);
        let ring_color = rgba(self.theme, crate::theme::keys::RADIO_FOCUS_RING_COLOR);

        // The choice arrives on the row's enter rung and the press leaves on the
        // exit one, the way the reference splits a segmented row's transitions. A
        // segment never scales, so the press reads as its deeper overlay.
        let recolour = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(
                theme,
                crate::theme::keys::RADIO_BUTTON_DURATION,
            ))
            .ease(easing(theme, crate::theme::keys::RADIO_BUTTON_EASE))
        };
        let release = |theme: YueTheme| {
            gpui_kit::base::Transition::new(duration(
                theme,
                crate::theme::keys::RADIO_BUTTON_DURATION_FAST,
            ))
            .ease(easing(theme, crate::theme::keys::RADIO_BUTTON_EASE))
        };

        let mut progress = |channel: &'static str, target: f32, policy| {
            gpui_kit::base::transition(
                (id.clone(), format!("yue-radio-{channel}")),
                target,
                policy,
                window,
                cx,
            )
            .clamp(0.0, 1.0)
        };
        let selected = progress("segment-fill", f32::from(checked), recolour(self.theme));
        // Only an unchosen segment takes a hover or a press overlay: the chosen one
        // already reads as selected, and the reference lifts neither its fill nor
        // its label further.
        let hover = progress(
            "segment-hover",
            f32::from(hovered && !checked),
            recolour(self.theme),
        );
        let press = progress(
            "segment-press",
            f32::from(pressed && !checked),
            release(self.theme),
        );
        let lift = progress(
            "segment-text",
            f32::from(hovered && !checked),
            recolour(self.theme),
        );

        // A disabled segment keeps the fill it died with — losing the chosen tint
        // would hide the very selection the caller still holds.
        let background = if self.disabled && !checked {
            rgba(
                self.theme,
                crate::theme::keys::RADIO_BUTTON_DISABLED_BACKGROUND,
            )
        } else {
            let fill = mix_rgba(
                rgba(self.theme, crate::theme::keys::RADIO_BUTTON_BACKGROUND),
                rgba(
                    self.theme,
                    crate::theme::keys::RADIO_BUTTON_BACKGROUND_SELECTED,
                ),
                selected,
            );
            let fill = tint(
                fill,
                rgba(
                    self.theme,
                    crate::theme::keys::RADIO_BUTTON_HOVER_BACKGROUND,
                ),
                hover,
            );
            tint(
                fill,
                rgba(
                    self.theme,
                    crate::theme::keys::RADIO_BUTTON_PRESSED_BACKGROUND,
                ),
                press,
            )
        };
        let color = if self.disabled {
            rgba(self.theme, crate::theme::keys::RADIO_BUTTON_DISABLED_COLOR)
        } else {
            let ink = mix_rgba(
                rgba(self.theme, crate::theme::keys::RADIO_BUTTON_COLOR),
                rgba(self.theme, crate::theme::keys::RADIO_BUTTON_COLOR_HOVER),
                lift,
            );
            mix_rgba(
                ink,
                rgba(self.theme, crate::theme::keys::RADIO_BUTTON_COLOR_SELECTED),
                selected,
            )
        };

        self.base(frame)
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .min_h(px(pixels(self.theme, height_token)))
            .px(px(pixels(self.theme, padding_token)))
            .text_size(px(pixels(self.theme, font_token)))
            .text_color(color)
            .bg(background)
            .whitespace_nowrap()
            // The chosen label carries weight as well as colour, so the selection
            // still reads in a monochrome or high-contrast profile.
            .when(checked && !self.disabled, |radio| {
                radio.font_weight(FontWeight(number(
                    self.theme,
                    crate::theme::keys::RADIO_BUTTON_WEIGHT_SELECTED,
                )))
            })
            .when_some(self.label.clone(), |radio, label| radio.child(label))
            // The row outlines itself once, so a segment adds only the divider it
            // shares with the neighbour before it — on the axis the row runs along.
            .when_some(self.segment, |radio, segment| {
                if segment.first {
                    radio
                } else if segment.vertical {
                    radio.border_t(px(border))
                } else {
                    radio.border_l(px(border))
                }
            })
            .when(self.segment.is_none(), |radio| {
                radio
                    .border(px(border))
                    .rounded(gpui_kit::rems(radius / 16.0))
            })
            .border_color(rgba(
                self.theme,
                crate::theme::keys::RADIO_BUTTON_BORDER_COLOR,
            ))
            .when(ring_visible, move |radio| {
                let inset = ring_inset(ring_offset, border);
                radio.child(
                    div()
                        .absolute()
                        .top(px(inset))
                        .left(px(inset))
                        .right(px(inset))
                        .bottom(px(inset))
                        .border(px(ring_width))
                        .border_color(ring_color)
                        .rounded(gpui_kit::rems(
                            (radius - ring_offset - ring_width).max(0.0) / 16.0,
                        )),
                )
            })
    }
}

impl RenderOnce for YueRadio {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let id = self.id.clone();

        // The ring belongs to the control rather than to the row around it, so the
        // focus handle is Yue's own — or the group's, when one drives roving focus
        // through the option. `last_input_was_keyboard` is what GPUI's own
        // `focus_visible` condition uses, so a mouse click leaves it unringed.
        let focus_handle = match self.focus_handle.as_ref() {
            Some(handle) => handle.clone(),
            None => window
                .use_keyed_state((id.clone(), FOCUS_SLOT), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone(),
        };
        let ring_visible =
            !self.disabled && focus_handle.is_focused(window) && window.last_input_was_keyboard();

        let pointer = (!self.disabled).then(|| {
            window.use_keyed_state((id.clone(), "yue-radio-pointer"), cx, |_, _| {
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

        let frame = RadioFrame {
            id,
            name: self.accessible_name(),
            focus: &focus_handle,
            ring_visible,
            hovered,
            pressed,
        };
        let mut radio = match self.appearance {
            RadioAppearance::Indicator => self.indicator(&frame, window, cx),
            RadioAppearance::Button => self.segment_row(&frame, window, cx),
        };

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
            radio = radio
                .on_hover(hover)
                .on_mouse_down(MouseButton::Left, down)
                .on_mouse_up(MouseButton::Left, up)
                .on_mouse_up_out(MouseButton::Left, up_out);
        }
        if let Some(handler) = self.on_change {
            radio = radio.on_change(move |checked, event, window, cx| {
                handler(checked, event, window, cx);
            });
        }
        self.layout.apply(radio)
    }
}

impl RenderOnce for YueRadioGroup {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let gap = rems(self.theme, crate::theme::keys::RADIO_GROUP_GAP);
        let segmented = self.appearance == RadioAppearance::Button;
        let vertical = self.direction == GroupDirection::Vertical;
        let group_disabled = self.disabled;
        let selected = self.selected.clone();
        let count = self.options.len();
        let stop = tab_stop_index(&self.options, selected.as_ref(), group_disabled);

        // The options a Tab or an arrow key can reach, in visual order. The group
        // holds their handles because it, not the rows, decides where a move goes,
        // and reads them back to tell which option is focused.
        let mut operable: Vec<(SharedString, FocusHandle)> = Vec::new();
        let mut rows = Vec::with_capacity(count);
        for (index, option) in self.options.iter().enumerate() {
            let option_id = ElementId::from((self.id.clone(), option.value.clone()));
            let focus = if group_disabled || option.disabled {
                None
            } else {
                Some(
                    window
                        .use_keyed_state((option_id.clone(), FOCUS_SLOT), cx, |_, cx| {
                            cx.focus_handle()
                        })
                        .read(cx)
                        .clone(),
                )
            };
            if let Some(handle) = focus.as_ref() {
                operable.push((option.value.clone(), handle.clone()));
            }
            let mut radio = YueRadio::new(
                option_id,
                self.theme,
                Some(&option.value) == selected.as_ref(),
            )
            .size(self.size)
            .appearance(self.appearance)
            .disabled(group_disabled || option.disabled)
            .label(option.label.clone())
            .tab_stop(stop == Some(index))
            .set_position(index + 1, count);
            if segmented {
                radio = radio.segment(vertical, index == 0);
            }
            if let Some(handle) = focus {
                radio = radio.track_focus(&handle);
            }
            if let Some(handler) = self.on_change.as_ref() {
                let handler = Rc::clone(handler);
                let value = option.value.clone();
                radio = radio.on_change(move |_, _, window, cx| {
                    handler(value.clone(), window, cx);
                });
            }
            rows.push(radio);
        }

        // The container is Yue's own div rather than Base's RadioGroup, because the
        // group has to be an observable node: it carries the name and the axis a
        // screen reader announces before the options, and it is the element that
        // receives the arrow keys while an option inside it holds the focus.
        let mut group = div()
            .id((self.id.clone(), "yue-radio-group"))
            .test_support()
            .role(Role::RadioGroup)
            .aria_orientation(if vertical {
                gpui_kit::accesskit::Orientation::Vertical
            } else {
                gpui_kit::accesskit::Orientation::Horizontal
            })
            .flex()
            .flex_col()
            .items_start()
            .gap(gap);
        // The name is carried by the container as well as by the visible caption,
        // so a screen reader announces the group before its options.
        group = group.when_some(self.label.clone(), |group, label| {
            group.aria_label(label.clone()).child(
                div()
                    .text_size(rems(
                        self.theme,
                        crate::theme::keys::RADIO_LABEL_FONT_SIZE_MD,
                    ))
                    .text_color(rgba(self.theme, crate::theme::keys::RADIO_LABEL_COLOR))
                    .child(label),
            )
        });
        if let Some(handler) = self.on_change.as_ref() {
            let handler = Rc::clone(handler);
            let operable = operable.clone();
            group = group.on_key_down(move |event: &KeyDownEvent, window, cx| {
                let Some(direction) = move_for(&event.keystroke.key, vertical) else {
                    return;
                };
                let Some(current) = operable
                    .iter()
                    .position(|(_, focus)| focus.is_focused(window))
                else {
                    return;
                };
                let Some(target) = move_index(operable.len(), current, direction) else {
                    return;
                };
                let (value, focus) = &operable[target];
                focus.focus(window, cx);
                // A radio follows its focus: the option an arrow key lands on is
                // chosen, the way a native group behaves.
                handler(value.clone(), window, cx);
                // The keys belong to the group; an enclosing scroll view must not
                // also treat them as a scroll.
                cx.stop_propagation();
            });
        }

        self.layout.apply(
            group.child(
                div()
                    .flex()
                    .when(vertical, |rows| rows.flex_col())
                    .when(!vertical, |rows| rows.flex_row())
                    .when(segmented, |track| {
                        // One continuous outline, drawn once around the row and clipped
                        // so only its ends show a radius; the segments between them
                        // carry the shared dividers instead of gaps. A wrapped segment
                        // row would no longer read as one choice set, so it never
                        // wraps — long labels belong to the Indicator appearance.
                        track
                            .gap(gpui_kit::rems(0.0))
                            .items_stretch()
                            .border(px(pixels(
                                self.theme,
                                crate::theme::keys::RADIO_BUTTON_BORDER_WIDTH,
                            )))
                            .border_color(rgba(
                                self.theme,
                                crate::theme::keys::RADIO_BUTTON_BORDER_COLOR,
                            ))
                            .rounded(gpui_kit::rems(
                                pixels(self.theme, crate::theme::keys::RADIO_BUTTON_BORDER_RADIUS)
                                    / 16.0,
                            ))
                            .overflow_hidden()
                    })
                    .when(!segmented, |rows| rows.gap(gap).items_start())
                    .when(!segmented && !vertical, |rows| rows.flex_wrap())
                    .children(rows),
            ),
        )
    }
}

pub(super) fn size_tokens(
    size: RadioSize,
) -> (
    crate::theme::TokenKey,
    crate::theme::TokenKey,
    crate::theme::TokenKey,
    crate::theme::TokenKey,
) {
    match size {
        RadioSize::Small => (
            crate::theme::keys::RADIO_SIZE_SM,
            crate::theme::keys::RADIO_DOT_SIZE_SM,
            crate::theme::keys::RADIO_HEIGHT_SM,
            crate::theme::keys::RADIO_LABEL_FONT_SIZE_SM,
        ),
        RadioSize::Medium => (
            crate::theme::keys::RADIO_SIZE_MD,
            crate::theme::keys::RADIO_DOT_SIZE_MD,
            crate::theme::keys::RADIO_HEIGHT_MD,
            crate::theme::keys::RADIO_LABEL_FONT_SIZE_MD,
        ),
        RadioSize::Large => (
            crate::theme::keys::RADIO_SIZE_LG,
            crate::theme::keys::RADIO_DOT_SIZE_LG,
            crate::theme::keys::RADIO_HEIGHT_LG,
            crate::theme::keys::RADIO_LABEL_FONT_SIZE_LG,
        ),
    }
}
