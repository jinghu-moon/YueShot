//! Shared animated mark strokes used by Checkbox, Tag and Dropdown.

use gpui_kit::{Bounds, PathBuilder, PathStyle, Pixels, Window, px};

use lyon::tessellation::{LineCap, LineJoin, StrokeOptions};

/// The reference's `viewBox="0 0 16 16"`: the mark coordinates below are in
/// those units, and a drawn box of `16 - 2 * border` pixels maps onto it.
pub(crate) const MARK_BOX: f32 = 16.0;

/// The check: the reference's `d="M4 8.5l3 3 5-6"`.
pub(crate) const CHECK_LINE: [(f32, f32); 3] = [(4.0, 8.5), (7.0, 11.5), (12.0, 5.5)];

pub(crate) fn paint_mark(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    points: &[(f32, f32)],
    ink: f32,
    unit: f32,
    stroke: f32,
    color: gpui_kit::Hsla,
) {
    if ink <= 0.0 {
        return;
    }
    let centre = bounds.center();
    let place = |point: (f32, f32)| {
        gpui_kit::point(
            centre.x + px((point.0 - MARK_BOX / 2.0) * unit),
            centre.y + px((point.1 - MARK_BOX / 2.0) * unit),
        )
    };
    let mut length = 0.0;
    for pair in points.windows(2) {
        length += ((pair[1].0 - pair[0].0).powi(2) + (pair[1].1 - pair[0].1).powi(2)).sqrt();
    }
    let length = length * unit;
    let mut builder = PathBuilder::default().with_style(PathStyle::Stroke(
        StrokeOptions::default()
            .with_line_width(stroke)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round),
    ));
    builder.move_to(place(points[0]));
    for point in &points[1..] {
        builder.line_to(place(*point));
    }
    // `pathLength="1"` with `stroke-dasharray: 1` and a dash offset running 1 → 0
    // inks the leading `ink` of the stroke, so the mark is written from the end
    // its path starts at, the way the reference draws it.
    let dashes = [px(length * ink.clamp(0.0, 1.0)), px(length)];
    if let Ok(path) = builder.dash_array(&dashes).build() {
        window.paint_path(path, color);
    }
}
