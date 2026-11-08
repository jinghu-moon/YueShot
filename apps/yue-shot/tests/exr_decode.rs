//! Keep EXR's SIMD compression/decompression covered when updating transitive dependencies.
use std::io::Cursor;

#[test]
fn exr_roundtrip_preserves_float_pixels_across_multiple_blocks() {
    let original = image::Rgb32FImage::from_fn(131, 67, |x, y| {
        image::Rgb([
            (x as f32 - 64.0) / 8.0,
            y as f32 / 16.0,
            ((x * 17 + y * 31) % 257) as f32 / 4.0,
        ])
    });
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb32F(original.clone())
        .write_to(&mut encoded, image::ImageFormat::OpenExr)
        .unwrap();
    let decoded =
        image::load_from_memory_with_format(encoded.get_ref(), image::ImageFormat::OpenExr)
            .unwrap()
            .into_rgb32f();
    assert_eq!(decoded.dimensions(), original.dimensions());
    assert_eq!(decoded.as_raw(), original.as_raw());
}
