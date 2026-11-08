use super::element::*;
use crate::tokens::{duration, easing, mix_rgba, pixels, rgba};

use super::paint::*;

use std::time::Duration;

use super::*;
use crate::tokens::token;
use crate::{Accent, ThemeMode, YueTheme};

#[test]
fn switch_builder_keeps_size_shape_and_indicator_selection() {
    let switch = YueSwitch::new(
        "power",
        YueTheme::new(ThemeMode::Light, Accent::Azure),
        false,
        "Power",
    )
    .size(SwitchSize::Large)
    .shape(SwitchShape::RoundedRectangle)
    .indicator(SwitchIndicator::CheckX)
    .loading(true)
    .disabled(true);
    assert_eq!(switch.size, SwitchSize::Large);
    assert_eq!(switch.shape, SwitchShape::RoundedRectangle);
    assert_eq!(switch.indicator, SwitchIndicator::CheckX);
    assert!(switch.loading);
    assert!(switch.disabled);
}

#[test]
fn switch_profiles_resolve_geometry_and_icon_tokens() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    for (size, width, height, thumb, inset_token) in [
        (SwitchSize::Small, 32.0, 20.0, 16.0, "--switch-inset-sm"),
        (SwitchSize::Medium, 40.0, 24.0, 20.0, "--switch-inset-md"),
        (SwitchSize::Large, 48.0, 28.0, 24.0, "--switch-inset-lg"),
    ] {
        let (width_token, height_token, thumb_token) = size_tokens(size);
        let inset = pixels(theme, inset_token);
        assert_eq!(pixels(theme, width_token), width);
        assert_eq!(pixels(theme, height_token), height);
        assert_eq!(pixels(theme, thumb_token), thumb);
        assert_eq!(inset, 2.0, "{inset_token}");
        assert_eq!(
            height,
            thumb + 2.0 * inset,
            "{size:?} centers the thumb: two insets stack on top of it"
        );
    }
    assert_eq!(
        pixels(theme, crate::theme::keys::SWITCH_INDICATOR_SIZE_MD),
        12.0
    );
    for (indicator_token, stroke_token) in [
        (
            "--switch-indicator-size-sm",
            "--checkbox-mark-stroke-width-sm",
        ),
        (
            "--switch-indicator-size-md",
            "--checkbox-mark-stroke-width-md",
        ),
        (
            "--switch-indicator-size-lg",
            "--checkbox-mark-stroke-width-lg",
        ),
    ] {
        assert_eq!(
            pixels(theme, stroke_token),
            GLYPH_STROKE * pixels(theme, indicator_token) / GLYPH_BOX,
            "{stroke_token} keeps the checkbox marks at the weight these glyphs paint with"
        );
    }
    assert_eq!(
        token(theme, crate::theme::keys::SWITCH_BACKGROUND).color(),
        Some(0xa3a3a3),
        "the unchecked track stays visibly distinct from its surface"
    );
    assert_eq!(
        token(theme, crate::theme::keys::SWITCH_THUMB_COLOR).color(),
        Some(0xffffff),
        "the unchecked thumb is a light surface against the gray track"
    );
    assert_eq!(
        token(theme, crate::theme::keys::SWITCH_THUMB_COLOR_CHECKED).color(),
        Some(0xffffff),
        "the checked thumb remains white against the accent track"
    );
    assert_eq!(
        token(theme, crate::theme::keys::SWITCH_INDICATOR_COLOR_CHECKED).color(),
        Some(0x125fb8),
        "the checked indicator uses the accent color on the white thumb"
    );
    assert_eq!(
        duration(theme, crate::theme::keys::SWITCH_DURATION),
        std::time::Duration::from_millis(200)
    );
}

#[test]
fn switch_thumb_shadow_lifts_the_thumb_without_borrowing_its_size() {
    for (mode, alpha) in [(ThemeMode::Light, 0.2), (ThemeMode::Dark, 0.5)] {
        let shadows = thumb_shadow(mode);
        let [shadow] = shadows.as_slice() else {
            panic!("the thumb casts exactly one shadow");
        };
        assert_eq!(shadow.offset.x, gpui_kit::px(0.0));
        assert_eq!(shadow.offset.y, gpui_kit::px(1.0), "{mode:?} drops 1px");
        assert_eq!(shadow.blur_radius, gpui_kit::px(2.0));
        assert_eq!(
            shadow.spread_radius,
            gpui_kit::px(0.0),
            "no spread, so the shadow never implies a bigger thumb"
        );
        assert!(!shadow.inset);
        assert!(
            (shadow.color.a - alpha).abs() < 1e-6,
            "{mode:?} keeps the thumb readable without darkening the track"
        );
    }
}

#[test]
fn switch_check_glyph_is_the_demo_path_grown_and_centred() {
    let extent = |axis: usize, points: &[(f32, f32)]| {
        let coord = |point: &(f32, f32)| if axis == 0 { point.0 } else { point.1 };
        let min = points.iter().map(coord).fold(f32::MAX, f32::min);
        let max = points.iter().map(coord).fold(f32::MIN, f32::max);
        (min, max)
    };
    let (left, right) = extent(0, &CHECK_LINE);
    let (top, bottom) = extent(1, &CHECK_LINE);
    assert_eq!(
        left + right,
        top + bottom,
        "the enlarged check stays centred on the glyph box"
    );
    let (cross_top, cross_bottom) = extent(1, &CROSS_SECOND);
    assert_eq!(
        bottom - top,
        cross_bottom - cross_top - 1.0,
        "the check is 11 units tall: 1.1x the demo's 10, one short of the cross"
    );
    for pair in CHECK_LINE.windows(2) {
        let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
        assert_eq!(dx.abs(), dy.abs(), "both legs of the check stay at 45°");
    }
    assert!(
        left - GLYPH_STROKE / 2.0 >= 0.0 && right + GLYPH_STROKE / 2.0 <= GLYPH_BOX,
        "the check's ink, round caps included, stays inside the shared glyph box"
    );
}

#[test]
fn switch_press_geometry_keeps_a_stretched_thumb_inside_the_track() {
    let (inset, shift, thumb) = (2.0, 16.0, 20.0);
    let stretch = press_stretch(true, false, inset);
    assert_eq!(press_stretch(false, false, inset), 0.0);
    assert_eq!(
        press_stretch(true, true, inset),
        0.0,
        "a switch that cannot be toggled cannot be stretched either"
    );
    assert_eq!(stretch, 4.0, "the demo swells its 20px thumb to 24px");
    assert_eq!(thumb_offset(false, shift, 0.0), 0.0);
    assert_eq!(
        thumb_offset(false, shift, stretch),
        0.0,
        "an un-checked thumb grows towards the checked side, not off its corner"
    );
    assert_eq!(thumb_offset(true, shift, 0.0), shift);
    assert_eq!(
        thumb_offset(true, shift, stretch) + thumb + stretch,
        shift + thumb,
        "the stretched thumb hands its extra width back to the offset, so the leading edge stays put"
    );
}

#[test]
fn switch_indicator_glyphs_mirror_across_the_cross_fade() {
    for (progress, cross, check) in [
        (0.0, (0.0, 1.0), (-0.125, 0.5)),
        (0.5, (0.0625, 0.75), (-0.0625, 0.75)),
        (1.0, (0.125, 0.5), (0.0, 1.0)),
    ] {
        assert_eq!(cross_motion(progress), cross);
        assert_eq!(check_motion(progress), check);
    }
}

#[test]
fn switch_motion_legs_resolve_token_durations_and_curves() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert_eq!(
        duration(theme, crate::theme::keys::SWITCH_DURATION_FAST),
        Duration::from_millis(100),
        "the erase leg and the ink delay run on the switch's own fast rung"
    );
    // The ink leg is no longer on a literal clock of its own: it reads the same
    // two rungs the checkbox mark is drawn with.
    assert_eq!(
        duration(theme, crate::theme::keys::SWITCH_DURATION),
        duration(theme, crate::theme::keys::CHECKBOX_DURATION),
        "the glyph is written for as long as the checkbox mark"
    );
    assert_eq!(
        duration(theme, crate::theme::keys::SWITCH_DURATION_FAST),
        duration(theme, crate::theme::keys::CHECKBOX_DURATION_FAST),
        "and starts as late as the checkbox mark"
    );
    assert_eq!(
        duration(theme, crate::theme::keys::SPINNER_DURATION),
        Duration::from_millis(900),
        "the ring resolves a calc() duration"
    );
    for name in ["--switch-ease", "--motion-ease-enter", "--motion-ease-exit"] {
        let curve = easing(theme, name);
        assert_eq!(curve(0.0), 0.0, "{name} starts where the value starts");
        assert_eq!(curve(1.0), 1.0, "{name} ends on target");
        let mut previous = 0.0;
        for step in 1..=20u32 {
            let value = curve(step as f32 / 20.0);
            assert!(
                (previous - 1e-6..=1.0 + 1e-6).contains(&value),
                "{name} never rewinds or overshoots at {step}/20"
            );
            previous = value;
        }
    }
}

#[test]
fn switch_track_colour_mixes_along_the_rgb_axes() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let off = rgba(theme, crate::theme::keys::SWITCH_BACKGROUND);
    let on = rgba(theme, crate::theme::keys::SWITCH_BACKGROUND_CHECKED);
    let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
    for (progress, want, note) in [
        (0.0, off, "an unmoved track keeps its unchecked token"),
        (
            1.0,
            on,
            "a committed track lands on its checked token, not near it",
        ),
    ] {
        let mixed = mix_rgba(off, on, progress);
        assert!(
            close(mixed.r, want.r) && close(mixed.g, want.g) && close(mixed.b, want.b),
            "{note}: got {mixed:?} want {want:?}"
        );
    }
    let half = mix_rgba(off, on, 0.5);
    for (from, to, mid, channel) in [
        (off.r, on.r, half.r, "red"),
        (off.g, on.g, half.g, "green"),
        (off.b, on.b, half.b, "blue"),
    ] {
        assert!(
            (from.min(to)..from.max(to)).contains(&mid),
            "{channel} travels the straight line, got {mid} for {from} and {to}"
        );
        assert!(
            close(mid, (from + to) / 2.0),
            "{channel} reaches its average at half progress"
        );
    }
    assert!(close(half.a, off.a), "both track colours are opaque");
}
