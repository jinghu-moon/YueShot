use super::*;

#[test]
fn contiguous_and_sparse_working_items_match_the_sorted_union_on_both_axes() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for known in [false, true] {
            let mut state = VirtualizerState::new(
                axis,
                source(0, 100_000),
                if known {
                    VirtualizerSizing::known(vec![px(20.); 100_000])
                } else {
                    VirtualizerSizing::fixed(px(20.))
                },
            )
            .unwrap();
            for keys in [
                vec![],
                vec![0usize.into(), 99_999usize.into(), 51usize.into()],
            ] {
                state.set_retained_keys(keys).unwrap();
                for offset in [0., 1000., 1_999_900.] {
                    let viewport = VirtualizerViewport::new(
                        state.binding().unwrap(),
                        1,
                        offset,
                        px(100.),
                        px(240.),
                    );
                    let snapshot = state.snapshot(&viewport).unwrap();
                    let actual: Vec<_> = snapshot
                        .working_items()
                        .map(|item| item.unwrap().index())
                        .collect();
                    assert_eq!(actual, snapshot.working_indices());
                    assert!(actual.windows(2).all(|pair| pair[0] < pair[1]));
                    assert!(actual.len() <= 9);
                }
            }
        }
    }
}

#[gpui_kit::test]
fn masonry_final_frame_metadata_is_prepared_once(cx: &mut TestAppContext) {
    struct Harness {
        state: Entity<yue_component::VirtualMasonryState>,
        builds: Rc<Cell<usize>>,
    }
    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let builds = self.builds.clone();
            yue_component::VirtualMasonry::new("metadata-once", &self.state, move |_, _, _| {
                builds.set(builds.get() + 1);
                div().h(px(40.))
            })
            .w(px(300.))
            .h(px(100.))
        }
    }
    let source = source(0, 10_000);
    let builds = Rc::new(Cell::new(0));
    let (_, cx) = cx.add_window_view(|_, cx| Harness {
        state: cx.new(|_| {
            yue_component::VirtualMasonryState::new(
                source.clone(),
                VirtualizerSizing::measured(px(40.)),
                3,
            )
            .unwrap()
        }),
        builds: builds.clone(),
    });
    for _ in 0..3 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx)
        });
    }
    source.calls.set(0);
    builds.set(0);
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx)
    });
    assert_eq!(builds.get(), 9);
    assert_eq!(source.calls.get(), builds.get() * 4);
}

#[test]
fn masonry_noop_batches_validate_duplicates_and_fail_atomically() {
    use yue_component::{VirtualMasonryState, VirtualizerSizeUpdate};
    let mut state =
        VirtualMasonryState::new(source(0, 100), VirtualizerSizing::measured(px(40.)), 3).unwrap();
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(40.))])
        .unwrap();
    let before = state
        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
        .unwrap();
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(40.))])
        .unwrap();
    for batch in [
        vec![
            VirtualizerSizeUpdate::new(0usize, px(40.)),
            VirtualizerSizeUpdate::new(0usize, px(40.)),
        ],
        vec![
            VirtualizerSizeUpdate::new(0usize, px(80.)),
            VirtualizerSizeUpdate::new(1usize, px(0.)),
        ],
    ] {
        assert!(state.update_sizes(&batch).is_err());
    }
    let after = state
        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
        .unwrap();
    assert_eq!(before.revision(), after.revision());
    assert_eq!(after.item(0).unwrap().item().extent(), px(40.));
}

#[gpui_kit::test]
fn active_scroll_deadline_moves_without_early_end_and_can_restart(cx: &mut TestAppContext) {
    let state = cx.new(|_| fixed(100));
    let ends = Rc::new(Cell::new(0));
    let observed = ends.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&state, move |_, event, _| {
            if matches!(event, VirtualizerEvent::ScrollEnd) {
                observed.set(observed.get() + 1);
            }
        })
    });
    for _ in 0..300 {
        state.update(cx, |s, cx| s.user_scroll_activity(cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(1));
    }
    assert_eq!(ends.get(), 0);
    cx.executor().advance_clock(Duration::from_millis(118));
    cx.run_until_parked();
    assert_eq!(ends.get(), 0);
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(ends.get(), 1);
    state.update(cx, |s, cx| s.user_scroll_activity(cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(120));
    cx.run_until_parked();
    assert_eq!(ends.get(), 2);
}

#[test]
fn measured_native_small_steps_survive_large_cumulative_coordinates() {
    use gpui_kit::{ListAlignment, ListOffset, ListState};
    let list = ListState::new_with_uniform_height(1000, ListAlignment::Top, px(0.), px(20.25));
    list.splice_with_size_hints(0..1, [px(16_777_216.)]);
    list.scroll_to(ListOffset {
        item_ix: 600,
        offset_in_item: px(0.),
    });
    for step in 1..=4 {
        list.scroll_by(px(0.25));
        let position = list.logical_scroll_top();
        assert_eq!(position.item_ix, 600);
        assert_eq!(position.offset_in_item, px(step as f32 * 0.25));
    }
    list.scroll_by(px(-1.));
    assert_eq!(list.logical_scroll_top().offset_in_item, px(0.));
}

#[test]
fn measured_precise_tree_queries_and_frozen_geometry_keep_fractional_prefixes() {
    let list = ListState::new_with_uniform_height(1000, ListAlignment::Top, px(0.), px(20.25));
    list.splice_with_size_hints(0..1, [px(16_777_216.)]);
    let frozen = list.geometry();
    for ix in [1, 2, 31, 600, 999] {
        let start = 16_777_216. + (ix - 1) as f64 * 20.25;
        assert_eq!(frozen.item(ix).unwrap().precise_start(), start);
        assert_eq!(frozen.precise_range(start + 0.25, 20.), ix..ix + 1);
    }
    assert_eq!(frozen.precise_total_extent(), 16_777_216. + 999. * 20.25);
    list.set_item_sizes(&[(1, gpui_kit::size(px(100.), px(20.5)))], true);
    assert_eq!(
        frozen.item(600).unwrap().precise_start(),
        16_777_216. + 599. * 20.25
    );
    assert_eq!(
        list.geometry().item(600).unwrap().precise_start(),
        16_777_216. + 599. * 20.25 + 0.25
    );
}

#[gpui_kit::test]
fn measured_large_coordinates_keep_native_input_local_bounds_and_hits(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, ScrollDelta, ScrollWheelEvent, TouchPhase, point};
    struct Harness {
        state: Entity<VirtualizerState>,
        clicked: Rc<Cell<usize>>,
    }
    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let clicked = self.clicked.clone();
            VirtualList::new("measured-precise", &self.state, move |item, _, _| {
                let clicked = clicked.clone();
                let ix = item.index();
                div()
                    .id("click")
                    .w(px(20.))
                    .h(px(20.))
                    .on_mouse_down(MouseButton::Left, move |_, _, _| clicked.set(ix))
            })
            .w(px(240.))
            .h(px(100.))
        }
    }
    for axis in [Axis::Vertical, Axis::Horizontal] {
        let clicked = Rc::new(Cell::new(0));
        let (view, cx) = cx.add_window_view(|_, cx| Harness {
            state: cx.new(|_| {
                VirtualizerState::new(
                    axis,
                    source(0, 1_000_000),
                    VirtualizerSizing::measured(px(20.)),
                )
                .unwrap()
            }),
            clicked: clicked.clone(),
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        state.update(cx, |s, cx| {
            s.scroll_to_key(
                838_861usize.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
            )
            .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        for step in [1., 2., 3., 4., 3., 2., 1., 0.] {
            let old = state.read_with(cx, |s, _| s.viewport().unwrap().offset());
            let goal = 16_777_220. + step * 0.25;
            let delta = Pixels::from(old - goal);
            cx.simulate_event(ScrollWheelEvent {
                position: point(px(50.), px(50.)),
                delta: ScrollDelta::Pixels(if axis == Axis::Vertical {
                    point(px(0.), delta)
                } else {
                    point(delta, px(0.))
                }),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            assert_eq!(
                state.read_with(cx, |s, _| s.viewport().unwrap().offset()),
                goal
            );
            let bounds = cx.update(|w, _| {
                snapshots(w)
                    .into_iter()
                    .find(|n| n.path().last() == Some(&838_862usize.into()))
                    .unwrap()
                    .bounds()
            });
            let start = if axis == Axis::Vertical {
                bounds.top()
            } else {
                bounds.left()
            };
            assert_eq!(
                start,
                cx.update(|w, _| w.pixel_snap(Pixels::from(20. - step * 0.25)))
            );
        }
        cx.simulate_event(MouseDownEvent {
            position: if axis == Axis::Vertical {
                point(px(5.), px(21.))
            } else {
                point(px(21.), px(5.))
            },
            button: MouseButton::Left,
            ..Default::default()
        });
        assert_eq!(clicked.get(), 838_862);
        state.update(cx, |s, cx| {
            s.scroll_to_key(
                999_999usize.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::End),
            )
            .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let v = state.read_with(cx, |s, _| s.viewport().unwrap().clone());
        assert_eq!(v.offset(), 20_000_000. - f64::from(v.extent()));
        assert_eq!(
            state.read_with(cx, |s, _| s.snapshot(&v).unwrap().visible_range().end),
            1_000_000
        );
    }
}

#[gpui_kit::test]
fn masonry_blank_region_anchors_survive_padding_gap_and_trailing_changes(cx: &mut TestAppContext) {
    use yue_component::{VirtualMasonry, VirtualMasonryState, VirtualizerSizeUpdate};
    struct Harness(Entity<VirtualMasonryState>);
    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            VirtualMasonry::new("blank-anchor", &self.0, |_, _, _| div().h(px(20.)))
                .w(px(300.))
                .h(px(100.))
        }
    }
    let mut failures = Vec::new();
    for (case, offset, expected) in [(0, 5., 15.), (1, 50., 60.), (2, 3095., 3135.)] {
        let (view, cx) = cx.add_window_view(|_, cx| {
            Harness(cx.new(|_| {
                let mut state =
                    VirtualMasonryState::new(source(0, 12), VirtualizerSizing::fixed(px(20.)), 3)
                        .unwrap();
                state
                    .set_layout(
                        VirtualizerLayout::default()
                            .gap(px(1000.))
                            .padding(px(10.), px(2000.)),
                        px(0.),
                    )
                    .unwrap();
                state
            }))
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.0.clone());
        state.read_with(cx, |s, _| {
            s.scroll_handle()
                .set_precise_offset(gpui_kit::point(0., -offset))
        });
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        state.update(cx, |s, _| {
            if case == 0 {
                s.set_layout(
                    VirtualizerLayout::default()
                        .gap(px(1000.))
                        .padding(px(20.), px(2000.)),
                    px(0.),
                )
                .unwrap();
            } else {
                s.update_sizes(
                    &(0usize..if case == 1 { 3 } else { 12 })
                        .map(|i| VirtualizerSizeUpdate::new(i, px(30.)))
                        .collect::<Vec<_>>(),
                )
                .unwrap();
            }
        });
        let actual = state.read_with(cx, |s, _| -s.scroll_handle().precise_offset().y);
        if actual != expected {
            failures.push((case, actual, expected));
        }
    }
    assert!(failures.is_empty(), "blank region anchors: {failures:?}");
}

#[gpui_kit::test]
fn masonry_commands_finish_once_on_settle_cancel_missing_and_stall(cx: &mut TestAppContext) {
    use yue_component::{VirtualMasonry, VirtualMasonryState};
    struct Harness(Entity<VirtualMasonryState>, bool);
    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let invalid = self.1;
            VirtualMasonry::new("command-results", &self.0, move |_, _, _| {
                div().h(px(if invalid { 0. } else { 20. }))
            })
            .w(px(300.))
            .h(px(100.))
        }
    }
    for invalid in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| {
            Harness(
                cx.new(|_| {
                    VirtualMasonryState::new(
                        source(0, 1000),
                        if invalid {
                            VirtualizerSizing::measured(px(20.))
                        } else {
                            VirtualizerSizing::fixed(px(20.))
                        },
                        3,
                    )
                    .unwrap()
                }),
                invalid,
            )
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.0.clone());
        let results = Rc::new(std::cell::RefCell::new(Vec::new()));
        let observed = results.clone();
        let _subscription = cx.update(|_, cx| {
            cx.subscribe(&state, move |_, e, _| {
                if let VirtualizerEvent::CommandFinished { id, outcome } = e {
                    observed.borrow_mut().push((*id, *outcome));
                }
            })
        });
        let id = state.update(cx, |s, cx| {
            let id = s
                .scroll_to_key(400usize.into(), VirtualizerAlign::Start)
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
        assert_eq!(
            *results.borrow(),
            vec![(
                id,
                if invalid {
                    VirtualizerCommandOutcome::NoProgress
                } else {
                    VirtualizerCommandOutcome::Settled
                }
            )]
        );
        assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
        let cancelled = state.update(cx, |s, _| {
            let id = s
                .scroll_to_key(500usize.into(), VirtualizerAlign::Start)
                .unwrap();
            s.cancel_scroll();
            s.cancel_scroll();
            id
        });
        let events = state.update(cx, |s, _| s.take_events());
        assert!(
            matches!(&events[..], [VirtualizerEvent::CommandFinished { id, outcome: VirtualizerCommandOutcome::Cancelled }] if *id == cancelled)
        );
        let missing = state.update(cx, |s, _| {
            let id = s
                .scroll_to_key(500usize.into(), VirtualizerAlign::Start)
                .unwrap();
            s.replace_source(
                source(0, 10),
                if invalid {
                    VirtualizerSizing::measured(px(20.))
                } else {
                    VirtualizerSizing::fixed(px(20.))
                },
            )
            .unwrap();
            id
        });
        let events = state.update(cx, |s, _| s.take_events());
        assert!(
            matches!(&events[..], [VirtualizerEvent::CommandFinished { id, outcome: VirtualizerCommandOutcome::Missing }] if *id == missing)
        );
    }
}
