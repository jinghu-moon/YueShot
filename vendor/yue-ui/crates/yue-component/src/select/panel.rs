//! Select popup, search, slots and list viewport.

use super::{model::*, state::SelectResultState, *};

use crate::popup::{
    PopupDismissReason, PopupDismissRoute, PopupHost, PopupId, PopupPlacement, PopupPointerMode,
    PopupSpec,
};

use crate::tokens::{rems, rgba, shadow};

use crate::{InputType, YueInput, outline};

use gpui_kit::base::{
    A11ySelectionExt as _, Align, Button, InteractiveElementExt as _, Placement, SelectionA11y,
    TestSupportExt as _,
    virtualization::{VirtualList, VirtualizerChange, VirtualizerScrollOptions, VirtualizerSizing},
};

use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, ParentElement as _, Pixels, Role,
    StatefulInteractiveElement as _, Styled as _, canvas, div, prelude::FluentBuilder as _, px,
};

use std::rc::Rc;

use super::layout::*;

use super::option::*;

pub(super) fn popup(
    state: &mut YueSelectState,
    collapsed: bool,
    p: &Presentation,
    theme: YueTheme,
    window: &mut Window,
    cx: &mut Context<YueSelectState>,
) -> Option<AnyElement> {
    let present = if collapsed {
        state.collapsed_open
    } else {
        state.open
    };
    let entity = cx.entity();
    let id = p.id.clone();
    let anchor = if collapsed {
        state.collapse_anchor.handle()
    } else {
        state.anchor.handle()
    };
    let margin = metric(theme, "--space-8", window);
    let shadows = shadow(theme, "--popover-shadow");
    // Shadows may clip at a window edge. Reserve space for the selectable
    // surface, not its diffuse decoration, so a fitting trigger stays aligned.
    let outsets: gpui_kit::Edges<Pixels> = Default::default();
    let available = PopupHost::available_size(None, outsets, margin, window);
    let trigger_width = if state.measured_width > px(0.) {
        state.measured_width
    } else {
        p.layout
            .width
            .map_or(metric(theme, "--select-width", window), |width| {
                width.to_pixels(window.viewport_size().width.into(), window.rem_size())
            })
    };
    let font = metric(
        theme,
        &format!("--input-font-size-{}", p.size.suffix()),
        window,
    );
    let width = match p.panel_width {
        SelectPanelWidth::MatchTrigger => trigger_width,
        SelectPanelWidth::Fixed(width) => width,
        SelectPanelWidth::Content => {
            let face = format!("{:?}:{:?}", window.text_style().font(), p.row_layout);
            let cached = state
                .width_cache
                .as_ref()
                .filter(|(rows, old_font, old_face, _)| {
                    Rc::ptr_eq(rows, &state.rows) && *old_font == font && *old_face == face
                });
            let measured = if let Some((_, _, _, measured)) = cached {
                *measured
            } else {
                let measured = state
                    .rows
                    .iter()
                    .map(|row| match row {
                        Row::Option(option) => {
                            measure(&option.label, font, window).max(
                                option
                                    .description
                                    .as_ref()
                                    .filter(|_| p.row_layout == SelectRowLayout::Description)
                                    .map_or(px(0.), |text| measure(text, font, window)),
                            ) + option
                                .meta
                                .as_ref()
                                .map_or(px(0.), |text| measure(text, font, window))
                                + metric(theme, "--input-height-md", window) * 2.
                        }
                        Row::Group(_, label) | Row::Create(label) => {
                            measure(label, font, window)
                                + metric(theme, "--input-height-md", window) * 2.
                        }
                    })
                    .fold(px(0.), Pixels::max);
                state.width_cache = Some((state.rows.clone(), font, face, measured));
                measured
            };
            measured.max(trigger_width)
        }
    }
    .min(available.width)
    .max(px(1.));
    let anchor_bounds = anchor.bounds();
    let gap = metric(theme, "--select-panel-gap", window);
    let side_available = anchor_bounds.map_or(available.height, |bounds| {
        (window.viewport_size().height - bounds.bottom() - gap - margin - outsets.bottom)
            .max(bounds.top() - gap - margin - outsets.top)
    });
    let maximum = available.height.min(side_available).max(px(1.));
    let owner = entity.clone();
    let mut spec = PopupSpec::new(
        PopupId::new(
            cx.entity_id(),
            (
                id.clone(),
                if collapsed {
                    "select-collapsed-popup"
                } else {
                    "select-popup"
                },
            ),
        ),
        anchor,
        present,
        PopupPlacement::Side(Placement::Bottom, Align::Start),
    )
    .surface_gap(gap)
    .margin(margin)
    .pointer_mode(PopupPointerMode::Block)
    .dismiss_route(PopupDismissRoute::Runtime)
    // Outside presses use the shared router; query Escape stays with the
    // owner so IME composition can consume it before closing the field.
    .dismiss_on(collapsed, true)
    .dismiss_unplaceable()
    .exclude(state.anchor.handle())
    .exclude(state.collapse_anchor.handle())
    .visual_outsets(outsets)
    .motion(crate::motion::dropdown_popup(theme))
    .on_dismiss(move |request, window, cx| {
        owner.update(cx, |state, cx| {
            let escape = request.reason() == PopupDismissReason::Escape;
            if collapsed {
                state.collapsed_open = false;
                if escape {
                    state.collapsed_focus.focus(window, cx);
                }
                cx.notify();
            } else {
                state.close(
                    if escape {
                        SelectCloseReason::Escape
                    } else if request.reason() == PopupDismissReason::OutsidePress {
                        SelectCloseReason::OutsidePress
                    } else {
                        SelectCloseReason::Lifecycle
                    },
                    escape,
                    window,
                    cx,
                );
            }
        });
    });
    if let Some(parent) = &state.parent {
        spec = spec.parent(parent.clone());
    }
    let use_virtual = p.virtualization == SelectVirtualization::Always
        || (p.virtualization == SelectVirtualization::Auto && state.rows.len() > 100);
    state.virtualized = use_virtual;
    let p = p.clone();
    let scroll = state.scroll.clone();
    let input = if state.search == SelectSearch::None {
        None
    } else {
        state.input.clone()
    };
    let multiple = state.mode == SelectMode::Multiple;
    let rows = state.rows.clone();
    let option_h = option_height(&p, theme, window);
    let group_h = metric(theme, "--input-height-sm", window);
    let geometry = if let Some(cached) = state.list_cache.as_ref().filter(|cached| {
        Rc::ptr_eq(&cached.rows, &rows)
            && cached.option_height == option_h
            && cached.group_height == group_h
    }) {
        cached.clone()
    } else {
        let extents = rows
            .iter()
            .map(|row| {
                if matches!(row, Row::Group(..)) {
                    group_h
                } else {
                    option_h
                }
            })
            .collect::<Vec<_>>();
        let source = Rc::new(super::state::SelectVirtualSource {
            rows: rows.clone(),
            positions: state.positions.clone(),
        });
        state.virtualizer.update(cx, |virtualizer, _| {
            virtualizer
                .replace_source(
                    source,
                    VirtualizerSizing::known(Rc::<[Pixels]>::from(extents)),
                    VirtualizerChange::Replace {
                        preserve_anchor: true,
                    },
                )
                .expect("valid Select geometry");
        });
        let total = Pixels::from(state.virtualizer.read(cx).total_extent());
        let cached = Rc::new(super::state::ListGeometry {
            rows: rows.clone(),
            option_height: option_h,
            group_height: group_h,
            total,
            option_count: rows
                .iter()
                .filter(|row| matches!(row, Row::Option(_)))
                .count(),
            has_icons: rows
                .iter()
                .any(|row| matches!(row, Row::Option(option) if option.icon.is_some())),
        });
        state.list_cache = Some(cached.clone());
        cached
    };
    if use_virtual && let Some(key) = state.pending_reveal.take() {
        state.virtualizer.update(cx, |virtualizer, _| {
            let _ = virtualizer.scroll_to_key(key, VirtualizerScrollOptions::default());
        });
    }
    let virtualizer = state.virtualizer.clone();
    let inline_rows = if !use_virtual && !collapsed {
        Some(
            rows.iter()
                .map(|row| render_row(row, state, &p, theme, present, window, cx))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let hidden = state
        .selected_items()
        .into_iter()
        .filter(|option| state.hidden_keys.contains(&option.key))
        .collect::<Vec<_>>();
    let focus = if collapsed {
        state.collapsed_focus.clone()
    } else {
        state.list_focus.clone()
    };
    let result = state.result.clone();
    let more = state.more_result.clone();
    let create = state.create_result.clone();
    let has_more = state.has_more();
    let at_limit = state.at_limit();
    let limit = state.max_selected;
    let slot_heights = state.slot_heights;
    PopupHost::new(spec).mount(
        move |presentation, _, window, cx| {
            let interactive = presentation.is_interactive();
            let padding = metric(theme, "--popover-padding", window);
            let row_h = option_height(&p, theme, window);
            let mut panel = div()
                .id((
                    id.clone(),
                    if collapsed {
                        "collapsed-panel"
                    } else {
                        "select-panel"
                    },
                ))
                .test_support()
                .key_context("YueSelectPanel")
                .w(width)
                .max_h(maximum)
                .flex()
                .flex_col()
                .text_left()
                .overflow_hidden()
                .p(padding)
                .gap(metric(theme, "--space-4", window))
                .bg(rgba(theme, "--popover-background"))
                .border(rems(theme, "--popover-border-width"))
                .border_color(rgba(theme, "--popover-border-color"))
                .rounded(rems(theme, "--popover-border-radius"))
                .shadow(shadows)
                .when(!interactive, |this| this.occlude());
            if collapsed {
                let mut list = div()
                    .id((id.clone(), "hidden-selections"))
                    .flex()
                    .flex_col()
                    .gap(metric(theme, "--select-tag-gap", window))
                    .max_h(maximum - 2. * padding)
                    .overflow_y_scroll();
                for option in hidden {
                    let owner = entity.clone();
                    let key = option.key.clone();
                    let mut tag = YueTag::new(
                        (id.clone(), format!("hidden:{}", option.key)),
                        option.label.clone(),
                    )
                    .theme(theme)
                    .size(p.size.tag());
                    if !option.disabled && interactive {
                        tag = tag.close_label(format!("移除 {}", option.label)).on_close(
                            move |_, window, cx| {
                                owner.update(cx, |state, cx| {
                                    state.remove(&key, SelectChangeSource::TagRemove, window, cx)
                                })
                            },
                        );
                    }
                    list = list.child(tag);
                }
                return panel
                    .role(Role::Group)
                    .aria_label("其余已选项")
                    .child(list)
                    .into_any_element();
            }
            let mut reserved = 2. * padding;
            if let Some(input) = input
                && interactive
            {
                // An existing YueInput, not a Select-specific text editor. The
                // context only overrides keys required by this composite.
                let search = YueInput::new((id.clone(), "query"), theme, &input)
                    .size(p.size.input())
                    .input_type(InputType::Search)
                    .accessibility_label("搜索选项")
                    .accessibility_description("方向键或回车进入结果，回车选择，Ctrl+F 返回搜索")
                    .placeholder("搜索选项")
                    .leading_icon(outline::Search)
                    .clearable(true)
                    .block();
                panel = panel.child(div().key_context("YueSelectQuery").child(search));
                reserved += metric(
                    theme,
                    &format!("--input-height-{}", p.size.suffix()),
                    window,
                ) + metric(theme, "--space-4", window);
            }
            if let Some(header) = &p.header {
                panel = panel.child(panel_slot(
                    header(interactive, window, cx),
                    0,
                    &entity,
                    &id,
                    maximum / 3.,
                ));
                reserved += slot_heights[0].max(row_h) + metric(theme, "--space-4", window);
            }
            if p.footer.is_some() {
                reserved += slot_heights[1].max(row_h) + metric(theme, "--space-4", window);
            }
            if has_more
                || !matches!(more, SelectResultState::Ready)
                || !matches!(create, SelectResultState::Ready)
                || at_limit
            {
                reserved += row_h;
            }
            let list_max = p
                .max_height
                .unwrap_or_else(|| metric(theme, "--select-list-max-height", window))
                .min((maximum - reserved).max(px(1.)));
            let total = geometry.total;
            let list_height = total.min(list_max).max(row_h.min(list_max));
            let list_id: ElementId = (id.clone(), "list").into();
            let mut list = div()
                .id(list_id.clone())
                .test_support()
                .key_context("YueSelectList")
                .h(list_height)
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .when(interactive, |this| {
                    this.role(Role::ListBox)
                        .aria_label(format!("{}选项", p.name))
                        .aria_description(match &result {
                            SelectResultState::Loading => p.loading_text.to_string(),
                            SelectResultState::Failed(message) => message.to_string(),
                            SelectResultState::Ready if has_more => {
                                format!("已加载 {} 个选项", geometry.option_count)
                            }
                            SelectResultState::Ready => {
                                format!("共 {} 个选项", geometry.option_count)
                            }
                        })
                        .track_focus(&focus)
                });
            let content = if matches!(result, SelectResultState::Loading) {
                div()
                    .p(metric(theme, "--space-8", window))
                    .child(p.loading_text.clone())
                    .into_any_element()
            } else if let SelectResultState::Failed(message) = &result {
                status_button(
                    (id.clone(), "retry"),
                    message.clone(),
                    &entity,
                    theme,
                    interactive,
                )
            } else if rows.is_empty() {
                div()
                    .p(metric(theme, "--space-8", window))
                    .child(p.empty_text.clone())
                    .into_any_element()
            } else if use_virtual {
                let config = p.clone();
                let owner = entity.clone();
                VirtualList::new(
                    (id.clone(), "virtual-list"),
                    &virtualizer,
                    move |item, window, cx| {
                        owner.update(cx, |state, cx| {
                            render_row(
                                &state.rows[item.index()].clone(),
                                state,
                                &config,
                                theme,
                                interactive,
                                window,
                                cx,
                            )
                        })
                    },
                )
                .into_any_element()
            } else {
                div()
                    .id((id.clone(), "option-list"))
                    .size_full()
                    .overflow_y_scroll()
                    .lock_scroll_axis()
                    .track_scroll(&scroll)
                    .children(inline_rows.unwrap_or_default())
                    .into_any_element()
            };
            let page_owner = entity.downgrade();
            list = list.child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = page_owner.update(cx, |state, _| {
                            state.page_size = (f32::from(bounds.size.height) / f32::from(row_h))
                                .floor()
                                .max(1.) as usize
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
            if interactive
                && has_more
                && matches!(more, SelectResultState::Ready)
                && -scroll.offset().y + list_height >= total - row_h
            {
                let owner = entity.downgrade();
                window.on_next_frame(move |_, cx| {
                    let _ = owner.update(cx, |state, cx| state.request_more(cx));
                });
            }
            let more_owner = entity.clone();
            let observed_scroll = scroll.clone();
            list = list.child(content).on_scroll_wheel(move |_, window, _| {
                let owner = more_owner.clone();
                let handle = observed_scroll.clone();
                window.on_next_frame(move |_, cx| {
                    if f32::from(-handle.offset().y) + f32::from(list_height)
                        >= f32::from(total - row_h)
                    {
                        owner.update(cx, |state, cx| state.request_more(cx));
                    }
                });
            });
            panel = panel.child(list.a11y_selection(SelectionA11y::default().multiple(multiple)));
            if at_limit {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(rgba(theme, "--text-secondary"))
                        .child(format!("最多选择 {} 项", limit.unwrap_or(0))),
                );
            }
            if matches!(create, SelectResultState::Loading) {
                panel = panel.child("正在创建选项");
            }
            if let SelectResultState::Failed(message) = &create {
                panel = panel.child(status_button(
                    (id.clone(), "create-retry"),
                    message.clone(),
                    &entity,
                    theme,
                    interactive,
                ));
            }
            if let SelectResultState::Failed(message) = &more {
                panel = panel.child(status_button(
                    (id.clone(), "more-retry"),
                    message.clone(),
                    &entity,
                    theme,
                    interactive,
                ));
            } else if matches!(more, SelectResultState::Loading) {
                panel = panel.child("正在加载更多");
            } else if has_more {
                let owner = entity.clone();
                panel = panel.child(
                    Button::new((id.clone(), "load-more"))
                        .disabled(!interactive)
                        .accessibility_label("加载更多选项")
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            owner.update(cx, |state, cx| state.request_more(cx))
                        })
                        .child("加载更多"),
                );
            }
            if let Some(footer) = &p.footer {
                panel = panel.child(panel_slot(
                    footer(interactive, window, cx),
                    1,
                    &entity,
                    &id,
                    maximum / 3.,
                ));
            }
            panel.into_any_element()
        },
        window,
        cx,
    )
}

fn status_button(
    id: impl Into<ElementId>,
    message: SharedString,
    owner: &Entity<YueSelectState>,
    theme: YueTheme,
    interactive: bool,
) -> AnyElement {
    let owner = owner.clone();
    div()
        .flex()
        .flex_col()
        .text_color(rgba(theme, "--error-foreground"))
        .child(message)
        .child(
            Button::new(id)
                .accessibility_label("重试加载")
                .disabled(!interactive)
                .cursor_pointer()
                .on_click(move |_, _, cx| owner.update(cx, |state, cx| state.retry_request(cx)))
                .child("重试"),
        )
        .into_any_element()
}

// Measure the actual slot after layout. The retained measurement updates the
// list budget on the next frame; custom content is capped to keep results usable.
fn panel_slot(
    content: AnyElement,
    slot: usize,
    owner: &Entity<YueSelectState>,
    id: &ElementId,
    maximum: Pixels,
) -> AnyElement {
    let owner = owner.downgrade();
    div()
        .id((id.clone(), format!("slot:{slot}")))
        .relative()
        .w_full()
        .flex_shrink_0()
        .max_h(maximum)
        .overflow_y_scroll()
        .child(content)
        .child(
            canvas(
                move |bounds, window, cx| {
                    let height = bounds.size.height;
                    let _ = owner.update(cx, |state, _| {
                        if state.slot_heights[slot] != height {
                            state.slot_heights[slot] = height;
                            let owner = owner.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = owner.update(cx, |_, cx| cx.notify());
                            });
                        }
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}
