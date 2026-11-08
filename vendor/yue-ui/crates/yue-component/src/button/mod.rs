//! button: public component contract and builders.

mod element;

use std::rc::Rc;

use gpui_kit::{App, Axis, ClickEvent, ElementId, IntoElement, SharedString, Window};

use crate::tokens::rems as token_rems;

use crate::YueTheme;

use yue_component_icons::{Icon, IconDefinition};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonVariant {
    Primary,
    #[default]
    Default,
    Outline,
    Ghost,
    Text,
    Danger,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IconPosition {
    #[default]
    Leading,
    Trailing,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonShape {
    #[default]
    RoundedRectangle,
    RoundedSquare,
    Pill,
    Circle,
}

enum ButtonContent {
    Text(SharedString),
    Icon {
        icon: Icon,
        accessibility_label: SharedString,
    },
    IconText {
        icon: Icon,
        label: SharedString,
    },
}

/// Yue button with text-only, icon-only, and icon-plus-text content.
#[derive(IntoElement)]
pub struct YueButton {
    id: ElementId,
    theme: YueTheme,
    use_default_theme: bool,
    variant: ButtonVariant,
    size: ButtonSize,
    shape: ButtonShape,
    icon_position: IconPosition,
    icon_size_override: Option<gpui_kit::Rems>,
    dropdown_indicator: bool,
    content: ButtonContent,
    disabled: bool,
    loading: bool,
    block: bool,
    expanded: Option<bool>,
    tooltip: Option<SharedString>,
    tooltip_options: crate::TooltipOptions,
    on_click: Option<ClickHandler>,
    layout: crate::control_layout::ControlLayout,
    focus_handle: Option<gpui_kit::FocusHandle>,
    key_context: Option<gpui_kit::SharedString>,
    description: Option<gpui_kit::SharedString>,
    accessibility_label: Option<SharedString>,
    tab_index: isize,
    tab_stop: bool,
}

crate::control_layout::control_layout!(YueButton);

impl YueButton {
    /// Override the accessible command name without changing visible text.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        assert!(!label.is_empty(), "YueButton requires an accessible name");
        self.accessibility_label = Some(label);
        self
    }
    /// Use caller-owned focus without replacing the activation handler.
    pub fn track_focus(mut self, focus: &gpui_kit::FocusHandle) -> Self {
        self.focus_handle = Some(focus.clone());
        self
    }
    pub fn key_context(mut self, context: impl Into<gpui_kit::SharedString>) -> Self {
        self.key_context = Some(context.into());
        self
    }
    pub fn accessibility_description(
        mut self,
        description: impl Into<gpui_kit::SharedString>,
    ) -> Self {
        self.description = Some(description.into());
        self
    }
    /// Focus traversal order. It is applied to whichever handle the control
    /// tracks, including one passed to `track_focus`, so set the order here
    /// rather than on that handle.
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = index;
        self
    }
    /// Whether Tab traversal stops on the control; like `tab_index`, it is
    /// applied to a caller-owned handle too. The default is `true`.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// A text command using the application theme. The owner supplies its handler.
    /// ```
    /// use yue_component::YueButton;
    /// let save = YueButton::new("save", "保存").on_click(|_, _, _| {});
    /// ```
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::text(
            id,
            YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure),
            label,
        )
        .use_default_theme()
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

    pub fn text(id: impl Into<ElementId>, theme: YueTheme, label: impl Into<SharedString>) -> Self {
        Self::with_content(id, theme, ButtonContent::Text(label.into()))
    }

    pub fn icon(
        id: impl Into<ElementId>,
        theme: YueTheme,
        icon: impl IconDefinition,
        accessibility_label: impl Into<SharedString>,
    ) -> Self {
        let accessibility_label = accessibility_label.into();
        assert!(
            !accessibility_label.is_empty(),
            "icon-only YueButton requires an accessible name"
        );
        Self::with_content(
            id,
            theme,
            ButtonContent::Icon {
                icon: Icon::new(icon),
                accessibility_label,
            },
        )
    }

    pub fn icon_text(
        id: impl Into<ElementId>,
        theme: YueTheme,
        icon: impl IconDefinition,
        label: impl Into<SharedString>,
    ) -> Self {
        Self::with_content(
            id,
            theme,
            ButtonContent::IconText {
                icon: Icon::new(icon),
                label: label.into(),
            },
        )
    }

    fn with_content(id: impl Into<ElementId>, theme: YueTheme, content: ButtonContent) -> Self {
        Self {
            id: id.into(),
            theme,
            use_default_theme: false,
            layout: Default::default(),
            focus_handle: None,
            key_context: None,
            description: None,
            accessibility_label: None,
            tab_index: 0,
            tab_stop: true,
            variant: ButtonVariant::Default,
            size: ButtonSize::Medium,
            shape: ButtonShape::RoundedRectangle,
            icon_position: IconPosition::Leading,
            icon_size_override: None,
            dropdown_indicator: false,
            content,
            disabled: false,
            loading: false,
            block: false,
            expanded: None,
            tooltip: None,
            tooltip_options: crate::TooltipOptions::default(),
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
    /// Internal component glyph sizing; preserves the normal Button hit target.
    pub(crate) fn icon_size_override(mut self, size: gpui_kit::Rems) -> Self {
        self.icon_size_override = Some(size);
        self
    }
    /// Pair an icon command with a separate disclosure chevron. The command
    /// icon stays still when a menu supplies its expanded state.
    pub(crate) fn dropdown_indicator(mut self) -> Self {
        self.dropdown_indicator = true;
        self
    }
    pub fn shape(mut self, shape: ButtonShape) -> Self {
        self.shape = shape;
        self
    }
    pub fn icon_position(mut self, position: IconPosition) -> Self {
        self.icon_position = position;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }
    /// The expanded state of a disclosure control: announces itself, and turns
    /// the button's own chevron half a circle towards the panel it opened.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    /// Configures the shared Tooltip layer; advanced triggers use YueTooltip.
    pub fn tooltip_options(mut self, options: crate::TooltipOptions) -> Self {
        self.tooltip_options = options;
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

/// Lays out related buttons on one axis without changing their individual behavior.
#[derive(IntoElement)]
pub struct ButtonGroup {
    axis: Axis,
    gap: gpui_kit::Rems,
    children: Vec<YueButton>,
}

impl ButtonGroup {
    pub fn new(theme: YueTheme) -> Self {
        Self {
            axis: Axis::Horizontal,
            gap: token_rems(theme, crate::theme::keys::BUTTON_GAP),
            children: Vec::new(),
        }
    }
    pub fn vertical(mut self) -> Self {
        self.axis = Axis::Vertical;
        self
    }
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }
    pub fn gap(mut self, gap: gpui_kit::Rems) -> Self {
        self.gap = gap;
        self
    }
    pub fn child(mut self, button: YueButton) -> Self {
        self.children.push(button);
        self
    }
    pub fn children(mut self, buttons: impl IntoIterator<Item = YueButton>) -> Self {
        self.children.extend(buttons);
        self
    }
}

#[cfg(test)]
mod tests;
