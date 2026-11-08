use super::element::*;
use gpui_kit::{Bounds, point, px, size};

use super::behavior::*;

use super::*;
use crate::{Accent, ThemeMode};
fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}
#[test]
fn navigation_wraps_and_unknown_keys_stay_outside() {
    assert_eq!(move_to("left", 0, 3), Some(2));
    assert_eq!(move_to("right", 2, 3), Some(0));
    assert_eq!(move_to("end", 0, 0), None);
    assert_eq!(move_to("up", 0, 3), None);
}
#[test]
#[should_panic(expected = "duplicate value")]
fn duplicate_values_rejected() {
    let _ = YueSegmented::new("test", theme(), "a").options([
        YueSegmentedOption::new("a", "A"),
        YueSegmentedOption::new("a", "B"),
    ]);
}
#[test]
#[should_panic(expected = "empty option value")]
fn empty_value_rejected() {
    let _ = YueSegmented::new("test", theme(), "a").options([YueSegmentedOption::new("", "A")]);
}
#[test]
fn missing_or_disabled_selection_uses_first_enabled_stop() {
    let options = [
        YueSegmentedOption::new("a", "A").disabled(true),
        YueSegmentedOption::new("b", "B"),
    ];
    assert_eq!(stop(&options, &"a".into(), false), Some(1));
    assert_eq!(stop(&options, &"unknown".into(), false), Some(1));
    assert_eq!(stop(&options, &"b".into(), true), None);
}
#[test]
fn rounded_masks_stay_inside_and_cover_the_center() {
    let b = Bounds::new(point(px(10.), px(5.)), size(px(80.), px(28.)));
    let masks = rounded_masks(b, px(14.), 1.5);
    assert!(masks.iter().all(|m| m.left() >= b.left()
        && m.right() <= b.right()
        && m.top() >= b.top()
        && m.bottom() <= b.bottom()));
    assert!(masks.iter().any(|m| m.contains(&b.center())));
}
