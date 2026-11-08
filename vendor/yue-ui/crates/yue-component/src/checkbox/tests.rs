use super::element::*;

use super::behavior::*;

use super::paint::*;

use std::time::Duration;

use super::*;
use crate::tokens::{duration, easing, number, pixels, rgba};
use crate::{Accent, ThemeMode, YueTheme};
use gpui_kit::SharedString;

fn option(value: &str, disabled: bool) -> YueCheckboxOption {
    let option = YueCheckboxOption::new(value, value);
    if disabled {
        option.disabled(true)
    } else {
        option
    }
}

fn selected(values: &[&str]) -> Vec<SharedString> {
    values
        .iter()
        .map(|value| SharedString::from(*value))
        .collect()
}

#[test]
fn checkbox_state_names_the_accent_fill_for_both_chosen_states() {
    assert!(!CheckboxState::Unchecked.chosen());
    assert!(CheckboxState::Checked.chosen());
    assert!(
        CheckboxState::Indeterminate.chosen(),
        "the reference fills a mixed box the same way it fills a checked one"
    );
}

#[test]
fn checkbox_needs_a_name_from_text_or_label() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert_eq!(
        YueCheckbox::new("a", theme, CheckboxState::Unchecked)
            .label("读取")
            .accessible_name(),
        "读取"
    );
    assert_eq!(
        YueCheckbox::new("b", theme, CheckboxState::Unchecked)
            .accessibility_label("Accept terms")
            .accessible_name(),
        "Accept terms",
        "a text-less box is still named"
    );
}

#[test]
#[should_panic(expected = "YueCheckbox needs a label")]
fn a_nameless_checkbox_is_rejected() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let checkbox = YueCheckbox::new("c", theme, CheckboxState::Unchecked);
    let _ = checkbox.accessible_name();
}

#[test]
fn select_all_state_follows_the_derivation_table() {
    let options = [option("read", false), option("write", false)];
    assert_eq!(
        select_all_state(&options, &[]),
        CheckboxState::Unchecked,
        "nothing selected"
    );
    assert_eq!(
        select_all_state(&options, &["write".into()]),
        CheckboxState::Indeterminate,
        "part of the eligible set selected"
    );
    assert_eq!(
        select_all_state(&options, &["read".into(), "write".into()]),
        CheckboxState::Checked,
        "the whole eligible set selected"
    );
    assert_eq!(
        select_all_state(&[], &[]),
        CheckboxState::Unchecked,
        "an empty group has nothing to complete"
    );

    let with_disabled = [
        option("read", false),
        option("write", false),
        option("admin", true),
    ];
    assert_eq!(
        select_all_state(&with_disabled, &["read".into(), "write".into()]),
        CheckboxState::Checked,
        "an unselected disabled option does not hold the header at mixed"
    );
    assert_eq!(
        select_all_state(&with_disabled, &["admin".into()]),
        CheckboxState::Unchecked,
        "a disabled option's own value does not count as progress"
    );
    assert_eq!(
        select_all_state(&[option("admin", true)], &["admin".into()]),
        CheckboxState::Unchecked,
        "an all-disabled group reports an empty eligible set"
    );
}

#[test]
fn header_click_moves_only_eligible_values() {
    let options = [
        option("read", false),
        option("write", false),
        option("admin", true),
    ];
    let clearing = apply_select_all(&options, &["admin".into()], true);
    assert_eq!(
        clearing,
        selected(&["admin", "read", "write"]),
        "checking adds S ∪ E and leaves the disabled option's value in place"
    );
    let unchecking = apply_select_all(&options, &["admin".into(), "read".into()], false);
    assert_eq!(
        unchecking,
        selected(&["admin"]),
        "clearing subtracts only S ∩ E, so a disabled option keeps its value"
    );

    let already = apply_select_all(&options, &["read".into(), "write".into()], true);
    assert_eq!(
        already,
        selected(&["read", "write"]),
        "a header click on a complete group cannot duplicate a value"
    );
    let foreign = apply_select_all(&options, &["other".into()], true);
    assert_eq!(
        foreign,
        selected(&["other", "read", "write"]),
        "a value this group does not offer passes through untouched"
    );
}

#[test]
fn an_inert_header_offers_nothing() {
    let empty: Vec<YueCheckboxOption> = Vec::new();
    assert!(
        eligible_values(&empty).is_empty(),
        "a group with no options has no header to operate"
    );
    let all_disabled = [option("admin", true)];
    assert!(
        eligible_values(&all_disabled).is_empty(),
        "a group whose every option is disabled has nothing to change"
    );
}

#[test]
fn option_toggle_is_idempotent_and_keeps_order() {
    let current = selected(&["read", "write"]);
    let added = apply_option(&current, &"admin".into(), CheckboxState::Checked);
    assert_eq!(added, selected(&["read", "write", "admin"]));
    let repeated = apply_option(&added, &"admin".into(), CheckboxState::Checked);
    assert_eq!(repeated, added, "selecting a value twice stays one value");
    let removed = apply_option(&repeated, &"read".into(), CheckboxState::Unchecked);
    assert_eq!(removed, selected(&["write", "admin"]));
    assert_eq!(
        apply_option(&removed, &"missing".into(), CheckboxState::Unchecked),
        removed,
        "clearing a value that was never selected changes nothing"
    );
}

#[test]
#[should_panic(expected = "option values must be unique")]
fn duplicate_option_values_are_rejected() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let _ = YueCheckboxGroup::new("permissions", theme, Vec::<String>::new()).options([
        YueCheckboxOption::new("read", "读取"),
        YueCheckboxOption::new("read", "读取（重复）"),
    ]);
}

#[test]
fn checkbox_marks_are_the_reference_paths_on_a_16_unit_box() {
    assert_eq!(MARK_BOX, 16.0);
    // `d="M4 8.5l3 3 5-6"`: the legs the check is written with, in order.
    assert_eq!(CHECK_LINE[0], (4.0, 8.5));
    let legs = CHECK_LINE
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1));
    let want = [(3.0, 3.0), (5.0, -6.0)];
    for (leg, want) in legs.zip(want) {
        assert_eq!(leg, want, "the check keeps the reference's leg lengths");
    }
    // `d="M4.5 8h7"`: a 7-unit dash, centred on the box.
    assert_eq!(DASH_LINE[0], (4.5, 8.0));
    assert_eq!(DASH_LINE[1], (11.5, 8.0));
    assert_eq!(
        DASH_LINE[0].0 + DASH_LINE[1].0,
        MARK_BOX,
        "the dash is centred horizontally"
    );
    assert_eq!(DASH_LINE[0].1, MARK_BOX / 2.0, "and on the middle line");
    for point in CHECK_LINE.iter().chain(DASH_LINE.iter()) {
        assert!(
            point.0 > 0.0 && point.0 < MARK_BOX && point.1 > 0.0 && point.1 < MARK_BOX,
            "{point:?} stays inside the box it is drawn on"
        );
    }
}

#[test]
fn size_profiles_pair_each_box_with_its_row_and_label() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    let border = pixels(theme, crate::theme::keys::CHECKBOX_BORDER_WIDTH);
    for (size, box_size, height, font, stroke_size) in [
        (CheckboxSize::Small, 12.0, 28.0, 12.0, 1.0),
        (CheckboxSize::Medium, 16.0, 32.0, 14.0, 1.5),
        (CheckboxSize::Large, 20.0, 40.0, 16.0, 2.0),
    ] {
        let (box_token, height_token, font_token, stroke_token) = size_tokens(size);
        assert_eq!(pixels(theme, box_token), box_size, "{box_token}");
        assert_eq!(pixels(theme, height_token), height, "{height_token}");
        assert_eq!(pixels(theme, font_token), font, "{font_token}");
        assert!(box_size < height, "{size:?} keeps its box inside its row");
        // The mark is painted on the box's content area, so the border has to
        // leave room for it at every rung.
        let unit = (box_size - 2.0 * border) / MARK_BOX;
        assert!(unit > 0.0, "{size:?} keeps a positive mark unit");
        assert_eq!(
            pixels(theme, stroke_token),
            stroke_size,
            "{size:?} keeps the mark on the glyph stroke ladder, not on its box"
        );
    }
}

#[test]
fn checkbox_motion_resolves_the_reference_rungs() {
    let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
    assert_eq!(
        duration(theme, crate::theme::keys::CHECKBOX_DURATION),
        Duration::from_millis(200),
        "the box changes state on the enter rung"
    );
    assert_eq!(
        duration(theme, crate::theme::keys::CHECKBOX_DURATION_FAST),
        Duration::from_millis(100),
        "the mark erases, the delay runs and the box shrinks on the exit rung"
    );
    assert_eq!(
        number(theme, crate::theme::keys::CHECKBOX_PRESS_SCALE),
        0.9,
        "the press scales the box the way the reference does"
    );
    assert_eq!(
        pixels(theme, crate::theme::keys::CHECKBOX_HALO_WIDTH),
        4.0,
        "the hover halo reaches 4px past the box"
    );
    for name in [
        "--checkbox-ease",
        "--checkbox-ease-enter",
        "--checkbox-ease-exit",
    ] {
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
}

#[test]
fn checkbox_colours_come_from_yue_roles_in_every_profile() {
    for (mode, accent) in [
        (ThemeMode::Light, Accent::Azure),
        (ThemeMode::Dark, Accent::Azure),
        (ThemeMode::Light, Accent::Neutral),
        (ThemeMode::Dark, Accent::Neutral),
    ] {
        let theme = YueTheme::new(mode, accent);
        let fill = rgba(theme, crate::theme::keys::CHECKBOX_BACKGROUND_CHECKED);
        let mark = rgba(theme, crate::theme::keys::CHECKBOX_MARK_COLOR);
        assert!(
            fill.a > 0.99 && mark.a > 0.99,
            "{mode:?}/{accent:?} keeps an opaque fill and mark, so the mark is not a ghost"
        );
        let disabled = rgba(theme, crate::theme::keys::CHECKBOX_DISABLED_BACKGROUND);
        assert!(
            disabled.a < fill.a,
            "{mode:?}/{accent:?} dims the disabled fill below the checked one"
        );
        assert!(
            rgba(theme, crate::theme::keys::CHECKBOX_DISABLED_BORDER_COLOR).a == 0.0,
            "{mode:?}/{accent:?} drops the disabled border out, as the reference does"
        );
        let halo = rgba(theme, crate::theme::keys::CHECKBOX_HALO_COLOR);
        assert!(
            halo.a > 0.0 && halo.a < 0.5,
            "{mode:?}/{accent:?} keeps the hover halo a tint, not a fill"
        );
    }
}

use crate::icon::{CHECK_LINE, MARK_BOX};
