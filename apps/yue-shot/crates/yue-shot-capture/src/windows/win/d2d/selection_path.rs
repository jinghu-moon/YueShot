//! D2D adapter for the shared selection superellipse; other UI panels keep their
//! existing circular corners. The two existing render inputs (rect, radius)
//! completely determine the path; there is no separate smoothing preference.
use super::*;
use ::windows::Win32::Graphics::Direct2D::Common::{
    D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
};
use ::windows::Win32::Graphics::Direct2D::{
    D2D1_GEOMETRY_SIMPLIFICATION_OPTION_LINES, ID2D1Factory,
};

pub(super) fn geometry(
    factory: &ID2D1Factory,
    bounds: Rect,
    radius: i32,
) -> Result<ID2D1Geometry, String> {
    let path = unsafe { factory.CreatePathGeometry() }
        .map_err(|e| super::super::hresult("CreatePathGeometry(selection)", &e))?;
    let sink =
        unsafe { path.Open() }.map_err(|e| super::super::hresult("Open(selection path)", &e))?;
    let r = radius.max(0) as f32;
    // Top-right, bottom-right, bottom-left, top-left, clockwise. Each normalized
    // corner runs (1,0) -> (0,1); rotation puts its endpoints on the straight edges.
    let centers = [
        (bounds.right as f32 - r, bounds.top as f32 + r),
        (bounds.right as f32 - r, bounds.bottom as f32 - r),
        (bounds.left as f32 + r, bounds.bottom as f32 - r),
        (bounds.left as f32 + r, bounds.top as f32 + r),
    ];
    unsafe {
        sink.BeginFigure(
            vector2(bounds.left as f32 + r, bounds.top as f32),
            D2D1_FIGURE_BEGIN_FILLED,
        );
        for (corner, (cx, cy)) in centers.into_iter().enumerate() {
            let transform = |[x, y]: [f32; 2]| {
                let (x, y) = match corner {
                    0 => (y, -x),
                    1 => (x, y),
                    2 => (-y, x),
                    _ => (-x, -y),
                };
                vector2(cx + r * x, cy + r * y)
            };
            sink.AddLine(transform([1., 0.]));
            if radius > 0 {
                for segment in yue_shot_model::geometry::selection_shape::corner_cubics() {
                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: transform(segment[1]),
                        point2: transform(segment[2]),
                        point3: transform(segment[3]),
                    });
                }
            }
        }
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()
            .map_err(|e| super::super::hresult("Close(selection path)", &e))?;
    }
    // D2D's default curve flattening can move a partially covered edge by a
    // substantial fraction of a pixel, despite precise Bezier controls. Freeze
    // a 0.005px polyline once when the cached shape changes, so subsequent fill,
    // blur clips and strokes all consume the same fine boundary.
    let flattened = unsafe { factory.CreatePathGeometry() }
        .map_err(|e| super::super::hresult("CreatePathGeometry(selection flatten)", &e))?;
    let flat_sink = unsafe { flattened.Open() }
        .map_err(|e| super::super::hresult("Open(selection flatten)", &e))?;
    unsafe {
        path.Simplify(
            D2D1_GEOMETRY_SIMPLIFICATION_OPTION_LINES,
            None,
            0.005,
            &flat_sink,
        )
        .map_err(|e| super::super::hresult("Simplify(selection)", &e))?;
        flat_sink
            .Close()
            .map_err(|e| super::super::hresult("Close(selection flatten)", &e))?;
    }
    flattened
        .cast()
        .map_err(|e| super::super::hresult("cast selection geometry", &e))
}
