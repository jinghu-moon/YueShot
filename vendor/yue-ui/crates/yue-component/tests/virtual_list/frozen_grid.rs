use super::*;
use std::{cell::RefCell, collections::HashSet};
use yue_component::VirtualGrid;

type ObservedCell = (usize, usize, bool, bool);

struct OrderedSource(Vec<usize>);
impl VirtualizerSource for OrderedSource {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key_at(&self, index: usize) -> ElementId {
        self.0[index].into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        self.0
            .iter()
            .position(|value| ElementId::from(*value) == *key)
    }
}

#[gpui_kit::test]
fn frozen_grid_splice_and_reorder_preserve_the_body_key_on_both_axes(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, true));
    view.update(cx, |v, cx| {
        v.frozen = (3, 3);
        v.scroll
            .set_precise_offset(gpui_kit::point(-213.25, -103.5));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let axes = view.read_with(cx, |v, _| [v.rows.clone(), v.columns.clone()]);
    let relative = |s: &VirtualizerState| {
        let v = s.viewport().unwrap();
        let snap = s.snapshot(v).unwrap();
        snap.item_for_key(&ElementId::from(7usize)).unwrap().start()
            - v.offset()
            - snap.item(3).unwrap().start()
    };
    let baseline = axes
        .clone()
        .map(|axis| axis.read_with(cx, |s, _| relative(s)));
    let mut keys: Vec<usize> = (0..100).collect();
    for step in 0..3 {
        match step {
            0 => keys.insert(6, 1000),
            1 => {
                keys.remove(4);
            }
            _ => keys.swap(4, 5),
        }
        for (axis, extent) in axes.iter().zip([20.5, 50.25]) {
            axis.update(cx, |s, cx| {
                s.replace_source(
                    Rc::new(OrderedSource(keys.clone())),
                    VirtualizerSizing::known(vec![px(extent); keys.len()]),
                    VirtualizerChange::Replace {
                        preserve_anchor: true,
                    },
                )
                .unwrap();
                cx.notify();
            });
        }
        cx.update(|w, cx| w.draw(cx).clear(cx));
        for (axis, expected) in axes.iter().zip(baseline) {
            assert!((axis.read_with(cx, |s, _| relative(s)) - expected).abs() < 0.001);
        }
    }
}

struct Harness {
    rows: Entity<VirtualizerState>,
    columns: Entity<VirtualizerState>,
    scroll: ScrollHandle,
    focus: gpui_kit::FocusHandle,
    frozen: (usize, usize),
    clicks: Rc<RefCell<Vec<(usize, usize)>>>,
    seen: Rc<RefCell<Vec<ObservedCell>>>,
    viewport: (f32, f32),
    rtl: bool,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        let seen = self.seen.clone();
        let focus = self.focus.clone();
        VirtualGrid::new(
            "frozen-grid",
            &self.rows,
            &self.columns,
            &self.scroll,
            move |cell, _, _| {
                let key = (cell.row().index(), cell.column().index());
                seen.borrow_mut().push((
                    key.0,
                    key.1,
                    cell.is_frozen_row(),
                    cell.is_frozen_column(),
                ));
                let clicks = clicks.clone();
                let cell = div().id("cell").size_full();
                let cell = if key == (0, 0) {
                    cell.track_focus(&focus)
                } else {
                    cell
                };
                cell.role(Role::Cell)
                    .aria_label(format!("{}:{}", key.0, key.1))
                    .on_click(move |_, _, _| clicks.borrow_mut().push(key))
                    .test_support()
            },
        )
        .frozen_rows(self.frozen.0)
        .frozen_columns(self.frozen.1)
        .right_to_left(self.rtl)
        .w(px(self.viewport.0))
        .h(px(self.viewport.1))
        .role(Role::Grid)
        .test_support()
    }
}
fn harness(cx: &mut Context<Harness>, count: usize, known: bool) -> Harness {
    let binding = VirtualizerBinding::new();
    let focus = cx.focus_handle();
    let mut axis = |vertical: bool| {
        cx.new(|_| {
            let extent = if vertical { 20.5 } else { 50.25 };
            let mut s = VirtualizerState::new_attached(
                if vertical {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                },
                source(0, count),
                if known {
                    VirtualizerSizing::known(vec![px(extent); count])
                } else {
                    VirtualizerSizing::fixed(px(extent))
                },
                &binding,
            )
            .unwrap();
            s.set_layout(
                VirtualizerLayout::default()
                    .gap(px(1.5))
                    .padding(px(2.), px(3.)),
            )
            .unwrap();
            s.set_focus_handle(0usize.into(), Some(focus.clone()))
                .unwrap();
            s
        })
    };
    Harness {
        rows: axis(true),
        columns: axis(false),
        scroll: ScrollHandle::new(),
        frozen: (1, 1),
        clicks: Rc::default(),
        seen: Rc::default(),
        focus,
        viewport: (240., 100.),
        rtl: false,
    }
}
#[gpui_kit::test]
fn frozen_grid_four_regions_are_unique_clipped_and_route_native_input(cx: &mut TestAppContext) {
    for known in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, known));
        cx.update(|w, cx| w.draw(cx).clear(cx));
        view.update(cx, |v, cx| {
            v.scroll.set_precise_offset(gpui_kit::point(-75., -30.));
            v.seen.borrow_mut().clear();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let clicks = view.read_with(cx, |v, _| v.clicks.clone());
        for (point, key) in [
            ((10., 10.), (0, 0)),
            ((70., 10.), (0, 2)),
            ((10., 40.), (3, 0)),
            ((70., 40.), (3, 2)),
        ] {
            clicks.borrow_mut().clear();
            cx.simulate_click(
                gpui_kit::point(px(point.0), px(point.1)),
                Default::default(),
            );
            assert_eq!(clicks.borrow().as_slice(), &[key]);
        }
        // Frozen row 0 ends at 22.5. The boundary gap belongs to the strip;
        // scrolling rows behind it cannot receive a click through that gap.
        clicks.borrow_mut().clear();
        cx.simulate_click(gpui_kit::point(px(70.), px(23.)), Default::default());
        assert!(
            clicks.borrow().is_empty(),
            "body must be clipped before hit testing"
        );
        let nodes = cx.update(|w, _| snapshots(w));
        assert_eq!(
            nodes
                .iter()
                .filter(|n| n.role() == Some(Role::Grid))
                .count(),
            1
        );
        let corner = nodes.iter().find(|n| n.label() == Some("0:0")).unwrap();
        assert_eq!(corner.bounds().origin, gpui_kit::point(px(2.), px(2.)));
        let seen = view.read_with(cx, |v, _| v.seen.borrow().clone());
        let mut unique = HashSet::new();
        // A draw may contain multiple internal passes; the corner must occur
        // exactly as often as any other consistently present cell.
        let corner_count = seen.iter().filter(|c| c.0 == 0 && c.1 == 0).count();
        for &(r, c, fr, fc) in &seen {
            assert_eq!(fr, r == 0);
            assert_eq!(fc, c == 0);
            unique.insert((r, c));
        }
        assert_eq!(seen.len(), unique.len() * corner_count);
        // Preserve region stacking and row-major order within each region;
        // descendants may deliberately overflow adjacent cell bounds.
        for pass in seen.chunks(unique.len()) {
            assert!(pass.windows(2).all(|pair| {
                let order =
                    |&(r, c, fr, fc): &ObservedCell| (usize::from(fr) + 2 * usize::from(fc), r, c);
                order(&pair[0]) <= order(&pair[1])
            }));
        }
        let before = view.read_with(cx, |v, _| v.scroll.precise_offset());
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: gpui_kit::point(px(10.), px(10.)),
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-10.), px(-10.))),
            modifiers: Default::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            view.read_with(cx, |v, _| v.scroll.precise_offset()),
            before - gpui_kit::point(10., 10.)
        );
    }
}

#[gpui_kit::test]
fn rtl_frozen_grid_mirrors_hit_masks_and_keeps_one_logical_owner(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, true));
    view.update(cx, |v, cx| {
        v.rtl = true;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let axes = view.read_with(cx, |v, _| [v.rows.clone(), v.columns.clone()]);
    for (axis, offset) in axes.iter().zip([30., 75.]) {
        axis.update(cx, |s, cx| {
            s.scroll_to_offset(offset, VirtualizerScrollOptions::default())
                .unwrap();
            cx.notify();
        });
    }
    cx.update(|w, cx| w.draw(cx).clear(cx));
    for (point, key) in [
        ((230., 10.), (0, 0)),
        ((170., 10.), (0, 2)),
        ((230., 40.), (3, 0)),
        ((170., 40.), (3, 2)),
    ] {
        view.read_with(cx, |v, _| v.clicks.borrow_mut().clear());
        cx.simulate_click(
            gpui_kit::point(px(point.0), px(point.1)),
            Default::default(),
        );
        assert_eq!(
            *view.read_with(cx, |v, _| v.clicks.clone()).borrow(),
            vec![key]
        );
    }
    for point in [(170., 23.), (187., 40.)] {
        view.read_with(cx, |v, _| v.clicks.borrow_mut().clear());
        cx.simulate_click(
            gpui_kit::point(px(point.0), px(point.1)),
            Default::default(),
        );
        assert!(
            view.read_with(cx, |v, _| v.clicks.borrow().is_empty()),
            "RTL frozen gap must block body hit"
        );
    }
    for delta in [10., -10.] {
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: gpui_kit::point(px(100.), px(50.)),
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(delta), px(0.))),
            modifiers: Default::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let expected = if delta > 0. { 85. } else { 75. };
        assert!(
            (axes[1].read_with(cx, |s, _| s.viewport().unwrap().offset()) - expected).abs() < 0.001
        );
    }
    for rtl in [false, true] {
        view.update(cx, |v, cx| {
            v.rtl = rtl;
            v.viewport = (280., 135.);
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            axes[1].read_with(cx, |s, _| s.viewport().unwrap().offset()),
            75.
        );
        let maximum = axes[1].read_with(cx, |s, _| s.total_extent()) - 280.;
        assert!(
            (view.read_with(cx, |v, _| -v.scroll.precise_offset().x)
                - if rtl { maximum - 75. } else { 75. })
            .abs()
                < 0.001
        );
    }
    // An external scrollbar writes physical coordinates; Base observes logical 125.
    view.update(cx, |v, cx| {
        let maximum = v.columns.read(cx).total_extent() - 280.;
        v.scroll
            .set_precise_offset(gpui_kit::point(-(maximum - 125.), -30.));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        axes[1].read_with(cx, |s, _| s.viewport().unwrap().offset()),
        125.
    );
}

#[gpui_kit::test]
fn rtl_grid_alignments_smooth_and_large_logical_coordinates_stay_coherent(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, false));
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    view.update(cx, |v, cx| {
        v.rtl = true;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let axes = view.read_with(cx, |v, _| [v.rows.clone(), v.columns.clone()]);
    for align in [
        VirtualizerAlign::Start,
        VirtualizerAlign::Center,
        VirtualizerAlign::End,
        VirtualizerAlign::Nearest,
    ] {
        axes[1].update(cx, |s, cx| {
            s.scroll_to_key(
                5usize.into(),
                VirtualizerScrollOptions::default().align(align),
            )
            .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let nodes = cx.update(|w, _| snapshots(w));
        let cell = nodes.iter().find(|n| n.label() == Some("0:5")).unwrap();
        assert!(
            cell.bounds().origin.x >= px(0.) && cell.bounds().right() <= px(186.25) + px(0.001)
        );
    }
    let from = axes
        .clone()
        .map(|s| s.read_with(cx, |s, _| s.viewport().unwrap().offset()));
    for (axis, offset) in axes.iter().zip(from.map(|x| x + 20.)) {
        axis.update(cx, |s, cx| {
            s.scroll_to_offset(
                offset,
                VirtualizerScrollOptions::default().motion(VirtualizerMotion::Smooth {
                    duration: Duration::from_millis(200),
                    max_distance: px(100.),
                }),
            )
            .unwrap();
            cx.notify();
        });
    }
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    for (axis, from) in axes.iter().zip(from) {
        assert!(
            (axis.read_with(cx, |s, _| s.viewport().unwrap().offset())
                - (from + 20. * (1. - 0.75f64.powi(3))))
            .abs()
                < 0.01
        );
    }
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    axes[1].update(cx, |s, cx| {
        s.replace_source(
            source(0, 1_000_000_000),
            VirtualizerSizing::fixed(px(50.25)),
            VirtualizerChange::Replace {
                preserve_anchor: false,
            },
        )
        .unwrap();
        s.scroll_to_key(
            800_000_000usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let expected = 800_000_000f64 * 51.75 + 2. - 53.75;
    assert!(
        (axes[1].read_with(cx, |s, _| s.viewport().unwrap().offset()) - expected).abs() < 0.0001
    );
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.25), px(0.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        (axes[1].read_with(cx, |s, _| s.viewport().unwrap().offset()) - expected - 0.25).abs()
            < 0.0001
    );
}

#[gpui_kit::test]
fn frozen_grid_alignment_uses_body_clearance_without_mutating_axis_layout(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, false));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let rows = view.read_with(cx, |v, _| v.rows.clone());
    for align in [
        VirtualizerAlign::Start,
        VirtualizerAlign::Center,
        VirtualizerAlign::End,
        VirtualizerAlign::Nearest,
    ] {
        rows.update(cx, |s, cx| {
            s.scroll_to_key(
                50usize.into(),
                VirtualizerScrollOptions::default().align(align),
            )
            .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        rows.read_with(cx, |s, _| {
            let v = s.viewport().unwrap();
            let item = s.snapshot(v).unwrap().item(50).unwrap();
            assert!(
                item.start() - v.offset() >= 24. - 0.01,
                "body target behind frozen strip"
            );
            assert!(item.start() + f64::from(item.extent()) - v.offset() <= 100.01);
        });
    }
    let before = view.read_with(cx, |v, _| v.scroll.precise_offset());
    rows.update(cx, |s, cx| {
        s.scroll_to_key(
            0usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |v, _| v.scroll.precise_offset()),
        before,
        "already frozen key must not reset offset"
    );
    rows.update(cx, |s, cx| {
        s.scroll_to_offset(300., VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |v, _| -v.scroll.precise_offset().y),
        300.
    );
    view.update(cx, |v, cx| {
        v.frozen = (0, 0);
        cx.notify();
    });
    rows.update(cx, |s, cx| {
        s.scroll_to_key(
            50usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |v, _| -v.scroll.precise_offset().y),
        1102.
    );
}

#[gpui_kit::test]
fn frozen_grid_huge_prefix_and_hidden_target_remain_bounded(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 1_000_000, false));
    view.update(cx, |v, cx| {
        v.frozen = (usize::MAX, usize::MAX);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(view.read_with(cx, |v, _| v.seen.borrow().len()) < 100);
    let rows = view.read_with(cx, |v, _| v.rows.clone());
    rows.update(cx, |s, cx| {
        s.scroll_to_key(
            900_000usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        !rows.read_with(cx, |s, _| s.has_scroll_command()),
        "hidden frozen target must terminate"
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.scroll.precise_offset()),
        gpui_kit::point(0., 0.)
    );
    rows.update(cx, |s, cx| {
        s.replace_source(
            source(0, 0),
            VirtualizerSizing::fixed(px(20.5)),
            VirtualizerChange::Replace {
                preserve_anchor: false,
            },
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    rows.read_with(cx, |s, _| {
        assert!(s.snapshot(s.viewport().unwrap()).unwrap().is_empty())
    });
}

#[gpui_kit::test]
fn frozen_grid_focus_survives_toggle_and_geometry_change(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, true));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let focus = view.read_with(cx, |v, _| v.focus.clone());
    cx.update(|w, cx| focus.focus(w, cx));
    view.update(cx, |v, cx| {
        v.scroll
            .set_precise_offset(gpui_kit::point(-1_000., -1_000.));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(cx.update(|w, _| focus.is_focused(w)));
    view.update(cx, |v, cx| {
        v.frozen = (0, 0);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(cx.update(|w, _| focus.is_focused(w)));
    let rows = view.read_with(cx, |v, _| v.rows.clone());
    rows.update(cx, |s, cx| {
        s.update_sizes(&[yue_component::VirtualizerSizeUpdate::new(0usize, px(25.5))])
            .unwrap();
        cx.notify();
    });
    view.update(cx, |v, cx| {
        v.frozen = (1, 1);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(cx.update(|w, _| focus.is_focused(w)));
    assert_eq!(
        cx.update(|w, _| snapshots(w))
            .iter()
            .filter(|n| n.label() == Some("0:0"))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn frozen_grid_zero_body_area_rejects_inaccessible_body_target(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, false));
    view.update(cx, |v, cx| {
        v.frozen = (8, 1);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let rows = view.read_with(cx, |v, _| v.rows.clone());
    rows.update(cx, |s, cx| {
        s.scroll_to_key(50usize.into(), VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!rows.read_with(cx, |s, _| s.has_scroll_command()));
    assert_eq!(
        view.read_with(cx, |v, _| v.scroll.precise_offset()),
        gpui_kit::point(0., 0.)
    );
}

fn check_size_feedback_reading_position(cx: &mut TestAppContext, frozen_item: bool) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, true));
    // Three rows/columns obscure multiple ordinary items at a fractional
    // offset. The reading anchor belongs at the body's clear leading edge.
    view.update(cx, |v, cx| {
        v.frozen = (3, 3);
        v.scroll
            .set_precise_offset(gpui_kit::point(-213.25, -103.5));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (rows, columns, before) = view.read_with(cx, |v, _| {
        (v.rows.clone(), v.columns.clone(), v.scroll.precise_offset())
    });
    let anchor = |state: &Entity<VirtualizerState>,
                  edge: f64,
                  offset: f64,
                  cx: &mut gpui_kit::VisualTestContext| {
        state.read_with(cx, |s, _| {
            let snap = s.snapshot(s.viewport().unwrap()).unwrap();
            let item = snap.item(snap.visible_range().start).unwrap();
            (item.index(), item.start() - offset - edge)
        })
    };
    let row_before = anchor(&rows, 68., -before.y, cx);
    let column_before = anchor(&columns, 157.25, -before.x, cx);
    for (axis, state, reading) in [(true, &rows, row_before), (false, &columns, column_before)] {
        state.update(cx, |s, cx| {
            let ix = if frozen_item { 0 } else { reading.0 - 1 };
            let extent = if axis { 20.5 } else { 50.25 };
            s.update_sizes(&[yue_component::VirtualizerSizeUpdate::new(
                ix,
                px(extent + 7.25),
            )])
            .unwrap();
            cx.notify();
        });
    }
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let after = view.read_with(cx, |v, _| v.scroll.precise_offset());
    let strip_delta = if frozen_item { 7.25 } else { 0. };
    let row_after = anchor(&rows, 68. + strip_delta, -after.y, cx);
    let column_after = anchor(&columns, 157.25 + strip_delta, -after.x, cx);
    println!(
        "FROZEN_ANCHOR,frozen={frozen_item},before={before:?},after={after:?},rows={row_before:?}->{row_after:?},columns={column_before:?}->{column_after:?}"
    );
    assert_eq!(
        row_after.0, row_before.0,
        "row key changed; frozen={frozen_item}"
    );
    assert_eq!(
        column_after.0, column_before.0,
        "column key changed; frozen={frozen_item}"
    );
    assert!(
        (row_after.1 - row_before.1).abs() < 0.001,
        "row reading position moved: {row_before:?} -> {row_after:?}; frozen={frozen_item}"
    );
    assert!(
        (column_after.1 - column_before.1).abs() < 0.001,
        "column reading position moved: {column_before:?} -> {column_after:?}; frozen={frozen_item}"
    );
    let expected = if frozen_item {
        before
    } else {
        before - gpui_kit::point(7.25, 7.25)
    };
    assert_eq!(
        after, expected,
        "native owner correction; frozen={frozen_item}"
    );
    let clicks = view.read_with(cx, |v, _| v.clicks.clone());
    clicks.borrow_mut().clear();
    cx.simulate_click(gpui_kit::point(px(190.), px(86.)), Default::default());
    assert_eq!(clicks.borrow().as_slice(), &[(8, 7)]);
}

#[gpui_kit::test]
fn frozen_grid_frozen_size_feedback_preserves_body_reading_position(cx: &mut TestAppContext) {
    check_size_feedback_reading_position(cx, true);
}

#[gpui_kit::test]
fn frozen_grid_hidden_body_feedback_preserves_body_reading_position(cx: &mut TestAppContext) {
    check_size_feedback_reading_position(cx, false);
}

#[gpui_kit::test]
fn frozen_grid_losing_body_or_moving_anchor_into_prefix_keeps_native_offset(
    cx: &mut TestAppContext,
) {
    for remove_body in [true, false] {
        let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, true));
        view.update(cx, |v, cx| {
            v.frozen = (3, 3);
            v.scroll
                .set_precise_offset(gpui_kit::point(-213.25, -103.5));
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let (rows, before) = view.read_with(cx, |v, _| (v.rows.clone(), v.scroll.precise_offset()));
        rows.update(cx, |s, cx| {
            if remove_body {
                s.update_sizes(&[yue_component::VirtualizerSizeUpdate::new(0usize, px(500.))])
                    .unwrap();
            } else {
                // The old body reading key is 7. It now belongs to the
                // stationary prefix, so it cannot remain a scroll anchor.
                s.replace_source(
                    source(7, 100),
                    VirtualizerSizing::known(vec![px(20.5); 100]),
                    VirtualizerChange::Replace {
                        preserve_anchor: true,
                    },
                )
                .unwrap();
            }
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            view.read_with(cx, |v, _| v.scroll.precise_offset()),
            before,
            "no scroll correction for an inaccessible body anchor; remove_body={remove_body}"
        );
    }
}

#[gpui_kit::test]
fn frozen_grid_layout_changes_compensate_only_scrollable_geometry(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, true));
    view.update(cx, |v, cx| {
        v.frozen = (3, 3);
        v.scroll
            .set_precise_offset(gpui_kit::point(-213.25, -103.5));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (axes, before) = view.read_with(cx, |v, _| {
        (
            [v.rows.clone(), v.columns.clone()],
            v.scroll.precise_offset(),
        )
    });
    for state in &axes {
        state.update(cx, |s, cx| {
            let old = s.snapshot(s.viewport().unwrap()).unwrap();
            s.set_layout(
                VirtualizerLayout::default()
                    .gap(px(1.5))
                    .padding(px(2.), px(3.)),
            )
            .unwrap();
            assert!(
                s.validate_snapshot(&old).is_ok(),
                "unchanged layout is a no-op"
            );
            s.set_layout(
                VirtualizerLayout::default()
                    .gap(px(2.75))
                    .padding(px(8.), px(3.)),
            )
            .unwrap();
            assert!(matches!(
                s.validate_snapshot(&old),
                Err(VirtualizerError::StaleSnapshot)
            ));
            cx.notify();
        });
    }
    cx.update(|w, cx| w.draw(cx).clear(cx));
    // Reading key 7 moves by 7*1.25+6; the frozen body edge by 3*1.25+6.
    // Only the 5px difference belongs to the native scroll correction.
    assert_eq!(
        view.read_with(cx, |v, _| v.scroll.precise_offset()),
        before - gpui_kit::point(5., 5.)
    );
    for state in &axes {
        state.read_with(cx, |s, _| {
            let snap = s.snapshot(s.viewport().unwrap()).unwrap();
            assert_eq!(snap.visible_range().start, 7);
        });
    }
}

#[gpui_kit::test]
fn frozen_grid_repeated_feedback_resize_toggle_and_input_keep_one_coherent_owner(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, 100, true));
    view.update(cx, |v, cx| {
        v.frozen = (3, 3);
        v.scroll
            .set_precise_offset(gpui_kit::point(-213.25, -103.5));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    for step in 0..24 {
        let extra = if step % 2 == 0 { 5.25 } else { 0. };
        let (axes, before) = view.read_with(cx, |v, _| {
            (
                [v.rows.clone(), v.columns.clone()],
                v.scroll.precise_offset(),
            )
        });
        for (state, extent) in axes.iter().zip([20.5, 50.25]) {
            state.update(cx, |s, cx| {
                s.update_sizes(&[
                    yue_component::VirtualizerSizeUpdate::new(0usize, px(extent + extra)),
                    yue_component::VirtualizerSizeUpdate::new(6usize, px(extent + extra)),
                ])
                .unwrap();
                cx.notify();
            });
        }
        view.update(cx, |v, cx| {
            v.viewport = if step % 2 == 0 {
                (280., 135.)
            } else {
                (240., 100.)
            };
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let expected = gpui_kit::point(-213.25 - f64::from(extra), -103.5 - f64::from(extra));
        assert_eq!(
            view.read_with(cx, |v, _| v.scroll.precise_offset()),
            expected
        );
        for state in &axes {
            state.read_with(cx, |s, _| {
                let snap = s.snapshot(s.viewport().unwrap()).unwrap();
                assert_eq!(snap.visible_range().start, 7);
            });
        }
        // Toggling only presentation preserves the native content offset.
        for frozen in [(1, 1), (0, 0), (3, 3)] {
            view.update(cx, |v, cx| {
                v.frozen = frozen;
                cx.notify();
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            assert_eq!(
                view.read_with(cx, |v, _| v.scroll.precise_offset()),
                expected
            );
        }
        assert!(before.x.is_finite());
    }
    let axes = view.read_with(cx, |v, _| [v.rows.clone(), v.columns.clone()]);
    for state in &axes {
        state.update(cx, |s, cx| {
            s.scroll_to_key(50usize.into(), VirtualizerScrollOptions::default())
                .unwrap();
            cx.notify();
        });
    }
    // A real input delta before the command frame takes over both axes.
    let before = view.read_with(cx, |v, _| v.scroll.precise_offset());
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(190.), px(86.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-1.25), px(-2.5))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |v, _| v.scroll.precise_offset()),
        before - gpui_kit::point(1.25, 2.5)
    );
    for state in &axes {
        assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    }
}
