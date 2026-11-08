use super::*;

struct Probe {
    state: Entity<VirtualizerState>,
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        VirtualList::new("owned-reflow", &self.state, |_, _, _| div().h(px(32.)))
            .item_role(Some(Role::ListItem))
            .test_support()
    }
}
fn draw(cx: &mut gpui_kit::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
#[gpui_kit::test]
fn owned_measured_remap_distinguishes_geometry_from_input(cx: &mut TestAppContext) {
    let mut probe = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Probe {
            state: cx.new(|_| {
                VirtualizerState::new(
                    Axis::Vertical,
                    source(100, 1_000),
                    VirtualizerSizing::measured(px(32.)),
                )
                .unwrap()
            }),
        });
        probe = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    draw(cx);
    let state = probe.unwrap().read_with(cx, |v, _| v.state.clone());
    state.update(cx, |s, cx| {
        s.scroll_to_key(700_usize.into(), VirtualizerAlign::Start.into())
            .unwrap();
        cx.notify();
    });
    draw(cx);
    state.update(cx, |s, cx| {
        s.scroll_by(7., Default::default()).unwrap();
        cx.notify();
    });
    draw(cx);
    state.update(cx, |s, cx| {
        s.replace_source(
            source(0, 1_100),
            VirtualizerSizing::measured(px(48.)),
            VirtualizerChange::Replace {
                preserve_anchor: true,
            },
        )
        .unwrap();
        s.set_layout(VirtualizerLayout::default().revision(1))
            .unwrap();
        cx.notify();
    });
    draw(cx);
    let (index, relative) = state.read_with(cx, |s, _| {
        let v = s.viewport().unwrap();
        let item = s
            .snapshot(v)
            .unwrap()
            .item_for_key(&700_usize.into())
            .unwrap();
        (item.index(), v.offset() - item.start())
    });
    assert_eq!(index, 700);
    assert!(
        (relative - 7.).abs() < 0.5,
        "reading key must survive native prefix remapping: {relative}"
    );
    let before = state.read_with(cx, |s, _| s.viewport().unwrap().offset());
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(100.), px(100.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-19.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    draw(cx);
    let after = state.read_with(cx, |s, _| s.viewport().unwrap().offset());
    assert!(
        (after - before - 19.).abs() < 0.5,
        "real wheel input must still override the anchor"
    );
}
