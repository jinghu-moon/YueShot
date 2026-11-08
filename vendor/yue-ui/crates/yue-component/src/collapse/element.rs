use super::*;
use crate::{
    YueIcon,
    theme::{TokenKey, keys},
    tokens::{duration, easing, number, rems, rgba},
};
use gpui_kit::base::motion::MotionDisclosure;
use gpui_kit::base::{
    Accordion, AccordionController, AccordionDescriptor, AccordionHeader, AccordionSnapshot,
    AccordionTrigger, MotionStatus, TestSupportExt as _, Transition, transition_at,
};
use gpui_kit::{
    Bounds, InteractiveElement as _, MouseButton, ParentElement as _, Pixels, RenderOnce, Role,
    StatefulInteractiveElement as _, Styled as _, canvas, div, prelude::FluentBuilder as _, px,
};
use std::time::Duration;

#[derive(Default)]
struct Pointer {
    pressed: bool,
}

impl CollapseSize {
    fn keys(self) -> [TokenKey; 8] {
        match self {
            Self::Small => [
                keys::COLLAPSE_HEADER_MIN_HEIGHT_SM,
                keys::COLLAPSE_FONT_SIZE_SM,
                keys::COLLAPSE_LINE_HEIGHT_SM,
                keys::COLLAPSE_HEADER_PADDING_X_SM,
                keys::COLLAPSE_HEADER_PADDING_Y_SM,
                keys::COLLAPSE_INDICATOR_SIZE_SM,
                keys::COLLAPSE_INDICATOR_HIT_SIZE_SM,
                keys::COLLAPSE_CONTENT_PADDING_SM,
            ],
            Self::Medium => [
                keys::COLLAPSE_HEADER_MIN_HEIGHT_MD,
                keys::COLLAPSE_FONT_SIZE_MD,
                keys::COLLAPSE_LINE_HEIGHT_MD,
                keys::COLLAPSE_HEADER_PADDING_X_MD,
                keys::COLLAPSE_HEADER_PADDING_Y_MD,
                keys::COLLAPSE_INDICATOR_SIZE_MD,
                keys::COLLAPSE_INDICATOR_HIT_SIZE_MD,
                keys::COLLAPSE_CONTENT_PADDING_MD,
            ],
            Self::Large => [
                keys::COLLAPSE_HEADER_MIN_HEIGHT_LG,
                keys::COLLAPSE_FONT_SIZE_LG,
                keys::COLLAPSE_LINE_HEIGHT_LG,
                keys::COLLAPSE_HEADER_PADDING_X_LG,
                keys::COLLAPSE_HEADER_PADDING_Y_LG,
                keys::COLLAPSE_INDICATOR_SIZE_LG,
                keys::COLLAPSE_INDICATOR_HIT_SIZE_LG,
                keys::COLLAPSE_CONTENT_PADDING_LG,
            ],
        }
    }
}

fn schedule(bounds: Bounds<Pixels>, running: bool, window: &mut Window) {
    if running
        && window.is_visible()
        && window.is_window_active()
        && bounds.intersects(&window.content_mask().bounds)
    {
        window.request_animation_frame();
    }
}

impl RenderOnce for YueCollapse {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        assert!(
            self.trigger != CollapseExpandTrigger::Icon || self.show_indicator,
            "Icon-only collapse requires a visible indicator"
        );
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let [
            height_key,
            font_key,
            line_key,
            pad_x_key,
            pad_y_key,
            icon_key,
            hit_key,
            content_key,
        ] = self.size.keys();
        let p = |key| rems(theme, key).to_pixels(window.rem_size());
        let (height, font, line, pad_x, pad_y, hit, content_pad, gap, focus) = (
            p(height_key),
            p(font_key),
            p(line_key),
            p(pad_x_key),
            p(pad_y_key),
            p(hit_key),
            p(content_key),
            p(keys::COLLAPSE_HEADER_GAP),
            p(keys::COLLAPSE_FOCUS_WIDTH),
        );
        let signature = format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}",
            theme.mode(),
            theme.accent(),
            theme.neutral(),
            self.size,
            self.variant,
            self.trigger,
            self.position,
            self.show_indicator,
            window.rem_size(),
            window.scale_factor()
        );
        let previous =
            window.use_keyed_state((self.id.clone(), "geometry"), cx, |_, _| signature.clone());
        let geometry_changed = previous.read(cx) != &signature;
        if geometry_changed {
            previous.update(cx, |s, _| *s = signature.clone());
        }
        let controller = window.use_keyed_state((self.id.clone(), "controller"), cx, |_, cx| {
            AccordionController::new(cx)
        });
        controller.update(cx, |state, cx| {
            state.sync(
                AccordionSnapshot::new(
                    self.items
                        .iter()
                        .map(|i| AccordionDescriptor::new(i.key.clone()).disabled(i.disabled)),
                    self.expanded,
                )
                .mode(self.mode)
                .disabled(self.disabled),
                window,
                cx,
            );
            state.clear_handler();
            if let Some(handler) = self.on_change {
                state.on_change(move |r, w, cx| handler(r, w, cx));
            }
        });
        let animated = self.motion == CollapseMotion::Height
            && !cx.reduce_motion()
            && window.is_visible()
            && window.is_window_active();
        let enter = if animated {
            duration(theme, keys::COLLAPSE_EXPAND_DURATION)
        } else {
            Duration::ZERO
        };
        let exit = if animated {
            duration(theme, keys::COLLAPSE_COLLAPSE_DURATION)
        } else {
            Duration::ZERO
        };
        let policy = |duration| Transition::new(duration).ease(easing(theme, keys::COLLAPSE_EASE));
        let now = cx.background_executor().now();
        let mut items = Vec::new();
        let last = self.items.len().saturating_sub(1);
        for (ix, item) in self.items.into_iter().enumerate() {
            let id: ElementId = (self.id.clone(), item.key.clone()).into();
            let open = controller.read(cx).is_expanded(&item.key);
            let disabled = self.disabled || item.disabled;
            let leading = self.position == CollapseIndicatorPosition::Leading;
            let icon_only = self.trigger == CollapseExpandTrigger::Icon;
            let pointer =
                window.use_keyed_state((id.clone(), "pointer"), cx, |_, _| Pointer::default());
            if disabled || geometry_changed {
                pointer.update(cx, |p, _| {
                    p.pressed = false;
                });
            }
            let wash = if disabled {
                gpui_kit::Rgba::from(gpui_kit::transparent_black())
            } else if pointer.read(cx).pressed {
                rgba(theme, keys::COLLAPSE_HEADER_BACKGROUND_PRESSED)
            } else {
                gpui_kit::Rgba::from(gpui_kit::transparent_black())
            };
            let wash = transition_at(
                (id.clone(), "wash"),
                gpui_kit::Hsla::from(wash),
                policy(if geometry_changed || disabled {
                    Duration::ZERO
                } else {
                    duration(theme, keys::COLLAPSE_INTERACTION_DURATION)
                }),
                now,
                false,
                window,
                cx,
            );
            let rotation = transition_at(
                (id.clone(), "rotation"),
                if open { 0.25_f32 } else { 0. },
                policy(if geometry_changed {
                    Duration::ZERO
                } else if open {
                    enter
                } else {
                    exit
                }),
                now,
                false,
                window,
                cx,
            );
            let rotating = item.indicator.is_none()
                && animated
                && matches!(
                    rotation.status,
                    MotionStatus::Running | MotionStatus::Delayed
                );
            let mut trigger = AccordionTrigger::new((id.clone(), "trigger"))
                .in_group(&controller, item.key.clone())
                .accessibility_label(item.title.clone())
                .min_w_0()
                .min_h(if icon_only { hit } else { height })
                .line_height(line)
                .justify_start()
                .border(focus)
                .border_color(gpui_kit::transparent_black())
                .rounded(rems(theme, keys::COLLAPSE_RADIUS))
                .cursor_default()
                .bg(wash.value)
                .when(disabled, |t| {
                    t.opacity(number(theme, keys::COLLAPSE_OPACITY_DISABLED))
                })
                .when(!disabled, |t| {
                    t.focus_visible(|s| s.border_color(rgba(theme, keys::COLLAPSE_FOCUS_COLOR)))
                });
            if !disabled {
                let hover = pointer.clone();
                let press = pointer.clone();
                let release = pointer.clone();
                let outside = pointer.clone();
                trigger = trigger
                    .on_hover(move |value, _, cx| {
                        hover.update(cx, |p, cx| {
                            if !*value && p.pressed {
                                p.pressed = false;
                                cx.notify();
                            }
                        });
                    })
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        press.update(cx, |p, cx| {
                            p.pressed = true;
                            cx.notify();
                        });
                    })
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        release.update(cx, |p, cx| {
                            p.pressed = false;
                            cx.notify();
                        });
                    })
                    .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                        outside.update(cx, |p, cx| {
                            p.pressed = false;
                            cx.notify();
                        });
                    });
            }
            let indicator = || {
                let custom = item.indicator.is_some();
                div()
                    .id((id.clone(), "indicator"))
                    .test_support()
                    .relative()
                    .w(hit - if icon_only { focus * 2. } else { px(0.) })
                    .h(line)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        item.indicator
                            .unwrap_or_else(|| Icon::new(YueIcon::ChevronRight))
                            .size(rems(theme, icon_key))
                            .color_value(rgba(theme, keys::COLLAPSE_INDICATOR_COLOR))
                            .rotation(if custom { 0. } else { rotation.value }),
                    )
                    .child(
                        canvas(move |b, w, _| schedule(b, rotating, w), |_, _, _, _| {})
                            .absolute()
                            .inset_0(),
                    )
            };
            let title = || {
                div()
                    .id((id.clone(), "title"))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .when(icon_only && disabled, |d| {
                        d.opacity(number(theme, keys::COLLAPSE_OPACITY_DISABLED))
                    })
                    .child(item.title.clone())
            };
            let heading = if icon_only {
                trigger = trigger.w(hit).h(hit).justify_center().child(indicator());
                AccordionHeader::new(trigger)
                    .id((id.clone(), "heading"))
                    .level(self.heading_level)
                    .flex_shrink_0()
                    .into_any_element()
            } else {
                trigger = trigger
                    .w_full()
                    .py((pad_y - focus).max(px(0.)))
                    .gap(gap)
                    .when(self.show_indicator && leading, |t| t.child(indicator()))
                    .child(title())
                    .when(self.show_indicator && !leading, |t| t.child(indicator()));
                AccordionHeader::new(trigger)
                    .id((id.clone(), "heading"))
                    .level(self.heading_level)
                    .flex_1()
                    .min_w_0()
                    .into_any_element()
            };
            let mut header = div()
                .id((id.clone(), "header"))
                .test_support()
                .relative()
                .flex()
                .items_center()
                .min_w_0()
                .min_h(height)
                .px(pad_x)
                .bg(rgba(theme, keys::COLLAPSE_HEADER_BACKGROUND))
                .when(ix == 0, |d| d.rounded_t(rems(theme, keys::COLLAPSE_RADIUS)))
                .when(ix == last && !open, |d| {
                    d.rounded_b(rems(theme, keys::COLLAPSE_RADIUS))
                })
                .when(icon_only, |d| {
                    d.pl(pad_x + focus).py(((height - hit) / 2.).max(px(0.)))
                });
            if icon_only {
                header = if leading {
                    header
                        .child(heading)
                        .child(div().w(gap).flex_shrink_0())
                        .child(title())
                } else {
                    header
                        .child(title())
                        .child(div().w(gap).flex_shrink_0())
                        .child(heading)
                };
            } else {
                header = header.child(heading);
            }
            if let Some(actions) = item.actions {
                header = header.child(
                    div()
                        .id((id.clone(), "actions"))
                        .test_support()
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .ml(rems(theme, keys::COLLAPSE_ACTIONS_GAP))
                        .child(actions(window, cx)),
                );
            }
            let wash_running = matches!(wash.status, MotionStatus::Running | MotionStatus::Delayed);
            header = header.child(
                canvas(move |b, w, _| schedule(b, wash_running, w), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            );
            let body = if open {
                let panel = controller
                    .read(cx)
                    .panel_focus(&item.key)
                    .expect("Collapse panel focus");
                Some(
                    div()
                        .id((id.clone(), "panel"))
                        .test_support()
                        .track_focus(&panel)
                        .w_full()
                        .when(item.region, |d| {
                            d.role(Role::Region).aria_label(item.title.clone())
                        })
                        .when(item.padding, |d| {
                            d.p(content_pad).pl(content_pad
                                + if leading && self.show_indicator {
                                    hit + gap + focus
                                } else {
                                    focus
                                })
                        })
                        .when_some(item.content, |d, render| d.child(render(window, cx)))
                        .into_any_element(),
                )
            } else {
                None
            };
            let disclosure = MotionDisclosure::new((id.clone(), "reveal"), open, body)
                .transitions(policy(enter), policy(exit))
                .geometry_key(signature.clone());
            items.push(
                div()
                    .id(id.clone())
                    .test_support()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .bg(rgba(theme, keys::COLLAPSE_CONTENT_BACKGROUND))
                    .when(ix == 0, |d| d.rounded_t(rems(theme, keys::COLLAPSE_RADIUS)))
                    .when(ix == last, |d| {
                        d.rounded_b(rems(theme, keys::COLLAPSE_RADIUS))
                    })
                    .when(ix > 0, |d| {
                        d.border_t(rems(theme, keys::COLLAPSE_BORDER_WIDTH))
                            .border_color(rgba(theme, keys::COLLAPSE_BORDER_COLOR))
                    })
                    .child(header)
                    .child(
                        div()
                            .id((id, "body"))
                            .test_support()
                            .w_full()
                            .child(disclosure),
                    ),
            );
        }
        let face = crate::typography::ui_font(theme);
        let mut root = Accordion::new(self.id)
            .track_focus(&controller.read(cx).boundary_focus())
            .flex()
            .flex_col()
            .min_w_0()
            .w_full()
            .font_family(face.family.clone())
            .text_size(font)
            .line_height(line)
            .text_color(rgba(theme, keys::COLLAPSE_COLOR))
            .rounded(rems(theme, keys::COLLAPSE_RADIUS))
            .when(self.variant == CollapseVariant::Bordered, |d| {
                d.border(rems(theme, keys::COLLAPSE_BORDER_WIDTH))
                    .border_color(rgba(theme, keys::COLLAPSE_BORDER_COLOR))
            })
            .when_some(self.label, |d, label| d.aria_label(label))
            .children(items);
        root.text_style().font_fallbacks = face.font_fallbacks();
        self.layout.apply(root)
    }
}
