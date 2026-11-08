//! Headless checks for what Yue adds on top of GPUI Base's checkbox: the group's
//! controlled set write-back, the select-all header's derived three states, the
//! per-option tab stops, and the toggle a radio does not offer.
//!
//! Queries go through `gpui-base`'s observation layer because enabling
//! `gpui-kit/test-support` would also activate the optional GPUI Fast backend, whose
//! pinned dependencies conflict with the GPUI backend this workspace locks.
//!
//! The mark paths, the size rungs, the header's set arithmetic and the colour and
//! motion ramps are pinned as pure functions in `src/checkbox.rs`. What the tree
//! cannot see stays unclaimed here: the halo, the press shrink and the mark's
//! inked-in delay are motion this snapshot does not sample, and nothing here proves
//! what a screen reader speaks on Windows.

use gpui_base::test_support::ElementSnapshot;
use gpui_kit::{
    App, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Render, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestAppContext, VisualTestContext, Window, div, prelude::FluentBuilder as _, rems,
};
use yue_component::{
    Accent, CheckboxState, GroupDirection, ThemeMode, YueCheckbox, YueCheckboxGroup,
    YueCheckboxOption, YueTheme,
};

/// Four options with the third disabled, so a header click always has a value it is
/// not allowed to touch.
const VALUES: [&str; 4] = ["mail", "tasks", "calendar", "notes"];

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}

fn options() -> [YueCheckboxOption; 4] {
    [
        YueCheckboxOption::new(VALUES[0], "邮件"),
        YueCheckboxOption::new(VALUES[1], "任务"),
        YueCheckboxOption::new(VALUES[2], "日历").disabled(true),
        YueCheckboxOption::new(VALUES[3], "备忘"),
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

/// What the fixture puts on screen: whether the group carries a select-all header,
/// whether it is disabled as a whole, whether it has any options at all, and its
/// axis.
#[derive(Default)]
struct Fixture {
    header: bool,
    group_disabled: bool,
    without_options: bool,
    direction: GroupDirection,
}

impl Fixture {
    fn headed() -> Self {
        Self {
            header: true,
            ..Default::default()
        }
    }

    fn locked() -> Self {
        Self {
            header: true,
            group_disabled: true,
            ..Default::default()
        }
    }

    fn without_options() -> Self {
        Self {
            header: true,
            without_options: true,
            ..Default::default()
        }
    }
}

/// The group view the tests drive: a named focusable before and after it, so a tab
/// stop inside the group can be told from one outside it.
struct Probe {
    theme: YueTheme,
    selected: Vec<SharedString>,
    changes: Vec<Vec<String>>,
    fixture: Fixture,
    before: FocusHandle,
    after: FocusHandle,
}

impl Probe {
    fn new(
        selected: &[&str],
        fixture: Fixture,
    ) -> impl FnOnce(&mut Window, &mut Context<Self>) -> Self {
        let selected: Vec<SharedString> = selected.iter().map(|value| (*value).into()).collect();
        move |_, cx| Self {
            theme: theme(),
            selected,
            changes: Vec::new(),
            fixture,
            before: cx.focus_handle(),
            after: cx.focus_handle(),
        }
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected.clone();
        let this = cx.entity();
        let group = YueCheckboxGroup::new("channels", self.theme, selected)
            .label("通知渠道")
            .direction(self.fixture.direction)
            .disabled(self.fixture.group_disabled)
            // The controlled contract: the group reports the whole new set and the
            // owner writes it back before the next draw.
            .on_change(move |values, _, _, cx| {
                let announced: Vec<String> = values.iter().map(|value| value.to_string()).collect();
                this.update(cx, |probe, cx| {
                    probe.changes.push(announced);
                    probe.selected = values;
                    cx.notify();
                });
            })
            .when(self.fixture.header, |group| group.select_all("全选"))
            .when(!self.fixture.without_options, |group| {
                group.options(options())
            });
        div()
            .id("probe")
            .flex()
            .flex_col()
            .gap(rems(1.0))
            .child(neighbour("before", "前一个控件", &self.before))
            .child(group)
            .child(neighbour("after", "后一个控件", &self.after))
    }
}

/// One standalone control, so the activation rule of a half-filled box can be read
/// without a group's set arithmetic in the way.
struct SingleProbe {
    theme: YueTheme,
    state: CheckboxState,
    changes: Vec<String>,
}

impl SingleProbe {
    fn new(state: CheckboxState) -> impl FnOnce(&mut Window, &mut Context<Self>) -> Self {
        move |_, _| Self {
            theme: theme(),
            state,
            changes: Vec::new(),
        }
    }
}

impl Render for SingleProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        div().id("single").child(
            YueCheckbox::new("half", self.theme, self.state)
                .label("半选单项")
                .on_change(move |next, _, _, cx| {
                    let announced = match next {
                        CheckboxState::Unchecked => "unchecked",
                        CheckboxState::Checked => "checked",
                        CheckboxState::Indeterminate => "mixed",
                    };
                    this.update(cx, |probe, cx| {
                        probe.changes.push(announced.to_string());
                        probe.state = next;
                        cx.notify();
                    });
                }),
        )
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
    selected: &[&str],
    fixture: Fixture,
) -> (Entity<Probe>, &'a mut VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.add_window_view(Probe::new(selected, fixture))
}

fn open_single(
    cx: &mut TestAppContext,
    state: CheckboxState,
) -> (Entity<SingleProbe>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.add_window_view(SingleProbe::new(state))
}

/// The sets the group has reported so far, oldest first.
fn reported(view: &Entity<Probe>, cx: &mut VisualTestContext) -> Vec<Vec<String>> {
    view.update(cx, |probe, _| probe.changes.clone())
}

fn held(view: &Entity<Probe>, cx: &mut VisualTestContext) -> Vec<String> {
    view.update(cx, |probe, _| {
        probe
            .selected
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

fn click(cx: &mut VisualTestContext, label: &str) {
    let centre = state(cx, label).bounds().center();
    cx.simulate_click(centre, Modifiers::default());
    draw(cx);
}

#[gpui_kit::test]
fn the_group_publishes_one_named_checkbox_per_option(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, &["mail"], Fixture::headed());
    draw(cx);
    cx.update(|window, _| {
        assert_eq!(
            node(window, "通知渠道").role(),
            Some(Role::Group),
            "the options sit inside a container the tree can name"
        );
        for (index, label) in ["邮件", "任务", "日历", "备忘"].iter().enumerate() {
            let checkbox = node(window, label);
            assert_eq!(checkbox.role(), Some(Role::CheckBox), "{label}");
            assert_eq!(
                checkbox.checked(),
                Some(index == 0),
                "{label} announces the value the owner holds"
            );
            assert_eq!(
                checkbox.indeterminate(),
                Some(false),
                "{label} is a plain option, not a mixed one"
            );
        }
        assert_eq!(
            node(window, "日历").disabled(),
            Some(true),
            "the disabled option says so in the tree"
        );
        assert_ne!(
            node(window, "邮件").disabled(),
            Some(true),
            "and the others are not marked disabled"
        );
    });
}

#[gpui_kit::test]
fn clicking_toggles_one_value_at_a_time(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, &["mail"], Fixture::default());
    draw(cx);
    click(cx, "任务");
    assert_eq!(
        reported(&view, cx),
        [["mail", "tasks"]],
        "one report per change, carrying the whole set rather than the delta"
    );
    click(cx, "邮件");
    assert_eq!(
        reported(&view, cx),
        vec![vec!["mail", "tasks"], vec!["tasks"]],
        "unchecking writes the value out instead of leaving it behind"
    );
    click(cx, "备忘");
    click(cx, "备忘");
    assert_eq!(
        held(&view, cx),
        ["tasks"],
        "the owner's set stays free of duplicates whatever the clicks do"
    );
    cx.update(|window, _| {
        assert_eq!(node(window, "任务").checked(), Some(true));
        assert_eq!(node(window, "邮件").checked(), Some(false));
    });
}

#[gpui_kit::test]
fn a_disabled_option_asks_for_nothing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, &["mail"], Fixture::default());
    draw(cx);
    click(cx, "日历");
    assert!(
        reported(&view, cx).is_empty(),
        "clicking a disabled option reports nothing"
    );
    assert_eq!(
        held(&view, cx),
        ["mail"],
        "and the selection stays where it was"
    );
}

#[gpui_kit::test]
fn a_disabled_group_asks_for_nothing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, &["mail"], Fixture::locked());
    draw(cx);
    for label in ["邮件", "全选"] {
        click(cx, label);
    }
    assert!(
        reported(&view, cx).is_empty(),
        "a group disabled as a whole leaves every option and the header inert"
    );
    cx.update(|window, _| {
        assert_eq!(node(window, "邮件").disabled(), Some(true));
        assert_eq!(
            node(window, "全选").disabled(),
            Some(true),
            "the header is disabled too, rather than quietly rewriting the set"
        );
        assert_eq!(
            node(window, "邮件").checked(),
            Some(true),
            "and the options still announce what the owner holds"
        );
    });
}

#[gpui_kit::test]
fn the_header_aggregates_only_the_options_it_may_change(cx: &mut TestAppContext) {
    // The disabled option is selected, so it is outside the header's reach but still
    // inside the selection.
    let (view, cx) = open(cx, &["calendar"], Fixture::headed());
    draw(cx);
    assert_eq!(
        state(cx, "全选").checked(),
        Some(false),
        "nothing the header may change is selected, so it reads clear"
    );
    assert_eq!(
        state(cx, "全选").indeterminate(),
        Some(false),
        "a selected option it cannot touch does not make it mixed"
    );

    click(cx, "全选");
    assert_eq!(
        reported(&view, cx),
        [["calendar", "mail", "tasks", "notes"]],
        "checking adds every eligible value and leaves the caller's order alone"
    );
    assert_eq!(
        state(cx, "全选").checked(),
        Some(true),
        "the header is full once all eligible options are in"
    );

    click(cx, "邮件");
    assert_eq!(
        state(cx, "全选").indeterminate(),
        Some(true),
        "one eligible option missing is the mixed state, not unchecked"
    );

    click(cx, "全选");
    assert_eq!(
        held(&view, cx),
        ["calendar", "tasks", "notes", "mail"],
        "a mixed header fills rather than clears, like a mixed option"
    );

    click(cx, "全选");
    assert_eq!(
        held(&view, cx),
        ["calendar"],
        "clearing removes only the values the header was allowed to change"
    );
}

#[gpui_kit::test]
fn an_empty_group_offers_the_header_nothing_to_do(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, &[], Fixture::without_options());
    draw(cx);
    assert_eq!(
        state(cx, "全选").disabled(),
        Some(true),
        "with no options to aggregate the header is inert rather than silent"
    );
    click(cx, "全选");
    assert!(
        reported(&view, cx).is_empty(),
        "and clicking it reports nothing"
    );
}

#[gpui_kit::test]
fn every_operable_option_is_its_own_tab_stop(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, &["mail"], Fixture::headed());
    draw(cx);
    tab_forward(cx);
    assert_eq!(state(cx, "前一个控件").focused(), Some(true));
    tab_forward(cx);
    assert_eq!(
        state(cx, "全选").focused(),
        Some(true),
        "the header is a stop of its own, ahead of the options"
    );
    for label in ["邮件", "任务", "备忘"] {
        tab_forward(cx);
        assert_eq!(
            state(cx, label).focused(),
            Some(true),
            "a multi-select group gives each option its own stop instead of roving one"
        );
    }
    assert_eq!(
        state(cx, "日历").focused(),
        None,
        "a disabled option registers no focus binding at all"
    );
    tab_forward(cx);
    assert_eq!(
        state(cx, "后一个控件").focused(),
        Some(true),
        "one more Tab is past the whole group"
    );
    tab_backward(cx);
    assert_eq!(state(cx, "备忘").focused(), Some(true));
}

#[gpui_kit::test]
fn space_and_enter_toggle_the_focused_option(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, &["mail"], Fixture::default());
    draw(cx);
    tab_forward(cx);
    tab_forward(cx);
    assert_eq!(state(cx, "邮件").focused(), Some(true));
    activate(cx, "space");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        vec![Vec::<String>::new()],
        "Space on a chosen option clears it — the key a radio spends on moving"
    );
    activate(cx, "enter");
    draw(cx);
    assert_eq!(
        reported(&view, cx),
        vec![Vec::new(), vec!["mail"]],
        "and Enter fills it again"
    );
}

#[gpui_kit::test]
fn a_checkbox_group_leaves_the_arrow_keys_alone(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, &["mail"], Fixture::default());
    draw(cx);
    tab_forward(cx);
    tab_forward(cx);
    assert_eq!(state(cx, "邮件").focused(), Some(true));
    cx.simulate_keystrokes("down up right left");
    draw(cx);
    assert!(
        reported(&view, cx).is_empty(),
        "the group owns no axis: arrows belong to whatever else is on the screen"
    );
}

#[gpui_kit::test]
fn an_indeterminate_control_asks_to_be_checked(cx: &mut TestAppContext) {
    let (view, cx) = open_single(cx, CheckboxState::Indeterminate);
    draw(cx);
    assert_eq!(
        state(cx, "半选单项").indeterminate(),
        Some(true),
        "a mixed box announces Mixed, not checked"
    );
    let centre = state(cx, "半选单项").bounds().center();
    cx.simulate_click(centre, Modifiers::default());
    draw(cx);
    assert_eq!(
        view.update(cx, |probe, _| probe.changes.clone()),
        ["checked"],
        "activating a mixed box fills it instead of clearing it"
    );
    cx.simulate_click(centre, Modifiers::default());
    draw(cx);
    assert_eq!(
        view.update(cx, |probe, _| probe.changes.clone()),
        ["checked", "unchecked"],
        "and from there it behaves like any other chosen box"
    );
}

/// One axis to check: the direction it runs along and the edge that grows with it.
struct Axis {
    direction: GroupDirection,
    edge: fn(&ElementSnapshot) -> f32,
    /// Whether the options also have to share one line.
    shares_a_line: bool,
}

#[gpui_kit::test]
fn a_group_lays_its_options_along_its_own_axis(cx: &mut TestAppContext) {
    let axes = [
        Axis {
            direction: GroupDirection::Vertical,
            edge: |node| f32::from(node.bounds().origin.y),
            shares_a_line: false,
        },
        Axis {
            direction: GroupDirection::Horizontal,
            edge: |node| f32::from(node.bounds().origin.x),
            shares_a_line: true,
        },
    ];
    for axis in axes {
        let (_view, cx) = open(
            cx,
            &[],
            Fixture {
                direction: axis.direction,
                ..Default::default()
            },
        );
        draw(cx);
        let nodes: Vec<ElementSnapshot> = ["邮件", "任务", "日历", "备忘"]
            .iter()
            .map(|label| state(cx, label))
            .collect();
        let edges: Vec<f32> = nodes.iter().map(axis.edge).collect();
        assert!(
            edges.windows(2).all(|pair| pair[0] < pair[1]),
            "{:?} keeps its options in visual order: {edges:?}",
            axis.direction
        );
        if axis.shares_a_line {
            let rows: Vec<f32> = nodes
                .iter()
                .map(|node| f32::from(node.bounds().origin.y))
                .collect();
            assert!(
                rows.windows(2).all(|pair| pair[0] == pair[1]),
                "a horizontal group puts them on one line instead of stacking them: {rows:?}"
            );
        }
    }
}
