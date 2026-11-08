use super::*;
use gpui_kit::base::test_support::snapshots;
use yue_component::{VirtualizerAnchorPosition as Position, VirtualizerCache};

#[test]
fn blank_and_content_anchors_round_trip_portable_cache_on_both_axes() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for (offset, key, position) in [
            (5., 0usize, Position::Before(5.)),
            (25., 0, Position::Within(15.)),
            (35., 1, Position::Before(5.)),
        ] {
            let layout = VirtualizerLayout::default()
                .gap(px(10.))
                .padding(px(10.), px(10.));
            let mut state = VirtualizerState::new(axis, source(100), sizing(2, 100)).unwrap();
            state.set_layout(layout).unwrap();
            state.commit(&input(&state, 1, offset)).unwrap();
            let cache = state.measurement_cache().unwrap();
            assert_eq!(cache.version(), 2);
            assert_eq!(cache.scroll_anchor(), Some((&key.into(), position)));
            let mut restored = VirtualizerState::new(axis, source(100), sizing(2, 100)).unwrap();
            restored.set_layout(layout).unwrap();
            restored.commit(&input(&restored, 1, 0.)).unwrap();
            restored.restore_measurement_cache(&cache).unwrap();
            let (_, request) = restored
                .prepare(&input(&restored, 2, 0.), Instant::now())
                .unwrap();
            assert_eq!(request.unwrap().offset(), offset);
        }
    }
}

#[test]
fn content_shrink_gap_resize_and_tail_padding_keep_distinct_meaning() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 1..3 {
            let mut state = VirtualizerState::new(axis, source(100), sizing(mode, 100)).unwrap();
            let layout = VirtualizerLayout::default()
                .gap(px(10.))
                .padding(px(10.), px(10.));
            state.set_layout(layout).unwrap();
            state.commit(&input(&state, 1, 25.)).unwrap();
            state
                .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(5.))])
                .unwrap();
            let (_, request) = state
                .prepare(&input(&state, 2, 25.), Instant::now())
                .unwrap();
            assert_eq!(request.unwrap().offset(), 15.);
        }
        for mode in 0..3 {
            for (offset, next, expected) in [
                (
                    35.,
                    VirtualizerLayout::default()
                        .gap(px(3.))
                        .padding(px(10.), px(10.)),
                    30.,
                ),
                (
                    5.,
                    VirtualizerLayout::default()
                        .gap(px(10.))
                        .padding(px(20.), px(10.)),
                    15.,
                ),
            ] {
                let mut state =
                    VirtualizerState::new(axis, source(100), sizing(mode, 100)).unwrap();
                state
                    .set_layout(
                        VirtualizerLayout::default()
                            .gap(px(10.))
                            .padding(px(10.), px(10.)),
                    )
                    .unwrap();
                state.commit(&input(&state, 1, offset)).unwrap();
                state.set_layout(next).unwrap();
                let (_, request) = state
                    .prepare(&input(&state, 2, offset), Instant::now())
                    .unwrap();
                assert_eq!(request.unwrap().offset(), expected);
            }
        }
        let mut state = VirtualizerState::new(axis, source(2), sizing(2, 2)).unwrap();
        state
            .set_layout(
                VirtualizerLayout::default()
                    .gap(px(10.))
                    .padding(px(10.), px(100.)),
            )
            .unwrap();
        let v = VirtualizerViewport::new(state.binding().unwrap(), 1, 75., px(20.), px(240.))
            .input_revision(0);
        state.commit(&v).unwrap();
        let cache = state.measurement_cache().unwrap();
        assert_eq!(
            cache.scroll_anchor(),
            Some((&1usize.into(), Position::After(15.)))
        );
        state.restore_measurement_cache(&cache).unwrap();
        let (_, request) = state.prepare(&v.with_revision(2), Instant::now()).unwrap();
        assert_eq!(request.unwrap().offset(), 75.);
    }
}

#[test]
fn invalid_portable_anchor_is_atomic_and_source_revision_controls_persistence() {
    let mut state = VirtualizerState::new(Axis::Vertical, source(100), sizing(2, 100)).unwrap();
    let v = input(&state, 1, 0.);
    state.commit(&v).unwrap();
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(60.))])
        .unwrap();
    for distance in [-1., f64::NAN, f64::INFINITY] {
        for position in [
            Position::Before(distance),
            Position::Within(distance),
            Position::After(distance),
        ] {
            let cache = VirtualizerCache::new(Axis::Vertical, 0, px(240.), px(0.), vec![])
                .anchor(0usize, position);
            assert!(state.restore_measurement_cache(&cache).is_err());
            let item = state.snapshot(&v).unwrap().item(0).unwrap();
            assert!(item.is_measured());
            assert_eq!(item.extent(), px(60.));
        }
    }
}

#[test]
fn attached_header_overlap_and_footer_clear_old_anchor_but_large_gap_remains_owned() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 0..3 {
            for offset in [450., 900., 600.] {
                let binding = VirtualizerBinding::new();
                let initial_count = if offset == 900. { 1 } else { 10 };
                let mut state = VirtualizerState::new_attached(
                    axis,
                    source(initial_count),
                    sizing(mode, initial_count),
                    &binding,
                )
                .unwrap();
                state
                    .set_layout(VirtualizerLayout::default().gap(px(200.)))
                    .unwrap();
                let v = VirtualizerViewport::new(&binding, 1, 505., px(100.), px(240.))
                    .input_revision(0)
                    .with_content_origin(500.)
                    .with_scroll_extent(4000.);
                state.commit(&v).unwrap();
                let v = v.with_revision(3).with_offset(offset);
                state.commit(&v).unwrap();
                let count = state.snapshot(&v).unwrap().len();
                state
                    .replace_source(
                        source(count + 1),
                        sizing(mode, count + 1),
                        VirtualizerChange::Splice {
                            old: count..count,
                            inserted: 1,
                        },
                    )
                    .unwrap();
                let (_, r) = state.prepare(&v.with_revision(4), Instant::now()).unwrap();
                if offset < 500. || offset == 900. {
                    assert!(r.is_none());
                } else {
                    assert_eq!(r.as_ref().map_or(offset, |r| r.offset()), offset);
                }
            }
        }
    }
}

#[gpui_kit::test]
fn exact_native_position_drives_local_bounds_hit_testing_reversal_and_jump_reset(
    cx: &mut TestAppContext,
) {
    for (axis, mode) in [
        (Axis::Vertical, 0),
        (Axis::Vertical, 1),
        (Axis::Horizontal, 0),
        (Axis::Horizontal, 1),
    ] {
        let clicked = Rc::new(Cell::new(0));
        struct ClickHarness {
            state: Entity<VirtualizerState>,
            clicked: Rc<Cell<usize>>,
        }
        impl Render for ClickHarness {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let clicked = self.clicked.clone();
                VirtualList::new("exact-hit", &self.state, move |item, _, _| {
                    let clicked = clicked.clone();
                    let index = item.index();
                    div()
                        .id("click")
                        .size_full()
                        .on_mouse_down(gpui_kit::MouseButton::Left, move |_, _, _| {
                            clicked.set(index)
                        })
                })
                .w(px(240.))
                .h(px(100.))
            }
        }
        let (view, cx) = cx.add_window_view(|_, cx| ClickHarness {
            state: cx.new(|_| {
                VirtualizerState::new(axis, source(1_000_000), sizing(mode, 1_000_000)).unwrap()
            }),
            clicked: clicked.clone(),
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        let handle = state.read_with(cx, |s, _| s.native_scroll_handle().unwrap().clone());
        let set = |offset: f64| {
            handle.set_precise_offset(if axis == Axis::Vertical {
                point(0., -offset)
            } else {
                point(-offset, 0.)
            })
        };
        set(16_777_216.25);
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        let row = cx.update(|w, _| {
            snapshots(w)
                .into_iter()
                .find(|n| n.path().last() == Some(&838_861usize.into()))
                .unwrap()
                .bounds()
        });
        assert_eq!(
            if axis == Axis::Vertical {
                row.top()
            } else {
                row.left()
            },
            cx.update(|w, _| w.pixel_snap(px(3.75)))
        );
        cx.simulate_event(gpui_kit::MouseDownEvent {
            position: if axis == Axis::Vertical {
                point(px(5.), px(4.))
            } else {
                point(px(4.), px(5.))
            },
            button: gpui_kit::MouseButton::Left,
            ..Default::default()
        });
        assert_eq!(clicked.get(), 838_861);
        for delta in [0.25, -0.25] {
            cx.simulate_event(ScrollWheelEvent {
                position: point(px(50.), px(50.)),
                delta: ScrollDelta::Pixels(if axis == Axis::Vertical {
                    point(px(0.), px(-delta))
                } else {
                    point(px(-delta), px(0.))
                }),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
        }
        assert_eq!(
            state.read_with(cx, |s, _| s.viewport().unwrap().offset()),
            16_777_216.25
        );
        // A jump to the same f32 projection must discard the old .25 fraction.
        handle.set_offset(if axis == Axis::Vertical {
            point(px(0.), px(-16_777_216.))
        } else {
            point(px(-16_777_216.), px(0.))
        });
        assert_eq!(
            if axis == Axis::Vertical {
                handle.precise_offset().y
            } else {
                handle.precise_offset().x
            },
            -16_777_216.
        );
        set(0.);
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(50.), px(50.)),
            delta: ScrollDelta::Pixels(if axis == Axis::Vertical {
                point(px(0.), px(0.25))
            } else {
                point(px(0.25), px(0.))
            }),
            modifiers: Default::default(),
            touch_phase: TouchPhase::Moved,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(handle.precise_offset(), point(0., 0.));
    }
}

#[gpui_kit::test]
fn shared_grid_host_preserves_both_exact_axis_components(cx: &mut TestAppContext) {
    struct Grid {
        rows: Entity<VirtualizerState>,
        columns: Entity<VirtualizerState>,
        scroll: gpui_kit::ScrollHandle,
    }
    impl Render for Grid {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            yue_component::VirtualGrid::new(
                "exact-grid",
                &self.rows,
                &self.columns,
                &self.scroll,
                |_, _, _| div().size_full(),
            )
            .w(px(240.))
            .h(px(100.))
        }
    }
    let (view, cx) = cx.add_window_view(|_, cx| {
        let binding = VirtualizerBinding::new();
        Grid {
            rows: cx.new(|_| {
                VirtualizerState::new_attached(
                    Axis::Vertical,
                    source(1_000_000),
                    sizing(0, 1_000_000),
                    &binding,
                )
                .unwrap()
            }),
            columns: cx.new(|_| {
                VirtualizerState::new_attached(
                    Axis::Horizontal,
                    source(1_000_000),
                    sizing(0, 1_000_000),
                    &binding,
                )
                .unwrap()
            }),
            scroll: gpui_kit::ScrollHandle::new(),
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let scroll = view.read_with(cx, |v, _| v.scroll.clone());
    scroll.set_precise_offset(point(-16_777_216.25, -16_777_216.75));
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(50.)),
        delta: ScrollDelta::Pixels(point(px(-0.25), px(-0.25))),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(scroll.precise_offset(), point(-16_777_216.5, -16_777_217.));
    view.read_with(cx, |v, cx| {
        assert_eq!(v.rows.read(cx).viewport().unwrap().offset(), 16_777_217.);
        assert_eq!(
            v.columns.read(cx).viewport().unwrap().offset(),
            16_777_216.5
        );
    });
}

#[gpui_kit::test]
fn masonry_native_drag_and_explicit_navigation_retire_pending_correction(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| MasonryHarness {
        state: cx.new(|_| {
            let mut s =
                VirtualMasonryState::new(source(10_000), VirtualizerSizing::measured(px(40.)), 3)
                    .unwrap();
            s.set_layout(VirtualizerLayout::default().measurement_budget(1), px(0.))
                .unwrap();
            s.scroll_to_key(500usize.into(), VirtualizerAlign::Start)
                .unwrap();
            s
        }),
        wheel_events: Rc::new(Cell::new(0)),
        height: 1.,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    assert!(state.read_with(cx, |s, _| s.has_scroll_command()));
    let next = state.read_with(cx, |s, _| s.scroll_handle().precise_offset().y - 20.);
    state.update(cx, |s, _| {
        s.scroll_handle().set_precise_offset(point(0., next))
    });
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        state.read_with(cx, |s, _| s.scroll_handle().precise_offset().y),
        next
    );
    state.update(cx, |s, _| {
        s.scroll_to_key(800usize.into(), VirtualizerAlign::Start)
            .unwrap();
        assert!(s.has_scroll_command());
        s.cancel_scroll();
        assert!(!s.has_scroll_command());
    });
}
