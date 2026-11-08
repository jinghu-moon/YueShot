use super::{color::Color, state::YueColorPickerState};
use crate::{
    YueTheme,
    tokens::{pixels, rems, rgba},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Bounds, ColorSpace, ContentMask, Corners, Entity, InteractiveElement as _,
    IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement as _, Pixels,
    Point, Rgba, StatefulInteractiveElement as _, Styled as _, Window, canvas, div, fill,
    linear_color_stop, linear_gradient, point, px, size,
};
use std::{cell::Cell, rc::Rc};

pub(crate) fn checker(
    bounds: Bounds<Pixels>,
    light: Rgba,
    dark: Rgba,
    cell: Pixels,
    window: &mut Window,
) {
    checker_with_radius(bounds, light, dark, cell, px(0.), window);
}

fn checker_with_radius(
    bounds: Bounds<Pixels>,
    light: Rgba,
    dark: Rgba,
    cell: Pixels,
    radius: Pixels,
    window: &mut Window,
) {
    let step = f32::from(cell).max(1.);
    let scale = window.scale_factor();
    let cols = (f32::from(bounds.size.width) / step).ceil() as usize;
    let rows = (f32::from(bounds.size.height) / step).ceil() as usize;
    for y in 0..rows {
        for x in 0..cols {
            let left = (f32::from(bounds.left()) + x as f32 * step)
                .mul_add(scale, 0.)
                .round()
                / scale;
            let top = (f32::from(bounds.top()) + y as f32 * step)
                .mul_add(scale, 0.)
                .round()
                / scale;
            let right = ((f32::from(bounds.left()) + (x + 1) as f32 * step)
                .min(f32::from(bounds.right()))
                * scale)
                .round()
                / scale;
            let bottom = ((f32::from(bounds.top()) + (y + 1) as f32 * step)
                .min(f32::from(bounds.bottom()))
                * scale)
                .round()
                / scale;
            let tile = Bounds::new(
                point(px(left), px(top)),
                size(px((right - left).max(0.)), px((bottom - top).max(0.))),
            );
            let color = if (x + y) % 2 == 0 { light } else { dark };
            if radius > px(0.) {
                // The mask chooses a checker cell; the full rounded quad owns
                // the outer silhouette. Rounded divs do not clip Canvas paint.
                window.with_content_mask(Some(ContentMask { bounds: tile }), |window| {
                    window.paint_quad(fill(bounds, color).corner_radii(radius));
                });
            } else {
                window.paint_quad(fill(tile, color));
            }
        }
    }
}
pub(crate) fn preview(color: Rgba, theme: YueTheme, size_token: &'static str) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(rems(theme, size_token))
        .rounded(rems(theme, "--swatch-radius"))
        .overflow_hidden()
        .border_1()
        .border_color(rgba(theme, "--swatch-border-color"))
        .child(
            canvas(
                |bounds, _, _| bounds,
                move |_, bounds, window, _| {
                    let cell = px(pixels(theme, "--color-picker-checker-size")
                        * f32::from(window.rem_size())
                        / 16.);
                    checker(
                        bounds,
                        rgba(theme, "--color-picker-checker-light-color"),
                        rgba(theme, "--color-picker-checker-dark-color"),
                        cell,
                        window,
                    );
                    window.paint_quad(fill(bounds, color));
                },
            )
            .size_full(),
        )
        .into_any_element()
}
fn gradient(angle: f32, from: Rgba, to: Rgba) -> gpui_kit::Background {
    linear_gradient(
        angle,
        linear_color_stop(from, 0.),
        linear_color_stop(to, 1.),
    )
    .color_space(ColorSpace::Srgb)
}
fn hue_quad(bounds: Bounds<Pixels>, ix: usize) -> PaintQuad {
    let left = bounds.left() + bounds.size.width * (ix as f32 / 6.);
    let right = bounds.left() + bounds.size.width * ((ix + 1) as f32 / 6.);
    let c1 = Color::from_hsv([ix as f64 * 60., 1., 1.], 1.).rgba();
    let c2 = Color::from_hsv([(ix + 1) as f64 * 60., 1., 1.], 1.).rgba();
    fill(
        Bounds::new(
            point(left, bounds.top()),
            size(right - left, bounds.size.height),
        ),
        gradient(90., c1, c2),
    )
    .corner_radii(Corners {
        top_left: if ix == 0 {
            bounds.size.height / 2.
        } else {
            px(0.)
        },
        bottom_left: if ix == 0 {
            bounds.size.height / 2.
        } else {
            px(0.)
        },
        top_right: if ix == 5 {
            bounds.size.height / 2.
        } else {
            px(0.)
        },
        bottom_right: if ix == 5 {
            bounds.size.height / 2.
        } else {
            px(0.)
        },
    })
}
fn alpha_quad(bounds: Bounds<Pixels>, color: Color) -> PaintQuad {
    let color = color.rgba().alpha(1.);
    fill(bounds, gradient(90., color.alpha(0.), color)).corner_radii(bounds.size.height / 2.)
}
pub(super) fn hue_strip(bounds: Bounds<Pixels>, window: &mut Window) {
    for ix in 0..6 {
        window.paint_quad(hue_quad(bounds, ix));
    }
}
pub(super) fn slider_ink(alpha: bool, color: Color, theme: YueTheme) -> AnyElement {
    canvas(
        |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            if alpha {
                checker_with_radius(
                    bounds,
                    rgba(theme, "--color-picker-checker-light-color"),
                    rgba(theme, "--color-picker-checker-dark-color"),
                    px(
                        pixels(theme, "--color-picker-checker-size") * f32::from(window.rem_size())
                            / 16.,
                    ),
                    bounds.size.height / 2.,
                    window,
                );
                window.paint_quad(alpha_quad(bounds, color));
            } else {
                hue_strip(bounds, window);
            }
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

/// Only this painted part grows. Base's fixed thumb and value-mapping bounds stay intact.
pub(super) fn slider_thumb(
    alpha: bool,
    color: Rgba,
    theme: YueTheme,
    diameter: Pixels,
) -> AnyElement {
    use gpui_kit::base::TestSupportExt as _;
    let outline = rems(theme, "--color-picker-thumb-outline-width");
    div()
        .id(if alpha {
            "color-picker-alpha-thumb"
        } else {
            "color-picker-hue-thumb"
        })
        .test_support()
        .absolute()
        .left(gpui_kit::relative(0.5))
        .top(gpui_kit::relative(0.5))
        .ml(-diameter / 2.)
        .mt(-diameter / 2.)
        .size(diameter)
        .rounded_full()
        .shadow(crate::tokens::shadow(theme, "--color-picker-thumb-shadow"))
        .child(
            div()
                .absolute()
                .inset(-outline)
                .rounded_full()
                .border(outline)
                .border_color(rgba(theme, "--color-picker-thumb-outline-color")),
        )
        .child(
            div()
                .size_full()
                .rounded_full()
                .overflow_hidden()
                .border(rems(theme, "--color-picker-thumb-border-width"))
                .border_color(rgba(theme, "--color-picker-thumb-border-color"))
                .child(
                    canvas(
                        |bounds, _, _| bounds,
                        move |_, bounds, window, _| {
                            if alpha {
                                checker_with_radius(
                                    bounds,
                                    rgba(theme, "--color-picker-checker-light-color"),
                                    rgba(theme, "--color-picker-checker-dark-color"),
                                    px(pixels(theme, "--color-picker-checker-size")
                                        * f32::from(window.rem_size())
                                        / 16.),
                                    bounds.size.height / 2.,
                                    window,
                                );
                            }
                            window.paint_quad(
                                fill(bounds, color).corner_radii(bounds.size.height / 2.),
                            );
                        },
                    )
                    .size_full(),
                ),
        )
        .into_any_element()
}
pub(super) fn sv(
    id: gpui_kit::ElementId,
    state: &Entity<YueColorPickerState>,
    theme: YueTheme,
    disabled: bool,
    interactive: bool,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let hsv = state.read(cx).hsv;
    let focus = state.read(cx).focus.clone();
    let ring_visible =
        interactive && !disabled && focus.is_focused(_window) && _window.last_input_was_keyboard();
    let bounds = Rc::new(Cell::new(Bounds::default()));
    let capture = bounds.clone();
    let editor = state.clone();
    let weak = state.downgrade();
    let move_bounds = bounds.clone();
    let up_editor = state.downgrade();
    let paint = canvas(
        move |b, _, _| {
            capture.set(b);
        },
        move |b, _, window, _| {
            let hue = Color::from_hsv([hsv[0], 1., 1.], 1.).rgba();
            let radius = px(pixels(theme, "--color-picker-panel-radius")
                * f32::from(window.rem_size())
                / 16.);
            let corners = Corners {
                top_left: radius,
                top_right: radius,
                bottom_left: px(0.),
                bottom_right: px(0.),
            };
            window.paint_quad(
                fill(b, gradient(90., gpui_kit::white().into(), hue)).corner_radii(corners),
            );
            window.paint_quad(
                fill(
                    b,
                    gradient(
                        180.,
                        gpui_kit::black().opacity(0.).into(),
                        gpui_kit::black().into(),
                    ),
                )
                .corner_radii(corners),
            );
            let center = point(
                b.left() + b.size.width * hsv[1] as f32,
                b.bottom() - b.size.height * hsv[2] as f32,
            );
            let diameter =
                px(pixels(theme, "--color-picker-thumb-size") * f32::from(window.rem_size()) / 16.);
            let ring = Bounds::new(
                center - point(diameter / 2., diameter / 2.),
                size(diameter, diameter),
            );
            window.paint_quad(
                fill(ring, gpui_kit::transparent_black())
                    .corner_radii(diameter / 2.)
                    .border_widths(px((pixels(theme, "--color-picker-thumb-border-width")
                        + 1.)
                        * f32::from(window.rem_size())
                        / 16.))
                    .border_color(gpui_kit::black()),
            );
            window.paint_quad(
                fill(ring, gpui_kit::transparent_black())
                    .corner_radii(diameter / 2.)
                    .border_widths(px(pixels(theme, "--color-picker-thumb-border-width")
                        * f32::from(window.rem_size())
                        / 16.))
                    .border_color(gpui_kit::white()),
            );
            if interactive && !disabled {
                let weak = weak.clone();
                let bounds = move_bounds.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                    if phase.bubble()
                        && e.pressed_button == Some(MouseButton::Left)
                        && let Some(editor) = weak.upgrade().filter(|s| s.read(cx).dragging_sv)
                    {
                        apply_sv(&editor, bounds.get(), e.position, window, cx);
                        cx.stop_propagation();
                    }
                });
                let weak = up_editor.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _window, cx| {
                    if phase.bubble()
                        && e.button == MouseButton::Left
                        && let Some(editor) = weak.upgrade().filter(|s| s.read(cx).dragging_sv)
                    {
                        editor.update(cx, |s, cx| {
                            s.dragging_sv = false;
                            s.finish(true, cx);
                        });
                    }
                });
            }
        },
    )
    .absolute()
    .size_full();
    use gpui_kit::base::TestSupportExt as _;
    let down_bounds = bounds.clone();
    let label = format!(
        "色板，饱和度 {}%，明度 {}%",
        (hsv[1] * 100.).round(),
        (hsv[2] * 100.).round()
    );
    div()
        .id(id)
        .test_support()
        .when(interactive, |b| {
            b.role(gpui_kit::Role::Group)
                .aria_label("饱和度和明度")
                .aria_description(label)
        })
        .h(rems(theme, "--color-picker-sv-height"))
        .w_full()
        .relative()
        .cursor_crosshair()
        .when(interactive && !disabled, |b| {
            b.track_focus(&focus)
                .on_mouse_down(MouseButton::Left, move |e, window, cx| {
                    cx.stop_propagation();
                    focus.focus(window, cx);
                    editor.update(cx, |s, cx| {
                        s.finish(false, cx);
                        s.begin();
                        s.dragging_sv = true;
                    });
                    apply_sv(&editor, down_bounds.get(), e.position, window, cx);
                })
        })
        .child(paint)
        .child(
            div()
                .absolute()
                .inset(-gpui_kit::rems(
                    (pixels(theme, "--color-picker-focus-ring-offset")
                        + pixels(theme, "--color-picker-focus-ring-width"))
                        / 16.,
                ))
                .rounded(rems(theme, "--color-picker-panel-radius"))
                .border(rems(theme, "--color-picker-focus-ring-width"))
                .border_color(if ring_visible {
                    rgba(theme, "--color-picker-focus-ring-color")
                } else {
                    gpui_kit::transparent_black().into()
                }),
        )
        .into_any_element()
}
fn apply_sv(
    editor: &Entity<YueColorPickerState>,
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
        return;
    }
    editor.update(cx, |s, cx| {
        if !s.enabled {
            return;
        }
        s.begin();
        s.hsv[1] = ((position.x - bounds.left()) / bounds.size.width).clamp(0., 1.) as f64;
        s.hsv[2] = (1. - (position.y - bounds.top()) / bounds.size.height).clamp(0., 1.) as f64;
        s.apply_coordinates(window, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_gradient_quads_have_semicircular_ends_and_no_internal_rounding() {
        for height in [12., 18., 24.] {
            let bounds = Bounds::new(point(px(13.), px(25.)), size(px(266.), px(height)));
            let radius = px(height / 2.);
            for ix in 0..6 {
                let quad = hue_quad(bounds, ix);
                let left = if ix == 0 { radius } else { px(0.) };
                let right = if ix == 5 { radius } else { px(0.) };
                assert_eq!(quad.corner_radii.top_left, left);
                assert_eq!(quad.corner_radii.bottom_left, left);
                assert_eq!(quad.corner_radii.top_right, right);
                assert_eq!(quad.corner_radii.bottom_right, right);
                assert_eq!(quad.bounds.size.height, bounds.size.height);
                if ix < 5 {
                    assert_eq!(quad.bounds.right(), hue_quad(bounds, ix + 1).bounds.left());
                }
            }
            let alpha = alpha_quad(bounds, Color::BLACK);
            assert_eq!(alpha.bounds, bounds);
            assert_eq!(alpha.corner_radii, Corners::all(radius));
        }
    }
}
