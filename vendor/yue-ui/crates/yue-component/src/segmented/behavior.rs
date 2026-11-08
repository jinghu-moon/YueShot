//! Controlled selection and keyboard navigation rules.

use super::*;

use gpui_kit::SharedString;

pub(super) fn stop(
    options: &[YueSegmentedOption],
    value: &SharedString,
    disabled: bool,
) -> Option<usize> {
    if disabled {
        return None;
    }
    options
        .iter()
        .position(|o| !o.disabled && o.value == *value)
        .or_else(|| options.iter().position(|o| !o.disabled))
}

pub(super) fn move_to(key: &str, current: usize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match key {
        "right" => Some((current + 1) % count),
        "left" => Some((current + count - 1) % count),
        "home" => Some(0),
        "end" => Some(count - 1),
        _ => None,
    }
}
