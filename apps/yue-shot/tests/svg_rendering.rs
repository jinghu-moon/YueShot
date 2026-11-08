//! Exercise the production GPUI graphical SVG contract without font discovery.
use gpui_kit::{DevicePixels, Image, ImageFormat, Size, SvgError, SvgRenderer, SvgSize};
use std::{path::PathBuf, sync::Arc};

fn render(name: &str, content: &[u8], width: i32, height: i32) -> Vec<u8> {
    let renderer = SvgRenderer::new(Arc::new(()));
    let svg = renderer.parse_svg(content).expect("valid SVG parses");
    let size = Size::new(DevicePixels(width), DevicePixels(height));
    let image = renderer
        .render_parsed(&svg, SvgSize::ExactSize(size))
        .expect("parsed SVG rasterizes");
    assert_eq!(image.size(0), size);
    let bytes = image.as_bytes(0).unwrap();
    assert_eq!(bytes.len(), width as usize * height as usize * 4);
    assert!(
        bytes.as_chunks::<4>().0.iter().any(|p| p[3] != 0),
        "{name} is blank"
    );
    if let Some(directory) = std::env::var_os("YUESHOT_SVG_EVIDENCE") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &bytes
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect::<Vec<_>>(),
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    bytes.to_vec()
}

#[test]
fn svg_live_text_is_explicitly_unsupported() {
    let renderer = SvgRenderer::new(Arc::new(()));
    for body in [
        "<text>YueShot 截图 😀</text>",
        "<text><tspan>é̂̃̄̆̇̈̊</tspan></text>",
        "<textPath>截图</textPath>",
        "<defs><text id='label'>YueShot</text></defs><use href='#label'/>",
        "<s:text xmlns:s='http://www.w3.org/2000/svg'>YueShot</s:text>",
    ] {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><rect width="80" height="40"/>{body}</svg>"#
        );
        assert!(matches!(
            renderer.parse_svg(svg.as_bytes()),
            Err(SvgError::UnsupportedText)
        ));
        assert!(matches!(
            renderer.render_single_frame(svg.as_bytes(), 1.0),
            Err(SvgError::UnsupportedText)
        ));
        assert!(
            Image::from_bytes(ImageFormat::Svg, svg.into_bytes())
                .to_image_data(renderer.clone())
                .is_err()
        );
    }
    // Metadata or comments mentioning "text" do not require a font parser.
    render("metadata", r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><title>text 截图</title><!-- <text>not a live element</text> --><path d="M0 0H80V40H0Z"/></svg>"#.as_bytes(), 80, 40);
}

#[test]
fn svg_icons_preserve_pixels_and_aspect_ratio() {
    for (name, bytes) in [
        (
            "camera",
            include_bytes!("../../../vendor/yue-ui/crates/yue-component-icons/assets/tabler/outline/camera.svg").as_slice(),
        ),
        (
            "clipboard",
            include_bytes!("../../../vendor/yue-ui/crates/yue-component-icons/assets/tabler/outline/clipboard.svg").as_slice(),
        ),
    ] {
        let first = render(name, bytes, 48, 48);
        let second = render(name, bytes, 48, 48);
        assert_eq!(first, second);
    }
    let renderer = SvgRenderer::new(Arc::new(()));
    let svg = renderer
        .parse_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20"/></svg>"#)
        .unwrap();
    let image = renderer
        .render_parsed(
            &svg,
            SvgSize::Size(Size::new(DevicePixels(80), DevicePixels(80))),
        )
        .unwrap();
    assert_eq!(image.size(0), Size::new(DevicePixels(80), DevicePixels(40)));
    assert!(renderer.parse_svg(b"invalid SVG").is_err());
}

#[test]
fn svg_graphics_preserve_gradients_clipping_filters_and_embedded_raster() {
    for (name, content) in [
        (
            "gradient-clip",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><defs><linearGradient id="g"><stop stop-color="#f00"/><stop offset="1" stop-color="#08f"/></linearGradient><clipPath id="c"><circle cx="40" cy="20" r="18"/></clipPath></defs><path d="M0 0H80V40H0Z" fill="url(#g)" clip-path="url(#c)"/></svg>"##,
        ),
        (
            "filter",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><defs><filter id="b"><feGaussianBlur stdDeviation="2"/></filter></defs><rect x="20" y="10" width="40" height="20" rx="5" fill="#08f" filter="url(#b)"/></svg>"##,
        ),
        (
            "embedded-raster",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><image width="80" height="40" href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="/></svg>"##,
        ),
    ] {
        render(name, content.as_bytes(), 80, 40);
    }
}

#[test]
fn svg_does_not_scan_a_malformed_system_font() {
    const PROBE: &str = "YUESHOT_SVG_ISOLATED_FONT_PROBE";
    if std::env::var_os(PROBE).is_some() {
        let renderer = SvgRenderer::new(Arc::new(()));
        let text = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><text>YueShot</text></svg>"#;
        assert!(matches!(
            renderer.parse_svg(text),
            Err(SvgError::UnsupportedText)
        ));
        render("no-font-scan", br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><path d="M0 0H20V20H0Z"/></svg>"#, 20, 20);
        return;
    }
    let directory = std::env::temp_dir().join(format!(
        "yueshot-svg-no-font-scan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let fonts = directory.join("Fonts");
    std::fs::create_dir(&directory).unwrap();
    std::fs::create_dir(&fonts).unwrap();
    let file = fonts.join("malformed.ttc");
    // The exact 12-byte collection header that panics in the original
    // fontdb Debug system-font scan. Never install it as a real system font.
    std::fs::write(&file, b"ttcf\x00\x01\x00\x00\x40\x00\x00\x00").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "svg_does_not_scan_a_malformed_system_font"])
        .env(PROBE, "1")
        .env("SYSTEMROOT", &directory)
        .env("USERPROFILE", directory.join("user"))
        .output()
        .unwrap();
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(fonts).unwrap();
    std::fs::remove_dir(directory).unwrap();
    assert!(
        result.status.success(),
        "isolated font probe failed: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
