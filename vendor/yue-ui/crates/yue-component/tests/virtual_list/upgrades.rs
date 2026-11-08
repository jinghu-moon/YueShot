//! Regression contracts for the source-review follow-up. No native UI is launched.
use super::*;
use yue_component::{
    VirtualizerCache, VirtualizerMeasurement, VirtualizerScrollDirection, VirtualizerSizeUpdate,
};

struct Keys {
    values: Vec<(usize, u64)>,
    lookup: std::collections::HashMap<ElementId, usize>,
}
impl Keys {
    fn new(values: Vec<(usize, u64)>) -> Rc<Self> {
        let lookup = values
            .iter()
            .enumerate()
            .map(|(ix, (key, _))| ((*key).into(), ix))
            .collect();
        Rc::new(Self { values, lookup })
    }
}
impl VirtualizerSource for Keys {
    fn len(&self) -> usize {
        self.values.len()
    }
    fn key_at(&self, ix: usize) -> ElementId {
        self.values[ix].0.into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        self.lookup.get(key).copied()
    }
    fn revision_at(&self, ix: usize) -> u64 {
        self.values[ix].1
    }
}
fn measured(count: usize) -> VirtualizerState {
    VirtualizerState::new(
        Axis::Vertical,
        source(0, count),
        VirtualizerSizing::measured(px(20.)),
    )
    .unwrap()
}

#[test]
fn horizontal_fixed_keeps_constant_metadata() {
    let source = source(0, 1_000_000);
    let state = VirtualizerState::new(
        Axis::Horizontal,
        source.clone(),
        VirtualizerSizing::fixed(px(20.)),
    )
    .expect("fixed arithmetic is independent of axis");
    assert_eq!(source.calls.get(), 0);
    let snapshot = state.snapshot(&viewport(&state, 1, 80.)).unwrap();
    assert_eq!(snapshot.visible_range(), 4..9);
    assert_eq!(snapshot.total_extent(), 20_000_000.);
}

#[test]
fn actual_scrolling_direction_idle_and_command_lifetime_are_distinct() {
    let mut state = fixed(100);
    let now = Instant::now();
    state.prepare(&viewport(&state, 1, 0.), now).unwrap();
    state
        .scroll_to_key(80usize.into(), VirtualizerScrollOptions::default())
        .unwrap();
    assert!(state.has_scroll_command());
    assert!(!state.is_scrolling());
    state
        .prepare(&viewport(&state, 2, 30.).input_revision(1), now)
        .unwrap();
    assert!(!state.has_scroll_command());
    assert!(state.is_scrolling());
    assert_eq!(
        state.scroll_direction(),
        Some(VirtualizerScrollDirection::Forward)
    );
    state
        .prepare(
            &viewport(&state, 3, 10.).input_revision(2),
            now + Duration::from_millis(30),
        )
        .unwrap();
    assert_eq!(
        state.scroll_direction(),
        Some(VirtualizerScrollDirection::Backward)
    );
    state.poll_scroll_idle(now + Duration::from_millis(149));
    assert!(state.is_scrolling());
    state.poll_scroll_idle(now + Duration::from_millis(150));
    assert!(!state.is_scrolling());
    assert_eq!(state.scroll_direction(), None);
    state.poll_scroll_idle(now + Duration::from_secs(1));
    assert_eq!(
        state
            .take_events()
            .iter()
            .filter(|e| matches!(e, VirtualizerEvent::ScrollEnd))
            .count(),
        1
    );
}

#[test]
fn fixed_and_known_follow_tail_pause_and_resume_at_end() {
    for known in [false, true] {
        let sizing = |count| {
            if known {
                VirtualizerSizing::known(vec![px(20.); count])
            } else {
                VirtualizerSizing::fixed(px(20.))
            }
        };
        let mut state = VirtualizerState::new(Axis::Horizontal, source(0, 10), sizing(10)).unwrap();
        state.set_follow_tail(true).unwrap();
        let v = viewport(&state, 1, 0.);
        let (_, request) = state.prepare(&v, Instant::now()).unwrap();
        let request = request.unwrap();
        assert_eq!(request.offset(), 100.);
        state.confirm(&request, 100., &v).unwrap();
        state
            .replace_source(
                source(0, 12),
                sizing(12),
                VirtualizerChange::Splice {
                    old: 10..10,
                    inserted: 2,
                },
            )
            .unwrap();
        let v = viewport(&state, 2, 100.);
        let (_, request) = state.prepare(&v, Instant::now()).unwrap();
        assert_eq!(request.as_ref().unwrap().offset(), 140.);
        state.confirm(&request.unwrap(), 140., &v).unwrap();
        let v = viewport(&state, 3, 40.).input_revision(1);
        state.prepare(&v, Instant::now()).unwrap();
        state.commit(&v).unwrap();
        assert!(!state.is_following_tail());
        let v = viewport(&state, 4, 140.).input_revision(2);
        state.prepare(&v, Instant::now()).unwrap();
        state.commit(&v).unwrap();
        assert!(state.is_following_tail());
        state
            .replace_source(
                source(500, 10),
                sizing(10),
                VirtualizerChange::Replace {
                    preserve_anchor: false,
                },
            )
            .unwrap();
        assert!(
            !state.is_following_tail(),
            "new query must not inherit old tail session"
        );
        let v = viewport(&state, 5, 140.).input_revision(2);
        let (_, request) = state.prepare(&v, Instant::now()).unwrap();
        assert_eq!(request.unwrap().offset(), 0.);
    }
}

#[test]
fn per_item_estimates_batch_sizes_and_stale_callbacks_are_atomic() {
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        source(0, 10),
        VirtualizerSizing::measured_with_estimates(px(20.), |ix| px(10. + ix as f32)),
    )
    .unwrap();
    assert_eq!(state.total_extent(), 145.);
    state
        .set_layout(VirtualizerLayout::default().gap(px(3.)))
        .unwrap();
    // A gap-only layout change preserves per-item estimates too.
    let snapshot = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
    assert_eq!(snapshot.item(3).unwrap().extent(), px(13.));
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(3usize, px(70.))])
        .unwrap();
    let snapshot = state.snapshot(&viewport(&state, 2, 0.)).unwrap();
    assert!(snapshot.item(3).unwrap().is_measured());
    assert_eq!(snapshot.item(3).unwrap().extent(), px(70.));
    let before = state.total_extent();
    assert!(
        state
            .update_sizes(&[
                VirtualizerSizeUpdate::new(3usize, px(1.)),
                VirtualizerSizeUpdate::new(4usize, px(f32::NAN))
            ])
            .is_err()
    );
    assert_eq!(state.total_extent(), before);
    state
        .update_size_hints(&[VirtualizerSizeUpdate::new(3usize, px(45.))])
        .unwrap();
    assert!(
        !state
            .snapshot(&viewport(&state, 3, 0.))
            .unwrap()
            .item(3)
            .unwrap()
            .is_measured()
    );
    assert_eq!(
        state.update_sizes_from_snapshot(&snapshot, &[VirtualizerSizeUpdate::new(3usize, px(80.))]),
        Err(VirtualizerError::StaleSnapshot)
    );
    state
        .update_sizes(&[VirtualizerSizeUpdate::new(4usize, px(60.))])
        .unwrap();
    state
        .replace_source(
            source(0, 10),
            VirtualizerSizing::measured_with_estimates(px(20.), |ix| px(100. + ix as f32)),
            VirtualizerChange::Reorder,
        )
        .unwrap();
    let next = state.snapshot(&viewport(&state, 4, 0.)).unwrap();
    assert_eq!(
        next.item(0).unwrap().extent(),
        px(100.),
        "new hints replace old estimates"
    );
    assert_eq!(next.item(3).unwrap().extent(), px(103.));
    assert_eq!(
        next.item(4).unwrap().extent(),
        px(60.),
        "actual measurements take precedence"
    );
    assert!(next.item(4).unwrap().is_measured());
}

#[test]
fn known_updates_keep_old_snapshots_and_recompute_only_changed_tree_paths() {
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        source(0, 100_000),
        VirtualizerSizing::known(vec![px(20.); 100_000]),
    )
    .unwrap();
    let old = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
    state
        .update_sizes(&[
            VirtualizerSizeUpdate::new(5usize, px(40.)),
            VirtualizerSizeUpdate::new(90_000usize, px(10.)),
        ])
        .unwrap();
    let new = state.snapshot(&viewport(&state, 2, 0.)).unwrap();
    assert_eq!(old.item(6).unwrap().start(), 120.);
    assert_eq!(new.item(6).unwrap().start(), 140.);
    assert_eq!(new.total_extent(), 2_000_010.);
    state
        .set_layout(VirtualizerLayout::default().gap(px(2.)))
        .unwrap();
    assert_eq!(
        state
            .snapshot(&viewport(&state, 3, 0.))
            .unwrap()
            .item(5)
            .unwrap()
            .extent(),
        px(40.)
    );
}

#[test]
fn reorder_retains_measurements_by_key_but_invalidates_changed_content() {
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        Keys::new(vec![(10, 0), (20, 0), (30, 0)]),
        VirtualizerSizing::measured(px(20.)),
    )
    .unwrap();
    state
        .set_layout(VirtualizerLayout::default().gap(px(5.)))
        .unwrap();
    state
        .update_sizes(&[
            VirtualizerSizeUpdate::new(10usize, px(31.)),
            VirtualizerSizeUpdate::new(20usize, px(42.)),
            VirtualizerSizeUpdate::new(30usize, px(53.)),
        ])
        .unwrap();
    state
        .replace_source(
            Keys::new(vec![(30, 0), (20, 1), (10, 0)]),
            VirtualizerSizing::measured(px(20.)),
            VirtualizerChange::Reorder,
        )
        .unwrap();
    let snapshot = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
    assert_eq!(snapshot.item(0).unwrap().extent(), px(53.));
    assert!(snapshot.item(0).unwrap().is_measured());
    assert_eq!(snapshot.item(1).unwrap().extent(), px(20.));
    assert!(!snapshot.item(1).unwrap().is_measured());
    assert_eq!(snapshot.item(2).unwrap().extent(), px(31.));
    assert!(snapshot.item(2).unwrap().is_measured());
}

#[test]
fn retained_and_extracted_items_form_a_small_unique_working_set() {
    let mut state = fixed(1_000_000);
    state
        .set_retained_keys(vec![999_999usize.into(), 20usize.into()])
        .unwrap();
    state.set_range_extractor(Some(Rc::new(|range| {
        vec![range.visible_range().start / 10 * 10, 20]
    })));
    let snapshot = state.snapshot(&viewport(&state, 1, 470.)).unwrap();
    assert_eq!(
        snapshot.working_indices(),
        vec![20, 23, 24, 25, 26, 27, 28, 999_999]
    );
    assert!(
        state
            .set_retained_keys(vec![1_000_000usize.into()])
            .is_err()
    );
    state.set_range_extractor(Some(Rc::new(|_| vec![1_000_000])));
    assert!(matches!(
        state.snapshot(&viewport(&state, 2, 0.)),
        Err(VirtualizerError::InvalidChange)
    ));
}

#[test]
fn alignment_clearance_origin_absolute_and_relative_commands_share_one_protocol() {
    let mut state = fixed(100);
    state
        .set_layout(VirtualizerLayout::default().scroll_padding(px(15.), px(10.)))
        .unwrap();
    let v = viewport(&state, 1, 0.)
        .with_content_origin(60.)
        .with_scroll_extent(2_200.);
    state.commit(&v).unwrap();
    let options = VirtualizerScrollOptions::default().align(VirtualizerAlign::Start);
    state.scroll_to_key(10usize.into(), options).unwrap();
    let (_, request) = state.prepare(&v, Instant::now()).unwrap();
    assert_eq!(request.as_ref().unwrap().offset(), 245.);
    state.confirm(&request.unwrap(), 245., &v).unwrap();
    state.scroll_by(35., options).unwrap();
    let v = v.with_offset(245.);
    let (_, request) = state.prepare(&v, Instant::now()).unwrap();
    assert_eq!(request.as_ref().unwrap().offset(), 280.);
    state.confirm(&request.unwrap(), 280., &v).unwrap();
    state.scroll_to_offset(9_000., options).unwrap();
    let (_, request) = state.prepare(&v.with_offset(280.), Instant::now()).unwrap();
    assert_eq!(request.unwrap().offset(), 2_100.);
}

#[test]
fn measurement_cache_survives_controller_recreation_and_validates_layout_and_content() {
    let mut state = measured(100);
    state.commit(&viewport(&state, 1, 10.)).unwrap();
    state
        .update_sizes(&[
            VirtualizerSizeUpdate::new(0usize, px(50.)),
            VirtualizerSizeUpdate::new(90usize, px(70.)),
        ])
        .unwrap();
    let cache = state.measurement_cache().unwrap();
    assert_eq!(cache.items().len(), 2);
    drop(state);
    let mut restored = measured(100);
    assert_eq!(restored.restore_measurement_cache(&cache).unwrap(), 2);
    let v = viewport(&restored, 1, 0.);
    let (snapshot, request) = restored.prepare(&v, Instant::now()).unwrap();
    assert_eq!(snapshot.item(90).unwrap().extent(), px(70.));
    assert!(snapshot.item(90).unwrap().is_measured());
    assert_eq!(request.unwrap().offset(), 10.);
    let bad = VirtualizerCache::new(Axis::Vertical, 1, px(240.), px(0.), cache.items().to_vec());
    assert!(restored.restore_measurement_cache(&bad).is_err());
    let bad = VirtualizerCache::new(
        Axis::Vertical,
        0,
        px(240.),
        px(0.),
        vec![VirtualizerMeasurement::new(0usize, 0, px(f32::NAN))],
    );
    let before = restored.total_extent();
    assert!(restored.restore_measurement_cache(&bad).is_err());
    assert_eq!(restored.total_extent(), before);
    let mut changed = VirtualizerState::new(
        Axis::Vertical,
        Keys::new(vec![(0, 1), (90, 0)]),
        VirtualizerSizing::measured(px(20.)),
    )
    .unwrap();
    assert_eq!(changed.restore_measurement_cache(&cache).unwrap(), 1);
}

#[gpui_kit::test]
fn measured_retention_mounts_a_remote_card_without_building_intervening_rows(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut state = measured(10_000);
        state.set_retained_keys(vec![9_999usize.into()]).unwrap();
        CardHarness {
            state: cx.new(|_| state),
            builds: Rc::new(Cell::new(0)),
            children: 1,
            yue: false,
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| {
        assert!(v.builds.get() <= 14);
        v.state.clone()
    });
    let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert!(snapshot.item(9_999).unwrap().is_measured());
    assert!(snapshot.working_indices().contains(&9_999));
}

struct HorizontalMeasured {
    state: Entity<VirtualizerState>,
    builds: Rc<Cell<usize>>,
    clicks: Rc<std::cell::RefCell<Vec<usize>>>,
}
impl Render for HorizontalMeasured {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.builds.clone();
        let clicks = self.clicks.clone();
        VirtualList::new("horizontal-measured", &self.state, move |item, _, _| {
            builds.set(builds.get() + 1);
            let clicks = clicks.clone();
            let index = item.index();
            div()
                .id(("horizontal-child", index))
                .w(px(30. + (item.index() % 3) as f32 * 10.))
                .h_full()
                .child(format!("{}", item.index()))
                .on_click(move |_, _, _| clicks.borrow_mut().push(index))
        })
        .w(px(200.))
        .h(px(80.))
        .test_support()
    }
}
#[gpui_kit::test]
fn horizontal_measured_uses_intrinsic_width_native_wheel_and_scrollbar_projection(
    cx: &mut TestAppContext,
) {
    use gpui_kit::base::ScrollbarHandle as _;
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut state = VirtualizerState::new(
            Axis::Horizontal,
            source(0, 100),
            VirtualizerSizing::measured(px(40.)),
        )
        .unwrap();
        state
            .set_layout(
                VirtualizerLayout::default()
                    .gap(px(4.))
                    .padding(px(12.), px(24.)),
            )
            .unwrap();
        HorizontalMeasured {
            state: cx.new(|_| state),
            builds: Rc::new(Cell::new(0)),
            clicks: Rc::default(),
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let state = view.read_with(cx, |v, _| {
        assert!(v.builds.get() < 25);
        v.state.clone()
    });
    let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert_eq!(snapshot.item(0).unwrap().extent(), px(30.));
    assert_eq!(snapshot.item(1).unwrap().extent(), px(40.));
    assert_eq!(snapshot.item(2).unwrap().extent(), px(50.));
    assert_eq!(snapshot.item(0).unwrap().start(), 12.);
    assert_eq!(snapshot.item(1).unwrap().start(), 46.);
    let handle = state.read_with(cx, |s, _| s.scrollbar_handle().unwrap());
    assert_eq!(
        handle.viewport_bounds().size,
        gpui_kit::size(px(200.), px(80.))
    );
    assert_eq!(handle.content_size().height, px(80.));
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(40.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-40.), px(0.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        state.read_with(cx, |s, _| s.viewport().unwrap().offset()),
        40.
    );
    assert_eq!(handle.offset().x, px(-40.));
    cx.simulate_click(gpui_kit::point(px(16.), px(10.)), Default::default());
    assert_eq!(
        view.read_with(cx, |v, _| v.clicks.borrow().clone()),
        vec![1],
        "horizontal measurement projection must preserve physical child hitboxes"
    );
    state.update(cx, |s, cx| {
        s.scroll_to_key(
            99usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::End),
        )
        .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert!(snapshot.visible_range().contains(&99));
    assert!(snapshot.item(99).unwrap().is_measured());
    assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
    handle.set_offset(gpui_kit::point(px(-100.), px(0.)));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        state.read_with(cx, |s, _| s.viewport().unwrap().offset()),
        100.
    );
}

#[gpui_kit::test]
fn attached_measurement_respects_origin_padding_and_a_host_footer_on_both_axes(
    cx: &mut TestAppContext,
) {
    struct Empty;
    impl Render for Empty {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui_kit::canvas(|_, w, cx| {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let binding = VirtualizerBinding::new();
            let mut state = VirtualizerState::new_attached(axis, source(0, 10),
                VirtualizerSizing::measured(px(20.)), &binding).unwrap();
            state.set_layout(VirtualizerLayout::default().padding(px(10.), px(15.))).unwrap();
            for (revision, offset) in [(1, 0.), (2, 230.), (3, 350.)] {
                let v = VirtualizerViewport::new(&binding, revision, offset, px(100.), px(80.))
                    .with_content_origin(60.).with_scroll_extent(500.);
                let (snapshot, request) = state.prepare(&v, Instant::now()).unwrap();
                assert!(request.is_none());
                let batch = state.layout_measured(&snapshot,
                    Bounds::new(gpui_kit::point(px(0.), px(0.)),
                        if axis == Axis::Vertical { gpui_kit::size(px(80.), px(100.)) }
                        else { gpui_kit::size(px(100.), px(80.)) }),
                    |_, _, _| div().w(px(20.)).h(px(20.)).into_any_element(), w, cx).unwrap();
                assert_eq!(f64::from(batch.offset()), offset,
                    "list measurement must not move the host at {axis:?}, offset {offset}");
                let after = state.snapshot(&v).unwrap();
                assert_eq!(after.item(0).unwrap().start(), 70.);
                assert_eq!(after.item(1).unwrap().start(), 90.);
                assert_eq!(after.total_extent(), 225.);
                state.commit(&v).unwrap();
            }
        }
            }, |_, _, _, _| {}).size(px(100.))
        }
    }
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    cx.update(|w, cx| w.draw(cx).clear(cx));
}
