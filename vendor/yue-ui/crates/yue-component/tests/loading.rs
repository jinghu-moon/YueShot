//! Production feedback geometry, native semantics, delay and branch regressions.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    Role, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use std::{cell::Cell, rc::Rc, time::Duration};
use yue_component::{
    Accent, FeedbackSize, ProgressShape, ProgressStatus, ProgressValue, SkeletonAnimation,
    SkeletonShape, ThemeMode, YueButton, YueLoadingRegion, YueProgress, YueSkeleton, YueSpinner,
    YueTheme,
};

struct Probe {
    percent: f32,
    shape: ProgressShape,
    status: ProgressStatus,
    size: FeedbackSize,
    animated: bool,
    active: bool,
    decorative: bool,
    delay: Duration,
    animation: SkeletonAnimation,
    loading: bool,
    theme: YueTheme,
    show_value: bool,
    builds: Rc<[Cell<usize>; 2]>,
    actions: Rc<Cell<usize>>,
}
impl Default for Probe {
    fn default() -> Self {
        Self {
            percent: 42.,
            shape: ProgressShape::Linear,
            status: ProgressStatus::Normal,
            size: FeedbackSize::Medium,
            animated: true,
            active: false,
            decorative: false,
            delay: Duration::from_millis(150),
            animation: SkeletonAnimation::Wave,
            loading: true,
            theme: YueTheme::new(ThemeMode::Light, Accent::Azure),
            show_value: true,
            builds: Rc::new([Cell::new(0), Cell::new(0)]),
            actions: Rc::new(Cell::new(0)),
        }
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let a = self.builds.clone();
        let b = a.clone();
        let actions = self.actions.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .p_4()
            .gap_3()
            .child(
                YueProgress::new("progress", ProgressValue::new(self.percent))
                    .label("上传")
                    .theme(self.theme)
                    .size(self.size)
                    .shape(self.shape)
                    .status(self.status)
                    .animated(self.animated)
                    .show_value(self.show_value)
                    .width(px(240.)),
            )
            .child(
                YueSpinner::new("spinner")
                    .active(self.active)
                    .delay(self.delay)
                    .decorative(self.decorative)
                    .animated(self.animated)
                    .label("搜索")
                    .theme(self.theme),
            )
            .child(
                YueSkeleton::new("text")
                    .theme(self.theme)
                    .size(self.size)
                    .animation(self.animation)
                    .width(px(160.)),
            )
            .child(
                YueSkeleton::new("text-sync")
                    .theme(self.theme)
                    .size(self.size)
                    .animation(self.animation)
                    .width(px(160.)),
            )
            .child(
                YueSkeleton::new("circle")
                    .theme(self.theme)
                    .shape(SkeletonShape::Circle)
                    .animation(self.animation)
                    .width(px(80.))
                    .height(px(40.)),
            )
            .child(
                YueSkeleton::new("rectangle")
                    .theme(self.theme)
                    .shape(SkeletonShape::Rectangle)
                    .animation(self.animation)
                    .width(px(120.))
                    .height(px(32.)),
            )
            .child(
                YueSkeleton::new("rounded")
                    .theme(self.theme)
                    .shape(SkeletonShape::Rounded)
                    .animation(self.animation)
                    .width(px(120.))
                    .height(px(32.)),
            )
            .child(
                YueLoadingRegion::new("region", self.loading)
                    .label("加载资料")
                    .placeholder(move |_, _| {
                        a[0].set(a[0].get() + 1);
                        div().child("占位").into_any_element()
                    })
                    .content(move |_, _| {
                        b[1].set(b[1].get() + 1);
                        let actions = actions.clone();
                        YueButton::new("result-action", "结果动作")
                            .on_click(move |_, _, _| actions.set(actions.get() + 1))
                            .into_any_element()
                    }),
            )
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
    cx.run_until_parked();
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
        let v = cx.new(|_| Probe::default());
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
        snapshots(w)
            .into_iter()
            .find(|n| n.label() == Some(label))
            .unwrap_or_else(|| panic!("missing {label}"))
    })
}
fn bounds(cx: &mut VisualTestContext, id: &'static str) -> Bounds<Pixels> {
    cx.update(|w, _| {
        snapshots(w)
            .into_iter()
            .find(|n| n.path().last() == Some(&id.into()))
            .unwrap()
            .bounds()
    })
}
fn bar_width(cx: &mut VisualTestContext, theme: YueTheme) -> f32 {
    cx.update(|w, _| {
        let color: gpui_kit::Hsla = gpui_kit::rgb(theme.accent_solid()).into();
        w.painted_quads()
            .iter()
            .filter(|q| q.background.as_solid() == Some(color))
            .map(|q| q.bounds.size.width.as_f32() / w.scale_factor())
            .fold(0., f32::max)
    })
}
fn surface(cx: &mut VisualTestContext, id: &'static str) -> (f32, f32, f32, gpui_kit::Hsla) {
    let b = bounds(cx, id);
    cx.update(|w, _| {
        let scale = w.scale_factor();
        let b = b.scale(scale);
        let q = w
            .painted_quads()
            .into_iter()
            .find(|q| {
                q.bounds.left() >= b.left()
                    && q.bounds.right() <= b.right()
                    && q.bounds.top() >= b.top()
                    && q.bounds.bottom() <= b.bottom()
                    && q.background.as_solid().is_some_and(|c| c.a > 0.)
            })
            .unwrap();
        (
            q.bounds.size.width.as_f32() / scale,
            q.bounds.size.height.as_f32() / scale,
            q.corner_radii.top_left.as_f32() / scale,
            q.background.as_solid().unwrap(),
        )
    })
}

#[gpui_kit::test]
fn real_values_reach_semantics_while_paint_can_interpolate_and_reverse(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let theme = view.read_with(cx, |v, _| v.theme);
    assert_eq!(node(cx, "上传").numeric_value(), Some(42.));
    assert!(
        (bar_width(cx, theme) - 100.8).abs() < 0.2,
        "first paint is already 42%"
    );
    view.update(cx, |v, cx| {
        v.percent = 100.;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "上传").numeric_value(), Some(100.));
    assert!(bar_width(cx, theme) < 110.);
    tick(cx, 60);
    let midway = bar_width(cx, theme);
    assert!(midway > 100.8 && midway < 240.);
    view.update(cx, |v, cx| {
        v.percent = 0.;
        cx.notify();
    });
    draw(cx);
    assert!((bar_width(cx, theme) - midway).abs() < 0.3);
    tick(cx, 200);
    assert_eq!(bar_width(cx, theme), 0.);
    assert_eq!(node(cx, "上传").numeric_value(), Some(0.));
}

#[gpui_kit::test]
fn clamp_unknown_status_and_shape_changes_preserve_real_semantics(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for (p, expected) in [
        (-20., Some(0.)),
        (120., Some(100.)),
        (f32::NAN, None),
        (f32::INFINITY, None),
    ] {
        view.update(cx, |v, cx| {
            v.percent = p;
            v.animated = false;
            cx.notify();
        });
        draw(cx);
        let n = node(cx, "上传");
        assert_eq!(n.numeric_value(), expected);
        assert_eq!(n.role(), Some(Role::ProgressIndicator));
        assert_eq!(n.min_numeric_value(), Some(0.));
        assert_eq!(n.max_numeric_value(), Some(100.));
        assert_eq!(n.focused(), None);
    }
    for (shape, status, text) in [
        (ProgressShape::Linear, ProgressStatus::Error, "失败"),
        (ProgressShape::Circle, ProgressStatus::Paused, "已暂停"),
        (ProgressShape::Circle, ProgressStatus::Success, "已完成"),
    ] {
        view.update(cx, |v, cx| {
            v.percent = 45.;
            v.shape = shape;
            v.status = status;
            v.show_value = false;
            cx.notify();
        });
        draw(cx);
        assert_eq!(node(cx, "上传").numeric_value(), Some(45.));
        assert_eq!(node(cx, "上传").value(), Some(text));
    }
}

#[gpui_kit::test]
fn spinner_delay_short_tasks_cancellation_and_reactivation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let visible = |cx: &mut VisualTestContext| {
        cx.update(|w, _| snapshots(w).iter().any(|n| n.label() == Some("搜索")))
    };
    assert!(!visible(cx));
    view.update(cx, |v, cx| {
        v.active = true;
        cx.notify();
    });
    draw(cx);
    tick(cx, 80);
    assert!(!visible(cx));
    view.update(cx, |v, cx| {
        v.active = false;
        cx.notify();
    });
    draw(cx);
    tick(cx, 200);
    assert!(!visible(cx));
    view.update(cx, |v, cx| {
        v.active = true;
        cx.notify();
    });
    draw(cx);
    tick(cx, 149);
    assert!(!visible(cx));
    tick(cx, 2);
    assert!(visible(cx));
    assert_eq!(node(cx, "搜索").numeric_value(), None);
    view.update(cx, |v, cx| {
        v.active = false;
        cx.notify();
    });
    draw(cx);
    assert!(!visible(cx));
    view.update(cx, |v, cx| {
        v.active = true;
        cx.notify();
    });
    draw(cx);
    assert!(!visible(cx));
    tick(cx, 151);
    assert!(visible(cx));
    view.update(cx, |v, cx| {
        v.decorative = true;
        cx.notify();
    });
    draw(cx);
    assert!(cx.update(|w, _| {
        snapshots(w)
            .iter()
            .filter(|n| n.role() == Some(Role::ProgressIndicator))
            .count()
            == 1
    }));
}

#[gpui_kit::test]
fn region_builds_only_one_branch_and_old_action_leaves_the_input_tree(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let builds = view.read_with(cx, |v, _| v.builds.clone());
    assert!(builds[0].get() > 0);
    assert_eq!(builds[1].get(), 0);
    assert_eq!(node(cx, "加载资料").role(), Some(Role::Status));
    view.update(cx, |v, cx| {
        v.loading = false;
        cx.notify();
    });
    draw(cx);
    let old = builds[0].get();
    let target = node(cx, "结果动作").bounds().center();
    cx.simulate_click(target, Default::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.actions.get()), 1);
    assert_eq!(builds[0].get(), old);
    view.update(cx, |v, cx| {
        v.loading = true;
        cx.notify();
    });
    draw(cx);
    assert!(cx.update(|w, _| snapshots(w).iter().all(|n| n.label() != Some("结果动作"))));
    cx.simulate_click(target, Default::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.actions.get()), 1);
    assert_eq!(
        cx.update(|w, _| snapshots(w)
            .iter()
            .filter(|n| n.role() == Some(Role::Status))
            .count()),
        1
    );
}

#[gpui_kit::test]
fn skeleton_geometry_respects_text_frame_circle_and_corner_shapes(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.animation = SkeletonAnimation::None;
        cx.notify();
    });
    draw(cx);
    assert_eq!(bounds(cx, "text").size.height, px(20.));
    let (w, h, r, _) = surface(cx, "text");
    assert_eq!((w, h), (160., 12.));
    assert!(r > 0.);
    let (w, h, r, _) = surface(cx, "circle");
    assert_eq!((w, h, r), (40., 40., 20.));
    assert_eq!(surface(cx, "rectangle").2, 0.);
    assert!(surface(cx, "rounded").2 > 0.);
    for id in ["text", "circle", "rectangle", "rounded"] {
        assert!(cx.update(|w, _| {
            snapshots(w)
                .iter()
                .filter(|n| n.path().last() == Some(&id.into()))
                .all(|n| n.role().is_none() && n.focused().is_none())
        }));
    }
}

#[gpui_kit::test]
fn pulse_changes_real_fill_in_sync_and_reduced_motion_is_static(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.animation = SkeletonAnimation::Pulse;
        cx.notify();
    });
    draw(cx);
    let initial = surface(cx, "text").3;
    tick(cx, 450);
    let next = surface(cx, "text").3;
    assert_ne!(initial, next);
    assert_eq!(next, surface(cx, "text-sync").3);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    draw(cx);
    let static_fill = surface(cx, "text").3;
    tick(cx, 450);
    assert_eq!(static_fill, surface(cx, "text").3);
    cx.update(|_, cx| cx.set_reduce_motion(false));
    view.update(cx, |v, cx| {
        v.animation = SkeletonAnimation::None;
        cx.notify();
    });
    draw(cx);
    let static_fill = surface(cx, "text").3;
    tick(cx, 450);
    assert_eq!(static_fill, surface(cx, "text").3);
}

#[gpui_kit::test]
fn wave_submits_moving_gradients_clipped_to_the_rounded_silhouette(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    tick(cx, 600);
    let b = bounds(cx, "rounded");
    let signature = |cx: &mut VisualTestContext| {
        cx.update(|w, _| {
            let b = b.scale(w.scale_factor());
            w.painted_quads()
                .iter()
                .filter(|q| q.bounds == b && q.background.as_solid().is_none())
                .map(|q| {
                    format!(
                        "{:?}/{:?}/{:?}",
                        q.background, q.content_mask, q.corner_radii
                    )
                })
                .collect::<Vec<_>>()
        })
    };
    let first = signature(cx);
    assert!(!first.is_empty());
    tick(cx, 200);
    let next = signature(cx);
    assert_ne!(first, next);
    cx.update(|w, _| {
        let b = b.scale(w.scale_factor());
        for q in w
            .painted_quads()
            .iter()
            .filter(|q| q.bounds == b && q.background.as_solid().is_none())
        {
            assert!(q.corner_radii.top_left.as_f32() > 0.);
            assert!(
                q.content_mask.bounds.left() >= b.left()
                    && q.content_mask.bounds.right() <= b.right()
            );
        }
    });
}

#[gpui_kit::test]
fn feedback_geometry_scales_with_rem_and_survives_display_scale_and_theme_changes(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    for rem in [16., 20., 24.] {
        for scale in [1., 1.5, 2.] {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                cx.simulate_scale_factor_change(scale);
                cx.update(|w, _| w.set_rem_size(px(rem)));
                view.update(cx, |v, cx| {
                    v.theme = YueTheme::new(mode, Accent::Azure);
                    v.animation = SkeletonAnimation::None;
                    v.animated = false;
                    cx.notify();
                });
                draw(cx);
                assert!(
                    (bounds(cx, "text").size.height - px(rem * 1.25)).abs()
                        <= px(1. / scale + 0.01),
                    "rem={rem}, scale={scale}, actual={:?}",
                    bounds(cx, "text")
                );
                assert!((surface(cx, "text").1 - rem * 0.75).abs() <= 1. / scale + 0.01);
                assert_eq!(node(cx, "上传").numeric_value(), Some(42.));
            }
        }
    }
}
