//! Select row semantics and content composition.

use super::{model::*, *};

use crate::tokens::{duration, easing, rems, rgba};

use crate::{Icon, YueOverflowTooltip, outline};

use gpui_kit::base::{A11yDisabledExt as _, TestSupportExt as _, Transition, transition};

use gpui_kit::{
    AccessibleAction, AnyElement, Context, InteractiveElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _,
};

use std::time::Duration;

use super::layout::*;

pub(super) fn render_row(
    row: &Row,
    state: &YueSelectState,
    p: &Presentation,
    theme: YueTheme,
    interactive: bool,
    window: &mut Window,
    cx: &mut Context<YueSelectState>,
) -> AnyElement {
    let id: ElementId = (
        p.id.clone(),
        match row {
            Row::Group(key, _) => format!("group:{key}"),
            Row::Option(option) => format!("option:{}", option.key),
            Row::Create(query) => format!("create:{query}"),
        },
    )
        .into();
    let height = row_height(row, p, theme, window);
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
    if let Row::Group(_, label) = row {
        return div()
            .id(id)
            .h(height)
            .px(padding)
            .flex()
            .items_center()
            .text_sm()
            .text_color(rgba(theme, "--text-secondary"))
            .child(label.clone())
            .into_any_element();
    }
    let key = row.active_key().expect("selectable row identity");
    let enabled = interactive && state.eligible(row) && state.editable();
    let active = state.active.as_ref() == Some(&key);
    let selected = matches!(row,Row::Option(option) if state.value.contains(&option.key));
    let target = enabled
        && if state.keyboard_navigation {
            active
        } else {
            state.hovered.as_ref() == Some(&key)
        };
    let amount = transition(
        (id.clone(), "hover"),
        if target { 1_f32 } else { 0_f32 },
        Transition::new(if interactive {
            duration(theme, "--menu-item-duration")
        } else {
            Duration::ZERO
        })
        .ease(easing(theme, "--menu-item-ease")),
        window,
        cx,
    );
    let mut hover = rgba(theme, "--menu-item-background-hover");
    hover.a *= amount;
    let selected_bg = if selected {
        rgba(theme, "--select-option-background-selected")
    } else {
        gpui_kit::rgba(0)
    };
    let fg = rgba(
        theme,
        if !enabled {
            "--input-color-disabled"
        } else if selected {
            "--select-option-color-selected"
        } else {
            "--menu-item-color"
        },
    );
    let name = match row {
        Row::Option(option) => option.label.clone(),
        Row::Create(query) => format!("创建「{query}」").into(),
        _ => unreachable!(),
    };
    let owner = cx.entity();
    let activation = key.clone();
    let a11y_activation = key.clone();
    let a11y_owner = owner.clone();
    let hovered = key.clone();
    let moving = key.clone();
    let mouse_owner = owner.clone();
    let hover_owner = owner.clone();
    let mut shell = div()
        .id(id.clone())
        .test_support()
        .h(height)
        .w_full()
        .px(padding)
        .flex()
        .items_center()
        .gap(metric(theme, "--input-gap", window))
        .text_size(font)
        .text_color(fg)
        .relative()
        .rounded(rems(theme, "--menu-item-border-radius"))
        .bg(selected_bg)
        .when(interactive, |this| {
            this.role(Role::ListBoxOption)
                .aria_label(name.clone())
                .aria_selected(selected)
        })
        .when(
            interactive && active && state.list_focus.is_focused(window),
            |this| this.aria_active_descendant(),
        )
        .when(enabled, |this| {
            this.cursor_pointer()
                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                    a11y_owner.update(cx, |state, cx| {
                        state.activate(
                            a11y_activation.clone(),
                            SelectChangeSource::Keyboard,
                            window,
                            cx,
                        )
                    });
                })
                .on_click(move |_, window, cx| {
                    owner.update(cx, |state, cx| {
                        state.activate(activation.clone(), SelectChangeSource::Pointer, window, cx)
                    })
                })
                .on_mouse_move(move |_, _, cx| {
                    mouse_owner.update(cx, |state, cx| {
                        if state.active.as_ref() != Some(&moving) || state.keyboard_navigation {
                            state.active = Some(moving.clone());
                            state.keyboard_navigation = false;
                            cx.notify();
                        }
                    })
                })
        })
        .on_hover(move |hover, _, cx| {
            hover_owner.update(cx, |state, cx| {
                if *hover {
                    if state.hovered.as_ref() != Some(&hovered) {
                        state.hovered = Some(hovered.clone());
                        cx.notify();
                    }
                } else if state.hovered.as_ref() == Some(&hovered) {
                    state.hovered = None;
                    cx.notify();
                }
            })
        });
    shell = shell.child(
        div()
            .absolute()
            .inset_0()
            .rounded(rems(theme, "--menu-item-border-radius"))
            .bg(hover),
    );
    if let Row::Option(option) = row {
        if let Some(renderer) = &p.option_renderer {
            // The slot owns vertical alignment. A custom Tag can keep its
            // horizontal self_start without overriding the row's centering.
            shell = shell.child(
                div()
                    .min_w_0()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .child(renderer(
                        &SelectOptionContext {
                            option: option.clone(),
                            selected,
                            active,
                            disabled: !enabled,
                            query: state.query(cx),
                            size: p.size,
                        },
                        window,
                        cx,
                    )),
            );
        } else {
            // Reserve one shared icon lane only when this result set has icons.
            // A plain list starts at its normal text inset, including narrow selects.
            if state
                .list_cache
                .as_ref()
                .is_some_and(|geometry| geometry.has_icons)
            {
                shell = shell.child(div().size(icon).flex_shrink_0().when_some(
                    option.icon,
                    |this, image| {
                        this.child(
                            image
                                .size(gpui_kit::rems(
                                    f32::from(icon) / f32::from(window.rem_size()),
                                ))
                                .color_value(fg),
                        )
                    },
                ));
            }
            let mut text = div()
                .id((id.clone(), "text"))
                .test_support()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    YueOverflowTooltip::new((id.clone(), "label"), theme, option.label.clone())
                        .width(gpui_kit::relative(1.)),
                );
            if p.row_layout == SelectRowLayout::Description
                && let Some(description) = &option.description
            {
                text = text.child(
                    div()
                        .truncate()
                        .text_sm()
                        .text_color(rgba(theme, "--text-secondary"))
                        .child(description.clone()),
                );
            }
            shell = shell.child(text);
            if let Some(meta) = &option.meta {
                shell = shell.child(div().text_sm().flex_shrink_0().child(meta.clone()));
            }
        }
    } else {
        shell = shell.child(div().flex_1().min_w_0().truncate().child(name));
    }
    shell
        .child(div().size(icon).flex_shrink_0().when(selected, |this| {
            this.child(
                Icon::new(outline::Check)
                    .size(gpui_kit::rems(
                        f32::from(icon) / f32::from(window.rem_size()),
                    ))
                    .color_value(fg),
            )
        }))
        .a11y_disabled(!enabled)
        .into_any_element()
}
