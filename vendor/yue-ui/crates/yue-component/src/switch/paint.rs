//! Switch glyph paths and loading ring drawing.

use gpui_kit::{
    IntoElement, PathBuilder, PathStyle, Pixels, Point, Styled as _, Window, canvas, px,
};

use lyon::tessellation::{LineCap, LineJoin, StrokeOptions};

/// Side of the square grid the reference icons are drawn on: the demo's SVGs use
/// a 24×24 viewBox, so every glyph coordinate below is in those units.
pub(super) const GLYPH_BOX: f32 = 24.0;

/// `stroke-width: 3` of the demo, in viewBox units. It shrinks with the glyph,
/// the way an element transform scales its whole rendering.
pub(super) const GLYPH_STROKE: f32 = 3.0;

/// The loading ring: `circle r=9` carrying `stroke-dasharray:28 100`, so the ink
/// spans 28 of the 9-unit radius's arc units.
const RING_RADIUS: f32 = 9.0;

const RING_DASH: f32 = 28.0;

/// The two diagonals of ×, in the order the demo inks them. Endpoints follow the
/// demo's own path data.
pub(super) const CROSS_FIRST: [(f32, f32); 2] = [(18.0, 6.0), (6.0, 18.0)];

pub(super) const CROSS_SECOND: [(f32, f32); 2] = [(6.0, 6.0), (18.0, 18.0)];

/// The single polyline of ✓: the demo's `M5 12l5 5l10 -10` grown 1.1× about its
/// own box, whose centre then moves back to the centre of the glyph box. The
/// demo sizes both glyphs by the same box, where × spans 12×12 but ✓ only 15×10,
/// so the cross-fade swapped a tall icon for a short one and the check read as
/// the smaller of the pair. 1.1 takes ✓ to 11 units tall — one short of ×, which
/// is the point at which it still reads as the same icon and not as a bulky one;
/// full parity would need 1.2. Re-derive the endpoints from the demo's path as
/// `x' = 12 + 1.1 * (x - 12.5)`, `y' = 12 + 1.1 * (y - 12)`. The stroke keeps
/// [`GLYPH_STROKE`]: only the geometry grew, not the weight.
pub(super) const CHECK_LINE: [(f32, f32); 3] = [(3.75, 12.0), (9.25, 17.5), (20.25, 6.5)];

/// One stroked sub-path of a thumb glyph: `drawn` is the fraction of the stroke the
/// ink has covered. The reference writes `pathLength="1"` + a dash offset on the
/// checkbox and menu marks only, and just fades these glyphs; Yue draws the switch
/// icon the same way it draws the check, so both components ink with one pen.
#[derive(Clone, Copy)]
pub(super) struct GlyphStroke {
    points: &'static [(f32, f32)],
    drawn: f32,
}

impl GlyphStroke {
    pub(super) fn new(points: &'static [(f32, f32)], drawn: f32) -> Self {
        Self { points, drawn }
    }
}

pub(super) const NO_STROKE: GlyphStroke = GlyphStroke {
    points: &[],
    drawn: 0.0,
};

/// One `<svg class="ico">` container of the demo: a glyph box centred on the
/// thumb, turned by `turns` of a turn, shrunk to `scale` and faded to `alpha`.
/// The alpha goes into the paint, not the element style, because path drawing
/// does not read an element's own opacity.
pub(super) fn glyph_layer(
    size: f32,
    alpha: f32,
    turns: f32,
    scale: f32,
    color: gpui_kit::Hsla,
    strokes: [GlyphStroke; 2],
) -> impl IntoElement {
    canvas(
        move |_, _, _| (),
        move |bounds, _, window, _| {
            let centre = bounds.center();
            for stroke in strokes {
                paint_glyph(window, centre, size, scale, turns, stroke, color, alpha);
            }
        },
    )
    .absolute()
    .inset_0()
}

#[expect(clippy::too_many_arguments, reason = "one glyph's whole paint recipe")]
fn paint_glyph(
    window: &mut Window,
    centre: Point<Pixels>,
    size: f32,
    scale: f32,
    turns: f32,
    stroke: GlyphStroke,
    color: gpui_kit::Hsla,
    alpha: f32,
) {
    if stroke.points.len() < 2 || stroke.drawn <= 0.0 || alpha <= 0.0 {
        return;
    }
    let unit = size / GLYPH_BOX;
    let (sin, cos) = (std::f32::consts::TAU * turns).sin_cos();
    // Offsets from the glyph centre stay in plain f32 until the path is built:
    // `Pixels` keeps its own arithmetic private, and the ink length is a ratio.
    let offset = |point: (f32, f32)| {
        let x = (point.0 - GLYPH_BOX / 2.0) * unit * scale;
        let y = (point.1 - GLYPH_BOX / 2.0) * unit * scale;
        (x * cos - y * sin, x * sin + y * cos)
    };
    let place = |point: (f32, f32)| {
        let (x, y) = offset(point);
        gpui_kit::point(centre.x + px(x), centre.y + px(y))
    };
    let mut length = 0.0;
    for pair in stroke.points.windows(2) {
        let (from, to) = (offset(pair[0]), offset(pair[1]));
        length += (to.0 - from.0).hypot(to.1 - from.1);
    }
    if length <= 0.0 {
        return;
    }
    let mut builder = PathBuilder::default().with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(GLYPH_STROKE * unit * scale)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round),
    ));
    builder.move_to(place(stroke.points[0]));
    for point in &stroke.points[1..] {
        builder.line_to(place(*point));
    }
    // `stroke-dasharray: 1` with `stroke-dashoffset: 1 - drawn` on a normalised
    // path leaves the leading `drawn` of the stroke inked, so the ink grows from
    // the end the path starts at, exactly as the demo draws it.
    let dashes = [px(length * stroke.drawn), px(length)];
    if let Ok(path) = builder.dash_array(&dashes).build() {
        window.paint_path(path, color.opacity(alpha));
    }
}

/// The loading ring: the demo's `circle r=9` with `stroke-dasharray: 28 100`,
/// which inks 28 of its 56-unit circumference. Rotating it is the same as
/// starting that arc later along the circle, so the arc is drawn where it should
/// be instead of transforming an element.
pub(super) fn ring_layer(
    size: f32,
    alpha: f32,
    turns: f32,
    color: gpui_kit::Hsla,
) -> impl IntoElement {
    canvas(
        move |_, _, _| (),
        move |bounds, _, window, _| {
            if alpha <= 0.0 {
                return;
            }
            let centre = bounds.center();
            let unit = size / GLYPH_BOX;
            let radius = RING_RADIUS * unit;
            let at = |angle: f32| {
                gpui_kit::point(
                    centre.x + px(radius * angle.cos()),
                    centre.y + px(radius * angle.sin()),
                )
            };
            // The dashed arc is shorter than half the circle, so the sweep is the
            // small one and `large_arc` stays off.
            let span = RING_DASH / RING_RADIUS;
            let start = std::f32::consts::TAU * turns;
            let mut builder = PathBuilder::default().with_style(PathStyle::Stroke(
                StrokeOptions::default()
                    .with_line_width(GLYPH_STROKE * unit)
                    .with_line_cap(LineCap::Round),
            ));
            builder.move_to(at(start));
            builder.arc_to(
                gpui_kit::point(px(radius), px(radius)),
                px(0.0),
                span > std::f32::consts::PI,
                true,
                at(start + span),
            );
            if let Ok(path) = builder.build() {
                window.paint_path(path, color.opacity(alpha));
            }
        },
    )
    .absolute()
    .inset_0()
}
