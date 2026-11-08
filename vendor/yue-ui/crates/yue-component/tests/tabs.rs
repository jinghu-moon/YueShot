//! Real native input, geometry and scene regressions for controlled tabs.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, ParentElement as _, Pixels, Render, Role, SharedString, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use std::{cell::Cell, collections::HashSet, rc::Rc, time::Duration};
use yue_component::{
    Accent, InputState, TabsActivation, TabsContentMotion, TabsHandle, TabsPlacement,
    TabsScrollAlign, TabsSize, TabsVariant, ThemeMode, YueButton, YueInput, YueTabItem, YueTabs,
    YueTheme, outline,
};

struct Probe {
    selected: Option<SharedString>,
    keys: Vec<SharedString>,
    disabled_keys: HashSet<SharedString>,
    disabled: bool,
    accept: bool,
    accept_close: bool,
    closable: bool,
    addable: bool,
    changes: Vec<SharedString>,
    closes: Vec<SharedString>,
    added: usize,
    placement: TabsPlacement,
    size: TabsSize,
    variant: TabsVariant,
    activation: TabsActivation,
    motion: TabsContentMotion,
    lazy: bool,
    icon: bool,
    width: f32,
    builds: Rc<Vec<Cell<usize>>>,
    actions: Rc<Cell<usize>>,
    input: Entity<InputState>,
    with_input: bool,
    with_menu: bool,
    panel_tab_stop: bool,
    before: FocusHandle,
    after: FocusHandle,
    handle: TabsHandle,
}
impl Probe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            selected: Some("a".into()),
            keys: ["a", "b", "c"].map(Into::into).to_vec(),
            disabled_keys: HashSet::new(),
            disabled: false,
            accept: true,
            accept_close: true,
            closable: false,
            addable: false,
            changes: vec![],
            closes: vec![],
            added: 0,
            placement: TabsPlacement::Top,
            size: TabsSize::Medium,
            variant: TabsVariant::Normal,
            activation: TabsActivation::Automatic,
            motion: TabsContentMotion::None,
            lazy: true,
            icon: false,
            width: 420.,
            builds: Rc::new((0..50).map(|_| Cell::new(0)).collect()),
            actions: Rc::new(Cell::new(0)),
            input: cx.new(|cx| InputState::new(window, cx)),
            with_input: false,
            with_menu: false,
            panel_tab_stop: false,
            before: cx.focus_handle(),
            after: cx.focus_handle(),
            handle: TabsHandle::new(),
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self
            .keys
            .iter()
            .enumerate()
            .map(|(ix, key)| {
                let builds = self.builds.clone();
                let action = self.actions.clone();
                let input = self.input.clone();
                let with_input = self.with_input && key.as_ref() == "a";
                // Identical local ids in different panels must remain isolated.
                let with_menu = self.with_menu;
                let key_id = key.clone();
                let mut item = YueTabItem::new(key.clone(), format!("标签 {key}"))
                    .disabled(self.disabled_keys.contains(key))
                    .closable(self.closable)
                    .panel_tab_stop(self.panel_tab_stop)
                    .content(move |_, _| {
                        builds[ix].set(builds[ix].get() + 1);
                        let action = action.clone();
                        let mut panel = div().p_3().child(
                            YueButton::new(
                                (gpui_kit::ElementId::from("panel-action"), key_id.clone()),
                                format!("面板动作 {key_id}"),
                            )
                            .on_click(move |_, _, _| action.set(action.get() + 1)),
                        );
                        if with_input {
                            panel = panel.child(YueInput::new(
                                "panel-input",
                                YueTheme::new(ThemeMode::Light, Accent::Azure),
                                &input,
                            ));
                        }
                        if with_menu {
                            panel = panel.child(
                                yue_component::YueDropdownMenu::new(
                                    "panel-menu",
                                    YueTheme::new(ThemeMode::Light, Accent::Azure),
                                    YueButton::new("panel-menu-trigger", "面板菜单"),
                                )
                                .label("面板操作")
                                .items([yue_component::YueMenuItem::new("面板菜单命令")]),
                            );
                        }
                        panel.into_any_element()
                    });
                if self.icon {
                    item = item.icon(outline::Folder);
                }
                item
            })
            .collect::<Vec<_>>();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .p_4()
            .gap_3()
            .child(YueButton::new("before", "前一个").track_focus(&self.before))
            .child(
                YueTabs::new("probe-tabs", self.selected.clone())
                    .accessibility_label("测试标签页")
                    .items(items)
                    .width(px(self.width))
                    .height(px(180.))
                    .handle(&self.handle)
                    .placement(self.placement)
                    .size(self.size)
                    .variant(self.variant)
                    .activation(self.activation)
                    .content_motion(self.motion)
                    .lazy(self.lazy)
                    .disabled(self.disabled)
                    .addable(self.addable)
                    .on_change(cx.listener(
                        |v, request: &yue_component::TabsChangeRequest, _, cx| {
                            v.changes.push(request.key().clone());
                            if v.accept {
                                v.selected = Some(request.key().clone());
                            }
                            cx.notify();
                        },
                    ))
                    .on_close(
                        cx.listener(|v, request: &yue_component::TabsCloseRequest, _, cx| {
                            v.closes.push(request.key().clone());
                            if v.accept_close {
                                v.keys.retain(|key| key != request.key());
                                if v.selected.as_ref() == Some(request.key()) {
                                    v.selected = request.next_key().cloned();
                                }
                            }
                            cx.notify();
                        }),
                    )
                    .on_add({
                        let view = cx.entity();
                        move |_, cx| {
                            view.update(cx, |v, cx| {
                                v.added += 1;
                                cx.notify();
                            })
                        }
                    }),
            )
            .child(YueButton::new("after", "后一个").track_focus(&self.after))
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
fn tick(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    draw(cx);
}
fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
    });
    let mut view = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let v = cx.new(|cx| Probe::new(w, cx));
        view = Some(v.clone());
        gpui_kit::base::Root::new(v, w, cx)
    });
    cx.update(|w, _| w.activate_window());
    draw(cx);
    draw(cx);
    (view.unwrap(), cx)
}
fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|w, _| {
        let all = snapshots(w)
            .into_iter()
            .filter(|n| n.label() == Some(label))
            .collect::<Vec<_>>();
        let all = if all.iter().any(|n| n.role() == Some(Role::Tab)) {
            all.into_iter()
                .filter(|n| n.role() == Some(Role::Tab))
                .collect::<Vec<_>>()
        } else {
            all
        };
        assert_eq!(all.len(), 1, "unique {label}");
        all[0].clone()
    })
}
fn bounds(cx: &mut VisualTestContext, part: &str) -> Bounds<Pixels> {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| {
                n.path().last() == Some(&(gpui_kit::ElementId::from("probe-tabs"), part).into())
            })
            .unwrap_or_else(|| panic!("missing {part}"))
            .bounds()
    })
}
fn click(cx: &mut VisualTestContext, label: &str) {
    let point = node(cx, label).bounds().center();
    cx.simulate_click(point, Modifiers::default());
    draw(cx);
}
fn key(cx: &mut VisualTestContext, value: &str) {
    let stroke = Keystroke::parse(value).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: stroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke: stroke });
    draw(cx);
}
fn focus(view: &Entity<Probe>, cx: &mut VisualTestContext, value: &str) {
    cx.update(|w, cx| view.read(cx).handle.clone().focus(value, w, cx));
    draw(cx);
}
fn selected(view: &Entity<Probe>, cx: &mut VisualTestContext) -> Option<SharedString> {
    view.read_with(cx, |v, _| v.selected.clone())
}
fn line(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.update(|w, _| {
        let color: gpui_kit::Hsla =
            gpui_kit::rgb(YueTheme::new(ThemeMode::Light, Accent::Azure).accent_solid()).into();
        w.painted_quads()
            .into_iter()
            .map(|q| {
                let scale = w.scale_factor();
                let b = Bounds::new(
                    gpui_kit::point(
                        px(q.bounds.origin.x.as_f32() / scale),
                        px(q.bounds.origin.y.as_f32() / scale),
                    ),
                    gpui_kit::size(
                        px(q.bounds.size.width.as_f32() / scale),
                        px(q.bounds.size.height.as_f32() / scale),
                    ),
                );
                (b, q.background)
            })
            .find(|(b, background)| {
                (b.size.height - w.rem_size() / 8.).abs() < px(1. / w.scale_factor() + 0.01)
                    && b.size.width > w.rem_size() / 8.
                    && background.as_solid() == Some(color)
            })
            .expect("selected underline")
            .0
    })
}

#[gpui_kit::test]
fn exposes_one_tab_list_and_only_the_active_panel(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    assert_eq!(node(cx, "测试标签页").role(), Some(Role::TabList));
    assert_eq!(node(cx, "标签 a").selected(), Some(true));
    assert_eq!(node(cx, "标签 b").selected(), Some(false));
    cx.update(|w, _| {
        let all = snapshots(w);
        assert_eq!(
            all.iter()
                .filter(|n| n.role() == Some(Role::TabPanel))
                .count(),
            1
        );
        assert!(!all.iter().any(|n| n.label() == Some("面板动作 b")));
    });
}

#[gpui_kit::test]
fn selection_is_controlled_and_repeat_activation_is_silent(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "标签 b");
    click(cx, "标签 b");
    assert_eq!(selected(&view, cx).as_deref(), Some("b"));
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 1);
    view.update(cx, |v, cx| {
        v.accept = false;
        cx.notify();
    });
    draw(cx);
    click(cx, "标签 c");
    assert_eq!(selected(&view, cx).as_deref(), Some("b"));
    assert_eq!(node(cx, "标签 b").selected(), Some(true));
    node(cx, "面板动作 b");
    view.update(cx, |v, cx| {
        v.selected = Some("a".into());
        cx.notify();
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 2);
}

#[gpui_kit::test]
fn automatic_navigation_skips_disabled_wraps_and_supports_endpoints(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.disabled_keys.insert("b".into());
        cx.notify();
    });
    draw(cx);
    focus(&view, cx, "a");
    key(cx, "right");
    assert_eq!(selected(&view, cx).as_deref(), Some("c"));
    key(cx, "right");
    assert_eq!(selected(&view, cx).as_deref(), Some("a"));
    key(cx, "end");
    assert_eq!(selected(&view, cx).as_deref(), Some("c"));
    key(cx, "home");
    assert_eq!(selected(&view, cx).as_deref(), Some("a"));
    key(cx, "up");
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 4);
}

#[gpui_kit::test]
fn manual_navigation_moves_focus_before_requesting_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.activation = TabsActivation::Manual;
        cx.notify();
    });
    draw(cx);
    focus(&view, cx, "a");
    key(cx, "right");
    assert_eq!(selected(&view, cx).as_deref(), Some("a"));
    assert_eq!(node(cx, "标签 b").focused(), Some(true));
    key(cx, "enter");
    assert_eq!(selected(&view, cx).as_deref(), Some("b"));
    key(cx, "space");
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 1);
}

#[gpui_kit::test]
fn vertical_layout_navigates_with_vertical_keys(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.placement = TabsPlacement::Right;
        cx.notify();
    });
    draw(cx);
    focus(&view, cx, "a");
    key(cx, "down");
    assert_eq!(selected(&view, cx).as_deref(), Some("b"));
    key(cx, "right");
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 1);
    assert!(bounds(cx, "header").left() >= bounds(cx, "content-viewport").right());
}

#[gpui_kit::test]
fn close_is_an_independent_request_and_delete_uses_the_same_contract(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.closable = true;
        v.addable = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "关闭 标签 b");
    assert_eq!(selected(&view, cx).as_deref(), Some("a"));
    assert!(view.read_with(cx, |v, _| v.changes.is_empty()));
    focus(&view, cx, "a");
    click(cx, "关闭 标签 a");
    assert_eq!(selected(&view, cx).as_deref(), Some("c"));
    assert_eq!(node(cx, "标签 c").focused(), Some(true));
    key(cx, "delete");
    assert_eq!(selected(&view, cx), None);
    assert_eq!(node(cx, "新增标签页").focused(), Some(true));
    click(cx, "新增标签页");
    assert_eq!(view.read_with(cx, |v, _| v.added), 1);
}

#[gpui_kit::test]
fn refused_close_does_not_remove_or_activate_a_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.closable = true;
        v.accept_close = false;
        cx.notify();
    });
    draw(cx);
    click(cx, "关闭 标签 b");
    assert_eq!(view.read_with(cx, |v, _| v.keys.len()), 3);
    assert!(view.read_with(cx, |v, _| v.changes.is_empty()));
    assert_eq!(selected(&view, cx).as_deref(), Some("a"));
}

#[gpui_kit::test]
fn disabled_tabs_and_group_block_requests_without_hiding_selected_content(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.closable = true;
        v.disabled_keys.insert("b".into());
        cx.notify();
    });
    draw(cx);
    click(cx, "标签 b");
    click(cx, "关闭 标签 b");
    assert!(view.read_with(cx, |v, _| v.changes.is_empty() && v.closes.is_empty()));
    view.update(cx, |v, cx| {
        v.disabled = true;
        v.addable = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "标签 c");
    click(cx, "新增标签页");
    assert!(view.read_with(cx, |v, _| v.changes.is_empty() && v.added == 0));
    click(cx, "面板动作 a");
    assert_eq!(view.read_with(cx, |v, _| v.actions.get()), 1);
}

#[gpui_kit::test]
fn lazy_building_and_inactive_input_gating_are_distinct(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    assert_eq!(view.read_with(cx, |v, _| v.builds[1].get()), 0);
    click(cx, "标签 b");
    assert!(view.read_with(cx, |v, _| v.builds[1].get()) > 0);
    click(cx, "面板动作 b");
    view.update(cx, |v, cx| {
        v.lazy = false;
        cx.notify();
    });
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.builds[2].get()) > 0);
    cx.update(|w, _| assert!(!snapshots(w).iter().any(|n| n.label() == Some("面板动作 c"))));
    assert_eq!(view.read_with(cx, |v, _| v.actions.get()), 1);
}

#[gpui_kit::test]
fn retained_input_survives_switch_and_old_panel_focus_is_reconciled(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.with_input = true;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| {
        view.read(cx).input.clone().update(cx, |input, cx| {
            input.set_value("保留内容", w, cx);
            input.focus(w, cx);
        })
    });
    draw(cx);
    view.update(cx, |v, cx| {
        v.selected = Some("b".into());
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标签 b").focused(), Some(true));
    click(cx, "标签 a");
    assert_eq!(
        view.read_with(cx, |v, cx| v.input.read(cx).value().to_string()),
        "保留内容"
    );
}

#[gpui_kit::test]
fn geometry_covers_every_placement_variant_and_size(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for placement in [
        TabsPlacement::Top,
        TabsPlacement::Bottom,
        TabsPlacement::Left,
        TabsPlacement::Right,
    ] {
        for variant in [TabsVariant::Normal, TabsVariant::Card] {
            for size in [TabsSize::Small, TabsSize::Medium, TabsSize::Large] {
                view.update(cx, |v, cx| {
                    v.placement = placement;
                    v.variant = variant;
                    v.size = size;
                    v.closable = true;
                    v.icon = true;
                    cx.notify();
                });
                draw(cx);
                draw(cx);
                let header = bounds(cx, "header");
                let content = bounds(cx, "content-viewport");
                let tab = node(cx, "标签 a").bounds();
                let close = node(cx, "关闭 标签 a").bounds();
                assert!(tab.right() <= close.left() + px(0.01));
                assert!(tab.size.height >= px(26.));
                match placement {
                    TabsPlacement::Top => assert!(header.bottom() <= content.top() + px(0.01)),
                    TabsPlacement::Bottom => assert!(header.top() >= content.bottom() - px(0.01)),
                    TabsPlacement::Left => assert!(header.right() <= content.left() + px(0.01)),
                    TabsPlacement::Right => assert!(header.left() >= content.right() - px(0.01)),
                }
            }
        }
    }
}

#[gpui_kit::test]
fn indicator_has_intermediate_geometry_and_reduced_motion_snaps(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    let a = line(cx);
    click(cx, "标签 b");
    assert_eq!(line(cx), a);
    tick(cx, 60);
    let middle = line(cx);
    tick(cx, 180);
    let b = line(cx);
    assert!(
        middle.origin.x > a.origin.x && middle.origin.x < b.origin.x,
        "{a:?} {middle:?} {b:?}"
    );
    click(cx, "标签 a");
    tick(cx, 40);
    let reversing = line(cx);
    click(cx, "标签 b");
    assert!((line(cx).origin.x - reversing.origin.x).abs() < px(0.01));
    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    assert_eq!(line(cx), b);
}

#[gpui_kit::test]
fn overflow_commands_and_key_based_reveal_keep_the_target_visible(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.keys = (0..25).map(|ix| format!("long-{ix}").into()).collect();
        v.selected = Some("long-0".into());
        v.width = 240.;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    node(cx, "向后滚动标签");
    click(cx, "向后滚动标签");
    tick(cx, 250);
    cx.update(|w, cx| {
        view.read(cx)
            .handle
            .clone()
            .scroll_to("long-24", TabsScrollAlign::End, w, cx)
    });
    draw(cx);
    tick(cx, 250);
    draw(cx);
    let target = node(cx, "标签 long-24").bounds();
    let viewport = bounds(cx, "viewport");
    assert!(
        target.right() <= viewport.right() + px(1.),
        "{target:?}, {viewport:?}"
    );
    assert!(target.right() > viewport.left());
}

#[gpui_kit::test]
fn none_invalid_key_and_empty_collection_do_not_silently_select_another_panel(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    for value in [None, Some("missing".into())] {
        view.update(cx, |v, cx| {
            v.selected = value;
            cx.notify();
        });
        draw(cx);
        cx.update(|w, _| {
            assert!(
                !snapshots(w)
                    .iter()
                    .any(|n| n.role() == Some(Role::TabPanel))
            )
        });
    }
    view.update(cx, |v, cx| {
        v.keys.clear();
        v.addable = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "新增标签页");
    assert_eq!(view.read_with(cx, |v, _| v.added), 1);
}

#[gpui_kit::test]
fn logical_traversal_is_header_then_panel_at_every_placement(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for placement in [
        TabsPlacement::Top,
        TabsPlacement::Bottom,
        TabsPlacement::Left,
        TabsPlacement::Right,
    ] {
        view.update(cx, |v, cx| {
            v.placement = placement;
            v.closable = true;
            cx.notify();
        });
        draw(cx);
        cx.update(|w, cx| view.read(cx).before.clone().focus(w, cx));
        key(cx, "tab");
        assert_eq!(node(cx, "标签 a").focused(), Some(true), "{placement:?}");
        key(cx, "tab");
        assert_eq!(
            node(cx, "面板动作 a").focused(),
            Some(true),
            "{placement:?}"
        );
        key(cx, "tab");
        assert_eq!(node(cx, "后一个").focused(), Some(true), "{placement:?}");
        key(cx, "shift-tab");
        assert_eq!(node(cx, "面板动作 a").focused(), Some(true));
        key(cx, "shift-tab");
        assert_eq!(node(cx, "标签 a").focused(), Some(true));
    }
}

#[gpui_kit::test]
fn stable_keys_preserve_focus_when_reordered_and_skip_newly_disabled_items(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    focus(&view, cx, "b");
    view.update(cx, |v, cx| {
        v.keys.reverse();
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标签 b").focused(), Some(true));
    assert_eq!(selected(&view, cx).as_deref(), Some("a"));
    view.update(cx, |v, cx| {
        v.disabled_keys.insert("b".into());
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标签 a").focused(), Some(true));
    view.update(cx, |v, cx| {
        v.disabled_keys.extend(v.keys.clone());
        cx.notify();
    });
    draw(cx);
    cx.update(|w, _| {
        assert!(
            snapshots(w)
                .into_iter()
                .filter(|n| n.role() == Some(Role::Tab))
                .all(|n| n.focused() != Some(true))
        )
    });
    node(cx, "面板动作 a");
    view.update(cx, |v, cx| {
        v.disabled_keys.remove(&SharedString::from("c"));
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| view.read(cx).before.clone().focus(w, cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "标签 c").focused(), Some(true));
    key(cx, "right");
    key(cx, "right");
    assert_eq!(selected(&view, cx).as_deref(), Some("c"));
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 1);
}

#[gpui_kit::test]
fn content_motion_keeps_only_one_live_panel_during_rapid_switches(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for motion in [TabsContentMotion::Fade, TabsContentMotion::Slide] {
        view.update(cx, |v, cx| {
            v.motion = motion;
            v.selected = Some("a".into());
            cx.notify();
        });
        draw(cx);
        tick(cx, 180);
        for target in ["b", "c", "a", "b"] {
            click(cx, &format!("标签 {target}"));
            tick(cx, 35);
            cx.update(|w, _| {
                let all = snapshots(w);
                assert_eq!(
                    all.iter()
                        .filter(|n| n.role() == Some(Role::TabPanel))
                        .count(),
                    1
                );
                assert_eq!(
                    all.iter()
                        .filter(|n| n.label().is_some_and(|s| s.starts_with("面板动作")))
                        .count(),
                    1
                );
            });
            click(cx, &format!("面板动作 {target}"));
            key(cx, "delete");
            assert!(view.read_with(cx, |v, _| v.closes.is_empty()));
        }
        tick(cx, 200);
    }
    assert_eq!(view.read_with(cx, |v, _| v.actions.get()), 8);
}

#[gpui_kit::test]
fn slide_uses_navigation_axis_and_reduced_motion_removes_offset(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for placement in [
        TabsPlacement::Top,
        TabsPlacement::Bottom,
        TabsPlacement::Left,
        TabsPlacement::Right,
    ] {
        cx.update(|_, cx| cx.set_reduce_motion(false));
        view.update(cx, |v, cx| {
            v.motion = TabsContentMotion::Slide;
            v.placement = placement;
            v.selected = Some("a".into());
            cx.notify();
        });
        draw(cx);
        tick(cx, 180);
        let rest = node(cx, "面板动作 a").bounds().origin;
        click(cx, "标签 b");
        let entered = node(cx, "面板动作 b").bounds().origin;
        if matches!(placement, TabsPlacement::Left | TabsPlacement::Right) {
            assert!(entered.y > rest.y);
            assert_eq!(entered.x, rest.x);
        } else {
            assert!(entered.x > rest.x);
            assert_eq!(entered.y, rest.y);
        }
        tick(cx, 60);
        let middle = node(cx, "面板动作 b").bounds().origin;
        assert_ne!(middle, entered);
        cx.update(|_, cx| cx.set_reduce_motion(true));
        draw(cx);
        assert_eq!(node(cx, "面板动作 b").bounds().origin, rest);
    }
}

#[gpui_kit::test]
fn rem_and_dpi_changes_remeasure_tabs_and_keep_close_commands_separate(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.closable = true;
        v.icon = true;
        v.width = 640.;
        cx.notify();
    });
    draw(cx);
    for scale in [1., 1.5, 2.] {
        cx.simulate_scale_factor_change(scale);
        for rem in [16., 20., 24.] {
            cx.update(|w, _| w.set_rem_size(px(rem)));
            draw(cx);
            draw(cx);
            let tab = node(cx, "标签 a").bounds();
            let close = node(cx, "关闭 标签 a").bounds();
            assert!(tab.right() <= close.left() + px(0.01));
            assert!((tab.size.height - px(2. * rem)).abs() < px(0.1));
            click(cx, "标签 b");
            tick(cx, 200);
            click(cx, "标签 a");
            tick(cx, 200);
            assert!(line(cx).right() <= bounds(cx, "viewport").right() + px(1.));
        }
    }
}

#[gpui_kit::test]
fn user_scroll_is_not_pulled_back_to_selected_on_redraw(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.keys = (0..25).map(|ix| format!("long-{ix}").into()).collect();
        v.selected = Some("long-0".into());
        v.width = 240.;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    tick(cx, 200);
    let origin = node(cx, "标签 long-0").bounds().left();
    let position = bounds(cx, "viewport").center();
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position,
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-180.), px(0.))),
        touch_phase: gpui_kit::TouchPhase::Moved,
        ..Default::default()
    });
    draw(cx);
    tick(cx, 200);
    let scrolled = node(cx, "标签 long-0").bounds().left();
    assert!(scrolled < origin);
    view.update(cx, |_, cx| cx.notify());
    draw(cx);
    tick(cx, 200);
    assert_eq!(node(cx, "标签 long-0").bounds().left(), scrolled);
    assert_eq!(selected(&view, cx).as_deref(), Some("long-0"));
}

#[gpui_kit::test]
fn switching_panels_removes_its_open_dropdown_and_does_not_reopen_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.with_menu = true;
        v.motion = TabsContentMotion::Fade;
        cx.notify();
    });
    draw(cx);
    click(cx, "面板菜单");
    tick(cx, 250);
    node(cx, "面板菜单命令");
    // External selection changes while the menu owns focus, not a click-away dismissal.
    view.update(cx, |v, cx| {
        v.selected = Some("b".into());
        cx.notify();
    });
    draw(cx);
    tick(cx, 250);
    cx.update(|w, _| {
        assert!(
            !snapshots(w)
                .iter()
                .any(|n| n.role() == Some(Role::Menu) || n.label() == Some("面板菜单命令"))
        )
    });
    click(cx, "标签 a");
    tick(cx, 250);
    node(cx, "面板菜单");
    cx.update(|w, _| assert!(!snapshots(w).iter().any(|n| n.role() == Some(Role::Menu))));
    click(cx, "面板菜单");
    tick(cx, 250);
    node(cx, "面板菜单命令");
}

#[gpui_kit::test]
fn viewport_and_font_changes_reveal_the_same_key_after_remeasurement(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.keys = (0..25).map(|ix| format!("long-{ix}").into()).collect();
        v.selected = Some("long-12".into());
        v.width = 600.;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    tick(cx, 250);
    for (width, rem) in [(240., 16.), (220., 20.), (360., 24.)] {
        view.update(cx, |v, cx| {
            v.width = width;
            cx.notify();
        });
        cx.update(|w, _| w.set_rem_size(px(rem)));
        draw(cx);
        draw(cx);
        tick(cx, 250);
        draw(cx);
        let target = node(cx, "标签 long-12").bounds();
        let viewport = bounds(cx, "viewport");
        assert!(
            target.right() <= viewport.right() + px(1.),
            "{target:?} {viewport:?}"
        );
        assert!(
            target.left() >= viewport.left() - px(1.),
            "{target:?} {viewport:?}"
        );
        assert_eq!(selected(&view, cx).as_deref(), Some("long-12"));
    }
}

#[gpui_kit::test]
fn explicit_panel_stop_is_a_keyboard_entry_before_descendants(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.panel_tab_stop = true;
        cx.notify();
    });
    draw(cx);
    focus(&view, cx, "a");
    key(cx, "tab");
    cx.update(|w, _| {
        assert_eq!(
            snapshots(w)
                .iter()
                .find(|n| n.role() == Some(Role::TabPanel))
                .unwrap()
                .focused(),
            Some(true)
        )
    });
    key(cx, "tab");
    assert_eq!(node(cx, "面板动作 a").focused(), Some(true));
    key(cx, "shift-tab");
    cx.update(|w, _| {
        assert_eq!(
            snapshots(w)
                .iter()
                .find(|n| n.role() == Some(Role::TabPanel))
                .unwrap()
                .focused(),
            Some(true)
        )
    });
    key(cx, "shift-tab");
    assert_eq!(node(cx, "标签 a").focused(), Some(true));
}
