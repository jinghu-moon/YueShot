use super::*;
use std::cell::RefCell;

struct Harness {
    state: Entity<VirtualizerState>,
    extent: Rc<Cell<f32>>,
    builds: Rc<Cell<usize>>,
    horizontal: bool,
    viewport: f32,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let extent = self.extent.clone();
        let builds = self.builds.clone();
        VirtualList::new("measured-motion", &self.state, move |item, _, _| {
            builds.set(builds.get() + 1);
            let main = if item.index() == 0 { extent.get() } else { 20. };
            div().w(px(main)).h(px(main))
        })
        .w(px(if self.horizontal { self.viewport } else { 240. }))
        .h(px(if self.horizontal { 100. } else { self.viewport }))
    }
}
fn harness(cx: &mut Context<Harness>, horizontal: bool) -> Harness {
    Harness {
        state: cx.new(|_| {
            let mut s = VirtualizerState::new(
                if horizontal {
                    Axis::Horizontal
                } else {
                    Axis::Vertical
                },
                source(0, 1000),
                VirtualizerSizing::measured(px(20.)),
            )
            .unwrap();
            s.set_layout(VirtualizerLayout::default().overscan(px(100.)))
                .unwrap();
            s
        }),
        extent: Rc::new(Cell::new(20.)),
        builds: Rc::default(),
        horizontal,
        viewport: 100.,
    }
}
#[gpui_kit::test]
fn measured_smooth_final_frame_feedback_must_settle_at_the_actual_target(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, cx| {
        s.scroll_to_key(2usize.into(), options(false)).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    view.read_with(cx, |v, _| v.extent.set(30.));
    state.update(cx, |s, cx| {
        s.invalidate_items(0..1).unwrap();
        cx.notify();
    });
    frame(cx, 200);
    assert!(
        (offset(&state, cx) - 50.).abs() < 0.01,
        "deadline feedback offset={}",
        offset(&state, cx)
    );
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
}
fn options(tail: bool) -> VirtualizerScrollOptions {
    VirtualizerScrollOptions::default()
        .align(VirtualizerAlign::Start)
        .motion(if tail {
            VirtualizerMotion::JumpThenSmooth {
                duration: Duration::from_millis(200),
                max_distance: px(100.),
            }
        } else {
            VirtualizerMotion::Smooth {
                duration: Duration::from_millis(200),
                max_distance: px(100.),
            }
        })
}

#[gpui_kit::test]
fn measured_smooth_later_viewport_shrink_falls_back_within_the_new_bound(cx: &mut TestAppContext) {
    for horizontal in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| harness(cx, horizontal));
        cx.update(|w, _| w.activate_window());
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        state.update(cx, |s, cx| {
            s.scroll_to_key(2usize.into(), options(false)).unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        view.update(cx, |v, cx| {
            v.viewport = 20.;
            cx.notify();
        });
        frame(cx, 50);
        assert!(
            (offset(&state, cx) - 40.).abs() < 0.01,
            "new short-path bound must apply during motion"
        );
        assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    }
}

#[gpui_kit::test]
fn measured_smooth_later_unknown_path_falls_back_without_waiting_for_the_deadline(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, _| {
        let input = s.viewport().unwrap().clone();
        let now = Instant::now();
        s.scroll_to_key(2usize.into(), options(false)).unwrap();
        s.prepare(&input, now).unwrap();
        s.invalidate_items(0..1).unwrap();
        let (_, request) = s.prepare(&input, now + Duration::from_millis(50)).unwrap();
        assert_eq!(
            request.unwrap().offset(),
            40.,
            "unknown path must use bounded instant convergence"
        );
    });
}

#[gpui_kit::test]
fn measured_smooth_completed_sample_reresolves_same_time_size_feedback(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, _| {
        let input = s.viewport().unwrap().clone();
        let now = Instant::now();
        s.scroll_to_key(2usize.into(), options(false)).unwrap();
        s.prepare(&input, now).unwrap();
        let (_, request) = s.prepare(&input, now + Duration::from_millis(200)).unwrap();
        assert_eq!(request.unwrap().offset(), 40.);
        s.update_sizes(&[yue_component::VirtualizerSizeUpdate::new(0usize, px(30.))])
            .unwrap();
        let (_, request) = s.prepare(&input, now + Duration::from_millis(200)).unwrap();
        assert_eq!(
            request.unwrap().offset(),
            50.,
            "completed animation must still resolve actual geometry"
        );
    });
}
fn offset(state: &Entity<VirtualizerState>, cx: &gpui_kit::VisualTestContext) -> f64 {
    state.read_with(cx, |s, _| s.viewport().unwrap().offset())
}
fn frame(cx: &mut gpui_kit::VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
}
type Events = Rc<RefCell<Vec<(u64, VirtualizerCommandOutcome)>>>;
fn observe(
    state: &Entity<VirtualizerState>,
    cx: &mut gpui_kit::VisualTestContext,
) -> (Events, gpui_kit::Subscription) {
    let events: Events = Rc::default();
    let copy = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(state, move |_, event, _| {
            if let VirtualizerEvent::CommandFinished { id, outcome } = event {
                copy.borrow_mut().push((*id, *outcome));
            }
        })
    });
    (events, subscription)
}
#[gpui_kit::test]
fn measured_smooth_samples_once_per_time_and_high_refresh_keeps_the_deadline(
    cx: &mut TestAppContext,
) {
    for horizontal in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| harness(cx, horizontal));
        cx.update(|w, _| w.activate_window());
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        let (events, _sub) = observe(&state, cx);
        assert!(state.read_with(cx, |s, _| s.capabilities().supports_smooth()));
        let id = state.update(cx, |s, cx| {
            let id = s.scroll_to_key(2usize.into(), options(false)).unwrap();
            cx.notify();
            id
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(offset(&state, cx), 0.);
        for _ in 0..50 {
            frame(cx, 1);
            assert!(state.read_with(cx, |s, _| s.has_scroll_command()));
        }
        assert!((offset(&state, cx) - 40. * (1. - 0.75f64.powi(3))).abs() < 0.01);
        let sample = offset(&state, cx);
        for _ in 0..3 {
            cx.update(|w, cx| w.draw(cx).clear(cx));
            assert_eq!(offset(&state, cx), sample);
        }
        frame(cx, 150);
        assert!((offset(&state, cx) - 40.).abs() < 0.01);
        assert_eq!(
            *events.borrow(),
            vec![(id, VirtualizerCommandOutcome::Settled)]
        );
    }
}
#[gpui_kit::test]
fn measured_smooth_rebases_changed_geometry_and_wheel_cancels_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _sub) = observe(&state, cx);
    let id = state.update(cx, |s, cx| {
        let id = s.scroll_to_key(2usize.into(), options(false)).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 50);
    view.read_with(cx, |v, _| v.extent.set(30.));
    state.update(cx, |s, cx| {
        s.invalidate_items(0..1).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 50);
    frame(cx, 100);
    assert!(
        (offset(&state, cx) - 50.).abs() < 0.01,
        "offset={}",
        offset(&state, cx)
    );
    assert_eq!(
        *events.borrow(),
        vec![(id, VirtualizerCommandOutcome::Settled)]
    );
    let second = state.update(cx, |s, cx| {
        let id = s.scroll_to_key(0usize.into(), options(false)).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 30);
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(50.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-5.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|(i, _)| *i == second)
            .copied()
            .collect::<Vec<_>>(),
        vec![(second, VirtualizerCommandOutcome::Cancelled)]
    );
}
#[gpui_kit::test]
fn measured_long_jump_then_smooth_only_materializes_a_bounded_tail(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _sub) = observe(&state, cx);
    view.read_with(cx, |v, _| v.builds.set(0));
    let id = state.update(cx, |s, cx| {
        let id = s.scroll_to_key(900usize.into(), options(true)).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!((offset(&state, cx) - 17950.).abs() < 0.01);
    assert!(view.read_with(cx, |v, _| v.builds.get()) < 100);
    frame(cx, 1);
    frame(cx, 50);
    assert!(offset(&state, cx) > 17950. && offset(&state, cx) < 18000.);
    frame(cx, 150);
    assert!((offset(&state, cx) - 18000.).abs() < 0.01);
    assert_eq!(
        *events.borrow(),
        vec![(id, VirtualizerCommandOutcome::Settled)]
    );
    // Normal Smooth keeps the existing safe long-distance instant policy.
    state.update(cx, |s, cx| {
        s.scroll_to_key(0usize.into(), options(false)).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(offset(&state, cx), 0.);
}

#[gpui_kit::test]
fn measured_smooth_rejected_owner_timeout_and_preferences_have_one_terminal(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    for timeout in [false, true] {
        state.update(cx, |s, _| {
            let input = s.viewport().unwrap().clone();
            let now = Instant::now();
            let id = s.scroll_to_key(2usize.into(), options(false)).unwrap();
            if timeout {
                for _ in 0..100 {
                    s.prepare(&input, now).unwrap();
                }
                assert!(
                    s.has_scroll_command(),
                    "same-frame preparation is not 100 failed retries"
                );
                s.prepare(&input, now + Duration::from_millis(2001))
                    .unwrap();
            } else {
                for step in 0..12 {
                    if !s.has_scroll_command() {
                        break;
                    }
                    let (_, request) = s
                        .prepare(&input, now + Duration::from_millis(step * 5))
                        .unwrap();
                    let request = request.unwrap();
                    s.confirm(&request, input.offset(), &input).unwrap();
                }
            }
            assert!(!s.has_scroll_command());
            assert_eq!(
                s.take_events()
                    .into_iter()
                    .filter(|e| matches!(e, VirtualizerEvent::CommandFinished { .. }))
                    .collect::<Vec<_>>(),
                vec![VirtualizerEvent::CommandFinished {
                    id,
                    outcome: VirtualizerCommandOutcome::NoProgress
                }]
            );
        });
    }
    state.update(cx, |s, cx| {
        s.scroll_to_key(2usize.into(), options(false).reduced_motion(true))
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(offset(&state, cx), 40.);
    state.update(cx, |s, cx| {
        s.scroll_to_key(0usize.into(), options(false)).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.deactivate_window();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(offset(&state, cx), 0.);
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
}

#[gpui_kit::test]
fn measured_smooth_estimates_fall_back_and_noop_or_deleted_targets_finish(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    let state = view.read_with(cx, |v, _| {
        v.extent.set(30.);
        v.state.clone()
    });
    state.update(cx, |s, cx| {
        s.invalidate_items(0..1).unwrap();
        s.scroll_to_key(2usize.into(), options(false)).unwrap();
        cx.notify();
    });
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        (offset(&state, cx) - 50.).abs() < 0.01,
        "cold offset={}",
        offset(&state, cx)
    );
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    let (events, _sub) = observe(&state, cx);
    let id = state.update(cx, |s, cx| {
        let id = s.scroll_to_key(2usize.into(), options(false)).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        *events.borrow(),
        vec![(id, VirtualizerCommandOutcome::Settled)]
    );
    let removed = state.update(cx, |s, cx| {
        let id = s.scroll_to_key(4usize.into(), options(false)).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    state.update(cx, |s, cx| {
        s.replace_source(
            source(5, 995),
            VirtualizerSizing::measured(px(20.)),
            VirtualizerChange::Splice {
                old: 0..5,
                inserted: 0,
            },
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|(id, _)| *id == removed)
            .count(),
        1
    );
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
}
