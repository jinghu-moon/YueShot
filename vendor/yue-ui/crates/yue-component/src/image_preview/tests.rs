use super::{geometry::*, *};
fn vp() -> Viewport {
    Viewport {
        origin: [20., 30.],
        size: [600., 440.],
        scale: 1.5,
        inset: 12.,
    }
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "{a} != {b}");
}
#[test]
fn viewport_uses_symmetric_insets_and_the_full_viewport_center() {
    let v = vp();
    assert_eq!(v.safe_size(), [576., 416.]);
    assert_eq!(v.midpoint(), [320., 250.]);
    let image = [600., 1200.];
    let c = v.fit(image, ImagePreviewFit::FitAll);
    close(c.zoom(image), 416. * v.scale / image[1]);
    let o = c.top_left(image, v);
    let size = c.display_size(image, v.scale);
    close(o[0] + size[0] / 2., v.midpoint()[0]);
    close(o[1] + size[1] / 2., v.midpoint()[1]);
}
#[test]
fn pixel_grid_is_image_anchored_visible_and_bounded_at_fractional_dpi() {
    for scale in [1., 1.25, 1.5, 2.] {
        let v = Viewport { scale, ..vp() };
        let image = [100000., 500000.];
        let c = Camera {
            center: [0.734, 0.812],
            width: image[0] * 4.,
        };
        let grid = PixelGrid::new(c, image, v).unwrap();
        for axis in 0..2 {
            close(grid.step[axis], 4. / scale);
            for index in grid.indices[axis].clone() {
                let line = grid.origin[axis] + index as f32 * grid.step[axis];
                assert!(line >= grid.clip[axis] - 0.1 && line <= grid.clip[axis + 2] + 0.1);
            }
            assert!(grid.indices[axis].clone().count() <= (v.size[axis] * scale / 4.) as usize + 2);
        }
        assert!(
            PixelGrid::new(
                Camera {
                    width: image[0] * 3.99,
                    ..c
                },
                image,
                v
            )
            .is_none()
        );
    }
}
#[test]
fn physical_actual_size_accounts_for_dpi() {
    let c = Camera {
        center: [0.5, 0.5],
        width: 1440.,
    };
    assert_eq!(c.display_size([1440., 900.], 1.5), [960., 600.]);
    close(c.zoom([1440., 900.]), 1.);
}

#[test]
fn settled_integer_zoom_snaps_by_at_most_half_a_device_pixel() {
    for scale in [1., 1.25, 1.5, 2.] {
        let v = Viewport {
            origin: [20.125, 30.375],
            size: [601.5, 401.25],
            scale,
            ..vp()
        };
        let image = [641., 403.];
        for zoom in [1., 2., 4., 8.] {
            let original = Camera {
                center: [0.371, 0.617],
                width: image[0] * zoom,
            };
            let aligned = original.pixel_aligned(image, v);
            let a = original.top_left(image, v);
            let b = aligned.top_left(image, v);
            close(aligned.width, original.width);
            for axis in 0..2 {
                close(b[axis] * scale, (b[axis] * scale).round());
                assert!((b[axis] - a[axis]).abs() * scale <= 0.501);
            }
            assert_eq!(
                aligned.pixel_aligned(image, v),
                aligned,
                "no accumulating drift"
            );
        }
        let fractional = Camera {
            center: [0.371, 0.617],
            width: image[0] * 1.25,
        };
        assert_eq!(fractional.pixel_aligned(image, v), fractional);
    }
}
#[test]
fn fit_uses_safe_area_and_does_not_upscale_small_images() {
    let v = vp();
    let image = [1440., 900.];
    let c = v.fit(image, ImagePreviewFit::FitAll);
    let size = c.display_size(image, v.scale);
    assert!(size[0] <= v.safe_size()[0] + 0.001 && size[1] <= v.safe_size()[1] + 0.001);
    close(v.fit_zoom([10., 10.], ImagePreviewFit::FitAll), 1.);
}
#[test]
fn extremely_long_images_can_fit_below_five_percent() {
    let v = vp();
    let image = [4000., 400000.];
    let z = v.fit_zoom(image, ImagePreviewFit::FitAll);
    assert!(z > 0. && z < 0.05);
    close(v.zoom_limits(image)[0], z);
}
#[test]
fn pointer_zoom_keeps_image_point_anchored() {
    let v = vp();
    let image = [1440., 900.];
    let c = Camera {
        center: [0.48, 0.54],
        width: 2880.,
    };
    let anchor = [180., 160.];
    let q = c.image_point(anchor, image, v);
    let next = c.anchored(3600., anchor, image, v);
    let r = next.image_point(anchor, image, v);
    close(q[0], r[0]);
    close(q[1], r[1]);
}
#[test]
fn reference_resolution_changes_percent_without_moving_rectangle() {
    let v = vp();
    let c = Camera {
        center: [0.35, 0.62],
        width: 1440.,
    };
    assert_eq!(
        c.display_size([1440., 900.], v.scale),
        c.display_size([720., 450.], v.scale)
    );
    assert_eq!(c.top_left([1440., 900.], v), c.top_left([720., 450.], v));
    close(c.zoom([1440., 900.]), 1.);
    close(c.zoom([720., 450.]), 2.);
}
#[test]
fn pan_keeps_a_visible_strip_for_fitted_large_and_tiny_images() {
    let v = vp();
    for image in [[1440., 900.], [10., 10.], [1000., 90000.]] {
        let fitted = v.fit(image, ImagePreviewFit::FitAll);
        let moved = fitted.pan([36., 24.], image, v);
        let start = fitted.top_left(image, v);
        let end = moved.top_left(image, v);
        close(end[0] - start[0], 36.);
        close(end[1] - start[1], 24.);
        for width in [fitted.width, image[0] * 2.] {
            for delta in [[100000., -100000.], [-100000., 100000.]] {
                let c = Camera { width, ..fitted }.pan(delta, image, v);
                let origin = c.top_left(image, v);
                let size = c.display_size(image, v.scale);
                let safe = v.safe_size();
                for axis in 0..2 {
                    let min = v.origin[axis] + v.inset;
                    let max = min + safe[axis];
                    let overlap = (origin[axis] + size[axis]).min(max) - origin[axis].max(min);
                    let visible = (v.inset * 2.).max(1.).min(size[axis]).min(safe[axis]);
                    let roundoff = size[axis] * f32::EPSILON * 2. + 0.001;
                    assert!(
                        overlap + roundoff >= visible,
                        "{image:?}/{delta:?}/{axis}: {overlap} < {visible}"
                    );
                }
            }
        }
    }
}

#[test]
fn checker_uses_one_adaptive_side_and_never_crops_a_tile() {
    for size in [
        [600., 440.],
        [763.5, 337.25],
        [11., 9.],
        [1., 1.],
        [1900., 900.],
    ] {
        for rem in [16., 20., 24.] {
            for scale in [1., 1.25, 1.5, 2.] {
                let radius = 6. * rem / 16.;
                let grid = CheckerGrid::new(size, radius, 12. * rem / 16., scale);
                let Some(grid) = grid else {
                    assert!(size[0].min(size[1]) <= radius * 2.);
                    continue;
                };
                assert!(grid.side > 0. && grid.columns > 0 && grid.rows > 0);
                assert!(grid.columns <= 256 && grid.rows <= 256);
                for (axis, count) in [grid.columns, grid.rows].into_iter().enumerate() {
                    assert!(grid.origin[axis] >= radius - 0.001);
                    let end = grid.origin[axis] + grid.side * count as f32;
                    assert!(end <= size[axis] - radius + 0.001);
                    close(grid.origin[axis], size[axis] - end);
                }
            }
        }
    }
    let a = CheckerGrid::new([80., 50.], 6., 12., 2.).unwrap();
    let b = CheckerGrid::new([84., 54.], 6., 12., 2.).unwrap();
    assert_ne!(
        a.side, b.side,
        "side adapts to the viewport, not a clipped fixed-size tail"
    );
    assert!(CheckerGrid::new([0., 1.], 0., 12., 1.).is_none());
}
#[test]
fn fit_width_starts_at_top_of_long_image() {
    let v = vp();
    let image = [1000., 9000.];
    let c = v.fit(image, ImagePreviewFit::FitWidth);
    close(c.top_left(image, v)[1], v.origin[1] + v.inset);
}
#[test]
fn rounding_is_allowed_but_crops_are_not_aligned() {
    assert!(aligned([1440., 900.], [720., 450.]));
    assert!(aligned([1440., 900.], [721., 450.]));
    assert!(!aligned([1440., 900.], [720., 720.]));
}
#[test]
fn split_snaps_only_near_edges() {
    close(snap_split(0.011), 0.);
    close(snap_split(0.989), 1.);
    close(snap_split(0.5), 0.5);
}
#[test]
fn log_width_interpolation_has_exact_endpoints_and_positive_scale() {
    let a = Camera {
        center: [0.2, 0.3],
        width: 100.,
    };
    let b = Camera {
        center: [0.7, 0.9],
        width: 6400.,
    };
    close(a.interpolate(b, 0.).width, a.width);
    close(a.interpolate(b, 1.).width, b.width);
    close(a.interpolate(b, 0.5).width, 800.);
}
#[test]
fn preview_motion_tokens_are_available_in_all_profiles() {
    for mode in [crate::ThemeMode::Light, crate::ThemeMode::Dark] {
        let theme = crate::YueTheme::new(mode, crate::Accent::Azure);
        close(
            crate::tokens::duration(theme, "--image-preview-view-duration").as_secs_f32(),
            0.260,
        );
        close(
            crate::tokens::duration(theme, "--image-preview-compare-duration").as_secs_f32(),
            0.180,
        );
        for name in [
            "--image-preview-checker-dark-a",
            "--image-preview-checker-light-b",
            "--image-preview-grip-background",
        ] {
            let _ = crate::tokens::rgba(theme, name);
        }
    }
}

#[test]
fn inset_images_keep_square_corners_while_viewport_edges_are_rounded() {
    use gpui_kit::{Bounds, point, px, size};
    let bounds = Bounds::new(point(px(10.), px(20.)), size(px(600.), px(400.)));
    let inset = Bounds::new(point(px(22.), px(32.)), size(px(576.), px(376.)));
    let radii = super::paint::image_corners(bounds, inset, px(6.));
    assert_eq!(radii, gpui_kit::Corners::all(px(0.)));
    assert_eq!(
        super::paint::image_corners(bounds, bounds, px(6.)),
        gpui_kit::Corners::all(px(6.))
    );
}
