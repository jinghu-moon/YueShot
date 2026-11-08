//! Exact external feedback must not turn per-item tolerances into scroll drift.
use super::*;
use gpui_kit::size;
use yue_component::{VirtualMasonry, VirtualMasonryState, VirtualizerSizeUpdate};

#[test]
fn fractional_feedback_preserves_cumulative_geometry_and_frozen_snapshots() {
    let count = 8_192;
    let old_extent = px(20.);
    let new_extent = px(20. + 1. / 2_048.);
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for measured in [false, true] {
            let sizing = if measured {
                VirtualizerSizing::measured(old_extent)
            } else {
                VirtualizerSizing::known(vec![old_extent; count])
            };
            let mut state = VirtualizerState::new(axis, source(0, count), sizing).unwrap();
            state.commit(&viewport(&state, 1, 0.)).unwrap();
            let updates = (0..count)
                .map(|ix| VirtualizerSizeUpdate::new(ix, old_extent))
                .collect::<Vec<_>>();
            state.update_sizes(&updates).unwrap();
            let offset = (count / 2) as f64 * f64::from(old_extent) + 5.;
            state.commit(&viewport(&state, 2, offset)).unwrap();
            let frozen = state.snapshot(&viewport(&state, 2, offset)).unwrap();
            let updates = (0..count)
                .rev()
                .map(|ix| VirtualizerSizeUpdate::new(ix, new_extent))
                .collect::<Vec<_>>();
            state.update_sizes(&updates).unwrap();
            let next = state.snapshot(&viewport(&state, 3, offset)).unwrap();
            assert_eq!(next.total_extent(), count as f64 * f64::from(new_extent));
            assert_eq!(frozen.total_extent(), count as f64 * f64::from(old_extent));
            for ix in [0, 127, 128, count / 2, count - 1] {
                assert_eq!(next.item(ix).unwrap().extent(), new_extent);
                assert_eq!(
                    next.item(ix).unwrap().start(),
                    ix as f64 * f64::from(new_extent)
                );
                assert_eq!(frozen.item(ix).unwrap().extent(), old_extent);
            }
            assert_eq!(
                state.validate_snapshot(&frozen),
                Err(VirtualizerError::StaleSnapshot)
            );
            state.update_sizes(&updates).unwrap();
            state.validate_snapshot(&next).unwrap();
            let (_, request) = state
                .prepare(&viewport(&state, 3, offset), Instant::now())
                .unwrap();
            assert_eq!(
                request.unwrap().offset(),
                (count / 2) as f64 * f64::from(new_extent) + 5.
            );
        }
    }
}

#[test]
fn fractional_hints_and_padded_feedback_are_idempotent_in_native_slot_units() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for gap in [px(0.), px(0.1)] {
            let mut state =
                VirtualizerState::new(axis, source(0, 3), VirtualizerSizing::measured(px(20.)))
                    .unwrap();
            state
                .set_layout(VirtualizerLayout::default().gap(gap))
                .unwrap();
            state.commit(&viewport(&state, 1, 0.)).unwrap();
            let initial = [VirtualizerSizeUpdate::new(0usize, px(20.))];
            state.update_size_hints(&initial).unwrap();
            let old = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
            let extent = px(20. + 1. / 2_048.);
            let changed = [VirtualizerSizeUpdate::new(0usize, extent)];
            state.update_size_hints(&changed).unwrap();
            assert_eq!(
                state.validate_snapshot(&old),
                Err(VirtualizerError::StaleSnapshot)
            );
            let next = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
            assert_eq!(next.item(1).unwrap().start(), f64::from(extent + gap));
            assert!(!next.item(0).unwrap().is_measured());
            state.update_size_hints(&changed).unwrap();
            state.validate_snapshot(&next).unwrap();
            state.update_sizes(&changed).unwrap();
            let actual = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
            assert!(actual.item(0).unwrap().is_measured());
            state.update_sizes(&changed).unwrap();
            state.validate_snapshot(&actual).unwrap();
        }
    }
}

#[test]
fn masonry_fractional_feedback_updates_singletons_and_batches_without_lane_drift() {
    let count = 4_096;
    let old_extent = px(20.);
    let new_extent = px(20. + 1. / 256.);
    let mut state =
        VirtualMasonryState::new(source(0, count), VirtualizerSizing::measured(old_extent), 3)
            .unwrap();
    let bounds = size(px(300.), px(100.));
    let updates = (0..count)
        .map(|ix| VirtualizerSizeUpdate::new(ix, old_extent))
        .collect::<Vec<_>>();
    state.update_sizes(&updates).unwrap();
    let frozen = state.snapshot(0., bounds).unwrap();
    let singleton = [VirtualizerSizeUpdate::new(0usize, new_extent)];
    state.update_sizes(&singleton).unwrap();
    let single = state.snapshot(0., bounds).unwrap();
    assert_eq!(single.item(0).unwrap().item().extent(), new_extent);
    assert_eq!(single.revision(), frozen.revision() + 1);
    state.update_sizes(&singleton).unwrap();
    assert_eq!(
        state.snapshot(0., bounds).unwrap().revision(),
        single.revision()
    );
    let updates = (1..count)
        .rev()
        .map(|ix| VirtualizerSizeUpdate::new(ix, new_extent))
        .collect::<Vec<_>>();
    state.update_sizes(&updates).unwrap();
    let next = state.snapshot(0., bounds).unwrap();
    assert_eq!(next.revision(), single.revision() + 1);
    assert_eq!(
        next.total_extent(),
        count.div_ceil(3) as f64 * f64::from(new_extent)
    );
    assert_eq!(
        frozen.total_extent(),
        count.div_ceil(3) as f64 * f64::from(old_extent)
    );
    for ix in [0, 127, 128, count - 1] {
        assert_eq!(next.item(ix).unwrap().item().extent(), new_extent);
        assert_eq!(
            next.item(ix).unwrap().lane(),
            frozen.item(ix).unwrap().lane()
        );
        assert_eq!(frozen.item(ix).unwrap().item().extent(), old_extent);
    }
    state.update_sizes(&updates).unwrap();
    assert_eq!(
        state.snapshot(0., bounds).unwrap().revision(),
        next.revision()
    );
}

struct MeasuredMasonry {
    state: Entity<VirtualMasonryState>,
    builds: Rc<Cell<usize>>,
}
impl Render for MeasuredMasonry {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.builds.clone();
        VirtualMasonry::new("fractional-feedback", &self.state, move |_, _, _| {
            builds.set(builds.get() + 1);
            div().h(px(20.))
        })
        .w(px(300.))
        .h(px(100.))
    }
}

#[gpui_kit::test]
fn native_masonry_layout_corrects_fractional_feedback_then_stops_changing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut state =
            VirtualMasonryState::new(source(0, 300), VirtualizerSizing::measured(px(20.)), 3)
                .unwrap();
        let updates = (0..300)
            .map(|ix| VirtualizerSizeUpdate::new(ix, px(20. - 1. / 256.)))
            .collect::<Vec<_>>();
        state.update_sizes(&updates).unwrap();
        MeasuredMasonry {
            state: cx.new(|_| state),
            builds: Rc::new(Cell::new(0)),
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, builds) = view.read_with(cx, |v, _| (v.state.clone(), v.builds.clone()));
    let snapshot = state.read_with(cx, |s, _| s.snapshot(0., size(px(300.), px(100.))).unwrap());
    for ix in snapshot.working_indices() {
        assert_eq!(snapshot.item(ix).unwrap().item().extent(), px(20.));
    }
    assert_eq!(
        snapshot.item(299).unwrap().item().extent(),
        px(20. - 1. / 256.)
    );
    builds.set(0);
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    let next = state.read_with(cx, |s, _| s.snapshot(0., size(px(300.), px(100.))).unwrap());
    assert_eq!(next.revision(), snapshot.revision());
    assert_eq!(builds.get(), next.working_indices().len());
    // Consume the one continuation from the correction frame, then verify idle.
    cx.update(|w, cx| w.simulate_next_frame(cx));
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
}

#[test]
fn linear_feedback_keeps_empty_single_and_multi_batch_transactions_atomic() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 0..3 {
            for count in [0, 1, 32, 256] {
                let sizing = if mode == 0 {
                    VirtualizerSizing::known(vec![px(20.); 300])
                } else {
                    VirtualizerSizing::measured(px(20.))
                };
                let mut state = VirtualizerState::new(axis, source(0, 300), sizing).unwrap();
                state
                    .set_layout(VirtualizerLayout::default().gap(px(0.1)))
                    .unwrap();
                state.commit(&viewport(&state, 1, 105.)).unwrap();
                let old = state.snapshot(&viewport(&state, 1, 105.)).unwrap();
                let updates = (0..count)
                    .rev()
                    .map(|ix| VirtualizerSizeUpdate::new(ix, px(30.)))
                    .collect::<Vec<_>>();
                if mode == 2 {
                    state.update_size_hints(&updates).unwrap();
                } else {
                    state.update_sizes(&updates).unwrap();
                }
                let next = state.snapshot(&viewport(&state, 1, 105.)).unwrap();
                for ix in 0..300 {
                    assert!(
                        (f32::from(next.item(ix).unwrap().extent())
                            - if ix < count { 30. } else { 20. })
                        .abs()
                            < 0.00001
                    );
                    assert_eq!(
                        next.item(ix).unwrap().is_measured(),
                        mode == 0 || mode == 1 && ix < count
                    );
                }
                assert_eq!(state.validate_snapshot(&old).is_err(), count > 0);
                if mode == 2 {
                    state.update_size_hints(&updates).unwrap();
                } else {
                    state.update_sizes(&updates).unwrap();
                }
                state.validate_snapshot(&next).unwrap();
                for bad in [
                    vec![VirtualizerSizeUpdate::new(300usize, px(40.))],
                    vec![VirtualizerSizeUpdate::new(0usize, px(f32::NAN))],
                    vec![VirtualizerSizeUpdate::new(0usize, px(0.))],
                    vec![VirtualizerSizeUpdate::new(0usize, px(f32::INFINITY))],
                    vec![
                        VirtualizerSizeUpdate::new(0usize, px(40.)),
                        VirtualizerSizeUpdate::new(300usize, px(40.)),
                    ],
                    vec![
                        VirtualizerSizeUpdate::new(0usize, px(30.)),
                        VirtualizerSizeUpdate::new(0usize, px(40.)),
                    ],
                ] {
                    let result = if mode == 2 {
                        state.update_size_hints(&bad)
                    } else {
                        state.update_sizes(&bad)
                    };
                    assert!(result.is_err());
                    state.validate_snapshot(&next).unwrap();
                    assert_eq!(state.total_extent(), next.total_extent());
                }
                assert!((f32::from(old.item(0).unwrap().extent()) - 20.).abs() < 0.00001);
            }
        }
    }
    let mut state = fixed(3);
    for updates in [vec![], vec![VirtualizerSizeUpdate::new(0usize, px(30.))]] {
        assert_eq!(
            state.update_sizes(&updates),
            Err(VirtualizerError::WrongBackend)
        );
        assert_eq!(
            state.update_size_hints(&updates),
            Err(VirtualizerError::WrongBackend)
        );
    }
}
