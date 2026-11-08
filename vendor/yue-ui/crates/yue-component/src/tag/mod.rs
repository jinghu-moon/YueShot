//! Compact labels and controlled tag buttons. Base owns activation and focus.
mod element;

use std::rc::Rc;

use gpui_kit::{App, ClickEvent, ElementId, FocusHandle, IntoElement, Rgba, SharedString, Window};

use crate::{Icon, IconDefinition, YueTheme};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TagTone {
    #[default]
    Default,
    Primary,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TagVariant {
    Filled,
    #[default]
    Tint,
    Outline,
    TintOutline,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TagSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TagShape {
    #[default]
    Rounded,
    Pill,
    /// Rounded on the leading side and square on the trailing side.
    Mark,
}

/// Explicit data colors for a static/action tag. Checkable tags use semantic tones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TagColors {
    background: Rgba,
    foreground: Rgba,
    border: Rgba,
}

impl TagColors {
    /// Colors must be finite sRGB channels in 0..=1. The caller owns contrast
    /// against the actual containing surface, including transparent backgrounds.
    pub fn new(background: Rgba, foreground: Rgba, border: Rgba) -> Self {
        for color in [background, foreground, border] {
            assert!(
                [color.r, color.g, color.b, color.a]
                    .into_iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(&v)),
                "TagColors requires finite sRGB channels in 0..=1"
            );
        }
        Self {
            background,
            foreground,
            border,
        }
    }

    pub fn background(&self) -> Rgba {
        self.background
    }
    pub fn foreground(&self) -> Rgba {
        self.foreground
    }
    pub fn border(&self) -> Rgba {
        self.border
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Behavior {
    Static,
    Action,
    Checkable(bool),
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type ChangeHandler = Rc<dyn Fn(bool, &ClickEvent, &mut Window, &mut App)>;

/// A static label, action, or controlled toggle, optionally with an independent
/// close button. The host owns checked values, removal and focus after removal.
///
/// ```
/// use yue_component::{YueTag, TagVariant, outline};
/// let label = YueTag::new("kind", "PNG").variant(TagVariant::Outline);
/// let action = YueTag::action("folder", "截图目录")
///     .leading_icon(outline::Folder)
///     .on_click(|_, _, _| {})
///     .on_close(|_, _, _| {});
/// let toggle = YueTag::checkable("copy-path", "复制路径", false)
///     .on_change(|next, _, _, _| { let _ = next; });
/// ```
#[derive(IntoElement)]
pub struct YueTag {
    id: ElementId,
    label: SharedString,
    behavior: Behavior,
    theme: Option<YueTheme>,
    tone: TagTone,
    variant: TagVariant,
    size: TagSize,
    shape: TagShape,
    leading: Option<Icon>,
    colors: Option<TagColors>,
    disabled: bool,
    overflow_tooltip: bool,
    name: Option<SharedString>,
    description: Option<SharedString>,
    close_label: Option<SharedString>,
    focus: Option<FocusHandle>,
    close_focus: Option<FocusHandle>,
    tab_index: isize,
    tab_stop: bool,
    close_tab_stop: bool,
    key_context: Option<SharedString>,
    on_click: Option<ClickHandler>,
    on_change: Option<ChangeHandler>,
    on_close: Option<ClickHandler>,
    layout: crate::control_layout::ControlLayout,
}

impl YueTag {
    /// Creates a noninteractive label using the current application theme.
    /// Panics for an empty or whitespace-only label.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        assert!(!label.trim().is_empty(), "YueTag requires visible text");
        Self {
            id: id.into(),
            label,
            behavior: Behavior::Static,
            theme: None,
            tone: TagTone::Default,
            variant: TagVariant::default(),
            size: TagSize::default(),
            shape: TagShape::default(),
            leading: None,
            colors: None,
            disabled: false,
            overflow_tooltip: true,
            name: None,
            description: None,
            close_label: None,
            focus: None,
            close_focus: None,
            tab_index: 0,
            tab_stop: true,
            close_tab_stop: true,
            key_context: None,
            on_click: None,
            on_change: None,
            on_close: None,
            layout: Default::default(),
        }
    }

    /// Creates an action tag. Supply `on_click` before rendering.
    pub fn action(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            behavior: Behavior::Action,
            ..Self::new(id, label)
        }
    }

    /// Creates a controlled toggle. Supply `on_change` before rendering.
    /// A request never changes the supplied checked value inside the component.
    pub fn checkable(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        checked: bool,
    ) -> Self {
        Self {
            behavior: Behavior::Checkable(checked),
            tone: TagTone::Primary,
            ..Self::new(id, label)
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
    pub fn tone(mut self, tone: TagTone) -> Self {
        self.tone = tone;
        self
    }
    pub fn variant(mut self, variant: TagVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: TagSize) -> Self {
        self.size = size;
        self
    }
    pub fn shape(mut self, shape: TagShape) -> Self {
        self.shape = shape;
        self
    }
    pub fn leading_icon(mut self, icon: impl IconDefinition) -> Self {
        self.leading = Some(Icon::new(icon));
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn overflow_tooltip(mut self, enabled: bool) -> Self {
        self.overflow_tooltip = enabled;
        self
    }

    pub fn width(mut self, width: impl Into<gpui_kit::DefiniteLength>) -> Self {
        self.layout.width = Some(width.into());
        self
    }
    /// Absolute minimum, clamped to the space needed by the icon, text and close
    /// control. Use `width`/`max_width` for parent-relative constraints.
    pub fn min_width(mut self, width: impl Into<gpui_kit::AbsoluteLength>) -> Self {
        self.layout.min_width = Some(gpui_kit::Length::Definite(
            gpui_kit::DefiniteLength::Absolute(width.into()),
        ));
        self
    }
    pub fn max_width(mut self, width: impl Into<gpui_kit::Length>) -> Self {
        self.layout.max_width = Some(width.into());
        self
    }
    pub fn margin(mut self, margin: impl Into<gpui_kit::Length>) -> Self {
        self.layout.margin = Some(margin.into());
        self
    }
    pub fn flex_grow(mut self, grow: f32) -> Self {
        assert!(grow.is_finite() && grow >= 0.0);
        self.layout.grow = Some(grow);
        self
    }

    /// Overrides the basic three colors on static/action tags. This replaces
    /// tone/variant colors; geometry and interaction feedback remain token-owned.
    pub fn colors(mut self, colors: TagColors) -> Self {
        assert!(
            !matches!(self.behavior, Behavior::Checkable(_)),
            "Checkable tags use semantic tones"
        );
        self.colors = Some(colors);
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        assert!(
            !label.trim().is_empty(),
            "YueTag requires an accessible name"
        );
        self.name = Some(label);
        self
    }
    pub fn accessibility_description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
    pub fn close_label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        assert!(!label.trim().is_empty(), "YueTag requires a close name");
        self.close_label = Some(label);
        self
    }
    /// Bind the real body control. Static bodies never acquire focus.
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
    pub fn track_close_focus(mut self, focus: &FocusHandle) -> Self {
        self.close_focus = Some(focus.clone());
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
    pub fn close_tab_stop(mut self, stop: bool) -> Self {
        self.close_tab_stop = stop;
        self
    }
    pub fn key_context(mut self, context: impl Into<SharedString>) -> Self {
        self.key_context = Some(context.into());
        self
    }

    /// Handles action activation once for pointer, Enter or Space.
    /// Panics if used on a static/checkable tag.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        assert_eq!(
            self.behavior,
            Behavior::Action,
            "on_click requires YueTag::action"
        );
        self.on_click = Some(Rc::new(handler));
        self
    }
    /// Requests a new checked value; updating the host is the caller's job.
    /// Panics unless constructed using `YueTag::checkable`.
    pub fn on_change(
        mut self,
        handler: impl Fn(bool, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        assert!(
            matches!(self.behavior, Behavior::Checkable(_)),
            "on_change requires YueTag::checkable"
        );
        self.on_change = Some(Rc::new(handler));
        self
    }
    /// Adds an independent close button. This only requests removal: it neither
    /// activates/toggles the body nor hides the tag before the host accepts.
    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    // A Select tag renderer may style the label, but cannot replace its value
    // controller or turn a locked selected item into an independent action.
    pub(crate) fn reset_select_behavior(mut self) -> Self {
        self.behavior = Behavior::Static;
        self.on_click = None;
        self.on_change = None;
        self.on_close = None;
        self
    }

    fn validate(&self) {
        match self.behavior {
            Behavior::Static => {}
            Behavior::Action => {
                assert!(self.on_click.is_some(), "YueTag::action requires on_click")
            }
            Behavior::Checkable(_) => assert!(
                self.on_change.is_some(),
                "YueTag::checkable requires on_change"
            ),
        }
    }
}

/// Flow layout for a small, host-owned set of tags. This container has no
/// selection model, roving focus, deletion or implicit toolbar/listbox role.
///
/// ```
/// use yue_component::{YueTag, YueTagGroup};
/// let tags = YueTagGroup::new("attributes")
///     .children([YueTag::new("kind", "PNG"), YueTag::new("mode", "透明背景")]);
/// ```
#[derive(IntoElement)]
pub struct YueTagGroup {
    id: ElementId,
    tags: Vec<YueTag>,
    wrap: bool,
    theme: Option<YueTheme>,
    layout: crate::control_layout::ControlLayout,
}
crate::control_layout::control_layout!(YueTagGroup);

impl YueTagGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            tags: vec![],
            wrap: true,
            theme: None,
            layout: Default::default(),
        }
    }
    pub fn child(mut self, tag: YueTag) -> Self {
        self.tags.push(tag);
        self
    }
    pub fn children(mut self, tags: impl IntoIterator<Item = YueTag>) -> Self {
        self.tags.extend(tags);
        self
    }
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }
    /// Inherited by children without an explicit theme; local tag themes win.
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn use_default_theme(mut self) -> Self {
        self.theme = None;
        self
    }
}

#[cfg(test)]
mod tests;
