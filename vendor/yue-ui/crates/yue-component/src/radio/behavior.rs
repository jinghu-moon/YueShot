//! Roving focus and radio navigation rules.

use super::*;

use gpui_kit::SharedString;

/// The option Tab reaches: the selected one while it can still be operated,
/// otherwise the first operable option, and nothing when the whole group is inert.
///
/// Unlike a checkbox group's select-all header, which reports a state and so keeps
/// reading its options even while the group is disabled, this is about what Tab
/// can land on — a disabled group offers nothing.
pub(super) fn tab_stop_index(
    options: &[YueRadioOption],
    selected: Option<&SharedString>,
    group_disabled: bool,
) -> Option<usize> {
    if group_disabled {
        return None;
    }
    let operable = |option: &YueRadioOption| !option.disabled;
    options
        .iter()
        .position(|option| operable(option) && Some(&option.value) == selected)
        .or_else(|| options.iter().position(operable))
}

/// Where an arrow key inside a group goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RadioMove {
    Next,
    Previous,
    First,
    Last,
}

/// The keystroke a group of this running direction reads as a move, or `None` for
/// a key the group leaves to the rest of the window.
///
/// A vertical column is read top to bottom and a horizontal row left to right, so
/// the perpendicular pair stays free — a nested group of the other orientation
/// keeps its own keys.
pub(super) fn move_for(key: &str, vertical: bool) -> Option<RadioMove> {
    match key {
        "home" => Some(RadioMove::First),
        "end" => Some(RadioMove::Last),
        "down" if vertical => Some(RadioMove::Next),
        "up" if vertical => Some(RadioMove::Previous),
        "right" if !vertical => Some(RadioMove::Next),
        "left" if !vertical => Some(RadioMove::Previous),
        _ => None,
    }
}

/// The option a move lands on, counted among the `operable` options in visual
/// order and skipping the ones that cannot be chosen. `current` is the focused
/// option's own place in that list.
///
/// Next and Previous wrap, so an arrow key never runs out of group; First and Last
/// ignore `current`. With nothing to move to there is nothing to focus.
pub(super) fn move_index(operable: usize, current: usize, direction: RadioMove) -> Option<usize> {
    if operable == 0 {
        return None;
    }
    Some(match direction {
        RadioMove::First => 0,
        RadioMove::Last => operable - 1,
        RadioMove::Next => (current + 1) % operable,
        RadioMove::Previous => (current + operable - 1) % operable,
    })
}
