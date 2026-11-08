//! Pure group selection rules.

use super::*;

use gpui_kit::SharedString;

/// The select-all header's state, given the options and the current selection.
///
/// `E` is the set of options a click on the header can still change — every
/// option that is not individually disabled — and `S` the selection. No eligible
/// option is selected, or there are none at all, leaves the header unchecked; a
/// selection that covers all of `E` checks it; anything in between is the mixed
/// state. Disabled options never decide the header, whether or not they are
/// selected.
pub(super) fn select_all_state(
    options: &[YueCheckboxOption],
    selected: &[SharedString],
) -> CheckboxState {
    let eligible = eligible_values(options);
    if eligible.is_empty() || eligible.iter().all(|value| !selected.contains(value)) {
        CheckboxState::Unchecked
    } else if eligible.iter().all(|value| selected.contains(value)) {
        CheckboxState::Checked
    } else {
        CheckboxState::Indeterminate
    }
}

/// The values a header click is allowed to change.
///
/// A group disabled as a whole still reports its options through the header: the
/// group flag makes the header inert rather than rewriting what it says, so this
/// looks only at the options' own `disabled`.
pub(super) fn eligible_values(options: &[YueCheckboxOption]) -> Vec<SharedString> {
    options
        .iter()
        .filter(|option| !option.disabled)
        .map(|option| option.value.clone())
        .collect()
}

/// Applying the header: checking moves to `S ∪ E` and clearing to `S − E`, so a
/// disabled option keeps whatever value it had and the caller's own order of the
/// selection is preserved. Values the caller selected that are not offered by
/// this group pass through untouched.
pub(super) fn apply_select_all(
    options: &[YueCheckboxOption],
    selected: &[SharedString],
    checked: bool,
) -> Vec<SharedString> {
    let eligible = eligible_values(options);
    if checked {
        let mut next = selected.to_vec();
        next.extend(
            eligible
                .into_iter()
                .filter(|value| !next.contains(value))
                .collect::<Vec<_>>(),
        );
        next
    } else {
        selected
            .iter()
            .filter(|value| !eligible.contains(value))
            .cloned()
            .collect()
    }
}

/// Applying one option: the value enters or leaves the selection in place.
pub(super) fn apply_option(
    selected: &[SharedString],
    value: &SharedString,
    state: CheckboxState,
) -> Vec<SharedString> {
    let mut next = selected.to_vec();
    if state.chosen() {
        if !next.contains(value) {
            next.push(value.clone());
        }
    } else {
        next.retain(|entry| entry != value);
    }
    next
}
