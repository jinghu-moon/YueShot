//! Controlled hierarchical paths composed from Base buttons and Yue Dropdown.

mod element;

use crate::{
    Icon, YueTheme,
    control_layout::ControlLayout,
    theme::keys,
    tokens::{rems, rgba},
};

use action::decoration_element;

use gpui_kit::base::TestSupportExt as _;

use gpui_kit::{
    AnyElement, App, DefiniteLength, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, prelude::FluentBuilder as _,
};

use model::Decoration;

use std::rc::Rc;

mod action;

mod model;

pub use model::{
    BreadcrumbCollapse, BreadcrumbEllipsis, BreadcrumbNavigate, BreadcrumbNavigateSource,
    BreadcrumbSeparator, YueBreadcrumbItem,
};

type NavigateHandler = Rc<dyn Fn(&BreadcrumbNavigate, &mut Window, &mut App)>;

type ClickHandler = Rc<dyn Fn(&gpui_kit::ClickEvent, &mut Window, &mut App)>;

/// The host owns navigation. The final item is current and static by default.
///
/// ```no_run
/// use yue_component::{YueBreadcrumb, YueBreadcrumbItem, YueTheme, ThemeMode, Accent};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let path = YueBreadcrumb::new("path", theme)
///     .items([YueBreadcrumbItem::new("home", "首页"), YueBreadcrumbItem::new("current", "当前页")])
///     .on_navigate(|request, _, _| println!("{}", request.value()));
/// ```
#[derive(IntoElement)]
pub struct YueBreadcrumb {
    id: ElementId,
    theme: YueTheme,
    inherited: bool,
    items: Vec<YueBreadcrumbItem>,
    separator: BreadcrumbSeparator,
    ellipsis: BreadcrumbEllipsis,
    collapse: Option<BreadcrumbCollapse>,
    handler: Option<NavigateHandler>,
    current_clickable: bool,
    max_width: Option<DefiniteLength>,
    name: SharedString,
    overflow_name: SharedString,
    current_description: SharedString,
    layout: ControlLayout,
}

crate::control_layout::control_layout!(YueBreadcrumb);

impl YueBreadcrumb {
    pub fn new(id: impl Into<ElementId>, theme: YueTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            inherited: false,
            items: vec![],
            separator: Default::default(),
            ellipsis: Default::default(),
            collapse: None,
            handler: None,
            current_clickable: false,
            max_width: None,
            name: "Breadcrumb".into(),
            overflow_name: "Show hidden levels".into(),
            current_description: "Current page".into(),
            layout: Default::default(),
        }
    }
    pub fn item(mut self, item: YueBreadcrumbItem) -> Self {
        self.items.push(item);
        self
    }
    pub fn items(mut self, items: impl IntoIterator<Item = YueBreadcrumbItem>) -> Self {
        self.items.extend(items);
        self
    }
    pub fn on_navigate(
        mut self,
        handler: impl Fn(&BreadcrumbNavigate, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Some(Rc::new(handler));
        self
    }
    pub fn separator(mut self, separator: BreadcrumbSeparator) -> Self {
        self.separator = separator;
        self
    }
    pub fn ellipsis(mut self, ellipsis: BreadcrumbEllipsis) -> Self {
        self.ellipsis = ellipsis;
        self
    }
    pub fn collapse(mut self, collapse: BreadcrumbCollapse) -> Self {
        self.collapse = Some(collapse);
        self
    }
    pub fn current_clickable(mut self, clickable: bool) -> Self {
        self.current_clickable = clickable;
        self
    }
    pub fn item_max_width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.max_width = Some(width.into());
        self
    }
    pub fn label(mut self, text: impl Into<SharedString>) -> Self {
        self.name = named(text);
        self
    }
    pub fn overflow_label(mut self, text: impl Into<SharedString>) -> Self {
        self.overflow_name = named(text);
        self
    }
    pub fn current_description(mut self, text: impl Into<SharedString>) -> Self {
        self.current_description = named(text);
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.inherited = false;
        self
    }
    pub fn use_default_theme(mut self) -> Self {
        self.inherited = true;
        self
    }
}

fn named(text: impl Into<SharedString>) -> SharedString {
    let text = text.into();
    assert!(
        !text.trim().is_empty(),
        "Breadcrumb accessible name must not be blank"
    );
    text
}

#[cfg(all(test, feature = "headless-tests"))]
mod tests;
