//! Headless checks for what Yue adds on top of GPUI Base's radio: the group's
//! controlled write-back, the single tab stop the whole set lives behind, and the
//! arrow keys that move the selection between operable options.
//!
//! Queries go through `gpui-base`'s observation layer because enabling
//! `gpui-kit/test-support` would also activate the optional GPUI Fast backend, whose
//! pinned dependencies conflict with the GPUI backend this workspace locks.
//!
//! What the layer cannot see stays unclaimed here: the halo, the press shrink and
//! the dot's growth are motion the frame-by-frame snapshot does not sample, the
//! group's `aria-orientation` has no getter, and a segmented row's outline,
//! dividers and state fills are paint — the tests that cover that appearance read
//! its geometry and its roles, not its colours.

use gpui_base::test_support::ElementSnapshot;
use gpui_kit::{
    App, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Render, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestAppContext, VisualTestContext, Window, div, rems,
};
use yue_component::{
    Accent, GroupDirection, RadioAppearance, ThemeMode, YueRadioGroup, YueRadioOption, YueTheme,
};

/// Four options with the second one disabled, so every test that counts options
/// also has one to step over.
const VALUES: [&str; 4] = ["png", "svg", "pdf", "jpg"];

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}

fn options() -> [YueRadioOption; 4] {
    [
        YueRadioOption::new(VALUES[0], "PNG"),
        YueRadioOption::new(VALUES[1], "SVG").disabled(true),
        YueRadioOption::new(VALUES[2], "PDF"),
        YueRadioOption::new(VALUES[3], "JPG"),
    ]
}

fn render(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn node(window: &Window, label: &str) -> ElementSnapshot {
    let matches: Vec<ElementSnapshot> = gpui_base::test_support::snapshots(window)
        .into_iter()
        .filter(|node| node.label() == Some(label))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one node labelled {label}, found {}",
        matches.len()
    );
    matches.into_iter().next().expect("one match")
}

/// The same view the tests drive: a named focusable before and after the group, so
/// a tab stop inside the group can be told from one outside it.
struct Probe {
    theme: YueTheme,
    selected: Option<SharedString>,
    changes: Vec<SharedString>,
    direction: GroupDirection,
    appearance: RadioAppearance,
    before: FocusHandle,
    after: FocusHandle,
}

impl Probe {
    fn new(
        selected: Option<&str>,
        direction: GroupDirection,
        appearance: RadioAppearance,
    ) -> impl FnOnce(&mut Window, &mut Context<Self>) -> Self {
        let selected = selected.map(SharedString::from);
        move |_, cx| Self {
            theme: theme(),
            selected,
            changes: Vec::new(),
            direction,
            appearance,
            before: cx.focus_handle(),
            after: cx.focus_handle(),
        }
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected.clone();
        let this = cx.entity();
        div()
            .id("probe")
            .flex()
            .flex_col()
            .gap(rems(1.0))
            .child(neighbour("before", "前一个控件", &self.before))
            .child(
                YueRadioGroup::new("format", self.theme, selected)
                    .label("导出格式")
                    .direction(self.direction)
                    .appearance(self.appearance)
                    .options(options())
                    // The controlled contract: the group reports a value and the
                    // owner writes it back before the next draw.
                    .on_change(move |value, _, cx| {
                        let announced = value.clone();
                        this.update(cx, |probe, cx| {
                            probe.changes.push(announced);
                            probe.selected = Some(value);
                            cx.notify();
                        });
                    }),
            )
            .child(neighbour("after", "后一个控件", &self.after))
    }
}

fn neighbour(id: &'static str, label: &'static str, handle: &FocusHandle) -> impl IntoElement {
    // GPUI starts a fresh handle outside the tab order, so a fixture that has to be
    // reachable by Tab declares it the way a real focusable control does.
    gpui_kit::base::TestSupportExt::test_support(div().id(id))
        .track_focus(&handle.clone().tab_stop(true))
        .aria_label(label)
        .child(label)
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    selected: Option<&str>,
    direction: GroupDirection,
) -> (Entity<Probe>, &'a mut VisualTestContext) {
    open_with(cx, selected, direction, RadioAppearance::Indicator)
}

fn open_with<'a>(
    cx: &'a mut TestAppContext,
    selected: Option<&str>,
    direction: GroupDirection,
    appearance: RadioAppearance,
) -> (Entity<Probe>, &'a mut VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.add_window_view(Probe::new(selected, direction, appearance))
}

/// The values the group has reported so far, oldest first.
fn reported(view: &Entity<Probe>, cx: &mut VisualTestContext) -> Vec<String> {
    view.update(cx, |probe, _| {
        probe
            .changes
            .iter()
            .map(|value| value.to_string())
            .collect()
    })
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(render);
}

/// Tab to the next stop and repaint: a snapshot records the focus its frame saw,
/// so an assertion about focus has to follow a frame.
fn tab_forward(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.focus_next(cx);
        render(window, cx);
    });
}

fn tab_backward(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.focus_prev(cx);
        render(window, cx);
    });
}

fn state(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|window, _| node(window, label))
}

/// GPUI maps an activation key to a click on key *up*, while `simulate_keystrokes`
/// only sends the press, so both halves are dispatched by hand.
fn activate(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).expect("fixture key");
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
}

#[gpui_kit::test]
fn the_group_publishes_one_named_radio_button_per_option(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Some("png"), GroupDirection::Vertical);
    draw(cx);
    cx.update(|window, _| {
        assert_eq!(
            node(window, "导出格式").role(),
            Some(Role::RadioGroup),
            "the options sit inside a group the tree can name"
        );
        for (index, label) in ["PNG", "SVG", "PDF", "JPG"].iter().enumerate() {
            let radio = node(window, label);
            assert_eq!(radio.role(), Some(Role::RadioButton), "{label}");
            assert_eq!(
                radio.checked(),
                Some(index == 0),
                "{label} announces the option the owner holds"
            );
        }
        assert_eq!(
            node(window, "SVG").disabled(),
            Some(true),
            "the disabled option says so in the tree"
        );
        assert_ne!(
            node(window, "PNG").disabled(),
            Some(true),
            "and the others are not marked disabled"
        );
    });
}

#[gpui_kit::test]
fn clicking_an_option_writes_the_selection_back_once(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Some("png"), GroupDirection::Vertical);
    draw(cx);
    let target = state(cx, "PDF").bounds().center();
    cx.simulate_click(target, Modifiers::default());
    draw(cx);
    assert_eq!(reported(&view, cx), ["pdf"], "one report per change");
    cx.update(|window, _| {
        assert_eq!(node(window, "PDF").checked(), Some(true));
        assert_eq!(
            node(window, "PNG").checked(),
            Some(false),
            "mutual exclusion is the group's, so the previous option reads clear"
        );
    });
    let selected = view.update(cx, |probe, _| {
        probe
            .selected
            .as_ref()
            .map(|value| value.to_string())
            .unwrap_or_default()
    });
    assert_eq!(selected, "pdf", "the owner holds what the group reported");
}

#[gpui_kit::test]
fn a_chosen_or_disabled_option_never_asks_to_change(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Some("png"), GroupDirection::Vertical);
    draw(cx);
    for label in ["PNG", "SVG"] {
        let centre = state(cx, label).bounds().center();
        cx.simulate_click(centre, Modifiers::default());
    }
    draw(cx);
    assert!(
        reported(&view, cx).is_empty(),
        "re-clicking the chosen option and clicking a disabled one both report nothing"
    );
    assert_eq!(
        view.update(cx, |probe, _| probe
            .selected
            .as_ref()
            .map(|value| value.to_string())),
        Some("png".to_string()),
        "and the selection stays where it was"
    );
}

#[gpui_kit::test]
fn tab_crosses_the_group_in_one_step_and_lands_on_the_chosen_option(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Some("pdf"), GroupDirection::Vertical);
    draw(cx);
    tab_forward(cx);
    assert_eq!(
        state(cx, "前一个控件").focused(),
        Some(true),
        "fixture: the control before the group is the first stop"
    );
    tab_forward(cx);
    assert_eq!(
        state(cx, "PDF").focused(),
        Some(true),
        "Tab enters the group on the chosen option"
    );
    assert_eq!(state(cx, "PNG").focused(), Some(false));
    assert_eq!(state(cx, "JPG").focused(), Some(false));
    assert_eq!(
        state(cx, "SVG").focused(),
        None,
        "a disabled option registers no focus binding at all"
    );
    tab_forward(cx);
    assert_eq!(
        state(cx, "后一个控件").focused(),
        Some(true),
        "one more Tab leaves the whole group behind"
    );
    tab_backward(cx);
    assert_eq!(
        state(cx, "PDF").focused(),
        Some(true),
        "Shift+Tab returns to the same stop"
    );

    // The stop follows the selection, so Tab keeps entering where the group is.
    view.update(cx, |probe, _| probe.selected = Some("jpg".into()));
    let before = view.update(cx, |probe, _| probe.before.clone());
    cx.update(|window, cx| {
        window.focus(&before, cx);
        render(window, cx);
    });
    tab_forward(cx);
    assert_eq!(
        state(cx, "JPG").focused(),
        Some(true),
        "the stop moved with the value the owner wrote"
    );
    assert_eq!(state(cx, "PDF").focused(), Some(false));
}

#[gpui_kit::test]
fn an_empty_or_unmatched_selection_still_leaves_the_group_enterable(cx: &mut TestAppContext) {
    for (selected, expected) in [(None, "PNG"), (Some("removed"), "PNG")] {
        let (_view, cx) = open(cx, selected, GroupDirection::Vertical);
        draw(cx);
        tab_forward(cx);
        tab_forward(cx);
        assert_eq!(
            state(cx, expected).focused(),
            Some(true),
            "with {selected:?} chosen the group is entered on the first operable option"
        );
        assert_eq!(
            state(cx, "SVG").focused(),
            None,
            "a disabled option registers no focus binding at all"
        );
    }
}

#[gpui_kit::test]
fn arrow_keys_move_focus_and_selection_and_skip_the_disabled_option(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, None, GroupDirection::Vertical);
    draw(cx);
    tab_forward(cx);
    tab_forward(cx);
    assert_eq!(
        state(cx, "PNG").focused(),
        Some(true),
        "an empty selection still enters the group on the first operable option"
    );

    // Down goes to the next operable option, over the disabled SVG, and selects it.
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["pdf"],
        "the option an arrow lands on is chosen"
    );
    assert_eq!(state(cx, "PDF").focused(), Some(true));
    assert_eq!(state(cx, "PDF").checked(), Some(true));

    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(reported(&view, cx), ["pdf", "jpg"]);

    // Past the end it wraps to the first operable option, and back over the start
    // it wraps to the last.
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(reported(&view, cx), ["pdf", "jpg", "png"], "wraps forward");
    cx.simulate_keystrokes("up");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["pdf", "jpg", "png", "jpg"],
        "wraps back to the last option"
    );

    cx.simulate_keystrokes("home");
    draw(cx);
    assert_eq!(
        state(cx, "PNG").checked(),
        Some(true),
        "Home is the first operable option"
    );
    cx.simulate_keystrokes("end");
    draw(cx);
    assert_eq!(state(cx, "JPG").checked(), Some(true), "End is the last");

    // The perpendicular pair belongs to whatever else is on the screen, so the
    // group reports nothing for it.
    let before = reported(&view, cx).len();
    cx.simulate_keystrokes("left right");
    draw(cx);
    assert_eq!(
        reported(&view, cx).len(),
        before,
        "a vertical group leaves left and right alone"
    );
}

#[gpui_kit::test]
fn a_horizontal_group_reads_left_and_right_instead(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, None, GroupDirection::Horizontal);
    draw(cx);
    tab_forward(cx);
    tab_forward(cx);
    cx.simulate_keystrokes("right");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["pdf"],
        "the row moves along its own axis"
    );
    let before = reported(&view, cx).len();
    cx.simulate_keystrokes("up down");
    draw(cx);
    assert_eq!(
        reported(&view, cx).len(),
        before,
        "and leaves the column's keys alone"
    );
}

#[gpui_kit::test]
fn space_and_enter_activate_the_focused_option(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, None, GroupDirection::Vertical);
    draw(cx);
    tab_forward(cx);
    tab_forward(cx);
    assert_eq!(state(cx, "PNG").focused(), Some(true));
    activate(cx, "space");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["png"],
        "Space chooses the focused option"
    );
    activate(cx, "enter");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["png"],
        "choosing the option that is already chosen reports nothing"
    );
}

#[gpui_kit::test]
fn the_button_appearance_keeps_the_groups_contract(cx: &mut TestAppContext) {
    let (view, cx) = open_with(
        cx,
        Some("pdf"),
        GroupDirection::Horizontal,
        RadioAppearance::Button,
    );
    draw(cx);
    cx.update(|window, _| {
        assert_eq!(
            node(window, "导出格式").role(),
            Some(Role::RadioGroup),
            "a segmented row is still the named group it was as circles"
        );
        for (index, label) in ["PNG", "SVG", "PDF", "JPG"].iter().enumerate() {
            let segment = node(window, label);
            assert_eq!(segment.role(), Some(Role::RadioButton), "{label}");
            assert_eq!(
                segment.checked(),
                Some(index == 2),
                "{label} still announces the option the owner holds"
            );
        }
        assert_eq!(
            node(window, "SVG").disabled(),
            Some(true),
            "a disabled segment says so in the tree"
        );
    });

    tab_forward(cx);
    tab_forward(cx);
    assert_eq!(
        state(cx, "PDF").focused(),
        Some(true),
        "Tab still enters the whole row on the chosen segment"
    );
    cx.simulate_keystrokes("right");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["jpg"],
        "the arrows still choose along the row's own axis, over the disabled segment"
    );
    cx.simulate_keystrokes("up");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["jpg"],
        "and a row of segments keeps leaving the column's keys alone"
    );

    let segment = state(cx, "PNG").bounds().center();
    cx.simulate_click(segment, Modifiers::default());
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        ["jpg", "png"],
        "a segment is clicked like any other option"
    );
}

/// One axis: the direction a group runs along and the edge that grows with it.
struct Axis {
    direction: GroupDirection,
    edge: fn(&ElementSnapshot) -> f32,
}

#[gpui_kit::test]
fn a_segmented_row_lays_its_options_end_to_end(cx: &mut TestAppContext) {
    let axes = [
        Axis {
            direction: GroupDirection::Horizontal,
            edge: |node| f32::from(node.bounds().origin.x),
        },
        Axis {
            direction: GroupDirection::Vertical,
            edge: |node| f32::from(node.bounds().origin.y),
        },
    ];
    for axis in axes {
        let (_view, cx) = open_with(cx, None, axis.direction, RadioAppearance::Button);
        draw(cx);
        let edges: Vec<f32> = ["PNG", "SVG", "PDF", "JPG"]
            .iter()
            .map(|label| (axis.edge)(&state(cx, label)))
            .collect();
        assert!(
            edges.windows(2).all(|pair| pair[0] < pair[1]),
            "{:?} keeps its segments in visual order: {edges:?}",
            axis.direction
        );
    }
}
