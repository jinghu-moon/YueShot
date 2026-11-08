//! Chrome shares one surface with the viewport; only floating inspectors have
//! their own border and elevation. Retained controls are scoped to the viewer.
use super::*;
use crate::{
    ButtonSize, ButtonVariant, DropdownAlign, DropdownPlacement, SliderDomain, SliderSize,
    SliderTooltip, YueButton, YueDropdownMenu, YueMenuItem, YueSlider, YueSliderEvent,
    YueSliderState,
    tokens::{rems, shadow},
};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{
    AppContext as _, FocusHandle, InteractiveElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _,
};
use std::time::Instant;

pub(super) struct ChromeState {
    information: bool,
    information_focus: FocusHandle,
}

pub(super) fn chrome_state(
    state: &Entity<YueImagePreviewState>,
    w: &mut Window,
    cx: &mut App,
) -> Entity<ChromeState> {
    w.use_keyed_state(state.read(cx).child_id("chrome"), cx, |_, cx| ChromeState {
        information: false,
        information_focus: cx.focus_handle(),
    })
}

pub(super) fn information_button(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    w: &mut Window,
    cx: &mut App,
) -> YueButton {
    let chrome = chrome_state(state, w, cx);
    let focus = chrome.read(cx).information_focus.clone();
    YueButton::icon(
        state.read(cx).child_id("information"),
        theme,
        crate::outline::InfoCircle,
        "图片信息",
    )
    .size(ButtonSize::Small)
    .variant(if chrome.read(cx).information {
        ButtonVariant::Outline
    } else {
        ButtonVariant::Ghost
    })
    .track_focus(&focus)
    .tooltip("图片信息")
    .on_click(move |_, w, cx| {
        chrome.update(cx, |s, cx| {
            s.information = !s.information;
            cx.notify();
        });
        w.refresh();
    })
}

pub(super) fn dismiss_information(
    chrome: &Entity<ChromeState>,
    w: &mut Window,
    cx: &mut App,
) -> bool {
    if !chrome.read(cx).information {
        return false;
    }
    chrome.update(cx, |s, cx| {
        s.information = false;
        s.information_focus.focus(w, cx);
        cx.notify();
    });
    w.refresh();
    true
}

pub(super) fn information_panel(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    w: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let chrome = chrome_state(state, w, cx);
    let open = chrome.read(cx).information;
    let s = state.read(cx);
    let opacity = gpui_kit::base::transition(
        s.child_id("information-opacity"),
        if open { 1. } else { 0. },
        gpui_kit::base::Transition::new(if s.animated {
            crate::motion::MotionDuration::Interaction.duration(theme)
        } else {
            std::time::Duration::ZERO
        }),
        w,
        cx,
    );
    if opacity <= 0. {
        return div().into_any_element();
    }
    let s = state.read(cx);
    let image = s.reference();
    let row = |label: &str, value: String| {
        div()
            .flex()
            .justify_between()
            .gap_3()
            .py_1()
            .child(
                div()
                    .text_color(gpui_kit::rgb(theme.text_secondary()))
                    .child(label.to_owned()),
            )
            .child(div().min_w_0().text_right().child(value))
    };
    let owner = chrome.clone();
    div()
        .id(s.child_id("information-panel"))
        .test_support()
        .role(Role::Group)
        .aria_label("图片信息面板")
        .absolute()
        .top_3()
        .right(gpui_kit::rems(0.75 - (1. - opacity) * 0.75))
        .w(gpui_kit::rems(16.))
        .max_w_full()
        .p_4()
        .rounded(rems(theme, "--image-preview-radius"))
        .bg(gpui_kit::rgb(theme.surface()))
        .border_1()
        .border_color(gpui_kit::rgb(theme.border_default()))
        .text_color(gpui_kit::rgb(theme.text_primary()))
        .text_xs()
        .shadow(shadow(theme, "--image-preview-glass-shadow"))
        .opacity(opacity)
        .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| {
            cx.stop_propagation()
        })
        .on_scroll_wheel(|_, w, cx| {
            w.prevent_default();
            cx.stop_propagation();
        })
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb_3()
                .child("图片信息")
                .child(
                    YueButton::icon(
                        s.child_id("information-close"),
                        theme,
                        crate::outline::X,
                        "关闭图片信息",
                    )
                    .size(ButtonSize::Small)
                    .variant(ButtonVariant::Ghost)
                    .on_click(move |_, w, cx| {
                        dismiss_information(&owner, w, cx);
                    }),
                ),
        )
        .when_some(image, |d, image| {
            let [width, height] = image.pixel_size();
            let texture = image.texture_size();
            d.child(row("尺寸", format!("{width} × {height} px")))
                .child(row(
                    "预览资源",
                    format!("{} × {} px", texture[0], texture[1]),
                ))
                .child(row(
                    "倍率",
                    format!(
                        "{:.1}%",
                        s.display_camera(Instant::now()).zoom(image.dimensions()) * 100.
                    ),
                ))
                .child(row(
                    "采样",
                    match s.sampling {
                        ImagePreviewSampling::Smooth => "平滑",
                        ImagePreviewSampling::Nearest => "像素",
                    }
                    .into(),
                ))
                .child(row("背景", s.background.label().into()))
        })
        .into_any_element()
}

fn menu(
    id: gpui_kit::ElementId,
    theme: crate::YueTheme,
    trigger: YueButton,
    label: &'static str,
    items: Vec<YueMenuItem>,
) -> YueDropdownMenu {
    YueDropdownMenu::new(
        id,
        theme,
        trigger
            .size(ButtonSize::Small)
            .variant(ButtonVariant::Ghost),
    )
    .label(label)
    .items(items)
    .placement(DropdownPlacement::Above)
    .align(DropdownAlign::Start)
}

fn fit_button(
    id: gpui_kit::ElementId,
    theme: crate::YueTheme,
    icon: impl yue_component_icons::IconDefinition,
    label: &'static str,
    selected: bool,
    disabled: bool,
) -> YueButton {
    YueButton::icon(id, theme, icon, label)
        .size(ButtonSize::Small)
        .variant(if selected {
            ButtonVariant::Default
        } else {
            ButtonVariant::Ghost
        })
        .accessibility_description(if selected {
            format!("当前：{label}")
        } else {
            label.into()
        })
        .disabled(disabled)
}

pub(super) fn footer_toolbar(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    background_picker: bool,
    on_expand: Option<ExpandHandler>,
    w: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let s = state.read(cx);
    let id = |name| s.child_id(name);
    let disabled = s.disabled || s.reference().is_none();
    let compact = s.viewport.size[0] < 760.;
    let zoom = s.reference().map_or(1., |i| {
        s.display_camera(Instant::now()).zoom(i.dimensions())
    });
    let limits = s
        .reference()
        .map(|i| s.viewport.zoom_limits(i.dimensions()))
        .unwrap_or([0.05, 8.]);
    let fit_current = s.fit == ImagePreviewFit::FitAll;
    let width_current = s.fit == ImagePreviewFit::FitWidth;
    let owner = state.clone();
    let slider = w.use_keyed_state(id("zoom-slider-state"), cx, move |w, cx| {
        let slider = cx.new(|cx| YueSliderState::new(SliderDomain::new(0., 1000.), 0., w, cx));
        let subscription = cx.subscribe_in(&slider, w, move |_, _, event, w, cx| {
            if let YueSliderEvent::Change(value) = event {
                owner.update(cx, |s, cx| {
                    if s.disabled {
                        return;
                    }
                    let Some(image) = s.reference() else {
                        return;
                    };
                    let limits = s.viewport.zoom_limits(image.dimensions());
                    let target = (limits[0].ln()
                        + value.end() / 1000. * (limits[1].ln() - limits[0].ln()))
                    .exp();
                    let current = s.display_camera(Instant::now()).zoom(image.dimensions());
                    s.user_zoom(
                        target / current,
                        s.viewport.midpoint(),
                        false,
                        ImagePreviewChangeSource::Toolbar,
                        w,
                        cx,
                    );
                });
            }
        });
        (slider, subscription)
    });
    let slider = slider.read(cx).0.clone();
    let position = ((zoom.ln() - limits[0].ln()) / (limits[1].ln() - limits[0].ln()) * 1000.)
        .clamp(0., 1000.)
        .round();
    slider.update(cx, |s, cx| {
        s.set_value(position, w, cx);
        s.set_disabled(disabled, w, cx);
    });
    let s = state.read(cx);
    let id = |name| s.child_id(name);
    let mut left = div().flex().items_center().gap_1().min_w_0().flex_1();
    if background_picker {
        let items = [
            ImagePreviewBackground::CheckerDark,
            ImagePreviewBackground::CheckerLight,
            ImagePreviewBackground::White,
            ImagePreviewBackground::Black,
        ]
        .into_iter()
        .map(|background| {
            let owner = state.clone();
            YueMenuItem::new(background.label())
                .radio(s.background == background)
                .on_click(move |_, _, cx| {
                    owner.update(cx, |s, cx| s.user_background(background, cx))
                })
        })
        .collect();
        left = left.child(menu(
            id("background-menu"),
            theme,
            YueButton::icon(
                id("background-trigger"),
                theme,
                crate::outline::GridPattern,
                "背景",
            )
            .dropdown_indicator()
            .tooltip("预览背景")
            .disabled(disabled),
            "预览背景",
            items,
        ));
    }
    let mut display_items = Vec::new();
    for (sampling, label) in [
        (ImagePreviewSampling::Smooth, "平滑显示"),
        (ImagePreviewSampling::Nearest, "像素显示"),
    ] {
        let owner = state.clone();
        display_items.push(
            YueMenuItem::new(label)
                .radio(s.sampling == sampling)
                .disabled(
                    sampling == ImagePreviewSampling::Nearest
                        && (!cfg!(target_os = "windows") || s.has_proxy()),
                )
                .on_click(move |_, _, cx| owner.update(cx, |s, cx| s.user_sampling(sampling, cx))),
        );
    }
    let grid_owner = state.clone();
    display_items.push(YueMenuItem::separator());
    display_items.push(
        YueMenuItem::new("像素网格（≥400%）")
            .checked(s.pixel_grid)
            .disabled(!s.is_pixel_grid_available())
            .on_click(move |_, _, cx| {
                grid_owner.update(cx, |s, cx| s.user_pixel_grid(!s.pixel_grid, cx))
            }),
    );
    if s.mode == ImagePreviewMode::Compare {
        let owner = state.clone();
        display_items.push(
            YueMenuItem::new("对比归中（50%）").on_click(move |_, _, cx| {
                owner.update(cx, |s, cx| {
                    s.user_split(0.5, ImagePreviewChangeSource::Toolbar, cx)
                })
            }),
        );
    }
    if let Some(expand) = on_expand {
        let owner = state.clone();
        display_items.push(YueMenuItem::separator());
        display_items.push(
            YueMenuItem::new("放大预览…")
                .icon(crate::outline::ArrowsDiagonal)
                .on_click(move |_, w, cx| {
                    let view = owner.update(cx, |s, _| {
                        s.cancel_gesture(w);
                        s.finish_motion();
                        s.capture_view()
                    });
                    expand(&view, w, cx);
                }),
        );
    }
    left = left.child(menu(
        id("display-menu"),
        theme,
        YueButton::icon(
            id("display-trigger"),
            theme,
            crate::outline::AdjustmentsHorizontal,
            "显示",
        )
        .dropdown_indicator()
        .tooltip("显示设置")
        .disabled(disabled),
        "显示设置",
        display_items,
    ));
    let out = state.clone();
    let inside = state.clone();
    let fit = state.clone();
    let fit_width = state.clone();
    let zoom_items = [0.25, 0.5, 1., 2., 4., 8.]
        .into_iter()
        .map(|value| {
            let owner = state.clone();
            YueMenuItem::new(format!("{:.0}%", value * 100.))
                .radio((zoom - value).abs() < 0.001)
                .on_click(move |_, w, cx| {
                    owner.update(cx, |s, cx| {
                        let Some(image) = s.reference() else {
                            return;
                        };
                        let current = s.display_camera(Instant::now()).zoom(image.dimensions());
                        s.user_zoom(
                            value / current,
                            s.viewport.midpoint(),
                            true,
                            ImagePreviewChangeSource::Toolbar,
                            w,
                            cx,
                        );
                    })
                })
        })
        .collect();
    let help_items = [
        "滚轮缩放 · 拖动平移",
        "适应窗口　0",
        "适应宽度　W",
        "原始比例　1",
        "缩放　+ / −",
        "全屏　F / Esc",
        "上一张 / 下一张　PageUp / PageDown",
    ]
    .into_iter()
    .map(|label| YueMenuItem::new(label).disabled(true))
    .collect();
    let right = div()
        .id(id("zoom-toolbar"))
        .test_support()
        .role(Role::Toolbar)
        .aria_label("缩放工具条")
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .font_features(crate::tabular_nums())
        .child(
            YueButton::icon(id("zoom-out"), theme, crate::outline::Minus, "缩小")
                .size(ButtonSize::Small)
                .variant(ButtonVariant::Ghost)
                .disabled(disabled || zoom <= limits[0] + 0.000001)
                .tooltip("缩小（−）")
                .on_click(move |_, w, cx| {
                    out.update(cx, |s, cx| {
                        s.user_zoom(
                            1. / 1.25,
                            s.viewport.midpoint(),
                            true,
                            ImagePreviewChangeSource::Toolbar,
                            w,
                            cx,
                        )
                    })
                }),
        )
        .child(
            YueSlider::new(id("zoom-slider"), theme, &slider)
                .size(SliderSize::Small)
                .tooltip(SliderTooltip::None)
                .width(gpui_kit::rems(if compact { 3. } else { 5.5 }))
                .accessibility_label("图片缩放倍率"),
        )
        .child(
            YueButton::icon(id("zoom-in"), theme, crate::outline::Plus, "放大")
                .size(ButtonSize::Small)
                .variant(ButtonVariant::Ghost)
                .disabled(disabled || zoom >= limits[1] - 0.000001)
                .tooltip("放大（+）")
                .on_click(move |_, w, cx| {
                    inside.update(cx, |s, cx| {
                        s.user_zoom(
                            1.25,
                            s.viewport.midpoint(),
                            true,
                            ImagePreviewChangeSource::Toolbar,
                            w,
                            cx,
                        )
                    })
                }),
        )
        .child(menu(
            id("zoom-presets"),
            theme,
            YueButton::icon_text(
                id("zoom-value"),
                theme,
                crate::outline::ChevronDown,
                format!("{:.0}%", zoom * 100.),
            )
            .icon_position(crate::IconPosition::Trailing)
            .width(gpui_kit::rems(4.5))
            .accessibility_label("缩放倍率预设")
            .disabled(disabled),
            "缩放倍率",
            zoom_items,
        ))
        .child(
            div()
                .h_4()
                .border_l_1()
                .border_color(gpui_kit::rgb(theme.border_default()))
                .mx_1(),
        )
        .child(
            fit_button(
                id("fit-all"),
                theme,
                crate::outline::ArrowsMaximize,
                "适应窗口",
                fit_current,
                disabled,
            )
            .tooltip("适应窗口（0）")
            .on_click(move |_, w, cx| {
                fit.update(cx, |s, cx| {
                    s.user_fit(
                        ImagePreviewFit::FitAll,
                        false,
                        ImagePreviewChangeSource::Toolbar,
                        w,
                        cx,
                    )
                })
            }),
        )
        .child(
            fit_button(
                id("fit-width"),
                theme,
                crate::outline::ArrowsHorizontal,
                "适应宽度",
                width_current,
                disabled,
            )
            .tooltip("适应宽度（W）")
            .on_click(move |_, w, cx| {
                fit_width.update(cx, |s, cx| {
                    s.user_fit(
                        ImagePreviewFit::FitWidth,
                        false,
                        ImagePreviewChangeSource::Toolbar,
                        w,
                        cx,
                    )
                })
            }),
        )
        .child(menu(
            id("keyboard-menu"),
            theme,
            YueButton::icon(
                id("keyboard-trigger"),
                theme,
                crate::outline::Keyboard,
                "键盘操作",
            )
            .tooltip("键盘操作"),
            "键盘操作",
            help_items,
        ));
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_3()
        .child(left)
        .child(gallery_navigation(state, theme, cx))
        .child(right)
        .into_any_element()
}

fn gallery_navigation(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    cx: &App,
) -> AnyElement {
    let s = state.read(cx);
    let count = s.gallery().len();
    if count <= 1 {
        return div().into_any_element();
    }
    let index = s.gallery_index();
    let prev = state.clone();
    let next = state.clone();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .child(
            YueButton::icon(
                s.child_id("gallery-previous"),
                theme,
                crate::outline::ChevronLeft,
                "上一张",
            )
            .size(ButtonSize::Small)
            .variant(ButtonVariant::Ghost)
            .disabled(s.disabled || index == 0)
            .on_click(move |_, w, cx| {
                prev.update(cx, |s, cx| {
                    let _ = s.select_gallery_item(s.gallery_index().saturating_sub(1), w, cx);
                })
            }),
        )
        .child(
            div()
                .id(s.child_id("gallery-count"))
                .test_support()
                .aria_label(format!("第 {} 张，共 {} 张", index + 1, count))
                .text_sm()
                .font_features(crate::tabular_nums())
                .child(format!("{} / {count}", index + 1)),
        )
        .child(
            YueButton::icon(
                s.child_id("gallery-next"),
                theme,
                crate::outline::ChevronRight,
                "下一张",
            )
            .size(ButtonSize::Small)
            .variant(ButtonVariant::Ghost)
            .disabled(s.disabled || index + 1 == count)
            .on_click(move |_, w, cx| {
                next.update(cx, |s, cx| {
                    let _ = s.select_gallery_item(
                        (s.gallery_index() + 1).min(s.gallery().len() - 1),
                        w,
                        cx,
                    );
                })
            }),
        )
        .into_any_element()
}

pub(super) fn gallery_sides(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    cx: &App,
) -> AnyElement {
    let s = state.read(cx);
    let index = s.gallery_index();
    let count = s.gallery().len();
    let prev = state.clone();
    let next = state.clone();
    let surface = || {
        div()
            .absolute()
            .top(gpui_kit::relative(0.5))
            .mt(-rems(theme, "--button-height-lg") / 2.)
            .rounded_full()
            .border_1()
            .border_color(crate::tokens::rgba(theme, "--image-preview-glass-border"))
            .bg(crate::tokens::rgba(
                theme,
                "--image-preview-glass-background",
            ))
            .backdrop_blur(rems(theme, "--image-preview-glass-blur"))
    };
    let controls_theme =
        crate::YueTheme::new(crate::ThemeMode::Dark, theme.accent()).with_neutral(theme.neutral());
    div()
        .absolute()
        .inset_0()
        .when(index > 0, |d| {
            d.child(
                surface().left_3().child(
                    YueButton::icon(
                        s.child_id("gallery-side-previous"),
                        controls_theme,
                        crate::outline::ChevronLeft,
                        "上一张图片",
                    )
                    .size(ButtonSize::Large)
                    .shape(crate::ButtonShape::Circle)
                    .variant(ButtonVariant::Ghost)
                    .disabled(s.disabled)
                    .on_click(move |_, w, cx| {
                        prev.update(cx, |s, cx| {
                            let _ =
                                s.select_gallery_item(s.gallery_index().saturating_sub(1), w, cx);
                        })
                    }),
                ),
            )
        })
        .when(index + 1 < count, |d| {
            d.child(
                surface().right_3().child(
                    YueButton::icon(
                        s.child_id("gallery-side-next"),
                        controls_theme,
                        crate::outline::ChevronRight,
                        "下一张图片",
                    )
                    .size(ButtonSize::Large)
                    .shape(crate::ButtonShape::Circle)
                    .variant(ButtonVariant::Ghost)
                    .disabled(s.disabled)
                    .on_click(move |_, w, cx| {
                        next.update(cx, |s, cx| {
                            let _ = s.select_gallery_item(
                                (s.gallery_index() + 1).min(s.gallery().len() - 1),
                                w,
                                cx,
                            );
                        })
                    }),
                ),
            )
        })
        .into_any_element()
}
