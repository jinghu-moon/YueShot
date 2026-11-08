//! Reading Yue tokens as the GPUI values a component styles with.
//!
//! A control asks for a token by its CSS name and gets back a length, colour,
//! duration or easing curve. The parsing lives here so that every component
//! resolves `calc()` sizes, `color-mix()` halos and `cubic-bezier()` curves from
//! the same code path and a retuned token behaves the same wherever it is read.

use std::time::Duration;

use gpui_kit::px;

use crate::theme::{TokenSource, TokenValue};
use crate::{YueTheme, YueToken};

pub(crate) fn token(theme: YueTheme, name: impl TokenSource) -> YueToken {
    theme
        .token(name)
        .unwrap_or_else(|| panic!("missing Yue token {name}"))
}

/// A token already scaled to pixels: a plain `12px` or a `calc()` of one.
pub(crate) fn pixels(theme: YueTheme, name: impl TokenSource) -> f32 {
    token(theme, name)
        .pixels()
        .unwrap_or_else(|| panic!("Yue token {name} is not a pixel length"))
}

pub(crate) fn rems(theme: YueTheme, name: impl TokenSource) -> gpui_kit::Rems {
    gpui_kit::rems(pixels(theme, name) / 16.0)
}

pub(crate) fn rgba(theme: YueTheme, name: impl TokenSource) -> gpui_kit::Rgba {
    token(theme, name)
        .rgba()
        .or_else(|| parse_shadow_color(token(theme, name).value()).map(Into::into))
        .unwrap_or_else(|| panic!("Yue token {name} is not a supported color"))
}

pub(crate) fn number(theme: YueTheme, name: impl TokenSource) -> f32 {
    token(theme, name)
        .number()
        .unwrap_or_else(|| panic!("Yue token {name} is not a number"))
}

/// A CSS `box-shadow` list, read as the layers GPUI draws.
///
/// Yue keeps a shadow whole because a shadow is a stack of layers, and one
/// retuned token has to change everywhere it is read; GPUI wants them one at a
/// time, so the split lives here instead of at every call site. Unsupported
/// layers fail explicitly rather than disappearing from a partially drawn shadow.
pub(crate) fn shadow(theme: YueTheme, name: impl TokenSource) -> Vec<gpui_kit::BoxShadow> {
    let value = token(theme, name).value().trim().to_owned();
    if value == "none" {
        return Vec::new();
    }
    let layers = split_top_level(&value, ',');
    let parsed: Vec<_> = layers
        .into_iter()
        .map(|layer| {
            parse_shadow_layer(layer).unwrap_or_else(|| {
                panic!("Yue token {name} has an unsupported shadow layer: {layer}")
            })
        })
        .collect();
    assert!(
        !parsed.is_empty(),
        "Yue token {name} is not a box-shadow list"
    );
    parsed
}

fn parse_shadow_layer(layer: &str) -> Option<gpui_kit::BoxShadow> {
    let mut lengths: Vec<f32> = Vec::new();
    let mut color: Option<gpui_kit::Hsla> = None;
    for part in split_top_level(layer, ' ') {
        if part == "inset" {
            return None;
        }
        if let Some(parsed) = parse_shadow_color(part) {
            color = Some(parsed);
            continue;
        }
        lengths.push(
            part.trim()
                .strip_suffix("px")
                .or(
                    if part
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
                    {
                        Some(part)
                    } else {
                        None
                    },
                )?
                .trim()
                .parse()
                .ok()?,
        );
    }
    // CSS needs both offsets; blur and spread default to zero.
    if !(2..=4).contains(&lengths.len()) || lengths.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut lengths = lengths.into_iter();
    let first = lengths.next().unwrap_or(0.0);
    let second = lengths.next().unwrap_or(0.0);
    let third = lengths.next().unwrap_or(0.0);
    let fourth = lengths.next().unwrap_or(0.0);
    Some(
        gpui_kit::BoxShadow::new(px(first), px(second), color?)
            .blur_radius(px(third))
            .spread_radius(px(fourth)),
    )
}

/// Split on a separator that is not inside brackets, so a colour's own commas
/// and spaces stay in one part.
fn split_top_level(value: &str, separator: char) -> Vec<&str> {
    let bytes = value.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ if depth == 0 && *byte == separator as u8 => {
                push_part(&mut parts, &value[start..index]);
                start = index + separator.len_utf8();
            }
            _ => {}
        }
    }
    push_part(&mut parts, &value[start..]);
    parts
}

fn push_part<'a>(parts: &mut Vec<&'a str>, value: &'a str) {
    let value = value.trim();
    if !value.is_empty() {
        parts.push(value);
    }
}

fn parse_shadow_color(value: &str) -> Option<gpui_kit::Hsla> {
    if value == "transparent" {
        return Some(gpui_kit::black().opacity(0.0));
    }
    if let Some(hex) = value.strip_prefix('#') {
        let channels = match hex.len() {
            3 => hex
                .chars()
                .map(|channel| format!("{channel}{channel}"))
                .collect::<String>(),
            6 => hex.to_owned(),
            8 => hex[..6].to_owned(),
            _ => return None,
        };
        let colour = u32::from_str_radix(&channels, 16).ok()?;
        let alpha = match hex.len() {
            8 => u32::from_str_radix(&hex[6..], 16).ok()? as f32 / 255.0,
            _ => 1.0,
        };
        return Some(gpui_kit::Hsla::from(gpui_kit::rgb(colour)).opacity(alpha));
    }
    let inside = value
        .strip_prefix("rgb(")
        .or_else(|| value.strip_prefix("rgba("))
        .and_then(|value| value.strip_suffix(')'))?;
    // `rgb(0 0 0/.16)` and `rgba(0, 0, 0, 0.16)` are the two forms the tokens
    // use: the first keeps its alpha after a slash, the second in a fourth
    // channel that is already a fraction of one.
    let (channels, alpha) = match inside.split_once('/') {
        Some((channels, alpha)) => (channels, Some(alpha.trim())),
        None => (inside, None),
    };
    let channels: Vec<f32> = split_top_level(channels, ',')
        .into_iter()
        .flat_map(|part| split_top_level(part, ' '))
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<_>>()?;
    // Three channels with an optional slash alpha, or the legacy four.
    let alpha = match (channels.len(), alpha) {
        (3, Some(alpha)) => parse_alpha(alpha)?,
        (3, None) => 1.0,
        (4, None) => channels[3],
        _ => return None,
    };
    if !alpha.is_finite() {
        return None;
    }
    let alpha = alpha.clamp(0.0, 1.0);
    let channel = |value: f32| value.clamp(0.0, 255.0).round() as u32;
    let colour = gpui_kit::rgb(
        (channel(channels[0]) << 16) | (channel(channels[1]) << 8) | channel(channels[2]),
    );
    Some(gpui_kit::Hsla::from(colour).opacity(alpha))
}

fn parse_alpha(value: &str) -> Option<f32> {
    if let Some(percent) = value.strip_suffix('%') {
        return Some(percent.trim().parse::<f32>().ok()? / 100.0);
    }
    value.parse::<f32>().ok()
}

/// A millisecond duration, including the `calc(200ms*0.5)` form a retuned
/// motion ladder can produce.
pub(crate) fn duration(theme: YueTheme, name: impl TokenSource) -> Duration {
    let from_millis = |millis: f64| {
        if !(millis.is_finite() && millis >= 0.0) {
            panic!("Yue token {name} is not a non-negative millisecond duration");
        }
        Duration::from_secs_f64(millis / 1000.0)
    };
    if let TokenValue::Milliseconds(value) = token(theme, name).typed() {
        return from_millis(value as f64);
    }
    let value = token(theme, name).value().trim();
    let value = value
        .strip_prefix("calc(")
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(value);
    if let Some((left, right)) = value.split_once('*') {
        let millis = left
            .trim()
            .strip_suffix("ms")
            .and_then(|value| value.trim().parse::<f64>().ok());
        let factor = right.trim().parse::<f64>().ok();
        if let (Some(millis), Some(factor)) = (millis, factor) {
            return from_millis(millis * factor);
        }
    }
    let millis = value
        .strip_suffix("ms")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or_else(|| panic!("Yue token {name} is not a millisecond duration"));
    from_millis(millis)
}

/// A `cubic-bezier()` token read as its four control points.
///
/// Split from [`easing`] because GPUI's own `Easing::CubicBezier` solves the same
/// curve, so a transition that can hand it the raw numbers should not pay for a
/// second solver. `linear` is the straight curve `(0, 0, 1, 1)`.
pub(crate) fn cubic_bezier(theme: YueTheme, name: impl TokenSource) -> (f32, f32, f32, f32) {
    if let TokenValue::CubicBezier([x1, y1, x2, y2]) = token(theme, name).typed() {
        return (x1, y1, x2, y2);
    }
    let value = token(theme, name).value().trim();
    if value == "linear" {
        return (0.0, 0.0, 1.0, 1.0);
    }
    value
        .strip_prefix("cubic-bezier(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| {
            let values: Vec<_> = value
                .split(',')
                .map(|part| part.trim().parse::<f32>().ok())
                .collect::<Option<Vec<_>>>()?;
            (values.len() == 4).then_some((values[0], values[1], values[2], values[3]))
        })
        .unwrap_or_else(|| panic!("Yue token {name} is not a cubic-bezier easing"))
}

/// A `linear()` token read as its evenly spaced outputs.
///
/// Only the evenly spaced form the spring token uses is read: a stop with its
/// own input percentage would be silently misplaced, so it panics instead.
pub(crate) fn linear_stops(theme: YueTheme, name: impl TokenSource) -> Vec<f32> {
    token(theme, name)
        .value()
        .trim()
        .strip_prefix("linear(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| {
            value
                .split(',')
                .map(|part| part.trim().parse::<f32>().ok())
                .collect::<Option<Vec<_>>>()
        })
        .filter(|stops| stops.len() >= 2)
        .unwrap_or_else(|| panic!("Yue token {name} is not an evenly spaced linear() easing"))
}

/// A `cubic-bezier()` curve as the progress remapper a transition wants.
///
/// GPUI drives a transition by time, so the curve is read the way a browser
/// reads it: solve for the parameter whose x equals the elapsed fraction, then
/// report that parameter's y.
pub(crate) fn easing(theme: YueTheme, name: impl TokenSource) -> Box<dyn Fn(f32) -> f32> {
    let (x1, y1, x2, y2) = cubic_bezier(theme, name);
    Box::new(move |time| {
        let sample_x = |t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t
        };
        let sample_y = |t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t
        };
        let derivative = |t: f32| {
            3.0 * (1.0 - t) * (1.0 - t) * x1
                + 6.0 * (1.0 - t) * t * (x2 - x1)
                + 3.0 * t * t * (1.0 - x2)
        };
        let mut t = time;
        for _ in 0..8 {
            let slope = derivative(t);
            if slope.abs() < 1e-6 {
                break;
            }
            t = (t - (sample_x(t) - time) / slope).clamp(0.0, 1.0);
        }
        sample_y(t)
    })
}

/// Mix two token colours the way a CSS transition does: in premultiplied alpha.
///
/// Each colour is weighted by its own alpha before the channels move, so a
/// `transparent` end contributes no colour of its own and a fade from nothing
/// to a tint stays that tint throughout. Mixing the straight channels instead
/// would drag the middle of the ramp towards transparent's black, which shows
/// as a dip on a dark surface. Between two opaque colours the two agree.
pub(crate) fn mix_rgba(from: gpui_kit::Rgba, to: gpui_kit::Rgba, progress: f32) -> gpui_kit::Rgba {
    let alpha = from.a + (to.a - from.a) * progress;
    if alpha <= f32::EPSILON {
        return gpui_kit::Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
    }
    let channel = |from_c: f32, to_c: f32| {
        (from_c * from.a + (to_c * to.a - from_c * from.a) * progress) / alpha
    };
    gpui_kit::Rgba {
        r: channel(from.r, to.r),
        g: channel(from.g, to.g),
        b: channel(from.b, to.b),
        a: alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cubic_bezier, easing, mix_rgba, parse_shadow_color, parse_shadow_layer, pixels, rgba,
        shadow,
    };
    use crate::{Accent, ThemeMode, YueTheme};
    use gpui_kit::base::Easing;
    use gpui_kit::px;

    #[test]
    fn unsupported_shadow_layers_are_not_silently_approximated() {
        for value in [
            "inset 0px 1px 2px #000",
            "0px 1em 2px #000",
            "0 1 2 3 4 #000",
        ] {
            assert!(parse_shadow_layer(value).is_none(), "{value}");
        }
    }

    fn colour(value: gpui_kit::Hsla) -> (f32, f32, f32, f32) {
        let value = value.to_rgb();
        (value.r, value.g, value.b, value.a)
    }

    /// The panel shadow the popover component token carries.
    #[test]
    fn a_popover_shadow_reads_as_one_layer() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let layers = shadow(YueTheme::new(mode, Accent::Azure), "--popover-shadow");
            assert_eq!(layers.len(), 1, "{mode:?} popover shadow");
            let layer = &layers[0];
            assert_eq!(layer.offset.x, px(0.0));
            assert_eq!(layer.offset.y, px(12.0));
            assert_eq!(layer.blur_radius, px(36.0));
            assert_eq!(layer.spread_radius, px(0.0));
            assert!(!layer.inset);
            // Dark keeps more of the black, because a shadow on a dark surface
            // has to be darker to read as one.
            let expected = if mode == ThemeMode::Light { 0.16 } else { 0.38 };
            assert_eq!(colour(layer.color), (0.0, 0.0, 0.0, expected));
        }
    }

    #[test]
    fn a_shadow_layer_fills_the_lengths_css_defaults() {
        let layer = parse_shadow_layer("0 12px 36px rgb(0 0 0/.16)").unwrap();
        assert_eq!(layer.blur_radius, px(36.0));
        let tight = parse_shadow_layer("0 1px 2px #000").unwrap();
        assert_eq!(tight.offset.y, px(1.0));
        assert_eq!(tight.blur_radius, px(2.0));
        assert_eq!(tight.spread_radius, px(0.0));
        // CSS requires both offsets, so a lone length is rejected rather than
        // guessed at.
        assert!(parse_shadow_layer("6px rgb(0 0 0/.2)").is_none());
        assert!(parse_shadow_layer("0 1px 2px 3px 4px #000").is_none());
    }

    #[test]
    fn a_shadow_colour_accepts_the_forms_the_tokens_use() {
        assert_eq!(colour(parse_shadow_color("transparent").unwrap()).3, 0.0);
        assert_eq!(
            colour(parse_shadow_color("#eee").unwrap()),
            (238.0 / 255.0, 238.0 / 255.0, 238.0 / 255.0, 1.0)
        );
        let slash = colour(parse_shadow_color("rgb(0 0 0/.25)").unwrap());
        let commas = colour(parse_shadow_color("rgba(0, 0, 0, 0.25)").unwrap());
        assert_eq!(slash, commas);
        assert_eq!(slash.3, 0.25);
        assert_eq!(colour(parse_shadow_color("rgb(0 0 0/50%)").unwrap()).3, 0.5);
        assert!(parse_shadow_color("12px").is_none());
        // A fourth channel and a slash alpha cannot both hold, nor can five
        // channels; alpha out of range is clamped as CSS does.
        assert!(parse_shadow_color("rgb(0 0 0 1/.5)").is_none());
        assert!(parse_shadow_color("rgba(0, 0, 0, 0.5, 1)").is_none());
        assert_eq!(
            colour(parse_shadow_color("rgb(0 0 0/150%)").unwrap()).3,
            1.0
        );
    }

    /// The popover's own curve, read both ways: as the four numbers GPUI's
    /// `Easing::CubicBezier` wants, and as the remapped progress the closure
    /// produces. The two must agree at every step, or the same token drives two
    /// different animations.
    #[test]
    fn a_cubic_bezier_reads_as_numbers_and_as_a_curve() {
        let theme = YueTheme::new(ThemeMode::Light, Accent::Azure);
        let (x1, y1, x2, y2) = cubic_bezier(theme, crate::theme::keys::POPOVER_EASE);
        assert_eq!((x1, y1, x2, y2), (0.2, 0.0, 0.38, 0.9));
        let curve = easing(theme, crate::theme::keys::POPOVER_EASE);
        let policy = Easing::CubicBezier { x1, y1, x2, y2 };
        assert_eq!((curve(0.0), curve(1.0)), (0.0, 1.0));
        for step in 1..10 {
            let time = step as f32 / 10.0;
            assert!(
                (curve(time) - policy.sample(time)).abs() < 1e-4,
                "popover ease at {time}: closure {} vs GPUI {}",
                curve(time),
                policy.sample(time)
            );
        }
        // The (.38, .9) end front-loads the travel, so half time is already most of
        // the way open — the shape an entrance ramp wants.
        assert!(curve(0.5) > 0.5, "popover ease at half time");
    }

    /// A ghost hover fades in from `transparent`: the tint must stay the tint
    /// all the way, not pass through a darker grey on a dark surface.
    #[test]
    fn a_fade_from_transparent_keeps_its_colour() {
        let theme = YueTheme::new(ThemeMode::Dark, Accent::Azure);
        let from = rgba(theme, crate::theme::keys::BUTTON_GHOST_BACKGROUND);
        let to = rgba(theme, crate::theme::keys::BUTTON_GHOST_BACKGROUND_HOVER);
        assert_eq!(from.a, 0.0);
        for progress in [0.25, 0.5, 0.75] {
            let mixed = mix_rgba(from, to, progress);
            assert!((mixed.r - to.r).abs() < 1e-6, "{progress}: {mixed:?}");
            assert!(
                (mixed.a - to.a * progress).abs() < 1e-6,
                "{progress}: {mixed:?}"
            );
        }
    }

    /// Between two opaque colours premultiplying changes nothing.
    #[test]
    fn opaque_colours_mix_along_the_straight_line() {
        let from = gpui_kit::rgb(0x000000);
        let to = gpui_kit::rgb(0xffffff);
        let mixed = mix_rgba(from, to, 0.25);
        assert!((mixed.r - 0.25).abs() < 1e-6 && mixed.a == 1.0, "{mixed:?}");
        assert_eq!(mix_rgba(gpui_kit::rgba(0), gpui_kit::rgba(0), 0.5).a, 0.0);
    }

    #[test]
    fn menu_radius_expression_and_select_metrics_are_generated_as_lengths() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            for accent in [Accent::Azure, Accent::Neutral] {
                let theme = YueTheme::new(mode, accent);
                assert_eq!(pixels(theme, "--menu-item-border-radius"), 4.);
                assert_eq!(pixels(theme, "--select-width"), 240.);
                assert_eq!(pixels(theme, "--select-option-height-md"), 32.);
            }
        }
    }
}
