use super::*;
use crate::{
    ButtonSize, ButtonVariant, InputSize, InputStatus, TooltipPlacement, TooltipTrigger, YueButton,
    YueIcon, YueNumberInput, YueOverflowTooltip, YueTooltip,
    theme::{TokenKey, keys as K},
    tokens::{duration, easing, mix_rgba, number, pixels, rems, rgba, shadow},
};
use gpui_kit::base::{
    TestSupportExt as _, Transition,
    slider::{Slider, SliderIndicator, SliderThumb, SliderTrack},
    transition,
};
use gpui_kit::{
    AnyElement, App, Axis, InteractiveElement as _, ParentElement as _, Pixels, RenderOnce,
    StatefulInteractiveElement as _, Styled as _, TextRun, Window, canvas, div,
    prelude::FluentBuilder as _, px, relative,
};

#[derive(Clone, Copy)]
struct Profile {
    track: TokenKey,
    thumb: TokenKey,
    hit: TokenKey,
    font: TokenKey,
    line: TokenKey,
    input: InputSize,
}
impl SliderSize {
    fn profile(self) -> Profile {
        match self {
            Self::Small => Profile {
                track: K::SLIDER_TRACK_HEIGHT_SM,
                thumb: K::SLIDER_THUMB_SIZE_SM,
                hit: K::SLIDER_HIT_SIZE_SM,
                font: K::SLIDER_LABEL_FONT_SIZE_SM,
                line: K::SLIDER_LABEL_LINE_HEIGHT_SM,
                input: InputSize::Small,
            },
            Self::Medium => Profile {
                track: K::SLIDER_TRACK_HEIGHT_MD,
                thumb: K::SLIDER_THUMB_SIZE_MD,
                hit: K::SLIDER_HIT_SIZE_MD,
                font: K::SLIDER_LABEL_FONT_SIZE_MD,
                line: K::SLIDER_LABEL_LINE_HEIGHT_MD,
                input: InputSize::Medium,
            },
            Self::Large => Profile {
                track: K::SLIDER_TRACK_HEIGHT_LG,
                thumb: K::SLIDER_THUMB_SIZE_LG,
                hit: K::SLIDER_HIT_SIZE_LG,
                font: K::SLIDER_LABEL_FONT_SIZE_LG,
                line: K::SLIDER_LABEL_LINE_HEIGHT_LG,
                input: InputSize::Large,
            },
        }
    }
}
impl SliderLayout {
    fn axis(self) -> Axis {
        if self == Self::Horizontal {
            Axis::Horizontal
        } else {
            Axis::Vertical
        }
    }
}

/// One coupled presentation sample keeps fill and both endpoints coherent.
#[derive(Clone, Copy, PartialEq)]
struct SliderPosition {
    lower: f32,
    upper: f32,
}
impl gpui_kit::base::Interpolate for SliderPosition {
    fn interpolate(&self, target: &Self, progress: f32) -> Self {
        Self {
            lower: self.lower + (target.lower - self.lower) * progress,
            upper: self.upper + (target.upper - self.upper) * progress,
        }
    }
}

impl RenderOnce for YueSlider {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.default_theme {
            self.theme = crate::current_theme(cx);
        }
        let label = self
            .label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .expect("YueSlider needs a non-empty accessibility_label");
        let (base, disabled, domain) = {
            let s = self.state.read(cx);
            (s.base.clone(), s.disabled, s.domain(cx))
        };
        let scale = f32::from(window.rem_size()) / 16.;
        let profile = self.size.profile();
        let hit = px(pixels(self.theme, profile.hit) * scale);
        let thumb = px(pixels(self.theme, profile.thumb) * scale);
        let ring = px((pixels(self.theme, K::SLIDER_FOCUS_RING_WIDTH)
            + pixels(self.theme, K::SLIDER_FOCUS_RING_OFFSET))
            * scale);
        let inset = (thumb / 2. + ring).max(hit / 2.);
        let vertical = self.direction == SliderLayout::Vertical;
        let buttons = self.controls == SliderControls::Buttons;
        let button_gap = px(pixels(self.theme, K::SLIDER_STEPPER_GAP) * scale);
        let button_inset = if buttons { hit + button_gap } else { px(0.) };
        let minimum_span = px(pixels(self.theme, K::SLIDER_MIN_TRACK_LENGTH) * scale)
            + inset * 2.
            + button_inset * 2.;
        let input_width = if self.controls == SliderControls::None {
            px(0.)
        } else {
            self.input_width(domain, !buttons, window, cx)
        };
        self.state
            .update(cx, |s, cx| s.configure_controls(self.controls, window, cx));
        self.marks
            .retain(|m| m.value >= domain.min() && m.value <= domain.max());
        self.state.update(cx, |s, cx| {
            s.configure_marks(
                self.marks
                    .iter()
                    .filter(|_| self.snap_to_marks)
                    .map(|m| m.value),
                px(pixels(self.theme, K::SLIDER_SNAP_DISTANCE) * scale),
                window,
                cx,
            )
        });
        let value = base.read(cx).value();
        let percentage = base.read(cx).percentage();
        let animate =
            self.state.read(cx).animate_value && !disabled && !base.read(cx).is_dragging();
        let position = transition(
            (self.id.clone(), "value-position"),
            SliderPosition {
                lower: percentage.start,
                upper: percentage.end,
            },
            Transition::new(if animate {
                duration(self.theme, K::SLIDER_STATE_DURATION)
            } else {
                std::time::Duration::ZERO
            })
            .ease(easing(self.theme, K::SLIDER_STATE_EASE)),
            window,
            cx,
        );
        let percentage = position.lower..position.upper;
        let thickness = px(pixels(self.theme, profile.track) * scale);
        let rail = div()
            .absolute()
            .bg(rgba(
                self.theme,
                if disabled {
                    K::SLIDER_DISABLED_TRACK_BACKGROUND
                } else {
                    K::SLIDER_TRACK_BACKGROUND
                },
            ))
            .rounded(rems(self.theme, K::SLIDER_RADIUS))
            .when(!vertical, |b| {
                b.top((hit - thickness) / 2.).w_full().h(thickness)
            })
            .when(vertical, |b| {
                b.left((hit - thickness) / 2.).h_full().w(thickness)
            });
        let fill = div()
            .absolute()
            .bg(rgba(
                self.theme,
                if disabled {
                    K::SLIDER_DISABLED_FILL_BACKGROUND
                } else {
                    K::SLIDER_FILL_BACKGROUND
                },
            ))
            .rounded(rems(self.theme, K::SLIDER_RADIUS))
            .when(!vertical, |b| {
                b.top((hit - thickness) / 2.)
                    .left(relative(percentage.start))
                    .w(relative(percentage.end - percentage.start))
                    .h(thickness)
            })
            .when(vertical, |b| {
                b.left((hit - thickness) / 2.)
                    .top(relative(1. - percentage.end))
                    .h(relative(percentage.end - percentage.start))
                    .w(thickness)
            });
        let mut mapping = SliderIndicator::new(&base)
            .relative()
            .size_full()
            .child(rail)
            .child(fill);
        let mark_size = px(pixels(self.theme, K::SLIDER_MARK_SIZE) * scale);
        for mark in &self.marks {
            let p = ((mark.value as f64 - domain.min() as f64)
                / (domain.max() as f64 - domain.min() as f64)) as f32;
            let selected = if value.is_single() {
                mark.value <= value.end()
            } else {
                mark.value >= value.start() && mark.value <= value.end()
            };
            mapping = mapping.child(
                div()
                    .absolute()
                    .size(mark_size)
                    .rounded(rems(self.theme, K::SLIDER_RADIUS))
                    .bg(rgba(
                        self.theme,
                        if disabled {
                            K::SLIDER_LABEL_COLOR_DISABLED
                        } else if selected {
                            K::SLIDER_MARK_COLOR_ACTIVE
                        } else {
                            K::SLIDER_MARK_COLOR
                        },
                    ))
                    .when(!vertical, |b| {
                        b.left(relative(p))
                            .ml(-mark_size / 2.)
                            .top((hit - mark_size) / 2.)
                    })
                    .when(vertical, |b| {
                        b.top(relative(1. - p))
                            .mt(-mark_size / 2.)
                            .left((hit - mark_size) / 2.)
                    }),
            );
        }
        // Focus nodes stay lower -> upper. Only passive decoration changes paint
        // order, so a coincident active lower thumb cannot reorder Tab navigation.
        let lower = value
            .is_range()
            .then(|| self.thumb(true, percentage.start, &label, hit, thumb, window, cx));
        let (upper_node, upper_paint) =
            self.thumb(false, percentage.end, &label, hit, thumb, window, cx);
        let active_lower = base.read(cx).active_start() == Some(true);
        if let Some((lower_node, lower_paint)) = lower {
            mapping = mapping.child(lower_node).child(upper_node);
            mapping = if active_lower {
                mapping.child(upper_paint).child(lower_paint)
            } else {
                mapping.child(lower_paint).child(upper_paint)
            };
        } else {
            mapping = mapping.child(upper_node).child(upper_paint);
        }
        let slider = Slider::new(&base)
            .thumb_semantics(true)
            .disabled(disabled)
            .axis(self.direction.axis())
            .relative()
            .when(!vertical, |b| b.w_full().h(hit))
            .when(vertical, |b| b.w(hit).h_full())
            .child(
                SliderTrack::new(&base)
                    .disabled(disabled)
                    .axis(self.direction.axis())
                    .relative()
                    .size_full()
                    .when(!vertical, |b| b.px(inset))
                    .when(vertical, |b| b.py(inset))
                    .child(mapping),
            );
        let track = if buttons {
            div()
                .flex()
                .items_center()
                .gap(button_gap)
                .min_w_0()
                .min_h_0()
                .when(!vertical, |b| b.w_full())
                .when(vertical, |b| b.flex_col().h_full())
                .child(self.step_button(vertical, &label, cx))
                .child(slider.flex_1().min_w_0().min_h_0())
                .child(self.step_button(!vertical, &label, cx))
                .into_any_element()
        } else {
            slider.into_any_element()
        };
        let marks_exist = self.marks.iter().any(|m| m.visible && !m.label.is_empty());
        let mut main = div()
            .id((self.id.clone(), "main"))
            .relative()
            .min_w_0()
            .min_h_0()
            .flex_1()
            .flex()
            .when(!vertical, |b| b.flex_col().min_w(minimum_span))
            .when(vertical, |b| b.min_h(minimum_span))
            .child(track);
        if marks_exist {
            main = main.child(self.labels(inset + button_inset, window, cx));
        }
        let mut frame = div()
            .id(self.id.clone())
            .test_support()
            .relative()
            .flex()
            .items_center()
            .gap(rems(self.theme, K::SLIDER_INPUT_GAP))
            .when(!vertical, |b| {
                b.items_start()
                    .flex_wrap()
                    // Reserve the same span as the track. A narrow width must
                    // not place neighboring controls underneath the rail/thumb.
                    .min_w(minimum_span)
                    .w(rems(self.theme, K::SLIDER_WIDTH))
            })
            .when(vertical, |b| {
                b.flex_col()
                    .h(rems(self.theme, K::SLIDER_HEIGHT))
                    .w(rems(self.theme, profile.hit)
                        + if marks_exist {
                            rems(self.theme, K::SLIDER_LABEL_MAX_WIDTH)
                        } else {
                            gpui_kit::rems(0.)
                        })
            })
            .when(self.block, |b| b.w_full())
            .max_w_full()
            .when_some(self.height, |b, h| b.h(h))
            .child(main);
        if vertical && buttons {
            frame = frame.h_auto().min_h(
                self.height
                    .unwrap_or_else(|| rems(self.theme, K::SLIDER_HEIGHT).into()),
            );
        }
        if self.controls != SliderControls::None {
            let (inputs, invalid) = {
                let s = self.state.read(cx);
                (s.inputs.clone(), s.invalid.clone())
            };
            let mut fields = div()
                .id((self.id.clone(), "inputs"))
                .flex()
                .flex_wrap()
                .flex_shrink_0()
                .min_w_0()
                .max_w_full()
                .items_start()
                .gap(rems(self.theme, K::SLIDER_INPUT_GAP));
            for (ix, input) in inputs.iter().enumerate() {
                let endpoint = if ix == 0 {
                    self.lower
                        .clone()
                        .unwrap_or_else(|| format!("{label} lower").into())
                } else {
                    self.upper
                        .clone()
                        .unwrap_or_else(|| format!("{label} upper").into())
                };
                let field_label = if value.is_single() {
                    format!("{label} value")
                } else {
                    format!("{endpoint} value")
                };
                let step = self.state.clone();
                let reset = self.state.clone();
                let error = invalid[ix];
                let number = YueNumberInput::new(
                    (self.id.clone(), if ix == 0 { "input-0" } else { "input-1" }),
                    self.theme,
                    input,
                )
                .accessibility_label(field_label)
                .size(profile.input)
                .controls(!buttons)
                .tab_stop(self.tab_stop)
                .disabled(disabled)
                .width(input_width)
                .status(if error {
                    InputStatus::Error
                } else {
                    InputStatus::Normal
                })
                .when_some(
                    if error {
                        Some(self.input_error.clone())
                    } else {
                        self.description.clone()
                    },
                    |b, d| b.accessibility_description(d),
                )
                .on_step(move |action, w, cx| {
                    step.update(cx, |s, cx| s.step_input(ix, action, w, cx))
                });
                fields = fields.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_none()
                        .w(input_width)
                        .on_key_down(move |e, w, cx| {
                            if e.keystroke.key == "escape" {
                                reset.update(cx, |s, cx| s.reset_draft(ix, w, cx));
                                cx.stop_propagation();
                            }
                        })
                        .child(number),
                );
            }
            if vertical {
                frame = frame.min_w(input_width).h_auto().min_h(
                    self.height
                        .unwrap_or_else(|| rems(self.theme, K::SLIDER_HEIGHT).into()),
                );
            }
            frame = frame.child(fields);
        }
        self.layout.apply(frame)
    }
}

impl YueSlider {
    fn input_width(
        &self,
        domain: SliderDomain,
        controls: bool,
        window: &Window,
        cx: &App,
    ) -> Pixels {
        let scale = f32::from(window.rem_size()) / 16.;
        let font_size = px(pixels(
            self.theme,
            match self.size {
                SliderSize::Small => K::INPUT_FONT_SIZE_SM,
                SliderSize::Medium => K::INPUT_FONT_SIZE_MD,
                SliderSize::Large => K::INPUT_FONT_SIZE_LG,
            },
        ) * scale);
        let digits = number(self.theme, K::SLIDER_INPUT_MIN_DIGITS);
        assert!(
            digits >= 1. && digits.fract() == 0.,
            "slider input minimum digits must be a positive integer"
        );
        let values = [domain.min(), domain.max(), domain.step_value()];
        let decimals = values
            .iter()
            .map(|v| {
                let text = super::state::numeric(*v);
                text.split_once('.')
                    .map_or(0, |(_, fraction)| fraction.len())
            })
            .max()
            .unwrap_or(0);
        let sign = if domain.min() < 0. { "-" } else { "" };
        let fraction = if decimals == 0 {
            String::new()
        } else {
            format!(".{}", "0".repeat(decimals))
        };
        let mut samples = (b'0'..=b'9')
            .map(|digit| {
                format!(
                    "{sign}{}{fraction}",
                    char::from(digit).to_string().repeat(digits as usize)
                )
            })
            .collect::<Vec<_>>();
        let value = self.state.read(cx).value(cx);
        samples.extend(
            [domain.min(), domain.max(), value.start(), value.end()].map(super::state::numeric),
        );
        let font = window.text_style().font();
        let text_width = samples
            .into_iter()
            .map(|text| {
                window
                    .text_system()
                    .shape_line(
                        text.clone().into(),
                        font_size,
                        &[TextRun {
                            len: text.len(),
                            font: font.clone(),
                            ..Default::default()
                        }],
                        None,
                    )
                    .width()
            })
            .fold(px(0.), Pixels::max);
        let border = px(pixels(self.theme, K::INPUT_BORDER_WIDTH) * scale);
        let padding = px(pixels(self.theme, K::INPUT_NUMBER_PADDING_INLINE) * scale);
        // Leave one caret width inside the editor, then account for both borders,
        // number-specific padding and optional icons. Invalid drafts never resize it.
        px(f32::from(text_width + border).ceil())
            + (padding + border) * 2.
            + if controls { font_size * 2. } else { px(0.) }
    }

    fn step_button(&self, increase: bool, label: &SharedString, cx: &App) -> AnyElement {
        let action = if increase {
            gpui_kit::base::StepAction::Increment
        } else {
            gpui_kit::base::StepAction::Decrement
        };
        let s = self.state.read(cx);
        let endpoint = if s.is_range(cx) {
            if s.step_target(cx) == 0 {
                self.lower
                    .clone()
                    .unwrap_or_else(|| format!("{label} lower").into())
            } else {
                self.upper
                    .clone()
                    .unwrap_or_else(|| format!("{label} upper").into())
            }
        } else {
            label.clone()
        };
        let focus = &s
            .step_focus
            .as_ref()
            .expect("button mode prepares its focus handles")[usize::from(increase)];
        let state = self.state.clone();
        YueButton::icon(
            (
                self.id.clone(),
                if increase { "increase" } else { "decrease" },
            ),
            self.theme,
            if increase {
                YueIcon::Plus
            } else {
                YueIcon::Minus
            },
            format!(
                "{endpoint} {}",
                if increase { "increase" } else { "decrease" }
            ),
        )
        .variant(ButtonVariant::Ghost)
        .size(match self.size {
            SliderSize::Small => ButtonSize::Small,
            SliderSize::Medium => ButtonSize::Medium,
            SliderSize::Large => ButtonSize::Large,
        })
        .disabled(!s.can_step(action, cx))
        .track_focus(focus)
        .tab_index(self.tab_index)
        .tab_stop(self.tab_stop)
        .when_some(self.description.clone(), |b, text| {
            b.accessibility_description(text)
        })
        .on_click(move |_, window, cx| state.update(cx, |s, cx| s.step_buttons(action, window, cx)))
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn thumb(
        &self,
        start: bool,
        p: f32,
        label: &SharedString,
        hit: Pixels,
        diameter: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> (AnyElement, AnyElement) {
        let ix = usize::from(!start);
        let (base, focus, disabled, hovered, closed, dragging, active, value) = {
            let s = self.state.read(cx);
            (
                s.base.clone(),
                s.focus[ix].clone(),
                s.disabled,
                s.hovered[ix],
                s.tooltip_closed[ix],
                s.is_dragging(cx),
                s.base.read(cx).active_start(),
                s.value(cx),
            )
        };
        let pressed = !disabled && dragging && active == Some(start);
        let scale = f32::from(window.rem_size()) / 16.;
        let suffix = if start { "lower" } else { "upper" };
        let hover = transition(
            (self.id.clone(), format!("hover-{suffix}")),
            if !disabled && hovered { 1.0_f32 } else { 0. },
            Transition::new(duration(self.theme, K::SLIDER_STATE_DURATION))
                .ease(easing(self.theme, K::SLIDER_STATE_EASE)),
            window,
            cx,
        );
        let shrink = transition(
            (self.id.clone(), format!("press-{suffix}")),
            if pressed {
                number(self.theme, K::SLIDER_THUMB_SCALE_PRESSED)
            } else {
                1.
            },
            Transition::new(duration(
                self.theme,
                if pressed {
                    K::SLIDER_STATE_DURATION
                } else {
                    K::SLIDER_RELEASE_DURATION
                },
            ))
            .ease(easing(self.theme, K::SLIDER_STATE_EASE)),
            window,
            cx,
        );
        let display_scale = window.scale_factor();
        let painted =
            px((f32::from(diameter) * shrink * display_scale / 2.).round() * 2. / display_scale);
        let halo = px(pixels(self.theme, K::SLIDER_THUMB_HALO_SIZE) * scale);
        let focus_width = px(pixels(self.theme, K::SLIDER_FOCUS_RING_WIDTH) * scale);
        let focus_offset = px(pixels(self.theme, K::SLIDER_FOCUS_RING_OFFSET) * scale);
        let focus_visible =
            !disabled && focus.is_focused(window) && window.last_input_was_keyboard();
        let border = if disabled {
            rgba(self.theme, K::SLIDER_DISABLED_THUMB_BORDER_COLOR)
        } else if pressed {
            rgba(self.theme, K::SLIDER_THUMB_BORDER_COLOR_ACTIVE)
        } else {
            mix_rgba(
                rgba(self.theme, K::SLIDER_THUMB_BORDER_COLOR),
                rgba(self.theme, K::SLIDER_THUMB_BORDER_COLOR_HOVER),
                hover,
            )
        };
        let surface = div()
            .relative()
            .size(hit)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .size(diameter + halo * 2.)
                    .rounded(rems(self.theme, K::SLIDER_THUMB_RADIUS))
                    .bg(rgba(self.theme, K::SLIDER_THUMB_HALO_COLOR).opacity(hover)),
            )
            .child(
                div()
                    .absolute()
                    .size(diameter + (focus_width + focus_offset) * 2.)
                    .rounded(rems(self.theme, K::SLIDER_THUMB_RADIUS))
                    .border(focus_width)
                    .border_color(if focus_visible {
                        rgba(self.theme, K::SLIDER_FOCUS_RING_COLOR)
                    } else {
                        gpui_kit::rgba(0)
                    }),
            )
            .child(
                div()
                    .size(painted)
                    .rounded(rems(self.theme, K::SLIDER_THUMB_RADIUS))
                    .bg(rgba(
                        self.theme,
                        if disabled {
                            K::SLIDER_DISABLED_THUMB_BACKGROUND
                        } else {
                            K::SLIDER_THUMB_BACKGROUND
                        },
                    ))
                    .border(rems(self.theme, K::SLIDER_THUMB_BORDER_WIDTH))
                    .border_color(border)
                    .when(!disabled, |b| {
                        b.shadow(shadow(self.theme, K::SLIDER_THUMB_SHADOW))
                    }),
            );
        let name = if value.is_single() {
            label.clone()
        } else if start {
            self.lower
                .clone()
                .unwrap_or_else(|| format!("{label} lower").into())
        } else {
            self.upper
                .clone()
                .unwrap_or_else(|| format!("{label} upper").into())
        };
        let current = if start { value.start() } else { value.end() };
        let text = (self.format)(current);
        let hovered_state = self.state.clone();
        let thumb = SliderThumb::new(&base)
            .axis(self.direction.axis())
            .start(start)
            .disabled(disabled)
            .track_focus(&focus)
            .tab_index(self.tab_index)
            .tab_stop(self.tab_stop)
            .accessibility_label(name)
            .when_some(self.description.clone(), |b, v| {
                b.accessibility_description(v)
            })
            .value_text(text.clone())
            .size(hit)
            .relative()
            .when(!disabled, |b| b.cursor_pointer())
            .on_hover(move |hovered, _, cx| {
                hovered_state.update(cx, |s, cx| {
                    if s.hovered[ix] != *hovered {
                        s.hovered[ix] = *hovered;
                        if *hovered && !s.is_dragging(cx) {
                            s.tooltip_closed[ix] = false;
                        }
                        cx.notify();
                    }
                })
            });
        let notify = self.state.clone();
        let tooltip = YueTooltip::new(
            (self.id.clone(), format!("value-{suffix}")),
            self.theme,
            text,
        )
        .trigger_mode(TooltipTrigger::HoverFocus)
        .track_focus(&focus)
        .dismiss_on_press(false)
        .anchor_inset(gpui_kit::rems(
            f32::from(hit - diameter) / (2. * f32::from(window.rem_size())),
        ))
        .gap(rems(self.theme, K::SLIDER_TOOLTIP_GAP))
        .placement(if self.direction == SliderLayout::Vertical {
            TooltipPlacement::Left
        } else {
            TooltipPlacement::Top
        })
        .disabled(disabled || self.tooltip == SliderTooltip::None)
        .when(closed, |b| b.open(false))
        .when(pressed && !closed, |b| b.open(true))
        .on_open_change(move |open, _, cx| {
            notify.update(cx, |s, cx| {
                if !*open && !s.tooltip_closed[ix] {
                    s.tooltip_closed[ix] = true;
                    cx.notify();
                }
            })
        })
        .trigger(thumb);
        let node = div()
            .id((self.id.clone(), format!("handle-{suffix}")))
            .absolute()
            .when(self.direction == SliderLayout::Horizontal, |b| {
                b.left(relative(p)).ml(-hit / 2.)
            })
            .when(self.direction == SliderLayout::Vertical, |b| {
                b.top(relative(1. - p)).mt(-hit / 2.)
            })
            .child(tooltip)
            .into_any_element();
        let paint = div()
            .absolute()
            .when(self.direction == SliderLayout::Horizontal, |b| {
                b.left(relative(p)).ml(-hit / 2.)
            })
            .when(self.direction == SliderLayout::Vertical, |b| {
                b.top(relative(1. - p)).mt(-hit / 2.)
            })
            .child(surface)
            .into_any_element();
        (node, paint)
    }

    fn labels(&self, inset: Pixels, window: &mut Window, cx: &mut App) -> AnyElement {
        let axis = self.direction.axis();
        let vertical = self.direction == SliderLayout::Vertical;
        let profile = self.size.profile();
        let scale = f32::from(window.rem_size()) / 16.;
        let font_size = px(pixels(self.theme, profile.font) * scale);
        let line = px(pixels(self.theme, profile.line) * scale);
        let width = px(pixels(self.theme, K::SLIDER_LABEL_MAX_WIDTH) * scale);
        let (span, domain, value, disabled) = {
            let s = self.state.read(cx);
            (
                if s.label_axis == axis {
                    s.label_span
                } else {
                    px(0.)
                },
                s.domain(cx),
                s.value(cx),
                s.disabled,
            )
        };
        let font = window.text_style().font();
        let marks = self
            .marks
            .iter()
            .filter(|m| m.visible && !m.label.is_empty())
            .collect::<Vec<_>>();
        let mut positions = vec![];
        let mut extents = vec![];
        for mark in &marks {
            let p = ((mark.value as f64 - domain.min() as f64)
                / (domain.max() as f64 - domain.min() as f64)) as f32;
            positions.push(f32::from(span) * if vertical { 1. - p } else { p });
            let shaped = window.text_system().shape_line(
                mark.label.clone(),
                font_size,
                &[TextRun {
                    len: mark.label.len(),
                    font: font.clone(),
                    ..Default::default()
                }],
                None,
            );
            extents.push(if vertical {
                f32::from(line)
            } else {
                f32::from(shaped.width())
            });
        }
        let slots = label_slots(
            f32::from(span),
            &positions,
            &extents,
            pixels(self.theme, K::SLIDER_LABEL_MIN_GAP) * scale,
        );
        let measured = self.state.clone();
        let observer = canvas(
            move |bounds, _, cx| {
                let next = if vertical {
                    bounds.size.height
                } else {
                    bounds.size.width
                };
                if measured.read(cx).label_span != next || measured.read(cx).label_axis != axis {
                    measured.update(cx, |s, _| {
                        s.label_span = next;
                        s.label_axis = axis;
                    });
                    cx.defer(move |cx| measured.update(cx, |_, cx| cx.notify()));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let mut body = div()
            .id((self.id.clone(), "labels"))
            .test_support()
            .relative()
            .when(!vertical, |b| b.w_full().h(line))
            .when(vertical, |b| b.h_full().w(width))
            .child(observer);
        for (ix, mark) in marks.iter().enumerate() {
            let Some((offset, extent)) = slots[ix] else {
                continue;
            };
            let selected = if value.is_single() {
                mark.value <= value.end()
            } else {
                mark.value >= value.start() && mark.value <= value.end()
            };
            let token = if disabled {
                K::SLIDER_LABEL_COLOR_DISABLED
            } else if selected {
                K::SLIDER_LABEL_COLOR_ACTIVE
            } else {
                K::SLIDER_LABEL_COLOR
            };
            let cell = div()
                .absolute()
                .text_size(font_size)
                .line_height(line)
                .text_color(rgba(self.theme, token))
                .when(!vertical, |b| b.left(px(offset)).w(px(extent)).h(line))
                .when(vertical, |b| b.top(px(offset)).h(px(extent)).w(width))
                .child(
                    YueOverflowTooltip::new(
                        (
                            self.id.clone(),
                            format!("mark-{:08x}", mark.value.to_bits()),
                        ),
                        self.theme,
                        mark.label.clone(),
                    )
                    .width(if vertical { width } else { px(extent) }),
                );
            body = body.child(cell);
        }
        div()
            .when(!vertical, |b| {
                b.mx(inset).pt(rems(self.theme, K::SLIDER_LABEL_GAP))
            })
            .when(vertical, |b| {
                b.py(inset).pl(rems(self.theme, K::SLIDER_LABEL_GAP))
            })
            .child(body)
            .into_any_element()
    }
}

/// Greedy collision layout with endpoint priority. Returns offset and allocated
/// main-axis extent; hidden labels retain their independent tick marks.
fn label_slots(span: f32, positions: &[f32], extents: &[f32], gap: f32) -> Vec<Option<(f32, f32)>> {
    let mut slots = vec![None; positions.len()];
    if span <= 0. || positions.is_empty() {
        return slots;
    }
    let mut order = (0..positions.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| positions[*a].total_cmp(&positions[*b]));
    let first = order[0];
    let last = *order.last().unwrap();
    let w = extents[first].min(span);
    slots[first] = Some(((positions[first] - w / 2.).clamp(0., span - w), w));
    if first != last {
        let w = extents[last].min(span);
        let x = (positions[last] - w / 2.).clamp(0., span - w);
        if x < slots[first].unwrap().0 + slots[first].unwrap().1 + gap {
            let split = ((positions[first] + positions[last]) / 2.).clamp(0., span);
            let left = (split - gap / 2.).max(0.);
            let right = (split + gap / 2.).min(span);
            let a = extents[first].min(left);
            let b = extents[last].min(span - right);
            slots[first] =
                (a > 0.).then_some(((positions[first] - a / 2.).clamp(0., (left - a).max(0.)), a));
            slots[last] = (b > 0.).then_some((
                (positions[last] - b / 2.).clamp(right, (span - b).max(right)),
                b,
            ));
        } else {
            slots[last] = Some((x, w));
        }
    }
    for ix in order
        .into_iter()
        .skip(1)
        .take(positions.len().saturating_sub(2))
    {
        let w = extents[ix];
        if w > span {
            continue;
        }
        let x = (positions[ix] - w / 2.).clamp(0., span - w);
        if slots
            .iter()
            .flatten()
            .all(|(a, b)| x + w + gap <= *a || x >= a + b + gap)
        {
            slots[ix] = Some((x, w));
        }
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_keep_endpoints_and_never_overlap() {
        let slots = label_slots(100., &[0., 20., 40., 60., 100.], &[40.; 5], 8.);
        assert!(slots[0].is_some() && slots[4].is_some());
        assert!(slots[1..4].iter().all(Option::is_none));
        let tiny = label_slots(20., &[0., 20.], &[100., 100.], 8.);
        assert!(tiny[0].unwrap().0 + tiny[0].unwrap().1 + 8. <= tiny[1].unwrap().0);
    }
    #[test]
    fn a_zero_span_hides_labels_without_panicking() {
        assert_eq!(label_slots(0., &[0.], &[20.], 8.), vec![None]);
    }
}
