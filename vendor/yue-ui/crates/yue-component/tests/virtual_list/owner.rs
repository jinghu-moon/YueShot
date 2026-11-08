//! Fourth-round owner regressions, through production public seams.
use gpui_kit::base::virtualization::VirtualList;
use gpui_kit::{
    AppContext as _, Axis, Context, ElementId, Entity, InputEvent, InteractiveElement, IntoElement,
    Pixels, Render, ScrollDelta, ScrollWheelEvent, Styled, TestAppContext, TouchPhase, Window, div,
    point, px, size,
};
use std::{cell::Cell, rc::Rc, time::Instant};
#[path = "owner_edges.rs"]
mod edges;
use yue_component::{
    VirtualMasonry, VirtualMasonryState, VirtualizerAlign, VirtualizerBinding, VirtualizerChange,
    VirtualizerLayout, VirtualizerSizeUpdate, VirtualizerSizing, VirtualizerSource,
    VirtualizerState, VirtualizerViewport,
};

struct Source(usize, u64);
impl VirtualizerSource for Source {
    fn len(&self) -> usize {
        self.0
    }
    fn key_at(&self, ix: usize) -> ElementId {
        ix.into()
    }
    fn revision_at(&self, _: usize) -> u64 {
        self.1
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        let ElementId::Integer(key) = key else {
            return None;
        };
        ((*key as usize) < self.0).then_some(*key as usize)
    }
}
fn source(count: usize) -> Rc<dyn VirtualizerSource> {
    Rc::new(Source(count, 0))
}
struct RangeSource {
    start: usize,
    count: usize,
}
impl VirtualizerSource for RangeSource {
    fn len(&self) -> usize {
        self.count
    }
    fn key_at(&self, ix: usize) -> ElementId {
        (self.start + ix).into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        let ElementId::Integer(key) = key else {
            return None;
        };
        let key = *key as usize;
        (self.start..self.start + self.count)
            .contains(&key)
            .then_some(key - self.start)
    }
}
fn range_source(start: usize, count: usize) -> Rc<dyn VirtualizerSource> {
    Rc::new(RangeSource { start, count })
}
fn sizing(mode: usize, count: usize) -> VirtualizerSizing {
    match mode {
        0 => VirtualizerSizing::fixed(px(20.)),
        1 => VirtualizerSizing::known(vec![px(20.); count]),
        _ => VirtualizerSizing::measured(px(20.)),
    }
}
fn input(state: &VirtualizerState, revision: u64, offset: f64) -> VirtualizerViewport {
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
fn anchor_blank_regions_observation() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 0..3 {
            for offset in [5., 25., 35.] {
                let mut state =
                    VirtualizerState::new(axis, source(100), sizing(mode, 100)).unwrap();
                state
                    .set_layout(
                        VirtualizerLayout::default()
                            .gap(px(10.))
                            .padding(px(10.), px(10.)),
                    )
                    .unwrap();
                state.commit(&input(&state, 1, offset)).unwrap();
                state
                    .replace_source(
                        source(101),
                        sizing(mode, 101),
                        VirtualizerChange::Splice {
                            old: 100..100,
                            inserted: 1,
                        },
                    )
                    .unwrap();
                let (_, request) = state
                    .prepare(&input(&state, 2, offset), Instant::now())
                    .unwrap();
                assert_eq!(request.as_ref().map_or(offset, |r| r.offset()), offset);
                println!(
                    "OWNER_ANCHOR,axis={axis:?},mode={mode},before={offset},after={}",
                    request.as_ref().map_or(offset, |r| r.offset())
                );
            }
        }
    }
}

#[test]
fn prepend_keeps_the_keyed_reading_anchor_on_both_axes_and_all_sizing_modes() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 0..3 {
            let sizing = |count| match mode {
                0 => VirtualizerSizing::fixed(px(20.)),
                1 => VirtualizerSizing::known(vec![px(20.); count]),
                _ => VirtualizerSizing::measured(px(20.)),
            };
            let mut state =
                VirtualizerState::new(axis, range_source(100, 100), sizing(100)).unwrap();
            let before =
                VirtualizerViewport::new(state.binding().unwrap(), 1, 400., px(100.), px(240.));
            state.commit(&before).unwrap();

            // The new source prepends keys 98 and 99 while preserving the old
            // key domain. The visible key 120 therefore moves from index 20
            // to index 22, and its physical reading position must remain fixed.
            state
                .replace_source(
                    range_source(98, 102),
                    sizing(102),
                    VirtualizerChange::Splice {
                        old: 0..0,
                        inserted: 2,
                    },
                )
                .unwrap();
            let after = before.with_revision(2);
            let (_, request) = state.prepare(&after, Instant::now()).unwrap();
            assert_eq!(
                request.as_ref().map_or(400., |request| request.offset()),
                440.,
                "prepend must compensate {axis:?}/{mode} by the inserted extent"
            );
        }
    }
}

#[test]
fn attached_outside_region_observation() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 0..3 {
            for offset in [0., 900.] {
                let binding = VirtualizerBinding::new();
                let mut state =
                    VirtualizerState::new_attached(axis, source(10), sizing(mode, 10), &binding)
                        .unwrap();
                let v = VirtualizerViewport::new(&binding, 1, offset, px(100.), px(240.))
                    .input_revision(0)
                    .with_content_origin(500.)
                    .with_scroll_extent(1500.);
                let before = state.snapshot(&v).unwrap().visible_range();
                assert!(before.is_empty());
                state.commit(&v).unwrap();
                state
                    .replace_source(
                        source(11),
                        sizing(mode, 11),
                        VirtualizerChange::Splice {
                            old: 10..10,
                            inserted: 1,
                        },
                    )
                    .unwrap();
                let v = v.with_revision(2).with_scroll_extent(1520.);
                let (_, request) = state.prepare(&v, Instant::now()).unwrap();
                assert!(
                    request.is_none(),
                    "offscreen list must not move host: {axis:?}/{mode}/{offset}"
                );
                println!(
                    "OWNER_OUTSIDE,axis={axis:?},mode={mode},before={offset},after={},visible_before={before:?}",
                    request.as_ref().map_or(offset, |r| r.offset())
                );
            }
        }
    }
}

#[test]
fn explicit_content_epoch_and_portable_cache_observation() {
    for bump_source in [false, true] {
        let mut state = VirtualizerState::new(Axis::Vertical, source(100), sizing(2, 100)).unwrap();
        let v = input(&state, 1, 0.);
        state.commit(&v).unwrap();
        state
            .update_sizes(&[VirtualizerSizeUpdate::new(0usize, px(60.))])
            .unwrap();
        let cache = state.measurement_cache().unwrap();
        state.invalidate_items(0..1).unwrap();
        if bump_source {
            state
                .replace_source(
                    Rc::new(Source(100, 1)),
                    sizing(2, 100),
                    VirtualizerChange::Reorder,
                )
                .unwrap();
        }
        let before = state.snapshot(&v).unwrap().item(0).unwrap();
        let restored = state.restore_measurement_cache(&cache).unwrap();
        let after = state.snapshot(&v).unwrap().item(0).unwrap();
        assert_eq!(restored, usize::from(!bump_source));
        assert!(!before.is_measured());
        assert_eq!(after.is_measured(), !bump_source);
        println!(
            "OWNER_CACHE,bump_source={bump_source},before_measured={},restored={restored},after_measured={},after_extent={}",
            before.is_measured(),
            after.is_measured(),
            f64::from(after.extent())
        );
    }
}

struct PrecisionHarness {
    state: Entity<VirtualizerState>,
}
impl Render for PrecisionHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        VirtualList::new("owner-precision", &self.state, |_, _, _| {
            div().w(px(20.)).h(px(20.))
        })
        .w(px(240.))
        .h(px(100.))
    }
}
fn actual(state: &Entity<VirtualizerState>, cx: &gpui_kit::VisualTestContext, axis: Axis) -> f64 {
    let _ = axis;
    state.read_with(cx, |s, _| s.viewport().unwrap().offset())
}
#[gpui_kit::test]
fn native_small_delta_precision_observation(cx: &mut TestAppContext) {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 0..2 {
            let (view, cx) = cx.add_window_view(|_, cx| PrecisionHarness {
                state: cx.new(|_| {
                    VirtualizerState::new(axis, source(1_000_000), sizing(mode, 1_000_000)).unwrap()
                }),
            });
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let state = view.read_with(cx, |v, _| v.state.clone());
            for (base, delta, repeats) in [
                (1000., 0.25, 1),
                (16_777_216., 0.25, 1),
                (16_777_216., 1., 1),
                (16_777_216., 2., 1),
                (16_777_216., 0.25, 8),
            ] {
                state.update(cx, |s, cx| {
                    s.native_scroll_handle()
                        .unwrap()
                        .set_offset(if axis == Axis::Vertical {
                            point(px(0.), Pixels::from(-base))
                        } else {
                            point(Pixels::from(-base), px(0.))
                        });
                    cx.notify();
                });
                cx.update(|w, cx| {
                    w.refresh();
                    w.draw(cx).clear(cx);
                });
                let before = actual(&state, cx, axis);
                for _ in 0..repeats {
                    cx.simulate_event(ScrollWheelEvent {
                        position: point(px(100.), px(50.)),
                        delta: ScrollDelta::Pixels(if axis == Axis::Vertical {
                            point(px(0.), px(-delta))
                        } else {
                            point(px(-delta), px(0.))
                        }),
                        modifiers: Default::default(),
                        touch_phase: TouchPhase::Moved,
                    });
                    cx.update(|w, cx| w.draw(cx).clear(cx));
                }
                let after = actual(&state, cx, axis);
                assert_eq!(after - before, f64::from(delta) * f64::from(repeats));
                println!(
                    "OWNER_PRECISION,axis={axis:?},mode={mode},base={base},requested_delta={delta},repeats={repeats},observed_delta={}",
                    after - before
                );
            }
        }
    }
}

struct MasonryHarness {
    state: Entity<VirtualMasonryState>,
    wheel_events: Rc<Cell<usize>>,
    height: f32,
}
impl Render for MasonryHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let height = self.height;
        let wheel_events = self.wheel_events.clone();
        VirtualMasonry::new("owner-masonry", &self.state, move |_, _, _| {
            div().h(px(height))
        })
        .w(px(300.))
        .h(px(100.))
        .on_scroll_wheel(move |_, _, _| wheel_events.set(wheel_events.get() + 1))
    }
}
fn dispatch_masonry_wheel(
    state: &Entity<VirtualMasonryState>,
    cx: &mut gpui_kit::VisualTestContext,
) -> f64 {
    let after_dispatch = cx.update(|w, cx| {
        w.dispatch_event(
            ScrollWheelEvent {
                position: point(px(100.), px(50.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-20.))),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
            }
            .to_platform_input(),
            cx,
        );
        -f64::from(state.read(cx).scroll_handle().offset().y)
    });
    cx.background_executor.run_until_parked();
    after_dispatch
}
#[gpui_kit::test]
fn masonry_input_during_correction_observation(cx: &mut TestAppContext) {
    for height in [1., 40.] {
        let (view, cx) = cx.add_window_view(|_, cx| MasonryHarness {
            state: cx.new(|_| {
                let mut state = VirtualMasonryState::new(
                    source(10_000),
                    VirtualizerSizing::measured(px(40.)),
                    3,
                )
                .unwrap();
                state
                    .set_layout(VirtualizerLayout::default().measurement_budget(1), px(0.))
                    .unwrap();
                state
            }),
            wheel_events: Rc::new(Cell::new(0)),
            height,
        });
        let (state, wheel_events) =
            view.read_with(cx, |v, _| (v.state.clone(), v.wheel_events.clone()));
        state.update(cx, |s, cx| {
            s.scroll_to_key(500usize.into(), VirtualizerAlign::Start)
                .unwrap();
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let before = state.read_with(cx, |s, _| -f64::from(s.scroll_handle().offset().y));
        let after_dispatch = dispatch_masonry_wheel(&state, cx);
        assert_eq!(wheel_events.get(), 1);
        let after_input = state.read_with(cx, |s, _| -f64::from(s.scroll_handle().offset().y));
        let callbacks = cx.update(|w, cx| {
            let n = w.simulate_next_frame(cx);
            w.draw(cx).clear(cx);
            n
        });
        let (after_frame, target) = state.read_with(cx, |s, _| {
            (
                -f64::from(s.scroll_handle().offset().y),
                s.snapshot(0., size(px(300.), px(100.)))
                    .unwrap()
                    .item(500)
                    .unwrap()
                    .item()
                    .start(),
            )
        });
        assert_eq!(after_dispatch, before + 20.);
        assert_eq!(after_input, after_dispatch);
        assert_eq!(after_frame, after_dispatch);
        assert!(!state.read_with(cx, |s, _| s.has_scroll_command()));
        println!(
            "OWNER_MASONRY_INPUT,height={height},before={before},after_dispatch={after_dispatch},after_input={after_input},after_frame={after_frame},target_start={target},callbacks={callbacks},wheel_events={}",
            wheel_events.get()
        );
        // A direct native-owner displacement isolates input delivery from replay.
        state.update(cx, |s, _| {
            s.scroll_handle()
                .set_offset(point(px(0.), Pixels::from(-(after_frame + 20.))))
        });
        let displaced = state.read_with(cx, |s, _| -f64::from(s.scroll_handle().offset().y));
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        let after_draw = state.read_with(cx, |s, _| -f64::from(s.scroll_handle().offset().y));
        assert_eq!(after_draw, displaced);
        println!(
            "OWNER_MASONRY_OWNER,height={height},displaced={displaced},after_draw={after_draw}"
        );
    }
}

#[gpui_kit::test]
fn masonry_plain_wheel_observation(cx: &mut TestAppContext) {
    for mode in 0..3 {
        let (view, cx) = cx.add_window_view(|_, cx| MasonryHarness {
            state: cx.new(|_| {
                let sizing = match mode {
                    0 => VirtualizerSizing::fixed(px(40.)),
                    1 => VirtualizerSizing::known(vec![px(40.); 10_000]),
                    _ => VirtualizerSizing::measured(px(40.)),
                };
                VirtualMasonryState::new(source(10_000), sizing, 3).unwrap()
            }),
            wheel_events: Rc::new(Cell::new(0)),
            height: 40.,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let (state, wheel_events) =
            view.read_with(cx, |v, _| (v.state.clone(), v.wheel_events.clone()));
        for step in 1..=4 {
            let before = state.read_with(cx, |s, _| -f64::from(s.scroll_handle().offset().y));
            let after_dispatch = dispatch_masonry_wheel(&state, cx);
            assert_eq!(wheel_events.get(), step);
            let after_input = state.read_with(cx, |s, _| -f64::from(s.scroll_handle().offset().y));
            cx.update(|w, cx| w.draw(cx).clear(cx));
            let after_draw = state.read_with(cx, |s, _| -f64::from(s.scroll_handle().offset().y));
            assert_eq!(
                after_draw,
                before + 20.,
                "ordinary native wheel {mode}/{step}"
            );
            println!(
                "OWNER_MASONRY_WHEEL,mode={mode},step={step},before={before},after_dispatch={after_dispatch},after_input={after_input},after_draw={after_draw},wheel_events={}",
                wheel_events.get()
            );
        }
    }
}
