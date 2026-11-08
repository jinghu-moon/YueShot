//! Element composition and component-local presentation.

use super::*;

use super::paint::*;

use gpui_kit::StatefulInteractiveElement as _;

use gpui_kit::prelude::FluentBuilder as _;

use gpui_kit::{
    Animation, AnimationExt as _, App, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, RenderOnce, Styled as _, Window, div, px,
};

use crate::tokens::{duration, easing, mix_rgba, number, pixels, rems, rgba};

use crate::{ThemeMode, YueTheme};

/// How much wider the thumb goes while the pointer is held down: two track
/// insets on the trailing side, and nothing on a switch that cannot be pressed.
/// The demo swells its 20px thumb to 24px against a 2px inset, so the amount
/// scales with the inset rather than with a fixed pixel count.
pub(super) fn press_stretch(pressed: bool, disabled: bool, inset: f32) -> f32 {
    if pressed && !disabled {
        2.0 * inset
    } else {
        0.0
    }
}

/// The thumb's travel from its resting corner. A checked thumb hands its stretch
/// back to the offset, so its right edge stays flush with the track instead of
/// riding over the edge of it.
pub(super) fn thumb_offset(checked: bool, shift: f32, stretch: f32) -> f32 {
    if checked { shift - stretch } else { 0.0 }
}

/// The cross icon's (rotation, scale) at `progress`: it swings out towards the
/// checked side while it collapses.
pub(super) fn cross_motion(progress: f32) -> (f32, f32) {
    (
        INDICATOR_SWING_TURNS * progress,
        1.0 - (1.0 - INDICATOR_MIN_SCALE) * progress,
    )
}

/// The check icon's (rotation, scale) at `progress`: the mirror image, entering
/// from the other side of where it settles.
pub(super) fn check_motion(progress: f32) -> (f32, f32) {
    (
        -INDICATOR_SWING_TURNS * (1.0 - progress),
        INDICATOR_MIN_SCALE + (1.0 - INDICATOR_MIN_SCALE) * progress,
    )
}

/// The thumb's drop shadow: the demo's `0 1px 2px` over black at 20%, lifted to
/// 50% on the dark profile so the thumb still separates from a dark track. Yue
/// keeps its shadows as multi-layer CSS strings with no single-layer equivalent
/// to read, so this one stays here.
pub(super) fn thumb_shadow(mode: ThemeMode) -> Vec<gpui_kit::BoxShadow> {
    let alpha = match mode {
        ThemeMode::Light => 0.2,
        ThemeMode::Dark => 0.5,
    };
    vec![
        gpui_kit::BoxShadow::new(px(0.0), px(1.0), gpui_kit::black().opacity(alpha))
            .blur_radius(px(2.0)),
    ]
}

/// The ink leg of a stroke. The reference only fades and swings these glyphs, so
/// the drawing is Yue's own addition, and it borrows the checkbox mark's clock:
/// the enter rung, started one fast rung after the click.
fn ink_policy(theme: YueTheme) -> gpui_kit::base::Transition {
    gpui_kit::base::Transition::new(duration(theme, crate::theme::keys::SWITCH_DURATION))
        .delay(duration(theme, crate::theme::keys::SWITCH_DURATION_FAST))
        .ease(easing(theme, crate::theme::keys::MOTION_EASE_ENTER))
}

/// The erase leg: the ink goes before the container has finished fading out.
fn erase_policy(theme: YueTheme) -> gpui_kit::base::Transition {
    gpui_kit::base::Transition::new(duration(theme, crate::theme::keys::SWITCH_DURATION_FAST))
        .ease(easing(theme, crate::theme::keys::MOTION_EASE_EXIT))
}

pub(super) fn size_tokens(
    size: SwitchSize,
) -> (
    crate::theme::TokenKey,
    crate::theme::TokenKey,
    crate::theme::TokenKey,
) {
    match size {
        SwitchSize::Small => (
            crate::theme::keys::SWITCH_WIDTH_SM,
            crate::theme::keys::SWITCH_HEIGHT_SM,
            crate::theme::keys::SWITCH_THUMB_SM,
        ),
        SwitchSize::Medium => (
            crate::theme::keys::SWITCH_WIDTH_MD,
            crate::theme::keys::SWITCH_HEIGHT_MD,
            crate::theme::keys::SWITCH_THUMB_MD,
        ),
        SwitchSize::Large => (
            crate::theme::keys::SWITCH_WIDTH_LG,
            crate::theme::keys::SWITCH_HEIGHT_LG,
            crate::theme::keys::SWITCH_THUMB_LG,
        ),
    }
}

impl RenderOnce for YueSwitch {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let focus_handle = self.focus_handle.clone().unwrap_or_else(|| {
            window
                .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone()
        });
        let unavailable = self.disabled || self.loading;
        let ring_visible =
            !unavailable && focus_handle.is_focused(window) && window.last_input_was_keyboard();
        let (width_token, height_token, thumb_token) = size_tokens(self.size);
        let inset_token = match self.size {
            SwitchSize::Small => crate::theme::keys::SWITCH_INSET_SM,
            SwitchSize::Medium => crate::theme::keys::SWITCH_INSET_MD,
            SwitchSize::Large => crate::theme::keys::SWITCH_INSET_LG,
        };
        let (track_radius, thumb_radius) = match self.shape {
            SwitchShape::Capsule => (
                crate::theme::keys::SWITCH_BORDER_RADIUS_CAPSULE,
                crate::theme::keys::SWITCH_THUMB_BORDER_RADIUS_CAPSULE,
            ),
            SwitchShape::RoundedRectangle => (
                crate::theme::keys::SWITCH_BORDER_RADIUS_ROUNDED,
                crate::theme::keys::SWITCH_THUMB_BORDER_RADIUS_ROUNDED,
            ),
        };
        let indicator_token = match self.size {
            SwitchSize::Small => crate::theme::keys::SWITCH_INDICATOR_SIZE_SM,
            SwitchSize::Medium => crate::theme::keys::SWITCH_INDICATOR_SIZE_MD,
            SwitchSize::Large => crate::theme::keys::SWITCH_INDICATOR_SIZE_LG,
        };
        let width = pixels(self.theme, width_token);
        let thumb_size = pixels(self.theme, thumb_token);
        let inset = pixels(self.theme, inset_token);
        let shift = width - thumb_size - 2.0 * inset;
        let switch_duration = duration(self.theme, crate::theme::keys::SWITCH_DURATION);
        let spinner_duration = duration(self.theme, crate::theme::keys::SPINNER_DURATION);
        // Every animated property of a switch rides the same duration and curve,
        // so each channel below asks for the same policy.
        let policy = |theme: YueTheme| {
            gpui_kit::base::Transition::new(switch_duration)
                .ease(easing(theme, crate::theme::keys::SWITCH_EASE))
        };
        let press_state = (!unavailable).then(|| {
            window.use_keyed_state((self.id.clone(), "yue-switch-press"), cx, |_, _| false)
        });
        let pressed = press_state.as_ref().is_some_and(|state| *state.read(cx));
        let stretch = press_stretch(pressed, unavailable, inset);
        let offset = gpui_kit::base::transition(
            (self.id.clone(), "yue-switch-offset"),
            thumb_offset(self.checked, shift, stretch),
            policy(self.theme),
            window,
            cx,
        );
        let thumb_width = gpui_kit::base::transition(
            (self.id.clone(), "yue-switch-width"),
            thumb_size + stretch,
            policy(self.theme),
            window,
            cx,
        );
        let cross_progress = gpui_kit::base::transition(
            (self.id.clone(), "yue-switch-indicator"),
            f32::from(self.checked),
            policy(self.theme),
            window,
            cx,
        )
        .clamp(0.0, 1.0);
        let loading_progress = gpui_kit::base::transition(
            (self.id.clone(), "yue-switch-loading"),
            f32::from(self.loading),
            policy(self.theme),
            window,
            cx,
        )
        .clamp(0.0, 1.0);
        let track_color = gpui_kit::base::transition(
            (self.id.clone(), "yue-switch-track"),
            f32::from(!self.disabled && self.checked),
            policy(self.theme),
            window,
            cx,
        );
        let track_color = mix_rgba(
            rgba(self.theme, crate::theme::keys::SWITCH_BACKGROUND),
            rgba(self.theme, crate::theme::keys::SWITCH_BACKGROUND_CHECKED),
            track_color.clamp(0.0, 1.0),
        );
        // The thumb colour only changes with the disabled state, which the demo
        // also leaves unanimated.
        let thumb_color = rgba(
            self.theme,
            if self.disabled {
                crate::theme::keys::SWITCH_THUMB_COLOR
            } else if self.checked {
                crate::theme::keys::SWITCH_THUMB_COLOR_CHECKED
            } else {
                crate::theme::keys::SWITCH_THUMB_COLOR
            },
        );
        // The ink runs on its own clock: it starts a fast rung after the click and
        // is wiped off before the fade is done, so it cannot ride the cross-fade.
        // The cross's two diagonals are drawn from the same moment, so they share
        // one channel rather than carrying a stagger neither glyph needs.
        let cross_inked = gpui_kit::base::transition(
            (self.id.clone(), "yue-switch-cross"),
            f32::from(!self.checked),
            if self.checked {
                erase_policy(self.theme)
            } else {
                ink_policy(self.theme)
            },
            window,
            cx,
        );
        let check_inked = gpui_kit::base::transition(
            (self.id.clone(), "yue-switch-check"),
            f32::from(self.checked),
            if self.checked {
                ink_policy(self.theme)
            } else {
                erase_policy(self.theme)
            },
            window,
            cx,
        );
        let drawn = |ink: f32| ink.clamp(0.0, 1.0);
        let indicator_size = pixels(self.theme, indicator_token);
        let icons_alpha = 1.0 - loading_progress;
        let (cross_turns, cross_scale) = cross_motion(cross_progress);
        let (check_turns, check_scale) = check_motion(cross_progress);
        let cross_color = gpui_kit::Hsla::from(rgba(
            self.theme,
            crate::theme::keys::SWITCH_INDICATOR_COLOR_UNCHECKED,
        ));
        let check_color = gpui_kit::Hsla::from(rgba(
            self.theme,
            crate::theme::keys::SWITCH_INDICATOR_COLOR_CHECKED,
        ));
        let spinner_color =
            gpui_kit::Hsla::from(rgba(self.theme, crate::theme::keys::SPINNER_COLOR));
        let thumb = gpui_kit::base::SwitchThumb::new(self.checked)
            .disabled(self.disabled)
            .absolute()
            .top(rems(self.theme, inset_token))
            .left(gpui_kit::rems((inset + offset) / 16.0))
            .w(gpui_kit::rems(thumb_width / 16.0))
            .h(rems(self.theme, thumb_token))
            .rounded(rems(self.theme, thumb_radius))
            .bg(thumb_color)
            .when(!self.disabled, |thumb| {
                thumb.shadow(thumb_shadow(self.theme.mode()))
            })
            .when(
                matches!(self.indicator, SwitchIndicator::X | SwitchIndicator::CheckX),
                |thumb| {
                    thumb.child(glyph_layer(
                        indicator_size,
                        (1.0 - cross_progress) * icons_alpha,
                        cross_turns,
                        cross_scale,
                        cross_color,
                        [
                            GlyphStroke::new(&CROSS_FIRST, drawn(cross_inked)),
                            GlyphStroke::new(&CROSS_SECOND, drawn(cross_inked)),
                        ],
                    ))
                },
            )
            .when(
                matches!(
                    self.indicator,
                    SwitchIndicator::Check | SwitchIndicator::CheckX
                ),
                |thumb| {
                    thumb.child(glyph_layer(
                        indicator_size,
                        cross_progress * icons_alpha,
                        check_turns,
                        check_scale,
                        check_color,
                        [GlyphStroke::new(&CHECK_LINE, drawn(check_inked)), NO_STROKE],
                    ))
                },
            )
            .when(loading_progress > 0.0, |thumb| {
                thumb.child(div().absolute().inset_0().with_animation(
                    "yue-switch-spinner",
                    Animation::new(spinner_duration).repeat(),
                    move |layer, delta| {
                        layer.child(ring_layer(
                            indicator_size,
                            loading_progress,
                            delta,
                            spinner_color,
                        ))
                    },
                ))
            });
        let track = gpui_kit::base::SwitchTrack::new((self.id.clone(), "track"))
            .checked(self.checked)
            .disabled(self.disabled)
            .relative()
            .w(rems(self.theme, width_token))
            .h(rems(self.theme, height_token))
            .rounded(rems(self.theme, track_radius))
            .bg(track_color)
            // The track carries no border, like the demo's `border: 0`: a border
            // would shrink the box the thumb is placed inside, so `top: inset`
            // would measure from below it and the thumb would sit low.
            .when(self.disabled, |track| {
                track.opacity(number(
                    self.theme,
                    crate::theme::keys::SWITCH_OPACITY_DISABLED,
                ))
            })
            .child(thumb)
            // The ring sits outside the track by the focus-ring offset, as the
            // demo's `outline-offset` does, so it reads against the page rather
            // than against a track that may share its colour. It hangs off the
            // track rather than the root so a widened root does not stretch it.
            .when(ring_visible, |track| {
                let width = pixels(self.theme, crate::theme::keys::FOCUS_RING_WIDTH);
                let offset = pixels(self.theme, crate::theme::keys::FOCUS_RING_OFFSET);
                let outset = gpui_kit::rems(-(width + offset) / 16.0);
                track.child(
                    div()
                        .absolute()
                        .top(outset)
                        .right(outset)
                        .bottom(outset)
                        .left(outset)
                        .rounded(gpui_kit::rems(
                            (pixels(self.theme, track_radius) + width + offset) / 16.0,
                        ))
                        .border(gpui_kit::rems(width / 16.0))
                        .border_color(rgba(self.theme, crate::theme::keys::FOCUS_RING_COLOR)),
                )
            });

        let mut switch = gpui_kit::base::Switch::new(self.id)
            .checked(self.checked)
            .disabled(unavailable)
            .track_focus(&focus_handle)
            .accessibility_label(self.accessibility_label)
            .relative()
            .w(rems(self.theme, width_token))
            .h(rems(self.theme, height_token))
            .child(track);
        switch = self
            .layout
            .apply(switch)
            .tab_index(self.tab_index)
            .tab_stop(self.tab_stop);
        if let Some(context) = self.key_context {
            switch = switch.key_context(context.as_ref());
        }
        if let Some(description) = self.description {
            switch = switch.aria_description(description);
        }
        if let Some(state) = press_state {
            let down = window.listener_for(&state, |state, _, window, cx| {
                if !*state {
                    *state = true;
                    cx.notify();
                    window.refresh();
                }
            });
            let up = window.listener_for(&state, |state, _, window, cx| {
                if *state {
                    *state = false;
                    cx.notify();
                    window.refresh();
                }
            });
            let up_out = window.listener_for(&state, |state, _, window, cx| {
                if *state {
                    *state = false;
                    cx.notify();
                    window.refresh();
                }
            });
            switch = switch
                .on_mouse_down(MouseButton::Left, down)
                .on_mouse_up(MouseButton::Left, up)
                .on_mouse_up_out(MouseButton::Left, up_out);
        }
        let loading = self.loading;
        if let Some(handler) = self.on_change.filter(|_| !loading) {
            switch = switch.on_change(move |checked, event, window, cx| {
                handler(checked, event, window, cx);
            });
        }
        switch
    }
}
