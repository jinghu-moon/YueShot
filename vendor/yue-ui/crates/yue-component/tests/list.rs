//! Public composition, measured geometry, identity and real control interaction.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Bounds, Context, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Render, Role, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use yue_component::{
    Accent, CheckboxState, ListItemLayout, ListLayout, ListSize, ThemeMode, YueButton, YueCheckbox,
    YueList, YueListGroup, YueListItem, YueListItemMeta, YueTheme,
};

struct Probe {
    width: f32,
    size: ListSize,
    item_layout: ListItemLayout,
    horizontal: bool,
    grouped: bool,
    empty: bool,
    split: bool,
    disabled: bool,
    selected: bool,
    checked: bool,
    actions: usize,
    keys: Vec<&'static str>,
    before: FocusHandle,
    after: FocusHandle,
    action_focus: FocusHandle,
    theme: YueTheme,
}
impl Probe {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            width: 420.,
            size: ListSize::Medium,
            item_layout: ListItemLayout::Horizontal,
            horizontal: false,
            grouped: false,
            empty: false,
            split: true,
            disabled: false,
            selected: false,
            checked: false,
            actions: 0,
            keys: vec!["alex", "bo"],
            before: cx.focus_handle().tab_stop(true),
            after: cx.focus_handle().tab_stop(true),
            action_focus: cx.focus_handle().tab_stop(true),
            theme: YueTheme::new(ThemeMode::Light, Accent::Azure),
        }
    }
    fn item(&self, key: &'static str, cx: &mut Context<Self>) -> YueListItem {
        let mut item = YueListItem::new(key)
            .selected(self.selected)
            .disabled(self.disabled)
            .content(
                YueListItemMeta::new(format!("成员 {key} · 可自然收缩的标题"))
                    .theme(self.theme)
                    .description("主副标题与常用操作组合；正文由宿主提供。"),
            )
            .extra(div().w(px(32.)).h(px(24.)).child("状态"));
        if key == "alex" {
            let owner = cx.entity();
            item = item
                .leading(
                    YueCheckbox::new(
                        "choose-alex",
                        self.theme,
                        if self.checked {
                            CheckboxState::Checked
                        } else {
                            CheckboxState::Unchecked
                        },
                    )
                    .label("选择 Alex")
                    .disabled(self.disabled)
                    .on_change(move |state, _, _, cx| {
                        owner.update(cx, |v, cx| {
                            v.checked = state == CheckboxState::Checked;
                            cx.notify();
                        })
                    }),
                )
                .body("补充正文，布局不会把右侧状态或操作压到文本之上。")
                .actions(
                    YueButton::new("open-alex", "查看")
                        .track_focus(&self.action_focus)
                        .disabled(self.disabled)
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.actions += 1;
                            cx.notify();
                        })),
                );
        }
        if key == "bo" {
            item = item.size(ListSize::Small);
        }
        item
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = YueList::new("members")
            .theme(self.theme)
            .width(px(self.width))
            .size(self.size)
            .item_layout(self.item_layout)
            .layout(if self.horizontal {
                ListLayout::Horizontal
            } else {
                ListLayout::Vertical
            })
            .split(self.split)
            .striped(true)
            .hoverable(true)
            .bordered(true)
            .accessibility_label("成员列表")
            .header("成员 Header")
            .footer("共计 Footer")
            .empty("暂无成员");
        if !self.empty {
            if self.grouped {
                list = list
                    .group(YueListGroup::new("empty-group", "不应出现"))
                    .child(self.item("alex", cx))
                    .group(
                        YueListGroup::new("team", "团队")
                            .children([self.item("bo", cx), self.item("chen", cx)]),
                    );
            } else {
                list = list.children(self.keys.iter().map(|key| self.item(key, cx)));
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .p_4()
            .child(YueButton::new("before", "之前").track_focus(&self.before))
            .child(list)
            .child(YueButton::new("after", "之后").track_focus(&self.after))
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
fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap();
        cx.set_reduce_motion(true);
    });
    let mut probe = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let view = cx.new(Probe::new);
        probe = Some(view.clone());
        gpui_kit::base::Root::new(view, w, cx)
    });
    draw(cx);
    (probe.unwrap(), cx)
}
fn nodes(cx: &mut VisualTestContext) -> Vec<ElementSnapshot> {
    cx.update(|w, _| snapshots(w))
}
fn node(cx: &mut VisualTestContext, id: &str) -> ElementSnapshot {
    nodes(cx)
        .into_iter()
        .find(|n| n.path().last() == Some(&ElementId::from(id.to_string())))
        .unwrap_or_else(|| panic!("missing list part {id}"))
}
fn part(cx: &mut VisualTestContext, item: &str, slot: &str) -> Bounds<Pixels> {
    let item = ElementId::from(item.to_string());
    let slot = ElementId::from(slot.to_string());
    nodes(cx)
        .into_iter()
        .find(|n| n.path().contains(&item) && n.path().last() == Some(&slot))
        .unwrap_or_else(|| panic!("missing {item:?}/{slot:?}"))
        .bounds()
}
fn key(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    draw(cx);
}
fn click(cx: &mut VisualTestContext, id: &str) {
    let center = node(cx, id).bounds().center();
    cx.simulate_click(center, Modifiers::default());
    draw(cx);
}
fn close(a: Pixels, b: Pixels) {
    assert!((a - b).abs() < px(0.1), "{a:?} != {b:?}");
}

#[gpui_kit::test]
fn linear_list_has_one_collection_real_rows_and_aligned_bands(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    assert_eq!(node(cx, "list-body").role(), Some(Role::List));
    assert_eq!(node(cx, "list-body").label(), Some("成员列表"));
    let rows: Vec<_> = nodes(cx)
        .into_iter()
        .filter(|n| n.role() == Some(Role::ListItem))
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|n| n.focused().is_none() && n.selected().is_none())
    );
    let alex = node(cx, "alex").bounds();
    let bo = node(cx, "bo").bounds();
    close(alex.origin.x, bo.origin.x);
    close(alex.size.width, bo.size.width);
    assert!(alex.bottom() < bo.top()); // Exactly one separating strip.
    assert!(node(cx, "list-header").bounds().bottom() < alex.top());
    assert!(node(cx, "list-footer").bounds().top() > bo.bottom());
    assert!(part(cx, "alex", "list-content").right() <= part(cx, "alex", "list-extra").left());
    assert!(part(cx, "alex", "list-extra").right() <= part(cx, "alex", "list-actions").left());
}

#[gpui_kit::test]
fn vertical_item_keeps_extra_right_and_actions_below_body(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.item_layout = ListItemLayout::Vertical;
        v.width = 340.;
        cx.notify();
    });
    draw(cx);
    let body = part(cx, "alex", "list-item-body");
    let actions = part(cx, "alex", "list-actions");
    let main = part(cx, "alex", "list-main");
    let extra = part(cx, "alex", "list-extra");
    assert!(body.bottom() <= actions.top());
    assert!(main.right() <= extra.left());
    assert!(extra.right() <= node(cx, "alex").bounds().right());
    close(actions.left(), body.left());
}

#[gpui_kit::test]
fn size_inheritance_explicit_override_and_wrapping_use_natural_bounds(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let small_override = node(cx, "bo").bounds().size.height;
    let medium = node(cx, "alex").bounds().size.height;
    view.update(cx, |v, cx| {
        v.size = ListSize::Large;
        cx.notify();
    });
    draw(cx);
    assert!(node(cx, "alex").bounds().size.height > medium);
    close(node(cx, "bo").bounds().size.height, small_override);
    let wide_body = part(cx, "alex", "list-item-body").size.height;
    view.update(cx, |v, cx| {
        v.width = 310.;
        cx.notify();
    });
    draw(cx);
    assert!(part(cx, "alex", "list-item-body").size.height > wide_body);
    assert!(part(cx, "alex", "list-content").size.width > px(0.));
    assert!(part(cx, "alex", "list-extra").right() <= part(cx, "alex", "list-actions").left());
}

#[gpui_kit::test]
fn empty_replaces_only_body_and_empty_groups_do_not_create_phantom_items(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.empty = true;
        cx.notify();
    });
    draw(cx);
    assert!(nodes(cx).iter().all(|n| n.role() != Some(Role::ListItem)));
    assert!(node(cx, "list-header").bounds().bottom() < node(cx, "list-empty").bounds().top());
    assert!(node(cx, "list-empty").bounds().bottom() < node(cx, "list-footer").bounds().top());
    view.update(cx, |v, cx| {
        v.empty = false;
        v.grouped = true;
        cx.notify();
    });
    draw(cx);
    assert!(
        nodes(cx)
            .iter()
            .all(|n| !n.path().contains(&ElementId::from("empty-group")))
    );
    assert_eq!(node(cx, "list-group-body").label(), Some("团队"));
    assert!(node(cx, "alex").bounds().bottom() < node(cx, "list-group-header").bounds().top());
    assert!(node(cx, "list-group-header").bounds().bottom() <= node(cx, "bo").bounds().top());
}

#[gpui_kit::test]
fn horizontal_items_do_not_wrap_and_split_can_be_removed(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.horizontal = true;
        v.keys = vec!["bo", "chen"];
        v.split = false;
        cx.notify();
    });
    draw(cx);
    let bo = node(cx, "bo").bounds();
    let chen = node(cx, "chen").bounds();
    close(bo.top(), chen.top());
    close(bo.right(), chen.left());
    assert!(chen.left() > bo.left());
}

#[gpui_kit::test]
fn child_controls_keep_pointer_keyboard_focus_and_explicit_disabled_contract(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    click(cx, "open-alex");
    assert_eq!(view.read_with(cx, |v, _| v.actions), 1);
    assert_eq!(node(cx, "open-alex").focused(), Some(true));
    key(cx, "enter");
    assert_eq!(view.read_with(cx, |v, _| v.actions), 2);
    view.update(cx, |v, cx| {
        v.keys.reverse();
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "open-alex").focused(), Some(true));
    key(cx, "space");
    assert_eq!(view.read_with(cx, |v, _| v.actions), 3);
    click(cx, "choose-alex");
    assert!(view.read_with(cx, |v, _| v.checked));
    assert_eq!(node(cx, "choose-alex").checked(), Some(true));
    // Disable an actually focused action, then dispatch both activation keys.
    cx.update(|w, cx| w.focus(&view.read(cx).action_focus.clone(), cx));
    view.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    key(cx, "enter");
    key(cx, "space");
    click(cx, "open-alex");
    assert_eq!(view.read_with(cx, |v, _| v.actions), 3);
    assert_eq!(node(cx, "open-alex").disabled(), Some(true));
    assert_eq!(node(cx, "choose-alex").disabled(), Some(true));
}

#[gpui_kit::test]
fn tab_order_skips_shells_and_visits_real_slot_controls(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|w, cx| w.focus(&view.read(cx).before.clone(), cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "choose-alex").focused(), Some(true));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "open-alex").focused(), Some(true));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "after").focused(), Some(true));
}

#[gpui_kit::test]
fn stripe_follows_complete_item_order_across_groups_and_reordering(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let stripe = view.read_with(cx, |v, _| {
        v.theme.token("--surface-subtle").unwrap().rgba().unwrap()
    });
    let painted_at = |cx: &mut VisualTestContext, id: &str| {
        let bounds = node(cx, id).bounds();
        cx.update(|w, _| {
            w.painted_quads().iter().any(|q| {
                q.bounds == bounds.scale(w.scale_factor())
                    && q.background.as_solid() == Some(stripe.into())
            })
        })
    };
    assert!(painted_at(cx, "bo"));
    assert!(!painted_at(cx, "alex"));
    view.update(cx, |v, cx| {
        v.grouped = true;
        cx.notify();
    });
    draw(cx);
    assert!(painted_at(cx, "bo"));
    assert!(!painted_at(cx, "chen"));
    view.update(cx, |v, cx| {
        v.grouped = false;
        v.keys.reverse();
        cx.notify();
    });
    draw(cx);
    assert!(!painted_at(cx, "bo"));
    assert!(painted_at(cx, "alex"));
}

#[gpui_kit::test]
fn themes_and_rem_keep_selected_surface_and_slot_geometry(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for rem in [12., 16., 20.] {
                let theme = YueTheme::new(mode, accent);
                view.update(cx, |v, cx| {
                    v.theme = theme;
                    v.size = ListSize::Medium;
                    v.selected = true;
                    v.width = 420. * rem / 16.;
                    cx.notify();
                });
                cx.update(|w, _| w.set_rem_size(px(rem)));
                draw(cx);
                let row = node(cx, "alex").bounds();
                let leading = part(cx, "alex", "list-leading");
                close(leading.left() - row.left(), px(12. * rem / 16.));
                assert!(
                    part(cx, "alex", "list-main").right() <= part(cx, "alex", "list-extra").left()
                );
                assert!(
                    part(cx, "alex", "list-extra").right()
                        <= part(cx, "alex", "list-actions").left()
                );
                let color = theme
                    .token("--list-row-background-selected")
                    .unwrap()
                    .rgba()
                    .unwrap();
                cx.update(|w, _| {
                    assert!(
                        w.painted_quads()
                            .iter()
                            .any(|q| q.bounds == row.scale(w.scale_factor())
                                && q.background.as_solid() == Some(color.into())),
                        "selected surface {mode:?}/{accent:?}/rem={rem}"
                    )
                });
            }
        }
    }
}

mod virtual_composition {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    use yue_component::{
        VirtualizerScrollOptions, VirtualizerSizing, VirtualizerSource, VirtualizerState,
        YueVirtualList,
    };
    struct Records;
    impl VirtualizerSource for Records {
        fn len(&self) -> usize {
            10_000
        }
        fn key_at(&self, index: usize) -> ElementId {
            index.into()
        }
        fn index_of(&self, key: &ElementId) -> Option<usize> {
            match key {
                ElementId::Integer(ix) if *ix < 10_000 => Some(*ix as usize),
                _ => None,
            }
        }
    }
    struct VirtualProbe {
        state: Entity<VirtualizerState>,
        builds: Rc<Cell<usize>>,
    }
    fn item(id: impl Into<ElementId>, ordinal: usize) -> YueListItem {
        YueListItem::new(id)
            .striped(true)
            .logical_index(ordinal)
            .content(
                YueListItemMeta::new("一套条目内容")
                    .description("复用现有动态测量与滚动，不另建引擎"),
            )
    }
    impl Render for VirtualProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let builds = self.builds.clone();
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    YueList::new("ordinary")
                        .width(px(420.))
                        .child(item("ordinary-row", 0)),
                )
                .child(
                    div().w(px(420.)).h(px(180.)).child(
                        YueVirtualList::new("virtual", &self.state, move |entry, _, _| {
                            builds.set(builds.get() + 1);
                            item(entry.key().clone(), entry.index()).semantics(false)
                        })
                        .scrollbar(false),
                    ),
                )
        }
    }
    #[gpui_kit::test]
    fn measured_virtual_rows_share_geometry_and_do_not_duplicate_item_roles(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
        });
        let mut view = None;
        let (_, cx) = cx.add_window_view(|w, cx| {
            let probe = cx.new(|cx| VirtualProbe {
                state: cx.new(|_| {
                    VirtualizerState::new(
                        gpui_kit::Axis::Vertical,
                        Rc::new(Records),
                        VirtualizerSizing::measured(px(64.)),
                    )
                    .unwrap()
                }),
                builds: Rc::new(Cell::new(0)),
            });
            view = Some(probe.clone());
            gpui_kit::base::Root::new(probe, w, cx)
        });
        draw(cx);
        let view = view.unwrap();
        let ordinary = node(cx, "ordinary-row").bounds();
        let virtual_rows: Vec<_> = nodes(cx)
            .into_iter()
            .filter(|n| {
                n.path().contains(&ElementId::from("virtual")) && n.role() == Some(Role::ListItem)
            })
            .collect();
        assert!(!virtual_rows.is_empty() && virtual_rows.len() < 30);
        for row in &virtual_rows {
            close(row.bounds().size.width, ordinary.size.width);
            close(row.bounds().size.height, ordinary.size.height);
        }
        let state = view.read_with(cx, |v, _| v.state.clone());
        state.update(cx, |state, cx| {
            state
                .scroll_to_key(9_900_usize.into(), VirtualizerScrollOptions::default())
                .unwrap();
            cx.notify();
        });
        draw(cx);
        let near_end: Vec<_> = nodes(cx)
            .into_iter()
            .filter(|n| {
                n.path().contains(&ElementId::from("virtual")) && n.role() == Some(Role::ListItem)
            })
            .collect();
        assert!(!near_end.is_empty() && near_end.len() < 30);
        assert!(
            nodes(cx)
                .iter()
                .any(|n| n.path().contains(&ElementId::from("virtual"))
                    && n.path().last() == Some(&ElementId::from(9_900_usize))),
            "requested domain key is mounted"
        );
        assert!(
            view.read_with(cx, |v, _| v.builds.get()) < 200,
            "jumping must not render ten thousand ordinary elements"
        );
    }
}
