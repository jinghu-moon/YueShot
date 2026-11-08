//! Text inputs backed by the existing Base editing state.

mod element;
mod frame;
mod number_input;
mod textarea;

pub(crate) use frame::border_token;
pub use number_input::YueNumberInput;
pub use textarea::YueTextarea;

use gpui_kit::{AnyElement, DefiniteLength, ElementId, Entity, IntoElement, Role, SharedString};

use crate::YueTheme;

use yue_component_icons::{Icon, IconDefinition};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputStatus {
    #[default]
    Normal,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputTextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// What kind of value a [`YueInput`] collects, announced to assistive technology.
///
/// Windows has no native channel for the text content type, so the choice is
/// carried by the accessibility role rather than by an on-screen keyboard hint.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputType {
    #[default]
    Text,
    Search,
    Email,
    Url,
}

impl InputType {
    fn role(self, password: bool) -> Role {
        match self {
            // A masked field is protected even when its value is plain text.
            InputType::Text if password => Role::PasswordInput,
            InputType::Text => Role::TextInput,
            InputType::Search => Role::SearchInput,
            InputType::Email => Role::EmailInput,
            InputType::Url => Role::UrlInput,
        }
    }
}

enum InputSlot {
    Element(AnyElement),
    Icon(Icon),
}

///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueInput};
/// use yue_component::gpui_kit::{Entity, base::input::InputState};
/// # fn example(state: &Entity<InputState>) {
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let control = YueInput::new("field", theme, state).accessibility_label("名称");
/// # }
/// ```
#[derive(IntoElement)]
pub struct YueInput {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    state: Entity<gpui_kit::base::input::InputState>,
    size: InputSize,
    status: InputStatus,
    text_align: InputTextAlign,
    input_type: InputType,
    accessibility_label: Option<SharedString>,
    accessibility_description: Option<SharedString>,
    max_length: Option<usize>,
    show_counter: bool,
    placeholder: Option<SharedString>,
    leading: Option<InputSlot>,
    trailing: Option<InputSlot>,
    disabled: bool,
    readonly: bool,
    bordered: bool,
    block: bool,
    clearable: bool,
    password: bool,
    width: Option<DefiniteLength>,
}

impl YueInput {
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
        state: &Entity<gpui_kit::base::input::InputState>,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            state: state.clone(),
            size: InputSize::Medium,
            status: InputStatus::Normal,
            text_align: InputTextAlign::Left,
            input_type: InputType::Text,
            accessibility_label: None,
            accessibility_description: None,
            max_length: None,
            show_counter: false,
            placeholder: None,
            leading: None,
            trailing: None,
            disabled: false,
            readonly: false,
            bordered: true,
            block: false,
            clearable: false,
            password: false,
            width: None,
        }
    }

    pub fn size(mut self, size: InputSize) -> Self {
        self.size = size;
        self
    }
    pub fn status(mut self, status: InputStatus) -> Self {
        self.status = status;
        self
    }
    pub fn text_align(mut self, align: InputTextAlign) -> Self {
        self.text_align = align;
        self
    }
    /// Declare what the value means, so screen readers announce the right kind of box.
    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.input_type = input_type;
        self
    }
    /// Name this control to assistive technology when no [`crate::YueField`] label owns it.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    /// Announce a hint, requirement, or error together with this control.
    pub fn accessibility_description(mut self, description: impl Into<SharedString>) -> Self {
        self.accessibility_description = Some(description.into());
        self
    }
    /// Trim every edit that overshoots this character count.
    ///
    /// Pasted and IME text is trimmed while rendering, one frame after it lands.
    /// Filters that must reject an edit before it appears belong to GPUI Base: build
    /// the state with `mask_pattern`, `pattern`, or `validate`.
    pub fn max_length(mut self, limit: usize) -> Self {
        self.max_length = Some(limit);
        self
    }
    /// Show the character count inside the frame, as `used / limit`.
    pub fn show_counter(mut self, show: bool) -> Self {
        self.show_counter = show;
        self
    }
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = Some(text.into());
        self
    }
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(InputSlot::Element(element.into_any_element()));
        self
    }
    pub fn leading_icon(mut self, icon: impl IconDefinition) -> Self {
        self.leading = Some(InputSlot::Icon(Icon::new(icon)));
        self
    }
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(InputSlot::Element(element.into_any_element()));
        self
    }
    pub fn trailing_icon(mut self, icon: impl IconDefinition) -> Self {
        self.trailing = Some(InputSlot::Icon(Icon::new(icon)));
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }
    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }
    pub fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }
}

#[cfg(test)]
mod tests;
