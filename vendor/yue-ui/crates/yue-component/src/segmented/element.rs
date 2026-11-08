//! Element composition and component-local presentation.

use super::*;

use super::behavior::*;

use std::{cell::RefCell, rc::Rc};

use gpui_kit::base::{Radio, TestSupportExt as _, Transition, transition};

use gpui_kit::{
    App, Bounds, ContentMask, Corners, CursorStyle, ElementId, FocusHandle, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Pixels,
    RenderOnce, Role, ShapedLine, SharedString, StatefulInteractiveElement as _, Styled as _,
    TextAlign, TextRun, Window, canvas, div, fill, font, point, px, size,
};

use crate::theme::{TokenKey, keys};

use crate::tokens::{duration, easing, mix_rgba, number, pixels, rgba, shadow};

#[derive(Default)]
struct Pointer {
    hover: bool,
    press: bool,
}

#[derive(Default)]
struct LayoutHistory {
    cells: Vec<Bounds<Pixels>>,
    revision: String,
    valid: bool,
}

#[derive(Default)]
struct FocusHistory {
    items: Vec<(SharedString, FocusHandle)>,
}

#[derive(Clone)]
struct Label {
    normal: ShapedLine,
    inverse: ShapedLine,
    svg: Option<&'static [u8]>,
    color: Hsla,
    icon_size: Pixels,
    line_height: Pixels,
    gap: Pixels,
}

impl Label {
    fn content_width(&self) -> Pixels {
        self.normal.width()
            + if self.svg.is_some() {
                self.icon_size
                    + if self.normal.len() > 0 {
                        self.gap
                    } else {
                        px(0.)
                    }
            } else {
                px(0.)
            }
    }
    fn paint(
        &self,
        bounds: Bounds<Pixels>,
        inverse: bool,
        color: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut origin = point(
            bounds.center().x - self.content_width() / 2.,
            bounds.center().y - self.line_height / 2.,
        );
        if let Some(svg) = self.svg {
            let icon = Bounds::new(
                point(origin.x, bounds.center().y - self.icon_size / 2.),
                size(self.icon_size, self.icon_size),
            );
            // Identical SVG identity for both paint passes; the atlas caches its alpha mask.
            let path: SharedString = format!("yue-segmented-{:p}", svg.as_ptr()).into();
            let _ = window.paint_svg(icon, path, Some(svg), Default::default(), color, cx);
            origin.x += self.icon_size
                + if self.normal.len() > 0 {
                    self.gap
                } else {
                    px(0.)
                };
        }
        let line = if inverse { &self.inverse } else { &self.normal };
        let _ = line.paint(origin, self.line_height, TextAlign::Left, None, window, cx);
    }
}

fn shaped(
    text: SharedString,
    font: gpui_kit::Font,
    font_size: Pixels,
    color: Hsla,
    window: &Window,
) -> ShapedLine {
    window.text_system().shape_line(
        text.clone(),
        font_size,
        &[TextRun {
            len: text.len(),
            font,
            color,
            ..Default::default()
        }],
        None,
    )
}

impl RenderOnce for YueSegmented {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.use_default_theme {
            self.theme = crate::current_theme(cx);
        }
        let name = self
            .accessibility_label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.label.clone().filter(|s| !s.trim().is_empty()))
            .unwrap_or_else(|| panic!("YueSegmented {:?} needs a group accessible name", self.id));
        let theme = self.theme;
        let scale = window.rem_size() / px(16.);
        let p = move |key: TokenKey| px(pixels(theme, key) * scale);
        let (height_key, padding_key, font_key, line_key, icon_key) = match self.size {
            SegmentedSize::Small => (
                keys::SEGMENTED_HEIGHT_SM,
                keys::SEGMENTED_PADDING_INLINE_SM,
                keys::SEGMENTED_FONT_SIZE_SM,
                keys::SEGMENTED_LINE_HEIGHT_SM,
                keys::SEGMENTED_ICON_SIZE_SM,
            ),
            SegmentedSize::Medium => (
                keys::SEGMENTED_HEIGHT_MD,
                keys::SEGMENTED_PADDING_INLINE_MD,
                keys::SEGMENTED_FONT_SIZE_MD,
                keys::SEGMENTED_LINE_HEIGHT_MD,
                keys::SEGMENTED_ICON_SIZE_MD,
            ),
            SegmentedSize::Large => (
                keys::SEGMENTED_HEIGHT_LG,
                keys::SEGMENTED_PADDING_INLINE_LG,
                keys::SEGMENTED_FONT_SIZE_LG,
                keys::SEGMENTED_LINE_HEIGHT_LG,
                keys::SEGMENTED_ICON_SIZE_LG,
            ),
        };
        let height = p(height_key);
        let pad = p(keys::SEGMENTED_TRACK_PADDING);
        let cell_height = height - pad * 2.;
        let padding = p(padding_key);
        let font_size = p(font_key);
        let line_height = p(line_key);
        let icon_size = p(icon_key);
        let gap = p(keys::SEGMENTED_CONTENT_GAP);
        let accent = self.variant == SegmentedVariant::Accent;
        let selected = self.options.iter().position(|o| o.value == self.value);
        let selected_disabled =
            selected.is_some_and(|ix| self.disabled || self.options[ix].disabled);
        let track_disabled = self.disabled || self.options.iter().all(|option| option.disabled);
        let inverse_color = rgba(
            theme,
            if accent {
                keys::SEGMENTED_ACCENT_THUMB_COLOR
            } else {
                keys::SEGMENTED_THUMB_COLOR
            },
        )
        .into();
        let family = crate::typography::ui_font(theme);
        let mut base_font = font(family.family.clone());
        base_font.fallbacks = family.font_fallbacks();
        base_font.weight = FontWeight(number(theme, keys::SEGMENTED_WEIGHT));
        let mut selected_font = base_font.clone();
        selected_font.weight = FontWeight(number(theme, keys::SEGMENTED_WEIGHT_SELECTED));
        let focus_history =
            window.use_keyed_state((self.id.clone(), "focus-history"), cx, |_, _| {
                FocusHistory::default()
            });
        let mut handles = Vec::new();
        let mut labels = Vec::new();
        let mut widths = Vec::new();
        let mut feedback = Vec::new();
        let mut rows = Vec::new();
        let frames = Rc::new(RefCell::new(vec![Bounds::default(); self.options.len()]));
        for (ix, option) in self.options.iter().enumerate() {
            let id = ElementId::from((self.id.clone(), option.value.clone()));
            let focus = window
                .use_keyed_state((id.clone(), "focus"), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone();
            handles.push((option.value.clone(), focus.clone()));
            let disabled = self.disabled || option.disabled;
            let pointer =
                window.use_keyed_state((id.clone(), "pointer"), cx, |_, _| Pointer::default());
            let (hover, press) = {
                let s = pointer.read(cx);
                (!disabled && s.hover, !disabled && s.press)
            };
            let policy = || {
                Transition::new(if disabled {
                    std::time::Duration::ZERO
                } else {
                    duration(theme, keys::SEGMENTED_STATE_DURATION)
                })
                .ease(easing(theme, keys::SEGMENTED_STATE_EASE))
            };
            let hover = transition(
                (id.clone(), "hover"),
                f32::from(hover && selected != Some(ix)),
                policy(),
                window,
                cx,
            );
            let press = transition(
                (id.clone(), "press"),
                f32::from(press && selected != Some(ix)),
                policy(),
                window,
                cx,
            );
            let ink = if disabled {
                rgba(
                    theme,
                    if selected == Some(ix) {
                        keys::SEGMENTED_DISABLED_THUMB_COLOR
                    } else {
                        keys::SEGMENTED_DISABLED_COLOR
                    },
                )
            } else if accent {
                rgba(theme, keys::SEGMENTED_ACCENT_COLOR)
            } else if selected == Some(ix) {
                rgba(theme, keys::SEGMENTED_THUMB_COLOR)
            } else {
                mix_rgba(
                    rgba(theme, keys::SEGMENTED_COLOR),
                    rgba(theme, keys::SEGMENTED_COLOR_HOVER),
                    hover,
                )
            };
            let face = if selected == Some(ix) {
                selected_font.clone()
            } else {
                base_font.clone()
            };
            let normal = shaped(
                option.label.clone(),
                face.clone(),
                font_size,
                ink.into(),
                window,
            );
            let inverse = shaped(option.label.clone(), face, font_size, inverse_color, window);
            let width_regular = shaped(
                option.label.clone(),
                base_font.clone(),
                font_size,
                ink.into(),
                window,
            )
            .width();
            let width_selected = shaped(
                option.label.clone(),
                selected_font.clone(),
                font_size,
                ink.into(),
                window,
            )
            .width();
            let label = Label {
                normal,
                inverse,
                svg: option.icon,
                color: ink.into(),
                icon_size,
                line_height,
                gap,
            };
            let extra = if option.icon.is_some() {
                icon_size + if option.label.is_empty() { px(0.) } else { gap }
            } else {
                px(0.)
            };
            widths
                .push((width_regular.max(width_selected) + extra + padding * 2.).max(cell_height));
            labels.push(label);
            feedback.push((hover, press));
            let captured = frames.clone();
            let mut row = Radio::new(id)
                .checked(selected == Some(ix))
                .disabled(disabled)
                .track_focus(&focus)
                .tab_stop(stop(&self.options, &self.value, self.disabled) == Some(ix))
                .set_position(ix + 1, self.options.len())
                .accessibility_label(option.name.clone())
                .h(cell_height)
                .flex_shrink_0()
                .cursor(if disabled {
                    CursorStyle::OperationNotAllowed
                } else {
                    CursorStyle::PointingHand
                })
                .child(
                    canvas(
                        move |bounds, _, _| {
                            captured.borrow_mut()[ix] = bounds;
                        },
                        |_, _, _, _| {},
                    )
                    .size_full(),
                );
            if !disabled {
                if option.label.is_empty() {
                    let text = option.name.clone();
                    row = row.child(crate::tooltip::attach(
                        (self.id.clone(), option.value.clone()).into(),
                        crate::tooltip::Config::new(text, theme, crate::TooltipOptions::default())
                            .disabled(disabled),
                        Some(focus.clone()),
                        window,
                        cx,
                    ));
                }
                let value = option.value.clone();
                let handler = self.on_change.clone();
                let focus_on_click = focus.clone();
                row = row.on_change(move |_, _, window, cx| {
                    window.focus(&focus_on_click, cx);
                    if let Some(handler) = &handler {
                        handler(&value, window, cx);
                    }
                });
                row = row
                    .on_hover(window.listener_for(&pointer, |s, hovered, window, cx| {
                        s.hover = *hovered;
                        cx.notify();
                        window.refresh();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(&pointer, |s, _, window, cx| {
                            s.press = true;
                            cx.notify();
                            window.refresh();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        window.listener_for(&pointer, |s, _, window, cx| {
                            s.press = false;
                            cx.notify();
                            window.refresh();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        window.listener_for(&pointer, |s, _, window, cx| {
                            s.press = false;
                            s.hover = false;
                            cx.notify();
                            window.refresh();
                        }),
                    );
            } else {
                pointer.update(cx, |s, _| *s = Pointer::default());
            }
            rows.push(row);
        }
        let operable: Vec<_> = handles
            .iter()
            .enumerate()
            .filter(|(ix, _)| !self.disabled && !self.options[*ix].disabled)
            .map(|(_, v)| v.clone())
            .collect();
        // Repair only a focus that previously belonged to this group.
        let previous_focus = focus_history
            .read(cx)
            .items
            .iter()
            .position(|(_, h)| h.is_focused(window));
        if let Some(ix) = previous_focus {
            let old = &focus_history.read(cx).items[ix].0;
            if !operable.iter().any(|(v, _)| v == old) {
                if let Some((_, handle)) =
                    operable.iter().find(|(v, _)| *v == self.value).or_else(|| {
                        handles
                            .iter()
                            .enumerate()
                            .filter(|(pos, _)| !self.disabled && !self.options[*pos].disabled)
                            .min_by_key(|(pos, _)| pos.abs_diff(ix))
                            .map(|(_, item)| item)
                    })
                {
                    window.focus(handle, cx);
                } else {
                    window.on_next_frame(|window, cx| window.focus_next(cx));
                    window.request_animation_frame();
                }
            }
        }
        focus_history.update(cx, |s, _| s.items = handles.clone());
        let equal = self.distribution == SegmentedDistribution::Equal;
        let max_width = widths.iter().copied().fold(px(0.), Pixels::max);
        let min_width = pad * 2.
            + if equal {
                max_width * self.options.len() as f32
            } else {
                widths.iter().copied().sum::<Pixels>()
            };
        let radius = if self.shape == SegmentedShape::Pill {
            height / 2.
        } else {
            p(keys::SEGMENTED_RADIUS)
        };
        let thumb_radius = if self.shape == SegmentedShape::Pill {
            cell_height / 2.
        } else {
            p(keys::SEGMENTED_THUMB_RADIUS)
        };
        let mut track = div()
            .id((self.id.clone(), "track"))
            .test_support()
            .flex()
            .relative()
            .flex_shrink_0()
            .h(height)
            .w_full()
            .min_w(min_width)
            .p(pad)
            .rounded(radius)
            .bg(rgba(
                theme,
                if track_disabled {
                    keys::SEGMENTED_DISABLED_BACKGROUND
                } else if accent {
                    keys::SEGMENTED_ACCENT_BACKGROUND
                } else {
                    keys::SEGMENTED_BACKGROUND
                },
            ))
            .children(rows.into_iter().enumerate().map(|(ix, row)| {
                if equal {
                    row.flex_1().min_w(max_width)
                } else {
                    row.w(widths[ix])
                }
            }));
        let revision = format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}/{:?}",
            self.size,
            self.variant,
            self.shape,
            self.distribution,
            family,
            self.options
                .iter()
                .map(|o| (
                    &o.value,
                    &o.label,
                    o.icon.map(|svg| (svg.as_ptr(), svg.len()))
                ))
                .collect::<Vec<_>>()
        );
        let id = self.id.clone();
        let disabled = self.disabled;
        let count = self.options.len();
        let focus_handles = handles.iter().map(|(_, h)| h.clone()).collect::<Vec<_>>();
        track = track.child(
            canvas(
                move |bounds, window, cx| {
                    let cells: Vec<_> = frames
                        .borrow()
                        .iter()
                        .map(|cell| Bounds::new(cell.origin - bounds.origin, cell.size))
                        .collect();
                    let history =
                        window.use_keyed_state((id.clone(), "layout-history"), cx, |_, _| {
                            LayoutHistory::default()
                        });
                    let valid = selected.is_some();
                    let snap = {
                        let old = history.read(cx);
                        old.cells != cells
                            || old.revision != revision
                            || !old.valid
                            || !valid
                            || disabled
                            || selected_disabled
                    };
                    history.update(cx, |s, _| {
                        s.cells = cells.clone();
                        s.revision = revision;
                        s.valid = valid;
                    });
                    let target = selected.map(|ix| cells[ix]).unwrap_or_default();
                    let policy = Transition::new(if snap {
                        std::time::Duration::ZERO
                    } else {
                        duration(theme, keys::SEGMENTED_DURATION)
                    })
                    .ease(easing(theme, keys::SEGMENTED_EASE));
                    let indicator =
                        transition((id, "indicator-bounds"), target, policy, window, cx);
                    (cells, valid.then_some(indicator))
                },
                move |bounds, (cells, indicator), window, cx| {
                    let thumb_color = rgba(
                        theme,
                        if accent {
                            keys::SEGMENTED_ACCENT_THUMB_BACKGROUND
                        } else {
                            keys::SEGMENTED_THUMB_BACKGROUND
                        },
                    );
                    if let Some(local) = indicator {
                        let rect = Bounds::new(bounds.origin + local.origin, local.size);
                        let radii = Corners::all(
                            thumb_radius
                                .min(rect.size.height / 2.)
                                .min(rect.size.width / 2.),
                        );
                        if selected_disabled {
                            window.paint_quad(
                                fill(rect, rgba(theme, keys::SEGMENTED_DISABLED_THUMB_BACKGROUND))
                                    .corner_radii(radii),
                            );
                        } else {
                            if !accent {
                                window.paint_drop_shadows(
                                    rect,
                                    radii,
                                    &shadow(theme, keys::SEGMENTED_THUMB_SHADOW)
                                        .into_iter()
                                        .map(|mut shadow| {
                                            shadow.offset *= scale;
                                            shadow.blur_radius *= scale;
                                            shadow.spread_radius *= scale;
                                            shadow
                                        })
                                        .collect::<Vec<_>>(),
                                );
                            }
                            window.paint_quad(fill(rect, thumb_color).corner_radii(radii));
                        }
                    }
                    for ix in 0..count {
                        let rect = Bounds::new(bounds.origin + cells[ix].origin, cells[ix].size);
                        let (hover, press) = feedback[ix];
                        let mut overlay = rgba(theme, keys::SEGMENTED_PRESSED_BACKGROUND);
                        overlay.a *= press;
                        if accent {
                            let mut tint = rgba(theme, keys::SEGMENTED_HOVER_BACKGROUND);
                            tint.a *= hover;
                            window.paint_quad(fill(rect, tint).corner_radii(thumb_radius));
                        }
                        window.paint_quad(fill(rect, overlay).corner_radii(thumb_radius));
                        labels[ix].paint(rect, false, labels[ix].color, window, cx);
                    }
                    if accent
                        && !selected_disabled
                        && let Some(local) = indicator
                    {
                        let rect = Bounds::new(bounds.origin + local.origin, local.size);
                        for mask in rounded_masks(rect, thumb_radius, window.scale_factor()) {
                            window.with_content_mask(
                                Some(ContentMask { bounds: mask }),
                                |window| {
                                    for (ix, label) in labels.iter().enumerate() {
                                        let cell = Bounds::new(
                                            bounds.origin + cells[ix].origin,
                                            cells[ix].size,
                                        );
                                        if cell.intersects(&mask) {
                                            label.paint(cell, true, inverse_color, window, cx);
                                        }
                                    }
                                },
                            );
                        }
                    }
                    if !disabled && window.last_input_was_keyboard() {
                        for (ix, handle) in focus_handles.iter().enumerate() {
                            if handle.is_focused(window) {
                                let width = p(keys::SEGMENTED_FOCUS_RING_WIDTH);
                                let offset = p(keys::SEGMENTED_FOCUS_RING_OFFSET);
                                let rect =
                                    Bounds::new(bounds.origin + cells[ix].origin, cells[ix].size)
                                        .dilate(width + offset);
                                let mut quad = fill(rect, gpui_kit::transparent_black())
                                    .corner_radii(thumb_radius + width + offset);
                                quad.border_widths = width.into();
                                quad.border_color =
                                    rgba(theme, keys::SEGMENTED_FOCUS_RING_COLOR).into();
                                window.paint_quad(quad);
                            }
                        }
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        );
        let mut group = div()
            .id(self.id.clone())
            .test_support()
            .role(Role::RadioGroup)
            .aria_label(name)
            .aria_orientation(gpui_kit::accesskit::Orientation::Horizontal)
            .flex()
            .flex_col()
            .items_start()
            .min_w(min_width)
            .flex_shrink_0()
            .gap(gap);
        if let Some(label) = self.label {
            group = group.child(
                div()
                    .text_size(font_size)
                    .text_color(rgba(theme, keys::SEGMENTED_COLOR))
                    .child(label),
            );
        }
        if !self.options.is_empty() {
            group = group.w(min_width).child(track);
        }
        let selected = self.value;
        let handler = self.on_change;
        self.layout
            .apply(group)
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                if modifiers.control || modifiers.alt || modifiers.platform || modifiers.shift {
                    return;
                }
                let Some(current) = operable.iter().position(|(_, h)| h.is_focused(window)) else {
                    return;
                };
                let Some(target) = move_to(&event.keystroke.key, current, operable.len()) else {
                    return;
                };
                let (value, focus) = &operable[target];
                window.focus(focus, cx);
                if *value != selected
                    && let Some(handler) = &handler
                {
                    handler(value, window, cx);
                }
                window.refresh();
                cx.stop_propagation();
            })
    }
}

/// GPUI content masks are rectangles. Scan only the rounded caps at device-pixel
/// resolution; the middle is one mask. This also clips inverse SVG ink to a pill.
pub(super) fn rounded_masks(
    bounds: Bounds<Pixels>,
    radius: Pixels,
    scale: f32,
) -> Vec<Bounds<Pixels>> {
    let r = radius
        .min(bounds.size.width / 2.)
        .min(bounds.size.height / 2.);
    if r <= px(0.) {
        return vec![bounds];
    }
    let step = px(1. / scale);
    let mut result = Vec::new();
    let mut y = px(0.);
    while y < r {
        let h = step.min(r - y);
        let center = y + h / 2.;
        let dy = r - center;
        let rr = f32::from(r);
        let yy = f32::from(dy);
        let inset = r - px((rr * rr - yy * yy).max(0.).sqrt());
        let w = bounds.size.width - inset * 2.;
        result.push(Bounds::new(bounds.origin + point(inset, y), size(w, h)));
        result.push(Bounds::new(
            bounds.origin + point(inset, bounds.size.height - y - h),
            size(w, h),
        ));
        y += h;
    }
    if bounds.size.height > r * 2. {
        result.push(Bounds::new(
            bounds.origin + point(px(0.), r),
            size(bounds.size.width, bounds.size.height - r * 2.),
        ));
    }
    result
        .into_iter()
        .map(|mask| mask.intersect(&bounds))
        .collect()
}
