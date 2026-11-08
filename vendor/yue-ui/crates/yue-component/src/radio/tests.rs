use super::element::*;

use super::behavior::*;

use std::time::Duration;

use super::*;
use crate::tokens::{duration, easing, number, pixels, rgba};
use crate::{Accent, ThemeMode, YueTheme};
use gpui_kit::SharedString;

/// Every Yue profile a colour test has to walk.
const PROFILES: [(ThemeMode, Accent); 4] = [
    (ThemeMode::Light, Accent::Azure),
    (ThemeMode::Dark, Accent::Azure),
    (ThemeMode::Light, Accent::Neutral),
    (ThemeMode::Dark, Accent::Neutral),
];

/// How far two colours sit apart, counted the way the RGB mix travels.
fn gap(a: gpui_kit::Rgba, b: gpui_kit::Rgba) -> f32 {
    (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs()
}

/// A Yue easing token has to start on the current value, land on the target,
/// and travel between them without rewinding or overshooting.
fn ramp_is_sane(theme: YueTheme, name: &'static str) {
    let curve = easing(theme, name);
    assert_eq!(curve(0.0), 0.0, "{name} starts at the current value");
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

fn option(value: &str, disabled: bool) -> YueRadioOption {
    let option = YueRadioOption::new(value, value);
    if disabled {
        option.disabled(true)
    } else {
        option
    }
}

fn value(value: &str) -> SharedString {
    SharedString::from(value)
}

#[test]
fn radio_needs_a_name_from_text_or_label() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert_eq!(
        YueRadio::new("a", theme, false)
            .label("PNG")
            .accessible_name(),
        "PNG"
    );
    assert_eq!(
        YueRadio::new("b", theme, true)
            .accessibility_label("Compact")
            .accessible_name(),
        "Compact",
        "a text-less circle is still named"
    );
}

#[test]
#[should_panic(expected = "YueRadio needs a label")]
fn a_nameless_radio_is_rejected() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let radio = YueRadio::new("c", theme, false);
    let _ = radio.accessible_name();
}

#[test]
fn tab_lands_on_the_selected_option_and_falls_back_to_the_first() {
    let options = [
        option("png", false),
        option("svg", false),
        option("pdf", false),
    ];
    assert_eq!(
        tab_stop_index(&options, Some(&value("svg")), false),
        Some(1),
        "the chosen option is the one Tab reaches"
    );
    assert_eq!(
        tab_stop_index(&options, None, false),
        Some(0),
        "an empty selection still leaves the group enterable"
    );
    assert_eq!(
        tab_stop_index(&options, Some(&value("removed")), false),
        Some(0),
        "a value the group no longer offers falls back to the first option"
    );

    let with_disabled = [
        option("png", true),
        option("svg", false),
        option("pdf", false),
    ];
    assert_eq!(
        tab_stop_index(&with_disabled, Some(&value("png")), false),
        Some(1),
        "a disabled option cannot be the tab stop even while it holds the value"
    );
    assert_eq!(
        tab_stop_index(&[option("png", true)], Some(&value("png")), false),
        None,
        "an all-disabled group puts nothing in the tab order"
    );
    assert_eq!(
        tab_stop_index(&[], None, false),
        None,
        "an empty group has no stop"
    );
    assert_eq!(
        tab_stop_index(&options, Some(&value("png")), true),
        None,
        "a disabled group puts nothing in the tab order"
    );
}

#[test]
fn arrow_keys_move_only_along_the_groups_own_axis() {
    assert_eq!(move_for("down", true), Some(RadioMove::Next));
    assert_eq!(move_for("up", true), Some(RadioMove::Previous));
    assert_eq!(move_for("right", false), Some(RadioMove::Next));
    assert_eq!(move_for("left", false), Some(RadioMove::Previous));
    for key in ["up", "down"] {
        assert_eq!(move_for(key, false), None, "{key} is not a row's move");
    }
    for key in ["left", "right"] {
        assert_eq!(move_for(key, true), None, "{key} is not a column's move");
    }
    for vertical in [true, false] {
        assert_eq!(move_for("home", vertical), Some(RadioMove::First));
        assert_eq!(move_for("end", vertical), Some(RadioMove::Last));
    }
    for key in ["space", "enter", "tab", "a"] {
        assert_eq!(move_for(key, true), None, "{key} is left to the row");
        assert_eq!(move_for(key, false), None, "{key} is left to the row");
    }
}

#[test]
fn moves_walk_the_operable_options_and_wrap() {
    // Three operable options, with the disabled ones already dropped from the
    // count, so every move skips them by construction.
    assert_eq!(
        move_index(3, 0, RadioMove::Next),
        Some(1),
        "forward goes to the next option"
    );
    assert_eq!(
        move_index(3, 2, RadioMove::Next),
        Some(0),
        "past the last option wraps to the first"
    );
    assert_eq!(
        move_index(3, 0, RadioMove::Previous),
        Some(2),
        "before the first option wraps to the last"
    );
    assert_eq!(move_index(3, 1, RadioMove::First), Some(0));
    assert_eq!(move_index(3, 0, RadioMove::Last), Some(2));
    assert_eq!(
        move_index(1, 0, RadioMove::Next),
        Some(0),
        "a single option stays where it is"
    );
    for direction in [
        RadioMove::Next,
        RadioMove::Previous,
        RadioMove::First,
        RadioMove::Last,
    ] {
        assert_eq!(
            move_index(0, 0, direction),
            None,
            "{direction:?} has nowhere to go in an empty group"
        );
    }
}

#[test]
#[should_panic(expected = "option values must be unique")]
fn duplicate_option_values_are_rejected() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let _ = YueRadioGroup::new("format", theme, None).options([
        YueRadioOption::new("png", "PNG"),
        YueRadioOption::new("png", "PNG（重复）"),
    ]);
}

#[test]
fn size_profiles_pair_each_circle_with_its_row_and_label() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    for (size, box_size, height, font) in [
        (RadioSize::Small, 12.0, 28.0, 12.0),
        (RadioSize::Medium, 16.0, 32.0, 14.0),
        (RadioSize::Large, 20.0, 40.0, 16.0),
    ] {
        let (box_token, dot_token, height_token, font_token) = size_tokens(size);
        assert_eq!(pixels(theme, box_token), box_size, "{box_token}");
        assert_eq!(pixels(theme, height_token), height, "{height_token}");
        assert_eq!(pixels(theme, font_token), font, "{font_token}");
        assert!(
            box_size < height,
            "{size:?} keeps its circle inside its row"
        );
        assert!(
            pixels(theme, dot_token)
                < box_size - 2.0 * pixels(theme, crate::theme::keys::RADIO_BORDER_WIDTH),
            "{size:?} keeps the dot inside the circle it sits in"
        );
    }
}

#[test]
fn the_dot_keeps_the_reference_geometry_at_every_size() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let border = pixels(theme, crate::theme::keys::RADIO_BORDER_WIDTH);
    for (size, box_size) in [
        (RadioSize::Small, 12.0),
        (RadioSize::Medium, 16.0),
        (RadioSize::Large, 20.0),
    ] {
        let (_, dot_token, _, _) = size_tokens(size);
        let dot = pixels(theme, dot_token);
        assert_eq!(
            dot,
            box_size * 0.375,
            "{size:?} keeps the reference's 6-in-16 dot ratio"
        );
        assert!(
            dot > 0.0 && dot < box_size - 2.0 * border,
            "{size:?} keeps the dot inside the content box it centres in"
        );
        // The reference places that dot with `inset: var(--space-4)` on a 16px
        // box, which is what the 0.375 ratio works out to once the border is
        // paid for; the same ratio at other rungs scales the inset with it.
        if size == RadioSize::Medium {
            let inset = (box_size - 2.0 * border - dot) / 2.0;
            assert_eq!(inset, 4.0, "Medium is the reference's own 4px inset");
        }
    }
}

#[test]
fn radio_motion_resolves_the_reference_rungs() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert_eq!(
        duration(theme, crate::theme::keys::RADIO_DURATION),
        Duration::from_millis(200),
        "both the circle and the dot change state on the enter rung"
    );
    assert_eq!(
        duration(theme, crate::theme::keys::RADIO_DURATION_FAST),
        Duration::from_millis(100),
        "the press shrinks on the exit rung"
    );
    assert_eq!(
        number(theme, crate::theme::keys::RADIO_PRESS_SCALE),
        0.9,
        "the press scales the circle the way the reference does"
    );
    assert_eq!(
        pixels(theme, crate::theme::keys::RADIO_HALO_WIDTH),
        4.0,
        "the hover halo reaches 4px past the circle"
    );
    for name in ["--radio-ease", "--radio-ease-enter", "--radio-ease-exit"] {
        ramp_is_sane(theme, name);
    }
}

#[test]
fn radio_colours_come_from_yue_roles_in_every_profile() {
    for (mode, accent) in PROFILES {
        let theme = YueTheme::new(mode, accent);
        let checked = rgba(theme, crate::theme::keys::RADIO_BACKGROUND_CHECKED);
        let dot = rgba(theme, crate::theme::keys::RADIO_DOT_COLOR);
        assert!(
            checked.a > 0.99,
            "{mode:?}/{accent:?} fills the circle opaquely"
        );
        assert!(dot.a > 0.99, "{mode:?}/{accent:?} keeps a solid dot");
        // The dot is painted on the accent fill, never on the surface: at
        // Medium the unchecked circle and the dot are the same colour in the
        // light profiles, so a dot on the surface would read as nothing.
        assert!(
            gap(checked, dot) > 0.25,
            "{mode:?}/{accent:?} keeps the dot legible on the fill"
        );
        assert!(
            rgba(theme, crate::theme::keys::RADIO_DISABLED_BORDER_COLOR).a == 0.0,
            "{mode:?}/{accent:?} drops the disabled border out, as the reference does"
        );
        assert!(
            rgba(theme, crate::theme::keys::RADIO_DISABLED_DOT_COLOR).a < dot.a,
            "{mode:?}/{accent:?} dims a disabled dot below the live one"
        );
        let halo = rgba(theme, crate::theme::keys::RADIO_HALO_COLOR);
        assert!(
            halo.a > 0.0 && halo.a < 0.5,
            "{mode:?}/{accent:?} keeps the hover halo a tint, not a fill"
        );
    }
}

#[test]
fn an_option_defaults_to_the_circle() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert_eq!(RadioAppearance::default(), RadioAppearance::Indicator);
    let lone = YueRadio::new("png", theme, false);
    assert_eq!(lone.appearance, RadioAppearance::Indicator);
    assert!(
        lone.segment.is_none(),
        "an option nobody has placed in a row outlines itself"
    );
    assert_eq!(
        YueRadioGroup::new("format", theme, None).appearance,
        RadioAppearance::Indicator
    );
}

#[test]
fn segment_sizes_map_the_control_ladder() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let mut previous = (0.0, 0.0, 0.0);
    for (size, rung) in [
        (RadioSize::Small, (28.0, 12.0, 8.0)),
        (RadioSize::Medium, (32.0, 14.0, 12.0)),
        (RadioSize::Large, (40.0, 16.0, 16.0)),
    ] {
        let (height_token, font_token, padding_token) = segment_tokens(size);
        let actual = (
            pixels(theme, height_token),
            pixels(theme, font_token),
            pixels(theme, padding_token),
        );
        assert_eq!(actual, rung, "{size:?} reads the control ladder");
        assert!(
            actual.1 < actual.0,
            "{size:?} keeps its label inside its row"
        );
        assert!(
            actual.0 > previous.0 && actual.2 >= previous.2,
            "{size:?} grows the row and never shrinks its padding"
        );
        previous = actual;
    }
}

#[test]
fn the_segment_ring_stays_inside_its_own_row() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let border = pixels(theme, crate::theme::keys::RADIO_BUTTON_BORDER_WIDTH);
    let offset = pixels(theme, crate::theme::keys::RADIO_FOCUS_RING_OFFSET);
    let width = pixels(theme, crate::theme::keys::RADIO_FOCUS_RING_WIDTH);
    // The row clips its ends, so the ring is drawn inside the segment. It has
    // to clear the divider the segment shares with its neighbour, and still
    // leave ink on the row rather than past it.
    let inset = ring_inset(offset, border);
    assert!(inset >= 0.0, "the ring never reaches over the divider");
    for size in [RadioSize::Small, RadioSize::Medium, RadioSize::Large] {
        let (height_token, _, _) = segment_tokens(size);
        assert!(
            inset + 2.0 * width <= pixels(theme, height_token),
            "{size:?} keeps the whole ring inside the row it rings"
        );
    }
}

#[test]
fn a_hover_overlay_tints_its_fill_instead_of_punching_through_it() {
    for (mode, accent) in PROFILES {
        let theme = YueTheme::new(mode, accent);
        let surface = rgba(theme, crate::theme::keys::RADIO_BUTTON_BACKGROUND);
        let overlay = rgba(theme, crate::theme::keys::RADIO_BUTTON_HOVER_BACKGROUND);
        assert!(
            surface.a > 0.99,
            "{mode:?}/{accent:?} paints the row on an opaque surface"
        );
        assert!(
            overlay.a > 0.0 && overlay.a < 0.5,
            "{mode:?}/{accent:?} keeps hover a tint rather than a fill"
        );
        assert_eq!(
            tint(surface, overlay, 0.0),
            surface,
            "{mode:?}/{accent:?} leaves an unhovered row alone"
        );
        let hovered = tint(surface, overlay, 1.0);
        assert_eq!(
            hovered.a, surface.a,
            "{mode:?}/{accent:?} never turns a hovered segment see-through"
        );
        assert_ne!(
            hovered, surface,
            "{mode:?}/{accent:?} shows the hover at all"
        );
        assert!(
            gap(surface, hovered) > gap(hovered, tint(surface, overlay, 0.5)),
            "{mode:?}/{accent:?} travels the same axis at every rung"
        );
    }
}

#[test]
fn a_selected_segment_borrows_the_selection_roles() {
    for (mode, accent) in PROFILES {
        let theme = YueTheme::new(mode, accent);
        // The chosen row is filled with the system's own selection tint, never
        // with a pressed button's: a radio's checked state is a fact about the
        // value, not about the pointer.
        assert_eq!(
            rgba(theme, crate::theme::keys::RADIO_BUTTON_BACKGROUND_SELECTED),
            rgba(theme, crate::theme::keys::SELECTED_BACKGROUND),
            "{mode:?}/{accent:?} fills with the selected role"
        );
        assert_eq!(
            rgba(theme, crate::theme::keys::RADIO_BUTTON_COLOR_SELECTED),
            rgba(theme, crate::theme::keys::SELECTED_COLOR),
            "{mode:?}/{accent:?} writes the chosen label in the selected role"
        );
        let surface = rgba(theme, crate::theme::keys::RADIO_BUTTON_BACKGROUND);
        let chosen = rgba(theme, crate::theme::keys::RADIO_BUTTON_BACKGROUND_SELECTED);
        assert!(
            gap(chosen, surface) > 0.02,
            "{mode:?}/{accent:?} shows the choice without a border change"
        );
        assert!(
            gap(
                rgba(theme, crate::theme::keys::RADIO_BUTTON_COLOR_SELECTED),
                chosen
            ) > 0.5,
            "{mode:?}/{accent:?} keeps the chosen label legible on the chosen fill"
        );
        assert_eq!(
            rgba(theme, crate::theme::keys::RADIO_BUTTON_COLOR_HOVER),
            rgba(theme, crate::theme::keys::RADIO_LABEL_COLOR),
            "{mode:?}/{accent:?} lifts a hovered segment to the text a hovered circle uses"
        );
        let quiet = rgba(theme, crate::theme::keys::RADIO_BUTTON_COLOR);
        let lift = rgba(theme, crate::theme::keys::RADIO_BUTTON_COLOR_HOVER);
        assert!(
            gap(quiet, surface) < gap(lift, surface),
            "{mode:?}/{accent:?} keeps an unchosen label quieter than a hovered one"
        );
        assert!(
            number(theme, crate::theme::keys::RADIO_BUTTON_WEIGHT_SELECTED) > 400.0,
            "{mode:?}/{accent:?} weights the chosen label above normal text"
        );
    }
}

#[test]
fn segment_motion_splits_the_choice_from_the_press() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert_eq!(
        duration(theme, crate::theme::keys::RADIO_BUTTON_DURATION),
        Duration::from_millis(200),
        "the fill and the label colour arrive on the enter rung"
    );
    assert_eq!(
        duration(theme, crate::theme::keys::RADIO_BUTTON_DURATION_FAST),
        Duration::from_millis(100),
        "the press overlay leaves on the exit rung"
    );
    ramp_is_sane(theme, "--radio-button-ease");
    // Both appearances ride the same two semantic rungs, so a segmented row
    // and a circle never feel like they are on different clocks.
    assert_eq!(
        duration(theme, crate::theme::keys::RADIO_BUTTON_DURATION),
        duration(theme, crate::theme::keys::RADIO_DURATION)
    );
    assert_eq!(
        duration(theme, crate::theme::keys::RADIO_BUTTON_DURATION_FAST),
        duration(theme, crate::theme::keys::RADIO_DURATION_FAST)
    );
}
