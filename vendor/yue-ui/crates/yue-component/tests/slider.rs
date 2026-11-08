//! Slider contract regressions, including the shared Base consumer boundary.
use gpui_kit::base::slider::{SliderState as BaseState, SliderValue};

#[test]
fn base_assignment_clamps_and_orders_the_actual_value() {
    let state = BaseState::new().min(3.).max(13.).default_value((20., -4.));
    assert_eq!(state.value(), SliderValue::Range(3., 13.));
    assert_eq!(state.percentage(), 0.0..1.0);
}

#[test]
fn base_assignment_preserves_exact_color_picker_coordinates() {
    let state = BaseState::new().max(360.).step(1.).default_value(123.456);
    assert_eq!(state.value(), SliderValue::Single(123.456));
}

#[test]
#[should_panic(expected = "step")]
fn base_rejects_non_positive_step() {
    let _ = BaseState::new().step(0.);
}

use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Bounds, Context, ElementId, Entity, FocusHandle, IntoElement, Modifiers,
    MouseButton, ParentElement as _, Pixels, Point, Render, Role, ScaledPixels, Styled as _,
    Subscription, TestAppContext, VisualTestContext, Window, div, point, px,
};
use yue_component::{
    Accent, SliderControls, SliderDomain, SliderLayout, SliderMark, SliderSize, SliderTooltip,
    ThemeMode, YueButton, YueSlider, YueSliderEvent, YueSliderState, YueTheme,
};

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}
struct Probe {
    state: Entity<YueSliderState>,
    events: Vec<YueSliderEvent>,
    before: FocusHandle,
    after: FocusHandle,
    direction: SliderLayout,
    size: SliderSize,
    controls: SliderControls,
    tooltip: bool,
    width: Option<Pixels>,
    parent: Pixels,
    block: bool,
    marks: bool,
    snap_to_marks: bool,
    custom_marks: Vec<SliderMark>,
    skip: bool,
    echo: bool,
    replace: bool,
    _subscription: Subscription,
}
impl Probe {
    fn new(
        domain: SliderDomain,
        value: SliderValue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|cx| YueSliderState::new(domain, value, window, cx));
        let subscription = cx.subscribe_in(&state, window, |this, state, event, window, cx| {
            assert_eq!(
                state.read(cx).value(cx),
                match *event {
                    YueSliderEvent::Change(v) | YueSliderEvent::Commit(v) => v,
                }
            );
            this.events.push(*event);
            if let YueSliderEvent::Change(v) = *event {
                if this.echo {
                    state.update(cx, |s, cx| s.set_value(v, window, cx));
                }
                if this.replace {
                    state.update(cx, |s, cx| s.set_value(25., window, cx));
                }
            }
            cx.notify();
        });
        Self {
            state,
            events: vec![],
            before: cx.focus_handle(),
            after: cx.focus_handle(),
            direction: SliderLayout::Horizontal,
            size: SliderSize::Medium,
            controls: SliderControls::None,
            tooltip: false,
            width: None,
            parent: px(500.),
            block: false,
            marks: false,
            snap_to_marks: true,
            custom_marks: vec![],
            skip: false,
            echo: false,
            replace: false,
            _subscription: subscription,
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut control = YueSlider::new("subject", theme(), &self.state)
            .accessibility_label("数值")
            .lower_label("下限")
            .upper_label("上限")
            .accessibility_description("测试调节")
            .input_error_message("输入有效数字")
            .layout(self.direction)
            .size(self.size)
            .controls(self.controls)
            .snap_to_marks(self.snap_to_marks)
            .tab_stop(!self.skip)
            .tooltip(if self.tooltip {
                SliderTooltip::Auto
            } else {
                SliderTooltip::None
            })
            .value_format(|v| format!("{v}%").into());
        if let Some(w) = self.width {
            control = control.width(w);
        }
        if self.block {
            control = control.block();
        }
        if self.marks {
            control = control.marks([
                SliderMark::new(0., "起点"),
                SliderMark::new(20., "20°C"),
                SliderMark::new(100., "终点"),
            ]);
        }
        if !self.custom_marks.is_empty() {
            control = control.marks(self.custom_marks.clone());
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .p_4()
            .gap_4()
            .child(YueButton::text("before", theme(), "前一个").track_focus(&self.before))
            .child(div().w(self.parent).child(control))
            .child(YueButton::text("after", theme(), "后一个").track_focus(&self.after))
    }
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
}
fn open(
    cx: &mut TestAppContext,
    domain: SliderDomain,
    value: impl Into<SliderValue>,
) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| yue_component::init(cx, theme()).unwrap());
    let value = value.into();
    let mut probe = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let view = cx.new(|cx| Probe::new(domain, value, w, cx));
        probe = Some(view.clone());
        gpui_kit::base::Root::new(view, w, cx)
    });
    cx.update(|w, _| w.activate_window());
    cx.executor().run_until_parked();
    draw(cx);
    (probe.unwrap(), cx)
}
fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|w, _| {
        let nodes = snapshots(w)
            .into_iter()
            .filter(|n| n.label() == Some(label))
            .collect::<Vec<_>>();
        assert_eq!(nodes.len(), 1, "unique label {label}: {nodes:?}");
        nodes[0].clone()
    })
}
fn frame(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&ElementId::from("subject")))
            .expect("frame")
            .bounds()
    })
}
fn value(v: &Entity<Probe>, cx: &mut VisualTestContext) -> SliderValue {
    v.read_with(cx, |v, cx| v.state.read(cx).value(cx))
}
fn commits(v: &Entity<Probe>, cx: &mut VisualTestContext) -> usize {
    v.read_with(cx, |v, _| {
        v.events
            .iter()
            .filter(|e| matches!(e, YueSliderEvent::Commit(_)))
            .count()
    })
}
fn down(cx: &mut VisualTestContext, p: Point<Pixels>) {
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: p,
        button: MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
}

fn thumb_surface(cx: &mut VisualTestContext) -> Bounds<ScaledPixels> {
    let thumb = node(cx, "数值").bounds();
    cx.update(|w, _| {
        let thumb = thumb.scale(w.scale_factor());
        w.painted_quads()
            .into_iter()
            .find(|q| {
                q.bounds.center() == thumb.center()
                    && q.bounds.size.width == q.bounds.size.height
                    && q.bounds.size.width < thumb.size.width
                    && q.background.as_solid().is_some_and(|c| c.a > 0.9)
            })
            .expect("opaque thumb surface submitted to scene")
            .bounds
    })
}

#[gpui_kit::test]
fn all_sizes_shrink_around_the_center_without_changing_the_hitbox(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 50.);
    cx.simulate_scale_factor_change(1.);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    for (size, diameter) in [
        (SliderSize::Small, 12.),
        (SliderSize::Medium, 16.),
        (SliderSize::Large, 20.),
    ] {
        v.update(cx, |v, cx| {
            v.size = size;
            cx.notify();
        });
        draw(cx);
        let hit = node(cx, "数值").bounds();
        let normal = thumb_surface(cx);
        assert_eq!(normal.size.width, ScaledPixels(diameter));
        down(cx, hit.center());
        let pressed = thumb_surface(cx);
        assert!(
            pressed.size.width < normal.size.width,
            "{size:?}: pressed surface must shrink"
        );
        assert_eq!(pressed.center(), normal.center());
        assert_eq!(node(cx, "数值").bounds(), hit);
        up(cx, hit.center());
        assert_eq!(thumb_surface(cx), normal);
        assert_eq!(value(&v, cx), 50.0.into());
        assert_eq!(commits(&v, cx), 0);
    }
}
fn moving(cx: &mut VisualTestContext, p: Point<Pixels>) {
    cx.simulate_mouse_move(p, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
}
fn up(cx: &mut VisualTestContext, p: Point<Pixels>) {
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: p,
        button: MouseButton::Left,
        click_count: 1,
        ..Default::default()
    });
    draw(cx);
}
fn key_down(cx: &mut VisualTestContext, k: &str, held: bool) {
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke::parse(k).unwrap(),
        is_held: held,
        prefer_character_input: false,
    });
    draw(cx);
}
fn key_up(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_event(gpui_kit::KeyUpEvent {
        keystroke: gpui_kit::Keystroke::parse(k).unwrap(),
    });
    draw(cx);
}
fn key(cx: &mut VisualTestContext, k: &str) {
    key_down(cx, k, false);
    key_up(cx, k);
}
fn focus(v: &Entity<Probe>, lower: bool, cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        let s = v.read(cx).state.read(cx);
        let f = if lower {
            s.lower_focus_handle()
        } else {
            s.focus_handle()
        }
        .clone();
        f.focus(w, cx);
    });
    draw(cx);
}
fn replace(cx: &mut VisualTestContext, label: &str, text: &str) {
    click(cx, label);
    key(cx, "ctrl-a");
    cx.simulate_input(text);
    draw(cx);
}
fn click(cx: &mut VisualTestContext, label: &str) {
    let p = node(cx, label).bounds().center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
}

#[gpui_kit::test]
fn marked_slider_magnetizes_pointer_but_keeps_keyboard_and_numeric_steps(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default().step(3.), 0.);
    v.update(cx, |v, cx| {
        v.marks = true;
        v.echo = true;
        cx.notify();
    });
    draw(cx);
    let origin = node(cx, "数值").bounds().center();
    down(cx, origin);
    let span = frame(cx).size.width - node(cx, "数值").bounds().size.width;
    moving(cx, origin + point(span * 0.2, px(0.)));
    assert_eq!(
        value(&v, cx),
        20.0.into(),
        "dragging near 20 must snap exactly to the mark, not the step grid"
    );
    up(cx, origin + point(span * 0.2, px(0.)));
    assert_eq!(commits(&v, cx), 1);
    key(cx, "right");
    assert_eq!(value(&v, cx), 21.0.into());
    key(cx, "left");
    assert_eq!(value(&v, cx), 18.0.into());
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "37");
    key(cx, "enter");
    assert_eq!(value(&v, cx), 36.0.into());
    key(cx, "up");
    assert_eq!(value(&v, cx), 39.0.into());
    key(cx, "down");
    assert_eq!(value(&v, cx), 36.0.into());
    let increment = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&ElementId::from("increment")))
            .unwrap()
            .bounds()
    });
    let decrement = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&ElementId::from("decrement")))
            .unwrap()
            .bounds()
    });
    assert!(
        decrement.right() < increment.left()
            && (increment.center().y - decrement.center().y).abs() < px(0.5),
        "step controls flank the number on the same row"
    );
    cx.simulate_click(increment.center(), Modifiers::default());
    draw(cx);
    assert_eq!(value(&v, cx), 39.0.into());
    cx.simulate_click(decrement.center(), Modifiers::default());
    draw(cx);
    assert_eq!(value(&v, cx), 36.0.into());
    assert_eq!(node(cx, "数值").numeric_value_step(), Some(3.));
    assert_eq!(node(cx, "数值 value").numeric_value_step(), Some(3.));
}

#[gpui_kit::test]
fn marks_only_attract_nearby_pointer_positions_and_leave_free_dragging_available(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.marks = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        value(&v, cx),
        40.0.into(),
        "showing marks must not move an existing value"
    );
    let thumb = node(cx, "数值").bounds();
    let span = frame(cx).size.width - thumb.size.width;
    let origin = thumb.center() - point(span * 0.4, px(0.));
    down(cx, thumb.center());
    moving(cx, origin + point(span * 0.41, px(0.)));
    assert_eq!(
        value(&v, cx),
        41.0.into(),
        "dragging between marks stays free"
    );
    moving(cx, origin + point(span * 0.20 + px(2.), px(0.)));
    assert_eq!(
        value(&v, cx),
        20.0.into(),
        "only the small local mark region attracts"
    );
    moving(cx, origin + point(span * 0.20 + px(8.), px(0.)));
    assert!(
        value(&v, cx).end() > 20.,
        "leaving the magnetic region resumes the step grid"
    );
    up(cx, origin + point(span * 0.60, px(0.)));
    assert_eq!(value(&v, cx), 60.0.into());
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn dense_marks_leave_free_space_at_different_widths_and_in_both_directions(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::new(0., 100.).step(0.1), 0.);
    cx.update(|w, _| w.set_rem_size(px(16.)));
    for direction in [SliderLayout::Horizontal, SliderLayout::Vertical] {
        for width in [240., 480.] {
            v.update(cx, |v, cx| {
                v.direction = direction;
                v.width = Some(px(width));
                v.parent = px(width);
                v.custom_marks = vec![SliderMark::new(20., "20"), SliderMark::new(21., "21")];
                cx.notify();
            });
            cx.update(|w, cx| {
                let state = v.read(cx).state.clone();
                state.update(cx, |s, cx| s.set_value(0., w, cx));
            });
            draw(cx);
            let thumb = node(cx, "数值").bounds();
            let span = if direction == SliderLayout::Horizontal {
                frame(cx).size.width - thumb.size.width
            } else {
                px(180.) - thumb.size.height
            };
            let at = |p| {
                if direction == SliderLayout::Horizontal {
                    thumb.center() + point(span * p, px(0.))
                } else {
                    thumb.center() - point(px(0.), span * p)
                }
            };
            down(cx, thumb.center());
            moving(cx, at(0.201));
            assert_eq!(
                value(&v, cx),
                20.0.into(),
                "near mark {direction:?}/{width}"
            );
            moving(cx, at(0.205));
            assert_eq!(
                value(&v, cx),
                20.5.into(),
                "dense midpoint stays free {direction:?}/{width}"
            );
            moving(cx, at(0.211));
            assert_eq!(value(&v, cx), 21.0.into());
            up(cx, at(0.4));
            assert_eq!(value(&v, cx), 40.0.into());
        }
    }
    assert_eq!(commits(&v, cx), 4);
}

fn stepper_fill(cx: &mut VisualTestContext, bounds: Bounds<Pixels>) -> Option<gpui_kit::Hsla> {
    cx.update(|w, _| {
        let bounds = bounds.scale(w.scale_factor());
        w.painted_quads()
            .into_iter()
            .find(|quad| quad.bounds == bounds)
            .and_then(|quad| quad.background.as_solid())
    })
}

#[gpui_kit::test]
fn numeric_steppers_show_hover_and_press_feedback_without_layout_or_value_flicker(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    for (id, result) in [("increment", 41.), ("decrement", 40.)] {
        let button = cx.update(|w, _| {
            snapshots(w)
                .into_iter()
                .find(|n| n.path().last() == Some(&ElementId::from(id)))
                .unwrap()
                .bounds()
        });
        let field = node(cx, "数值 value").bounds();
        cx.simulate_mouse_move(point(px(1.), px(1.)), None, Modifiers::default());
        draw(cx);
        assert!(stepper_fill(cx, button).is_none_or(|color| color.a == 0.));
        cx.simulate_mouse_move(button.center(), None, Modifiers::default());
        draw(cx);
        let hover = stepper_fill(cx, button).expect("visible hover fill");
        assert!(hover.a > 0.);
        let before = value(&v, cx);
        down(cx, button.center());
        let pressed = stepper_fill(cx, button).expect("visible pressed fill");
        assert!(pressed.a > hover.a, "click has a stronger short feedback");
        assert_eq!(value(&v, cx), before, "pointer down does not double-step");
        assert_eq!(node(cx, "数值 value").bounds(), field);
        draw(cx);
        assert_eq!(
            stepper_fill(cx, button),
            Some(pressed),
            "held press stays stable"
        );
        up(cx, button.center());
        assert_eq!(
            stepper_fill(cx, button),
            Some(hover),
            "release restores hover"
        );
        assert_eq!(value(&v, cx), result.into());
        assert_eq!(node(cx, "数值 value").bounds(), field);
        assert_eq!(node(cx, "数值 value").focused(), Some(true));
    }
    assert_eq!(commits(&v, cx), 2);
    cx.update(|w, cx| {
        let state = v.read(cx).state.clone();
        state.update(cx, |s, cx| s.set_disabled(true, w, cx));
    });
    draw(cx);
    let button = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&ElementId::from("increment")))
            .unwrap()
            .bounds()
    });
    cx.simulate_mouse_move(button.center(), None, Modifiers::default());
    down(cx, button.center());
    assert!(stepper_fill(cx, button).is_none_or(|color| color.a == 0.));
    up(cx, button.center());
    assert_eq!(value(&v, cx), 40.0.into());
    assert_eq!(commits(&v, cx), 2);
}

#[gpui_kit::test]
fn numeric_stepper_keeps_editor_focus_through_pointer_down_and_release(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    click(cx, "数值 value");
    assert_eq!(node(cx, "数值 value").focused(), Some(true));
    let increment = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&ElementId::from("increment")))
            .unwrap()
            .bounds()
    });
    let field = node(cx, "数值 value").bounds();
    cx.simulate_mouse_move(increment.center(), None, Modifiers::default());
    draw(cx);
    down(cx, increment.center());
    assert_eq!(
        node(cx, "数值 value").focused(),
        Some(true),
        "pressing the chevron must not blur the editor"
    );
    assert_eq!(node(cx, "数值 value").bounds(), field);
    up(cx, increment.center());
    assert_eq!(node(cx, "数值 value").focused(), Some(true));
    assert_eq!(value(&v, cx), 41.0.into());
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn button_controls_include_a_plain_numeric_editor_after_the_plus(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    let field = node(cx, "数值 value").bounds();
    let plus = node(cx, "数值 increase").bounds();
    assert!(field.left() >= plus.right());
    cx.update(|w, _| {
        assert!(!snapshots(w).iter().any(|n| {
            matches!(n.path().last(), Some(id) if id == &ElementId::from("increment") || id == &ElementId::from("decrement"))
        }));
    });
    replace(cx, "数值 value", "75");
    assert_eq!(value(&v, cx), 75.0.into());
    key(cx, "up");
    assert_eq!(value(&v, cx), 76.0.into());
}

#[gpui_kit::test]
fn marked_range_fields_align_with_the_track_instead_of_the_label_row(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (20., 80.));
    for controls in [SliderControls::Input, SliderControls::Buttons] {
        for size in [SliderSize::Small, SliderSize::Medium, SliderSize::Large] {
            v.update(cx, |v, cx| {
                v.controls = controls;
                v.size = size;
                v.marks = true;
                v.width = Some(px(500.));
                cx.notify();
            });
            draw(cx);
            let track_y = node(cx, "上限").bounds().center().y;
            for label in ["下限 value", "上限 value"] {
                assert!(
                    (node(cx, label).bounds().center().y - track_y).abs() < px(0.5),
                    "{controls:?}/{size:?}: numeric fields align with the rail, not rail + labels"
                );
            }
        }
    }
}

#[gpui_kit::test]
fn numeric_updates_animate_geometry_without_delaying_values_or_events(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 0.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    let origin = node(cx, "数值").bounds().center();
    replace(cx, "数值 value", "80");
    assert_eq!(value(&v, cx), 80.0.into(), "canonical value is immediate");
    let start = node(cx, "数值").bounds().center();
    assert!(
        (start.x - origin.x).abs() < px(0.5),
        "the painted thumb starts from its current position"
    );
    tick(cx, 40);
    let middle = node(cx, "数值").bounds().center();
    tick(cx, 500);
    let end = node(cx, "数值").bounds().center();
    assert!(
        middle.x > origin.x && middle.x < end.x,
        "an intermediate frame exists"
    );
    assert_eq!(value(&v, cx), 80.0.into());
    assert_eq!(
        commits(&v, cx),
        0,
        "animation frames cannot submit the edit"
    );
}

#[gpui_kit::test]
fn grabbing_an_animating_thumb_takes_over_its_visible_position(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 0.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "80");
    tick(cx, 40);
    let p = node(cx, "数值").bounds().center();
    let grab = p + point(px(4.), px(0.));
    down(cx, grab);
    assert!(
        (node(cx, "数值").bounds().center().x - p.x).abs() < px(2.),
        "press takes over the displayed thumb, not its old numeric destination"
    );
    let held = node(cx, "数值").bounds().center();
    moving(cx, grab + point(px(20.), px(0.)));
    let moved = node(cx, "数值").bounds().center();
    assert!(
        (moved.x - held.x - px(20.)).abs() < px(3.),
        "drag follows without lag"
    );
    tick(cx, 500);
    assert_eq!(node(cx, "数值").bounds().center(), moved);
    up(cx, grab + point(px(20.), px(0.)));
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn new_numeric_targets_reverse_from_the_current_visible_position(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 0.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    let origin = node(cx, "数值").bounds().center();
    replace(cx, "数值 value", "80");
    tick(cx, 40);
    let before = node(cx, "数值").bounds().center();
    replace(cx, "数值 value", "0");
    assert_eq!(
        node(cx, "数值").bounds().center(),
        before,
        "reversal is continuous"
    );
    tick(cx, 20);
    let middle = node(cx, "数值").bounds().center();
    assert!(middle.x < before.x && middle.x > origin.x);
    tick(cx, 500);
    assert_eq!(node(cx, "数值").bounds().center(), origin);
    assert_eq!(value(&v, cx), 0.0.into());
    assert_eq!(commits(&v, cx), 0);
}

#[gpui_kit::test]
fn range_fill_and_both_thumbs_share_the_same_animation_sample(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (20., 80.));
    for direction in [SliderLayout::Horizontal, SliderLayout::Vertical] {
        v.update(cx, |v, cx| {
            v.controls = SliderControls::Input;
            v.direction = direction;
            v.width = Some(px(500.));
            cx.notify();
        });
        cx.update(|w, cx| {
            v.read(cx)
                .state
                .clone()
                .update(cx, |s, cx| s.set_value((20., 80.), w, cx))
        });
        draw(cx);
        let coordinate = |p: Point<Pixels>| {
            if direction == SliderLayout::Horizontal {
                p.x
            } else {
                -p.y
            }
        };
        let initial_lower = coordinate(node(cx, "下限").bounds().center());
        let initial_upper = coordinate(node(cx, "上限").bounds().center());
        replace(cx, "下限 value", "40");
        replace(cx, "上限 value", "90");
        assert_eq!(value(&v, cx), (40., 90.).into());
        tick(cx, 40);
        let lower = node(cx, "下限").bounds().center();
        let upper = node(cx, "上限").bounds().center();
        assert!(coordinate(lower) > initial_lower);
        assert!(coordinate(upper) > initial_upper);
        assert!(coordinate(lower) < coordinate(upper));
        let fill_color: gpui_kit::Hsla = theme()
            .token("--slider-fill-background")
            .unwrap()
            .rgba()
            .unwrap()
            .into();
        cx.update(|w, _| {
            let scale = w.scale_factor();
            let fill = w
                .painted_quads()
                .into_iter()
                .find(|q| q.background.as_solid() == Some(fill_color))
                .expect("selected fill submitted to scene")
                .bounds;
            let (a, b) = if direction == SliderLayout::Horizontal {
                (fill.left(), fill.right())
            } else {
                (fill.bottom(), fill.top())
            };
            let (expected_a, expected_b) = if direction == SliderLayout::Horizontal {
                (lower.x, upper.x)
            } else {
                (lower.y, upper.y)
            };
            assert!((a.0 - f32::from(expected_a) * scale).abs() <= 1.);
            assert!((b.0 - f32::from(expected_b) * scale).abs() <= 1.);
        });
        tick(cx, 500);
        assert!(coordinate(node(cx, "下限").bounds().center()) > coordinate(lower));
        assert!(coordinate(node(cx, "上限").bounds().center()) > coordinate(upper));
    }
}

#[gpui_kit::test]
fn reduced_motion_finishes_numeric_animation_immediately(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 0.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    let origin = node(cx, "数值").bounds().center();
    replace(cx, "数值 value", "80");
    tick(cx, 40);
    let middle = node(cx, "数值").bounds().center();
    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    let end = node(cx, "数值").bounds().center();
    assert!(end.x > middle.x);
    replace(cx, "数值 value", "0");
    assert_eq!(node(cx, "数值").bounds().center(), origin);
    tick(cx, 500);
    assert_eq!(node(cx, "数值").bounds().center(), origin);
    assert_eq!(commits(&v, cx), 0);
}

#[gpui_kit::test]
fn silent_replacement_disable_domain_and_mode_changes_interrupt_old_animation(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 0.);
    for cause in 0..4 {
        cx.update(|w, cx| {
            v.read(cx).state.clone().update(cx, |s, cx| {
                s.set_disabled(false, w, cx);
                s.set_domain(SliderDomain::default(), w, cx);
                s.set_value(0., w, cx);
            })
        });
        v.update(cx, |v, cx| {
            v.controls = SliderControls::Buttons;
            cx.notify();
        });
        draw(cx);
        let origin = node(cx, "数值").bounds().center();
        replace(cx, "数值 value", "80");
        tick(cx, 40);
        let middle = node(cx, "数值").bounds().center();
        if cause == 3 {
            v.update(cx, |v, cx| {
                v.controls = SliderControls::Input;
                cx.notify();
            });
        } else {
            cx.update(|w, cx| {
                v.read(cx).state.clone().update(cx, |s, cx| match cause {
                    0 => s.set_value(0., w, cx),
                    1 => s.set_disabled(true, w, cx),
                    _ => s.set_domain(SliderDomain::new(0., 200.), w, cx),
                })
            });
        }
        draw(cx);
        let interrupted = node(cx, "数值").bounds().center();
        assert_ne!(interrupted, middle);
        if cause == 0 {
            assert_eq!(interrupted, origin);
        }
        tick(cx, 500);
        assert_eq!(
            node(cx, "数值").bounds().center(),
            interrupted,
            "old animation cannot resume after cancellation {cause}"
        );
        assert_eq!(commits(&v, cx), 0);
    }
}

#[gpui_kit::test]
fn touch_takes_over_an_animated_vertical_thumb_without_a_destination_jump(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 0.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        v.direction = SliderLayout::Vertical;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "80");
    tick(cx, 40);
    let p = node(cx, "数值").bounds().center();
    touch(cx, gpui_kit::TouchPhase::Started, p);
    assert!((node(cx, "数值").bounds().center().y - p.y).abs() < px(2.));
    touch(cx, gpui_kit::TouchPhase::Moved, p - point(px(0.), px(20.)));
    assert!((node(cx, "数值").bounds().center().y - p.y + px(20.)).abs() < px(2.));
    touch(cx, gpui_kit::TouchPhase::Ended, p - point(px(0.), px(20.)));
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn numeric_edits_update_the_track_immediately_and_reject_out_of_range_drafts(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "75");
    assert_eq!(
        value(&v, cx),
        75.0.into(),
        "valid typing updates without Enter"
    );
    assert_eq!(commits(&v, cx), 0, "typing is a live edit, not a commit");
    replace(cx, "数值 value", "101");
    assert_eq!(
        value(&v, cx),
        10.0.into(),
        "101 is typed character by character: 10 is the last valid prefix"
    );
    assert_eq!(node(cx, "数值 value").value(), Some("101"));
    assert_eq!(node(cx, "数值 value").description(), Some("输入有效数字"));
    replace(cx, "数值 value", "85");
    assert_eq!(value(&v, cx), 85.0.into());
    assert_eq!(node(cx, "数值 value").description(), Some("测试调节"));
    key(cx, "enter");
    assert_eq!(commits(&v, cx), 1);
    key(cx, "enter");
    assert_eq!(
        commits(&v, cx),
        1,
        "Enter cannot duplicate a completed edit"
    );
}

#[gpui_kit::test]
fn numeric_fields_keep_horizontal_controls_and_a_readable_editor_at_every_size(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::new(-100., 100.).step(0.1), -99.9);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    for rem in [16., 20., 24.] {
        cx.update(|w, _| w.set_rem_size(px(rem)));
        for size in [SliderSize::Small, SliderSize::Medium, SliderSize::Large] {
            v.update(cx, |v, cx| {
                v.size = size;
                cx.notify();
            });
            draw(cx);
            let field = node(cx, "数值 value").bounds();
            let (decrement, increment) = cx.update(|w, _| {
                let nodes = snapshots(w);
                let find = |id| {
                    nodes
                        .iter()
                        .find(|n| n.path().last() == Some(&ElementId::from(id)))
                        .unwrap()
                        .bounds()
                };
                (find("decrement"), find("increment"))
            });
            assert!(
                (decrement.center().y - increment.center().y).abs() < px(0.5),
                "{size:?}/{rem}: both controls must be on the same row"
            );
            assert!(
                decrement.right() < increment.left(),
                "controls leave a separate editor region"
            );
            let font = match size {
                SliderSize::Small => 12.,
                SliderSize::Medium => 14.,
                SliderSize::Large => 16.,
            } * rem
                / 16.;
            let content = measure_numeric_text(cx, "-999.9", px(font));
            let expected =
                px(f32::from(content + px(rem / 16.)).ceil()) + px(10. * rem / 16.) + px(font * 2.);
            assert!(
                (field.size.width - expected).abs() < px(0.5),
                "{size:?}/{rem}: {field:?} / {expected:?}"
            );
            assert!(
                increment.left() - decrement.right() >= content,
                "{size:?}/{rem}: signed decimals fit the measured editor"
            );
            assert_eq!(node(cx, "数值 value").value(), Some("-99.9"));
            assert_eq!(value(&v, cx), (-99.9).into());
        }
    }
}

fn measure_numeric_text(cx: &mut VisualTestContext, text: &str, size: Pixels) -> Pixels {
    cx.update(|w, _| {
        w.text_system()
            .shape_line(
                text.to_owned().into(),
                size,
                &[gpui_kit::TextRun {
                    len: text.len(),
                    font: w.text_style().font(),
                    ..Default::default()
                }],
                None,
            )
            .width()
    })
}

#[gpui_kit::test]
fn numeric_width_fits_all_three_digit_values_and_adds_only_the_visible_controls(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 100.);
    for rem in [16., 20., 24.] {
        cx.update(|w, _| w.set_rem_size(px(rem)));
        for (size, font) in [
            (SliderSize::Small, 12.),
            (SliderSize::Medium, 14.),
            (SliderSize::Large, 16.),
        ] {
            let font = px(font * rem / 16.);
            let widest = (0..1000)
                .map(|n| measure_numeric_text(cx, &format!("{n:03}"), font))
                .fold(px(0.), Pixels::max);
            let padding_and_border = px(10. * rem / 16.);
            let text_and_caret = px(f32::from(widest + px(rem / 16.)).ceil());
            for controls in [SliderControls::Input, SliderControls::Buttons] {
                v.update(cx, |v, cx| {
                    v.size = size;
                    v.controls = controls;
                    cx.notify();
                });
                draw(cx);
                let field = node(cx, "数值 value").bounds();
                let icons = if controls == SliderControls::Input {
                    font * 2.
                } else {
                    px(0.)
                };
                let expected = text_and_caret + padding_and_border + icons;
                assert!(
                    (field.size.width - expected).abs() < px(0.5),
                    "{size:?}/{rem}/{controls:?}: fits 000..999 with no surplus: {field:?} vs {expected:?}"
                );
                assert!(field.size.width - padding_and_border - icons >= widest);
                println!(
                    "numeric-width rem={rem} size={size:?} mode={controls:?} digits={} field={}",
                    f32::from(widest),
                    f32::from(field.size.width)
                );
                if controls == SliderControls::Buttons {
                    let width = field.size.width;
                    replace(cx, "数值 value", "999999");
                    assert_eq!(
                        node(cx, "数值 value").bounds().size.width,
                        width,
                        "out-of-range text scrolls inside the editor without resizing the row"
                    );
                    assert_eq!(node(cx, "数值 value").description(), Some("输入有效数字"));
                    key(cx, "escape");
                }
            }
        }
    }
}

#[gpui_kit::test]
fn numeric_range_fields_wrap_without_overflowing_narrow_slider_frames(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::new(-100., 100.).step(0.1), (-99.9, 99.9));
    cx.update(|w, _| w.set_rem_size(px(16.)));
    for direction in [SliderLayout::Horizontal, SliderLayout::Vertical] {
        v.update(cx, |v, cx| {
            v.controls = SliderControls::Input;
            v.direction = direction;
            v.width = Some(px(180.));
            v.parent = px(180.);
            cx.notify();
        });
        draw(cx);
        let outer = frame(cx);
        assert!(
            outer.size.width <= px(180.),
            "{direction:?}: frame stays inside its parent"
        );
        for label in ["下限 value", "上限 value"] {
            let field = node(cx, label).bounds();
            assert!(
                field.left() >= outer.left() && field.right() <= outer.right(),
                "{direction:?}: field stays inside the frame horizontally: {field:?} / {outer:?}"
            );
            assert!(
                field.top() >= outer.top() && field.bottom() <= outer.bottom(),
                "{direction:?}: field stays inside the frame vertically: {field:?} / {outer:?}"
            );
            assert!(
                field.size.width < px(96.),
                "signed decimal fields are content-sized"
            );
        }
    }
    assert_eq!(value(&v, cx), (-99.9, 99.9).into());
}

#[gpui_kit::test]
fn end_buttons_share_numeric_steps_keyboard_activation_and_commit_once(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::new(3., 13.).step(2.), 7.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        v.echo = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        cx.update(|w, _| snapshots(w)
            .iter()
            .filter(|n| n.role() == Some(Role::SpinButton))
            .count()),
        1
    );
    let plus = node(cx, "数值 increase").bounds().center();
    down(cx, plus);
    assert_eq!(value(&v, cx), 7.0.into());
    assert_eq!(commits(&v, cx), 0);
    up(cx, plus);
    assert_eq!(value(&v, cx), 9.0.into());
    assert_eq!(commits(&v, cx), 1);
    assert_eq!(
        node(cx, "数值 increase").focused(),
        Some(true),
        "the button retains keyboard focus"
    );
    key(cx, "enter");
    assert_eq!(value(&v, cx), 11.0.into());
    key(cx, "space");
    assert_eq!(value(&v, cx), 13.0.into());
    assert_eq!(commits(&v, cx), 3);
    assert_eq!(node(cx, "数值 increase").disabled(), Some(true));
    click(cx, "数值 increase");
    assert_eq!(commits(&v, cx), 3, "bounded increment is inert");
    click(cx, "数值 decrease");
    assert_eq!(value(&v, cx), 11.0.into());
    assert_eq!(commits(&v, cx), 4);
    cx.update(|w, cx| {
        let state = v.read(cx).state.clone();
        state.update(cx, |s, cx| s.set_disabled(true, w, cx));
    });
    draw(cx);
    assert_eq!(node(cx, "数值 decrease").disabled(), Some(true));
    assert_eq!(node(cx, "数值 increase").disabled(), Some(true));
    click(cx, "数值 decrease");
    assert_eq!(value(&v, cx), 11.0.into());
    assert_eq!(commits(&v, cx), 4);
}

#[gpui_kit::test]
fn end_button_tab_order_follows_the_track_and_respects_tab_stop(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    click(cx, "前一个");
    for label in [
        "数值 decrease",
        "数值",
        "数值 increase",
        "数值 value",
        "后一个",
    ] {
        key(cx, "tab");
        assert_eq!(node(cx, label).focused(), Some(true), "Tab reaches {label}");
    }
    v.update(cx, |v, cx| {
        v.skip = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "前一个");
    key(cx, "tab");
    assert_eq!(node(cx, "后一个").focused(), Some(true));
}

#[gpui_kit::test]
fn switching_control_forms_cancels_a_held_drag_without_a_stale_commit(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    let origin = node(cx, "数值").bounds().center();
    down(cx, origin);
    moving(cx, origin + point(px(20.), px(0.)));
    let preview = value(&v, cx);
    assert!(preview.end() > 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    up(cx, origin + point(px(200.), px(0.)));
    assert_eq!(value(&v, cx), preview);
    assert_eq!(commits(&v, cx), 0);
    assert_eq!(
        node(cx, "数值 value").value(),
        Some(preview.end().to_string().as_str())
    );
}

#[gpui_kit::test]
fn control_forms_exclude_duplicate_arrows_and_switching_cancels_live_edits(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "80");
    assert_eq!(value(&v, cx), 80.0.into());
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        value(&v, cx),
        80.0.into(),
        "changing form preserves live values without submitting the old transaction"
    );
    assert_eq!(commits(&v, cx), 0);
    assert_eq!(node(cx, "数值").focused(), Some(true));
    assert_eq!(
        cx.update(|w, _| snapshots(w)
            .iter()
            .filter(|n| n.role() == Some(Role::SpinButton))
            .count()),
        1
    );
    click(cx, "数值 increase");
    assert_eq!(
        value(&v, cx),
        81.0.into(),
        "buttons step the current canonical live value"
    );
    v.update(cx, |v, cx| {
        v.controls = SliderControls::None;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "数值").focused(), Some(true));
    cx.update(|w, _| {
        let nodes = snapshots(w);
        assert!(!nodes.iter().any(|n| {
            n.role() == Some(Role::SpinButton)
                || n.label().is_some_and(|label| {
                    label.ends_with(" increase") || label.ends_with(" decrease")
                })
        }));
    });
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "数值 value").value(), Some("81"));
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn range_end_buttons_adjust_the_last_operated_endpoint_without_crossing(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (20., 80.));
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        v.marks = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "上限 increase");
    assert_eq!(
        value(&v, cx),
        (20., 81.).into(),
        "upper endpoint is the initial target and uses step"
    );
    focus(&v, true, cx);
    key(cx, "right");
    assert_eq!(value(&v, cx), (21., 81.).into());
    click(cx, "下限 increase");
    assert_eq!(value(&v, cx), (22., 81.).into());
    cx.update(|w, cx| {
        let state = v.read(cx).state.clone();
        state.update(cx, |s, cx| s.set_value((81., 81.), w, cx));
    });
    draw(cx);
    assert_eq!(node(cx, "下限 increase").disabled(), Some(true));
    click(cx, "下限 increase");
    assert_eq!(value(&v, cx), (81., 81.).into());
    click(cx, "下限 decrease");
    assert_eq!(value(&v, cx), (80., 81.).into());
    assert_eq!(commits(&v, cx), 4);
}

#[gpui_kit::test]
fn end_buttons_follow_axis_and_size_and_preserve_mark_geometry_and_dragging(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 0.);
    cx.update(|w, _| w.set_rem_size(px(16.)));
    for direction in [SliderLayout::Horizontal, SliderLayout::Vertical] {
        for (size, hit) in [
            (SliderSize::Small, 28.),
            (SliderSize::Medium, 32.),
            (SliderSize::Large, 40.),
        ] {
            v.update(cx, |v, cx| {
                v.controls = SliderControls::Buttons;
                v.direction = direction;
                v.size = size;
                v.marks = true;
                cx.notify();
            });
            cx.update(|w, cx| {
                let state = v.read(cx).state.clone();
                state.update(cx, |s, cx| s.set_value(0., w, cx));
            });
            draw(cx);
            let minus = node(cx, "数值 decrease").bounds();
            let plus = node(cx, "数值 increase").bounds();
            let thumb = node(cx, "数值").bounds();
            assert_eq!(minus.size.width, px(hit));
            assert_eq!(plus.size.height, px(hit));
            let outer = frame(cx);
            assert!(outer.contains(&minus.center()) && outer.contains(&plus.center()));
            if direction == SliderLayout::Horizontal {
                assert!(minus.right() <= thumb.left() && plus.left() >= thumb.right());
                assert!(
                    (thumb.left() - minus.right()).abs() < px(0.5),
                    "no duplicate gap between button and thumb hitboxes"
                );
                assert!((minus.center().y - thumb.center().y).abs() < px(0.5));
            } else {
                assert!(
                    plus.bottom() < thumb.top() && minus.top() >= thumb.bottom(),
                    "plus is at the vertical maximum/top"
                );
                assert!((minus.center().x - thumb.center().x).abs() < px(0.5));
                assert!((minus.top() - thumb.bottom()).abs() < px(0.5));
            }
            // Marks and pointer mapping must still agree after adding end controls.
            down(cx, thumb.center());
            let labels = cx.update(|w, _| {
                snapshots(w)
                    .into_iter()
                    .find(|n| {
                        n.path().last()
                            == Some(&ElementId::from((ElementId::from("subject"), "labels")))
                    })
                    .expect("mark lane follows the value track")
                    .bounds()
            });
            let target = if direction == SliderLayout::Horizontal {
                assert!((labels.left() - thumb.center().x).abs() < px(0.5));
                point(labels.left() + labels.size.width * 0.2, thumb.center().y)
            } else {
                assert!((labels.bottom() - thumb.center().y).abs() < px(0.5));
                point(thumb.center().x, labels.bottom() - labels.size.height * 0.2)
            };
            moving(cx, target);
            assert_eq!(
                value(&v, cx),
                20.0.into(),
                "labels and pointer use the same mapping"
            );
            up(cx, plus.center());
            assert_eq!(
                value(&v, cx),
                100.0.into(),
                "outside-track release still reaches max"
            );
            let end = node(cx, "数值").bounds();
            if direction == SliderLayout::Horizontal {
                assert!((plus.left() - end.right()).abs() < px(0.5));
            } else {
                assert!((end.top() - plus.bottom()).abs() < px(0.5));
            }
        }
    }
    assert_eq!(commits(&v, cx), 6);
}

#[gpui_kit::test]
fn mark_snapping_can_be_disabled_and_switching_policy_cancels_a_held_edit(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default().step(3.), 0.);
    v.update(cx, |v, cx| {
        v.marks = true;
        cx.notify();
    });
    draw(cx);
    let origin = node(cx, "数值").bounds().center();
    down(cx, origin);
    let span = frame(cx).size.width - node(cx, "数值").bounds().size.width;
    moving(cx, origin + point(span * 0.2, px(0.)));
    assert_eq!(value(&v, cx), 20.0.into());
    v.update(cx, |v, cx| {
        v.snap_to_marks = false;
        cx.notify();
    });
    draw(cx);
    moving(cx, origin + point(px(200.), px(0.)));
    up(cx, origin + point(px(200.), px(0.)));
    assert_eq!(value(&v, cx), 20.0.into());
    assert_eq!(commits(&v, cx), 0);
    cx.update(|w, cx| {
        let state = v.read(cx).state.clone();
        state.update(cx, |s, cx| s.set_value(20., w, cx));
    });
    draw(cx);
    assert_eq!(
        value(&v, cx),
        20.0.into(),
        "same-value echo after policy change stays silent"
    );
    key(cx, "right");
    assert_eq!(value(&v, cx), 21.0.into());
    assert_eq!(node(cx, "数值").numeric_value_step(), Some(3.));
}

#[gpui_kit::test]
fn hidden_marks_snap_vertical_ranges_without_crossing(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default().step(3.), (0., 100.));
    v.update(cx, |v, cx| {
        v.direction = SliderLayout::Vertical;
        v.custom_marks = vec![
            SliderMark::new(20., "20"),
            SliderMark::new(37., "37").label_visible(false),
        ];
        cx.notify();
    });
    draw(cx);
    let a = node(cx, "下限").bounds().center();
    let b = node(cx, "上限").bounds().center();
    let span = a.y - b.y;
    down(cx, a);
    moving(cx, a - point(px(0.), span * 0.37));
    assert_eq!(value(&v, cx), (37., 100.).into());
    up(cx, a - point(px(0.), span * 0.37));
    focus(&v, false, cx);
    key(cx, "home");
    assert_eq!(value(&v, cx), (37., 37.).into());
    key(cx, "left");
    assert_eq!(value(&v, cx), (37., 37.).into());
    focus(&v, true, cx);
    key(cx, "home");
    key(cx, "right");
    assert_eq!(value(&v, cx), (3., 37.).into());
}

#[gpui_kit::test]
fn tooltip_uses_the_visible_thumb_and_all_sizes_keep_two_pixel_borders(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 50.);
    cx.simulate_scale_factor_change(1.);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    for direction in [SliderLayout::Horizontal, SliderLayout::Vertical] {
        for (size, diameter) in [
            (SliderSize::Small, 12.),
            (SliderSize::Medium, 16.),
            (SliderSize::Large, 20.),
        ] {
            v.update(cx, |v, cx| {
                v.tooltip = true;
                v.direction = direction;
                v.size = size;
                cx.notify();
            });
            draw(cx);
            focus(&v, false, cx);
            tick(cx, 350);
            let center = node(cx, "数值").bounds().center();
            let surface = cx.update(|w, _| {
                let quad = w
                    .painted_quads()
                    .into_iter()
                    .find(|q| {
                        q.bounds.center() == center.scale(w.scale_factor())
                            && q.bounds.size.width == ScaledPixels(diameter)
                            && q.bounds.size.height == ScaledPixels(diameter)
                            && q.border_widths.top > ScaledPixels(0.)
                    })
                    .unwrap();
                assert_eq!(quad.border_widths.top, ScaledPixels(2.));
                assert_eq!(quad.border_widths.bottom, ScaledPixels(2.));
                quad.bounds
            });
            let tooltip = tooltip_nodes(cx);
            assert_eq!(tooltip.len(), 1);
            let bounds = tooltip[0].bounds();
            let gap = if direction == SliderLayout::Horizontal {
                if f32::from(bounds.center().y) < surface.center().y.0 {
                    surface.top().0 - f32::from(bounds.bottom())
                } else {
                    f32::from(bounds.top()) - surface.bottom().0
                }
            } else {
                if f32::from(bounds.center().x) < surface.center().x.0 {
                    surface.left().0 - f32::from(bounds.right())
                } else {
                    f32::from(bounds.left()) - surface.right().0
                }
            };
            assert!(
                (8.0..=12.0).contains(&gap),
                "{direction:?}/{size:?}: visible thumb-to-body gap={gap}"
            );
        }
    }
}

#[gpui_kit::test]
fn changing_marks_preserves_current_values_and_domain_changes_restore_a_valid_grid(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default().step(3.), 40.);
    v.update(cx, |v, cx| {
        v.marks = true;
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    assert_eq!(value(&v, cx), 39.0.into());
    assert_eq!(node(cx, "数值 value").value(), Some("39"));
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
    v.update(cx, |v, cx| {
        v.custom_marks = vec![SliderMark::new(37., "37").label_visible(false)];
        cx.notify();
    });
    draw(cx);
    assert_eq!(value(&v, cx), 39.0.into());
    assert_eq!(node(cx, "数值 value").value(), Some("39"));
    assert!(v.read_with(cx, |v, _| v.events.is_empty()));
    cx.update(|w, cx| {
        let state = v.read(cx).state.clone();
        state.update(cx, |s, cx| {
            s.set_domain(SliderDomain::new(0., 10.).step(3.), w, cx)
        });
    });
    draw(cx);
    assert_eq!(value(&v, cx), 10.0.into());
    focus(&v, false, cx);
    key(cx, "left");
    assert_eq!(value(&v, cx), 9.0.into());
    assert_eq!(node(cx, "数值").numeric_value_step(), Some(3.));
    assert_eq!(node(cx, "数值 value").value(), Some("9"));
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn marked_pointer_between_marks_stays_free_and_outside_release_reaches_the_endpoint(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default().step(3.), 0.);
    v.update(cx, |v, cx| {
        v.marks = true;
        cx.notify();
    });
    draw(cx);
    let thumb = node(cx, "数值").bounds();
    let origin = thumb.center();
    let span = frame(cx).size.width - thumb.size.width;
    down(cx, origin);
    moving(cx, origin + point(span * 0.1, px(0.)));
    assert_eq!(value(&v, cx), 9.0.into());
    moving(cx, origin + point(span * 0.60, px(0.)));
    assert_eq!(value(&v, cx), 60.0.into());
    moving(cx, origin + point(span * 0.59, px(0.)));
    assert_eq!(value(&v, cx), 60.0.into());
    moving(cx, origin - point(px(100.), px(100.)));
    assert_eq!(value(&v, cx), 0.0.into());
    up(cx, origin + point(span + px(100.), px(100.)));
    assert_eq!(value(&v, cx), 100.0.into());
    assert_eq!(commits(&v, cx), 1);
}

#[test]
fn numeric_grid_is_min_anchored_and_keeps_the_last_endpoint() {
    assert_eq!(SliderDomain::new(3., 13.).step(2.).snap(6.), 7.);
    assert_eq!(SliderDomain::new(-5., 5.).step(2.).snap(-2.), -1.);
    let d = SliderDomain::new(0., 10.).step(3.);
    assert_eq!(d.snap(9.8), 10.);
    assert_eq!(d.adjacent(10., false), 9.);
    let d = SliderDomain::new(0., 1.).step(0.1);
    assert_eq!(d.snap(0.26), 0.3);
    assert_eq!(d.adjacent(0.7, true), 0.8);
    assert_eq!(d.adjacent(0.3, false), 0.2);
    assert_eq!(d.snap(0.9), 0.9);
    assert_eq!(d.adjacent(0.8, true), 0.9);
    assert_eq!(d.snap(0.35), 0.4);
    let d = SliderDomain::new(0., 10.).step(20.);
    assert_eq!(d.adjacent(0., true), 10.);
    assert_eq!(d.adjacent(10., false), 0.);
    assert_eq!(d.normalize((30., -5.)), SliderValue::Range(0., 10.));
    let d = SliderDomain::new(0., 10.).step(f32::MAX);
    assert_eq!(d.snap(0.), 0.);
    assert_eq!(d.snap(4.), 0.);
    assert_eq!(d.snap(5.), 10.);
    assert_eq!(d.snap(10.), 10.);
}
#[test]
fn invalid_domains_and_nonfinite_values_are_rejected() {
    for (a, b) in [(0., 0.), (2., 1.), (f32::NAN, 1.), (0., f32::INFINITY)] {
        assert!(std::panic::catch_unwind(|| SliderDomain::new(a, b)).is_err());
    }
    for s in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(std::panic::catch_unwind(|| SliderDomain::default().step(s)).is_err());
    }
    assert!(std::panic::catch_unwind(|| SliderDomain::new(1e10, 2e10).step(0.1)).is_err());
    assert!(
        std::panic::catch_unwind(|| BaseState::new().with_domain(SliderDomain::new(1e10, 2e10)))
            .is_err()
    );
    assert!(std::panic::catch_unwind(|| SliderDomain::default().normalize(f32::NAN)).is_err());
}

#[gpui_kit::test]
fn held_track_drag_keeps_up_outside_and_commits_once_on_release(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    let b = frame(cx);
    let y = node(cx, "数值").bounds().center().y;
    let start = point(b.left() + px(16. + 288. * 0.2), y);
    down(cx, start);
    assert_eq!(value(&v, cx), 20.0.into());
    assert_eq!(commits(&v, cx), 0);
    let outside = point(b.right() + px(90.), y - px(45.));
    moving(cx, outside);
    assert_eq!(value(&v, cx), 100.0.into());
    assert_eq!(commits(&v, cx), 0);
    up(cx, outside);
    assert_eq!(commits(&v, cx), 1);
    moving(cx, start);
    assert_eq!(value(&v, cx), 100.0.into());
    v.read_with(cx, |v, _| {
        assert_eq!(v.events.last(), Some(&YueSliderEvent::Commit(100.0.into())));
    });
}
#[gpui_kit::test]
fn grabbing_the_thumb_edge_has_no_jump_and_does_not_move_siblings(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    let thumb = node(cx, "数值").bounds();
    let before = node(cx, "后一个").bounds();
    let p = thumb.center() + point(px(10.), px(0.));
    down(cx, p);
    moving(cx, p);
    assert_eq!(value(&v, cx), 40.0.into());
    assert_eq!(node(cx, "数值").bounds(), thumb);
    up(cx, p);
    assert_eq!(commits(&v, cx), 0);
    assert_eq!(node(cx, "后一个").bounds(), before);
    down(cx, p);
    moving(cx, p + point(px(28.8), px(0.)));
    up(cx, p + point(px(28.8), px(0.)));
    assert_eq!(value(&v, cx), 50.0.into());
    assert_eq!(commits(&v, cx), 1);
}
#[gpui_kit::test]
fn range_track_drag_clamps_the_selected_endpoint_without_swapping(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (30., 70.));
    let b = frame(cx);
    let y = node(cx, "下限").bounds().center().y;
    down(cx, point(b.left() + px(16. + 288. * 0.1), y));
    assert_eq!(value(&v, cx), (10., 70.).into());
    moving(cx, point(b.right() + px(60.), y));
    assert_eq!(value(&v, cx), (70., 70.).into());
    up(cx, point(b.right() + px(60.), y));
    assert_eq!(commits(&v, cx), 1);
    // Both hitboxes overlap. The last active lower keeps the gesture.
    let p = node(cx, "下限").bounds().center();
    down(cx, p);
    moving(cx, p - point(px(28.8), px(0.)));
    up(cx, p - point(px(28.8), px(0.)));
    assert_eq!(value(&v, cx), (60., 70.).into());
}
#[gpui_kit::test]
fn coincident_first_drag_uses_upper_and_tab_order_remains_lower_upper(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (50., 50.));
    let p = node(cx, "上限").bounds().center();
    down(cx, p);
    moving(cx, p + point(px(28.8), px(0.)));
    up(cx, p + point(px(28.8), px(0.)));
    assert_eq!(value(&v, cx), (50., 60.).into());
    cx.update(|w, cx| v.read(cx).before.clone().focus(w, cx));
    draw(cx);
    key(cx, "tab");
    assert_eq!(
        node(cx, "下限").focused(),
        Some(true),
        "{:?}",
        cx.update(|w, _| snapshots(w))
    );
    key(cx, "tab");
    assert_eq!(node(cx, "上限").focused(), Some(true));
    key(cx, "tab");
    assert_eq!(node(cx, "后一个").focused(), Some(true));
    v.update(cx, |v, cx| {
        v.skip = true;
        cx.notify();
    });
    cx.update(|w, cx| v.read(cx).before.clone().focus(w, cx));
    draw(cx);
    key(cx, "tab");
    assert_eq!(node(cx, "后一个").focused(), Some(true));
}
#[gpui_kit::test]
fn external_replacement_domain_change_and_disable_cancel_old_gestures(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    for mode in 0..3 {
        let p = node(cx, "数值").bounds().center();
        down(cx, p);
        moving(cx, p + point(px(28.8), px(0.)));
        cx.update(|w, cx| {
            let s = v.read(cx).state.clone();
            s.update(cx, |s, cx| match mode {
                0 => s.set_value(25., w, cx),
                1 => s.set_domain(SliderDomain::new(-10., 10.).step(2.), w, cx),
                _ => s.set_disabled(true, w, cx),
            });
        });
        draw(cx);
        let current = value(&v, cx);
        moving(cx, p + point(px(90.), px(0.)));
        up(cx, p + point(px(90.), px(0.)));
        assert_eq!(value(&v, cx), current);
        assert_eq!(commits(&v, cx), 0);
    }
    assert_eq!(node(cx, "数值").disabled(), Some(true));
}
#[gpui_kit::test]
fn same_value_echo_keeps_capture_and_reentrant_replacement_cancels_it(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, _| v.echo = true);
    let p = node(cx, "数值").bounds().center();
    down(cx, p);
    moving(cx, p + point(px(28.8), px(0.)));
    moving(cx, p + point(px(57.6), px(0.)));
    up(cx, p + point(px(57.6), px(0.)));
    assert_eq!(value(&v, cx), 60.0.into());
    assert_eq!(commits(&v, cx), 1);
    v.update(cx, |v, _| {
        v.echo = false;
        v.replace = true;
        v.events.clear();
    });
    let p = node(cx, "数值").bounds().center();
    down(cx, p);
    moving(cx, p + point(px(28.8), px(0.)));
    moving(cx, p + point(px(57.6), px(0.)));
    up(cx, p + point(px(57.6), px(0.)));
    assert_eq!(value(&v, cx), 25.0.into());
    assert_eq!(commits(&v, cx), 0);
}

#[gpui_kit::test]
fn replacement_in_a_final_mouse_up_change_prevents_a_stale_commit(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, _| v.replace = true);
    let p = node(cx, "数值").bounds().center();
    down(cx, p);
    // The platform can report the last movement only in MouseUp. Change and
    // Release then originate in the same Base update, before owner callbacks run.
    up(cx, p + point(px(57.6), px(0.)));
    assert_eq!(value(&v, cx), 25.0.into());
    assert_eq!(commits(&v, cx), 0);
}
#[gpui_kit::test]
fn keyboard_repeat_commits_after_all_adjustment_keys_are_released(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::new(3., 13.).step(2.), 7.);
    focus(&v, false, cx);
    key_down(cx, "right", false);
    key_down(cx, "right", true);
    assert_eq!(value(&v, cx), 11.0.into());
    assert_eq!(commits(&v, cx), 0);
    key_down(cx, "up", false);
    key_up(cx, "right");
    assert_eq!(commits(&v, cx), 0);
    key_up(cx, "up");
    assert_eq!(commits(&v, cx), 1);
    key(cx, "right");
    assert_eq!(commits(&v, cx), 1);
    key(cx, "home");
    assert_eq!(value(&v, cx), 3.0.into());
    key(cx, "pageup");
    assert_eq!(value(&v, cx), 5.0.into());
    key(cx, "end");
    assert_eq!(value(&v, cx), 13.0.into());
    key(cx, "pagedown");
    assert_eq!(value(&v, cx), 11.0.into());
    key(cx, "ctrl-left");
    assert_eq!(value(&v, cx), 11.0.into());
    cx.update(|w, cx| {
        let s = v.read(cx).state.clone();
        s.update(cx, |s, cx| s.set_disabled(true, w, cx));
    });
    draw(cx);
    key(cx, "left");
    assert_eq!(value(&v, cx), 11.0.into());
}
#[gpui_kit::test]
fn range_keyboard_uses_each_thumb_and_keeps_native_roles_unique(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (20., 80.));
    focus(&v, true, cx);
    key(cx, "end");
    assert_eq!(value(&v, cx), (80., 80.).into());
    focus(&v, false, cx);
    key(cx, "home");
    assert_eq!(value(&v, cx), (80., 80.).into());
    key(cx, "end");
    assert_eq!(value(&v, cx), (80., 100.).into());
    assert_eq!(node(cx, "下限").value(), Some("80%"));
    assert_eq!(node(cx, "上限").value(), Some("100%"));
    assert_eq!(node(cx, "下限").min_numeric_value(), Some(0.));
    assert_eq!(node(cx, "下限").max_numeric_value(), Some(100.));
    assert_eq!(node(cx, "上限").min_numeric_value(), Some(80.));
    assert_eq!(node(cx, "上限").max_numeric_value(), Some(100.));
    assert_eq!(node(cx, "下限").numeric_value(), Some(80.));
    assert_eq!(node(cx, "下限").numeric_value_step(), Some(1.));
    assert_eq!(
        node(cx, "下限").orientation(),
        Some(gpui_kit::Orientation::Horizontal)
    );
    assert_eq!(
        cx.update(|w, _| snapshots(w)
            .iter()
            .filter(|s| s.role() == Some(Role::Slider))
            .count()),
        2
    );
}
#[gpui_kit::test]
fn numeric_input_preserves_drafts_enter_commits_once_and_escape_restores(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::new(-10., 10.).step(0.5), 2.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "-");
    assert_eq!(value(&v, cx), 2.0.into());
    key(cx, "enter");
    assert_eq!(value(&v, cx), 2.0.into());
    assert_eq!(node(cx, "数值 value").value(), Some("-"));
    assert_eq!(node(cx, "数值 value").description(), Some("输入有效数字"));
    key(cx, "escape");
    assert_eq!(node(cx, "数值 value").value(), Some("2"));
    replace(cx, "数值 value", "3.3");
    key(cx, "enter");
    assert_eq!(value(&v, cx), 3.5.into());
    assert_eq!(commits(&v, cx), 1);
    cx.update(|w, cx| v.read(cx).after.clone().focus(w, cx));
    draw(cx);
    assert_eq!(commits(&v, cx), 1);
    replace(cx, "数值 value", "-");
    cx.update(|w, cx| v.read(cx).after.clone().focus(w, cx));
    draw(cx);
    assert_eq!(node(cx, "数值 value").value(), Some("3.5"));
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn invalid_live_drafts_restore_the_last_valid_prefix_on_blur_and_commit_once(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    let bounds = node(cx, "数值 value").bounds();
    replace(cx, "数值 value", "101");
    assert_eq!(value(&v, cx), 10.0.into());
    assert_eq!(node(cx, "数值 value").value(), Some("101"));
    assert_eq!(node(cx, "数值 value").description(), Some("输入有效数字"));
    assert_eq!(node(cx, "数值 value").bounds(), bounds);
    let invalid_border: gpui_kit::Hsla = theme()
        .token(yue_component::theme::keys::INPUT_BORDER_COLOR_INVALID)
        .unwrap()
        .rgba()
        .unwrap()
        .into();
    cx.update(|w, _| {
        assert!(
            w.painted_quads()
                .iter()
                .any(|q| q.bounds == bounds.scale(w.scale_factor())
                    && q.border_color == invalid_border),
            "the actual frame paints the invalid border"
        );
    });
    key(cx, "enter");
    assert_eq!(commits(&v, cx), 0);
    click(cx, "后一个");
    assert_eq!(
        value(&v, cx),
        10.0.into(),
        "blur must not apply Base's silent 101 -> 100 clamp"
    );
    assert_eq!(node(cx, "数值 value").value(), Some("10"));
    assert_eq!(node(cx, "数值 value").description(), Some("测试调节"));
    assert_eq!(commits(&v, cx), 1);
    for text in ["-", ".", ""] {
        replace(cx, "数值 value", text);
        assert_eq!(value(&v, cx), 10.0.into());
        key(cx, "enter");
        key(cx, "escape");
        assert_eq!(node(cx, "数值 value").value(), Some("10"));
        assert_eq!(commits(&v, cx), 1);
    }
}

#[gpui_kit::test]
fn invalid_paste_is_atomic_and_a_legal_multidigit_minimum_can_still_be_typed(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::new(10., 100.), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "15");
    assert_eq!(
        value(&v, cx),
        15.0.into(),
        "an out-of-range first digit cannot prevent completing 15"
    );
    key(cx, "ctrl-a");
    cx.update(|_, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("101".into())));
    key(cx, "ctrl-v");
    assert_eq!(node(cx, "数值 value").value(), Some("101"));
    assert_eq!(node(cx, "数值 value").description(), Some("输入有效数字"));
    assert_eq!(
        value(&v, cx),
        15.0.into(),
        "atomic invalid paste keeps the pre-paste valid value"
    );
    key(cx, "escape");
    assert_eq!(node(cx, "数值 value").value(), Some("15"));
    assert_eq!(commits(&v, cx), 1);
}

#[gpui_kit::test]
fn live_edits_keep_the_draft_on_echo_but_external_replacement_cancels_the_transaction(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Buttons;
        v.echo = true;
        cx.notify();
    });
    draw(cx);
    replace(cx, "数值 value", "65");
    assert_eq!(value(&v, cx), 65.0.into());
    assert_eq!(node(cx, "数值 value").value(), Some("65"));
    assert_eq!(commits(&v, cx), 0);
    click(cx, "后一个");
    assert_eq!(commits(&v, cx), 1);
    v.update(cx, |v, cx| {
        v.replace = true;
        cx.notify();
    });
    replace(cx, "数值 value", "9");
    assert_eq!(value(&v, cx), 25.0.into());
    assert_eq!(node(cx, "数值 value").value(), Some("25"));
    key(cx, "enter");
    assert_eq!(
        commits(&v, cx),
        1,
        "owner replacement prevents stale input Commit"
    );
}

#[gpui_kit::test]
fn step_buttons_and_arrow_actions_use_the_same_nondivisible_grid(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::new(0., 10.).step(3.), 9.);
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    click(cx, "数值 value");
    key(cx, "up");
    assert_eq!(value(&v, cx), 10.0.into());
    assert_eq!(commits(&v, cx), 1);
    key(cx, "down");
    assert_eq!(value(&v, cx), 9.0.into());
    assert_eq!(commits(&v, cx), 2);
    let p = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|s| s.path().last() == Some(&ElementId::from("decrement")))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
    assert_eq!(value(&v, cx), 6.0.into());
    assert_eq!(commits(&v, cx), 3);
    cx.update(|w, cx| {
        let s = v.read(cx).state.clone();
        s.update(cx, |s, cx| s.set_disabled(true, w, cx));
    });
    draw(cx);
    key(cx, "up");
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
    assert_eq!(value(&v, cx), 6.0.into());
    assert_eq!(commits(&v, cx), 3);
}

fn tooltip_nodes(cx: &mut VisualTestContext) -> Vec<ElementSnapshot> {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .filter(|n| n.role() == Some(Role::Tooltip))
            .collect()
    })
}
fn tick(cx: &mut VisualTestContext, ms: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(ms));
    draw(cx);
    cx.run_until_parked();
    draw(cx);
}
#[gpui_kit::test]
fn tooltip_delays_hover_tracks_dragged_value_and_escape_stays_closed(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    v.update(cx, |v, cx| {
        v.tooltip = true;
        cx.notify();
    });
    draw(cx);
    let p = node(cx, "数值").bounds().center();
    cx.simulate_mouse_move(p, None, Modifiers::default());
    draw(cx);
    assert!(tooltip_nodes(cx).is_empty());
    tick(cx, 200);
    assert!(tooltip_nodes(cx).is_empty());
    tick(cx, 150);
    assert_eq!(tooltip_nodes(cx).len(), 1);
    down(cx, p);
    moving(cx, p + point(px(28.8), px(0.)));
    assert_eq!(tooltip_nodes(cx).len(), 1);
    assert_eq!(tooltip_nodes(cx)[0].label(), Some("50%"));
    key(cx, "escape");
    draw(cx);
    assert!(tooltip_nodes(cx).is_empty());
    tick(cx, 400);
    assert!(tooltip_nodes(cx).is_empty());
    moving(cx, p + point(px(57.6), px(0.)));
    tick(cx, 400);
    assert!(tooltip_nodes(cx).is_empty());
    up(cx, p + point(px(57.6), px(0.)));
    assert_eq!(commits(&v, cx), 1);
}
#[gpui_kit::test]
fn keyboard_tooltip_is_immediate_and_window_deactivation_cancels_drag(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    v.update(cx, |v, cx| {
        v.tooltip = true;
        cx.notify();
    });
    draw(cx);
    key(cx, "tab");
    focus(&v, false, cx);
    draw(cx);
    assert_eq!(tooltip_nodes(cx).len(), 1);
    let p = node(cx, "数值").bounds().center();
    down(cx, p);
    moving(cx, p + point(px(28.8), px(0.)));
    cx.deactivate_window();
    draw(cx);
    assert!(tooltip_nodes(cx).is_empty());
    up(cx, p + point(px(57.6), px(0.)));
    assert_eq!(value(&v, cx), 50.0.into());
    assert_eq!(commits(&v, cx), 0);
}

fn touch(cx: &mut VisualTestContext, phase: gpui_kit::TouchPhase, position: Point<Pixels>) {
    cx.simulate_event(gpui_kit::TouchEvent {
        id: gpui_kit::TouchId(1),
        phase,
        position,
        ..Default::default()
    });
    draw(cx);
}
#[gpui_kit::test]
fn touch_drag_cancel_preserves_last_preview_without_committing(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    let p = node(cx, "数值").bounds().center();
    touch(cx, gpui_kit::TouchPhase::Started, p);
    touch(cx, gpui_kit::TouchPhase::Moved, p + point(px(28.8), px(0.)));
    assert_eq!(value(&v, cx), 50.0.into());
    touch(
        cx,
        gpui_kit::TouchPhase::Cancelled,
        p + point(px(57.6), px(0.)),
    );
    assert_eq!(value(&v, cx), 50.0.into());
    assert_eq!(commits(&v, cx), 0);
    touch(cx, gpui_kit::TouchPhase::Ended, p + point(px(86.4), px(0.)));
    assert_eq!(value(&v, cx), 50.0.into());
    assert_eq!(commits(&v, cx), 0);
}
#[gpui_kit::test]
fn dragging_discards_old_numeric_draft_and_range_inputs_do_not_cross(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (20., 80.));
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        cx.notify();
    });
    draw(cx);
    // Endpoint names can be shared with SpinButtons; choose by role below.
    let mut fields = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .filter(|s| s.role() == Some(Role::SpinButton))
            .collect::<Vec<_>>()
    });
    fields.sort_by(|a, b| a.bounds().left().partial_cmp(&b.bounds().left()).unwrap());
    assert_eq!(fields.len(), 2);
    cx.simulate_click(fields[0].bounds().center(), Modifiers::default());
    draw(cx);
    key(cx, "ctrl-a");
    cx.simulate_input("99");
    draw(cx);
    key(cx, "enter");
    assert_eq!(value(&v, cx), (9., 80.).into());
    assert_eq!(node(cx, "下限 value").value(), Some("99"));
    assert_eq!(node(cx, "下限 value").description(), Some("输入有效数字"));
    assert_eq!(
        commits(&v, cx),
        0,
        "Enter cannot accept an out-of-range draft"
    );
    cx.update(|w, cx| {
        v.read(cx)
            .state
            .clone()
            .update(cx, |s, cx| s.set_value((20., 80.), w, cx));
    });
    draw(cx);
    // A legal lower edit is live before starting an upper-thumb gesture.
    cx.simulate_click(fields[0].bounds().center(), Modifiers::default());
    draw(cx);
    key(cx, "ctrl-a");
    cx.simulate_input("5");
    draw(cx);
    assert_eq!(node(cx, "下限 value").value(), Some("5"));
    assert_eq!(value(&v, cx), (5., 80.).into());
    let p = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|s| s.role() == Some(Role::Slider) && s.label() == Some("上限"))
            .unwrap()
            .bounds()
            .center()
    });
    down(cx, p);
    assert_eq!(value(&v, cx).start(), 5.);
    assert_eq!(node(cx, "下限 value").value(), Some("5"));
    moving(cx, p + point(px(13.6), px(0.)));
    up(cx, p + point(px(13.6), px(0.)));
    assert!(value(&v, cx).end() > 80.);
    let final_value = value(&v, cx);
    assert_eq!(final_value.start(), 5.);
    cx.update(|w, cx| v.read(cx).after.clone().focus(w, cx));
    draw(cx);
    assert_eq!(value(&v, cx), final_value);
    assert_eq!(
        commits(&v, cx),
        1,
        "pointer takeover completes the live transaction on release"
    );
}
#[gpui_kit::test]
fn fixed_block_narrow_and_vertical_layouts_keep_a_real_mapping(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), 40.);
    assert_eq!(frame(cx).size.width, px(320.));
    v.update(cx, |v, cx| {
        v.block = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(frame(cx).size.width, px(500.));
    v.update(cx, |v, cx| {
        v.controls = SliderControls::Input;
        v.parent = px(180.);
        cx.notify();
    });
    draw(cx);
    assert!(node(cx, "数值 value").bounds().top() >= node(cx, "数值").bounds().bottom());
    v.update(cx, |v, cx| {
        v.direction = SliderLayout::Vertical;
        v.size = SliderSize::Small;
        v.block = false;
        v.parent = px(500.);
        v.marks = true;
        cx.notify();
    });
    draw(cx);
    let p = node(cx, "数值").bounds().center();
    down(cx, p);
    moving(cx, p - point(px(0.), px(300.)));
    up(cx, p - point(px(0.), px(300.)));
    assert_eq!(value(&v, cx), 100.0.into());
    assert_eq!(node(cx, "数值").bounds().size.width, px(28.));
}

#[gpui_kit::test]
fn vertical_mark_labels_measure_the_same_span_as_the_value_track(cx: &mut TestAppContext) {
    let (v, cx) = open(cx, SliderDomain::default(), (0., 100.));
    v.update(cx, |v, cx| {
        v.direction = SliderLayout::Vertical;
        v.marks = true;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    let upper = node(cx, "上限").bounds().center();
    let lower = node(cx, "下限").bounds().center();
    let label = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|s| {
                s.path().last() == Some(&ElementId::from((ElementId::from("subject"), "labels")))
            })
            .unwrap()
            .bounds()
    });
    assert_eq!(label.top(), upper.y);
    assert_eq!(label.size.height, lower.y - upper.y);
}
