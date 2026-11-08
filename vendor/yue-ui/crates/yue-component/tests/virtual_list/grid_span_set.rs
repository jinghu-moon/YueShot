use super::*;
use yue_component::{VirtualGrid, VirtualGridCellKey, VirtualGridSpan, VirtualGridSpanSet};

fn descriptor() -> VirtualGridSpan {
    VirtualGridSpan::new(VirtualGridCellKey::new(3usize, 3usize), 2, 2)
}
fn axes(cx: &mut App, sources: [Rc<Source>; 2]) -> [Entity<VirtualizerState>; 2] {
    let binding = VirtualizerBinding::new();
    [Axis::Vertical, Axis::Horizontal]
        .into_iter()
        .zip(sources)
        .map(|(axis, source)| {
            cx.new(|_| {
                VirtualizerState::new_attached(
                    axis,
                    source,
                    VirtualizerSizing::known(vec![
                        px(if axis == Axis::Vertical {
                            20.
                        } else {
                            50.
                        });
                        1000
                    ]),
                    &binding,
                )
                .unwrap()
            })
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}
fn configure(
    axes: &[Entity<VirtualizerState>; 2],
    set: &VirtualGridSpanSet,
    frozen: usize,
    cx: &App,
) -> Result<VirtualGrid, VirtualizerError> {
    VirtualGrid::new(
        "cached",
        &axes[0],
        &axes[1],
        &ScrollHandle::new(),
        |_, _, _| div(),
    )
    .frozen_rows(frozen)
    .span_set(set, cx)
}

#[gpui_kit::test]
fn span_set_reuses_resolution_and_rejects_invalid_configuration_atomically(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let sources = [source(0, 1000), source(0, 1000)];
        let axes = axes(cx, sources.clone());
        let set = VirtualGridSpanSet::new(vec![descriptor()]);
        let clone = set.clone();
        assert_eq!(clone.len(), 1);
        for source in &sources {
            source.calls.set(0);
        }
        drop(configure(&axes, &set, 0, cx).unwrap());
        assert_eq!(sources.iter().map(|s| s.calls.get()).sum::<usize>(), 2);
        for source in &sources {
            source.calls.set(0);
        }
        for _ in 0..32 {
            drop(configure(&axes, &clone, 0, cx).unwrap());
        }
        assert_eq!(sources.iter().map(|s| s.calls.get()).sum::<usize>(), 0);
        assert!(matches!(
            configure(&axes, &set, 4, cx),
            Err(VirtualizerError::Unsupported(
                "span crosses frozen boundary"
            ))
        ));
        for source in &sources {
            source.calls.set(0);
        }
        drop(configure(&axes, &set, 0, cx).unwrap());
        assert_eq!(
            sources.iter().map(|s| s.calls.get()).sum::<usize>(),
            0,
            "invalid miss preserves last good entry"
        );
        drop(configure(&axes, &set, 3, cx).unwrap());
        assert_eq!(
            sources.iter().map(|s| s.calls.get()).sum::<usize>(),
            2,
            "changed freeze revalidates"
        );
        let invalid = VirtualGridSpanSet::new(vec![descriptor(), descriptor()]);
        assert!(matches!(
            configure(&axes, &invalid, 0, cx),
            Err(VirtualizerError::InvalidGeometry("overlapping spans"))
        ));
        let empty = VirtualGridSpanSet::default();
        assert!(empty.is_empty());
        for source in &sources {
            source.calls.set(0);
        }
        drop(configure(&axes, &empty, 0, cx).unwrap());
        assert_eq!(sources.iter().map(|s| s.calls.get()).sum::<usize>(), 0);
    });
}

#[gpui_kit::test]
fn span_set_does_not_retain_owners_and_distinguishes_new_entities_at_same_generation(
    cx: &mut TestAppContext,
) {
    let set = VirtualGridSpanSet::new(vec![descriptor()]);
    let weak = cx.update(|cx| {
        let sources = [source(0, 1000), source(0, 1000)];
        let weak = Rc::downgrade(&sources[0]);
        let axes = axes(cx, sources);
        drop(configure(&axes, &set, 0, cx).unwrap());
        weak
    });
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "retained set cannot retain the source through its controllers"
    );
    cx.update(|cx| {
        let sources = [source(0, 1000), source(0, 1000)];
        let axes = axes(cx, sources.clone());
        for source in &sources {
            source.calls.set(0);
        }
        drop(configure(&axes, &set, 0, cx).unwrap());
        assert_eq!(sources.iter().map(|s| s.calls.get()).sum::<usize>(), 2);
        axes[0].update(cx, |s, _| {
            s.replace_source(
                source(4, 996),
                VirtualizerSizing::known(vec![px(20.); 996]),
                VirtualizerChange::Replace {
                    preserve_anchor: false,
                },
            )
            .unwrap()
        });
        assert!(matches!(
            configure(&axes, &set, 0, cx),
            Err(VirtualizerError::MissingKey)
        ));
        let binding = VirtualizerBinding::new();
        let measured = [Axis::Vertical, Axis::Horizontal].map(|axis| {
            cx.new(|_| {
                VirtualizerState::new_attached(
                    axis,
                    source(0, 1000),
                    VirtualizerSizing::measured(px(20.)),
                    &binding,
                )
                .unwrap()
            })
        });
        assert!(matches!(
            configure(&measured, &set, 0, cx),
            Err(VirtualizerError::Unsupported(
                "grid requires explicit row/column dimensions"
            ))
        ));
    });
}

struct Reordered;
impl VirtualizerSource for Reordered {
    fn len(&self) -> usize {
        1000
    }
    fn key_at(&self, index: usize) -> ElementId {
        (if index == 2 {
            3
        } else if index == 3 {
            2
        } else {
            index
        })
        .into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        match key {
            ElementId::Integer(i) if *i < 1000 => Some(if *i == 2 {
                3
            } else if *i == 3 {
                2
            } else {
                *i as usize
            }),
            _ => None,
        }
    }
}

struct Harness {
    axes: [Entity<VirtualizerState>; 2],
    set: VirtualGridSpanSet,
    cached: bool,
    scroll: ScrollHandle,
    clicks: Rc<std::cell::RefCell<Vec<(usize, usize)>>>,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        let grid = VirtualGrid::new(
            "span-parity",
            &self.axes[0],
            &self.axes[1],
            &self.scroll,
            move |cell, _, _| {
                let key = (cell.row().index(), cell.column().index());
                let clicks = clicks.clone();
                div()
                    .id("cell")
                    .size_full()
                    .role(Role::Cell)
                    .aria_label(format!("{}:{}", key.0, key.1))
                    .on_click(move |_, _, _| clicks.borrow_mut().push(key))
                    .test_support()
            },
        );
        let grid = if self.cached {
            grid.span_set(&self.set, cx)
        } else {
            grid.spans(&[descriptor()], cx)
        }
        .unwrap();
        grid.w(px(240.)).h(px(100.)).test_support()
    }
}
#[gpui_kit::test]
fn span_set_matches_full_render_hit_geometry_after_reorder_and_size_feedback(
    cx: &mut TestAppContext,
) {
    let mut results = Vec::new();
    for cached in [false, true] {
        let (view, cx) = cx.add_window_view(|_, cx| Harness {
            axes: axes(cx, [source(0, 1000), source(0, 1000)]),
            set: VirtualGridSpanSet::new(vec![descriptor()]),
            cached,
            scroll: ScrollHandle::new(),
            clicks: Rc::default(),
        });
        let mut sequence = Vec::new();
        for phase in 0..4 {
            view.update(cx, |v, cx| {
                if phase == 1 {
                    for (axis, extent) in v.axes.iter().zip([20., 50.]) {
                        axis.update(cx, |s, _| {
                            s.update_sizes(&[yue_component::VirtualizerSizeUpdate::new(
                                4usize,
                                px(extent + 5.),
                            )])
                            .unwrap()
                        });
                    }
                }
                if phase == 2 {
                    for (axis, extent) in v.axes.iter().zip([20., 50.]) {
                        axis.update(cx, |s, _| {
                            s.replace_source(
                                Rc::new(Reordered),
                                VirtualizerSizing::known(vec![px(extent); 1000]),
                                VirtualizerChange::Reorder,
                            )
                            .unwrap()
                        });
                    }
                }
                if phase == 3 {
                    v.scroll.set_precise_offset(gpui_kit::point(-125., -50.));
                }
                cx.notify();
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let nodes = cx.update(|w, _| snapshots(w));
            let mut cells = nodes
                .iter()
                .filter(|n| n.role() == Some(Role::Cell))
                .map(|n| (n.label().unwrap().to_owned(), n.bounds()))
                .collect::<Vec<_>>();
            cells.sort_by(|a, b| a.0.cmp(&b.0));
            let origin_label = if phase < 2 { "3:3" } else { "2:2" };
            let origin = cells
                .iter()
                .find(|(label, _)| label == origin_label)
                .unwrap();
            let expected = if phase == 1 {
                gpui_kit::size(px(105.), px(45.))
            } else {
                gpui_kit::size(px(100.), px(40.))
            };
            assert_eq!(origin.1.size, expected, "fresh track geometry on cache hit");
            // Point within the origin and viewport, including an offscreen origin.
            let click = gpui_kit::point(
                px(if phase < 2 {
                    160.
                } else if phase == 2 {
                    110.
                } else {
                    10.
                }),
                px(if phase < 2 {
                    70.
                } else if phase == 2 {
                    50.
                } else {
                    5.
                }),
            );
            view.read_with(cx, |v, _| v.clicks.borrow_mut().clear());
            cx.simulate_click(click, Default::default());
            let hit = view.read_with(cx, |v, _| v.clicks.borrow().clone());
            assert_eq!(hit, vec![if phase < 2 { (3, 3) } else { (2, 2) }]);
            sequence.push(cells);
        }
        results.push(sequence);
    }
    assert_eq!(
        results[0], results[1],
        "full real cell geometry agrees with uncached path"
    );
}
