use super::*;
use std::cell::RefCell;
use yue_component::{VirtualMasonry, VirtualMasonryState, VirtualizerSizeUpdate};

struct Harness {
    state: Entity<VirtualMasonryState>,
    mounted: bool,
    width: f32,
    height: f32,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if !self.mounted {
            return div().into_any_element();
        }
        VirtualMasonry::new("smooth-masonry", &self.state, |_, _, _| div().h(px(20.)))
            .w(px(self.width))
            .h(px(self.height))
            .into_any_element()
    }
}
fn options() -> VirtualizerScrollOptions {
    VirtualizerScrollOptions::default()
        .align(VirtualizerAlign::Start)
        .motion(VirtualizerMotion::Smooth {
            duration: Duration::from_millis(200),
            max_distance: px(100.),
        })
}
type Events = Rc<RefCell<Vec<(u64, VirtualizerCommandOutcome)>>>;
fn observe(
    state: &Entity<VirtualMasonryState>,
    cx: &mut gpui_kit::VisualTestContext,
) -> (Events, gpui_kit::Subscription) {
    let events: Events = Rc::default();
    let observed = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(state, move |_, event, _| {
            if let VirtualizerEvent::CommandFinished { id, outcome } = event {
                observed.borrow_mut().push((*id, *outcome));
            }
        })
    });
    (events, subscription)
}
fn offset(state: &Entity<VirtualMasonryState>, cx: &mut gpui_kit::VisualTestContext) -> f64 {
    state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y)
}
fn frame(cx: &mut gpui_kit::VisualTestContext, elapsed: u64) {
    cx.executor().advance_clock(Duration::from_millis(elapsed));
    cx.update(|w, cx| {
        assert!(
            w.simulate_next_frame(cx) > 0,
            "active command must request frames"
        );
        w.draw(cx).clear(cx);
    });
}

#[gpui_kit::test]
fn masonry_smooth_fixed_and_known_sample_frames_and_settle_once(cx: &mut TestAppContext) {
    for known in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| Harness {
            state: cx.new(|_| {
                VirtualMasonryState::new(
                    source(0, 300),
                    if known {
                        VirtualizerSizing::known(vec![px(20.); 300])
                    } else {
                        VirtualizerSizing::fixed(px(20.))
                    },
                    3,
                )
                .unwrap()
            }),
            mounted: true,
            width: 300.,
            height: 100.,
        });
        cx.update(|w, _| w.activate_window());
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        let (events, _subscription) = observe(&state, cx);
        assert!(state.read_with(cx, |s, _| s.capabilities().supports_smooth()));
        for key in [false, true] {
            let from = offset(&state, cx);
            let target = if key { 20. } else { 80. };
            let id = state.update(cx, |s, cx| {
                let id = if key {
                    s.scroll_to_key(3usize.into(), options()).unwrap()
                } else {
                    s.scroll_to_offset(target, options()).unwrap()
                };
                cx.notify();
                id
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            assert_eq!(
                offset(&state, cx),
                from,
                "first sample must preserve position"
            );
            frame(cx, 50);
            let expected = from + (target - from) * (1. - 0.75f64.powi(3));
            assert!((offset(&state, cx) - expected).abs() < 0.001);
            assert!(state.read_with(cx, |s, _| s.has_scroll_command()));
            assert!(!events.borrow().iter().any(|(event, _)| *event == id));
            frame(cx, 150);
            assert_eq!(offset(&state, cx), target);
            assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
            assert_eq!(
                events
                    .borrow()
                    .iter()
                    .filter(|(event, _)| *event == id)
                    .copied()
                    .collect::<Vec<_>>(),
                vec![(id, VirtualizerCommandOutcome::Settled)]
            );
            cx.update(|w, cx| {
                w.simulate_next_frame(cx);
                w.draw(cx).clear(cx);
            });
            assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
        }
    }
}

#[gpui_kit::test]
fn masonry_smooth_interrupts_from_native_position_and_preserves_follow_pause(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        mounted: true,
        width: 300.,
        height: 100.,
    });
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _subscription) = observe(&state, cx);
    state.update(cx, |s, cx| {
        s.set_follow_tail(true).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let first = state.update(cx, |s, cx| {
        let id = s.scroll_to_offset(1820., options()).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 50);
    let middle = offset(&state, cx);
    assert!(middle > 1820. && middle < 1900.);
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    let second = state.update(cx, |s, cx| {
        let id = s.scroll_to_offset(1890., options()).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(offset(&state, cx), middle);
    assert_eq!(
        *events.borrow(),
        vec![(first, VirtualizerCommandOutcome::Cancelled)]
    );
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(10.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!((offset(&state, cx) - middle + 10.).abs() < 0.01);
    assert_eq!(
        *events.borrow(),
        vec![
            (first, VirtualizerCommandOutcome::Cancelled),
            (second, VirtualizerCommandOutcome::Cancelled)
        ]
    );
    let third = state.update(cx, |s, cx| {
        let id = s.scroll_to_offset(1800., options()).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    state.update(cx, |s, _| {
        s.scroll_handle()
            .set_precise_offset(gpui_kit::point(0., -1750.))
    });
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert_eq!(offset(&state, cx), 1750.);
    assert_eq!(
        events.borrow().last(),
        Some(&(third, VirtualizerCommandOutcome::Cancelled))
    );
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
}

#[gpui_kit::test]
fn masonry_smooth_retargets_geometry_and_nearest_without_restarting_time(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        mounted: true,
        width: 300.,
        height: 100.,
    });
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _subscription) = observe(&state, cx);
    let id = state.update(cx, |s, cx| {
        let id = s.scroll_to_key(9usize.into(), options()).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 50);
    state.update(cx, |s, cx| {
        s.update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(40.))])
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!((offset(&state, cx) - 80. * (1. - 0.75f64.powi(3))).abs() < 0.001);
    frame(cx, 150);
    assert_eq!(offset(&state, cx), 80.);
    assert_eq!(
        *events.borrow(),
        vec![(id, VirtualizerCommandOutcome::Settled)]
    );
    state.update(cx, |s, cx| {
        s.scroll_to_offset(0., VirtualizerAlign::Start).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let id = state.update(cx, |s, cx| {
        let id = s
            .scroll_to_key(24usize.into(), options().align(VirtualizerAlign::Nearest))
            .unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 50);
    frame(cx, 150);
    assert_eq!(
        offset(&state, cx),
        100.,
        "nearest must keep the command's original reference viewport"
    );
    assert_eq!(
        events.borrow().last(),
        Some(&(id, VirtualizerCommandOutcome::Settled))
    );
}

#[gpui_kit::test]
fn masonry_smooth_safe_fallbacks_and_noops_finish_without_animation_loops(cx: &mut TestAppContext) {
    // Dynamic estimates use the same safe instant policy as a one-axis list.
    for case in 0..9 {
        let (view, cx) = cx.add_window_view(|_, cx| Harness {
            state: cx.new(|_| {
                VirtualMasonryState::new(
                    source(0, 300),
                    if case == 0 {
                        VirtualizerSizing::measured(px(20.))
                    } else {
                        VirtualizerSizing::fixed(px(20.))
                    },
                    3,
                )
                .unwrap()
            }),
            mounted: true,
            width: if case == 4 { 0. } else { 300. },
            height: if case == 5 { 0. } else { 100. },
        });
        cx.update(|w, _| w.activate_window());
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        let (events, _subscription) = observe(&state, cx);
        let target = if case == 2 {
            120.
        } else if case == 6 {
            0.
        } else {
            80.
        };
        let id = state.update(cx, |s, cx| {
            let motion = if case == 2 {
                // The viewport bound still applies with a larger caller cap.
                options().motion(VirtualizerMotion::Smooth {
                    duration: Duration::from_millis(200),
                    max_distance: px(200.),
                })
            } else {
                options()
            };
            let id = s
                .scroll_to_offset(target, motion.reduced_motion(case == 1))
                .unwrap();
            cx.notify();
            id
        });
        if case == 3 {
            cx.deactivate_window();
        }
        if case == 8 {
            cx.update(|_, cx| cx.set_reduce_motion(true));
        }
        cx.update(|w, cx| w.draw(cx).clear(cx));
        if case == 7 {
            // Deactivating a running command settles to the live target.
            assert!(state.read_with(cx, |s, _| s.has_scroll_command()));
            cx.deactivate_window();
            cx.update(|w, cx| w.draw(cx).clear(cx));
        }
        assert_eq!(offset(&state, cx), target, "fallback case {case}");
        assert!(
            !state.read_with(cx, |s, _| s.has_scroll_command()),
            "case {case}"
        );
        assert_eq!(
            *events.borrow(),
            vec![(id, VirtualizerCommandOutcome::Settled)]
        );
        cx.update(|w, cx| {
            w.simulate_next_frame(cx);
            w.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.update(|w, cx| w.simulate_next_frame(cx)),
            0,
            "case {case}"
        );
    }
}

#[gpui_kit::test]
fn masonry_smooth_unmount_cancels_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        mounted: true,
        width: 300.,
        height: 100.,
    });
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _subscription) = observe(&state, cx);
    let id = state.update(cx, |s, cx| {
        let id = s.scroll_to_offset(80., options()).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 50);
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert_eq!(
        *events.borrow(),
        vec![(id, VirtualizerCommandOutcome::Cancelled)]
    );
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
}

#[gpui_kit::test]
fn masonry_smooth_high_refresh_and_tiny_motion_do_not_consume_convergence_budgets(
    cx: &mut TestAppContext,
) {
    for case in 0..4 {
        let (view, cx) = cx.add_window_view(|_, cx| Harness {
            state: cx.new(|_| {
                VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(20.)), 3)
                    .unwrap()
            }),
            mounted: true,
            width: 300.,
            height: 100.,
        });
        cx.update(|w, _| w.activate_window());
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        let (events, _subscription) = observe(&state, cx);
        let id = state.update(cx, |s, cx| {
            let id = s
                .scroll_to_offset(
                    if case == 3 { 0.1 } else { 80. },
                    options().motion(VirtualizerMotion::Smooth {
                        duration: if case == 1 || case == 3 {
                            Duration::from_millis(200)
                        } else {
                            Duration::from_secs(10)
                        },
                        max_distance: px(100.),
                    }),
                )
                .unwrap();
            cx.notify();
            id
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        for _ in 0..64 {
            if !state.read_with(cx, |s, _| s.has_scroll_command()) {
                break;
            }
            frame(
                cx,
                match case {
                    0 => 0,
                    1 | 3 => 2,
                    _ => 2100,
                },
            );
        }
        if case == 1 {
            assert!(
                state.read_with(cx, |s, _| s.has_scroll_command()),
                "64 valid animation frames must not exhaust geometry retries"
            );
            assert!(events.borrow().is_empty());
            frame(cx, 72);
        }
        assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
        assert_eq!(
            *events.borrow(),
            vec![(
                id,
                if case == 1 || case == 3 {
                    VirtualizerCommandOutcome::Settled
                } else {
                    VirtualizerCommandOutcome::NoProgress
                }
            )],
            "case {case}"
        );
        if case == 1 || case == 3 {
            assert_eq!(offset(&state, cx), if case == 3 { 0.1 } else { 80. });
        } else {
            assert!(offset(&state, cx) < 80.);
        }
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        assert_eq!(events.borrow().len(), 1);
    }
}

#[gpui_kit::test]
fn masonry_smooth_preserves_subpixel_positions_and_boundary_wheel_cancels(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(1_000_000.)), 3)
                .unwrap()
        }),
        mounted: true,
        width: 300.,
        height: 100.,
    });
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _subscription) = observe(&state, cx);
    let from = 16_777_217.125;
    state.update(cx, |s, cx| {
        s.scroll_to_offset(from, VirtualizerAlign::Start).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let id = state.update(cx, |s, cx| {
        let id = s.scroll_to_offset(from + 0.25, options()).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    frame(cx, 50);
    assert!((offset(&state, cx) - from - 0.25 * (1. - 0.75f64.powi(3))).abs() < 1e-8);
    frame(cx, 150);
    assert_eq!(offset(&state, cx), from + 0.25);
    assert_eq!(
        events.borrow().last(),
        Some(&(id, VirtualizerCommandOutcome::Settled))
    );
    state.update(cx, |s, cx| {
        s.scroll_to_offset(0., VirtualizerAlign::Start).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let id = state.update(cx, |s, cx| {
        let id = s.scroll_to_offset(80., options()).unwrap();
        cx.notify();
        id
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(10.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(offset(&state, cx), 0.);
    assert_eq!(
        events.borrow().last(),
        Some(&(id, VirtualizerCommandOutcome::Cancelled))
    );
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
}
