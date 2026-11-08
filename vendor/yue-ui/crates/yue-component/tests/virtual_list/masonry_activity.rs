use super::*;
use std::cell::RefCell;
use yue_component::{VirtualMasonry, VirtualMasonryState, VirtualizerScrollDirection};

#[test]
fn masonry_activity_observes_native_input_direction_and_one_idle_end() {
    let mut state =
        VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(20.)), 3).unwrap();
    let now = Instant::now();
    state.observe_scroll_activity(now);
    state
        .scroll_to_offset(80., VirtualizerScrollOptions::default())
        .unwrap();
    assert!(
        !state.is_scrolling(),
        "pending intent is not native movement"
    );
    state
        .scroll_handle()
        .set_precise_offset(gpui_kit::point(0., -30.));
    state.observe_scroll_activity(now);
    assert!(!state.has_scroll_command());
    assert!(state.is_scrolling());
    assert_eq!(
        state.scroll_direction(),
        Some(VirtualizerScrollDirection::Forward)
    );
    state
        .scroll_handle()
        .set_precise_offset(gpui_kit::point(0., -10.));
    state.observe_scroll_activity(now + Duration::from_millis(30));
    assert_eq!(
        state.scroll_direction(),
        Some(VirtualizerScrollDirection::Backward)
    );
    state.observe_scroll_activity(now + Duration::from_millis(100));
    state.poll_scroll_idle(now + Duration::from_millis(149));
    assert!(
        state.is_scrolling(),
        "stable observation must not restart the clock"
    );
    state.poll_scroll_idle(now + Duration::from_millis(150));
    assert!(!state.is_scrolling());
    assert_eq!(state.scroll_direction(), None);
    state.poll_scroll_idle(now + Duration::from_secs(1));
    assert_eq!(
        state
            .take_events()
            .iter()
            .filter(|e| matches!(e, VirtualizerEvent::ScrollEnd))
            .count(),
        1
    );
}

struct Harness {
    state: Entity<VirtualMasonryState>,
    mounted: bool,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if !self.mounted {
            return div().into_any_element();
        }
        VirtualMasonry::new("activity-masonry", &self.state, |_, _, _| div().h(px(20.)))
            .w(px(240.))
            .h(px(100.))
            .into_any_element()
    }
}
#[gpui_kit::test]
fn masonry_activity_native_timer_boundary_wheel_and_unmount_are_bounded(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 300), VirtualizerSizing::fixed(px(20.)), 3).unwrap()
        }),
        mounted: true,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&state, move |_, event, _| {
            observed.borrow_mut().push(event.clone())
        })
    });
    // Upward input at the top has zero displacement, but is still user activity.
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(40.), px(40.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(10.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(state.read_with(cx, |s, _| s.is_scrolling()));
    cx.executor().advance_clock(Duration::from_millis(121));
    cx.run_until_parked();
    assert!(!state.read_with(cx, |s, _| s.is_scrolling()));
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, VirtualizerEvent::ScrollEnd))
            .count(),
        1
    );
    state.update(cx, |s, _| s.user_scroll_activity());
    view.update(cx, |_, cx| cx.notify());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(state.read_with(cx, |s, _| s.is_scrolling()));
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    for _ in 0..3 {
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert!(!state.read_with(cx, |s, _| s.is_scrolling()));
    let count = events.borrow().len();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        events.borrow().len(),
        count,
        "unmounted idle task must be cancelled"
    );
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, VirtualizerEvent::ScrollEnd))
            .count(),
        2
    );
}
