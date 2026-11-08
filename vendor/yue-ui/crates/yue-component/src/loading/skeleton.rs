use super::*;
use crate::{
    theme::keys as k,
    tokens::{duration, mix_rgba, rems, rgba},
};
use gpui_kit::{
    Bounds, ContentMask, DefiniteLength, InteractiveElement as _, canvas, fill, linear_color_stop,
    linear_gradient, point, prelude::FluentBuilder as _, px, size,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SkeletonShape {
    #[default]
    Text,
    Rectangle,
    Rounded,
    Circle,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SkeletonAnimation {
    #[default]
    Wave,
    Pulse,
    None,
}

/// Decorative loading geometry. Compose real layout with stable ids; a parent
/// LoadingRegion owns the single busy/status announcement.
///
/// ```
/// use yue_component::{YueSkeleton,SkeletonShape,SkeletonAnimation};
/// let image=YueSkeleton::new("image").shape(SkeletonShape::Rounded);
/// let text=YueSkeleton::new("title").animation(SkeletonAnimation::Pulse);
/// ```
#[derive(IntoElement)]
pub struct YueSkeleton {
    id: ElementId,
    theme: Option<YueTheme>,
    size: FeedbackSize,
    shape: SkeletonShape,
    animation: SkeletonAnimation,
    height: Option<DefiniteLength>,
    layout: crate::control_layout::ControlLayout,
}
crate::control_layout::control_layout!(YueSkeleton);
impl YueSkeleton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            theme: None,
            size: Default::default(),
            shape: Default::default(),
            animation: Default::default(),
            height: None,
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
    pub fn shape(mut self, shape: SkeletonShape) -> Self {
        self.shape = shape;
        self
    }
    pub fn animation(mut self, animation: SkeletonAnimation) -> Self {
        self.animation = animation;
        self
    }
    pub fn height(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = Some(height.into());
        self
    }
}
impl RenderOnce for YueSkeleton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui_kit::base::TestSupportExt as _;
        let theme = self.theme.unwrap_or_else(|| crate::current_theme(cx));
        let line = rems(
            theme,
            self.size.choose(
                k::SKELETON_LINE_SM,
                k::SKELETON_LINE_MD,
                k::SKELETON_LINE_LG,
            ),
        );
        let text = rems(
            theme,
            self.size.choose(
                k::SKELETON_TEXT_SM,
                k::SKELETON_TEXT_MD,
                k::SKELETON_TEXT_LG,
            ),
        );
        let circle = rems(
            theme,
            self.size.choose(
                k::SKELETON_CIRCLE_SM,
                k::SKELETON_CIRCLE_MD,
                k::SKELETON_CIRCLE_LG,
            ),
        );
        let mut root = div()
            .id(self.id)
            .test_support()
            .relative()
            .min_w_0()
            .flex_shrink_0()
            .w(rems(theme, k::SKELETON_WIDTH))
            .h(if self.shape == SkeletonShape::Text {
                line
            } else {
                rems(theme, k::SKELETON_HEIGHT)
            });
        if self.shape == SkeletonShape::Circle {
            root = root.size(circle);
        }
        root = self
            .layout
            .apply(root)
            .when_some(self.height, |d, h| d.h(h));
        root.child(
            canvas(
                |_, _, _| (),
                move |mut b, _, w, cx| {
                    if self.shape == SkeletonShape::Text {
                        let h = text.to_pixels(w.rem_size()).min(b.size.height);
                        b.origin.y += (b.size.height - h) / 2.;
                        b.size.height = h;
                    } else if self.shape == SkeletonShape::Circle {
                        let side = b.size.width.min(b.size.height);
                        b.origin += point((b.size.width - side) / 2., (b.size.height - side) / 2.);
                        b.size = size(side, side);
                    }
                    let radius = match self.shape {
                        SkeletonShape::Circle => b.size.width / 2.,
                        SkeletonShape::Rectangle => px(0.),
                        _ => rems(theme, k::SKELETON_RADIUS)
                            .to_pixels(w.rem_size())
                            .min(b.size.height / 2.)
                            .min(b.size.width / 2.),
                    };
                    let base = rgba(theme, k::SKELETON_BASE);
                    let highlight = rgba(theme, k::SKELETON_HIGHLIGHT);
                    let moving = self.animation != SkeletonAnimation::None && !cx.reduce_motion();
                    let phase = gpui_kit::base::loading_phase(
                        b,
                        duration(theme, k::SKELETON_DURATION),
                        moving,
                        w,
                        cx,
                    );
                    let color = if moving && self.animation == SkeletonAnimation::Pulse {
                        mix_rgba(
                            base,
                            highlight,
                            (1. - (phase * std::f32::consts::TAU).cos()) * 0.3,
                        )
                    } else {
                        base
                    };
                    w.paint_quad(fill(b, color).corner_radii(radius));
                    if moving && self.animation == SkeletonAnimation::Wave && b.size.width > px(0.)
                    {
                        let center = phase * 1.8 - 0.4;
                        let half = 0.3;
                        // Two gradient halves paint the same rounded full quad through
                        // rectangular masks: the moving band never squares the silhouette.
                        for (left, right, from, to) in [
                            (center - half, center, base, highlight),
                            (center, center + half, highlight, base),
                        ] {
                            let start = left.clamp(0., 1.);
                            let end = right.clamp(0., 1.);
                            if start >= end {
                                continue;
                            }
                            // GPUI color stops are normalized to [0, 1]. When
                            // a half-band crosses an edge, interpolate its edge
                            // colors as well so clipping does not stretch it.
                            let edge_from = mix_rgba(from, to, (start - left) / (right - left));
                            let edge_to = mix_rgba(from, to, (end - left) / (right - left));
                            let mask = Bounds::new(
                                b.origin + point(b.size.width * start, px(0.)),
                                size(b.size.width * (end - start), b.size.height),
                            )
                            .intersect(&b);
                            if mask.size.width > px(0.) && mask.size.height > px(0.) {
                                w.with_content_mask(Some(ContentMask { bounds: mask }), |w| {
                                    w.paint_quad(
                                        fill(
                                            b,
                                            linear_gradient(
                                                90.,
                                                linear_color_stop(edge_from, start),
                                                linear_color_stop(edge_to, end),
                                            ),
                                        )
                                        .corner_radii(radius),
                                    )
                                });
                            }
                        }
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
    }
}
