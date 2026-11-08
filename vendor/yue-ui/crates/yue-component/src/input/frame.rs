//! Shared input frame semantics, character limits and counters.

use super::*;

use gpui_kit::base::input::{InputBaseState, InputModeKind};

use gpui_kit::{
    AccessibleAction, AnyElement, App, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, prelude::FluentBuilder as _,
};

use crate::YueTheme;

use crate::tokens::{rems, rgba as color};

/// An input frame with its id, styles and accessibility setters applied.
///
/// `test_support()` registers the frame for UI queries; without the `test-support`
/// feature of `gpui-kit` the alias collapses to the plain stateful div.
type ObservedFrame = gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>;

pub(crate) fn border_token(
    status: InputStatus,
    disabled: bool,
    focused: bool,
) -> crate::theme::TokenKey {
    if disabled {
        crate::theme::keys::INPUT_BORDER_COLOR_DISABLED
    } else if status == InputStatus::Normal && focused {
        crate::theme::keys::INPUT_BORDER_COLOR_FOCUS
    } else {
        match status {
            InputStatus::Normal => crate::theme::keys::INPUT_BORDER_COLOR,
            InputStatus::Success => crate::theme::keys::SUCCESS,
            InputStatus::Warning => crate::theme::keys::WARNING,
            InputStatus::Error => crate::theme::keys::INPUT_BORDER_COLOR_INVALID,
        }
    }
}

/// Clamp the value to a character limit and park the caret where the user left it.
///
/// GPUI Base has no `max_length`: its `validate` and `pattern` hooks only see the
/// text one edit inserts, so a paste or an IME commit that overshoots is caught
/// here instead. `replace_all` keeps the trim undoable, unlike `set_value`.
pub(super) fn truncate_to_limit<M: InputModeKind>(
    state: &Entity<InputBaseState<M>>,
    limit: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let (value, selection) = state.update(cx, |input, _| (input.value(), input.selected_range()));
    let characters: Vec<char> = value.chars().collect();
    if characters.len() <= limit {
        return;
    }

    let mut caret_characters = 0;
    let mut bytes = 0;
    while bytes < selection.start && caret_characters < characters.len() {
        bytes += characters[caret_characters].len_utf8();
        caret_characters += 1;
    }
    let kept: String = characters[..limit].iter().collect();
    let caret_bytes = kept
        .chars()
        .take(caret_characters.min(limit))
        .map(char::len_utf8)
        .sum();

    state.update(cx, |input, cx| {
        input.replace_all(kept, window, cx);
        input.set_selected_range(caret_bytes..caret_bytes, cx);
    });
}

pub(super) fn counter_slot(theme: YueTheme, count: usize, limit: Option<usize>) -> AnyElement {
    let text = match limit {
        Some(limit) => format!("{count} / {limit}"),
        None => count.to_string(),
    };
    div()
        .flex_shrink_0()
        .text_size(rems(theme, crate::theme::keys::INPUT_FONT_SIZE_SM))
        .text_color(color(theme, crate::theme::keys::INPUT_AFFIX_COLOR))
        .child(text)
        .into_any_element()
}

/// The accessibility node a Yue input frame publishes.
pub(super) struct FrameA11y {
    pub(super) role: Role,
    pub(super) label: Option<SharedString>,
    pub(super) description: Option<SharedString>,
    pub(super) placeholder: Option<SharedString>,
    pub(super) value: Option<SharedString>,
    pub(super) editable: bool,
}

/// Turn the surrounding frame into the control's accessibility node.
///
/// GPUI Base renders the editor as an unlabelled generic container, so the frame
/// is the only node assistive technology can name, read, or focus.
pub(super) fn accessible_frame<M: InputModeKind>(
    frame: ObservedFrame,
    semantics: FrameA11y,
    state: &Entity<InputBaseState<M>>,
    focus_handle: &gpui_kit::FocusHandle,
) -> ObservedFrame {
    let FrameA11y {
        role,
        label,
        description,
        placeholder,
        value,
        editable,
    } = semantics;
    let focus_state = state.clone();
    let write_state = state.clone();
    frame
        .track_focus(focus_handle)
        .role(role)
        .when_some(label, |this, label| this.aria_label(label))
        .when_some(description, |this, description| {
            this.aria_description(description)
        })
        .when_some(placeholder, |this, placeholder| {
            this.aria_placeholder(placeholder)
        })
        .when_some(value, |this, value| this.aria_value(value))
        .on_a11y_action(AccessibleAction::Focus, move |_, window, cx| {
            focus_state.update(cx, |input, cx| input.focus(window, cx));
        })
        .when(editable, |this| {
            this.on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                if let Some(gpui_kit::accesskit::ActionData::Value(text)) = data {
                    write_state.update(cx, |input, cx| input.set_value(text.as_ref(), window, cx));
                }
            })
        })
}
