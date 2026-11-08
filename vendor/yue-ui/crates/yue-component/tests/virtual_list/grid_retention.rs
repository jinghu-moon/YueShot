use super::*;
use gpui_kit::FocusHandle;
use std::{cell::RefCell, collections::HashMap};
use yue_component::{VirtualGrid, VirtualGridCellKey};

struct Editor {
    focus: FocusHandle,
    edits: usize,
}
impl Render for Editor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id("editor").track_focus(&self.focus).role(Role::Cell)
    }
}
struct Harness {
    axes: [Entity<VirtualizerState>; 2],
    scroll: ScrollHandle,
    retained: Vec<VirtualGridCellKey>,
    focus: FocusHandle,
    fallback: FocusHandle,
    register: bool,
    frozen: usize,
    mounted: bool,
    revision: u64,
    views: Rc<RefCell<HashMap<VirtualGridCellKey, gpui_kit::WeakEntity<Editor>>>>,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if !self.mounted {
            return div().into_any_element();
        }
        let views = self.views.clone();
        let focus = self.focus.clone();
        let mut grid = VirtualGrid::new_with_cell_views(
            "retained-grid",
            &self.axes[0],
            &self.axes[1],
            &self.scroll,
            move |cell, _, cx| {
                let key =
                    VirtualGridCellKey::new(cell.row().key().clone(), cell.column().key().clone());
                let editor = cx.new(|cx| Editor {
                    focus: if key == VirtualGridCellKey::new(0usize, 0usize) {
                        focus.clone()
                    } else {
                        cx.focus_handle()
                    },
                    edits: 0,
                });
                views.borrow_mut().insert(key, editor.downgrade());
                editor
            },
        )
        .retained_cells(self.retained.clone())
        .content_revision(self.revision)
        .frozen_rows(self.frozen)
        .frozen_columns(self.frozen)
        .w(px(240.))
        .h(px(100.));
        if self.register {
            grid =
                grid.cell_focus_handle(VirtualGridCellKey::new(0usize, 0usize), self.focus.clone());
        }
        div()
            .id("fallback")
            .track_focus(&self.fallback)
            .child(grid.focus_fallback(self.fallback.clone()))
            .into_any_element()
    }
}
fn harness(cx: &mut Context<Harness>) -> Harness {
    let binding = VirtualizerBinding::new();
    Harness {
        axes: [Axis::Vertical, Axis::Horizontal].map(|axis| {
            cx.new(|_| {
                VirtualizerState::new_attached(
                    axis,
                    source(0, 1_000),
                    VirtualizerSizing::fixed(px(if axis == Axis::Vertical { 20. } else { 50. })),
                    &binding,
                )
                .unwrap()
            })
        }),
        scroll: ScrollHandle::new(),
        retained: Vec::new(),
        focus: cx.focus_handle(),
        fallback: cx.focus_handle(),
        register: false,
        frozen: 0,
        mounted: true,
        revision: 0,
        views: Rc::default(),
    }
}
#[gpui_kit::test]
fn grid_sparse_cell_union_never_expands_to_axis_cross_product(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| harness(cx));
    view.update(cx, |v, cx| {
        v.retained = vec![
            VirtualGridCellKey::new(500usize, 600usize),
            VirtualGridCellKey::new(700usize, 800usize),
            VirtualGridCellKey::new(500usize, 600usize),
            VirtualGridCellKey::new(0usize, 0usize),
            VirtualGridCellKey::new(2000usize, 0usize),
        ];
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let views = view.read_with(cx, |v, _| v.views.clone());
    assert_eq!(
        views
            .borrow()
            .values()
            .filter(|v| v.upgrade().is_some())
            .count(),
        27
    );
    assert!(
        !views
            .borrow()
            .contains_key(&VirtualGridCellKey::new(500usize, 800usize))
    );
    assert!(
        !views
            .borrow()
            .contains_key(&VirtualGridCellKey::new(700usize, 600usize))
    );
    let remote = views.borrow()[&VirtualGridCellKey::new(500usize, 600usize)].clone();
    view.update(cx, |v, cx| {
        v.frozen = 1;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(
        views
            .borrow()
            .values()
            .filter(|v| v.upgrade().is_some())
            .count(),
        27
    );
    assert_eq!(
        remote.upgrade(),
        views.borrow()[&VirtualGridCellKey::new(500usize, 600usize)].upgrade()
    );
    view.update(cx, |v, cx| {
        v.retained.clear();
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(remote.upgrade().is_none());
}
#[gpui_kit::test]
fn grid_cell_editor_survives_scroll_and_repairs_focus_on_delete_or_unregister(
    cx: &mut TestAppContext,
) {
    for delete in [true, false] {
        let (view, cx) = cx.add_window_view(|_, cx| harness(cx));
        view.update(cx, |v, cx| {
            v.register = true;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let views = view.read_with(cx, |v, _| v.views.clone());
        let editor = views.borrow()[&VirtualGridCellKey::new(0usize, 0usize)].clone();
        editor.upgrade().unwrap().update(cx, |v, _| v.edits = 7);
        cx.update(|w, cx| view.read(cx).focus.clone().focus(w, cx));
        view.update(cx, |v, cx| {
            v.scroll.set_precise_offset(gpui_kit::point(-2000., -2000.));
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(editor.upgrade().unwrap().read_with(cx, |v, _| v.edits), 7);
        assert_eq!(
            views
                .borrow()
                .values()
                .filter(|v| v.upgrade().is_some())
                .count(),
            26
        );
        assert!(cx.update(|w, cx| view.read(cx).focus.is_focused(w)));
        if delete {
            let axis = view.read_with(cx, |v, _| v.axes[0].clone());
            axis.update(cx, |s, cx| {
                s.replace_source(
                    source(1, 999),
                    VirtualizerSizing::fixed(px(20.)),
                    VirtualizerChange::Splice {
                        old: 0..1,
                        inserted: 0,
                    },
                )
                .unwrap();
                cx.notify();
            });
        } else {
            view.update(cx, |v, cx| {
                v.register = false;
                cx.notify();
            });
        }
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert!(cx.update(|w, cx| view.read(cx).fallback.is_focused(w)));
        assert!(editor.upgrade().is_none());
        view.update(cx, |v, cx| {
            v.mounted = false;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert!(views.borrow().values().all(|v| v.upgrade().is_none()));
    }
}
