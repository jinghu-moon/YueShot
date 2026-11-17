use super::{ScreenshotView, geometry};
use crate::{PinContent, PinImage, PinManager, PinSelection};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, TestAppContext, WindowHandle, base::Root, point, px, size,
};
use std::{cell::Cell, rc::Rc, sync::Arc};
use yue_media::pixels::{AlphaMode, PixelBuffer, PixelDescriptor, PixelEncoding, PixelSource};

type Fixture = (
    Entity<PinManager>,
    u64,
    WindowHandle<Root>,
    Entity<ScreenshotView>,
    Rc<Cell<usize>>,
);

fn fixture(cx: &mut TestAppContext) -> Fixture {
    fixture_with_claim(cx, None)
}

struct SourceClaim(Option<Rc<Cell<usize>>>);
impl Drop for SourceClaim {
    fn drop(&mut self) {
        if let Some(released) = &self.0 {
            released.set(released.get() + 1);
        }
    }
}

fn fixture_with_claim(cx: &mut TestAppContext, released: Option<Rc<Cell<usize>>>) -> Fixture {
    cx.update(|cx| {
        gpui_kit::init(cx);
        yue_component::init(
            cx,
            yue_component::YueTheme::new(
                yue_component::ThemeMode::Light,
                yue_component::Accent::Azure,
            ),
        )
        .unwrap();
        crate::init(cx);
    });
    let committed = Rc::new(Cell::new(0));
    let commit = committed.clone();
    let claim = SourceClaim(released);
    let (owner, id) = cx.update(|cx| {
        let descriptor = PixelDescriptor::new(
            yue_media::PixelFormat::Rgba16Float,
            PixelEncoding::ScRgb,
            AlphaMode::Straight,
        )
        .unwrap();
        let mut bytes = [-0.25_f32, 12.5, 0.5, 0.5]
            .into_iter()
            .flat_map(|v| half::f16::from_f32(v).to_le_bytes())
            .collect::<Vec<_>>()
            .repeat(64 * 32);
        yue_geometry::selection_shape::visit_corner_coverage(64, 32, 8, |index, coverage| {
            bytes[index * 8 + 6..index * 8 + 8].copy_from_slice(
                &half::f16::from_f32(0.5 * f32::from(coverage) / 255.).to_le_bytes(),
            );
        });
        let image = PinImage::from_pixels(
            PixelSource::new(descriptor, 91),
            Arc::new(PixelBuffer::from_tight(64, 32, descriptor, bytes).unwrap()),
        )
        .unwrap();
        let (manager, _) = PinManager::new().unwrap();
        let owner = cx.new(|_| manager);
        let id = PinManager::open_selection(
            PinSelection::new(yue_geometry::Rect::new(-100, -200, -36, -168), image).radius(8),
            &owner,
            cx,
            move || {
                commit.set(commit.get() + 1);
                drop(claim);
            },
        )
        .unwrap()
        .id();
        (owner, id)
    });
    let (window, view, _) = cx.update(|cx| owner.read(cx).screenshot_for_test(id));
    let view = view.upgrade().unwrap();
    cx.simulate_window_resize(window.into(), size(px(34.), px(18.)));
    (owner, id, window, view, committed)
}

#[gpui_kit::test]
fn duplicate_receipts_commit_and_release_the_source_claim_only_once(cx: &mut TestAppContext) {
    let released = Rc::new(Cell::new(0));
    let (owner, id, window, _, committed) = fixture_with_claim(cx, Some(released.clone()));
    assert_eq!(released.get(), 0);
    frame(cx, window);
    assert_eq!((committed.get(), released.get()), (1, 1));
    cx.update(|cx| {
        owner.update(cx, |m, cx| {
            m.screenshot_ready(id, Ok(()), cx);
            m.screenshot_ready(id, Err("late failure".into()), cx);
            assert!(m.is_open(id));
            m.close(id, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!((committed.get(), released.get()), (1, 1));
}

#[gpui_kit::test]
fn failed_receipts_release_the_claim_once_without_late_consumption(cx: &mut TestAppContext) {
    let released = Rc::new(Cell::new(0));
    let (owner, id, _, _, committed) = fixture_with_claim(cx, Some(released.clone()));
    cx.update(|cx| {
        owner.update(cx, |m, cx| {
            m.screenshot_ready(id, Err("failed upload".into()), cx);
            m.screenshot_ready(id, Ok(()), cx);
            m.screenshot_ready(id, Err("duplicate failure".into()), cx);
            assert!(!m.is_open(id));
        })
    });
    cx.run_until_parked();
    assert_eq!((committed.get(), released.get()), (0, 1));
}

#[gpui_kit::test]
fn cancelled_receipts_release_the_claim_once_without_late_consumption(cx: &mut TestAppContext) {
    let released = Rc::new(Cell::new(0));
    let (owner, id, _, _, committed) = fixture_with_claim(cx, Some(released.clone()));
    cx.update(|cx| {
        owner.update(cx, |m, cx| {
            m.close(id, cx);
            m.screenshot_ready(id, Ok(()), cx);
            m.screenshot_ready(id, Err("late failure".into()), cx);
            assert!(!m.is_open(id));
        })
    });
    cx.run_until_parked();
    assert_eq!((committed.get(), released.get()), (0, 1));
}
fn frame(cx: &mut TestAppContext, window: WindowHandle<Root>) {
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn first_successful_paint_commits_once_and_keeps_the_shared_hdr_source(cx: &mut TestAppContext) {
    let (owner, id, window, view, committed) = fixture(cx);
    assert_eq!(committed.get(), 0);
    frame(cx, window);
    assert_eq!(committed.get(), 1);
    cx.update(|cx| {
        assert!(owner.read(cx).screenshot_for_test(id).2);
        let v = view.read(cx);
        assert_eq!(v.position, (-100, -200));
        assert_eq!(v.zoom, 1.);
        let PinContent::Image(source) = v.snapshot.content() else {
            panic!("image")
        };
        assert_eq!(source.pixel_source().generation(), 91);
        assert_eq!(
            v.image.as_ref().unwrap().image().as_bytes(0).unwrap(),
            source.pixels().as_bytes()
        );
        assert_eq!(
            v.image.as_ref().unwrap().color_space(),
            yue_component::ImagePreviewColorSpace::ScRgb
        );
    });
    frame(cx, window);
    assert_eq!(committed.get(), 1, "redrawing must not consume twice");
}

#[gpui_kit::test]
fn wheel_events_zoom_about_the_cursor_without_rebuilding_the_display_resource(
    cx: &mut TestAppContext,
) {
    let (_owner, _, window, view, _) = fixture(cx);
    frame(cx, window);
    let resource = cx.update(|cx| view.read(cx).image.as_ref().unwrap().image().id);
    for _ in 0..4 {
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                gpui_kit::PlatformInput::ScrollWheel(gpui_kit::ScrollWheelEvent {
                    position: point(px(10.), px(8.)),
                    delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(30.))),
                    modifiers: Default::default(),
                    touch_phase: gpui_kit::TouchPhase::Moved,
                }),
                cx,
            );
        })
        .unwrap();
    }
    cx.update(|cx| {
        let v = view.read(cx);
        assert_eq!(v.zoom, 1.1);
        assert_eq!(
            v.position,
            geometry::zoom_with_cursor(1., 1.1, (-100, -200), 64, 32, 2, (-80, -184))
        );
        assert_eq!(v.image.as_ref().unwrap().image().id, resource);
    });
}

#[gpui_kit::test]
fn dpi_updates_preserve_source_pixel_geometry_and_rounding(cx: &mut TestAppContext) {
    let (_owner, _, window, view, _) = fixture(cx);
    frame(cx, window);
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_window_scale_factor_change(window.into(), scale);
        let b = geometry::border_px((scale * 96.).round() as u32);
        cx.simulate_window_resize(
            window.into(),
            size(
                px((64 + 2 * b) as f32 / scale),
                px((32 + 2 * b) as f32 / scale),
            ),
        );
        frame(cx, window);
        cx.update_window(window.into(), |_, w, cx| {
            let sprites: Vec<_> = w
                .painted_primitives()
                .into_iter()
                .filter_map(|p| match p {
                    gpui_kit::Primitive::PolychromeSprite(s) => Some(s),
                    _ => None,
                })
                .collect();
            assert_eq!(sprites.len(), 1);
            assert_eq!(sprites[0].bounds.size.width.0, 64.);
            assert_eq!(sprites[0].bounds.size.height.0, 32.);
            assert_eq!(view.read(cx).zoom, 1.);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn hiding_pending_preparation_defers_its_commit_until_it_is_shown(cx: &mut TestAppContext) {
    let (owner, id, window, _, committed) = fixture(cx);
    cx.update(|cx| owner.update(cx, |m, cx| m.set_visible(false, cx)));
    frame(cx, window);
    assert_eq!(committed.get(), 0);
    cx.update(|cx| assert!(owner.update(cx, |m, cx| m.raise(id, cx))));
    frame(cx, window);
    assert_eq!(committed.get(), 1);
}

#[gpui_kit::test]
fn double_click_closes_the_production_surface_and_retains_an_undo_source(cx: &mut TestAppContext) {
    let (owner, id, window, _, _) = fixture(cx);
    frame(cx, window);
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                position: point(px(10.), px(8.)),
                button: gpui_kit::MouseButton::Left,
                click_count: 2,
                modifiers: Default::default(),
                first_mouse: false,
            }),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(!owner.read(cx).is_open(id)));
    cx.update(|cx| PinManager::undo(&owner, (0, 0), cx).unwrap());
    cx.run_until_parked();
    cx.update(|cx| assert!(owner.read(cx).is_open(id + 1)));
}

#[gpui_kit::test]
fn rejected_preparation_does_not_commit_or_add_an_empty_undo_item(cx: &mut TestAppContext) {
    let (owner, id, window, _, committed) = fixture(cx);
    cx.update(|cx| {
        owner.update(cx, |m, cx| {
            m.screenshot_ready(id, Err("injected upload failure".into()), cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(committed.get(), 0);
    cx.update(|cx| assert!(!owner.read(cx).is_open(id)));
    assert!(cx.update_window(window.into(), |_, _, _| ()).is_err());
    cx.update(|cx| PinManager::undo(&owner, (0, 0), cx).unwrap());
    cx.update(|cx| assert!(!owner.read(cx).is_open(id + 1)));
}

#[gpui_kit::test]
fn screenshot_border_follows_the_export_contour_without_a_second_circular_clip(
    cx: &mut TestAppContext,
) {
    let (_owner, _, window, _, _) = fixture(cx);
    frame(cx, window);
    cx.update_window(window.into(), |_, w, _| {
        let primitives = w.painted_primitives();
        assert!(
            primitives
                .iter()
                .any(|p| matches!(p, gpui_kit::Primitive::Path(_))),
            "continuous export corners require a matching closed border path"
        );
        let sprite = primitives
            .into_iter()
            .find_map(|p| match p {
                gpui_kit::Primitive::PolychromeSprite(s) => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            sprite.corner_radii.top_left.0, 0.,
            "source alpha already defines the quartic corner; a circular shader clips it again"
        );
    })
    .unwrap();
}

// Inspect the actual production tessellation, not just a radius/style property.
fn mesh_contains(path: &gpui_kit::Path<gpui_kit::ScaledPixels>, x: f32, y: f32) -> bool {
    path.vertices.as_chunks::<3>().0.iter().any(|triangle| {
        let p: Vec<_> = triangle
            .iter()
            .map(|v| (v.xy_position.x.0, v.xy_position.y.0))
            .collect();
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[1].1 - p[0].1) * (p[2].0 - p[0].0);
        if area.abs() < 0.000001 {
            return false;
        }
        let cross =
            |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
        let c = [cross(p[0], p[1]), cross(p[1], p[2]), cross(p[2], p[0])];
        c.iter().all(|v| *v >= -0.00001) || c.iter().all(|v| *v <= 0.00001)
    })
}

#[gpui_kit::test]
fn border_mesh_covers_all_four_curves_at_each_zoom_and_dpi_without_filling_image(
    cx: &mut TestAppContext,
) {
    let (_owner, _, window, view, _) = fixture(cx);
    for scale in [1., 1.25, 1.5, 2.] {
        cx.simulate_window_scale_factor_change(window.into(), scale);
        for radius in [0, 1, 8] {
            for zoom in [0.1, 0.5, 1., 2., 10.] {
                cx.update_window(window.into(), |_, w, cx| {
                    view.update(cx, |v, cx| {
                        v.radius = radius;
                        v.zoom = zoom;
                        v.apply_bounds(w);
                        cx.notify();
                    })
                })
                .unwrap();
                // Deliver the resize notification a real HWND sends after SetWindowPos.
                // The headless platform records the request but has no Windows message loop.
                let b = geometry::border_px((scale * 96.).round() as u32);
                let (width, height) = geometry::window_size(64, 32, zoom, b);
                cx.simulate_window_resize(
                    window.into(),
                    size(px(width as f32 / scale), px(height as f32 / scale)),
                );
                frame(cx, window);
                cx.update_window(window.into(), |_, w, _| {
                    let path = w
                        .painted_primitives()
                        .into_iter()
                        .find_map(|p| match p {
                            gpui_kit::Primitive::Path(p) => Some(p),
                            _ => None,
                        })
                        .expect("closed border mesh");
                    let b = geometry::border_px((scale * 96.).round() as u32) as f32;
                    let (width, height) = geometry::window_size(64, 32, zoom, b as i32);
                    let (width, height) = (width as f32, height as f32);
                    assert!(
                        !mesh_contains(&path, width / 2., height / 2.),
                        "border fills translucent source radius={radius} zoom={zoom} scale={scale} bounds={:?} expected={width}x{height}", path.bounds
                    );
                    assert!(mesh_contains(&path, width / 2., b / 2.));
                    let r = geometry::scaled_radius(radius, zoom) as f32;
                    if r == 0. {
                        return;
                    }
                    let outer_r = r + b;
                    // Independent analytic quartic oracle at seven heights per corner.
                    for fraction in [0.15f32, 0.25, 0.35, 0.5, 0.65, 0.75, 0.85] {
                        let y = b + r * fraction;
                        let inset = |y: f32, r: f32| {
                            r * (1. - (1. - (1. - y / r).powi(4)).max(0.).powf(0.25))
                        };
                        let outer_x = inset(y, outer_r);
                        let inner_x = b + inset(y - b, r);
                        let x = (outer_x + inner_x) / 2.;
                        for (x, y) in [
                            (x, y),
                            (width - x, y),
                            (x, height - y),
                            (width - x, height - y),
                        ] {
                            assert!(
                                mesh_contains(&path, x, y),
                                "missing arc r={radius} zoom={zoom} scale={scale} at ({x},{y})"
                            );
                        }
                    }
                })
                .unwrap();
            }
        }
    }
}
