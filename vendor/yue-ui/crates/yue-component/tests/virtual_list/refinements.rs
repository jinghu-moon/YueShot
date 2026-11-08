use super::*;
use std::{cell::RefCell, collections::HashMap};
use yue_component::{VirtualMasonryState, VirtualizerSizeUpdate, VirtualizerSizingKind};

fn sizing(kind: VirtualizerSizingKind, count: usize) -> VirtualizerSizing {
    match kind {
        VirtualizerSizingKind::Fixed => VirtualizerSizing::fixed(px(20.)),
        VirtualizerSizingKind::Known => VirtualizerSizing::known(vec![px(20.); count]),
        VirtualizerSizingKind::Measured => VirtualizerSizing::measured(px(20.)),
    }
}

struct ContentRow(u64);
impl Render for ContentRow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(20.)).child(format!("version {}", self.0))
    }
}
struct ContentHarness {
    state: Entity<VirtualizerState>,
    version: Rc<Cell<u64>>,
    factories: Rc<Cell<usize>>,
    rows: Rc<RefCell<HashMap<usize, gpui_kit::WeakEntity<ContentRow>>>>,
}
impl Render for ContentHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let version = self.version.clone();
        let factories = self.factories.clone();
        let rows = self.rows.clone();
        VirtualList::new_with_item_views("content-list", &self.state, move |item, _, cx| {
            factories.set(factories.get() + 1);
            let row = cx.new(|_| ContentRow(version.get()));
            rows.borrow_mut().insert(item.index(), row.downgrade());
            row
        })
        .w(px(240.))
        .h(px(100.))
    }
}

#[gpui_kit::test]
fn refinements_content_invalidation_refreshes_each_backend(cx: &mut TestAppContext) {
    for kind in [
        VirtualizerSizingKind::Fixed,
        VirtualizerSizingKind::Known,
        VirtualizerSizingKind::Measured,
    ] {
        let (view, cx) = cx.add_window_view(|_, cx| ContentHarness {
            state: cx.new(|_| {
                VirtualizerState::new(Axis::Vertical, source(0, 1000), sizing(kind, 1000)).unwrap()
            }),
            version: Rc::new(Cell::new(0)),
            factories: Rc::new(Cell::new(0)),
            rows: Rc::default(),
        });
        for _ in 0..2 {
            cx.update(|w, cx| w.draw(cx).clear(cx));
        }
        let (state, rows) = view.read_with(cx, |v, _| {
            v.version.set(1);
            (v.state.clone(), v.rows.clone())
        });
        let unchanged = rows.borrow()[&1].clone();
        state.update(cx, |s, cx| {
            s.invalidate_items(0..1).unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let version = cx.update(|_, cx| rows.borrow()[&0].upgrade().unwrap().read(cx).0);
        assert_eq!(
            version, 1,
            "{kind:?} must refresh explicitly invalidated content"
        );
        assert_eq!(
            rows.borrow()[&1],
            unchanged,
            "unrelated content must retain its view"
        );
    }
}

#[test]
fn refinements_content_invalidates_late_callbacks_without_changing_fixed_geometry() {
    for kind in [
        VirtualizerSizingKind::Fixed,
        VirtualizerSizingKind::Known,
        VirtualizerSizingKind::Measured,
    ] {
        let mut s =
            VirtualizerState::new(Axis::Vertical, source(0, 100), sizing(kind, 100)).unwrap();
        let v = viewport(&s, 1, 0.);
        let before = s.snapshot(&v).unwrap();
        s.invalidate_items(3..3).unwrap();
        s.validate_snapshot(&before).unwrap();
        assert_eq!(
            s.invalidate_items(99..101),
            Err(VirtualizerError::InvalidChange)
        );
        s.validate_snapshot(&before).unwrap();
        s.invalidate_items(0..1).unwrap();
        assert_eq!(
            s.validate_snapshot(&before),
            Err(VirtualizerError::StaleSnapshot)
        );
        assert_eq!(
            before.item(3).unwrap().start(),
            s.snapshot(&v).unwrap().item(3).unwrap().start()
        );
    }
}

#[test]
fn refinements_alignment_policy_rejects_old_request_without_remeasurement() {
    for kind in [
        VirtualizerSizingKind::Fixed,
        VirtualizerSizingKind::Known,
        VirtualizerSizingKind::Measured,
    ] {
        let mut s =
            VirtualizerState::new(Axis::Vertical, source(0, 100), sizing(kind, 100)).unwrap();
        if kind == VirtualizerSizingKind::Measured {
            s.update_sizes(&[VirtualizerSizeUpdate::new(10usize, px(20.))])
                .unwrap();
        }
        let v = viewport(&s, 1, 0.);
        s.scroll_to_key(
            10usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        let (snapshot, request) = s.prepare(&v, Instant::now()).unwrap();
        let old = request.unwrap();
        assert_eq!(old.offset(), 200.);
        s.set_layout(VirtualizerLayout::default().scroll_padding(px(15.), px(10.)))
            .unwrap();
        assert_eq!(
            s.validate_request(&old, &v),
            Err(VirtualizerError::StaleRequest)
        );
        s.validate_snapshot(&snapshot).unwrap();
        assert!(s.snapshot(&v).unwrap().item(10).unwrap().is_measured());
        let (_, request) = s.prepare(&v, Instant::now()).unwrap();
        let current = request.unwrap();
        assert_eq!(current.offset(), 185.);
        s.set_layout(
            VirtualizerLayout::default()
                .scroll_padding(px(15.), px(10.))
                .overscan(px(30.)),
        )
        .unwrap();
        s.validate_request(&current, &v).unwrap();
    }
}

#[gpui_kit::test]
fn refinements_geometry_changes_reuse_prepared_content(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| ContentHarness {
        state: cx.new(|_| {
            VirtualizerState::new(
                Axis::Vertical,
                source(0, 1000),
                sizing(VirtualizerSizingKind::Known, 1000),
            )
            .unwrap()
        }),
        version: Rc::new(Cell::new(0)),
        factories: Rc::new(Cell::new(0)),
        rows: Rc::default(),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, factories, rows) = view.read_with(cx, |v, _| {
        (v.state.clone(), v.factories.clone(), v.rows.clone())
    });
    state.update(cx, |s, cx| {
        s.scroll_to_key(
            50usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let row = rows.borrow()[&50].clone();
    factories.set(0);
    state.update(cx, |s, cx| {
        s.update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(40.))])
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        state.read_with(cx, |s, _| s
            .snapshot(s.viewport().unwrap())
            .unwrap()
            .visible_range()),
        50..55
    );
    assert_eq!(
        factories.get(),
        0,
        "moving content is not a content preparation dependency"
    );
    assert_eq!(rows.borrow()[&50], row);
    state.update(cx, |s, cx| {
        s.set_layout(VirtualizerLayout::default().revision(1))
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        factories.get(),
        0,
        "normal row Render owns reactive layout dependencies"
    );
}

#[test]
fn refinements_splice_estimates_only_new_rows_and_rejects_invalid_batches_atomically() {
    let calls = Rc::new(Cell::new(0));
    let make_sizing = |invalid| {
        let calls = calls.clone();
        VirtualizerSizing::measured_with_estimates(px(20.), move |ix| {
            calls.set(calls.get() + 1);
            if invalid && ix == 10_001 {
                px(f32::NAN)
            } else {
                px(20. + (ix % 4) as f32)
            }
        })
    };
    let mut s =
        VirtualizerState::new(Axis::Vertical, source(0, 10_000), make_sizing(false)).unwrap();
    s.update_sizes(&[VirtualizerSizeUpdate::new(3usize, px(81.))])
        .unwrap();
    let v = viewport(&s, 1, 0.);
    let frozen = s.snapshot(&v).unwrap();
    calls.set(0);
    s.replace_source(
        source(0, 10_001),
        make_sizing(false),
        VirtualizerChange::Splice {
            old: 10_000..10_000,
            inserted: 1,
        },
    )
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(s.snapshot(&v).unwrap().item(3).unwrap().extent(), px(81.));
    assert_eq!(frozen.len(), 10_000);
    let before = s.snapshot(&v).unwrap();
    calls.set(0);
    assert!(
        s.replace_source(
            source(0, 10_003),
            make_sizing(true),
            VirtualizerChange::Splice {
                old: 10_001..10_001,
                inserted: 2
            }
        )
        .is_err()
    );
    assert_eq!(calls.get(), 2);
    s.validate_snapshot(&before).unwrap();
    assert_eq!(s.snapshot(&v).unwrap().len(), 10_001);
}

#[test]
fn refinements_masonry_append_keeps_lanes_measurements_and_frozen_snapshots() {
    let old_source = source(0, 10_000);
    let mut s =
        VirtualMasonryState::new(old_source.clone(), VirtualizerSizing::measured(px(40.)), 3)
            .unwrap();
    s.set_layout(VirtualizerLayout::default().gap(px(5.)), px(3.))
        .unwrap();
    s.update_sizes(&[VirtualizerSizeUpdate::new(1usize, px(85.))])
        .unwrap();
    let frozen = s.snapshot(0., gpui_kit::size(px(300.), px(100.))).unwrap();
    let calls = Rc::new(Cell::new(0));
    let estimates = calls.clone();
    let next_source = source(0, 10_003);
    old_source.calls.set(0);
    s.append_source(
        next_source.clone(),
        VirtualizerSizing::measured_with_estimates(px(40.), move |_| {
            estimates.set(estimates.get() + 1);
            px(45.)
        }),
    )
    .unwrap();
    assert_eq!(calls.get(), 3);
    assert!(old_source.calls.get() < 10 && next_source.calls.get() < 10);
    let next = s.snapshot(0., gpui_kit::size(px(300.), px(100.))).unwrap();
    for ix in [0, 1, 2, 9998, 9999] {
        let old = frozen.item(ix).unwrap();
        let new = next.item(ix).unwrap();
        assert_eq!(old.lane(), new.lane());
        assert_eq!(old.item().start(), new.item().start());
        assert_eq!(old.item().extent(), new.item().extent());
        assert_eq!(old.item().is_measured(), new.item().is_measured());
    }
    assert_eq!(next.item(1).unwrap().item().extent(), px(85.));
    assert!(!next.item(10_002).unwrap().item().is_measured());
    assert!(frozen.item(10_000).is_err());
    let revision = next.revision();
    assert!(
        s.append_source(
            source(0, 10_005),
            VirtualizerSizing::measured_with_estimates(px(40.), |ix| if ix == 10_004 {
                px(-1.)
            } else {
                px(40.)
            })
        )
        .is_err()
    );
    assert_eq!(
        s.snapshot(0., gpui_kit::size(px(300.), px(100.)))
            .unwrap()
            .revision(),
        revision
    );
}

#[test]
fn refinements_compact_index_matches_linear_oracle_across_leaf_boundaries() {
    let mut sizes: Vec<_> = (0..2000).map(|ix| px(1. + (ix % 61) as f32)).collect();
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        source(0, sizes.len()),
        VirtualizerSizing::known(sizes.clone()),
    )
    .unwrap();
    state
        .set_layout(
            VirtualizerLayout::default()
                .gap(px(3.))
                .padding(px(7.), px(5.)),
        )
        .unwrap();
    let old = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
    let updates: Vec<_> = [0, 127, 128, 255, 256, 1023, 1024, 1999]
        .into_iter()
        .map(|ix| {
            sizes[ix] = px(83.);
            VirtualizerSizeUpdate::new(ix, px(83.))
        })
        .collect();
    state.update_sizes(&updates).unwrap();
    assert_eq!(old.item(128).unwrap().extent(), px(7.));
    let mut start = 7.;
    let positions: Vec<_> = sizes
        .iter()
        .map(|size| {
            let pos = start;
            start += f64::from(*size) + 3.;
            pos
        })
        .collect();
    let total = start + 2.;
    let mut offsets: Vec<_> = (0..total as usize + 100)
        .step_by(37)
        .map(|value| value as f64)
        .collect();
    for ix in [127, 128, 255, 256, 1023, 1024, 1999] {
        offsets.extend([
            positions[ix] - 0.25,
            positions[ix],
            positions[ix] + 0.25,
            positions[ix] + f64::from(sizes[ix]),
        ]);
    }
    for offset in offsets {
        let snapshot = state.snapshot(&viewport(&state, 1, offset)).unwrap();
        assert_eq!(snapshot.total_extent(), total);
        let expected: Vec<_> = sizes
            .iter()
            .enumerate()
            .filter(|(ix, size)| {
                positions[*ix] < offset + 100. && positions[*ix] + f64::from(**size) > offset
            })
            .map(|(ix, _)| ix)
            .collect();
        assert_eq!(
            snapshot.visible_range().collect::<Vec<_>>(),
            expected,
            "offset {offset}"
        );
    }
    sizes[126] = px(1e25);
    sizes[127] = px(1.);
    let huge = VirtualizerState::new(
        Axis::Vertical,
        source(0, sizes.len()),
        VirtualizerSizing::known(sizes),
    )
    .unwrap();
    assert_eq!(
        huge.snapshot(&viewport(&huge, 1, 0.))
            .unwrap()
            .item(127)
            .unwrap()
            .extent(),
        px(1.),
        "actual extent must not be recovered by subtracting rounded huge offsets"
    );
}

#[gpui_kit::test]
fn refinements_overlapping_content_ranges_keep_independent_epochs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| ContentHarness {
        state: cx.new(|_| fixed(1000)),
        version: Rc::new(Cell::new(0)),
        factories: Rc::new(Cell::new(0)),
        rows: Rc::default(),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    for (version, range) in [(1, 1..4), (2, 2..3), (3, 3..5)] {
        view.update(cx, |v, cx| {
            v.version.set(version);
            v.state.update(cx, |s, cx| {
                s.invalidate_items(range).unwrap();
                cx.notify();
            });
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    view.read_with(cx, |v, cx| {
        for (ix, expected) in [0, 1, 2, 3, 3].into_iter().enumerate() {
            assert_eq!(v.rows.borrow()[&ix].upgrade().unwrap().read(cx).0, expected);
        }
    });
}

#[test]
fn refinements_sequential_items_seek_across_blocks_and_skip_remote_gaps() {
    for kind in [
        VirtualizerSizingKind::Fixed,
        VirtualizerSizingKind::Known,
        VirtualizerSizingKind::Measured,
    ] {
        let source = source(0, 100_000);
        let mut state =
            VirtualizerState::new(Axis::Vertical, source.clone(), sizing(kind, 100_000)).unwrap();
        state.set_retained_keys(vec![99_999usize.into()]).unwrap();
        let snapshot = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
        for item in snapshot.items(120..140).unwrap() {
            let item = item.unwrap();
            let single = snapshot.item(item.index()).unwrap();
            assert_eq!(item.start(), single.start());
            assert_eq!(item.extent(), single.extent());
            assert_eq!(item.is_measured(), single.is_measured());
        }
        assert!(snapshot.items(100_000..100_001).is_err());
        source.calls.set(0);
        let indices: Vec<_> = snapshot
            .working_items()
            .map(|item| item.unwrap().index())
            .collect();
        assert_eq!(indices, vec![0, 1, 2, 3, 4, 99_999]);
        assert!(
            source.calls.get() < 30,
            "remote retained items must not walk intervening records"
        );
    }
}
