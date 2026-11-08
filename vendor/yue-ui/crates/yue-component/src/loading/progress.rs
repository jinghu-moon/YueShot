use super::*;
use crate::{
    theme::keys as k,
    tokens::{duration, easing, rems, rgba},
};
use gpui_kit::base::{Transition, transition};
use gpui_kit::{canvas, fill, prelude::FluentBuilder as _};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProgressShape {
    #[default]
    Linear,
    Circle,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProgressStatus {
    #[default]
    Normal,
    Success,
    Error,
    Paused,
}

/// A host-controlled percentage or unknown progress. Native values reflect the
/// real target immediately; only its visible geometry may interpolate briefly.
///
/// ```
/// use yue_component::{YueProgress,ProgressValue,ProgressShape};
/// let bar=YueProgress::new("upload",ProgressValue::new(42.)).label("Uploading");
/// let ring=YueProgress::new("index",ProgressValue::indeterminate()).shape(ProgressShape::Circle);
/// ```
#[derive(IntoElement)]
pub struct YueProgress {
    id: ElementId,
    value: ProgressValue,
    theme: Option<YueTheme>,
    size: FeedbackSize,
    shape: ProgressShape,
    status: ProgressStatus,
    label: Option<SharedString>,
    name: SharedString,
    value_text: Option<SharedString>,
    show_value: bool,
    animated: bool,
    layout: crate::control_layout::ControlLayout,
}
crate::control_layout::control_layout!(YueProgress);
impl YueProgress {
    pub fn new(id: impl Into<ElementId>, value: ProgressValue) -> Self {
        Self {
            id: id.into(),
            value,
            theme: None,
            size: Default::default(),
            shape: Default::default(),
            status: Default::default(),
            label: None,
            name: "进度".into(),
            value_text: None,
            show_value: true,
            animated: true,
            layout: Default::default(),
        }
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn size(mut self, size: FeedbackSize) -> Self {
        self.size = size;
        self
    }
    pub fn shape(mut self, shape: ProgressShape) -> Self {
        self.shape = shape;
        self
    }
    pub fn status(mut self, status: ProgressStatus) -> Self {
        self.status = status;
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
    pub fn value_text(mut self, text: impl Into<SharedString>) -> Self {
        self.value_text = Some(text.into());
        self
    }
    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }
}
impl RenderOnce for YueProgress {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let signature = format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{:?}/{}",
            theme.mode(),
            theme.accent(),
            theme.neutral(),
            self.shape,
            self.size,
            self.status,
            w.rem_size(),
            self.value.is_indeterminate()
        );
        let history =
            w.use_keyed_state((self.id.clone(), "presentation"), cx, |_, _| String::new());
        let changed = *history.read(cx) != signature;
        if changed {
            history.update(cx, |s, _| *s = signature);
        }
        let travel = self.animated && self.status == ProgressStatus::Normal && !changed;
        let painted = transition(
            (self.id.clone(), "value"),
            self.value.percentage().unwrap_or(0.) / 100.,
            Transition::new(if travel {
                duration(theme, k::LOADING_TRANSITION_DURATION)
            } else {
                Duration::ZERO
            })
            .ease(easing(theme, k::LOADING_EASE)),
            w,
            cx,
        );
        let color = rgba(
            theme,
            match self.status {
                ProgressStatus::Normal => k::PROGRESS_FILL,
                ProgressStatus::Success => k::PROGRESS_SUCCESS,
                ProgressStatus::Error => k::PROGRESS_ERROR,
                ProgressStatus::Paused => k::PROGRESS_PAUSED,
            },
        );
        let unknown = self.value.is_indeterminate();
        let status_text: SharedString = match self.status {
            ProgressStatus::Normal => self
                .value
                .percentage()
                .map(|p| format!("{p:.0}%").into())
                .unwrap_or_else(|| "正在处理".into()),
            ProgressStatus::Success => "已完成".into(),
            ProgressStatus::Error => "失败".into(),
            ProgressStatus::Paused => "已暂停".into(),
        };
        let text = self.value_text.clone().unwrap_or(status_text);
        let detail_visible = self.show_value || self.status != ProgressStatus::Normal;
        let mut root = gpui_kit::base::Progress::new(self.id.clone())
            .value(self.value.percentage().unwrap_or(0.))
            .indeterminate(unknown)
            .accessibility_label(self.name)
            .value_text(text.clone())
            .flex()
            .flex_col()
            .min_w_0()
            .gap(rems(theme, k::LOADING_GAP))
            .text_size(rems(theme, self.size.font()))
            .text_color(rgba(theme, k::LOADING_COLOR));
        let running = self.animated && self.status == ProgressStatus::Normal;
        if self.shape == ProgressShape::Linear {
            if self.label.is_some() || detail_visible {
                root = root.child(
                    div()
                        .flex()
                        .items_start()
                        .min_w_0()
                        .gap(rems(theme, k::LOADING_GAP))
                        .child(div().min_w_0().flex_1().children(self.label))
                        .when(detail_visible, |d| {
                            d.child(div().flex_shrink_0().child(text))
                        }),
                );
            }
            let thickness = rems(
                theme,
                self.size.choose(
                    k::PROGRESS_HEIGHT_SM,
                    k::PROGRESS_HEIGHT_MD,
                    k::PROGRESS_HEIGHT_LG,
                ),
            );
            root = root.w_full().child(
                div().relative().w_full().h(thickness).child(
                    canvas(
                        |_, _, _| (),
                        move |b, _, w, cx| {
                            let radius = rems(theme, k::PROGRESS_BORDER_RADIUS)
                                .to_pixels(w.rem_size())
                                .min(b.size.height / 2.);
                            w.paint_quad(
                                fill(b, rgba(theme, k::PROGRESS_TRACK)).corner_radii(radius),
                            );
                            let (a, z) = if unknown {
                                let phase = gpui_kit::base::loading_phase(
                                    b,
                                    duration(theme, k::LOADING_TRACK_DURATION),
                                    running,
                                    w,
                                    cx,
                                );
                                paint::line_segment(phase)
                            } else {
                                (0., painted)
                            };
                            paint::linear(b, a, z, radius, color, w);
                        },
                    )
                    .absolute()
                    .inset_0(),
                ),
            );
        } else {
            let diameter = rems(
                theme,
                self.size.choose(
                    k::PROGRESS_CIRCLE_SIZE_SM,
                    k::PROGRESS_CIRCLE_SIZE_MD,
                    k::PROGRESS_CIRCLE_SIZE_LG,
                ),
            );
            let stroke = rems(
                theme,
                self.size.choose(
                    k::PROGRESS_CIRCLE_STROKE_SM,
                    k::PROGRESS_CIRCLE_STROKE_MD,
                    k::PROGRESS_CIRCLE_STROKE_LG,
                ),
            );
            let inner = match self.status {
                ProgressStatus::Normal => text,
                ProgressStatus::Success => "✓".into(),
                ProgressStatus::Error => "!".into(),
                ProgressStatus::Paused => "Ⅱ".into(),
            };
            root = root.items_center().children(self.label).child(
                div()
                    .relative()
                    .size(diameter)
                    .flex_shrink_0()
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |b, _, w, cx| {
                                let stroke = stroke.to_pixels(w.rem_size());
                                paint::arc(b, stroke, rgba(theme, k::PROGRESS_TRACK), 0., 1., w);
                                if unknown {
                                    let phase = gpui_kit::base::loading_phase(
                                        b,
                                        duration(theme, k::LOADING_SPIN_DURATION),
                                        running,
                                        w,
                                        cx,
                                    );
                                    paint::arc(b, stroke, color, phase, 0.72, w);
                                } else {
                                    paint::arc(b, stroke, color, 0., painted, w);
                                }
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .when(detail_visible, |d| {
                        d.child(
                            div()
                                .absolute()
                                .inset_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .px(stroke)
                                .text_size(rems(
                                    theme,
                                    self.size.choose(
                                        k::PROGRESS_CIRCLE_FONT_SIZE_SM,
                                        k::LOADING_FONT_SIZE_MD,
                                        k::LOADING_FONT_SIZE_LG,
                                    ),
                                ))
                                .child(div().min_w_0().text_ellipsis().child(
                                    if unknown && self.status == ProgressStatus::Normal {
                                        SharedString::from("…")
                                    } else {
                                        inner
                                    },
                                )),
                        )
                    }),
            );
        }
        self.layout.apply(root)
    }
}
