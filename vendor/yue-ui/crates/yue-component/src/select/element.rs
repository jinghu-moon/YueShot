//! Select field, selected values and trigger controls.

use super::{behavior::*, model::*, state::SelectResultState, *};

use crate::tokens::{duration, easing, rems, rgba};

use crate::{Icon, YueOverflowTooltip, outline};

use gpui_kit::base::{Button, TestSupportExt as _, Transition, transition};

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, Context, InteractiveElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, prelude::FluentBuilder as _, px,
};

use std::time::Duration;

use super::layout::*;

use super::panel::*;

pub(super) fn render(
    state: &mut YueSelectState,
    window: &mut Window,
    cx: &mut Context<YueSelectState>,
) -> AnyElement {
    let Some(p) = state.presentation.clone() else {
        return div().into_any_element();
    };
    assert!(
        !p.name.trim().is_empty(),
        "YueSelect requires accessibility_label or YueField label"
    );
    let entity = cx.entity();
    let theme = p.theme.unwrap_or_else(|| crate::current_theme(cx));
    let id = p.id.clone();
    let height = metric(
        theme,
        &format!("--input-height-{}", p.size.suffix()),
        window,
    );
    let padding = metric(
        theme,
        &format!("--input-padding-inline-{}", p.size.suffix()),
        window,
    );
    let font = metric(
        theme,
        &format!("--input-font-size-{}", p.size.suffix()),
        window,
    );
    let icon = metric(
        theme,
        &format!("--input-icon-size-{}", p.size.suffix()),
        window,
    );
    let gap = metric(theme, "--input-gap", window);
    let tag_gap = metric(theme, "--select-tag-gap", window);
    let selected = state.selected_items();
    let label: SharedString = selected
        .iter()
        .map(|option| option.label.as_str())
        .collect::<Vec<_>>()
        .join("、")
        .into();
    let default_width = metric(theme, "--select-width", window);
    let root_width = match p.width {
        Width::Fill => None,
        Width::Content => Some(
            (measure(
                if label.is_empty() {
                    &p.placeholder
                } else {
                    &label
                },
                font,
                window,
            ) + 2. * padding
                + icon
                + gap)
                .max(height * 2.),
        ),
        Width::Default => Some(p.layout.width.map_or(default_width, |width| {
            width.to_pixels(window.viewport_size().width.into(), window.rem_size())
        })),
    };
    let available = if state.measured_width > px(0.) {
        state.measured_width
    } else {
        root_width.unwrap_or(default_width)
    };
    let minimum = 2. * padding + 2. * icon + gap;
    let editable = state.editable();
    let focused = state.focus.contains_focused(window, cx);
    let border_key = crate::input::border_token(p.status, state.disabled, focused || state.open);
    let border_target = rgba(
        theme,
        if p.status == InputStatus::Normal
            && !focused
            && !state.open
            && state.field_hover
            && !state.disabled
        {
            "--input-border-color-hover"
        } else {
            border_key.name()
        },
    );
    let background = transition(
        (id.clone(), "select-background"),
        gpui_kit::Hsla::from(if state.disabled {
            rgba(theme, "--input-background-disabled")
        } else if state.readonly {
            rgba(theme, "--input-background-readonly")
        } else if p.variant == SelectVariant::Text {
            if state.field_hover || state.open {
                rgba(theme, "--menu-item-background-hover")
            } else {
                gpui_kit::rgba(0)
            }
        } else {
            rgba(theme, "--input-background")
        }),
        Transition::new(duration(theme, "--input-duration")).ease(easing(theme, "--input-ease")),
        window,
        cx,
    );
    let border = transition(
        (id.clone(), "select-border"),
        gpui_kit::Hsla::from(border_target),
        Transition::new(if state.disabled {
            Duration::ZERO
        } else {
            duration(theme, "--input-duration")
        })
        .ease(easing(theme, "--input-ease")),
        window,
        cx,
    );
    let foreground = rgba(
        theme,
        if state.disabled {
            "--input-color-disabled"
        } else {
            "--input-color"
        },
    );
    let mut content = div()
        .id((id.clone(), "selection"))
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(tag_gap);
    if p.tag_layout == SelectTagLayout::Wrap {
        content = content.flex_wrap();
    }
    state.hidden_keys.clear();
    if let Some(renderer) = &p.value_renderer {
        content = content.child(renderer(&selected, window, cx));
    } else if selected.is_empty() {
        content = content.child(
            div()
                .min_w_0()
                .truncate()
                .text_color(rgba(theme, "--input-placeholder-color"))
                .child(p.placeholder.clone()),
        );
    } else if state.mode == SelectMode::Single {
        content = content.child(
            YueOverflowTooltip::new((id.clone(), "selected-label"), theme, label.clone())
                .width(gpui_kit::relative(1.))
                .track_focus(&state.focus),
        );
    } else {
        let budget = (available
            - 2. * padding
            - icon
            - gap
            - if p.clearable { icon + gap } else { px(0.) }
            - if p.icon.is_some() { icon + gap } else { px(0.) }
            - p.prefix
                .as_ref()
                .map_or(px(0.), |prefix| measure(prefix, font, window) + gap))
        .max(px(0.));
        let tag_font = metric(
            theme,
            &format!("--tag-font-size-{}", p.size.suffix()),
            window,
        );
        let tag_padding = metric(
            theme,
            &format!("--tag-padding-inline-{}", p.size.suffix()),
            window,
        );
        let widths = selected
            .iter()
            .map(|option| {
                measure(&option.label, tag_font, window)
                    + 2. * tag_padding
                    + if state.removable(&option.key) {
                        metric(theme, "--tag-close-icon-size", window) + tag_gap
                    } else {
                        px(0.)
                    }
                    + 2. * metric(theme, "--tag-border-width", window)
            })
            .collect::<Vec<_>>();
        let mut show = if p.tag_layout == SelectTagLayout::Wrap {
            selected.len()
        } else if p.tag_layout == SelectTagLayout::Collapse {
            p.collapse_count.min(selected.len())
        } else {
            selected.len()
        };
        if p.tag_layout != SelectTagLayout::Wrap {
            while show > 0 {
                let used = widths[..show].iter().copied().fold(px(0.), |a, b| a + b)
                    + tag_gap * (show.saturating_sub(1) as f32);
                let count = if show < selected.len() {
                    measure(
                        &format!("+{}", selected.len() - show).into(),
                        tag_font,
                        window,
                    ) + 2. * tag_padding
                        + tag_gap
                } else {
                    px(0.)
                };
                if used + count <= budget {
                    break;
                }
                show -= 1;
            }
        }
        for (ix, option) in selected.iter().enumerate() {
            if ix >= show {
                state.hidden_keys.push(option.key.clone());
                continue;
            }
            let mut tag = YueTag::new(
                (id.clone(), format!("tag:{}", option.key)),
                option.label.clone(),
            )
            .theme(theme)
            .size(p.size.tag())
            .overflow_tooltip(true)
            .max_width(budget.max(px(1.)))
            .disabled(state.disabled);
            if let Some(renderer) = &p.tag_renderer {
                tag = renderer(tag, option, window, cx);
            }
            tag = tag.reset_select_behavior().disabled(state.disabled);
            if state.removable(&option.key) {
                let key = option.key.clone();
                let owner = entity.clone();
                tag = tag
                    .close_label(format!("移除 {}", option.label))
                    .close_tab_stop(p.tab_stop)
                    .track_close_focus(
                        state
                            .tag_focus
                            .get(&option.key)
                            .expect("selected tag focus"),
                    )
                    .on_close(move |_, window, cx| {
                        cx.stop_propagation();
                        owner.update(cx, |state, cx| {
                            state.remove(&key, SelectChangeSource::TagRemove, window, cx)
                        })
                    });
            }
            content = content.child(tag);
        }
        if show < selected.len() {
            let owner = entity.clone();
            let count = selected.len() - show;
            let mut count_button = Button::new((id.clone(), "collapsed-count"))
                .accessibility_label(format!("查看其余 {count} 个选择"))
                .track_focus(&state.collapsed_focus)
                .tab_stop(p.tab_stop && !state.disabled)
                .disabled(state.disabled || state.readonly)
                .cursor_pointer()
                .h(metric(
                    theme,
                    &format!("--tag-height-{}", p.size.suffix()),
                    window,
                ))
                .px(tag_padding)
                .rounded(rems(theme, "--tag-border-radius"))
                .bg(rgba(theme, "--tag-background"))
                .text_color(foreground)
                .relative()
                .child(state.collapse_anchor.capture())
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    owner.update(cx, |state, cx| {
                        let open = !state.collapsed_open;
                        if state.open {
                            state.close(SelectCloseReason::Trigger, false, window, cx);
                        }
                        state.collapsed_open = open;
                        if open {
                            state.collapsed_focus.focus(window, cx);
                        }
                        cx.notify();
                    });
                });
            count_button = if let Some(renderer) = &p.collapse_renderer {
                count_button.child(renderer(count, window, cx))
            } else {
                count_button.child(format!("+{count}"))
            };
            content = content.child(count_button);
        }
    }
    if state.collapsed_open && state.hidden_keys.is_empty() {
        state.collapsed_open = false;
        state.focus.focus(window, cx);
    }
    let trigger_owner = entity.clone();
    // Only the value/empty-space activation zone toggles the popup. Tag closes,
    // clear and +N are sibling controls; no whole-frame click handler can steal
    // their activation or nest one native Button inside another.
    let mut activation = div()
        .id((id.clone(), "trigger-value"))
        .test_support()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(gap)
        .when(editable, |this| {
            this.cursor_pointer().on_click(move |_, window, cx| {
                trigger_owner.update(cx, |state, cx| {
                    state.keyboard_navigation = false;
                    if state.open {
                        state.close(SelectCloseReason::Trigger, true, window, cx);
                    } else {
                        state.focus.focus(window, cx);
                        state.change_open(true, SelectCloseReason::Trigger, true, window, cx);
                        state.content_focus(cx).focus(window, cx);
                    }
                });
            })
        });
    if let Some(icon_image) = p.icon {
        activation = activation.child(
            icon_image
                .size(gpui_kit::rems(
                    f32::from(icon) / f32::from(window.rem_size()),
                ))
                .color_value(foreground),
        );
    }
    if let Some(prefix) = &p.prefix {
        activation = activation.child(div().flex_shrink_0().child(prefix.clone()));
    }
    activation = activation.child(content);
    let hover_owner = entity.clone();
    let mut row = div()
        .id((id.clone(), "frame"))
        .test_support()
        .key_context("YueSelectTrigger")
        .relative()
        .w_full()
        .min_w(minimum)
        .min_h(height)
        .px(padding)
        .flex()
        .items_center()
        .gap(gap)
        .text_size(font)
        .text_color(foreground)
        .border(rems(theme, "--input-border-width"))
        .border_color(
            if p.variant == SelectVariant::Text
                && !focused
                && !state.open
                && p.status == InputStatus::Normal
            {
                gpui_kit::Hsla::from(gpui_kit::rgba(0))
            } else {
                border
            },
        )
        .rounded(rems(theme, "--input-border-radius"))
        .bg(background)
        .on_hover(move |hover, _, cx| {
            hover_owner.update(cx, |state, cx| {
                if state.field_hover != *hover {
                    state.field_hover = *hover;
                    cx.notify();
                }
            })
        })
        .child(activation);
    row = if p.tag_layout == SelectTagLayout::Wrap && state.mode == SelectMode::Multiple {
        row.py(
            ((height - metric(theme, &format!("--tag-height-{}", p.size.suffix()), window)) / 2.
                - metric(theme, "--input-border-width", window))
            .max(px(0.)),
        )
    } else {
        row.h(height)
    };
    if p.clearable {
        let can_clear = state.value.keys().iter().any(|key| state.removable(key));
        let owner = entity.clone();
        row = row.child(
            Button::new((id.clone(), "clear"))
                .accessibility_label(
                    if state.value.keys().iter().any(|key| !state.removable(key)) {
                        "清除可移除的选择"
                    } else {
                        "清空选择"
                    },
                )
                .disabled(!can_clear)
                .tab_stop(p.tab_stop && can_clear)
                .size(icon)
                .flex_shrink_0()
                .cursor_pointer()
                .opacity(if can_clear && (state.field_hover || focused) {
                    1.
                } else {
                    0.
                })
                .on_click(move |_, window, cx| {
                    owner.update(cx, |state, cx| state.clear(window, cx))
                })
                .child(
                    Icon::new(outline::X)
                        .size(gpui_kit::rems(
                            f32::from(icon) / f32::from(window.rem_size()),
                        ))
                        .color_value(rgba(theme, "--input-clear-color")),
                ),
        );
    }
    let arrow_owner = entity.clone();
    let open = state.open;
    let arrow_rotation = transition(
        (id.clone(), "arrow-rotation"),
        if open { 0.5_f32 } else { 0_f32 },
        Transition::new(duration(theme, "--input-duration")).ease(easing(theme, "--input-ease")),
        window,
        cx,
    );
    row = row.child(
        Button::new((id.clone(), "arrow"))
            .accessibility_label(if open { "收起选项" } else { "展开选项" })
            .tab_stop(false)
            .disabled(!editable)
            .size(icon)
            .cursor_pointer()
            .on_click(move |_, window, cx| {
                arrow_owner.update(cx, |state, cx| {
                    if state.open {
                        state.close(SelectCloseReason::Trigger, true, window, cx);
                    } else {
                        state.change_open(true, SelectCloseReason::Trigger, true, window, cx);
                        state.content_focus(cx).focus(window, cx);
                    }
                })
            })
            .child({
                let loading = state.loading || matches!(state.result, SelectResultState::Loading);
                let image = Icon::new(if loading {
                    crate::YueIcon::Loading
                } else {
                    crate::YueIcon::ChevronDown
                })
                .size(gpui_kit::rems(
                    f32::from(icon) / f32::from(window.rem_size()),
                ))
                .rotation(if loading { 0. } else { arrow_rotation })
                .color_value(foreground);
                if loading
                    && !cx.reduce_motion()
                    && window.is_window_active()
                    && state.anchor.handle().bounds().is_some()
                {
                    image
                        .with_animation(
                            (id.clone(), "spinner"),
                            Animation::new(duration(theme, "--button-spinner-duration")).repeat(),
                            |icon, delta| icon.rotation(delta),
                        )
                        .into_any_element()
                } else {
                    image.into_any_element()
                }
            }),
    );
    if focused && !state.disabled {
        let outside = metric(theme, "--input-focus-ring-width", window)
            + metric(theme, "--input-focus-ring-offset", window);
        // Absolute insets start inside the frame's border; the ring's offset
        // is measured from its outside edge.
        let inset = outside + metric(theme, "--input-border-width", window);
        row = row.child(
            div()
                .absolute()
                .top(-inset)
                .bottom(-inset)
                .left(-inset)
                .right(-inset)
                .rounded(metric(theme, "--input-border-radius", window) + outside)
                .border(rems(theme, "--input-focus-ring-width"))
                .border_color(rgba(theme, "--input-focus-ring-color")),
        );
    }
    let measured = entity.downgrade();
    let measurement = canvas(
        move |bounds, window, cx| {
            let mounted_owner = measured.clone();
            let handle = window.window_handle();
            let _mount = window.use_keyed_state("select-mount-session", cx, |_, cx| {
                cx.on_release(move |_, cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = mounted_owner.update(cx, |state, cx| {
                            state.close(SelectCloseReason::Lifecycle, false, window, cx);
                        });
                    });
                })
                .detach();
            });
            let width = bounds.size.width;
            let _ = measured.update(cx, |state, _| {
                if state.measured_width != width {
                    state.measured_width = width;
                    let refresh = measured.clone();
                    window.on_next_frame(move |_, cx| {
                        let _ = refresh.update(cx, |_, cx| cx.notify());
                    });
                }
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0();
    let focus = state.focus.clone();
    let content_focus = state.content_focus(cx);
    let owner = entity.clone();
    let confirm = entity.clone();
    let mut semantic = gpui_kit::base::Select::new(id.clone())
        .open(state.open)
        .disabled(state.disabled)
        .readonly(state.readonly)
        .required(p.required)
        .invalid(p.status == InputStatus::Error)
        .tab_stop(p.tab_stop)
        .tab_index(p.tab_index)
        .focus_handle(&focus)
        .content_focus_handle(&content_focus)
        .accessibility_label(p.name.clone())
        .accessibility_value(label)
        .on_open_request(move |open, window, cx| {
            owner.update(cx, |state, cx| {
                state.keyboard_navigation = true;
                state.change_open(open, SelectCloseReason::Trigger, true, window, cx)
            })
        })
        .on_confirm(move |window, cx| {
            confirm.update(cx, |state, cx| {
                if let Some(key) = state.active.clone() {
                    state.activate(key, SelectChangeSource::Keyboard, window, cx);
                }
            })
        })
        .child(row)
        .w_full();
    if let Some(description) = &p.description {
        semantic = semantic.accessibility_description(description.clone());
    }
    if let Some(popup) = popup(state, false, &p, theme, window, cx) {
        semantic = semantic.child(popup);
    }
    if let Some(popup) = popup(state, true, &p, theme, window, cx) {
        semantic = semantic.child(popup);
    }
    let mut outer = div()
        .id((id.clone(), "select-owner"))
        .relative()
        .key_context("YueSelect")
        .flex()
        .flex_col()
        .text_left()
        .min_w(minimum)
        .when_some(p.key_context.clone(), |this, context| {
            this.key_context(context.as_str())
        });
    outer = match p.width {
        Width::Fill => outer.w_full(),
        _ => outer.w(root_width.unwrap_or(default_width)),
    };
    let layout = (*p.layout).clone();
    outer = layout.apply(outer);
    // Measure the border box, rather than the row's border-inset content box.
    outer = actions(outer, cx)
        .child(semantic)
        .child(state.anchor.capture())
        .child(measurement);
    outer.into_any_element()
}
