use super::color::{Color, ColorFormat, decimal, parse_hex, parse_number};
use gpui_kit::base::{
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState},
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable as _, Rgba,
    Subscription, Task, Window,
};

/// Real-time color changes and completed edits. External setters emit neither.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorPickerEvent {
    Change(Rgba),
    Commit(Rgba),
}
/// Internal activation, including deliberately confirming an unchanged color.
#[derive(Clone, Copy)]
pub(crate) struct ColorConfirmed;

/// Retain this entity in the host, as with InputState. A state has one editing panel.
pub struct YueColorPickerState {
    pub(super) color: Color,
    pub(super) hsv: [f64; 3],
    hsl_memory: Option<[f64; 3]>,
    pub(super) alpha: f64,
    pub(super) format: ColorFormat,
    recent: Vec<Rgba>,
    origin: Option<Color>,
    published: Color,
    pub(super) inputs: Vec<Entity<InputState>>,
    pub(super) invalid: [bool; 8],
    pub(super) editing: Option<usize>,
    pub(super) focus: FocusHandle,
    pub(super) hue_focus: FocusHandle,
    pub(super) alpha_focus: FocusHandle,
    pub(super) format_focus: FocusHandle,
    pub(super) copy_focus: FocusHandle,
    pub(super) hue: Entity<SliderState>,
    pub(super) alpha_slider: Entity<SliderState>,
    pub(super) enabled: bool,
    pub(super) dirty: bool,
    pub(super) copied: bool,
    pub(super) copy_task: Option<Task<()>>,
    pub(super) format_open: bool,
    pub(super) dragging_sv: bool,
    pub(super) keyboard_session: bool,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<ColorPickerEvent> for YueColorPickerState {}
impl EventEmitter<ColorConfirmed> for YueColorPickerState {}
impl YueColorPickerState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        super::panel::init(cx);
        let defaults = ["#000000", "0", "0", "0", "0", "0", "0", "100"];
        let inputs = defaults
            .into_iter()
            .map(|v| cx.new(|cx| InputState::new(window, cx).default_value(v)))
            .collect::<Vec<_>>();
        let hue = cx.new(|_| SliderState::new().min(0.).max(360.).step(1.));
        let alpha_slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        let mut subscriptions = vec![];
        for (ix, input) in inputs.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |state, input, event, window, cx| {
                    if !state.enabled {
                        return;
                    }
                    match event {
                        InputEvent::Focus => {
                            state.finish(false, cx);
                            state.editing = Some(ix);
                            state.begin();
                        }
                        InputEvent::Change => {
                            state.editing = Some(ix);
                            state.begin();
                            let text = input.read(cx).value().to_string();
                            state.edit(ix, &text, window, cx);
                        }
                        InputEvent::PressEnter { .. } => {
                            if !state.invalid[ix] {
                                state.finish(true, cx);
                                state.editing = None;
                                state.sync(window, cx);
                            }
                        }
                        InputEvent::Blur => {
                            state.finish(false, cx);
                            state.editing = None;
                            state.sync(window, cx);
                        }
                    }
                    cx.notify();
                },
            ));
        }
        for (is_alpha, slider) in [(false, &hue), (true, &alpha_slider)] {
            subscriptions.push(cx.subscribe_in(
                slider,
                window,
                move |state, _, event, window, cx| {
                    if !state.enabled {
                        return;
                    }
                    match event {
                        SliderEvent::Start(_) => {}
                        SliderEvent::Cancel(_) => {
                            state.origin = None;
                            cx.notify();
                        }
                        SliderEvent::Change(value) => {
                            state.begin();
                            if is_alpha {
                                state.alpha = value.end() as f64 / 100.;
                            } else {
                                state.hsv[0] = (value.end() as f64).rem_euclid(360.);
                            }
                            state.apply_coordinates(window, cx);
                        }
                        SliderEvent::Release(_) => {
                            if state.origin.is_some() {
                                state.finish(true, cx);
                            }
                        }
                    }
                },
            ));
            // Base's assistive Increment/Decrement changes SliderState silently.
            // Pointer Change arrives before its notify; synchronization therefore
            // already matches here. Only a genuinely silent value needs routing.
            subscriptions.push(
                cx.observe_in(slider, window, move |state, slider, window, cx| {
                    if !state.enabled || state.dirty {
                        return;
                    }
                    // Press/release can change thumb presentation without changing its value.
                    cx.notify();
                    let value = slider.read(cx).value().end() as f64;
                    let expected = if is_alpha {
                        state.alpha * 100.
                    } else {
                        state.hsv[0]
                    };
                    if (value - expected).abs() < 0.001 {
                        return;
                    }
                    state.begin();
                    if is_alpha {
                        state.alpha = (value / 100.).clamp(0., 1.);
                    } else {
                        state.hsv[0] = value.rem_euclid(360.);
                    }
                    state.apply_coordinates(window, cx);
                    state.finish(true, cx);
                }),
            );
        }
        let focus = cx.focus_handle();
        let hue_focus = cx.focus_handle();
        let alpha_focus = cx.focus_handle();
        for handle in [&focus, &hue_focus, &alpha_focus] {
            subscriptions.push(cx.on_blur(handle, window, |state, _, cx| {
                state.dragging_sv = false;
                state.finish(false, cx);
            }));
        }
        subscriptions.push(cx.observe_window_activation(window, |state, window, cx| {
            if !window.is_window_active() {
                state.cancel(cx);
                cx.notify();
            }
        }));
        Self {
            color: Color::BLACK,
            hsv: [0., 0., 0.],
            hsl_memory: None,
            alpha: 1.,
            format: ColorFormat::Hex,
            recent: vec![],
            origin: None,
            published: Color::BLACK,
            inputs,
            invalid: [false; 8],
            editing: None,
            focus,
            hue_focus,
            alpha_focus,
            format_focus: cx.focus_handle(),
            copy_focus: cx.focus_handle(),
            hue,
            alpha_slider,
            enabled: true,
            dirty: true,
            copied: false,
            copy_task: None,
            format_open: false,
            dragging_sv: false,
            keyboard_session: false,
            _subscriptions: subscriptions,
        }
    }
    /// # Panics
    /// If a channel is not finite or outside 0..=1.
    pub fn default_value(mut self, color: Rgba) -> Self {
        self.install(Color::from_rgba(color));
        self.published = self.color;
        self.dirty = true;
        self
    }
    pub fn default_format(mut self, format: ColorFormat) -> Self {
        self.format = format;
        self
    }
    pub fn value(&self) -> Rgba {
        self.color.rgba()
    }
    pub fn format(&self) -> ColorFormat {
        self.format
    }
    pub fn recent(&self) -> &[Rgba] {
        &self.recent
    }
    /// # Panics
    /// If a channel is not finite or outside 0..=1.
    pub fn set_value(&mut self, color: Rgba, window: &mut Window, cx: &mut Context<Self>) {
        let color = Color::from_rgba(color);
        self.cancel(cx);
        self.install(color);
        self.published = color;
        self.sync(window, cx);
        cx.notify();
    }
    pub fn set_format(&mut self, format: ColorFormat, window: &mut Window, cx: &mut Context<Self>) {
        if self.format == format {
            return;
        }
        self.finish(false, cx);
        self.editing = None;
        self.invalid = [false; 8];
        self.format = format;
        self.sync(window, cx);
        cx.notify();
    }
    /// Imports history silently, newest first. Invalid RGBA is a configuration error.
    /// # Panics
    /// If any supplied channel is not finite or outside 0..=1.
    pub fn set_recent(&mut self, colors: impl IntoIterator<Item = Rgba>, cx: &mut Context<Self>) {
        let mut keys = vec![];
        self.recent.clear();
        for c in colors {
            let key = Color::from_rgba(c);
            if !keys.contains(&key) && keys.len() < 8 {
                keys.push(key);
                self.recent.push(key.rgba());
            }
        }
        cx.notify();
    }
    pub(super) fn prepare(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.cancel(cx);
                self.sync(window, cx);
            }
        }
        if self.dirty {
            self.sync(window, cx);
            self.dirty = false;
        }
    }
    fn install(&mut self, c: Color) {
        self.color = c;
        self.hsv = c.hsv(self.hsv[0]);
        self.alpha = c.0[3] as f64 / 255.;
        self.hsl_memory = None;
    }
    pub(super) fn begin(&mut self) {
        if self.origin.is_none() {
            self.origin = Some(self.color);
        }
    }
    pub(super) fn finish(&mut self, explicit: bool, cx: &mut Context<Self>) {
        self.keyboard_session = false;
        let origin = self.origin.take();
        if origin.is_some_and(|c| c != self.color) {
            self.recent.retain(|c| Color::from_rgba(*c) != self.color);
            self.recent.insert(0, self.color.rgba());
            self.recent.truncate(8);
            cx.emit(ColorPickerEvent::Commit(self.color.rgba()));
        }
        if explicit {
            cx.emit(ColorConfirmed);
        }
        cx.notify();
    }
    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        for slider in [&self.hue, &self.alpha_slider] {
            slider.update(cx, |slider, _| slider.cancel_interaction());
        }
        self.dragging_sv = false;
        self.keyboard_session = false;
        self.origin = None;
        self.editing = None;
        self.invalid = [false; 8];
        self.copy_task = None;
        self.copied = false;
        self.hsl_memory = None;
        // Window deactivation and unmount do not have a Window to synchronize
        // immediately. Restore canonical drafts on the next prepare/render.
        self.dirty = true;
    }
    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish(false, cx);
        self.cancel(cx);
        self.format_open = false;
        self.sync(window, cx);
        cx.notify();
    }
    pub(super) fn select(&mut self, c: Color, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        self.finish(false, cx);
        self.editing = None;
        self.invalid = [false; 8];
        self.begin();
        self.install(c);
        self.changed(window, cx);
        self.finish(true, cx);
    }
    pub(super) fn apply_coordinates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.invalid = [false; 8];
        self.hsl_memory = None;
        self.color = Color::from_hsv(self.hsv, self.alpha);
        self.changed(window, cx);
    }
    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.published != self.color {
            self.published = self.color;
            cx.emit(ColorPickerEvent::Change(self.color.rgba()));
        }
        self.sync(window, cx);
        cx.notify();
    }
    pub(super) fn edit(
        &mut self,
        ix: usize,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = if ix == 0 {
            parse_hex(text)
        } else if ix == 7 {
            parse_number(text, 100., 2).map(|a| {
                let mut c = self.color;
                c.0[3] = (a / 100. * 255.).round() as u8;
                c
            })
        } else if ix <= 3 {
            parse_number(text, 255., 0).map(|v| {
                let mut c = self.color;
                c.0[ix - 1] = v as u8;
                c
            })
        } else {
            let mut hsl = self
                .hsl_memory
                .unwrap_or_else(|| self.color.hsl(self.hsv[0]));
            parse_number(text, if ix == 4 { 360. } else { 100. }, 2).map(|v| {
                hsl[ix - 4] = if ix == 4 {
                    v.rem_euclid(360.)
                } else {
                    v / 100.
                };
                self.hsl_memory = Some(hsl);
                Color::from_hsl(hsl, self.color.0[3] as f64 / 255.)
            })
        };
        self.invalid[ix] = next.is_none();
        if let Some(c) = next {
            if ix != 7 {
                let memory = self.hsl_memory;
                self.install(c);
                if ix >= 4 {
                    self.hsl_memory = memory;
                    self.hsv[0] = memory.unwrap()[0];
                }
            } else {
                self.color = c;
                self.alpha = c.0[3] as f64 / 255.;
            }
            self.changed(window, cx);
        }
        cx.notify();
    }
    pub(super) fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let [r, g, b, _] = self.color.0;
        let [h, s, l] = self
            .hsl_memory
            .unwrap_or_else(|| self.color.hsl(self.hsv[0]));
        let texts = [
            self.color.hex(),
            r.to_string(),
            g.to_string(),
            b.to_string(),
            decimal(h, 2),
            decimal(s * 100., 2),
            decimal(l * 100., 2),
            decimal(self.color.0[3] as f64 / 255. * 100., 2),
        ];
        for (ix, (input, text)) in self.inputs.iter().zip(texts).enumerate() {
            if self.editing == Some(ix) || self.invalid[ix] {
                continue;
            }
            if input.read(cx).value().as_str() != text {
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
        self.hue.update(cx, |slider, cx| {
            if (slider.value().end() - self.hsv[0] as f32).abs() > 0.001 {
                slider.set_value(self.hsv[0] as f32, window, cx);
            }
        });
        self.alpha_slider.update(cx, |slider, cx| {
            let a = self.alpha as f32 * 100.;
            if (slider.value().end() - a).abs() > 0.001 {
                slider.set_value(a, window, cx);
            }
        });
        self.dirty = false;
    }
    pub(super) fn fields(&self) -> Vec<usize> {
        match self.format {
            ColorFormat::Hex => vec![0, 7],
            ColorFormat::Rgb => vec![1, 2, 3, 7],
            ColorFormat::Hsl => vec![4, 5, 6, 7],
        }
    }
    pub(super) fn has_error(&self) -> bool {
        self.fields().iter().any(|ix| self.invalid[*ix])
    }
    pub(super) fn input_focus(&self, ix: usize, cx: &App) -> FocusHandle {
        self.inputs[ix].read(cx).focus_handle(cx)
    }
}

#[cfg(all(test, feature = "headless-tests"))]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;
    #[gpui_kit::test]
    fn cancellation_resynchronizes_invalid_text_before_the_next_frame(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let state = cx.new(|cx| YueColorPickerState::new(window, cx));
            state.update(cx, |s, cx| {
                s.prepare(true, window, cx);
                s.inputs[0].update(cx, |input, cx| input.set_value("#invalid", window, cx));
                s.editing = Some(0);
                s.edit(0, "#invalid", window, cx);
                assert!(s.invalid[0]);
                s.cancel(cx);
                s.prepare(true, window, cx);
                assert_eq!(s.inputs[0].read(cx).value().as_str(), "#000000");
                assert!(!s.has_error());
            });
        });
    }
    #[gpui_kit::test]
    fn endpoint_coordinates_survive_until_an_external_value_replaces_them(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let state = cx.new(|cx| YueColorPickerState::new(window, cx));
            state.update(cx, |s, cx| {
                s.hsv = [240., 0.8, 0.];
                s.apply_coordinates(window, cx);
                assert_eq!(s.color, Color::BLACK);
                assert_eq!(s.hsv, [240., 0.8, 0.]);
                s.hsv[2] = 1.;
                s.apply_coordinates(window, cx);
                assert_eq!(s.color, Color([51, 51, 255, 255]));
                s.set_value(gpui_kit::rgb(0), window, cx);
                assert_eq!(s.hsv, [240., 0., 0.]);
                s.edit(4, "240", window, cx);
                s.edit(5, "100", window, cx);
                s.edit(6, "100", window, cx);
                assert_eq!(s.color, Color([255, 255, 255, 255]));
                s.edit(6, "50", window, cx);
                assert_eq!(s.color, Color([0, 0, 255, 255]));
                s.set_value(gpui_kit::rgb(0), window, cx);
                assert!(s.hsl_memory.is_none());
            });
        });
    }
    #[gpui_kit::test]
    fn silent_slider_accessibility_path_reaches_the_color_without_feedback(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let cx = cx.add_empty_window();
        let state = cx.update(|window, cx| {
            cx.new(|cx| YueColorPickerState::new(window, cx).default_value(gpui_kit::rgb(0xFF0000)))
        });
        cx.update(|window, cx| state.update(cx, |s, cx| s.prepare(true, window, cx)));
        cx.update(|window, cx| {
            let hue = state.read(cx).hue.clone();
            hue.update(cx, |s, cx| s.set_value(120., window, cx));
        });
        cx.run_until_parked();
        state.read_with(cx, |s, _| {
            assert_eq!(s.value(), gpui_kit::rgb(0x00FF00));
            assert_eq!(s.recent(), &[gpui_kit::rgb(0x00FF00)]);
        });
        cx.update(|window, cx| {
            state.update(cx, |s, cx| {
                s.set_value(gpui_kit::rgba(0x3366CC80), window, cx)
            })
        });
        cx.run_until_parked();
        state.read_with(cx, |s, _| {
            assert_eq!(s.recent(), &[gpui_kit::rgb(0x00FF00)])
        });
    }
}
