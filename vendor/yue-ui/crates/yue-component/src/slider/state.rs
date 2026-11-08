use super::{SliderControls, SliderDomain, SliderValue};
use gpui_kit::base::{
    NumberStep, StepAction,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState as BaseState},
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable as _, Subscription,
    Window,
};

/// Current-value notifications and completed user edits. External setters are silent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum YueSliderEvent {
    Change(SliderValue),
    Commit(SliderValue),
}

/// Retain one state per mounted Slider. Base owns the only numeric value;
/// these input strings are editing drafts, not another canonical value.
pub struct YueSliderState {
    pub(super) base: Entity<BaseState>,
    pub(super) inputs: Vec<Entity<InputState>>,
    pub(super) invalid: Vec<bool>,
    pub(super) focus: [FocusHandle; 2],
    pub(super) step_focus: Option<[FocusHandle; 2]>,
    controls: SliderControls,
    pub(super) disabled: bool,
    pub(super) animate_value: bool,
    pub(super) hovered: [bool; 2],
    pub(super) tooltip_closed: [bool; 2],
    pub(super) label_span: gpui_kit::Pixels,
    pub(super) label_axis: gpui_kit::Axis,
    editing: Option<usize>,
    origin: Option<SliderValue>,
    generation: u64,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<YueSliderEvent> for YueSliderState {}

impl YueSliderState {
    /// Construct a single or range slider. Mode is fixed by the initial value.
    /// # Panics
    /// Panics for non-finite values or an unrepresentable domain step, including
    /// an implicit default step of one. The domain constructor validates bounds.
    pub fn new(
        domain: SliderDomain,
        value: impl Into<SliderValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let value = domain.normalize(value);
        let base = cx.new(|_| BaseState::new().with_domain(domain).default_value(value));
        let values = if value.is_range() {
            vec![value.start(), value.end()]
        } else {
            vec![value.end()]
        };
        let inputs = values
            .into_iter()
            .map(|v| cx.new(|cx| InputState::new(window, cx).default_value(numeric(v))))
            .collect::<Vec<_>>();
        let mut subscriptions = vec![
            cx.subscribe_in(&base, window, |s, _, event, window, cx| {
                if s.disabled {
                    return;
                }
                match *event {
                    SliderEvent::Start(value) => {
                        if s.is_dragging(cx) || s.focus.iter().any(|f| f.is_focused(window)) {
                            s.animate_value = false;
                        }
                        s.generation = s.generation.wrapping_add(1);
                        s.origin = Some(value);
                        let editing_input = s.editing.is_some_and(|ix| {
                            s.inputs[ix].read(cx).focus_handle(cx).is_focused(window)
                        });
                        if !editing_input {
                            s.editing = None;
                            s.invalid.fill(false);
                        }
                        s.tooltip_closed.fill(false);
                        s.sync_inputs(window, cx);
                    }
                    SliderEvent::Change(value) => {
                        // An owner may replace the value synchronously in a callback.
                        if value != s.value(cx) {
                            return;
                        }
                        s.sync_inputs(window, cx);
                        cx.emit(YueSliderEvent::Change(value));
                    }
                    SliderEvent::Release(value) => {
                        let origin = s.origin.take();
                        if value == s.value(cx) && origin.is_some_and(|v| v != value) {
                            // Final Change and Release can share one Base update.
                            // Let the owner's Change callback run before publishing
                            // Commit, so synchronous replacement/disable invalidates it.
                            let generation = s.generation;
                            let state = cx.weak_entity();
                            cx.defer(move |cx| {
                                let _ = state.update(cx, |s, cx| {
                                    if s.generation == generation
                                        && !s.disabled
                                        && s.value(cx) == value
                                    {
                                        cx.emit(YueSliderEvent::Commit(value));
                                    }
                                });
                            });
                        }
                    }
                    SliderEvent::Cancel(_) => {
                        s.animate_value = false;
                        s.origin = None;
                        s.generation = s.generation.wrapping_add(1);
                    }
                }
                cx.notify();
            }),
            cx.observe_in(&base, window, |s, _, window, cx| {
                s.sync_inputs(window, cx);
                cx.notify();
            }),
        ];
        for (ix, input) in inputs.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |s, input, event, window, cx| {
                    if s.disabled {
                        return;
                    }
                    match event {
                        InputEvent::Focus => {
                            s.editing = Some(ix);
                        }
                        InputEvent::Change => {
                            if input.read(cx).focus_handle(cx).is_focused(window) {
                                s.editing = Some(ix);
                                let value = s.draft_value(ix, cx);
                                s.invalid[ix] = value.is_none();
                                if let Some(value) = value {
                                    s.animate_value = true;
                                    let start = s.is_range(cx) && ix == 0;
                                    s.base.update(cx, |base, cx| {
                                        base.set_user_value(value, start, window, cx);
                                    });
                                }
                            }
                        }
                        InputEvent::PressEnter { .. } => {
                            s.submit(ix, true, window, cx);
                        }
                        InputEvent::Blur => {
                            if s.editing == Some(ix) {
                                if s.is_dragging(cx) {
                                    // A pointer gesture already owns the Base transaction.
                                    // Releasing it here would drop capture on mouse-down.
                                    s.editing = None;
                                    s.invalid[ix] = false;
                                    s.sync_inputs(window, cx);
                                } else {
                                    s.submit(ix, false, window, cx);
                                }
                            }
                        }
                    }
                    cx.notify();
                },
            ));
        }
        let focus = [cx.focus_handle(), cx.focus_handle()];
        for (ix, handle) in focus.iter().enumerate() {
            subscriptions.push(cx.on_focus(handle, window, move |s, _, cx| {
                s.tooltip_closed[ix] = false;
                cx.notify();
            }));
            subscriptions.push(cx.on_blur(handle, window, |_, _, cx| cx.notify()));
        }
        subscriptions.push(cx.observe_window_activation(window, |s, window, cx| {
            if !window.is_window_active() {
                s.cancel(cx);
                s.tooltip_closed.fill(true);
                s.sync_inputs(window, cx);
                cx.notify();
            }
        }));
        let mut state = Self {
            base,
            invalid: vec![false; inputs.len()],
            inputs,
            focus,
            step_focus: None,
            controls: SliderControls::None,
            disabled: false,
            animate_value: false,
            hovered: [false; 2],
            tooltip_closed: [false; 2],
            label_span: gpui_kit::px(0.),
            label_axis: gpui_kit::Axis::Horizontal,
            editing: None,
            origin: None,
            generation: 0,
            _subscriptions: subscriptions,
        };
        state.sync_inputs(window, cx);
        state
    }

    pub fn value(&self, cx: &App) -> SliderValue {
        self.base.read(cx).value()
    }
    pub(super) fn configure_controls(
        &mut self,
        controls: SliderControls,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.controls == controls {
            return;
        }
        let focused_input = self
            .inputs
            .iter()
            .position(|input| input.read(cx).focus_handle(cx).is_focused(window));
        let focused_button = self
            .step_focus
            .as_ref()
            .is_some_and(|handles| handles.iter().any(|f| f.is_focused(window)));
        let restore = if self.controls != SliderControls::None && focused_input.is_some() {
            focused_input.map(|ix| if self.is_range(cx) { ix } else { 1 })
        } else if focused_button {
            Some(usize::from(self.base.read(cx).active_start() != Some(true)))
        } else {
            None
        };
        self.cancel(cx);
        self.controls = controls;
        if controls == SliderControls::Buttons && self.step_focus.is_none() {
            self.step_focus = Some([cx.focus_handle(), cx.focus_handle()]);
        }
        self.sync_inputs(window, cx);
        if !self.disabled
            && let Some(ix) = restore
        {
            self.focus[ix].focus(window, cx);
        }
        cx.notify();
    }
    pub(super) fn configure_marks(
        &mut self,
        points: impl IntoIterator<Item = f32>,
        distance: gpui_kit::Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.base
            .update(cx, |s, cx| s.set_snap_points(points, distance, cx));
        self.sync_inputs(window, cx);
    }
    pub fn domain(&self, cx: &App) -> SliderDomain {
        self.base.read(cx).domain()
    }
    pub fn is_range(&self, cx: &App) -> bool {
        self.value(cx).is_range()
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn is_dragging(&self, cx: &App) -> bool {
        self.base.read(cx).is_dragging()
    }
    /// The single-value thumb, also the upper endpoint in range mode.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus[1]
    }
    pub fn lower_focus_handle(&self) -> &FocusHandle {
        &self.focus[0]
    }
    pub fn upper_focus_handle(&self) -> &FocusHandle {
        &self.focus[1]
    }

    /// Silent, normalized replacement. Same-value echoes preserve a live drag and draft.
    /// # Panics
    /// Panics for non-finite input or a mode different from the initial value.
    pub fn set_value(
        &mut self,
        value: impl Into<SliderValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.domain(cx).clamp(value);
        assert_eq!(
            value.is_range(),
            self.is_range(cx),
            "slider value must preserve its single/range mode"
        );
        if value == self.value(cx) {
            return;
        }
        let value = self.base.read(cx).normalize_value(value);
        if value == self.value(cx) {
            return;
        }
        self.cancel(cx);
        self.base.update(cx, |s, cx| s.set_value(value, window, cx));
        self.sync_inputs(window, cx);
        cx.notify();
    }
    /// Silently replace the domain and normalize the current value atomically.
    /// # Panics
    /// Panics if the domain's implicit default step is not representable.
    pub fn set_domain(
        &mut self,
        domain: SliderDomain,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.domain(cx) == domain {
            return;
        }
        self.cancel(cx);
        self.base.update(cx, |s, cx| {
            s.set_domain(domain, window, cx);
            let value = s.normalize_value(s.value());
            s.set_value(value, window, cx);
        });
        self.sync_inputs(window, cx);
        cx.notify();
    }
    pub fn set_disabled(&mut self, disabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled == disabled {
            return;
        }
        self.cancel(cx);
        self.disabled = disabled;
        self.hovered.fill(false);
        self.tooltip_closed.fill(disabled);
        self.base.update(cx, |s, cx| s.set_disabled(disabled, cx));
        self.sync_inputs(window, cx);
        cx.notify();
    }
    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.animate_value = false;
        self.generation = self.generation.wrapping_add(1);
        self.origin = None;
        self.editing = None;
        self.invalid.fill(false);
        self.base.update(cx, |s, _| s.cancel_interaction());
    }
    pub(super) fn reset_draft(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.invalid[ix] = false;
        self.base.update(cx, |s, cx| s.handle_release(cx));
        self.sync_inputs(window, cx);
        cx.notify();
    }
    fn draft_value(&self, ix: usize, cx: &App) -> Option<f32> {
        let value = parse_draft(&self.inputs[ix].read(cx).value())?;
        let current = self.value(cx);
        let domain = self.domain(cx);
        let lo = if current.is_range() && ix == 1 {
            current.start()
        } else {
            domain.min()
        };
        let hi = if current.is_range() && ix == 0 {
            current.end()
        } else {
            domain.max()
        };
        (value >= lo && value <= hi).then_some(value)
    }
    fn submit(&mut self, ix: usize, enter: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing != Some(ix) {
            return;
        }
        // Base clamps numeric drafts on blur before emitting Blur. An invalid
        // draft must restore the last accepted value, not apply that silent clamp.
        if !enter && self.invalid[ix] {
            self.reset_draft(ix, window, cx);
            return;
        }
        let Some(value) = self.draft_value(ix, cx) else {
            if enter {
                self.invalid[ix] = true;
            } else {
                self.reset_draft(ix, window, cx);
            }
            return;
        };
        self.editing = None;
        self.invalid[ix] = false;
        let start = self.is_range(cx) && ix == 0;
        self.base.update(cx, |s, cx| {
            s.set_user_value(value, start, window, cx);
            s.handle_release(cx);
        });
        self.sync_inputs(window, cx);
    }
    pub(super) fn step_input(
        &mut self,
        ix: usize,
        action: StepAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let start = self.is_range(cx) && ix == 0;
        let current = if start {
            self.value(cx).start()
        } else {
            self.value(cx).end()
        };
        let text = self.inputs[ix].read(cx).value();
        let current = parse_draft(&text).unwrap_or(current);
        let input = self.inputs[ix].clone();
        input.update(cx, |s, cx| s.focus(window, cx));
        self.apply_step(ix, current, action, window, cx);
    }
    pub(super) fn step_target(&self, cx: &App) -> usize {
        usize::from(self.is_range(cx) && self.base.read(cx).active_start() != Some(true))
    }
    pub(super) fn can_step(&self, action: StepAction, cx: &App) -> bool {
        if self.disabled {
            return false;
        }
        let ix = self.step_target(cx);
        let value = self.value(cx);
        let current = if self.is_range(cx) && ix == 0 {
            value.start()
        } else {
            value.end()
        };
        let next = self
            .base
            .read(cx)
            .adjacent_value(current, action == StepAction::Increment);
        if self.is_range(cx) && ix == 0 {
            next.min(value.end()) != current
        } else if self.is_range(cx) {
            next.max(value.start()) != current
        } else {
            next != current
        }
    }
    pub(super) fn step_buttons(
        &mut self,
        action: StepAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.controls != SliderControls::Buttons || !self.can_step(action, cx) {
            return;
        }
        let ix = self.step_target(cx);
        let value = self.value(cx);
        let current = if self.is_range(cx) && ix == 0 {
            value.start()
        } else {
            value.end()
        };
        self.apply_step(ix, current, action, window, cx);
    }
    fn apply_step(
        &mut self,
        ix: usize,
        current: f32,
        action: StepAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let start = self.is_range(cx) && ix == 0;
        let next = self
            .base
            .read(cx)
            .adjacent_value(current, action == StepAction::Increment);
        self.animate_value = true;
        self.editing = None;
        self.invalid[ix] = false;
        self.base.update(cx, |s, cx| {
            s.set_user_value(next, start, window, cx);
            s.handle_release(cx);
        });
        self.sync_inputs(window, cx);
    }
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.value(cx);
        let domain = self.domain(cx);
        let step = Some(NumberStep::Fixed(domain.step_value() as f64));
        for (ix, input) in self.inputs.iter().enumerate() {
            let start = value.is_range() && ix == 0;
            let current = if start { value.start() } else { value.end() };
            let lo = if value.is_range() && !start {
                value.start()
            } else {
                domain.min()
            };
            let hi = if start { value.end() } else { domain.max() };
            let text = numeric(current);
            input.update(cx, |s, cx| {
                s.set_disabled(self.disabled, cx);
                s.set_min(Some(lo as f64), window, cx);
                s.set_max(Some(hi as f64), window, cx);
                s.set_step(step.clone(), window, cx);
                if self.editing != Some(ix) && s.value().as_ref() != text {
                    s.set_value(text, window, cx);
                }
            });
        }
    }
}

pub(super) fn numeric(value: f32) -> String {
    if value == 0. {
        "0".into()
    } else {
        value.to_string()
    }
}
pub(super) fn parse_draft(text: &str) -> Option<f32> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().any(|v| v.is_ascii_digit()) {
        return None;
    }
    if !text
        .bytes()
        .enumerate()
        .all(|(ix, v)| v.is_ascii_digit() || v == b'.' || (ix == 0 && matches!(v, b'+' | b'-')))
    {
        return None;
    }
    text.parse::<f32>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_drafts_do_not_accept_units_or_nonfinite_values() {
        for s in ["", "-", ".", "1.2.3", "NaN", "inf", "1e3", "20%", "1,200"] {
            assert_eq!(parse_draft(s), None, "{s}");
        }
        for (s, v) in [("-2.5", -2.5), (".5", 0.5), ("+12", 12.)] {
            assert_eq!(parse_draft(s), Some(v));
        }
    }
}
