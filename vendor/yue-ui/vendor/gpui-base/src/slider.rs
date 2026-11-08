//! Unstyled slider behavior. Pointer capture is window-local and independent of
//! GPUI drag-and-drop: moving outside the track must still update a held thumb.
mod domain;
pub use domain::SliderDomain;

use crate::{
    TestSupportExt as _, a11y_disabled::A11yDisabledExt as _, element_ext::ElementExt,
    geometry::AxisExt,
};
use gpui::{
    AccessibleAction, Along, AnyElement, App, Axis, Bounds, Context, Div, Entity, EventEmitter,
    FocusHandle, HitboxBehavior, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, Orientation, ParentElement, Pixels, Point, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, Subscription, TouchDragEvent, TouchPhase,
    Window, canvas, div, prelude::FluentBuilder as _, px,
};
use std::{collections::BTreeSet, ops::Range};

/// A user edit lifecycle. Setters are silent; a Start precedes any Change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderEvent {
    Start(SliderValue),
    Change(SliderValue),
    Release(SliderValue),
    Cancel(SliderValue),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderValue {
    Single(f32),
    Range(f32, f32),
}
impl std::fmt::Display for SliderValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single(v) => write!(f, "{v}"),
            Self::Range(a, b) => write!(f, "{a}..{b}"),
        }
    }
}
impl From<f32> for SliderValue {
    fn from(v: f32) -> Self {
        Self::Single(v)
    }
}
impl From<(f32, f32)> for SliderValue {
    fn from(v: (f32, f32)) -> Self {
        Self::Range(v.0, v.1)
    }
}
impl From<Range<f32>> for SliderValue {
    fn from(v: Range<f32>) -> Self {
        Self::Range(v.start, v.end)
    }
}
impl Default for SliderValue {
    fn default() -> Self {
        Self::Single(0.)
    }
}
impl SliderValue {
    pub fn clamp(self, min: f32, max: f32) -> Self {
        match self {
            Self::Single(v) => Self::Single(v.clamp(min, max)),
            Self::Range(a, b) => {
                let (a, b) = (a.clamp(min, max), b.clamp(min, max));
                Self::Range(a.min(b), a.max(b))
            }
        }
    }
    pub fn is_single(&self) -> bool {
        matches!(self, Self::Single(_))
    }
    pub fn is_range(&self) -> bool {
        matches!(self, Self::Range(..))
    }
    pub fn start(&self) -> f32 {
        match *self {
            Self::Single(v) | Self::Range(v, _) => v,
        }
    }
    pub fn end(&self) -> f32 {
        match *self {
            Self::Single(v) | Self::Range(_, v) => v,
        }
    }
    fn set_thumb(&mut self, value: f32, start: bool) {
        *self = match *self {
            Self::Single(_) => Self::Single(value),
            Self::Range(_, b) if start => Self::Range(value.min(b), b),
            Self::Range(a, _) => Self::Range(a, value.max(a)),
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliderScale {
    #[default]
    Linear,
    Logarithmic,
}
impl SliderScale {
    pub fn is_linear(&self) -> bool {
        *self == Self::Linear
    }
    pub fn is_logarithmic(&self) -> bool {
        *self == Self::Logarithmic
    }
}

#[derive(Clone, Copy)]
struct PointerDrag {
    axis: Axis,
    start: bool,
    offset: Pixels,
}
#[derive(Clone, Copy)]
struct TouchDrag {
    start_position: Point<Pixels>,
    start: bool,
    offset: Pixels,
}

/// The numeric value has exactly one owner. Programmatic values retain precision;
/// user operations snap to the validated domain. Retain this entity in the host.
pub struct SliderState {
    domain: SliderDomain,
    snap_points: Vec<f32>,
    snap_distance: Pixels,
    value: SliderValue,
    percentage: Range<f32>,
    bounds: Bounds<Pixels>,
    thumb_bounds: [Bounds<Pixels>; 2],
    scale: SliderScale,
    dragging: bool,
    editing: bool,
    interaction_cancelled: bool,
    pointer: Option<PointerDrag>,
    touch_drag: Option<TouchDrag>,
    held_keys: BTreeSet<String>,
    active: Option<bool>,
    enabled: bool,
    thumb_semantics: bool,
    focus: [Option<FocusHandle>; 2],
    focus_subscriptions: [Option<Subscription>; 2],
    activation: Option<Subscription>,
}
impl Default for SliderState {
    fn default() -> Self {
        Self::new()
    }
}
impl SliderState {
    pub fn new() -> Self {
        Self {
            domain: SliderDomain::default(),
            snap_points: Vec::new(),
            snap_distance: px(0.),
            value: SliderValue::default(),
            percentage: 0.0..0.0,
            bounds: Bounds::default(),
            thumb_bounds: [Bounds::default(); 2],
            scale: SliderScale::Linear,
            dragging: false,
            editing: false,
            interaction_cancelled: false,
            pointer: None,
            touch_drag: None,
            held_keys: BTreeSet::new(),
            active: None,
            enabled: true,
            thumb_semantics: false,
            focus: [None, None],
            focus_subscriptions: [None, None],
            activation: None,
        }
    }
    /// Configure all numeric bounds atomically.
    /// Panics for invalid logarithmic bounds or an unrepresentable default step.
    pub fn with_domain(mut self, domain: SliderDomain) -> Self {
        let domain = domain.step(domain.step_value());
        self.check_scale(domain);
        self.domain = domain;
        self.value = domain.clamp(self.value);
        self.update_thumb_pos();
        self
    }
    /// Panics for invalid bounds. Prefer `with_domain` to avoid builder ordering.
    pub fn min(self, min: f32) -> Self {
        let d = SliderDomain::new(min, self.domain.max()).step(self.domain.step_value());
        self.with_domain(d)
    }
    /// Panics for invalid bounds. Prefer `with_domain` to avoid builder ordering.
    pub fn max(self, max: f32) -> Self {
        let d = SliderDomain::new(self.domain.min(), max).step(self.domain.step_value());
        self.with_domain(d)
    }
    /// Panics for an invalid or unrepresentable step.
    pub fn step(mut self, step: f32) -> Self {
        self.domain = self.domain.step(step);
        self
    }
    /// Panics unless logarithmic bounds are strictly positive.
    pub fn scale(mut self, scale: SliderScale) -> Self {
        self.scale = scale;
        self.check_scale(self.domain);
        self.update_thumb_pos();
        self
    }
    fn check_scale(&self, domain: SliderDomain) {
        if self.scale.is_logarithmic() {
            assert!(
                domain.min() > 0.,
                "`min` must be greater than 0 for logarithmic scale"
            );
        }
    }
    /// Silent, clamped and ordered initial value. Panics for non-finite input.
    pub fn default_value(mut self, value: impl Into<SliderValue>) -> Self {
        self.value = self.domain.clamp(value);
        self.update_thumb_pos();
        self
    }
    /// Silent programmatic replacement; preserves sub-step color coordinates.
    /// Panics for non-finite input. Equal values do not notify or interrupt a drag.
    pub fn set_value(
        &mut self,
        value: impl Into<SliderValue>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.domain.clamp(value);
        if value == self.value {
            return;
        }
        self.value = value;
        self.update_thumb_pos();
        cx.notify();
    }
    /// Silent, atomic configuration replacement; cancels the old interaction.
    pub fn set_domain(&mut self, domain: SliderDomain, _: &mut Window, cx: &mut Context<Self>) {
        let domain = domain.step(domain.step_value());
        self.check_scale(domain);
        if self.domain == domain {
            return;
        }
        self.cancel_interaction();
        self.domain = domain;
        self.snap_points
            .retain(|v| *v >= domain.min() && *v <= domain.max());
        self.value = domain.clamp(self.value);
        self.update_thumb_pos();
        cx.notify();
    }
    pub fn domain(&self) -> SliderDomain {
        self.domain
    }
    pub fn value(&self) -> SliderValue {
        self.value
    }
    pub fn min_value(&self) -> f32 {
        self.domain.min()
    }
    pub fn max_value(&self) -> f32 {
        self.domain.max()
    }
    pub fn step_value(&self) -> f32 {
        self.domain.step_value()
    }
    /// Whether pointer edits can magnetize to nearby marks.
    pub fn has_snap_points(&self) -> bool {
        !self.snap_points.is_empty()
    }
    /// Configure local pointer magnets without changing the current value.
    /// A changed configuration cancels any old edit. Keyboard and numeric edits
    /// retain the domain step. Distance is measured along the painted track.
    /// # Panics
    /// Panics if a point or distance is non-finite, or distance is negative.
    pub fn set_snap_points(
        &mut self,
        points: impl IntoIterator<Item = f32>,
        distance: Pixels,
        cx: &mut Context<Self>,
    ) {
        let mut points: Vec<_> = points.into_iter().collect();
        assert!(
            points.iter().all(|v| v.is_finite()),
            "slider snap points must be finite"
        );
        assert!(
            f32::from(distance).is_finite() && distance >= px(0.),
            "slider snap distance must be finite and non-negative"
        );
        points.retain(|v| *v >= self.min_value() && *v <= self.max_value());
        points.sort_by(f32::total_cmp);
        points.dedup();
        if points == self.snap_points && distance == self.snap_distance {
            return;
        }
        self.cancel_user(cx);
        self.snap_points = points;
        self.snap_distance = distance;
        cx.notify();
    }
    /// Normalize an owner-supplied value using the active user snapping policy.
    /// Exact mark values retain precision; all other values use the step grid.
    /// # Panics
    /// Panics for non-finite input.
    pub fn normalize_value(&self, value: impl Into<SliderValue>) -> SliderValue {
        let value = self.domain.clamp(value);
        match value {
            SliderValue::Single(v) => {
                SliderValue::Single(self.snap(v, self.min_value(), self.max_value()))
            }
            SliderValue::Range(a, b) => SliderValue::Range(
                self.snap(a, self.min_value(), self.max_value()),
                self.snap(b, self.min_value(), self.max_value()),
            ),
        }
    }
    fn snap(&self, value: f32, lo: f32, hi: f32) -> f32 {
        assert!(value.is_finite(), "slider value must be finite");
        let value = value.clamp(lo, hi);
        if self.snap_points.contains(&value) {
            value
        } else {
            self.domain.snap(value).clamp(lo, hi)
        }
    }
    /// Attract only within a small screen-space neighborhood. Limit each
    /// neighborhood to a quarter of the nearest mark gap, leaving free travel
    /// between dense marks. Test raw pointer coordinates before step rounding.
    fn pointer_value(&self, p: f32, length: Pixels) -> f32 {
        let mut nearest = None;
        for (ix, &value) in self.snap_points.iter().enumerate() {
            let mark = self.value_to_percentage(value);
            let distance = (p - mark).abs() * f32::from(length);
            let neighbor_gap = [ix.checked_sub(1), Some(ix + 1)]
                .into_iter()
                .flatten()
                .filter_map(|i| self.snap_points.get(i))
                .map(|v| (self.value_to_percentage(*v) - mark).abs() * f32::from(length))
                .fold(f32::INFINITY, f32::min);
            let radius = f32::from(self.snap_distance).min(neighbor_gap * 0.25);
            if distance <= radius && nearest.is_none_or(|(best, _)| distance <= best) {
                nearest = Some((distance, value));
            }
        }
        nearest.map_or_else(|| self.percentage_to_value(p), |(_, value)| value)
    }
    /// Next distinct domain step. This also drives composed numeric steppers.
    /// # Panics
    /// Panics for non-finite input.
    pub fn adjacent_value(&self, value: f32, increase: bool) -> f32 {
        self.domain.adjacent(value, increase)
    }
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
    pub fn active_start(&self) -> Option<bool> {
        self.active
    }
    pub fn is_interacting(&self) -> bool {
        self.editing
    }
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.enabled == !disabled {
            return;
        }
        self.enabled = !disabled;
        if disabled {
            self.cancel_user(cx);
        }
        cx.notify();
    }

    fn percentage_to_value(&self, p: f32) -> f32 {
        let (a, b, p) = (self.min_value() as f64, self.max_value() as f64, p as f64);
        match self.scale {
            SliderScale::Linear => (a + (b - a) * p) as f32,
            SliderScale::Logarithmic => (a * (b / a).powf(p)).clamp(a, b) as f32,
        }
    }
    fn value_to_percentage(&self, v: f32) -> f32 {
        let (a, b, v) = (self.min_value() as f64, self.max_value() as f64, v as f64);
        (match self.scale {
            SliderScale::Linear => (v - a) / (b - a),
            SliderScale::Logarithmic => (v / a).ln() / (b / a).ln(),
        })
        .clamp(0., 1.) as f32
    }
    fn update_thumb_pos(&mut self) {
        self.percentage = if self.value.is_single() {
            0.0..self.value_to_percentage(self.value.end())
        } else {
            self.value_to_percentage(self.value.start())..self.value_to_percentage(self.value.end())
        };
    }
    fn begin(&mut self, start: bool, cx: &mut Context<Self>) {
        self.active = Some(start);
        if !self.editing {
            self.editing = true;
            cx.emit(SliderEvent::Start(self.value));
        }
    }
    /// A user edit, shared by pointer, keyboard, accessibility and composed editors.
    /// Panics for non-finite input. Release separately to finish a transaction.
    pub fn set_user_value(
        &mut self,
        value: f32,
        start: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        let (lo, hi) = if self.value.is_single() {
            (self.min_value(), self.max_value())
        } else if start {
            (self.min_value(), self.value.end())
        } else {
            (self.value.start(), self.max_value())
        };
        let value = self.snap(value, lo, hi);
        self.begin(start, cx);
        let mut next = self.value;
        next.set_thumb(value, start);
        if next == self.value {
            return;
        }
        self.value = next;
        self.update_thumb_pos();
        cx.emit(SliderEvent::Change(next));
        cx.notify();
    }
    fn center_along(&self, axis: Axis, start: bool) -> Pixels {
        let p = if start {
            self.percentage.start
        } else {
            self.percentage.end
        };
        if axis.is_horizontal() {
            self.bounds.left() + self.bounds.size.width * p
        } else {
            self.bounds.bottom() - self.bounds.size.height * p
        }
    }
    // Presentation may move a thumb independently of the canonical value.
    // Capture must use the geometry the user actually grabbed.
    fn presented_center_along(&self, axis: Axis, start: bool) -> Pixels {
        let bounds = self.thumb_bounds[usize::from(!start)];
        if bounds.size.along(axis) > px(0.) {
            bounds.center().along(axis)
        } else {
            self.center_along(axis, start)
        }
    }
    fn focus_thumb(&self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(f) = &self.focus[usize::from(!start)] {
            f.focus(window, cx);
        }
    }
    fn press(
        &mut self,
        axis: Axis,
        position: Point<Pixels>,
        start: bool,
        thumb: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        self.interaction_cancelled = false;
        let center = self.presented_center_along(axis, start);
        let offset = if thumb {
            position.along(axis) - center
        } else {
            px(0.)
        };
        self.pointer = Some(PointerDrag {
            axis,
            start,
            offset,
        });
        self.dragging = true;
        self.begin(start, cx);
        self.focus_thumb(start, window, cx);
        if !thumb {
            self.update_value_by_position(axis, position, start, window, cx);
        } else if (center - self.center_along(axis, start)).abs() > px(0.5) {
            let mut presented = position;
            if axis.is_horizontal() {
                presented.x = center;
            } else {
                presented.y = center;
            }
            self.update_value_by_position(axis, presented, start, window, cx);
        }
        // Publish presentation only after Change has reached value subscribers.
        // Observers may otherwise mistake this user edit for a silent setter.
        cx.notify();
    }
    fn move_pointer(
        &mut self,
        mut position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.pointer else {
            return;
        };
        if drag.axis.is_horizontal() {
            position.x -= drag.offset;
        } else {
            position.y -= drag.offset;
        }
        self.update_value_by_position(drag.axis, position, drag.start, window, cx);
    }
    #[doc(hidden)]
    pub fn update_value_by_position(
        &mut self,
        axis: Axis,
        position: Point<Pixels>,
        start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.interaction_cancelled || !self.enabled {
            return;
        }
        let length = self.bounds.size.along(axis);
        if !f32::from(length).is_finite() || length <= px(0.) {
            return;
        }
        let distance = if axis.is_horizontal() {
            position.x - self.bounds.left()
        } else {
            self.bounds.bottom() - position.y
        };
        let p = (distance / length).clamp(0., 1.);
        if !p.is_finite() {
            return;
        }
        self.dragging = true;
        self.set_user_value(self.pointer_value(p, length), start, window, cx);
    }
    fn is_nearer_start(&self, axis: Axis, position: Point<Pixels>) -> bool {
        if self.value.is_single() {
            return false;
        }
        let distance = position.along(axis);
        let a = (distance - self.presented_center_along(axis, true)).abs();
        let b = (distance - self.presented_center_along(axis, false)).abs();
        if a == b {
            self.active.unwrap_or(false)
        } else {
            a < b
        }
    }
    #[doc(hidden)]
    pub fn handle_release(&mut self, cx: &mut Context<Self>) {
        let active = self.editing;
        self.dragging = false;
        self.editing = false;
        self.pointer = None;
        self.touch_drag = None;
        self.held_keys.clear();
        if active {
            cx.emit(SliderEvent::Release(self.value));
            cx.notify();
        }
    }
    /// Silent cancellation. Does not change any other control's application drag.
    pub fn cancel_interaction(&mut self) {
        self.dragging = false;
        self.editing = false;
        self.interaction_cancelled = true;
        self.pointer = None;
        self.touch_drag = None;
        self.held_keys.clear();
    }
    fn cancel_user(&mut self, cx: &mut Context<Self>) {
        let active = self.editing;
        self.cancel_interaction();
        if active {
            cx.emit(SliderEvent::Cancel(self.value));
            cx.notify();
        }
    }
    fn register_focus(
        &mut self,
        start: bool,
        focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = usize::from(!start);
        if self.focus[ix].as_ref() == Some(&focus) {
            return;
        }
        self.focus_subscriptions[ix] = Some(cx.on_blur(&focus, window, |state, _, cx| {
            if !state.held_keys.is_empty() {
                state.handle_release(cx);
            }
        }));
        self.focus[ix] = Some(focus);
    }
    fn key(&mut self, key: &str, start: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.enabled {
            return false;
        }
        let v = if start {
            self.value.start()
        } else {
            self.value.end()
        };
        let (lo, hi) = if self.value.is_single() {
            (self.min_value(), self.max_value())
        } else if start {
            (self.min_value(), self.value.end())
        } else {
            (self.value.start(), self.max_value())
        };
        let value = match key {
            "right" | "up" => self.adjacent_value(v, true),
            "left" | "down" => self.adjacent_value(v, false),
            "home" => lo,
            "end" => hi,
            "pageup" | "pagedown" => {
                let increase = key == "pageup";
                let span = (self.max_value() as f64 - self.min_value() as f64) * 0.1;
                let target = (v as f64 + if increase { span } else { -span })
                    .clamp(lo as f64, hi as f64) as f32;
                let next = self.snap(target, lo, hi);
                if next == v {
                    self.adjacent_value(v, increase)
                } else {
                    next
                }
            }
            _ => return false,
        };
        self.interaction_cancelled = false;
        self.held_keys.insert(key.to_owned());
        self.set_user_value(value.clamp(lo, hi), start, window, cx);
        true
    }
    fn discrete(
        &mut self,
        increase: bool,
        start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        let v = if start {
            self.value.start()
        } else {
            self.value.end()
        };
        self.set_user_value(self.adjacent_value(v, increase), start, window, cx);
        self.handle_release(cx);
    }
    #[doc(hidden)]
    pub fn percentage(&self) -> Range<f32> {
        self.percentage.clone()
    }
    #[doc(hidden)]
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }
    #[doc(hidden)]
    pub fn set_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.bounds = bounds;
    }
}
impl EventEmitter<SliderEvent> for SliderState {}

/// Behavior root; `thumb_semantics(true)` moves focus and a11y to each thumb.
#[derive(IntoElement)]
pub struct Slider {
    state: Entity<SliderState>,
    snap_points: Option<(Vec<f32>, Pixels)>,
    axis: Axis,
    disabled: bool,
    thumb_semantics: bool,
    accessibility_label: Option<SharedString>,
    focus_handle: Option<FocusHandle>,
    base: Div,
    children: Vec<AnyElement>,
}
impl Slider {
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            state: state.clone(),
            snap_points: None,
            axis: Axis::Horizontal,
            disabled: false,
            thumb_semantics: false,
            accessibility_label: None,
            focus_handle: None,
            base: div(),
            children: vec![],
        }
    }
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }
    pub fn vertical(mut self) -> Self {
        self.axis = Axis::Vertical;
        self
    }
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus_handle = Some(focus.clone());
        self
    }
    pub fn thumb_semantics(mut self, enabled: bool) -> Self {
        self.thumb_semantics = enabled;
        self
    }
    /// Attract the pointer within `distance` of a mark along the track.
    /// Empty points disable magnets. Out-of-domain points are ignored.
    /// Omitting this builder retains the state's current snapping policy.
    /// # Panics
    /// Panics if a point or distance is non-finite, or distance is negative.
    pub fn snap_points(mut self, points: impl IntoIterator<Item = f32>, distance: Pixels) -> Self {
        let points: Vec<_> = points.into_iter().collect();
        assert!(
            points.iter().all(|v| v.is_finite()),
            "slider snap points must be finite"
        );
        assert!(
            f32::from(distance).is_finite() && distance >= px(0.),
            "slider snap distance must be finite and non-negative"
        );
        self.snap_points = Some((points, distance));
        self
    }
}
impl ParentElement for Slider {
    fn extend(&mut self, e: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(e);
    }
}
impl Styled for Slider {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let per_thumb = self.thumb_semantics || self.state.read(cx).value.is_range();
        self.state.update(cx, |state, cx| {
            if let Some((points, distance)) = self.snap_points {
                state.set_snap_points(points, distance, cx);
            }
            state.thumb_semantics = per_thumb;
            if state.enabled == self.disabled {
                state.enabled = !self.disabled;
                if self.disabled {
                    state.cancel_user(cx);
                }
            }
            if let Some(focus) = self.focus_handle.clone() {
                state.register_focus(false, focus, window, cx);
            }
            if state.activation.is_none() {
                state.activation =
                    Some(cx.observe_window_activation(window, |state, window, cx| {
                        if !window.is_window_active() {
                            state.cancel_user(cx);
                        }
                    }));
            }
        });
        let state = self.state.read(cx);
        let axis = self.axis;
        let value = state.value.end();
        let domain = state.domain;
        let capture = self.state.clone();
        let pointer = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let moving = capture.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture || moving.read(cx).pointer.is_none() {
                        return;
                    }
                    moving.update(cx, |s, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            s.move_pointer(event.position, window, cx);
                        } else {
                            s.cancel_user(cx);
                        }
                    });
                });
                let release = capture.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture
                        || event.button != MouseButton::Left
                        || release.read(cx).pointer.is_none()
                    {
                        return;
                    }
                    release.update(cx, |s, cx| {
                        s.move_pointer(event.position, window, cx);
                        s.handle_release(cx);
                    });
                });
            },
        )
        .absolute()
        .inset_0();
        self.base
            .id(("slider", self.state.entity_id()))
            .test_support()
            .when(!per_thumb, |b| {
                b.role(Role::Slider)
                    .when_some(
                        (!self.disabled).then_some(self.focus_handle).flatten(),
                        |b, f| b.track_focus(&f),
                    )
                    .when_some(self.accessibility_label, |b, label| b.aria_label(label))
                    .aria_numeric_value(value as f64)
                    .aria_min_numeric_value(domain.min() as f64)
                    .aria_max_numeric_value(domain.max() as f64)
                    .aria_numeric_value_step(domain.step_value() as f64)
                    .aria_orientation(if axis.is_vertical() {
                        Orientation::Vertical
                    } else {
                        Orientation::Horizontal
                    })
                    .when(!self.disabled, |b| {
                        b.on_a11y_action(AccessibleAction::Increment, {
                            let s = self.state.clone();
                            move |_, w, cx| s.update(cx, |s, cx| s.discrete(true, false, w, cx))
                        })
                        .on_a11y_action(AccessibleAction::Decrement, {
                            let s = self.state.clone();
                            move |_, w, cx| s.update(cx, |s, cx| s.discrete(false, false, w, cx))
                        })
                    })
            })
            .children(self.children)
            .child(pointer)
            .a11y_disabled(self.disabled)
    }
}

#[derive(IntoElement)]
pub struct SliderTrack {
    state: Entity<SliderState>,
    axis: Axis,
    disabled: bool,
    base: Div,
    children: Vec<AnyElement>,
}
impl SliderTrack {
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            state: state.clone(),
            axis: Axis::Horizontal,
            disabled: false,
            base: div(),
            children: vec![],
        }
    }
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}
impl ParentElement for SliderTrack {
    fn extend(&mut self, e: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(e);
    }
}
impl Styled for SliderTrack {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl InteractiveElement for SliderTrack {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}
impl RenderOnce for SliderTrack {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let axis = self.axis;
        let touch = (!self.disabled).then(|| {
            let s = self.state.clone();
            canvas(
                |bounds, w, _| w.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |_, hitbox, w, _| {
                    let s = s.clone();
                    w.on_mouse_event(move |e: &TouchDragEvent, phase, w, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        s.update(cx, |s, cx| {
                            if e.phase == TouchPhase::Started {
                                if w.default_prevented()
                                    || !hitbox.is_hovered_at(e.start_position, w)
                                    || !s.enabled
                                {
                                    return;
                                }
                                s.cancel_interaction();
                                s.interaction_cancelled = false;
                                let start = s.is_nearer_start(axis, e.start_position);
                                let offset = if s.thumb_bounds[usize::from(!start)]
                                    .contains(&e.start_position)
                                {
                                    e.start_position.along(axis)
                                        - s.presented_center_along(axis, start)
                                } else {
                                    px(0.)
                                };
                                s.touch_drag = Some(TouchDrag {
                                    start_position: e.start_position,
                                    start,
                                    offset,
                                });
                                s.dragging = true;
                                s.begin(start, cx);
                                s.focus_thumb(start, w, cx);
                                w.prevent_default();
                            }
                            let Some(drag) = s.touch_drag else {
                                return;
                            };
                            if drag.start_position != e.start_position {
                                return;
                            }
                            cx.stop_propagation();
                            if e.phase == TouchPhase::Cancelled {
                                s.cancel_user(cx);
                                return;
                            }
                            let mut p = e.position;
                            if axis.is_horizontal() {
                                p.x -= drag.offset;
                            } else {
                                p.y -= drag.offset;
                            }
                            s.update_value_by_position(axis, p, drag.start, w, cx);
                            if e.phase == TouchPhase::Started {
                                cx.notify();
                            }
                            if e.phase == TouchPhase::Ended {
                                s.handle_release(cx);
                            }
                        });
                    });
                },
            )
            .absolute()
            .inset_0()
        });
        self.base
            .id("slider-bar-container")
            .test_support()
            .children(self.children)
            .children(touch)
            .when(!self.disabled, |b| {
                b.on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(&self.state, move |s, e: &gpui::MouseDownEvent, w, cx| {
                        let start = s.is_nearer_start(axis, e.position);
                        s.press(axis, e.position, start, false, w, cx);
                        cx.stop_propagation();
                    }),
                )
            })
    }
}

/// Measures the complete numeric mapping, never the changing selected fill.
#[derive(IntoElement)]
pub struct SliderIndicator {
    state: Entity<SliderState>,
    base: Div,
    children: Vec<AnyElement>,
}
impl SliderIndicator {
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            state: state.clone(),
            base: div(),
            children: vec![],
        }
    }
}
impl ParentElement for SliderIndicator {
    fn extend(&mut self, e: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(e);
    }
}
impl Styled for SliderIndicator {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl InteractiveElement for SliderIndicator {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}
impl StatefulInteractiveElement for SliderIndicator {}
impl RenderOnce for SliderIndicator {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.base
            .id("slider-bar")
            .children(self.children)
            .on_prepaint({
                let s = self.state;
                move |bounds, _, cx| {
                    if s.read(cx).bounds != bounds {
                        s.update(cx, |s, _| s.set_bounds(bounds));
                    }
                }
            })
    }
}

#[derive(IntoElement)]
pub struct SliderThumb {
    state: Entity<SliderState>,
    axis: Axis,
    start: bool,
    disabled: bool,
    base: Div,
    children: Vec<AnyElement>,
    focus: Option<FocusHandle>,
    label: Option<SharedString>,
    description: Option<SharedString>,
    value_text: Option<SharedString>,
    tab_index: isize,
    tab_stop: bool,
}
impl SliderThumb {
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            state: state.clone(),
            axis: Axis::Horizontal,
            start: false,
            disabled: false,
            base: div(),
            children: vec![],
            focus: None,
            label: None,
            description: None,
            value_text: None,
            tab_index: 0,
            tab_stop: true,
        }
    }
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
    pub fn start(mut self, start: bool) -> Self {
        self.start = start;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn accessibility_description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }
    pub fn value_text(mut self, text: impl Into<SharedString>) -> Self {
        self.value_text = Some(text.into());
        self
    }
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = index;
        self
    }
    pub fn tab_stop(mut self, stop: bool) -> Self {
        self.tab_stop = stop;
        self
    }
}
impl ParentElement for SliderThumb {
    fn extend(&mut self, e: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(e);
    }
}
impl Styled for SliderThumb {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl InteractiveElement for SliderThumb {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}
impl StatefulInteractiveElement for SliderThumb {}
impl RenderOnce for SliderThumb {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let start = self.start;
        let axis = self.axis;
        let sem = self.state.read(cx).thumb_semantics;
        let focus = if sem {
            Some(self.focus.unwrap_or_else(|| {
                window
                    .use_keyed_state(
                        (
                            gpui::ElementId::from(("slider-thumb-focus", self.state.entity_id())),
                            if start { "lower" } else { "upper" },
                        ),
                        cx,
                        |_, cx| cx.focus_handle(),
                    )
                    .read(cx)
                    .clone()
            }))
        } else {
            None
        };
        let focus = focus.map(|f| {
            f.tab_index(self.tab_index)
                .tab_stop(self.tab_stop && !self.disabled)
        });
        if let Some(f) = focus.clone() {
            self.state
                .update(cx, |s, cx| s.register_focus(start, f, window, cx));
        }
        let s = self.state.read(cx);
        let value = if start {
            s.value.start()
        } else {
            s.value.end()
        };
        let (lo, hi) = if s.value.is_single() {
            (s.min_value(), s.max_value())
        } else if start {
            (s.min_value(), s.value.end())
        } else {
            (s.value.start(), s.max_value())
        };
        let step = s.step_value();
        self.base
            .id(("slider-thumb", start as u32))
            .test_support()
            .when(sem, |b| {
                b.role(Role::Slider)
                    .aria_numeric_value(value as f64)
                    .aria_min_numeric_value(lo as f64)
                    .aria_max_numeric_value(hi as f64)
                    .aria_numeric_value_step(step as f64)
                    .aria_orientation(if axis.is_vertical() {
                        Orientation::Vertical
                    } else {
                        Orientation::Horizontal
                    })
                    .when_some(self.label, |b, label| b.aria_label(label))
                    .when_some(self.description, |b, v| b.aria_description(v))
                    .when_some(self.value_text, |b, v| b.aria_value(v))
                    .when_some((!self.disabled).then_some(focus).flatten(), |b, f| {
                        b.track_focus(&f)
                            .tab_index(self.tab_index)
                            .tab_stop(self.tab_stop)
                    })
                    .key_context("Slider")
                    .when(!self.disabled, |b| {
                        b.on_key_down({
                            let s = self.state.clone();
                            move |e, w, cx| {
                                let m = e.keystroke.modifiers;
                                if m.control || m.alt || m.platform {
                                    return;
                                }
                                if s.update(cx, |s, cx| s.key(&e.keystroke.key, start, w, cx)) {
                                    cx.stop_propagation();
                                    w.prevent_default();
                                }
                            }
                        })
                        .on_key_up({
                            let s = self.state.clone();
                            move |e, _, cx| {
                                s.update(cx, |s, cx| {
                                    if s.held_keys.remove(&e.keystroke.key)
                                        && s.held_keys.is_empty()
                                    {
                                        s.handle_release(cx);
                                    }
                                });
                            }
                        })
                        .on_a11y_action(AccessibleAction::Increment, {
                            let s = self.state.clone();
                            move |_, w, cx| s.update(cx, |s, cx| s.discrete(true, start, w, cx))
                        })
                        .on_a11y_action(AccessibleAction::Decrement, {
                            let s = self.state.clone();
                            move |_, w, cx| s.update(cx, |s, cx| s.discrete(false, start, w, cx))
                        })
                    })
            })
            .children(self.children)
            .on_prepaint({
                let s = self.state.clone();
                move |bounds, _, cx| {
                    if s.read(cx).thumb_bounds[usize::from(!start)] != bounds {
                        s.update(cx, |s, _| s.thumb_bounds[usize::from(!start)] = bounds);
                    }
                }
            })
            .when(!self.disabled, |b| {
                b.on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(&self.state, move |s, e: &gpui::MouseDownEvent, w, cx| {
                        let start = if s.value.is_range()
                            && s.thumb_bounds
                                .iter()
                                .all(|bounds| bounds.contains(&e.position))
                        {
                            s.is_nearer_start(axis, e.position)
                        } else {
                            start
                        };
                        s.press(axis, e.position, start, true, w, cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .a11y_disabled(self.disabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_preserve_exact_values_but_do_not_replace_numeric_steps() {
        let mut state = SliderState::new().step(3.);
        state.snap_points = vec![20., 37.];
        assert_eq!(state.normalize_value(20.), 20.0.into());
        assert_eq!(state.normalize_value(10.), 9.0.into());
        assert_eq!(state.normalize_value(28.5), 30.0.into());
        assert_eq!(state.normalize_value(68.5), 69.0.into());
        assert_eq!(state.normalize_value(-20.), 0.0.into());
        assert_eq!(state.normalize_value(120.), 100.0.into());
        assert_eq!(state.adjacent_value(20., true), 21.);
        assert_eq!(state.adjacent_value(37., false), 36.);
        assert_eq!(state.adjacent_value(100., true), 100.);
        assert_eq!(state.snap(90., 0., 37.), 37.);
        assert_eq!(state.snap(10., 37., 100.), 37.);
    }

    #[test]
    fn decimal_mark_midpoints_are_stable_and_plain_steps_are_unchanged() {
        let mut state = SliderState::new().with_domain(SliderDomain::new(0., 1.).step(0.1));
        state.snap_points = vec![0.3, 0.4];
        assert_eq!(state.normalize_value(0.35), 0.4.into());
        state.snap_points.clear();
        assert_eq!(state.normalize_value(0.26), 0.3.into());
        assert_eq!(state.adjacent_value(0.3, true), 0.4);
    }

    #[test]
    fn pointer_magnets_use_pixels_and_leave_gaps_between_dense_marks() {
        let mut state = SliderState::new();
        state.snap_points = vec![20., 21., 80.];
        state.snap_distance = px(4.);
        // Dense neighbors are 2px apart: each attraction radius is only 0.5px.
        assert_eq!(state.pointer_value(0.202, px(200.)), 20.);
        assert!((state.pointer_value(0.205, px(200.)) - 20.5).abs() < 0.001);
        assert_eq!(state.pointer_value(0.798, px(200.)), 80.);
        assert!((state.pointer_value(0.77, px(200.)) - 77.).abs() < 0.001);
        // The same pixel radius has a smaller numeric span on a wider track.
        assert_eq!(state.pointer_value(0.797, px(1000.)), 80.);
        assert!((state.pointer_value(0.795, px(1000.)) - 79.5).abs() < 0.001);
        state.snap_distance = px(0.);
        assert!(state.pointer_value(0.798, px(200.)) < 80.);
    }

    #[test]
    fn assigned_values_are_clamped_and_ranges_are_ordered() {
        let state = SliderState::new()
            .min(3.)
            .max(13.)
            .default_value((20., -4.));
        assert_eq!(state.value(), SliderValue::Range(3., 13.));
        assert_eq!(state.percentage(), 0.0..1.0);
        assert_eq!(
            SliderState::new().default_value(120.).value(),
            SliderValue::Single(100.)
        );
    }

    #[test]
    fn assigned_color_coordinates_keep_precision_between_steps() {
        let state = SliderState::new().max(360.).step(1.).default_value(123.456);
        assert_eq!(state.value(), SliderValue::Single(123.456));
    }

    #[test]
    #[should_panic(expected = "step")]
    fn non_positive_step_is_rejected() {
        let _ = SliderState::new().step(0.);
    }

    #[test]
    #[should_panic(expected = "finite")]
    fn non_finite_value_is_rejected() {
        let _ = SliderState::new().default_value(f32::NAN);
    }

    #[test]
    fn legacy_value_conversions_and_clamping_are_preserved() {
        assert_eq!(SliderValue::from(5.), SliderValue::Single(5.));
        assert_eq!(SliderValue::from((2., 8.)), SliderValue::Range(2., 8.));
        assert_eq!(SliderValue::from(2.0..8.0), SliderValue::Range(2., 8.));
        assert_eq!(
            SliderValue::Range(-1., 12.).clamp(0., 10.),
            SliderValue::Range(0., 10.)
        );
    }

    #[test]
    fn legacy_linear_state_keeps_percentage_and_range_ordering() {
        let state = SliderState::new()
            .min(0.)
            .max(200.)
            .default_value((50., 150.));
        assert_eq!(state.value(), SliderValue::Range(50., 150.));
        assert_eq!(state.percentage(), 0.25..0.75);
    }

    #[test]
    fn legacy_logarithmic_state_keeps_mapping() {
        let state = SliderState::new()
            .min(1.)
            .max(1000.)
            .scale(SliderScale::Logarithmic)
            .default_value(10.);
        let percentage = state.percentage().end;
        assert!((percentage - (1. / 3.)).abs() < 0.0001);
    }

    #[test]
    #[should_panic(expected = "`min` must be greater than 0")]
    fn legacy_logarithmic_validation_is_preserved() {
        let _ = SliderState::new().scale(SliderScale::Logarithmic);
    }

    /// A slider's bar beside a view that reads its state, under retained views.
    #[cfg(feature = "gpui-fast")]
    mod retained {
        use std::{cell::Cell, rc::Rc};

        use gpui::{
            AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
            TestAppContext, Window, div, px,
        };

        use super::super::{SliderIndicator, SliderState};

        /// A view that shows a slider's value, counting its renders.
        struct Readout {
            state: Entity<SliderState>,
            renders: Rc<Cell<usize>>,
        }

        impl Render for Readout {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.renders.set(self.renders.get() + 1);
                div().child(self.state.read(cx).value().to_string())
            }
        }

        /// A slider's bar in a view of its own.
        struct Bar {
            state: Entity<SliderState>,
        }

        impl Render for Bar {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .w(px(120.))
                    .h(px(24.))
                    .child(SliderIndicator::new(&self.state).size_full())
            }
        }

        /// The bar and the readout side by side.
        struct Pair {
            bar: Entity<Bar>,
            readout: Entity<Readout>,
        }

        impl Render for Pair {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .flex()
                    .child(self.bar.clone())
                    .child(self.readout.clone())
            }
        }

        /// Drawing a slider's bar again where it was changes nothing in the
        /// slider's state: a view beside it that reads the state is not built
        /// again for it.
        #[gpui::test]
        fn drawing_a_slider_again_leaves_its_readers_retained(cx: &mut TestAppContext) {
            let renders = Rc::new(Cell::new(0));
            let (view, cx) = cx.add_window_view({
                let renders = renders.clone();
                move |_, cx| {
                    let state = cx.new(|_| SliderState::new());
                    Pair {
                        bar: cx.new(|_| Bar {
                            state: state.clone(),
                        }),
                        readout: cx.new(|_| Readout { state, renders }),
                    }
                }
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let rendered = renders.get();
            for _ in 0..3 {
                view.update(cx, |pair, cx| pair.bar.update(cx, |_, cx| cx.notify()));
                cx.update(|window, cx| window.draw(cx).clear(cx));
            }
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(renders.get(), rendered, "the readout was built again");
        }
    }
}
