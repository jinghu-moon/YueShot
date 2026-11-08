use super::{PinTable, PinTableView};
use gpui_kit::base::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, ScrollDelta, TestAppContext, point, px, size};

fn init(cx: &mut App) {
    gpui_kit::init(cx);
    yue_component::init(
        cx,
        yue_component::YueTheme::new(
            yue_component::ThemeMode::Light,
            yue_component::Accent::Azure,
        ),
    )
    .unwrap();
    let theme = gpui_kit::base::Theme::global_mut(cx);
    theme.scrollbar = theme
        .scrollbar
        .clone()
        .with_mode(gpui_kit::base::ScrollbarMode::Always);
    super::init(cx);
}

#[gpui_kit::test]
fn large_table_scrolls_both_axes_freezes_first_column_and_resizes(cx: &mut TestAppContext) {
    cx.update(init);
    let data = PinTable::new(
        (0..2000)
            .map(|row| (0..8).map(|col| format!("{row}:{col}")).collect())
            .collect(),
        true,
    )
    .unwrap();
    let mut table = None;
    let handle = cx.open_window(size(px(480.), px(320.)), |window, cx| {
        let view = cx.new(|cx| PinTableView::new(data, window, cx));
        table = Some(view.clone());
        Root::new(view, window, cx)
    });
    let table = table.unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        let viewport = window.find("table").bounds();
        assert_eq!(
            window.find(("pin-table-header", 0usize)).bounds().left(),
            viewport.left()
        );
        assert!(
            window.try_find(("pin-table-cell", 1000 * 8usize)).is_none(),
            "off-screen rows must not be materialized"
        );
        let before = window.find(("pin-table-cell", 1usize)).bounds().left();
        window.scroll(
            ("pin-table-cell", 1usize),
            ScrollDelta::Pixels(point(px(-200.), px(0.))),
            cx,
        );
        assert!(
            table.read(cx).horizontal.offset().x < px(0.),
            "wheel must reach horizontal owner"
        );
        assert_eq!(
            window.find(("pin-table-header", 0usize)).bounds().left(),
            viewport.left()
        );
        assert_eq!(
            window.find(("pin-table-cell", 0usize)).bounds().left(),
            viewport.left()
        );
        let header = window.find(("pin-table-header", 3usize)).bounds();
        let cell = window.find(("pin-table-cell", 3usize)).bounds();
        assert_eq!(
            header.left(),
            cell.left(),
            "header and body share horizontal movement"
        );
        assert!(cell.left() < before + table.read(cx).widths[1] * 2.);

        // Scroll directly over the frozen cell; it must not swallow the vertical wheel.
        window.scroll(
            ("pin-table-cell", 0usize),
            ScrollDelta::Pixels(point(px(0.), px(-640.))),
            cx,
        );
        assert!(table.read(cx).rows.read(cx).viewport().unwrap().offset() >= 600.);
        assert!(window.try_find(("pin-table-cell", 0usize)).is_none());
        assert_eq!(
            window.find(("pin-table-header", 0usize)).bounds().top(),
            viewport.top()
        );

        let first = window.find(("pin-table-header", 0usize)).bounds();
        let from = point(first.right() - px(2.), first.center().y);
        let old_width = table.read(cx).widths[0];
        window.drag(from, from + point(px(40.), px(0.)), cx);
        assert_eq!(table.read(cx).widths[0], old_width + px(40.));
        assert_eq!(
            window
                .find(("pin-table-header", 0usize))
                .bounds()
                .size
                .width,
            old_width + px(40.)
        );
        assert!(table.read(cx).resizing.is_none());

        // The vertical thumb belongs to the visible body, even while content
        // is wider than the window and horizontally scrolled.
        let before = table.read(cx).rows.read(cx).viewport().unwrap().offset();
        let scrollbar = window.find("pin-table-vertical-scrollbar").bounds();
        assert_eq!(scrollbar.right(), viewport.right());
        assert_eq!(scrollbar.top(), viewport.top() + px(super::ROW_HEIGHT));
        let from = point(
            viewport.right() - px(8.),
            viewport.top() + px(super::ROW_HEIGHT + 16.),
        );
        let to = point(from.x, viewport.bottom() - px(24.));
        window.drag(from, to, cx);
        assert!(
            table.read(cx).rows.read(cx).viewport().unwrap().offset() > before + 10000.,
            "the vertical thumb must remain draggable at the viewport edge"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn keyboard_keeps_selected_cell_visible_and_clamps_at_edges(cx: &mut TestAppContext) {
    cx.update(init);
    let data = PinTable::new(
        (0..80)
            .map(|row| (0..8).map(|col| format!("{row}:{col}")).collect())
            .collect(),
        false,
    )
    .unwrap();
    let mut table = None;
    let handle = cx.open_window(size(px(400.), px(240.)), |window, cx| {
        let view = cx.new(|cx| PinTableView::new(data, window, cx));
        table = Some(view.clone());
        Root::new(view, window, cx)
    });
    let table = table.unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("pin-table-header", 0usize)).is_none());
        window.click(("pin-table-cell", 0usize), cx);
        window.press("up", cx);
        window.press("left", cx);
        assert_eq!(table.read(cx).selected_cell(), Some((0, 0)));
        for _ in 0..30 {
            window.press("down", cx);
        }
        for _ in 0..10 {
            window.press("right", cx);
        }
        assert_eq!(table.read(cx).selected_cell(), Some((30, 7)));
        assert_eq!(table.read(cx).selected_text().as_deref(), Some("30:7"));
        let cell = window.find(("pin-table-cell", 30 * 8 + 7usize));
        assert_eq!(cell.selected(), Some(true));
        let bounds = cell.bounds();
        let viewport = window.find("table").bounds();
        assert!(bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom());
        assert!(bounds.left() >= viewport.left() + table.read(cx).widths[0]);
        assert!(bounds.right() <= viewport.right());
        for _ in 0..60 {
            window.press("down", cx);
        }
        assert_eq!(table.read(cx).selected_cell(), Some((79, 7)));
    })
    .unwrap();
}

#[gpui_kit::test]
fn header_only_table_has_no_selectable_body(cx: &mut TestAppContext) {
    cx.update(init);
    let mut table = None;
    let handle = cx.open_window(size(px(400.), px(240.)), |window, cx| {
        let view = cx.new(|cx| {
            PinTableView::new(
                PinTable::new(vec![vec!["标题".into()]], true).unwrap(),
                window,
                cx,
            )
        });
        table = Some(view.clone());
        Root::new(view, window, cx)
    });
    let table = table.unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        table.read(cx).focus.clone().focus(window, cx);
        window.press("down", cx);
        assert!(window.find(("pin-table-header", 0usize)).visible());
        assert!(window.try_find(("pin-table-cell", 0usize)).is_none());
        assert_eq!(table.read(cx).selected_cell(), None);
        assert_eq!(table.read(cx).selected_text(), None);
    })
    .unwrap();
}

#[gpui_kit::test]
fn maximum_width_table_only_materializes_visible_columns(cx: &mut TestAppContext) {
    cx.update(init);
    let data = PinTable::new(
        (0..64)
            .map(|row| {
                (0..PinTable::MAX_COLUMNS)
                    .map(|col| format!("{row}:{col}"))
                    .collect()
            })
            .collect(),
        false,
    )
    .unwrap();
    let mut table = None;
    let handle = cx.open_window(size(px(480.), px(320.)), |window, cx| {
        let view = cx.new(|cx| PinTableView::new(data, window, cx));
        table = Some(view.clone());
        Root::new(view, window, cx)
    });
    let table = table.unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("pin-table-cell", 255usize)).is_none());
        assert!(table.read(cx).visible_columns(px(480.)).len() < 8);
        table.update(cx, |table, cx| table.select(0, 255, window, cx));
        window.render_frame(cx);
        assert!(window.find(("pin-table-cell", 255usize)).visible());
        assert!(window.try_find(("pin-table-cell", 1usize)).is_none());
        assert!(table.read(cx).visible_columns(px(480.)).len() < 8);
        window.click(("pin-table-cell", 0usize), cx);
        assert_eq!(
            table.read(cx).selected_cell(),
            Some((0, 0)),
            "the frozen cell must win over the scrolled cells behind it"
        );
        assert_eq!(table.read(cx).selected_text().as_deref(), Some("0:0"));
    })
    .unwrap();
}
