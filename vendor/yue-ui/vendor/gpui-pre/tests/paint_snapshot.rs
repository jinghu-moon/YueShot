//! Isolated integration target: avoids unrelated upstream unit-test assets and
//! dev-only macros omitted from the crates.io package. See test-gpui-snapshot.ps1.
use gpui::{
    AnyElement, App, AppContext as _, Bounds, ContentMask, Context, Corners, Element, ElementId,
    Entity, GlobalElementId, InspectorElementId, InteractiveElement as _, IntoElement, LayoutId,
    Modifiers, MouseButton, PaintSnapshot, ParentElement as _, PathBuilder, Pixels, Primitive,
    Render, RenderImage, StyleRefinement, Styled as _, TestAppContext, TransformationMatrix,
    VisualTestContext, Window, canvas, div, fill, point, px, rgb, size,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

struct Capture {
    child: AnyElement,
    snapshot: Rc<RefCell<Option<PaintSnapshot>>>,
}
impl IntoElement for Capture {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Capture {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some("snapshot-capture".into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(w, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(w, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        let (_, snapshot) = w.capture_paint(bounds.origin, |w| self.child.paint(w, cx));
        *self.snapshot.borrow_mut() = Some(snapshot);
    }
}
struct Leaf {
    image: Arc<RenderImage>,
    renders: Rc<Cell<usize>>,
    clicks: Rc<Cell<usize>>,
    nearest: bool,
    precise: bool,
}
impl Render for Leaf {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let image = self.image.clone();
        let nearest = self.nearest;
        let precise = self.precise;
        let clicks = self.clicks.clone();
        div().id("live-content").w(px(120.)).h(px(80.)).backdrop_blur(px(6.))
            .on_mouse_down(MouseButton::Left, move |_, _, _| clicks.set(clicks.get()+1))
            .child(canvas(|_, _, _| (), move |bounds, _, w, cx| {
                w.paint_quad(fill(bounds, rgb(0x2468ab)));
                let mut path = PathBuilder::stroke(px(2.));
                path.move_to(bounds.origin + point(px(3.), px(3.)));
                path.line_to(bounds.origin + point(px(30.), px(30.)));
                w.paint_path(path.build().unwrap(), rgb(0x123456));
                w.paint_svg(
                    Bounds::new(bounds.origin + point(px(8.), px(4.)), size(px(16.), px(16.))),
                    "paint-snapshot-test.svg".into(),
                    Some(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path fill="black" d="M0 0h16v16H0z"/></svg>"#),
                    TransformationMatrix::default(), rgb(0x123456).into(), cx,
                ).unwrap();
                let bounds = Bounds::new(bounds.origin + point(px(40.), px(2.)), size(px(20.), px(20.)));
                if nearest || precise {
                    let image_bounds = Bounds::new(bounds.origin - point(px(8.25), px(4.5)), size(px(64.), px(64.)));
                    if nearest { w.paint_image_nearest(bounds, image_bounds, Corners::all(px(4.)), image.clone(), 0, false).unwrap(); }
                    else { w.paint_image_precise(bounds, image_bounds, Corners::all(px(4.)), image.clone(), 0, false).unwrap(); }
                } else {
                    w.paint_image(bounds, bounds, Corners::default(), image.clone(), 0, false).unwrap();
                }
            }).size_full())
    }
}
struct Probe {
    leaf: Entity<Leaf>,
    snapshot: Rc<RefCell<Option<PaintSnapshot>>>,
    replay: bool,
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.replay {
            let snapshot = self.snapshot.clone();
            canvas(
                |_, _, _| (),
                move |bounds, _, w, _| {
                    let origin = bounds.origin + point(px(12.), px(16.));
                    let mask = ContentMask {
                        bounds: Bounds::new(origin, size(px(120.), px(24.))),
                    };
                    w.with_content_mask(Some(mask), |w| {
                        assert!(w.paint_snapshot(snapshot.borrow().as_ref().unwrap(), origin))
                    });
                },
            )
            .w(px(160.))
            .h(px(120.))
            .into_any_element()
        } else {
            Capture {
                child: self
                    .leaf
                    .clone()
                    .cached(StyleRefinement::default())
                    .into_any_element(),
                snapshot: self.snapshot.clone(),
            }
            .into_any_element()
        }
    }
}
fn run_case(cx: &mut TestAppContext, cached_capture: bool, nearest: bool, precise: bool) {
    for scale in [1., 1.5, 2.] {
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::from_pixel(4, 4, image::Rgba([80, 120, 180, 255])),
        )]));
        let renders = Rc::new(Cell::new(0));
        let clicks = Rc::new(Cell::new(0));
        let snapshot = Rc::new(RefCell::new(None));
        let (v, cx) = cx.add_window_view(|w, cx| {
            w.set_scale_factor(scale);
            let leaf = cx.new(|_| Leaf {
                image: image.clone(),
                renders: renders.clone(),
                clicks: clicks.clone(),
                nearest,
                precise,
            });
            Probe {
                leaf,
                snapshot: snapshot.clone(),
                replay: false,
            }
        });
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|w, cx| w.draw(cx).clear(cx));
        };
        draw(cx);
        if cached_capture {
            let built = renders.get();
            v.update(cx, |_, cx| cx.notify());
            draw(cx);
            assert_eq!(
                renders.get(),
                built,
                "Child must actually reuse cached paint"
            );
        }
        let original = cx.update(|w, _| w.painted_primitives());
        let original_blur = original
            .iter()
            .find_map(|p| {
                if let Primitive::BackdropBlur(b) = p {
                    Some(*b)
                } else {
                    None
                }
            })
            .expect("Live and cached paint must retain the filter");
        assert_eq!(original_blur.sigma, px(6.).scale(scale));
        let original_quad = original
            .iter()
            .find_map(|p| {
                if let Primitive::Quad(q) = p {
                    Some(*q)
                } else {
                    None
                }
            })
            .unwrap();
        let original_path = original
            .iter()
            .find_map(|p| {
                if let Primitive::Path(p) = p {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .unwrap();
        let original_sprite = original
            .iter()
            .find_map(|p| {
                if let Primitive::MonochromeSprite(p) = p {
                    Some(*p)
                } else {
                    None
                }
            })
            .unwrap();
        let original_image = original
            .iter()
            .find_map(|p| match p {
                Primitive::PolychromeSprite(p) => Some(*p),
                _ => None,
            })
            .unwrap();
        assert_eq!(original_image.pad & 1, u32::from(nearest));
        assert_eq!(original_image.pad & 2, u32::from(precise) * 2);
        if nearest || precise {
            assert_eq!(
                original_image.bounds.size,
                size(px(64.), px(64.)).scale(scale)
            );
            assert_eq!(
                original_image.tile.bounds.size,
                size(gpui::DevicePixels(4), gpui::DevicePixels(4))
            );
        }
        cx.update(|w, _| {
            w.drop_image(image.clone()).unwrap();
            assert!(!w.has_image_atlas_entry(&image));
        });
        v.update(cx, |v, cx| {
            v.replay = true;
            cx.notify();
        });
        draw(cx);
        cx.update(|w, _| {
            let primitives = w.painted_primitives();
            let replayed_blur = primitives
                .iter()
                .find_map(|p| {
                    if let Primitive::BackdropBlur(b) = p {
                        Some(b)
                    } else {
                        None
                    }
                })
                .expect("Snapshot must replay the filter, not a frozen bitmap");
            assert_eq!(
                replayed_blur.quad.bounds.origin,
                original_blur.quad.bounds.origin + point(px(12.), px(16.)).scale(scale)
            );
            assert_eq!(
                replayed_blur
                    .quad
                    .bounds
                    .intersect(&replayed_blur.quad.content_mask.bounds)
                    .size
                    .height,
                px(24.).scale(scale)
            );
            let quad = primitives
                .iter()
                .find_map(|p| {
                    if let Primitive::Quad(q) = p {
                        Some(q)
                    } else {
                        None
                    }
                })
                .unwrap();
            let path = primitives
                .iter()
                .find_map(|p| {
                    if let Primitive::Path(p) = p {
                        Some(p)
                    } else {
                        None
                    }
                })
                .unwrap();
            let sprite = primitives
                .iter()
                .find_map(|p| {
                    if let Primitive::MonochromeSprite(p) = p {
                        Some(p)
                    } else {
                        None
                    }
                })
                .unwrap();
            let delta = point(px(12.), px(16.)).scale(scale);
            let replayed_image = primitives
                .iter()
                .find_map(|p| match p {
                    Primitive::PolychromeSprite(p) => Some(p),
                    _ => None,
                })
                .unwrap();
            assert_eq!(replayed_image.pad & 1, u32::from(nearest));
            assert_eq!(replayed_image.pad & 2, u32::from(precise) * 2);
            assert_eq!(
                replayed_image.bounds.origin,
                original_image.bounds.origin + delta
            );
            assert_eq!(replayed_image.bounds.size, original_image.bounds.size);
            assert_eq!(
                replayed_image.tile.bounds.size,
                original_image.tile.bounds.size
            );
            assert_eq!(replayed_image.corner_radii, original_image.corner_radii);
            assert_eq!(quad.bounds.origin, original_quad.bounds.origin + delta);
            assert_eq!(
                path.vertices[0].xy_position,
                original_path.vertices[0].xy_position + delta
            );
            assert_eq!(sprite.bounds.origin, original_sprite.bounds.origin + delta);
            assert_eq!(
                quad.bounds.intersect(&quad.content_mask.bounds).size.height,
                px(24.).scale(scale)
            );
            assert_eq!(
                primitives
                    .iter()
                    .filter(|p| matches!(p, Primitive::PolychromeSprite(_)))
                    .count(),
                1
            );
            assert!(w.has_image_atlas_entry(&image));
        });
        cx.simulate_click(point(px(20.), px(20.)), Modifiers::default());
        draw(cx);
        assert_eq!(
            clicks.get(),
            0,
            "Frozen paint must not replay input listeners"
        );
    }
}
#[gpui::test]
fn snapshot_translates_clips_and_reloads_evicted_images_without_input(cx: &mut TestAppContext) {
    run_case(cx, false, false, false);
}
#[gpui::test]
fn capturing_reused_paint_preserves_image_sources(cx: &mut TestAppContext) {
    run_case(cx, true, false, false);
}
#[cfg(target_os = "windows")]
#[gpui::test]
fn nearest_sampling_full_uvs_and_corners_survive_cached_capture_and_replay(
    cx: &mut TestAppContext,
) {
    run_case(cx, true, true, false);
}
#[cfg(target_os = "windows")]
#[gpui::test]
fn precise_linear_uvs_and_sampling_survive_cached_capture_and_replay(cx: &mut TestAppContext) {
    run_case(cx, true, false, true);
}

struct ImageLifecycle {
    image: Option<Arc<RenderImage>>,
    snapshot: Rc<RefCell<Option<PaintSnapshot>>>,
    capture: bool,
}
impl Render for ImageLifecycle {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.clone();
        let snapshot = self.snapshot.clone();
        let capture = self.capture;
        canvas(
            |_, _, _| (),
            move |bounds, _, w, _| {
                if let Some(image) = &image {
                    let draw = |w: &mut Window| {
                        w.paint_image_precise(
                            bounds,
                            bounds,
                            Corners::default(),
                            image.clone(),
                            0,
                            false,
                        )
                        .unwrap()
                    };
                    if capture {
                        let (_, captured) = w.capture_paint(bounds.origin, draw);
                        *snapshot.borrow_mut() = Some(captured);
                    } else {
                        draw(w);
                    }
                }
            },
        )
        .w(px(24.))
        .h(px(24.))
    }
}

#[cfg(target_os = "windows")]
#[gpui::test]
fn precise_image_atlas_lifetime_follows_all_resource_and_snapshot_owners(cx: &mut TestAppContext) {
    image_lifetime(cx, false);
}

#[cfg(target_os = "windows")]
#[gpui::test]
fn float_image_atlas_lifetime_follows_resource_snapshot_and_replacement(cx: &mut TestAppContext) {
    image_lifetime(cx, true);
}

#[cfg(target_os = "windows")]
fn image_lifetime(cx: &mut TestAppContext, float: bool) {
    let make_image = || {
        if float {
            let bytes = [0xb400_u16, 0x4a40, 0x4000, 0x3c00]
                .into_iter()
                .cycle()
                .take(64)
                .flat_map(u16::to_le_bytes)
                .collect();
            return Arc::new(
                RenderImage::from_rgba16f(
                    size(4.into(), 4.into()),
                    bytes,
                    gpui::RenderImageFormat::Rgba16FloatScRgb,
                )
                .unwrap(),
            );
        }
        Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::from_pixel(4, 4, image::Rgba([17, 70, 180, 127])),
        )]))
    };
    let image = make_image();
    let id = image.id;
    let weak = Arc::downgrade(&image);
    let snapshot = Rc::new(RefCell::new(None));
    let (view, cx) = cx.add_window_view(|_, _| ImageLifecycle {
        image: Some(image),
        snapshot: snapshot.clone(),
        capture: true,
    });
    let draw = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    };
    draw(cx);
    if float {
        cx.update(|w, _| {
            let sprite = w
                .painted_primitives()
                .into_iter()
                .find_map(|p| {
                    if let Primitive::PolychromeSprite(p) = p {
                        Some(p)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!((sprite.pad >> 2) & 3, 2);
            assert_eq!(
                sprite.tile.texture_id.kind,
                gpui::AtlasTextureKind::PolychromeFloat
            );
            assert_eq!(sprite.pad >> 16, 0x4a40);
        });
    }
    assert!(cx.update(|w, _| w.has_image_atlas_frame(id, 0)));
    view.update(cx, |v, cx| {
        v.image = None;
        v.capture = false;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(
        weak.upgrade().is_some(),
        "a frozen snapshot still owns its image"
    );
    assert!(
        cx.update(|w, _| w.has_image_atlas_frame(id, 0)),
        "another consumer must not evict a shared resource"
    );
    snapshot.borrow_mut().take();
    draw(cx);
    assert!(weak.upgrade().is_none());
    assert!(
        !cx.update(|w, _| w.has_image_atlas_frame(id, 0)),
        "last owner dropped: retire the upload"
    );
    let mut previous = None;
    for _ in 0..20 {
        let image = make_image();
        let current = image.id;
        view.update(cx, |v, cx| {
            v.image = Some(image);
            cx.notify();
        });
        draw(cx);
        if let Some(id) = previous {
            assert!(
                !cx.update(|w, _| w.has_image_atlas_frame(id, 0)),
                "revisions must not accumulate uploads"
            );
        }
        assert!(cx.update(|w, _| w.has_image_atlas_frame(current, 0)));
        previous = Some(current);
    }
}
