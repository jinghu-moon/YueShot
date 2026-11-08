//! Headless checks for what a Yue menu puts on the tree and what its keyboard does:
//! the panel appears only while it is open and carries the name it was given, the
//! rows arrive as menu items with their own names, one row at a time holds the
//! highlight, and each key does the one thing the reference asks of it.
//!
//! The placement arithmetic, the caret geometry, the dwell and the motion ramps are
//! pinned as pure functions in `src/dropdown.rs`. Hover tests also sample real
//! background quads across scheduled frames. What the observation layer cannot
//! see stays unclaimed here: nothing samples the entrance opacity or the caret's pixels,
//! and gpui has no `aria_disabled` and no accessibility wrapper Yue can reach, so a
//! disabled row is proven by what the menu refuses to do with it rather than by a
//! flag on it.

use gpui_base::test_support::ElementSnapshot;
use gpui_kit::{
    App, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers,
    MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestAppContext, VisualTestContext, Window, div,
    prelude::FluentBuilder as _, rems,
};
use yue_component::{
    Accent, DropdownIndicatorSide, ThemeMode, YueButton, YueContextMenu, YueDropdownMenu,
    YueMenuItem, YueTheme,
};

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}

/// The shape of menu a test puts on screen. The plain shape is three commands whose
/// third is disabled, so a keyboard walk always has a row it must skip.
#[derive(Default)]
struct Fixture {
    typed: bool,
    marks: bool,
    shortcut: bool,
    trailing: bool,
    submenu: bool,
    filler: bool,
    long: usize,
    grid: usize,
    context: bool,
    custom: bool,
}

impl Fixture {
    /// Rows whose labels all start with a Latin letter, so a keystroke is one
    /// character in the platform's own sense.
    fn typed() -> Self {
        Self {
            typed: true,
            ..Default::default()
        }
    }

    fn marked() -> Self {
        Self {
            marks: true,
            ..Default::default()
        }
    }

    /// The commands, each with a shortcut, so the row's trailing edge has a pixel
    /// to be measured against.
    fn shortcut() -> Self {
        Self {
            shortcut: true,
            ..Default::default()
        }
    }

    /// Marks and shortcuts with the column forced to the trailing end — the
    /// reference's `side:'r'`, where the tick still comes before the shortcut.
    fn trailing_marks() -> Self {
        Self {
            marks: true,
            shortcut: true,
            trailing: true,
            ..Default::default()
        }
    }

    fn nested() -> Self {
        Self {
            submenu: true,
            ..Default::default()
        }
    }

    /// A heading, a hairline and a loading row between the commands.
    fn filler() -> Self {
        Self {
            filler: true,
            ..Default::default()
        }
    }

    fn gridded(columns: usize) -> Self {
        Self {
            grid: columns,
            ..Default::default()
        }
    }

    /// A label made of `phrases` copies of a ten-character run, so the panel is
    /// asked for a width it cannot keep inside its own floor.
    fn long(phrases: usize) -> Self {
        Self {
            long: phrases,
            ..Default::default()
        }
    }

    fn right_click() -> Self {
        Self {
            context: true,
            ..Default::default()
        }
    }

    /// The menu on a trigger the host did not build, so that the wrapper's own
    /// click is the thing under test.
    fn custom() -> Self {
        Self {
            custom: true,
            ..Default::default()
        }
    }
}

/// The menu under test, with the marks it shows held here rather than inside the
/// component: a Yue menu reports what the owner already knows.
struct Probe {
    theme: YueTheme,
    fixture: Fixture,
    runs: Vec<String>,
    views: Vec<SharedString>,
    pace: SharedString,
    region: FocusHandle,
}

impl Probe {
    fn new(fixture: Fixture) -> impl FnOnce(&mut Window, &mut Context<Self>) -> Self {
        move |_, cx| Self {
            theme: theme(),
            fixture,
            runs: Vec::new(),
            views: vec![SharedString::from("已完成")],
            pace: SharedString::from("即时"),
            region: cx.focus_handle(),
        }
    }

    /// The shortcut a `shortcut()` menu hangs on each of its rows.
    fn hint(&self, item: YueMenuItem, label: &str) -> YueMenuItem {
        match self.fixture.shortcut {
            true => item.hotkey(format!("Ctrl+{label}")),
            false => item,
        }
    }

    /// A command that writes its own name into the log when it runs.
    fn run(&self, this: &Entity<Probe>, label: &'static str) -> YueMenuItem {
        let this = this.clone();
        self.hint(YueMenuItem::new(label), label)
            .on_click(move |_, _, cx| {
                this.update(cx, |probe, cx| {
                    probe.runs.push(label.to_string());
                    cx.notify();
                });
            })
    }

    /// A tick the probe owns: activating it flips the value and leaves the panel up.
    fn check(&self, this: &Entity<Probe>, label: &'static str) -> YueMenuItem {
        let this = this.clone();
        self.hint(YueMenuItem::new(label), label)
            .checked(self.views.iter().any(|held| held == label))
            .keep_open(true)
            .on_click(move |_, _, cx| {
                this.update(cx, |probe, cx| {
                    let value = SharedString::from(label);
                    match probe.views.iter().position(|held| *held == value) {
                        Some(position) => {
                            probe.views.remove(position);
                        }
                        None => probe.views.push(value),
                    }
                    cx.notify();
                });
            })
    }

    /// A dot the probe owns: activating it makes it the whole value and stays open.
    fn radio(&self, this: &Entity<Probe>, label: &'static str) -> YueMenuItem {
        let this = this.clone();
        self.hint(YueMenuItem::new(label), label)
            .radio(self.pace.as_ref() == label)
            .keep_open(true)
            .on_click(move |_, _, cx| {
                this.update(cx, |probe, cx| {
                    probe.pace = SharedString::from(label);
                    cx.notify();
                });
            })
    }

    fn items(&self, this: &Entity<Probe>) -> Vec<YueMenuItem> {
        let mut items = vec![
            self.run(this, "重命名"),
            self.run(this, "复制"),
            self.run(this, "导出为 PDF").disabled(true),
        ];
        if self.fixture.typed {
            items = vec![
                self.run(this, "Archive 归档"),
                self.run(this, "Copy 复制"),
                self.run(this, "Copy path 复制路径"),
                self.run(this, "Export 导出"),
            ];
        }
        if self.fixture.filler {
            items = vec![
                YueMenuItem::heading("分组"),
                self.run(this, "移动"),
                YueMenuItem::separator(),
                YueMenuItem::loading("正在同步"),
                self.run(this, "删除"),
            ];
        }
        if self.fixture.submenu {
            items.push(YueMenuItem::submenu(
                "导出为",
                [self.run(this, "PDF"), self.run(this, "Word")],
            ));
        }
        if self.fixture.marks {
            items.push(self.check(this, "已完成"));
            items.push(self.check(this, "归档"));
            items.push(self.radio(this, "即时"));
            items.push(self.radio(this, "每天"));
        }
        if self.fixture.grid > 0 {
            items = ["甲", "乙", "丙", "丁", "戊", "己"]
                .into_iter()
                .map(|label| self.run(this, label))
                .collect();
        }
        if self.fixture.long > 0 {
            items = vec![
                YueMenuItem::new(SharedString::from("完整路径".repeat(self.fixture.long)))
                    .on_click({
                        let this = this.clone();
                        move |_, _, cx| {
                            this.update(cx, |probe, cx| {
                                probe.runs.push("长".to_string());
                                cx.notify();
                            });
                        }
                    }),
            ];
        }
        items
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let items = self.items(&this);
        let menu = if self.fixture.context {
            YueContextMenu::new("probe-menu", self.theme)
                .label("文件")
                .items(items)
                .child(
                    gpui_kit::base::TestSupportExt::test_support(div().id("probe-region"))
                        .track_focus(&self.region.clone().tab_stop(true))
                        .aria_label("右键区域")
                        .w(rems(20.0))
                        .h(rems(6.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("在这个区域上点右键"),
                )
                .into_any_element()
        } else {
            let columns = self.fixture.grid;
            let menu = if self.fixture.custom {
                let region = self.region.clone();
                YueDropdownMenu::with_trigger("probe-menu", self.theme, move |_, _, _| {
                    gpui_kit::base::TestSupportExt::test_support(div().id("probe-custom-trigger"))
                        .track_focus(&region.tab_stop(true))
                        .aria_label("打开菜单")
                        .child("打开菜单")
                        .into_any_element()
                })
            } else {
                YueDropdownMenu::new(
                    "probe-menu",
                    self.theme,
                    YueButton::text("probe-trigger", self.theme, "打开菜单"),
                )
            };
            menu.label("选择操作")
                .items(items)
                .when(columns > 0, |menu| menu.columns(columns))
                .when(self.fixture.trailing, |menu| {
                    menu.indicator_side(DropdownIndicatorSide::Trailing)
                })
                .into_any_element()
        };
        div()
            .id("probe")
            .flex()
            .flex_col()
            .gap(rems(2.0))
            // Somewhere the pointer can land that is neither the trigger nor a row,
            // for the dismissal an outside press has to answer for. It sits above the
            // menu so a panel hanging below the trigger cannot cover it.
            .child(
                gpui_kit::base::TestSupportExt::test_support(div().id("probe-blank"))
                    .aria_label("空白区域")
                    .w(rems(20.0))
                    .h(rems(3.0)),
            )
            .child(menu)
    }
}

fn render(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn nodes(window: &Window, label: &str) -> Vec<ElementSnapshot> {
    gpui_base::test_support::snapshots(window)
        .into_iter()
        .filter(|node| node.label() == Some(label))
        .collect()
}

fn node(window: &Window, label: &str) -> ElementSnapshot {
    let matches = nodes(window, label);
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one node labelled {label}, found {}",
        matches.len()
    );
    matches.into_iter().next().expect("one match")
}

/// The names of the panels in the tree, sorted — `snapshots` walks a hash map, so
/// the order it returns is not the order the panels are painted.
fn panels(cx: &mut VisualTestContext) -> Vec<String> {
    let mut names: Vec<String> = cx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|node| node.role() == Some(Role::Menu))
            .map(|node| node.label().unwrap_or_default().to_string())
            .collect()
    });
    names.sort();
    names
}

/// The rows the tree says are the highlighted one, sorted for the same reason. A Yue
/// panel holds one focus and marks its own active descendant, so exactly one row
/// should answer — except in a marked menu, where a radio row's `selected` reports
/// its value instead of the highlight.
fn highlighted(cx: &mut VisualTestContext) -> Vec<String> {
    let mut labels: Vec<String> = cx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|node| {
                matches!(
                    node.role(),
                    Some(Role::MenuItem | Role::MenuItemCheckBox | Role::MenuItemRadio)
                ) && node.selected() == Some(true)
            })
            .map(|node| node.label().unwrap_or_default().to_string())
            .collect()
    });
    labels.sort();
    labels
}

fn row(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|window, _| node(window, label))
}

/// The one node with this name and this role. A submenu row and the panel it opens
/// carry the same name, so a test that has both on screen asks for one by role.
fn only(cx: &mut VisualTestContext, label: &str, role: Role) -> ElementSnapshot {
    cx.update(|window, _| {
        let matches: Vec<ElementSnapshot> = gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|node| node.label() == Some(label) && node.role() == Some(role))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one {role:?} labelled {label}, found {}",
            matches.len()
        );
        matches.into_iter().next().expect("one match")
    })
}

fn nodes_of_role(cx: &mut VisualTestContext, role: Role) -> Vec<ElementSnapshot> {
    cx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|node| node.role() == Some(role))
            .collect()
    })
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(render);
}

/// Sample the actual row background submitted to GPUI's scene, rather than
/// treating `aria_selected` as proof that a color animation was painted.
fn painted_row_lightness(cx: &mut VisualTestContext, label: &str) -> f32 {
    cx.update(|window, _| {
        let bounds = node(window, label).bounds();
        let scale = window.scale_factor();
        window
            .painted_quads()
            .into_iter()
            .find(|quad| quad.bounds == bounds.scale(scale))
            .and_then(|quad| quad.background.as_solid())
            .expect("row background quad")
            .l
    })
}

fn tick(cx: &mut VisualTestContext, millis: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(millis));
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

#[gpui_kit::test]
fn pointer_highlight_balances_acceleration_and_deceleration_over_300ms(cx: &mut TestAppContext) {
    let (_, cx) = open(cx, Fixture::default());
    draw(cx);
    open_by_click(cx);
    tick(cx, 600); // Isolate hover from the panel and row entrance animations.
    let unlit = painted_row_lightness(cx, "复制");
    let center = row(cx, "复制").bounds().center();
    cx.simulate_mouse_move(center, None, Modifiers::default());
    draw(cx);
    let first = painted_row_lightness(cx, "复制");
    assert!(
        (first - unlit).abs() < 0.0001,
        "hover must start at the painted color"
    );
    tick(cx, 30);
    let early = painted_row_lightness(cx, "复制");
    assert!(early < unlit, "the scheduled frame must advance the tint");
    tick(cx, 45);
    let quarter = painted_row_lightness(cx, "复制");
    tick(cx, 75);
    let middle = painted_row_lightness(cx, "复制");
    tick(cx, 75);
    let three_quarters = painted_row_lightness(cx, "复制");
    tick(cx, 75);
    let lit = painted_row_lightness(cx, "复制");
    assert!(middle < unlit, "the scheduled frame must advance the tint");
    assert!(
        middle > lit + 0.0001,
        "150ms must still be an intermediate color: {unlit}, {middle}, {lit}"
    );
    assert!(lit < unlit);
    let progress = |color: f32| (unlit - color) / (unlit - lit);
    assert!(
        (progress(early) - 0.0284).abs() < 0.002,
        "30ms must complete about 2.8% of the color change"
    );
    assert!(
        (progress(quarter) - 0.157).abs() < 0.002,
        "75ms must complete about 15.7% of the color change"
    );
    assert!(
        (progress(middle) - 0.5).abs() < 0.002,
        "150ms must complete 50% of the color change"
    );
    assert!(
        (progress(three_quarters) - 0.843).abs() < 0.002,
        "225ms must complete about 84.3% of the color change"
    );
    tick(cx, 100);
    assert_eq!(
        painted_row_lightness(cx, "复制"),
        lit,
        "300ms must reach the final color"
    );
}

#[gpui_kit::test]
fn interrupted_pointer_highlight_reverses_from_the_painted_color(cx: &mut TestAppContext) {
    let (_, cx) = open(cx, Fixture::default());
    draw(cx);
    open_by_click(cx);
    tick(cx, 600);
    let center = row(cx, "复制").bounds().center();
    cx.simulate_mouse_move(center, None, Modifiers::default());
    draw(cx);
    tick(cx, 60);
    let before = painted_row_lightness(cx, "复制");
    let center = row(cx, "重命名").bounds().center();
    cx.simulate_mouse_move(center, None, Modifiers::default());
    draw(cx);
    let reversed = painted_row_lightness(cx, "复制");
    assert!(
        (before - reversed).abs() < 0.0001,
        "retargeting must not jump"
    );
    tick(cx, 30);
    assert!(
        painted_row_lightness(cx, "复制") > reversed,
        "old row must fade out"
    );
    tick(cx, 250);
    assert!((painted_row_lightness(cx, "复制") - 1.0).abs() < 0.0001);
    let disabled = painted_row_lightness(cx, "导出为 PDF");
    let center = row(cx, "导出为 PDF").bounds().center();
    cx.simulate_mouse_move(center, None, Modifiers::default());
    draw(cx);
    tick(cx, 125);
    assert_eq!(painted_row_lightness(cx, "导出为 PDF"), disabled);
}

#[gpui_kit::test]
fn keyboard_highlight_animates_and_reduced_motion_snaps(cx: &mut TestAppContext) {
    let (_, cx) = open(cx, Fixture::default());
    draw(cx);
    open_by_click(cx);
    tick(cx, 600);
    let unlit = painted_row_lightness(cx, "复制");
    press(cx, "down");
    assert_eq!(painted_row_lightness(cx, "复制"), unlit);
    tick(cx, 150);
    let middle = painted_row_lightness(cx, "复制");
    tick(cx, 150);
    let lit = painted_row_lightness(cx, "复制");
    assert!(lit < middle && middle < unlit);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    press(cx, "up");
    assert_eq!(painted_row_lightness(cx, "复制"), unlit);
    assert!((painted_row_lightness(cx, "重命名") - lit).abs() < 0.0001);
}

fn open(cx: &mut TestAppContext, fixture: Fixture) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.add_window_view(Probe::new(fixture))
}

/// Click the trigger, then paint: the panel is placed from the anchor the previous
/// frame measured, so the surface only exists in the frame after the click.
fn open_by_click(cx: &mut VisualTestContext) {
    click(cx, "打开菜单");
}

fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).expect("fixture key");
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
}

fn click(cx: &mut VisualTestContext, label: &str) {
    let centre = row(cx, label).bounds().center();
    cx.simulate_click(centre, Modifiers::default());
    draw(cx);
}

/// The platform gives a right click as a press and a release of its own button;
/// `simulate_click` only ever sends the left one.
fn right_click(cx: &mut VisualTestContext, label: &str) {
    let position = row(cx, label).bounds().center();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Right,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    draw(cx);
}

fn runs(view: &Entity<Probe>, cx: &mut VisualTestContext) -> Vec<String> {
    view.update(cx, |probe, _| probe.runs.clone())
}

#[gpui_kit::test]
fn the_panel_appears_when_the_trigger_is_clicked(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Fixture::default());
    draw(cx);
    // A menu that has never opened is on no tree: the first frame of a shut panel
    // is not spent fading an invisible surface in over the trigger it belongs to.
    assert!(
        panels(cx).is_empty(),
        "a closed panel holds neither the keyboard nor a row"
    );
    assert_eq!(row(cx, "打开菜单").expanded(), Some(false));
    open_by_click(cx);
    assert_eq!(
        panels(cx),
        vec!["选择操作".to_string()],
        "and the panel it opens carries the name the host was given"
    );
    assert_eq!(row(cx, "打开菜单").expanded(), Some(true));
    open_by_click(cx);
    assert_eq!(
        row(cx, "打开菜单").expanded(),
        Some(false),
        "a second click on the trigger is what closes it again"
    );
    assert!(
        panels(cx).is_empty(),
        "and a closed menu hands the tree back: its surface is unmounted, not hidden"
    );
    open_by_click(cx);
    click(cx, "重命名");
    assert_eq!(
        runs(&view, cx),
        vec!["重命名".to_string()],
        "its rows are live while it is up"
    );
    assert!(
        panels(cx).is_empty(),
        "and a command closes the menu it came from"
    );
}

#[gpui_kit::test]
fn a_press_anywhere_else_folds_the_panel_away(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    click(cx, "空白区域");
    assert!(
        panels(cx).is_empty(),
        "the panel is a response to the trigger, so a press outside it ends it"
    );
    assert_eq!(
        nodes_of_role(cx, Role::MenuItem).len(),
        0,
        "and its rows go with it"
    );
}

#[gpui_kit::test]
fn rows_arrive_as_named_menu_items(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    for label in ["重命名", "复制", "导出为 PDF"] {
        assert_eq!(
            row(cx, label).role(),
            Some(Role::MenuItem),
            "{label} is a row a screen reader is told is a menu item"
        );
    }
    assert_eq!(
        row(cx, "重命名").checked(),
        None,
        "a plain command carries no tick state at all"
    );
    assert_eq!(
        row(cx, "重命名").expanded(),
        None,
        "and nothing to expand either"
    );
}

#[gpui_kit::test]
fn the_panel_takes_the_keyboard_and_hands_it_back(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::default());
    // Tab reaches the trigger the way the rest of the page would.
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    assert_eq!(
        row(cx, "打开菜单").focused(),
        Some(true),
        "the menu is one tab stop, and it is the trigger"
    );
    press(cx, "enter");
    assert_eq!(
        panels(cx),
        vec!["选择操作".to_string()],
        "Enter opens the same panel a click does, with no pointer involved"
    );
    assert_eq!(
        row(cx, "选择操作").focused(),
        Some(true),
        "opening moves the focus into the panel, so the very next arrow key works"
    );
    assert_eq!(
        row(cx, "打开菜单").focused(),
        Some(false),
        "and the trigger gives the keyboard up while the panel holds it"
    );
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["复制".to_string()],
        "a key straight after the open is read by the panel, not by the window"
    );
    press(cx, "escape");
    assert!(panels(cx).is_empty(), "Escape takes the panel away");
    assert_eq!(
        row(cx, "打开菜单").focused(),
        Some(true),
        "and gives the keyboard back to what opened it"
    );
}

#[gpui_kit::test]
fn a_panel_opened_by_the_pointer_hands_the_keyboard_back_too(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    assert_eq!(
        row(cx, "选择操作").focused(),
        Some(true),
        "a press on the trigger gives the keyboard to the panel it opened"
    );
    press(cx, "escape");
    assert!(panels(cx).is_empty(), "Escape takes the panel away");
    assert_eq!(
        row(cx, "打开菜单").focused(),
        Some(true),
        "and gives the keyboard back to what opened it"
    );
    open_by_click(cx);
    click(cx, "空白区域");
    assert_eq!(
        row(cx, "打开菜单").focused(),
        Some(true),
        "a dismissal by the pointer hands it back the same way"
    );
}

#[gpui_kit::test]
fn a_custom_trigger_answers_the_pointer(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Fixture::custom());
    open_by_click(cx);
    assert_eq!(
        panels(cx),
        vec!["选择操作".to_string()],
        "a trigger the host did not build still opens the menu, from its wrapper"
    );
    click(cx, "重命名");
    assert_eq!(
        runs(&view, cx),
        vec!["重命名".to_string()],
        "and the panel it opened is a panel like any other"
    );
}

#[gpui_kit::test]
fn arrows_stop_only_at_rows_the_menu_can_run(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    assert_eq!(
        highlighted(cx),
        vec!["重命名".to_string()],
        "opening marks the first available row rather than nothing"
    );
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["复制".to_string()],
        "the disabled row in between is transparent to the keyboard"
    );
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["重命名".to_string()],
        "one column wraps at the end, as the reference's single list does"
    );
    press(cx, "up");
    assert_eq!(highlighted(cx), vec!["复制".to_string()]);
    press(cx, "home");
    assert_eq!(highlighted(cx), vec!["重命名".to_string()]);
    press(cx, "end");
    assert_eq!(
        highlighted(cx),
        vec!["复制".to_string()],
        "End reaches the last row the keyboard stops at, not the disabled one after it"
    );
}

#[gpui_kit::test]
fn a_caption_a_hairline_and_a_loading_row_are_outside_the_walk(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::filler());
    open_by_click(cx);
    assert_eq!(
        highlighted(cx),
        vec!["移动".to_string()],
        "the panel opens on the first row a command can run"
    );
    assert_eq!(
        nodes_of_role(cx, Role::Caption).len(),
        2,
        "the group caption and the loading line both read as captions"
    );
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["删除".to_string()],
        "one step crosses the hairline and the loading line to the next command"
    );
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["移动".to_string()],
        "and wrapping comes back to the commands, not to the furniture"
    );
}

#[gpui_kit::test]
fn enter_runs_the_highlighted_row_and_closes_the_panel(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        runs(&view, cx),
        vec!["复制".to_string()],
        "Enter runs the row the keyboard is on, not the one the pointer last saw"
    );
    assert!(
        panels(cx).is_empty(),
        "and a command closes the menu it came from"
    );
}

#[gpui_kit::test]
fn a_disabled_row_answers_neither_pointer_nor_keyboard(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    click(cx, "导出为 PDF");
    assert!(
        runs(&view, cx).is_empty(),
        "clicking a disabled command runs nothing"
    );
    assert_eq!(
        highlighted(cx),
        vec!["重命名".to_string()],
        "and the highlight does not move to it either"
    );
    assert!(
        !panels(cx).is_empty(),
        "the panel is still up: a disabled row does not dismiss the menu it is in"
    );
    press(cx, "end");
    assert_eq!(
        highlighted(cx),
        vec!["复制".to_string()],
        "End stops on the last row the keyboard may use, not on the disabled one after it"
    );
    press(cx, "enter");
    assert_eq!(
        runs(&view, cx),
        vec!["复制".to_string()],
        "so the end of the list can only ever run the row the keyboard was allowed to reach"
    );
}

#[gpui_kit::test]
fn a_letter_marks_the_row_that_starts_with_it(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::typed());
    open_by_click(cx);
    press(cx, "a");
    assert_eq!(
        highlighted(cx),
        vec!["Archive 归档".to_string()],
        "a typed letter marks the first row whose text begins with it"
    );
    press(cx, "c");
    assert_eq!(
        highlighted(cx),
        vec!["Copy 复制".to_string()],
        "the walk starts one past the mark, so the next letter moves to its own row"
    );
    press(cx, "c");
    assert_eq!(
        highlighted(cx),
        vec!["Copy path 复制路径".to_string()],
        "and a second press of the same letter goes on to the next row that starts with it"
    );
    press(cx, "z");
    assert_eq!(
        highlighted(cx),
        vec!["Copy path 复制路径".to_string()],
        "a letter that matches nothing leaves the highlight where it was"
    );
}

#[gpui_kit::test]
fn marks_read_the_owner_and_keep_the_panel_up(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Fixture::marked());
    open_by_click(cx);
    assert_eq!(row(cx, "已完成").role(), Some(Role::MenuItemCheckBox));
    assert_eq!(
        row(cx, "已完成").checked(),
        Some(true),
        "the tick announces the value the owner holds"
    );
    assert_eq!(row(cx, "归档").checked(), Some(false));
    assert_eq!(row(cx, "即时").role(), Some(Role::MenuItemRadio));
    assert_eq!(
        row(cx, "即时").selected(),
        Some(true),
        "a dot row reports its own value, not the highlight"
    );
    assert_eq!(row(cx, "每天").selected(), Some(false));
    click(cx, "归档");
    assert_eq!(
        row(cx, "归档").checked(),
        Some(true),
        "the tick follows what the owner wrote back"
    );
    assert_eq!(
        panels(cx),
        vec!["选择操作".to_string()],
        "a toggle inside a menu stays open to show its own change"
    );
    click(cx, "每天");
    assert_eq!(
        row(cx, "每天").selected(),
        Some(true),
        "and the dot moves to the row just chosen"
    );
    assert_eq!(
        row(cx, "即时").selected(),
        Some(false),
        "leaving the one it replaced"
    );
    assert_eq!(
        view.update(cx, |probe, _| probe.pace.to_string()),
        "每天".to_string(),
        "the menu asked, the owner answered"
    );
}

#[gpui_kit::test]
fn the_right_arrow_opens_one_submenu_and_escape_closes_it_first(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Fixture::nested());
    open_by_click(cx);
    press(cx, "down");
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["导出为".to_string()],
        "the submenu row is a stop like any command"
    );
    assert_eq!(
        only(cx, "导出为", Role::MenuItem).expanded(),
        Some(false),
        "and it says it has nothing open under it yet"
    );
    press(cx, "right");
    assert_eq!(
        panels(cx),
        vec!["导出为".to_string(), "选择操作".to_string()],
        "the panel it opens takes its name from the row"
    );
    assert_eq!(only(cx, "导出为", Role::MenuItem).expanded(), Some(true));
    assert_eq!(
        row(cx, "PDF").role(),
        Some(Role::MenuItem),
        "a submenu holds rows of the same kind"
    );
    assert_eq!(
        highlighted(cx),
        vec!["PDF".to_string()],
        "opening a submenu marks its first row, and the one mark is inside it"
    );
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["Word".to_string()],
        "the keys inside a submenu move inside it"
    );
    press(cx, "left");
    assert_eq!(
        panels(cx),
        vec!["选择操作".to_string()],
        "the left arrow folds the submenu back into its row"
    );
    assert_eq!(
        highlighted(cx),
        vec!["导出为".to_string()],
        "and puts the mark where the user came from"
    );
    press(cx, "right");
    press(cx, "escape");
    assert_eq!(
        panels(cx),
        vec!["选择操作".to_string()],
        "the first Escape only closes the submenu"
    );
    press(cx, "enter");
    assert!(
        runs(&view, cx).is_empty(),
        "the row that holds a submenu runs no command of its own"
    );
    assert_eq!(
        panels(cx),
        vec!["导出为".to_string(), "选择操作".to_string()],
        "Enter on it is the other way to open what it holds"
    );
    press(cx, "escape");
    assert_eq!(
        panels(cx),
        vec!["选择操作".to_string()],
        "and Escape folds the submenu first, every time it is open"
    );
    press(cx, "escape");
    assert!(
        panels(cx).is_empty(),
        "only the next Escape closes the menu itself"
    );
}

#[gpui_kit::test]
fn a_grid_steps_down_a_column_and_across_a_row(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::gridded(3));
    open_by_click(cx);
    assert_eq!(highlighted(cx), vec!["甲".to_string()]);
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        vec!["丁".to_string()],
        "down steps a whole row of three, the way the reference's grid does"
    );
    press(cx, "right");
    assert_eq!(highlighted(cx), vec!["戊".to_string()]);
    press(cx, "right");
    assert_eq!(highlighted(cx), vec!["己".to_string()]);
    press(cx, "right");
    assert_eq!(
        highlighted(cx),
        vec!["己".to_string()],
        "a grid clamps at its edge instead of wrapping to the next row"
    );
    press(cx, "up");
    assert_eq!(
        highlighted(cx),
        vec!["丙".to_string()],
        "and up comes back over the same row it came from"
    );
}

#[gpui_kit::test]
fn a_right_click_puts_the_panel_on_the_pointer(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Fixture::right_click());
    draw(cx);
    assert!(
        panels(cx).is_empty(),
        "a region's panel is on no tree before the right click that opens it: \
         there is nothing to focus and no row to answer the pointer"
    );
    right_click(cx, "右键区域");
    assert_eq!(
        panels(cx),
        vec!["文件".to_string()],
        "the panel a right click opens is named by the caller, since it has no trigger to borrow from"
    );
    assert_eq!(
        row(cx, "文件").focused(),
        Some(true),
        "and the right click moves the keyboard into it, as a trigger click does"
    );
    click(cx, "重命名");
    assert_eq!(
        runs(&view, cx),
        vec!["重命名".to_string()],
        "and its rows run like any other command"
    );
    assert!(panels(cx).is_empty(), "activating a command closes it");
    right_click(cx, "右键区域");
    assert_eq!(
        panels(cx),
        vec!["文件".to_string()],
        "the same region opens it again on the next right click"
    );
}

#[gpui_kit::test]
fn the_panel_holds_its_own_highlight_across_a_repaint(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    press(cx, "down");
    // A frame the owner asks for while the panel stays up must not re-stagger the
    // rows or drop the mark: this is the repaint a hover or a notify produces.
    draw(cx);
    draw(cx);
    assert_eq!(
        highlighted(cx),
        vec!["复制".to_string()],
        "the mark survives a frame that changed nothing"
    );
    assert_eq!(
        nodes_of_role(cx, Role::MenuItem).len(),
        3,
        "and the rows are still the same three rows"
    );
}

#[gpui_kit::test]
fn the_panel_box_holds_every_row_inside_it(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::default());
    open_by_click(cx);
    let panel = only(cx, "选择操作", Role::Menu).bounds();
    assert_eq!(
        f32::from(panel.size.width),
        192.,
        "a menu of short rows keeps the design's minimum panel width"
    );
    for node in nodes_of_role(cx, Role::MenuItem) {
        let row = node.bounds();
        assert!(
            row.origin.x >= panel.origin.x
                && row.origin.x + row.size.width <= panel.origin.x + panel.size.width,
            "{:?} is painted outside the panel that holds it",
            node.label().unwrap_or_default()
        );
    }
}

#[gpui_kit::test]
fn a_row_too_long_for_the_floor_grows_the_panel_to_its_ceiling(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::long(20));
    open_by_click(cx);
    let panel = only(cx, "选择操作", Role::Menu).bounds();
    let width = f32::from(panel.size.width);
    assert!(
        width > 192. && width <= 320.,
        "the panel grows with its rows and stops at the reference's \
         min(320px, 90vw) ceiling, got {width}"
    );
    for node in nodes_of_role(cx, Role::MenuItem) {
        let row = node.bounds();
        assert!(
            row.origin.x >= panel.origin.x
                && row.origin.x + row.size.width <= panel.origin.x + panel.size.width,
            "and the row that forced the growth is inside it too, not over the page: {:?}",
            node.label().unwrap_or_default()
        );
    }
}

/// How far the last thing inside a row stops short of that row's right edge.
/// The rows pad themselves by `--space-8`, so a wider gap is a column the panel
/// paid for: the 16px mark slot plus the 8px between it and the hint.
fn hint_gap(label: &str, cx: &mut VisualTestContext) -> f32 {
    cx.update(|window, _| {
        let row = node(window, label);
        let hints: Vec<f32> = gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|node| {
                node.path().starts_with(row.path()) && node.path().len() > row.path().len()
            })
            .map(|node| {
                let bounds = node.bounds();
                f32::from(bounds.origin.x + bounds.size.width)
            })
            .collect();
        assert_eq!(
            hints.len(),
            1,
            "expected one observed node inside {label}, found {}",
            hints.len()
        );
        f32::from(row.bounds().origin.x + row.bounds().size.width) - hints[0]
    })
}

#[gpui_kit::test]
fn a_shortcut_ends_where_its_row_does(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::shortcut());
    open_by_click(cx);
    assert_eq!(
        hint_gap("复制", cx),
        8.,
        "a menu with nothing to mark reserves no trailing column, so the hint reaches \
         the row's own padding instead of stopping 24px short of it"
    );
}

#[gpui_kit::test]
fn a_forced_mark_column_ends_before_the_shortcut(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, Fixture::trailing_marks());
    open_by_click(cx);
    assert_eq!(
        hint_gap("已完成", cx),
        8.,
        "side:'r' moves the mark column, it does not push the shortcut back of it"
    );
}
