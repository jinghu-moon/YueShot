//! Resource links. Base owns activation and focus; Yue owns token presentation.

mod element;

use std::rc::Rc;

use gpui_kit::{App, ClickEvent, ElementId, FocusHandle, IntoElement, SharedString, Window};

use crate::{Icon, IconDefinition, YueTheme};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LinkVariant {
    Default,
    #[default]
    Primary,
    Danger,
    Warning,
    Success,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LinkSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LinkHover {
    #[default]
    Underline,
    Color,
    None,
}

type OpenHandler = Rc<dyn Fn(&str, &ClickEvent, &mut Window, &mut App)>;

type ActivationHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A text link to a URL or email resource, opened by the system by default.
/// Use a Button for application commands. Icons decorate the text; they do not
/// create additional focus targets. Enter and Space follow Base Link activation.
///
/// ```
/// use yue_component::{YueLink, LinkVariant, outline};
/// let link = YueLink::new("docs", "使用指南", "https://example.com/guide")
///     .variant(LinkVariant::Primary)
///     .trailing_icon(outline::ExternalLink);
/// ```
#[derive(IntoElement)]
pub struct YueLink {
    id: ElementId,
    label: SharedString,
    href: SharedString,
    theme: Option<YueTheme>,
    variant: LinkVariant,
    size: LinkSize,
    underline: Option<bool>,
    hover: LinkHover,
    disabled: bool,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    name: Option<SharedString>,
    description: Option<SharedString>,
    focus: Option<FocusHandle>,
    tab_index: isize,
    tab_stop: bool,
    key_context: Option<SharedString>,
    open_with: Option<OpenHandler>,
    on_activate: Option<ActivationHandler>,
    layout: crate::control_layout::ControlLayout,
}

crate::control_layout::control_layout!(YueLink);

impl YueLink {
    /// Creates a resource link with the application's current Yue theme.
    ///
    /// # Panics
    /// Panics if the visible label or href is empty or whitespace-only.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        href: impl Into<SharedString>,
    ) -> Self {
        let label = label.into();
        let href = href.into();
        assert!(!label.trim().is_empty(), "YueLink requires visible text");
        assert!(
            !href.trim().is_empty(),
            "YueLink requires a resource target"
        );
        Self {
            id: id.into(),
            label,
            href,
            theme: None,
            variant: LinkVariant::default(),
            size: LinkSize::default(),
            underline: None,
            hover: LinkHover::default(),
            disabled: false,
            leading: None,
            trailing: None,
            name: None,
            description: None,
            focus: None,
            tab_index: 0,
            tab_stop: true,
            key_context: None,
            open_with: None,
            on_activate: None,
            layout: Default::default(),
        }
    }

    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn use_default_theme(mut self) -> Self {
        self.theme = None;
        self
    }
    pub fn variant(mut self, variant: LinkVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: LinkSize) -> Self {
        self.size = size;
        self
    }
    /// Overrides the theme's default decoration independently of the hover policy.
    pub fn underline(mut self, underline: bool) -> Self {
        self.underline = Some(underline);
        self
    }
    pub fn hover(mut self, hover: LinkHover) -> Self {
        self.hover = hover;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn leading_icon(mut self, icon: impl IconDefinition) -> Self {
        self.leading = Some(Icon::new(icon));
        self
    }
    pub fn trailing_icon(mut self, icon: impl IconDefinition) -> Self {
        self.trailing = Some(Icon::new(icon));
        self
    }

    /// # Panics
    /// Panics if the accessible name is empty or whitespace-only.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        assert!(
            !label.trim().is_empty(),
            "YueLink requires an accessible name"
        );
        self.name = Some(label);
        self
    }
    pub fn accessibility_description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
    /// Caller-owned focus; tab_index and tab_stop also apply to this handle.
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
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
    pub fn key_context(mut self, context: impl Into<SharedString>) -> Self {
        self.key_context = Some(context.into());
        self
    }
    /// Replaces system opening. The href is passed through without normalization.
    pub fn open_with(
        mut self,
        open: impl Fn(&str, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.open_with = Some(Rc::new(open));
        self
    }
    /// Observes activation after the opening strategy; does not suppress opening.
    pub fn on_activate(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }
}

#[cfg(test)]
mod tests;
