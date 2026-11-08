use super::{Instant, MotionStatus, Transition};
use gpui::{
    AnyElement, App, Bounds, ContentMask, Display, Element, ElementId, FlexDirection,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, Style,
    Styled as _, Window, div, px, relative,
};
#[cfg(not(feature = "gpui-fast"))]
use std::rc::Rc;
use std::time::Duration;

/// A measured disclosure with immediate descendant detachment on close.
///
/// Unlike `MotionReveal`, this element owns its pixel timeline: it lays the
/// child's natural height out in the same layout pass, starts a newly opened
/// panel at zero, and retains frozen paint commands for its closing frame. Pass
/// `None` for a closed child; no live descendants remain for input or a11y.
/// The Fast backend currently retains geometry only, without a paint snapshot.
pub struct MotionDisclosure {
    id: ElementId,
    open: bool,
    child: Option<AnyElement>,
    enter: Transition,
    exit: Transition,
    geometry: gpui::SharedString,
}

#[derive(Clone)]
struct DisclosureState {
    open: bool,
    natural: Pixels,
    current: Pixels,
    from: Pixels,
    target: Pixels,
    started: Instant,
    active: bool,
    width: Option<Pixels>,
    geometry: gpui::SharedString,
    settle: bool,
    opening: bool,
    reversing_factor: f32,
    policy: Transition,
    #[cfg(not(feature = "gpui-fast"))]
    snapshot: Option<Rc<gpui::PaintSnapshot>>,
}
impl DisclosureState {
    fn sample(&mut self, now: Instant) {
        if !self.active {
            return;
        }
        let (progress, status) = self.policy.progress(
            now.saturating_duration_since(self.started),
            self.policy.duration,
        );
        self.current = self.from + (self.target - self.from) * self.policy.sample(progress);
        self.active = matches!(status, MotionStatus::Running | MotionStatus::Delayed);
        if !self.active {
            self.current = self.target;
        }
    }
    fn retarget(&mut self, target: Pixels, policy: Transition, now: Instant) {
        let (progress, _) = self.policy.progress(
            now.saturating_duration_since(self.started),
            self.policy.duration,
        );
        let reversing_factor = if self.active && target == self.from {
            (self.policy.sample(progress) * self.reversing_factor + 1. - self.reversing_factor)
                .clamp(0., 1.)
        } else {
            1.
        };
        self.sample(now);
        self.from = self.current;
        self.target = target;
        self.started = now;
        self.active = (self.from - target).abs() > px(0.01) && !policy.duration.is_zero();
        self.reversing_factor = reversing_factor;
        self.policy = Transition {
            duration: policy.duration.mul_f64(f64::from(reversing_factor)),
            ..policy
        };
        if !self.active {
            self.current = target;
        }
    }
}

impl MotionDisclosure {
    pub fn new(id: impl Into<ElementId>, open: bool, child: Option<AnyElement>) -> Self {
        assert!(
            open || child.is_none(),
            "Closed disclosure must detach its descendants"
        );
        Self {
            id: id.into(),
            open,
            child: child.map(|child| {
                div()
                    .w_full()
                    .flex_shrink_0()
                    .child(child)
                    .into_any_element()
            }),
            enter: Transition::new(Duration::ZERO),
            exit: Transition::new(Duration::ZERO),
            geometry: "".into(),
        }
    }
    pub fn transitions(mut self, enter: Transition, exit: Transition) -> Self {
        self.enter = enter;
        self.exit = exit;
        self
    }
    /// Changes in rem, theme, size or other geometry policy snap to a new
    /// natural measurement. Ordinary content-height changes need no revision.
    pub fn geometry_key(mut self, key: impl Into<gpui::SharedString>) -> Self {
        self.geometry = key.into();
        self
    }
}
impl IntoElement for MotionDisclosure {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for MotionDisclosure {
    type RequestLayoutState = (Option<LayoutId>, Instant);
    type PrepaintState = bool;
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let now = cx.background_executor().now();
        let immediate = cx.reduce_motion() || !window.is_window_active() || !window.is_visible();
        let policy = if self.open {
            self.enter.clone()
        } else {
            self.exit.clone()
        };
        let height = window.with_element_state(
            global_id.expect("Disclosure requires an id"),
            |state: Option<DisclosureState>, _| {
                let mut state = state.unwrap_or_else(|| DisclosureState {
                    open: self.open,
                    natural: px(0.),
                    current: px(0.),
                    from: px(0.),
                    target: px(0.),
                    started: now,
                    active: false,
                    width: None,
                    geometry: self.geometry.clone(),
                    settle: true,
                    opening: false,
                    reversing_factor: 1.,
                    policy: policy.clone(),
                    #[cfg(not(feature = "gpui-fast"))]
                    snapshot: None,
                });
                state.sample(now);
                if state.geometry != self.geometry || immediate || policy.duration.is_zero() {
                    state.active = false;
                    state.settle = true;
                    state.geometry = self.geometry.clone();
                    #[cfg(not(feature = "gpui-fast"))]
                    {
                        state.snapshot = None;
                    }
                }
                if state.open != self.open {
                    state.open = self.open;
                    state.opening = self.open && !state.settle;
                    if !state.settle {
                        let target = if self.open { state.natural } else { px(0.) };
                        state.retarget(target, policy, now);
                        // A first expansion still needs a natural measurement,
                        // even when its previous cached height was zero.
                    }
                }
                let height = if state.settle || (!state.active && self.open && !state.opening) {
                    None
                } else {
                    Some(state.current)
                };
                let height = if !self.open && !state.active {
                    Some(px(0.))
                } else {
                    height
                };
                (height, state)
            },
        );
        let child = self.child.as_mut().map(|c| c.request_layout(window, cx));
        let mut style = Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..Default::default()
        };
        style.size.width = relative(1.).into();
        if let Some(height) = height {
            style.size.height = height.into();
        }
        (window.request_layout(style, child, cx), (child, now))
    }
    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let natural = layout.0.map(|id| window.layout_bounds(id).size.height);
        // Sampling and natural-height retargeting belong to one frame clock.
        // Measuring a costly subtree must not restart the transition later in
        // prepaint, or advance its starting value beyond the pixels laid out.
        let now = layout.1;
        let visible = window.content_mask().bounds.intersects(&Bounds::new(
            bounds.origin,
            gpui::size(bounds.size.width, bounds.size.height.max(px(1.))),
        ));
        let enabled = window.is_visible() && window.is_window_active() && !cx.reduce_motion();
        let (active, relayout) = window.with_element_state(
            global_id.expect("Disclosure requires an id"),
            |state: Option<DisclosureState>, _| {
                let mut state = state.expect("Disclosure layout must precede prepaint");
                let width_changed = state
                    .width
                    .is_some_and(|w| (w - bounds.size.width).abs() > px(0.5));
                state.width = Some(bounds.size.width);
                if width_changed || !enabled {
                    state.active = false;
                    state.settle = true;
                    #[cfg(not(feature = "gpui-fast"))]
                    {
                        state.snapshot = None;
                    }
                }
                if let Some(height) = natural {
                    if state.settle || (!state.active && state.open && !state.opening) {
                        state.current = height;
                        state.target = height;
                        state.active = false;
                    } else if (height - state.target).abs() > px(0.5) && state.open {
                        state.retarget(height, self.enter.clone(), now);
                    }
                    state.natural = height;
                    state.opening = false;
                }
                if state.settle {
                    state.current = if state.open { state.natural } else { px(0.) };
                    state.target = state.current;
                    state.settle = false;
                }
                let relayout =
                    !state.active && (bounds.size.height - state.current).abs() > px(0.5);
                #[cfg(not(feature = "gpui-fast"))]
                if !state.open && !state.active {
                    state.snapshot = None;
                }
                ((state.active && enabled && visible, relayout), state)
            },
        );
        if active || relayout {
            window.request_animation_frame();
        }
        let clip = natural.is_some_and(|h| h > bounds.size.height + px(0.5));
        if let Some(child) = self.child.as_mut() {
            if clip {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    child.prepaint(window, cx);
                });
            } else {
                child.prepaint(window, cx);
            }
        }
        clip
    }
    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        clip: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        #[cfg(feature = "gpui-fast")]
        let _ = global_id;
        if let Some(child) = self.child.as_mut() {
            let paint = |window: &mut Window| {
                if *clip {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        child.paint(window, cx)
                    });
                } else {
                    child.paint(window, cx);
                }
            };
            #[cfg(not(feature = "gpui-fast"))]
            if !self.exit.duration.is_zero() {
                let (_, snapshot) = window.capture_paint(bounds.origin, paint);
                window.with_element_state(
                    global_id.expect("Disclosure requires an id"),
                    |state: Option<DisclosureState>, _| {
                        let mut state = state.expect("Disclosure prepaint must precede paint");
                        state.snapshot = Some(Rc::new(snapshot));
                        ((), state)
                    },
                );
                return;
            }
            let mut paint = paint;
            paint(window);
        } else {
            #[cfg(not(feature = "gpui-fast"))]
            {
                let snapshot = window.with_element_state(
                    global_id.expect("Disclosure requires an id"),
                    |state: Option<DisclosureState>, _| {
                        let state = state.expect("Disclosure prepaint must precede paint");
                        let snapshot = state.snapshot.clone();
                        (snapshot, state)
                    },
                );
                if let Some(snapshot) = snapshot {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        window.paint_snapshot(&snapshot, bounds.origin);
                    });
                }
            }
        }
    }
}
