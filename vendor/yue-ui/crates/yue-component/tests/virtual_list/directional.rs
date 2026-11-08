use super::*;
use gpui_kit::base::ScrollbarHandle as _;
use yue_component::{VirtualGridSnapshot, VirtualMasonryState};

#[test]
fn directional_ranges_keep_forward_margin_reverse_reserve_and_idle_symmetry() {
    for mode in 0..3 {
        let mut state = VirtualizerState::new(
            Axis::Vertical,
            source(0, 1_000),
            match mode {
                0 => VirtualizerSizing::fixed(px(20.)),
                1 => VirtualizerSizing::known(vec![px(20.); 1_000]),
                _ => VirtualizerSizing::measured(px(20.)),
            },
        )
        .unwrap();
        state
            .set_layout(
                VirtualizerLayout::default()
                    .overscan(px(200.))
                    .directional_overscan(px(40.)),
            )
            .unwrap();
        let now = Instant::now();
        state.prepare(&viewport(&state, 1, 0.), now).unwrap();
        let (forward, _) = state
            .prepare(&viewport(&state, 2, 1_000.), now + Duration::from_millis(1))
            .unwrap();
        assert_eq!(forward.rendered_range(), 48..65);
        state.set_retained_keys(vec![800usize.into()]).unwrap();
        assert_eq!(
            state
                .snapshot(&viewport(&state, 2, 1_000.))
                .unwrap()
                .working_items()
                .count(),
            18
        );
        let (backward, _) = state
            .prepare(&viewport(&state, 3, 980.), now + Duration::from_millis(2))
            .unwrap();
        assert_eq!(backward.rendered_range(), 39..56);
        state.poll_scroll_idle(now + Duration::from_millis(122));
        assert_eq!(
            state
                .snapshot(&viewport(&state, 3, 980.))
                .unwrap()
                .rendered_range(),
            39..64
        );
        assert!(
            state
                .set_layout(
                    VirtualizerLayout::default()
                        .overscan(px(20.))
                        .directional_overscan(px(40.))
                )
                .is_err()
        );
        assert_eq!(
            state
                .snapshot(&viewport(&state, 3, 980.))
                .unwrap()
                .rendered_range(),
            39..64
        );
    }
}

#[test]
fn directional_grid_axes_and_masonry_reduce_actual_worksets() {
    let binding = VirtualizerBinding::new();
    let mut rows = VirtualizerState::new_attached(
        Axis::Vertical,
        source(0, 1_000),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let mut columns = VirtualizerState::new_attached(
        Axis::Horizontal,
        source(0, 1_000),
        VirtualizerSizing::fixed(px(50.)),
        &binding,
    )
    .unwrap();
    let layout = VirtualizerLayout::default()
        .overscan(px(200.))
        .directional_overscan(px(40.));
    rows.set_layout(layout).unwrap();
    columns.set_layout(layout).unwrap();
    let now = Instant::now();
    for (state, main, cross) in [(&mut rows, 100., 240.), (&mut columns, 240., 100.)] {
        state
            .prepare(
                &VirtualizerViewport::new(&binding, 1, 0., px(main), px(cross)),
                now,
            )
            .unwrap();
        state
            .prepare(
                &VirtualizerViewport::new(&binding, 2, 1_000., px(main), px(cross)),
                now + Duration::from_millis(1),
            )
            .unwrap();
    }
    let grid = VirtualGridSnapshot::new(
        rows.snapshot(&VirtualizerViewport::new(
            &binding,
            2,
            1_000.,
            px(100.),
            px(240.),
        ))
        .unwrap(),
        columns
            .snapshot(&VirtualizerViewport::new(
                &binding,
                2,
                1_000.,
                px(240.),
                px(100.),
            ))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(grid.cells().count(), 170);
    let mut masonry =
        VirtualMasonryState::new(source(0, 1_000), VirtualizerSizing::measured(px(20.)), 3)
            .unwrap();
    masonry.set_layout(layout, px(0.)).unwrap();
    masonry.observe_scroll_activity(now);
    masonry
        .scroll_handle()
        .set_precise_offset(gpui_kit::point(0., -1_000.));
    masonry.observe_scroll_activity(now + Duration::from_millis(1));
    assert_eq!(
        masonry
            .snapshot(1_000., gpui_kit::size(px(240.), px(100.)))
            .unwrap()
            .working_indices()
            .len(),
        51
    );
    masonry.poll_scroll_idle(now + Duration::from_millis(121));
    assert_eq!(
        masonry
            .snapshot(1_000., gpui_kit::size(px(240.), px(100.)))
            .unwrap()
            .working_indices()
            .len(),
        75
    );
}

struct MeasuredHarness {
    state: Entity<VirtualizerState>,
    seen: Rc<std::cell::RefCell<std::collections::HashSet<usize>>>,
}
impl Render for MeasuredHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seen = self.seen.clone();
        VirtualList::new("directional-measured", &self.state, move |item, _, _| {
            seen.borrow_mut().insert(item.index());
            div().h(px(20.))
        })
        .w(px(240.))
        .h(px(100.))
    }
}
#[gpui_kit::test]
fn directional_measured_native_layout_limits_intrinsic_roots(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| MeasuredHarness {
        state: cx.new(|_| {
            let mut s = VirtualizerState::new(
                Axis::Vertical,
                source(0, 1_000),
                VirtualizerSizing::measured(px(20.)),
            )
            .unwrap();
            s.set_layout(
                VirtualizerLayout::default()
                    .overscan(px(200.))
                    .directional_overscan(px(40.)),
            )
            .unwrap();
            s
        }),
        seen: Rc::default(),
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (state, seen) = view.read_with(cx, |v, _| (v.state.clone(), v.seen.clone()));
    seen.borrow_mut().clear();
    state.update(cx, |s, cx| {
        s.scrollbar_handle()
            .unwrap()
            .set_offset(gpui_kit::point(px(0.), px(-1_000.)));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        seen.borrow().len() <= 20,
        "must shrink native roots, not only public range: {:?}",
        seen.borrow()
    );
    assert!(
        (50..55).all(|ix| seen.borrow().contains(&ix)),
        "all visible rows remain present"
    );
}
