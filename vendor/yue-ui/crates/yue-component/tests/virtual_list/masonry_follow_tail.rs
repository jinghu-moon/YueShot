use super::*;
use gpui_kit::InputEvent;
use yue_component::{VirtualMasonry, VirtualMasonryState};

struct FollowHarness {
    state: Entity<VirtualMasonryState>,
    mounted: bool,
    height: f32,
    wheel_events: Rc<Cell<usize>>,
}
impl Render for FollowHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let height = self.height;
        let wheel_events = self.wheel_events.clone();
        if self.mounted {
            VirtualMasonry::new("follow-masonry", &self.state, move |_, _, _| {
                div().h(px(height))
            })
            .w(px(300.))
            .h(px(100.))
            .on_scroll_wheel(move |_, _, _| wheel_events.set(wheel_events.get() + 1))
            .into_any_element()
        } else {
            div().into_any_element()
        }
    }
}

fn follow_wheel(delta: f32, cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|w, cx| {
        w.dispatch_event(
            gpui_kit::ScrollWheelEvent {
                position: gpui_kit::point(px(100.), px(50.)),
                delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(delta))),
                modifiers: Default::default(),
                touch_phase: gpui_kit::TouchPhase::Moved,
            }
            .to_platform_input(),
            cx,
        );
    });
    cx.background_executor.run_until_parked();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
}

#[gpui_kit::test]
fn masonry_follow_tail_tracks_append_and_pauses_on_user_scroll(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| FollowHarness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 30), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        mounted: true,
        height: 20.,
        wheel_events: Rc::new(Cell::new(0)),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());

    state.update(cx, |s, cx| {
        s.set_follow_tail(true).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(state.read_with(cx, |s, _| s.is_following_tail()));
    assert_eq!(
        state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
        100.
    );

    state.update(cx, |s, cx| {
        s.append_source(source(0, 33), VirtualizerSizing::fixed(px(20.)))
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
        120.
    );
    assert!(state.read_with(cx, |s, _| s.is_following_tail()));

    state.update(cx, |s, _| {
        s.scroll_handle()
            .set_precise_offset(gpui_kit::point(0., -20.));
    });
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    state.update(cx, |s, cx| {
        s.append_source(source(0, 36), VirtualizerSizing::fixed(px(20.)))
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
        20.
    );
}

#[gpui_kit::test]
fn masonry_follow_tail_resets_on_replace_and_explicit_commands(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| FollowHarness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 30), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        mounted: true,
        height: 20.,
        wheel_events: Rc::new(Cell::new(0)),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, cx| {
        s.set_follow_tail(true).unwrap();
        s.user_scroll_activity();
        s.replace_source(source(100, 30), VirtualizerSizing::fixed(px(20.)))
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    assert_eq!(
        state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
        100.
    );

    state.update(cx, |s, cx| {
        s.set_follow_tail(true).unwrap();
        s.scroll_to_offset(20., VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    assert_eq!(
        state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
        20.
    );
}

#[gpui_kit::test]
fn masonry_measured_append_and_user_return_track_the_native_tail(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| FollowHarness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 600), VirtualizerSizing::measured(px(40.)), 3)
                .unwrap()
        }),
        mounted: true,
        height: 20.,
        wheel_events: Rc::new(Cell::new(0)),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let wheel_events = view.read_with(cx, |v, _| v.wheel_events.clone());
    state.update(cx, |s, cx| {
        s.set_follow_tail(true).unwrap();
        cx.notify();
    });
    for _ in 0..8 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
    let at_tail = |s: &VirtualMasonryState| {
        let viewport = gpui_kit::size(px(300.), px(100.));
        let limit = (s.snapshot(0., viewport).unwrap().total_extent() - 100.).max(0.);
        (
            limit,
            (-s.scroll_handle().precise_offset().y - limit).abs() <= 0.01,
        )
    };
    assert!(state.read_with(cx, |s, _| at_tail(s).1));

    state.update(cx, |s, cx| {
        s.append_source(source(0, 633), VirtualizerSizing::measured(px(40.)))
            .unwrap();
        cx.notify();
    });
    for _ in 0..8 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
    assert!(state.read_with(cx, |s, _| s.is_following_tail() && at_tail(s).1));

    let tail = state.read_with(cx, |s, _| at_tail(s).0);
    follow_wheel(200., cx);
    assert_eq!(wheel_events.get(), 1);
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    let paused = state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y);
    assert!(paused < tail, "wheel input should move away from tail");
    assert!(!state.read_with(cx, |s, _| at_tail(s).1));
    state.update(cx, |s, cx| {
        s.append_source(source(0, 666), VirtualizerSizing::measured(px(40.)))
            .unwrap();
        cx.notify();
    });
    for _ in 0..8 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
    assert_eq!(
        state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
        paused
    );
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    assert!(state.read_with(cx, |s, _| at_tail(s).0 > paused));

    follow_wheel(-20_000., cx);
    assert_eq!(wheel_events.get(), 2);
    for _ in 0..8 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
    assert!(state.read_with(cx, |s, _| s.is_following_tail() && at_tail(s).1));
    cx.update(|w, cx| w.simulate_next_frame(cx));
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
}

#[gpui_kit::test]
fn masonry_key_and_offset_commands_override_follow_without_enabling_a_session(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| FollowHarness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 30), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        mounted: true,
        height: 20.,
        wheel_events: Rc::new(Cell::new(0)),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, cx| {
        s.set_follow_tail(true).unwrap();
        s.scroll_to_offset(20., VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    state.update(cx, |s, cx| {
        s.scroll_to_key(29usize.into(), VirtualizerAlign::Start)
            .unwrap();
        cx.notify();
    });
    for _ in 0..3 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));

    state.update(cx, |s, cx| {
        s.scroll_to_offset(100., VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));

    state.update(cx, |s, cx| {
        s.set_follow_tail(false).unwrap();
        s.scroll_to_offset(100., VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    state.update(cx, |s, cx| {
        s.append_source(source(0, 33), VirtualizerSizing::fixed(px(20.)))
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.is_following_tail()));
    assert_eq!(
        state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y),
        100.
    );
}
