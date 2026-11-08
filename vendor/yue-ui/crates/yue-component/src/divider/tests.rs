use super::*;
use crate::{Accent, NeutralTone, ThemeMode};

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}

#[test]
fn defaults_and_empty_labels_are_plain_horizontal_rules() {
    let divider = YueDivider::new("rule", theme());
    assert_eq!(divider.orientation, DividerLayout::Horizontal);
    assert_eq!(divider.align, DividerAlign::Center);
    assert_eq!(divider.line_style, DividerLineStyle::Solid);
    assert!(divider.label.is_none());
    assert!(divider.height.is_none());
    for label in ["", "  \n\t", "　"] {
        assert!(
            YueDivider::new("rule", theme())
                .label(label)
                .label
                .is_none()
        );
    }
    let divider = YueDivider::new("rule", theme())
        .label("详细信息")
        .align(DividerAlign::End)
        .dashed(true);
    assert_eq!(divider.label.as_deref(), Some("详细信息"));
    assert_eq!(divider.align, DividerAlign::End);
    assert_eq!(divider.line_style, DividerLineStyle::Dashed);
}

#[test]
fn line_style_supports_dots_and_preserves_dashed_shorthand() {
    let rule = || YueDivider::new("rule", theme());
    assert_eq!(
        rule().line_style(DividerLineStyle::Dotted).line_style,
        DividerLineStyle::Dotted
    );
    assert_eq!(
        rule()
            .dashed(true)
            .line_style(DividerLineStyle::Dotted)
            .line_style,
        DividerLineStyle::Dotted
    );
    assert_eq!(
        rule()
            .line_style(DividerLineStyle::Dotted)
            .dashed(true)
            .line_style,
        DividerLineStyle::Dashed
    );
    assert_eq!(
        rule()
            .line_style(DividerLineStyle::Dotted)
            .dashed(false)
            .line_style,
        DividerLineStyle::Solid
    );
}

#[test]
fn divider_tokens_resolve_in_every_theme_profile() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for tone in NeutralTone::ALL {
                let theme = YueTheme::new(mode, accent).with_neutral(tone);
                assert!(pixels(theme, keys::DIVIDER_BORDER_WIDTH) > 0.);
                assert!(pixels(theme, keys::DIVIDER_MARGIN_BLOCK) > 0.);
                assert!(rgba(theme, keys::DIVIDER_BORDER_COLOR).a > 0.);
                assert!(rgba(theme, keys::TEXT_PRIMARY).a > 0.);
            }
        }
    }
}

#[test]
fn divider_colors_are_defined_and_resolve_to_opaque_palette_values() {
    for (tone, light, dark) in [
        (NeutralTone::Neutral, 0xc7c7c7ff, 0x525252ff),
        (NeutralTone::Slate, 0xbdc8d7ff, 0x49535fff),
        (NeutralTone::Stone, 0xcec6bcff, 0x585149ff),
        (NeutralTone::Haze, 0xc8c5d6ff, 0x53505fff),
    ] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for (mode, expected) in [(ThemeMode::Light, light), (ThemeMode::Dark, dark)] {
                let theme = YueTheme::new(mode, accent).with_neutral(tone);
                let color = rgba(theme, keys::DIVIDER_BORDER_COLOR);
                assert_eq!(
                    color,
                    gpui_kit::rgba(expected),
                    "{mode:?} {accent:?} {tone:?}"
                );
                assert_eq!(
                    color,
                    rgba(theme, "--divider-border-color"),
                    "static key and CSS name must resolve to the same color"
                );
            }
        }
    }
}

#[cfg(feature = "headless-tests")]
mod layout {
    use super::*;
    use crate::YueButton;
    use gpui_kit::base::{
        Root,
        test_support::{ElementSnapshot, snapshots},
    };
    use gpui_kit::{
        AppContext as _, Bounds, Context, Entity, FocusHandle, Pixels, Render, TestAppContext,
        VisualTestContext,
    };

    struct Probe {
        theme: YueTheme,
        align: DividerAlign,
        vertical: bool,
        line_style: DividerLineStyle,
        label: &'static str,
        width: f32,
        height: Option<f32>,
        before: FocusHandle,
        after: FocusHandle,
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let mut rule = YueDivider::new("probe-divider", self.theme)
                .align(self.align)
                .label(self.label)
                .line_style(self.line_style);
            if self.vertical {
                rule = rule.layout(DividerLayout::Vertical);
            }
            if let Some(height) = self.height {
                rule = rule.height(px(height));
            }
            div()
                .size_full()
                .flex()
                .flex_col()
                .p_4()
                .child(YueButton::new("before-divider", "Before").track_focus(&self.before))
                .child(
                    div()
                        .id("probe-container")
                        .test_support()
                        .w(px(self.width))
                        .flex()
                        .flex_col()
                        .child(rule),
                )
                .child(YueButton::new("after-divider", "After").track_focus(&self.after))
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
            crate::init(cx, theme()).unwrap();
            cx.set_reduce_motion(true);
        });
        let mut probe = None;
        let (_, cx) = cx.add_window_view(|w, cx| {
            let view = cx.new(|cx| Probe {
                theme: theme(),
                align: DividerAlign::Center,
                vertical: false,
                line_style: DividerLineStyle::Solid,
                label: "",
                width: 400.,
                height: None,
                before: cx.focus_handle(),
                after: cx.focus_handle(),
            });
            probe = Some(view.clone());
            Root::new(view, w, cx)
        });
        draw(cx);
        (probe.unwrap(), cx)
    }
    fn node(cx: &mut VisualTestContext, id: &'static str) -> ElementSnapshot {
        cx.update(|w, _| {
            snapshots(w)
                .into_iter()
                .find(|n| n.path().last() == Some(&ElementId::from(id)))
                .unwrap_or_else(|| panic!("missing divider part {id}"))
        })
    }
    fn bounds(cx: &mut VisualTestContext, id: &'static str) -> Bounds<Pixels> {
        node(cx, id).bounds()
    }
    fn near(a: Pixels, b: Pixels) {
        assert!((a - b).abs() <= px(1.), "{a:?} != {b:?}");
    }

    #[gpui_kit::test]
    fn strokes_paint_filled_pixels_instead_of_degenerate_single_borders(cx: &mut TestAppContext) {
        let (view, cx) = open(cx);
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = YueTheme::new(mode, Accent::Azure);
            for rem in [16., 24.] {
                cx.update(|w, _| w.set_rem_size(px(rem)));
                for (vertical, label) in [(false, ""), (false, "Details"), (true, "")] {
                    for style in [
                        DividerLineStyle::Solid,
                        DividerLineStyle::Dashed,
                        DividerLineStyle::Dotted,
                    ] {
                        view.update(cx, |v, cx| {
                            v.theme = theme;
                            v.vertical = vertical;
                            v.label = label;
                            v.line_style = style;
                            cx.notify();
                        });
                        draw(cx);
                        let ids: &[&str] = if label.is_empty() {
                            &["probe-divider"]
                        } else {
                            &["divider-leading", "divider-trailing"]
                        };
                        for id in ids {
                            let bounds = bounds(cx, id);
                            cx.update(|w, _| {
                                let bounds = bounds.scale(w.scale_factor());
                                let color = rgba(theme, keys::DIVIDER_BORDER_COLOR).into();
                                let mut strokes: Vec<_> = w.painted_quads().into_iter()
                                    .filter(|q| q.background.as_solid() == Some(color)
                                        && q.bounds.left() >= bounds.left()
                                        && q.bounds.right() <= bounds.right()
                                        && q.bounds.top() >= bounds.top()
                                        && q.bounds.bottom() <= bounds.bottom())
                                    .collect();
                                assert!(!strokes.is_empty(), "{id}: missing filled stroke ({mode:?}, rem={rem}, style={style:?})");
                                for stroke in &strokes {
                                    assert_eq!(stroke.border_widths.top.0, 0.);
                                    assert_eq!(stroke.border_widths.left.0, 0.);
                                    assert_eq!(stroke.corner_radii.top_left.0 > 0., style == DividerLineStyle::Dotted);
                                    if vertical {
                                        assert_eq!(stroke.bounds.size.width, bounds.size.width);
                                    } else {
                                        assert_eq!(stroke.bounds.size.height, bounds.size.height);
                                    }
                                }
                                if style != DividerLineStyle::Solid {
                                    strokes.sort_by(|a, b| {
                                        let axis = |q: &gpui_kit::Quad| if vertical { q.bounds.top().0 } else { q.bounds.left().0 };
                                        axis(a).total_cmp(&axis(b))
                                    });
                                    assert!(strokes.len() > 1, "dashes must be separate filled runs");
                                    assert!(strokes.windows(2).all(|pair| if vertical {
                                        pair[0].bounds.bottom() < pair[1].bounds.top()
                                    } else {
                                        pair[0].bounds.right() < pair[1].bounds.left()
                                    }), "dashes must have visible gaps");
                                    let unit = rem / 16. * w.scale_factor();
                                    let expected_run = if style == DividerLineStyle::Dashed { 4. } else { 1. } * unit;
                                    for pair in strokes.windows(2) {
                                        let (run, gap) = if vertical {
                                            (pair[0].bounds.size.height.0, (pair[1].bounds.top() - pair[0].bounds.bottom()).0)
                                        } else {
                                            (pair[0].bounds.size.width.0, (pair[1].bounds.left() - pair[0].bounds.right()).0)
                                        };
                                        assert!((run - expected_run).abs() <= 1., "wrong run length: {run} / {expected_run}");
                                        assert!((gap - 3. * unit).abs() <= 1., "wrong gap: {gap}");
                                    }
                                } else {
                                    assert_eq!(strokes.len(), 1);
                                    assert_eq!(strokes[0].bounds, bounds);
                                }
                            });
                        }
                    }
                }
            }
        }
    }

    #[gpui_kit::test]
    fn plain_rule_fills_parent_and_scales_with_rem(cx: &mut TestAppContext) {
        let (view, cx) = open(cx);
        for rem in [16., 24.] {
            cx.update(|w, _| w.set_rem_size(px(rem)));
            draw(cx);
            let rule = bounds(cx, "probe-divider");
            let parent = bounds(cx, "probe-container");
            near(rule.size.width, px(400.));
            near(rule.size.height, px(rem / 16.));
            near(rule.top() - parent.top(), px(12. * rem / 16.));
            near(parent.bottom() - rule.bottom(), px(12. * rem / 16.));
            assert_eq!(node(cx, "probe-divider").role(), None);
        }
        view.update(cx, |v, cx| {
            v.line_style = DividerLineStyle::Dashed;
            cx.notify();
        });
        draw(cx);
        near(bounds(cx, "probe-divider").size.width, px(400.));
    }

    #[gpui_kit::test]
    fn labels_use_three_alignments_and_keep_lines_centered(cx: &mut TestAppContext) {
        let (view, cx) = open(cx);
        for rem in [16., 24.] {
            cx.update(|w, _| w.set_rem_size(px(rem)));
            for align in [DividerAlign::Start, DividerAlign::Center, DividerAlign::End] {
                view.update(cx, |v, cx| {
                    v.align = align;
                    v.label = "Details";
                    cx.notify();
                });
                draw(cx);
                let rule = node(cx, "probe-divider");
                let leading = bounds(cx, "divider-leading");
                let label = bounds(cx, "divider-label");
                let trailing = bounds(cx, "divider-trailing");
                assert_eq!(rule.role(), Some(Role::Caption));
                assert_eq!(rule.label(), Some("Details"));
                near(leading.center().y, label.center().y);
                near(trailing.center().y, label.center().y);
                near(label.left() - leading.right(), px(rem));
                near(trailing.left() - label.right(), px(rem));
                match align {
                    DividerAlign::Start => near(leading.size.width, px(24. * rem / 16.)),
                    DividerAlign::Center => near(leading.size.width, trailing.size.width),
                    DividerAlign::End => near(trailing.size.width, px(24. * rem / 16.)),
                }
            }
        }
    }

    #[gpui_kit::test]
    fn long_labels_wrap_inside_narrow_rules(cx: &mut TestAppContext) {
        let (view, cx) = open(cx);
        for align in [DividerAlign::Start, DividerAlign::Center, DividerAlign::End] {
            view.update(cx, |v, cx| {
                v.width = 160.;
                v.align = align;
                v.label = "这是一段用于验证窄容器换行的较长分隔线文字";
                cx.notify();
            });
            draw(cx);
            let root = bounds(cx, "probe-divider");
            let label = bounds(cx, "divider-label");
            assert!(label.size.height > px(22.), "long label wraps: {label:?}");
            for id in ["divider-leading", "divider-label", "divider-trailing"] {
                let part = bounds(cx, id);
                assert!(
                    part.left() >= root.left() - px(1.),
                    "{id}: {part:?} inside {root:?}"
                );
                assert!(
                    part.right() <= root.right() + px(1.),
                    "{id}: {part:?} inside {root:?}"
                );
            }
        }
    }

    #[gpui_kit::test]
    fn vertical_rule_has_explicit_or_body_height_and_inline_margins(cx: &mut TestAppContext) {
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.vertical = true;
            cx.notify();
        });
        for rem in [16., 24.] {
            cx.update(|w, _| w.set_rem_size(px(rem)));
            for height in [None, Some(64.)] {
                view.update(cx, |v, cx| {
                    v.height = height;
                    cx.notify();
                });
                draw(cx);
                let rule = bounds(cx, "probe-divider");
                let parent = bounds(cx, "probe-container");
                near(rule.size.width, px(rem / 16.));
                near(
                    rule.size.height,
                    px(height.unwrap_or_else(|| pixels(theme(), keys::LINE_HEIGHT_MD) * rem / 16.)),
                );
                near(rule.left() - parent.left(), px(12. * rem / 16.));
                assert_eq!(node(cx, "probe-divider").role(), None);
            }
        }
    }

    #[gpui_kit::test]
    fn divider_adds_no_tab_stop(cx: &mut TestAppContext) {
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.label = "Details";
            cx.notify();
        });
        draw(cx);
        let before = view.read_with(cx, |v, _| v.before.clone());
        cx.update(|w, cx| w.focus(&before, cx));
        cx.update(|w, cx| w.focus_next(cx));
        assert!(cx.update(|w, cx| view.read(cx).after.is_focused(w)));
        cx.update(|w, cx| w.focus_prev(cx));
        assert!(cx.update(|w, cx| view.read(cx).before.is_focused(w)));
    }
}
