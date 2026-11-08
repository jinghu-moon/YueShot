use super::state::Gesture;
use super::*;
use crate::{
    ButtonSize, ButtonVariant, FeedbackSize, SegmentedDistribution, SegmentedSize, YueButton,
    YueSegmented, YueSegmentedOption, YueSpinner,
    motion::{MotionDuration, MotionEase},
    tokens::{number, pixels, rems, rgba, shadow},
};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{
    InteractiveElement as _, MouseButton, Orientation, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, canvas, div, prelude::FluentBuilder as _, px,
};
use std::time::Instant;

struct Mount {
    _observe: gpui_kit::Subscription,
    _release: gpui_kit::Subscription,
}
impl RenderOnce for YueImagePreview {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let state = self.state;
        let mounted = state.clone();
        let mount_id = state.read(cx).child_id("mount");
        w.use_keyed_state(mount_id, cx, move |w, cx| {
            let observe = cx.observe(&mounted, |_, _, cx| cx.notify());
            let weak = mounted.downgrade();
            let handle = w.window_handle();
            let release = cx.on_release(move |_: &mut Mount, cx| {
                let _ = handle.update(cx, |_, w, cx| {
                    let _ = weak.update(cx, |s, _| {
                        s.cancel_gesture(w);
                        s.finish_motion();
                        s.mounted = false;
                    });
                });
            });
            Mount {
                _observe: observe,
                _release: release,
            }
        });
        state.update(cx, |s, cx| {
            s.theme = theme;
            s.animated = self.animated;
            s.disabled = self.disabled;
            s.mounted = true;
            if self.disabled {
                s.cancel_gesture(w);
                s.finish_motion();
                if s.focus.is_focused(w) || s.grip_focus.is_focused(w) {
                    w.blur(cx);
                }
            }
            if !self.animated {
                s.finish_motion();
            }
        });
        let s = state.read(cx);
        let gallery = s.gallery().to_vec();
        let gallery_index = s.gallery_index();
        let filmstrip_visible = s.filmstrip_visible();
        let center_filmstrip = gallery.len() as f32 * 88. + 16. < s.viewport.size[0];
        let filmstrip_id = s.child_id("filmstrip");
        let bottom_toolbar_id = s.child_id("bottom-toolbar");
        let titlebar_id = s.child_id("titlebar");
        let title_fullscreen_id = s.child_id("title-fullscreen");
        let gallery_previous_id = s.child_id("gallery-previous");
        let gallery_next_id = s.child_id("gallery-next");
        let gallery_count_id = s.child_id("gallery-count");
        let title_image = s.source().cloned().or_else(|| s.reference().cloned());
        let id = s.id.clone();
        let viewport_id = s.child_id("viewport");
        let mode_id = s.child_id("modes");
        let focus = s.focus.clone();
        let mode = s.mode;
        let source_available = s.mode_available(ImagePreviewMode::Source);
        let compare_available = s.mode_available(ImagePreviewMode::Compare);
        let result_available = s.mode_available(ImagePreviewMode::Result);
        let disabled = self.disabled;
        let (status, busy, error) = s.status();
        let description = s
            .reference()
            .map(|i| i.description.clone())
            .unwrap_or_else(|| "图片预览".into());
        let focus_visible = focus.is_focused(w) && w.last_input_was_keyboard();
        let mut viewport = div()
            .id(viewport_id)
            .test_support()
            .role(Role::Group)
            .aria_label("图片预览")
            .aria_description(description)
            .relative()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .when(!disabled, |d| d.track_focus(&focus))
            .key_context("YueImagePreview")
            .child(paint::viewport(&state, theme, self.overlay));
        let gallery_owner = state.clone();
        let gallery_owner_leave = state.clone();
        let gallery_owner_strip = state.clone();
        viewport = viewport
            .on_mouse_move(move |_, w, cx| {
                gallery_owner.update(cx, |s, cx| {
                    let bottom = s.viewport.origin[1] + s.viewport.size[1];
                    let y = f32::from(w.mouse_position().y);
                    let clearance = if s.filmstrip_visible() { 104. } else { 24. };
                    s.update_filmstrip_pointer(y >= bottom - clearance, w, cx);
                });
            })
            .on_hover(move |hovered, w, cx| {
                if !hovered {
                    gallery_owner_leave
                        .update(cx, |s, cx| s.update_filmstrip_pointer(false, w, cx));
                }
            });
        macro_rules! action {
            ($view:ident,$ty:ty,$op:expr) => {{
                let owner = state.clone();
                $view = $view.on_action(move |_: &$ty, w, cx| {
                    owner.update(cx, |s, cx| {
                        if s.disabled || !s.focus.is_focused(w) {
                            cx.propagate();
                            return;
                        }
                        ($op)(s, w, cx);
                    });
                });
            }};
        }
        action!(viewport, ZoomIn, |s: &mut YueImagePreviewState,
                                   w: &mut Window,
                                   cx: &mut gpui_kit::Context<
            YueImagePreviewState,
        >| s.user_zoom(
            1.25,
            s.viewport.midpoint(),
            true,
            ImagePreviewChangeSource::Keyboard,
            w,
            cx
        ));
        action!(viewport, ZoomOut, |s: &mut YueImagePreviewState,
                                    w: &mut Window,
                                    cx: &mut gpui_kit::Context<
            YueImagePreviewState,
        >| s.user_zoom(
            1. / 1.25,
            s.viewport.midpoint(),
            true,
            ImagePreviewChangeSource::Keyboard,
            w,
            cx
        ));
        action!(viewport, ActualSize, |s: &mut YueImagePreviewState,
                                       w: &mut Window,
                                       cx: &mut gpui_kit::Context<
            YueImagePreviewState,
        >| s.user_fit(
            ImagePreviewFit::Manual,
            true,
            ImagePreviewChangeSource::Keyboard,
            w,
            cx
        ));
        action!(viewport, FitAll, |s: &mut YueImagePreviewState,
                                   w: &mut Window,
                                   cx: &mut gpui_kit::Context<
            YueImagePreviewState,
        >| s.user_fit(
            ImagePreviewFit::FitAll,
            false,
            ImagePreviewChangeSource::Keyboard,
            w,
            cx
        ));
        action!(viewport, FitWidth, |s: &mut YueImagePreviewState,
                                     w: &mut Window,
                                     cx: &mut gpui_kit::Context<
            YueImagePreviewState,
        >| s.user_fit(
            ImagePreviewFit::FitWidth,
            false,
            ImagePreviewChangeSource::Keyboard,
            w,
            cx
        ));
        let owner = state.clone();
        viewport = viewport.on_action(move |_: &Fullscreen, w, cx| {
            if owner.read(cx).disabled || !owner.read(cx).focus.is_focused(w) {
                cx.propagate();
                return;
            }
            w.toggle_fullscreen();
        });
        viewport = viewport.on_action(move |_: &ExitFullscreen, w, cx| {
            if !w.is_fullscreen() {
                cx.propagate();
                return;
            }
            w.toggle_fullscreen();
        });
        macro_rules! pan_action {
            ($ty:ty,$dx:expr,$dy:expr) => {
                action!(viewport, $ty, |s: &mut YueImagePreviewState,
                                        w: &mut Window,
                                        cx: &mut gpui_kit::Context<
                    YueImagePreviewState,
                >| {
                    let step =
                        pixels(s.theme, "--image-preview-pan-step") * f32::from(w.rem_size()) / 16.
                            * if w.modifiers().shift { 4. } else { 1. };
                    let direction = [$dx, $dy];
                    s.user_pan(direction.map(|v| v * step), w, cx);
                });
            };
        }
        pan_action!(PanLeft, 1., 0.);
        pan_action!(PanRight, -1., 0.);
        pan_action!(PanUp, 0., 1.);
        pan_action!(PanDown, 0., -1.);
        let owner = state.clone();
        viewport = viewport.on_key_down(move |e, w, cx| {
            if e.keystroke.key == "escape" && owner.read(cx).gesture.is_some() {
                owner.update(cx, |s, cx| {
                    s.cancel_gesture(w);
                    cx.notify();
                });
                cx.stop_propagation();
                w.prevent_default();
            }
        });
        if mode == ImagePreviewMode::Compare && !disabled {
            viewport = viewport.child(grip(&state, theme, w, cx));
        }
        if !status.is_empty() {
            let status_id = state.read(cx).child_id("status");
            let retry_id = state.read(cx).child_id("retry");
            let spinner_id = state.read(cx).child_id("spinner");
            let owner = state.clone();
            let notice = div()
                .id(status_id)
                .test_support()
                .role(Role::Status)
                .aria_label(status.clone())
                .aria_value(status.clone())
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .rounded(rems(theme, "--image-preview-radius"))
                .bg(gpui_kit::rgb(theme.surface()))
                .text_color(gpui_kit::rgb(theme.text_primary()))
                .text_sm()
                .when(busy, |d| {
                    d.child(
                        YueSpinner::new(spinner_id)
                            .size(FeedbackSize::Small)
                            .decorative(true)
                            .animated(self.animated),
                    )
                })
                .child(status)
                .when(error, |d| {
                    d.child(
                        YueButton::new(retry_id, "重试")
                            .size(ButtonSize::Small)
                            .disabled(disabled)
                            .on_click(move |_, _, cx| {
                                owner.update(cx, |s, cx| s.retry(cx));
                            }),
                    )
                });
            let notice = gpui_kit::base::A11yLive::new(notice, gpui_kit::accesskit::Live::Polite)
                .atomic(true)
                .busy(busy);
            viewport = viewport.child(div().absolute().top_3().left_3().right_3().child(notice));
        }
        let mut footer = div()
            .flex()
            .items_center()
            .gap_1()
            .text_sm()
            .text_color(gpui_kit::rgb(theme.text_secondary()));
        let mut modes = None;
        if self.modes && (result_available || compare_available) {
            let owner = state.clone();
            modes = Some(
                YueSegmented::new(mode_id, theme, mode.value())
                    .size(SegmentedSize::Small)
                    .accessibility_label("预览模式")
                    .distribution(SegmentedDistribution::Equal)
                    .disabled(disabled)
                    .options([
                        YueSegmentedOption::new("source", "原图").disabled(!source_available),
                        YueSegmentedOption::new("compare", "对比").disabled(!compare_available),
                        YueSegmentedOption::new("result", self.result_label)
                            .disabled(!result_available),
                    ])
                    .on_change(move |v, w, cx| {
                        let mode = match v.as_ref() {
                            "compare" => ImagePreviewMode::Compare,
                            "result" => ImagePreviewMode::Result,
                            _ => ImagePreviewMode::Source,
                        };
                        owner.update(cx, |s, cx| s.user_mode(mode, w, cx));
                    }),
            );
        }
        if s.has_proxy() {
            footer = footer.child(
                div()
                    .id(s.child_id("proxy-info"))
                    .test_support()
                    .aria_label("代理预览；100% 按原始尺寸，细节已缩减")
                    .child("代理预览；100% 按原始尺寸，细节已缩减"),
            );
        }
        if s.reference()
            .is_some_and(|i| i.color_space() == ImagePreviewColorSpace::ScRgb)
            || (mode == ImagePreviewMode::Compare
                && s.source()
                    .is_some_and(|i| i.color_space() == ImagePreviewColorSpace::ScRgb))
        {
            let label = if w.color_output().mode == gpui_kit::WindowColorMode::HdrScRgb {
                "HDR"
            } else {
                "SDR预览 · HDR原图保留"
            };
            footer = footer.child(
                div()
                    .id(s.child_id("hdr-info"))
                    .test_support()
                    .aria_label(label)
                    .child(label),
            );
        }
        if let Some(renderer) = self.footer {
            footer = footer.child(renderer(w, cx));
        }
        let mut outer = div()
            .id(id)
            .test_support()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(rems(theme, "--image-preview-radius"))
            .border_1()
            .border_color(gpui_kit::rgb(if focus_visible {
                theme.focus_ring()
            } else {
                theme.border_default()
            }))
            .bg(gpui_kit::rgb(theme.surface()))
            .overflow_hidden();
        let mut bottom_toolbar = None;
        {
            let title = title_image
                .as_ref()
                .map(|image| image.description.clone())
                .unwrap_or_else(|| "图片预览".into());
            let dimensions = title_image.as_ref().map(ImagePreviewImage::pixel_size);
            outer = outer.child(
                div()
                    .id(titlebar_id)
                    .test_support()
                    .role(Role::Toolbar)
                    .aria_label("图片标题栏")
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .w_full()
                    .h(gpui_kit::rems(3.))
                    .flex_shrink_0()
                    .px_3()
                    .border_b_1()
                    .border_color(gpui_kit::rgb(theme.border_default()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(gpui_kit::rgb(theme.text_primary()))
                            .when_some(dimensions, |d, [width, height]| {
                                d.child(
                                    crate::Icon::new(crate::outline::Photo)
                                        .size(gpui_kit::rems(1.))
                                        .color(theme.text_secondary()),
                                )
                                .child(div().min_w_0().truncate().child(title))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(gpui_kit::rgb(theme.text_secondary()))
                                        .child(format!("{width} × {height}")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .children(modes)
                            .child(chrome::information_button(&state, theme, w, cx))
                            .child(
                                YueButton::icon(
                                    title_fullscreen_id,
                                    theme,
                                    crate::outline::ArrowsMaximize,
                                    if w.is_fullscreen() {
                                        "退出全屏"
                                    } else {
                                        "全屏"
                                    },
                                )
                                .size(ButtonSize::Small)
                                .variant(ButtonVariant::Ghost)
                                .tooltip(if w.is_fullscreen() {
                                    "退出全屏（Esc）"
                                } else {
                                    "全屏（F）"
                                })
                                .on_click(|_, w, _| w.toggle_fullscreen()),
                            ),
                    ),
            );
        }
        if self.toolbar_visible {
            let toolbar = if self.toolbar == ImagePreviewToolbar::Floating {
                zoom_toolbar(
                    &state,
                    theme,
                    true,
                    self.background_picker,
                    self.on_expand,
                    w,
                    cx,
                )
            } else {
                chrome::footer_toolbar(&state, theme, self.background_picker, self.on_expand, w, cx)
            };
            if self.toolbar == ImagePreviewToolbar::Floating {
                viewport = viewport.child(div().absolute().bottom_3().right_3().child(toolbar));
            } else {
                bottom_toolbar = Some(toolbar);
            }
        }
        if gallery.len() > 1 && bottom_toolbar.is_none() {
            bottom_toolbar = Some(div().into_any_element());
        }
        if gallery.len() > 1 {
            let mut strip = div()
                .id(filmstrip_id)
                .test_support()
                .role(Role::Toolbar)
                .aria_label("图片胶片条")
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .items_center()
                .gap_1()
                .p_2()
                .when(center_filmstrip, |d| d.justify_center())
                .bg(gpui_kit::rgb(theme.surface()))
                .border_t_1()
                .border_color(gpui_kit::rgb(theme.border_default()))
                .overflow_x_scroll()
                .on_hover(move |hovered, w, cx| {
                    if !hovered {
                        gallery_owner_strip
                            .update(cx, |s, cx| s.update_filmstrip_pointer(false, w, cx));
                    }
                });
            for (index, document) in gallery.iter().enumerate() {
                let selected = index == gallery_index;
                let owner = state.clone();
                let image = document.source.ready().cloned();
                let mut item = div()
                    .id(format!("filmstrip-item-{index}"))
                    .test_support()
                    .role(Role::Button)
                    .aria_label(format!(
                        "第 {} 张：{}{}",
                        index + 1,
                        document.id,
                        if selected { "，当前图片" } else { "" }
                    ))
                    .tab_stop(true)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .w(px(84.))
                    .h(px(76.))
                    .flex_shrink_0()
                    .rounded(rems(theme, "--image-preview-radius"))
                    .border_1()
                    .border_color(gpui_kit::rgb(if selected {
                        theme.accent_solid()
                    } else {
                        theme.border_default()
                    }))
                    .bg(gpui_kit::rgb(if selected {
                        theme.accent_subtle()
                    } else {
                        theme.surface_component()
                    }))
                    .on_click(move |_, w, cx| {
                        owner.update(cx, |s, cx| {
                            let _ = s.select_gallery_item(index, w, cx);
                        });
                    });
                if let Some(image) = image {
                    item = item.child(gallery_thumbnail(image));
                } else {
                    item = item.child(div().w(px(68.)).h(px(48.)).child("图片"));
                }
                item = item.child(div().text_xs().child(format!("{}", index + 1)));
                strip = strip.child(item);
            }
            if filmstrip_visible {
                viewport = viewport.child(strip);
            }
            viewport = viewport.child(chrome::gallery_sides(&state, theme, cx));
        }
        viewport = viewport.child(chrome::information_panel(&state, theme, w, cx));
        outer = outer.child(viewport);
        if let Some(toolbar) = bottom_toolbar {
            outer = outer.child(
                div()
                    .id(bottom_toolbar_id)
                    .test_support()
                    .role(Role::Toolbar)
                    .aria_label("图片工具栏")
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .min_h(gpui_kit::rems(3.25))
                    .flex_shrink_0()
                    .gap_3()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(gpui_kit::rgb(theme.border_default()))
                    .when(
                        gallery.len() > 1 && self.toolbar == ImagePreviewToolbar::Floating,
                        |d| {
                            let prev = state.clone();
                            let next = state.clone();
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        YueButton::icon(
                                            gallery_previous_id,
                                            theme,
                                            crate::outline::ChevronLeft,
                                            "上一张",
                                        )
                                        .size(ButtonSize::Small)
                                        .variant(ButtonVariant::Ghost)
                                        .disabled(disabled || gallery_index == 0)
                                        .on_click(
                                            move |_, w, cx| {
                                                prev.update(cx, |s, cx| {
                                                    let _ = s.select_gallery_item(
                                                        s.gallery_index().saturating_sub(1),
                                                        w,
                                                        cx,
                                                    );
                                                });
                                            },
                                        ),
                                    )
                                    .child(
                                        div()
                                            .id(gallery_count_id)
                                            .test_support()
                                            .aria_label(format!(
                                                "第 {} 张，共 {} 张",
                                                gallery_index + 1,
                                                gallery.len()
                                            ))
                                            .font_features(crate::tabular_nums())
                                            .text_sm()
                                            .child(format!(
                                                "{} / {}",
                                                gallery_index + 1,
                                                gallery.len()
                                            )),
                                    )
                                    .child(
                                        YueButton::icon(
                                            gallery_next_id,
                                            theme,
                                            crate::outline::ChevronRight,
                                            "下一张",
                                        )
                                        .size(ButtonSize::Small)
                                        .variant(ButtonVariant::Ghost)
                                        .disabled(disabled || gallery_index + 1 >= gallery.len())
                                        .on_click(
                                            move |_, w, cx| {
                                                next.update(cx, |s, cx| {
                                                    let _ = s.select_gallery_item(
                                                        (s.gallery_index() + 1)
                                                            .min(s.gallery().len() - 1),
                                                        w,
                                                        cx,
                                                    );
                                                });
                                            },
                                        ),
                                    ),
                            )
                        },
                    )
                    .child(div().flex_1().min_w_0().child(toolbar))
                    .child(footer),
            );
        } else {
            outer = outer.child(footer);
        }
        let chrome = chrome::chrome_state(&state, w, cx);
        outer
            .key_context("YueImagePreview")
            .on_key_down(move |event, window, cx| {
                if event.keystroke.key == "escape"
                    && chrome::dismiss_information(&chrome, window, cx)
                {
                    cx.stop_propagation();
                    window.prevent_default();
                } else if event.keystroke.key == "escape" && window.is_fullscreen() {
                    window.toggle_fullscreen();
                    cx.stop_propagation();
                    window.prevent_default();
                }
            })
    }
}

fn gallery_thumbnail(image: ImagePreviewImage) -> AnyElement {
    canvas(
        |bounds, _, _| bounds,
        move |bounds, _, window, _| {
            let [width, height] = image.dimensions();
            let scale =
                (f32::from(bounds.size.width) / width).min(f32::from(bounds.size.height) / height);
            let size = gpui_kit::size(px(width * scale), px(height * scale));
            let origin = bounds.origin
                + gpui_kit::point(
                    (bounds.size.width - size.width) / 2.,
                    (bounds.size.height - size.height) / 2.,
                );
            let _ = image.paint(
                bounds,
                gpui_kit::Bounds::new(origin, size),
                px(2.),
                ImagePreviewSampling::Smooth,
                window,
            );
        },
    )
    .w(px(68.))
    .h(px(48.))
    .into_any_element()
}
fn bg_value(bg: ImagePreviewBackground) -> &'static str {
    match bg {
        ImagePreviewBackground::CheckerDark => "dark",
        ImagePreviewBackground::CheckerLight => "light",
        ImagePreviewBackground::White => "white",
        ImagePreviewBackground::Black => "black",
    }
}
struct ToolbarInteraction {
    focus: gpui_kit::FocusHandle,
    hovered: bool,
    menu_open: bool,
    _subscriptions: [gpui_kit::Subscription; 2],
}
fn zoom_toolbar(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    floating: bool,
    background_picker: bool,
    on_expand: Option<ExpandHandler>,
    w: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let s = state.read(cx);
    let available = s.reference().is_some();
    let disabled = s.disabled || !available;
    let zoom = s.reference().map_or(1., |i| {
        s.display_camera(Instant::now()).zoom(i.dimensions())
    });
    let percentage = if zoom < 0.01 {
        format!("{:.3}%", zoom * 100.)
    } else if zoom < 0.1 {
        format!("{:.1}%", zoom * 100.)
    } else {
        format!("{:.0}%", zoom * 100.)
    };
    let limits = s
        .reference()
        .map(|i| s.viewport.zoom_limits(i.dimensions()))
        .unwrap_or([0.05, 8.]);
    let zoom_enabled = [zoom > limits[0] + 0.000001, zoom < limits[1] - 0.000001];
    let fit_current = available && s.fit == ImagePreviewFit::FitAll;
    let width_current = available && s.fit == ImagePreviewFit::FitWidth;
    let actual_current = s.fit == ImagePreviewFit::Manual
        && s.reference()
            .is_some_and(|i| (s.camera.zoom(i.dimensions()) - 1.).abs() < 0.000001);
    let panning = matches!(s.gesture, Some(Gesture::Pan { moved: true, .. }));
    let animated = s.animated && w.is_visible() && w.is_window_active();
    let toolbar_id = s.child_id("zoom-toolbar");
    let interaction_id = s.child_id("toolbar-interaction");
    let fade_id = s.child_id("toolbar-opacity");
    let interaction = w.use_keyed_state(interaction_id, cx, |w, cx| {
        let focus = cx.focus_handle().tab_stop(false);
        let focus_in = cx.on_focus_in(&focus, w, |_, w, cx| {
            cx.notify();
            w.refresh();
        });
        let focus_out = cx.on_focus_out(&focus, w, |_, _, w, cx| {
            cx.notify();
            w.refresh();
        });
        ToolbarInteraction {
            focus,
            hovered: false,
            menu_open: false,
            _subscriptions: [focus_in, focus_out],
        }
    });
    let focus = interaction.read(cx).focus.clone();
    let focused = focus.contains_focused(w, cx);
    let dim = floating
        && !disabled
        && panning
        && !interaction.read(cx).hovered
        && !interaction.read(cx).menu_open
        && !focused;
    let duration = if animated && !focused {
        MotionDuration::Interaction.duration(theme)
    } else {
        std::time::Duration::ZERO
    };
    let opacity = gpui_kit::base::transition(
        fade_id,
        if dim {
            number(theme, "--image-preview-toolbar-opacity-dragged").clamp(0., 1.)
        } else {
            1.
        },
        gpui_kit::base::Transition::new(duration).ease(MotionEase::Fade.easing(theme)),
        w,
        cx,
    );
    let controls_theme = if floating {
        crate::YueTheme::new(crate::ThemeMode::Dark, theme.accent()).with_neutral(theme.neutral())
    } else {
        theme
    };
    let id = |name| state.read(cx).child_id(name);
    let out = state.clone();
    let actual = state.clone();
    let inside = state.clone();
    let fit = state.clone();
    let fit_width = state.clone();
    let more = more_menu(state, controls_theme, background_picker, &interaction, cx);
    let mut toolbar = div()
        .id(toolbar_id)
        .test_support()
        .role(Role::Toolbar)
        .aria_label("缩放工具条")
        .track_focus(&focus)
        .on_hover(w.listener_for(&interaction, |s, hovered, w, cx| {
            if s.hovered != *hovered {
                s.hovered = *hovered;
                cx.notify();
                w.refresh();
            }
        }))
        .opacity(opacity)
        .flex()
        .items_center()
        .gap_1()
        .font_features(crate::tabular_nums())
        .p_1()
        .rounded(rems(theme, "--image-preview-radius"))
        .bg(gpui_kit::rgb(theme.surface()))
        .border_1()
        .border_color(gpui_kit::rgb(theme.border_default()))
        .when(floating, |el| {
            el.backdrop_blur(rems(theme, "--image-preview-glass-blur"))
                .bg(rgba(theme, "--image-preview-glass-background"))
                .border_color(rgba(theme, "--image-preview-glass-border"))
                .text_color(rgba(theme, "--image-preview-glass-color"))
                .shadow(shadow(theme, "--image-preview-glass-shadow"))
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, w, cx| {
            cx.stop_propagation();
            w.prevent_default();
        })
        .child(
            YueButton::icon(
                id("zoom-out"),
                controls_theme,
                crate::outline::ZoomOut,
                "缩小",
            )
            .size(ButtonSize::Small)
            .variant(ButtonVariant::Ghost)
            .disabled(disabled || !zoom_enabled[0])
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
            YueButton::new(id("actual-size"), percentage)
                .theme(controls_theme)
                .accessibility_label("回到100%")
                .accessibility_description(if actual_current {
                    "当前：100%；一个图像像素对应一个屏幕物理像素"
                } else {
                    "回到100%；一个图像像素对应一个屏幕物理像素"
                })
                .size(ButtonSize::Small)
                .variant(if actual_current {
                    ButtonVariant::Outline
                } else {
                    ButtonVariant::Ghost
                })
                .width(gpui_kit::rems(4.5))
                .disabled(disabled)
                .tooltip(if actual_current {
                    "当前：100%（1）"
                } else {
                    "100%：一个图像像素对应一个屏幕物理像素（1）"
                })
                .on_click(move |_, w, cx| {
                    actual.update(cx, |s, cx| {
                        s.user_fit(
                            ImagePreviewFit::Manual,
                            true,
                            ImagePreviewChangeSource::Toolbar,
                            w,
                            cx,
                        )
                    })
                }),
        )
        .child(
            YueButton::icon(
                id("zoom-in"),
                controls_theme,
                crate::outline::ZoomIn,
                "放大",
            )
            .size(ButtonSize::Small)
            .variant(ButtonVariant::Ghost)
            .disabled(disabled || !zoom_enabled[1])
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
        .child(
            YueButton::icon(
                id("fit-all"),
                controls_theme,
                crate::outline::ArrowsMaximize,
                "适应窗口",
            )
            .accessibility_description(if fit_current {
                "当前：适应窗口；自动随视口调整"
            } else {
                "适应窗口；自动随视口调整"
            })
            .size(ButtonSize::Small)
            .variant(if fit_current {
                ButtonVariant::Outline
            } else {
                ButtonVariant::Ghost
            })
            .disabled(disabled)
            .tooltip(if fit_current {
                "当前：适应窗口（0）"
            } else {
                "适应窗口（0）"
            })
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
            YueButton::icon(
                id("fit-width"),
                controls_theme,
                crate::outline::ArrowsHorizontal,
                "适应宽度",
            )
            .size(ButtonSize::Small)
            .variant(if width_current {
                ButtonVariant::Outline
            } else {
                ButtonVariant::Ghost
            })
            .accessibility_description(if width_current {
                "当前：适应宽度；长图从顶部开始查看"
            } else {
                "适应宽度；长图从顶部开始查看"
            })
            .disabled(disabled)
            .tooltip(if width_current {
                "当前：适应宽度（W）"
            } else {
                "适应宽度（W）"
            })
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
        );
    if let Some(expand) = on_expand {
        let owner = state.clone();
        toolbar = toolbar.child(
            YueButton::icon(
                id("expand"),
                controls_theme,
                crate::outline::ArrowsDiagonal,
                "放大预览",
            )
            .size(ButtonSize::Small)
            .variant(ButtonVariant::Ghost)
            .disabled(disabled)
            .tooltip("放大预览")
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
    toolbar.child(more).into_any_element()
}

fn more_menu(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    background_picker: bool,
    interaction: &Entity<ToolbarInteraction>,
    cx: &App,
) -> crate::YueDropdownMenu {
    use crate::{DropdownAlign, DropdownPlacement, YueDropdownMenu, YueMenuItem};
    let s = state.read(cx);
    let disabled = s.disabled || s.reference().is_none();
    let mut items = Vec::new();
    for (sampling, label, id) in [
        (ImagePreviewSampling::Smooth, "平滑显示", "sampling-smooth"),
        (
            ImagePreviewSampling::Nearest,
            "像素显示",
            "sampling-nearest",
        ),
    ] {
        let owner = state.clone();
        items.push(
            YueMenuItem::new(label)
                .id(id)
                .radio(s.sampling == sampling)
                .disabled(
                    sampling == ImagePreviewSampling::Nearest
                        && (!cfg!(target_os = "windows") || s.has_proxy()),
                )
                .on_click(move |_, _, cx| owner.update(cx, |s, cx| s.user_sampling(sampling, cx))),
        );
    }
    items.push(YueMenuItem::separator());
    if background_picker {
        items.push(YueMenuItem::heading("预览背景"));
        for background in [
            ImagePreviewBackground::CheckerDark,
            ImagePreviewBackground::CheckerLight,
            ImagePreviewBackground::White,
            ImagePreviewBackground::Black,
        ] {
            let owner = state.clone();
            items.push(
                YueMenuItem::new(background.label())
                    .id(bg_value(background))
                    .radio(s.background == background)
                    .on_click(move |_, _, cx| {
                        owner.update(cx, |s, cx| s.user_background(background, cx))
                    }),
            );
        }
        items.push(YueMenuItem::separator());
    }
    let owner = state.clone();
    let can_grid = s.is_pixel_grid_available();
    items.push(
        YueMenuItem::new("像素网格（≥400%）")
            .id("pixel-grid")
            .checked(s.pixel_grid)
            .disabled(!can_grid)
            .on_click(move |_, _, cx| {
                owner.update(cx, |s, cx| s.user_pixel_grid(!s.pixel_grid, cx))
            }),
    );
    if s.mode == ImagePreviewMode::Compare {
        let owner = state.clone();
        items.push(YueMenuItem::separator());
        items.push(
            YueMenuItem::new("对比归中（50%）")
                .id("center-comparison")
                .on_click(move |_, _, cx| {
                    owner.update(cx, |s, cx| {
                        s.user_split(0.5, ImagePreviewChangeSource::Toolbar, cx)
                    })
                }),
        );
    }
    let owner = interaction.clone();
    YueDropdownMenu::new(
        s.child_id("more-menu"),
        theme,
        YueButton::icon(
            s.child_id("more-trigger"),
            theme,
            crate::outline::Dots,
            "更多预览操作",
        )
        .size(ButtonSize::Small)
        .variant(ButtonVariant::Ghost)
        .disabled(disabled)
        .tooltip("更多预览操作"),
    )
    .label("更多预览操作")
    .placement(DropdownPlacement::Above)
    .align(DropdownAlign::End)
    .items(items)
    .on_open_change(move |open, w, cx| {
        owner.update(cx, |s, cx| {
            if s.menu_open != *open {
                s.menu_open = *open;
                cx.notify();
                w.refresh();
            }
        })
    })
}
fn grip(
    state: &Entity<YueImagePreviewState>,
    theme: crate::YueTheme,
    w: &Window,
    cx: &App,
) -> AnyElement {
    let s = state.read(cx);
    let split = s.display_split(Instant::now());
    let value = s.split;
    let diameter = rems(theme, "--image-preview-grip-size").to_pixels(w.rem_size());
    let width = px(s.viewport.size[0]);
    let x = (width * split - diameter / 2.)
        .max(px(0.))
        .min((width - diameter).max(px(0.)));
    let id = s.child_id("split");
    let focus = s.grip_focus.clone();
    let keyboard = focus.is_focused(w) && w.last_input_was_keyboard();
    let down = state.clone();
    let key = state.clone();
    let increment = state.clone();
    let decrement = state.clone();
    let set_value = state.clone();
    div()
        .id(id)
        .test_support()
        .role(Role::Slider)
        .aria_label("对比分割线")
        .aria_min_numeric_value(0.)
        .aria_max_numeric_value(100.)
        .aria_numeric_value((value * 100.) as f64)
        .aria_numeric_value_step(2.)
        .aria_orientation(Orientation::Horizontal)
        .aria_value(format!("原图占比{:.0}%", value * 100.))
        .absolute()
        .left(x)
        .top(px((s.viewport.size[1] - f32::from(diameter)) / 2.))
        .size(diameter)
        .rounded_full()
        .backdrop_blur(rems(theme, "--image-preview-glass-blur"))
        .shadow(shadow(theme, "--image-preview-glass-shadow"))
        .border_1()
        .border_color(if keyboard {
            gpui_kit::rgb(theme.focus_ring())
        } else {
            rgba(theme, "--image-preview-grip-border")
        })
        .bg(rgba(theme, "--image-preview-grip-background"))
        .text_color(rgba(theme, "--image-preview-glass-color"))
        .flex()
        .items_center()
        .justify_center()
        .cursor_ew_resize()
        .track_focus(&focus)
        .key_context("YueImagePreviewSplit")
        .on_a11y_action(gpui_kit::accesskit::Action::Increment, move |_, w, cx| {
            increment.update(cx, |s, cx| {
                s.cancel_gesture(w);
                s.user_split(s.split + 0.02, ImagePreviewChangeSource::Accessibility, cx);
            });
        })
        .on_a11y_action(gpui_kit::accesskit::Action::Decrement, move |_, w, cx| {
            decrement.update(cx, |s, cx| {
                s.cancel_gesture(w);
                s.user_split(s.split - 0.02, ImagePreviewChangeSource::Accessibility, cx);
            });
        })
        .on_a11y_action(gpui_kit::accesskit::Action::SetValue, move |data, w, cx| {
            if let Some(gpui_kit::accesskit::ActionData::NumericValue(value)) = data {
                set_value.update(cx, |s, cx| {
                    if value.is_finite() {
                        s.cancel_gesture(w);
                        s.user_split(
                            (*value / 100.) as f32,
                            ImagePreviewChangeSource::Accessibility,
                            cx,
                        );
                    }
                });
            }
        })
        .child(
            crate::Icon::new(crate::outline::ArrowBarBoth)
                .size(crate::IconSize::Medium.rems(theme))
                .color(rgba(theme, "--image-preview-glass-color").into()),
        )
        .on_mouse_down(MouseButton::Left, move |e, w, cx| {
            down.update(cx, |s, cx| {
                if e.click_count == 2 {
                    s.cancel_gesture(w);
                    s.user_split(0.5, ImagePreviewChangeSource::Pointer, cx);
                } else if let Some(hit) = s.hitbox {
                    s.begin([e.position.x.into(), e.position.y.into()], true, hit, w, cx);
                }
            });
            cx.stop_propagation();
            w.prevent_default();
        })
        .on_key_down(move |e, w, cx| {
            if e.keystroke.modifiers.control
                || e.keystroke.modifiers.alt
                || e.keystroke.modifiers.platform
            {
                return;
            }
            let current = key.read(cx).split;
            let step = if e.keystroke.modifiers.shift {
                0.1
            } else {
                0.02
            };
            let next = match e.keystroke.key.as_str() {
                "left" => current - step,
                "right" => current + step,
                "home" => 0.,
                "end" => 1.,
                _ => return,
            };
            key.update(cx, |s, cx| {
                s.user_split(next, ImagePreviewChangeSource::Keyboard, cx)
            });
            cx.stop_propagation();
            w.prevent_default();
        })
        .into_any_element()
}
