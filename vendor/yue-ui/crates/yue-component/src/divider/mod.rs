//! Quiet, non-interactive rules for separating related content.
use crate::{
    YueTheme,
    control_layout::ControlLayout,
    theme::keys,
    tokens::{pixels, rems, rgba},
};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{
    App, Bounds, DefiniteLength, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, Rgba, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, div, fill, point,
    prelude::FluentBuilder as _, px, size,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DividerLayout {
    #[default]
    Horizontal,
    Vertical,
}

/// Position of a horizontal divider's label; the short end uses `--space-24`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DividerAlign {
    Start,
    #[default]
    Center,
    End,
}

/// The painted pattern of a horizontal or vertical divider.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DividerLineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// A solid, dashed or dotted rule. Horizontal labels may align at either end or center.
/// Vertical rules are unlabelled and default to the body line height.
///
/// ```no_run
/// use yue_component::{DividerAlign, YueDivider, YueTheme, ThemeMode, Accent};
/// let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
/// let divider = YueDivider::new("details-rule", theme)
///     .label("详细信息")
///     .align(DividerAlign::Start)
///     .dashed(true);
/// ```
#[derive(IntoElement)]
pub struct YueDivider {
    id: ElementId,
    theme: YueTheme,
    inherited: bool,
    orientation: DividerLayout,
    align: DividerAlign,
    line_style: DividerLineStyle,
    label: Option<SharedString>,
    height: Option<DefiniteLength>,
    layout: ControlLayout,
}
crate::control_layout::control_layout!(YueDivider);

impl YueDivider {
    pub fn new(id: impl Into<ElementId>, theme: YueTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            inherited: false,
            orientation: DividerLayout::Horizontal,
            align: DividerAlign::Center,
            line_style: DividerLineStyle::Solid,
            label: None,
            height: None,
            layout: Default::default(),
        }
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.theme = theme;
        self.inherited = false;
        self
    }
    pub fn use_default_theme(mut self) -> Self {
        self.inherited = true;
        self
    }
    pub fn layout(mut self, layout: DividerLayout) -> Self {
        self.orientation = layout;
        self
    }
    pub fn align(mut self, align: DividerAlign) -> Self {
        self.align = align;
        self
    }
    pub fn line_style(mut self, style: DividerLineStyle) -> Self {
        self.line_style = style;
        self
    }
    /// Compatibility shorthand for Dashed / Solid; the last pattern setter wins.
    pub fn dashed(self, dashed: bool) -> Self {
        self.line_style(if dashed {
            DividerLineStyle::Dashed
        } else {
            DividerLineStyle::Solid
        })
    }
    /// Horizontal label. Empty / whitespace-only text produces a plain rule.
    ///
    /// # Panics
    /// Rendering a vertical rule with a nonempty label is invalid. Use a
    /// neighbouring text element for inline content instead.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.label = (!label.trim().is_empty()).then_some(label);
        self
    }
    /// Set the outer height, normally used for a vertical rule in a toolbar.
    pub fn height(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = Some(height.into());
        self
    }
}

/// A rule is a filled strip, not the edge of a rectangle. A one-pixel top/left
/// border has its only sample on the rectangle's centre, where GPUI's border
/// shader selects the opposite (zero-width) edge and can discard the stroke.
fn stroke(
    vertical: bool,
    style: DividerLineStyle,
    thickness: Pixels,
    color: Rgba,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let length = if vertical {
                bounds.size.height
            } else {
                bounds.size.width
            };
            let cross = thickness.min(if vertical {
                bounds.size.width
            } else {
                bounds.size.height
            });
            if length <= px(0.) || cross <= px(0.) {
                return;
            }
            // Pattern dimensions are ratios of the theme's rem-scaled line width.
            let (dash, gap) = match style {
                DividerLineStyle::Solid => (length, px(0.)),
                DividerLineStyle::Dashed => (thickness * 4., thickness * 3.),
                DividerLineStyle::Dotted => (cross, thickness * 3.),
            };
            let step = dash + gap;
            let mut offset = px(0.);
            while offset < length {
                let run = dash.min(length - offset);
                let (origin, extent) = if vertical {
                    (bounds.origin + point(px(0.), offset), size(cross, run))
                } else {
                    (bounds.origin + point(offset, px(0.)), size(run, cross))
                };
                let mut quad = fill(Bounds::new(origin, extent), color);
                if style == DividerLineStyle::Dotted {
                    quad.corner_radii = (run.min(cross) / 2.).into();
                }
                window.paint_quad(quad);
                offset += step;
            }
        },
    )
    .absolute()
    .inset_0()
}

impl RenderOnce for YueDivider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = if self.inherited {
            crate::current_theme(cx)
        } else {
            self.theme
        };
        let vertical = self.orientation == DividerLayout::Vertical;
        assert!(
            !vertical || self.label.is_none(),
            "Vertical YueDivider does not accept a label"
        );
        let thickness =
            px(pixels(theme, keys::DIVIDER_BORDER_WIDTH) / 16. * f32::from(window.rem_size()));
        let color = rgba(theme, keys::DIVIDER_BORDER_COLOR);
        let margin = rems(theme, keys::DIVIDER_MARGIN_BLOCK);
        let mut root = div().id(self.id).test_support().relative().flex_shrink_0();
        if vertical {
            root = root
                .w(thickness)
                .h(self
                    .height
                    .unwrap_or_else(|| rems(theme, keys::LINE_HEIGHT_MD).into()))
                .mx(margin)
                .child(stroke(true, self.line_style, thickness, color));
        } else {
            root = root.w_full().min_w_0().my(margin);
            if let Some(label) = self.label {
                let rule = |id, short| {
                    div()
                        .id(id)
                        .test_support()
                        .relative()
                        .min_w_0()
                        .h(thickness)
                        .child(stroke(false, self.line_style, thickness, color))
                        .when(short, |line| line.w(rems(theme, keys::SPACE_24)))
                        .when(!short, |line| line.flex_1())
                };
                root = root
                    .flex()
                    .items_center()
                    .gap(rems(theme, keys::SPACE_16))
                    .role(Role::Caption)
                    .aria_label(label.clone())
                    .child(rule("divider-leading", self.align == DividerAlign::Start))
                    .child(
                        div()
                            .id("divider-label")
                            .test_support()
                            .min_w_0()
                            .text_size(rems(theme, keys::FONT_SIZE_MD))
                            .line_height(rems(theme, keys::LINE_HEIGHT_MD))
                            .text_color(rgba(theme, keys::TEXT_PRIMARY))
                            .child(label),
                    )
                    .child(rule("divider-trailing", self.align == DividerAlign::End));
            } else {
                root = root
                    .h(thickness)
                    .child(stroke(false, self.line_style, thickness, color));
            }
            if let Some(height) = self.height {
                root = root.h(height);
            }
        }
        self.layout.apply(root)
    }
}

#[cfg(test)]
mod tests;
