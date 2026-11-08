//! Immutable clipboard table. Yue virtualizes rows; native handles own each scroll axis.
use crate::PinTable;
#[cfg(feature = "headless-tests")]
use gpui_kit::test::TestSupportExt as _;
use gpui_kit::{
    base::{Scrollbar, StyledExt as _},
    prelude::*,
    *,
};
use std::rc::Rc;
use yue_component::{
    VirtualizerAlign, VirtualizerScrollOptions, VirtualizerSizing, VirtualizerSource,
    VirtualizerState, YueVirtualList, current_theme,
};

const ROW_HEIGHT: f32 = 32.;

actions!(pin_table, [CellUp, CellDown, CellLeft, CellRight]);
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", CellUp, Some("SnapClipPinTable")),
        KeyBinding::new("down", CellDown, Some("SnapClipPinTable")),
        KeyBinding::new("left", CellLeft, Some("SnapClipPinTable")),
        KeyBinding::new("right", CellRight, Some("SnapClipPinTable")),
    ]);
}

struct TableRows(usize);
impl VirtualizerSource for TableRows {
    fn len(&self) -> usize {
        self.0
    }
    fn key_at(&self, index: usize) -> ElementId {
        index.into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        if let ElementId::Integer(ix) = key {
            usize::try_from(*ix).ok().filter(|ix| *ix < self.0)
        } else {
            None
        }
    }
}

fn numeric_column(data: &PinTable, col: usize) -> bool {
    let mut found = false;
    let valid = data
        .rows()
        .iter()
        .skip(usize::from(data.has_header()))
        .all(|row| {
            let cell = row[col].trim();
            if cell.is_empty() {
                return true;
            }
            found = true;
            cell.strip_suffix('%')
                .unwrap_or(cell)
                .parse::<f64>()
                .is_ok_and(f64::is_finite)
        });
    valid && found
}

pub(crate) struct PinTableView {
    data: PinTable,
    widths: Vec<Pixels>,
    numeric: Vec<bool>,
    selected: Option<(usize, usize)>,
    rows: Entity<VirtualizerState>,
    horizontal: ScrollHandle,
    focus: FocusHandle,
    resizing: Option<(usize, Pixels, Pixels)>,
}
impl PinTableView {
    pub(crate) fn new(data: PinTable, window: &Window, cx: &mut Context<Self>) -> Self {
        let numeric = (0..data.columns())
            .map(|col| numeric_column(&data, col))
            .collect();
        let widths = (0..data.columns())
            .map(|col| {
                let chars = data
                    .rows()
                    .iter()
                    .map(|row| {
                        row[col]
                            .chars()
                            .map(|ch| if ch.is_ascii() { 1 } else { 2 })
                            .sum::<usize>()
                    })
                    .max()
                    .unwrap_or(8);
                rems((chars as f32 * 0.5 + 2.).clamp(6., 18.)).to_pixels(window.rem_size())
            })
            .collect();
        let count = data.rows().len() - usize::from(data.has_header());
        let rows = cx.new(|_| {
            VirtualizerState::new(
                Axis::Vertical,
                Rc::new(TableRows(count)),
                VirtualizerSizing::fixed(px(ROW_HEIGHT)),
            )
            .expect("fixed table geometry")
        });
        Self {
            data,
            widths,
            numeric,
            selected: None,
            rows,
            horizontal: ScrollHandle::new(),
            focus: cx.focus_handle().tab_stop(true),
            resizing: None,
        }
    }
    #[cfg(all(test, feature = "headless-tests"))]
    pub(crate) fn selected_cell(&self) -> Option<(usize, usize)> {
        self.selected
    }
    pub(crate) fn selected_text(&self) -> Option<String> {
        let (row, col) = self.selected?;
        self.data
            .rows()
            .get(row + usize::from(self.data.has_header()))?
            .get(col)
            .cloned()
    }
    fn select(&mut self, row: usize, col: usize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.data.rows().len() - usize::from(self.data.has_header());
        if row >= count || col >= self.data.columns() {
            return;
        }
        self.selected = Some((row, col));
        self.focus.focus(window, cx);
        self.rows.update(cx, |rows, cx| {
            rows.scroll_to_key(
                row.into(),
                VirtualizerScrollOptions::default().align(VirtualizerAlign::Nearest),
            )
            .expect("table row exists");
            cx.notify();
        });
        if col > 0 {
            let left: Pixels = self.widths[..col].iter().copied().sum();
            let right = left + self.widths[col];
            let viewport = self.horizontal.bounds().size.width;
            let mut offset = self.horizontal.offset();
            let visible_left = -offset.x + self.widths[0];
            let visible_right = -offset.x + viewport;
            if left < visible_left {
                offset.x = self.widths[0] - left;
            } else if right > visible_right {
                offset.x = viewport - right;
            }
            self.horizontal.set_offset(offset);
        }
        cx.notify();
    }
    fn move_cell(&mut self, dr: isize, dc: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.data.rows().len() - usize::from(self.data.has_header());
        if count == 0 {
            return;
        }
        let Some((row, col)) = self.selected else {
            self.select(0, 0, window, cx);
            return;
        };
        self.select(
            row.saturating_add_signed(dr).min(count - 1),
            col.saturating_add_signed(dc).min(self.data.columns() - 1),
            window,
            cx,
        );
    }
    fn cell(&self, row: Option<usize>, col: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = current_theme(cx);
        let header = row.is_none();
        let data_row = row.map_or(0, |row| row + usize::from(self.data.has_header()));
        let text = self.data.rows()[data_row][col].clone();
        let selected = row.is_some_and(|row| self.selected == Some((row, col)));
        let id = if let Some(row) = row {
            ("pin-table-cell", row * self.data.columns() + col)
        } else {
            ("pin-table-header", col)
        };
        let cell = div()
            .id(id)
            .role(if header {
                Role::ColumnHeader
            } else {
                Role::Cell
            })
            .aria_column_index(col + 1)
            .aria_row_index(row.map_or(1, |row| row + 1 + usize::from(self.data.has_header())))
            .aria_selected(selected)
            .when(selected, |cell| cell.aria_active_descendant())
            .aria_label(text.clone())
            .relative()
            .h(px(ROW_HEIGHT))
            .w(self.widths[col])
            .flex_none()
            .overflow_hidden()
            .px_3()
            .h_flex()
            .items_center()
            .border_b_1()
            .border_r_1()
            .border_color(rgb(theme.border_default()))
            .bg(rgb(if selected {
                theme.accent_subtle()
            } else if header {
                theme.surface_component()
            } else {
                theme.page()
            }))
            .text_color(rgb(theme.text_primary()))
            .when(selected, |cell| {
                cell.border_1().border_color(rgb(theme.accent_solid()))
            })
            .when(self.numeric[col], |s| s.justify_end())
            .when(header, |s| s.font_weight(FontWeight::SEMIBOLD))
            .child(div().truncate().child(text));
        let cell = if let Some(row) = row {
            cell.on_click(cx.listener(move |this, _, window, cx| {
                this.select(row, col, window, cx);
                cx.stop_propagation();
            }))
        } else {
            cell.child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .h_full()
                    .w(px(5.))
                    .cursor_col_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.resizing = Some((col, event.position.x, this.widths[col]));
                            cx.stop_propagation();
                        }),
                    ),
            )
        };
        #[cfg(feature = "headless-tests")]
        let cell = cell.test_support();
        cell.into_any_element()
    }
    fn visible_columns(&self, viewport: Pixels) -> std::ops::Range<usize> {
        let offset = -self.horizontal.offset().x;
        let mut left = self.widths[0];
        let mut visible = 1..1;
        for col in 1..self.widths.len() {
            let right = left + self.widths[col];
            if right > offset + self.widths[0] && left < offset + viewport {
                if visible.is_empty() {
                    visible.start = col;
                }
                visible.end = col + 1;
            }
            left = right;
        }
        visible
    }
    fn row(&self, row: Option<usize>, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        // One horizontal owner moves header and body. The final, opaque first cell
        // counter-translates that offset. Do not occlude ancestor hitboxes: native
        // wheel scrolling must still work over the frozen column.
        let offset = -self.horizontal.offset().x;
        let viewport = self.horizontal.bounds().size.width;
        let visible = self.visible_columns(if viewport > px(0.) {
            viewport
        } else {
            window.viewport_size().width
        });
        let leading: Pixels = self.widths[..visible.start].iter().copied().sum();
        let trailing: Pixels = self.widths[visible.end..].iter().copied().sum();
        div()
            .id(("pin-table-row", row.map_or(0, |ix| ix + 1)))
            .role(Role::Row)
            .aria_row_index(row.map_or(1, |row| row + 1 + usize::from(self.data.has_header())))
            .relative()
            .h(px(ROW_HEIGHT))
            .w_full()
            .h_flex()
            .child(div().w(leading).flex_none())
            .children(visible.map(|col| self.cell(row, col, cx)))
            .child(div().w(trailing).flex_none())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(offset)
                    .child(self.cell(row, 0, cx)),
            )
            .into_any_element()
    }
}

#[cfg(all(test, feature = "headless-tests"))]
#[path = "table_tests.rs"]
mod interaction_tests;
impl Focusable for PinTableView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PinTableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let width: Pixels = self.widths.iter().copied().sum();
        let vertical = self
            .rows
            .read(cx)
            .scrollbar_handle()
            .expect("owned table rows");
        let vertical_scrollbar = div()
            .id("pin-table-vertical-scrollbar")
            .absolute()
            .top(px(if self.data.has_header() {
                ROW_HEIGHT
            } else {
                0.
            }))
            .bottom_0()
            .left_0()
            .right_0()
            .child(Scrollbar::vertical(&vertical).viewport_from_layout());
        #[cfg(feature = "headless-tests")]
        let vertical_scrollbar = vertical_scrollbar.test_support();
        let content = div()
            .v_flex()
            .h_full()
            .w(width)
            .min_w_full()
            .when(self.data.has_header(), |s| {
                s.child(self.row(None, window, cx))
            })
            .child(
                YueVirtualList::new("pin-table-rows", &self.rows, move |item, window, cx| {
                    owner
                        .update(cx, |table, cx| table.row(Some(item.index()), window, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                })
                .scrollbar(false)
                .semantics(Some(Role::RowGroup), None)
                .flex_1()
                .min_h_0(),
            );
        let table = div()
            .id("table")
            .role(Role::Table)
            .aria_label("剪贴板表格")
            .aria_row_count(self.data.rows().len())
            .aria_column_count(self.data.columns())
            .key_context("SnapClipPinTable")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &CellUp, w, cx| this.move_cell(-1, 0, w, cx)))
            .on_action(cx.listener(|this, _: &CellDown, w, cx| this.move_cell(1, 0, w, cx)))
            .on_action(cx.listener(|this, _: &CellLeft, w, cx| this.move_cell(0, -1, w, cx)))
            .on_action(cx.listener(|this, _: &CellRight, w, cx| this.move_cell(0, 1, w, cx)))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some((col, start, width)) = this.resizing {
                    if event.pressed_button == Some(MouseButton::Left) {
                        this.widths[col] = (width + event.position.x - start).max(px(48.));
                        cx.notify();
                    } else {
                        this.resizing = None;
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.resizing = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.resizing = None),
            )
            .child(
                div()
                    .id("pin-table-horizontal")
                    .size_full()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.horizontal)
                    .child(content),
            )
            .child(vertical_scrollbar)
            .child(Scrollbar::horizontal(&self.horizontal));
        #[cfg(feature = "headless-tests")]
        let table = table.test_support();
        table
    }
}

#[cfg(test)]
mod tests {
    use super::numeric_column;
    use crate::PinTable;
    #[test]
    fn only_unambiguous_finite_numbers_use_numeric_alignment() {
        for (values, expected) in [
            (vec!["1", "-2.5", "3e2", ""], true),
            (vec!["25%", "0.5%", ""], true),
            (vec!["NaN", "1"], false),
            (vec!["inf", "1"], false),
            (vec!["", " "], false),
            (vec!["1", "项目2"], false),
            (vec!["1,000", "200"], false),
        ] {
            let table = PinTable::new(
                std::iter::once(vec!["表头".into()])
                    .chain(values.into_iter().map(|v| vec![v.to_string()]))
                    .collect(),
                true,
            )
            .unwrap();
            assert_eq!(numeric_column(&table, 0), expected);
        }
    }
}
