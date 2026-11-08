//! Yue radio: one controlled option and the group that owns mutual exclusion.
//!
//! The option keeps GPUI Base's activation, focus and accessibility behaviour and
//! adds Yue's appearance: either a circle that fills with the accent and grows a
//! centre dot, or a segment of the outlined row the docs call the Button
//! appearance. The group owns what Base leaves open — the selected value, the one
//! tab stop the whole set lives behind, and the arrow keys that move between
//! options.

mod element;

mod behavior;

use std::rc::Rc;

use gpui_kit::{App, ClickEvent, ElementId, FocusHandle, IntoElement, SharedString, Window};

use crate::{GroupDirection, YueTheme};

type ChangeHandler = Rc<dyn Fn(bool, &ClickEvent, &mut Window, &mut App)>;

type ValueHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// The keyed slot a Yue radio keeps its focus handle in, so the ring it draws and
/// the handle a group focuses are the same handle.
const FOCUS_SLOT: &str = "yue-radio-focus";

/// Three Yue radio size profiles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RadioSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// How an option presents itself. Both appearances carry the same roles, the same
/// keyboard and the same mutual exclusion; only the drawing changes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RadioAppearance {
    /// A circle and a label — the default, and what a form reads as a radio.
    #[default]
    Indicator,
    /// One segment of a continuous, outlined row of mutually exclusive short
    /// labels. A [`YueRadioGroup`] composes these; an option used on its own so
    /// styled draws the whole outline around itself.
    ///
    /// Long labels, many options, or a row that would have to wrap belong to the
    /// Indicator appearance or to a Select: a wrapped segment row no longer reads
    /// as one choice set.
    Button,
}

/// Where a segment sits inside its row. The row outlines itself once and clips
/// its own ends, so a segment only ever carries the divider it shares with the
/// neighbour before it — on the axis the row runs along.
#[derive(Clone, Copy, Debug)]
struct Segment {
    vertical: bool,
    first: bool,
}

/// A controlled Yue radio option. The caller owns the selected value.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueRadio};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueRadio::new("mode", theme, false).label("标准");
/// ```
#[derive(IntoElement)]
pub struct YueRadio {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    checked: bool,
    disabled: bool,
    size: RadioSize,
    appearance: RadioAppearance,
    segment: Option<Segment>,
    label: Option<SharedString>,
    accessibility_label: Option<SharedString>,
    focus_handle: Option<FocusHandle>,
    tab_stop: bool,
    position: Option<(usize, usize)>,
    on_change: Option<ChangeHandler>,
    layout: crate::control_layout::ControlLayout,
}

crate::control_layout::control_layout!(YueRadio);

impl YueRadio {
    /// Resolve the application default at render time, including later theme changes.
    pub fn use_default_theme(mut self) -> Self {
        self.use_default_theme = true;
        self
    }
    /// Override the application default for this component instance.
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.use_default_theme = false;
        self
    }

    pub fn new(id: impl Into<ElementId>, theme: YueTheme, checked: bool) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            layout: Default::default(),
            checked,
            disabled: false,
            size: RadioSize::Medium,
            appearance: RadioAppearance::default(),
            segment: None,
            label: None,
            accessibility_label: None,
            focus_handle: None,
            tab_stop: true,
            position: None,
            on_change: None,
        }
    }

    pub fn size(mut self, size: RadioSize) -> Self {
        self.size = size;
        self
    }

    /// Draws the option as a segment of an outlined row instead of as a circle. A
    /// group sets this for its whole row, so a lone radio keeps the group's own
    /// outline shape around itself.
    pub fn appearance(mut self, appearance: RadioAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    /// Visible text beside the circle. The whole row — circle and label — is the
    /// click target, and the text names the control in the accessibility tree.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Names a radio that carries no visible text.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Reports the requested selection. Base only asks when the option is enabled
    /// and not already chosen, so a radio cannot switch itself off and this never
    /// carries `false`.
    pub fn on_change(
        mut self,
        handler: impl Fn(bool, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    /// Uses the handle a group drives roving focus through, instead of creating a
    /// keyed one. The group also reads it to tell which option is focused.
    pub fn track_focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.focus_handle = Some(focus_handle.clone());
        self
    }

    /// Whether Tab reaches this option. A group keeps exactly one option in the
    /// tab order and moves that stop with the selection.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Places this segment in a group's row. Internal: the row is the one that
    /// knows its own axis and which option leads it.
    pub(crate) fn segment(mut self, vertical: bool, first: bool) -> Self {
        self.segment = Some(Segment { vertical, first });
        self
    }

    /// This option's one-based position and the size of its set, so assistive
    /// technology can announce "option 2 of 5".
    pub(crate) fn set_position(mut self, position: usize, size: usize) -> Self {
        self.position = Some((position, size));
        self
    }

    fn accessible_name(&self) -> SharedString {
        match (&self.label, &self.accessibility_label) {
            (Some(label), _) | (None, Some(label)) => label.clone(),
            (None, None) => {
                panic!("YueRadio needs a label, or an accessibility_label without visible text")
            }
        }
    }
}

/// One option of a [`YueRadioGroup`]: a stable value, its visible text, and
/// whether it can currently be chosen.
#[derive(Clone)]
pub struct YueRadioOption {
    value: SharedString,
    label: SharedString,
    disabled: bool,
}

impl YueRadioOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A single-choice group: the caller holds the selected value, the group offers
/// the options and moves the selection one option at a time.
///
/// The callback takes the new value and the contexts, without the `ClickEvent` a
/// pointer or a Space key carries: a group's selection also changes when an arrow
/// key moves onto another option, and a click event invented for that keystroke
/// would report a button the user never pressed.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueRadioGroup, YueRadioOption};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let group = YueRadioGroup::new("format", theme, None)
///     .options([YueRadioOption::new("png", "PNG")]);
/// ```
#[derive(IntoElement)]
pub struct YueRadioGroup {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    selected: Option<SharedString>,
    options: Vec<YueRadioOption>,
    label: Option<SharedString>,
    disabled: bool,
    size: RadioSize,
    appearance: RadioAppearance,
    direction: GroupDirection,
    on_change: Option<ValueHandler>,
    layout: crate::control_layout::ControlLayout,
}

crate::control_layout::control_layout!(YueRadioGroup);

impl YueRadioGroup {
    /// Resolve the application default at render time, including later theme changes.
    pub fn use_default_theme(mut self) -> Self {
        self.use_default_theme = true;
        self
    }
    /// Override the application default for this component instance.
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.use_default_theme = false;
        self
    }

    pub fn new(id: impl Into<ElementId>, theme: YueTheme, selected: Option<SharedString>) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            layout: Default::default(),
            selected,
            options: Vec::new(),
            label: None,
            disabled: false,
            size: RadioSize::Medium,
            appearance: RadioAppearance::default(),
            direction: GroupDirection::Vertical,
            on_change: None,
        }
    }

    /// The options offered, in visual order. Values must be unique: they are the
    /// identity the selection is stored against, and a repeated value would make
    /// two options indistinguishable.
    pub fn options(mut self, options: impl IntoIterator<Item = YueRadioOption>) -> Self {
        self.options = options.into_iter().collect();
        let mut seen: Vec<SharedString> = Vec::with_capacity(self.options.len());
        for option in &self.options {
            assert!(
                !seen.contains(&option.value),
                "YueRadioGroup option values must be unique, found {:?} twice",
                option.value.as_ref()
            );
            seen.push(option.value.clone());
        }
        self
    }

    /// A visible caption that also names the group in the accessibility tree.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn size(mut self, size: RadioSize) -> Self {
        self.size = size;
        self
    }

    /// Whether the options read as circles beside their labels or as one outlined
    /// row of segments. The keyboard contract is the same either way: one tab stop
    /// for the group, arrows moving and choosing along its axis.
    pub fn appearance(mut self, appearance: RadioAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn direction(mut self, direction: GroupDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Reports the newly selected value. Choosing the option that is already
    /// selected asks for nothing, so this is not called.
    pub fn on_change(
        mut self,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

#[cfg(test)]
mod tests;
