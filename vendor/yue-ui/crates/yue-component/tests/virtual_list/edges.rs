use super::*;
use std::cell::RefCell;

#[test]
fn reduced_motion_redirection_stalled_host_and_detach_have_terminal_results() {
    let mut state = fixed(100);
    let v = viewport(&state, 1, 0.);
    state.commit(&v).unwrap();
    let smooth = VirtualizerScrollOptions::default()
        .align(VirtualizerAlign::Start)
        .motion(VirtualizerMotion::Smooth {
            duration: Duration::from_millis(180),
            max_distance: px(100.),
        });
    let first = state.scroll_to_key(2usize.into(), smooth).unwrap();
    let second = state
        .scroll_to_key(3usize.into(), smooth.reduced_motion(true))
        .unwrap();
    assert_ne!(first, second);
    assert!(state.take_events().iter().any(|e| matches!(e, VirtualizerEvent::CommandFinished { id, outcome: VirtualizerCommandOutcome::Cancelled } if *id == first)));
    let (old, request) = state.prepare(&v, Instant::now()).unwrap();
    let request = request.unwrap();
    assert_eq!(request.offset(), 60.);
    assert!(request.is_settled());
    state.confirm(&request, 0., &v).unwrap();
    // A host that repeatedly refuses the destination cannot cause an infinite
    // animation, including an otherwise exact (settled) request.
    for revision in 2..=10 {
        let v = viewport(&state, revision, 0.);
        let (_, request) = state.prepare(&v, Instant::now()).unwrap();
        if let Some(request) = request {
            state.confirm(&request, 0., &v).unwrap();
        }
    }
    assert!(!state.has_scroll_command());
    assert!(state.take_events().iter().any(|e| matches!(e, VirtualizerEvent::CommandFinished { id, outcome: VirtualizerCommandOutcome::NoProgress } if *id == second)));
    state.scroll_to_key(4usize.into(), smooth).unwrap();
    let binding = state.binding().unwrap().clone();
    let v = viewport(&state, 11, 0.);
    let (_, request) = state.prepare(&v, Instant::now()).unwrap();
    state.detach();
    assert!(!state.has_scroll_command());
    assert_eq!(
        state.validate_snapshot(&old),
        Err(VirtualizerError::Detached)
    );
    assert_eq!(
        state.validate_request(&request.unwrap(), &v),
        Err(VirtualizerError::Detached)
    );
    state.bind(&binding).unwrap();
    assert_eq!(
        state.validate_snapshot(&old),
        Err(VirtualizerError::StaleSnapshot)
    );
}

#[test]
fn zero_area_never_builds_an_overscan_window_and_reorder_anchors_by_key() {
    let mut state = fixed(100);
    state
        .set_layout(VirtualizerLayout::default().overscan(px(100.)))
        .unwrap();
    for (main, cross) in [(0., 100.), (100., 0.), (0., 0.)] {
        let v = VirtualizerViewport::new(state.binding().unwrap(), 1, 0., px(main), px(cross));
        let snapshot = state.snapshot(&v).unwrap();
        assert!(snapshot.visible_range().is_empty());
        assert!(snapshot.rendered_range().is_empty());
    }
    struct Keys(Vec<usize>);
    impl VirtualizerSource for Keys {
        fn len(&self) -> usize {
            self.0.len()
        }
        fn key_at(&self, ix: usize) -> ElementId {
            self.0[ix].into()
        }
        fn index_of(&self, key: &ElementId) -> Option<usize> {
            self.0.iter().position(|n| ElementId::from(*n) == *key)
        }
    }
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        Rc::new(Keys(vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100])),
        VirtualizerSizing::fixed(px(20.)),
    )
    .unwrap();
    state.commit(&viewport(&state, 1, 45.)).unwrap();
    state
        .replace_source(
            Rc::new(Keys(vec![10, 20, 40, 50, 30, 60, 70, 80, 90, 100])),
            VirtualizerSizing::fixed(px(20.)),
            VirtualizerChange::Reorder,
        )
        .unwrap();
    let (_, request) = state
        .prepare(&viewport(&state, 2, 45.), Instant::now())
        .unwrap();
    assert_eq!(request.unwrap().offset(), 85.);
}

struct EdgeView {
    state: Entity<VirtualizerState>,
    row_height: f32,
    height: f32,
    mounted: bool,
    events: Rc<RefCell<Vec<VirtualizerEvent>>>,
    _subscription: gpui_kit::Subscription,
}
impl EdgeView {
    fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| {
            VirtualizerState::new(
                Axis::Vertical,
                source(0, 100),
                VirtualizerSizing::measured(px(20.)),
            )
            .unwrap()
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let _subscription = cx.subscribe(&state, |this, _, event, _| {
            this.events.borrow_mut().push(event.clone())
        });
        Self {
            state,
            row_height: 20.,
            height: 60.,
            mounted: true,
            events,
            _subscription,
        }
    }
}
impl Render for EdgeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let row_height = self.row_height;
        div().children(self.mounted.then(|| {
            VirtualList::new("edge-list", &self.state, move |_, _, _| {
                div().h(px(row_height))
            })
            .w(px(240.))
            .h(px(self.height))
        }))
    }
}

#[gpui_kit::test]
fn owned_measured_padding_gap_tail_shrink_and_zero_viewport(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| EdgeView::new(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, _| {
        s.set_layout(
            VirtualizerLayout::default()
                .gap(px(5.))
                .padding(px(10.), px(15.))
                .overscan(px(20.)),
        )
        .unwrap();
        s.set_follow_tail(true).unwrap();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert_eq!(snapshot.item(0).unwrap().start(), 10.);
    assert_eq!(snapshot.item(99).unwrap().extent(), px(20.));
    assert_eq!(snapshot.total_extent(), 2_520.);
    assert_eq!(snapshot.viewport().offset(), 2_460.);
    let handle = state.read_with(cx, |s, _| s.scrollbar_handle().unwrap());
    use gpui_kit::base::ScrollbarHandle as _;
    assert_eq!(handle.content_size().height, px(2_520.));
    assert!(state.read_with(cx, |s, _| s.is_following_tail()));
    state.update(cx, |s, cx| {
        s.replace_source(
            source(0, 2),
            VirtualizerSizing::measured(px(20.)),
            VirtualizerChange::Splice {
                old: 2..100,
                inserted: 0,
            },
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert_eq!(snapshot.total_extent(), 70.);
    assert_eq!(snapshot.viewport().offset(), 10.);
    // Padding and gaps are not visible item content.
    let binding = state.read_with(cx, |s, _| s.binding().unwrap().clone());
    for offset in [0., 30.] {
        let empty = VirtualizerViewport::new(&binding, 100, offset, px(5.), px(240.));
        assert!(state.read_with(cx, |s, _| {
            s.snapshot(&empty).unwrap().visible_range().is_empty()
        }));
    }
    view.update(cx, |v, cx| {
        v.height = 0.;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(state.read_with(cx, |s, _| {
        s.snapshot(s.viewport().unwrap())
            .unwrap()
            .rendered_range()
            .is_empty()
    }));
    view.update(cx, |v, cx| {
        v.height = 60.;
        cx.notify();
    });
    state.update(cx, |s, cx| {
        s.replace_source(
            source(0, 0),
            VirtualizerSizing::measured(px(20.)),
            VirtualizerChange::Splice {
                old: 0..2,
                inserted: 0,
            },
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        state.read_with(cx, |s, _| s.viewport().unwrap().offset()),
        0.
    );
}

#[gpui_kit::test]
fn invalid_measurement_diagnostics_are_deduplicated_and_recover(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut view = EdgeView::new(cx);
        view.row_height = 0.;
        view
    });
    for _ in 0..3 {
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    let (state, events) = view.read_with(cx, |v, _| (v.state.clone(), v.events.clone()));
    let invalid_count = || {
        events
            .borrow()
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    VirtualizerEvent::Diagnostic("invalid measured extent; using estimate")
                )
            })
            .count()
    };
    assert_eq!(invalid_count(), 1);
    let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert!(!snapshot.item(0).unwrap().is_measured());
    view.update(cx, |v, cx| {
        v.row_height = 20.;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    view.update(cx, |v, cx| {
        v.row_height = 0.;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(invalid_count(), 2);
}

#[gpui_kit::test]
fn user_scroll_idle_is_coalesced_and_unmount_cancels_work(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| EdgeView::new(cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, events) = view.read_with(cx, |v, _| (v.state.clone(), v.events.clone()));
    for _ in 0..3 {
        state.update(cx, |s, cx| s.user_scroll_activity(cx));
    }
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(119));
    cx.run_until_parked();
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, VirtualizerEvent::ScrollEnd))
    );
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, VirtualizerEvent::ScrollEnd))
            .count(),
        1
    );
    let old = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    state.update(cx, |s, cx| {
        s.user_scroll_activity(cx);
        s.scroll_to_key(90usize.into(), VirtualizerScrollOptions::default())
            .unwrap();
    });
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    for _ in 0..3 {
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert!(
        state
            .read_with(cx, |s, _| s.validate_snapshot(&old))
            .is_err()
    );
    let ends = events
        .borrow()
        .iter()
        .filter(|e| matches!(e, VirtualizerEvent::ScrollEnd))
        .count();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, VirtualizerEvent::ScrollEnd))
            .count(),
        ends
    );
}

#[test]
fn obsolete_viewport_cannot_issue_or_confirm_a_new_request() {
    let mut state = fixed(100);
    let old = viewport(&state, 1, 0.);
    state
        .scroll_to_key(10usize.into(), VirtualizerScrollOptions::default())
        .unwrap();
    let (_, request) = state.prepare(&old, Instant::now()).unwrap();
    state.commit(&viewport(&state, 2, 10.)).unwrap();
    assert_eq!(
        state.validate_request(&request.unwrap(), &old),
        Err(VirtualizerError::StaleRequest)
    );
    assert!(matches!(
        state.prepare(&old, Instant::now()),
        Err(VirtualizerError::StaleSnapshot)
    ));
}

#[gpui_kit::test]
fn known_horizontal_owned_projects_bounds_commands_and_native_wheel_to_x(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| CardHarness {
        state: cx.new(|_| {
            VirtualizerState::new(
                Axis::Horizontal,
                source(0, 100),
                VirtualizerSizing::known(vec![px(20.); 100]),
            )
            .unwrap()
        }),
        builds: Rc::new(Cell::new(0)),
        children: 1,
        yue: false,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, cx| {
        s.scroll_to_key(
            99usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::End),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert_eq!(snapshot.axis(), Axis::Horizontal);
    assert_eq!(snapshot.viewport().offset(), 1_760.);
    assert_eq!(snapshot.visible_range(), 88..100);
    let last = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&ElementId::from(99usize)))
            .unwrap()
            .bounds()
    });
    assert_eq!(last.left(), px(220.));
    assert_eq!(last.right(), px(240.));
    state.update(cx, |s, cx| {
        s.scroll_to_key(
            2usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-20.), px(0.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        state.read_with(cx, |s, _| s.native_scroll_handle().unwrap().offset()),
        gpui_kit::point(px(-60.), px(0.))
    );
}

#[gpui_kit::test]
fn swapping_controller_under_the_same_element_id_releases_the_old_mount(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| EdgeView::new(cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (old, events) = view.read_with(cx, |v, _| (v.state.clone(), v.events.clone()));
    let command = old.update(cx, |s, _| {
        s.scroll_to_key(90usize.into(), VirtualizerScrollOptions::default())
            .unwrap()
    });
    let next = cx.new(|_| fixed(100));
    view.update(cx, |v, cx| {
        v.state = next.clone();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!old.read_with(cx, |s, _| s.has_scroll_command()));
    assert!(old.read_with(cx, |s, _| s.viewport().is_none()));
    cx.run_until_parked();
    assert!(events.borrow().iter().any(|e| matches!(e, VirtualizerEvent::CommandFinished { id, outcome: VirtualizerCommandOutcome::Cancelled } if *id == command)));
    assert!(next.read_with(cx, |s, _| s.viewport().is_some()));
    next.update(cx, |s, _| {
        s.scroll_to_key(90usize.into(), VirtualizerScrollOptions::default())
            .unwrap();
    });
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    for _ in 0..3 {
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert!(!next.read_with(cx, |s, _| s.has_scroll_command()));
}

#[gpui_kit::test]
fn scrollbar_drag_keeps_native_range_frozen_until_release_and_interrupts_command(
    cx: &mut TestAppContext,
) {
    use gpui_kit::base::ScrollbarHandle as _;
    let (view, cx) = cx.add_window_view(|_, cx| EdgeView::new(cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let handle = state.read_with(cx, |s, _| s.scrollbar_handle().unwrap());
    let before = handle.content_size().height;
    handle.start_drag();
    view.update(cx, |v, cx| {
        v.row_height = 40.;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(handle.content_size().height, before);
    assert!(state.read_with(cx, |s, _| s.total_extent()) > f64::from(before));
    state.update(cx, |s, _| {
        s.scroll_to_key(90usize.into(), VirtualizerScrollOptions::default())
            .unwrap();
    });
    handle.set_offset(gpui_kit::point(px(0.), px(-400.)));
    handle.end_drag();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(handle.content_size().height > before);
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert!(state.read_with(cx, |s, _| s.viewport().unwrap().offset()) < 1_000.);
}

#[gpui_kit::test]
fn native_measurement_budget_emits_one_diagnostic_per_failure_episode(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut v = EdgeView::new(cx);
        v.row_height = 0.;
        v.height = 100_000.;
        v
    });
    let (state, events) = view.read_with(cx, |v, _| (v.state.clone(), v.events.clone()));
    state.update(cx, |s, _| {
        s.replace_source(
            source(0, 100_000),
            VirtualizerSizing::measured(px(20.)),
            VirtualizerChange::Replace {
                preserve_anchor: false,
            },
        )
        .unwrap()
    });
    for _ in 0..3 {
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(
                e,
                VirtualizerEvent::Diagnostic("native measurement budget exhausted")
            ))
            .count(),
        1
    );
    assert!(state.read_with(cx, |s, _| s.total_extent()) >= 2_000_000.);
}

#[gpui_kit::test]
fn owned_smooth_uses_requested_frames_and_wheel_or_hidden_window_stops_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| CardHarness {
        state: cx.new(|_| fixed(100)),
        builds: Rc::new(Cell::new(0)),
        children: 1,
        yue: false,
    });
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(cx.update(|w, _| w.is_window_active()));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let options = VirtualizerScrollOptions::default()
        .align(VirtualizerAlign::Start)
        .motion(VirtualizerMotion::Smooth {
            duration: Duration::from_millis(200),
            max_distance: px(100.),
        });
    state.update(cx, |s, cx| {
        s.scroll_to_key(5usize.into(), options).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.update(|w, cx| {
        assert!(w.simulate_next_frame(cx) > 0);
        w.draw(cx).clear(cx);
    });
    let middle = state.read_with(cx, |s, _| s.viewport().unwrap().offset());
    assert!(middle > 50. && middle < 70., "middle={middle}");
    state.update(cx, |s, cx| {
        s.scroll_to_key(4usize.into(), options).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!((state.read_with(cx, |s, _| s.viewport().unwrap().offset()) - middle).abs() < 0.5);
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-10.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert!(
        (state.read_with(cx, |s, _| s.viewport().unwrap().offset()) - middle - 10.).abs() < 0.5
    );
    state.update(cx, |s, cx| {
        s.scroll_to_key(5usize.into(), options).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.deactivate_window();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        state.read_with(cx, |s, _| s.viewport().unwrap().offset()),
        100.
    );
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
}

#[gpui_kit::test]
fn pointer_release_after_replacement_cannot_activate_the_new_row_at_the_same_index(
    cx: &mut TestAppContext,
) {
    struct ClickCards {
        state: Entity<VirtualizerState>,
        clicked: Rc<RefCell<Vec<ElementId>>>,
    }
    impl Render for ClickCards {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let clicked = self.clicked.clone();
            VirtualList::new("click-cards", &self.state, move |item, _, _| {
                let key = item.key().clone();
                let clicked = clicked.clone();
                div()
                    .id("activate")
                    .size_full()
                    .on_click(move |_, _, _| clicked.borrow_mut().push(key.clone()))
            })
            .w(px(240.))
            .h(px(100.))
        }
    }
    let (view, cx) = cx.add_window_view(|_, cx| ClickCards {
        state: cx.new(|_| fixed(10)),
        clicked: Rc::new(RefCell::new(Vec::new())),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let point = gpui_kit::point(px(10.), px(10.));
    cx.simulate_mouse_down(point, gpui_kit::MouseButton::Left, Default::default());
    let (state, clicked) = view.read_with(cx, |v, _| (v.state.clone(), v.clicked.clone()));
    state.update(cx, |s, cx| {
        s.replace_source(
            source(100, 10),
            VirtualizerSizing::fixed(px(20.)),
            VirtualizerChange::Replace {
                preserve_anchor: false,
            },
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_mouse_up(point, gpui_kit::MouseButton::Left, Default::default());
    assert!(clicked.borrow().is_empty());
    cx.simulate_click(point, Default::default());
    assert_eq!(clicked.borrow().as_slice(), &[ElementId::from(100usize)]);
}
