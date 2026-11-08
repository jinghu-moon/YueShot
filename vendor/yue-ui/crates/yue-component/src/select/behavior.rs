use super::{YueSelectState, model::*};
use gpui_kit::{App, Context, KeyBinding, KeyDownEvent, Window};
use std::time::Duration;

gpui_kit::actions!(
    yue_select,
    [
        Next,
        Previous,
        First,
        Last,
        PageNext,
        PagePrevious,
        Accept,
        Leave,
        LeaveBack,
        Find,
        TagBack,
        Dismiss
    ]
);
pub(crate) fn init(cx: &mut App) {
    for context in ["YueSelectList", "YueSelectQuery > Input"] {
        cx.bind_keys([
            KeyBinding::new("down", Next, Some(context)),
            KeyBinding::new("up", Previous, Some(context)),
            KeyBinding::new("enter", Accept, Some(context)),
            KeyBinding::new("tab", Leave, Some(context)),
            KeyBinding::new("shift-tab", LeaveBack, Some(context)),
        ]);
    }
    cx.bind_keys([
        KeyBinding::new("tab", Leave, Some("YueSelectPanel > Button")),
        KeyBinding::new("shift-tab", LeaveBack, Some("YueSelectPanel > Button")),
        KeyBinding::new("tab", Leave, Some("YueSelectPanel > Input")),
        KeyBinding::new("shift-tab", LeaveBack, Some("YueSelectPanel > Input")),
        KeyBinding::new("tab", Leave, Some("YueSelectPanel && !Input")),
        KeyBinding::new("shift-tab", LeaveBack, Some("YueSelectPanel && !Input")),
        KeyBinding::new("home", First, Some("YueSelectList")),
        KeyBinding::new("end", Last, Some("YueSelectList")),
        KeyBinding::new("pageup", PagePrevious, Some("YueSelectList")),
        KeyBinding::new("pagedown", PageNext, Some("YueSelectList")),
        KeyBinding::new("space", Accept, Some("YueSelectList")),
        KeyBinding::new("ctrl-f", Find, Some("YueSelectList")),
        KeyBinding::new(
            "backspace",
            TagBack,
            Some("YueSelect > Select && !Input && !YueSelectList"),
        ),
    ]);
    for context in [
        "YueSelect > Select",
        "YueSelect > Button",
        "YueSelectList",
        "YueSelectQuery > Input",
    ] {
        cx.bind_keys([KeyBinding::new("escape", Dismiss, Some(context))]);
    }
}
#[derive(Clone, Copy)]
pub(super) enum Move {
    Next,
    Previous,
    First,
    Last,
    PageNext(usize),
    PagePrevious(usize),
}
pub(super) fn move_slot(current: Option<usize>, len: usize, movement: Move) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let current = current.unwrap_or(0).min(last);
    Some(match movement {
        Move::Next => current.saturating_add(1).min(last),
        Move::Previous => current.saturating_sub(1),
        Move::First => 0,
        Move::Last => last,
        Move::PageNext(n) => current.saturating_add(n).min(last),
        Move::PagePrevious(n) => current.saturating_sub(n),
    })
}
impl YueSelectState {
    pub(super) fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(window, cx) {
            return;
        }
        if self.collapsed_open {
            self.collapsed_open = false;
            self.collapsed_focus.focus(window, cx);
            cx.notify();
        } else if self.open {
            self.close(SelectCloseReason::Escape, true, window, cx);
        } else {
            cx.propagate();
        }
    }
    pub(super) fn navigate(&mut self, movement: Move, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || !self.editable() || self.composing(window, cx) {
            return;
        }
        self.keyboard_navigation = true;
        let query_focused =
            self.search != SelectSearch::None && self.content_focus(cx).is_focused(window);
        self.list_focus.focus(window, cx);
        if !query_focused {
            self.move_active(movement, cx);
        }
        cx.notify();
    }
    pub(super) fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || !self.editable() || self.composing(window, cx) {
            return;
        }
        if self.search != SelectSearch::None && self.content_focus(cx).is_focused(window) {
            self.list_focus.focus(window, cx);
            cx.notify();
            return;
        }
        if let Some(key) = self.active.clone() {
            self.activate(key, SelectChangeSource::Keyboard, window, cx);
        }
    }
    pub(super) fn leave(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(window, cx) {
            return;
        }
        if let Some(p) = &self.presentation
            && (!p.header_focus.is_empty() || !p.footer_focus.is_empty())
        {
            let mut order = vec![];
            if self.search != SelectSearch::None {
                order.push(self.content_focus(cx));
            }
            order.extend(p.header_focus.iter().cloned());
            order.push(self.list_focus.clone());
            order.extend(p.footer_focus.iter().cloned());
            if let Some(current) = order
                .iter()
                .position(|focus| focus.contains_focused(window, cx))
            {
                let next = if back {
                    current.checked_sub(1)
                } else {
                    Some(current + 1)
                };
                if let Some(target) = next.and_then(|next| order.get(next)) {
                    target.focus(window, cx);
                    return;
                }
            }
        }
        self.close(SelectCloseReason::Tab, true, window, cx);
        // Skip the field's global Tag/clear stops when exiting its popup.
        // Those stops remain available during ordinary field traversal.
        for _ in 0..self.value.keys().len() + 5 {
            if back {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            if !self.owns_focus(window, cx) {
                return;
            }
        }
        window.blur(cx);
    }
    pub(super) fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search != SelectSearch::None {
            self.content_focus(cx).focus(window, cx);
        } else {
            cx.propagate();
        }
    }
    pub(super) fn tag_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != SelectMode::Multiple || !self.focus.is_focused(window) || !self.editable() {
            cx.propagate();
            return;
        }
        if let Some(key) = self
            .value
            .keys()
            .iter()
            .rev()
            .find(|key| self.removable(key))
            && let Some(handle) = if self.hidden_keys.contains(key) {
                Some(&self.collapsed_focus)
            } else {
                self.tag_focus.get(key)
            }
        {
            handle.focus(window, cx);
        }
    }
    pub(super) fn type_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search != SelectSearch::None || !self.editable() {
            return;
        }
        let m = &event.keystroke.modifiers;
        if m.control || m.alt || m.platform {
            return;
        }
        let key = event.keystroke.key.as_str();
        let mut chars = key.chars();
        let Some(character) = chars.next() else {
            return;
        };
        if chars.next().is_some() || character.is_control() || character.is_whitespace() {
            return;
        }
        let now = cx.background_executor().now();
        if self
            .typeahead_at
            .is_none_or(|last| now.duration_since(last) > Duration::from_millis(500))
        {
            self.typeahead = "".into();
        }
        let character = character.to_lowercase().collect::<String>();
        let repeat = self.typeahead.as_str() == character;
        self.typeahead = if repeat {
            character.clone().into()
        } else {
            format!("{}{character}", self.typeahead).into()
        };
        self.typeahead_at = Some(now);
        if !self.open {
            self.change_open(true, SelectCloseReason::Trigger, true, window, cx);
        }
        let start = if repeat {
            self.active
                .as_ref()
                .and_then(|key| {
                    self.rows
                        .iter()
                        .position(|row| row.active_key().as_ref() == Some(key))
                })
                .map_or(0, |ix| ix + 1)
        } else {
            0
        };
        let query = self.typeahead.to_lowercase();
        let count = self.rows.len();
        if let Some(row)=(0..count).map(|offset|(start+offset)%count).map(|ix|&self.rows[ix]).find(|row|self.eligible(row)&&matches!(row,Row::Option(option) if option.label.to_lowercase().starts_with(&query))){self.active=row.active_key();self.list_focus.focus(window,cx);self.scroll_active();cx.notify();cx.stop_propagation();}
    }
}

use gpui_kit::InteractiveElement as _;

pub(super) fn actions(
    root: gpui_kit::Stateful<gpui_kit::Div>,
    cx: &mut Context<YueSelectState>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    root.on_action(cx.listener(|state, _: &Next, w, cx| {
        state.keyboard_navigation = true;
        state.navigate(Move::Next, w, cx);
    }))
    .on_action(cx.listener(|state, _: &Previous, w, cx| {
        state.keyboard_navigation = true;
        state.navigate(Move::Previous, w, cx);
    }))
    .on_action(cx.listener(|state, _: &First, w, cx| state.navigate(Move::First, w, cx)))
    .on_action(cx.listener(|state, _: &Last, w, cx| state.navigate(Move::Last, w, cx)))
    .on_action(cx.listener(|state, _: &PageNext, w, cx| {
        state.navigate(Move::PageNext(state.page_size), w, cx)
    }))
    .on_action(cx.listener(|state, _: &PagePrevious, w, cx| {
        state.navigate(Move::PagePrevious(state.page_size), w, cx)
    }))
    .on_action(cx.listener(|state, _: &Accept, w, cx| state.accept(w, cx)))
    .on_action(cx.listener(|state, _: &Leave, w, cx| state.leave(false, w, cx)))
    .on_action(cx.listener(|state, _: &LeaveBack, w, cx| state.leave(true, w, cx)))
    .on_action(cx.listener(|state, _: &Find, w, cx| state.find(w, cx)))
    .on_action(cx.listener(|state, _: &TagBack, w, cx| state.tag_back(w, cx)))
    .on_action(cx.listener(|state, _: &Dismiss, w, cx| state.dismiss(w, cx)))
    .on_key_down(cx.listener(YueSelectState::type_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_is_bounded() {
        assert_eq!(move_slot(Some(2), 3, Move::Next), Some(2));
        assert_eq!(move_slot(Some(0), 3, Move::Previous), Some(0));
        assert_eq!(move_slot(None, 0, Move::First), None);
    }
    #[test]
    fn pages_and_ends_use_model_positions() {
        assert_eq!(move_slot(Some(5), 100, Move::PageNext(8)), Some(13));
        assert_eq!(move_slot(Some(5), 100, Move::PagePrevious(8)), Some(0));
        assert_eq!(move_slot(Some(5), 100, Move::Last), Some(99));
    }
}
