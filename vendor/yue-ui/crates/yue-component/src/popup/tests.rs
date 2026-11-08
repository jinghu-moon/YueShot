use super::{ArrowSpec, geometry};
use gpui_kit::base::Placement;
use gpui_kit::{Bounds, Edges, Pixels, point, px, size};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

#[test]
fn arrow_projects_to_each_resolved_edge_and_preserves_the_menu_diamond() {
    let radius = 8. / 2. * 2.0_f32.sqrt();
    let spec = ArrowSpec::new(px(radius * 2.), px(radius)).edge_inset(px(16.));
    let surface = rect(100., 100., 240., 180.);
    let target = point(px(200.), px(180.));
    for (side, tip) in [
        (Placement::Bottom, point(px(200.), px(100. - radius))),
        (Placement::Top, point(px(200.), px(280. + radius))),
        (Placement::Right, point(px(100. - radius), px(180.))),
        (Placement::Left, point(px(340. + radius), px(180.))),
    ] {
        let arrow = spec.resolve(surface, target, side).unwrap();
        assert_eq!(arrow.outline()[1], tip);
        assert!(geometry::contains(
            geometry::union(surface, arrow.bounds()),
            arrow.bounds()
        ));
    }
    let arrow = spec
        .resolve(surface, point(px(0.), px(0.)), Placement::Bottom)
        .unwrap();
    assert_eq!(arrow.outline()[1].x, px(116.));
    let arrow = spec
        .resolve(surface, point(px(1000.), px(0.)), Placement::Bottom)
        .unwrap();
    assert_eq!(arrow.outline()[1].x, px(324.));
}

#[test]
fn arrow_envelope_includes_the_stroked_miter_tip_on_every_side() {
    let surface = rect(100., 100., 80., 60.);
    let target = surface.center();
    let spec = ArrowSpec::new(px(12.), px(6.)).border_width(px(2.));
    let reach = px(6. + 2.0_f32.sqrt());
    for side in [
        Placement::Top,
        Placement::Bottom,
        Placement::Left,
        Placement::Right,
    ] {
        let arrow = spec.resolve(surface, target, side).unwrap();
        let actual = match side {
            Placement::Bottom => surface.top() - arrow.bounds().top(),
            Placement::Top => arrow.bounds().bottom() - surface.bottom(),
            Placement::Right => surface.left() - arrow.bounds().left(),
            Placement::Left => arrow.bounds().right() - surface.right(),
        };
        assert!((actual - reach).abs() < px(0.001));
        assert!(actual > px(7.)); // A half-stroke outset would miss the tip.
    }
}

#[test]
fn an_arrow_without_a_legal_edge_interval_is_rejected() {
    let spec = ArrowSpec::new(px(12.), px(6.))
        .corner_radii([px(8.); 4])
        .border_width(px(1.));
    assert!(
        spec.resolve(
            rect(0., 0., 20., 40.),
            point(px(10.), px(-10.)),
            Placement::Bottom
        )
        .is_none()
    );
    assert!(
        spec.resolve(
            rect(0., 0., 40., 40.),
            point(px(10.), px(-10.)),
            Placement::Bottom
        )
        .is_some()
    );
}

#[test]
fn asymmetric_corners_and_border_reserve_a_safe_arrow_base() {
    let arrow = ArrowSpec::new(px(10.), px(6.))
        .corner_radii([px(12.), px(3.), px(0.), px(0.)])
        .border_width(px(2.))
        .resolve(
            rect(0., 0., 60., 30.),
            point(px(-100.), px(-20.)),
            Placement::Bottom,
        )
        .unwrap();
    assert_eq!(arrow.outline()[1].x, px(18.));
}

#[test]
fn movement_proves_the_whole_sweep_and_the_matching_corner_budget() {
    let viewport = rect(0., 0., 800., 600.);
    let from = rect(20., 20., 80., 30.);
    let to = rect(180., 20., 80., 30.);
    assert!(geometry::translation_allowed(
        from,
        to,
        px(160.),
        &[],
        viewport
    ));
    assert!(!geometry::translation_allowed(
        from,
        to,
        px(159.),
        &[],
        viewport
    ));
    let blocker = rect(120., 25., 20., 20.);
    assert!(!geometry::overlaps(from, blocker) && !geometry::overlaps(to, blocker));
    assert!(!geometry::translation_allowed(
        from,
        to,
        px(200.),
        &[blocker],
        viewport
    ));
    assert!(!geometry::translation_allowed(
        from,
        rect(20., 20., 260., 30.),
        px(160.),
        &[],
        viewport
    ));
    assert!(!geometry::translation_allowed(
        from,
        rect(-1., 20., 80., 30.),
        px(160.),
        &[],
        viewport
    ));
    assert!(!geometry::translation_allowed(
        from,
        to,
        px(f32::NAN),
        &[],
        viewport
    ));
}

#[test]
fn visual_expansion_does_not_change_interaction_bounds() {
    let surface = rect(20., 30., 80., 40.);
    let visual = geometry::expand(
        surface,
        Edges {
            top: px(2.),
            right: px(4.),
            bottom: px(8.),
            left: px(6.),
        },
    );
    assert_eq!(visual, rect(14., 28., 90., 50.));
    assert_eq!(surface, rect(20., 30., 80., 40.));
}

#[cfg(feature = "headless-tests")]
mod interaction;
