//! color_picker element composition.

use super::*;

use crate::popup::{
    PopupAnchor, PopupDismissRoute, PopupHost, PopupId, PopupMotion, PopupPlacement,
    PopupPointerMode, PopupSpec,
};

use crate::tokens::{duration, easing, pixels, rems, rgba};

use gpui_kit::base::{PopoverState, TestSupportExt as _, Transition, transition};

use gpui_kit::{
    App, Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

impl RenderOnce for YueColorPickerPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let options = self.options.resolved(cx);
        self.state
            .update(cx, |s, cx| s.prepare(!options.disabled, window, cx));
        let body = panel::body(
            self.id.clone(),
            &self.state,
            &options,
            None,
            true,
            window,
            cx,
        );
        let surface = div()
            .id(self.id)
            .w(rems(options.theme, "--color-picker-panel-width"))
            .child(body);
        self.layout.apply(surface)
    }
}

impl RenderOnce for YueColorPicker {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let options = self.options.resolved(cx);
        let theme = options.theme;
        let id = self.id;
        self.state
            .update(cx, |s, cx| s.prepare(!options.disabled, window, cx));
        let popup = window.use_keyed_state((id.clone(), "color-popup"), cx, |_, cx| {
            PopoverState::new(false, cx)
        });
        let meta = window.use_keyed_state((id.clone(), "color-geometry"), cx, |_, cx| {
            (
                cx.focus_handle(),
                PopupAnchor::default(),
                Rc::new(Cell::new(None)),
            )
        });
        let (default_trigger, anchor, format_bounds) = meta.read(cx).clone();
        let trigger = self.focus.unwrap_or(default_trigger);
        let host = Popup {
            state: popup.clone(),
            trigger: trigger.clone(),
            anchor: anchor.clone(),
            format_bounds,
        };
        if options.disabled && popup.read(cx).is_open() {
            host.dismiss(&self.state, window, cx, false);
        }
        // The subscription is retained, its callback is refreshed each render.
        // A controlled SwatchGroup may change selection or handler meanwhile.
        let editor = self.state.clone();
        let completion = window.use_keyed_state(
            (
                id.clone(),
                format!("color-completion-{}", editor.entity_id().as_u64()),
            ),
            cx,
            move |window, cx| {
                let latest: Rc<RefCell<Option<ConfirmHandler>>> = Rc::default();
                let callback = latest.clone();
                let subscription = cx.subscribe_in(
                    &editor,
                    window,
                    move |_, _, _: &state::ColorConfirmed, window, cx| {
                        let handler = callback.borrow().clone();
                        if let Some(handler) = handler {
                            handler(window, cx);
                        }
                    },
                );
                (latest, subscription)
            },
        );
        *completion.read(cx).0.borrow_mut() = self.on_confirm.clone();
        let open = popup.read(cx).is_open();
        let color = self.state.read(cx).color;
        let formatted = color.text(self.state.read(cx).format, self.state.read(cx).hsv[0]);
        let (height, font, padding, swatch) = match self.size {
            ColorPickerSize::Small => (
                "--color-picker-height-sm",
                "--color-picker-font-size-sm",
                "--color-picker-padding-inline-sm",
                "--color-picker-swatch-size-sm",
            ),
            ColorPickerSize::Medium => (
                "--color-picker-height-md",
                "--color-picker-font-size-md",
                "--color-picker-padding-inline-md",
                "--color-picker-swatch-size-md",
            ),
            ColorPickerSize::Large => (
                "--color-picker-height-lg",
                "--color-picker-font-size-lg",
                "--color-picker-padding-inline-lg",
                "--color-picker-swatch-size-lg",
            ),
        };
        let toggle = host.clone();
        let state = self.state.clone();
        let disabled = options.disabled;
        let hovered = window.use_keyed_state((id.clone(), "color-hover"), cx, |_, _| false);
        let border_target = rgba(
            theme,
            if disabled {
                "--color-picker-disabled-border-color"
            } else if open {
                "--color-picker-border-color-open"
            } else if *hovered.read(cx) {
                "--color-picker-border-color-hover"
            } else {
                "--color-picker-border-color"
            },
        );
        let border = transition(
            (id.clone(), "color-border"),
            gpui_kit::Hsla::from(border_target),
            Transition::new(duration(theme, "--color-picker-state-duration"))
                .ease(easing(theme, "--color-picker-state-ease")),
            window,
            cx,
        );
        let preview = if self.group_selected.is_some() {
            div()
                .relative()
                .size(rems(theme, "--swatch-size"))
                .child(paint::preview(color.rgba(), theme, "--swatch-size"))
                .child(
                    div()
                        .absolute()
                        .inset(gpui_kit::rems(
                            (pixels(theme, "--swatch-size")
                                - pixels(theme, "--color-picker-swatch-size-sm"))
                                / 32.,
                        ))
                        .rounded(rems(theme, "--swatch-radius"))
                        .bg(rgba(theme, "--color-picker-background"))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            crate::Icon::new(crate::outline::Palette)
                                .size(rems(theme, "--color-picker-swatch-size-sm"))
                                .color(theme.text_primary()),
                        ),
                )
                .into_any_element()
        } else {
            paint::preview(color.rgba(), theme, swatch)
        };
        let description = match self.group_selected {
            Some(true) => format!("已选中，{formatted}"),
            Some(false) => format!("未选中，{formatted}"),
            None => formatted.clone(),
        };
        let button = gpui_kit::base::Button::new((id.clone(), "trigger"))
            .accessibility_label(options.name.clone())
            .aria_description(description)
            .aria_expanded(open)
            .disabled(disabled)
            .track_focus(&trigger)
            .tab_stop(self.tab_stop)
            .flex()
            .items_center()
            .justify_center()
            .gap(rems(theme, "--color-picker-trigger-gap"))
            .h(rems(theme, height))
            .text_size(rems(theme, font))
            .px(if self.trigger == ColorPickerTrigger::Swatch {
                gpui_kit::rems(0.)
            } else {
                rems(theme, padding)
            })
            .rounded(rems(theme, "--color-picker-radius"))
            .border_1()
            .border_color(border)
            .bg(rgba(
                theme,
                if disabled {
                    "--color-picker-disabled-background"
                } else {
                    "--color-picker-background"
                },
            ))
            .text_color(rgba(
                theme,
                if disabled {
                    "--color-picker-disabled-color"
                } else {
                    "--color-picker-color"
                },
            ))
            .when(!disabled, |b| b.cursor_pointer())
            .when(!disabled, |b| {
                b.on_hover(move |hover, window, cx| {
                    hovered.update(cx, |v, _| *v = *hover);
                    window.refresh();
                })
                .focus_visible(|b| b.border_color(rgba(theme, "--color-picker-focus-ring-color")))
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if disabled {
                    return;
                }
                if toggle.state.read(cx).is_open() {
                    toggle.dismiss(&state, window, cx, true);
                } else {
                    toggle.trigger.focus(window, cx);
                    toggle.state.update(cx, |s, cx| s.show(window, cx));
                    let focus = state.read(cx).focus.clone();
                    focus.focus(window, cx);
                    window.refresh();
                }
            })
            .relative()
            .child(preview)
            .when(self.group_selected == Some(true), |b| {
                b.child(
                    div()
                        .absolute()
                        .inset(gpui_kit::rems(
                            ((pixels(theme, "--swatch-hit-size") - pixels(theme, "--swatch-size"))
                                / 2.
                                - pixels(theme, "--swatch-selected-offset")
                                - pixels(theme, "--swatch-selected-width"))
                                / 16.,
                        ))
                        .rounded(rems(theme, "--swatch-radius"))
                        .border(rems(theme, "--swatch-selected-width"))
                        .border_color(rgba(theme, "--swatch-selected-color")),
                )
            })
            .when(self.trigger == ColorPickerTrigger::Field, |b| {
                b.child(div().flex_1().min_w_0().truncate().child(formatted))
                    .child(
                        crate::Icon::new(crate::outline::ChevronDown)
                            .size(rems(theme, swatch))
                            .color(theme.text_secondary()),
                    )
            })
            .w_full();
        let trigger_row = button.relative().child(anchor.capture());
        let enter = Transition::new(duration(theme, "--color-picker-enter-duration"))
            .ease(easing(theme, "--color-picker-enter-ease"));
        let exit = Transition::new(duration(theme, "--color-picker-exit-duration"))
            .ease(easing(theme, "--color-picker-exit-ease"));
        let mut root = div()
            .id((id.clone(), "color-field"))
            .flex()
            .flex_col()
            .items_start()
            .w(if self.trigger == ColorPickerTrigger::Swatch {
                rems(theme, height)
            } else {
                rems(theme, "--color-picker-trigger-width")
            })
            .gap_1();
        root = self.layout.apply(root);
        if let Some(label) = options.label.clone() {
            root = root.child(div().text_sm().child(label));
        }
        root = root.child(trigger_row);
        let scale = f32::from(window.rem_size()) / 16.;
        let margin = px(pixels(theme, "--color-picker-window-margin") * scale);
        let shadows = crate::tokens::shadow(theme, "--color-picker-panel-shadow");
        let mut outsets: gpui_kit::Edges<Pixels> = Default::default();
        for shadow in &shadows {
            let radius = (shadow.blur_radius + shadow.spread_radius).max(px(0.));
            outsets.left = outsets.left.max(radius - shadow.offset.x);
            outsets.right = outsets.right.max(radius + shadow.offset.x);
            outsets.top = outsets.top.max(radius - shadow.offset.y);
            outsets.bottom = outsets.bottom.max(radius + shadow.offset.y);
        }
        let reserve = [outsets.left, outsets.right, outsets.top, outsets.bottom]
            .into_iter()
            .fold(px(0.), Pixels::max);
        let inset = 2. * (margin + reserve);
        let width = px(pixels(theme, "--color-picker-panel-width") * scale)
            .min(window.viewport_size().width - inset)
            .max(px(1.));
        let maximum_height = (window.viewport_size().height - inset).max(px(1.));
        let close_host = host.clone();
        let editor = self.state.clone();
        let spec = PopupSpec::new(
            PopupId::new(window.current_view(), (id.clone(), "color-popup-surface")),
            anchor.handle(),
            open,
            PopupPlacement::Side(self.placement, self.align),
        )
        .surface_gap(px(pixels(theme, "--color-picker-panel-offset") * scale))
        .margin(margin)
        .pointer_mode(PopupPointerMode::Block)
        .dismiss_route(PopupDismissRoute::Owner)
        .motion(PopupMotion::new(enter, exit))
        .visual_outsets(outsets)
        .on_dismiss(move |_, window, cx| close_host.dismiss(&editor, window, cx, false));
        if let Some(surface) = PopupHost::new(spec).mount(
            move |presentation, _, window, cx| {
                let interactive = presentation.is_interactive();
                let content = panel::body(
                    id.clone(),
                    &self.state,
                    &options,
                    Some(host),
                    interactive,
                    window,
                    cx,
                );
                div()
                    .id((id, "popup-surface"))
                    .test_support()
                    .when(interactive, |s| {
                        s.role(gpui_kit::Role::Dialog)
                            .aria_label(format!("{}颜色编辑器", options.name))
                            .track_focus(&popup.read(cx).focus_handle(cx))
                            .overflow_y_scroll()
                    })
                    .w(width)
                    .max_h(maximum_height)
                    .bg(rgba(theme, "--color-picker-panel-background"))
                    .border_1()
                    .border_color(rgba(theme, "--color-picker-panel-border-color"))
                    .rounded(rems(theme, "--color-picker-panel-radius"))
                    .shadow(shadows)
                    .child(content)
                    .into_any_element()
            },
            window,
            cx,
        ) {
            root = root.child(surface);
        }
        root
    }
}
