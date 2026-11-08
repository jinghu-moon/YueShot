//! Native semantic/event/geometry coverage; no OS browser or Windows GPU assertions.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers,
    MouseMoveEvent, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestAppContext, VisualTestContext, Window, div,
    point, prelude::FluentBuilder as _, px,
};
use yue_component::{
    Accent, LinkHover, LinkSize, LinkVariant, ThemeMode, YueButton, YueLink, YueTheme, outline,
};

gpui_kit::actions!(link_test, [HostCommand]);

struct Probe {
    disabled: bool,
    custom: bool,
    href: SharedString,
    events: Vec<String>,
    parent_clicks: usize,
    commands: usize,
    focus: FocusHandle,
    provided_focus: bool,
    before: FocusHandle,
    after: FocusHandle,
    skip: bool,
    index: isize,
    variant: LinkVariant,
    size: LinkSize,
    hover: LinkHover,
    underline: bool,
    leading: bool,
    trailing: bool,
    width: Option<gpui_kit::Pixels>,
    theme: Option<YueTheme>,
}
impl Probe {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            disabled: false,
            custom: false,
            href: "https://example.com/guide?q=Yue#intro".into(),
            events: vec![],
            parent_clicks: 0,
            commands: 0,
            focus: cx.focus_handle().tab_stop(false),
            provided_focus: true,
            before: cx.focus_handle(),
            after: cx.focus_handle(),
            skip: false,
            index: 0,
            variant: LinkVariant::Primary,
            size: LinkSize::Medium,
            hover: LinkHover::Underline,
            underline: false,
            leading: false,
            trailing: false,
            width: None,
            theme: None,
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity();
        let mut link = YueLink::new("resource", "使用指南", self.href.clone())
            .disabled(self.disabled)
            .variant(self.variant)
            .size(self.size)
            .hover(self.hover)
            .underline(self.underline)
            .when(self.provided_focus, |link| link.track_focus(&self.focus))
            .tab_index(self.index)
            .tab_stop(!self.skip)
            .accessibility_label("打开指南")
            .accessibility_description("外部资源说明")
            .key_context("YueResource")
            .when(self.leading, |link| link.leading_icon(outline::Link))
            .when(self.trailing, |link| {
                link.trailing_icon(outline::ExternalLink)
            })
            .on_activate(cx.listener(|this, _, _, cx| {
                this.events.push("observe".into());
                cx.notify();
            }));
        if let Some(width) = self.width {
            link = link.width(width);
        }
        if let Some(theme) = self.theme {
            link = link.theme(theme);
        }
        if self.custom {
            link = link.open_with(move |href, _, _, cx| {
                owner.update(cx, |this, cx| {
                    this.events.push(format!("open:{href}"));
                    cx.notify();
                })
            });
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .on_action(cx.listener(|this, _: &HostCommand, _, cx| {
                this.commands += 1;
                cx.notify();
            }))
            .child(YueButton::new("before", "前一个").track_focus(&self.before))
            // A stretching column catches accidental full-width links.
            .child(
                div()
                    .id("link-parent")
                    .w(px(400.))
                    .flex()
                    .flex_col()
                    .child(link)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.parent_clicks += 1;
                        cx.notify();
                    })),
            )
            .child(YueButton::new("after", "后一个").track_focus(&self.after))
    }
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
}
fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let mut probe = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(Probe::new);
        probe = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    draw(cx);
    (probe.unwrap(), cx)
}
fn find(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|w, _| {
        let nodes: Vec<_> = snapshots(w)
            .into_iter()
            .filter(|n| n.label() == Some(label))
            .collect();
        assert_eq!(nodes.len(), 1, "unique semantic target {label}: {nodes:?}");
        nodes[0].clone()
    })
}
fn click(cx: &mut VisualTestContext) {
    let center = find(cx, "打开指南").bounds().center();
    cx.simulate_click(center, Modifiers::default());
    draw(cx);
}
fn activate(cx: &mut VisualTestContext, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke });
    draw(cx);
}
fn hover(cx: &mut VisualTestContext, inside: bool) {
    let position = if inside {
        find(cx, "打开指南").bounds().center()
    } else {
        point(px(600.), px(400.))
    };
    cx.simulate_event(MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Modifiers::default(),
    });
    draw(cx);
}
fn tick(cx: &mut VisualTestContext, ms: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(ms));
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
}
fn lines(cx: &mut VisualTestContext) -> Vec<gpui_kit::Underline> {
    let bounds = find(cx, "打开指南").bounds();
    cx.update(|w, _| {
        let bounds = bounds.scale(w.scale_factor());
        w.painted_underlines()
            .into_iter()
            .filter(|line| bounds.contains(&line.bounds.center()))
            .collect()
    })
}
fn line_alpha(cx: &mut VisualTestContext) -> f32 {
    lines(cx).iter().map(|line| line.color.a).fold(0., f32::max)
}
fn line_color(cx: &mut VisualTestContext) -> gpui_kit::Rgba {
    lines(cx).first().expect("visible underline").color.into()
}

#[gpui_kit::test]
fn pointer_uses_one_native_link_and_default_system_open(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    assert_eq!(find(cx, "打开指南").role(), Some(Role::Link));
    assert_eq!(cx.opened_url(), None);
    click(cx);
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://example.com/guide?q=Yue#intro")
    );
    assert_eq!(view.read_with(cx, |v, _| v.events.clone()), ["observe"]);
    assert_eq!(find(cx, "打开指南").focused(), Some(true));
}

#[gpui_kit::test]
fn default_focus_is_retained_and_keyboard_uses_system_open(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.provided_focus = false;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| w.focus(&view.read(cx).before.clone(), cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(find(cx, "打开指南").focused(), Some(true));
    view.update(cx, |v, cx| {
        v.leading = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(find(cx, "打开指南").focused(), Some(true));
    activate(cx, "enter");
    activate(cx, "space");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://example.com/guide?q=Yue#intro")
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.events.clone()),
        ["observe", "observe"]
    );
}

#[gpui_kit::test]
fn override_opens_before_observing_once_for_pointer_enter_and_space(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.custom = true;
        v.href = "mailto:help@example.com?subject=Yue".into();
        cx.notify();
    });
    draw(cx);
    click(cx);
    activate(cx, "enter");
    activate(cx, "space");
    let events = view.read_with(cx, |v, _| v.events.clone());
    assert_eq!(
        events,
        ["open:mailto:help@example.com?subject=Yue", "observe"].repeat(3)
    );
    assert_eq!(
        cx.opened_url(),
        None,
        "host strategy replaces system opening"
    );
}

#[gpui_kit::test]
fn focused_then_disabled_rejects_pointer_keyboard_and_parent_activation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let focus = view.read_with(cx, |v, _| v.focus.clone());
    cx.update(|w, cx| w.focus(&focus, cx));
    draw(cx);
    assert_eq!(find(cx, "打开指南").focused(), Some(true));
    view.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    click(cx);
    activate(cx, "enter");
    activate(cx, "space");
    assert_eq!(find(cx, "打开指南").disabled(), Some(true));
    assert_eq!(cx.opened_url(), None);
    assert!(view.read_with(cx, |v, _| v.events.is_empty()));
    assert_eq!(view.read_with(cx, |v, _| v.parent_clicks), 0);
    cx.update(|w, cx| w.focus(&view.read(cx).before.clone(), cx));
    draw(cx);
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(find(cx, "后一个").focused(), Some(true));
}

#[gpui_kit::test]
fn caller_focus_obeys_tab_stop_index_and_key_context(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let focus = view.read_with(cx, |v, _| v.focus.clone());
    // The caller created tab_stop(false); Link's explicit policy must apply.
    cx.update(|w, cx| w.focus(&view.read(cx).before.clone(), cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert!(cx.update(|w, _| focus.is_focused(w)));
    cx.update(|_, cx| {
        cx.bind_keys([gpui_kit::KeyBinding::new(
            "ctrl-k",
            HostCommand,
            Some("YueResource"),
        )])
    });
    cx.simulate_keystrokes("ctrl-k");
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.commands), 1);
    view.update(cx, |v, cx| {
        v.skip = true;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| w.focus(&view.read(cx).before.clone(), cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(find(cx, "后一个").focused(), Some(true));
    view.update(cx, |v, cx| {
        v.skip = false;
        v.index = -1;
        cx.notify();
    });
    draw(cx);
    cx.update(|w, cx| w.focus(&view.read(cx).before.clone(), cx));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(find(cx, "后一个").focused(), Some(true));
}

#[gpui_kit::test]
fn sizes_icons_and_width_follow_the_content_contract(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let mut widths = vec![];
    for (size, height, icon) in [
        (LinkSize::Small, 16., 16.),
        (LinkSize::Medium, 20., 16.),
        (LinkSize::Large, 24., 20.),
    ] {
        view.update(cx, |v, cx| {
            v.size = size;
            v.leading = false;
            v.trailing = false;
            cx.notify();
        });
        draw(cx);
        let bare = find(cx, "打开指南").bounds();
        assert_eq!(bare.size.height, px(height));
        assert!(bare.size.width < px(400.));
        widths.push(bare.size.width);
        view.update(cx, |v, cx| {
            v.leading = true;
            v.trailing = true;
            v.underline = true;
            cx.notify();
        });
        draw(cx);
        let decorated = find(cx, "打开指南").bounds();
        assert_eq!(decorated.size.height, bare.size.height);
        assert_eq!(decorated.size.width, bare.size.width + px((icon + 4.) * 2.));
        cx.update(|w, _| {
            assert_eq!(
                snapshots(w)
                    .iter()
                    .filter(|n| n.role() == Some(Role::Link))
                    .count(),
                1
            )
        });
        let scale = cx.update(|w, _| w.scale_factor());
        for line in lines(cx) {
            assert!(line.bounds.left() >= (bare.left() + px(icon + 4.)).scale(scale));
            assert!(
                line.bounds.right() <= (decorated.right() - px(icon + 4.)).scale(scale),
                "{size:?}: line={:?}, link={decorated:?}, bare={bare:?}",
                line.bounds
            );
        }
    }
    assert!(widths.windows(2).all(|p| p[0] < p[1]));
    view.update(cx, |v, cx| {
        v.width = Some(px(180.));
        cx.notify();
    });
    draw(cx);
    assert_eq!(find(cx, "打开指南").bounds().size.width, px(180.));
    click(cx);
    assert_eq!(view.read_with(cx, |v, _| v.events.len()), 1);
}

#[gpui_kit::test]
fn underline_hover_has_intermediate_alpha_reverses_and_reduced_motion_snaps(
    cx: &mut TestAppContext,
) {
    let (_, cx) = open(cx);
    let bounds = find(cx, "打开指南").bounds();
    assert_eq!(line_alpha(cx), 0.);
    hover(cx, true);
    tick(cx, 35);
    let middle = line_alpha(cx);
    assert!(middle > 0. && middle < 1., "{middle}");
    hover(cx, false);
    let reversed = line_alpha(cx);
    assert!((middle - reversed).abs() < 0.01);
    tick(cx, 200);
    assert_eq!(line_alpha(cx), 0.);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    hover(cx, true);
    assert_eq!(line_alpha(cx), 1.);
    hover(cx, false);
    assert_eq!(line_alpha(cx), 0.);
    assert_eq!(find(cx, "打开指南").bounds(), bounds);
}

#[gpui_kit::test]
fn color_hover_interpolates_and_all_hover_modes_keep_explicit_underline(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.hover = LinkHover::Color;
        v.underline = true;
        cx.notify();
    });
    draw(cx);
    let rest = line_color(cx);
    hover(cx, true);
    tick(cx, 35);
    let middle = line_color(cx);
    tick(cx, 200);
    let end = line_color(cx);
    assert_ne!(rest, middle);
    assert_ne!(middle, end);
    assert_eq!(line_alpha(cx), 1.);
    hover(cx, false);
    tick(cx, 200);
    assert_eq!(line_color(cx), rest);
    for mode in [LinkHover::None, LinkHover::Color, LinkHover::Underline] {
        view.update(cx, |v, cx| {
            v.hover = mode;
            cx.notify();
        });
        draw(cx);
        hover(cx, true);
        tick(cx, 200);
        hover(cx, false);
        tick(cx, 200);
        assert_eq!(line_alpha(cx), 1., "explicit underline survives {mode:?}");
    }
}

#[gpui_kit::test]
fn themes_are_inherited_while_explicit_overrides_and_disabled_colors_are_stable(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.underline = true;
        cx.notify();
    });
    draw(cx);
    let light = line_color(cx);
    let dark = YueTheme::new(ThemeMode::Dark, Accent::Azure);
    cx.update(|_, cx| yue_component::set_theme(cx, dark));
    draw(cx);
    assert_ne!(line_color(cx), light);
    view.update(cx, |v, cx| {
        v.theme = Some(YueTheme::new(ThemeMode::Light, Accent::Azure));
        cx.notify();
    });
    draw(cx);
    assert_eq!(line_color(cx), light);
    for variant in [
        LinkVariant::Default,
        LinkVariant::Primary,
        LinkVariant::Danger,
        LinkVariant::Warning,
        LinkVariant::Success,
    ] {
        view.update(cx, |v, cx| {
            v.variant = variant;
            v.disabled = false;
            cx.notify();
        });
        draw(cx);
        let enabled = line_color(cx);
        view.update(cx, |v, cx| {
            v.disabled = true;
            cx.notify();
        });
        draw(cx);
        let disabled = line_color(cx);
        assert_eq!(
            (disabled.r, disabled.g, disabled.b),
            (enabled.r, enabled.g, enabled.b)
        );
        assert!(disabled.a > 0. && disabled.a < enabled.a);
        hover(cx, true);
        tick(cx, 200);
        assert_eq!(line_color(cx), disabled);
    }
}

#[gpui_kit::test]
fn keyboard_focus_ring_is_outside_and_does_not_reflow(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    let before = find(cx, "打开指南").bounds();
    activate(cx, "tab");
    for _ in 0..3 {
        if find(cx, "打开指南").focused() == Some(true) {
            break;
        }
        cx.update(|w, cx| w.focus_next(cx));
        draw(cx);
    }
    assert_eq!(find(cx, "打开指南").focused(), Some(true));
    assert_eq!(find(cx, "打开指南").bounds(), before);
    cx.update(|w, _| {
        let expected = gpui_kit::Bounds::new(
            before.origin - point(px(4.), px(4.)),
            gpui_kit::size(before.size.width + px(8.), before.size.height + px(8.)),
        );
        assert!(
            w.painted_quads()
                .iter()
                .any(|q| q.bounds == expected.scale(w.scale_factor())
                    && q.border_widths.top == px(2.).scale(w.scale_factor()))
        );
    });
}
