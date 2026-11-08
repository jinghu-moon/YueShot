use super::{state::Record, *};
use crate::{
    ButtonSize, ButtonVariant, Icon, TextRole, YueButton, YueTheme,
    theme::keys,
    tokens::{self, rems, rgba},
};
use gpui_kit::base::{
    A11yLive, ElementExt as _, Toast, ToastTransitionStatus, Transition, transition_at,
};
use gpui_kit::{
    AnyElement, App, Bounds, ContentMask, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Role, StatefulInteractiveElement as _, Styled as _, Window, canvas,
    div, fill, point, prelude::FluentBuilder as _, px, size,
};
use std::time::Instant;

pub(super) fn length(theme: YueTheme, key: crate::theme::TokenKey, w: &Window) -> Pixels {
    w.rem_size() * (tokens::pixels(theme, key) / 16.)
}

fn colors(
    spec: &ToastSpec,
    t: YueTheme,
) -> (
    gpui_kit::Rgba,
    gpui_kit::Rgba,
    gpui_kit::Rgba,
    gpui_kit::Rgba,
) {
    let (background, foreground, border) = match spec.appearance {
        ToastAppearance::Inverse => (
            keys::BOX_BACKGROUND_TOAST,
            keys::BOX_COLOR_TOAST,
            keys::BOX_BACKGROUND_TOAST,
        ),
        ToastAppearance::Rich => match spec.kind {
            ToastKind::Success => (
                keys::SUCCESS_BG,
                keys::SUCCESS_FOREGROUND,
                keys::SUCCESS_BORDER,
            ),
            ToastKind::Info => (keys::INFO_BG, keys::INFO_FOREGROUND, keys::INFO_BORDER),
            ToastKind::Warning => (
                keys::WARNING_BG,
                keys::WARNING_FOREGROUND,
                keys::WARNING_BORDER,
            ),
            ToastKind::Error => (keys::ERROR_BG, keys::ERROR_FOREGROUND, keys::ERROR_BORDER),
            _ => (
                keys::TOAST_BACKGROUND,
                keys::TOAST_COLOR,
                keys::TOAST_BORDER_COLOR,
            ),
        },
        _ => (
            keys::TOAST_BACKGROUND,
            keys::TOAST_COLOR,
            keys::TOAST_BORDER_COLOR,
        ),
    };
    let accent = match spec.kind {
        ToastKind::Success => keys::SUCCESS,
        ToastKind::Warning => keys::WARNING,
        ToastKind::Error => keys::ERROR,
        ToastKind::Info => keys::INFO,
        _ => keys::ACCENT_SOLID,
    };
    (
        rgba(t, background),
        rgba(t, foreground),
        rgba(t, border),
        if spec.appearance == ToastAppearance::Inverse {
            rgba(t, foreground)
        } else {
            rgba(t, accent)
        },
    )
}

pub(super) struct Body<'a> {
    pub spec: &'a ToastSpec,
    pub record: &'a Record,
    pub theme: YueTheme,
    pub width: Pixels,
    pub ticket: Option<ToastTicket>,
    pub controls: bool,
    pub keyboard: bool,
    pub close_label: gpui_kit::SharedString,
    pub now: Instant,
    pub reduced: bool,
}
pub(super) fn body(input: Body<'_>, w: &mut Window) -> AnyElement {
    let Body {
        spec,
        record,
        theme: t,
        width,
        ticket,
        controls,
        keyboard,
        close_label,
        now,
        reduced,
    } = input;
    let (_, foreground, _, accent) = colors(spec, t);
    let pad = length(t, keys::TOAST_PADDING_INLINE, w);
    let gap = length(t, keys::SPACE_8, w);
    let icon_width = if record.icon_slot {
        length(t, keys::TOAST_ICON_SIZE, w) + gap
    } else {
        px(0.)
    };
    let close_width = if spec.can_close() {
        length(t, keys::TOAST_CLOSE_SIZE, w) + gap
    } else {
        px(0.)
    };
    let available = (width - pad * 2. - icon_width - close_width).max(px(0.));
    // Match the action's font metrics and token-owned Small padding.
    let action_width = spec
        .action
        .as_ref()
        .map_or(px(0.), |a| measured_action_width(a, t, w));
    let inline = spec.action.is_some()
        && available - action_width - gap >= length(t, keys::TOAST_CONTENT_MIN_WIDTH, w);
    let mut copy = div()
        .min_w_0()
        .flex_1()
        .flex()
        .flex_col()
        .gap(rems(t, keys::SPACE_4))
        .child(
            div()
                .min_w_0()
                .font_weight(FontWeight::MEDIUM)
                .child(spec.title.clone()),
        );
    if let Some(description) = &spec.description {
        copy = copy.child(
            div()
                .min_w_0()
                .text_color(if spec.appearance == ToastAppearance::Standard {
                    rgba(t, keys::TOAST_DESCRIPTION_COLOR)
                } else {
                    foreground
                })
                .child(description.clone()),
        );
    }
    let mut action = spec.action.as_ref().map(|a| {
        if !controls {
            return div()
                .h(rems(t, keys::BUTTON_HEIGHT_SM))
                .px(rems(t, keys::BUTTON_PADDING_INLINE_SM))
                .border_1()
                .border_color(rgba(t, keys::BUTTON_DEFAULT_BORDER_COLOR))
                .rounded(rems(t, keys::TOAST_BORDER_RADIUS))
                .flex()
                .items_center()
                .text_size(rems(t, keys::BUTTON_FONT_SIZE_SM))
                .font_weight(FontWeight::MEDIUM)
                .bg(rgba(t, keys::BUTTON_DEFAULT_BACKGROUND))
                .text_color(rgba(t, keys::BUTTON_DEFAULT_COLOR))
                .child(a.label.clone())
                .into_any_element();
        }
        let handler = a.handler.clone();
        let revision = record.revision;
        let ticket = ticket.clone();
        let button = YueButton::text("toast-action", t, a.label.clone())
            .size(ButtonSize::Small)
            .variant(ButtonVariant::Default)
            .track_focus(&record.action_focus)
            .tab_stop(controls && keyboard)
            .disabled(!controls)
            .on_click(move |_, w, cx| {
                let Some(ticket) = &ticket else {
                    return;
                };
                if ticket
                    .owner
                    .upgrade()
                    .is_none_or(|state| !state.read(cx).actionable(ticket, revision, w))
                {
                    return;
                }
                if handler(ticket, w, cx) == ToastDecision::Close {
                    let _ = ticket.owner.update(cx, |s, cx| {
                        s.dismiss(ticket, ToastDismissReason::Action, w, cx)
                    });
                }
            });
        let observed = record.action_bounds.clone();
        div()
            .flex_shrink_0()
            .on_prepaint(move |bounds, _, _| observed.set(Some(bounds)))
            .child(button)
            .into_any_element()
    });
    if !inline && let Some(action) = action.take() {
        copy = copy.child(div().min_w_0().mt(rems(t, keys::SPACE_4)).child(action));
    }
    let mut row = div().w_full().min_w_0().flex().items_start().gap(gap);
    if record.icon_slot {
        let mut slot = div().size(rems(t, keys::TOAST_ICON_SIZE)).flex_shrink_0();
        if let Some(glyph) = spec.glyph() {
            let turns = if glyph == crate::YueIcon::Loading && !reduced {
                now.saturating_duration_since(record.changed_at)
                    .as_secs_f32()
                    / crate::MotionDuration::Loop.duration(t).as_secs_f32()
            } else {
                0.
            };
            slot = slot.child(
                Icon::new(glyph)
                    .size(rems(t, keys::TOAST_ICON_SIZE))
                    .color_value(accent)
                    .rotation(turns),
            );
        }
        row = row.child(slot);
    }
    row = row.child(copy);
    if let Some(action) = action {
        row = row.child(action);
    }
    if spec.can_close() {
        let ticket = ticket.clone();
        if controls {
            let observed = record.close_bounds.clone();
            row = row.child(
                div()
                    .flex_shrink_0()
                    .on_prepaint(move |bounds, _, _| observed.set(Some(bounds)))
                    .child(
                        YueButton::icon("toast-close", t, crate::YueIcon::Close, close_label)
                            .size(ButtonSize::Small)
                            .icon_size_override(rems(t, keys::TOAST_CLOSE_ICON_SIZE))
                            .variant(ButtonVariant::Ghost)
                            .track_focus(&record.close_focus)
                            .tab_stop(controls && keyboard)
                            .disabled(!controls)
                            .on_click(move |_, w, cx| {
                                if let Some(ticket) = &ticket {
                                    let _ = ticket.owner.update(cx, |s, cx| {
                                        s.dismiss(ticket, ToastDismissReason::CloseButton, w, cx)
                                    });
                                }
                            }),
                    ),
            );
        } else {
            row = row.child(
                div()
                    .size(rems(t, keys::TOAST_CLOSE_SIZE))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(crate::YueIcon::Close)
                            .size(rems(t, keys::TOAST_CLOSE_ICON_SIZE))
                            .color_value(foreground),
                    ),
            );
        }
    }
    TextRole::Body
        .apply(
            t,
            div()
                .w_full()
                .min_w_0()
                .px(rems(t, keys::TOAST_PADDING_INLINE))
                .py(rems(t, keys::TOAST_PADDING_BLOCK))
                .text_color(foreground)
                .line_height(gpui_kit::rems(TextRole::Body.style(t).line_height / 16.))
                .child(row)
                .when(spec.progress && spec.lifetime().is_some(), |d| {
                    d.pb(rems(t, keys::TOAST_PADDING_BLOCK) + rems(t, keys::TOAST_PROGRESS_HEIGHT))
                }),
        )
        .into_any_element()
}

fn measured_action_width(action: &ToastAction, t: YueTheme, w: &Window) -> Pixels {
    // Match Button's Small text font and horizontal padding. GPUI measures Unicode glyphs.
    let size = length(t, keys::BUTTON_FONT_SIZE_SM, w);
    let font = crate::typography::FontStack::from_token(t, "--font-ui");
    let run = gpui_kit::TextRun {
        len: action.label.len(),
        font: gpui_kit::Font {
            family: font.family,
            weight: FontWeight::MEDIUM,
            ..Default::default()
        },
        color: rgba(t, keys::TOAST_COLOR).into(),
        ..Default::default()
    };
    w.text_system()
        .shape_text(action.label.clone(), size, &[run], None, None)
        .map(|lines| {
            lines
                .iter()
                .map(|l| l.size(size).width)
                .fold(px(0.), Pixels::max)
        })
        .unwrap_or(px(0.))
        + length(t, keys::BUTTON_PADDING_INLINE_SM, w) * 2.
        + px(2.)
}

pub(super) struct Card<'a> {
    pub record: &'a Record,
    pub spec: &'a ToastSpec,
    pub theme: YueTheme,
    pub width: Pixels,
    pub height: Pixels,
    pub ticket: ToastTicket,
    pub phase: ToastTransitionStatus,
    pub content: f32,
    pub controls: bool,
    pub keyboard: bool,
    pub progress: Option<f32>,
    pub close_label: gpui_kit::SharedString,
    pub now: Instant,
    pub reduced: bool,
    pub running: bool,
}
pub(super) struct PaintedCard {
    pub element: AnyElement,
    pub colors: [gpui_kit::Rgba; 4],
    pub body: ToastSpec,
    pub body_alpha: f32,
}
pub(super) fn content_sample(
    record: &Record,
    now: Instant,
    duration: std::time::Duration,
    reduced: bool,
) -> (&ToastSpec, f32) {
    let changed = now
        .saturating_duration_since(record.changed_at)
        .as_secs_f32()
        / duration.as_secs_f32();
    if !reduced
        && changed < 1.
        && let Some(previous) = &record.previous
    {
        if changed < 0.5 {
            (previous, record.previous_alpha * (1. - changed * 2.))
        } else {
            (&record.display, (changed - 0.5) * 2.)
        }
    } else {
        (&record.display, 1.)
    }
}
pub(super) fn render(card: Card<'_>, w: &mut Window, cx: &mut App) -> PaintedCard {
    let Card {
        record,
        spec,
        theme: t,
        width,
        height,
        ticket,
        phase,
        content,
        controls,
        keyboard,
        progress,
        close_label,
        now,
        reduced,
        running,
    } = card;
    let (bg, fg, border, accent) = colors(spec, t);
    let duration = tokens::duration(t, keys::TOAST_DURATION_CONTENT);
    let sampled = if phase == ToastTransitionStatus::Ending {
        record
            .exit
            .and_then(|v| v.colors)
            .unwrap_or([bg, fg, border, accent])
    } else {
        {
            let mut sampled = [bg, fg, border, accent];
            for (index, value) in sampled.iter_mut().enumerate() {
                *value = transition_at(
                    (
                        gpui_kit::ElementId::NamedInteger(
                            format!("toast-color-{index}").into(),
                            record.generation,
                        ),
                        "value",
                    ),
                    gpui_kit::Hsla::from(*value),
                    Transition::new(duration).ease(crate::MotionEase::Fade.easing(t)),
                    now,
                    running && !reduced,
                    w,
                    cx,
                )
                .value
                .into();
            }
            sampled
        }
    };
    let [bg, fg, border, accent] = sampled;
    // The exit layer freezes the selected paragraph and its opacity too.
    let (display, swap_alpha) = if phase == ToastTransitionStatus::Ending {
        (spec, 1.)
    } else {
        content_sample(record, now, duration, reduced)
    };
    let mut inner = div()
        .size_full()
        .rounded(rems(t, keys::TOAST_BORDER_RADIUS))
        .overflow_hidden();
    if content > 0.001 {
        let body = body(
            Body {
                spec: display,
                record,
                theme: t,
                width,
                ticket: Some(ticket),
                controls: controls
                    && swap_alpha > 0.999
                    && (record.previous.is_none()
                        || now.saturating_duration_since(record.changed_at) >= duration),
                keyboard,
                close_label,
                now: if phase == ToastTransitionStatus::Ending {
                    record.exit.and_then(|v| v.sampled_at).unwrap_or(now)
                } else {
                    now
                },
                reduced,
            },
            w,
        );
        inner = inner.child(
            A11yLive::new(
                div()
                    .id("toast-content")
                    .role(Role::Group)
                    .opacity(content * swap_alpha)
                    .child(body),
                gpui_kit::accesskit::Live::Off,
            )
            .hidden(!controls),
        );
    }
    if content > 0.001
        && let Some(progress) = progress.filter(|_| spec.progress)
    {
        let progress = progress.clamp(0., 1.);
        let strip_height = length(t, keys::TOAST_PROGRESS_HEIGHT, w);
        // A div's overflow mask is rectangular. Paint the same rounded surface
        // as the card, revealed only through the countdown strip; rounding the
        // thin strip itself would produce a different silhouette.
        let radius = (length(t, keys::TOAST_BORDER_RADIUS, w) - px(1.)).max(px(0.));
        inner = inner.child(
            div().absolute().inset_0().opacity(content).child(
                canvas(
                    |bounds, _, _| bounds,
                    move |_, bounds, window, _| {
                        let strip_height = strip_height.min(bounds.size.height);
                        let strip = Bounds::new(
                            point(bounds.left(), bounds.bottom() - strip_height),
                            size(bounds.size.width * progress, strip_height),
                        );
                        window.with_content_mask(Some(ContentMask { bounds: strip }), |window| {
                            window.paint_quad(fill(bounds, accent).corner_radii(radius));
                        });
                    },
                )
                .size_full(),
            ),
        );
    }
    let root = Toast::new(("yue-toast-card", record.generation))
        .transition_status(phase)
        .semantic_role(Role::Group)
        .relative()
        .w_full()
        .h(height)
        .bg(bg)
        .text_color(fg)
        .border_1()
        .border_color(border)
        .rounded(rems(t, keys::TOAST_BORDER_RADIUS))
        .shadow(tokens::shadow(t, keys::TOAST_SHADOW))
        .aria_label(spec.spoken())
        .when(controls, |d| d.track_focus(&record.focus).tab_stop(false))
        .child(inner);
    PaintedCard {
        element: root
            .live(gpui_kit::accesskit::Live::Off)
            .a11y_hidden(!controls)
            .into_any_element(),
        colors: sampled,
        body: display.clone(),
        body_alpha: swap_alpha,
    }
}
