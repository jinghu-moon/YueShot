//! Stateless content lists. Hosts own data, selection, actions and scrolling.
use crate::{YueTheme, control_layout::ControlLayout};
use gpui_kit::{AnyElement, ElementId, IntoElement, SharedString};

mod element;
mod grid;
mod virtual_grid;
pub use grid::{ListGrid, ListGridGeometry};
pub use virtual_grid::{ListGridSizing, ListGridState, YueVirtualListGrid};

/// Density changes padding and typography, never forces a fixed row height.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ListSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// Collection direction. Horizontal lists do not wrap or create a scroller.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ListLayout {
    #[default]
    Vertical,
    Horizontal,
}

/// Vertical puts actions below the body; extra remains beside the main content.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ListItemLayout {
    #[default]
    Horizontal,
    Vertical,
}

/// A content collection with optional header, footer and empty body.
/// No entities, focus handles, selection values or scroll handles are created.
///
/// ```
/// use yue_component::{YueList, YueListItem, YueListItemMeta};
/// let list = YueList::new("members").bordered(true).child(
///     YueListItem::new("alex").content(
///         YueListItemMeta::new("Alex").description("Design team"),
///     ),
/// );
/// ```
#[derive(IntoElement)]
pub struct YueList {
    id: ElementId,
    entries: Vec<Entry>,
    theme: Option<YueTheme>,
    size: ListSize,
    orientation: ListLayout,
    grid: Option<ListGrid>,
    item_orientation: ListItemLayout,
    split: Option<bool>,
    striped: bool,
    bordered: bool,
    hoverable: bool,
    label: Option<SharedString>,
    header: Option<AnyElement>,
    footer: Option<AnyElement>,
    empty: Option<AnyElement>,
    layout: ControlLayout,
}
crate::control_layout::control_layout!(YueList);

// Lists already own their elements inline. Boxing every ordinary item adds an
// allocation to the common path solely to save space in uncommon group entries.
#[allow(clippy::large_enum_variant)]
enum Entry {
    Item(YueListItem),
    Group(YueListGroup),
}

impl YueList {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            entries: Vec::new(),
            theme: None,
            size: ListSize::Medium,
            orientation: ListLayout::Vertical,
            grid: None,
            item_orientation: ListItemLayout::Horizontal,
            split: None,
            striped: false,
            bordered: false,
            hoverable: false,
            label: None,
            header: None,
            footer: None,
            empty: None,
            layout: Default::default(),
        }
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn size(mut self, size: ListSize) -> Self {
        self.size = size;
        self
    }
    pub fn layout(mut self, layout: ListLayout) -> Self {
        self.orientation = layout;
        self.grid = None;
        self
    }
    /// Use a native, row-major card grid. Disables continuous split/striping.
    /// A later `layout` call switches back to a linear collection.
    pub fn grid(mut self, grid: ListGrid) -> Self {
        self.grid = Some(grid);
        self.orientation = ListLayout::Vertical;
        self
    }
    /// Inherited by items without an explicit item layout.
    pub fn item_layout(mut self, layout: ListItemLayout) -> Self {
        self.item_orientation = layout;
        self
    }
    /// Defaults to true vertically and false horizontally.
    pub fn split(mut self, split: bool) -> Self {
        self.split = Some(split);
        self
    }
    /// Alternates data rows, continuously across groups; vertical lists only.
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }
    /// Presentation only: does not add an action, cursor or Tab stop.
    pub fn hoverable(mut self, hoverable: bool) -> Self {
        self.hoverable = hoverable;
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn child(mut self, item: YueListItem) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }
    pub fn children(mut self, items: impl IntoIterator<Item = YueListItem>) -> Self {
        self.entries.extend(items.into_iter().map(Entry::Item));
        self
    }
    pub fn group(mut self, group: YueListGroup) -> Self {
        self.entries.push(Entry::Group(group));
        self
    }
    pub fn header(mut self, content: impl IntoElement) -> Self {
        self.header = Some(content.into_any_element());
        self
    }
    pub fn footer(mut self, content: impl IntoElement) -> Self {
        self.footer = Some(content.into_any_element());
        self
    }
    /// Used only when all data items are absent. Header/footer remain visible.
    pub fn empty(mut self, content: impl IntoElement) -> Self {
        self.empty = Some(content.into_any_element());
        self
    }
}

/// A named, non-sticky group. Empty groups are omitted, including their heading.
/// Group identity and item identities must be unique within the containing list.
pub struct YueListGroup {
    id: ElementId,
    title: SharedString,
    header: Option<AnyElement>,
    items: Vec<YueListItem>,
}
impl YueListGroup {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        let title = title.into();
        assert!(!title.trim().is_empty(), "List group requires a title");
        Self {
            id: id.into(),
            title,
            header: None,
            items: Vec::new(),
        }
    }
    /// Replaces the visible title, retaining its accessible group name.
    pub fn header(mut self, content: impl IntoElement) -> Self {
        self.header = Some(content.into_any_element());
        self
    }
    pub fn child(mut self, item: YueListItem) -> Self {
        self.items.push(item);
        self
    }
    pub fn children(mut self, items: impl IntoIterator<Item = YueListItem>) -> Self {
        self.items.extend(items);
        self
    }
}

/// A non-focusable row shell with independent content and control slots.
/// Put real YueButton/Checkbox controls in slots; the shell never handles clicks.
#[derive(IntoElement)]
pub struct YueListItem {
    id: ElementId,
    theme: Option<YueTheme>,
    size: Option<ListSize>,
    orientation: Option<ListItemLayout>,
    hoverable: Option<bool>,
    selected: bool,
    disabled: bool,
    striped: bool,
    logical_index: usize,
    set_size: Option<usize>,
    semantics: bool,
    leading: Option<AnyElement>,
    content: Option<AnyElement>,
    body: Option<AnyElement>,
    extra: Option<AnyElement>,
    actions: Option<AnyElement>,
    layout: ControlLayout,
}
crate::control_layout::control_layout!(YueListItem);
impl YueListItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            theme: None,
            size: None,
            orientation: None,
            hoverable: None,
            selected: false,
            disabled: false,
            striped: false,
            logical_index: 0,
            set_size: None,
            semantics: true,
            leading: None,
            content: None,
            body: None,
            extra: None,
            actions: None,
            layout: Default::default(),
        }
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn size(mut self, size: ListSize) -> Self {
        self.size = Some(size);
        self
    }
    pub fn layout(mut self, layout: ListItemLayout) -> Self {
        self.orientation = Some(layout);
        self
    }
    pub fn hoverable(mut self, hoverable: bool) -> Self {
        self.hoverable = Some(hoverable);
        self
    }
    /// Controlled visual state only; does not create a selection pattern.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Dims shell text and suppresses shell hover. Slots are NOT disabled:
    /// the host must explicitly pass disabled to each real action control.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// For standalone/virtual items. A containing YueList supplies this value.
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }
    /// Complete data ordinal, not the local mounted-window position.
    pub fn logical_index(mut self, index: usize) -> Self {
        self.logical_index = index;
        self
    }
    /// Disable when a surrounding virtual/composite item already owns the role.
    pub fn semantics(mut self, enabled: bool) -> Self {
        self.semantics = enabled;
        self
    }
    pub fn leading(mut self, content: impl IntoElement) -> Self {
        self.leading = Some(content.into_any_element());
        self
    }
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
    pub fn body(mut self, content: impl IntoElement) -> Self {
        self.body = Some(content.into_any_element());
        self
    }
    pub fn extra(mut self, content: impl IntoElement) -> Self {
        self.extra = Some(content.into_any_element());
        self
    }
    pub fn actions(mut self, content: impl IntoElement) -> Self {
        self.actions = Some(content.into_any_element());
        self
    }
}

/// Title/description typography; arbitrary content can be used instead.
/// Title truncates to one line by default, description wraps naturally.
#[derive(IntoElement)]
pub struct YueListItemMeta {
    title: AnyElement,
    description: Option<AnyElement>,
    theme: Option<YueTheme>,
    truncate_title: bool,
}
impl YueListItemMeta {
    pub fn new(title: impl IntoElement) -> Self {
        Self {
            title: title.into_any_element(),
            description: None,
            theme: None,
            truncate_title: true,
        }
    }
    pub fn description(mut self, content: impl IntoElement) -> Self {
        self.description = Some(content.into_any_element());
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn truncate_title(mut self, truncate: bool) -> Self {
        self.truncate_title = truncate;
        self
    }
}
