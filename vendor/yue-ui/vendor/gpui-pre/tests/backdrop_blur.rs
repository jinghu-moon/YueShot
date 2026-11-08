//! Scene contracts run without a native window. WARP pixel tests live in the
//! Windows backend and exercise the same production shaders as the application.
use gpui::{
    BackdropBlur, Bounds, ContentMask, Corners, Hsla, PaddedBool32, Primitive, Quad, ScaledPixels,
    Scene, Underline, point, size,
};

fn quad(x: f32) -> Quad {
    let bounds = Bounds::new(
        point(ScaledPixels(x), ScaledPixels(0.)),
        size(ScaledPixels(20.), ScaledPixels(20.)),
    );
    Quad {
        bounds,
        content_mask: ContentMask { bounds },
        ..Default::default()
    }
}
fn blur() -> BackdropBlur {
    BackdropBlur {
        quad: quad(200.),
        sigma: ScaledPixels(8.),
        opacity: 0.75,
    }
}
fn underline() -> Underline {
    let q = quad(0.);
    Underline {
        order: 0,
        pad: 0,
        bounds: q.bounds,
        content_mask: q.content_mask,
        color: Hsla::default(),
        thickness: ScaledPixels(1.),
        wavy: PaddedBool32::from(false),
    }
}

#[test]
fn filter_is_a_global_order_barrier_even_for_disjoint_batches() {
    let mut scene = Scene::default();
    scene.insert_primitive(quad(0.));
    scene.insert_primitive(underline());
    scene.insert_primitive(blur());
    scene.insert_primitive(quad(100.));
    scene.insert_primitive(underline());
    scene.insert_primitive(blur());
    scene.insert_primitive(quad(300.));
    scene.finish();
    let first = scene.backdrop_blurs[0].quad.order;
    let second = scene.backdrop_blurs[1].quad.order;
    assert!(scene.quads[0].order < first);
    assert!(scene.underlines[0].order < first);
    assert!(
        scene
            .quads
            .iter()
            .find(|q| q.bounds.origin.x.0 == 100.)
            .unwrap()
            .order
            > first
    );
    assert!(scene.underlines[1].order > first && scene.underlines[1].order < second);
    assert!(scene.quads.last().unwrap().order > second);
    assert_eq!(
        scene
            .quads
            .iter()
            .filter(|q| q.order == first || q.order == second)
            .count(),
        2
    );
}

#[test]
fn layers_replay_and_frame_reset_preserve_barriers() {
    let mut source = Scene::default();
    source.push_layer(Bounds::new(
        point(ScaledPixels(0.), ScaledPixels(0.)),
        size(ScaledPixels(1000.), ScaledPixels(1000.)),
    ));
    source.insert_primitive(quad(0.));
    source.insert_primitive(blur());
    source.insert_primitive(quad(0.));
    source.pop_layer();
    source.finish();
    let mut replay = Scene::default();
    replay.insert_primitive(quad(0.));
    replay.replay(0..source.len(), &source);
    replay.finish();
    let order = replay.backdrop_blurs[0].quad.order;
    assert!(replay.quads[0].order < order && replay.quads[1].order < order);
    assert!(replay.quads.last().unwrap().order > order);
    assert_eq!(replay.backdrop_blurs[0].sigma, ScaledPixels(8.));
    assert_eq!(replay.backdrop_blurs[0].opacity, 0.75);
    replay.clear();
    replay.insert_primitive(quad(0.));
    assert!(replay.backdrop_blurs.is_empty());
    let mut fresh = Scene::default();
    fresh.insert_primitive(quad(0.));
    assert_eq!(replay.quads[0].order, fresh.quads[0].order);
}

#[test]
fn invisible_filter_creates_no_marker_or_barrier() {
    let mut scene = Scene::default();
    let mut b = blur();
    b.quad.content_mask.bounds = quad(400.).bounds;
    scene.insert_primitive(b);
    assert_eq!(scene.len(), 0);
    assert!(scene.backdrop_blurs.is_empty());
    assert!(scene.quads.is_empty());
}

// Preserve typed API compilation, including rems and style refinement.
#[test]
fn reusable_styled_interface_accepts_absolute_lengths() {
    use gpui::{Styled, div, px, rems};
    let mut element = div().backdrop_blur(rems(1.)).rounded(px(12.));
    assert_eq!(element.style().backdrop_blur, Some(rems(1.).into()));
    let _ = Primitive::BackdropBlur(BackdropBlur {
        quad: Quad {
            corner_radii: Corners::all(ScaledPixels(12.)),
            ..quad(0.)
        },
        ..blur()
    });
}

struct StyledProbe;
impl gpui::Render for StyledProbe {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement, Styled, div, px, rems};
        div().size_full().opacity(0.5).children([
            div().w(px(20.)).h(px(20.)).backdrop_blur(rems(1.)),
            div().w(px(20.)).h(px(20.)).backdrop_blur(px(128.)),
            div().w(px(20.)).h(px(20.)).backdrop_blur(px(0.)),
            div().w(px(20.)).h(px(20.)).backdrop_blur(px(-1.)),
            div().w(px(20.)).h(px(20.)).backdrop_blur(px(f32::NAN)),
            div().w(px(20.)).h(px(20.)).backdrop_blur(px(f32::INFINITY)),
            div()
                .w(px(20.))
                .h(px(20.))
                .opacity(0.)
                .backdrop_blur(px(8.)),
        ])
    }
}

#[gpui::test]
fn styled_paint_resolves_rem_dpi_opacity_and_ignores_invalid_filters(
    cx: &mut gpui::TestAppContext,
) {
    let (_, cx) = cx.add_window_view(|w, _| {
        w.set_rem_size(gpui::px(24.));
        w.set_scale_factor(1.5);
        StyledProbe
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
    });
    let filters = cx.update(|w, _| {
        w.painted_primitives()
            .into_iter()
            .filter_map(|p| {
                if let Primitive::BackdropBlur(b) = p {
                    Some(b)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(filters.len(), 2);
    assert_eq!(filters[0].sigma, ScaledPixels(36.));
    assert_eq!(filters[1].sigma, ScaledPixels(96.));
    assert!(filters.iter().all(|b| b.opacity == 0.5));
    cx.update(|w, cx| {
        assert_eq!(
            w.simulate_next_frame(cx),
            0,
            "Static blur must not keep requesting frames"
        );
    });
}
