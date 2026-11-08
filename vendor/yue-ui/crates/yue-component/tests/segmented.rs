use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::{
    Context, Entity, FocusHandle, IntoElement, Modifiers, ParentElement as _, Render, Role,
    SharedString, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use yue_component::{
    Accent, SegmentedDistribution, SegmentedShape, SegmentedSize, SegmentedVariant, ThemeMode,
    YueSegmented, YueSegmentedOption, YueTheme,
};

struct Probe {
    value: SharedString,
    changes: Vec<SharedString>,
    accept: bool,
    disabled: bool,
    selected_disabled: bool,
    all_disabled: bool,
    theme: YueTheme,
    variant: SegmentedVariant,
    removed: Option<&'static str>,
    reversed: bool,
    empty: bool,
    equal: bool,
    width: Option<f32>,
    full_width: bool,
    minimum: Option<f32>,
    size: SegmentedSize,
    icons: bool,
    long_label: bool,
    default_theme: bool,
    second: bool,
    second_value: SharedString,
    before: FocusHandle,
    after: FocusHandle,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut options = if self.empty {
            vec![]
        } else {
            [
                YueSegmentedOption::new("day", "日"),
                YueSegmentedOption::new("week", "一整周"),
                YueSegmentedOption::new("board", "看板").disabled(true),
                YueSegmentedOption::new("year", "按年 · 省 20%"),
            ]
            .into_iter()
            .enumerate()
            .filter(|(ix, _)| Some(["day", "week", "board", "year"][*ix]) != self.removed)
            .map(|(_, o)| o)
            .collect()
        };
        if self.icons {
            options = vec![
                YueSegmentedOption::icon_only(
                    "day",
                    yue_component_icons::outline::List,
                    "列表图标",
                ),
                YueSegmentedOption::icon_only(
                    "week",
                    yue_component_icons::outline::LayoutGrid,
                    "网格图标",
                ),
            ];
        } else if self.long_label {
            options[0] = YueSegmentedOption::new("day", "更长的日期标签");
        }
        for (ix, option) in options.iter_mut().enumerate() {
            if self.all_disabled || (self.selected_disabled && ix == 0) {
                *option = option.clone().disabled(true);
            }
        }
        if self.reversed {
            options.reverse();
        }
        let mut segmented = YueSegmented::new("period", self.theme, self.value.clone())
            .label("周期")
            .variant(self.variant)
            .shape(SegmentedShape::Pill)
            .distribution(if self.equal {
                SegmentedDistribution::Equal
            } else {
                SegmentedDistribution::Content
            })
            .disabled(self.disabled)
            .size(self.size)
            .options(options)
            .on_change(cx.listener(|this, value: &SharedString, _, cx| {
                this.changes.push(value.clone());
                if this.accept {
                    this.value = value.clone();
                }
                cx.notify();
            }));
        if let Some(width) = self.width {
            segmented = segmented.width(px(width));
        }
        if self.full_width {
            segmented = segmented.width(gpui_kit::relative(1.));
        }
        if let Some(minimum) = self.minimum {
            segmented = segmented.min_width(px(minimum));
        }
        if self.default_theme {
            segmented = segmented.use_default_theme();
        }
        div()
            .w(px(700.))
            .flex()
            .flex_col()
            .items_start()
            .gap_4()
            .child(
                gpui_kit::base::Button::new("before")
                    .track_focus(&self.before)
                    .accessibility_label("Before")
                    .child("Before"),
            )
            .child(segmented)
            .child(
                gpui_kit::base::Button::new("after")
                    .track_focus(&self.after)
                    .accessibility_label("After")
                    .child("After"),
            )
            .children(self.second.then(|| {
                YueSegmented::new(
                    "independent",
                    YueTheme::new(ThemeMode::Light, Accent::Azure),
                    self.second_value.clone(),
                )
                .label("独立实例")
                .options([
                    YueSegmentedOption::new("a", "Alpha"),
                    YueSegmentedOption::new("b", "Beta"),
                ])
                .on_change(cx.listener(|this, value: &SharedString, _, cx| {
                    this.second_value = value.clone();
                    cx.notify();
                }))
            }))
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Probe>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|_, cx| Probe {
        value: "day".into(),
        changes: vec![],
        accept: true,
        disabled: false,
        selected_disabled: false,
        all_disabled: false,
        theme: YueTheme::new(ThemeMode::Light, Accent::Azure),
        variant: SegmentedVariant::Accent,
        removed: None,
        reversed: false,
        empty: false,
        equal: false,
        width: None,
        full_width: false,
        minimum: None,
        size: SegmentedSize::Medium,
        icons: false,
        long_label: false,
        default_theme: false,
        second: false,
        second_value: "a".into(),
        before: cx.focus_handle(),
        after: cx.focus_handle(),
    });
    draw(cx);
    (view, cx)
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}
fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|window, _| {
        let nodes: Vec<_> = snapshots(window)
            .into_iter()
            .filter(|n| n.label() == Some(label))
            .collect();
        assert_eq!(nodes.len(), 1, "one semantic node for {label}");
        nodes[0].clone()
    })
}
fn track(cx: &mut VisualTestContext) -> ElementSnapshot {
    cx.update(|window, _| {
        snapshots(window)
            .into_iter()
            .find(|n| {
                n.path().last()
                    == Some(&gpui_kit::ElementId::from((
                        gpui_kit::ElementId::from("period"),
                        "track",
                    )))
            })
            .expect("track geometry")
    })
}
fn click(cx: &mut VisualTestContext, label: &str) {
    let p = node(cx, label).bounds().center();
    cx.simulate_click(p, Modifiers::default());
    draw(cx);
}
fn key(cx: &mut VisualTestContext, key: &str) {
    let k = gpui_kit::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: k.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui_kit::KeyUpEvent { keystroke: k });
    draw(cx);
}

#[gpui_kit::test]
fn controlled_clicks_activation_and_disabled_options(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    assert_eq!(node(cx, "周期").role(), Some(Role::RadioGroup));
    assert_eq!(node(cx, "看板").disabled(), Some(true));
    click(cx, "一整周");
    click(cx, "一整周");
    key(cx, "space");
    key(cx, "enter");
    click(cx, "看板");
    assert_eq!(
        view.read_with(cx, |v, _| v.changes.clone()),
        vec![SharedString::from("week")]
    );
    assert_eq!(node(cx, "一整周").selected(), Some(true));
    click(cx, "After");
    click(cx, "一整周");
    assert_eq!(
        node(cx, "一整周").focused(),
        Some(true),
        "selected items remain pointer-focusable without emitting"
    );
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 1);
    view.update(cx, |v, cx| {
        v.accept = false;
        cx.notify();
    });
    draw(cx);
    click(cx, "按年 · 省 20%");
    assert_eq!(node(cx, "一整周").selected(), Some(true));
    assert_eq!(node(cx, "按年 · 省 20%").focused(), Some(true));
    view.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    key(cx, "space");
    key(cx, "right");
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 2);
}

#[gpui_kit::test]
fn one_tab_stop_arrows_home_end_and_modifiers(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "Before").focused(), Some(true));
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "日").focused(), Some(true));
    key(cx, "right");
    assert_eq!(node(cx, "一整周").focused(), Some(true));
    key(cx, "right");
    assert_eq!(node(cx, "按年 · 省 20%").focused(), Some(true));
    key(cx, "right");
    assert_eq!(node(cx, "日").focused(), Some(true));
    key(cx, "end");
    assert_eq!(node(cx, "按年 · 省 20%").focused(), Some(true));
    key(cx, "home");
    assert_eq!(node(cx, "日").focused(), Some(true));
    let before = view.read_with(cx, |v, _| v.changes.len());
    key(cx, "ctrl-right");
    key(cx, "down");
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), before);
    cx.update(|w, cx| w.focus_next(cx));
    draw(cx);
    assert_eq!(node(cx, "After").focused(), Some(true));
    cx.update(|w, cx| w.focus_prev(cx));
    draw(cx);
    assert_eq!(node(cx, "日").focused(), Some(true));
}

#[gpui_kit::test]
fn content_and_equal_widths_do_not_reflow_on_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let day = node(cx, "日").bounds();
    let track = track(cx).bounds();
    assert_eq!(day.left() - track.left(), px(4.));
    assert_eq!(day.top() - track.top(), px(4.));
    assert_eq!(track.bottom() - day.bottom(), px(4.));
    let year = node(cx, "按年 · 省 20%").bounds();
    assert!(year.size.width > day.size.width);
    click(cx, "按年 · 省 20%");
    assert_eq!(node(cx, "日").bounds(), day);
    assert_eq!(node(cx, "按年 · 省 20%").bounds(), year);
    view.update(cx, |v, cx| {
        v.equal = true;
        v.width = Some(600.);
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        node(cx, "日").bounds().size.width,
        node(cx, "按年 · 省 20%").bounds().size.width
    );
    assert!((node(cx, "日").bounds().size.width - px(148.)).abs() < px(0.01));
    view.update(cx, |v, cx| {
        v.width = Some(100.);
        cx.notify();
    });
    draw(cx);
    assert!(
        node(cx, "日").bounds().size.width >= year.size.width,
        "content minimum wins over an undersized constraint"
    );
}

#[gpui_kit::test]
fn parent_relative_width_and_content_floor_are_independent(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let natural = node(cx, "日").bounds().size.width;
    assert!(node(cx, "周期").bounds().size.width < px(700.));
    view.update(cx, |v, cx| {
        v.full_width = true;
        v.equal = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "周期").bounds().size.width, px(700.));
    assert!((node(cx, "日").bounds().size.width - px(173.)).abs() < px(0.01));
    view.update(cx, |v, cx| {
        v.full_width = false;
        v.width = Some(40.);
        v.minimum = Some(1.);
        cx.notify();
    });
    draw(cx);
    assert!(
        node(cx, "日").bounds().size.width > natural,
        "too-small explicit min_width cannot collapse the track"
    );
    view.update(cx, |v, cx| {
        v.width = None;
        v.minimum = Some(800.);
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "周期").bounds().size.width, px(800.));
    assert!((node(cx, "日").bounds().size.width - px(198.)).abs() < px(0.01));
}

#[gpui_kit::test]
fn disabling_focused_group_leaves_without_requesting_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "一整周");
    view.update(cx, |v, cx| {
        v.disabled = true;
        cx.notify();
    });
    draw(cx);
    tick(cx, 20);
    assert_eq!(node(cx, "After").focused(), Some(true));
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 1);
    assert_eq!(node(cx, "一整周").selected(), Some(true));
    view.update(cx, |v, cx| {
        v.disabled = false;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "After").focused(), Some(true));
    cx.update(|w, cx| w.focus_prev(cx));
    draw(cx);
    assert_eq!(node(cx, "一整周").focused(), Some(true));
}

#[gpui_kit::test]
fn disabled_selection_uses_a_flat_surface_in_both_variants_and_themes(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for variant in [SegmentedVariant::Neutral, SegmentedVariant::Accent] {
                for state in 0..3 {
                    let theme = YueTheme::new(mode, accent);
                    view.update(cx, |v, cx| {
                        v.theme = theme;
                        v.variant = variant;
                        v.disabled = state == 0;
                        v.selected_disabled = state == 1;
                        v.all_disabled = state == 2;
                        cx.notify();
                    });
                    draw(cx);
                    let bounds = node(cx, "日").bounds();
                    let track_bounds = track(cx).bounds();
                    cx.update(|window, _| {
                        let expected: gpui_kit::Hsla = gpui_kit::rgb(theme.surface()).into();
                        let quad = window
                            .painted_quads()
                            .into_iter()
                            .find(|quad| {
                                quad.bounds == bounds.scale(window.scale_factor())
                                    && quad.background.as_solid() == Some(expected)
                            })
                            .expect("disabled selection keeps a neutral filled surface");
                        assert_eq!(
                            quad.border_widths,
                            Default::default(),
                            "no input-like disabled outline"
                        );
                    });
                    if state != 1 {
                        cx.update(|window, _| {
                            let expected: gpui_kit::Hsla =
                                gpui_kit::rgb(theme.surface_component()).into();
                            assert!(
                                window
                                    .painted_quads()
                                    .into_iter()
                                    .any(|q| q.bounds == track_bounds.scale(window.scale_factor())
                                        && q.background.as_solid() == Some(expected)),
                                "an entirely disabled accent group has a neutral track"
                            );
                        });
                    }
                    assert_eq!(node(cx, "日").selected(), Some(true));
                    assert_eq!(node(cx, "日").disabled(), Some(true));
                    click(cx, "日");
                    assert!(view.read_with(cx, |v, _| v.changes.is_empty()));
                }
            }
        }
    }
}

#[gpui_kit::test]
fn missing_and_empty_values_do_not_emit_or_steal_focus(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "After");
    view.update(cx, |v, cx| {
        v.value = "absent".into();
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "After").focused(), Some(true));
    for label in ["日", "一整周", "看板", "按年 · 省 20%"] {
        assert_eq!(node(cx, label).selected(), Some(false));
    }
    assert!(view.read_with(cx, |v, _| v.changes.is_empty()));
    view.update(cx, |v, cx| {
        v.empty = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "After").focused(), Some(true));
    assert!(
        node(cx, "周期").bounds().size.width > px(4.),
        "an empty group keeps its label's natural width"
    );
}

fn tick(cx: &mut VisualTestContext, millis: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(millis));
    cx.update(|w, cx| {
        w.simulate_next_frame(cx);
        w.draw(cx).clear(cx);
    });
}
fn indicator(cx: &mut VisualTestContext) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update(|w, _| {
        let color: gpui_kit::Hsla =
            gpui_kit::rgb(YueTheme::new(ThemeMode::Light, Accent::Azure).accent_solid()).into();
        let quad = w
            .painted_quads()
            .into_iter()
            .find(|q| q.background.as_solid() == Some(color))
            .expect("actual indicator quad");
        let scale = w.scale_factor();
        gpui_kit::Bounds::new(
            gpui_kit::point(
                px(quad.bounds.origin.x.as_f32() / scale),
                px(quad.bounds.origin.y.as_f32() / scale),
            ),
            gpui_kit::size(
                px(quad.bounds.size.width.as_f32() / scale),
                px(quad.bounds.size.height.as_f32() / scale),
            ),
        )
    })
}

#[gpui_kit::test]
fn indicator_has_position_and_width_frames_and_retargets(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx);
    let start = indicator(cx);
    let target = node(cx, "按年 · 省 20%").bounds();
    assert_eq!(start, node(cx, "日").bounds());
    click(cx, "按年 · 省 20%");
    assert_eq!(
        indicator(cx),
        start,
        "selection starts at the currently painted location"
    );
    tick(cx, 90);
    let middle = indicator(cx);
    assert!(middle.left() > start.left() && middle.left() < target.left());
    assert!(middle.size.width > start.size.width && middle.size.width < target.size.width);
    click(cx, "一整周");
    let retarget = indicator(cx);
    assert!(
        (retarget.left() - middle.left()).abs() < px(1.),
        "no jump on retarget"
    );
    tick(cx, 200);
    assert_eq!(
        indicator(cx),
        node(cx, "一整周").bounds(),
        "retarget settles within the new 200ms duration"
    );
    cx.update(|_, cx| cx.set_reduce_motion(true));
    click(cx, "日");
    assert_eq!(indicator(cx), node(cx, "日").bounds());
}

#[gpui_kit::test]
fn resize_rem_reorder_and_removed_focus_repair_use_current_geometry(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "一整周");
    tick(cx, 400);
    view.update(cx, |v, cx| {
        v.reversed = true;
        v.equal = true;
        v.width = Some(650.);
        cx.notify();
    });
    draw(cx);
    assert_eq!(indicator(cx), node(cx, "一整周").bounds());
    assert_eq!(node(cx, "一整周").focused(), Some(true));
    cx.update(|w, _| w.set_rem_size(px(20.)));
    draw(cx);
    assert_eq!(indicator(cx), node(cx, "一整周").bounds());
    view.update(cx, |v, cx| {
        v.removed = Some("week");
        cx.notify();
    });
    draw(cx);
    assert_eq!(node(cx, "日").focused(), Some(true));
    assert_eq!(
        view.read_with(cx, |v, _| v.changes.len()),
        1,
        "focus repair never changes controlled value"
    );
}

#[gpui_kit::test]
fn icon_sizes_and_dynamic_labels_snap_without_owner_events(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "一整周");
    tick(cx, 400);
    view.update(cx, |v, cx| {
        v.long_label = true;
        cx.notify();
    });
    draw(cx);
    assert_eq!(indicator(cx), node(cx, "一整周").bounds());
    assert_eq!(view.read_with(cx, |v, _| v.changes.len()), 1);
    for (size, height) in [
        (SegmentedSize::Small, 20.),
        (SegmentedSize::Medium, 24.),
        (SegmentedSize::Large, 32.),
    ] {
        view.update(cx, |v, cx| {
            v.icons = true;
            v.size = size;
            cx.notify();
        });
        draw(cx);
        let bounds = node(cx, "列表图标").bounds();
        // Icon-only content still includes the size's horizontal padding.
        assert!(bounds.size.width >= bounds.size.height);
        assert_eq!(bounds.size.height, px(height));
        assert_eq!(indicator(cx), node(cx, "网格图标").bounds());
    }
    click(cx, "列表图标");
    tick(cx, 400);
    assert_eq!(
        view.read_with(cx, |v, _| v.value.clone()),
        SharedString::from("day")
    );
}

#[gpui_kit::test]
fn independent_groups_keep_separate_values_focus_and_indicator_channels(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.second = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "按年 · 省 20%");
    tick(cx, 90);
    let moving = indicator(cx);
    click(cx, "Beta");
    assert_eq!(
        indicator(cx),
        moving,
        "another group cannot reset this group's transition"
    );
    assert_eq!(node(cx, "Beta").selected(), Some(true));
    assert_eq!(node(cx, "Beta").focused(), Some(true));
    tick(cx, 400);
    assert_eq!(indicator(cx), node(cx, "按年 · 省 20%").bounds());
    assert_eq!(
        view.read_with(cx, |v, _| (v.value.clone(), v.second_value.clone())),
        ("year".into(), "b".into())
    );
}

#[gpui_kit::test]
fn default_theme_changes_and_explicit_override_keep_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |v, cx| {
        v.default_theme = true;
        cx.notify();
    });
    draw(cx);
    let dark = YueTheme::new(ThemeMode::Dark, Accent::Neutral);
    cx.update(|_, cx| yue_component::set_theme(cx, dark));
    draw(cx);
    let has_color = |cx: &mut VisualTestContext, theme: YueTheme| {
        cx.update(|w, _| {
            let expected: gpui_kit::Hsla = gpui_kit::rgb(theme.accent_solid()).into();
            w.painted_quads()
                .into_iter()
                .any(|q| q.background.as_solid() == Some(expected))
        })
    };
    assert!(has_color(cx, dark));
    view.update(cx, |v, cx| {
        v.default_theme = false;
        cx.notify();
    });
    draw(cx);
    assert!(has_color(
        cx,
        YueTheme::new(ThemeMode::Light, Accent::Azure)
    ));
    assert_eq!(node(cx, "日").selected(), Some(true));
    assert!(view.read_with(cx, |v, _| v.changes.is_empty()));
}
