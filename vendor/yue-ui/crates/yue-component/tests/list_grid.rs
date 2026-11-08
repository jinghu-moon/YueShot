use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Bounds, Context, ElementId, Entity, FocusHandle, IntoElement,
    ParentElement as _, Pixels, Render, Role, Styled as _, TestAppContext, VisualTestContext,
    Window, div, px, rems,
};

use std::{cell::Cell, collections::HashMap, rc::Rc};
use yue_component::{
    Accent, ListGrid, ListGridSizing, ListGridState, ThemeMode, VirtualizerAlign,
    VirtualizerScrollOptions, VirtualizerSource, YueButton, YueList, YueListItem, YueListItemMeta,
    YueTheme, YueVirtualListGrid,
};
#[path = "list_grid/performance.rs"]
mod performance;

struct GroupProbe;
impl Render for GroupProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        YueList::new("grouped-cards")
            .grid(ListGrid::columns(3, px(12.)).unwrap())
            .width(px(600.))
            .header("Cards header")
            .footer("Cards footer")
            .group(yue_component::YueListGroup::new("empty-group", "Omitted"))
            .group(
                yue_component::YueListGroup::new("card-group", "Projects")
                    .children((0..5).map(|ix| card(ix.into(), ix, ix % 3 == 1))),
            )
            .child(card(5_usize.into(), 5, false))
    }
}
#[gpui_kit::test]
fn card_groups_and_bands_keep_full_width_across_themes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
    });
    let (_, cx) = cx.add_window_view(|w, cx| {
        let view = cx.new(|_| GroupProbe);
        gpui_kit::base::Root::new(view, w, cx)
    });
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            cx.update(|_, cx| yue_component::set_theme(cx, YueTheme::new(mode, accent)));
            draw(cx);
            let parts = nodes(cx);
            let group = parts
                .iter()
                .find(|n| n.path().last() == Some(&ElementId::from("card-group")))
                .unwrap()
                .bounds();
            close(group.size.width, px(600.));
            for id in ["list-header", "list-footer"] {
                let band = parts
                    .iter()
                    .find(|n| n.path().last() == Some(&ElementId::from(id)))
                    .unwrap()
                    .bounds();
                close(band.size.width, group.size.width);
            }
            assert!(
                !parts
                    .iter()
                    .any(|n| n.path().contains(&ElementId::from("empty-group")))
            );
            let first = bounds(cx, "grouped-cards", 0);
            close(first.size.width, px(192.));
            close(bounds(cx, "grouped-cards", 4).size.width, first.size.width);
            assert!(bounds(cx, "grouped-cards", 5).top() >= group.bottom());
        }
    }
}

#[derive(Clone)]
struct Records {
    keys: Vec<usize>,
    reverse: HashMap<usize, usize>,
    revision: u64,
}
impl Records {
    fn from(keys: Vec<usize>, revision: u64) -> Rc<Self> {
        Rc::new(Self {
            reverse: keys
                .iter()
                .enumerate()
                .map(|(ix, key)| (*key, ix))
                .collect(),
            keys,
            revision,
        })
    }
}
impl VirtualizerSource for Records {
    fn len(&self) -> usize {
        self.keys.len()
    }
    fn key_at(&self, ix: usize) -> ElementId {
        self.keys[ix].into()
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        if let ElementId::Integer(key) = key {
            self.reverse.get(&(*key as usize)).copied()
        } else {
            None
        }
    }
    fn revision_at(&self, _: usize) -> u64 {
        self.revision
    }
}
fn card(key: ElementId, index: usize, tall: bool) -> YueListItem {
    YueListItem::new(key)
        .content(
            YueListItemMeta::new(format!("Card {index}"))
                .description("内容随宽度自然换行。Card description with wrapping text."),
        )
        .body(div().h(px(if tall { 92. } else { 20. })))
}
struct Probe {
    width: f32,
    grid: ListGrid,
    count: usize,
    state: Entity<ListGridState>,
    builds: Rc<Cell<usize>>,
    actions: Rc<Cell<usize>>,
    focus: FocusHandle,
    fallback: FocusHandle,
    tall: bool,
    virtual_only: bool,
}
impl Probe {
    fn new(count: usize, sizing: ListGridSizing, cx: &mut Context<Self>) -> Self {
        let grid = ListGrid::min_width(px(180.), px(12.)).unwrap();
        let state = cx.new(|cx| {
            ListGridState::new(Records::from((0..count).collect(), 0), grid, sizing, cx).unwrap()
        });
        let focus = cx.focus_handle().tab_stop(true);
        let fallback = cx.focus_handle().tab_stop(true);
        if count > 7 {
            state.update(cx, |state, _| {
                state
                    .set_focus_handle(7_usize.into(), Some(focus.clone()))
                    .unwrap();
                state.set_focus_fallback(Some(fallback.clone()));
            });
        }
        Self {
            width: 600.,
            grid,
            count,
            state,
            builds: Rc::new(Cell::new(0)),
            actions: Rc::new(Cell::new(0)),
            focus,
            fallback,
            tall: false,
            virtual_only: false,
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.builds.clone();
        let focus = self.focus.clone();
        let actions = self.actions.clone();
        let tall = self.tall;
        let virtual_list =
            YueVirtualListGrid::new("virtual-cards", &self.state, move |index, key, _, _| {
                builds.set(builds.get() + 1);
                let mut item = card(key.clone(), index, index % 3 == 1);
                if tall {
                    item = item.body(div().h(px(170.)));
                }
                if *key == ElementId::from(7_usize) {
                    let actions = actions.clone();
                    item.actions(
                        YueButton::new("edit-card", "Edit")
                            .track_focus(&focus)
                            .on_click(move |_, _, _| actions.set(actions.get() + 1)),
                    )
                } else {
                    item
                }
            })
            .scrollbar(false)
            .accessibility_label("Virtual cards");
        let mut root = div().w(px(self.width)).flex().flex_col();
        if !self.virtual_only {
            root = root.child(
                YueList::new("ordinary-cards")
                    .grid(self.grid)
                    .split(true)
                    .striped(true)
                    .children((0..self.count.min(10)).map(|ix| {
                        let item = card(ix.into(), ix, ix % 3 == 1);
                        if tall {
                            item.body(div().h(px(170.)))
                        } else {
                            item
                        }
                    })),
            );
        }
        root.child(div().h(px(320.)).w_full().child(virtual_list))
            .child(YueButton::new("fallback", "After cards").track_focus(&self.fallback))
    }
}
fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}
fn open(
    cx: &mut TestAppContext,
    count: usize,
    sizing: ListGridSizing,
) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
    });
    let mut view = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let probe = cx.new(|cx| Probe::new(count, sizing, cx));
        view = Some(probe.clone());
        gpui_kit::base::Root::new(probe, w, cx)
    });
    draw(cx);
    (view.unwrap(), cx)
}
fn nodes(cx: &mut VisualTestContext) -> Vec<ElementSnapshot> {
    cx.update(|w, _| snapshots(w))
}
fn bounds(cx: &mut VisualTestContext, list: &str, key: usize) -> Bounds<Pixels> {
    nodes(cx)
        .into_iter()
        .find(|n| {
            n.path().contains(&ElementId::from(list.to_string()))
                && n.path().last() == Some(&ElementId::from(key))
                && n.role() == Some(Role::ListItem)
        })
        .unwrap_or_else(|| panic!("missing {list}/{key}"))
        .bounds()
}
fn close(a: Pixels, b: Pixels) {
    assert!((a - b).abs() < px(0.6), "{a:?} != {b:?}");
}
fn state(view: &Entity<Probe>, cx: &mut VisualTestContext) -> Entity<ListGridState> {
    view.read_with(cx, |v, _| v.state.clone())
}

#[test]
fn configuration_rejects_invalid_values_and_resolves_breakpoints() {
    assert!(ListGrid::columns(0, px(0.)).is_err());
    for value in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(ListGrid::min_width(px(value), px(0.)).is_err());
    }
    for value in [-1., f32::NAN, f32::INFINITY] {
        assert!(ListGrid::columns(2, px(value)).is_err());
    }
    let grid = ListGrid::min_width(rems(10.), rems(1.)).unwrap();
    for (width, columns) in [(0., 1), (335., 1), (336., 2), (511., 2), (512., 3)] {
        assert_eq!(grid.resolve(px(width), px(16.)).unwrap().columns(), columns);
    }
    assert!(grid.resolve(px(f32::NAN), px(16.)).is_err());
    assert!(grid.resolve(px(100.), px(0.)).is_err());
    assert_eq!(
        ListGrid::columns(3, px(12.))
            .unwrap()
            .resolve(px(0.), px(16.))
            .unwrap()
            .card_width(),
        px(0.)
    );
}

#[gpui_kit::test]
fn native_grid_and_measured_rows_agree_on_real_bounds_and_partial_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 10, ListGridSizing::measured(px(140.)));
    for width in [600., 390., 170., 600.] {
        view.update(cx, |view, cx| {
            view.width = width;
            cx.notify();
        });
        draw(cx);
        let geometry = state(&view, cx).read_with(cx, |state, _| state.geometry());
        let columns = usize::from(geometry.columns());
        for ix in 0..columns.min(10) {
            let a = bounds(cx, "ordinary-cards", ix);
            let b = bounds(cx, "virtual-cards", ix);
            close(a.size.width, geometry.card_width());
            close(a.size.width, b.size.width);
            close(a.size.height, b.size.height);
            close(a.left(), b.left());
        }
        let a = bounds(cx, "ordinary-cards", 0);
        let next = bounds(cx, "ordinary-cards", columns);
        close(next.top() - a.bottom(), px(12.));
        let last = bounds(cx, "ordinary-cards", 9);
        close(last.size.width, a.size.width);
        assert_eq!(
            nodes(cx)
                .iter()
                .filter(|n| n.path().contains(&ElementId::from("ordinary-cards"))
                    && n.role() == Some(Role::ListItem))
                .count(),
            10
        );
    }
}

#[gpui_kit::test]
fn responsive_grid_uses_nested_bounds_and_rem_and_fixed_columns_stay_fixed(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, 5, ListGridSizing::measured(px(140.)));
    let grid = ListGrid::min_width(rems(10.), rems(1.)).unwrap();
    view.update(cx, |v, cx| {
        v.width = 512.;
        v.grid = grid;
        v.state.update(cx, |s, cx| s.set_grid(grid, cx).unwrap());
        cx.notify();
    });
    for (rem, columns) in [(16., 3), (20., 2), (12., 3)] {
        cx.update(|w, _| w.set_rem_size(px(rem)));
        draw(cx);
        assert_eq!(
            state(&view, cx).read_with(cx, |s, _| s.geometry().columns()),
            columns
        );
        let first = bounds(cx, "ordinary-cards", 0);
        let last = bounds(cx, "ordinary-cards", usize::from(columns - 1));
        close(first.top(), last.top());
        close(last.right() - first.left(), px(512.));
    }
    let fixed = ListGrid::columns(4, px(12.)).unwrap();
    view.update(cx, |v, cx| {
        v.width = 170.;
        v.grid = fixed;
        v.state.update(cx, |s, cx| s.set_grid(fixed, cx).unwrap());
        cx.notify();
    });
    draw(cx);
    close(
        bounds(cx, "ordinary-cards", 0).top(),
        bounds(cx, "ordinary-cards", 3).top(),
    );
    close(bounds(cx, "ordinary-cards", 0).size.width, px(33.5));
}

#[gpui_kit::test]
fn reflow_preserves_business_reading_key_and_intra_row_offset(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 10_000, ListGridSizing::measured(px(140.)));
    view.update(cx, |v, cx| {
        v.virtual_only = true;
        cx.notify();
    });
    let state = state(&view, cx);
    state.update(cx, |s, cx| {
        s.scroll_to_key(&603_usize.into(), VirtualizerAlign::Start.into(), cx)
            .unwrap()
    });
    draw(cx);
    let base = state.read_with(cx, |s, _| s.virtualizer().clone());
    base.update(cx, |s, cx| {
        s.scroll_by(23., VirtualizerScrollOptions::default())
            .unwrap();
        cx.notify();
    });
    draw(cx);
    let before = bounds(cx, "virtual-cards", 603).top();
    for width in [390., 790., 600.] {
        view.update(cx, |v, cx| {
            v.width = width;
            cx.notify();
        });
        draw(cx);
        close(bounds(cx, "virtual-cards", 603).top(), before);
    }
    // Inserting/removing/reversing source records must route the same domain key.
    for keys in [
        (20_000..20_005).chain(0..10_000).collect(),
        (0..10_000).rev().collect(),
    ] {
        state.update(cx, |s, cx| {
            s.replace_source(
                Records::from(keys, 1),
                ListGridSizing::measured(px(140.)),
                cx,
            )
            .unwrap()
        });
        draw(cx);
        close(bounds(cx, "virtual-cards", 603).top(), before);
    }
    assert!(
        view.read_with(cx, |v, _| v.builds.get()) < 2_000,
        "reflow must not render all ten thousand cards"
    );
}

#[gpui_kit::test]
fn focused_card_survives_reparenting_remote_unload_and_deleted_key_uses_fallback(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, 10_000, ListGridSizing::measured(px(140.)));
    view.update(cx, |v, cx| {
        v.virtual_only = true;
        cx.notify();
    });
    let state = state(&view, cx);
    state.update(cx, |s, cx| {
        s.scroll_to_key(&7_usize.into(), VirtualizerAlign::Start.into(), cx)
            .unwrap()
    });
    draw(cx);
    let focus = view.read_with(cx, |v, _| v.focus.clone());
    cx.update(|w, cx| focus.focus(w, cx));
    draw(cx);
    for width in [390., 790.] {
        view.update(cx, |v, cx| {
            v.width = width;
            cx.notify();
        });
        draw(cx);
        assert!(cx.update(|w, _| focus.is_focused(w)));
        assert!(
            nodes(cx)
                .iter()
                .any(|n| n.path().last() == Some(&ElementId::from("edit-card"))
                    && n.focused() == Some(true))
        );
    }
    let center = nodes(cx)
        .into_iter()
        .find(|n| n.path().last() == Some(&ElementId::from("edit-card")))
        .unwrap()
        .bounds()
        .center();
    cx.simulate_click(center, Default::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.actions.get()), 1);
    let keystroke = gpui_kit::Keystroke::parse("enter").unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.actions.get()), 2);
    state.update(cx, |s, cx| {
        s.scroll_to_key(&9_900_usize.into(), VirtualizerAlign::Start.into(), cx)
            .unwrap()
    });
    draw(cx);
    assert!(cx.update(|w, _| focus.is_focused(w)));
    bounds(cx, "virtual-cards", 7); // Retained remotely, no intervening cards.
    assert!(
        nodes(cx)
            .iter()
            .filter(|n| n.role() == Some(Role::ListItem))
            .count()
            < 80
    );
    state.update(cx, |s, cx| {
        s.replace_source(
            Records::from((0..10_000).filter(|ix| *ix != 7).collect(), 1),
            ListGridSizing::measured(px(140.)),
            cx,
        )
        .unwrap()
    });
    draw(cx);
    let fallback = view.read_with(cx, |v, _| v.fallback.clone());
    assert!(cx.update(|w, _| fallback.is_focused(w)));
}

#[gpui_kit::test]
fn content_size_feedback_remeasures_rows_without_losing_the_reading_position(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, 100, ListGridSizing::measured(px(140.)));
    view.update(cx, |v, cx| {
        v.virtual_only = true;
        cx.notify();
    });
    let state = state(&view, cx);
    state.update(cx, |s, cx| {
        s.scroll_to_key(&30_usize.into(), VirtualizerAlign::Start.into(), cx)
            .unwrap()
    });
    draw(cx);
    let before = bounds(cx, "virtual-cards", 30);
    view.update(cx, |v, cx| {
        v.tall = true;
        v.state.update(cx, |s, cx| s.invalidate(cx).unwrap());
        cx.notify();
    });
    draw(cx);
    let after = bounds(cx, "virtual-cards", 30);
    close(before.top(), after.top());
    assert!(after.size.height > before.size.height + px(70.));
    let following = bounds(cx, "virtual-cards", 33);
    close(following.top() - after.bottom(), px(12.));
}

#[gpui_kit::test]
fn fixed_and_known_card_heights_use_existing_base_sizing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 10, ListGridSizing::known(vec![px(180.); 10]));
    let state = state(&view, cx);
    assert_eq!(
        state.read_with(cx, |s, cx| s.virtualizer().read(cx).total_extent()),
        756.
    );
    close(bounds(cx, "virtual-cards", 0).size.height, px(180.));
    // Known heights are regrouped by max, including the final partial row.
    state.update(cx, |s, cx| {
        s.replace_source(
            Records::from((0..10).collect(), 0),
            ListGridSizing::known(vec![
                px(200.),
                px(170.),
                px(220.),
                px(180.),
                px(180.),
                px(180.),
                px(180.),
                px(180.),
                px(180.),
                px(190.),
            ]),
            cx,
        )
        .unwrap()
    });
    draw(cx);
    close(bounds(cx, "virtual-cards", 0).size.height, px(220.));
    assert_eq!(
        state.read_with(cx, |s, cx| s.virtualizer().read(cx).total_extent()),
        806.
    );
    // Changing sizing kinds requires rebuilding the controller, exactly as Base.
    assert!(
        state
            .update(cx, |s, cx| s.replace_source(
                Records::from((0..10).collect(), 0),
                ListGridSizing::fixed(px(180.)),
                cx
            ))
            .is_err()
    );
    assert_eq!(state.read_with(cx, |s, _| s.len()), 10);
}

#[gpui_kit::test]
fn source_edits_that_remove_the_reading_item_use_successor_then_predecessor(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx, 10_000, ListGridSizing::measured(px(140.)));
    view.update(cx, |v, cx| {
        v.virtual_only = true;
        cx.notify();
    });
    let state = state(&view, cx);
    state.update(cx, |s, cx| {
        s.scroll_to_key(&603_usize.into(), VirtualizerAlign::Start.into(), cx)
            .unwrap()
    });
    draw(cx);
    state.update(cx, |s, cx| {
        s.replace_source(
            Records::from((0..10_000).filter(|ix| *ix != 603).collect(), 1),
            ListGridSizing::measured(px(140.)),
            cx,
        )
        .unwrap()
    });
    draw(cx);
    let anchor = bounds(cx, "virtual-cards", 604);
    close(anchor.top(), px(0.));
    state.update(cx, |s, cx| {
        s.replace_source(
            Records::from((0..10_000).filter(|ix| *ix != 604).collect(), 2),
            ListGridSizing::measured(px(140.)),
            cx,
        )
        .unwrap()
    });
    draw(cx);
    close(bounds(cx, "virtual-cards", 605).top(), px(0.));
    // No successor survives: the last predecessor is mounted at the clamped end.
    state.update(cx, |s, cx| {
        s.replace_source(
            Records::from((0..603).collect(), 3),
            ListGridSizing::measured(px(140.)),
            cx,
        )
        .unwrap()
    });
    draw(cx);
    let last = bounds(cx, "virtual-cards", 602);
    assert!(last.bottom() <= px(320.6) && last.bottom() > px(300.));
}

#[gpui_kit::test]
fn empty_single_and_partial_collections_have_no_phantom_items(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, 0, ListGridSizing::fixed(px(180.)));
    let state = state(&view, cx);
    assert_eq!(
        nodes(cx)
            .iter()
            .filter(|n| n.role() == Some(Role::ListItem))
            .count(),
        0
    );
    for count in [1, 2, 3, 4] {
        view.update(cx, |v, cx| {
            v.count = count;
            v.state.update(cx, |s, cx| {
                s.replace_source(
                    Records::from((0..count).collect(), 0),
                    ListGridSizing::fixed(px(180.)),
                    cx,
                )
                .unwrap()
            });
            cx.notify();
        });
        draw(cx);
        let cards: Vec<_> = nodes(cx)
            .into_iter()
            .filter(|n| {
                n.path().contains(&ElementId::from("virtual-cards"))
                    && n.role() == Some(Role::ListItem)
            })
            .collect();
        assert_eq!(cards.len(), count);
        close(
            bounds(cx, "virtual-cards", count - 1).size.width,
            bounds(cx, "virtual-cards", 0).size.width,
        );
        assert_eq!(state.read_with(cx, |s, _| s.len()), count);
    }
}
