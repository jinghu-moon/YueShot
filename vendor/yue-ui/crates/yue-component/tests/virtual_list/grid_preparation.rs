use super::*;
use std::cell::RefCell;
use yue_component::{VirtualGrid, VirtualGridCellIdentity, VirtualizerSizeUpdate};

struct CellView;
impl Render for CellView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id("prepared-cell").role(Role::Cell)
    }
}
struct Harness {
    rows: Entity<VirtualizerState>,
    columns: Entity<VirtualizerState>,
    scroll: ScrollHandle,
    prepared: Rc<Cell<usize>>,
    weak: Rc<RefCell<Vec<gpui_kit::WeakEntity<CellView>>>>,
    revision: u64,
    mounted: bool,
    frozen: (usize, usize),
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if !self.mounted {
            return div().into_any_element();
        }
        let prepared = self.prepared.clone();
        let weak = self.weak.clone();
        VirtualGrid::new_with_cell_views(
            "prepared-grid",
            &self.rows,
            &self.columns,
            &self.scroll,
            move |identity: VirtualGridCellIdentity, _, cx| {
                assert!(identity.row().index() < 1_000);
                assert!(identity.column().index() < 1_000);
                prepared.set(prepared.get() + 1);
                let view = cx.new(|_| CellView);
                weak.borrow_mut().push(view.downgrade());
                view
            },
        )
        .content_revision(self.revision)
        .frozen_rows(self.frozen.0)
        .frozen_columns(self.frozen.1)
        .w(px(240.))
        .h(px(100.))
        .into_any_element()
    }
}
fn harness(cx: &mut Context<Harness>) -> Harness {
    let binding = VirtualizerBinding::new();
    Harness {
        rows: cx.new(|_| {
            VirtualizerState::new_attached(
                Axis::Vertical,
                source(0, 1_000),
                VirtualizerSizing::known(vec![px(20.); 1_000]),
                &binding,
            )
            .unwrap()
        }),
        columns: cx.new(|_| {
            VirtualizerState::new_attached(
                Axis::Horizontal,
                source(0, 1_000),
                VirtualizerSizing::fixed(px(50.)),
                &binding,
            )
            .unwrap()
        }),
        scroll: ScrollHandle::new(),
        prepared: Rc::default(),
        weak: Rc::default(),
        revision: 0,
        mounted: true,
        frozen: (0, 0),
    }
}

#[gpui_kit::test]
fn grid_prepared_corner_keeps_identity_across_freezing_and_prunes_after_unfreeze(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (weak, prepared) = view.read_with(cx, |v, _| (v.weak.clone(), v.prepared.clone()));
    let corner = weak.borrow()[0].clone();
    let initial = prepared.get();
    view.update(cx, |v, cx| {
        v.frozen = (1, 1);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(prepared.get(), initial, "freezing is geometry, not content");
    view.update(cx, |v, cx| {
        v.scroll
            .set_precise_offset(gpui_kit::point(-1_000., -2_000.));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        corner.upgrade().is_some(),
        "corner keeps the original prepared entity"
    );
    view.update(cx, |v, cx| {
        v.frozen = (0, 0);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        corner.upgrade().is_none(),
        "offscreen unfrozen cell leaves the cache"
    );
}

#[gpui_kit::test]
fn grid_content_preparation_reuses_geometry_and_invalidates_both_axis_content(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (rows, columns, prepared) = view.read_with(cx, |v, _| {
        (v.rows.clone(), v.columns.clone(), v.prepared.clone())
    });
    assert_eq!(prepared.get(), 25);
    for _ in 0..10 {
        view.update(cx, |_, cx| cx.notify());
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert_eq!(prepared.get(), 25);
    rows.update(cx, |s, cx| {
        s.invalidate_items(0..1).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(prepared.get(), 30);
    columns.update(cx, |s, cx| {
        s.invalidate_items(0..1).unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(prepared.get(), 35);
    rows.update(cx, |s, cx| {
        s.update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(30.))])
            .unwrap();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(prepared.get(), 35, "geometry is not content identity");
    view.update(cx, |v, cx| {
        v.revision += 1;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(prepared.get(), 60);
}

#[gpui_kit::test]
fn grid_content_preparation_prunes_workset_and_releases_on_unmount(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (weak, prepared, scroll) = view.read_with(cx, |v, _| {
        (v.weak.clone(), v.prepared.clone(), v.scroll.clone())
    });
    assert_eq!(prepared.get(), 25);
    for _ in 0..3 {
        view.update(cx, |_, cx| {
            scroll.set_precise_offset(gpui_kit::point(-1_000., -2_000.));
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert_eq!(prepared.get(), 50);
    assert!(weak.borrow()[..25].iter().all(|w| w.upgrade().is_none()));
    for _ in 0..3 {
        view.update(cx, |v, cx| {
            v.mounted = false;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    assert!(weak.borrow().iter().all(|w| w.upgrade().is_none()));
}

#[gpui_kit::test]
fn grid_content_preparation_resets_when_axis_owner_changes(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let (rows, columns, prepared) = view.read_with(cx, |v, _| {
        (v.rows.clone(), v.columns.clone(), v.prepared.clone())
    });
    rows.update(cx, |s, _| s.detach());
    let binding = columns.read_with(cx, |s, _| s.binding().unwrap().clone());
    let replacement = cx.update(|_, cx| {
        cx.new(|_| {
            VirtualizerState::new_attached(
                Axis::Vertical,
                source(0, 1_000),
                VirtualizerSizing::fixed(px(20.)),
                &binding,
            )
            .unwrap()
        })
    });
    view.update(cx, |v, cx| {
        v.rows = replacement;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        prepared.get(),
        50,
        "same keys from another owner are new content"
    );
}
