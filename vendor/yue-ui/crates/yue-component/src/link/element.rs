//! Element composition and component-local presentation.

use super::*;

use gpui_kit::base::{Link, Transition, transition};

use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    StatefulInteractiveElement as _, Styled as _, UnderlineStyle, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::theme::{TokenKey, keys};

use crate::tokens::{duration, easing, mix_rgba, number, pixels, rems, rgba, token};

impl LinkVariant {
    fn colors(self) -> (TokenKey, TokenKey) {
        match self {
            Self::Default => (keys::LINK_DEFAULT_COLOR, keys::LINK_DEFAULT_COLOR_HOVER),
            Self::Primary => (keys::LINK_COLOR, keys::LINK_COLOR_HOVER),
            Self::Danger => (keys::LINK_DANGER_COLOR, keys::LINK_DANGER_COLOR_HOVER),
            Self::Warning => (keys::LINK_WARNING_COLOR, keys::LINK_WARNING_COLOR_HOVER),
            Self::Success => (keys::LINK_SUCCESS_COLOR, keys::LINK_SUCCESS_COLOR_HOVER),
        }
    }
}

impl LinkSize {
    fn metrics(self) -> (TokenKey, TokenKey, TokenKey) {
        match self {
            Self::Small => (
                keys::LINK_FONT_SIZE_SM,
                keys::LINK_LINE_HEIGHT_SM,
                keys::LINK_ICON_SIZE_SM,
            ),
            Self::Medium => (
                keys::LINK_FONT_SIZE_MD,
                keys::LINK_LINE_HEIGHT_MD,
                keys::LINK_ICON_SIZE_MD,
            ),
            Self::Large => (
                keys::LINK_FONT_SIZE_LG,
                keys::LINK_LINE_HEIGHT_LG,
                keys::LINK_ICON_SIZE_LG,
            ),
        }
    }
}

#[derive(Default)]
struct LinkMotion {
    hovered: bool,
}

impl RenderOnce for YueLink {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let focus = self.focus.unwrap_or_else(|| {
            window
                .use_keyed_state((self.id.clone(), "yue-link-focus"), cx, |_, cx| {
                    cx.focus_handle()
                })
                .read(cx)
                .clone()
        });
        let motion = window.use_keyed_state((self.id.clone(), "yue-link-motion"), cx, |_, _| {
            LinkMotion::default()
        });
        if self.disabled && motion.read(cx).hovered {
            motion.update(cx, |state, _| state.hovered = false);
        }
        let hovered = !self.disabled && motion.read(cx).hovered;
        let progress = transition(
            (self.id.clone(), "yue-link-hover"),
            if hovered { 1.0 } else { 0.0 },
            Transition::new(duration(theme, keys::LINK_DURATION))
                .ease(easing(theme, keys::LINK_EASE)),
            window,
            cx,
        );
        let (rest, hover) = self.variant.colors();
        let foreground = if self.disabled {
            let mut color = rgba(theme, rest);
            color.a *= number(theme, keys::LINK_OPACITY_DISABLED);
            color
        } else if self.hover == LinkHover::Color {
            mix_rgba(rgba(theme, rest), rgba(theme, hover), progress)
        } else {
            rgba(theme, rest)
        };
        let always_underline = self
            .underline
            .unwrap_or_else(|| token(theme, keys::LINK_DECORATION).value() == "underline");
        let underline_progress = if always_underline {
            1.0
        } else if !self.disabled && self.hover == LinkHover::Underline {
            progress
        } else {
            0.0
        };
        let (font, line, icon) = self.size.metrics();
        let mut text = div().child(self.label.clone());
        // Keep the same text layout at every frame; only underline alpha changes.
        let mut underline_color = foreground;
        underline_color.a *= underline_progress;
        text.text_style().underline = Some(UnderlineStyle {
            thickness: px(pixels(theme, keys::LINK_UNDERLINE_WIDTH)),
            color: Some(underline_color.into()),
            wavy: false,
        });
        let disabled = self.disabled;
        let mut link = Link::new(self.id.clone())
            .href(self.href)
            .track_focus(&focus)
            .tab_index(self.tab_index)
            .tab_stop(self.tab_stop)
            .accessibility_label(self.name.unwrap_or(self.label))
            .disabled(disabled)
            .relative()
            .flex()
            .self_start()
            .items_center()
            .flex_shrink_0()
            .gap(rems(theme, keys::LINK_GAP))
            .text_size(rems(theme, font))
            .line_height(rems(theme, line))
            .font_weight(FontWeight(number(theme, keys::LINK_FONT_WEIGHT)))
            .text_color(foreground)
            .cursor_default()
            .when(!disabled, |link| link.cursor_pointer())
            .on_hover(
                window.listener_for(&motion, move |state, hovered, window, cx| {
                    let hovered = !disabled && *hovered;
                    if state.hovered != hovered {
                        state.hovered = hovered;
                        cx.notify();
                        window.refresh();
                    }
                }),
            );
        if let Some(icon_value) = self.leading {
            link = link.child(icon_value.size(rems(theme, icon)).color_value(foreground));
        }
        link = link.child(text);
        if let Some(icon_value) = self.trailing {
            link = link.child(icon_value.size(rems(theme, icon)).color_value(foreground));
        }
        if !disabled && focus.is_focused(window) && window.last_input_was_keyboard() {
            let outside = rems(theme, keys::LINK_FOCUS_RING_OFFSET)
                + rems(theme, keys::LINK_FOCUS_RING_WIDTH);
            link = link.child(
                div()
                    .absolute()
                    .top(-outside)
                    .right(-outside)
                    .bottom(-outside)
                    .left(-outside)
                    .rounded(rems(theme, keys::LINK_FOCUS_RING_RADIUS))
                    .border(rems(theme, keys::LINK_FOCUS_RING_WIDTH))
                    .border_color(rgba(theme, keys::LINK_FOCUS_RING_COLOR)),
            );
        }
        if let Some(description) = self.description {
            link = link.aria_description(description);
        }
        if let Some(context) = self.key_context {
            link = link.key_context(context.as_ref());
        }
        link = match self.open_with {
            Some(open) => {
                link.open_with(move |href, event, window, cx| open(href, event, window, cx))
            }
            None => link.open_with(|href, _, _, cx| cx.open_url(href)),
        };
        if let Some(handler) = self.on_activate {
            link = link.on_activate(move |event, window, cx| handler(event, window, cx));
        }
        self.layout.apply(link)
    }
}
