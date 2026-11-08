use gpui::{
    Context, IntoElement, ListAlignment, ListState, Pixels, Render, Styled, TestAppContext, Window,
    div, list, px, uniform_list_range,
};

#[test]
fn sparse_table_metadata_reaches_native_accesskit_without_synthetic_cells() {
    use gpui::{Element as _, InteractiveElement as _, Role, StatefulInteractiveElement as _};
    let mut node = gpui::accesskit::Node::new(Role::Cell);
    let ordinary = div().id("ordinary").aria_row_index(0).aria_column_index(9);
    ordinary.write_a11y_info(&mut node);
    assert_eq!((node.row_index(), node.column_index()), (Some(0), Some(9)));
    assert_eq!((node.row_span(), node.column_span()), (None, None));
    let span = div()
        .id("merged")
        .aria_row_index(998)
        .aria_column_index(12)
        .aria_row_span(3)
        .aria_column_span(2);
    let mut merged = gpui::accesskit::Node::new(Role::Cell);
    span.write_a11y_info(&mut merged);
    assert_eq!(
        (merged.row_index(), merged.column_index()),
        (Some(998), Some(12))
    );
    assert_eq!(
        (merged.row_span(), merged.column_span()),
        (Some(3), Some(2))
    );
    let grid = div()
        .id("grid")
        .aria_row_count(1_000_000)
        .aria_column_count(1_000);
    let mut root = gpui::accesskit::Node::new(Role::Grid);
    grid.write_a11y_info(&mut root);
    assert_eq!(
        (root.row_count(), root.column_count()),
        (Some(1_000_000), Some(1_000))
    );
    assert!(
        root.children().is_empty(),
        "logical counts never synthesize a million nodes"
    );
}

#[gpui::test]
fn native_overdraw_edges_bound_layout_and_legacy_setter_restores_symmetry(cx: &mut TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    struct Probe {
        list: ListState,
        seen: Rc<RefCell<Vec<usize>>>,
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let seen = self.seen.clone();
            list(self.list.clone(), move |ix, _, _| {
                seen.borrow_mut().push(ix);
                div().h(px(20.)).into_any_element()
            })
            .w(px(240.))
            .h(px(100.))
        }
    }
    let state = ListState::new_with_uniform_height(1000, ListAlignment::Top, px(200.), px(20.));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        let seen = seen.clone();
        move |_, _| Probe { list: state, seen }
    });
    state.scroll_to(gpui::ListOffset {
        item_ix: 50,
        offset_in_item: px(0.),
    });
    state.set_overdraw_edges(px(40.), px(200.));
    seen.borrow_mut().clear();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert!(
        seen.borrow().iter().all(|i| (48..65).contains(i)),
        "actual roots: {:?}",
        seen.borrow()
    );
    assert!((50..55).all(|ix| seen.borrow().contains(&ix)));
    state.set_overdraw(px(200.));
    seen.borrow_mut().clear();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert!(seen.borrow().iter().any(|i| *i == 40));
    assert!((50..55).all(|ix| seen.borrow().contains(&ix)));
}

#[gpui::test]
fn sparse_focus_handles_follow_live_and_frozen_geometry_lifetimes(cx: &mut TestAppContext) {
    let state = ListState::new_with_uniform_height(1000, ListAlignment::Top, px(0.), px(20.));
    let weak = cx.update(|cx| {
        let focus = cx.focus_handle();
        let weak = focus.downgrade();
        state.set_item_focus_handle(900, Some(focus));
        weak
    });
    state.set_item_sizes(&[(900, gpui::size(px(240.), px(40.)))], true);
    let frozen = state.geometry();
    state.remeasure_items(899..902);
    state.splice(0..1, 0);
    state.set_item_sizes(&[(899, gpui::size(px(480.), px(30.)))], true);
    assert!(weak.upgrade().is_some());
    state.set_item_focus_handle(899, None);
    drop(state);
    assert!(
        weak.upgrade().is_some(),
        "frozen geometry keeps its focus handle alive"
    );
    drop(frozen);
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "no leaked focus allocation after final owner release"
    );

    let state = ListState::new(0, ListAlignment::Top, px(0.));
    let weak = cx.update(|cx| {
        let focus = cx.focus_handle();
        let weak = focus.downgrade();
        state.splice_focusable(0..0, [Some(focus)]);
        weak
    });
    assert!(weak.upgrade().is_some());
    state.splice(0..1, 0);
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "focusable splice uses the same lifetime contract"
    );
}

#[test]
fn size_batches_preserve_order_last_duplicate_hints_and_frozen_geometry() {
    let state = ListState::new_with_uniform_height(1025, ListAlignment::Top, px(0.), px(20.));
    let original = state.geometry();
    state.set_item_sizes(&[], true);
    assert_eq!(state.geometry().revision(), original.revision());
    for measured in [true, false] {
        let before = state.geometry();
        state.set_item_sizes(
            &[
                (1024, gpui::size(px(300.), px(31.))),
                (256, gpui::size(px(300.), px(32.))),
                (0, gpui::size(px(300.), px(33.))),
                (256, gpui::size(px(320.), px(34.))),
                (255, gpui::size(px(300.), px(35.))),
                (1024, gpui::size(px(340.), px(36.))),
            ],
            measured,
        );
        let next = state.geometry();
        assert_eq!(next.revision(), before.revision() + 1);
        assert_eq!(next.measured_count(), if measured { 4 } else { 0 });
        for (ix, width, height) in [
            (0, 300., 33.),
            (255, 300., 35.),
            (256, 320., 34.),
            (1024, 340., 36.),
        ] {
            let item = next.item(ix).unwrap();
            assert_eq!(item.extent(), px(height));
            assert_eq!(item.cross_extent(), px(width));
            assert_eq!(item.is_measured(), measured);
            assert_eq!(original.item(ix).unwrap().extent(), px(20.));
            assert!(!original.item(ix).unwrap().is_measured());
        }
        assert_eq!(next.item(257).unwrap().extent(), px(20.));
        assert_eq!(before.measured_count(), if measured { 0 } else { 4 });
    }
}

#[test]
fn invalid_size_batches_leave_geometry_and_revision_unchanged() {
    let state = ListState::new_with_uniform_height(1025, ListAlignment::Top, px(0.), px(20.));
    let frozen = state.geometry();
    for (index, height) in [(1025, 30.), (1, 0.), (1, f32::NAN), (1, f32::INFINITY)] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            state.set_item_sizes(
                &[
                    (0, gpui::size(px(300.), px(31.))),
                    (index, gpui::size(px(300.), px(height))),
                ],
                true,
            );
        }));
        assert!(result.is_err());
        let current = state.geometry();
        assert_eq!(current.revision(), frozen.revision());
        assert_eq!(current.measured_count(), 0);
        assert_eq!(current.item(0).unwrap().extent(), px(20.));
    }
}

#[test]
fn mixed_size_batches_match_input_order_and_precise_prefixes_across_tree_nodes() {
    let state = ListState::new_with_uniform_height(5000, ListAlignment::Top, px(0.), px(20.));
    let original = state.geometry();
    let mut expected = vec![(px(0.), px(20.), false); 5000];
    let mut seed = 7u64;
    for batch in 0..6 {
        let measured = batch % 2 == 0;
        let mut updates = Vec::new();
        for at in 0..512 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let index = (seed >> 32) as usize % expected.len();
            updates.push((
                index,
                gpui::size(px(200. + batch as f32), px(10. + (at % 29) as f32)),
            ));
        }
        if batch % 3 == 0 {
            updates.sort_by_key(|(index, _)| *index);
        } else if batch % 3 == 1 {
            updates.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
        }
        for (index, size) in &updates {
            expected[*index] = (size.width, size.height, measured);
        }
        state.set_item_sizes(&updates, measured);
        let current = state.geometry();
        let mut start = 0.;
        for (index, (width, height, valid)) in expected.iter().copied().enumerate() {
            let item = current.item(index).unwrap();
            assert_eq!(item.precise_start(), start);
            assert_eq!(item.extent(), height);
            assert_eq!(item.cross_extent(), width);
            assert_eq!(item.is_measured(), valid);
            start += f64::from(height);
            assert_eq!(
                original.item(index).unwrap().precise_start(),
                index as f64 * 20.
            );
            assert_eq!(original.item(index).unwrap().extent(), px(20.));
        }
        assert_eq!(current.revision(), original.revision() + batch + 1);
        assert_eq!(
            current.measured_count(),
            expected.iter().filter(|(_, _, valid)| *valid).count()
        );
    }
}

#[gpui::test]
fn size_batches_keep_focus_alive_until_live_and_frozen_owners_release_it(cx: &mut TestAppContext) {
    let state = ListState::new_with_uniform_height(1025, ListAlignment::Top, px(0.), px(20.));
    let weak = cx.update(|cx| {
        let focus = cx.focus_handle();
        let weak = focus.downgrade();
        state.set_item_focus_handle(256, Some(focus));
        weak
    });
    let updates = [
        (1024, gpui::size(px(300.), px(31.))),
        (256, gpui::size(px(300.), px(32.))),
        (0, gpui::size(px(300.), px(33.))),
        (256, gpui::size(px(320.), px(34.))),
    ];
    state.set_item_sizes(&updates, true);
    let frozen = state.geometry();
    state.set_item_sizes(&updates, false);
    state.set_item_focus_handle(256, None);
    drop(state);
    assert!(weak.upgrade().is_some());
    drop(frozen);
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
}

#[gpui::test]
fn width_invalidation_keeps_sparse_hints_and_frozen_geometry(cx: &mut TestAppContext) {
    for dense in [false, true] {
        let state = ListState::new_with_uniform_height(1000, ListAlignment::Top, px(0.), px(20.));
        let (view, cx) = cx.add_window_view(|_, _| View {
            list: state.clone(),
            width: px(240.),
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let updates: Vec<_> = (0..1000)
            .filter(|ix| dense || *ix == 900 || *ix == 999)
            .map(|ix| (ix, gpui::size(px(240.), px(37.))))
            .collect();
        state.set_item_sizes(&updates, true);
        let frozen = state.geometry();
        view.update(cx, |v, cx| {
            v.width = px(480.);
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let next = state.geometry();
        for ix in [900, 999] {
            assert!(!next.item(ix).unwrap().is_measured());
            assert_eq!(next.item(ix).unwrap().extent(), px(37.));
            assert!(frozen.item(ix).unwrap().is_measured());
            assert_eq!(frozen.item(ix).unwrap().cross_extent(), px(240.));
        }
        assert!(
            next.measured_items()
                .iter()
                .all(|(_, extent)| extent.width == px(480.))
        );
    }
}

#[test]
fn scroll_handle_preserves_exact_positions_and_explicit_jumps_reset_fractional_input() {
    let scroll = gpui::ScrollHandle::new();
    scroll.set_precise_offset(gpui::point(-16_777_216.25, -16_777_216.75));
    assert_eq!(
        scroll.precise_offset(),
        gpui::point(-16_777_216.25, -16_777_216.75)
    );
    let projection = scroll.offset();
    scroll.set_offset(projection);
    assert_eq!(scroll.precise_offset(), projection.map(f64::from));
    scroll.set_precise_offset(gpui::point(-0.25, -0.5));
    assert_eq!(scroll.offset(), gpui::point(px(-0.25), px(-0.5)));
    assert_eq!(scroll.precise_offset(), gpui::point(-0.25, -0.5));
}

#[test]
fn frozen_geometry_retains_actual_cross_constraint_when_equal_extents_are_remeasured() {
    let state = ListState::new_with_uniform_height(2, ListAlignment::Top, px(0.), px(20.));
    state.set_item_sizes(&[(0, gpui::size(px(240.), px(60.)))], true);
    let old = state.geometry();
    state.set_item_sizes(&[(0, gpui::size(px(480.), px(60.)))], true);
    assert_eq!(old.item(0).unwrap().cross_extent(), px(240.));
    assert_eq!(state.geometry().item(0).unwrap().cross_extent(), px(480.));
    state.remeasure_items(0..1);
    assert!(!state.geometry().item(0).unwrap().is_measured());
    assert!(old.item(0).unwrap().is_measured());
}

#[test]
fn uniform_geometry_is_shared_and_half_open_even_inside_gaps() {
    assert_eq!(uniform_list_range(100, 20., 0., 0., 60.), 0..3);
    assert_eq!(uniform_list_range(100, 20., 10., 21., 5.), 1..1);
    assert_eq!(uniform_list_range(100, 20., 10., 29., 10.), 1..2);
    assert_eq!(uniform_list_range(0, 20., 0., 0., 100.), 0..0);
    assert_eq!(
        uniform_list_range(1_000_000_000, 20., 0., 19_999_999_900., 100.),
        999_999_995..1_000_000_000
    );
}

#[test]
fn direct_estimated_construction_has_the_same_geometry_without_a_replacement_tree() {
    let direct = ListState::new_with_uniform_height(100_000, ListAlignment::Top, px(20.), px(20.));
    let previous =
        ListState::new(100_000, ListAlignment::Top, px(20.)).with_uniform_item_height(px(20.));
    let a = direct.geometry();
    let b = previous.geometry();
    assert_eq!(a.total_extent(), b.total_extent());
    assert_eq!(a.measured_count(), b.measured_count());
    assert!(a.is_estimated());
    assert_eq!(a.item(99_999).unwrap().start(), px(1_999_980.));
    direct.splice(100_000..100_000, 1);
    assert_eq!(direct.geometry().total_extent(), px(2_000_020.));
}

#[test]
fn per_item_hint_splice_preserves_existing_measurements_and_frozen_geometry() {
    let state = ListState::new_with_uniform_height(10, ListAlignment::Top, px(0.), px(20.));
    state.set_item_sizes(&[(3, gpui::size(px(100.), px(50.)))], true);
    let frozen = state.geometry();
    state.splice_with_size_hints(8..9, [px(30.), px(40.)]);
    let next = state.geometry();
    assert_eq!(frozen.len(), 10);
    assert_eq!(frozen.total_extent(), px(230.));
    assert_eq!(next.len(), 11);
    assert_eq!(next.total_extent(), px(280.));
    assert_eq!(next.item(3).unwrap().extent(), px(50.));
    assert!(next.item(3).unwrap().is_measured());
    assert_eq!(next.item(8).unwrap().extent(), px(30.));
    assert_eq!(next.item(9).unwrap().extent(), px(40.));
    assert!(!next.item(9).unwrap().is_measured());
    assert_eq!(next.item(10).unwrap().start(), px(260.));
    let invalid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.splice_with_size_hints(5..5, [px(40.), px(f32::NAN)]);
    }));
    assert!(invalid.is_err());
    assert_eq!(state.geometry().total_extent(), next.total_extent());
    assert_eq!(state.geometry().len(), next.len());
    state.splice_focusable(0..0, [None]);
    assert_eq!(state.geometry().item(4).unwrap().extent(), px(50.));
}

struct View {
    list: ListState,
    width: Pixels,
}
impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        list(self.list.clone(), |_, _, _| {
            div().h(px(20.)).into_any_element()
        })
        .w(self.width)
        .h(px(60.))
    }
}

#[gpui::test]
fn native_hints_survive_first_layout_resize_splice_and_snapshot_reads(cx: &mut TestAppContext) {
    let list = ListState::new(1_000, ListAlignment::Top, px(20.)).with_uniform_item_height(px(20.));
    let (view, cx) = cx.add_window_view({
        let list = list.clone();
        move |_, _| View {
            list,
            width: px(100.),
        }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let before = list.geometry();
    assert_eq!(before.total_extent(), px(20_000.));
    assert!(before.is_estimated());
    assert!(before.measured_count() < 10);
    assert_eq!(before.range(px(0.), px(60.)), 0..3);
    list.splice(1_000..1_000, 5);
    assert_eq!(list.geometry().total_extent(), px(20_100.));
    assert_eq!(before.total_extent(), px(20_000.));
    view.update(cx, |v, cx| {
        v.width = px(200.);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(list.geometry().total_extent(), px(20_100.));
    assert!(list.geometry().measured_count() < 10);
}

#[gpui::test]
fn invalid_zero_height_cannot_pull_all_items_into_one_frame(cx: &mut TestAppContext) {
    struct InvalidView(ListState);
    impl Render for InvalidView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| div().h(px(0.)).into_any_element())
                .w(px(100.))
                .h(px(100_000.))
        }
    }
    let list =
        ListState::new(100_000, ListAlignment::Top, px(0.)).with_uniform_item_height(px(20.));
    list.set_layout_budget(64);
    let (_, cx) = cx.add_window_view({
        let list = list.clone();
        move |_, _| InvalidView(list)
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(list.last_layout_count() <= 64);
    assert!(list.invalid_size_count() > 0);
    assert!(list.geometry().total_extent() > px(1_000_000.));
    assert!(list.geometry().is_estimated());
    assert_eq!(list.geometry().measured_count(), 0);
    assert!(!list.geometry().item(0).unwrap().is_measured());
}

#[gpui::test]
fn horizontal_viewport_uses_physical_left_right_padding(cx: &mut TestAppContext) {
    struct Probe;
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui::canvas(
                |_, w, cx| {
                    let list =
                        ListState::new_with_uniform_height(10, ListAlignment::Top, px(0.), px(20.));
                    let batch = list.layout_for_viewport_on_axis(
                        gpui::Axis::Horizontal,
                        gpui::Bounds::new(
                            gpui::point(px(0.), px(0.)),
                            gpui::size(px(100.), px(80.)),
                        ),
                        px(135.),
                        gpui::Edges {
                            top: px(0.),
                            bottom: px(0.),
                            left: px(10.),
                            right: px(25.),
                        },
                        |_, _, _| div().w(px(20.)).h(px(80.)).into_any_element(),
                        w,
                        cx,
                    );
                    assert_eq!(batch.offset(), px(135.));
                    assert_eq!(batch.geometry().total_extent(), px(200.));
                },
                |_, _, _, _| {},
            )
            .size(px(100.))
        }
    }
    let (_, cx) = cx.add_window_view(|_, _| Probe);
    cx.update(|w, cx| w.draw(cx).clear(cx));
}

#[gpui::test]
fn measured_native_large_prefix_keeps_input_bounds_and_scrollbar_end(cx: &mut TestAppContext) {
    struct PreciseView(ListState);
    impl Render for PreciseView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(20.)).into_any_element()
            })
            .w(px(100.))
            .h(px(59.))
        }
    }
    let state = ListState::new_with_uniform_height(1000, ListAlignment::Top, px(0.), px(20.));
    state.splice_with_size_hints(0..1, [px(16_777_216.)]);
    let frozen = state.geometry();
    assert_eq!(
        frozen.item(600).unwrap().precise_start(),
        16_777_216. + 599. * 20.
    );
    assert_eq!(frozen.precise_range(16_777_216.25, 19.75), 1..2);
    state.scroll_to(gpui::ListOffset {
        item_ix: 600,
        offset_in_item: px(0.),
    });
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| PreciseView(state)
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let start = -state.precise_scroll_offset().y;
    for step in 1..=4 {
        state.scroll_by(px(0.25));
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        assert_eq!(
            -state.precise_scroll_offset().y,
            start + f64::from(step) * 0.25
        );
        assert_eq!(
            state.bounds_for_item(601).unwrap().top(),
            px(20. - step as f32 * 0.25)
        );
    }
    state.scrollbar_drag_started();
    let projected_max = state.max_offset_for_scrollbar().y;
    state.set_offset_from_scrollbar(gpui::point(px(0.), -projected_max));
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert_eq!(
        -state.precise_scroll_offset().y,
        state.geometry().precise_total_extent() - 59.
    );
    assert_eq!(state.is_scrolled_to_end(), Some(true));
    state.scrollbar_drag_ended();
    state.set_offset_from_scrollbar(gpui::point(px(0.), px(0.)));
    assert_eq!(state.precise_scroll_offset().y, 0.);
    assert_eq!(frozen.precise_total_extent(), 16_777_216. + 999. * 20.);
}
