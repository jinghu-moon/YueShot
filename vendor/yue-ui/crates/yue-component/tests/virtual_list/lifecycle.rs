use super::*;
use std::cell::RefCell;

struct RetainedRow;
impl Render for RetainedRow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(20.))
    }
}
struct LifetimeHarness {
    state: Option<Entity<VirtualizerState>>,
    rows: Rc<RefCell<Vec<gpui_kit::WeakEntity<RetainedRow>>>>,
}
impl Render for LifetimeHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let Some(state) = &self.state else {
            return div().into_any_element();
        };
        let rows = self.rows.clone();
        VirtualList::new_with_item_views("lifetime-list", state, move |_, _, cx| {
            let row = cx.new(|_| RetainedRow);
            rows.borrow_mut().push(row.downgrade());
            row
        })
        .w(px(240.))
        .h(px(100.))
        .into_any_element()
    }
}
fn assert_released(rows: &[gpui_kit::WeakEntity<RetainedRow>]) {
    assert!(
        rows.iter().all(|row| row.upgrade().is_none()),
        "retained virtual rows after unmount"
    );
}

#[gpui_kit::test]
fn repeated_mount_scroll_replace_and_unmount_release_owners(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| LifetimeHarness {
        state: None,
        rows: Rc::default(),
    });
    for cycle in 0..64 {
        let weak = view.update(cx, |view, cx| {
            let state = cx.new(|_| fixed(1000));
            let weak = state.downgrade();
            view.state = Some(state);
            view.rows.borrow_mut().clear();
            cx.notify();
            weak
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            view.state.as_ref().unwrap().update(cx, |state, cx| {
                state
                    .scroll_to_key((100 + cycle).into(), VirtualizerScrollOptions::default())
                    .unwrap();
                state
                    .replace_source(
                        source(0, 1001),
                        VirtualizerSizing::fixed(px(20.)),
                        VirtualizerChange::Splice {
                            old: 1000..1000,
                            inserted: 1,
                        },
                    )
                    .unwrap();
                state
                    .native_scroll_handle()
                    .unwrap()
                    .set_offset(gpui_kit::point(px(0.), px(-200.)));
                cx.notify();
            });
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            view.state = None;
            cx.notify();
        });
        for _ in 0..3 {
            cx.update(|w, cx| w.draw(cx).clear(cx));
        }
        cx.run_until_parked();
        assert!(weak.upgrade().is_none(), "state retained on cycle {cycle}");
        view.read_with(cx, |view, _| assert_released(&view.rows.borrow()));
    }
}

#[gpui_kit::test]
fn release_gate_self_test_detects_an_intentionally_retained_owner(cx: &mut TestAppContext) {
    let retained = cx.new(|_| RetainedRow);
    let rows = [retained.downgrade()];
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| assert_released(&rows)));
    assert!(failure.is_err(), "the gate must detect a retained owner");
    drop(retained);
    cx.run_until_parked();
    assert_released(&rows);
}

enum LayoutOwner {
    Grid(
        Entity<VirtualizerState>,
        Entity<VirtualizerState>,
        ScrollHandle,
    ),
    Masonry(Entity<yue_component::VirtualMasonryState>),
}
struct LayoutLifetimeHarness {
    owner: Option<LayoutOwner>,
    rows: Rc<RefCell<Vec<gpui_kit::WeakEntity<RetainedRow>>>>,
}
impl Render for LayoutLifetimeHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        match &self.owner {
            None => div().into_any_element(),
            Some(LayoutOwner::Grid(a, b, scroll)) => {
                yue_component::VirtualGrid::new("layout-lifetime", a, b, scroll, move |_, _, cx| {
                    let row = cx.new(|_| RetainedRow);
                    rows.borrow_mut().push(row.downgrade());
                    row
                })
                .w(px(240.))
                .h(px(100.))
                .into_any_element()
            }
            Some(LayoutOwner::Masonry(state)) => {
                yue_component::VirtualMasonry::new("layout-lifetime", state, move |_, _, cx| {
                    let row = cx.new(|_| RetainedRow);
                    rows.borrow_mut().push(row.downgrade());
                    row
                })
                .w(px(240.))
                .h(px(100.))
                .into_any_element()
            }
        }
    }
}

#[gpui_kit::test]
fn grid_and_masonry_release_owners_and_child_entities_over_repeated_mounts(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, _| LayoutLifetimeHarness {
        owner: None,
        rows: Rc::default(),
    });
    for cycle in 0..64 {
        let (axes, masonry) = view.update(cx, |view, cx| {
            view.rows.borrow_mut().clear();
            let (owner, axes, masonry) = if cycle % 2 == 0 {
                let binding = VirtualizerBinding::new();
                let a = cx.new(|_| {
                    VirtualizerState::new_attached(
                        Axis::Vertical,
                        source(0, 1000),
                        VirtualizerSizing::fixed(px(20.)),
                        &binding,
                    )
                    .unwrap()
                });
                let b = cx.new(|_| {
                    VirtualizerState::new_attached(
                        Axis::Horizontal,
                        source(0, 1000),
                        VirtualizerSizing::fixed(px(20.)),
                        &binding,
                    )
                    .unwrap()
                });
                let weak = vec![a.downgrade(), b.downgrade()];
                (LayoutOwner::Grid(a, b, ScrollHandle::new()), weak, None)
            } else {
                let state = cx.new(|_| {
                    let mut s = yue_component::VirtualMasonryState::new(
                        source(0, 1000),
                        VirtualizerSizing::measured(px(80.)),
                        3,
                    )
                    .unwrap();
                    s.set_layout(VirtualizerLayout::default().measurement_budget(1), px(0.))
                        .unwrap();
                    s
                });
                let weak = state.downgrade();
                (LayoutOwner::Masonry(state), vec![], Some(weak))
            };
            view.owner = Some(owner);
            cx.notify();
            (axes, masonry)
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        view.update(cx, |view, cx| match view.owner.as_ref().unwrap() {
            LayoutOwner::Grid(a, b, _) => {
                for state in [a, b] {
                    state.update(cx, |s, cx| {
                        s.replace_source(
                            source(0, 1001),
                            VirtualizerSizing::fixed(px(20.)),
                            VirtualizerChange::Splice {
                                old: 1000..1000,
                                inserted: 1,
                            },
                        )
                        .unwrap();
                        s.scroll_to_key(500usize.into(), VirtualizerScrollOptions::default())
                            .unwrap();
                        cx.notify();
                    });
                }
            }
            LayoutOwner::Masonry(state) => state.update(cx, |s, cx| {
                s.append_source(source(0, 1001), VirtualizerSizing::measured(px(80.)))
                    .unwrap();
                s.scroll_to_key(500usize.into(), VirtualizerAlign::Start)
                    .unwrap();
                cx.notify();
            }),
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            view.owner = None;
            cx.notify();
        });
        for _ in 0..3 {
            cx.update(|w, cx| {
                w.simulate_next_frame(cx);
                w.draw(cx).clear(cx);
            });
        }
        cx.run_until_parked();
        assert!(
            axes.iter().all(|weak| weak.upgrade().is_none()),
            "Grid retained after cycle {cycle}"
        );
        assert!(
            masonry.is_none_or(|weak| weak.upgrade().is_none()),
            "Masonry retained after cycle {cycle}"
        );
        view.read_with(cx, |view, _| assert_released(&view.rows.borrow()));
    }
}
