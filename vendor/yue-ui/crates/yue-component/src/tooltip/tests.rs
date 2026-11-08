use super::*;
use gpui_base::test_support::snapshots;
use gpui_kit::base::Root;
use gpui_kit::{
    AppContext as _, Context, Entity, Modifiers, Render, TestAppContext, VisualTestContext, point,
    px,
};

struct Demo {
    text: SharedString,
    mode: TooltipTrigger,
    open: Option<bool>,
    disabled: bool,
    focus: FocusHandle,
    changes: Vec<bool>,
    commands: usize,
    visible: bool,
    arrow: bool,
    side: TooltipPlacement,
    align: TooltipAlign,
    auto: Option<Duration>,
    delay: Option<Duration>,
}
impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure);
        let owner = cx.entity();
        let mut help = YueTooltip::new("help", theme, self.text.clone())
            .track_focus(&self.focus)
            .trigger_mode(self.mode)
            .disabled(self.disabled)
            .show_arrow(self.arrow)
            .placement(self.side)
            .align(self.align)
            .auto_hide_after(self.auto)
            .on_open_change(move |open, _, cx| {
                owner.update(cx, |demo, _| demo.changes.push(*open))
            });
        if let Some(open) = self.open {
            help = help.open(open);
        }
        if let Some(delay) = self.delay {
            help = help.show_delay(delay);
        }
        let target = crate::YueButton::new("target", "Target")
            .track_focus(&self.focus)
            .on_click(cx.listener(|demo, _, _, _| demo.commands += 1));
        let mut content = div().size_full().flex().items_center().justify_center();
        if self.visible {
            content = content.child(help.trigger(target));
        }
        content
    }
}
fn open(cx: &mut TestAppContext) -> (Entity<Demo>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::popup::init(cx);
        super::init(cx);
        cx.set_reduce_motion(true);
    });
    let mut saved = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Demo {
            text: "A short explanation".into(),
            mode: TooltipTrigger::HoverFocus,
            open: None,
            disabled: false,
            focus: cx.focus_handle(),
            changes: vec![],
            commands: 0,
            visible: true,
            arrow: true,
            side: TooltipPlacement::Top,
            align: TooltipAlign::Center,
            auto: None,
            delay: None,
        });
        saved = Some(view.clone());
        Root::new(view, window, cx)
    });
    cx.update(|w, _| w.activate_window());
    draw(cx);
    (saved.unwrap(), cx)
}
fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.run_until_parked();
}
fn tick(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    draw(cx);
}
fn shown(cx: &mut VisualTestContext) -> bool {
    cx.update(|w, _| {
        snapshots(w)
            .iter()
            .any(|n| n.role() == Some(gpui_kit::Role::Tooltip))
    })
}
fn target(cx: &mut VisualTestContext) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update(|w, _| {
        snapshots(w)
            .iter()
            .find(|n| n.label() == Some("Target"))
            .unwrap()
            .bounds()
    })
}
fn hover(cx: &mut VisualTestContext) {
    let p = target(cx).center();
    cx.simulate_mouse_move(p, None, Modifiers::default());
    draw(cx);
}
fn change(view: &Entity<Demo>, cx: &mut VisualTestContext, edit: impl FnOnce(&mut Demo)) {
    view.update(cx, |v, cx| {
        edit(v);
        cx.notify();
    });
    draw(cx);
}

struct TextProbe {
    config: Config,
    measured: Option<surface::Measure>,
}
impl Render for TextProbe {
    fn render(&mut self, w: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Measuring and laying out in different inherited styles reproduces
        // a tooltip moving from a button's context into the overlay host.
        let measured = w.with_text_style(
            Some(gpui_kit::TextStyleRefinement {
                font_family: Some(crate::typography::mono_font(self.config.theme).family),
                font_weight: Some(gpui_kit::FontWeight::BOLD),
                font_style: Some(gpui_kit::FontStyle::Italic),
                font_features: Some(crate::tabular_nums()),
                ..Default::default()
            }),
            |w| surface::measure(&self.config, w),
        );
        self.measured = Some(measured);
        let text = surface::content(&self.config, measured, true, 1., None, w);
        let family = crate::typography::ui_font(self.config.theme);
        div().font_family(family.family).child(text)
    }
}

#[gpui_kit::test]
fn short_tooltip_text_and_wrapped_text_match_the_measured_surface(cx: &mut TestAppContext) {
    let theme = YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure);
    cx.update(|cx| {
        gpui_kit::init(cx);
    });
    let mut saved = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let view = cx.new(|_| TextProbe {
            config: Config::new("缩小（−）".into(), theme, TooltipOptions::default()),
            measured: None,
        });
        saved = Some(view.clone());
        Root::new(view, w, cx)
    });
    let view = saved.unwrap();
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        for rem in [16., 20., 24.] {
            cx.update(|w, _| w.set_rem_size(px(rem)));
            for text in [
                "缩小（−）".to_owned(),
                "放大（+）".to_owned(),
                "适应窗口（0）".to_owned(),
                "100%：一个图像像素对应一个屏幕物理像素（1）".to_owned(),
                "较长的提示文字应该在最大宽度内自然换行，而不是因为字体测量不一致多出一行。"
                    .repeat(3),
                "第一行\n第二行".to_owned(),
            ] {
                view.update(cx, |v, cx| {
                    v.config.text = text.clone().into();
                    cx.notify();
                });
                draw(cx);
                let measured = view.read_with(cx, |v, _| v.measured.unwrap());
                let text_bounds = cx.update(|w, _| {
                    snapshots(w)
                        .into_iter()
                        .find(|n| n.path().last() == Some(&"yue-tooltip-text".into()))
                        .expect("real text layout")
                        .bounds()
                });
                let padding = cx.update(|w, _| {
                    surface::length(theme, crate::theme::keys::BOX_PADDING_BLOCK_TOOLTIP, w)
                });
                assert!(
                    (text_bounds.size.height - (measured.size.height - padding * 2.)).abs()
                        <= px(1. / scale),
                    "{text}, scale={scale}, rem={rem}: actual={text_bounds:?}, measured={:?}",
                    measured.size,
                );
                if text.chars().count() < 12 && !text.contains('\n') {
                    assert!(
                        measured.single_line,
                        "short label must stay on one line: {text}"
                    );
                    cx.update(|w, _| {
                        let family = crate::typography::ui_font(theme);
                        let mut font = gpui_kit::font(family.family.clone());
                        font.fallbacks = family.font_fallbacks();
                        let lines = w
                            .text_system()
                            .shape_text(
                                text.clone().into(),
                                surface::length(
                                    theme,
                                    crate::theme::keys::BOX_FONT_SIZE_TOOLTIP,
                                    w,
                                ),
                                &[gpui_kit::TextRun {
                                    len: text.len(),
                                    font,
                                    ..Default::default()
                                }],
                                None,
                                None,
                            )
                            .unwrap();
                        assert!(
                            lines
                                .iter()
                                .all(|line| line.size(measured.line_height).width
                                    <= text_bounds.size.width),
                            "nowrap must not conceal an undersized content area: {text}"
                        );
                    });
                }
                // Width must be independent of the trigger's inherited face.
                let neutral = cx.update(|w, cx| surface::measure(&view.read(cx).config, w));
                assert_eq!(
                    measured.size, neutral.size,
                    "inherited font changed measurement: {text}"
                );
            }
        }
    }
}

#[gpui_kit::test]
fn hover_waits_300ms_and_leaving_waits_for_grace(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    hover(cx);
    tick(cx, 299);
    assert!(!shown(cx));
    tick(cx, 1);
    assert!(shown(cx));
    cx.simulate_mouse_move(point(px(1.), px(1.)), None, Modifiers::default());
    draw(cx);
    tick(cx, 299);
    assert!(shown(cx));
    tick(cx, 1);
    assert!(!shown(cx));
}

#[gpui_kit::test]
fn installed_defaults_and_per_trigger_delay_remain_authoritative(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::base::TooltipDefaults::new()
            .with_show_delay(Duration::from_millis(700))
            .install(cx);
    });
    let (view, cx) = open(cx);
    hover(cx);
    tick(cx, 699);
    assert!(!shown(cx));
    tick(cx, 1);
    assert!(shown(cx));
    cx.simulate_mouse_move(point(px(1.), px(1.)), None, Modifiers::default());
    draw(cx);
    tick(cx, 300);
    assert!(!shown(cx));
    change(&view, cx, |view| {
        view.delay = Some(Duration::from_millis(120))
    });
    hover(cx);
    tick(cx, 119);
    assert!(!shown(cx));
    tick(cx, 1);
    assert!(shown(cx));
}

#[gpui_kit::test]
fn twelve_placements_fade_and_drift_four_pixels_with_continuous_reentry(cx: &mut TestAppContext) {
    use gpui_kit::base::PresencePhase;
    let (view, cx) = open(cx);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    for side in [
        TooltipPlacement::Top,
        TooltipPlacement::Bottom,
        TooltipPlacement::Left,
        TooltipPlacement::Right,
    ] {
        for align in [TooltipAlign::Start, TooltipAlign::Center, TooltipAlign::End] {
            change(&view, cx, |view| {
                view.mode = TooltipTrigger::Manual;
                view.text = "First line explains the action\nSecond line explains its scope\nThird line keeps the offset visible".into();
                view.side = side;
                view.align = align;
                view.open = Some(true);
            });
            let start = sample(cx).position().unwrap().bounds;
            assert_eq!(sample(cx).phase(), PresencePhase::Entering);
            tick(cx, 100);
            assert_eq!(
                sample(cx).phase(),
                PresencePhase::Entering,
                "entry lasts 200ms"
            );
            tick(cx, 100);
            assert_eq!(sample(cx).phase(), PresencePhase::Present);
            let rest = sample(cx).position().unwrap().bounds;
            let distance = match side {
                TooltipPlacement::Top => start.top() - rest.top(),
                TooltipPlacement::Bottom => rest.top() - start.top(),
                TooltipPlacement::Left => start.left() - rest.left(),
                TooltipPlacement::Right => rest.left() - start.left(),
            };
            assert_eq!(distance, px(4.), "{side:?}/{align:?}");
            let target = target(cx);
            let check_safe = |cx: &mut VisualTestContext| {
                let visual = sample(cx).visual_bounds().unwrap();
                assert!(
                    visual.right() <= target.left()
                        || target.right() <= visual.left()
                        || visual.bottom() <= target.top()
                        || target.bottom() <= visual.top(),
                    "{side:?}/{align:?} must protect the trigger and arrow envelope"
                );
            };
            check_safe(cx);
            change(&view, cx, |view| view.open = Some(false));
            tick(cx, 50);
            assert_eq!(sample(cx).phase(), PresencePhase::Exiting);
            let exiting = sample(cx).position().unwrap().bounds;
            assert_ne!(exiting, rest, "exit drifts back toward the trigger");
            check_safe(cx);
            change(&view, cx, |view| view.open = Some(true));
            assert_eq!(
                sample(cx).position().unwrap().bounds,
                exiting,
                "reentry resumes the current sample"
            );
            tick(cx, 200);
            assert_eq!(sample(cx).position().unwrap().bounds, rest);
            change(&view, cx, |view| view.open = Some(false));
            tick(cx, 100);
            assert!(
                cx.update(|w, cx| controller::snapshot(w, cx).is_none()),
                "exit unmounts at 100ms"
            );
        }
    }
}
#[gpui_kit::test]
fn entrance_distance_scales_with_rem_and_reduced_motion_finishes_immediately(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    cx.update(|w, cx| {
        w.set_rem_size(px(24.));
        cx.set_reduce_motion(false);
    });
    change(&view, cx, |view| view.open = Some(true));
    let start = sample(cx).position().unwrap().bounds;
    tick(cx, 200);
    let rest = sample(cx).position().unwrap().bounds;
    assert_eq!(start.top() - rest.top(), px(6.));
    change(&view, cx, |view| view.open = Some(false));
    tick(cx, 50);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    assert!(cx.update(|w, cx| controller::snapshot(w, cx).is_none()));
    change(&view, cx, |view| view.open = Some(true));
    assert_eq!(sample(cx).position().unwrap().bounds, rest);
    assert_eq!(sample(cx).phase(), gpui_kit::base::PresencePhase::Present);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    draw(cx);
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
}

#[gpui_kit::test]
fn focus_is_immediate_and_escape_does_not_move_focus_or_reopen(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let focus = v.read_with(cx, |v, _| v.focus.clone());
    cx.update(|w, cx| focus.focus(w, cx));
    draw(cx);
    assert!(shown(cx));
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!shown(cx));
    cx.update(|w, _| assert!(focus.is_focused(w)));
    tick(cx, 1000);
    assert!(!shown(cx));
}
#[gpui_kit::test]
fn command_click_closes_help_and_runs_the_original_command_once(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    hover(cx);
    tick(cx, 500);
    assert!(shown(cx));
    let p = target(cx).center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
    assert!(!shown(cx));
    assert_eq!(v.read_with(cx, |v, _| v.commands), 1);
    tick(cx, 1000);
    assert!(!shown(cx));
}
#[gpui_kit::test]
fn controlled_open_sync_is_silent_and_cannot_override_disabled_or_empty(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    change(&v, cx, |v| v.open = Some(true));
    draw(cx);
    assert!(shown(cx));
    assert!(v.read_with(cx, |v, _| v.changes.is_empty()));
    change(&v, cx, |v| v.disabled = true);
    assert!(!shown(cx));
    change(&v, cx, |v| v.text = " ".into());
    assert!(!shown(cx));
}
#[gpui_kit::test]
fn pending_unmount_does_not_show_later_and_auto_hide_does_not_loop(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    hover(cx);
    tick(cx, 200);
    change(&v, cx, |v| v.visible = false);
    tick(cx, 500);
    assert!(!shown(cx));
    change(&v, cx, |v| {
        v.visible = true;
        v.auto = Some(Duration::from_secs(2));
    });
    hover(cx);
    tick(cx, 300);
    assert!(shown(cx));
    tick(cx, 1999);
    assert!(shown(cx));
    tick(cx, 1);
    assert!(!shown(cx));
    tick(cx, 2000);
    assert!(!shown(cx));
}
#[test]
fn special_dimensions_degrade_only_the_relevant_axis_and_restore_request() {
    use gpui_kit::base::{Align, Placement};
    for side in [Placement::Left, Placement::Right] {
        for a in [Align::Start, Align::End] {
            assert_eq!(
                controller::effective_align(side, a, true, px(100.), px(40.)),
                Align::Center
            );
            assert_eq!(
                controller::effective_align(side, a, false, px(100.), px(40.)),
                a
            );
        }
    }
    for side in [Placement::Top, Placement::Bottom] {
        for a in [Align::Start, Align::End] {
            for w in [39., 40.] {
                assert_eq!(
                    controller::effective_align(side, a, false, px(w), px(40.)),
                    Align::Center
                );
            }
            assert_eq!(
                controller::effective_align(side, a, false, px(41.), px(40.)),
                a
            );
        }
    }
}

#[gpui_kit::test]
fn hovering_the_bubble_keeps_it_readable_then_resumes_grace(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    hover(cx);
    tick(cx, 500);
    let bubble = cx.update(|w, cx| {
        controller::snapshot(w, cx)
            .unwrap()
            .position()
            .unwrap()
            .bounds
    });
    cx.simulate_mouse_move(bubble.center(), None, Modifiers::default());
    draw(cx);
    tick(cx, 1000);
    assert!(shown(cx));
    cx.simulate_mouse_move(point(px(1.), px(1.)), None, Modifiers::default());
    draw(cx);
    tick(cx, 300);
    assert!(!shown(cx));
}

#[gpui_kit::test]
fn hover_and_focus_form_a_union_and_keyboard_activation_keeps_command(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let focus = v.read_with(cx, |v, _| v.focus.clone());
    cx.update(|w, cx| focus.focus(w, cx));
    draw(cx);
    hover(cx);
    cx.simulate_mouse_move(point(px(1.), px(1.)), None, Modifiers::default());
    draw(cx);
    tick(cx, 500);
    assert!(shown(cx));
    let keystroke = gpui_kit::Keystroke::parse("enter").unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
    assert!(!shown(cx));
    assert_eq!(v.read_with(cx, |v, _| v.commands), 1);
    tick(cx, 1000);
    assert!(!shown(cx));
}

#[gpui_kit::test]
fn controlled_false_true_rearms_after_hard_close_without_echo(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    change(&v, cx, |v| {
        v.mode = TooltipTrigger::Manual;
        v.open = Some(true);
    });
    assert!(shown(cx));
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(v.read_with(cx, |v, _| v.changes.clone()), vec![false]);
    change(&v, cx, |_| {});
    assert!(!shown(cx));
    change(&v, cx, |v| v.open = Some(false));
    change(&v, cx, |v| v.open = Some(true));
    assert!(shown(cx));
    assert_eq!(v.read_with(cx, |v, _| v.changes.clone()), vec![false]);
    change(&v, cx, |v| v.disabled = true);
    assert!(!shown(cx));
    change(&v, cx, |v| v.disabled = false);
    assert!(!shown(cx));
    assert_eq!(
        v.read_with(cx, |v, _| v.changes.clone()),
        vec![false, false]
    );
}

#[gpui_kit::test]
fn click_and_context_menu_use_real_pointer_inputs(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    change(&v, cx, |v| v.mode = TooltipTrigger::Click);
    hover(cx);
    tick(cx, 1000);
    assert!(!shown(cx));
    let p = target(cx).center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
    assert!(shown(cx));
    assert_eq!(v.read_with(cx, |v, _| v.commands), 1);
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
    assert!(!shown(cx));
    change(&v, cx, |v| v.mode = TooltipTrigger::ContextMenu);
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: p,
        button: gpui_kit::MouseButton::Right,
        click_count: 1,
        modifiers: Modifiers::default(),
        first_mouse: false,
    });
    draw(cx);
    assert!(shown(cx));
    cx.simulate_click(point(px(1.), px(1.)), Modifiers::default());
    draw(cx);
    assert!(!shown(cx));
}

struct Toolbar {
    axis: gpui_kit::Axis,
    gap: f32,
    middle: bool,
    different_size: bool,
    offset: f32,
}
impl Render for Toolbar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut group = YueTooltipGroup::new("toolbar", self.axis).gap(px(self.gap));
        group = group.child(crate::YueButton::new("a", "A").tooltip("Help"));
        if self.middle {
            group = group.protected_child("plain", crate::YueButton::new("plain-button", "Plain"));
        }
        group = group.child(
            crate::YueButton::new("b", "B").tooltip(if self.different_size {
                "A longer explanation"
            } else {
                "Help"
            }),
        );
        group = group.child(crate::YueButton::new("c", "C").tooltip("Help"));
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .pl(px(self.offset))
            .child(group)
    }
}
fn toolbar(
    cx: &mut TestAppContext,
    axis: gpui_kit::Axis,
) -> (Entity<Toolbar>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::popup::init(cx);
        super::init(cx);
        cx.set_reduce_motion(false);
    });
    let mut saved = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let v = cx.new(|_| Toolbar {
            axis,
            gap: 8.,
            middle: false,
            different_size: false,
            offset: 0.,
        });
        saved = Some(v.clone());
        Root::new(v, w, cx)
    });
    cx.update(|w, _| w.activate_window());
    draw(cx);
    (saved.unwrap(), cx)
}
fn button_bounds(cx: &mut VisualTestContext, name: &str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.label() == Some(name))
            .unwrap()
            .bounds()
    })
}
fn hover_button(cx: &mut VisualTestContext, name: &str) {
    let p = button_bounds(cx, name).center();
    cx.simulate_mouse_move(p, None, Modifiers::default());
    draw(cx);
}
fn sample(cx: &mut VisualTestContext) -> crate::popup::PopupSnapshot {
    cx.update(|w, cx| controller::snapshot(w, cx).unwrap())
}
fn no_overlap(cx: &mut VisualTestContext) {
    let visual = sample(cx).visual_bounds().expect("safe envelope");
    for name in ["A", "B", "C"] {
        let b = button_bounds(cx, name);
        assert!(
            visual.right() <= b.left()
                || b.right() <= visual.left()
                || visual.bottom() <= b.top()
                || b.bottom() <= visual.top()
        );
    }
}
#[gpui_kit::test]
fn horizontal_group_translates_on_one_rail_and_quick_reversal_is_continuous(
    cx: &mut TestAppContext,
) {
    let (_, cx) = toolbar(cx, gpui_kit::Axis::Horizontal);
    hover_button(cx, "A");
    tick(cx, 300);
    tick(cx, 200);
    let a = sample(cx).position().unwrap();
    no_overlap(cx);
    hover_button(cx, "B");
    let start = sample(cx).position().unwrap();
    assert_eq!(start.bounds, a.bounds);
    assert_eq!(sample(cx).phase(), gpui_kit::base::PresencePhase::Present);
    tick(cx, 75);
    let midway = sample(cx).position().unwrap();
    assert_eq!(sample(cx).phase(), gpui_kit::base::PresencePhase::Present);
    assert!(midway.bounds.left() > a.bounds.left());
    assert_eq!(midway.bounds.bottom(), a.bounds.bottom());
    no_overlap(cx);
    hover_button(cx, "A");
    assert_eq!(sample(cx).position().unwrap().bounds, midway.bounds);
    tick(cx, 150);
    assert_eq!(sample(cx).position().unwrap().bounds, a.bounds);
    no_overlap(cx);
}
#[gpui_kit::test]
fn vertical_group_translates_beside_the_entire_column(cx: &mut TestAppContext) {
    let (_, cx) = toolbar(cx, gpui_kit::Axis::Vertical);
    hover_button(cx, "A");
    tick(cx, 300);
    tick(cx, 200);
    let a = sample(cx).position().unwrap();
    assert_eq!(a.placement, Some(gpui_kit::base::Placement::Right));
    hover_button(cx, "B");
    tick(cx, 75);
    let b = sample(cx).position().unwrap();
    assert!(b.bounds.top() > a.bounds.top());
    assert_eq!(b.bounds.left(), a.bounds.left());
    no_overlap(cx);
}
#[gpui_kit::test]
fn distant_and_nonadjacent_targets_restart_delay(cx: &mut TestAppContext) {
    let (v, cx) = toolbar(cx, gpui_kit::Axis::Horizontal);
    v.update(cx, |v, cx| {
        v.middle = true;
        cx.notify();
    });
    draw(cx);
    hover_button(cx, "A");
    tick(cx, 300);
    tick(cx, 200);
    hover_button(cx, "B");
    assert!(!shown(cx));
    tick(cx, 299);
    assert!(!shown(cx));
    tick(cx, 1);
    assert!(shown(cx));
    tick(cx, 100);
    v.update(cx, |v, cx| {
        v.gap = 40.;
        cx.notify();
    });
    draw(cx);
    hover_button(cx, "C");
    tick(cx, 299);
    assert!(!shown(cx));
    tick(cx, 1);
    assert!(shown(cx));
}
#[gpui_kit::test]
fn size_change_fades_in_place_without_an_extra_hover_delay(cx: &mut TestAppContext) {
    let (v, cx) = toolbar(cx, gpui_kit::Axis::Horizontal);
    v.update(cx, |v, cx| {
        v.different_size = true;
        cx.notify();
    });
    draw(cx);
    hover_button(cx, "A");
    tick(cx, 300);
    tick(cx, 200);
    let a = sample(cx).position().unwrap().bounds;
    hover_button(cx, "B");
    assert!(!shown(cx)); // Exiting copy is paint-only.
    assert_eq!(sample(cx).position().unwrap().bounds, a);
    tick(cx, 100);
    draw(cx);
    assert!(shown(cx));
    let b = sample(cx).position().unwrap().bounds;
    assert!(b.size.width > a.size.width);
    no_overlap(cx);
}
#[gpui_kit::test]
fn moving_anchor_layout_follows_immediately_without_chasing(cx: &mut TestAppContext) {
    let (v, cx) = toolbar(cx, gpui_kit::Axis::Horizontal);
    hover_button(cx, "A");
    tick(cx, 300);
    tick(cx, 200);
    hover_button(cx, "B");
    tick(cx, 50);
    v.update(cx, |v, cx| {
        v.offset = 100.;
        cx.notify();
    });
    draw(cx);
    let b = button_bounds(cx, "B");
    let p = sample(cx).position().unwrap().bounds;
    assert!((p.center().x - b.center().x).abs() <= px(1.));
    no_overlap(cx);
}

#[gpui_kit::test]
fn real_layout_applies_twelve_alignments_and_restores_after_dimension_changes(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx);
    change(&v, cx, |v| {
        v.open = Some(true);
        v.text = "A fairly long first line\nSecond line".into();
    });
    for side in [
        TooltipPlacement::Top,
        TooltipPlacement::Bottom,
        TooltipPlacement::Left,
        TooltipPlacement::Right,
    ] {
        for align in [TooltipAlign::Start, TooltipAlign::Center, TooltipAlign::End] {
            change(&v, cx, |v| {
                v.side = side;
                v.align = align;
            });
            let b = sample(cx).position().unwrap().bounds;
            let t = target(cx);
            let (actual, expected) =
                if matches!(side, TooltipPlacement::Top | TooltipPlacement::Bottom) {
                    match align {
                        TooltipAlign::Start => (b.left(), t.left()),
                        TooltipAlign::Center => (b.center().x, t.center().x),
                        TooltipAlign::End => (b.right(), t.right()),
                    }
                } else {
                    match align {
                        TooltipAlign::Start => (b.top(), t.top()),
                        TooltipAlign::Center => (b.center().y, t.center().y),
                        TooltipAlign::End => (b.bottom(), t.bottom()),
                    }
                };
            assert!(
                (actual - expected).abs() <= px(1.),
                "{side:?} / {align:?}: {b:?} {t:?}"
            );
        }
    }
    change(&v, cx, |v| {
        v.side = TooltipPlacement::Left;
        v.align = TooltipAlign::Start;
        v.text = "Short".into();
    });
    assert!(
        (sample(cx).position().unwrap().bounds.center().y - target(cx).center().y).abs() <= px(1.)
    );
    change(&v, cx, |v| v.text = "First line\nSecond line".into());
    assert!((sample(cx).position().unwrap().bounds.top() - target(cx).top()).abs() <= px(1.));
    change(&v, cx, |v| {
        v.side = TooltipPlacement::Top;
        v.align = TooltipAlign::End;
        v.text = "Hi".into();
    });
    assert!(
        (sample(cx).position().unwrap().bounds.center().x - target(cx).center().x).abs() <= px(1.)
    );
    change(&v, cx, |v| v.text = "A much longer explanation".into());
    assert!((sample(cx).position().unwrap().bounds.right() - target(cx).right()).abs() <= px(1.));
    assert!(v.read_with(cx, |v, _| v.changes.is_empty()));
}

#[gpui_kit::test]
fn no_space_hard_closes_once_and_does_not_draw_over_the_target(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    change(&v, cx, |v| v.open = Some(true));
    assert!(shown(cx));
    cx.simulate_resize(gpui_kit::size(px(80.), px(45.)));
    draw(cx);
    draw(cx);
    assert!(!shown(cx));
    assert_eq!(v.read_with(cx, |v, _| v.changes.clone()), vec![false]);
    cx.simulate_resize(gpui_kit::size(px(800.), px(600.)));
    draw(cx);
    tick(cx, 1000);
    assert!(!shown(cx));
    assert_eq!(v.read_with(cx, |v, _| v.changes.clone()), vec![false]);
}

#[gpui_kit::test]
fn reduced_motion_switches_safely_and_stable_tooltip_stops_frames(cx: &mut TestAppContext) {
    let (_, cx) = toolbar(cx, gpui_kit::Axis::Horizontal);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    hover_button(cx, "A");
    tick(cx, 500);
    hover_button(cx, "B");
    assert!(shown(cx));
    let b = button_bounds(cx, "B");
    assert!((sample(cx).position().unwrap().bounds.center().x - b.center().x).abs() <= px(1.));
    no_overlap(cx);
    tick(cx, 500);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    draw(cx);
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
}

#[gpui_kit::test]
fn replacing_shared_focus_handle_rebinds_observers(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let old = v.read_with(cx, |v, _| v.focus.clone());
    let new = cx.update(|_, cx| cx.focus_handle());
    change(&v, cx, |v| v.focus = new.clone());
    cx.update(|w, cx| old.focus(w, cx));
    draw(cx);
    assert!(!shown(cx));
    cx.update(|w, cx| new.focus(w, cx));
    draw(cx);
    assert!(shown(cx));
}

struct OverflowDemo {
    width: f32,
    text: SharedString,
}
impl Render for OverflowDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::base::TestSupportExt as _;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div().id("slot-bounds").test_support().relative().child(
                    YueOverflowTooltip::new(
                        "overflow",
                        YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure),
                        self.text.clone(),
                    )
                    .width(px(self.width)),
                ),
            )
    }
}
#[gpui_kit::test]
fn actual_text_overflow_rechecks_during_hover_and_cancels_pending(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::popup::init(cx);
        super::init(cx);
        cx.set_reduce_motion(true);
    });
    let mut saved = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let v = cx.new(|_| OverflowDemo {
            width: 40.,
            text: "A path that cannot fit in this slot".into(),
        });
        saved = Some(v.clone());
        Root::new(v, w, cx)
    });
    let v = saved.unwrap();
    cx.update(|w, _| w.activate_window());
    draw(cx);
    draw(cx);
    let p = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&ElementId::from("slot-bounds")))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_mouse_move(p, None, Modifiers::default());
    draw(cx);
    tick(cx, 200);
    v.update(cx, |v, cx| {
        v.width = 1000.;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    tick(cx, 500);
    assert!(!shown(cx));
    // The same pointer is still inside the centered slot: shrinking re-evaluates
    // its already-active hover source without requiring another mouse event.
    v.update(cx, |v, cx| {
        v.width = 40.;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    tick(cx, 500);
    assert!(shown(cx));
    v.update(cx, |v, cx| {
        v.text = "Hi".into();
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(!shown(cx));
    tick(cx, 1000);
    assert!(!shown(cx));
}

#[gpui_kit::test]
fn rapid_switch_during_a_size_fade_keeps_the_hot_session(cx: &mut TestAppContext) {
    let (v, cx) = toolbar(cx, gpui_kit::Axis::Horizontal);
    v.update(cx, |v, cx| {
        v.different_size = true;
        cx.notify();
    });
    draw(cx);
    hover_button(cx, "A");
    tick(cx, 300);
    tick(cx, 200);
    hover_button(cx, "B");
    tick(cx, 40);
    hover_button(cx, "C");
    tick(cx, 100);
    draw(cx);
    assert!(shown(cx));
    tick(cx, 100);
    assert!(
        (sample(cx).position().unwrap().bounds.center().x - button_bounds(cx, "C").center().x)
            .abs()
            <= px(1.)
    );
    no_overlap(cx);
}

#[gpui_kit::test]
fn removed_visible_owner_cannot_leave_a_floating_tooltip(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    hover(cx);
    tick(cx, 500);
    assert!(shown(cx));
    change(&v, cx, |v| v.visible = false);
    draw(cx);
    assert!(!shown(cx));
    tick(cx, 1000);
    assert!(!shown(cx));
}

#[test]
fn adjacency_limits_are_inclusive_and_respect_rows_and_protected_intermediates() {
    use gpui_kit::{Axis, Bounds, size};
    let rect = |x, y, w, h| Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
    let a = rect(0., 0., 136., 32.);
    let b = rect(160., 0., 136., 32.);
    assert!(controller::adjacent(
        a,
        b,
        Axis::Horizontal,
        &[],
        px(24.),
        px(160.)
    ));
    assert!(!controller::adjacent(
        a,
        b,
        Axis::Horizontal,
        &[],
        px(23.99),
        px(160.)
    ));
    assert!(!controller::adjacent(
        a,
        b,
        Axis::Horizontal,
        &[],
        px(24.),
        px(159.99)
    ));
    assert!(!controller::adjacent(
        a,
        b,
        Axis::Horizontal,
        &[rect(145., 0., 8., 32.)],
        px(24.),
        px(160.)
    ));
    assert!(!controller::adjacent(
        a,
        rect(160., 17., 136., 32.),
        Axis::Horizontal,
        &[],
        px(24.),
        px(200.)
    ));
}

#[test]
fn tooltip_text_contrast_meets_45_in_all_profiles_and_neutral_tones() {
    use crate::{Accent, NeutralTone, ThemeMode};
    let luminance = |c: gpui_kit::Rgba| {
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    };
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for neutral in NeutralTone::ALL {
                for variant in [
                    TooltipVariant::Default,
                    TooltipVariant::Primary,
                    TooltipVariant::Success,
                    TooltipVariant::Danger,
                    TooltipVariant::Warning,
                    TooltipVariant::Light,
                ] {
                    let config = Config::new(
                        "Text".into(),
                        YueTheme::new(mode, accent).with_neutral(neutral),
                        TooltipOptions::new().variant(variant),
                    );
                    let (bg, fg, _, _) = surface::colors(&config);
                    assert_eq!(bg.a, 1.);
                    assert_eq!(fg.a, 1.);
                    let (a, b) = (luminance(bg), luminance(fg));
                    let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                    assert!(
                        ratio >= 4.5,
                        "{mode:?}/{accent:?}/{neutral:?}/{variant:?}: {ratio}"
                    );
                }
            }
        }
    }
}

struct ParentPopup {
    anchor: crate::popup::PopupAnchor,
    focus: FocusHandle,
    open: bool,
    escapes: usize,
}
impl Render for ParentPopup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::popup::{PopupDismissRoute, PopupHost, PopupId, PopupPlacement, PopupSpec};
        use gpui_kit::base::{Align, Placement};
        let focus = self.focus.clone();
        let surface = PopupHost::new(
            PopupSpec::new(
                PopupId::new(cx.entity_id(), "parent"),
                self.anchor.handle(),
                self.open,
                PopupPlacement::Side(Placement::Bottom, Align::Center),
            )
            .surface_gap(px(8.))
            .dismiss_route(PopupDismissRoute::Owner),
        )
        .mount(
            move |_, _, _, _| {
                crate::YueButton::new("nested-button", "Nested")
                    .track_focus(&focus)
                    .tooltip("Nested help")
                    .into_any_element()
            },
            window,
            cx,
        );
        div()
            .id("parent-key-path")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .on_key_down(cx.listener(|view, event: &gpui_kit::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    view.open = false;
                    view.escapes += 1;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .relative()
                    .w(px(80.))
                    .h(px(32.))
                    .child(self.anchor.capture()),
            )
            .children(surface)
    }
}
#[gpui_kit::test]
fn tooltip_in_a_parent_popup_takes_only_the_first_escape_and_preserves_focus(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::popup::init(cx);
        super::init(cx);
        cx.set_reduce_motion(true);
    });
    let mut saved = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let v = cx.new(|cx| ParentPopup {
            anchor: Default::default(),
            focus: cx.focus_handle(),
            open: true,
            escapes: 0,
        });
        saved = Some(v.clone());
        Root::new(v, w, cx)
    });
    let v = saved.unwrap();
    cx.update(|w, _| w.activate_window());
    draw(cx);
    let focus = v.read_with(cx, |v, _| v.focus.clone());
    cx.update(|w, cx| focus.focus(w, cx));
    draw(cx);
    assert!(shown(cx));
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!shown(cx));
    assert_eq!(v.read_with(cx, |v, _| (v.open, v.escapes)), (true, 0));
    cx.update(|w, _| assert!(focus.is_focused(w)));
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(v.read_with(cx, |v, _| (v.open, v.escapes)), (false, 1));
}

#[gpui_kit::test]
fn narrow_window_wraps_inside_popup_visual_reserves_before_positioning(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    cx.simulate_resize(gpui_kit::size(px(260.), px(600.)));
    draw(cx);
    change(&v, cx, |v| {
        v.text = "A long explanation including https://example.invalid/very/long/path/without/truncation that must wrap in a narrow window".into();
        v.open = Some(true);
    });
    assert!(shown(cx));
    let p = sample(cx);
    let bounds = p.position().unwrap().bounds;
    assert!(bounds.size.width <= px(236.));
    assert!(bounds.size.height > px(24.));
    let visual = p.visual_bounds().unwrap();
    assert!(visual.left() >= px(8.) && visual.right() <= px(252.));
    let target = target(cx);
    assert!(visual.bottom() <= target.top() || visual.top() >= target.bottom());
}
