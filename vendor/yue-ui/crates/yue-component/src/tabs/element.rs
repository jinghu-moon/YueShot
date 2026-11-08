use super::*;
use crate::theme::{TokenKey, keys};
use crate::tokens::{duration, easing, rems, rgba};
use gpui_kit::base::{
    Button, MotionStatus, Presence, Tab, TabDescriptor, TabPanel, Tabs, TabsController,
    TabsSnapshot, TestSupportExt as _, Transition, transition, transition_with_status,
};
use gpui_kit::{
    Axis, Bounds, FontWeight, InteractiveElement as _, MouseButton, ParentElement as _, Pixels,
    RenderOnce, StatefulInteractiveElement as _, Styled as _, TextRun, canvas, div, fill, font,
    point, prelude::FluentBuilder as _, px,
};
use std::{cell::RefCell, collections::HashMap, time::Duration};

#[derive(Default)]
struct VisualState {
    signature: String,
    selected: Option<SharedString>,
    serial: u64,
    direction: f32,
    scroll_target: Option<Pixels>,
    previous: Option<Bounds<Pixels>>,
    geometry: String,
    overflow: bool,
    initialized: bool,
}
#[derive(Default)]
struct Pointer {
    hover: bool,
    pressed: bool,
}

impl TabsSize {
    fn keys(self) -> [TokenKey; 7] {
        match self {
            Self::Small => [
                keys::TABS_HEIGHT_SM,
                keys::TABS_FONT_SIZE_SM,
                keys::TABS_ICON_SIZE_SM,
                keys::TABS_PADDING_INLINE_SM,
                keys::TABS_GAP_SM,
                keys::TABS_ACTION_SIZE_SM,
                keys::TABS_CLOSE_ICON_SIZE_SM,
            ],
            Self::Medium => [
                keys::TABS_HEIGHT_MD,
                keys::TABS_FONT_SIZE_MD,
                keys::TABS_ICON_SIZE_MD,
                keys::TABS_PADDING_INLINE_MD,
                keys::TABS_GAP_MD,
                keys::TABS_ACTION_SIZE_MD,
                keys::TABS_CLOSE_ICON_SIZE_MD,
            ],
            Self::Large => [
                keys::TABS_HEIGHT_LG,
                keys::TABS_FONT_SIZE_LG,
                keys::TABS_ICON_SIZE_LG,
                keys::TABS_PADDING_INLINE_LG,
                keys::TABS_GAP_LG,
                keys::TABS_ACTION_SIZE_LG,
                keys::TABS_CLOSE_ICON_SIZE_LG,
            ],
        }
    }
}

struct CommandStyle {
    theme: YueTheme,
    size: Pixels,
    icon: gpui_kit::Rems,
}
impl CommandStyle {
    fn button(&self, id: ElementId, label: SharedString, icon: Icon, disabled: bool) -> Button {
        let theme = self.theme;
        Button::new(id)
            .accessibility_label(label)
            .disabled(disabled)
            .w(self.size)
            .h(self.size)
            .flex_shrink_0()
            .cursor_default()
            .rounded(rems(theme, keys::TABS_BORDER_RADIUS))
            .text_color(rgba(
                theme,
                if disabled {
                    keys::TABS_COLOR_DISABLED
                } else {
                    keys::TABS_COLOR
                },
            ))
            .border(rems(theme, keys::TABS_FOCUS_RING_WIDTH))
            .border_color(gpui_kit::transparent_black())
            .when(!disabled, |b| {
                b.hover(|s| s.bg(rgba(theme, keys::TABS_BACKGROUND_HOVER)))
                    .active(|s| s.bg(rgba(theme, keys::TABS_BACKGROUND_PRESSED)))
                    .focus_visible(|s| s.border_color(rgba(theme, keys::TABS_FOCUS_RING_COLOR)))
            })
            .child(icon.size(self.icon).color_value(rgba(
                theme,
                if disabled {
                    keys::TABS_COLOR_DISABLED
                } else {
                    keys::TABS_COLOR
                },
            )))
    }
}

fn label_width(label: &SharedString, theme: YueTheme, size: Pixels, window: &Window) -> Pixels {
    let family = crate::typography::ui_font(theme);
    let mut face = font(family.family.clone());
    face.fallbacks = family.font_fallbacks();
    [FontWeight::NORMAL, FontWeight::MEDIUM]
        .into_iter()
        .map(|weight| {
            face.weight = weight;
            window
                .text_system()
                .shape_line(
                    label.clone(),
                    size,
                    &[TextRun {
                        len: label.len(),
                        font: face.clone(),
                        color: rgba(theme, keys::TABS_COLOR).into(),
                        ..Default::default()
                    }],
                    None,
                )
                .width()
        })
        .fold(px(0.), Pixels::max)
}

impl RenderOnce for YueTabs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let vertical = self.placement.vertical();
        let placement = self.placement;
        let card = self.variant == TabsVariant::Card;
        let [
            height_key,
            font_key,
            icon_key,
            pad_key,
            gap_key,
            action_key,
            close_key,
        ] = self.size.keys();
        let rem_size = window.rem_size();
        let p = |key| rems(theme, key).to_pixels(rem_size);
        let height = p(height_key);
        let font_size = p(font_key);
        let icon_size = p(icon_key);
        let pad = p(pad_key);
        let gap = p(gap_key);
        let action_size = p(action_key);
        let border = p(keys::TABS_BORDER_WIDTH);
        let focus_border = p(keys::TABS_FOCUS_RING_WIDTH);
        let signature = format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}",
            theme.mode(),
            theme.accent(),
            theme.neutral(),
            self.placement,
            self.variant,
            self.size,
            self.motion,
            window.rem_size(),
            self.disabled
        );
        let state = window.use_keyed_state((self.id.clone(), "tabs-controller"), cx, |_, cx| {
            TabsController::new(cx)
        });
        let visual = window.use_keyed_state((self.id.clone(), "tabs-visual"), cx, |_, _| {
            VisualState::default()
        });
        let add_focus = window
            .use_keyed_state((self.id.clone(), "tabs-add-focus"), cx, |_, cx| {
                cx.focus_handle()
            })
            .read(cx)
            .clone();
        state.update(cx, |state, cx| {
            state.fallback_focus((self.addable && !self.disabled).then_some(add_focus.clone()));
            state.sync(
                TabsSnapshot::new(
                    self.items.iter().map(|item| {
                        TabDescriptor::new(item.key.clone())
                            .disabled(item.disabled)
                            .closable(item.closable)
                    }),
                    self.selected.clone(),
                )
                .axis(if vertical {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                })
                .disabled(self.disabled)
                .activation(self.activation),
                window,
                cx,
            );
            state.clear_handlers();
            if let Some(handler) = self.on_change.clone() {
                state.on_change(move |request, w, cx| handler(request, w, cx));
            }
            if let Some(handler) = self.on_close.clone() {
                state.on_close(move |request, w, cx| handler(request, w, cx));
            }
        });
        if let Some(handle) = &self.handle {
            *handle.0.borrow_mut() = Some(state.downgrade());
        }
        let selected = state.read(cx).selected_key().cloned();
        let changed_layout = visual.read(cx).signature != signature;
        let changed_selection = visual.read(cx).selected != selected;
        let old_ix = visual
            .read(cx)
            .selected
            .as_ref()
            .and_then(|key| self.items.iter().position(|i| &i.key == key));
        let next_ix = selected
            .as_ref()
            .and_then(|key| self.items.iter().position(|i| &i.key == key));
        let initial = !visual.read(cx).initialized;
        visual.update(cx, |v, _| {
            if changed_selection {
                v.serial += 1;
                v.direction = if next_ix >= old_ix { 1. } else { -1. };
            }
            if changed_layout {
                v.previous = None;
                v.scroll_target = None;
            }
            v.signature = signature.clone();
            v.selected = selected.clone();
            v.initialized = true;
        });
        let policy = || {
            Transition::new(if changed_layout || self.disabled {
                Duration::ZERO
            } else {
                duration(theme, keys::TABS_DURATION)
            })
            .ease(easing(theme, keys::TABS_EASE))
        };
        let frames = Rc::new(RefCell::new(HashMap::<SharedString, Bounds<Pixels>>::new()));
        let titles = Rc::new(RefCell::new(HashMap::<SharedString, Bounds<Pixels>>::new()));
        let viewport = Rc::new(RefCell::new(Bounds::<Pixels>::default()));
        let commands = CommandStyle {
            theme,
            size: action_size,
            icon: rems(theme, close_key),
        };
        let mut cells = Vec::new();
        let mut track_width = px(0.);
        let mut minimum_width = px(0.);
        let mut revision = format!("{signature}/");
        for (ix, item) in self.items.iter().enumerate() {
            let id: ElementId = (self.id.clone(), item.key.clone()).into();
            let disabled = self.disabled || item.disabled;
            let checked = selected.as_ref() == Some(&item.key);
            let pointer =
                window.use_keyed_state((id.clone(), "pointer"), cx, |_, _| Pointer::default());
            if disabled || changed_layout {
                pointer.update(cx, |s, _| {
                    s.hover = false;
                    s.pressed = false;
                });
            }
            let fg = transition(
                (id.clone(), "foreground"),
                gpui_kit::Hsla::from(rgba(
                    theme,
                    if disabled {
                        keys::TABS_COLOR_DISABLED
                    } else if checked {
                        keys::TABS_COLOR_SELECTED
                    } else {
                        keys::TABS_COLOR
                    },
                )),
                policy(),
                window,
                cx,
            );
            let wash = transition(
                (id.clone(), "wash"),
                gpui_kit::Hsla::from(if disabled {
                    gpui_kit::transparent_black().into()
                } else if pointer.read(cx).pressed {
                    rgba(theme, keys::TABS_BACKGROUND_PRESSED)
                } else if pointer.read(cx).hover {
                    rgba(theme, keys::TABS_BACKGROUND_HOVER)
                } else {
                    gpui_kit::transparent_black().into()
                }),
                policy(),
                window,
                cx,
            );
            let base = if card {
                rgba(
                    theme,
                    if checked {
                        keys::TABS_CARD_BACKGROUND_SELECTED
                    } else {
                        keys::TABS_CARD_BACKGROUND
                    },
                )
            } else {
                gpui_kit::transparent_black().into()
            };
            let background = transition(
                (id.clone(), "surface"),
                gpui_kit::Hsla::from(base),
                policy(),
                window,
                cx,
            );
            let content_width = label_width(&item.label, theme, font_size, window)
                + if item.icon.is_some() {
                    icon_size + gap
                } else {
                    px(0.)
                };
            let extra = pad * 2.
                + focus_border * 2.
                + if card { border * 2. } else { px(0.) }
                + if item.closable { action_size } else { px(0.) };
            let minimum = extra
                + font_size
                + if item.icon.is_some() {
                    icon_size + gap
                } else {
                    px(0.)
                };
            minimum_width = minimum_width.max(minimum);
            let width = (content_width + extra)
                .min(p(keys::TABS_MAX_ITEM_WIDTH))
                .max(minimum);
            track_width += width;
            revision.push_str(&format!(
                "{:?}:{:?}:{:?}:{:?}/",
                item.key,
                item.label,
                item.icon.is_some(),
                item.closable
            ));
            let capture_title = titles.clone();
            let key_title = item.key.clone();
            let focus = state
                .read(cx)
                .focus_handle(&item.key)
                .expect("synced Tab focus");
            let title = div()
                .relative()
                .flex()
                .items_center()
                .gap(gap)
                .min_w_0()
                .flex_1()
                .when_some(item.icon, |d, icon| {
                    d.child(icon.size(rems(theme, icon_key)).color_value(fg.into()))
                })
                .child(div().min_w_0().flex_1().child(if disabled {
                    div()
                        .text_ellipsis()
                        .child(item.label.clone())
                        .into_any_element()
                } else {
                    crate::YueOverflowTooltip::new(
                        (id.clone(), "overflow"),
                        theme,
                        item.label.clone(),
                    )
                    .track_focus(&focus)
                    .into_any_element()
                }))
                .child(
                    canvas(
                        move |bounds, _, _| {
                            capture_title.borrow_mut().insert(key_title.clone(), bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                );
            let mut tab = Tab::new((id.clone(), "tab"))
                .in_list(&state, item.key.clone())
                .selected(checked)
                .disabled(disabled)
                .accessibility_label(item.label.clone())
                .set_position(ix + 1, self.items.len())
                .h_full()
                .min_w_0()
                .flex_1()
                .px(pad)
                .cursor_default()
                .border(focus_border)
                .border_color(gpui_kit::transparent_black())
                .rounded(rems(theme, keys::TABS_BORDER_RADIUS))
                .text_color(fg)
                .font_weight(if checked {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .focus_visible(|s| s.border_color(rgba(theme, keys::TABS_FOCUS_RING_COLOR)))
                .child(title);
            if !disabled {
                let hover = pointer.clone();
                let press = pointer.clone();
                let release = pointer.clone();
                let cancel = pointer.clone();
                tab = tab
                    .on_hover(move |value, _, cx| {
                        hover.update(cx, |s, cx| {
                            s.hover = *value;
                            if !*value {
                                s.pressed = false;
                            }
                            cx.notify();
                        })
                    })
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        press.update(cx, |s, cx| {
                            s.pressed = true;
                            cx.notify();
                        })
                    })
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        release.update(cx, |s, cx| {
                            s.pressed = false;
                            cx.notify();
                        })
                    })
                    .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                        cancel.update(cx, |s, cx| {
                            s.pressed = false;
                            cx.notify();
                        })
                    });
            }
            let capture = frames.clone();
            let key_capture = item.key.clone();
            let mut cell = div()
                .id(id.clone())
                .test_support()
                .relative()
                .flex()
                .items_center()
                .h(height)
                .flex_shrink_0()
                .when(vertical, |d| d.w_full())
                .when(!vertical, |d| d.w(width))
                .bg(background)
                .when(card, |d| {
                    let d = d.border_color(rgba(theme, keys::TABS_BORDER_COLOR));
                    match placement {
                        TabsPlacement::Top => d
                            .border_t(border)
                            .border_r(border)
                            .when(ix == 0, |d| d.border_l(border))
                            .rounded_t(rems(theme, keys::TABS_BORDER_RADIUS)),
                        TabsPlacement::Bottom => d
                            .border_b(border)
                            .border_r(border)
                            .when(ix == 0, |d| d.border_l(border))
                            .rounded_b(rems(theme, keys::TABS_BORDER_RADIUS)),
                        TabsPlacement::Left => d
                            .border_l(border)
                            .border_b(border)
                            .when(ix == 0, |d| d.border_t(border))
                            .rounded_l(rems(theme, keys::TABS_BORDER_RADIUS)),
                        TabsPlacement::Right => d
                            .border_r(border)
                            .border_b(border)
                            .when(ix == 0, |d| d.border_t(border))
                            .rounded_r(rems(theme, keys::TABS_BORDER_RADIUS)),
                    }
                })
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(rems(theme, keys::TABS_BORDER_RADIUS))
                        .bg(wash),
                )
                .child(tab)
                .child(
                    canvas(
                        move |bounds, _, _| {
                            capture.borrow_mut().insert(key_capture.clone(), bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                );
            if item.closable {
                let close_state = state.clone();
                let key = item.key.clone();
                cell = cell.child(
                    commands
                        .button(
                            (id, "close").into(),
                            format!("关闭 {}", item.label).into(),
                            Icon::new(crate::outline::X),
                            disabled,
                        )
                        .tab_stop(false)
                        .on_click(move |_, w, cx| {
                            // A pointer/native close command briefly owns focus.
                            // Hand it to its Tab before the owner can remove the
                            // command, so the next snapshot can choose a neighbor.
                            if let Some(focus) = close_state.read(cx).focus_handle(&key) {
                                focus.focus(w, cx);
                            }
                            TabsController::close(&close_state, &key, w, cx);
                        }),
                );
            }
            cells.push(cell);
        }
        let scroll = state.read(cx).scroll_handle();
        let capture_viewport = viewport.clone();
        let observe_state = state.clone();
        let observe_visual = visual.clone();
        let boundary_frames = frames.clone();
        let boundary_selected = selected.clone();
        let item_keys = self.items.iter().map(|i| i.key.clone()).collect::<Vec<_>>();
        let paint_selected = selected.clone();
        let paint_id = self.id.clone();
        let mut track = Tabs::new((self.id.clone(), "list"))
            .axis(if vertical {
                Axis::Vertical
            } else {
                Axis::Horizontal
            })
            .aria_label(self.label.clone())
            .relative()
            .flex()
            .flex_shrink_0()
            .when(vertical, |d| {
                d.flex_col().w_full().h(height * self.items.len() as f32)
            })
            .when(!vertical, |d| d.h(height).w(track_width))
            .children(cells);
        track = track.child(
            canvas(
                move |track_bounds, w, cx| {
                    let view = *viewport.borrow();
                    let local = item_keys
                        .iter()
                        .filter_map(|key| {
                            frames.borrow().get(key).map(|b| {
                                (
                                    key.clone(),
                                    Bounds::new(b.origin - track_bounds.origin, b.size),
                                )
                            })
                        })
                        .collect::<Vec<_>>();
                    let main_length = if vertical {
                        view.size.height
                    } else {
                        view.size.width
                    };
                    let previous_size = observe_state.read(cx).viewport();
                    observe_state.update(cx, |s, _| s.record_layout(local.clone(), main_length));
                    let overflow = observe_state.read(cx).is_overflowing();
                    if observe_visual.read(cx).overflow != overflow {
                        observe_visual.update(cx, |s, cx| {
                            s.overflow = overflow;
                            cx.notify();
                        });
                        w.request_animation_frame();
                    }
                    let pending = observe_state.update(cx, |s, _| s.take_scroll_target());
                    let native = observe_state.read(cx).offset();
                    if let Some(target) = pending {
                        observe_visual.update(cx, |s, _| s.scroll_target = Some(target));
                    }
                    let target = observe_visual.read(cx).scroll_target.unwrap_or(native);
                    let snap = changed_layout || initial || previous_size != main_length;
                    let scroll_sample = transition_with_status(
                        (paint_id.clone(), "scroll"),
                        target,
                        Transition::new(
                            if snap || observe_visual.read(cx).scroll_target.is_none() {
                                Duration::ZERO
                            } else {
                                duration(theme, keys::TABS_SCROLL_DURATION)
                            },
                        )
                        .ease(easing(theme, keys::TABS_EASE)),
                        w,
                        cx,
                    );
                    if (native - scroll_sample.value).abs() > px(0.05) {
                        observe_state
                            .read(cx)
                            .scroll_handle()
                            .set_offset(if vertical {
                                point(px(0.), -scroll_sample.value)
                            } else {
                                point(-scroll_sample.value, px(0.))
                            });
                        w.request_animation_frame();
                    }
                    if scroll_sample.status == MotionStatus::Finished {
                        observe_visual.update(cx, |s, _| s.scroll_target = None);
                    }
                    let rect = paint_selected.as_ref().and_then(|key| {
                        if vertical {
                            local.iter().find(|(k, _)| k == key).map(|(_, b)| {
                                let x = if placement == TabsPlacement::Left {
                                    b.right()
                                        - rems(theme, keys::TABS_INDICATOR_SIZE)
                                            .to_pixels(w.rem_size())
                                } else {
                                    b.left()
                                };
                                Bounds::new(
                                    point(x, b.top()),
                                    gpui_kit::size(
                                        rems(theme, keys::TABS_INDICATOR_SIZE)
                                            .to_pixels(w.rem_size()),
                                        b.size.height,
                                    ),
                                )
                            })
                        } else {
                            titles.borrow().get(key).map(|b| {
                                let thickness =
                                    rems(theme, keys::TABS_INDICATOR_SIZE).to_pixels(w.rem_size());
                                let y = if placement == TabsPlacement::Top {
                                    track_bounds.size.height - thickness
                                } else {
                                    px(0.)
                                };
                                Bounds::new(
                                    point(b.left() - track_bounds.left(), y),
                                    gpui_kit::size(b.size.width, thickness),
                                )
                            })
                        }
                    });
                    let old = observe_visual.read(cx).previous;
                    let geometry_changed = observe_visual.read(cx).geometry != revision;
                    let start = if vertical {
                        view.top() - track_bounds.top()
                    } else {
                        view.left() - track_bounds.left()
                    };
                    let visible = |b: Bounds<Pixels>| {
                        let (a, z) = if vertical {
                            (b.top(), b.bottom())
                        } else {
                            (b.left(), b.right())
                        };
                        a >= start && z <= start + main_length
                    };
                    let travel = old.zip(rect).is_some_and(|(a, b)| {
                        let delta = if vertical {
                            b.origin.y - a.origin.y
                        } else {
                            b.origin.x - a.origin.x
                        };
                        visible(a) && visible(b) && delta.abs() <= main_length / 2.
                    });
                    let animate = !card && !snap && !geometry_changed && travel;
                    observe_visual.update(cx, |s, _| {
                        s.previous = rect;
                        s.geometry = revision;
                    });
                    let target = rect.unwrap_or_default();
                    let line = transition(
                        (paint_id, "indicator"),
                        target,
                        Transition::new(if animate {
                            duration(theme, keys::TABS_INDICATOR_DURATION)
                        } else {
                            Duration::ZERO
                        })
                        .ease(easing(theme, keys::TABS_EASE)),
                        w,
                        cx,
                    );
                    (!card && rect.is_some()).then_some(line)
                },
                move |bounds, line, w, _| {
                    if let Some(line) = line {
                        w.paint_quad(
                            fill(
                                Bounds::new(bounds.origin + line.origin, line.size),
                                rgba(theme, keys::TABS_INDICATOR_COLOR),
                            )
                            .corner_radii(
                                rems(theme, keys::TABS_INDICATOR_RADIUS).to_pixels(w.rem_size()),
                            ),
                        );
                    }
                },
            )
            .absolute()
            .inset_0(),
        );
        let cancel_scroll = visual.clone();
        let mut viewport_element = div()
            .id((self.id.clone(), "viewport"))
            .test_support()
            .relative()
            .min_w_0()
            .min_h_0()
            .flex_1()
            .when(!vertical, |d| d.min_w(minimum_width))
            .when(vertical, |d| d.overflow_y_scroll())
            .when(!vertical, |d| d.overflow_x_scroll())
            .track_scroll(&scroll)
            .child(
                canvas(
                    move |bounds, _, _| {
                        *capture_viewport.borrow_mut() = bounds;
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(track)
            .on_scroll_wheel(move |_, w, cx| {
                cancel_scroll.update(cx, |s, _| s.scroll_target = None);
                w.refresh();
            });
        if !vertical {
            viewport_element = viewport_element.h(height);
        }
        let mut header = div()
            .id((self.id.clone(), "header"))
            .test_support()
            .relative()
            .flex()
            .items_center()
            .flex_shrink_0()
            .min_w_0()
            .min_h_0()
            .when(vertical, |d| {
                d.flex_col().min_w(minimum_width).w(self
                    .header_width
                    .unwrap_or_else(|| rems(theme, keys::TABS_HEADER_WIDTH).into()))
            })
            .when(!vertical, |d| d.w_full().h(height))
            .child(
                canvas(
                    |bounds, _, _| bounds,
                    move |_, bounds, w, _| {
                        let mut edge = bounds;
                        match placement {
                            TabsPlacement::Top => {
                                edge.origin.y = edge.bottom() - border;
                                edge.size.height = border;
                            }
                            TabsPlacement::Bottom => edge.size.height = border,
                            TabsPlacement::Left => {
                                edge.origin.x = edge.right() - border;
                                edge.size.width = border;
                            }
                            TabsPlacement::Right => edge.size.width = border,
                        }
                        let selected = card
                            .then(|| {
                                boundary_selected
                                    .as_ref()
                                    .and_then(|key| boundary_frames.borrow().get(key).copied())
                            })
                            .flatten();
                        // One structural edge; an active Card opens onto its panel.
                        if let Some(cell) = selected {
                            let mut before = edge;
                            let mut after = edge;
                            if vertical {
                                before.size.height = (cell.top().clamp(edge.top(), edge.bottom())
                                    - edge.top())
                                .max(px(0.));
                                after.origin.y = cell.bottom().clamp(edge.top(), edge.bottom());
                                after.size.height = (edge.bottom() - after.top()).max(px(0.));
                            } else {
                                before.size.width = (cell.left().clamp(edge.left(), edge.right())
                                    - edge.left())
                                .max(px(0.));
                                after.origin.x = cell.right().clamp(edge.left(), edge.right());
                                after.size.width = (edge.right() - after.left()).max(px(0.));
                            }
                            for part in [before, after] {
                                w.paint_quad(fill(part, rgba(theme, keys::TABS_BORDER_COLOR)));
                            }
                        } else {
                            w.paint_quad(fill(edge, rgba(theme, keys::TABS_BORDER_COLOR)));
                        }
                    },
                )
                .absolute()
                .inset_0(),
            );
        if vertical {
            header = if self.height.is_some() {
                header.h_full()
            } else {
                header.h(rems(theme, keys::TABS_HEADER_WIDTH))
            };
        }
        if visual.read(cx).overflow {
            let at_start = state.read(cx).offset() <= px(0.5);
            let at_end = state.read(cx).offset() + state.read(cx).viewport()
                >= state.read(cx).extent() - px(0.5);
            let previous = state.clone();
            let next = state.clone();
            header = header.child(
                commands
                    .button(
                        (self.id.clone(), "previous").into(),
                        "向前滚动标签".into(),
                        if vertical {
                            Icon::new(crate::outline::ChevronUp)
                        } else {
                            Icon::new(crate::outline::ChevronLeft)
                        },
                        self.disabled || at_start,
                    )
                    .on_click(move |_, w, cx| {
                        previous.update(cx, |s, cx| {
                            s.page(false);
                            cx.notify();
                        });
                        w.refresh();
                    }),
            );
            header = header.child(viewport_element).child(
                commands
                    .button(
                        (self.id.clone(), "next").into(),
                        "向后滚动标签".into(),
                        if vertical {
                            Icon::new(crate::outline::ChevronDown)
                        } else {
                            Icon::new(crate::outline::ChevronRight)
                        },
                        self.disabled || at_end,
                    )
                    .on_click(move |_, w, cx| {
                        next.update(cx, |s, cx| {
                            s.page(true);
                            cx.notify();
                        });
                        w.refresh();
                    }),
            );
        } else {
            header = header.child(viewport_element);
        }
        if self.addable {
            header = header.child(
                commands
                    .button(
                        (self.id.clone(), "add").into(),
                        "新增标签页".into(),
                        Icon::new(crate::outline::Plus),
                        self.disabled,
                    )
                    .track_focus(&add_focus)
                    .on_click(move |_, w, cx| {
                        if let Some(handler) = &self.on_add {
                            handler(w, cx);
                        }
                    }),
            );
        }
        let mut active = None;
        for item in &self.items {
            let current = selected.as_ref() == Some(&item.key);
            if (!self.lazy || current)
                && let Some(render) = &item.content
            {
                let content = render(window, cx);
                if current {
                    active = Some(content);
                }
            }
        }
        let mut panel_area = div()
            .id((self.id.clone(), "content-viewport"))
            .test_support()
            .relative()
            .min_w_0()
            .min_h_0()
            .flex_1()
            .when(self.motion != TabsContentMotion::None, |d| {
                d.overflow_hidden()
            });
        if let Some(item) = self
            .items
            .iter()
            .find(|i| selected.as_ref() == Some(&i.key))
        {
            let mut content = div()
                .relative()
                .min_w_0()
                .min_h_0()
                .w_full()
                .when(self.height.is_some(), |d| d.h_full())
                .children(active);
            if self.motion != TabsContentMotion::None {
                let serial = visual.read(cx).serial;
                let progress =
                    Presence::new((self.id.clone(), format!("panel-enter-{serial}")), true)
                        .transition(
                            Transition::new(if changed_layout || initial || self.disabled {
                                Duration::ZERO
                            } else {
                                duration(theme, keys::TABS_CONTENT_DURATION_ENTER)
                            })
                            .ease(easing(theme, keys::MOTION_EASE_ENTER)),
                        )
                        .sample(window, cx)
                        .progress;
                content = content.opacity(progress);
                if self.motion == TabsContentMotion::Slide {
                    let distance =
                        p(keys::TABS_CONTENT_OFFSET) * (1. - progress) * visual.read(cx).direction;
                    content = content
                        .relative()
                        .when(vertical, |d| d.top(distance))
                        .when(!vertical, |d| d.left(distance));
                }
            }
            let panel_id: ElementId = (self.id.clone(), item.key.clone()).into();
            panel_area = panel_area.child(
                TabPanel::new(
                    (panel_id, "panel"),
                    &state,
                    item.key.clone(),
                    item.label.clone(),
                )
                .tab_stop(item.panel_tab_stop)
                .when(item.panel_tab_stop, |d| {
                    d.border(focus_border)
                        .border_color(gpui_kit::transparent_black())
                        .focus_visible(|s| s.border_color(rgba(theme, keys::TABS_FOCUS_RING_COLOR)))
                })
                .w_full()
                .when(self.height.is_some(), |d| d.h_full())
                .content(content),
            );
        } else if let Some(empty) = self.empty {
            panel_area = panel_area.child(empty(window, cx));
        }
        let mut root = div()
            .id(self.id)
            .test_support()
            .flex()
            .w_full()
            .min_w_0()
            .min_h_0()
            .text_size(font_size)
            .line_height(gpui_kit::relative(1.))
            .font_family(crate::typography::ui_font(theme).family);
        root = match placement {
            TabsPlacement::Top => root.flex_col(),
            TabsPlacement::Bottom => root.flex_col_reverse(),
            TabsPlacement::Left => root.flex_row(),
            TabsPlacement::Right => root.flex_row_reverse(),
        };
        root = root.when_some(self.height, |d, h| d.h(h));
        // Logical traversal remains header -> panel at every visual placement.
        root = root.child(header).child(panel_area);
        self.layout.apply(root)
    }
}
