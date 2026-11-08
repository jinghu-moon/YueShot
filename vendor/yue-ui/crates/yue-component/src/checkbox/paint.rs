//! Checkbox mark drawing.

use gpui_kit::{IntoElement, Styled as _, canvas};

/// The indeterminate dash: the reference's `d="M4.5 8h7"`.
pub(super) const DASH_LINE: [(f32, f32); 2] = [(4.5, 8.0), (11.5, 8.0)];

/// One mark of the box, inked to `ink` of its own length.
///
/// The canvas fills the box's content area, which is where the reference places
/// its `position: absolute; inset: 0` SVG, so the 16-unit path scales with the
/// box and the press shrink moves the mark with it.
pub(super) fn mark_layer(
    points: &'static [(f32, f32)],
    ink: f32,
    unit: f32,
    stroke: f32,
    color: gpui_kit::Hsla,
) -> impl IntoElement {
    canvas(
        move |_, _, _| (),
        move |bounds, _, window, _| paint_mark(window, bounds, points, ink, unit, stroke, color),
    )
    .absolute()
    .inset_0()
}

use crate::icon::paint_mark;
