//! Composition, invalid-input and progress boundaries for the follow-up fixes.
use super::*;
use yue_component::{
    VirtualGridSnapshot, VirtualMasonryState, VirtualizerCache, VirtualizerMeasurement,
    VirtualizerSizeUpdate,
};

fn measured(axis: Axis) -> VirtualizerState {
    VirtualizerState::new(
        axis,
        super::source(0, 100),
        VirtualizerSizing::measured(px(20.)),
    )
    .unwrap()
}

struct RemountMasonryHarness {
    state: Entity<VirtualMasonryState>,
    mounted: bool,
}
impl Render for RemountMasonryHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.mounted {
            yue_component::VirtualMasonry::new("remount-masonry", &self.state, |_, _, _| {
                div().h(px(20.))
            })
            .w(px(300.))
            .h(px(100.))
            .into_any_element()
        } else {
            div().into_any_element()
        }
    }
}

#[gpui_kit::test]
fn masonry_unmount_expires_old_writes_even_when_the_state_survives(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| RemountMasonryHarness {
        state: cx.new(|_| {
            VirtualMasonryState::new(source(0, 100), VirtualizerSizing::measured(px(20.)), 3)
                .unwrap()
        }),
        mounted: true,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| v.state.clone());
    let old = state.read_with(cx, |s, _| {
        s.snapshot(0., gpui_kit::size(px(300.), px(100.))).unwrap()
    });
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
    assert_eq!(
        state.update(cx, |s, _| s.update_sizes_from_snapshot(
            &old,
            &[VirtualizerSizeUpdate::new(0usize, px(99.))]
        )),
        Err(VirtualizerError::StaleSnapshot)
    );
    assert_eq!(old.item(0).unwrap().item().extent(), px(20.));
    view.update(cx, |v, cx| {
        v.mounted = true;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    state.update(cx, |s, _| {
        let current = s.snapshot(0., gpui_kit::size(px(300.), px(100.))).unwrap();
        s.update_sizes_from_snapshot(&current, &[VirtualizerSizeUpdate::new(0usize, px(20.))])
            .unwrap();
        assert_eq!(
            s.update_sizes_from_snapshot(&old, &[]),
            Err(VirtualizerError::StaleSnapshot)
        );
    });
}

struct CacheNativeHarness {
    state: Entity<VirtualizerState>,
    axis: Axis,
    cross: Pixels,
}
impl Render for CacheNativeHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        VirtualList::new("cache-native", &self.state, |_, _, _| {
            div().w(px(60.)).h(px(20.))
        })
        .w(if self.axis == Axis::Vertical {
            self.cross
        } else {
            px(100.)
        })
        .h(if self.axis == Axis::Horizontal {
            self.cross
        } else {
            px(100.)
        })
    }
}

#[gpui_kit::test]
fn native_resize_finishes_constraint_transition_before_export(cx: &mut TestAppContext) {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        let (view, cx) = cx.add_window_view(|_, cx| CacheNativeHarness {
            state: cx.new(|_| measured(axis)),
            axis,
            cross: px(240.),
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let state = view.read_with(cx, |v, _| v.state.clone());
        let old = state.read_with(cx, |s, _| s.measurement_cache().unwrap());
        assert!(!old.items().is_empty());
        assert_eq!(old.cross_extent(), px(240.));
        view.update(cx, |v, cx| {
            v.cross = px(480.);
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let new = state.read_with(cx, |s, _| s.measurement_cache().unwrap());
        assert!(!new.items().is_empty());
        assert_eq!(new.cross_extent(), px(480.));
        assert_eq!(old.cross_extent(), px(240.));
        state.update(cx, |s, cx| {
            s.restore_measurement_cache(&new).unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            state.read_with(cx, |s, _| s.measurement_cache().unwrap().cross_extent()),
            px(480.)
        );
    }
}

#[test]
fn empty_grid_rows_do_not_materialize_a_large_column_workset() {
    let binding = VirtualizerBinding::new();
    let a = super::source(0, 0);
    let b = super::source(0, 100_000);
    let rows = VirtualizerState::new_attached(
        Axis::Vertical,
        a.clone(),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let columns = VirtualizerState::new_attached(
        Axis::Horizontal,
        b.clone(),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let rv = VirtualizerViewport::new(&binding, 1, 0., px(100.), px(2_000_000.));
    let cv = VirtualizerViewport::new(&binding, 1, 0., px(2_000_000.), px(100.));
    let grid =
        VirtualGridSnapshot::new(rows.snapshot(&rv).unwrap(), columns.snapshot(&cv).unwrap())
            .unwrap();
    a.calls.set(0);
    b.calls.set(0);
    assert_eq!(grid.cells().count(), 0);
    assert_eq!(a.calls.get(), 0);
    assert_eq!(b.calls.get(), 0);
}

#[test]
fn cache_export_preserves_both_axes_and_rejects_mixed_measurement_constraints() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        let mut state = measured(axis);
        assert!(state.measurement_cache().is_err());
        state.commit(&super::viewport(&state, 1, 0.)).unwrap();
        state
            .update_sizes(&[
                VirtualizerSizeUpdate::new(0usize, px(60.)),
                VirtualizerSizeUpdate::new(1usize, px(70.)),
            ])
            .unwrap();
        let old = state.measurement_cache().unwrap();
        assert_eq!(old.cross_extent(), px(240.));
        let next = VirtualizerViewport::new(state.binding().unwrap(), 2, 0., px(100.), px(480.))
            .input_revision(0);
        state.prepare(&next, Instant::now()).unwrap();
        assert!(state.measurement_cache().is_err());
        // An equal main extent at a different constraint is a fresh measurement.
        state
            .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(60.))])
            .unwrap();
        assert!(
            state.measurement_cache().is_err(),
            "one old-width record remains"
        );
        state
            .update_sizes(&[VirtualizerSizeUpdate::new(1usize, px(70.))])
            .unwrap();
        let new = state.measurement_cache().unwrap();
        assert_eq!(new.cross_extent(), px(480.));
        assert_eq!(new.items().len(), 2);
        assert_eq!(old.cross_extent(), px(240.));
        let mut restored = measured(axis);
        assert_eq!(restored.restore_measurement_cache(&new).unwrap(), 2);
        let next = VirtualizerViewport::new(restored.binding().unwrap(), 1, 0., px(100.), px(480.));
        let frozen = restored.snapshot(&next).unwrap();
        assert_eq!(frozen.item(0).unwrap().extent(), px(60.));
        assert!(frozen.item(0).unwrap().is_measured());
        restored.commit(&next).unwrap();
        restored
            .set_layout(VirtualizerLayout::default().revision(1))
            .unwrap();
        assert!(restored.measurement_cache().unwrap().items().is_empty());
        restored
            .commit(&VirtualizerViewport::new(
                restored.binding().unwrap(),
                2,
                0.,
                px(100.),
                px(0.),
            ))
            .unwrap();
        assert!(restored.measurement_cache().is_err());
    }
}

#[test]
fn cache_restore_validates_atomically_then_invalidates_incompatible_existing_measurements() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        let mut state = measured(axis);
        state.commit(&super::viewport(&state, 1, 0.)).unwrap();
        state
            .update_sizes(&[VirtualizerSizeUpdate::new(1usize, px(70.))])
            .unwrap();
        let next = VirtualizerViewport::new(state.binding().unwrap(), 2, 0., px(100.), px(480.))
            .input_revision(0);
        state.prepare(&next, Instant::now()).unwrap();
        let before = state.snapshot(&next).unwrap();
        for records in [
            vec![VirtualizerMeasurement::new(0usize, 0, px(f32::NAN))],
            vec![
                VirtualizerMeasurement::new(0usize, 0, px(50.)),
                VirtualizerMeasurement::new(0usize, 0, px(60.)),
            ],
        ] {
            let bad = VirtualizerCache::new(axis, 0, px(480.), px(0.), records);
            assert!(state.restore_measurement_cache(&bad).is_err());
            state.validate_snapshot(&before).unwrap();
            assert!(
                state
                    .snapshot(&next)
                    .unwrap()
                    .item(1)
                    .unwrap()
                    .is_measured()
            );
        }
        let good = VirtualizerCache::new(
            axis,
            0,
            px(480.),
            px(0.),
            vec![VirtualizerMeasurement::new(0usize, 0, px(50.))],
        );
        assert_eq!(state.restore_measurement_cache(&good).unwrap(), 1);
        let after = state.snapshot(&next).unwrap();
        assert!(after.item(0).unwrap().is_measured());
        assert!(
            !after.item(1).unwrap().is_measured(),
            "old constraint must become a hint"
        );
        assert_eq!(after.item(1).unwrap().extent(), px(70.));
        assert_eq!(state.measurement_cache().unwrap().items().len(), 1);
        assert_eq!(
            state.validate_snapshot(&before),
            Err(VirtualizerError::StaleSnapshot)
        );
    }
}

#[test]
fn masonry_content_epochs_are_atomic_repeatable_and_owner_guarded() {
    let make = || {
        VirtualMasonryState::new(
            super::source(0, 100),
            VirtualizerSizing::measured(px(20.)),
            3,
        )
        .unwrap()
    };
    let mut state = make();
    for _ in 0..3 {
        let old = state
            .snapshot(0., gpui_kit::size(px(300.), px(100.)))
            .unwrap();
        state.invalidate_items(&[]).unwrap();
        assert!(state.update_sizes_from_snapshot(&old, &[]).is_ok());
        assert_eq!(
            state.invalidate_items(&[0usize.into(), 100usize.into()]),
            Err(VirtualizerError::MissingKey)
        );
        state.update_sizes_from_snapshot(&old, &[]).unwrap();
        state
            .invalidate_items(&[0usize.into(), 0usize.into()])
            .unwrap();
        assert_eq!(
            state.update_sizes_from_snapshot(&old, &[VirtualizerSizeUpdate::new(0usize, px(99.))]),
            Err(VirtualizerError::StaleSnapshot)
        );
        assert_eq!(old.item(0).unwrap().item().extent(), px(20.));
    }
    let foreign = make()
        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
        .unwrap();
    assert_eq!(
        state.update_sizes_from_snapshot(&foreign, &[]),
        Err(VirtualizerError::StaleSnapshot)
    );
    let old = state
        .snapshot(0., gpui_kit::size(px(300.), px(100.)))
        .unwrap();
    drop(state);
    assert_eq!(
        make().update_sizes_from_snapshot(&old, &[]),
        Err(VirtualizerError::StaleSnapshot)
    );
}

#[test]
fn smooth_noop_nearest_clamp_and_tolerance_settle_once_without_hiding_stalls() {
    let options = VirtualizerScrollOptions::default().motion(VirtualizerMotion::Smooth {
        duration: Duration::from_millis(200),
        max_distance: px(100.),
    });
    for target in [0_f64, 0.005, 1900., 9999.] {
        let mut state = super::fixed(100);
        let actual = target.min(1900.);
        let v = super::viewport(&state, 1, if target < 1. { 0. } else { actual });
        state.commit(&v).unwrap();
        state.scroll_to_offset(target, options).unwrap();
        let (_, request) = state.prepare(&v, Instant::now()).unwrap();
        let request = request.unwrap();
        assert!(request.is_settled());
        state.confirm(&request, v.offset(), &v).unwrap();
        assert!(!state.has_scroll_command());
        assert_eq!(
            state
                .take_events()
                .iter()
                .filter(|e| matches!(
                    e,
                    VirtualizerEvent::CommandFinished {
                        outcome: VirtualizerCommandOutcome::Settled,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            state.confirm(&request, v.offset(), &v),
            Err(VirtualizerError::RequestAlreadyAccepted)
        );
    }
    let mut state = super::fixed(100);
    let v = super::viewport(&state, 1, 0.);
    state.commit(&v).unwrap();
    state.scroll_to_key(2usize.into(), options).unwrap(); // Nearest is already visible.
    let (_, request) = state.prepare(&v, Instant::now()).unwrap();
    assert!(request.as_ref().unwrap().is_settled());
    state.confirm(&request.unwrap(), 0., &v).unwrap();
    state.take_events();
    state.scroll_to_offset(50., options).unwrap();
    let start = Instant::now();
    for frame in 0..12 {
        if let Some(request) = state
            .prepare(&v, start + Duration::from_millis(frame * 30))
            .unwrap()
            .1
        {
            state.confirm(&request, 0., &v).unwrap(); // a host refusing every proposal
        }
    }
    assert!(state.take_events().iter().any(|e| matches!(
        e,
        VirtualizerEvent::CommandFinished {
            outcome: VirtualizerCommandOutcome::NoProgress,
            ..
        }
    )));
    let mut dynamic = measured(Axis::Vertical);
    let v = super::viewport(&dynamic, 1, 0.);
    dynamic.scroll_to_key(0usize.into(), options).unwrap();
    assert!(!dynamic.prepare(&v, start).unwrap().1.unwrap().is_settled());
    dynamic.cancel_scroll();
    assert!(!dynamic.has_scroll_command());
}

#[test]
fn grid_axis_preparation_matches_frozen_sparse_geometry_in_both_backends() {
    for known in [false, true] {
        for (height, width) in [(100., 240.), (0., 240.), (100., 0.)] {
            let binding = VirtualizerBinding::new();
            let a = super::source(0, 100_000);
            let b = super::source(0, 100_000);
            let sizing = || {
                if known {
                    VirtualizerSizing::known(vec![px(20.); 100_000])
                } else {
                    VirtualizerSizing::fixed(px(20.))
                }
            };
            let mut rows =
                VirtualizerState::new_attached(Axis::Vertical, a.clone(), sizing(), &binding)
                    .unwrap();
            let mut columns =
                VirtualizerState::new_attached(Axis::Horizontal, b.clone(), sizing(), &binding)
                    .unwrap();
            rows.set_retained_keys(vec![99_999usize.into()]).unwrap();
            columns.set_retained_keys(vec![99_998usize.into()]).unwrap();
            let rv = VirtualizerViewport::new(&binding, 1, 200., px(height), px(width));
            let cv = VirtualizerViewport::new(&binding, 1, 400., px(width), px(height));
            let grid = VirtualGridSnapshot::new(
                rows.snapshot(&rv).unwrap(),
                columns.snapshot(&cv).unwrap(),
            )
            .unwrap();
            let expected: Vec<_> = grid
                .rows()
                .working_indices()
                .into_iter()
                .flat_map(|r| {
                    grid.columns()
                        .working_indices()
                        .into_iter()
                        .map(move |c| (r, c))
                })
                .collect();
            a.calls.set(0);
            b.calls.set(0);
            let actual: Vec<_> = grid
                .cells()
                .map(|cell| {
                    (
                        cell.row().index(),
                        cell.column().index(),
                        cell.row().start(),
                        cell.column().start(),
                    )
                })
                .collect();
            let row_reads = a.calls.get();
            let column_reads = b.calls.get();
            assert_eq!(
                actual
                    .iter()
                    .map(|&(r, c, _, _)| (r, c))
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(row_reads <= grid.rows().working_indices().len() * 2 + 1);
            assert!(column_reads <= grid.columns().working_indices().len() * 2 + 1);
            for (r, c, y, x) in actual {
                assert_eq!(y, r as f64 * 20.);
                assert_eq!(x, c as f64 * 20.);
            }
            drop(rows);
            drop(columns);
            assert_eq!(
                grid.cells().count(),
                expected.len(),
                "immutable snapshots survive owner release"
            );
        }
    }
}
