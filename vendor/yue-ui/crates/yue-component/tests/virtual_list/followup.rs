//! Regression gates for the six confirmed follow-up review defects.
use gpui_kit::base::{TestSupportExt as _, test_support::snapshots, virtualization::VirtualList};
use gpui_kit::{
    AppContext as _, Axis, Context, ElementId, Entity, InteractiveElement as _, IntoElement,
    Render, Role, StatefulInteractiveElement as _, Styled, TestAppContext, Window, div, px, size,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
use yue_component::{
    VirtualGridSnapshot, VirtualMasonry, VirtualMasonryState, VirtualizerAlign, VirtualizerChange,
    VirtualizerEvent, VirtualizerLayout, VirtualizerMotion, VirtualizerScrollOptions,
    VirtualizerSizeUpdate, VirtualizerSizing, VirtualizerSource, VirtualizerState,
    VirtualizerViewport,
};

struct Source {
    count: usize,
    reads: Rc<Cell<usize>>,
}
impl VirtualizerSource for Source {
    fn len(&self) -> usize {
        self.count
    }
    fn key_at(&self, ix: usize) -> ElementId {
        self.reads.set(self.reads.get() + 1);
        ix.into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        self.reads.set(self.reads.get() + 1);
        match key {
            ElementId::Integer(ix) => ((*ix as usize) < self.count).then_some(*ix as usize),
            _ => None,
        }
    }
}
fn source(count: usize) -> Rc<Source> {
    Rc::new(Source {
        count,
        reads: Rc::default(),
    })
}

struct PermutedTail(usize);
impl VirtualizerSource for PermutedTail {
    fn len(&self) -> usize {
        self.0
    }
    fn key_at(&self, index: usize) -> ElementId {
        match index {
            98 => 99usize.into(),
            99 => 98usize.into(),
            _ => index.into(),
        }
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        let ElementId::Integer(key) = key else {
            return None;
        };
        let index = match *key as usize {
            98 => 99,
            99 => 98,
            index => index,
        };
        (index < self.0).then_some(index)
    }
}

struct PreparedRow(u64);
impl Render for PreparedRow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(20.))
    }
}
struct ContentHarness {
    state: Entity<VirtualizerState>,
    version: Rc<Cell<u64>>,
    row: Rc<RefCell<Option<Entity<PreparedRow>>>>,
    factories: Rc<Cell<usize>>,
}
impl Render for ContentHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let version = self.version.clone();
        let row = self.row.clone();
        let factories = self.factories.clone();
        VirtualList::new_with_item_views("followup-content", &self.state, move |item, _, cx| {
            factories.set(factories.get() + 1);
            let view = cx.new(|_| PreparedRow(version.get()));
            if item.index() == 0 {
                *row.borrow_mut() = Some(view.clone());
            }
            view
        })
        .w(px(240.))
        .h(px(100.))
    }
}
#[gpui_kit::test]
fn content_invalidation_before_append_regression(cx: &mut TestAppContext) {
    for mode in 0..3 {
        for order in [
            "invalidate_only",
            "append_then_invalidate",
            "invalidate_then_append",
        ] {
            let sizing = |count| match mode {
                0 => VirtualizerSizing::fixed(px(20.)),
                1 => VirtualizerSizing::known(vec![px(20.); count]),
                _ => VirtualizerSizing::measured(px(20.)),
            };
            let (view, cx) = cx.add_window_view(|_, cx| ContentHarness {
                state: cx.new(|_| {
                    VirtualizerState::new(Axis::Vertical, source(100), sizing(100)).unwrap()
                }),
                version: Rc::new(Cell::new(0)),
                row: Rc::default(),
                factories: Rc::default(),
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let state = view.read_with(cx, |v, _| {
                v.version.set(1);
                v.factories.set(0);
                v.state.clone()
            });
            state.update(cx, |s, cx| {
                if order != "append_then_invalidate" {
                    s.invalidate_items(0..1).unwrap();
                }
                if order != "invalidate_only" {
                    s.replace_source(
                        source(101),
                        sizing(101),
                        VirtualizerChange::Splice {
                            old: 100..100,
                            inserted: 1,
                        },
                    )
                    .unwrap();
                }
                if order == "append_then_invalidate" {
                    s.invalidate_items(0..1).unwrap();
                }
                cx.notify();
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let (value, factories) = view.read_with(cx, |v, cx| {
                (
                    v.row.borrow().as_ref().unwrap().read(cx).0,
                    v.factories.get(),
                )
            });
            assert_eq!(
                value, 1,
                "mode={mode}, order={order}, factories={factories}"
            );
            assert!(factories > 0);
        }
    }
}

#[gpui_kit::test]
fn content_epoch_barriers_survive_transactions_without_refreshing_every_future_append(
    cx: &mut TestAppContext,
) {
    for mode in 0..3 {
        for transaction in 0..4 {
            let transaction_source = |count| -> Rc<dyn VirtualizerSource> {
                if transaction == 1 {
                    Rc::new(PermutedTail(count))
                } else {
                    source(count)
                }
            };
            let sizing = |count| match mode {
                0 => VirtualizerSizing::fixed(px(20.)),
                1 => VirtualizerSizing::known(vec![px(20.); count]),
                _ => VirtualizerSizing::measured(px(20.)),
            };
            let (view, cx) = cx.add_window_view(|_, cx| ContentHarness {
                state: cx.new(|_| {
                    VirtualizerState::new(Axis::Vertical, source(100), sizing(100)).unwrap()
                }),
                version: Rc::new(Cell::new(0)),
                row: Rc::default(),
                factories: Rc::default(),
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let state = view.read_with(cx, |v, _| {
                v.version.set(1);
                v.state.clone()
            });
            let count = if transaction == 0 { 99 } else { 100 };
            state.update(cx, |s, cx| {
                let old = s.snapshot(s.viewport().unwrap()).unwrap();
                s.invalidate_items(0..1).unwrap();
                s.invalidate_items(0..1).unwrap();
                let change = match transaction {
                    0 => VirtualizerChange::Splice {
                        old: 99..100,
                        inserted: 0,
                    },
                    1 => VirtualizerChange::Reorder,
                    _ => VirtualizerChange::Replace {
                        preserve_anchor: transaction == 2,
                    },
                };
                s.replace_source(transaction_source(count), sizing(count), change)
                    .unwrap();
                assert_eq!(
                    s.validate_snapshot(&old),
                    Err(yue_component::VirtualizerError::StaleSnapshot)
                );
                cx.notify();
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let first = view.read_with(cx, |v, cx| {
                let row = v.row.borrow().as_ref().unwrap().clone();
                assert_eq!(row.read(cx).0, 1);
                v.factories.set(0);
                row
            });
            // A local unrelated invalidation still preserves row zero.
            state.update(cx, |s, cx| {
                s.invalidate_items(1..2).unwrap();
                cx.notify();
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            view.read_with(cx, |v, _| {
                assert_eq!(v.row.borrow().as_ref().unwrap(), &first);
                assert_eq!(v.factories.get(), 1);
            });
            // One further structural barrier compacts that local invalidation.
            for extra in 0..3 {
                view.read_with(cx, |v, _| v.factories.set(0));
                state.update(cx, |s, cx| {
                    s.replace_source(
                        transaction_source(count + extra + 1),
                        sizing(count + extra + 1),
                        VirtualizerChange::Splice {
                            old: count + extra..count + extra,
                            inserted: 1,
                        },
                    )
                    .unwrap();
                    cx.notify();
                });
                cx.update(|w, cx| w.draw(cx).clear(cx));
                if extra > 0 {
                    assert_eq!(
                        view.read_with(cx, |v, _| v.factories.get()),
                        0,
                        "an empty epoch map needs no new barrier"
                    );
                }
            }
        }
    }
}
fn viewport(state: &VirtualizerState, revision: u64, cross: f32) -> VirtualizerViewport {
    VirtualizerViewport::new(state.binding().unwrap(), revision, 0., px(100.), px(cross))
}

#[test]
fn grid_axis_metadata_reads_regression() {
    let rows_source = source(100_000);
    let columns_source = source(100_000);
    let binding = yue_component::VirtualizerBinding::new();
    let rows = VirtualizerState::new_attached(
        Axis::Vertical,
        rows_source.clone(),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let columns = VirtualizerState::new_attached(
        Axis::Horizontal,
        columns_source.clone(),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let grid = VirtualGridSnapshot::new(
        rows.snapshot(&VirtualizerViewport::new(
            &binding,
            1,
            0.,
            px(400.),
            px(400.),
        ))
        .unwrap(),
        columns
            .snapshot(&VirtualizerViewport::new(
                &binding,
                1,
                0.,
                px(400.),
                px(400.),
            ))
            .unwrap(),
    )
    .unwrap();
    rows_source.reads.set(0);
    columns_source.reads.set(0);
    let cells = grid.cells().count();
    assert_eq!(cells, 400);
    assert_eq!(rows_source.reads.get(), 40);
    assert_eq!(columns_source.reads.get(), 40);
    rows_source.reads.set(0);
    columns_source.reads.set(0);
    let row_items: Vec<_> = grid.rows().working_items().map(Result::unwrap).collect();
    let col_items: Vec<_> = grid.columns().working_items().map(Result::unwrap).collect();
    let pairs = row_items
        .iter()
        .flat_map(|row| col_items.iter().map(move |col| (row, col)))
        .count();
    println!(
        "FOLLOWUP_GRID_ONCE,cells={pairs},row_source_calls={},column_source_calls={}",
        rows_source.reads.get(),
        columns_source.reads.get()
    );
}

#[test]
fn masonry_unmeasured_invalidation_regression() {
    for measured in [false, true] {
        let mut state =
            VirtualMasonryState::new(source(100), VirtualizerSizing::measured(px(20.)), 3).unwrap();
        if measured {
            state
                .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(20.))])
                .unwrap();
        }
        let old = state.snapshot(0., size(px(300.), px(100.))).unwrap();
        state.invalidate_items(&[0usize.into()]).unwrap();
        let current = state.snapshot(0., size(px(300.), px(100.))).unwrap();
        let accepted = state
            .update_sizes_from_snapshot(&old, &[VirtualizerSizeUpdate::new(0usize, px(99.))])
            .is_ok();
        assert!(current.revision() > old.revision());
        assert!(!accepted, "late feedback after content invalidation");
    }
}

struct MasonryHarness {
    state: Entity<VirtualMasonryState>,
    rendered: Rc<RefCell<Vec<usize>>>,
    height: f32,
}
impl Render for MasonryHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rendered = self.rendered.clone();
        let height = self.height;
        VirtualMasonry::new("followup-masonry", &self.state, move |item, _, _| {
            rendered.borrow_mut().push(item.item().index());
            div()
                .id("card")
                .h(px(height))
                .role(Role::ListItem)
                .aria_label(item.item().index().to_string())
                .test_support()
        })
        .w(px(300.))
        .h(px(100.))
    }
}
#[gpui_kit::test]
fn masonry_budget_rendering_regression(cx: &mut TestAppContext) {
    for measured in [false, true] {
        for budget in [2, 1024] {
            let (view, cx) = cx.add_window_view(|_, cx| {
                let mut state = VirtualMasonryState::new(
                    source(100),
                    if measured {
                        VirtualizerSizing::measured(px(20.))
                    } else {
                        VirtualizerSizing::fixed(px(20.))
                    },
                    3,
                )
                .unwrap();
                state
                    .set_layout(
                        VirtualizerLayout::default().measurement_budget(budget),
                        px(0.),
                    )
                    .unwrap();
                MasonryHarness {
                    state: cx.new(|_| state),
                    rendered: Rc::default(),
                    height: 20.,
                }
            });
            let state = view.read_with(cx, |v, _| v.state.clone());
            for frame in 0..3 {
                view.read_with(cx, |v, _| v.rendered.borrow_mut().clear());
                let painted = cx.update(|w, cx| {
                    w.refresh();
                    w.draw(cx).clear(cx);
                    snapshots(w)
                        .iter()
                        .filter(|n| n.role() == Some(Role::ListItem))
                        .count()
                });
                let working = state.read_with(cx, |s, _| {
                    s.snapshot(0., size(px(300.), px(100.)))
                        .unwrap()
                        .working_indices()
                        .len()
                });
                let built = view.read_with(cx, |v, _| v.rendered.borrow().clone());
                assert_eq!(
                    painted, working,
                    "mode={measured}, budget={budget}, frame={frame}, built={built:?}"
                );
                assert_eq!(painted, 15);
            }
        }
    }
}

#[gpui_kit::test]
fn masonry_correction_budget_presents_all_slots_and_resumes_only_with_progress(
    cx: &mut TestAppContext,
) {
    for (estimate, height, overscan) in [(100., 20., 0.), (40., 1., 20.), (1., 20., 0.)] {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut state = VirtualMasonryState::new(
                source(10_000),
                VirtualizerSizing::measured(px(estimate)),
                3,
            )
            .unwrap();
            state
                .set_layout(
                    VirtualizerLayout::default()
                        .measurement_budget(1)
                        .overscan(px(overscan)),
                    px(0.),
                )
                .unwrap();
            state.set_retained_keys(vec![9999usize.into()]).unwrap();
            MasonryHarness {
                state: cx.new(|_| state),
                rendered: Rc::default(),
                height: 0.,
            }
        });
        let state = view.read_with(cx, |v, _| v.state.clone());
        view.update(cx, |v, cx| {
            v.height = height;
            cx.notify();
        });
        let mut settled = false;
        for frame in 0..128 {
            view.read_with(cx, |v, _| v.rendered.borrow_mut().clear());
            let before = state.read_with(cx, |s, _| {
                s.snapshot(
                    (-f64::from(s.scroll_handle().offset().y)).max(0.),
                    size(px(300.), px(100.)),
                )
                .unwrap()
                .working_indices()
                .len()
            });
            let nodes = cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
                snapshots(w)
                    .into_iter()
                    .filter(|n| n.role() == Some(Role::ListItem))
                    .collect::<Vec<_>>()
            });
            let current = state.read_with(cx, |s, _| {
                s.snapshot(
                    (-f64::from(s.scroll_handle().offset().y)).max(0.),
                    size(px(300.), px(100.)),
                )
                .unwrap()
            });
            let working = current.working_indices();
            let mut presented: Vec<usize> = nodes
                .iter()
                .map(|n| n.label().unwrap().parse().unwrap())
                .collect();
            presented.sort_unstable();
            assert_eq!(presented, working, "estimate={estimate}, frame={frame}");
            assert!(working.contains(&9999));
            assert!(
                view.read_with(cx, |v, _| v.rendered.borrow().len()) <= before + 1 + working.len()
            );
            let callbacks = cx.update(|w, cx| w.simulate_next_frame(cx));
            if callbacks == 0 {
                assert!(
                    working
                        .iter()
                        .all(|&ix| current.item(ix).unwrap().item().is_measured())
                );
                settled = true;
                break;
            }
        }
        assert!(settled, "correction must converge, estimate={estimate}");
    }
}

#[gpui_kit::test]
fn masonry_dense_workset_can_exceed_the_default_measurement_budget(cx: &mut TestAppContext) {
    for dynamic in [false, true] {
        let (_, cx) = cx.add_window_view(|_, cx| MasonryHarness {
            state: cx.new(|_| {
                VirtualMasonryState::new(
                    source(10_000),
                    if dynamic {
                        VirtualizerSizing::measured(px(0.125))
                    } else {
                        VirtualizerSizing::fixed(px(0.125))
                    },
                    3,
                )
                .unwrap()
            }),
            rendered: Rc::default(),
            height: 0.125,
        });
        let count = cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
            snapshots(w)
                .iter()
                .filter(|n| n.role() == Some(Role::ListItem))
                .count()
        });
        assert_eq!(count, 2400);
    }
}

#[test]
fn measurement_cache_cross_identity_regression() {
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        source(100),
        VirtualizerSizing::measured(px(20.)),
    )
    .unwrap();
    state.commit(&viewport(&state, 1, 240.)).unwrap();
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(60.))])
        .unwrap();
    let old = state.measurement_cache().unwrap();
    state
        .prepare(&viewport(&state, 2, 480.), Instant::now())
        .unwrap();
    assert_eq!(old.cross_extent(), px(240.));
    assert!(
        state.measurement_cache().is_err(),
        "old measurements cannot be relabeled with new cross extent"
    );
}

#[test]
fn smooth_noop_terminal_regression() {
    for smooth in [false, true] {
        let mut state = VirtualizerState::new(
            Axis::Vertical,
            source(100),
            VirtualizerSizing::fixed(px(20.)),
        )
        .unwrap();
        let v = viewport(&state, 1, 240.);
        state.commit(&v).unwrap();
        state
            .scroll_to_key(
                0usize.into(),
                VirtualizerScrollOptions::default()
                    .align(VirtualizerAlign::Start)
                    .motion(if smooth {
                        VirtualizerMotion::Smooth {
                            duration: Duration::from_millis(200),
                            max_distance: px(100.),
                        }
                    } else {
                        VirtualizerMotion::Instant
                    }),
            )
            .unwrap();
        state.take_events();
        let start = Instant::now();
        let mut confirmations = 0;
        for frame in 0..15 {
            let (_, request) = state
                .prepare(&v, start + Duration::from_millis(frame * 10))
                .unwrap();
            if let Some(request) = request {
                confirmations += 1;
                state.confirm(&request, request.offset(), &v).unwrap();
            }
            if !state.has_scroll_command() {
                break;
            }
        }
        let outcome: Vec<_> = state
            .take_events()
            .into_iter()
            .filter(|event| matches!(event, VirtualizerEvent::CommandFinished { .. }))
            .collect();
        assert_eq!(confirmations, 1, "smooth={smooth}, events={outcome:?}");
        assert!(matches!(
            outcome.as_slice(),
            [VirtualizerEvent::CommandFinished {
                outcome: yue_component::VirtualizerCommandOutcome::Settled,
                ..
            }]
        ));
    }
}
