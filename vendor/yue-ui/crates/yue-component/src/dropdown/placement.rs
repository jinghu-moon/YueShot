use super::*;

/// Where a panel reserves its tick/dot column — the reference's `ind`.
//
// A reserved column is paid for by every row, marked or not, so the marks line
// up; `Absent` is the reference's `null` and the reason its shortcuts can sit
// flush against the panel's right edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuColumn {
    Leading,
    Trailing,
    Absent,
}

// The column a panel reserves, from its rows and the side its caller asked for.
//
// `Auto` follows the reference (`o.side==='r'?'r':has?'l':null`): a menu with
// nothing to mark reserves no column at all, and one with checkable rows puts
// the mark at the leading end. An explicit side is a column whether or not any
// row can fill it.
pub(super) fn menu_column(rows: &[YueMenuItem], side: DropdownIndicatorSide) -> MenuColumn {
    match side {
        DropdownIndicatorSide::Leading => MenuColumn::Leading,
        DropdownIndicatorSide::Trailing => MenuColumn::Trailing,
        DropdownIndicatorSide::Auto => {
            if rows.iter().any(YueMenuItem::is_checkable) {
                MenuColumn::Leading
            } else {
                MenuColumn::Absent
            }
        }
    }
}

// The gap a panel of this kind keeps from its anchor.
//
// The trigger gap comes from the spacing scale rather than a literal, so a
// retuned `--space-12` moves every dropdown's panel with it.
pub(super) fn gap_for(kind: MenuKind, theme: YueTheme) -> Pixels {
    match kind {
        MenuKind::Trigger => px(pixels(theme, crate::theme::keys::SPACE_12)),
        MenuKind::Submenu => SUBMENU_GAP,
        MenuKind::Context => CONTEXT_GAP,
    }
}

// How far a panel sits from its anchor at one point of the entrance ramp. The
// reference's closed panel is `translateY(-4px)` on the side that faces its
// anchor, so it starts tucked against the trigger and opens outward; its rows
// fall the same four pixels, and a panel sliding the other way cancels them into
// a bare fade.
#[cfg(test)]
pub(super) fn offset_for(kind: MenuKind, theme: YueTheme, ramp: f32) -> Pixels {
    gap_for(kind, theme) - px(4.0 * (1.0 - ramp))
}
