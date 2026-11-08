//! Presentation-neutral loading values, display gating and visible loop timing.
use crate::{A11yLive, TestSupportExt as _};
use gpui::{
    AnyElement, App, Bounds, Context, ElementId, Global, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Task, Window, accesskit, div,
};
use std::time::Duration;
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

/// A controlled percentage; non-finite inputs represent unknown progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressValue(Option<f32>);
impl ProgressValue {
    pub fn new(percent: f32) -> Self {
        Self(percent.is_finite().then(|| percent.clamp(0., 100.)))
    }
    pub const fn indeterminate() -> Self {
        Self(None)
    }
    pub const fn percentage(self) -> Option<f32> {
        self.0
    }
    pub const fn is_indeterminate(self) -> bool {
        self.0.is_none()
    }
}
impl Default for ProgressValue {
    fn default() -> Self {
        Self::new(0.)
    }
}

/// One cancellable display deadline. It neither runs nor extends business work.
#[derive(Default)]
pub struct LoadingDelay {
    active: bool,
    delay: Duration,
    visible: bool,
    task: Option<Task<()>>,
    generation: u64,
}
impl LoadingDelay {
    pub fn sync(&mut self, active: bool, delay: Duration, cx: &mut Context<Self>) -> bool {
        if self.active != active || self.delay != delay {
            self.task = None;
            self.generation = self.generation.wrapping_add(1);
            let generation = self.generation;
            self.active = active;
            self.delay = delay;
            self.visible = active && delay.is_zero();
            if active && !delay.is_zero() {
                self.task = Some(cx.spawn(async move |weak, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = weak.update(cx, |state, cx| {
                        if state.active && state.generation == generation {
                            state.visible = true;
                            cx.notify();
                        }
                    });
                }));
            }
        }
        self.visible
    }
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

struct LoadingEpoch(Instant);
impl Global for LoadingEpoch {}

/// Call from paint with the real bounds. All instances share one epoch; fully
/// clipped/hidden/inactive surfaces never request continuous animation frames.
/// Returning to an active window rejoins the shared phase rather than replaying.
pub fn loading_phase(
    bounds: Bounds<Pixels>,
    period: Duration,
    animated: bool,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    if !animated || cx.reduce_motion() || period.is_zero() {
        return 0.25;
    }
    let now = cx.background_executor().now();
    if !cx.has_global::<LoadingEpoch>() {
        cx.set_global(LoadingEpoch(now));
    }
    let clip = window.content_mask().bounds;
    if bounds.size.width > gpui::px(0.)
        && bounds.size.height > gpui::px(0.)
        && clip.size.width > gpui::px(0.)
        && clip.size.height > gpui::px(0.)
        && window.is_visible()
        && window.is_window_active()
        && bounds.right() > clip.left()
        && bounds.left() < clip.right()
        && bounds.bottom() > clip.top()
        && bounds.top() < clip.bottom()
    {
        window.request_animation_frame();
    }
    (now.saturating_duration_since(cx.global::<LoadingEpoch>().0)
        .as_secs_f64()
        % period.as_secs_f64()) as f32
        / period.as_secs_f32()
}

/// A replacement boundary. Only the active branch enters input and native trees.
#[derive(IntoElement)]
pub struct LoadingRegion {
    id: ElementId,
    loading: bool,
    label: SharedString,
    placeholder: Option<AnyElement>,
    content: Option<AnyElement>,
}
impl LoadingRegion {
    pub fn new(id: impl Into<ElementId>, loading: bool) -> Self {
        Self {
            id: id.into(),
            loading,
            label: "Loading content".into(),
            placeholder: None,
            content: None,
        }
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    pub fn placeholder(mut self, element: impl IntoElement) -> Self {
        self.placeholder = Some(element.into_any_element());
        self
    }
    pub fn content(mut self, element: impl IntoElement) -> Self {
        self.content = Some(element.into_any_element());
        self
    }
}
impl RenderOnce for LoadingRegion {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut root = div()
            .id(self.id.clone())
            .test_support()
            .role(Role::Group)
            .relative()
            .min_w_0()
            .min_h_0();
        if self.loading {
            root = root.children(self.placeholder).child(
                A11yLive::new(
                    div()
                        .id((self.id, "loading-status"))
                        .test_support()
                        .role(Role::Status)
                        .aria_label(self.label.clone())
                        .aria_value(self.label)
                        .absolute()
                        .size_0()
                        .overflow_hidden(),
                    accesskit::Live::Polite,
                )
                .atomic(true),
            );
        } else {
            root = root.children(self.content);
        }
        A11yLive::new(root, accesskit::Live::Off).busy(self.loading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, Element as _};
    #[test]
    fn finite_percentages_are_clamped_and_nonfinite_values_are_unknown() {
        for (input, expected) in [(-20., 0.), (0., 0.), (42., 42.), (120., 100.)] {
            assert_eq!(ProgressValue::new(input).percentage(), Some(expected));
        }
        for input in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(ProgressValue::new(input).is_indeterminate());
        }
    }

    #[gpui::test]
    fn delay_cancellation_and_reactivation_use_a_new_deadline(cx: &mut gpui::TestAppContext) {
        let gate = cx.new(|_| LoadingDelay::default());
        gate.update(cx, |s, cx| {
            assert!(!s.sync(true, Duration::from_millis(150), cx))
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        gate.update(cx, |s, cx| {
            assert!(!s.is_visible());
            s.sync(false, Duration::from_millis(150), cx);
            s.sync(true, Duration::from_millis(150), cx);
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(60));
        cx.run_until_parked();
        assert!(!gate.read_with(cx, |s, _| s.is_visible()));
        cx.executor().advance_clock(Duration::from_millis(90));
        cx.run_until_parked();
        assert!(gate.read_with(cx, |s, _| s.is_visible()));
        gate.update(cx, |s, cx| {
            assert!(!s.sync(false, Duration::from_millis(150), cx))
        });
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert!(!gate.read_with(cx, |s, _| s.is_visible()));
    }

    #[gpui::test]
    fn zero_delay_and_changed_delay_are_immediate_or_restart(cx: &mut gpui::TestAppContext) {
        let gate = cx.new(|_| LoadingDelay::default());
        gate.update(cx, |s, cx| assert!(s.sync(true, Duration::ZERO, cx)));
        gate.update(cx, |s, cx| {
            assert!(!s.sync(true, Duration::from_millis(100), cx))
        });
        cx.run_until_parked();
        gate.update(cx, |s, cx| assert!(s.sync(true, Duration::ZERO, cx)));
    }

    #[gpui::test]
    fn replacement_root_projects_native_busy_without_focus(cx: &mut gpui::TestAppContext) {
        cx.add_empty_window().update(|w, cx| {
            let mut node = accesskit::Node::new(Role::Group);
            LoadingRegion::new("region", true)
                .render(w, cx)
                .into_element()
                .write_a11y_info(&mut node);
            assert!(node.is_busy());
            assert_eq!(node.live(), Some(accesskit::Live::Off));
            LoadingRegion::new("region", false)
                .render(w, cx)
                .into_element()
                .write_a11y_info(&mut node);
            assert!(!node.is_busy());
        });
    }

    #[gpui::test]
    fn shared_phase_requests_frames_only_for_visible_active_moving_content(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{
            ContentMask, Render, VisualTestContext, WindowVisibility, canvas, point, px, size,
        };
        use std::{cell::Cell, rc::Rc};
        struct Probe {
            animated: bool,
            mask: u8,
            empty: bool,
            samples: Rc<Cell<[f32; 2]>>,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let animated = self.animated;
                let mask = self.mask;
                let empty = self.empty;
                let samples = self.samples.clone();
                div().size_full().child(
                    canvas(
                        |_, _, _| (),
                        move |mut b, _, w, cx| {
                            if empty {
                                b.size = size(px(0.), px(0.));
                            }
                            let sample = |w: &mut Window, cx: &mut App| {
                                let a = loading_phase(b, Duration::from_secs(1), animated, w, cx);
                                let z = loading_phase(b, Duration::from_secs(1), animated, w, cx);
                                samples.set([a, z]);
                            };
                            match mask {
                                1 => w.with_content_mask(
                                    Some(ContentMask {
                                        bounds: Bounds::new(
                                            b.origin + point(b.size.width + px(10.), px(0.)),
                                            size(px(20.), px(20.)),
                                        ),
                                    }),
                                    |w| sample(w, cx),
                                ),
                                2 => w.with_content_mask(
                                    Some(ContentMask {
                                        bounds: Bounds::new(b.origin, size(px(0.), px(0.))),
                                    }),
                                    |w| sample(w, cx),
                                ),
                                _ => sample(w, cx),
                            }
                        },
                    )
                    .size(px(40.)),
                )
            }
        }
        fn frame(cx: &mut VisualTestContext) {
            cx.run_until_parked();
            cx.update(|w, cx| {
                w.simulate_next_frame(cx);
                w.refresh();
                w.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
        fn pending(cx: &mut VisualTestContext) -> usize {
            cx.update(|w, cx| w.simulate_next_frame(cx))
        }
        let samples = Rc::new(Cell::new([0., 0.]));
        let (view, cx) = cx.add_window_view(|_, _| Probe {
            animated: true,
            mask: 0,
            empty: false,
            samples: samples.clone(),
        });
        cx.update(|w, _| w.activate_window());
        frame(cx);
        assert_eq!(samples.get()[0], samples.get()[1]);
        assert!(pending(cx) > 0);
        view.update(cx, |v, cx| {
            v.animated = false;
            cx.notify();
        });
        frame(cx);
        assert_eq!(pending(cx), 0);
        view.update(cx, |v, cx| {
            v.animated = true;
            v.empty = true;
            cx.notify();
        });
        frame(cx);
        assert_eq!(pending(cx), 0);
        for mask in [1, 2] {
            view.update(cx, |v, cx| {
                v.empty = false;
                v.mask = mask;
                cx.notify();
            });
            frame(cx);
            assert_eq!(pending(cx), 0);
        }
        view.update(cx, |v, cx| {
            v.mask = 0;
            cx.notify();
        });
        cx.update(|_, cx| cx.set_reduce_motion(true));
        frame(cx);
        assert_eq!(pending(cx), 0);
        cx.update(|_, cx| cx.set_reduce_motion(false));
        cx.deactivate_window();
        frame(cx);
        assert_eq!(pending(cx), 0);
        cx.update(|w, _| w.activate_window());
        cx.run_until_parked();
        cx.simulate_visibility_change(WindowVisibility::Hidden);
        frame(cx);
        assert_eq!(pending(cx), 0);
        cx.simulate_visibility_change(WindowVisibility::Visible);
        cx.executor().advance_clock(Duration::from_millis(250));
        frame(cx);
        assert!((samples.get()[0] - 0.25).abs() < 0.01);
        assert_eq!(samples.get()[0], samples.get()[1]);
        assert!(pending(cx) > 0);
    }
}
