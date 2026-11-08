use super::*;
use gpui_kit::base::virtualization::{VirtualMasonry, VirtualMasonryState};
use std::cell::RefCell;

type PreparedRows = Rc<RefCell<Vec<(usize, gpui_kit::WeakEntity<MasonryRow>)>>>;
struct MasonryRow(Option<gpui_kit::FocusHandle>);
impl Render for MasonryRow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if let Some(focus) = &self.0 {
            div()
                .id("prepared-focus")
                .track_focus(focus)
                .h(px(20.))
                .into_any_element()
        } else {
            div().h(px(20.)).into_any_element()
        }
    }
}
struct MasonryPreparation {
    state: Entity<VirtualMasonryState>,
    rows: PreparedRows,
    revision: u64,
    mounted: bool,
    cached: bool,
    width: f32,
    focus: Option<gpui_kit::FocusHandle>,
}
impl Render for MasonryPreparation {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if !self.mounted {
            return div().into_any_element();
        }
        let rows = self.rows.clone();
        let focus = self.focus.clone();
        let prepare = move |identity: yue_component::VirtualizerItemIdentity,
                            _: &mut Window,
                            cx: &mut App| {
            let row = cx.new(|_| {
                MasonryRow(if identity.index() == 0 {
                    focus.clone()
                } else {
                    None
                })
            });
            rows.borrow_mut().push((identity.index(), row.downgrade()));
            row
        };
        let element = if self.cached {
            VirtualMasonry::new_with_item_views("adoption-masonry", &self.state, prepare)
                .content_revision(self.revision)
        } else {
            VirtualMasonry::new("adoption-masonry", &self.state, move |item, w, cx| {
                prepare(item.item().identity(), w, cx)
            })
        };
        element.w(px(self.width)).h(px(100.)).into_any_element()
    }
}
fn sizing(mode: usize, n: usize) -> VirtualizerSizing {
    match mode {
        0 => VirtualizerSizing::fixed(px(20.)),
        1 => VirtualizerSizing::known(vec![px(20.); n]),
        _ => VirtualizerSizing::measured(px(20.)),
    }
}
struct RevisionSource(u64);
impl VirtualizerSource for RevisionSource {
    fn len(&self) -> usize {
        1000
    }
    fn key_at(&self, ix: usize) -> ElementId {
        ix.into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        if let ElementId::Integer(ix) = key {
            (*ix < 1000).then_some(*ix as usize)
        } else {
            None
        }
    }
    fn revision_at(&self, ix: usize) -> u64 {
        if ix == 0 { self.0 } else { 0 }
    }
}

#[gpui_kit::test]
fn masonry_preparation_reuses_geometry_and_refreshes_only_explicit_content(
    cx: &mut TestAppContext,
) {
    for mode in 0..3 {
        let (view, cx) = cx.add_window_view(|_, cx| MasonryPreparation {
            state: cx.new(|_| {
                VirtualMasonryState::new(Rc::new(RevisionSource(0)), sizing(mode, 1000), 3).unwrap()
            }),
            rows: Rc::default(),
            revision: 0,
            mounted: true,
            cached: true,
            width: 300.,
            focus: None,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let (state, rows) = view.read_with(cx, |v, _| (v.state.clone(), v.rows.clone()));
        let initial = rows.borrow().len();
        for _ in 0..4 {
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            });
        }
        assert_eq!(rows.borrow().len(), initial);
        view.update(cx, |v, cx| {
            v.width = 330.;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            rows.borrow().len(),
            initial,
            "width changes must not refresh content"
        );
        state.update(cx, |s, cx| {
            assert_eq!(
                s.invalidate_items(&[0usize.into(), 9999usize.into()]),
                Err(VirtualizerError::MissingKey)
            );
            s.invalidate_items(&[0usize.into(), 0usize.into()]).unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(rows.borrow().len(), initial + 1);
        assert_eq!(rows.borrow().last().unwrap().0, 0);
        let before = rows.borrow().len();
        state.update(cx, |s, cx| {
            s.append_source(source(0, 1001), sizing(mode, 1001))
                .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            rows.borrow().len(),
            before,
            "append preserves prefix content epochs"
        );
        // Dropping a local index domain establishes a new barrier, once only.
        state.update(cx, |s, cx| {
            s.replace_source(Rc::new(RevisionSource(0)), sizing(mode, 1000))
                .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let before = rows.borrow().len();
        state.update(cx, |s, cx| {
            s.replace_source(Rc::new(RevisionSource(1)), sizing(mode, 1000))
                .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            rows.borrow().len(),
            before + 1,
            "source revision refresh is item-local"
        );
        let before = rows.borrow().len();
        view.update(cx, |v, cx| {
            v.revision += 1;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(rows.borrow().len() - before, initial);
    }
}

#[gpui_kit::test]
fn masonry_preparation_releases_evicted_swapped_and_unmounted_views(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| MasonryPreparation {
        state: cx.new(|_| VirtualMasonryState::new(source(0, 1000), sizing(2, 1000), 3).unwrap()),
        rows: Rc::default(),
        revision: 0,
        mounted: true,
        cached: true,
        width: 300.,
        focus: None,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, rows) = view.read_with(cx, |v, _| (v.state.clone(), v.rows.clone()));
    let initial = rows.borrow().len();
    state.update(cx, |s, cx| {
        s.set_retained_keys(vec![0usize.into()]).unwrap();
        s.scroll_to_key(900usize.into(), VirtualizerAlign::Start)
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(
        rows.borrow()[0].1.upgrade().is_some(),
        "remote retained row stays prepared"
    );
    assert!(
        rows.borrow()[1..initial]
            .iter()
            .all(|(_, row)| row.upgrade().is_none())
    );
    let old = rows.borrow().clone();
    view.update(cx, |v, cx| {
        v.state =
            cx.new(|_| VirtualMasonryState::new(source(0, 1000), sizing(2, 1000), 3).unwrap());
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(
        old.iter().all(|(_, row)| row.upgrade().is_none()),
        "same mount id with a new owner releases old views"
    );
    view.update(cx, |v, cx| {
        v.cached = false;
        cx.notify();
    });
    let before = rows.borrow().len();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert!(
        rows.borrow().len() >= before + initial * 2,
        "ordinary renderer still runs every frame"
    );
    view.update(cx, |v, cx| {
        v.mounted = false;
        cx.notify();
    });
    for _ in 0..3 {
        cx.update(|w, cx| {
            w.simulate_next_frame(cx);
            w.draw(cx).clear(cx);
        });
    }
    cx.run_until_parked();
    assert!(rows.borrow().iter().all(|(_, row)| row.upgrade().is_none()));
}

#[test]
fn sparse_measurement_export_preserves_order_constraints_and_frozen_geometry() {
    let list = ListState::new_with_uniform_height(10_000, ListAlignment::Top, px(0.), px(20.));
    assert!(list.geometry().measured_items().is_empty());
    let updates = [
        (9999, gpui_kit::size(px(240.), px(37.))),
        (0, gpui_kit::size(px(240.), px(21.))),
        (501, gpui_kit::size(px(240.), px(23.))),
    ];
    list.set_item_sizes(&updates, true);
    let frozen = list.geometry();
    let expected = vec![updates[1], updates[2], updates[0]];
    assert_eq!(frozen.measured_items(), expected);
    list.remeasure_items(500..502);
    assert_eq!(
        list.geometry().measured_items(),
        vec![updates[1], updates[0]]
    );
    assert_eq!(frozen.measured_items(), expected);
    list.set_item_sizes(&[(700, gpui_kit::size(px(300.), px(24.)))], false);
    assert_eq!(
        list.geometry().measured_items(),
        vec![updates[1], updates[0]]
    );
}

#[test]
fn masonry_append_crosses_index_blocks_without_changing_frozen_lane_assignments() {
    let mut state =
        VirtualMasonryState::new(source(0, 761), VirtualizerSizing::measured(px(20.)), 3).unwrap();
    let mut frozen = Vec::new();
    for batch in 0..20 {
        let count = 761 + batch * 11;
        let snapshot = state
            .snapshot(0., gpui_kit::size(px(300.), px(100.)))
            .unwrap();
        frozen.push((count, snapshot));
        state
            .append_source(source(0, count + 11), VirtualizerSizing::measured(px(20.)))
            .unwrap();
        for (old_count, snapshot) in &frozen {
            assert!(snapshot.item(*old_count).is_err());
            for ix in [0, 255, 256, 760, old_count - 1] {
                let item = snapshot.item(ix).unwrap();
                assert_eq!(item.lane(), ix % 3);
                assert_eq!(item.item().start(), (ix / 3) as f64 * 20.);
                assert!(!item.item().is_measured());
            }
        }
    }
    state
        .update_sizes(&[yue_component::VirtualizerSizeUpdate::new(255usize, px(40.))])
        .unwrap();
    let current = state
        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
        .unwrap();
    assert_eq!(current.item(255).unwrap().item().extent(), px(40.));
    for (_, old) in frozen {
        assert_eq!(old.item(255).unwrap().item().extent(), px(20.));
        assert!(!old.item(255).unwrap().item().is_measured());
    }
}

#[gpui_kit::test]
fn masonry_preparation_keeps_a_focused_remote_view_and_releases_it_after_focus_moves(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let focus = cx.focus_handle();
        let mut state = VirtualMasonryState::new(source(0, 1000), sizing(2, 1000), 3).unwrap();
        state
            .set_focus_handle(0usize.into(), Some(focus.clone()))
            .unwrap();
        MasonryPreparation {
            state: cx.new(|_| state),
            rows: Rc::default(),
            revision: 0,
            mounted: true,
            cached: true,
            width: 300.,
            focus: Some(focus),
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, rows, focus) = view.read_with(cx, |v, _| {
        (v.state.clone(), v.rows.clone(), v.focus.clone().unwrap())
    });
    cx.update(|w, cx| focus.focus(w, cx));
    state.update(cx, |s, cx| {
        s.scroll_to_key(900usize.into(), VirtualizerAlign::Start)
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(rows.borrow()[0].1.upgrade().is_some());
    assert_eq!(
        rows.borrow().iter().filter(|(ix, _)| *ix == 0).count(),
        1,
        "focused content is prepared once"
    );
    assert!(cx.update(|w, _| focus.is_focused(w)));
    state.update(cx, |s, cx| {
        s.set_focus_handle(0usize.into(), None).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| {
        w.blur(cx);
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    assert!(rows.borrow()[0].1.upgrade().is_none());
}

#[gpui_kit::test]
fn masonry_width_epochs_never_resurrect_old_measurements_through_tail_writes(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| MasonryPreparation {
        state: cx.new(|_| VirtualMasonryState::new(source(0, 769), sizing(2, 769), 3).unwrap()),
        rows: Rc::default(),
        revision: 0,
        mounted: true,
        cached: true,
        width: 300.,
        focus: None,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let remote = [381usize, 384, 765, 768];
    state.update(cx, |s, _| {
        s.update_sizes(&remote.map(|ix| yue_component::VirtualizerSizeUpdate::new(ix, px(37.))))
            .unwrap()
    });
    let frozen = state.read_with(cx, |s, _| {
        s.snapshot(0., gpui_kit::size(px(300.), px(100.))).unwrap()
    });
    for frame in 0..3 {
        view.update(cx, |v, cx| {
            v.width = if frame % 2 == 0 { 360. } else { 300. };
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        state.update(cx, |s, _| {
            let n = 800 + frame * 11;
            s.append_source(source(0, n), sizing(2, n)).unwrap();
            s.update_sizes(&[yue_component::VirtualizerSizeUpdate::new(n - 1, px(41.))])
                .unwrap();
            let current = s.snapshot(0., gpui_kit::size(px(360.), px(100.))).unwrap();
            for ix in remote {
                let old = frozen.item(ix).unwrap();
                let new = current.item(ix).unwrap();
                assert!(old.item().is_measured());
                assert!(!new.item().is_measured(), "old width resurrected at {ix}");
                assert_eq!(new.item().extent(), old.item().extent());
            }
            assert!(current.item(n - 1).unwrap().item().is_measured());
        });
    }
    state.update(cx, |s, _| {
        s.replace_source(source(0, 822), sizing(2, 822)).unwrap();
        let current = s.snapshot(0., gpui_kit::size(px(360.), px(100.))).unwrap();
        for ix in remote {
            assert!(!current.item(ix).unwrap().item().is_measured());
        }
        s.reflow(4).unwrap();
        let current = s.snapshot(0., gpui_kit::size(px(360.), px(100.))).unwrap();
        assert!(!current.item(821).unwrap().item().is_measured());
    });
}

#[test]
fn masonry_cold_pack_preserves_slots_at_block_boundaries_and_with_empty_lanes() {
    for count in [0, 1, 255, 256, 257, 769] {
        for lane_count in [1, 3, 300] {
            let extents: Vec<_> = (0..count).map(|ix| px(10. + (ix % 13) as f32)).collect();
            let mut state = VirtualMasonryState::new(
                source(0, count),
                VirtualizerSizing::known(Rc::<[gpui_kit::Pixels]>::from(extents.clone())),
                lane_count,
            )
            .unwrap();
            for lanes in [lane_count, lane_count + 1] {
                if lanes != lane_count {
                    state.reflow(lanes).unwrap();
                }
                let snapshot = state
                    .snapshot(0., gpui_kit::size(px(300.), px(100.)))
                    .unwrap();
                let mut ends = vec![0f64; lanes];
                for (ix, extent) in extents.iter().copied().enumerate() {
                    let lane = (0..lanes)
                        .min_by(|a, b| ends[*a].total_cmp(&ends[*b]).then(a.cmp(b)))
                        .unwrap();
                    let item = snapshot.item(ix).unwrap();
                    assert_eq!(item.lane(), lane);
                    assert_eq!(item.item().start(), ends[lane]);
                    assert_eq!(item.item().extent(), extent);
                    ends[lane] += f64::from(extent);
                }
                assert!(snapshot.item(count).is_err());
                assert_eq!(state.total_extent(), ends.into_iter().fold(0., f64::max));
            }
        }
    }
}
