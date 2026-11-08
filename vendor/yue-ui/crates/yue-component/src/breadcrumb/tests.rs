use super::*;
use crate::{Accent, ThemeMode, YueButton};
use gpui_kit::base::{
    Root,
    test_support::{ElementSnapshot, snapshots},
};
use gpui_kit::{AppContext as _, Entity};
use gpui_kit::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point, px, size};

struct Probe {
    count: usize,
    collapsed: bool,
    readonly: bool,
    disabled: Option<usize>,
    disabled_all: bool,
    current_clickable: bool,
    long: bool,
    width: f32,
    events: Vec<BreadcrumbNavigate>,
    before: FocusHandle,
    after: FocusHandle,
    move_focus: bool,
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
        let mut path = YueBreadcrumb::new("path", theme)
            .width(px(self.width))
            .current_clickable(self.current_clickable)
            .items((0..self.count).map(|i| {
                YueBreadcrumbItem::new(
                    format!("node-{i}"),
                    if self.long {
                        format!(
                            "Level {} — 这是一段很长的面包屑路径用于测试文字截断与完整提示",
                            i + 1
                        )
                    } else {
                        format!("Level {}", i + 1)
                    },
                )
                .disabled(self.disabled_all || self.disabled == Some(i))
            }));
        if self.collapsed {
            path = path.collapse(BreadcrumbCollapse::new(5).before(2).after(2));
        }
        if !self.readonly {
            path = path.on_navigate(cx.listener(|this, request: &BreadcrumbNavigate, w, cx| {
                this.events.push(request.clone());
                if this.move_focus {
                    w.focus(&this.after, cx);
                }
                cx.notify();
            }));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .p_4()
            .gap_4()
            .child(YueButton::new("before-path", "Before").track_focus(&self.before))
            .child(path)
            .child(YueButton::new("after-path", "After").track_focus(&self.after))
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
        crate::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap();
        cx.set_reduce_motion(true);
    });
    let mut probe = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        let view = cx.new(|cx| Probe {
            count: 4,
            collapsed: false,
            readonly: false,
            disabled: None,
            disabled_all: false,
            current_clickable: false,
            long: false,
            width: 650.,
            events: vec![],
            before: cx.focus_handle(),
            after: cx.focus_handle(),
            move_focus: false,
        });
        probe = Some(view.clone());
        Root::new(view, w, cx)
    });
    cx.simulate_resize(size(px(1000.), px(800.)));
    cx.update(|w, _| w.activate_window());
    draw(cx);
    (probe.unwrap(), cx)
}
fn node(cx: &mut VisualTestContext, label: &str, role: Role) -> ElementSnapshot {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.label() == Some(label) && n.role() == Some(role) && n.visible())
            .unwrap_or_else(|| panic!("missing {role:?}: {label}; {:?}", snapshots(w)))
    })
}
fn click(cx: &mut VisualTestContext, label: &str, role: Role) {
    let bounds = node(cx, label, role).bounds();
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}
fn key(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
}
fn edit(view: &Entity<Probe>, cx: &mut VisualTestContext, edit: impl FnOnce(&mut Probe)) {
    view.update(cx, |p, cx| {
        edit(p);
        cx.notify();
    });
    draw(cx);
}
fn menus(cx: &mut VisualTestContext) -> usize {
    cx.update(|w, _| {
        snapshots(w)
            .iter()
            .filter(|n| n.role() == Some(Role::Menu) && n.visible())
            .count()
    })
}

#[gpui_kit::test]
fn ancestors_navigate_once_current_is_static_and_host_owns_path(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    assert_eq!(
        node(cx, "Breadcrumb", Role::Navigation).role(),
        Some(Role::Navigation)
    );
    click(cx, "Level 2", Role::Button);
    key(cx, "enter");
    key(cx, "space");
    assert_eq!(
        view.read_with(cx, |p, _| p.events.clone()),
        vec![
            BreadcrumbNavigate {
                value: "node-1".into(),
                source: BreadcrumbNavigateSource::Item
            };
            3
        ]
    );
    click(cx, "Level 4", Role::Caption);
    assert_eq!(view.read_with(cx, |p, _| p.events.len()), 3);
    edit(&view, cx, |p| p.current_clickable = true);
    click(cx, "Level 4", Role::Button);
    assert_eq!(
        view.read_with(cx, |p, _| p.events.last().unwrap().value.clone()),
        "node-3"
    );
    assert_eq!(view.read_with(cx, |p, _| p.count), 4);
}

#[gpui_kit::test]
fn tab_skips_disabled_and_current_and_readonly_has_no_fake_commands(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| p.disabled = Some(1));
    click(cx, "Before", Role::Button);
    key(cx, "tab");
    assert_eq!(node(cx, "Level 1", Role::Button).focused(), Some(true));
    key(cx, "tab");
    assert_eq!(node(cx, "Level 3", Role::Button).focused(), Some(true));
    key(cx, "tab");
    assert_eq!(node(cx, "After", Role::Button).focused(), Some(true));
    click(cx, "Level 2", Role::Button);
    assert!(view.read_with(cx, |p, _| p.events.is_empty()));
    edit(&view, cx, |p| {
        p.readonly = true;
        p.disabled = None;
    });
    node(cx, "Level 1", Role::Caption);
    click(cx, "Before", Role::Button);
    key(cx, "tab");
    assert_eq!(node(cx, "After", Role::Button).focused(), Some(true));
}

#[gpui_kit::test]
fn overflow_uses_real_dropdown_keyboard_and_preserves_host_focus(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| {
        p.count = 8;
        p.collapsed = true;
        p.disabled = Some(3);
        p.move_focus = true;
    });
    click(cx, "Show hidden levels", Role::Button);
    assert_eq!(menus(cx), 1);
    node(cx, "Level 3", Role::MenuItem);
    assert_eq!(node(cx, "Level 4", Role::MenuItem).disabled(), Some(true));
    key(cx, "down");
    key(cx, "enter");
    assert_eq!(
        view.read_with(cx, |p, _| p.events.clone()),
        vec![BreadcrumbNavigate {
            value: "node-4".into(),
            source: BreadcrumbNavigateSource::OverflowMenu
        }]
    );
    assert_eq!(menus(cx), 0);
    assert_eq!(node(cx, "After", Role::Button).focused(), Some(true));
}

#[gpui_kit::test]
fn escape_outside_click_and_dynamic_removal_close_without_navigation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| {
        p.count = 8;
        p.collapsed = true;
    });
    click(cx, "Show hidden levels", Role::Button);
    key(cx, "escape");
    assert_eq!(menus(cx), 0);
    assert_eq!(
        node(cx, "Show hidden levels", Role::Button).focused(),
        Some(true)
    );
    click(cx, "Show hidden levels", Role::Button);
    cx.simulate_click(point(px(900.), px(700.)), Modifiers::default());
    draw(cx);
    assert_eq!(menus(cx), 0);
    click(cx, "Show hidden levels", Role::Button);
    edit(&view, cx, |p| p.count = 3);
    assert_eq!(menus(cx), 0);
    assert_eq!(node(cx, "Level 2", Role::Button).focused(), Some(true));
    assert!(view.read_with(cx, |p, _| p.events.is_empty()));
    edit(&view, cx, |p| p.count = 0);
    cx.update(|w, _| {
        assert!(
            !snapshots(w)
                .iter()
                .any(|n| n.role() == Some(Role::Navigation))
        )
    });
}

#[gpui_kit::test]
fn readonly_overflow_exposes_static_names_and_escape(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| {
        p.count = 8;
        p.collapsed = true;
        p.readonly = true;
    });
    click(cx, "Show hidden levels", Role::Button);
    node(cx, "Level 3", Role::Caption);
    node(cx, "Level 6", Role::Caption);
    cx.update(|w, _| {
        assert!(
            !snapshots(w)
                .iter()
                .any(|n| n.role() == Some(Role::MenuItem))
        )
    });
    key(cx, "escape");
    assert_eq!(menus(cx), 0);
}

#[gpui_kit::test]
fn fully_disabled_hidden_path_still_opens_and_escapes_without_navigation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| {
        p.count = 8;
        p.collapsed = true;
        p.disabled_all = true;
    });
    click(cx, "Show hidden levels", Role::Button);
    assert_eq!(menus(cx), 1);
    for i in 3..=6 {
        assert_eq!(
            node(cx, &format!("Level {i}"), Role::MenuItem).disabled(),
            Some(true)
        );
    }
    key(cx, "down");
    key(cx, "enter");
    assert_eq!(menus(cx), 1);
    assert!(view.read_with(cx, |p, _| p.events.is_empty()));
    key(cx, "escape");
    assert_eq!(menus(cx), 0);
    assert_eq!(
        node(cx, "Show hidden levels", Role::Button).focused(),
        Some(true)
    );
}

#[gpui_kit::test]
fn long_text_is_measured_and_current_remains_reachable_in_narrow_layout(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| {
        p.long = true;
        p.width = 280.;
    });
    for rem in [16., 24.] {
        cx.update(|w, _| w.set_rem_size(px(rem)));
        draw(cx);
        let current_label = view.read_with(cx, |p, _| {
            format!(
                "Level {} — 这是一段很长的面包屑路径用于测试文字截断与完整提示",
                p.count
            )
        });
        let current = node(cx, &current_label, Role::Caption).bounds();
        assert!(
            current.size.width <= px(192. * rem / 16.) + px(1.),
            "{current:?}"
        );
        assert!(current.size.height >= px(24. * rem / 16.));
        let viewport = cx.update(|w, _| {
            snapshots(w)
                .into_iter()
                .find(|n| n.path().last() == Some(&ElementId::from("breadcrumb-list")))
                .unwrap()
                .bounds()
        });
        assert!(
            current.right() <= viewport.right() + px(1.),
            "current {current:?} in {viewport:?}"
        );
        assert!(
            current.left() >= viewport.left() - px(1.),
            "current {current:?} in {viewport:?}"
        );
    }
}

#[gpui_kit::test]
fn overflow_help_uses_measured_text_without_an_extra_tab_stop(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| {
        p.long = true;
        p.count = 1;
        p.width = 240.;
    });
    let label = "Level 1 — 这是一段很长的面包屑路径用于测试文字截断与完整提示";
    let bounds = node(cx, label, Role::Caption).bounds();
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::default());
    draw(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    draw(cx);
    node(cx, label, Role::Tooltip);
    cx.simulate_mouse_move(point(px(900.), px(700.)), None, Modifiers::default());
    draw(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    draw(cx);
    edit(&view, cx, |p| p.long = false);
    let bounds = node(cx, "Level 1", Role::Caption).bounds();
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::default());
    draw(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    draw(cx);
    cx.update(|w, _| {
        assert!(
            !snapshots(w)
                .iter()
                .any(|n| n.role() == Some(Role::Tooltip) && n.visible())
        )
    });
    click(cx, "Before", Role::Button);
    key(cx, "tab");
    assert_eq!(node(cx, "After", Role::Button).focused(), Some(true));
}

#[gpui_kit::test]
fn dynamic_disabled_rows_and_focused_path_removal_do_not_leave_stale_actions(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    edit(&view, cx, |p| {
        p.count = 8;
        p.collapsed = true;
    });
    click(cx, "Show hidden levels", Role::Button);
    edit(&view, cx, |p| p.disabled = Some(2));
    click(cx, "Level 3", Role::MenuItem);
    assert!(view.read_with(cx, |p, _| p.events.is_empty()));
    key(cx, "end");
    key(cx, "enter");
    assert_eq!(
        view.read_with(cx, |p, _| p.events.last().unwrap().value.clone()),
        "node-5"
    );
    edit(&view, cx, |p| {
        p.count = 4;
        p.collapsed = false;
        p.disabled = None;
    });
    click(cx, "Level 3", Role::Button);
    edit(&view, cx, |p| p.count = 2);
    assert_eq!(node(cx, "Level 1", Role::Button).focused(), Some(true));
    edit(&view, cx, |p| p.count = 0);
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
    draw(cx);
    let before = node(cx, "Before", Role::Button).focused() == Some(true);
    let after = node(cx, "After", Role::Button).focused() == Some(true);
    assert!(before || after, "focus moves back into the host");
}
