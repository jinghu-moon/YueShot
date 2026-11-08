use super::*;
use std::time::Duration;

use gpui_kit::base::{Button, Interpolate, TestSupportExt as _, Toggle, Transition, transition};
use gpui_kit::{
    AnyElement, DefiniteLength, Entity, FontWeight, InteractiveElement, Length, MouseButton,
    ParentElement, Rems, RenderOnce, Role, StatefulInteractiveElement, Styled, canvas, div,
    prelude::FluentBuilder as _, relative,
};

use crate::theme::{TokenKey, keys};
use crate::tokens::{duration, easing, mix_rgba, number, pixels, rems, rgba};
use crate::{Accent, NeutralTone, ThemeMode, outline};

impl TagTone {
    fn colors(self, variant: TagVariant, theme: YueTheme) -> TagColors {
        use TagTone::*;
        use TagVariant::*;
        // Static keys keep the full source matrix auditable and avoid catalog
        // name construction in render. There are no substitute color defaults.
        let (background, foreground, border) = match (self, variant) {
            (Default, Filled) => (
                keys::TAG_DEFAULT_FILLED_BACKGROUND,
                keys::TAG_DEFAULT_FILLED_COLOR,
                keys::TAG_DEFAULT_FILLED_BORDER_COLOR,
            ),
            (Default, Tint) => (
                keys::TAG_DEFAULT_TINT_BACKGROUND,
                keys::TAG_DEFAULT_TINT_COLOR,
                keys::TAG_DEFAULT_TINT_BORDER_COLOR,
            ),
            (Default, Outline) => (
                keys::TAG_DEFAULT_OUTLINE_BACKGROUND,
                keys::TAG_DEFAULT_OUTLINE_COLOR,
                keys::TAG_DEFAULT_OUTLINE_BORDER_COLOR,
            ),
            (Default, TintOutline) => (
                keys::TAG_DEFAULT_TINT_OUTLINE_BACKGROUND,
                keys::TAG_DEFAULT_TINT_OUTLINE_COLOR,
                keys::TAG_DEFAULT_TINT_OUTLINE_BORDER_COLOR,
            ),
            (Primary, Filled) => (
                keys::TAG_PRIMARY_FILLED_BACKGROUND,
                keys::TAG_PRIMARY_FILLED_COLOR,
                keys::TAG_PRIMARY_FILLED_BORDER_COLOR,
            ),
            (Primary, Tint) => (
                keys::TAG_PRIMARY_TINT_BACKGROUND,
                keys::TAG_PRIMARY_TINT_COLOR,
                keys::TAG_PRIMARY_TINT_BORDER_COLOR,
            ),
            (Primary, Outline) => (
                keys::TAG_PRIMARY_OUTLINE_BACKGROUND,
                keys::TAG_PRIMARY_OUTLINE_COLOR,
                keys::TAG_PRIMARY_OUTLINE_BORDER_COLOR,
            ),
            (Primary, TintOutline) => (
                keys::TAG_PRIMARY_TINT_OUTLINE_BACKGROUND,
                keys::TAG_PRIMARY_TINT_OUTLINE_COLOR,
                keys::TAG_PRIMARY_TINT_OUTLINE_BORDER_COLOR,
            ),
            (Success, Filled) => (
                keys::TAG_SUCCESS_FILLED_BACKGROUND,
                keys::TAG_SUCCESS_FILLED_COLOR,
                keys::TAG_SUCCESS_FILLED_BORDER_COLOR,
            ),
            (Success, Tint) => (
                keys::TAG_SUCCESS_TINT_BACKGROUND,
                keys::TAG_SUCCESS_TINT_COLOR,
                keys::TAG_SUCCESS_TINT_BORDER_COLOR,
            ),
            (Success, Outline) => (
                keys::TAG_SUCCESS_OUTLINE_BACKGROUND,
                keys::TAG_SUCCESS_OUTLINE_COLOR,
                keys::TAG_SUCCESS_OUTLINE_BORDER_COLOR,
            ),
            (Success, TintOutline) => (
                keys::TAG_SUCCESS_TINT_OUTLINE_BACKGROUND,
                keys::TAG_SUCCESS_TINT_OUTLINE_COLOR,
                keys::TAG_SUCCESS_TINT_OUTLINE_BORDER_COLOR,
            ),
            (Warning, Filled) => (
                keys::TAG_WARNING_FILLED_BACKGROUND,
                keys::TAG_WARNING_FILLED_COLOR,
                keys::TAG_WARNING_FILLED_BORDER_COLOR,
            ),
            (Warning, Tint) => (
                keys::TAG_WARNING_TINT_BACKGROUND,
                keys::TAG_WARNING_TINT_COLOR,
                keys::TAG_WARNING_TINT_BORDER_COLOR,
            ),
            (Warning, Outline) => (
                keys::TAG_WARNING_OUTLINE_BACKGROUND,
                keys::TAG_WARNING_OUTLINE_COLOR,
                keys::TAG_WARNING_OUTLINE_BORDER_COLOR,
            ),
            (Warning, TintOutline) => (
                keys::TAG_WARNING_TINT_OUTLINE_BACKGROUND,
                keys::TAG_WARNING_TINT_OUTLINE_COLOR,
                keys::TAG_WARNING_TINT_OUTLINE_BORDER_COLOR,
            ),
            (Danger, Filled) => (
                keys::TAG_DANGER_FILLED_BACKGROUND,
                keys::TAG_DANGER_FILLED_COLOR,
                keys::TAG_DANGER_FILLED_BORDER_COLOR,
            ),
            (Danger, Tint) => (
                keys::TAG_DANGER_TINT_BACKGROUND,
                keys::TAG_DANGER_TINT_COLOR,
                keys::TAG_DANGER_TINT_BORDER_COLOR,
            ),
            (Danger, Outline) => (
                keys::TAG_DANGER_OUTLINE_BACKGROUND,
                keys::TAG_DANGER_OUTLINE_COLOR,
                keys::TAG_DANGER_OUTLINE_BORDER_COLOR,
            ),
            (Danger, TintOutline) => (
                keys::TAG_DANGER_TINT_OUTLINE_BACKGROUND,
                keys::TAG_DANGER_TINT_OUTLINE_COLOR,
                keys::TAG_DANGER_TINT_OUTLINE_BORDER_COLOR,
            ),
        };
        TagColors::new(
            rgba(theme, background),
            rgba(theme, foreground),
            rgba(theme, border),
        )
    }
}

impl TagSize {
    pub(super) fn metrics(self) -> (TokenKey, TokenKey, TokenKey) {
        match self {
            Self::Small => (
                keys::TAG_HEIGHT_SM,
                keys::TAG_PADDING_INLINE_SM,
                keys::TAG_FONT_SIZE_SM,
            ),
            Self::Medium => (
                keys::TAG_HEIGHT_MD,
                keys::TAG_PADDING_INLINE_MD,
                keys::TAG_FONT_SIZE_MD,
            ),
            Self::Large => (
                keys::TAG_HEIGHT_LG,
                keys::TAG_PADDING_INLINE_LG,
                keys::TAG_FONT_SIZE_LG,
            ),
        }
    }
}

impl Interpolate for TagColors {
    fn interpolate(&self, target: &Self, progress: f32) -> Self {
        Self {
            background: mix_rgba(self.background, target.background, progress),
            foreground: mix_rgba(self.foreground, target.foreground, progress),
            border: mix_rgba(self.border, target.border, progress),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Feedback {
    hovered: bool,
    pressed: bool,
}

#[derive(Default)]
struct TagMotion {
    body: Feedback,
    close: Feedback,
    theme: Option<(ThemeMode, Accent, NeutralTone)>,
    disabled: bool,
    kind: Option<std::mem::Discriminant<Behavior>>,
}

// One reversible timeline keeps both the viewport and the actual path
// continuous on interruption: open the slot, then ink the check; erase the
// check before collapsing the slot. No invisible space remains at rest.
pub(super) fn check_progress(progress: f32, theme: YueTheme) -> (f32, f32) {
    let reveal = duration(theme, keys::TAG_DURATION).as_secs_f32();
    let draw = duration(theme, keys::CHECKBOX_DURATION).as_secs_f32();
    let elapsed = progress.clamp(0., 1.) * (reveal + draw);
    let slot = if reveal > 0. {
        (elapsed / reveal).clamp(0., 1.)
    } else {
        progress
    };
    let ink = if draw > 0. {
        ((elapsed - reveal) / draw).clamp(0., 1.)
    } else {
        progress
    };
    (
        easing(theme, keys::TAG_EASE)(slot),
        easing(theme, keys::CHECKBOX_EASE_ENTER)(ink),
    )
}

fn rounded<T: Styled>(
    element: T,
    shape: TagShape,
    theme: YueTheme,
    leading: bool,
    trailing: bool,
) -> T {
    let radius = rems(
        theme,
        if shape == TagShape::Rounded {
            keys::TAG_BORDER_RADIUS
        } else {
            keys::TAG_BORDER_RADIUS_ROUND
        },
    );
    let zero = gpui_kit::rems(0.);
    let left = if leading { radius } else { zero };
    let right = if trailing && shape != TagShape::Mark {
        radius
    } else {
        zero
    };
    element
        .rounded_tl(left)
        .rounded_bl(left)
        .rounded_tr(right)
        .rounded_br(right)
}

fn bind_feedback<T: StatefulInteractiveElement>(
    element: T,
    state: &Entity<TagMotion>,
    close: bool,
    window: &Window,
) -> T {
    element
        .on_hover(
            window.listener_for(state, move |state, hovered, window, cx| {
                let feedback = if close {
                    &mut state.close
                } else {
                    &mut state.body
                };
                let next = Feedback {
                    hovered: *hovered,
                    pressed: feedback.pressed && *hovered,
                };
                if *feedback != next {
                    *feedback = next;
                    cx.notify();
                    window.refresh();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(state, move |state, _, window, cx| {
                let feedback = if close {
                    &mut state.close
                } else {
                    &mut state.body
                };
                if !feedback.pressed {
                    feedback.pressed = true;
                    cx.notify();
                    window.refresh();
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(state, move |state, _, window, cx| {
                let feedback = if close {
                    &mut state.close
                } else {
                    &mut state.body
                };
                if feedback.pressed {
                    feedback.pressed = false;
                    cx.notify();
                    window.refresh();
                }
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            window.listener_for(state, move |state, _, window, cx| {
                let feedback = if close {
                    &mut state.close
                } else {
                    &mut state.body
                };
                if *feedback != Feedback::default() {
                    *feedback = Feedback::default();
                    cx.notify();
                    window.refresh();
                }
            }),
        )
}

fn focus_ring(
    focus: Option<&FocusHandle>,
    disabled: bool,
    window: &Window,
    theme: YueTheme,
    shape: TagShape,
    leading: bool,
    trailing: bool,
) -> Option<gpui_kit::Div> {
    if disabled || !window.last_input_was_keyboard() || !focus.is_some_and(|f| f.is_focused(window))
    {
        return None;
    }
    let outside =
        rems(theme, keys::TAG_FOCUS_RING_OFFSET) + rems(theme, keys::TAG_FOCUS_RING_WIDTH);
    Some(rounded(
        div()
            .absolute()
            .top(-outside)
            .right(-outside)
            .bottom(-outside)
            .left(-outside)
            .border(rems(theme, keys::TAG_FOCUS_RING_WIDTH))
            .border_color(rgba(theme, keys::TAG_FOCUS_RING_COLOR)),
        shape,
        theme,
        leading,
        trailing,
    ))
}

fn wash(foreground: Rgba, amount: f32) -> Rgba {
    Rgba {
        a: foreground.a * amount,
        ..foreground
    }
}

impl RenderOnce for YueTag {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.validate();
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let interactive = self.behavior != Behavior::Static;
        let checkable = matches!(self.behavior, Behavior::Checkable(_));
        let checked = matches!(self.behavior, Behavior::Checkable(true));
        let closable = self.on_close.is_some();
        let body_id: ElementId = (self.id.clone(), "body").into();
        let close_id: ElementId = (self.id.clone(), "close").into();
        let focus = interactive.then(|| {
            self.focus.clone().unwrap_or_else(|| {
                window
                    .use_keyed_state((self.id.clone(), "tag-body-focus"), cx, |_, cx| {
                        cx.focus_handle()
                    })
                    .read(cx)
                    .clone()
            })
        });
        let close_focus = closable.then(|| {
            self.close_focus.clone().unwrap_or_else(|| {
                window
                    .use_keyed_state((self.id.clone(), "tag-close-focus"), cx, |_, cx| {
                        cx.focus_handle()
                    })
                    .read(cx)
                    .clone()
            })
        });
        let state = window.use_keyed_state((self.id.clone(), "tag-motion"), cx, |_, _| {
            TagMotion::default()
        });
        let identity = Some((theme.mode(), theme.accent(), theme.neutral()));
        let changed_theme = state.read(cx).theme != identity;
        let changed_disabled = state.read(cx).disabled != self.disabled;
        let kind = Some(std::mem::discriminant(&self.behavior));
        let changed_kind = state.read(cx).kind != kind;
        if changed_theme || changed_disabled || changed_kind {
            state.update(cx, |state, _| {
                state.theme = identity;
                state.disabled = self.disabled;
                state.kind = kind;
                state.body = Feedback::default();
                state.close = Feedback::default();
            });
        }
        let policy = Transition::new(
            if self.disabled || changed_theme || changed_disabled || changed_kind {
                Duration::ZERO
            } else {
                duration(theme, keys::TAG_DURATION)
            },
        )
        .ease(easing(theme, keys::TAG_EASE));
        let target = self.colors.unwrap_or_else(|| {
            let tone = if checkable && !checked {
                TagTone::Default
            } else {
                self.tone
            };
            tone.colors(self.variant, theme)
        });
        let colors = transition(
            (self.id.clone(), "tag-colors"),
            target,
            policy.clone(),
            window,
            cx,
        );
        let check_timeline = transition(
            (self.id.clone(), "tag-check"),
            if checked { 1.0_f32 } else { 0.0 },
            Transition::new(
                if self.disabled || changed_theme || changed_disabled || changed_kind {
                    Duration::ZERO
                } else {
                    duration(theme, keys::TAG_DURATION) + duration(theme, keys::CHECKBOX_DURATION)
                },
            )
            .ease(|progress| progress),
            window,
            cx,
        );
        let (mark, check_ink) = check_progress(check_timeline, theme);
        let feedback = state.read(cx).body;
        let hover = number(theme, keys::OPACITY_HOVER);
        let pressed = number(theme, keys::OPACITY_PRESSED);
        let amount = |feedback: Feedback| {
            if self.disabled {
                0.
            } else if feedback.pressed {
                pressed
            } else if feedback.hovered {
                hover
            } else {
                0.
            }
        };
        let body_wash = transition(
            (self.id.clone(), "tag-body-wash"),
            if interactive { amount(feedback) } else { 0. },
            policy.clone(),
            window,
            cx,
        );
        let close_wash = transition(
            (self.id.clone(), "tag-close-wash"),
            amount(state.read(cx).close),
            policy,
            window,
            cx,
        );

        let (height_key, padding_key, font_key) = self.size.metrics();
        let height = rems(theme, height_key);
        let padding = rems(theme, padding_key);
        let font = rems(theme, font_key);
        let icon_size = rems(theme, keys::TAG_CLOSE_ICON_SIZE);
        let gap = rems(theme, keys::TAG_GAP);
        let border = rems(theme, keys::TAG_BORDER_WIDTH);
        let icon_slot = self.leading.is_some() || checkable;
        // Only real content consumes space. A check without a supplied leading
        // icon reveals its own width and gap; an unchecked tag has neither a
        // leading placeholder nor a compensating trailing spacer.
        let icon_progress = if self.leading.is_some() { 1. } else { mark };
        let icon_width = icon_size * icon_progress;
        let icon_gap = gap * icon_progress;
        // Reserve a full em for an ellipsis even with a very narrow external
        // constraint. The visible icon and the close hit region never shrink.
        let body_minimum = padding
            + if closable {
                gpui_kit::rems(0.)
            } else {
                padding
            }
            + border * 2.
            + font
            + if icon_slot {
                icon_width + icon_gap
            } else {
                gpui_kit::rems(0.)
            };
        let minimum = body_minimum + if closable { height } else { gpui_kit::rems(0.) };
        let minimum = match self.layout.min_width {
            Some(Length::Definite(DefiniteLength::Absolute(value))) => value
                .to_pixels(window.rem_size())
                .max(minimum.to_pixels(window.rem_size())),
            _ => minimum.to_pixels(window.rem_size()),
        };
        let ring = focus_ring(
            focus.as_ref(),
            self.disabled,
            window,
            theme,
            self.shape,
            true,
            !closable,
        );
        let mut content = div()
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .flex_1()
            .gap(icon_gap);
        if icon_slot {
            let mut slot = div()
                .relative()
                .w(icon_width)
                .h(icon_size)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                // Clip only the decorative reveal viewport, never the body or
                // its focus ring. The glyph itself keeps its token size.
                .overflow_hidden();
            if let Some(icon) = self.leading {
                slot = slot.child(
                    div()
                        .absolute()
                        .inset_0()
                        .opacity(if checkable { 1. - mark } else { 1. })
                        .child(icon.size(icon_size).color_value(colors.foreground)),
                );
            }
            if checkable {
                // Optical correction: the checkbox path occupies half its
                // viewbox width. Fill two thirds of the same Tag icon slot.
                let unit = f32::from(icon_size.to_pixels(window.rem_size()))
                    / crate::icon::MARK_BOX
                    * (4. / 3.);
                let stroke = pixels(theme, keys::CHECKBOX_MARK_STROKE_WIDTH_MD)
                    * f32::from(window.rem_size())
                    / 16.;
                let color = colors.foreground.into();
                slot = slot.child(
                    // Fixed drawing bounds: expanding the viewport must not
                    // rescale or recenter the check's 16-unit coordinate system.
                    canvas(
                        move |_, _, _| (),
                        move |bounds, _, window, _| {
                            crate::icon::paint_mark(
                                window,
                                bounds,
                                &crate::icon::CHECK_LINE,
                                check_ink,
                                unit,
                                stroke,
                                color,
                            );
                        },
                    )
                    .absolute()
                    .left_0()
                    .top_0()
                    .size(icon_size),
                );
            }
            content = content.child(slot);
        }
        let label = div()
            .id((self.id.clone(), "label"))
            .test_support()
            .min_w_0()
            .flex_1()
            .child(if self.overflow_tooltip && !self.disabled {
                let mut text = crate::YueOverflowTooltip::new(
                    (self.id.clone(), "overflow"),
                    theme,
                    self.label.clone(),
                );
                if let Some(focus) = &focus {
                    text = text.track_focus(focus);
                }
                text.into_any_element()
            } else {
                div()
                    .text_ellipsis()
                    .child(self.label.clone())
                    .into_any_element()
            });
        content = content.child(label);

        let style_body = BodyStyle {
            theme,
            minimum: body_minimum,
            padding,
            border,
            closable,
            shape: self.shape,
            background: wash(colors.foreground, body_wash),
        };
        let name = self.name.unwrap_or_else(|| self.label.clone());
        let body: AnyElement = match self.behavior {
            Behavior::Static => {
                // Caption carries the complete label but is not a focus target.
                let element = div()
                    .id(body_id)
                    .test_support()
                    .role(Role::Caption)
                    .aria_label(name)
                    .when_some(self.description, |d, text| d.aria_description(text));
                style_body.apply(element).child(content).into_any_element()
            }
            Behavior::Action => {
                let handler = self.on_click.expect("validated action handler");
                let mut button = Button::new(body_id)
                    .accessibility_label(name)
                    .disabled(self.disabled)
                    .track_focus(focus.as_ref().unwrap())
                    .tab_index(self.tab_index)
                    .tab_stop(self.tab_stop)
                    .when_some(self.description, |d, text| d.aria_description(text))
                    .when_some(self.key_context.clone(), |d, context| {
                        d.key_context(context.as_ref())
                    })
                    .on_click(move |event, w, cx| handler(event, w, cx));
                if !self.disabled {
                    button = bind_feedback(button, &state, false, window);
                }
                style_body
                    .apply(button)
                    .child(content)
                    .children(ring)
                    .into_any_element()
            }
            Behavior::Checkable(value) => {
                let handler = self.on_change.expect("validated change handler");
                let mut toggle = Toggle::new(body_id)
                    .pressed(value)
                    .accessibility_label(name)
                    .disabled(self.disabled)
                    .track_focus(focus.as_ref().unwrap())
                    .tab_index(self.tab_index)
                    .tab_stop(self.tab_stop)
                    .when_some(self.description, |d, text| d.aria_description(text))
                    .when_some(self.key_context.clone(), |d, context| {
                        d.key_context(context.as_ref())
                    })
                    .on_change(move |next, event, w, cx| handler(next, event, w, cx));
                if !self.disabled {
                    toggle = bind_feedback(toggle, &state, false, window);
                }
                style_body
                    .apply(toggle)
                    .child(content)
                    .children(ring)
                    .into_any_element()
            }
        };
        let mut root = rounded(
            div()
                .id(self.id)
                .test_support()
                .relative()
                .flex()
                .items_center()
                .self_start()
                .h(height)
                .min_h(height)
                .max_h(height)
                .max_w_full()
                .text_size(font)
                .line_height(relative(number(theme, keys::TAG_LINE_HEIGHT)))
                .font_weight(FontWeight(number(theme, keys::TAG_FONT_WEIGHT)))
                .text_color(colors.foreground)
                .bg(colors.background)
                .cursor_default()
                .opacity(if self.disabled {
                    number(theme, keys::TAG_OPACITY_DISABLED)
                } else {
                    1.
                })
                .child(body),
            self.shape,
            theme,
            true,
            true,
        );
        if let Some(handler) = self.on_close {
            let focus = close_focus.as_ref().unwrap();
            let mut close = rounded(
                Button::new(close_id)
                    .relative()
                    .disabled(self.disabled)
                    .track_focus(focus)
                    .tab_index(self.tab_index)
                    .tab_stop(self.close_tab_stop)
                    .accessibility_label(
                        self.close_label
                            .unwrap_or_else(|| format!("移除 {}", self.label).into()),
                    )
                    .when_some(self.key_context, |d, context| {
                        d.key_context(context.as_ref())
                    })
                    .size(height)
                    .min_w(height)
                    .flex_shrink_0()
                    .cursor_default()
                    .bg(wash(colors.foreground, close_wash))
                    .child(
                        Icon::new(outline::X)
                            .size(icon_size)
                            .color_value(colors.foreground),
                    )
                    .children(focus_ring(
                        Some(focus),
                        self.disabled,
                        window,
                        theme,
                        self.shape,
                        false,
                        true,
                    ))
                    .on_click(move |event, w, cx| handler(event, w, cx)),
                self.shape,
                theme,
                false,
                true,
            );
            if !self.disabled {
                close = bind_feedback(close, &state, true, window);
            }
            root = root.child(close);
        }
        // Paint the shared outline after both controls. It consumes no layout,
        // so the body and the close hit region remain exactly Tag-height tall.
        root = root.child(rounded(
            div()
                .absolute()
                .inset_0()
                .border(border)
                .border_color(colors.border),
            self.shape,
            theme,
            true,
            true,
        ));
        self.layout.apply(root).min_w(minimum)
    }
}

struct BodyStyle {
    theme: YueTheme,
    minimum: Rems,
    padding: Rems,
    border: Rems,
    closable: bool,
    shape: TagShape,
    background: Rgba,
}
impl BodyStyle {
    fn apply<T: Styled>(&self, element: T) -> T {
        rounded(
            element
                .relative()
                .flex()
                .items_center()
                .justify_start()
                .h_full()
                .min_w(self.minimum)
                .flex_1()
                .pl(self.padding)
                .pr(if self.closable {
                    gpui_kit::rems(0.)
                } else {
                    self.padding
                })
                .py(rems(self.theme, keys::TAG_PADDING_BLOCK))
                .border(self.border)
                .border_color(gpui_kit::transparent_black())
                .bg(self.background)
                .cursor_default(),
            self.shape,
            self.theme,
            true,
            !self.closable,
        )
    }
}

impl RenderOnce for YueTagGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let mut ids = std::collections::HashSet::new();
        let tags = self
            .tags
            .into_iter()
            .map(|mut tag| {
                assert!(
                    ids.insert(tag.id.clone()),
                    "YueTagGroup requires unique tag IDs"
                );
                if tag.theme.is_none() {
                    tag.theme = Some(theme);
                }
                tag
            })
            .collect::<Vec<_>>();
        self.layout.apply(
            div()
                .id(self.id)
                .test_support()
                .flex()
                .items_start()
                .min_w_0()
                .when(self.wrap, |d| d.flex_wrap())
                .gap(rems(theme, keys::SPACE_8))
                .children(tags),
        )
    }
}

#[cfg(test)]
pub(super) fn source_colors(tone: TagTone, variant: TagVariant, theme: YueTheme) -> TagColors {
    tone.colors(variant, theme)
}
