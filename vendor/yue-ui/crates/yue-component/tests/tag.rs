//! Tag contracts exercised through Base's native input and scene observations.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, MouseMoveEvent, ParentElement as _, Pixels, Render, Role, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point,
    prelude::FluentBuilder as _, px,
};
use yue_component::{
    Accent, TagColors, TagShape, TagSize, TagTone, TagVariant, ThemeMode, YueButton, YueTag,
    YueTagGroup, YueTheme, outline,
};

struct Probe {
    mode: u8,
    checked: bool,
    accept: bool,
    visible: bool,
    disabled: bool,
    closable: bool,
    leading: bool,
    label: SharedString,
    actions: usize,
    changes: Vec<bool>,
    closes: usize,
    size: TagSize,
    shape: TagShape,
    variant: TagVariant,
    tone: TagTone,
    width: Option<Pixels>,
    min_width: Option<Pixels>,
    max_width: Option<Pixels>,
    before: FocusHandle,
    body: FocusHandle,
    close: FocusHandle,
    after: FocusHandle,
    skip_body: bool,
    skip_close: bool,
    group: bool,
    wrap: bool,
    reversed: bool,
    theme: Option<YueTheme>,
    colors: Option<TagColors>,
}
impl Probe {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            mode: 1,
            checked: false,
            accept: true,
            visible: true,
            disabled: false,
            closable: true,
            leading: false,
            label: "标签内容".into(),
            actions: 0,
            changes: vec![],
            closes: 0,
            size: TagSize::Medium,
            shape: TagShape::Rounded,
            variant: TagVariant::Tint,
            tone: TagTone::Primary,
            width: None,
            min_width: None,
            max_width: None,
            before: cx.focus_handle(),
            body: cx.focus_handle(),
            close: cx.focus_handle(),
            after: cx.focus_handle(),
            skip_body: false,
            skip_close: false,
            group: false,
            wrap: true,
            reversed: false,
            theme: None,
            colors: None,
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tag = match self.mode {
            0 => YueTag::new("probe-tag", self.label.clone()),
            1 => YueTag::action("probe-tag", self.label.clone()).on_click(cx.listener(
                |v, _, _, cx| {
                    v.actions += 1;
                    cx.notify();
                },
            )),
            _ => YueTag::checkable("probe-tag", self.label.clone(), self.checked).on_change({
                let view = cx.entity();
                move |next, _, _, cx| {
                    view.update(cx, |v, cx| {
                        v.changes.push(next);
                        if v.accept {
                            v.checked = next;
                        }
                        cx.notify();
                    })
                }
            }),
        }
        .size(self.size)
        .shape(self.shape)
        .variant(self.variant)
        .tone(self.tone)
        .disabled(self.disabled)
        .tab_stop(!self.skip_body)
        .close_tab_stop(!self.skip_close)
        .track_focus(&self.body)
        .track_close_focus(&self.close)
        .accessibility_label("主体完整名称")
        .accessibility_description("标签说明")
        .when(self.leading, |t| t.leading_icon(outline::Folder));
        if self.closable {
            tag = tag.on_close(cx.listener(|v, _, window, cx| {
                v.closes += 1;
                if v.accept {
                    let restore = v.body.is_focused(window) || v.close.is_focused(window);
                    v.visible = false;
                    if restore {
                        v.after.focus(window, cx);
                    }
                }
                cx.notify();
            }));
        }
        if let Some(width) = self.width {
            tag = tag.width(width);
        }
        if let Some(width) = self.min_width {
            tag = tag.min_width(width);
        }
        if let Some(width) = self.max_width {
            tag = tag.max_width(width);
        }
        if let Some(theme) = self.theme {
            tag = tag.theme(theme);
        }
        if let Some(colors) = self.colors {
            tag = tag.colors(colors);
        }
        let mut tags = if self.visible { vec![tag] } else { vec![] };
        if self.group {
            for id in ["second", "third", "fourth"] {
                tags.push(YueTag::action(id, format!("{id} 标签")).on_click(|_, _, _| {}));
            }
        }
        if self.reversed {
            tags.reverse();
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .p_4()
            .gap_4()
            .child(YueButton::new("before", "前一个").track_focus(&self.before))
            .child(
                YueTagGroup::new("probe-group")
                    .width(px(if self.group { 230. } else { 500. }))
                    .wrap(self.wrap)
                    .children(tags),
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
fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
    });
    let mut view = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let probe = cx.new(Probe::new);
        view = Some(probe.clone());
        gpui_kit::base::Root::new(probe, w, cx)
    });
    cx.update(|w, _| w.activate_window());
    draw(cx);
    (view.unwrap(), cx)
}
fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|w, _| {
        let nodes = snapshots(w)
            .into_iter()
            .filter(|n| n.label() == Some(label))
            .collect::<Vec<_>>();
        assert_eq!(nodes.len(), 1, "unique {label}: {nodes:?}");
        nodes[0].clone()
    })
}
fn bounds(cx: &mut VisualTestContext, id: impl Into<gpui_kit::ElementId>) -> Bounds<Pixels> {
    let id = id.into();
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&id))
            .unwrap_or_else(|| panic!("missing {id:?}"))
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
fn tick(cx: &mut VisualTestContext, ms: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(ms));
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    draw(cx);
}
fn hover(cx: &mut VisualTestContext, label: Option<&str>) {
    let position = label
        .map(|l| node(cx, l).bounds().center())
        .unwrap_or(point(px(700.), px(500.)));
    cx.simulate_event(MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Modifiers::default(),
    });
    draw(cx);
}
fn wash_alpha(cx: &mut VisualTestContext, label: &str) -> f32 {
    let bounds = node(cx, label).bounds();
    cx.update(|w, _| {
        let b = bounds.scale(w.scale_factor());
        w.painted_quads()
            .iter()
            .filter(|q| q.bounds == b)
            .filter_map(|q| q.background.as_solid().map(|c| c.a))
            .fold(0., f32::max)
    })
}
fn surface_color(cx: &mut VisualTestContext) -> gpui_kit::Rgba {
    let b = bounds(cx, "probe-tag");
    cx.update(|w, _| {
        let b = b.scale(w.scale_factor());
        w.painted_quads()
            .iter()
            .filter(|q| q.bounds == b)
            .filter_map(|q| q.background.as_solid())
            .find(|c| c.a > 0.)
            .expect("tag fill")
            .into()
    })
}
fn near_color(actual: gpui_kit::Rgba, expected: gpui_kit::Rgba) {
    for (a, b) in [
        (actual.r, expected.r),
        (actual.g, expected.g),
        (actual.b, expected.b),
        (actual.a, expected.a),
    ] {
        assert!((a - b).abs() < 0.001, "{actual:?} versus {expected:?}");
    }
}

#[gpui_kit::test]
fn action_and_close_are_sibling_buttons_with_independent_activation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let body = node(cx, "主体完整名称");
    let close = node(cx, "移除 标签内容");
    assert_eq!(body.role(), Some(Role::Button));
    assert_eq!(close.role(), Some(Role::Button));
    assert_eq!(body.description(), Some("标签说明"));
    assert_eq!(body.path().len(), close.path().len());
    assert_eq!(
        &body.path()[..body.path().len() - 1],
        &close.path()[..close.path().len() - 1]
    );
    click(cx, "主体完整名称");
    assert_eq!(view.read_with(cx, |v, _| (v.actions, v.closes)), (1, 0));
    click(cx, "移除 标签内容");
    assert_eq!(
        view.read_with(cx, |v, _| (v.actions, v.closes, v.visible)),
        (1, 1, false)
    );
}

#[gpui_kit::test]
fn static_body_has_no_button_role_or_tab_stop(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.mode = 0;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "主体完整名称").role(), Some(Role::Caption));
    assert_eq!(node(cx, "主体完整名称").focused(), None);
    cx.update(|w, cx| view.read(cx).before.clone().focus(w, cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "移除 标签内容").focused(), Some(true));
    click(cx, "主体完整名称");
    assert_eq!(view.read_with(cx, |v, _| v.actions), 0);
}

#[gpui_kit::test]
fn toggle_requests_are_controlled_and_close_never_unchecks(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.mode = 2;
        v.accept = false;
        cx.notify();
    });
    draw(cx);
    click(cx, "主体完整名称");
    click(cx, "主体完整名称");
    assert_eq!(node(cx, "主体完整名称").checked(), Some(false));
    assert_eq!(view.read_with(cx, |v, _| v.changes.clone()), [true, true]);
    view.update(cx, |v, cx| {
        v.checked = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 2);
    assert_eq!(node(cx, "主体完整名称").checked(), Some(true));
    click(cx, "移除 标签内容");
    assert_eq!(
        view.read_with(cx, |v, _| (v.visible, v.checked, v.changes.len(), v.closes)),
        (true, true, 2, 1)
    );
    view.update(cx, |v, cx| {
        v.accept = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "主体完整名称");
    assert_eq!(node(cx, "主体完整名称").checked(), Some(false));
}

#[gpui_kit::test]
fn keyboard_activates_body_and_close_once_and_host_restores_focus(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|w, cx| view.read(cx).body.clone().focus(w, cx));
    key(cx, "enter");
    key(cx, "space");
    assert_eq!(view.read_with(cx, |v, _| v.actions), 2);
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "移除 标签内容").focused(), Some(true));
    key(cx, "space");
    assert_eq!(view.read_with(cx, |v, _| (v.actions, v.closes)), (2, 1));
    assert_eq!(node(cx, "后一个").focused(), Some(true));
}

#[gpui_kit::test]
fn disabled_toggle_preserves_value_blocks_both_parts_and_skips_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.mode = 2;
        v.checked = true;
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    for label in ["主体完整名称", "移除 标签内容"] {
        assert_eq!(node(cx, label).disabled(), Some(true));
        click(cx, label);
    }
    assert_eq!(node(cx, "主体完整名称").checked(), Some(true));
    cx.update(|w, cx| view.read(cx).before.clone().focus(w, cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "后一个").focused(), Some(true));
    assert_eq!(
        view.read_with(cx, |v, _| (v.changes.len(), v.closes)),
        (0, 0)
    );
}

#[gpui_kit::test]
fn independent_tab_policies_apply_to_caller_owned_handles(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for (skip_body, skip_close, target) in [
        (false, false, "主体完整名称"),
        (true, false, "移除 标签内容"),
        (true, true, "后一个"),
    ] {
        view.update(cx, |v, cx| {
            v.skip_body = skip_body;
            v.skip_close = skip_close;
            cx.notify();
        });
        draw(cx);
        cx.update(|w, cx| view.read(cx).before.clone().focus(w, cx));
        cx.update(|w, cx| w.focus_next(cx));
        draw(cx);
        assert_eq!(node(cx, target).focused(), Some(true));
    }
}

#[gpui_kit::test]
fn compact_geometry_scales_and_text_does_not_overlap_close(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for rem in [16., 24.] {
        cx.update(|w, _| w.set_rem_size(px(rem)));
        for (size, height) in [
            (TagSize::Small, 20.),
            (TagSize::Medium, 24.),
            (TagSize::Large, 30.),
        ] {
            for shape in [TagShape::Rounded, TagShape::Pill, TagShape::Mark] {
                view.update(cx, |v, cx| {
                    v.leading = true;
                    v.width = Some(px(170. * rem / 16.));
                    v.size = size;
                    v.shape = shape;
                    cx.notify();
                });
                draw(cx);
                let root = bounds(cx, "probe-tag");
                let label = bounds(cx, (gpui_kit::ElementId::from("probe-tag"), "label"));
                let close = node(cx, "移除 标签内容").bounds();
                assert_eq!(root.size.height, px(height * rem / 16.));
                assert_eq!(close.size.width, root.size.height);
                assert_eq!(close.size.height, root.size.height);
                assert!(label.right() <= close.left());
                assert!(close.right() <= root.right());
            }
        }
    }
}

#[gpui_kit::test]
fn a_supplied_leading_icon_keeps_width_stable_when_selection_changes(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.mode = 2;
        v.leading = true;
        cx.notify();
    });
    draw(cx);
    let original = bounds(cx, "probe-tag");
    for variant in [
        TagVariant::Filled,
        TagVariant::Tint,
        TagVariant::Outline,
        TagVariant::TintOutline,
    ] {
        view.update(cx, |v, cx| {
            v.variant = variant;
            cx.notify();
        });
        draw(cx);
        click(cx, "主体完整名称");
        tick(cx, 150);
        assert_eq!(bounds(cx, "probe-tag").size, original.size);
    }
}

#[gpui_kit::test]
fn an_unchecked_tag_has_no_mark_reserve_and_selection_reveals_it_smoothly(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.closable = false;
        cx.notify();
    });
    draw(cx);
    let plain = bounds(cx, "probe-tag");
    let plain_label = bounds(cx, (gpui_kit::ElementId::from("probe-tag"), "label"));
    view.update(cx, |v, cx| {
        v.mode = 2;
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        bounds(cx, "probe-tag"),
        plain,
        "unchecked tags must not reserve an invisible icon"
    );
    let label = bounds(cx, (gpui_kit::ElementId::from("probe-tag"), "label"));
    assert_eq!(
        label, plain_label,
        "neither leading nor trailing padding gains an empty slot"
    );
    let difference = f32::from((label.left() - plain.left()) - (plain.right() - label.right()));
    assert!(difference.abs() < 0.01);
    click(cx, "主体完整名称");
    assert_eq!(bounds(cx, "probe-tag").size.width, plain.size.width);
    tick(cx, 35);
    let middle = bounds(cx, "probe-tag").size.width;
    tick(cx, 150);
    let selected = bounds(cx, "probe-tag");
    assert!(
        middle > plain.size.width && middle < selected.size.width,
        "width changes continuously"
    );
    click(cx, "主体完整名称");
    tick(cx, 350);
    assert_eq!(bounds(cx, "probe-tag"), plain);
    assert_eq!(
        bounds(cx, (gpui_kit::ElementId::from("probe-tag"), "label")),
        plain_label
    );
    // A caller that needs stable item widths can still provide an explicit width.
    view.update(cx, |v, cx| {
        v.width = Some(px(180.));
        cx.notify();
    });
    draw(cx);
    click(cx, "主体完整名称");
    tick(cx, 35);
    assert_eq!(bounds(cx, "probe-tag").size.width, px(180.));
    tick(cx, 150);
    assert_eq!(bounds(cx, "probe-tag").size.width, px(180.));
}

#[gpui_kit::test]
fn check_stroke_erases_before_slot_collapse_and_reverses_without_a_jump(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.mode = 2;
        v.closable = false;
        cx.notify();
    });
    draw(cx);
    let unchecked = bounds(cx, "probe-tag");
    click(cx, "主体完整名称");
    tick(cx, 110);
    let checked = bounds(cx, "probe-tag");
    assert!(checked.size.width > unchecked.size.width);
    tick(cx, 210);
    assert_eq!(bounds(cx, "probe-tag"), checked);

    click(cx, "主体完整名称");
    tick(cx, 120);
    assert_eq!(
        bounds(cx, "probe-tag"),
        checked,
        "keep full drawing bounds while the stroke is erasing"
    );
    // Reverse while the stroke is still visible; do not replay slot expansion.
    click(cx, "主体完整名称");
    assert_eq!(bounds(cx, "probe-tag"), checked);
    tick(cx, 350);
    click(cx, "主体完整名称");
    tick(cx, 250);
    let collapsing = bounds(cx, "probe-tag");
    assert!(collapsing.size.width > unchecked.size.width);
    assert!(collapsing.size.width < checked.size.width);
    click(cx, "主体完整名称");
    assert_eq!(bounds(cx, "probe-tag"), collapsing);
    tick(cx, 20);
    assert!(bounds(cx, "probe-tag").size.width > collapsing.size.width);

    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    assert_eq!(bounds(cx, "probe-tag"), checked);
    click(cx, "主体完整名称");
    assert_eq!(bounds(cx, "probe-tag"), unchecked);
}

#[gpui_kit::test]
fn structural_minimum_wins_over_tiny_width_and_maximum(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.leading = true;
        v.width = Some(px(1.));
        v.max_width = Some(px(1.));
        cx.notify();
    });
    draw(cx);
    let root = bounds(cx, "probe-tag");
    let close = node(cx, "移除 标签内容").bounds();
    let label = bounds(cx, (gpui_kit::ElementId::from("probe-tag"), "label"));
    assert!(root.size.width > px(50.));
    assert!(label.size.width > px(0.));
    assert!(label.right() <= close.left());
    assert!(close.right() <= root.right());
    view.update(cx, |v, cx| {
        v.width = None;
        v.max_width = None;
        v.min_width = Some(px(250.));
        cx.notify();
    });
    draw(cx);
    assert!(bounds(cx, "probe-tag").size.width >= px(250.));
}

#[gpui_kit::test]
fn grouping_wraps_and_reordering_preserves_focus_by_id(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.group = true;
        cx.notify();
    });
    draw(cx);
    let first = bounds(cx, "probe-tag");
    let last = bounds(cx, "fourth");
    assert!(last.top() > first.top());
    cx.update(|w, cx| view.read(cx).body.clone().focus(w, cx));
    view.update(cx, |v, cx| {
        v.reversed = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "主体完整名称").focused(), Some(true));
    view.update(cx, |v, cx| {
        v.wrap = false;
        cx.notify();
    });
    draw(cx);
    assert_eq!(bounds(cx, "probe-tag").top(), bounds(cx, "fourth").top());
    assert_eq!(
        cx.update(|w, _| snapshots(w)
            .iter()
            .find(|n| n.path().last() == Some(&"probe-group".into()))
            .unwrap()
            .role()),
        None
    );
}

#[gpui_kit::test]
fn hover_feedback_is_local_reverses_and_reduced_motion_settles(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    assert_eq!(wash_alpha(cx, "主体完整名称"), 0.);
    hover(cx, Some("主体完整名称"));
    tick(cx, 35);
    let mid = wash_alpha(cx, "主体完整名称");
    assert!(mid > 0. && mid < 0.08, "intermediate alpha {mid}");
    assert_eq!(wash_alpha(cx, "移除 标签内容"), 0.);
    hover(cx, None);
    let reversed = wash_alpha(cx, "主体完整名称");
    assert!((mid - reversed).abs() < 0.01);
    tick(cx, 150);
    assert_eq!(wash_alpha(cx, "主体完整名称"), 0.);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    hover(cx, Some("移除 标签内容"));
    assert!(wash_alpha(cx, "移除 标签内容") > 0.);
    assert_eq!(wash_alpha(cx, "主体完整名称"), 0.);
}

#[gpui_kit::test]
fn overflow_help_only_appears_when_text_is_truncated_and_never_on_close(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    hover(cx, Some("主体完整名称"));
    tick(cx, 800);
    tick(cx, 300);
    assert!(cx.update(|w, _| snapshots(w).iter().all(|n| n.role() != Some(Role::Tooltip))));
    view.update(cx, |v, cx| {
        v.label = "这是一段需要实际排版后才能判断是否省略的完整标签文字".into();
        v.width = Some(px(160.));
        cx.notify();
    });
    draw(cx);
    // Hover the actual label slot, since close/body padding are not text help.
    let p = bounds(cx, (gpui_kit::ElementId::from("probe-tag"), "label")).center();
    cx.simulate_event(MouseMoveEvent {
        position: p,
        pressed_button: None,
        modifiers: Modifiers::default(),
    });
    draw(cx);
    tick(cx, 800);
    tick(cx, 300);
    assert!(cx.update(|w, _| snapshots(w).iter().any(|n| n.role() == Some(Role::Tooltip))));
    hover(
        cx,
        Some("移除 这是一段需要实际排版后才能判断是否省略的完整标签文字"),
    );
    tick(cx, 800);
    tick(cx, 300);
    assert!(cx.update(|w, _| snapshots(w).iter().all(|n| n.role() != Some(Role::Tooltip))));
}

#[gpui_kit::test]
fn custom_palette_and_theme_inheritance_are_supported(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let geometry = bounds(cx, "probe-tag");
    let light = surface_color(cx);
    let dark = YueTheme::new(ThemeMode::Dark, Accent::Neutral);
    cx.update(|_, cx| yue_component::set_theme(cx, dark));
    draw(cx);
    let expected = dark
        .token("--tag-primary-tint-background")
        .unwrap()
        .rgba()
        .unwrap();
    near_color(surface_color(cx), expected);
    assert_ne!(surface_color(cx), light);
    view.update(cx, |v, cx| {
        v.theme = Some(YueTheme::new(ThemeMode::Light, Accent::Azure));
        cx.notify();
    });
    draw(cx);
    near_color(surface_color(cx), light);
    assert_eq!(bounds(cx, "probe-tag").size, geometry.size);
    let colors = TagColors::new(
        gpui_kit::rgb(dark.accent_subtle()),
        gpui_kit::rgb(dark.accent_subtle_text()),
        gpui_kit::rgb(dark.border_default()),
    );
    view.update(cx, |v, cx| {
        v.colors = Some(colors);
        cx.notify();
    });
    draw(cx);
    tick(cx, 150);
    near_color(surface_color(cx), colors.background());
    view.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    let mut disabled = colors.background();
    disabled.a *= dark
        .token("--tag-opacity-disabled")
        .unwrap()
        .number()
        .unwrap();
    near_color(surface_color(cx), disabled);
}

#[gpui_kit::test]
fn drawn_corners_follow_each_shape_and_keyboard_focus_does_not_reflow(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for shape in [TagShape::Rounded, TagShape::Pill, TagShape::Mark] {
        view.update(cx, |v, cx| {
            v.shape = shape;
            cx.notify();
        });
        draw(cx);
        let b = bounds(cx, "probe-tag");
        cx.update(|w, _| {
            let b = b.scale(w.scale_factor());
            let root = w
                .painted_quads()
                .into_iter()
                .find(|q| q.bounds == b && q.background.as_solid().is_some_and(|c| c.a > 0.))
                .unwrap();
            assert!(root.corner_radii.top_left.0 > 0.);
            assert_eq!(root.corner_radii.top_right.0 == 0., shape == TagShape::Mark);
        });
        cx.update(|w, cx| view.read(cx).body.clone().focus(w, cx));
        key(cx, "tab");
        assert_eq!(bounds(cx, "probe-tag"), b);
        let close = node(cx, "移除 标签内容").bounds();
        cx.update(|w, _| {
            let close = close.scale(w.scale_factor());
            assert!(
                w.painted_quads()
                    .iter()
                    .any(|q| q.bounds.left() < close.left()
                        && q.bounds.right() > close.right()
                        && q.bounds.top() < close.top()
                        && q.bounds.bottom() > close.bottom()
                        && q.border_widths.top.0 > 0.),
                "outward focus ring is painted"
            );
        });
    }
}

#[gpui_kit::test]
fn toggle_keyboard_uses_base_semantics_and_disabled_clears_hover_immediately(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.mode = 2;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| view.read(cx).body.clone().focus(w, cx));
    key(cx, "enter");
    key(cx, "space");
    assert_eq!(view.read_with(cx, |v, _| v.changes.clone()), [true, false]);
    hover(cx, Some("主体完整名称"));
    tick(cx, 150);
    assert!(wash_alpha(cx, "主体完整名称") > 0.);
    view.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(wash_alpha(cx, "主体完整名称"), 0.);
    view.update(cx, |v, cx| {
        v.disabled = false;
        v.mode = 0;
        cx.notify();
    });
    draw(cx);
    assert_eq!(wash_alpha(cx, "主体完整名称"), 0.);
}
