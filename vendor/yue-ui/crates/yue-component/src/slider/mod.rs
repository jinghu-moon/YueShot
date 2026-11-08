//! Bounded numeric input with one Base value, fixed drag geometry and optional drafts.
mod element;
mod state;
use crate::{YueTheme, control_layout::ControlLayout};
pub use gpui_kit::base::slider::{SliderDomain, SliderValue};
use gpui_kit::{DefiniteLength, ElementId, Entity, IntoElement, SharedString};
pub use state::{YueSliderEvent, YueSliderState};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SliderSize {
    Small,
    #[default]
    Medium,
    Large,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SliderLayout {
    #[default]
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SliderTooltip {
    #[default]
    Auto,
    None,
}

/// Optional editing controls. A slider can display exactly one of these forms.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SliderControls {
    #[default]
    None,
    /// A compact numeric editor with decrement/increment chevrons.
    Input,
    /// Minus and plus buttons at the ends, followed by numeric editors without arrows.
    /// Range sliders adjust the last operated endpoint (upper initially).
    Buttons,
}

/// A display mark and, by default, a local pointer magnet.
#[derive(Clone, Debug)]
pub struct SliderMark {
    value: f32,
    label: SharedString,
    visible: bool,
}
impl SliderMark {
    /// # Panics
    /// Panics unless the mark value is finite.
    pub fn new(value: f32, label: impl Into<SharedString>) -> Self {
        assert!(value.is_finite(), "slider mark value must be finite");
        Self {
            value,
            label: label.into(),
            visible: true,
        }
    }
    pub fn label_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    pub fn label(&self) -> &SharedString {
        &self.label
    }
    pub fn is_label_visible(&self) -> bool {
        self.visible
    }
}

/// A token-styled slider. Retain the state and event subscriptions in the host.
///
/// ```no_run
/// use yue_component::{YueSlider, YueSliderState, SliderControls, SliderDomain, YueTheme, ThemeMode, Accent};
/// use gpui_kit::{AppContext as _, Context, Window};
/// fn create(window: &mut Window, cx: &mut Context<()>) {
///     let state = cx.new(|cx| YueSliderState::new(SliderDomain::new(0., 100.), 40., window, cx));
///     let control = YueSlider::new("volume", YueTheme::new(ThemeMode::Light, Accent::Azure), &state)
///         .accessibility_label("Volume").controls(SliderControls::Input);
/// }
/// ```
#[derive(IntoElement)]
pub struct YueSlider {
    id: ElementId,
    theme: YueTheme,
    default_theme: bool,
    state: Entity<YueSliderState>,
    size: SliderSize,
    direction: SliderLayout,
    marks: Vec<SliderMark>,
    snap_to_marks: bool,
    controls: SliderControls,
    tooltip: SliderTooltip,
    format: Rc<dyn Fn(f32) -> SharedString>,
    label: Option<SharedString>,
    description: Option<SharedString>,
    lower: Option<SharedString>,
    upper: Option<SharedString>,
    input_error: SharedString,
    tab_index: isize,
    tab_stop: bool,
    layout: ControlLayout,
    height: Option<DefiniteLength>,
    block: bool,
}
crate::control_layout::control_layout!(YueSlider);
impl YueSlider {
    pub fn new(id: impl Into<ElementId>, theme: YueTheme, state: &Entity<YueSliderState>) -> Self {
        Self {
            id: id.into(),
            theme,
            default_theme: false,
            state: state.clone(),
            size: SliderSize::Medium,
            direction: SliderLayout::Horizontal,
            marks: vec![],
            snap_to_marks: true,
            controls: SliderControls::None,
            tooltip: SliderTooltip::Auto,
            format: Rc::new(|v| state::numeric(v).into()),
            label: None,
            description: None,
            lower: None,
            upper: None,
            input_error: "Enter a valid number.".into(),
            tab_index: 0,
            tab_stop: true,
            layout: ControlLayout::default(),
            height: None,
            block: false,
        }
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.default_theme = false;
        self
    }
    pub fn use_default_theme(mut self) -> Self {
        self.default_theme = true;
        self
    }
    pub fn size(mut self, size: SliderSize) -> Self {
        self.size = size;
        self
    }
    pub fn layout(mut self, layout: SliderLayout) -> Self {
        self.direction = layout;
        self
    }
    pub fn height(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = Some(height.into());
        self
    }
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }
    /// Select one editing form; numeric inputs and end buttons never coexist.
    /// Changing form cancels the old gesture and discards unsubmitted drafts.
    pub fn controls(mut self, controls: SliderControls) -> Self {
        self.controls = controls;
        self
    }
    pub fn tooltip(mut self, tooltip: SliderTooltip) -> Self {
        self.tooltip = tooltip;
        self
    }
    pub fn value_format(mut self, format: impl Fn(f32) -> SharedString + 'static) -> Self {
        self.format = Rc::new(format);
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
    pub fn lower_label(mut self, label: impl Into<SharedString>) -> Self {
        self.lower = Some(label.into());
        self
    }
    pub fn upper_label(mut self, label: impl Into<SharedString>) -> Self {
        self.upper = Some(label.into());
        self
    }
    /// Localized error copy; the formatter never acts as a numeric parser.
    pub fn input_error_message(mut self, text: impl Into<SharedString>) -> Self {
        self.input_error = text.into();
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
    /// Marks locally attract nearby pointer edits by default. Set false for
    /// decorative marks. Keyboard and numeric edits always retain regular steps.
    pub fn snap_to_marks(mut self, enabled: bool) -> Self {
        self.snap_to_marks = enabled;
        self
    }
    /// Marks are exact pointer magnets within the token's small radius. Applying changed
    /// marks cancels any old gesture without moving the displayed value.
    /// # Panics
    /// Panics for duplicate mark values. Out-of-domain marks are not displayed.
    pub fn marks(mut self, marks: impl IntoIterator<Item = SliderMark>) -> Self {
        self.marks = marks.into_iter().collect();
        self.marks.sort_by(|a, b| a.value.total_cmp(&b.value));
        assert!(
            !self.marks.windows(2).any(|m| m[0].value == m[1].value),
            "slider marks must have unique values"
        );
        self
    }
}
