use gpui_kit::base::Placement;
use gpui_kit::{Bounds, PathBuilder, PathStyle, Pixels, Point, Rgba, Window, point, px};
use lyon::tessellation::{FillOptions, LineCap, LineJoin, StrokeOptions};

/// Arrow geometry in logical pixels. The caller resolves all product tokens.
#[derive(Clone, Copy)]
pub(crate) struct ArrowSpec {
    half_base: Pixels,
    depth: Pixels,
    inset: Pixels,
    radii: [Pixels; 4], // top-left, top-right, bottom-right, bottom-left
    border: Pixels,
}

#[derive(Clone, Copy)]
pub(crate) struct ArrowStyle {
    spec: ArrowSpec,
    fill: Rgba,
    stroke: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ArrowGeometry {
    outline: [Point<Pixels>; 3],
    fill: [Point<Pixels>; 4],
    bounds: Bounds<Pixels>,
}

impl ArrowSpec {
    pub(crate) fn new(base: Pixels, depth: Pixels) -> Self {
        Self {
            half_base: base / 2.,
            depth,
            inset: px(0.),
            radii: [px(0.); 4],
            border: px(0.),
        }
    }
    pub(crate) fn edge_inset(mut self, inset: Pixels) -> Self {
        self.inset = inset;
        self
    }
    pub(crate) fn corner_radii(mut self, radii: [Pixels; 4]) -> Self {
        self.radii = radii;
        self
    }
    pub(crate) fn border_width(mut self, width: Pixels) -> Self {
        self.border = width;
        self
    }
    pub(crate) fn style(self, fill: Rgba, stroke: Rgba) -> ArrowStyle {
        ArrowStyle {
            spec: self,
            fill,
            stroke,
        }
    }
    pub(super) fn depth(self) -> Pixels {
        self.depth
    }

    pub(super) fn resolve(
        self,
        surface: Bounds<Pixels>,
        target: Point<Pixels>,
        side: Placement,
    ) -> Option<ArrowGeometry> {
        if [self.half_base, self.depth, self.inset, self.border]
            .into_iter()
            .chain(self.radii)
            .any(|v| !f32::from(v).is_finite() || v < px(0.))
        {
            return None;
        }
        let (origin, length, aim, start_radius, end_radius) = match side {
            Placement::Bottom => (
                surface.left(),
                surface.size.width,
                target.x,
                self.radii[0],
                self.radii[1],
            ),
            Placement::Top => (
                surface.left(),
                surface.size.width,
                target.x,
                self.radii[3],
                self.radii[2],
            ),
            Placement::Right => (
                surface.top(),
                surface.size.height,
                target.y,
                self.radii[0],
                self.radii[3],
            ),
            Placement::Left => (
                surface.top(),
                surface.size.height,
                target.y,
                self.radii[1],
                self.radii[2],
            ),
        };
        let lower = self
            .inset
            .max(start_radius + self.half_base + self.border / 2.);
        let upper = length
            - self
                .inset
                .max(end_radius + self.half_base + self.border / 2.);
        if upper < lower {
            return None;
        }
        let along = origin + (aim - origin).clamp(lower, upper);
        let (centre, tangent, outward) = match side {
            Placement::Bottom => (
                point(along, surface.top()),
                point(self.half_base, px(0.)),
                point(px(0.), -self.depth),
            ),
            Placement::Top => (
                point(along, surface.bottom()),
                point(self.half_base, px(0.)),
                point(px(0.), self.depth),
            ),
            Placement::Right => (
                point(surface.left(), along),
                point(px(0.), self.half_base),
                point(-self.depth, px(0.)),
            ),
            Placement::Left => (
                point(surface.right(), along),
                point(px(0.), self.half_base),
                point(self.depth, px(0.)),
            ),
        };
        let outline = [centre - tangent, centre + outward, centre + tangent];
        // Fill through the existing surface border; stroke only exposed edges.
        let fill = [centre - outward, outline[0], outline[1], outline[2]];
        let mut bounds = Bounds::new(outline[0], Default::default());
        for p in outline {
            bounds = super::geometry::union(bounds, Bounds::new(p, Default::default()));
        }
        let stroke = self.border / 2.;
        let mut outsets = gpui_kit::Edges::all(stroke);
        // The exposed tip is a miter join: half the stroke width alone does
        // not enclose it. Lyon bevels joins past twice its miter-limit ratio;
        // the capped ideal reach conservatively covers that bevel as well.
        let miter_ratio = if self.half_base > px(0.) {
            (self.depth / self.half_base)
                .hypot(1.)
                .min(2. * StrokeOptions::DEFAULT_MITER_LIMIT)
        } else {
            1.
        };
        let tip_stroke = stroke * miter_ratio;
        match side {
            Placement::Bottom => outsets.top = tip_stroke,
            Placement::Top => outsets.bottom = tip_stroke,
            Placement::Right => outsets.left = tip_stroke,
            Placement::Left => outsets.right = tip_stroke,
        }
        bounds = super::geometry::expand(bounds, outsets);
        Some(ArrowGeometry {
            outline,
            fill,
            bounds,
        })
    }
}

impl ArrowStyle {
    pub(super) fn spec(self) -> ArrowSpec {
        self.spec
    }
    pub(super) fn paint(self, geometry: ArrowGeometry, window: &mut Window) {
        let mut fill = PathBuilder::default().with_style(PathStyle::Fill(FillOptions::default()));
        fill.move_to(geometry.fill[0]);
        for corner in &geometry.fill[1..] {
            fill.line_to(*corner);
        }
        fill.close();
        if let Ok(path) = fill.build() {
            window.paint_path(path, self.fill);
        }
        if self.spec.border <= px(0.) {
            return;
        }
        let mut stroke = PathBuilder::default().with_style(PathStyle::Stroke(
            StrokeOptions::default()
                .with_line_width(f32::from(self.spec.border))
                .with_line_cap(LineCap::Butt)
                .with_line_join(LineJoin::Miter),
        ));
        stroke.move_to(geometry.outline[0]);
        for p in &geometry.outline[1..] {
            stroke.line_to(*p);
        }
        if let Ok(path) = stroke.build() {
            window.paint_path(path, self.stroke);
        }
    }
}

impl ArrowGeometry {
    pub(super) fn tip(self) -> Point<Pixels> {
        self.outline[1]
    }
    pub(super) fn bounds(self) -> Bounds<Pixels> {
        self.bounds
    }
    #[cfg(test)]
    #[cfg(test)]
    pub(super) fn outline(self) -> [Point<Pixels>; 3] {
        self.outline
    }
}
