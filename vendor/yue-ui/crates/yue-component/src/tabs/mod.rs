//! Controlled, keyboard-accessible tabs. The owner retains the selection and panels.
use crate::{Icon, IconDefinition, YueTheme};
use gpui_kit::{
    AnyElement, App, DefiniteLength, ElementId, IntoElement, SharedString, WeakEntity, Window,
};
use std::{cell::RefCell, rc::Rc};

mod element;
pub use gpui_kit::base::{
    TabsActivation, TabsChangeRequest, TabsChangeSource, TabsCloseRequest, TabsScrollAlign,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TabsSize {
    Small,
    #[default]
    Medium,
    Large,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TabsVariant {
    #[default]
    Normal,
    Card,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TabsPlacement {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}
impl TabsPlacement {
    pub(crate) fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}
/// Optional incoming-panel motion. Outgoing descendants are always detached
/// immediately so hidden editors and menus cannot continue receiving input.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TabsContentMotion {
    #[default]
    None,
    Fade,
    Slide,
}

type ContentRenderer = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type ChangeHandler = Rc<dyn Fn(&TabsChangeRequest, &mut Window, &mut App)>;
type CloseHandler = Rc<dyn Fn(&TabsCloseRequest, &mut Window, &mut App)>;
type AddHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A tab's stable identity and content factory. Retain input/editor entities in
/// the owner; this factory must not launch work or allocate retained state.
#[derive(Clone)]
pub struct YueTabItem {
    key: SharedString,
    label: SharedString,
    icon: Option<Icon>,
    disabled: bool,
    closable: bool,
    content: Option<ContentRenderer>,
    panel_tab_stop: bool,
}
impl YueTabItem {
    pub fn new(key: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        let key = key.into();
        let label = label.into();
        assert!(
            !key.trim().is_empty() && !label.trim().is_empty(),
            "Tab requires a nonempty key and label"
        );
        Self {
            key,
            label,
            icon: None,
            disabled: false,
            closable: false,
            content: None,
            panel_tab_stop: true,
        }
    }
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn label(&self) -> &SharedString {
        &self.label
    }
    pub fn icon(mut self, icon: impl IconDefinition) -> Self {
        self.icon = Some(Icon::new(icon));
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn is_closable(&self) -> bool {
        self.closable
    }
    pub fn content(
        mut self,
        renderer: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Rc::new(renderer));
        self
    }
    /// Set false when the panel already contains a natural keyboard entry point.
    pub fn panel_tab_stop(mut self, enabled: bool) -> Self {
        self.panel_tab_stop = enabled;
        self
    }
}

/// Optional imperative focus/scroll seam. It does not own the selected key and
/// becomes a no-op after the owning tab list is unmounted.
#[derive(Clone, Default)]
pub struct TabsHandle(Rc<RefCell<Option<WeakEntity<gpui_kit::base::TabsController>>>>);
impl TabsHandle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn scroll_to(
        &self,
        key: impl Into<SharedString>,
        align: TabsScrollAlign,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(state) = self.0.borrow().as_ref().and_then(WeakEntity::upgrade) {
            state.update(cx, |s, cx| {
                s.request_reveal(key.into(), align);
                cx.notify();
            });
            window.refresh();
        }
    }
    pub fn focus(&self, key: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        let key = key.into();
        if let Some(state) = self.0.borrow().as_ref().and_then(WeakEntity::upgrade) {
            let focus = state.update(cx, |s, cx| {
                if s.is_item_disabled(&key) {
                    return None;
                }
                s.request_reveal(key.clone(), TabsScrollAlign::Auto);
                cx.notify();
                s.focus_handle(&key)
            });
            if let Some(focus) = focus {
                focus.focus(window, cx);
            }
        }
    }
}

/// A controlled tab list and its active content. `on_change` requests a value;
/// only the owner's next `selected` snapshot changes selection.
///
/// ```
/// use yue_component::{YueTabs, YueTabItem};
/// use yue_component::gpui_kit::{IntoElement, ParentElement, div};
/// let tabs = YueTabs::new("settings", Some("general".into()))
///     .accessibility_label("Settings categories")
///     .items([
///         YueTabItem::new("general", "General")
///             .content(|_, _| div().child("General settings").into_any_element()),
///         YueTabItem::new("appearance", "Appearance")
///             .content(|_, _| div().child("Appearance settings").into_any_element()),
///     ]).on_change(|request, _, _| { let _ = request.key(); });
/// ```
#[derive(IntoElement)]
pub struct YueTabs {
    id: ElementId,
    selected: Option<SharedString>,
    items: Vec<YueTabItem>,
    label: SharedString,
    theme: Option<YueTheme>,
    size: TabsSize,
    variant: TabsVariant,
    placement: TabsPlacement,
    activation: TabsActivation,
    motion: TabsContentMotion,
    disabled: bool,
    addable: bool,
    lazy: bool,
    height: Option<DefiniteLength>,
    header_width: Option<DefiniteLength>,
    layout: crate::control_layout::ControlLayout,
    on_change: Option<ChangeHandler>,
    on_close: Option<CloseHandler>,
    on_add: Option<AddHandler>,
    empty: Option<ContentRenderer>,
    handle: Option<TabsHandle>,
}
crate::control_layout::control_layout!(YueTabs);
impl YueTabs {
    pub fn new(id: impl Into<ElementId>, selected: Option<SharedString>) -> Self {
        Self {
            id: id.into(),
            selected,
            items: vec![],
            label: "标签页".into(),
            theme: None,
            size: TabsSize::Medium,
            variant: TabsVariant::Normal,
            placement: TabsPlacement::Top,
            activation: TabsActivation::Automatic,
            motion: TabsContentMotion::None,
            disabled: false,
            addable: false,
            lazy: true,
            height: None,
            header_width: None,
            layout: Default::default(),
            on_change: None,
            on_close: None,
            on_add: None,
            empty: None,
            handle: None,
        }
    }
    pub fn items(mut self, items: impl IntoIterator<Item = YueTabItem>) -> Self {
        self.items = items.into_iter().collect();
        let mut keys = std::collections::HashSet::new();
        for item in &self.items {
            assert!(
                keys.insert(item.key.clone()),
                "Tabs requires unique item keys"
            );
        }
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        assert!(!self.label.trim().is_empty());
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn size(mut self, size: TabsSize) -> Self {
        self.size = size;
        self
    }
    pub fn variant(mut self, variant: TabsVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn placement(mut self, placement: TabsPlacement) -> Self {
        self.placement = placement;
        self
    }
    pub fn activation(mut self, activation: TabsActivation) -> Self {
        self.activation = activation;
        self
    }
    pub fn content_motion(mut self, motion: TabsContentMotion) -> Self {
        self.motion = motion;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn addable(mut self, addable: bool) -> Self {
        self.addable = addable;
        self
    }
    /// Build only the active panel (default). With false, all factories are
    /// called, but only the active result is attached. Retain state in the owner.
    pub fn lazy(mut self, lazy: bool) -> Self {
        self.lazy = lazy;
        self
    }
    pub fn height(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = Some(height.into());
        self
    }
    pub fn header_width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.header_width = Some(width.into());
        self
    }
    pub fn handle(mut self, handle: &TabsHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }
    pub fn empty(mut self, render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.empty = Some(Rc::new(render));
        self
    }
    pub fn on_change(
        mut self,
        handler: impl Fn(&TabsChangeRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
    pub fn on_close(
        mut self,
        handler: impl Fn(&TabsCloseRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }
    pub fn on_add(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_add = Some(Rc::new(handler));
        self
    }
}
