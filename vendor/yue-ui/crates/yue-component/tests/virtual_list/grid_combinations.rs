//! Consumer-observable mixed traces, inspired by TanStack scroll traces and
//! vlist's scoped integration sweeps. No snapshot of private controller state.
use super::*;
use gpui_kit::{FocusHandle, WeakEntity};
use std::{cell::RefCell, collections::HashSet};
use yue_component::{
    VirtualGrid, VirtualGridCellKey, VirtualGridSnapshot, VirtualGridSpan, VirtualizerSizeUpdate,
};

struct Keys(Vec<usize>);
impl VirtualizerSource for Keys {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key_at(&self, i: usize) -> ElementId {
        self.0[i].into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        self.0.iter().position(|i| ElementId::from(*i) == *key)
    }
}
type Clicks = Rc<RefCell<Vec<(usize, usize)>>>;
struct Editor {
    key: (usize, usize),
    focus: FocusHandle,
    clicks: Clicks,
    edits: usize,
}
impl Render for Editor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let key = self.key;
        let clicks = self.clicks.clone();
        div()
            .id("mixed-editor")
            .size_full()
            .track_focus(&self.focus)
            .role(Role::Cell)
            .aria_label(format!("{}:{}", key.0, key.1))
            .on_click(move |_, _, _| clicks.borrow_mut().push(key))
            .test_support()
    }
}
struct Harness {
    axes: Option<[Entity<VirtualizerState>; 2]>,
    scroll: ScrollHandle,
    focus: FocusHandle,
    fallback: FocusHandle,
    register: bool,
    rtl: bool,
    viewport: (f32, f32),
    clicks: Clicks,
    editors: Rc<RefCell<Vec<WeakEntity<Editor>>>>,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(axes) = &self.axes else {
            return div().into_any_element();
        };
        let editors = self.editors.clone();
        let focus = self.focus.clone();
        let clicks = self.clicks.clone();
        let mut grid = VirtualGrid::new_with_cell_views(
            "mixed-grid",
            &axes[0],
            &axes[1],
            &self.scroll,
            move |cell, _, cx| {
                let integer = |key: &ElementId| {
                    if let ElementId::Integer(i) = key {
                        *i as usize
                    } else {
                        panic!("integer domain")
                    }
                };
                let key = (integer(cell.row().key()), integer(cell.column().key()));
                let editor = cx.new(|cx| Editor {
                    key,
                    focus: if key == (70, 70) {
                        focus.clone()
                    } else {
                        cx.focus_handle()
                    },
                    clicks: clicks.clone(),
                    edits: 0,
                });
                editors.borrow_mut().push(editor.downgrade());
                editor
            },
        )
        .frozen_rows(1)
        .frozen_columns(1)
        .right_to_left(self.rtl)
        .focus_fallback(self.fallback.clone())
        .spans(
            &[VirtualGridSpan::new(
                VirtualGridCellKey::new(2usize, 2usize),
                2,
                2,
            )],
            cx,
        )
        .unwrap()
        .w(px(self.viewport.0))
        .h(px(self.viewport.1));
        if self.register {
            grid = grid
                .retained_cells(vec![VirtualGridCellKey::new(70usize, 70usize)])
                .cell_focus_handle(
                    VirtualGridCellKey::new(70usize, 70usize),
                    self.focus.clone(),
                );
        }
        div()
            .id("mixed-fallback")
            .track_focus(&self.fallback)
            .child(grid)
            .into_any_element()
    }
}
fn axes(cx: &mut Context<Harness>) -> [Entity<VirtualizerState>; 2] {
    let binding = VirtualizerBinding::new();
    [Axis::Vertical, Axis::Horizontal].map(|axis| {
        cx.new(|_| {
            VirtualizerState::new_attached(
                axis,
                Rc::new(Keys((0..100).collect())),
                VirtualizerSizing::known(vec![
                    px(if axis == Axis::Vertical { 20. } else { 50. });
                    100
                ]),
                &binding,
            )
            .unwrap()
        })
    })
}
fn draw(cx: &mut gpui_kit::VisualTestContext) {
    cx.update(|w, cx| w.draw(cx).clear(cx));
}

/// Independent linear hit oracle. Frozen prefix uses stationary coordinates;
/// RTL mirrors the physical x before translating the scrollable body.
fn expected_hit(keys: &[usize], sizes: &[f32], coordinate: f64, offset: f64) -> usize {
    let logical = if coordinate < f64::from(sizes[0]) {
        coordinate
    } else {
        coordinate + offset
    };
    let mut start = 0.;
    for (key, extent) in keys.iter().zip(sizes) {
        if logical >= start && logical < start + f64::from(*extent) {
            return *key;
        }
        start += f64::from(*extent);
    }
    panic!("hit outside finite domain")
}
fn assert_hits(
    view: &Entity<Harness>,
    cx: &mut gpui_kit::VisualTestContext,
    keys: &[usize],
    sizes: &[Vec<f32>; 2],
) {
    assert!(
        cx.update(|w, cx| view.read(cx).focus.is_focused(w)),
        "geometry changes preserve the registered editor focus before pointer input"
    );
    let (axes, width, rtl, clicks) = view.read_with(cx, |v, _| {
        (
            v.axes.clone().unwrap(),
            v.viewport.0,
            v.rtl,
            v.clicks.clone(),
        )
    });
    let offsets = axes.map(|axis| axis.read_with(cx, |s, _| s.viewport().unwrap().offset()));
    let nodes = cx.update(|w, _| snapshots(w));
    let labels: Vec<_> = nodes
        .iter()
        .filter(|n| n.role() == Some(Role::Cell))
        .map(|n| n.label().unwrap().to_string())
        .collect();
    assert_eq!(
        labels.len(),
        labels.iter().collect::<HashSet<_>>().len(),
        "no duplicate semantic cell across frozen/span/retained regions"
    );
    assert!(labels.len() < 120, "working set remains bounded");
    for (logical_x, y) in [(10., 10.), (80., 10.), (10., 65.), (165., 75.)] {
        let x = if rtl {
            f64::from(width) - logical_x
        } else {
            logical_x
        };
        let row = expected_hit(keys, &sizes[0], y, offsets[0]);
        let column = expected_hit(keys, &sizes[1], logical_x, offsets[1]);
        let ri = keys.iter().position(|k| *k == row).unwrap();
        let ci = keys.iter().position(|k| *k == column).unwrap();
        let origin = keys.iter().position(|k| *k == 2).unwrap();
        let expected = if (origin..origin + 2).contains(&ri) && (origin..origin + 2).contains(&ci) {
            (2, 2)
        } else {
            (row, column)
        };
        clicks.borrow_mut().clear();
        cx.simulate_click(
            gpui_kit::point(px(x as f32), px(y as f32)),
            Default::default(),
        );
        assert_eq!(
            *clicks.borrow(),
            vec![expected],
            "one hit at {x},{y}, rtl={rtl}"
        );
    }
    // Clicking another cell may legitimately blur the editor. Restore the
    // user's editing focus before the next geometry-only transition.
    cx.update(|w, cx| view.read(cx).focus.clone().focus(w, cx));
}

#[gpui_kit::test]
fn mixed_grid_trace_preserves_anchor_hits_focus_and_releases_every_mount(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| Harness {
        axes: Some(axes(cx)),
        scroll: ScrollHandle::new(),
        focus: cx.focus_handle(),
        fallback: cx.focus_handle(),
        register: true,
        rtl: false,
        viewport: (240., 100.),
        clicks: Rc::default(),
        editors: Rc::default(),
    });
    cx.update(|w, _| w.activate_window());
    cx.run_until_parked();
    for cycle in 0..32 {
        if cycle > 0 {
            view.update(cx, |v, cx| {
                v.axes = Some(axes(cx));
                v.scroll = ScrollHandle::new();
                v.register = true;
                v.rtl = cycle % 2 == 1;
                cx.notify();
            });
        }
        draw(cx);
        let axes = view.read_with(cx, |v, _| v.axes.clone().unwrap());
        let weak_axes = axes.clone().map(|s| s.downgrade());
        let editors = view.read_with(cx, |v, _| v.editors.clone());
        let editor = editors
            .borrow()
            .iter()
            .filter_map(|w| w.upgrade())
            .find(|v| v.read_with(cx, |e, _| e.key) == (70, 70))
            .unwrap();
        editor.update(cx, |e, _| e.edits = cycle + 7);
        cx.update(|w, cx| view.read(cx).focus.clone().focus(w, cx));
        let mut keys: Vec<_> = (0..100).collect();
        let mut sizes = [vec![20.; 100], vec![50.; 100]];
        // Explicit commands avoid assuming native RTL physical coordinates.
        for (axis, target) in axes.iter().zip([35., 80.]) {
            axis.update(cx, |s, cx| {
                s.scroll_to_offset(target, VirtualizerScrollOptions::default())
                    .unwrap();
                cx.notify();
            });
        }
        draw(cx);
        assert_hits(&view, cx, &keys, &sizes);
        assert_eq!(editor.read_with(cx, |e, _| e.edits), cycle + 7);
        assert!(cx.update(|w, cx| view.read(cx).focus.is_focused(w)));

        // virtua-style anchor check during feedback: stable body key + subpixel
        // offset. Frozen growth is excluded from the scrollable correction.
        let anchors = axes.clone().map(|axis| {
            axis.read_with(cx, |s, _| {
                let snap = s.snapshot(s.viewport().unwrap()).unwrap();
                let item = snap.item(snap.visible_range().start).unwrap();
                (
                    item.key().clone(),
                    item.start() - snap.viewport().offset() - snap.item(1).unwrap().start(),
                )
            })
        });
        for (i, axis) in axes.iter().enumerate() {
            sizes[i][0] += 3.25;
            axis.update(cx, |s, cx| {
                s.update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(sizes[i][0]))])
                    .unwrap();
                cx.notify();
            });
        }
        view.update(cx, |v, cx| {
            v.viewport = (280., 125.);
            cx.notify();
        });
        draw(cx);
        for (axis, (key, relative)) in axes.iter().zip(&anchors) {
            axis.read_with(cx, |s, _| {
                let snap = s.snapshot(s.viewport().unwrap()).unwrap();
                assert!(
                    (snap.item_for_key(key).unwrap().start()
                        - snap.viewport().offset()
                        - snap.item(1).unwrap().start()
                        - relative)
                        .abs()
                        < 0.001
                );
            });
        }
        assert_hits(&view, cx, &keys, &sizes);

        // Move two tracks behind the current anchor; keep sizes attached to keys.
        keys.swap(6, 7);
        for size in &mut sizes {
            size.swap(6, 7);
        }
        let old_editors = editors
            .borrow()
            .iter()
            .filter(|weak| {
                weak.upgrade().is_some_and(|e| {
                    e.read_with(cx, |e, _| {
                        [6, 7].contains(&e.key.0) || [6, 7].contains(&e.key.1)
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        assert!(!old_editors.is_empty());
        let editing = editor.downgrade();
        drop(editor);
        for (axis, values) in axes.iter().zip(&sizes) {
            axis.update(cx, |s, cx| {
                s.replace_source(
                    Rc::new(Keys(keys.clone())),
                    VirtualizerSizing::known(values.iter().map(|v| px(*v)).collect::<Vec<_>>()),
                    VirtualizerChange::Replace {
                        preserve_anchor: true,
                    },
                )
                .unwrap();
                cx.notify();
            });
        }
        draw(cx);
        assert_hits(&view, cx, &keys, &sizes);
        for (axis, (key, relative)) in axes.iter().zip(&anchors) {
            axis.read_with(cx, |s, _| {
                let snap = s.snapshot(s.viewport().unwrap()).unwrap();
                assert!(
                    (snap.item_for_key(key).unwrap().start()
                        - snap.viewport().offset()
                        - snap.item(1).unwrap().start()
                        - relative)
                        .abs()
                        < 0.001,
                    "reordering preserves the reading key"
                );
            });
        }
        assert!(
            old_editors.iter().all(|e| e.upgrade().is_none()),
            "changed index/content identities are released"
        );
        assert_eq!(
            editing.upgrade().unwrap().read_with(cx, |e, _| e.edits),
            cycle + 7,
            "unaffected stable editor survives source reordering"
        );
        assert!(cx.update(|w, cx| view.read(cx).focus.is_focused(w)));

        let from = axes
            .clone()
            .map(|axis| axis.read_with(cx, |s, _| s.viewport().unwrap().offset()));
        for (axis, offset) in axes.iter().zip(from) {
            axis.update(cx, |s, cx| {
                s.scroll_to_offset(
                    offset + 20.,
                    VirtualizerScrollOptions::default().motion(VirtualizerMotion::Smooth {
                        duration: Duration::from_millis(200),
                        max_distance: px(100.),
                    }),
                )
                .unwrap();
                cx.notify();
            });
        }
        draw(cx);
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.update(|w, cx| {
            w.simulate_next_frame(cx);
            w.draw(cx).clear(cx);
        });
        for (axis, offset) in axes.iter().zip(from) {
            assert!(
                (axis.read_with(cx, |s, _| s.viewport().unwrap().offset())
                    - offset
                    - 20. * (1. - 0.75f64.powi(3)))
                .abs()
                    < 0.001
            );
        }
        assert_hits(&view, cx, &keys, &sizes);
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: gpui_kit::point(px(120.), px(80.)),
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-1.25), px(-2.5))),
            modifiers: Default::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        draw(cx);
        assert!(
            axes.iter()
                .all(|axis| !axis.read_with(cx, |s, _| s.has_scroll_command()))
        );
        assert_hits(&view, cx, &keys, &sizes);
        view.update(cx, |v, cx| {
            v.register = false;
            cx.notify();
        });
        draw(cx);
        assert!(cx.update(|w, cx| view.read(cx).fallback.is_focused(w)));
        assert!(
            editors
                .borrow()
                .iter()
                .filter_map(|e| e.upgrade())
                .all(|e| e.read_with(cx, |e, _| e.key != (70, 70)))
        );
        // Deliberate retained-owner negative control proves weak checks observe
        // an external reference, then release it along with the mount.
        let held = if cycle == 0 {
            editors.borrow().iter().find_map(|e| e.upgrade())
        } else {
            None
        };
        // Leave a fresh animation pending on unmount as well as the wheel's
        // idle timer; neither may retain an old owner or request stale frames.
        for axis in &axes {
            axis.update(cx, |s, cx| {
                s.scroll_to_offset(
                    150.,
                    VirtualizerScrollOptions::default().motion(VirtualizerMotion::Smooth {
                        duration: Duration::from_millis(200),
                        max_distance: px(200.),
                    }),
                )
                .unwrap();
                cx.notify();
            });
        }
        draw(cx);
        view.update(cx, |v, cx| {
            v.axes = None;
            cx.notify();
        });
        drop(axes);
        draw(cx);
        cx.executor().advance_clock(Duration::from_millis(1000));
        cx.run_until_parked();
        draw(cx);
        assert!(weak_axes.iter().all(|s| s.upgrade().is_none()));
        assert_eq!(
            editors
                .borrow()
                .iter()
                .filter(|e| e.upgrade().is_some())
                .count(),
            usize::from(held.is_some())
        );
        drop(held);
        assert!(editors.borrow().iter().all(|e| e.upgrade().is_none()));
        editors.borrow_mut().clear();
    }
}

fn trace_rand(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *seed
}

#[gpui_kit::test]
fn generated_grid_state_trace_keeps_worksets_unique_and_two_axes_coherent(cx: &mut TestAppContext) {
    let binding = VirtualizerBinding::new();
    let rows = cx.new(|_| {
        VirtualizerState::new_attached(
            Axis::Vertical,
            Rc::new(Keys((0..32).collect())),
            VirtualizerSizing::known(vec![px(20.); 32]),
            &binding,
        )
        .unwrap()
    });
    let columns = cx.new(|_| {
        VirtualizerState::new_attached(
            Axis::Horizontal,
            Rc::new(Keys((0..32).collect())),
            VirtualizerSizing::known(vec![px(50.); 32]),
            &binding,
        )
        .unwrap()
    });
    let mut seed = 0x5eed_2026_u64;
    let mut row_offset = 0.;
    let mut column_offset = 0.;
    for step in 0..128u64 {
        match trace_rand(&mut seed) % 5 {
            0 => {
                let row = (trace_rand(&mut seed) % 32) as usize;
                let column = (trace_rand(&mut seed) % 32) as usize;
                let row_extent = 18. + (trace_rand(&mut seed) % 12) as f32;
                let column_extent = 42. + (trace_rand(&mut seed) % 20) as f32;
                cx.update(|app| {
                    VirtualGrid::update_sizes(
                        &rows,
                        &columns,
                        &[VirtualizerSizeUpdate::new(row, px(row_extent))],
                        &[VirtualizerSizeUpdate::new(column, px(column_extent))],
                        app,
                    )
                    .unwrap();
                });
            }
            1 => {
                row_offset = (trace_rand(&mut seed) % 500) as f64;
                column_offset = (trace_rand(&mut seed) % 900) as f64;
            }
            2 => {
                let row = (trace_rand(&mut seed) % 32) as usize;
                let column = (trace_rand(&mut seed) % 32) as usize;
                cx.update_entity(&rows, |state, cx| {
                    state
                        .scroll_to_offset(row as f64 * 20., VirtualizerScrollOptions::default())
                        .unwrap();
                    cx.notify();
                });
                cx.update_entity(&columns, |state, cx| {
                    state
                        .scroll_to_offset(column as f64 * 50., VirtualizerScrollOptions::default())
                        .unwrap();
                    cx.notify();
                });
            }
            3 => {
                let shift = (trace_rand(&mut seed) % 31 + 1) as usize;
                let mut keys: Vec<_> = (0..32).collect();
                keys.rotate_left(shift);
                let row_sizes = keys.iter().map(|_| px(20.)).collect::<Vec<_>>();
                let column_sizes = keys.iter().map(|_| px(50.)).collect::<Vec<_>>();
                cx.update_entity(&rows, |state, cx| {
                    state
                        .replace_source(
                            Rc::new(Keys(keys.clone())),
                            VirtualizerSizing::known(row_sizes),
                            VirtualizerChange::Reorder,
                        )
                        .unwrap();
                    cx.notify();
                });
                cx.update_entity(&columns, |state, cx| {
                    state
                        .replace_source(
                            Rc::new(Keys(keys)),
                            VirtualizerSizing::known(column_sizes),
                            VirtualizerChange::Reorder,
                        )
                        .unwrap();
                    cx.notify();
                });
            }
            _ => {
                row_offset = 0.;
                column_offset = 0.;
            }
        }
        let rv = VirtualizerViewport::new(&binding, step + 1, row_offset, px(100.), px(240.));
        let cv = VirtualizerViewport::new(&binding, step + 1, column_offset, px(240.), px(100.));
        let grid = cx.read(|app| {
            VirtualGridSnapshot::new(
                rows.read_with(app, |state, _| state.snapshot(&rv).unwrap()),
                columns.read_with(app, |state, _| state.snapshot(&cv).unwrap()),
            )
            .unwrap()
        });
        let cells = grid.cells().collect::<Vec<_>>();
        let unique = cells
            .iter()
            .map(|cell| (cell.row().key().clone(), cell.column().key().clone()))
            .collect::<HashSet<_>>();
        assert_eq!(cells.len(), unique.len(), "step {step} duplicates a cell");
        assert!(
            cells.len() <= 120,
            "step {step} escaped the bounded workset"
        );
    }
}

struct TableHarness {
    axes: [Entity<VirtualizerState>; 2],
    scroll: ScrollHandle,
    selected: (usize, usize),
}

impl Render for TableHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected;
        VirtualGrid::new(
            "consumer-table",
            &self.axes[0],
            &self.axes[1],
            &self.scroll,
            move |cell, _, _| {
                let key = (cell.row().key().clone(), cell.column().key().clone());
                let row = match key.0 {
                    ElementId::Integer(value) => value as usize,
                    _ => unreachable!("table keys use integer domain identities"),
                };
                let column = match key.1 {
                    ElementId::Integer(value) => value as usize,
                    _ => unreachable!("table keys use integer domain identities"),
                };
                let is_selected = selected == (row, column);
                div()
                    .id(("table-cell", row * 32 + column))
                    .role(Role::Cell)
                    .aria_label(format!("{row}:{column}"))
                    .aria_row_index(cell.row().index())
                    .aria_column_index(cell.column().index())
                    .aria_selected(is_selected)
                    .size_full()
                    .test_support()
            },
        )
        .role(Role::Grid)
        .aria_label("业务表格")
        .aria_row_count(32)
        .aria_column_count(32)
        .retained_cells(vec![VirtualGridCellKey::new(selected.0, selected.1)])
        .w(px(240.))
        .h(px(100.))
        .test_support()
    }
}

fn table_harness(cx: &mut Context<TableHarness>) -> TableHarness {
    let binding = VirtualizerBinding::new();
    TableHarness {
        axes: [Axis::Vertical, Axis::Horizontal].map(|axis| {
            cx.new(|_| {
                VirtualizerState::new_attached(
                    axis,
                    Rc::new(Keys((0..32).collect())),
                    VirtualizerSizing::known(vec![
                        px(if axis == Axis::Vertical {
                            20.
                        } else {
                            50.
                        });
                        32
                    ]),
                    &binding,
                )
                .unwrap()
            })
        }),
        scroll: ScrollHandle::new(),
        selected: (7, 3),
    }
}

#[gpui_kit::test]
fn table_consumer_keeps_domain_selection_and_semantics_through_reorder_and_resize(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| table_harness(cx));
    let selected_cell = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|w, _| {
            snapshots(w)
                .into_iter()
                .filter(|node| node.role() == Some(Role::Cell))
                .filter(|node| node.selected() == Some(true))
                .collect::<Vec<_>>()
        })
    };
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let initial = selected_cell(cx);
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].label(), Some("7:3"));
    assert_eq!(initial[0].row_index(), Some(7));
    assert_eq!(initial[0].column_index(), Some(3));

    let axes = view.read_with(cx, |table, _| table.axes.clone());
    for (axis, target) in axes.iter().zip([120., 250.]) {
        axis.update(cx, |state, cx| {
            state
                .scroll_to_offset(target, VirtualizerScrollOptions::default())
                .unwrap();
            cx.notify();
        });
    }
    cx.update(|w, cx| w.draw(cx).clear(cx));

    let mut order: Vec<_> = (0..32).collect();
    order.rotate_left(5);
    let row_sizes = order.iter().map(|_| px(20.)).collect::<Vec<_>>();
    let column_sizes = order.iter().map(|_| px(50.)).collect::<Vec<_>>();
    axes[0].update(cx, |state, cx| {
        state
            .replace_source(
                Rc::new(Keys(order.clone())),
                VirtualizerSizing::known(row_sizes),
                VirtualizerChange::Reorder,
            )
            .unwrap();
        cx.notify();
    });
    axes[1].update(cx, |state, cx| {
        state
            .replace_source(
                Rc::new(Keys(order.clone())),
                VirtualizerSizing::known(column_sizes),
                VirtualizerChange::Reorder,
            )
            .unwrap();
        cx.notify();
    });
    cx.update(|_, app| {
        VirtualGrid::update_sizes(
            &axes[0],
            &axes[1],
            &[VirtualizerSizeUpdate::new(7usize, px(27.))],
            &[VirtualizerSizeUpdate::new(3usize, px(61.))],
            app,
        )
        .unwrap();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let after = selected_cell(cx);
    assert_eq!(
        after.len(),
        1,
        "selection must remain a single semantic cell"
    );
    assert_eq!(after[0].label(), Some("7:3"));
    assert_eq!(
        after[0].row_index(),
        Some(order.iter().position(|key| *key == 7).unwrap())
    );
    assert_eq!(
        after[0].column_index(),
        Some(order.iter().position(|key| *key == 3).unwrap())
    );
    let all_cells = cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .filter(|node| node.role() == Some(Role::Cell))
            .collect::<Vec<_>>()
    });
    let identities = all_cells
        .iter()
        .map(|node| (node.row_index(), node.column_index()))
        .collect::<HashSet<_>>();
    assert_eq!(
        identities.len(),
        all_cells.len(),
        "reorder must not duplicate cell semantics"
    );
}
