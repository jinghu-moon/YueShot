use super::*;
use crate::{
    theme::keys as k,
    tokens::{duration, rems, rgba},
};
use gpui_kit::{InteractiveElement as _, prelude::FluentBuilder as _};
use std::time::Duration;

/// Delayed, noninteractive waiting indicator. A label explains the real task.
/// Use decorative(true) inside an already named busy button/region.
///
/// ```
/// use yue_component::YueSpinner;
/// let spinner=YueSpinner::new("search-wait").active(true).label("Searching files");
/// ```
#[derive(IntoElement)]
pub struct YueSpinner {
    id: ElementId,
    active: bool,
    theme: Option<YueTheme>,
    size: FeedbackSize,
    label: Option<SharedString>,
    name: SharedString,
    delay: Option<Duration>,
    decorative: bool,
    animated: bool,
    layout: crate::control_layout::ControlLayout,
}
crate::control_layout::control_layout!(YueSpinner);
impl YueSpinner {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            active: true,
            theme: None,
            size: Default::default(),
            label: None,
            name: "正在加载".into(),
            delay: None,
            decorative: false,
            animated: true,
            layout: Default::default(),
        }
    }
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn size(mut self, size: FeedbackSize) -> Self {
        self.size = size;
        self
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        assert!(!label.trim().is_empty());
        self.name = label.clone();
        self.label = Some(label);
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.name = label.into();
        assert!(!self.name.trim().is_empty());
        self
    }
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
    pub fn decorative(mut self, decorative: bool) -> Self {
        self.decorative = decorative;
        self
    }
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }
}
impl RenderOnce for YueSpinner {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let gate = w.use_keyed_state((self.id.clone(), "delay"), cx, |_, _| {
            gpui_kit::base::LoadingDelay::default()
        });
        let visible = gate.update(cx, |s, cx| {
            s.sync(
                self.active,
                self.delay
                    .unwrap_or_else(|| duration(theme, k::LOADING_DELAY)),
                cx,
            )
        });
        if !visible {
            return div().into_any_element();
        }
        let diameter = rems(
            theme,
            self.size
                .choose(k::SPINNER_SIZE_SM, k::SPINNER_SIZE_MD, k::SPINNER_SIZE_LG),
        );
        let stroke = rems(
            theme,
            self.size.choose(
                k::SPINNER_STROKE_SM,
                k::SPINNER_STROKE_MD,
                k::SPINNER_STROKE_LG,
            ),
        );
        let glyph = paint::spinner_glyph(
            theme,
            diameter,
            stroke,
            rgba(theme, k::SPINNER_COLOR),
            duration(theme, k::SPINNER_DURATION),
            self.animated,
        );
        if self.decorative {
            self.layout
                .apply(
                    div()
                        .id(self.id)
                        .flex()
                        .items_center()
                        .gap(rems(theme, k::LOADING_GAP))
                        .text_size(rems(theme, self.size.font()))
                        .text_color(rgba(theme, k::LOADING_COLOR))
                        .child(glyph)
                        .children(self.label),
                )
                .into_any_element()
        } else {
            self.layout
                .apply(
                    gpui_kit::base::Progress::new(self.id)
                        .indeterminate(true)
                        .accessibility_label(self.name)
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap(rems(theme, k::LOADING_GAP))
                        .text_size(rems(theme, self.size.font()))
                        .text_color(rgba(theme, k::LOADING_COLOR))
                        .child(glyph)
                        .when_some(self.label, |p, label| p.child(div().min_w_0().child(label))),
                )
                .into_any_element()
        }
    }
}
