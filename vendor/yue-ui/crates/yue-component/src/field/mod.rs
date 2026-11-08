//! Labels, descriptions and validation around a typed control.

mod element;

use crate::{InputStatus, YueForm, YueInput, YueNumberInput, YueTextarea, YueTheme};

use crate::tokens::{rems, rgba as color};

use gpui_kit::{
    AnyElement, App, Entity, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled as _, Window, div, prelude::FluentBuilder as _,
};

fn field_message_token(status: InputStatus) -> crate::theme::TokenKey {
    match status {
        InputStatus::Error => crate::theme::keys::ERROR_FOREGROUND,
        InputStatus::Warning => crate::theme::keys::WARNING_FOREGROUND,
        InputStatus::Success => crate::theme::keys::SUCCESS_FOREGROUND,
        InputStatus::Normal => crate::theme::keys::INPUT_AFFIX_COLOR,
    }
}

/// The copy assistive technology announces with a field's control.
///
/// GPUI has no `aria-required` or `aria-invalid` slot, so the visible label carries
/// the asterisk and the announced text carries the rest.
fn announced(
    required: bool,
    description: &Option<SharedString>,
    message: &Option<SharedString>,
) -> Option<SharedString> {
    let mut parts: Vec<&str> = Vec::new();
    if required {
        parts.push("Required");
    }
    if let Some(description) = description {
        parts.push(description.as_str());
    }
    if let Some(message) = message {
        parts.push(message.as_str());
    }
    (!parts.is_empty()).then(|| SharedString::from(parts.join(", ")))
}

/// The control a [`YueField`] wraps.
///
/// Keeping the builder, rather than a finished element, lets the field inject its
/// label, description, and validation status into the control's accessibility node.
enum FieldChild {
    Select(Box<crate::YueSelect>),
    Element(AnyElement),
    Input(YueInput),
    Textarea(YueTextarea),
    Number(YueNumberInput),
}

/// A field layout that keeps labels and validation copy outside the input editor.
///
/// Prefer [`input`](YueField::input), [`textarea`](YueField::textarea), and
/// [`number_input`](YueField::number_input): the field then names the control for
/// assistive technology and makes the message the single source of its status, so
/// a red border always arrives with the copy that explains it.
///
/// ```
/// use yue_component::{Accent, ThemeMode, YueTheme, YueField};
/// let field = YueField::new(YueTheme::new(ThemeMode::Light, Accent::Azure)).label("名称");
/// ```
#[derive(IntoElement)]
pub struct YueField {
    theme: YueTheme,
    use_default_theme: bool,
    label: Option<SharedString>,
    description: Option<SharedString>,
    message: Option<SharedString>,
    message_status: InputStatus,
    required: bool,
    form: Option<(Entity<YueForm>, SharedString)>,
    child: Option<FieldChild>,
}

impl YueField {
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

    pub fn new(theme: YueTheme) -> Self {
        Self {
            theme,
            use_default_theme: false,
            label: None,
            description: None,
            message: None,
            message_status: InputStatus::Normal,
            required: false,
            form: None,
            child: None,
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
    pub fn message(mut self, status: InputStatus, message: impl Into<SharedString>) -> Self {
        self.message_status = status;
        self.message = Some(message.into());
        self
    }
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
    /// Take this field's error from a [`YueForm`], revealed by its `submit`.
    ///
    /// The `id` is the one registered with [`YueForm::require`] or [`YueForm::rule`].
    /// While a rule fails, the wrapped control becomes invalid and the rule's message
    /// replaces any set by [`message`](YueField::message).
    pub fn bind(mut self, form: &Entity<YueForm>, id: impl Into<SharedString>) -> Self {
        self.form = Some((form.clone(), id.into()));
        self
    }
    pub fn input(mut self, input: YueInput) -> Self {
        self.child = Some(FieldChild::Input(input));
        self
    }
    /// Preserve the Select's typed value and forward field semantics/status.
    pub fn select(mut self, select: crate::YueSelect) -> Self {
        self.child = Some(FieldChild::Select(Box::new(select)));
        self
    }
    pub fn textarea(mut self, textarea: YueTextarea) -> Self {
        self.child = Some(FieldChild::Textarea(textarea));
        self
    }
    pub fn number_input(mut self, number: YueNumberInput) -> Self {
        self.child = Some(FieldChild::Number(number));
        self
    }
    /// Place an element the field cannot reach: it keeps no label or status.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(FieldChild::Element(child.into_any_element()));
        self
    }
}

#[cfg(test)]
mod tests;
