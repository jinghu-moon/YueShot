use super::*;
use crate::{
    theme::{TokenKey, keys},
    tokens::{rems, rgba},
};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{
    App, Div, InteractiveElement as _, ParentElement as _, RenderOnce, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

impl ListSize {
    fn spacing(self) -> (TokenKey, TokenKey) {
        match self {
            Self::Small => (keys::SPACE_4, keys::SPACE_8),
            Self::Medium => (keys::SPACE_8, keys::SPACE_12),
            Self::Large => (keys::SPACE_12, keys::SPACE_16),
        }
    }
    fn text(self) -> (TokenKey, TokenKey) {
        match self {
            Self::Small => (keys::FONT_SIZE_SM, keys::LINE_HEIGHT_SM),
            Self::Medium => (keys::FONT_SIZE_MD, keys::LINE_HEIGHT_MD),
            Self::Large => (keys::FONT_SIZE_LG, keys::LINE_HEIGHT_LG),
        }
    }
}

// A filled strip avoids the single-sided Windows border shader artefact.
// It participates in layout, so fixed-size virtual items must include it.
fn separator(theme: YueTheme, horizontal: bool) -> Div {
    let line = div().flex_shrink_0().bg(rgba(theme, keys::BORDER_DIVIDER));
    if horizontal {
        line.w(rems(theme, keys::DIVIDER_BORDER_WIDTH))
            .self_stretch()
    } else {
        line.h(rems(theme, keys::DIVIDER_BORDER_WIDTH)).w_full()
    }
}

fn band(
    id: &'static str,
    content: AnyElement,
    theme: YueTheme,
    size: ListSize,
) -> impl IntoElement {
    let (block, inline) = size.spacing();
    div()
        .id(id)
        .test_support()
        .min_w_0()
        .flex_shrink_0()
        .px(rems(theme, inline))
        .py(rems(theme, block))
        .child(content)
}

impl RenderOnce for YueList {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let horizontal = self.orientation == ListLayout::Horizontal;
        let split = self.grid.is_none() && self.split.unwrap_or(!horizontal);
        let grid_id = (self.id.clone(), "grid");
        // Validate the seam once, before consuming any user elements.
        let mut ids = std::collections::HashSet::new();
        let mut count = 0;
        for entry in &self.entries {
            match entry {
                Entry::Item(item) => {
                    assert!(
                        ids.insert(item.id.clone()),
                        "List requires unique item/group IDs"
                    );
                    count += 1;
                }
                Entry::Group(group) => {
                    assert!(
                        ids.insert(group.id.clone()),
                        "List requires unique item/group IDs"
                    );
                    for item in &group.items {
                        assert!(
                            ids.insert(item.id.clone()),
                            "List requires unique item/group IDs"
                        );
                        count += 1;
                    }
                }
            }
        }
        let (font, line_height) = self.size.text();
        // Continuous rows and bands need normal block flow. Nested column flex
        // containers repeatedly query intrinsic text sizes during page layout.
        // Keep flex for horizontal rows, and let GridBody select native grid.
        let mut root = div()
            .id(self.id)
            .test_support()
            .w_full()
            .min_w_0()
            .block()
            .flex_shrink_0()
            .text_size(rems(theme, font))
            .line_height(rems(theme, line_height))
            .text_color(rgba(theme, keys::LIST_ROW_COLOR));
        if self.bordered {
            root = root
                .border_1()
                .border_color(rgba(theme, keys::BORDER_DEFAULT))
                .rounded(rems(theme, keys::RADIUS_MD));
        }
        // Round painted backgrounds directly; do not clip child focus rings.
        let has_header = self.header.is_some();
        let has_footer = self.footer.is_some();
        if let Some(header) = self.header {
            root = root.child(band("list-header", header, theme, self.size));
            if count > 0 || self.empty.is_some() || has_footer {
                root = root.child(separator(theme, false));
            }
        }
        let mut body = div()
            .id("list-body")
            .test_support()
            .min_w_0()
            .block()
            .flex_shrink_0()
            .role(Role::List)
            .when(horizontal, |el| el.flex());
        if let Some(label) = self.label {
            body = body.aria_label(label);
        }
        let entries: Vec<_> = self
            .entries
            .into_iter()
            .filter(|entry| match entry {
                Entry::Group(group) => !group.items.is_empty(),
                Entry::Item(_) => true,
            })
            .collect();
        let entry_count = entries.len();
        let mut ordinal = 0;
        for (entry_ix, entry) in entries.into_iter().enumerate() {
            if entry_ix > 0 && split {
                body = body.child(separator(theme, horizontal));
            }
            let inherit = |mut item: YueListItem, ix| {
                item.theme = item.theme.or(Some(theme));
                item.size = item.size.or(Some(self.size));
                item.orientation = item.orientation.or(Some(self.item_orientation));
                item.hoverable = item.hoverable.or(Some(self.hoverable));
                item.striped = self.striped && !horizontal && self.grid.is_none();
                item.logical_index = ix;
                item
            };
            match entry {
                Entry::Item(item) => {
                    let item = inherit(item, ordinal);
                    ordinal += 1;
                    body = body.child(item.surface(
                        horizontal,
                        self.grid.is_none() && self.bordered && !has_header && entry_ix == 0,
                        self.grid.is_none()
                            && self.bordered
                            && !has_footer
                            && entry_ix + 1 == entry_count,
                    ));
                }
                Entry::Group(group) => {
                    let group_grid_id = (group.id.clone(), "grid");
                    let mut container = div()
                        .id(group.id)
                        .test_support()
                        .min_w_0()
                        .block()
                        .flex_shrink_0()
                        .role(Role::ListItem);
                    if self.grid.is_some() {
                        container = container.col_span_full();
                    }
                    let header = group
                        .header
                        .unwrap_or_else(|| group.title.clone().into_any_element());
                    container =
                        container.child(band("list-group-header", header, theme, self.size));
                    let mut children = div()
                        .id("list-group-body")
                        .test_support()
                        .min_w_0()
                        .block()
                        .role(Role::List)
                        .aria_label(group.title);
                    let len = group.items.len();
                    for (ix, item) in group.items.into_iter().enumerate() {
                        if ix > 0 && split {
                            children = children.child(separator(theme, false));
                        }
                        children = children.child(inherit(item, ordinal).surface(
                            false,
                            false,
                            self.grid.is_none()
                                && self.bordered
                                && !has_footer
                                && entry_ix + 1 == entry_count
                                && ix + 1 == len,
                        ));
                        ordinal += 1;
                    }
                    body = if let Some(grid) = self.grid {
                        body.child(container.child(GridBody {
                            id: group_grid_id.into(),
                            grid,
                            base: children,
                        }))
                    } else {
                        body.child(container.child(children))
                    };
                }
            }
        }
        let has_body = count > 0 || self.empty.is_some();
        if count == 0 {
            if let Some(empty) = self.empty {
                // Empty status is not a list item. Do not emit a phantom row.
                root = root.child(band("list-empty", empty, theme, self.size));
            }
        } else {
            root = if let Some(grid) = self.grid {
                root.child(GridBody {
                    id: grid_id.into(),
                    grid,
                    base: body,
                })
            } else {
                root.child(body)
            };
        }
        if let Some(footer) = self.footer {
            if has_body {
                root = root.child(separator(theme, false));
            }
            root = root.child(band("list-footer", footer, theme, self.size));
        }
        self.layout.apply(root)
    }
}

/// Retains geometry only. Native grid owns tracks, natural row heights and stretch.
/// Bounds are captured after layout; changed widths converge on the next frame.
#[derive(IntoElement)]
struct GridBody<T: IntoElement + gpui_kit::Styled + gpui_kit::ParentElement + 'static> {
    id: gpui_kit::ElementId,
    grid: ListGrid,
    base: T,
}
impl<T: IntoElement + gpui_kit::Styled + gpui_kit::ParentElement + 'static> RenderOnce
    for GridBody<T>
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let width = window.use_keyed_state(self.id, cx, |_, _| gpui_kit::px(0.));
        let geometry = self
            .grid
            .resolve(*width.read(cx), window.rem_size())
            .expect("valid list grid geometry");
        let capture = gpui_kit::canvas(
            move |bounds, window, cx| {
                if *width.read(cx) != bounds.size.width {
                    let next = bounds.size.width;
                    window.defer(cx, move |_, cx| {
                        width.update(cx, |width, cx| {
                            *width = next;
                            cx.notify();
                        })
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        self.base
            .relative()
            .grid()
            .grid_cols(geometry.columns())
            .gap(geometry.gap())
            .child(capture)
    }
}

impl YueListItem {
    fn surface(self, horizontal: bool, first: bool, last: bool) -> impl IntoElement {
        // RenderOnce wrapper preserves inherited settings without another Entity.
        ItemSurface {
            item: self,
            horizontal,
            first,
            last,
        }
    }
}
#[derive(IntoElement)]
struct ItemSurface {
    item: YueListItem,
    horizontal: bool,
    first: bool,
    last: bool,
}
impl RenderOnce for ItemSurface {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let item = self.item;
        let theme = item.theme.unwrap_or_else(|| crate::current_theme(cx));
        let size = item.size.unwrap_or_default();
        let (block, inline) = size.spacing();
        let (font, line) = size.text();
        let vertical = item.orientation.unwrap_or_default() == ListItemLayout::Vertical;
        let mut root = div()
            .id(item.id)
            .test_support()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_shrink_0()
            .items_center()
            .px(rems(theme, inline))
            .py(rems(theme, block))
            .gap(rems(theme, inline))
            .text_size(rems(theme, font))
            .line_height(rems(theme, line))
            .text_color(rgba(
                theme,
                if item.disabled {
                    keys::TEXT_DISABLED
                } else {
                    keys::LIST_ROW_COLOR
                },
            ))
            .when(!self.horizontal, |el| el.w_full())
            .when(self.horizontal && item.layout.width.is_none(), |el| {
                el.whitespace_nowrap()
            })
            .when(vertical, |el| el.items_start());
        if item.semantics {
            root = root.role(Role::ListItem);
            if let Some(count) = item.set_size {
                root = root
                    .aria_position_in_set(item.logical_index + 1)
                    .aria_size_of_set(count);
            }
        }
        if self.horizontal {
            root = root
                .when(self.first, |el| el.rounded_l(rems(theme, keys::RADIUS_MD)))
                .when(self.last, |el| el.rounded_r(rems(theme, keys::RADIUS_MD)));
        } else {
            root = root
                .when(self.first, |el| el.rounded_t(rems(theme, keys::RADIUS_MD)))
                .when(self.last, |el| el.rounded_b(rems(theme, keys::RADIUS_MD)));
        }
        if item.selected {
            root = root.bg(rgba(theme, keys::LIST_ROW_BACKGROUND_SELECTED));
        } else if item.striped && item.logical_index % 2 == 1 {
            root = root.bg(rgba(theme, keys::SURFACE_SUBTLE));
        }
        if !item.disabled && !item.selected && item.hoverable.unwrap_or(false) {
            root = root.hover(|style| style.bg(rgba(theme, keys::LIST_ROW_BACKGROUND_HOVER)));
        }
        let leading = item.leading.map(|el| {
            div()
                .id("list-leading")
                .test_support()
                .flex_shrink_0()
                .child(el)
        });
        let mut main = div()
            .id("list-main")
            .test_support()
            .min_w_0()
            .when(!self.horizontal, |el| el.flex_1())
            .when(self.horizontal, |el| el.flex_none())
            .flex()
            .flex_col()
            .gap(rems(theme, keys::SPACE_4));
        if vertical {
            let mut meta = div()
                .id("list-meta-row")
                .test_support()
                .min_w_0()
                .flex()
                .items_center()
                .gap(rems(theme, inline));
            if let Some(leading) = leading {
                meta = meta.child(leading);
            }
            if let Some(content) = item.content {
                meta = meta.child(
                    div()
                        .id("list-content")
                        .test_support()
                        .min_w_0()
                        .flex_1()
                        .child(content),
                );
            }
            main = main.child(meta);
        } else {
            if let Some(leading) = leading {
                root = root.child(leading);
            }
            if let Some(content) = item.content {
                main = main.child(
                    div()
                        .id("list-content")
                        .test_support()
                        .min_w_0()
                        .child(content),
                );
            }
        }
        if let Some(body) = item.body {
            main = main.child(
                div()
                    .id("list-item-body")
                    .test_support()
                    .min_w_0()
                    .child(body),
            );
        }
        let actions = item.actions.map(|el| {
            div()
                .id("list-actions")
                .test_support()
                .flex()
                .flex_wrap()
                .items_center()
                .flex_shrink_0()
                .gap(rems(theme, keys::SPACE_8))
                .child(el)
        });
        let actions = if vertical {
            if let Some(actions) = actions {
                main = main.child(actions);
            }
            None
        } else {
            actions
        };
        root = root.child(main);
        if let Some(extra) = item.extra {
            root = root.child(
                div()
                    .id("list-extra")
                    .test_support()
                    .flex_shrink_0()
                    .child(extra),
            );
        }
        if let Some(actions) = actions {
            root = root.child(actions);
        }
        item.layout.apply(root)
    }
}
impl RenderOnce for YueListItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.surface(false, false, false)
    }
}
impl RenderOnce for YueListItemMeta {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let title = div()
            .id("list-meta-title")
            .test_support()
            .min_w_0()
            .when(self.truncate_title, |el| el.truncate())
            .child(self.title);
        let mut root = div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(rems(theme, keys::SPACE_4))
            .child(title);
        if let Some(description) = self.description {
            root = root.child(
                div()
                    .id("list-meta-description")
                    .test_support()
                    .min_w_0()
                    .text_color(rgba(theme, keys::TEXT_SECONDARY))
                    .child(description),
            );
        }
        root
    }
}
