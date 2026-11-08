//! Dialog surface composition and lifecycle wiring.

use super::*;

use crate::{
    ButtonVariant, Icon, TextRole, YueButton, YueIcon,
    theme::keys,
    tokens::{duration, easing, pixels, rems, rgba, shadow},
};

use gpui_kit::base::{
    Dialog, DialogIntent, ModalBoundary, Presence, PresencePhase, TestSupportExt as _, Transition,
};

use gpui_kit::{
    App, InteractiveElement as _, IntoElement, ParentElement, RenderOnce, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};

use std::rc::Rc;

impl RenderOnce for YueDialog {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = if self.inherited {
            crate::current_theme(cx)
        } else {
            self.theme
        };
        let weak = self.state.downgrade();
        let lease = w
            .use_keyed_state((self.id.clone(), "dialog-mount"), cx, move |w, cx| {
                let handle = w.window_handle();
                cx.on_release(move |_, cx| {
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, w, cx| {
                            let _ = weak.update(cx, |s, cx| {
                                if s.mount.strong_count() == 0 {
                                    s.unmount(w, cx);
                                }
                            });
                        });
                    });
                })
                .detach();
                Rc::new(())
            })
            .read(cx)
            .clone();
        let owner = w.current_view();
        let enter = self.enter.unwrap_or(
            !self.alert && self.footer_content.is_none() && self.footer != DialogFooter::None,
        );
        let mut behavior = self.behavior;
        if self.alert
            && behavior.initial_focus.is_none()
            && self.footer_content.is_none()
            && self.footer == DialogFooter::Standard
        {
            behavior.initial_focus = Some(self.state.read(cx).cancel_focus.clone());
        }
        self.state.update(cx, |s, cx| {
            s.mounted(self.id.clone(), &lease, owner, behavior, w, cx)
        });
        let state = self.state.read(cx);
        let open = state.open;
        let visible = state.visible;
        let session = state.session;
        let pending = state.is_pending();
        let focus = state.focus.clone();
        let cancel_focus = state.cancel_focus.clone();
        let disabled = state.behavior.confirm_disabled;
        let scope = state.scope.clone();
        let drag = state.drag.clone();
        let settled = if open { state.opened } else { !visible };
        // Sample even when closed: its retained zero seeds the next entrance.
        let presence = Presence::new((self.id.clone(), "dialog-presence"), open)
            .transition(
                Transition::new(duration(
                    theme,
                    if open {
                        keys::DIALOG_DURATION_ENTER
                    } else {
                        keys::DIALOG_DURATION_EXIT
                    },
                ))
                .ease(easing(theme, keys::DIALOG_EASE)),
            )
            .sample(w, cx);
        if !settled
            && matches!(
                presence.phase,
                PresencePhase::Present | PresencePhase::Absent
            )
        {
            let state = self.state.downgrade();
            let handle = w.window_handle();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, w, cx| {
                    let _ = state.update(cx, |s, cx| s.settled(session, open, w, cx));
                });
            });
        }
        let Some(scope) = scope.filter(|_| visible && presence.should_render()) else {
            return div().id(self.id).into_any_element();
        };
        self.state
            .update(cx, |s, cx| s.schedule_initial_focus(w, cx));
        let top =
            gpui_kit::base::active_modal_scope(w, cx).is_some_and(|a| a.owner() == scope.owner());
        let nested = layer::DialogLayer::for_window(w, cx).is_some_and(|l| l.read(cx).has_parent());
        let label = self.label.unwrap_or_else(|| self.title.clone());
        assert!(
            !label.trim().is_empty(),
            "YueDialog requires a nonempty title or semantic label"
        );
        let margin = rems(theme, keys::SPACE_16).to_pixels(w.rem_size());
        let viewport = w.viewport_size();
        let max_width = (viewport.width - margin * 2.).max(px(0.));
        let max_height = (viewport.height - margin * 2.).max(px(0.));
        let minimum_chrome = rems(theme, keys::DIALOG_PADDING).to_pixels(w.rem_size()) * 2.
            + rems(theme, keys::DIALOG_HEADER_HEIGHT).to_pixels(w.rem_size())
            + rems(theme, keys::BUTTON_HEIGHT_MD).to_pixels(w.rem_size())
            + rems(theme, keys::DIALOG_SECTION_PADDING).to_pixels(w.rem_size()) * 2.;
        let shell_scroll = max_height < minimum_chrome;
        let spacing = rems(theme, keys::DIALOG_SPACING);
        let pad = rems(theme, keys::DIALOG_PADDING);
        let weak_close = self.state.downgrade();
        let close = YueButton::icon("dialog-close", theme, YueIcon::Close, self.close_label)
            .variant(ButtonVariant::Ghost)
            .disabled(pending || !open)
            .on_click(move |_, w, cx| {
                let _ = weak_close.update(cx, |s, cx| {
                    s.request_close(DialogCloseReason::CloseButton, w, cx);
                });
            });
        let show_header = self.header_visible || self.closable;
        let mut header = div()
            .id("dialog-header")
            .test_support()
            .flex()
            .items_start()
            .gap(spacing)
            .min_h(rems(theme, keys::DIALOG_HEADER_HEIGHT))
            .flex_shrink_0();
        if self.header_visible {
            let default_header = self.header.is_none();
            if let Some(handle) = self.drag_handle {
                header = header.child(if self.draggable && open && !pending && top {
                    drag.handle("dialog-explicit-drag-handle", handle)
                } else {
                    handle
                });
            }
            let heading = self.header.unwrap_or_else(|| {
                let mut line = div().flex().items_center().gap(spacing);
                if let Some(status) = self.status {
                    let icon = match status {
                        DialogStatus::Info => YueIcon::Info,
                        DialogStatus::Success => YueIcon::Success,
                        DialogStatus::Warning => YueIcon::Warning,
                        DialogStatus::Danger => YueIcon::Error,
                    };
                    let color = match status {
                        DialogStatus::Info => keys::INFO,
                        DialogStatus::Success => keys::SUCCESS,
                        DialogStatus::Warning => keys::WARNING,
                        DialogStatus::Danger => keys::ERROR,
                    };
                    line = line.child(
                        Icon::new(icon)
                            .size(rems(theme, keys::FONT_SIZE_LG))
                            .color_value(rgba(theme, color)),
                    );
                }
                let title = TextRole::Title
                    .apply(theme, div().child(self.title))
                    .line_height(gpui_kit::rems(
                        TextRole::Title.style(theme).line_height / 16.,
                    ));
                line.child(title).into_any_element()
            });
            header = header.child(
                if self.draggable && open && !pending && top && default_header {
                    drag.handle("dialog-drag-handle", heading)
                } else {
                    div().min_w_0().flex_1().child(heading).into_any_element()
                },
            );
        } else {
            header = header.child(div().flex_1());
        }
        if self.closable {
            header = header.child(close);
        }
        // Keep outlines inside the scroll mask without moving content or
        // increasing the header/body/footer gaps.
        let focus_gutter = rems(theme, keys::FOCUS_RING_WIDTH).to_pixels(w.rem_size())
            + rems(theme, keys::FOCUS_RING_OFFSET).to_pixels(w.rem_size());
        let mut body = div()
            .id("dialog-body")
            .test_support()
            .min_h_0()
            .min_w_0()
            .flex_shrink(1.)
            .when(!shell_scroll, |body| {
                body.overflow_y_scroll().p(focus_gutter).m(-focus_gutter)
            })
            .flex()
            .flex_col()
            .gap(spacing);
        if let Some(description) = self.description.clone() {
            body = body.child(
                div()
                    .text_color(rgba(theme, keys::TEXT_SECONDARY))
                    .child(description),
            );
        }
        body = body.children(self.children);
        if shell_scroll {
            body = body.flex_shrink_0();
        }
        let mut footer = div()
            .id("dialog-footer")
            .test_support()
            .flex()
            .items_center()
            .justify_end()
            .flex_wrap()
            .gap(rems(theme, keys::DIALOG_FOOTER_GAP))
            .flex_shrink_0();
        let has_footer = self.footer_content.is_some() || self.footer != DialogFooter::None;
        if let Some(content) = self.footer_content {
            footer = footer.child(content);
        } else if self.footer != DialogFooter::None {
            if self.footer == DialogFooter::Standard {
                let state = self.state.downgrade();
                footer = footer.child(
                    YueButton::text("dialog-cancel", theme, self.cancel_label)
                        .track_focus(&cancel_focus)
                        .disabled(pending || !open)
                        .on_click(move |_, w, cx| {
                            let _ = state.update(cx, |s, cx| {
                                s.cancel(w, cx);
                            });
                        }),
                );
            }
            let state = self.state.downgrade();
            footer = footer.child(
                YueButton::text("dialog-confirm", theme, self.confirm_label)
                    .variant(if self.status == Some(DialogStatus::Danger) {
                        ButtonVariant::Danger
                    } else {
                        ButtonVariant::Primary
                    })
                    .loading(pending)
                    .disabled(pending || disabled || !open)
                    .on_click(move |_, w, cx| {
                        let _ = state.update(cx, |s, cx| {
                            s.confirm(w, cx);
                        });
                    }),
            );
        }
        let distance =
            rems(theme, keys::SPACE_8).to_pixels(w.rem_size()) * (1. - presence.progress);
        if !self.draggable || !self.header_visible || pending || !open {
            drag.cancel();
        }
        let offset = drag.offset();
        let measured = drag.clone();
        let border = px(pixels(theme, keys::DIALOG_BORDER_WIDTH) / 16. * f32::from(w.rem_size()));
        let capture = gpui_kit::canvas(
            move |bounds: gpui_kit::Bounds<gpui_kit::Pixels>, w, _| {
                let resting = gpui_kit::Bounds::new(
                    bounds.origin - offset - gpui_kit::point(border, border + distance),
                    gpui_kit::size(
                        bounds.size.width + border * 2.,
                        bounds.size.height + border * 2.,
                    ),
                );
                if measured.measure(
                    resting,
                    gpui_kit::Bounds::new(Default::default(), w.viewport_size()),
                    margin,
                ) {
                    w.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        // The surface clips its own outline. The inner shell owns scrolling
        // only in tiny viewports; geometry observation stays outside all scroll
        // content, so scrolling cannot feed back into drag constraints.
        let contents = div()
            .id("dialog-shell")
            .min_w_0()
            .min_h_0()
            .flex_shrink(1.)
            .flex()
            .flex_col()
            .gap(rems(theme, keys::DIALOG_SECTION_PADDING))
            .p(pad)
            .when(shell_scroll, |d| d.overflow_y_scroll())
            .when(show_header, |d| d.child(header))
            .child(body)
            .when(has_footer, |d| d.child(footer));
        let surface = TextRole::Body.apply(
            theme,
            div()
                .id(self.id)
                .test_support()
                .track_focus(&focus)
                .tab_stop(false)
                .role(if self.alert {
                    Role::AlertDialog
                } else {
                    Role::Dialog
                })
                .aria_label(label)
                .when_some(self.description, |d, text| d.aria_description(text))
                .relative()
                .line_height(gpui_kit::rems(
                    TextRole::Body.style(theme).line_height / 16.,
                ))
                .left(offset.x)
                .top(offset.y + distance)
                .w(self.width.unwrap_or_else(|| self.size.width(theme).into()))
                .max_w(max_width)
                .max_h(max_height)
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .flex()
                .flex_col()
                .bg(rgba(theme, keys::DIALOG_BACKGROUND))
                .text_color(rgba(theme, keys::DIALOG_COLOR))
                .border(border)
                .border_color(rgba(theme, keys::DIALOG_BORDER_COLOR))
                .rounded(rems(theme, keys::DIALOG_BORDER_RADIUS))
                .shadow(shadow(theme, keys::DIALOG_SHADOW))
                .opacity(presence.progress)
                .occlude()
                .child(contents)
                .child(capture)
                .when(
                    self.draggable && self.header_visible && open && !pending && top,
                    |d| d.child(drag.capture()),
                ),
        );
        let top =
            gpui_kit::base::active_modal_scope(w, cx).is_some_and(|a| a.owner() == scope.owner());
        let state = self.state.downgrade();
        Dialog::new(cx)
            .open(true)
            .focus_handle(focus)
            .layer(scope.priority(), top)
            .priority(scope.priority())
            .role_on_popup(true)
            .confirm_on_enter(enter)
            .interactive(open && top)
            .backdrop(
                div()
                    .size_full()
                    .when(self.overlay && top, |d| {
                        d.bg(rgba(theme, keys::DIALOG_SCRIM_COLOR))
                    })
                    .opacity(if nested { 1. } else { presence.progress }),
            )
            .popup(ModalBoundary::surface(surface, scope).busy(pending))
            .on_request(move |intent, w, cx| {
                if intent == DialogIntent::BackdropPress
                    && crate::popup::PopupRuntime::for_window(w, cx).owner_press_suppressed()
                {
                    return;
                }
                let _ = state.update(cx, |s, cx| match intent {
                    DialogIntent::Confirm => {
                        s.confirm(w, cx);
                    }
                    DialogIntent::Cancel => {
                        s.cancel(w, cx);
                    }
                    DialogIntent::Escape => {
                        s.request_close(DialogCloseReason::Escape, w, cx);
                    }
                    DialogIntent::BackdropPress => {
                        s.request_close(DialogCloseReason::OverlayPress, w, cx);
                    }
                });
            })
            .into_any_element()
    }
}
