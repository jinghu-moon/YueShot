use super::*;
use crate::{
    popup::{ArrowSpec, ArrowStyle},
    theme::keys,
    tokens::{duration, easing, number, pixels, rems, rgba, shadow},
};
use gpui_kit::{Pixels, Rgba, Size, TextRun, px, size};

#[derive(Clone, Copy)]
pub(super) struct Measure {
    pub size: Size<Pixels>,
    pub line_height: Pixels,
    pub single_line: bool,
}
pub(super) fn length(theme: YueTheme, key: crate::theme::TokenKey, window: &Window) -> Pixels {
    px(pixels(theme, key) / 16. * f32::from(window.rem_size()))
}
pub(super) fn colors(config: &Config) -> (Rgba, Rgba, Rgba, Pixels) {
    let t = config.theme;
    let (background, foreground) = match config.options.variant {
        TooltipVariant::Default => (keys::BOX_BACKGROUND_TOOLTIP, keys::BOX_COLOR_TOOLTIP),
        TooltipVariant::Primary => (keys::ACCENT_SOLID, keys::ON_ACCENT),
        TooltipVariant::Success => (keys::SUCCESS_SOLID, keys::ON_SUCCESS_SOLID),
        TooltipVariant::Danger => (keys::ERROR_SOLID, keys::ON_ERROR_SOLID),
        TooltipVariant::Warning => (keys::WARNING_SOLID, keys::ON_WARNING_SOLID),
        TooltipVariant::Light => (keys::BOX_BACKGROUND_POPOVER, keys::BOX_COLOR_POPOVER),
    };
    let border = if config.options.variant == TooltipVariant::Light {
        rgba(t, keys::BOX_BORDER_COLOR_POPOVER)
    } else {
        rgba(t, background)
    };
    (
        rgba(t, background),
        rgba(t, foreground),
        border,
        if config.options.variant == TooltipVariant::Light {
            px(pixels(t, keys::BOX_BORDER_WIDTH_POPOVER))
        } else {
            px(0.)
        },
    )
}
pub(super) fn measure(config: &Config, window: &Window) -> Measure {
    let t = config.theme;
    let (_, color, _, raw_border) = colors(config);
    let border = raw_border * (f32::from(window.rem_size()) / 16.);
    let font_size = length(t, keys::BOX_FONT_SIZE_TOOLTIP, window);
    let line_height = window.pixel_snap(length(t, keys::LINE_HEIGHT_SM, window));
    let padding_x = length(t, keys::BOX_PADDING_INLINE_TOOLTIP, window) + border;
    let padding_y = length(t, keys::BOX_PADDING_BLOCK_TOOLTIP, window) + border;
    let max_width = length(t, keys::TOOLTIP_MAX_WIDTH, window)
        .min(
            crate::popup::PopupHost::available_size(
                arrow(config, window),
                outsets(config, window),
                length(t, keys::TOOLTIP_VIEWPORT_MARGIN, window),
                window,
            )
            .width,
        )
        .max(padding_x * 2. + px(1.));
    let runs = [TextRun {
        len: config.text.len(),
        font: text_font(config),
        color: color.into(),
        ..Default::default()
    }];
    let lines = window
        .text_system()
        .shape_text(
            config.text.clone(),
            font_size,
            &runs,
            Some(max_width - padding_x * 2.),
            None,
        )
        .expect("Tooltip text shaping");
    let mut width = px(0.);
    let mut height = px(0.);
    let mut count = 0;
    for line in &lines {
        let s = line.size(line_height);
        width = width.max(s.width);
        height += s.height;
        count += line.wrap_boundaries.len() + 1;
    }
    Measure {
        size: size(
            // Layout snaps the inner edges independently. Reserve one device
            // pixel, then round outwards so a final glyph cannot wrap at the
            // measured boundary (especially with fractional DPI/rem sizes).
            px(
                ((f32::from(width + padding_x * 2.) * window.scale_factor()).ceil() + 1.)
                    / window.scale_factor(),
            )
            .min(max_width),
            height + padding_y * 2.,
        ),
        line_height,
        single_line: count <= 1,
    }
}
fn text_font(config: &Config) -> gpui_kit::Font {
    let family = crate::typography::ui_font(config.theme);
    let mut font = gpui_kit::font(family.family.clone());
    font.fallbacks = family.font_fallbacks();
    font.weight = gpui_kit::FontWeight(number(config.theme, keys::WEIGHT_REGULAR));
    font
}
pub(super) fn arrow(config: &Config, window: &Window) -> Option<ArrowStyle> {
    if !config.options.arrow {
        return None;
    }
    let (fill, _, stroke, border) = colors(config);
    let base = length(config.theme, keys::TOOLTIP_ARROW_SIZE, window);
    Some(
        ArrowSpec::new(base, base / 2.)
            .corner_radii([length(config.theme, keys::BOX_BORDER_RADIUS_TOOLTIP, window); 4])
            .border_width(border * (f32::from(window.rem_size()) / 16.))
            .style(fill, stroke),
    )
}
pub(super) fn motion(config: &Config, window: &Window) -> crate::popup::PopupMotion {
    use gpui_kit::base::Transition;
    crate::popup::PopupMotion::new(
        Transition::new(duration(config.theme, keys::TOOLTIP_DURATION_ENTER))
            .ease(easing(config.theme, keys::TOOLTIP_EASE_ENTER)),
        Transition::new(duration(config.theme, keys::TOOLTIP_DURATION_EXIT))
            .ease(easing(config.theme, keys::TOOLTIP_EASE_EXIT)),
    )
    .entrance_distance(length(config.theme, keys::SPACE_4, window))
}
pub(super) fn shadows(config: &Config, window: &Window) -> Vec<gpui_kit::BoxShadow> {
    let scale = f32::from(window.rem_size()) / 16.;
    shadow(config.theme, keys::TOOLTIP_SHADOW)
        .into_iter()
        .map(|mut s| {
            s.offset.x *= scale;
            s.offset.y *= scale;
            s.blur_radius *= scale;
            s.spread_radius *= scale;
            s
        })
        .collect()
}
pub(super) fn outsets(config: &Config, window: &Window) -> gpui_kit::Edges<Pixels> {
    let mut outsets = gpui_kit::Edges::all(px(0.));
    for s in shadows(config, window) {
        let radius = (s.blur_radius + s.spread_radius).max(px(0.));
        outsets.left = outsets.left.max(radius - s.offset.x);
        outsets.right = outsets.right.max(radius + s.offset.x);
        outsets.top = outsets.top.max(radius - s.offset.y);
        outsets.bottom = outsets.bottom.max(radius + s.offset.y);
    }
    outsets
}
pub(super) fn content(
    config: &Config,
    measure: Measure,
    semantic: bool,
    text_opacity: f32,
    old_text: Option<(Config, Measure, f32)>,
    window: &Window,
) -> AnyElement {
    use gpui_kit::base::TestSupportExt as _;
    use gpui_kit::{
        InteractiveElement as _, StatefulInteractiveElement as _, prelude::FluentBuilder as _,
    };
    let t = config.theme;
    let (background, color, border, width) = colors(config);
    div()
        .id("yue-tooltip-surface")
        .test_support()
        .when(semantic, |surface| {
            surface
                .role(gpui_kit::Role::Tooltip)
                .aria_label(config.text.clone())
        })
        .w(measure.size.width)
        .h(measure.size.height)
        .relative()
        .rounded(rems(t, keys::BOX_BORDER_RADIUS_TOOLTIP))
        .bg(background)
        .shadow(shadows(config, window))
        .text_color(color)
        .border(gpui_kit::rems(f32::from(width) / 16.))
        .border_color(border)
        .children(old_text.map(|(old, measure, opacity)| {
            text_layer(&old, measure, opacity, "yue-tooltip-old-text")
        }))
        .child(text_layer(
            config,
            measure,
            text_opacity,
            "yue-tooltip-text",
        ))
        .into_any_element()
}

fn text_layer(config: &Config, measure: Measure, opacity: f32, id: &'static str) -> AnyElement {
    use gpui_kit::InteractiveElement as _;
    use gpui_kit::base::TestSupportExt as _;
    use gpui_kit::prelude::FluentBuilder as _;
    let (_, foreground, _, _) = colors(config);
    div()
        .absolute()
        .inset_0()
        .px(rems(config.theme, keys::BOX_PADDING_INLINE_TOOLTIP))
        .py(rems(config.theme, keys::BOX_PADDING_BLOCK_TOOLTIP))
        .text_color(foreground)
        .font(text_font(config))
        .text_size(rems(config.theme, keys::BOX_FONT_SIZE_TOOLTIP))
        .line_height(measure.line_height)
        .when(measure.single_line, |d| d.whitespace_nowrap())
        .when(!measure.single_line, |d| d.whitespace_normal())
        .opacity(opacity)
        .child(
            div()
                .id(id)
                .test_support()
                .w_full()
                .child(config.text.clone()),
        )
        .into_any_element()
}
