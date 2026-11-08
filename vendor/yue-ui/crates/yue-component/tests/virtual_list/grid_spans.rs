use super::*;
use std::cell::RefCell;
use yue_component::{VirtualGrid, VirtualGridCellKey, VirtualGridSpan};

struct Harness {
    axes: [Entity<VirtualizerState>; 2],
    scroll: ScrollHandle,
    spans: Vec<VirtualGridSpan>,
    clicks: Rc<RefCell<Vec<(usize, usize)>>>,
    frozen: (usize, usize),
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        VirtualGrid::new(
            "span-grid",
            &self.axes[0],
            &self.axes[1],
            &self.scroll,
            move |cell, _, _| {
                let key = (cell.row().index(), cell.column().index());
                let clicks = clicks.clone();
                if key == (0, 0) {
                    assert_eq!(cell.row().extent(), px(20.));
                    assert_eq!(cell.column().extent(), px(50.));
                }
                div()
                    .id("cell")
                    .size_full()
                    .role(Role::Cell)
                    .aria_label(format!("{}:{}", key.0, key.1))
                    .on_click(move |_, _, _| clicks.borrow_mut().push(key))
                    .test_support()
            },
        )
        .frozen_rows(self.frozen.0)
        .frozen_columns(self.frozen.1)
        .spans(&self.spans, cx)
        .unwrap()
        .w(px(240.))
        .h(px(100.))
        .test_support()
    }
}
fn harness(cx: &mut Context<Harness>, known: bool) -> Harness {
    let binding = VirtualizerBinding::new();
    Harness {
        axes: [Axis::Vertical, Axis::Horizontal].map(|axis| {
            cx.new(|_| {
                let extent = if axis == Axis::Vertical { 20. } else { 50. };
                VirtualizerState::new_attached(
                    axis,
                    source(0, 1000),
                    if known {
                        VirtualizerSizing::known(vec![px(extent); 1000])
                    } else {
                        VirtualizerSizing::fixed(px(extent))
                    },
                    &binding,
                )
                .unwrap()
            })
        }),
        scroll: ScrollHandle::new(),
        spans: Vec::new(),
        clicks: Rc::default(),
        frozen: (0, 0),
    }
}
#[gpui_kit::test]
fn grid_column_then_row_spans_have_one_offscreen_origin_and_hit_target(cx: &mut TestAppContext) {
    for known in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| harness(cx, known));
        for rows in [1, 3] {
            view.update(cx, |v, cx| {
                v.spans = vec![VirtualGridSpan::new(
                    VirtualGridCellKey::new(0usize, 0usize),
                    rows,
                    4,
                )];
                v.scroll
                    .set_precise_offset(gpui_kit::point(-125., if rows == 1 { 0. } else { -30. }));
                cx.notify();
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let nodes = cx.update(|w, _| snapshots(w));
            let origins: Vec<_> = nodes.iter().filter(|n| n.label() == Some("0:0")).collect();
            assert_eq!(origins.len(), 1);
            assert_eq!(
                origins[0].bounds().size,
                gpui_kit::size(px(200.), px(rows as f32 * 20.))
            );
            assert!(
                !nodes
                    .iter()
                    .any(|n| n.label() == Some(if rows == 1 { "0:2" } else { "1:2" }))
            );
            view.read_with(cx, |v, _| v.clicks.borrow_mut().clear());
            cx.simulate_click(gpui_kit::point(px(10.), px(10.)), Default::default());
            assert_eq!(
                *view.read_with(cx, |v, _| v.clicks.clone()).borrow(),
                vec![(0, 0)]
            );
        }
    }
}
#[gpui_kit::test]
fn grid_span_validation_is_atomic_and_freeze_boundary_is_explicit(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let spans =
        |row, columns| VirtualGridSpan::new(VirtualGridCellKey::new(row, 0usize), 1, columns);
    for (items, frozen, error) in [
        (
            vec![spans(0usize, 0)],
            0,
            VirtualizerError::InvalidGeometry("span count"),
        ),
        (
            vec![spans(0usize, 1001)],
            0,
            VirtualizerError::InvalidGeometry("span bounds"),
        ),
        (vec![spans(2000usize, 2)], 0, VirtualizerError::MissingKey),
        (
            vec![spans(0usize, 4), spans(0usize, 2)],
            0,
            VirtualizerError::InvalidGeometry("overlapping spans"),
        ),
        (
            vec![spans(0usize, 4)],
            1,
            VirtualizerError::Unsupported("span crosses frozen boundary"),
        ),
    ] {
        cx.update(|_, cx| {
            let v = view.read(cx);
            let before = v.scroll.precise_offset();
            let result =
                VirtualGrid::new("invalid", &v.axes[0], &v.axes[1], &v.scroll, |_, _, _| {
                    div()
                })
                .frozen_columns(frozen)
                .spans(&items, cx);
            assert!(matches!(result,Err(e) if e==error));
            assert_eq!(v.scroll.precise_offset(), before);
        });
    }
    // Adjacent spans and a span wholly within a frozen strip are valid.
    view.update(cx, |v, cx| {
        v.frozen = (1, 0);
        v.spans = vec![spans(0usize, 4), spans(1usize, 4)];
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_click(gpui_kit::point(px(175.), px(5.)), Default::default());
    assert_eq!(
        *view.read_with(cx, |v, _| v.clicks.clone()).borrow(),
        vec![(0, 0)]
    );
}

#[gpui_kit::test]
fn grid_span_resolves_stable_origin_after_source_change_and_track_feedback(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, true));
    view.update(cx, |v, cx| {
        v.spans = vec![VirtualGridSpan::new(
            VirtualGridCellKey::new(3usize, 3usize),
            2,
            2,
        )];
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let axes = view.read_with(cx, |v, _| v.axes.clone());
    for (axis, extent) in axes.iter().zip([20., 50.]) {
        axis.update(cx, |s, cx| {
            s.replace_source(
                source(1, 999),
                VirtualizerSizing::known(vec![px(extent); 999]),
                VirtualizerChange::Splice {
                    old: 0..1,
                    inserted: 0,
                },
            )
            .unwrap();
            s.update_sizes(&[yue_component::VirtualizerSizeUpdate::new(
                4usize,
                px(extent + 5.),
            )])
            .unwrap();
            cx.notify();
        });
    }
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let nodes = cx.update(|w, _| snapshots(w));
    // key (3,3) is now index (2,2), spanning tracks 2 and 3 (key 4 grew).
    let node = nodes.iter().find(|n| n.label() == Some("2:2")).unwrap();
    assert_eq!(node.bounds().size, gpui_kit::size(px(105.), px(45.)));
}

#[gpui_kit::test]
fn grid_huge_span_materializes_one_cell_without_scanning_covered_area(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx, false));
    let calls = Rc::new(Cell::new(0));
    let axes = view.read_with(cx, |v, _| v.axes.clone());
    for (axis, extent) in axes.iter().zip([20., 50.]) {
        axis.update(cx, |s, _| {
            s.replace_source(
                Rc::new(Source {
                    start: 0,
                    count: 1_000_000,
                    calls: calls.clone(),
                }),
                VirtualizerSizing::fixed(px(extent)),
                VirtualizerChange::Replace {
                    preserve_anchor: false,
                },
            )
            .unwrap()
        });
    }
    view.update(cx, |v, cx| {
        v.spans = vec![VirtualGridSpan::new(
            VirtualGridCellKey::new(0usize, 0usize),
            100_000,
            100_000,
        )];
        cx.notify();
    });
    calls.set(0);
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        calls.get() < 300,
        "source reads must be bounded by tracks + sparse spans, got {}",
        calls.get()
    );
    let nodes = cx.update(|w, _| snapshots(w));
    assert_eq!(
        nodes
            .iter()
            .filter(|n| n.role() == Some(Role::Cell))
            .count(),
        1
    );
}
