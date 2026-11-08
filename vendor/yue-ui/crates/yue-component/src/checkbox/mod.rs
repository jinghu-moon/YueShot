//! Yue checkbox: one controlled three-state control and a multi-select group.
//!
//! The control keeps GPUI Base's activation, focus and accessibility behaviour
//! and adds Yue's appearance: an accent-filled box whose check and dash marks are
//! drawn as vector strokes, so the mark scales with the box instead of being an
//! icon asset sized against it.

mod element;

mod behavior;

mod paint;

use std::rc::Rc;

use gpui_kit::{App, ClickEvent, ElementId, IntoElement, SharedString, Window};

use crate::{GroupDirection, YueTheme};

type ChangeHandler = Rc<dyn Fn(CheckboxState, &ClickEvent, &mut Window, &mut App)>;

type SelectionHandler = Rc<dyn Fn(Vec<SharedString>, &ClickEvent, &mut Window, &mut App)>;

/// Which selection a checkbox reports.
///
/// This is Yue's own type rather than the base crate's so a consumer reads its
/// value without naming a `gpui-base` module.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CheckboxState {
    #[default]
    Unchecked,
    Checked,
    /// Part of what the control stands for is selected. Activating it asks for
    /// [`CheckboxState::Checked`] rather than clearing the selection.
    Indeterminate,
}

impl CheckboxState {
    fn to_base(self) -> gpui_kit::base::CheckboxState {
        match self {
            Self::Unchecked => gpui_kit::base::CheckboxState::Unchecked,
            Self::Checked => gpui_kit::base::CheckboxState::Checked,
            Self::Indeterminate => gpui_kit::base::CheckboxState::Indeterminate,
        }
    }

    fn from_base(state: gpui_kit::base::CheckboxState) -> Self {
        match state {
            gpui_kit::base::CheckboxState::Unchecked => Self::Unchecked,
            gpui_kit::base::CheckboxState::Checked => Self::Checked,
            gpui_kit::base::CheckboxState::Indeterminate => Self::Indeterminate,
        }
    }

    /// Whether the box takes the accent fill: the reference fills an
    /// indeterminate box exactly like a checked one and only the mark differs.
    fn chosen(self) -> bool {
        self != Self::Unchecked
    }
}

/// Three Yue checkbox size profiles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CheckboxSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// A controlled Yue checkbox. The caller owns and updates the state.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueCheckbox, CheckboxState};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueCheckbox::new("agree", theme, CheckboxState::Unchecked).label("同意");
/// ```
#[derive(IntoElement)]
pub struct YueCheckbox {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    state: CheckboxState,
    disabled: bool,
    size: CheckboxSize,
    label: Option<SharedString>,
    accessibility_label: Option<SharedString>,
    on_change: Option<ChangeHandler>,
    focus_handle: Option<gpui_kit::FocusHandle>,
    layout: crate::control_layout::ControlLayout,
}

crate::control_layout::control_layout!(YueCheckbox);

impl YueCheckbox {
    /// Use caller-owned focus while preserving the checkbox's native behavior.
    pub fn track_focus(mut self, focus: &gpui_kit::FocusHandle) -> Self {
        self.focus_handle = Some(focus.clone());
        self
    }

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

    pub fn new(id: impl Into<ElementId>, theme: YueTheme, state: CheckboxState) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            layout: Default::default(),
            state,
            disabled: false,
            size: CheckboxSize::Medium,
            label: None,
            accessibility_label: None,
            on_change: None,
            focus_handle: None,
        }
    }

    pub fn size(mut self, size: CheckboxSize) -> Self {
        self.size = size;
        self
    }

    /// Visible text beside the box. The whole row — box and label — is the click
    /// target, and the text names the control in the accessibility tree.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Names a checkbox that carries no visible text.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Reports the state the control should move to: `Unchecked` or
    /// `Indeterminate` become `Checked` and `Checked` becomes `Unchecked`. The
    /// caller writes the new value back; the control keeps no second copy.
    pub fn on_change(
        mut self,
        handler: impl Fn(CheckboxState, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    fn accessible_name(&self) -> SharedString {
        match (&self.label, &self.accessibility_label) {
            (Some(label), _) | (None, Some(label)) => label.clone(),
            (None, None) => {
                panic!("YueCheckbox needs a label, or an accessibility_label without visible text")
            }
        }
    }
}

/// One option of a [`YueCheckboxGroup`]: a stable value, its visible text, and
/// whether it can currently be changed.
#[derive(Clone)]
pub struct YueCheckboxOption {
    value: SharedString,
    label: SharedString,
    disabled: bool,
}

impl YueCheckboxOption {
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

/// A multi-select group: the caller holds the selected values, the group edits
/// them one option at a time or through a derived select-all header.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueCheckboxGroup, YueCheckboxOption};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let group = YueCheckboxGroup::new("channels", theme, Vec::<&str>::new())
///     .options([YueCheckboxOption::new("mail", "邮件")]);
/// ```
#[derive(IntoElement)]
pub struct YueCheckboxGroup {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    selected: Vec<SharedString>,
    options: Vec<YueCheckboxOption>,
    label: Option<SharedString>,
    select_all: Option<SharedString>,
    disabled: bool,
    size: CheckboxSize,
    direction: GroupDirection,
    on_change: Option<SelectionHandler>,
    layout: crate::control_layout::ControlLayout,
}

crate::control_layout::control_layout!(YueCheckboxGroup);

impl YueCheckboxGroup {
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

    pub fn new(
        id: impl Into<ElementId>,
        theme: YueTheme,
        selected: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            layout: Default::default(),
            selected: selected.into_iter().map(Into::into).collect(),
            options: Vec::new(),
            label: None,
            select_all: None,
            disabled: false,
            size: CheckboxSize::Medium,
            direction: GroupDirection::Vertical,
            on_change: None,
        }
    }

    /// The options offered. Values must be unique: they are the identity the
    /// selection is stored against, and a repeated value would make two rows
    /// write the same entry.
    pub fn options(mut self, options: impl IntoIterator<Item = YueCheckboxOption>) -> Self {
        self.options = options.into_iter().collect();
        let mut seen: Vec<SharedString> = Vec::with_capacity(self.options.len());
        for option in &self.options {
            assert!(
                !seen.contains(&option.value),
                "YueCheckboxGroup option values must be unique, found {:?} twice",
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

    /// Add a header row above the options. Its state is never stored: it is
    /// derived from the options, and clicking it selects or clears every option
    /// that is not individually disabled. The text is the header's visible label
    /// and its accessible name.
    pub fn select_all(mut self, label: impl Into<SharedString>) -> Self {
        self.select_all = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn size(mut self, size: CheckboxSize) -> Self {
        self.size = size;
        self
    }

    pub fn direction(mut self, direction: GroupDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Reports the whole new selection, not a delta, so the caller writes one
    /// value back per change.
    pub fn on_change(
        mut self,
        handler: impl Fn(Vec<SharedString>, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

#[cfg(test)]
mod tests;
