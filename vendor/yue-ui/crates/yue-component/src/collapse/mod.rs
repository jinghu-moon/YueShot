//! Controlled disclosure groups with safe content unmounting and measured motion.
use crate::{Icon, IconDefinition, YueTheme};
use gpui_kit::{AnyElement, App, ElementId, IntoElement, SharedString, Window};
use std::rc::Rc;

mod element;
pub use gpui_kit::base::{CollapseChangeRequest, CollapseChangeSource, CollapseMode};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollapseSize {
    Small,
    #[default]
    Medium,
    Large,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollapseVariant {
    #[default]
    Bordered,
    Borderless,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollapseExpandTrigger {
    #[default]
    Header,
    Icon,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollapseIndicatorPosition {
    #[default]
    Leading,
    Trailing,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollapseMotion {
    #[default]
    Height,
    None,
}

type Renderer = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type ChangeHandler = Rc<dyn Fn(&CollapseChangeRequest, &mut Window, &mut App)>;

/// Stable identity and lazy content. Retain editor/input entities in the owner;
/// a closed item's content factory is not called and its old subtree is detached.
#[derive(Clone)]
pub struct YueCollapseItem {
    key: SharedString,
    title: SharedString,
    disabled: bool,
    indicator: Option<Icon>,
    content: Option<Renderer>,
    actions: Option<Renderer>,
    padding: bool,
    region: bool,
}
impl YueCollapseItem {
    pub fn new(key: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        let key = key.into();
        let title = title.into();
        assert!(
            !key.trim().is_empty() && !title.trim().is_empty(),
            "Collapse requires a nonempty key and title"
        );
        Self {
            key,
            title,
            disabled: false,
            indicator: None,
            content: None,
            actions: None,
            padding: true,
            region: false,
        }
    }
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn title(&self) -> &SharedString {
        &self.title
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// A custom glyph is static. Only the default chevron rotates on expansion.
    pub fn indicator(mut self, icon: impl IconDefinition) -> Self {
        self.indicator = Some(Icon::new(icon));
        self
    }
    pub fn content(
        mut self,
        renderer: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Rc::new(renderer));
        self
    }
    /// Commands are siblings of the trigger and remain enabled independently.
    pub fn actions(
        mut self,
        renderer: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.actions = Some(Rc::new(renderer));
        self
    }
    pub fn content_padding(mut self, padding: bool) -> Self {
        self.padding = padding;
        self
    }
    pub fn region(mut self, region: bool) -> Self {
        self.region = region;
        self
    }
}

/// Expand/collapse requests are proposals; the owner commits their complete key
/// set in the next render. Every enabled heading remains a normal Tab stop.
///
/// ```
/// use yue_component::{YueCollapse, YueCollapseItem, CollapseMode};
/// use yue_component::gpui_kit::{IntoElement, ParentElement as _, div};
/// let collapse = YueCollapse::new("settings", vec!["general".into()])
///     .mode(CollapseMode::Accordion)
///     .items([
///         YueCollapseItem::new("general", "General")
///             .content(|_, _| div().child("General settings").into_any_element()),
///         YueCollapseItem::new("advanced", "Advanced")
///             .content(|_, _| div().child("Advanced settings").into_any_element()),
///     ])
///     .on_change(|request, _, _| { let _ = request.expanded_keys(); });
/// ```
#[derive(IntoElement)]
pub struct YueCollapse {
    id: ElementId,
    expanded: Vec<SharedString>,
    items: Vec<YueCollapseItem>,
    theme: Option<YueTheme>,
    label: Option<SharedString>,
    heading_level: usize,
    mode: CollapseMode,
    size: CollapseSize,
    variant: CollapseVariant,
    trigger: CollapseExpandTrigger,
    position: CollapseIndicatorPosition,
    motion: CollapseMotion,
    disabled: bool,
    show_indicator: bool,
    layout: crate::control_layout::ControlLayout,
    on_change: Option<ChangeHandler>,
}
crate::control_layout::control_layout!(YueCollapse);
impl YueCollapse {
    pub fn new(id: impl Into<ElementId>, expanded: impl IntoIterator<Item = SharedString>) -> Self {
        Self {
            id: id.into(),
            expanded: expanded.into_iter().collect(),
            items: Vec::new(),
            theme: None,
            label: None,
            heading_level: 3,
            mode: CollapseMode::Multiple,
            size: CollapseSize::Medium,
            variant: CollapseVariant::Bordered,
            trigger: CollapseExpandTrigger::Header,
            position: CollapseIndicatorPosition::Leading,
            motion: CollapseMotion::Height,
            disabled: false,
            show_indicator: true,
            layout: Default::default(),
            on_change: None,
        }
    }
    pub fn items(mut self, items: impl IntoIterator<Item = YueCollapseItem>) -> Self {
        self.items = items.into_iter().collect();
        let mut keys = std::collections::HashSet::new();
        for item in &self.items {
            assert!(
                keys.insert(item.key.clone()),
                "Collapse requires unique item keys"
            );
        }
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        assert!(!label.trim().is_empty());
        self.label = Some(label);
        self
    }
    pub fn heading_level(mut self, level: usize) -> Self {
        assert!((1..=6).contains(&level));
        self.heading_level = level;
        self
    }
    pub fn mode(mut self, mode: CollapseMode) -> Self {
        self.mode = mode;
        self
    }
    pub fn size(mut self, size: CollapseSize) -> Self {
        self.size = size;
        self
    }
    pub fn variant(mut self, variant: CollapseVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn expand_trigger(mut self, trigger: CollapseExpandTrigger) -> Self {
        self.trigger = trigger;
        self
    }
    pub fn indicator_position(mut self, position: CollapseIndicatorPosition) -> Self {
        self.position = position;
        self
    }
    pub fn motion(mut self, motion: CollapseMotion) -> Self {
        self.motion = motion;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn show_indicator(mut self, show: bool) -> Self {
        self.show_indicator = show;
        self
    }
    pub fn on_change(
        mut self,
        handler: impl Fn(&CollapseChangeRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}
