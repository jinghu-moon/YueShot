//! Real input and submitted geometry; these are not Windows pixel/reader tests.
use gpui_base::test_support::ElementSnapshot;
use gpui_kit::InteractiveElement as _;
use gpui_kit::{
    Context, Entity, IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    ParentElement as _, Point, Render, Styled as _, TestAppContext, VisualTestContext, Window, div,
    point, prelude::FluentBuilder as _, px,
};
use yue_component::{Accent, ThemeMode, YueButton, YueSwitch, YueTheme};
gpui_kit::actions!(control_test, [SaveCommand]);

#[derive(Default)]
struct Probe {
    disabled: bool,
    loading: bool,
    checked: bool,
    clicks: usize,
    changes: Vec<bool>,
    inherited: bool,
    width: Option<gpui_kit::Pixels>,
    focus: Option<gpui_kit::FocusHandle>,
    switch_focus: Option<gpui_kit::FocusHandle>,
    skip_button: bool,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
        let owner = cx.entity();
        div()
            .flex()
            .gap(px(24.))
            .p(px(20.))
            .on_action(cx.listener(|this, _: &SaveCommand, _, cx| {
                this.clicks += 1;
                cx.notify();
            }))
            .children([
                (if self.inherited {
                    YueButton::new("command", "执行")
                } else {
                    YueButton::text("command", theme, "执行")
                })
                .disabled(self.disabled)
                .loading(self.loading)
                .key_context("YueCommand")
                .accessibility_description("保存当前文件")
                .map(|button| match self.width {
                    Some(width) => button.width(width),
                    None => button,
                })
                .map(|button| match &self.focus {
                    Some(focus) => button.track_focus(focus),
                    None => button,
                })
                .tab_stop(!self.skip_button)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.clicks += 1;
                    cx.notify();
                }))
                .into_any_element(),
                YueSwitch::new("toggle", theme, self.checked, "开关")
                    .when(self.inherited, |switch| switch.use_default_theme())
                    .key_context("YueToggle")
                    .map(|switch| match &self.switch_focus {
                        Some(focus) => switch.track_focus(focus),
                        None => switch,
                    })
                    .disabled(self.disabled)
                    .loading(self.loading)
                    .on_change(move |value, _, _, cx| {
                        owner.update(cx, |this, cx| {
                            this.checked = value;
                            this.changes.push(value);
                            cx.notify();
                        })
                    })
                    .into_any_element(),
            ])
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|_, _| Probe::default());
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|window, _| {
        let mut nodes = gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|node| node.label() == Some(label));
        let node = nodes.next().expect("named control");
        assert!(nodes.next().is_none(), "unique control");
        node
    })
}

fn click(cx: &mut VisualTestContext, label: &str) {
    let p = node(cx, label).bounds().center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
}

fn activate(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
}

fn tick(cx: &mut VisualTestContext, millis: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(millis));
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

fn press(cx: &mut VisualTestContext, position: Point<gpui_kit::Pixels>) {
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    draw(cx);
}

fn release(cx: &mut VisualTestContext, position: Point<gpui_kit::Pixels>) {
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    draw(cx);
}

#[gpui_kit::test]
fn pointer_and_keyboard_commands_fire_once(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "执行");
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    // The button is the first tab stop; obtain it independently of pointer focus policy.
    cx.update(|window, cx| window.focus_prev(cx));
    draw(cx);
    assert_eq!(node(cx, "执行").focused(), Some(true));
    activate(cx, "enter");
    activate(cx, "space");
    assert_eq!(view.read_with(cx, |p, _| p.clicks), 3);
}

#[gpui_kit::test]
fn switch_reports_controlled_values_once_and_external_updates_are_silent(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "开关");
    assert_eq!(view.read_with(cx, |p, _| p.changes.clone()), [true]);
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    cx.update(|window, cx| window.focus_prev(cx));
    draw(cx);
    assert_eq!(node(cx, "开关").focused(), Some(true));
    activate(cx, "space");
    activate(cx, "enter");
    assert_eq!(
        view.read_with(cx, |p, _| p.changes.clone()),
        [true, false, true]
    );
    view.update(cx, |p, cx| {
        p.checked = false;
        cx.notify();
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |p, _| p.changes.len()), 3);
    assert_eq!(node(cx, "开关").checked(), Some(false));
}

#[gpui_kit::test]
fn disabled_and_loading_controls_ignore_activation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for loading in [false, true] {
        for label in ["执行", "开关"] {
            // Focus is taken while the control is live, so the keys below reach
            // a focused control rather than nothing at all.
            view.update(cx, |p, cx| {
                p.disabled = false;
                p.loading = false;
                cx.notify();
            });
            draw(cx);
            cx.update(|window, cx| window.focus_next(cx));
            draw(cx);
            if node(cx, label).focused() != Some(true) {
                cx.update(|window, cx| window.focus_next(cx));
                draw(cx);
            }
            assert_eq!(node(cx, label).focused(), Some(true), "{label}");
            view.update(cx, |p, cx| {
                p.disabled = !loading;
                p.loading = loading;
                cx.notify();
            });
            draw(cx);
            if label == "开关" {
                assert_eq!(
                    node(cx, label).disabled(),
                    Some(true),
                    "loading and disabled are unavailable"
                );
            }
            click(cx, label);
            activate(cx, "space");
            activate(cx, "enter");
            assert_eq!(
                view.read_with(cx, |p, _| (p.clicks, p.changes.len())),
                (0, 0),
                "{label} loading={loading}"
            );
            cx.update(|window, cx| window.blur(cx));
        }
    }
}

#[gpui_kit::test]
fn release_outside_cancels_commands_and_preserves_layout(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = node(cx, "执行").bounds();
    let surface = button_surface(cx).0;
    for label in ["执行", "开关"] {
        let centre = node(cx, label).bounds().center();
        press(cx, centre);
        tick(cx, 500);
        assert_eq!(node(cx, "执行").bounds(), before);
        let outside = point(px(650.), px(400.));
        cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::default());
        release(cx, outside);
        tick(cx, 500);
        assert_eq!(button_surface(cx).0, surface, "{label} press released");
    }
    assert_eq!(
        view.read_with(cx, |p, _| (p.clicks, p.changes.len())),
        (0, 0)
    );
    click(cx, "执行");
    click(cx, "开关");
    assert_eq!(
        view.read_with(cx, |p, _| (p.clicks, p.changes.len())),
        (1, 1)
    );
}

/// Inspect an actual solid surface quad inside the control's hit box. The
/// observation bounds alone would miss the press animation completely.
fn button_surface(
    cx: &mut VisualTestContext,
) -> (gpui_kit::Bounds<gpui_kit::ScaledPixels>, gpui_kit::Hsla) {
    let bounds = node(cx, "执行").bounds();
    cx.update(|window, _| {
        let bounds = bounds.scale(window.scale_factor());
        window
            .painted_quads()
            .into_iter()
            .filter_map(|quad| {
                quad.background
                    .as_solid()
                    .filter(|color| color.a > 0.9)
                    .map(|color| (quad.bounds, color))
            })
            .find(|(surface, _)| {
                surface.center() == bounds.center() && surface.size.width <= bounds.size.width
            })
            .expect("button surface submitted to scene")
    })
}

#[gpui_kit::test]
fn held_press_shrinks_surface_about_its_center_and_release_restores_it(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    let before = button_surface(cx).0;
    let position = node(cx, "执行").bounds().center();
    press(cx, position);
    tick(cx, 500);
    let held = button_surface(cx).0;
    assert_eq!(held.center(), before.center());
    assert!(held.size.width < before.size.width);
    assert!(held.size.height < before.size.height);
    tick(cx, 500);
    assert_eq!(button_surface(cx).0, held);
    release(cx, position);
    tick(cx, 500);
    assert_eq!(button_surface(cx).0, before);
}

#[gpui_kit::test]
fn disabling_during_a_press_does_not_leave_a_stuck_press_after_release(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = button_surface(cx).0;
    let position = node(cx, "执行").bounds().center();
    press(cx, position);
    tick(cx, 500);
    view.update(cx, |view, cx| {
        view.disabled = true;
        cx.notify();
    });
    draw(cx);
    release(cx, point(px(650.), px(400.)));
    tick(cx, 500);
    view.update(cx, |view, cx| {
        view.disabled = false;
        cx.notify();
    });
    draw(cx);
    tick(cx, 500);
    assert_eq!(button_surface(cx).0, before);
    assert_eq!(view.read_with(cx, |view, _| view.clicks), 0);
}

#[gpui_kit::test]
fn default_theme_updates_but_explicit_overrides_stay_local(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let light = button_surface(cx).1;
    // The unchecked track is the same grey in both modes; the checked one is not.
    view.update(cx, |view, cx| {
        view.checked = true;
        cx.notify();
    });
    draw(cx);
    tick(cx, 500);
    let light_track = switch_track(cx);
    view.update(cx, |view, cx| {
        view.inherited = true;
        cx.notify();
    });
    let dark = YueTheme::new(ThemeMode::Dark, Accent::Azure);
    cx.update(|_, cx| yue_component::set_theme(cx, dark));
    draw(cx);
    tick(cx, 500);
    assert_ne!(button_surface(cx).1, light);
    assert_ne!(switch_track(cx), light_track);
    cx.update(|_, cx| {
        assert_eq!(yue_component::current_theme(cx).mode(), ThemeMode::Dark);
        assert_eq!(
            gpui_kit::base::Theme::global(cx).appearance,
            gpui_kit::base::ThemeAppearance::Dark
        );
    });
    view.update(cx, |view, cx| {
        view.inherited = false;
        cx.notify();
    });
    draw(cx);
    tick(cx, 500);
    assert_eq!(button_surface(cx).1, light);
    assert_eq!(switch_track(cx), light_track);
}

/// The opaque track quad, which fills the switch's semantic node exactly.
fn switch_track(cx: &mut VisualTestContext) -> gpui_kit::Hsla {
    let bounds = node(cx, "开关").bounds();
    cx.update(|window, _| {
        let bounds = bounds.scale(window.scale_factor());
        window
            .painted_quads()
            .into_iter()
            .filter(|quad| quad.bounds == bounds)
            .find_map(|quad| quad.background.as_solid().filter(|color| color.a > 0.9))
            .expect("switch track submitted to scene")
    })
}

#[gpui_kit::test]
fn a_switch_with_caller_focus_keeps_activation_context_and_tab_stop(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let focus = cx.update(|_, cx| cx.focus_handle());
    view.update(cx, |view, cx| {
        view.switch_focus = Some(focus.clone());
        view.skip_button = true;
        cx.notify();
    });
    draw(cx);
    // With the button out of the tab order, the first stop is the switch, on
    // the caller's handle: the control applies its own order to that handle.
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "执行").focused(), Some(false));
    assert_eq!(node(cx, "开关").focused(), Some(true));
    assert!(cx.update(|window, _| focus.is_focused(window)));
    activate(cx, "space");
    assert_eq!(view.read_with(cx, |view, _| view.changes.clone()), [true]);
    cx.update(|_, cx| {
        cx.bind_keys([gpui_kit::KeyBinding::new(
            "ctrl-s",
            SaveCommand,
            Some("YueToggle"),
        )])
    });
    cx.simulate_keystrokes("ctrl-s");
    draw(cx);
    assert_eq!(view.read_with(cx, |view, _| view.clicks), 1);
}

#[gpui_kit::test]
fn layout_and_caller_focus_keep_activation_and_parent_actions(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let focus = cx.update(|_, cx| cx.focus_handle());
    view.update(cx, |view, cx| {
        view.width = Some(px(180.));
        view.focus = Some(focus.clone());
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "执行").bounds().size.width, px(180.));
    cx.update(|window, cx| window.focus(&focus, cx));
    draw(cx);
    assert_eq!(node(cx, "执行").focused(), Some(true));
    activate(cx, "enter");
    assert_eq!(view.read_with(cx, |view, _| view.clicks), 1);
    cx.update(|_, cx| {
        cx.bind_keys([gpui_kit::KeyBinding::new(
            "ctrl-s",
            SaveCommand,
            Some("YueCommand"),
        )])
    });
    cx.simulate_keystrokes("ctrl-s");
    draw(cx);
    assert_eq!(view.read_with(cx, |view, _| view.clicks), 2);
}

fn switch_thumb(cx: &mut VisualTestContext) -> gpui_kit::Bounds<gpui_kit::ScaledPixels> {
    let bounds = node(cx, "开关").bounds();
    cx.update(|window, _| {
        let bounds = bounds.scale(window.scale_factor());
        window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                quad.background
                    .as_solid()
                    .is_some_and(|color| color.l > 0.99)
                    && bounds.contains(&quad.bounds.center())
                    && quad.bounds.size.width < bounds.size.width
            })
            .map(|quad| quad.bounds)
            .next()
            .expect("switch thumb submitted to scene")
    })
}

#[gpui_kit::test]
fn switch_thumb_moves_through_intermediate_positions_and_reduced_motion_snaps(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let off = switch_thumb(cx);
    view.update(cx, |view, cx| {
        view.checked = true;
        cx.notify();
    });
    draw(cx);
    tick(cx, 40);
    let middle = switch_thumb(cx);
    tick(cx, 500);
    let on = switch_thumb(cx);
    assert!(off.origin.x < middle.origin.x && middle.origin.x < on.origin.x);
    assert_eq!(off.size, on.size);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    view.update(cx, |view, cx| {
        view.checked = false;
        cx.notify();
    });
    draw(cx);
    assert_eq!(switch_thumb(cx), off);
}

#[gpui_kit::test]
fn keyboard_focus_submits_a_visible_ring_without_changing_layout(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    let before = node(cx, "执行").bounds();
    let switch_before = node(cx, "开关").bounds();
    activate(cx, "tab");
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    // Focus may have advanced to the switch if Tab was bound by the host.
    if node(cx, "执行").focused() != Some(true) {
        cx.update(|window, cx| window.focus_prev(cx));
        draw(cx);
    }
    assert_eq!(node(cx, "执行").focused(), Some(true));
    assert_eq!(node(cx, "执行").bounds(), before);
    cx.update(|window, _| {
        let ring: gpui_kit::Hsla = YueTheme::new(ThemeMode::Light, Accent::Azure)
            .token("--button-focus-ring-color")
            .unwrap()
            .rgba()
            .unwrap()
            .into();
        assert!(
            window
                .painted_quads()
                .iter()
                .any(|quad| quad.bounds == before.scale(window.scale_factor())
                    && quad.border_color == ring)
        );
    });
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "开关").focused(), Some(true));
    assert_eq!(node(cx, "开关").bounds(), switch_before);
    // The row stretches its children to the button's height, but the switch
    // keeps its token size, so its hit box is the track.
    assert_eq!(switch_before.size.width, px(40.));
    assert_eq!(switch_before.size.height, px(24.));
    // The ring stands off the track by offset + width on every side, so it
    // reads against the page whatever colour the track is.
    cx.update(|window, _| {
        let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
        let ring: gpui_kit::Hsla = theme
            .token("--focus-ring-color")
            .unwrap()
            .rgba()
            .unwrap()
            .into();
        let outset = px(4.);
        let expected = gpui_kit::Bounds::new(
            switch_before.origin - point(outset, outset),
            gpui_kit::size(
                switch_before.size.width + outset * 2.,
                switch_before.size.height + outset * 2.,
            ),
        )
        .scale(window.scale_factor());
        assert!(
            window
                .painted_quads()
                .iter()
                .any(|quad| quad.bounds == expected && quad.border_color == ring),
            "switch focus ring outside the track"
        );
    });
}
