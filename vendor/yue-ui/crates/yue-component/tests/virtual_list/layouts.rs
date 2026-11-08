use super::*;
use yue_component::{
    VirtualGrid, VirtualGridSnapshot, VirtualMasonry, VirtualMasonryState, VirtualizerSizeUpdate,
};

#[test]
fn grid_workset_is_the_product_of_visible_axes_not_the_dataset() {
    let binding = VirtualizerBinding::new();
    let rows = VirtualizerState::new_attached(
        Axis::Vertical,
        source(0, 1_000_000),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let columns = VirtualizerState::new_attached(
        Axis::Horizontal,
        source(0, 1_000_000),
        VirtualizerSizing::fixed(px(50.)),
        &binding,
    )
    .unwrap();
    let grid = VirtualGridSnapshot::new(
        rows.snapshot(&viewport(&rows, 1, 2_000.)).unwrap(),
        columns
            .snapshot(&VirtualizerViewport::new(
                &binding,
                1,
                3_000.,
                px(240.),
                px(100.),
            ))
            .unwrap(),
    )
    .unwrap();
    let cells: Vec<_> = grid.cells().collect();
    assert_eq!(cells.len(), 25);
    assert_eq!(cells[0].row().index(), 100);
    assert_eq!(cells[0].column().index(), 60);
    assert_eq!(cells.last().unwrap().row().index(), 104);
}

struct GridView {
    rows: Entity<VirtualizerState>,
    columns: Entity<VirtualizerState>,
    scroll: ScrollHandle,
    builds: Rc<Cell<usize>>,
}
impl Render for GridView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.builds.clone();
        VirtualGrid::new(
            "production-grid",
            &self.rows,
            &self.columns,
            &self.scroll,
            move |cell, _, _| {
                builds.set(builds.get() + 1);
                div().id("cell-body").role(Role::Cell).child(format!(
                    "{}:{}",
                    cell.row().index(),
                    cell.column().index()
                ))
            },
        )
        .w(px(240.))
        .h(px(100.))
        .role(Role::Grid)
        .test_support()
    }
}
#[gpui_kit::test]
fn production_grid_merges_axis_commands_and_native_wheel_in_one_scroll_owner(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let binding = VirtualizerBinding::new();
        GridView {
            rows: cx.new(|_| {
                VirtualizerState::new_attached(
                    Axis::Vertical,
                    source(0, 1_000_000),
                    VirtualizerSizing::fixed(px(20.)),
                    &binding,
                )
                .unwrap()
            }),
            columns: cx.new(|_| {
                VirtualizerState::new_attached(
                    Axis::Horizontal,
                    source(0, 100_000),
                    VirtualizerSizing::fixed(px(50.)),
                    &binding,
                )
                .unwrap()
            }),
            scroll: ScrollHandle::new(),
            builds: Rc::new(Cell::new(0)),
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (rows, columns, scroll, builds) = view.read_with(cx, |v, _| {
        (
            v.rows.clone(),
            v.columns.clone(),
            v.scroll.clone(),
            v.builds.clone(),
        )
    });
    assert!(builds.get() <= 50);
    for (state, key) in [(&rows, 1_000usize), (&columns, 2_000usize)] {
        state.update(cx, |s, cx| {
            s.scroll_to_key(
                key.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
            )
            .unwrap();
            cx.notify();
        });
    }
    builds.set(0);
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        scroll.offset(),
        gpui_kit::point(px(-100_000.), px(-20_000.))
    );
    assert!(!rows.read_with(cx, |s, _| s.has_scroll_command()));
    assert!(!columns.read_with(cx, |s, _| s.has_scroll_command()));
    assert!(builds.get() <= 50);
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-25.), px(-10.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        scroll.offset(),
        gpui_kit::point(px(-100_025.), px(-20_010.))
    );
    assert!(rows.read_with(cx, |s, _| s.is_scrolling()));
    let nodes = cx.update(|w, _| snapshots(w));
    assert!(nodes.iter().any(|node| node.role() == Some(Role::Grid)));
    // Replacing one axis under the same ElementId must not cancel the other axis.
    columns.update(cx, |s, _| s.detach());
    let binding = rows.read_with(cx, |s, _| s.binding().unwrap().clone());
    let replacement = cx.update(|_, cx| {
        cx.new(|_| {
            VirtualizerState::new_attached(
                Axis::Horizontal,
                source(0, 10),
                VirtualizerSizing::fixed(px(50.)),
                &binding,
            )
            .unwrap()
        })
    });
    rows.update(cx, |s, _| {
        s.scroll_to_key(
            1_200usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap()
    });
    view.update(cx, |v, cx| {
        v.columns = replacement.clone();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(scroll.offset().y, px(-24_000.));
    assert!(columns.read_with(cx, |s, _| s.viewport().is_none()));
    assert!(replacement.read_with(cx, |s, _| s.viewport().is_some()));
}

#[gpui_kit::test]
fn grid_smooth_axes_share_frames_and_honor_input_and_motion_preferences(cx: &mut TestAppContext) {
    for case in 0..5 {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let binding = VirtualizerBinding::new();
            GridView {
                rows: cx.new(|_| {
                    VirtualizerState::new_attached(
                        Axis::Vertical,
                        source(0, 1_000_000),
                        VirtualizerSizing::fixed(px(20.)),
                        &binding,
                    )
                    .unwrap()
                }),
                columns: cx.new(|_| {
                    VirtualizerState::new_attached(
                        Axis::Horizontal,
                        source(0, 1000),
                        VirtualizerSizing::known(vec![px(50.); 1000]),
                        &binding,
                    )
                    .unwrap()
                }),
                scroll: ScrollHandle::new(),
                builds: Rc::new(Cell::new(0)),
            }
        });
        cx.update(|w, cx| {
            cx.set_reduce_motion(false);
            w.activate_window();
        });
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let (rows, columns, scroll, builds) = view.read_with(cx, |v, _| {
            (
                v.rows.clone(),
                v.columns.clone(),
                v.scroll.clone(),
                v.builds.clone(),
            )
        });
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut subscriptions = Vec::new();
        for (axis, state, key) in [
            (Axis::Vertical, &rows, 4usize),
            (Axis::Horizontal, &columns, 2usize),
        ] {
            assert!(state.read_with(cx, |s, _| s.capabilities().supports_smooth()));
            let observed = events.clone();
            subscriptions.push(cx.update(|_, cx| {
                cx.subscribe(state, move |_, event, _| {
                    if let VirtualizerEvent::CommandFinished { outcome, .. } = event {
                        observed.borrow_mut().push((axis, *outcome));
                    }
                })
            }));
            state.update(cx, |s, cx| {
                let options = VirtualizerScrollOptions::default()
                    .align(VirtualizerAlign::Start)
                    .motion(VirtualizerMotion::Smooth {
                        duration: Duration::from_millis(200),
                        max_distance: px(200.),
                    });
                if case == 4 {
                    s.scroll_to_offset(0.1, options).unwrap();
                } else {
                    s.scroll_to_key(key.into(), options).unwrap();
                }
                cx.notify();
            });
        }
        builds.set(0);
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(scroll.precise_offset(), gpui_kit::point(0., 0.));
        assert!(events.borrow().is_empty());
        if case == 4 {
            for _ in 0..64 {
                if !rows.read_with(cx, |s, _| s.has_scroll_command())
                    && !columns.read_with(cx, |s, _| s.has_scroll_command())
                {
                    break;
                }
                cx.executor().advance_clock(Duration::from_millis(2));
                cx.update(|w, cx| {
                    assert!(w.simulate_next_frame(cx) > 0);
                    w.draw(cx).clear(cx);
                });
            }
            assert_eq!(
                *events.borrow(),
                vec![
                    (Axis::Vertical, VirtualizerCommandOutcome::Settled),
                    (Axis::Horizontal, VirtualizerCommandOutcome::Settled)
                ],
                "advancing tiny motion must not be classified as a stalled host"
            );
            assert_eq!(scroll.precise_offset(), gpui_kit::point(-0.1, -0.1));
            continue;
        }
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.update(|w, cx| {
            assert!(w.simulate_next_frame(cx) > 0);
            w.draw(cx).clear(cx);
        });
        let middle = scroll.precise_offset();
        assert!((middle.x + 100. * (1. - 0.75f64.powi(3))).abs() < 0.001);
        assert!((middle.y + 80. * (1. - 0.75f64.powi(3))).abs() < 0.001);
        assert!(events.borrow().is_empty());
        assert!(
            builds.get() <= 100,
            "grid must only build the current cell workset"
        );
        match case {
            0 => {
                cx.executor().advance_clock(Duration::from_millis(150));
                cx.update(|w, cx| {
                    assert!(w.simulate_next_frame(cx) > 0);
                    w.draw(cx).clear(cx);
                });
            }
            1 => {
                cx.simulate_event(gpui_kit::ScrollWheelEvent {
                    position: gpui_kit::point(px(100.), px(50.)),
                    delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-5.), px(-10.))),
                    modifiers: Default::default(),
                    touch_phase: gpui_kit::TouchPhase::Moved,
                });
                cx.update(|w, cx| w.draw(cx).clear(cx));
            }
            2 => {
                cx.deactivate_window();
                cx.update(|w, cx| w.draw(cx).clear(cx));
            }
            _ => {
                cx.update(|_, cx| cx.set_reduce_motion(true));
                cx.update(|w, cx| w.draw(cx).clear(cx));
            }
        }
        let outcome = if case == 1 {
            VirtualizerCommandOutcome::Cancelled
        } else {
            VirtualizerCommandOutcome::Settled
        };
        assert_eq!(
            *events.borrow(),
            vec![(Axis::Vertical, outcome), (Axis::Horizontal, outcome)],
            "case {case}"
        );
        assert_eq!(
            scroll.precise_offset(),
            if case == 1 {
                middle + gpui_kit::point(-5., -10.)
            } else {
                gpui_kit::point(-100., -80.)
            }
        );
        assert!(!rows.read_with(cx, |s, _| s.has_scroll_command()));
        assert!(!columns.read_with(cx, |s, _| s.has_scroll_command()));
        cx.update(|w, cx| {
            w.simulate_next_frame(cx);
            w.draw(cx).clear(cx);
        });
        assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
    }
}

#[test]
fn masonry_packs_shortest_lanes_updates_locally_and_keeps_frozen_snapshots() {
    let mut state = VirtualMasonryState::new(
        source(0, 6),
        VirtualizerSizing::known(vec![px(100.), px(40.), px(60.), px(20.), px(70.), px(30.)]),
        2,
    )
    .unwrap();
    state
        .set_layout(VirtualizerLayout::default().gap(px(10.)), px(12.))
        .unwrap();
    let old = state
        .snapshot(0., gpui_kit::size(px(412.), px(100.)))
        .unwrap();
    assert_eq!(old.item(0).unwrap().cross_extent(), px(200.));
    assert_eq!(old.item(1).unwrap().cross_start(), px(212.));
    assert_eq!(old.item(2).unwrap().lane(), 1);
    assert!(old.working_indices().len() < 6);
    let old_start = old.item(2).unwrap().item().start();
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(1usize, px(80.))])
        .unwrap();
    let new = state
        .snapshot(0., gpui_kit::size(px(412.), px(100.)))
        .unwrap();
    assert_eq!(old.item(2).unwrap().item().start(), old_start);
    assert_eq!(new.item(2).unwrap().item().start(), old_start + 40.);
    assert_eq!(new.item(2).unwrap().lane(), old.item(2).unwrap().lane());
    assert_eq!(
        state.update_sizes_from_snapshot(&old, &[]),
        Err(VirtualizerError::StaleSnapshot)
    );
    let before = state.total_extent();
    assert!(
        state
            .update_sizes(&[
                VirtualizerSizeUpdate::new(0usize, px(5.)),
                VirtualizerSizeUpdate::new(1usize, px(f32::NAN))
            ])
            .is_err()
    );
    assert_eq!(state.total_extent(), before);
    state.reflow(3).unwrap();
    assert_eq!(state.lane_count(), 3);
    assert!(state.reflow(0).is_err());
    assert!(
        state
            .snapshot(0., gpui_kit::size(px(0.), px(100.)))
            .unwrap()
            .working_indices()
            .is_empty()
    );
}

struct MasonryView {
    state: Entity<VirtualMasonryState>,
    builds: Rc<Cell<usize>>,
    children: usize,
}
impl Render for MasonryView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.builds.clone();
        let children = self.children;
        VirtualMasonry::new("production-masonry", &self.state, move |item, _, _| {
            builds.set(builds.get() + 1);
            div()
                .h(px(30. + (item.item().index() % 4) as f32 * 10.))
                .flex()
                .children((0..children).map(|ix| div().child(format!("{ix}"))))
        })
        .w(px(300.))
        .h(px(100.))
    }
}
#[gpui_kit::test]
fn production_masonry_measures_only_working_cards_and_scrolls_to_a_key(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut state =
            VirtualMasonryState::new(source(0, 100_000), VirtualizerSizing::measured(px(40.)), 3)
                .unwrap();
        state
            .set_layout(
                VirtualizerLayout::default().gap(px(4.)).overscan(px(12.)),
                px(6.),
            )
            .unwrap();
        MasonryView {
            state: cx.new(|_| state),
            builds: Rc::new(Cell::new(0)),
            children: 2,
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, builds) = view.read_with(cx, |v, _| (v.state.clone(), v.builds.clone()));
    assert!(builds.get() < 40, "{}", builds.get());
    let snapshot = state.read_with(cx, |s, _| {
        s.snapshot(0., gpui_kit::size(px(300.), px(100.))).unwrap()
    });
    for ix in snapshot.working_indices() {
        assert!(snapshot.item(ix).unwrap().item().is_measured());
    }
    state.update(cx, |s, cx| {
        s.scroll_to_key(99_999usize.into(), VirtualizerAlign::End)
            .unwrap();
        cx.notify();
    });
    builds.set(0);
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let snapshot = state.read_with(cx, |s, _| {
        s.snapshot(
            -f64::from(s.scroll_handle().offset().y),
            gpui_kit::size(px(300.), px(100.)),
        )
        .unwrap()
    });
    assert!(snapshot.working_indices().contains(&99_999));
    assert!(builds.get() < 50, "{}", builds.get());
    assert!(snapshot.item(99_999).unwrap().item().is_measured());
}

#[test]
fn masonry_replacement_retains_keyed_real_sizes_and_bounds_empty_layouts() {
    let mut state =
        VirtualMasonryState::new(source(0, 100), VirtualizerSizing::measured(px(40.)), 3).unwrap();
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(50usize, px(90.))])
        .unwrap();
    state
        .replace_source(source(40, 100), VirtualizerSizing::measured(px(40.)))
        .unwrap();
    let snapshot = state
        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
        .unwrap();
    assert_eq!(snapshot.item(10).unwrap().item().extent(), px(90.));
    assert!(snapshot.item(10).unwrap().item().is_measured());
    state
        .replace_source(
            source(40, 100),
            VirtualizerSizing::known(vec![px(25.); 100]),
        )
        .unwrap();
    assert_eq!(
        state
            .snapshot(0., gpui_kit::size(px(300.), px(100.)))
            .unwrap()
            .item(10)
            .unwrap()
            .item()
            .extent(),
        px(25.)
    );
    state
        .set_layout(VirtualizerLayout::default(), px(150.))
        .unwrap();
    assert!(
        state
            .snapshot(0., gpui_kit::size(px(300.), px(100.)))
            .unwrap()
            .working_indices()
            .is_empty()
    );
    state
        .replace_source(source(0, 0), VirtualizerSizing::measured(px(40.)))
        .unwrap();
    assert_eq!(state.total_extent(), 0.);
    assert!(
        state
            .snapshot(0., gpui_kit::size(px(300.), px(100.)))
            .unwrap()
            .working_indices()
            .is_empty()
    );
}

#[test]
fn grid_rejects_foreign_hosts_stale_axes_and_mismatched_dimensions() {
    let binding = VirtualizerBinding::new();
    let rows = VirtualizerState::new_attached(
        Axis::Vertical,
        source(0, 100),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let columns = VirtualizerState::new_attached(
        Axis::Horizontal,
        source(0, 100),
        VirtualizerSizing::fixed(px(50.)),
        &binding,
    )
    .unwrap();
    let rv = VirtualizerViewport::new(&binding, 1, 0., px(100.), px(240.));
    for cv in [
        VirtualizerViewport::new(&binding, 2, 0., px(240.), px(100.)),
        VirtualizerViewport::new(&binding, 1, 0., px(240.), px(101.)),
    ] {
        assert!(
            VirtualGridSnapshot::new(rows.snapshot(&rv).unwrap(), columns.snapshot(&cv).unwrap())
                .is_err()
        );
    }
    let foreign = VirtualizerState::new(
        Axis::Horizontal,
        source(0, 100),
        VirtualizerSizing::fixed(px(50.)),
    )
    .unwrap();
    let cv = VirtualizerViewport::new(foreign.binding().unwrap(), 1, 0., px(240.), px(100.));
    assert!(
        VirtualGridSnapshot::new(rows.snapshot(&rv).unwrap(), foreign.snapshot(&cv).unwrap())
            .is_err()
    );
}

struct MasonryEdges {
    state: Entity<VirtualMasonryState>,
    focus: gpui_kit::FocusHandle,
    fallback: gpui_kit::FocusHandle,
    height: f32,
    builds: Rc<Cell<usize>>,
}
impl Render for MasonryEdges {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let height = self.height;
        let builds = self.builds.clone();
        div().track_focus(&self.fallback).child(
            VirtualMasonry::new("masonry-edges", &self.state, move |item, _, _| {
                builds.set(builds.get() + 1);
                let row = div().h(px(height));
                if item.item().key() == &ElementId::from(0usize) {
                    row.track_focus(&focus)
                } else {
                    row
                }
            })
            .w(px(300.))
            .h(px(100.)),
        )
    }
}
#[gpui_kit::test]
fn masonry_budget_invalid_sizes_and_removed_focus_have_bounded_results(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let focus = cx.focus_handle();
        let fallback = cx.focus_handle();
        let mut state =
            VirtualMasonryState::new(source(0, 10_000), VirtualizerSizing::measured(px(40.)), 3)
                .unwrap();
        state
            .set_layout(VirtualizerLayout::default().measurement_budget(2), px(0.))
            .unwrap();
        state
            .set_focus_handle(0usize.into(), Some(focus.clone()))
            .unwrap();
        state.set_focus_fallback(Some(fallback.clone()));
        MasonryEdges {
            state: cx.new(|_| state),
            focus,
            fallback,
            height: 0.,
            builds: Rc::new(Cell::new(0)),
        }
    });
    let events = Rc::new(std::cell::RefCell::new(Vec::new()));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let subscription = cx.update(|_, cx| {
        cx.subscribe(&state, {
            let events = events.clone();
            move |_, event, _| events.borrow_mut().push(event.clone())
        })
    });
    // add_window_view may already draw; start a fresh failure episode after subscribing.
    view.update(cx, |v, cx| {
        v.height = 30.;
        cx.notify();
    });
    state.update(cx, |s, _| {
        s.set_layout(VirtualizerLayout::default(), px(0.)).unwrap()
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    events.borrow_mut().clear();
    // Consume a continuation requested by the preceding valid correction pass.
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
    });
    view.update(cx, |v, cx| {
        v.height = 0.;
        cx.notify();
    });
    state.update(cx, |s, _| {
        s.set_layout(VirtualizerLayout::default().measurement_budget(2), px(0.))
            .unwrap()
    });
    for _ in 0..3 {
        view.read_with(cx, |v, _| v.builds.set(0));
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        let working = state.read_with(cx, |s, _| {
            s.snapshot(0., gpui_kit::size(px(300.), px(100.)))
                .unwrap()
                .working_indices()
                .len()
        });
        assert_eq!(view.read_with(cx, |v, _| v.builds.get()), working);
        assert_eq!(
            cx.update(|w, cx| w.simulate_next_frame(cx)),
            0,
            "invalid sizes must not create a self-sustaining frame loop"
        );
    }
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(
                e,
                VirtualizerEvent::Diagnostic("masonry measurement budget exhausted")
            ))
            .count(),
        0 // no extra correction work: all required roots were attempted
    );
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(
                e,
                VirtualizerEvent::Diagnostic("invalid masonry extent; using estimate")
            ))
            .count(),
        1
    );
    assert!(!state.read_with(cx, |s, _| {
        s.snapshot(0., gpui_kit::size(px(300.), px(100.)))
            .unwrap()
            .item(0)
            .unwrap()
            .item()
            .is_measured()
    }));
    view.update(cx, |v, cx| {
        v.height = 30.;
        cx.notify();
    });
    state.update(cx, |s, _| {
        s.set_layout(VirtualizerLayout::default(), px(0.)).unwrap()
    });
    cx.update(|w, cx| {
        view.read(cx).focus.clone().focus(w, cx);
        w.draw(cx).clear(cx);
    });
    state.update(cx, |s, cx| {
        s.scroll_to_key(9_999usize.into(), VirtualizerAlign::End)
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(cx.update(|w, cx| view.read(cx).focus.is_focused(w)));
    state.update(cx, |s, cx| {
        s.replace_source(source(1, 9_999), VirtualizerSizing::measured(px(40.)))
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(cx.update(|w, cx| view.read(cx).fallback.is_focused(w)));
    drop(subscription);
}
