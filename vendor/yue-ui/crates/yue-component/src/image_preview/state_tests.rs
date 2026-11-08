use super::*;

fn motion(at: Instant, anchor: Option<[f32; 2]>) -> ViewMotion {
    ViewMotion {
        start: Camera {
            center: [0.5, 0.5],
            width: 1600.,
        },
        end: Camera {
            center: [0.6, 0.4],
            width: 3200.,
        },
        at,
        duration: Duration::from_millis(260),
        anchor,
        ease: easing(
            YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure),
            "--image-preview-view-ease",
        ),
    }
}
fn viewport() -> Viewport {
    Viewport {
        origin: [10., 20.],
        size: [600., 400.],
        scale: 1.5,
        inset: 12.,
    }
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "{a} != {b}");
}

#[test]
fn view_motion_has_exact_endpoints_and_can_redirect_without_jumping() {
    let at = Instant::now();
    let first = motion(at, None);
    let image = [1600., 1000.];
    let vp = viewport();
    let start = first.sample(at, image, vp);
    close(start.width, first.start.width);
    assert_eq!(start.center, first.start.center);
    let end = first.sample(at + first.duration, image, vp);
    close(end.width, first.end.width);
    assert_eq!(end.center, first.end.center);
    let now = at + Duration::from_millis(90);
    let displayed = first.sample(now, image, vp);
    assert!(displayed.width > first.start.width && displayed.width < first.end.width);
    let redirected = ViewMotion {
        start: displayed,
        at: now,
        ..motion(at, None)
    };
    close(redirected.sample(now, image, vp).width, displayed.width);
    assert_eq!(redirected.sample(now, image, vp).center, displayed.center);
}

#[test]
fn animated_zoom_preserves_pointer_anchor_throughout_travel() {
    let at = Instant::now();
    let anchor = [210., 160.];
    let m = motion(at, Some(anchor));
    let image = [1600., 1000.];
    let vp = viewport();
    let initial = m.start.image_point(anchor, image, vp);
    for ms in [0, 20, 90, 180, 260] {
        let next = m
            .sample(at + Duration::from_millis(ms), image, vp)
            .image_point(anchor, image, vp);
        close(initial[0], next[0]);
        close(initial[1], next[1]);
    }
}

#[test]
fn split_transition_has_exact_endpoints_and_zero_duration_finishes() {
    let theme = YueTheme::new(crate::ThemeMode::Light, crate::Accent::Azure);
    let ease = easing(theme, "--image-preview-compare-ease");
    let at = Instant::now();
    let duration = duration(theme, "--image-preview-compare-duration");
    close(ease(elapsed(at, at, duration)), 0.);
    close(ease(elapsed(at + duration, at, duration)), 1.);
    close(elapsed(at, at, Duration::ZERO), 1.);
}
