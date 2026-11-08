use super::*;
use gpui_kit::InputEvent as _;
use std::cell::RefCell;
use yue_component::{VirtualMasonry, VirtualMasonryState};

#[test]
fn masonry_offset_validation_preserves_the_pending_command_and_native_owner() {
    let mut state =
        VirtualMasonryState::new(source(0, 100), VirtualizerSizing::measured(px(20.)), 3).unwrap();
    let old = state
        .scroll_to_key(50usize.into(), VirtualizerAlign::End)
        .unwrap();
    let frozen = state
        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
        .unwrap();
    for offset in [-1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            state.scroll_to_offset(offset, VirtualizerScrollOptions::default()),
            Err(VirtualizerError::InvalidGeometry("scroll offset"))
        );
        assert!(state.has_scroll_command());
        assert!(state.take_events().is_empty());
        assert_eq!(state.scroll_handle().precise_offset().y, 0.);
        state.update_sizes_from_snapshot(&frozen, &[]).unwrap();
    }
    let smooth = VirtualizerScrollOptions::default().motion(VirtualizerMotion::Smooth {
        duration: Duration::from_millis(100),
        max_distance: px(100.),
    });
    assert_eq!(
        state.scroll_to_offset(
            20.,
            smooth.motion(VirtualizerMotion::Smooth {
                duration: Duration::from_millis(100),
                max_distance: px(f32::INFINITY),
            })
        ),
        Err(VirtualizerError::InvalidGeometry("smooth bounds"))
    );
    assert!(state.take_events().is_empty());
    let invalid = VirtualizerScrollOptions::default()
        .reduced_motion(true)
        .motion(VirtualizerMotion::Smooth {
            duration: Duration::ZERO,
            max_distance: px(100.),
        });
    assert_eq!(
        state.scroll_to_offset(20., invalid),
        Err(VirtualizerError::InvalidGeometry("smooth bounds"))
    );
    assert!(state.take_events().is_empty());
    let next = state
        .scroll_to_offset(20., smooth.reduced_motion(true))
        .unwrap();
    assert_eq!(next, old + 1);
    assert_eq!(
        state.take_events(),
        vec![VirtualizerEvent::CommandFinished {
            id: old,
            outcome: VirtualizerCommandOutcome::Cancelled,
        }]
    );
    assert!(state.capabilities().supports_scroll_to_offset());
    assert!(state.capabilities().supports_follow_tail());
    assert!(!state.capabilities().supports_attached());
    assert!(!state.capabilities().supports_smooth());
}

struct OffsetHarness {
    state: Entity<VirtualMasonryState>,
    height: f32,
    mounted: bool,
}
type CommandEvents = Rc<RefCell<Vec<VirtualizerEvent>>>;
fn record_commands(
    state: &Entity<VirtualMasonryState>,
    cx: &mut gpui_kit::VisualTestContext,
) -> (CommandEvents, gpui_kit::Subscription) {
    let events: CommandEvents = Rc::default();
    let observed = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(state, move |_, event, _| {
            if matches!(event, VirtualizerEvent::CommandFinished { .. }) {
                observed.borrow_mut().push(event.clone());
            }
        })
    });
    (events, subscription)
}
impl Render for OffsetHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let height = self.height;
        if self.mounted {
            VirtualMasonry::new("offset-masonry", &self.state, move |_, _, _| {
                div().h(px(height))
            })
            .w(px(300.))
            .h(px(100.))
            .into_any_element()
        } else {
            div().into_any_element()
        }
    }
}

#[gpui_kit::test]
fn masonry_offsets_before_mount_clamp_and_preserve_f64_coordinates(cx: &mut TestAppContext) {
    for (count, extent, target) in [
        (0, 20., 123.),
        (300, 20., 123.5),
        (300, 20., f64::MAX),
        (1_000, 100_000., 16_777_217.125),
    ] {
        let state = cx.new(|_| {
            let mut s =
                VirtualMasonryState::new(source(0, count), VirtualizerSizing::fixed(px(extent)), 3)
                    .unwrap();
            s.scroll_to_offset(
                target,
                VirtualizerScrollOptions::default().align(VirtualizerAlign::Center),
            )
            .unwrap();
            s
        });
        let events: CommandEvents = Rc::default();
        let observed = events.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&state, move |_, event, _| {
                if matches!(event, VirtualizerEvent::CommandFinished { .. }) {
                    observed.borrow_mut().push(event.clone());
                }
            })
        });
        let (_view, cx) = cx.add_window_view(|_, _| OffsetHarness {
            state: state.clone(),
            height: extent,
            mounted: true,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        state.update(cx, |s, _| {
            let limit = (s
                .snapshot(0., gpui_kit::size(px(300.), px(100.)))
                .unwrap()
                .total_extent()
                - 100.)
                .max(0.);
            assert_eq!(-s.scroll_handle().precise_offset().y, target.min(limit));
            assert!(!s.has_scroll_command());
            assert_eq!(
                std::mem::take(&mut *events.borrow_mut()),
                vec![VirtualizerEvent::CommandFinished {
                    id: 1,
                    outcome: VirtualizerCommandOutcome::Settled,
                }]
            );
        });
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        assert!(events.borrow().is_empty());
    }
}

#[gpui_kit::test]
fn masonry_offset_rechecks_current_geometry_after_measurement_and_source_shrink(
    cx: &mut TestAppContext,
) {
    for known in [false, true] {
        let state = cx.new(|_| {
            let sizing = if known {
                VirtualizerSizing::known(vec![px(20.); 300])
            } else {
                VirtualizerSizing::measured(px(40.))
            };
            VirtualMasonryState::new(source(0, 300), sizing, 3).unwrap()
        });
        let (_view, cx) = cx.add_window_view(|_, _| OffsetHarness {
            state: state.clone(),
            height: 20.,
            mounted: true,
        });
        let (events, _subscription) = record_commands(&state, cx);
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let id = state.update(cx, |s, cx| {
            let id = s
                .scroll_to_offset(600.25, VirtualizerScrollOptions::default())
                .unwrap();
            cx.notify();
            id
        });
        for _ in 0..4 {
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            });
        }
        assert_eq!(
            state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
            600.25
        );
        assert_eq!(
            std::mem::take(&mut *events.borrow_mut()),
            vec![VirtualizerEvent::CommandFinished {
                id,
                outcome: VirtualizerCommandOutcome::Settled,
            }]
        );
        let id = state.update(cx, |s, cx| {
            let id = s
                .scroll_to_offset(900., VirtualizerScrollOptions::default())
                .unwrap();
            s.replace_source(
                source(0, 6),
                if known {
                    VirtualizerSizing::known(vec![px(20.); 6])
                } else {
                    VirtualizerSizing::measured(px(40.))
                },
            )
            .unwrap();
            cx.notify();
            id
        });
        for _ in 0..4 {
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            });
        }
        assert_eq!(
            state.read_with(cx, |s, _| s.scroll_handle().precise_offset().y),
            0.
        );
        assert_eq!(
            std::mem::take(&mut *events.borrow_mut()),
            vec![VirtualizerEvent::CommandFinished {
                id,
                outcome: VirtualizerCommandOutcome::Settled,
            }]
        );
    }
}

#[gpui_kit::test]
fn masonry_offset_correction_is_cancelled_by_native_input_and_unmount(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| OffsetHarness {
        state: cx.new(|_| {
            let mut s = VirtualMasonryState::new(
                source(0, 10_000),
                VirtualizerSizing::measured(px(40.)),
                3,
            )
            .unwrap();
            s.set_layout(VirtualizerLayout::default().measurement_budget(1), px(0.))
                .unwrap();
            s.scroll_to_offset(6_000., VirtualizerScrollOptions::default())
                .unwrap();
            s
        }),
        height: 1.,
        mounted: true,
    });
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _subscription) = record_commands(&state, cx);
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(state.read_with(cx, |s, _| s.has_scroll_command()));
    let position = state.read_with(cx, |s, _| s.scroll_handle().precise_offset().y - 20.);
    state.update(cx, |s, _| {
        s.scroll_handle()
            .set_precise_offset(gpui_kit::point(0., position))
    });
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        state.read_with(cx, |s, _| s.scroll_handle().precise_offset().y),
        position
    );
    assert_eq!(
        std::mem::take(&mut *events.borrow_mut()),
        vec![VirtualizerEvent::CommandFinished {
            id: 1,
            outcome: VirtualizerCommandOutcome::Cancelled,
        }]
    );
    let id = state.update(cx, |s, _| {
        s.scroll_to_offset(9_000., VirtualizerScrollOptions::default())
            .unwrap()
    });
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    for _ in 0..3 {
        cx.update(|w, cx| {
            w.simulate_next_frame(cx);
            w.draw(cx).clear(cx);
        });
    }
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        std::mem::take(&mut *events.borrow_mut()),
        vec![VirtualizerEvent::CommandFinished {
            id,
            outcome: VirtualizerCommandOutcome::Cancelled,
        }]
    );
    view.update(cx, |v, cx| {
        v.mounted = true;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn masonry_offset_boundary_wheel_cancels_once_without_moving_the_owner(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| OffsetHarness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        height: 20.,
        mounted: true,
    });
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _subscription) = record_commands(&state, cx);
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let id = state.update(cx, |s, _| {
        s.scroll_to_offset(500., VirtualizerScrollOptions::default())
            .unwrap()
    });
    for _ in 0..2 {
        cx.update(|w, cx| {
            w.dispatch_event(
                gpui_kit::ScrollWheelEvent {
                    position: gpui_kit::point(px(100.), px(50.)),
                    delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(20.))),
                    modifiers: Default::default(),
                    touch_phase: gpui_kit::TouchPhase::Moved,
                }
                .to_platform_input(),
                cx,
            );
        });
        cx.background_executor.run_until_parked();
    }
    assert_eq!(
        state.read_with(cx, |s, _| s.scroll_handle().precise_offset().y),
        0.
    );
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        std::mem::take(&mut *events.borrow_mut()),
        vec![VirtualizerEvent::CommandFinished {
            id,
            outcome: VirtualizerCommandOutcome::Cancelled,
        }]
    );
}

#[gpui_kit::test]
fn masonry_offset_incomplete_correction_stops_once_at_the_shared_budget(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| OffsetHarness {
        state: cx.new(|_| {
            let mut s = VirtualMasonryState::new(
                source(0, 20_000),
                VirtualizerSizing::measured(px(40.)),
                3,
            )
            .unwrap();
            s.set_layout(VirtualizerLayout::default().measurement_budget(1), px(0.))
                .unwrap();
            s
        }),
        height: 1.,
        mounted: true,
    });
    let state = view.read_with(cx, |v, _| v.state.clone());
    let (events, _subscription) = record_commands(&state, cx);
    let id = state.update(cx, |s, cx| {
        let id = s
            .scroll_to_offset(6_000., VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
        id
    });
    for _ in 0..40 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        *events.borrow(),
        vec![VirtualizerEvent::CommandFinished {
            id,
            outcome: VirtualizerCommandOutcome::NoProgress,
        }]
    );
}
