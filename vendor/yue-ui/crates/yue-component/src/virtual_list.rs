//! Vertical Yue presentation; behavior and native state belong exclusively to Base.
use crate::{YueTheme, tokens::rgba};
use gpui_kit::base::{Scrollbar, TestSupportExt as _, virtualization::VirtualList};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, IntoElement, ParentElement, Refineable as _, RenderOnce,
    Role, SharedString, StyleRefinement, Styled, Window, div,
};
use std::rc::Rc;

type ItemRenderer = Rc<dyn Fn(VirtualizerItem, &mut Window, &mut App) -> AnyElement>;
type ViewFactory = Rc<dyn Fn(VirtualizerItemIdentity, &mut Window, &mut App) -> gpui_kit::AnyView>;

pub use gpui_kit::base::virtualization::{
    VirtualGrid, VirtualGridCell, VirtualGridCellIdentity, VirtualGridCellKey, VirtualGridSnapshot,
    VirtualGridSpan, VirtualGridSpanSet, VirtualGridTrack, VirtualMasonry, VirtualMasonryCache,
    VirtualMasonryCacheItem, VirtualMasonryCapabilities, VirtualMasonryItem,
    VirtualMasonrySnapshot, VirtualMasonryState, VirtualizerAlign, VirtualizerAnchorPosition,
    VirtualizerAttachment, VirtualizerBinding, VirtualizerCache, VirtualizerCapabilities,
    VirtualizerChange, VirtualizerCommandOutcome, VirtualizerError, VirtualizerEvent,
    VirtualizerItem, VirtualizerItemIdentity, VirtualizerLayout, VirtualizerMeasurement,
    VirtualizerMotion, VirtualizerRange, VirtualizerRangeExtractor, VirtualizerScrollDirection,
    VirtualizerScrollOptions, VirtualizerScrollRequest, VirtualizerScrollbarHandle,
    VirtualizerSizeUpdate, VirtualizerSizing, VirtualizerSizingKind, VirtualizerSnapshot,
    VirtualizerSource, VirtualizerState, VirtualizerViewport,
};

/// A vertical list, with list/list-item semantics unless explicitly overridden.
/// Its parent must provide a finite height. Rows may contain any GPUI elements;
/// retain editing/expansion and resource state in the host under domain keys.
#[derive(IntoElement)]
pub struct YueVirtualList {
    id: ElementId,
    state: Entity<VirtualizerState>,
    theme: Option<YueTheme>,
    render: ItemRenderer,
    label: Option<SharedString>,
    role: Option<Role>,
    item_role: Option<Role>,
    scrollbar: bool,
    style: StyleRefinement,
    view_factory: Option<ViewFactory>,
    content_revision: u64,
}
impl YueVirtualList {
    pub fn new<R: IntoElement>(
        id: impl Into<ElementId>,
        state: &Entity<VirtualizerState>,
        render: impl Fn(VirtualizerItem, &mut Window, &mut App) -> R + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            theme: None,
            render: Rc::new(move |item, w, cx| render(item, w, cx).into_any_element()),
            label: None,
            role: Some(Role::List),
            item_role: Some(Role::ListItem),
            scrollbar: true,
            style: StyleRefinement::default(),
            view_factory: None,
            content_revision: 0,
        }
    }
    pub fn new_with_item_views<V: Into<gpui_kit::AnyView>>(
        id: impl Into<ElementId>,
        state: &Entity<VirtualizerState>,
        factory: impl Fn(VirtualizerItemIdentity, &mut Window, &mut App) -> V + 'static,
    ) -> Self {
        let mut list = Self::new(id, state, |_, _, _| div());
        list.view_factory = Some(Rc::new(move |item, w, cx| factory(item, w, cx).into()));
        list
    }
    pub fn content_revision(mut self, revision: u64) -> Self {
        self.content_revision = revision;
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    /// Set None/None when a composite such as Select owns all semantics.
    pub fn semantics(mut self, container: Option<Role>, item: Option<Role>) -> Self {
        self.role = container;
        self.item_role = item;
        self
    }
    pub fn scrollbar(mut self, enabled: bool) -> Self {
        self.scrollbar = enabled;
        self
    }
}
impl Styled for YueVirtualList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl RenderOnce for YueVirtualList {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        assert_eq!(
            self.state.read(cx).axis(),
            gpui_kit::Axis::Vertical,
            "YueVirtualList is a vertical presentation; use Base for horizontal layouts"
        );
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let render = self.render;
        let mut list = if let Some(factory) = self.view_factory {
            VirtualList::new_with_item_views(
                (self.id.clone(), "viewport"),
                &self.state,
                move |item, w, cx| factory(item, w, cx),
            )
            .content_revision(self.content_revision)
        } else {
            VirtualList::new(
                (self.id.clone(), "viewport"),
                &self.state,
                move |item, w, cx| render(item, w, cx),
            )
        }
        .item_role(self.item_role);
        if let Some(role) = self.role {
            list = list.role(role);
        }
        if let Some(label) = self.label {
            list = list.accessibility_label(label);
        }
        let mut container = gpui_kit::InteractiveElement::id(div(), self.id)
            .relative()
            .size_full()
            .min_h_0()
            .min_w_0()
            .text_color(rgba(theme, "--text-primary"));
        container.style().refine(&self.style);
        let container = container.child(list.test_support());
        if self.scrollbar {
            let handle = self
                .state
                .read(cx)
                .scrollbar_handle()
                .expect("YueVirtualList owns its viewport");
            container
                .child(Scrollbar::vertical(&handle))
                .into_any_element()
        } else {
            container.into_any_element()
        }
    }
}
