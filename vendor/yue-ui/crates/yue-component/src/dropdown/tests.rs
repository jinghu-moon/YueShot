use std::time::{Duration, Instant};

use gpui_kit::base::{Align, Easing, Placement};
use gpui_kit::{Modifiers, SharedString, Size, px};

use super::{
    DropdownAlign, DropdownIndicatorSide, DropdownPlacement, MenuColumn, MenuKind, MenuMark,
    MenuMotion, MenuMove, MenuRowKind, MenuShape, PanelMotion, PanelPath, ROW_ENTER_WINDOW,
    YueMenuItem, curve, dwell_elapsed, gap_for, menu_column, move_for, move_index, offset_for,
    typeable, typeahead,
};
use crate::{Accent, ThemeMode, YueTheme};

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}

fn labels(texts: &[&str]) -> Vec<SharedString> {
    texts.iter().map(|text| SharedString::from(*text)).collect()
}

#[test]
fn arrow_and_edge_keys_map_to_menu_moves() {
    assert_eq!(move_for("down"), Some(MenuMove::Line(1)));
    assert_eq!(move_for("up"), Some(MenuMove::Line(-1)));
    assert_eq!(move_for("home"), Some(MenuMove::First));
    assert_eq!(move_for("end"), Some(MenuMove::Last));
    // Left and right belong to the submenu, not to the walk.
    for key in ["left", "right", "enter", "space", "escape", "tab", "a"] {
        assert_eq!(move_for(key), None, "{key}");
    }
}

#[test]
fn only_a_bare_printable_key_types() {
    let plain = Modifiers::default();
    assert_eq!(typeable("a", &plain), Some('a'));
    assert_eq!(typeable("-", &plain), Some('-'));
    // A named key arrives as its name, which is the demo's `e.key.length === 1`.
    for key in ["escape", "space", "enter", "tab", "home", "f1"] {
        assert_eq!(typeable(key, &plain), None, "{key}");
    }
    // Shift still types: the letter is what the row is spelled with.
    let shifted = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    assert_eq!(typeable("c", &shifted), Some('c'));
    for modified in [
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
        Modifiers {
            platform: true,
            ..Modifiers::default()
        },
    ] {
        assert_eq!(typeable("c", &modified), None);
    }
}

#[test]
fn a_single_column_wraps_and_a_grid_clamps() {
    assert_eq!(move_index(0, None, MenuMove::Line(1), 1), None);
    // Nothing marked yet: the walk enters at the end the key points to.
    assert_eq!(move_index(3, None, MenuMove::Line(1), 1), Some(0));
    assert_eq!(move_index(3, None, MenuMove::Line(-1), 1), Some(2));
    assert_eq!(move_index(3, Some(2), MenuMove::Line(1), 1), Some(0));
    assert_eq!(move_index(3, Some(0), MenuMove::Line(-1), 1), Some(2));
    assert_eq!(move_index(3, None, MenuMove::First, 1), Some(0));
    assert_eq!(move_index(3, None, MenuMove::Last, 1), Some(2));
    assert_eq!(move_index(3, Some(1), MenuMove::First, 3), Some(0));
    assert_eq!(move_index(3, Some(1), MenuMove::Last, 3), Some(2));
    // A grid steps by whole rows and stops at the edge instead of wrapping.
    assert_eq!(move_index(9, Some(0), MenuMove::Line(1), 3), Some(3));
    assert_eq!(move_index(9, Some(8), MenuMove::Line(-1), 3), Some(5));
    assert_eq!(move_index(9, Some(8), MenuMove::Line(1), 3), Some(8));
    assert_eq!(move_index(9, Some(0), MenuMove::Line(-1), 3), Some(0));
    assert_eq!(move_index(9, Some(0), MenuMove::Column(1), 3), Some(1));
    assert_eq!(move_index(9, Some(8), MenuMove::Column(1), 3), Some(8));
    // One column has no sideways step: the arrow opens a submenu or does nothing.
    assert_eq!(move_index(3, Some(1), MenuMove::Column(1), 1), Some(1));
}

#[test]
fn typeahead_walks_from_the_row_after_the_mark() {
    // With nothing marked the walk starts at the first row; with a mark it starts
    // one past it, so the same letter twice moves between its matches.
    let rows = labels(&["ab", "ac"]);
    assert_eq!(
        typeahead(&rows, None, "", None, 'a'),
        (Some(0), SharedString::from("a"))
    );
    assert_eq!(
        typeahead(&rows, Some(0), "", None, 'a'),
        (Some(1), SharedString::from("a"))
    );
    assert_eq!(
        typeahead(&rows, Some(1), "", None, 'a'),
        (Some(0), SharedString::from("a"))
    );
    // The only row a letter spells is still found, by wrapping all the way round.
    let rows = labels(&["ab", "b"]);
    assert_eq!(
        typeahead(&rows, Some(0), "", None, 'a'),
        (Some(0), SharedString::from("a"))
    );
    // A mark is matched as spelled, in either case.
    let rows = labels(&["Copy link"]);
    assert_eq!(
        typeahead(&rows, None, "", None, 'c'),
        (Some(0), SharedString::from("c"))
    );
}

#[test]
fn a_longer_prefix_narrows_and_a_miss_falls_back_to_its_last_letter() {
    let rows = labels(&["copy", "color", "paste"]);
    assert_eq!(
        typeahead(&rows, None, "co", Some(Duration::from_millis(100)), 'l'),
        (Some(1), SharedString::from("col"))
    );
    assert_eq!(
        typeahead(&rows, None, "co", Some(Duration::from_millis(100)), 'p'),
        (Some(0), SharedString::from("cop"))
    );
    // Nothing starts with "coz", and the letter that survives is what the buffer
    // becomes: the demo does not carry a failed prefix into the next keystroke.
    let rows = labels(&["ab", "x"]);
    assert_eq!(
        typeahead(&rows, None, "a", Some(Duration::from_millis(100)), 'x'),
        (Some(1), SharedString::from("x"))
    );
    assert_eq!(
        typeahead(&rows, None, "a", Some(Duration::from_millis(100)), 'q'),
        (None, SharedString::from("q"))
    );
}

#[test]
fn a_silent_prefix_starts_again() {
    let rows = labels(&["ab", "b"]);
    // The demo restarts the buffer after 600ms of silence.
    assert_eq!(
        typeahead(&rows, Some(0), "a", Some(Duration::from_millis(700)), 'b'),
        (Some(1), SharedString::from("b"))
    );
    assert_eq!(
        typeahead(&rows, Some(0), "a", None, 'b'),
        (Some(1), SharedString::from("b"))
    );
}

#[test]
fn the_indicator_slot_follows_the_marks_the_menu_has() {
    let plain = [YueMenuItem::new("Copy"), YueMenuItem::new("Paste")];
    let marked = [
        YueMenuItem::new("Copy").checked(true),
        YueMenuItem::new("Paste"),
    ];
    // An auto menu with nothing to mark reserves no column, which is what lets
    // its shortcuts reach the panel's right edge.
    assert_eq!(
        menu_column(&plain, DropdownIndicatorSide::Auto),
        MenuColumn::Absent
    );
    assert_eq!(
        menu_column(&marked, DropdownIndicatorSide::Auto),
        MenuColumn::Leading
    );
    assert_eq!(
        menu_column(&plain, DropdownIndicatorSide::Leading),
        MenuColumn::Leading
    );
    assert_eq!(
        menu_column(&plain, DropdownIndicatorSide::Trailing),
        MenuColumn::Trailing
    );
    assert_eq!(
        menu_column(&marked, DropdownIndicatorSide::Trailing),
        MenuColumn::Trailing
    );
}

#[test]
fn a_submenu_row_has_to_hold_the_pointer() {
    assert!(!dwell_elapsed(Duration::from_millis(79)));
    assert!(dwell_elapsed(Duration::from_millis(80)));
    assert!(dwell_elapsed(Duration::from_secs(1)));
}

#[test]
fn only_a_trigger_panel_points_back_at_what_opened_it() {
    assert!(MenuKind::Trigger.wants_caret());
    assert!(!MenuKind::Submenu.wants_caret());
    assert!(!MenuKind::Context.wants_caret());
}

#[test]
fn each_kind_of_panel_keeps_its_own_gap() {
    let theme = theme();
    // The reference's 12px under a button, 8px between two panels, none at a pointer.
    assert_eq!(gap_for(MenuKind::Trigger, theme), px(12.));
    assert_eq!(gap_for(MenuKind::Submenu, theme), px(8.));
    assert_eq!(gap_for(MenuKind::Context, theme), px(0.));
}

#[test]
fn a_panel_opens_outward_from_its_anchor() {
    let theme = theme();
    // The reference's `translateY(-4px)` to `none`: tucked against the trigger
    // while the ramp has not moved, at its own gap once it has. The other sign
    // would slide the panel into the rows, which fall the same four pixels.
    assert_eq!(offset_for(MenuKind::Trigger, theme, 0.), px(8.));
    assert_eq!(offset_for(MenuKind::Trigger, theme, 0.5), px(10.));
    assert_eq!(offset_for(MenuKind::Trigger, theme, 1.), px(12.));
    assert_eq!(offset_for(MenuKind::Submenu, theme, 0.), px(4.));
    assert_eq!(offset_for(MenuKind::Submenu, theme, 1.), px(8.));
}

#[test]
fn a_panel_and_its_submenu_do_not_share_a_motion_channel() {
    assert_eq!(PanelPath::Root.channel("presence"), "yue-dropdown-presence");
    assert_eq!(
        PanelPath::Submenu(3).channel("presence"),
        "yue-dropdown-submenu-3-presence"
    );
    assert_ne!(
        PanelPath::Root.channel("row-0"),
        PanelPath::Submenu(0).channel("row-0")
    );
}

#[test]
fn placement_and_align_ask_base_for_the_reference_sides() {
    // Base treats an unset placement as "above first", so Auto names the lower
    // side explicitly to keep the demo's open-downwards order.
    assert_eq!(DropdownPlacement::Auto.side(), Placement::Bottom);
    assert_eq!(DropdownPlacement::Below.side(), Placement::Bottom);
    assert_eq!(DropdownPlacement::Above.side(), Placement::Top);
    assert_eq!(DropdownPlacement::Leading.side(), Placement::Left);
    assert_eq!(DropdownPlacement::Trailing.side(), Placement::Right);
    assert_eq!(DropdownAlign::Start.align(), Align::Start);
    assert_eq!(DropdownAlign::Center.align(), Align::Center);
    assert_eq!(DropdownAlign::End.align(), Align::End);
}

#[test]
fn a_row_is_named_by_its_id_then_its_label() {
    assert_eq!(YueMenuItem::new("Copy").key(7), SharedString::from("Copy"));
    assert_eq!(
        YueMenuItem::new("Copy").id("row-copy").key(7),
        SharedString::from("row-copy")
    );
    // A separator has no text to key on, so its position in the menu does.
    assert_eq!(YueMenuItem::separator().key(4), SharedString::from("row-4"));
    assert_eq!(YueMenuItem::heading("").key(0), SharedString::from("row-0"));
}

#[test]
fn the_keyboard_stops_only_at_rows_it_can_run() {
    let rows = [
        YueMenuItem::new("Copy"),
        YueMenuItem::submenu("Share", [YueMenuItem::new("Copy")]),
        YueMenuItem::heading("Group"),
        YueMenuItem::separator(),
        YueMenuItem::loading("Loading"),
        YueMenuItem::new("Delete").disabled(true),
        YueMenuItem::submenu("Share", [YueMenuItem::new("Copy")]).disabled(true),
    ];
    assert_eq!(
        rows.iter().map(YueMenuItem::is_roving).collect::<Vec<_>>(),
        vec![true, true, false, false, false, false, false]
    );
    assert_eq!(
        rows.iter()
            .map(YueMenuItem::is_checkable)
            .collect::<Vec<_>>(),
        vec![false; 7]
    );
    assert!(YueMenuItem::new("Wrap").checked(false).is_checkable());
    assert!(YueMenuItem::new("Small").radio(true).is_checkable());
    assert_eq!(
        YueMenuItem::new("Wrap").checked(true).mark,
        MenuMark::Check(true)
    );
    assert_eq!(
        YueMenuItem::new("Share").radio(false).mark,
        MenuMark::Radio(false)
    );
    assert_eq!(YueMenuItem::new("Copy").kind, MenuRowKind::Command);
    assert_eq!(YueMenuItem::loading("Loading").kind, MenuRowKind::Loading);
}

#[test]
fn opening_a_panel_clears_its_mark_and_restarts_its_entrance() {
    let now = Instant::now();
    let mut panel = PanelMotion::default();
    assert!(!panel.is_entering(now));
    panel.hovered = Some(2);
    panel.active = Some(1);
    panel.prefix = SharedString::from("co");
    panel.note_open(now);
    assert_eq!(panel.generation, 1);
    assert_eq!(panel.active, None);
    assert_eq!(panel.hovered, None);
    assert_eq!(panel.prefix, SharedString::from(""));
    assert_eq!(panel.dwell(now), Duration::ZERO);
    assert!(panel.is_entering(now));
    // Past the entrance window a row that rebuilds must not re-run it.
    let past = now + ROW_ENTER_WINDOW + Duration::from_millis(1);
    assert!(!panel.is_entering(past));
    // A second open gets a fresh generation, so the keyframes start again.
    panel.note_open(past);
    assert_eq!(panel.generation, 2);
    assert!(panel.is_entering(past));
    // And it re-arms the tint seed, even if the last open had already spent
    // it: the row this open highlights must fade up rather than be lit from
    // its very first sample.
    panel.tint_seeded = true;
    panel.note_open(past + Duration::from_millis(1));
    assert!(!panel.tint_seeded);
}

#[test]
fn a_submenu_waits_for_its_dwell_and_then_owns_the_mark() {
    let now = Instant::now();
    let panel = PanelMotion {
        hovered_at: Some(now),
        ..Default::default()
    };
    assert_eq!(
        panel.dwell(now + Duration::from_millis(40)),
        Duration::from_millis(40)
    );
    assert!(!dwell_elapsed(panel.dwell(now + Duration::from_millis(40))));
    assert!(dwell_elapsed(panel.dwell(now + Duration::from_millis(90))));
}

#[test]
fn a_typed_key_extends_the_prefix_until_the_buffer_goes_quiet() {
    let now = Instant::now();
    let rows = labels(&["copy", "color", "paste"]);
    let mut panel = PanelMotion::default();
    panel.note_open(now);
    assert_eq!(panel.type_into(&rows, None, now, 'c'), Some(0));
    assert_eq!(panel.prefix, SharedString::from("c"));
    // Still typing: "co" looks past the mark and lands on "color".
    assert_eq!(
        panel.type_into(&rows, Some(0), now + Duration::from_millis(80), 'o'),
        Some(1)
    );
    assert_eq!(panel.prefix, SharedString::from("co"));
    // Silence restarts it, and a key that matches nothing moves nothing.
    assert_eq!(
        panel.type_into(&rows, Some(1), now + Duration::from_millis(900), 'z'),
        None
    );
    assert_eq!(panel.prefix, SharedString::from("z"));
}

#[test]
fn opening_a_host_panel_marks_one_row_and_arms_nothing() {
    let now = Instant::now();
    let mut motion = MenuMotion {
        armed: Some(4),
        ..Default::default()
    };
    motion.note_open(now, Some(2));
    assert_eq!(motion.armed, None);
    assert_eq!(motion.root.active, Some(2));
    assert!(motion.root.is_entering(now));
    assert!(motion.submenu.is_entering(now));
    // The row the panel opens on is marked from its first frame, so both
    // panels must still owe that frame an unlit tint seed.
    assert!(!motion.root.tint_seeded);
    assert!(!motion.submenu.tint_seeded);

    // Arming a submenu gives it the only mark, and re-arming the same row is a
    // no-op so a repaint does not restart its entrance.
    let submenu_generation = motion.submenu.generation;
    motion.arm(Some(1), Some(0), now + Duration::from_millis(100));
    assert_eq!(motion.armed, Some(1));
    assert_eq!(motion.submenu.active, Some(0));
    assert_eq!(motion.root.active, None);
    assert_eq!(motion.submenu.generation, submenu_generation + 1);
    motion.submenu.active = None;
    motion.arm(Some(1), Some(3), now + Duration::from_millis(200));
    assert_eq!(motion.submenu.active, None);
    assert_eq!(motion.submenu.generation, submenu_generation + 1);

    // A different row arms a fresh submenu panel.
    motion.arm(Some(2), Some(0), now + Duration::from_millis(300));
    assert_eq!(motion.submenu.active, Some(0));
    assert_eq!(motion.submenu.generation, submenu_generation + 2);
    // Closing asks for no index and takes the mark back to the root row.
    motion.root.active = Some(2);
    motion.arm(None, None, now + Duration::from_millis(400));
    assert_eq!(motion.armed, None);
    assert_eq!(motion.root.active, Some(2));
}

#[test]
fn a_panel_is_capped_before_it_is_wider_than_the_window() {
    let shape = MenuShape::new(theme());
    assert_eq!(
        shape.bounds(Size {
            width: px(1000.),
            height: px(800.)
        }),
        (px(192.), px(320.))
    );
    // `min(--mw, 90vw)` of the reference, against this frame's viewport.
    assert_eq!(
        shape
            .bounds(Size {
                width: px(300.),
                height: px(800.)
            })
            .1,
        px(270.)
    );
    let mut asked = MenuShape::new(theme());
    asked.panel_width = Some(px(400.));
    assert_eq!(
        asked
            .bounds(Size {
                width: px(1000.),
                height: px(800.)
            })
            .0,
        px(400.)
    );
}

#[test]
fn panel_curves_come_from_the_easing_tokens() {
    let theme = theme();
    let Easing::CubicBezier { x1, y1, x2, y2 } = curve(theme, "--motion-ease-enter") else {
        panic!("--motion-ease-enter has to read as a cubic-bezier curve");
    };
    assert_eq!((x1, y1, x2, y2), (0.0, 0.0, 0.38, 0.9));
    let Easing::CubicBezier { x1, y1, x2, y2 } = curve(theme, "--motion-ease-exit") else {
        panic!("--motion-ease-exit has to read as a cubic-bezier curve");
    };
    assert_eq!((x1, y1, x2, y2), (0.2, 0.0, 1.0, 0.9));
}
