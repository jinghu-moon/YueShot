//! Controlled values, production geometry, native input and disclosure lifetimes.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Bounds, Context, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Render, Role, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, prelude::FluentBuilder as _, px,
};
use std::{cell::Cell, collections::HashSet, rc::Rc, time::Duration};
use yue_component::{
    Accent, CheckboxState, CollapseChangeRequest, CollapseChangeSource, CollapseExpandTrigger,
    CollapseIndicatorPosition, CollapseMode, CollapseMotion, CollapseSize, CollapseVariant,
    InputState, ThemeMode, YueButton, YueCheckbox, YueCollapse, YueCollapseItem, YueDropdownMenu,
    YueInput, YueMenuItem, YueTheme, outline,
};

struct Probe {
    keys: Vec<SharedString>,
    expanded: Vec<SharedString>,
    disabled_keys: HashSet<SharedString>,
    disabled: bool,
    accept: bool,
    requests: Vec<CollapseChangeRequest>,
    mode: CollapseMode,
    size: CollapseSize,
    variant: CollapseVariant,
    trigger: CollapseExpandTrigger,
    position: CollapseIndicatorPosition,
    motion: CollapseMotion,
    indicator: bool,
    custom: bool,
    padding: bool,
    region: bool,
    height: f32,
    width: f32,
    theme: YueTheme,
    actions: Rc<Cell<usize>>,
    builds: Rc<[Cell<usize>; 4]>,
    input: Entity<InputState>,
    with_input: bool,
    with_menu: bool,
    nested: bool,
    nested_value: Vec<SharedString>,
    before: FocusHandle,
    after: FocusHandle,
    clipped: bool,
    long_title: bool,
    paint_probe: bool,
}
impl Probe {
    fn new(w: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            keys: ["a", "b", "c"].map(Into::into).to_vec(),
            expanded: vec!["a".into()],
            disabled_keys: HashSet::new(),
            disabled: false,
            accept: true,
            requests: Vec::new(),
            mode: CollapseMode::Multiple,
            size: CollapseSize::Medium,
            variant: CollapseVariant::Bordered,
            trigger: CollapseExpandTrigger::Header,
            position: CollapseIndicatorPosition::Leading,
            motion: CollapseMotion::Height,
            indicator: true,
            custom: false,
            padding: false,
            region: false,
            height: 80.,
            width: 400.,
            theme: YueTheme::new(ThemeMode::Light, Accent::Azure),
            actions: Rc::new(Cell::new(0)),
            builds: Rc::new(std::array::from_fn(|_| Cell::new(0))),
            input: cx.new(|cx| InputState::new(w, cx)),
            with_input: false,
            with_menu: false,
            nested: false,
            nested_value: vec![],
            before: cx.focus_handle(),
            after: cx.focus_handle(),
            clipped: false,
            long_title: false,
            paint_probe: false,
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity();
        let items = self
            .keys
            .iter()
            .enumerate()
            .map(|(ix, key)| {
                let owner = owner.clone();
                let actions = self.actions.clone();
                let body_actions = actions.clone();
                let builds = self.builds.clone();
                let key_id = key.clone();
                let theme = self.theme;
                let disabled = self.disabled_keys.contains(key);
                let height = self.height;
                let paint_probe = self.paint_probe;
                let input = self.input.clone();
                let with_input = self.with_input && key.as_ref() == "a";
                let with_menu = self.with_menu && key.as_ref() == "a";
                let nested = self.nested && key.as_ref() == "a";
                let nested_value = self.nested_value.clone();
                let nested_owner = owner.clone();
                let title = if self.long_title && key.as_ref() == "a" {
                    "这是一个需要换行的折叠面板标题，它使用清晰一致的行高".to_string()
                } else {
                    format!("标题 {key}")
                };
                let mut item = YueCollapseItem::new(key.clone(), title)
                    .disabled(disabled)
                    .content_padding(self.padding)
                    .region(self.region)
                    .actions(move |_, _| {
                        let toggle = owner.clone();
                        let toggle_key = key_id.clone();
                        let actions = actions.clone();
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                YueButton::new(
                                    (ElementId::from("action"), key_id.clone()),
                                    format!("操作 {key_id}"),
                                )
                                .on_click(move |_, _, _| actions.set(actions.get() + 1)),
                            )
                            .child(
                                YueCheckbox::new(
                                    (ElementId::from("disable"), key_id.clone()),
                                    theme,
                                    if disabled {
                                        CheckboxState::Checked
                                    } else {
                                        CheckboxState::Unchecked
                                    },
                                )
                                .label(format!("禁用 {key_id}"))
                                .on_change(
                                    move |state, _, _, cx| {
                                        toggle.update(cx, |v, cx| {
                                            if state == CheckboxState::Checked {
                                                v.disabled_keys.insert(toggle_key.clone());
                                            } else {
                                                v.disabled_keys.remove(&toggle_key);
                                            }
                                            cx.notify();
                                        })
                                    },
                                ),
                            )
                            .into_any_element()
                    })
                    .content(move |_, _| {
                        builds[ix].set(builds[ix].get() + 1);
                        let actions = body_actions.clone();
                        let mut body = div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(height))
                                    .when(paint_probe, |d| d.bg(gpui_kit::rgb(0x2468ab)))
                                    .child("自然高度内容"),
                            )
                            .child(
                                YueButton::new(("body-action", ix), format!("正文动作 {ix}"))
                                    .on_click(move |_, _, _| actions.set(actions.get() + 1)),
                            );
                        if with_input {
                            body = body.child(YueInput::new("retained-input", theme, &input));
                        }
                        if with_menu {
                            body = body.child(
                                YueDropdownMenu::new(
                                    "body-menu",
                                    theme,
                                    YueButton::new("body-menu-trigger", "正文菜单"),
                                )
                                .items([YueMenuItem::new("正文命令")]),
                            );
                        }
                        if nested {
                            let owner = nested_owner.clone();
                            body = body.child(
                                YueCollapse::new("nested", nested_value.clone())
                                    .items([
                                        YueCollapseItem::new("x", "内层 x").content(|_, _| {
                                            div().child("内层内容").into_any_element()
                                        }),
                                        YueCollapseItem::new("y", "内层 y").content(|_, _| {
                                            div().child("内层内容 y").into_any_element()
                                        }),
                                    ])
                                    .on_change(move |r, _, cx| {
                                        owner.update(cx, |v, cx| {
                                            v.nested_value = r.expanded_keys().to_vec();
                                            cx.notify();
                                        })
                                    }),
                            );
                        }
                        body.into_any_element()
                    });
                if self.custom {
                    item = item.indicator(outline::Star);
                }
                item
            })
            .collect::<Vec<_>>();
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .p_4()
            .gap_3()
            .child(YueButton::new("before", "之前").track_focus(&self.before))
            .child(
                YueCollapse::new("collapse", self.expanded.clone())
                    .items(items)
                    .theme(self.theme)
                    .accessibility_label("测试折叠面板")
                    .mode(self.mode)
                    .size(self.size)
                    .variant(self.variant)
                    .expand_trigger(self.trigger)
                    .indicator_position(self.position)
                    .motion(self.motion)
                    .disabled(self.disabled)
                    .show_indicator(self.indicator)
                    .width(px(self.width))
                    .on_change(cx.listener(|v, r: &CollapseChangeRequest, _, cx| {
                        v.requests.push(r.clone());
                        if v.accept {
                            v.expanded = r.expanded_keys().to_vec();
                        }
                        cx.notify();
                    })),
            )
            .child(YueButton::new("after", "之后").track_focus(&self.after))
            .child(
                YueDropdownMenu::new(
                    "outside-menu",
                    self.theme,
                    YueButton::new("outside-menu-trigger", "外部菜单"),
                )
                .items([YueMenuItem::new("外部命令")]),
            );
        if self.clipped {
            div()
                .w_full()
                .h(px(1.))
                .overflow_hidden()
                .child(content)
                .into_any_element()
        } else {
            content.into_any_element()
        }
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

// Deliver only the frame the component itself requested. An unconditional
// refresh here would conceal failures in the real window's frame scheduling.
fn scheduled_frame(cx: &mut VisualTestContext, ms: u64) -> usize {
    cx.executor().advance_clock(Duration::from_millis(ms));
    let callbacks = cx.update(|w, cx| w.simulate_next_frame(cx));
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.run_until_parked();
    callbacks
}

#[gpui_kit::test]
fn hovering_headers_does_not_paint_a_background(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for trigger in [CollapseExpandTrigger::Header, CollapseExpandTrigger::Icon] {
            v.update(cx, |v, cx| {
                v.theme = YueTheme::new(mode, Accent::Azure);
                v.trigger = trigger;
                cx.notify();
            });
            cx.simulate_mouse_move(gpui_kit::point(px(1.), px(1.)), None, Modifiers::default());
            draw(cx);
            tick(cx, 300);
            let bounds = node(cx, "标题 a").bounds();
            let surfaces = |cx: &mut VisualTestContext| {
                cx.update(|w, _| {
                    let bounds = bounds.scale(w.scale_factor());
                    w.painted_quads()
                        .into_iter()
                        .filter_map(|q| {
                            q.background
                                .as_solid()
                                .filter(|c| c.a > 0.)
                                .and_then(|c| q.bounds.intersects(&bounds).then_some((q.bounds, c)))
                        })
                        .collect::<Vec<_>>()
                })
            };
            let before = surfaces(cx);
            cx.simulate_mouse_move(bounds.center(), None, Modifiers::default());
            draw(cx);
            tick(cx, 300);
            assert_eq!(surfaces(cx), before, "Hover must not add a header wash");
            assert!(v.read_with(cx, |v, _| v.requests.is_empty()));
        }
    }
}

#[gpui_kit::test]
fn disclosure_runs_both_directions_using_only_its_requested_frames(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let full = part(cx, "a", "body").size.height;
    v.update(cx, |v, cx| {
        v.expanded.clear();
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let mut previous = full;
    for _ in 0..18 {
        assert!(scheduled_frame(cx, 16) > 0);
        let current = part(cx, "a", "body").size.height;
        assert!(
            current < previous,
            "Close must advance each frame: {previous:?} -> {current:?}"
        );
        previous = current;
    }
    scheduled_frame(cx, 60);
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
    v.update(cx, |v, cx| {
        v.expanded = vec!["a".into()];
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
    previous = px(0.);
    for _ in 0..18 {
        assert!(scheduled_frame(cx, 16) > 0);
        let current = part(cx, "a", "body").size.height;
        assert!(
            current > previous,
            "Open must advance each frame: {previous:?} -> {current:?}"
        );
        previous = current;
    }
    scheduled_frame(cx, 60);
    assert_eq!(part(cx, "a", "body").size.height, full);
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
        let nodes: Vec<_> = snapshots(w)
            .into_iter()
            .filter(|n| n.label() == Some(label) && n.role() != Some(Role::Region))
            .collect();
        assert_eq!(nodes.len(), 1, "Unique {label}: {nodes:?}");
        nodes[0].clone()
    })
}
fn part(cx: &mut VisualTestContext, key: &str, part: &str) -> Bounds<Pixels> {
    let id: ElementId = (
        ElementId::from("collapse"),
        SharedString::from(key.to_string()),
    )
        .into();
    let id: ElementId = (id, SharedString::from(part.to_string())).into();
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&id))
            .unwrap_or_else(|| panic!("Missing {id:?}"))
            .bounds()
    })
}
fn click(cx: &mut VisualTestContext, label: &str) {
    let p = node(cx, label).bounds().center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
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
fn absent(cx: &mut VisualTestContext, label: &str) -> bool {
    cx.update(|w, _| !snapshots(w).iter().any(|n| n.label() == Some(label)))
}

#[gpui_kit::test]
fn initial_state_has_no_reveal_flash_and_projects_native_semantics(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    assert_eq!(node(cx, "测试折叠面板").role(), Some(Role::Group));
    assert_eq!(node(cx, "标题 a").role(), Some(Role::Button));
    assert_eq!(node(cx, "标题 a").expanded(), Some(true));
    assert_eq!(node(cx, "标题 b").expanded(), Some(false));
    let height = part(cx, "a", "body").size.height;
    assert!(height > px(80.));
    tick(cx, 60);
    assert_eq!(part(cx, "a", "body").size.height, height);
    assert_eq!(v.read_with(cx, |v, _| v.builds[1].get()), 0);
    v.update(cx, |v, cx| {
        v.region = true;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, _| {
        assert!(
            snapshots(w)
                .iter()
                .any(|n| n.role() == Some(Role::Region) && n.label() == Some("标题 a"))
        )
    });
}

#[gpui_kit::test]
fn controlled_rejection_and_accordion_switch_are_single_atomic_requests(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.mode = CollapseMode::Accordion;
        v.accept = false;
        cx.notify();
    });
    draw(cx);
    click(cx, "标题 b");
    assert_eq!(node(cx, "标题 a").expanded(), Some(true));
    assert_eq!(node(cx, "标题 b").expanded(), Some(false));
    v.read_with(cx, |v, _| {
        assert_eq!(v.requests.len(), 1);
        let r = &v.requests[0];
        assert_eq!(r.key().as_ref(), "b");
        assert!(r.is_expanded());
        assert_eq!(r.expanded_keys(), &[SharedString::from("b")]);
        assert_eq!(r.source(), CollapseChangeSource::Pointer);
    });
    v.update(cx, |v, cx| {
        v.accept = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "标题 b");
    assert!(absent(cx, "正文动作 0"));
    assert_eq!(node(cx, "标题 b").expanded(), Some(true));
    click(cx, "标题 b");
    assert!(v.read_with(cx, |v, _| v.expanded.is_empty()));
    assert_eq!(v.read_with(cx, |v, _| v.requests.len()), 3);
}

#[gpui_kit::test]
fn external_keys_are_projected_in_item_order_without_synthetic_requests(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.expanded = ["c", "missing", "b", "b"].map(Into::into).to_vec();
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标题 a").expanded(), Some(false));
    assert_eq!(node(cx, "标题 b").expanded(), Some(true));
    assert_eq!(node(cx, "标题 c").expanded(), Some(true));
    v.update(cx, |v, cx| {
        v.mode = CollapseMode::Accordion;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标题 b").expanded(), Some(true));
    assert_eq!(node(cx, "标题 c").expanded(), Some(false));
    assert!(v.read_with(cx, |v, _| v.requests.is_empty()));
}

#[gpui_kit::test]
fn keyboard_navigation_wraps_skips_disabled_and_never_auto_expands(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.disabled_keys.insert("b".into());
        cx.notify();
    });
    draw(cx);
    click(cx, "标题 a");
    key(cx, "down");
    assert_eq!(node(cx, "标题 c").focused(), Some(true));
    assert_eq!(node(cx, "标题 c").expanded(), Some(false));
    key(cx, "down");
    assert_eq!(node(cx, "标题 a").focused(), Some(true));
    key(cx, "up");
    assert_eq!(node(cx, "标题 c").focused(), Some(true));
    key(cx, "home");
    assert_eq!(node(cx, "标题 a").focused(), Some(true));
    key(cx, "end");
    assert_eq!(node(cx, "标题 c").focused(), Some(true));
    let count = v.read_with(cx, |v, _| v.requests.len());
    key(cx, "enter");
    assert_eq!(v.read_with(cx, |v, _| v.requests.len()), count + 1);
    assert_eq!(
        v.read_with(cx, |v, _| v.requests.last().unwrap().source()),
        CollapseChangeSource::Keyboard
    );
    key(cx, "space");
    assert_eq!(node(cx, "标题 c").expanded(), Some(false));
}

#[gpui_kit::test]
fn all_enabled_headers_and_independent_commands_remain_tab_stops(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.expanded.clear();
        v.motion = CollapseMotion::None;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| v.read(cx).before.clone().focus(w, cx));
    for label in [
        "标题 a", "操作 a", "禁用 a", "标题 b", "操作 b", "禁用 b", "标题 c",
    ] {
        key(cx, "tab");
        assert_eq!(node(cx, label).focused(), Some(true), "{label}");
    }
}

#[gpui_kit::test]
fn disabled_only_gates_disclosure_and_actions_can_reenable_it(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "禁用 a");
    assert_eq!(node(cx, "标题 a").disabled(), Some(true));
    click(cx, "标题 a");
    assert_eq!(node(cx, "标题 a").expanded(), Some(true));
    click(cx, "操作 a");
    click(cx, "正文动作 0");
    assert_eq!(v.read_with(cx, |v, _| v.actions.get()), 2);
    assert!(v.read_with(cx, |v, _| v.requests.is_empty()));
    click(cx, "禁用 a");
    click(cx, "标题 a");
    assert_eq!(node(cx, "标题 a").expanded(), Some(false));
    v.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "操作 b");
    assert_eq!(v.read_with(cx, |v, _| v.actions.get()), 3);
}

#[gpui_kit::test]
fn icon_trigger_and_actions_have_disjoint_hit_regions(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.trigger = CollapseExpandTrigger::Icon;
        cx.notify();
    });
    draw(cx);
    let title = part(cx, "a", "title").center();
    cx.simulate_click(title, Modifiers::default());
    draw(cx);
    assert!(v.read_with(cx, |v, _| v.requests.is_empty()));
    click(cx, "操作 a");
    assert!(v.read_with(cx, |v, _| v.requests.is_empty()));
    click(cx, "标题 a");
    assert_eq!(node(cx, "标题 a").expanded(), Some(false));
    let leading = part(cx, "a", "title");
    v.update(cx, |v, cx| {
        v.position = CollapseIndicatorPosition::Trailing;
        cx.notify();
    });
    draw(cx);
    assert!(part(cx, "a", "title").left() < leading.left());
    assert!(node(cx, "标题 a").bounds().right() <= part(cx, "a", "actions").left());
}

#[gpui_kit::test]
fn close_detaches_content_immediately_and_frozen_frame_has_intermediate_height(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx);
    let full = part(cx, "a", "body").size.height;
    click(cx, "标题 a");
    assert!(absent(cx, "正文动作 0"));
    let built = v.read_with(cx, |v, _| v.builds[0].get());
    assert_eq!(part(cx, "a", "body").size.height, full);
    tick(cx, 40);
    let middle = part(cx, "a", "body").size.height;
    assert_eq!(v.read_with(cx, |v, _| v.builds[0].get()), built);
    assert!(middle > px(0.) && middle < full, "{middle:?} / {full:?}");
    tick(cx, 300);
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
    click(cx, "标题 a");
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
    tick(cx, 70);
    let middle = part(cx, "a", "body").size.height;
    assert!(middle > px(0.) && middle < full);
    tick(cx, 300);
    assert_eq!(part(cx, "a", "body").size.height, full);
}

fn painted_body_markers(cx: &mut VisualTestContext) -> Vec<gpui_kit::Quad> {
    cx.update(|w, _| {
        let color = gpui_kit::Hsla::from(gpui_kit::rgb(0x2468ab));
        let mut markers = w
            .painted_quads()
            .into_iter()
            .filter(|quad| quad.background.as_solid() == Some(color))
            .collect::<Vec<_>>();
        markers.sort_by(|a, b| a.bounds.origin.y.partial_cmp(&b.bounds.origin.y).unwrap());
        markers
    })
}

#[gpui_kit::test]
fn closing_clips_frozen_content_until_zero_without_keeping_its_controls(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.paint_probe = true;
        cx.notify();
    });
    draw(cx);
    let original = painted_body_markers(cx).remove(0);
    let old_button = node(cx, "正文动作 0").bounds().center();
    let actions = v.read_with(cx, |v, _| v.actions.get());
    click(cx, "标题 a");
    let built = v.read_with(cx, |v, _| v.builds[0].get());
    assert!(absent(cx, "正文动作 0"));
    assert_eq!(painted_body_markers(cx)[0].bounds, original.bounds);
    cx.simulate_click(old_button, Modifiers::default());
    draw(cx);
    assert_eq!(v.read_with(cx, |v, _| v.actions.get()), actions);
    tick(cx, 100);
    let first = painted_body_markers(cx).remove(0);
    let visible = first.bounds.intersect(&first.content_mask.bounds);
    assert_eq!(
        first.bounds, original.bounds,
        "Frozen content must not scale its text or shapes"
    );
    assert!(visible.size.height > px(0.).scale(1.));
    assert!(visible.size.height < original.bounds.size.height);
    tick(cx, 100);
    let second = painted_body_markers(cx).remove(0);
    assert!(
        second
            .bounds
            .intersect(&second.content_mask.bounds)
            .size
            .height
            < visible.size.height
    );
    assert_eq!(v.read_with(cx, |v, _| v.builds[0].get()), built);
    tick(cx, 140);
    assert!(painted_body_markers(cx).is_empty());
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
}

#[gpui_kit::test]
fn simultaneous_closing_translates_each_frozen_frame_with_its_own_panel(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.paint_probe = true;
        v.expanded = vec!["a".into(), "b".into()];
        cx.notify();
    });
    draw(cx);
    tick(cx, 350);
    let original = painted_body_markers(cx);
    assert_eq!(original.len(), 2);
    v.update(cx, |v, cx| {
        v.expanded.clear();
        cx.notify();
    });
    draw(cx);
    tick(cx, 100);
    let closing = painted_body_markers(cx);
    assert_eq!(closing.len(), 2);
    assert_eq!(closing[0].bounds.origin, original[0].bounds.origin);
    assert!(closing[1].bounds.origin.y < original[1].bounds.origin.y);
    let panel_b = part(cx, "b", "body");
    cx.update(|w, _| {
        assert_eq!(
            closing[1].bounds.origin,
            panel_b.origin.scale(w.scale_factor())
        )
    });
    assert!(absent(cx, "正文动作 0") && absent(cx, "正文动作 1"));
    tick(cx, 250);
    assert!(painted_body_markers(cx).is_empty());
}

#[gpui_kit::test]
fn rapid_reversal_starts_at_current_pixels_and_dynamic_height_retargets(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "标题 a");
    // Reverse after meaningful travel so the shortened return still has an
    // intermediate frame before the dynamic-height retarget below.
    tick(cx, 110);
    let close_height = part(cx, "a", "body").size.height;
    click(cx, "标题 a");
    assert!((part(cx, "a", "body").size.height - close_height).abs() < px(1.));
    tick(cx, 45);
    let enter_height = part(cx, "a", "body").size.height;
    assert!(enter_height > close_height);
    v.update(cx, |v, cx| {
        v.height = 200.;
        cx.notify();
    });
    draw(cx);
    assert!((part(cx, "a", "body").size.height - enter_height).abs() < px(1.));
    tick(cx, 350);
    let full = part(cx, "a", "body").size.height;
    assert!(full > px(200.));
    v.update(cx, |v, cx| {
        v.height = 120.;
        cx.notify();
    });
    draw(cx);
    assert!(part(cx, "a", "body").size.height < full);
    let stable = part(cx, "a", "body").size.height;
    tick(cx, 60);
    assert_eq!(part(cx, "a", "body").size.height, stable);
}

#[gpui_kit::test]
fn external_collapse_repairs_content_focus_but_preserves_other_header_focus(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx);
    click(cx, "正文动作 0");
    v.update(cx, |v, cx| {
        v.expanded.clear();
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标题 a").focused(), Some(true));
    v.update(cx, |v, cx| {
        v.expanded = vec!["a".into()];
        v.mode = CollapseMode::Accordion;
        cx.notify();
    });
    draw(cx);
    tick(cx, 250);
    click(cx, "标题 b");
    assert_eq!(node(cx, "标题 b").focused(), Some(true));
}

#[gpui_kit::test]
fn removed_or_disabled_focus_target_falls_to_neighbor_then_group_boundary(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "正文动作 0");
    v.update(cx, |v, cx| {
        v.keys.remove(0);
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标题 b").focused(), Some(true));
    v.update(cx, |v, cx| {
        v.disabled_keys.insert("b".into());
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标题 c").focused(), Some(true));
    v.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "测试折叠面板").focused(), Some(true));
    assert_eq!(node(cx, "标题 c").disabled(), Some(true));
}

#[gpui_kit::test]
fn retained_input_preserves_value_and_hidden_input_leaves_the_tree(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.with_input = true;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| {
        v.read(cx).input.clone().update(cx, |i, cx| {
            i.set_value("保留文本", w, cx);
            i.focus(w, cx);
        })
    });
    draw(cx);
    v.update(cx, |v, cx| {
        v.expanded.clear();
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "标题 a").focused(), Some(true));
    cx.update(|w, _| {
        assert!(
            !snapshots(w)
                .iter()
                .any(|n| n.role() == Some(Role::TextInput))
        )
    });
    tick(cx, 120);
    click(cx, "标题 a");
    tick(cx, 220);
    assert_eq!(
        v.read_with(cx, |v, cx| v.input.read(cx).value().to_string()),
        "保留文本"
    );
}

#[gpui_kit::test]
fn nested_navigation_is_scoped_to_the_focused_group(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.nested = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "内层 x");
    key(cx, "down");
    assert_eq!(node(cx, "内层 y").focused(), Some(true));
    key(cx, "enter");
    assert_eq!(node(cx, "内层 y").expanded(), Some(true));
    assert!(v.read_with(cx, |v, _| v.requests.is_empty()));
    click(cx, "标题 a");
    tick(cx, 120);
    click(cx, "标题 a");
    tick(cx, 220);
    assert_eq!(node(cx, "内层 y").expanded(), Some(true));
}

#[gpui_kit::test]
fn reduced_motion_none_and_geometry_changes_land_at_final_height(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let full = part(cx, "a", "body").size.height;
    click(cx, "标题 a");
    tick(cx, 30);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
    click(cx, "标题 a");
    assert_eq!(part(cx, "a", "body").size.height, full);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    v.update(cx, |v, cx| {
        v.motion = CollapseMotion::None;
        cx.notify();
    });
    draw(cx);
    click(cx, "标题 a");
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
    click(cx, "标题 a");
    assert_eq!(part(cx, "a", "body").size.height, full);
    v.update(cx, |v, cx| {
        v.motion = CollapseMotion::Height;
        cx.notify();
    });
    draw(cx);
    click(cx, "标题 a");
    tick(cx, 30);
    v.update(cx, |v, cx| {
        v.size = CollapseSize::Large;
        v.width = 330.;
        v.theme = YueTheme::new(ThemeMode::Dark, Accent::Azure);
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
}

#[gpui_kit::test]
fn width_change_during_opening_remeasures_without_replaying_old_geometry(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "标题 a");
    tick(cx, 120);
    click(cx, "标题 a");
    tick(cx, 35);
    v.update(cx, |v, cx| {
        v.width = 300.;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(part(cx, "a", "body").size.height > px(80.));
    let landed = part(cx, "a", "body").size.height;
    tick(cx, 80);
    assert_eq!(part(cx, "a", "body").size.height, landed);
}

#[gpui_kit::test]
fn hidden_indicator_reclaims_slot_and_icon_modes_keep_title_alignment(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    let with_icon = part(cx, "a", "title").left();
    v.update(cx, |v, cx| {
        v.trigger = CollapseExpandTrigger::Icon;
        cx.notify();
    });
    draw(cx);
    assert!((part(cx, "a", "title").left() - with_icon).abs() <= px(0.01));
    v.update(cx, |v, cx| {
        v.trigger = CollapseExpandTrigger::Header;
        v.indicator = false;
        cx.notify();
    });
    draw(cx);
    assert!(part(cx, "a", "title").left() < with_icon - px(20.));
}

#[gpui_kit::test]
fn multiline_title_preserves_line_height_and_content_alignment_across_trigger_modes(
    cx: &mut TestAppContext,
) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.long_title = true;
        v.width = 340.;
        v.padding = true;
        v.motion = CollapseMotion::None;
        cx.notify();
    });
    draw(cx);
    let header_title = part(cx, "a", "title");
    assert!(header_title.size.height > px(20.));
    assert!((node(cx, "正文动作 0").bounds().left() - header_title.left()).abs() <= px(1.));
    v.update(cx, |v, cx| {
        v.trigger = CollapseExpandTrigger::Icon;
        cx.notify();
    });
    draw(cx);
    let icon_title = part(cx, "a", "title");
    assert_eq!(icon_title.size.height, header_title.size.height);
    assert_eq!(icon_title.left(), header_title.left());
    assert!((node(cx, "正文动作 0").bounds().left() - icon_title.left()).abs() <= px(1.));
    assert!(icon_title.right() <= part(cx, "a", "actions").left());
}

#[gpui_kit::test]
fn closing_a_panel_releases_only_its_popup_anchor(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.with_menu = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "正文菜单");
    tick(cx, 300);
    assert!(!absent(cx, "正文命令"));
    v.update(cx, |v, cx| {
        v.expanded.clear();
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(absent(cx, "正文命令"));
    tick(cx, 120);
    click(cx, "外部菜单");
    tick(cx, 300);
    assert!(!absent(cx, "外部命令"));
    v.update(cx, |v, cx| {
        v.keys.remove(0);
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(!absent(cx, "外部命令"));
}

#[gpui_kit::test]
fn geometry_scales_with_rem_display_scale_theme_and_all_sizes(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    for rem in [16., 20., 24.] {
        for scale in [1., 1.5, 2.] {
            cx.simulate_scale_factor_change(scale);
            cx.update(|w, _| w.set_rem_size(px(rem)));
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                for (size, hit) in [
                    (CollapseSize::Small, 24.),
                    (CollapseSize::Medium, 28.),
                    (CollapseSize::Large, 32.),
                ] {
                    v.update(cx, |v, cx| {
                        v.size = size;
                        v.theme = YueTheme::new(mode, Accent::Azure);
                        v.motion = CollapseMotion::None;
                        v.width = 600.;
                        v.trigger = CollapseExpandTrigger::Icon;
                        cx.notify();
                    });
                    draw(cx);
                    let trigger = node(cx, "标题 a").bounds();
                    let expected = px(hit * rem / 16.);
                    assert!(
                        (trigger.size.width - expected).abs() <= px(1. / scale + 0.01),
                        "{size:?} rem={rem} scale={scale}: {trigger:?}"
                    );
                    assert!(trigger.right() <= part(cx, "a", "title").left());
                    assert!(part(cx, "a", "title").right() <= part(cx, "a", "actions").left());
                }
            }
        }
    }
}

#[gpui_kit::test]
fn settled_and_clipped_disclosures_do_not_schedule_continuous_frames(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    // Drain finite initialization/hover invalidations, then inspect the actual
    // platform callback queue without forcing another redraw.
    tick(cx, 500);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
    v.update(cx, |v, cx| {
        v.expanded.clear();
        cx.notify();
    });
    draw(cx);
    tick(cx, 350);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
    v.update(cx, |v, cx| {
        v.clipped = true;
        v.expanded = vec!["a".into()];
        cx.notify();
    });
    draw(cx);
    draw(cx);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
}

#[gpui_kit::test]
#[should_panic(expected = "Icon-only collapse requires a visible indicator")]
fn icon_trigger_without_indicator_is_rejected(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    v.update(cx, |v, cx| {
        v.trigger = CollapseExpandTrigger::Icon;
        v.indicator = false;
        cx.notify();
    });
    draw(cx);
}

#[gpui_kit::test]
fn hidden_or_inactive_window_lands_and_stops_requesting_frames(cx: &mut TestAppContext) {
    let (v, cx) = open(cx);
    click(cx, "标题 a");
    tick(cx, 30);
    cx.deactivate_window();
    draw(cx);
    draw(cx);
    assert_eq!(part(cx, "a", "body").size.height, px(0.));
    tick(cx, 500);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
    cx.update(|w, _| w.activate_window());
    draw(cx);
    click(cx, "标题 a");
    tick(cx, 30);
    cx.simulate_visibility_change(gpui_kit::WindowVisibility::Hidden);
    draw(cx);
    draw(cx);
    let full = part(cx, "a", "body").size.height;
    assert!(full > px(80.));
    tick(cx, 500);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
    cx.simulate_visibility_change(gpui_kit::WindowVisibility::Visible);
    draw(cx);
    assert_eq!(part(cx, "a", "body").size.height, full);
    assert!(v.read_with(cx, |v, _| v.expanded.contains(&"a".into())));
}

#[test]
#[should_panic(expected = "unique item keys")]
fn duplicate_keys_are_rejected() {
    let _ = YueCollapse::new("duplicate", []).items([
        YueCollapseItem::new("a", "A"),
        YueCollapseItem::new("a", "B"),
    ]);
}

#[test]
#[should_panic(expected = "nonempty key and title")]
fn empty_keys_are_rejected() {
    let _ = YueCollapseItem::new(" ", "A");
}

#[test]
#[should_panic]
fn invalid_heading_levels_are_rejected() {
    let _ = YueCollapse::new("invalid-heading", []).heading_level(7);
}
