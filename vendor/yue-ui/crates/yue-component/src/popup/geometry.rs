use gpui_kit::{Bounds, Edges, Pixels, point, px};

pub(super) fn finite(bounds: Bounds<Pixels>) -> bool {
    [
        bounds.left(),
        bounds.top(),
        bounds.size.width,
        bounds.size.height,
    ]
    .into_iter()
    .all(|v| f32::from(v).is_finite())
        && bounds.size.width >= px(0.)
        && bounds.size.height >= px(0.)
}

pub(crate) fn overlaps(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    a.left() < b.right() && b.left() < a.right() && a.top() < b.bottom() && b.top() < a.bottom()
}

pub(super) fn contains(outer: Bounds<Pixels>, inner: Bounds<Pixels>) -> bool {
    inner.left() >= outer.left()
        && inner.right() <= outer.right()
        && inner.top() >= outer.top()
        && inner.bottom() <= outer.bottom()
}

pub(crate) fn union(a: Bounds<Pixels>, b: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::from_corners(
        point(a.left().min(b.left()), a.top().min(b.top())),
        point(a.right().max(b.right()), a.bottom().max(b.bottom())),
    )
}

pub(crate) fn expand(bounds: Bounds<Pixels>, outsets: Edges<Pixels>) -> Bounds<Pixels> {
    Bounds::from_corners(
        point(bounds.left() - outsets.left, bounds.top() - outsets.top),
        point(
            bounds.right() + outsets.right,
            bounds.bottom() + outsets.bottom,
        ),
    )
}

/// Conservative swept AABB: safe endpoints alone do not prove a safe path.
/// Matching-corner distance also bounds resizing, not just centre translation.
pub(super) fn translation_allowed(
    from: Bounds<Pixels>,
    to: Bounds<Pixels>,
    budget: Pixels,
    protected: &[Bounds<Pixels>],
    viewport: Bounds<Pixels>,
) -> bool {
    if !finite(from) || !finite(to) || !f32::from(budget).is_finite() || budget < px(0.) {
        return false;
    }
    let corners = |bounds: Bounds<Pixels>| {
        [
            bounds.origin,
            point(bounds.right(), bounds.top()),
            bounds.bottom_right(),
            point(bounds.left(), bounds.bottom()),
        ]
    };
    let within_budget = corners(from).into_iter().zip(corners(to)).all(|(a, b)| {
        let dx = f32::from(a.x - b.x);
        let dy = f32::from(a.y - b.y);
        dx.hypot(dy) <= f32::from(budget)
    });
    let sweep = union(from, to);
    within_budget
        && contains(viewport, sweep)
        && protected.iter().all(|region| !overlaps(sweep, *region))
}
