use crate::{YueTheme, theme::keys as k, tokens::rgba};
use gpui_kit::{
    AnyElement, Bounds, IntoElement, ParentElement as _, PathBuilder, PathStyle, Pixels, Rgba,
    Styled as _, Window, canvas, div, fill, point, px, size,
};
use lyon::tessellation::{LineCap, LineJoin, StrokeOptions};

/// Draw a circular stroke inside the allotted square. Fractions are geometric,
/// not product dimensions; 96 subdivisions bound tessellation for a full circle.
pub(super) fn arc(
    b: Bounds<Pixels>,
    stroke: Pixels,
    color: Rgba,
    start: f32,
    sweep: f32,
    w: &mut Window,
) {
    if sweep <= 0. || stroke <= px(0.) {
        return;
    }
    let radius = (b.size.width.min(b.size.height) - stroke) / 2.;
    if radius <= px(0.) {
        return;
    }
    let center = b.center();
    let point_at = |turn: f32| {
        let a = (turn - 0.25) * std::f32::consts::TAU;
        center + point(radius * a.cos(), radius * a.sin())
    };
    let mut path = PathBuilder::default().with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(f32::from(stroke))
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round),
    ));
    path.move_to(point_at(start));
    let segments = (sweep.min(1.) * 96.).ceil().max(2.) as usize;
    for ix in 1..=segments {
        path.line_to(point_at(
            start + sweep.min(1.) * ix as f32 / segments as f32,
        ));
    }
    if sweep >= 1. {
        path.close();
    }
    if let Ok(path) = path.build() {
        w.paint_path(path, color);
    }
}

pub(super) fn line_segment(phase: f32) -> (f32, f32) {
    let center = -0.3 + 1.6 * phase;
    let length = 0.2 + 0.15 * (phase * std::f32::consts::PI).sin();
    (
        (center - length / 2.).clamp(0., 1.),
        (center + length / 2.).clamp(0., 1.),
    )
}

pub(super) fn linear(
    b: Bounds<Pixels>,
    start: f32,
    end: f32,
    radius: Pixels,
    color: Rgba,
    w: &mut Window,
) {
    let width = b.size.width * (end - start).max(0.);
    if width > px(0.) {
        w.paint_quad(
            fill(
                Bounds::new(
                    b.origin + point(b.size.width * start, px(0.)),
                    size(width, b.size.height),
                ),
                color,
            )
            .corner_radii(radius.min(width / 2.).min(b.size.height / 2.)),
        );
    }
}

pub(crate) fn spinner_glyph(
    theme: YueTheme,
    diameter: gpui_kit::Rems,
    stroke: gpui_kit::Rems,
    color: Rgba,
    period: std::time::Duration,
    animated: bool,
) -> AnyElement {
    div()
        .relative()
        .size(diameter)
        .flex_shrink_0()
        .child(
            canvas(
                |_, _, _| (),
                move |b, _, w, cx| {
                    let phase = gpui_kit::base::loading_phase(b, period, animated, w, cx);
                    let stroke = stroke.to_pixels(w.rem_size());
                    arc(b, stroke, rgba(theme, k::SPINNER_TRACK), 0., 1., w);
                    arc(b, stroke, color, phase, 0.72, w);
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_segment_is_bounded_and_exits_at_both_cycle_ends() {
        for ix in 0..=100 {
            let (a, b) = line_segment(ix as f32 / 100.);
            assert!((0. ..=1.).contains(&a) && a <= b && b <= 1.);
        }
        assert_eq!(line_segment(0.), (0., 0.));
        assert_eq!(line_segment(1.), (1., 1.));
    }
}
