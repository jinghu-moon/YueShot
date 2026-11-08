use super::*;
use crate::{
    ButtonSize, ButtonVariant, InputStatus, InputTextAlign, YueButton, YueDropdownMenu, YueInput,
    YueMenuItem, YueSwatchGroup, YueSwatchOption,
};
use gpui_kit::base::slider::{Slider, SliderIndicator, SliderThumb, SliderTrack};
use gpui_kit::{AnyElement, ClipboardItem, KeyDownEvent, MouseButton, Role};
use std::time::Duration;

gpui_kit::actions!(yue_color_picker, [NextColorField, PreviousColorField]);
type TabHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;
#[derive(Clone, Copy)]
struct PanelStyle {
    theme: YueTheme,
    disabled: bool,
    interactive: bool,
}
const KEY_CONTEXT: &str = "YueColorPicker";
struct KeyBindingsRegistered;
impl gpui_kit::Global for KeyBindingsRegistered {}
pub(super) fn init(cx: &mut App) {
    if cx.try_global::<KeyBindingsRegistered>().is_none() {
        cx.bind_keys([
            gpui_kit::KeyBinding::new("tab", NextColorField, Some(KEY_CONTEXT)),
            gpui_kit::KeyBinding::new("shift-tab", PreviousColorField, Some(KEY_CONTEXT)),
        ]);
        cx.set_global(KeyBindingsRegistered);
    }
}

pub(super) fn body(
    id: ElementId,
    state: &Entity<YueColorPickerState>,
    options: &Options,
    popup: Option<Popup>,
    interactive: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if interactive {
        let editor = state.downgrade();
        let _mount = window.use_keyed_state((id.clone(), "color-panel-mount"), cx, move |_, cx| {
            cx.on_release(move |_, cx| {
                let _ = editor.update(cx, |s, cx| s.cancel(cx));
            })
            .detach();
        });
    }
    let theme = options.theme;
    let disabled = options.disabled || !interactive;
    let style = PanelStyle {
        theme,
        disabled,
        interactive,
    };
    let state = state.clone();
    let (
        color,
        format,
        format_focus,
        copy_focus,
        hue_focus,
        alpha_focus,
        inputs,
        invalid,
        fields,
        recent,
        error,
        copied,
    ) = {
        let s = state.read(cx);
        (
            s.color,
            s.format,
            s.format_focus.clone(),
            s.copy_focus.clone(),
            s.hue_focus.clone(),
            s.alpha_focus.clone(),
            s.inputs.clone(),
            s.invalid,
            s.fields(),
            s.recent().to_vec(),
            s.has_error(),
            s.copied,
        )
    };
    let rec_entry = Rc::new(RefCell::new(vec![]));
    let pre_entry = Rc::new(RefCell::new(vec![]));
    let plane = paint::sv(
        ElementId::from((id.clone(), "sv")),
        &state,
        theme,
        disabled,
        interactive,
        window,
        cx,
    );
    let controls = div()
        .flex()
        .flex_col()
        .gap(rems(theme, "--color-picker-panel-gap"))
        .p(rems(theme, "--color-picker-panel-padding"))
        .child(
            div()
                .flex()
                .items_center()
                .gap(rems(theme, "--color-picker-trigger-gap"))
                .child(paint::preview(
                    color.rgba(),
                    theme,
                    "--color-picker-preview-size",
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(gpui_kit::rems(
                            (pixels(theme, "--color-picker-slider-gap")
                                - (pixels(theme, "--color-picker-hue-hit-height")
                                    - pixels(theme, "--color-picker-hue-height"))
                                    / 2.
                                - (pixels(theme, "--color-picker-alpha-hit-height")
                                    - pixels(theme, "--color-picker-alpha-height"))
                                    / 2.)
                                .max(0.)
                                / 16.,
                        ))
                        .child(slider(false, &state, style, &hue_focus, window, cx))
                        .child(slider(true, &state, style, &alpha_focus, window, cx)),
                ),
        );
    let mut menu = YueDropdownMenu::new(
        (id.clone(), "format-menu"),
        theme,
        YueButton::text((id.clone(), "format-button"), theme, format.name())
            .size(ButtonSize::Small)
            .accessibility_label(format!("颜色格式：{}", format.name()))
            .track_focus(&format_focus)
            .disabled(disabled),
    )
    .label("颜色格式")
    .items(
        [ColorFormat::Hex, ColorFormat::Rgb, ColorFormat::Hsl]
            .into_iter()
            .map(|f| {
                let editor = state.clone();
                YueMenuItem::new(f.name())
                    .radio(format == f)
                    .on_click(move |_, window, cx| {
                        editor.update(cx, |s, cx| s.set_format(f, window, cx));
                    })
            }),
    );
    let editor = state.clone();
    menu = menu.on_open_change(move |open, _, cx| {
        editor.update(cx, |s, cx| {
            s.format_open = *open;
            cx.notify();
        })
    });
    if let Some(host) = &popup {
        let bounds = host.format_bounds.clone();
        menu = menu.on_surface_bounds(move |b| bounds.set(Some(b)));
    }
    let copy_state = state.clone();
    let copy = if copied {
        YueButton::icon(
            (id.clone(), "copy"),
            theme,
            crate::outline::Check,
            "复制请求已提交",
        )
    } else {
        YueButton::icon(
            (id.clone(), "copy"),
            theme,
            crate::outline::Copy,
            "复制颜色",
        )
    };
    let copy = copy
        .size(ButtonSize::Small)
        .variant(ButtonVariant::Ghost)
        .track_focus(&copy_focus)
        .disabled(disabled || error)
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            copy_state.update(cx, |s, cx| {
                if !s.enabled || s.has_error() {
                    return;
                }
                cx.write_to_clipboard(ClipboardItem::new_string(s.color.text(s.format, s.hsv[0])));
                s.copied = true;
                s.copy_task = Some(cx.spawn_in(window, async move |weak, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(1200))
                        .await;
                    let _ = weak.update(cx, |s, cx| {
                        s.copied = false;
                        cx.notify();
                    });
                }));
                cx.notify();
            });
        });
    let actions = if interactive {
        div().flex().items_center().gap_2().child(menu).child(copy)
    } else {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .h(rems(theme, "--button-height-sm"))
                    .px_2()
                    .flex()
                    .items_center()
                    .child(format.name()),
            )
            .child(
                crate::Icon::new(crate::outline::Copy)
                    .size(rems(theme, "--color-picker-swatch-size-sm")),
            )
    };
    let mut controls = controls.child(actions);
    let mut row = div()
        .flex()
        .flex_wrap()
        .gap(rems(theme, "--color-picker-channel-gap"));
    for ix in &fields {
        let ix = *ix;
        let (label, unit) = match ix {
            0 => ("HEX", ""),
            1 => ("红 R", ""),
            2 => ("绿 G", ""),
            3 => ("蓝 B", ""),
            4 => ("色相 H", "°"),
            5 => ("饱和度 S", "%"),
            6 => ("亮度 L", "%"),
            _ => ("透明度 A", "%"),
        };
        let input = YueInput::new((id.clone(), format!("channel-{ix}")), theme, &inputs[ix])
            .accessibility_label(label)
            .accessibility_description(if invalid[ix] {
                "输入无效，请检查范围和格式"
            } else {
                ""
            })
            .status(if invalid[ix] {
                InputStatus::Error
            } else {
                InputStatus::Normal
            })
            .text_align(InputTextAlign::Center)
            .disabled(disabled)
            .width(gpui_kit::relative(1.));
        let field = if interactive {
            input.into_any_element()
        } else {
            div()
                .h(rems(theme, "--input-height-md"))
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(theme, "--color-picker-background"))
                .rounded(rems(theme, "--color-picker-radius"))
                .border_1()
                .border_color(rgba(theme, "--color-picker-border-color"))
                .child(inputs[ix].read(cx).value())
                .into_any_element()
        };
        row = row.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .flex_1()
                .min_w(rems(theme, "--color-picker-channel-min-width"))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgba(theme, "--color-picker-recent-color"))
                        .child(format!("{label}{unit}")),
                )
                .child(field),
        );
    }
    controls = controls.child(row);
    if error {
        controls = controls.child(
            div()
                .text_sm()
                .text_color(rgba(theme, "--color-picker-error-color"))
                .child("输入无效，请检查范围和格式"),
        );
    }
    if options.recent && !recent.is_empty() {
        controls = controls.child(palette(
            (id.clone(), "recent").into(),
            "最近使用颜色",
            recent.into_iter().map(Color::from_rgba).collect(),
            color,
            &state,
            style,
            rec_entry.clone(),
        ));
    }
    if !options.presets.is_empty() {
        controls = controls.child(palette(
            (id.clone(), "presets").into(),
            "预设颜色",
            options.presets.clone(),
            color,
            &state,
            style,
            pre_entry.clone(),
        ));
    }
    let keyboard = state.clone();
    let key_popup = popup.clone();
    let key_rec = rec_entry.clone();
    let key_pre = pre_entry.clone();
    let tab_state = state.clone();
    let tab: TabHandler = Rc::new(move |backwards, window, cx| {
        let s = tab_state.read(cx);
        if !interactive || disabled || s.format_open {
            cx.propagate();
            return;
        }
        let mut order = vec![
            s.focus.clone(),
            s.hue_focus.clone(),
            s.alpha_focus.clone(),
            s.format_focus.clone(),
        ];
        order.extend(s.fields().into_iter().map(|ix| s.input_focus(ix, cx)));
        if !s.has_error() {
            order.push(s.copy_focus.clone());
        }
        order.extend(key_rec.borrow().iter().cloned());
        order.extend(key_pre.borrow().iter().cloned());
        let Some(ix) = order.iter().position(|f| f.is_focused(window)) else {
            cx.propagate();
            return;
        };
        cx.stop_propagation();
        let next = if backwards {
            ix.checked_sub(1)
        } else {
            (ix + 1 < order.len()).then_some(ix + 1)
        };
        if let Some(next) = next {
            order[next].focus(window, cx);
        } else if let Some(host) = &key_popup {
            host.dismiss(&tab_state, window, cx, true);
            if !backwards {
                window.focus_next(cx);
            }
        } else if backwards {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
    });
    let forward = tab.clone();
    let backward = tab.clone();
    let indent = tab.clone();
    let mut body = div()
        .id((id, "color-panel"))
        .test_support()
        .key_context(KEY_CONTEXT)
        .on_action(move |_: &NextColorField, window, cx| forward(false, window, cx))
        .on_action(move |_: &PreviousColorField, window, cx| backward(true, window, cx))
        .capture_action(move |_: &gpui_kit::base::input::IndentInline, window, cx| {
            indent(false, window, cx)
        })
        .capture_action(
            move |_: &gpui_kit::base::input::OutdentInline, window, cx| tab(true, window, cx),
        )
        .when(interactive, |b| {
            b.role(Role::Group)
                .aria_label(format!("{}调色面板", options.name))
        })
        .flex()
        .flex_col()
        .w_full()
        .bg(rgba(theme, "--color-picker-panel-background"))
        .rounded(rems(theme, "--color-picker-panel-radius"))
        .child(plane)
        .child(controls)
        // Base Input emits PressEnter and propagates its Enter action. The
        // retained input subscription confirms once; consume the action here
        // so a containing form cannot submit the color editor's keystroke.
        .on_action(|_: &gpui_kit::base::input::Enter, _, cx| cx.stop_propagation())
        .capture_key_down(move |event: &KeyDownEvent, window, cx| {
            if !interactive || disabled || keyboard.read(cx).format_open {
                return;
            }
            let modifiers = event.keystroke.modifiers;
            if modifiers.control || modifiers.alt || modifiers.platform {
                return;
            }
            let key = event.keystroke.key.as_str();
            if matches!(key, "left" | "right" | "up" | "down" | "home" | "end") {
                let (plane, hue, alpha) = {
                    let s = keyboard.read(cx);
                    (
                        s.focus.is_focused(window),
                        s.hue_focus.is_focused(window),
                        s.alpha_focus.is_focused(window),
                    )
                };
                if plane || hue || alpha {
                    cx.stop_propagation();
                    keyboard.update(cx, |s, cx| {
                        s.begin();
                        s.keyboard_session = true;
                        let step = if modifiers.shift { 10. } else { 1. };
                        if plane {
                            match key {
                                "left" => s.hsv[1] -= step / 100.,
                                "right" => s.hsv[1] += step / 100.,
                                "up" => s.hsv[2] += step / 100.,
                                "down" => s.hsv[2] -= step / 100.,
                                _ => {}
                            }
                            s.hsv[1] = s.hsv[1].clamp(0., 1.);
                            s.hsv[2] = s.hsv[2].clamp(0., 1.);
                        } else if hue {
                            s.hsv[0] = match key {
                                "home" => 0.,
                                "end" => 359.,
                                "left" | "down" => (s.hsv[0] - step).rem_euclid(360.),
                                _ => (s.hsv[0] + step).rem_euclid(360.),
                            };
                        } else {
                            s.alpha = match key {
                                "home" => 0.,
                                "end" => 1.,
                                "left" | "down" => (s.alpha - step / 100.).max(0.),
                                _ => (s.alpha + step / 100.).min(1.),
                            };
                        }
                        s.apply_coordinates(window, cx);
                    });
                }
            }
        });
    let keyup = state.clone();
    body = body.on_key_up(move |e, _, cx| {
        if interactive
            && keyup.read(cx).keyboard_session
            && matches!(
                e.keystroke.key.as_str(),
                "left" | "right" | "up" | "down" | "home" | "end"
            )
        {
            keyup.update(cx, |s, cx| s.finish(true, cx));
        }
    });
    if let Some(host) = popup.filter(|_| interactive) {
        let on_escape = host.clone();
        let editor = state.clone();
        body = body.on_key_down(move |e, window, cx| {
            if e.keystroke.key == "escape" && !editor.read(cx).format_open {
                cx.stop_propagation();
                on_escape.dismiss(&editor, window, cx, true);
            }
        });
        let editor = state.clone();
        body = body.on_mouse_down_out(move |e, window, cx| {
            if host
                .anchor
                .handle()
                .bounds()
                .is_some_and(|b| b.contains(&e.position))
            {
                return;
            }
            if editor.read(cx).format_open
                && host
                    .format_bounds
                    .get()
                    .is_some_and(|b| b.contains(&e.position))
            {
                return;
            }
            host.dismiss(&editor, window, cx, false);
        });
    }
    body.into_any_element()
}
fn palette(
    id: ElementId,
    label: &'static str,
    colors: Vec<Color>,
    current: Color,
    state: &Entity<YueColorPickerState>,
    style: PanelStyle,
    entry: Rc<RefCell<Vec<FocusHandle>>>,
) -> AnyElement {
    let PanelStyle {
        theme,
        disabled,
        interactive,
    } = style;
    let editor = state.clone();
    let values = colors.iter().map(|c| (c.hex(), *c)).collect::<Vec<_>>();
    let mut group = YueSwatchGroup::new(id, theme, current.hex())
        .label(label)
        .disabled(disabled)
        .width(gpui_kit::relative(1.))
        .options(
            colors
                .into_iter()
                .map(|c| YueSwatchOption::solid(c.hex(), c.hex(), c.rgba())),
        );
    group.on_activate = Some(Rc::new(move |value, window, cx| {
        if let Some((_, color)) = values.iter().find(|(v, _)| v == value.as_str()) {
            editor.update(cx, |s, cx| s.select(*color, window, cx));
        }
    }));
    group.entry = Some(entry);
    group.paint_only = !interactive;
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_sm()
                .text_color(rgba(theme, "--color-picker-recent-color"))
                .child(label),
        )
        .child(group)
        .into_any_element()
}
fn slider(
    alpha: bool,
    state: &Entity<YueColorPickerState>,
    style: PanelStyle,
    focus: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let PanelStyle {
        theme,
        disabled,
        interactive,
    } = style;
    let s = state.read(cx);
    let slider = if alpha {
        s.alpha_slider.clone()
    } else {
        s.hue.clone()
    };
    let value = if alpha {
        s.alpha as f32
    } else {
        s.hsv[0] as f32 / 360.
    };
    let color = s.color;
    let thumb_color = if alpha {
        color.rgba()
    } else {
        Color::from_hsv([s.hsv[0], 1., 1.], 1.).rgba()
    };
    let height = if alpha {
        "--color-picker-alpha-height"
    } else {
        "--color-picker-hue-height"
    };
    let hit = if alpha {
        "--color-picker-alpha-hit-height"
    } else {
        "--color-picker-hue-hit-height"
    };
    let focus = focus.clone();
    let pressed_focus = focus.clone();
    let ring_visible = !disabled && focus.is_focused(window) && window.last_input_was_keyboard();
    let scale = f32::from(window.rem_size()) / 16.;
    let thumb = px(pixels(theme, "--color-picker-thumb-size") * scale);
    let thumb_scale = if interactive {
        transition(
            (
                ElementId::from(("color-picker-slider", slider.entity_id())),
                "press",
            ),
            if !disabled && slider.read(cx).is_dragging() {
                crate::tokens::number(theme, "--color-picker-thumb-drag-scale")
            } else {
                1.
            },
            Transition::new(duration(theme, "--color-picker-state-duration"))
                .ease(easing(theme, "--color-picker-state-ease")),
            window,
            cx,
        )
    } else {
        1.
    };
    // Snap the radius, not each edge separately: both sides stay centered at
    // fractional Windows display scales while the Base hitbox remains fixed.
    let display_scale = window.scale_factor();
    let painted_thumb =
        px((f32::from(thumb) * thumb_scale * display_scale / 2.).round() * 2. / display_scale);
    if !interactive {
        return div()
            .h(rems(theme, hit))
            .w_full()
            .relative()
            .flex()
            .items_center()
            .child(
                div()
                    .w_full()
                    .h(rems(theme, height))
                    .relative()
                    .rounded_full()
                    .overflow_hidden()
                    .child(paint::slider_ink(alpha, color, theme)),
            )
            .child(
                div()
                    .absolute()
                    .top(px((pixels(theme, hit) * scale - f32::from(thumb)) / 2.))
                    .left(gpui_kit::relative(value))
                    .ml(-thumb / 2.)
                    .size(thumb)
                    .child(paint::slider_thumb(alpha, thumb_color, theme, thumb)),
            )
            .into_any_element();
    }
    let body = Slider::new(&slider)
        .accessibility_label(if alpha { "透明度" } else { "色相" })
        .disabled(disabled)
        .when(!disabled, |b| b.track_focus(&focus))
        .w_full()
        .child(
            SliderTrack::new(&slider)
                .disabled(disabled)
                .h(rems(theme, hit))
                .w_full()
                .flex()
                .items_center()
                .relative()
                .child(
                    SliderIndicator::new(&slider)
                        .relative()
                        .w_full()
                        .h(rems(theme, height))
                        .rounded_full()
                        .overflow_hidden()
                        .child(paint::slider_ink(alpha, color, theme)),
                )
                .child(
                    SliderThumb::new(&slider)
                        .disabled(disabled)
                        .absolute()
                        .top(px((pixels(theme, hit) * scale - f32::from(thumb)) / 2.))
                        .left(gpui_kit::relative(value))
                        .ml(-thumb / 2.)
                        .size(thumb)
                        .child(paint::slider_thumb(
                            alpha,
                            thumb_color,
                            theme,
                            painted_thumb,
                        )),
                ),
        );
    div()
        .id(("color-slider-focus", slider.entity_id()))
        .test_support()
        .when(!disabled, |b| b.cursor_pointer())
        .when(!disabled, |b| {
            b.capture_any_mouse_down(move |event, window, cx| {
                if event.button == MouseButton::Left {
                    pressed_focus.focus(window, cx);
                }
            })
        })
        .relative()
        .child(body)
        .child(
            div()
                .absolute()
                .inset(-gpui_kit::rems(
                    (pixels(theme, "--color-picker-focus-ring-offset")
                        + pixels(theme, "--color-picker-focus-ring-width"))
                        / 16.,
                ))
                .rounded_full()
                .border(rems(theme, "--color-picker-focus-ring-width"))
                .border_color(if ring_visible {
                    rgba(theme, "--color-picker-focus-ring-color")
                } else {
                    gpui_kit::transparent_black().into()
                }),
        )
        .into_any_element()
}
