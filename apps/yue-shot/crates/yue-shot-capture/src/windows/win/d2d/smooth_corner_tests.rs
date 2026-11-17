use super::*;

#[test]
#[ignore = "L4 real D2D geometry against independent quartic boundaries, no desktop capture"]
fn smooth_corner_d2d_path_matches_analytic_boundaries_in_all_four_corners() {
    let device = super::super::GraphicsDevice::create().unwrap();
    let context = device.create_d2d_context().unwrap();
    let factory = unsafe { context.GetFactory() }.unwrap();
    let rect = Rect::new(-80, -60, 560, 580);
    for radius in [0, 1, 2, 8, 32, 128] {
        let shape = super::super::selection_path::geometry(&factory, rect, radius).unwrap();
        let bounds = unsafe { shape.GetBounds(None) }.unwrap();
        for (actual, expected) in [bounds.left, bounds.top, bounds.right, bounds.bottom]
            .into_iter()
            .zip([-80., -60., 560., 580.])
        {
            assert!(
                (actual - expected).abs() < 0.0001,
                "geometry bound {actual} vs {expected}"
            );
        }
        if radius == 0 {
            assert!(
                unsafe {
                    shape.FillContainsPoint(super::super::vector2(-79.9, -59.9), None, 0.001)
                }
                .unwrap()
                .as_bool()
            );
            continue;
        }
        let r = f64::from(radius);
        for i in 1..512 {
            let y = r * f64::from(i) / 512.;
            let boundary = r * (1. - (1. - (1. - y / r).powi(4)).powf(0.25));
            // Offset along the analytic normal, not horizontally: near the
            // straight edge a horizontal offset can have almost zero distance
            // to the curve, below the geometry flattening tolerance.
            let nx = (1. - boundary / r).powi(3);
            let ny = (1. - y / r).powi(3);
            let length = nx.hypot(ny);
            for (offset, expected) in [(-0.03, false), (0.03, true)] {
                let x = boundary + offset * nx / length;
                let y = y + offset * ny / length;
                for (px, py) in [
                    (f64::from(rect.left) + x, f64::from(rect.top) + y),
                    (f64::from(rect.right) - x, f64::from(rect.top) + y),
                    (f64::from(rect.left) + x, f64::from(rect.bottom) - y),
                    (f64::from(rect.right) - x, f64::from(rect.bottom) - y),
                ] {
                    assert_eq!(
                        unsafe {
                            shape.FillContainsPoint(
                                super::super::vector2(px as f32, py as f32),
                                None,
                                0.001,
                            )
                        }
                        .unwrap()
                        .as_bool(),
                        expected,
                        "r={radius} ({px},{py})"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "L4 real GPU mask versus exported alpha; optional evidence PNGs, no desktop capture"]
fn smooth_corner_rendered_mask_matches_export_and_outline_cache_tracks_radius() {
    let mut errors = Vec::new();
    for requested in [0, 1, 2, 16, 48, 128] {
        let (rendered, stride) = render_masked_frame(requested);
        let (native, _) = render_masked_shape(requested, false, false, true);
        let (width, height) = (280u32, 200u32);
        let mut export = vec![255u8; (width * height * 4) as usize];
        crate::artifact::round_corners(&mut export, width, height, requested as u32);
        let outside = f32::from(pixel_at(&rendered, stride, 10, 10)[0]);
        let mut max_error = 0f32;
        let mut native_error = 0f32;
        let mut worst = (0, 0, 0., 0., 0.);
        for y in 0..height {
            for x in 0..width {
                let alpha = f32::from(export[((y * width + x) * 4 + 3) as usize]) / 255.;
                // The product paints black dimming and accent tint in two
                // source-over passes; partial coverage is not linear between
                // the final inside/outside colors.
                let coverage = 1. - alpha;
                let masked = 200. * (1. - MASK_ALPHA * coverage);
                let tint = MASK_TINT_ALPHA * coverage;
                let expected = masked * (1. - tint) + f32::from(ACCENT_RGB.b) * tint;
                let r = f64::from(requested.min(50));
                let px = f64::from(x.min(width - 1 - x));
                let py = f64::from(y.min(height - 1 - y));
                let mut inside = 64u32;
                if px < r && py < r {
                    inside = 0;
                    for sy in 0..8 {
                        for sx in 0..8 {
                            let dx = r - px - (f64::from(sx) + 0.5) / 8.;
                            let dy = r - py - (f64::from(sy) + 0.5) / 8.;
                            inside += u32::from(dx * dx + dy * dy <= r * r);
                        }
                    }
                }
                let circle_coverage = 1. - (inside * 255 / 64) as f32 / 255.;
                let circle_expected = 200.
                    * (1. - MASK_ALPHA * circle_coverage)
                    * (1. - MASK_TINT_ALPHA * circle_coverage)
                    + f32::from(ACCENT_RGB.b) * MASK_TINT_ALPHA * circle_coverage;
                native_error = native_error.max(
                    (f32::from(pixel_at(&native, stride, x + 60, y + 50)[0]) - circle_expected)
                        .abs(),
                );
                let actual = f32::from(pixel_at(&rendered, stride, x + 60, y + 50)[0]);
                if (actual - expected).abs() > max_error {
                    max_error = (actual - expected).abs();
                    worst = (x, y, alpha, expected, actual);
                }
            }
        }
        println!(
            "SMOOTH_CORNER mask_export_max_channel_error r={requested} error={max_error} outside={outside} native_circle_error={native_error} worst={worst:?}"
        );
        // Compare against this device's native circular geometry rasterizer,
        // measured with the same 8x8 oracle and actual two-pass color blending.
        // D2D coverage is not guaranteed to equal CPU supersampling bit-for-bit.
        // Geometry itself has a separate +/-0.03px analytic boundary test above.
        errors.push((requested, max_error, native_error + 2.));
        if let Ok(directory) = std::env::var("YUESHOT_CORNER_EVIDENCE") {
            write_png(
                &directory,
                &format!("mask-r{requested}.png"),
                stride,
                300,
                rendered,
            );
        }
    }

    assert!(
        errors.iter().all(|(_, error, limit)| error <= limit),
        "mask/export coverage: {errors:?}"
    );

    let device = super::super::GraphicsDevice::create().unwrap();
    let mut renderer = OverlayRenderer::new(std::sync::Arc::new(device), 144).unwrap();
    renderer
        .update_frame(640, 480, &solid_bgra(640, 480, [45, 40, 35, 255]))
        .unwrap();
    renderer.ensure_back_buffer(640, 480).unwrap();
    let target = renderer
        .device()
        .create_render_target_texture(640, 480, yue_media::PixelFormat::Bgra8Unorm)
        .unwrap();
    let context = renderer.device().create_d2d_context().unwrap();
    let bitmap = super::super::super::d3d11::create_bitmap_from_texture(
        &context,
        &target.texture,
        D2D1_BITMAP_OPTIONS_TARGET,
        D2D1_ALPHA_MODE_PREMULTIPLIED,
    )
    .unwrap();
    let mut view = RenderView::new(Rect::new(0, 0, 640, 480));
    view.selection = Rect::new(64, 64, 576, 416);
    view.show_chrome = true;
    view.show_size_label = false;
    view.cursor_visible = false;
    let mut previous = None;
    for radius in [48, 48, 16, 128, 0] {
        view.corner_radius = radius;
        renderer.draw_to(&bitmap, &view).unwrap();
        if radius > 0 {
            let (rect, r, geometry) = renderer.selection_outline.as_ref().unwrap();
            assert_eq!(*r, radius.min(crate::geometry::radius_cap(512, 352)) - 1);
            assert_eq!(*rect, Rect::new(65, 65, 575, 415));
            if let Some((previous_radius, pointer)) = previous
                && previous_radius == radius
            {
                assert_eq!(pointer, ::windows::core::Interface::as_raw(geometry));
            }
            previous = Some((radius, ::windows::core::Interface::as_raw(geometry)));
        }
        if let Ok(directory) = std::env::var("YUESHOT_CORNER_EVIDENCE") {
            write_png(
                &directory,
                &format!("selection-r{radius}.png"),
                640,
                480,
                renderer.device().read_back_bgra(&target.texture).unwrap(),
            );
        }
    }
    renderer.release_target();
    assert!(renderer.selection_outline.is_none());
}

fn write_png(directory: &str, name: &str, width: u32, height: u32, mut pixels: Vec<u8>) {
    for p in pixels.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    let file = std::fs::File::create(std::path::Path::new(directory).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
}

#[test]
#[ignore = "GPU checkerboard drag sequence; own targets only, no desktop capture"]
fn smooth_corner_changing_drag_bounds_keep_blur_in_all_corner_wedges_before_release() {
    let device = super::super::GraphicsDevice::create().unwrap();
    let mut renderer = OverlayRenderer::new(std::sync::Arc::new(device), 96).unwrap();
    let (width, height) = (400, 300);
    let background = checkerboard_bgra(width, height, 4);
    renderer.update_frame(width, height, &background).unwrap();
    renderer.ensure_back_buffer(width, height).unwrap();
    let target = renderer
        .device()
        .create_render_target_texture(width, height, yue_media::PixelFormat::Bgra8Unorm)
        .unwrap();
    let context = renderer.device().create_d2d_context().unwrap();
    let bitmap = super::super::super::d3d11::create_bitmap_from_texture(
        &context,
        &target.texture,
        D2D1_BITMAP_OPTIONS_TARGET,
        D2D1_ALPHA_MODE_PREMULTIPLIED,
    )
    .unwrap();
    let mut view = RenderView::new(Rect::new(0, 0, 400, 300));
    view.corner_radius = 48;
    view.show_chrome = false;
    view.show_handles = false;
    view.show_size_label = false;
    view.cursor_visible = false;
    for rect in [
        Rect::new(40, 40, 300, 240),
        Rect::new(40, 40, 220, 180),
        Rect::new(20, 30, 320, 270),
    ] {
        view.selection = rect;
        // The controller's transition tests prove that manual drag clears this
        // animated preview before supplying the first frame to this renderer.
        view.preview_bounds = None;
        view.mask_blur = false;
        renderer.draw_to(&bitmap, &view).unwrap();
        let sharp = renderer.device().read_back_bgra(&target.texture).unwrap();
        view.mask_blur = true;
        renderer.draw_to(&bitmap, &view).unwrap();
        let moving = renderer.device().read_back_bgra(&target.texture).unwrap();
        // Repaint the unchanged bounds after release; cache state must not affect pixels.
        renderer.draw_to(&bitmap, &view).unwrap();
        let released = renderer.device().read_back_bgra(&target.texture).unwrap();
        assert_eq!(moving, released, "release changed mask pixels");
        let center = rect.center();
        assert_eq!(
            pixel_at(&moving, width, center.x as u32, center.y as u32),
            pixel_at(&background, width, center.x as u32, center.y as u32)
        );
        for (x, y) in [
            (rect.left + 1, rect.top + 1),
            (rect.right - 2, rect.top + 1),
            (rect.left + 1, rect.bottom - 2),
            (rect.right - 2, rect.bottom - 2),
        ] {
            let a = pixel_at(&sharp, width, x as u32, y as u32)[0];
            let b = pixel_at(&moving, width, x as u32, y as u32)[0];
            assert!(
                (i32::from(a) - i32::from(b)).abs() > 12,
                "unblurred wedge {rect:?} ({x},{y}): sharp={a}, blurred={b}"
            );
        }
        if let Ok(directory) = std::env::var("YUESHOT_CORNER_EVIDENCE") {
            write_png(
                &directory,
                &format!("drag-{}x{}.png", rect.width(), rect.height()),
                width,
                height,
                moving,
            );
        }
    }
}
