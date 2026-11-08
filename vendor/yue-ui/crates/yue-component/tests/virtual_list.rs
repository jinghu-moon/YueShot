//! Production virtualization seams; no native UI process is launched.
use gpui_kit::base::{TestSupportExt as _, test_support::snapshots, virtualization::VirtualList};
use gpui_kit::{
    AnyElement, App, AppContext as _, AvailableSpace, Axis, Bounds, ContentMask, Context, Div,
    Element, ElementId, Entity, GlobalElementId, Hitbox, InspectorElementId,
    InteractiveElement as _, IntoElement, LayoutId, ListAlignment, ListState, ListViewportLayout,
    ParentElement as _, Pixels, Render, Role, ScrollHandle, Stateful,
    StatefulInteractiveElement as _, Styled, TestAppContext, Window, div, list, px,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use yue_component::{
    VirtualizerAlign, VirtualizerBinding, VirtualizerChange, VirtualizerCommandOutcome,
    VirtualizerError, VirtualizerEvent, VirtualizerLayout, VirtualizerMotion,
    VirtualizerScrollOptions, VirtualizerSizing, VirtualizerSource, VirtualizerState,
    VirtualizerViewport, YueVirtualList,
};

#[path = "virtual_list/adoption.rs"]
mod adoption;
#[path = "virtual_list/api_contract.rs"]
mod api_contract;
#[path = "virtual_list/directional.rs"]
mod directional;
#[path = "virtual_list/feedback.rs"]
mod feedback;
#[path = "virtual_list/followup.rs"]
mod followup;
#[path = "virtual_list/followup_edges.rs"]
mod followup_edges;
#[path = "virtual_list/frozen_grid.rs"]
mod frozen_grid;
#[path = "virtual_list/grid_combinations.rs"]
mod grid_combinations;
#[path = "virtual_list/grid_observed.rs"]
mod grid_observed;
#[path = "virtual_list/grid_preparation.rs"]
mod grid_preparation;
#[path = "virtual_list/grid_retention.rs"]
mod grid_retention;
#[path = "virtual_list/grid_span_set.rs"]
mod grid_span_set;
#[path = "virtual_list/grid_spans.rs"]
mod grid_spans;
#[path = "virtual_list/grid_tracks.rs"]
mod grid_tracks;
#[path = "virtual_list/masonry_activity.rs"]
mod masonry_activity;
#[path = "virtual_list/masonry_cache.rs"]
mod masonry_cache;
#[path = "virtual_list/masonry_follow_tail.rs"]
mod masonry_follow_tail;
#[path = "virtual_list/masonry_offset.rs"]
mod masonry_offset;
#[path = "virtual_list/masonry_smooth.rs"]
mod masonry_smooth;
#[path = "virtual_list/measured_smooth.rs"]
mod measured_smooth;
#[path = "virtual_list/optimizations.rs"]
mod optimizations;
#[path = "virtual_list/owned_reflow.rs"]
mod owned_reflow;
#[path = "virtual_list/owner.rs"]
mod owner;
#[path = "virtual_list/priorities.rs"]
mod priorities;

#[path = "virtual_list/focus.rs"]
mod focus;

#[path = "virtual_list/edges.rs"]
mod edges;

#[path = "virtual_list/performance.rs"]
mod performance;

#[path = "virtual_list/layouts.rs"]
mod layouts;
#[path = "virtual_list/lifecycle.rs"]
mod lifecycle;
#[path = "virtual_list/preparation.rs"]
mod preparation;
#[path = "virtual_list/refinements.rs"]
mod refinements;
#[path = "virtual_list/traces.rs"]
mod traces;
#[path = "virtual_list/upgrades.rs"]
mod upgrades;

/// Arithmetic domain keys, O(1) metadata and reverse lookup regardless of N.
struct Source {
    start: usize,
    count: usize,
    calls: Rc<Cell<usize>>,
}
impl VirtualizerSource for Source {
    fn len(&self) -> usize {
        self.count
    }
    fn key_at(&self, index: usize) -> ElementId {
        self.calls.set(self.calls.get() + 1);
        (self.start + index).into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        self.calls.set(self.calls.get() + 1);
        if let ElementId::Integer(key) = key {
            let key = *key as usize;
            if key >= self.start && key - self.start < self.count {
                Some(key - self.start)
            } else {
                None
            }
        } else {
            None
        }
    }
}
fn source(start: usize, count: usize) -> Rc<Source> {
    Rc::new(Source {
        start,
        count,
        calls: Rc::new(Cell::new(0)),
    })
}
fn fixed(count: usize) -> VirtualizerState {
    VirtualizerState::new(
        Axis::Vertical,
        source(0, count),
        VirtualizerSizing::fixed(px(20.)),
    )
    .unwrap()
}
fn viewport(state: &VirtualizerState, revision: u64, offset: f64) -> VirtualizerViewport {
    VirtualizerViewport::new(
        state.binding().unwrap(),
        revision,
        offset,
        px(100.),
        px(240.),
    )
    .input_revision(0)
}

#[test]
fn fixed_queries_allocate_no_per_item_metadata_and_preserve_large_coordinate_precision() {
    for count in [0, 1, 1_000, 100_000, 1_000_000, 1_000_000_000] {
        let source = source(0, count);
        let state = VirtualizerState::new(
            Axis::Vertical,
            source.clone(),
            VirtualizerSizing::fixed(px(20.)),
        )
        .unwrap();
        assert_eq!(source.calls.get(), 0);
        let offset = if count > 10 {
            (count - 10) as f64 * 20.0 + 0.25
        } else {
            0.0
        };
        let snapshot = state.snapshot(&viewport(&state, 1, offset)).unwrap();
        assert_eq!(snapshot.total_extent(), count as f64 * 20.0);
        assert!(snapshot.rendered_range().len() <= 6);
        for index in snapshot.rendered_range() {
            let item = snapshot.item(index).unwrap();
            assert_eq!(item.start(), index as f64 * 20.0);
        }
        assert!(source.calls.get() <= 12);
    }
}

#[test]
fn fixed_and_known_ranges_match_naive_half_open_intersections_including_gaps() {
    for gap in [0., 3., 25.] {
        let mut fixed = fixed(100);
        fixed
            .set_layout(VirtualizerLayout::default().gap(px(gap)))
            .unwrap();
        let mut known = VirtualizerState::new(
            Axis::Horizontal,
            source(0, 100),
            VirtualizerSizing::known(vec![px(20.); 100]),
        )
        .unwrap();
        known
            .set_layout(VirtualizerLayout::default().gap(px(gap)))
            .unwrap();
        for offset in 0..2_500 {
            let offset = offset as f64 / 2.0;
            let expected: Vec<_> = (0..100)
                .filter(|index| {
                    let start = *index as f64 * (20.0 + gap as f64);
                    start < offset + 100.0 && start + 20.0 > offset
                })
                .collect();
            let a = fixed.snapshot(&viewport(&fixed, 1, offset)).unwrap();
            let b = known.snapshot(&viewport(&known, 1, offset)).unwrap();
            assert_eq!(a.visible_range().collect::<Vec<_>>(), expected);
            assert_eq!(b.visible_range().collect::<Vec<_>>(), expected);
        }
    }
}

#[test]
fn known_gap_revision_and_frozen_snapshots_have_one_consistent_geometry() {
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        source(0, 20),
        VirtualizerSizing::known(vec![px(20.); 20]),
    )
    .unwrap();
    let old = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
    state
        .set_layout(VirtualizerLayout::default().gap(px(10.)))
        .unwrap();
    let new = state.snapshot(&viewport(&state, 2, 0.)).unwrap();
    assert_eq!(old.item(8).unwrap().start(), 160.);
    assert_eq!(new.item(8).unwrap().start(), 240.);
    assert_eq!(new.total_extent(), 590.);
    assert_eq!(
        state.validate_snapshot(&old),
        Err(VirtualizerError::StaleSnapshot)
    );
    let before = new.total_extent();
    assert!(
        state
            .set_layout(VirtualizerLayout::default().gap(px(f32::NAN)))
            .is_err()
    );
    assert_eq!(state.total_extent(), before);
}

#[test]
fn invalid_and_unsupported_configurations_fail_before_querying_source_keys() {
    let source = source(0, 1_000_000);
    for extent in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(
            VirtualizerState::new(
                Axis::Vertical,
                source.clone(),
                VirtualizerSizing::fixed(px(extent))
            )
            .is_err()
        );
        assert!(
            VirtualizerState::new(
                Axis::Vertical,
                source.clone(),
                VirtualizerSizing::measured(px(extent))
            )
            .is_err()
        );
    }
    assert!(
        VirtualizerState::new(
            Axis::Vertical,
            source.clone(),
            VirtualizerSizing::known(vec![px(20.)])
        )
        .is_err()
    );
    assert_eq!(source.calls.get(), 0);
    let mut state = fixed(10);
    assert!(
        state
            .set_layout(VirtualizerLayout::default().measurement_budget(0))
            .is_err()
    );
    assert_eq!(state.total_extent(), 200.);
    assert_eq!(
        state.replace_source(
            source.clone(),
            VirtualizerSizing::fixed(px(20.)),
            VirtualizerChange::Splice {
                old: 0..0,
                inserted: usize::MAX
            }
        ),
        Err(VirtualizerError::InvalidChange)
    );
    let mut measured = VirtualizerState::new(
        Axis::Vertical,
        crate::source(0, 1),
        VirtualizerSizing::measured(px(f32::MAX)),
    )
    .unwrap();
    assert!(
        measured
            .set_layout(VirtualizerLayout::default().gap(px(f32::MAX)))
            .is_err()
    );
}

#[test]
fn attached_axis_slots_are_local_exclusive_and_reusable_after_detach() {
    let binding = VirtualizerBinding::new();
    let mut vertical = VirtualizerState::new_attached(
        Axis::Vertical,
        source(0, 100),
        VirtualizerSizing::fixed(px(20.)),
        &binding,
    )
    .unwrap();
    let horizontal = VirtualizerState::new_attached(
        Axis::Horizontal,
        source(0, 100),
        VirtualizerSizing::known(vec![px(30.); 100]),
        &binding,
    )
    .unwrap();
    assert!(matches!(
        VirtualizerState::new_attached(
            Axis::Vertical,
            source(0, 1),
            VirtualizerSizing::fixed(px(20.)),
            &binding
        ),
        Err(VirtualizerError::BindingConflict)
    ));
    assert!(vertical.scrollbar_handle().is_err());
    assert_eq!(
        vertical.bind(&VirtualizerBinding::new()),
        Err(VirtualizerError::AlreadyBound)
    );
    let old = viewport(&vertical, 1, 0.);
    vertical.detach();
    assert!(matches!(
        vertical.snapshot(&old),
        Err(VirtualizerError::Detached)
    ));
    vertical.bind(&binding).unwrap();
    assert_eq!(horizontal.axis(), Axis::Horizontal);
}

#[test]
fn requests_are_versioned_exactly_once_and_stale_input_cannot_move_the_host() {
    let mut state = fixed(100);
    let input = viewport(&state, 1, 0.);
    state.commit(&input).unwrap();
    state
        .scroll_to_key(
            50usize.into(),
            VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
        )
        .unwrap();
    let (_, request) = state.prepare(&input, Instant::now()).unwrap();
    let request = request.unwrap();
    assert_eq!(request.offset(), 1_000.);
    let newer = viewport(&state, 2, 0.);
    assert_eq!(
        state.validate_request(&request, &newer),
        Err(VirtualizerError::StaleRequest)
    );
    state.confirm(&request, 1_000., &input).unwrap();
    assert_eq!(
        state.confirm(&request, 1_000., &input),
        Err(VirtualizerError::RequestAlreadyAccepted)
    );
    let events = state.take_events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                VirtualizerEvent::CommandFinished {
                    outcome: VirtualizerCommandOutcome::Settled,
                    ..
                }
            ))
            .count(),
        1
    );
    state
        .scroll_to_key(90usize.into(), VirtualizerScrollOptions::default())
        .unwrap();
    let (_, request) = state.prepare(&newer, Instant::now()).unwrap();
    let request = request.unwrap();
    state
        .replace_source(
            source(0, 5),
            VirtualizerSizing::fixed(px(20.)),
            VirtualizerChange::Replace {
                preserve_anchor: false,
            },
        )
        .unwrap();
    assert_eq!(
        state.validate_request(&request, &newer),
        Err(VirtualizerError::StaleRequest)
    );
}

#[test]
fn prepend_delete_and_replace_preserve_domain_anchors_not_indexes() {
    let mut state = VirtualizerState::new(
        Axis::Vertical,
        source(10, 100),
        VirtualizerSizing::fixed(px(20.)),
    )
    .unwrap();
    state.commit(&viewport(&state, 1, 205.)).unwrap(); // key 20, five pixels inside
    state
        .replace_source(
            source(0, 110),
            VirtualizerSizing::fixed(px(20.)),
            VirtualizerChange::Splice {
                old: 0..0,
                inserted: 10,
            },
        )
        .unwrap();
    let input = viewport(&state, 2, 205.);
    let (_, request) = state.prepare(&input, Instant::now()).unwrap();
    assert_eq!(request.as_ref().unwrap().offset(), 405.);
    state.confirm(&request.unwrap(), 405., &input).unwrap();
    state
        .replace_source(
            source(21, 89),
            VirtualizerSizing::fixed(px(20.)),
            VirtualizerChange::Splice {
                old: 0..21,
                inserted: 0,
            },
        )
        .unwrap();
    let input = viewport(&state, 3, 405.);
    let (_, request) = state.prepare(&input, Instant::now()).unwrap();
    assert_eq!(request.unwrap().offset(), 0.); // first surviving successor
    let generation = state.generation();
    assert!(
        state
            .replace_source(
                source(0, 10),
                VirtualizerSizing::fixed(px(20.)),
                VirtualizerChange::Splice {
                    old: 0..999,
                    inserted: 0
                }
            )
            .is_err()
    );
    assert_eq!(state.generation(), generation);
}

#[test]
fn all_alignments_nearest_large_items_and_short_smooth_have_explicit_results() {
    for (align, expected) in [
        (VirtualizerAlign::Start, 200.),
        (VirtualizerAlign::Center, 160.),
        (VirtualizerAlign::End, 120.),
        (VirtualizerAlign::Nearest, 120.),
    ] {
        let mut state = fixed(100);
        let input = viewport(&state, 1, 0.);
        state
            .scroll_to_key(
                10usize.into(),
                VirtualizerScrollOptions::default().align(align),
            )
            .unwrap();
        let (_, request) = state.prepare(&input, Instant::now()).unwrap();
        assert_eq!(request.unwrap().offset(), expected);
    }
    let mut state = fixed(100);
    let input = viewport(&state, 1, 0.);
    state.commit(&input).unwrap();
    state
        .scroll_to_key(
            5usize.into(),
            VirtualizerScrollOptions::default()
                .align(VirtualizerAlign::Start)
                .motion(VirtualizerMotion::Smooth {
                    duration: Duration::from_millis(200),
                    max_distance: px(200.),
                }),
        )
        .unwrap();
    let now = Instant::now();
    state.prepare(&input, now).unwrap();
    let (_, request) = state
        .prepare(&input, now + Duration::from_millis(100))
        .unwrap();
    let request = request.unwrap();
    assert!(request.offset() > 70. && request.offset() < 99.);
    assert!(!request.is_settled());
    state.confirm(&request, request.offset(), &input).unwrap();
    let input = viewport(&state, 2, request.offset());
    let (_, request) = state
        .prepare(&input, now + Duration::from_millis(250))
        .unwrap();
    let request = request.unwrap();
    assert_eq!(request.offset(), 100.);
    assert!(request.is_settled());
    state.confirm(&request, 100., &input).unwrap();
    assert!(!state.has_scroll_command());
    state
        .scroll_to_key(
            99usize.into(),
            VirtualizerScrollOptions::default().motion(VirtualizerMotion::Smooth {
                duration: Duration::from_secs(1),
                max_distance: px(200.),
            }),
        )
        .unwrap();
    let (_, request) = state.prepare(&input, Instant::now()).unwrap();
    assert!(request.unwrap().is_settled()); // long jump is instantaneous
}

struct CardHarness {
    state: Entity<VirtualizerState>,
    builds: Rc<Cell<usize>>,
    children: usize,
    yue: bool,
}
impl Render for CardHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.builds.clone();
        let children = self.children;
        let render = move |item: yue_component::VirtualizerItem, _: &mut Window, _: &mut App| {
            builds.set(builds.get() + 1);
            div()
                .h(px(20.))
                .flex()
                .children((0..children).map(|n| div().child(format!("{}:{n}", item.index()))))
        };
        if self.yue {
            YueVirtualList::new("cards", &self.state, render)
                .scrollbar(false)
                .w(px(240.))
                .h(px(100.))
                .into_any_element()
        } else {
            VirtualList::new("cards", &self.state, render)
                .w(px(240.))
                .h(px(100.))
                .test_support()
                .into_any_element()
        }
    }
}

#[gpui_kit::test]
fn production_owned_paths_bound_builds_by_viewport_and_keep_semantics_in_adapters(
    cx: &mut TestAppContext,
) {
    for (kind, count) in [(0, 100_000), (0, 1_000_000), (1, 100_000), (2, 100_000)] {
        for children in [1, 20] {
            let builds = Rc::new(Cell::new(0));
            let (view, cx) = cx.add_window_view({
                let builds = builds.clone();
                move |_, cx| {
                    let sizing = match kind {
                        0 => VirtualizerSizing::fixed(px(20.)),
                        1 => VirtualizerSizing::known(vec![px(20.); count]),
                        _ => VirtualizerSizing::measured(px(20.)),
                    };
                    CardHarness {
                        state: cx.new(|_| {
                            VirtualizerState::new(Axis::Vertical, source(0, count), sizing).unwrap()
                        }),
                        builds,
                        children,
                        yue: kind == 0,
                    }
                }
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            assert!(
                builds.get() <= 12,
                "kind={kind} children={children} builds={}",
                builds.get()
            );
            let state = view.read_with(cx, |v, _| v.state.clone());
            let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
            assert_eq!(snapshot.visible_range(), 0..5);
            assert_eq!(snapshot.total_extent(), count as f64 * 20.);
            let observations = cx.update(|w, _| snapshots(w));
            let list = observations
                .iter()
                .find(|node| {
                    node.path().last()
                        == Some(
                            &(if kind == 0 {
                                (ElementId::from("cards"), "viewport").into()
                            } else {
                                ElementId::from("cards")
                            }),
                        )
                })
                .unwrap();
            assert_eq!(list.role(), if kind == 0 { Some(Role::List) } else { None });
            state.update(cx, |s, cx| {
                s.scroll_to_key(
                    (count - 1).into(),
                    VirtualizerScrollOptions::default().align(VirtualizerAlign::End),
                )
                .unwrap();
                cx.notify();
            });
            builds.set(0);
            cx.update(|w, cx| w.draw(cx).clear(cx));
            assert!(builds.get() <= 24);
            let snapshot = state.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
            assert!(snapshot.visible_range().contains(&(count - 1)));
            assert_eq!(snapshot.viewport().offset(), count as f64 * 20. - 100.);
        }
    }
}

/// A consumer-owned real scroll container. This fixture contains no range or
/// measurement algorithm: it consumes the same public snapshots as an app.
struct SharedHost {
    rows: Entity<VirtualizerState>,
    cols: Entity<VirtualizerState>,
    scroll: ScrollHandle,
    revision: Rc<Cell<u64>>,
    cells: Rc<Cell<usize>>,
    writes: Rc<Cell<usize>>,
    wheels: Rc<Cell<usize>>,
    measured: bool,
    width: Pixels,
}
impl SharedHost {
    fn new(measured: bool, known_rows: bool, cx: &mut Context<Self>) -> Self {
        let binding = VirtualizerBinding::new();
        let rows = cx.new(|_| {
            VirtualizerState::new_attached(
                Axis::Vertical,
                source(0, 10_000),
                if measured {
                    VirtualizerSizing::measured(px(20.))
                } else if known_rows {
                    VirtualizerSizing::known(vec![px(20.); 10_000])
                } else {
                    VirtualizerSizing::fixed(px(20.))
                },
                &binding,
            )
            .unwrap()
        });
        let cols = cx.new(|_| {
            VirtualizerState::new_attached(
                Axis::Horizontal,
                source(0, if measured { 1 } else { 10_000 }),
                VirtualizerSizing::known(vec![px(50.); if measured { 1 } else { 10_000 }]),
                &binding,
            )
            .unwrap()
        });
        Self {
            rows,
            cols,
            scroll: ScrollHandle::new(),
            revision: Rc::new(Cell::new(0)),
            cells: Rc::new(Cell::new(0)),
            writes: Rc::new(Cell::new(0)),
            wheels: Rc::new(Cell::new(0)),
            measured,
            width: px(200.),
        }
    }
}
impl Render for SharedHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let wheels = self.wheels.clone();
        let mut base = div()
            .id("shared-host")
            .w(self.width)
            .h(px(100.))
            .overflow_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(move |_, _, _| wheels.set(wheels.get() + 1));
        base.style().allow_concurrent_scroll = Some(true);
        SharedElement {
            base,
            rows: self.rows.clone(),
            cols: self.cols.clone(),
            scroll: self.scroll.clone(),
            revision: self.revision.clone(),
            cells: self.cells.clone(),
            writes: self.writes.clone(),
            measured: self.measured,
        }
    }
}
struct SharedElement {
    base: Stateful<Div>,
    rows: Entity<VirtualizerState>,
    cols: Entity<VirtualizerState>,
    scroll: ScrollHandle,
    revision: Rc<Cell<u64>>,
    cells: Rc<Cell<usize>>,
    writes: Rc<Cell<usize>>,
    measured: bool,
}
impl IntoElement for SharedElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
struct SharedFrame {
    cells: Vec<AnyElement>,
    batch: Option<ListViewportLayout>,
}
impl Element for SharedElement {
    type RequestLayoutState = SharedFrame;
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Some("shared-viewport".into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, SharedFrame) {
        let layout =
            self.base
                .interactivity()
                .request_layout(id, inspector, w, cx, |style, w, cx| {
                    w.request_layout(style, None, cx)
                });
        (
            layout,
            SharedFrame {
                cells: Vec::new(),
                batch: None,
            },
        )
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut SharedFrame,
        w: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        let revision = self.revision.get() + 1;
        self.revision.set(revision);
        let offset = self.scroll.offset();
        let rv = VirtualizerViewport::new(
            self.rows.read(cx).binding().unwrap(),
            revision,
            -f64::from(offset.y),
            bounds.size.height,
            bounds.size.width,
        )
        .input_revision(0);
        let cv = VirtualizerViewport::new(
            self.cols.read(cx).binding().unwrap(),
            revision,
            -f64::from(offset.x),
            bounds.size.width,
            bounds.size.height,
        )
        .input_revision(0);
        if self.measured {
            let narrow = bounds.size.width < px(180.);
            for _ in 0..2 {
                let (_, intent) = self
                    .rows
                    .update(cx, |state, _| state.prepare(&rv, Instant::now()))
                    .unwrap();
                let target = intent
                    .as_ref()
                    .map_or(rv.offset(), |request| request.offset());
                let snapshot = self
                    .rows
                    .read(cx)
                    .snapshot(&rv.clone().with_offset(target))
                    .unwrap();
                let batch = self
                    .rows
                    .update(cx, |rows, cx| {
                        rows.layout_measured(
                            &snapshot,
                            bounds,
                            move |index, _, _| {
                                div()
                                    .w_full()
                                    .min_h(px(if narrow { 40. } else { 20. }))
                                    .child(format!("Card {index}"))
                                    .into_any_element()
                            },
                            w,
                            cx,
                        )
                    })
                    .unwrap();
                let applied = f64::from(batch.offset());
                frame.batch = Some(batch);
                let (_, next) = self
                    .rows
                    .update(cx, |state, _| state.prepare(&rv, Instant::now()))
                    .unwrap();
                if next
                    .as_ref()
                    .is_none_or(|request| (request.offset() - applied).abs() <= 0.5)
                {
                    break;
                }
            }
        }
        let (_, rr) = self
            .rows
            .update(cx, |s, _| s.prepare(&rv, Instant::now()))
            .unwrap();
        let (_, cr) = self
            .cols
            .update(cx, |s, _| s.prepare(&cv, Instant::now()))
            .unwrap();
        let total = gpui_kit::size(
            Pixels::from(self.cols.read(cx).total_extent()).max(bounds.size.width),
            Pixels::from(self.rows.read(cx).total_extent()),
        );
        let x = cr
            .as_ref()
            .map_or(-f64::from(offset.x), |r| r.offset())
            .clamp(0., f64::from((total.width - bounds.size.width).max(px(0.))));
        let y = rr
            .as_ref()
            .map_or(-f64::from(offset.y), |r| r.offset())
            .clamp(
                0.,
                f64::from((total.height - bounds.size.height).max(px(0.))),
            );
        if cr.is_some() || rr.is_some() {
            self.scroll
                .set_offset(gpui_kit::point(Pixels::from(-x), Pixels::from(-y)));
            self.writes.set(self.writes.get() + 1);
        }
        let actual = self.scroll.offset();
        self.rows.update(cx, |s, _| {
            if let Some(r) = rr {
                s.confirm(&r, -f64::from(actual.y), &rv).unwrap();
            } else {
                s.commit(&rv.clone().with_offset(-f64::from(actual.y)))
                    .unwrap();
            }
        });
        self.cols.update(cx, |s, _| {
            if let Some(r) = cr {
                s.confirm(&r, -f64::from(actual.x), &cv).unwrap();
            } else {
                s.commit(&cv.clone().with_offset(-f64::from(actual.x)))
                    .unwrap();
            }
        });
        let rs = self
            .rows
            .read(cx)
            .snapshot(&rv.with_offset(-f64::from(actual.y)))
            .unwrap();
        let cs = self
            .cols
            .read(cx)
            .snapshot(&cv.with_offset(-f64::from(actual.x)))
            .unwrap();
        self.base.interactivity().prepaint(
            id,
            inspector,
            bounds,
            total,
            w,
            cx,
            |_, _, hitbox, w, cx| {
                if let Some(batch) = &mut frame.batch {
                    batch.prepaint(bounds, -actual.y, w, cx);
                } else {
                    w.with_content_mask(Some(ContentMask { bounds }), |w| {
                        for row in rs.rendered_range() {
                            for col in cs.rendered_range() {
                                let row = rs.item(row).unwrap();
                                let col = cs.item(col).unwrap();
                                let cell_id = ElementId::from(format!(
                                    "cell-{:?}-{:?}",
                                    row.key(),
                                    col.key()
                                ));
                                let mut cell = div()
                                    .id(cell_id)
                                    .role(Role::Cell)
                                    .w(col.extent())
                                    .h(row.extent())
                                    .test_support()
                                    .into_any_element();
                                cell.layout_as_root(
                                    gpui_kit::size(
                                        AvailableSpace::Definite(col.extent()),
                                        AvailableSpace::Definite(row.extent()),
                                    ),
                                    w,
                                    cx,
                                );
                                cell.prepaint_at(
                                    bounds.origin
                                        + gpui_kit::point(
                                            Pixels::from(col.start() + f64::from(actual.x)),
                                            Pixels::from(row.start() + f64::from(actual.y)),
                                        ),
                                    w,
                                    cx,
                                );
                                frame.cells.push(cell);
                                self.cells.set(self.cells.get() + 1);
                            }
                        }
                    });
                }
                hitbox
            },
        )
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut SharedFrame,
        hitbox: &mut Option<Hitbox>,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.base.interactivity().paint(
            id,
            inspector,
            bounds,
            hitbox.as_ref(),
            w,
            cx,
            |_, w, cx| {
                if let Some(batch) = &mut frame.batch {
                    batch.paint(bounds, w, cx);
                } else {
                    w.with_content_mask(Some(ContentMask { bounds }), |w| {
                        for cell in &mut frame.cells {
                            cell.paint(w, cx);
                        }
                    });
                }
            },
        );
    }
}

#[gpui_kit::test]
fn one_native_host_composes_axes_and_applies_simultaneous_requests_once(cx: &mut TestAppContext) {
    for known in [false, true] {
        let (host, cx) = cx.add_window_view(move |_, cx| SharedHost::new(false, known, cx));
        host.update(cx, |h, _| h.cells.set(0));
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(host.read_with(cx, |h, _| h.cells.get()), 20); // 5 rows x 4 columns
        let (rows, cols, scroll, writes) = host.read_with(cx, |h, _| {
            (
                h.rows.clone(),
                h.cols.clone(),
                h.scroll.clone(),
                h.writes.clone(),
            )
        });
        rows.update(cx, |s, _| {
            s.scroll_to_key(
                100usize.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
            )
            .unwrap();
        });
        cols.update(cx, |s, _| {
            s.scroll_to_key(
                200usize.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
            )
            .unwrap();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(writes.get(), 1);
        assert_eq!(scroll.offset(), gpui_kit::point(px(-10_000.), px(-2_000.)));
        let bounds = cx.update(|w, _| {
            snapshots(w)
                .into_iter()
                .filter(|n| n.role() == Some(Role::Cell))
                .map(|n| n.bounds())
                .collect::<Vec<_>>()
        });
        assert_eq!(bounds.len(), 20);
        assert!(bounds.iter().all(|b| b.left() >= px(0.)
            && b.right() <= px(200.)
            && b.top() >= px(0.)
            && b.bottom() <= px(100.)));
        cols.update(cx, |s, _| {
            s.scroll_to_key(
                210usize.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
            )
            .unwrap();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(scroll.offset().y, px(-2_000.));
        assert_eq!(scroll.offset().x, px(-10_500.));
        cx.simulate_event(gpui_kit::ScrollWheelEvent {
            position: gpui_kit::point(px(100.), px(50.)),
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-25.), px(-20.))),
            modifiers: Default::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(host.read_with(cx, |h, _| h.wheels.get()), 1);
        assert_eq!(scroll.offset(), gpui_kit::point(px(-10_525.), px(-2_020.)));
    }
}

#[gpui_kit::test]
fn measured_attached_uses_native_tree_resize_and_no_second_wheel_listener(cx: &mut TestAppContext) {
    let (host, cx) = cx.add_window_view(|_, cx| SharedHost::new(true, false, cx));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let rows = host.read_with(cx, |h, _| h.rows.clone());
    let old = rows.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert!(old.is_estimated());
    let old_extent = old.item(0).unwrap().extent();
    assert!(old_extent >= px(20.) && old_extent < px(40.));
    host.update(cx, |h, cx| {
        h.width = px(160.);
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let new = rows.read_with(cx, |s, _| s.snapshot(s.viewport().unwrap()).unwrap());
    assert_eq!(old.item(0).unwrap().extent(), old_extent);
    assert_eq!(new.item(0).unwrap().extent(), px(40.));
    assert!(new.is_estimated());
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: gpui_kit::point(px(80.), px(50.)),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-20.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(host.read_with(cx, |h, _| h.wheels.get()), 1);
    assert_eq!(host.read_with(cx, |h, _| h.scroll.offset().y), px(-20.));
}

#[gpui_kit::test]
fn native_estimate_survives_first_frame_splice_and_resize(cx: &mut TestAppContext) {
    struct Harness {
        list: ListState,
        width: Pixels,
    }
    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.list.clone(), |_, _, _| {
                div().h(px(20.)).into_any_element()
            })
            .w(self.width)
            .h(px(60.))
        }
    }
    let state =
        ListState::new(1_000, ListAlignment::Top, px(20.)).with_uniform_item_height(px(20.));
    let (view, cx) = cx.add_window_view({
        let list = state.clone();
        move |_, _| Harness {
            list,
            width: px(100.),
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(state.max_offset_for_scrollbar().y, px(19_940.));
    state.splice(1_000..1_000, 5);
    assert_eq!(state.max_offset_for_scrollbar().y, px(20_040.));
    view.update(cx, |view, cx| {
        view.width = px(200.);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(state.max_offset_for_scrollbar().y, px(20_040.));
}
