use super::*;

#[test]
fn consuming_owned_pixels_moves_original_color_bytes_without_copying() {
    let bytes = half_bytes(&[[-0.125, 2.5, 0.5, 0.25]]);
    let pointer = bytes.as_ptr();
    let pixels =
        ImagePreviewPixels::from_rgba16f(1, 1, 8, bytes, ImagePreviewColorSpace::ScRgb).unwrap();
    assert_eq!(pixels.color_space(), ImagePreviewColorSpace::ScRgb);
    let owned = pixels.into_bytes();
    assert_eq!(owned.as_ptr(), pointer);

    let borrowed = [17, 70, 180, 127];
    let pixels = ImagePreviewPixels::from_bgra8(1, 1, borrowed.as_slice()).unwrap();
    let owned = pixels.into_bytes();
    assert_eq!(owned, borrowed);
    assert_ne!(owned.as_ptr(), borrowed.as_ptr());
}

#[test]
fn pq_and_sixteen_bit_sdr_keep_high_precision_display_resources() {
    for bytes in [
        include_bytes!("../../tests/fixtures/hdr/pq-gray.png").as_slice(),
        include_bytes!("../../tests/fixtures/hdr/srgb16-gray.png").as_slice(),
    ] {
        let preview =
            ImagePreviewImage::from_encoded(bytes, &ImagePreviewLimits::default(), "16 bit")
                .unwrap();
        let [w, h] = preview.texture_size();
        assert_eq!(
            preview.image().as_bytes(0).unwrap().len(),
            w as usize * h as usize * 8
        );
    }
}

fn floats(bytes: &[u8]) -> Vec<[f32; 4]> {
    bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|p| {
            std::array::from_fn(|c| {
                half::f16::from_bits(u16::from_le_bytes([p[c * 2], p[c * 2 + 1]])).to_f32()
            })
        })
        .collect()
}

fn half_bytes(pixels: &[[f32; 4]]) -> Vec<u8> {
    pixels
        .iter()
        .flatten()
        .flat_map(|v| half::f16::from_f32(*v).to_bits().to_le_bytes())
        .collect()
}

#[test]
fn pq_pixels_have_absolute_nits_and_cicp_precedes_icc() {
    let limits = ImagePreviewLimits::default();
    let plain = ImagePreviewImage::from_encoded(
        include_bytes!("../../tests/fixtures/hdr/pq-gray.png"),
        &limits,
        "PQ",
    )
    .unwrap();
    let tagged = ImagePreviewImage::from_encoded(
        include_bytes!("../../tests/fixtures/hdr/cicp-over-icc.png"),
        &limits,
        "PQ+ICC",
    )
    .unwrap();
    assert_eq!(plain.color_space(), ImagePreviewColorSpace::ScRgb);
    assert_eq!(
        plain.image().format(),
        gpui_kit::RenderImageFormat::Rgba16FloatScRgb
    );
    assert_eq!(plain.image().as_bytes(0), tagged.image().as_bytes(0));
    for (pixel, nits) in floats(plain.image().as_bytes(0).unwrap())
        .iter()
        .zip([0., 80., 100., 203., 1000., 10000.])
    {
        assert!(
            (pixel[0] * 80.0 - nits).abs() <= 0.001 * nits + 0.001,
            "{pixel:?}: {nits}"
        );
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
        assert_eq!(pixel[3], 1.0);
    }
    assert_eq!(plain.image().peak_nits(), Some(10000.0));
}

#[test]
fn low_u16_bits_survive_srgb_and_icc_conversion() {
    for bytes in [
        include_bytes!("../../tests/fixtures/hdr/srgb16-gray.png").as_slice(),
        include_bytes!("../../tests/fixtures/hdr/icc-srgb16.png").as_slice(),
    ] {
        let preview =
            ImagePreviewImage::from_encoded(bytes, &ImagePreviewLimits::default(), "16").unwrap();
        assert_eq!(preview.color_space(), ImagePreviewColorSpace::LinearSrgb);
        assert_eq!(preview.image().peak_nits(), None);
        let p = floats(preview.image().as_bytes(0).unwrap());
        assert!(p[1][0] > p[0][0]);
        assert!(
            (f64::from(p[0][0]) - super::color::srgb_to_linear(4096.0 / 65535.0)).abs() < 0.00002
        );
    }
}

#[test]
fn sdr_icc_uses_its_primaries_and_retains_extended_linear_gamut() {
    use image::ImageEncoder as _;
    let profile = moxcms::ColorProfile::new_display_p3();
    let mut encoded = vec![];
    let mut encoder = image::codecs::png::PngEncoder::new(&mut encoded);
    encoder.set_icc_profile(profile.encode().unwrap()).unwrap();
    encoder
        .write_image(&[255, 0, 0, 127], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    let decoded = ImagePreviewPixels::decode(&encoded, &ImagePreviewLimits::default()).unwrap();
    assert_eq!(decoded.color_space(), ImagePreviewColorSpace::LinearSrgb);
    let pixel = floats(decoded.as_bytes())[0];
    assert!(
        pixel[0] > 1.1 && pixel[1] < -0.01 && pixel[2] < -0.01,
        "{pixel:?}"
    );
    assert!((pixel[3] - 127. / 255.).abs() < 0.001);
}

#[test]
fn unknown_hdr_and_non_rgb_metadata_are_errors() {
    for bytes in [
        include_bytes!("../../tests/fixtures/hdr/unsupported-hlg.png").as_slice(),
        include_bytes!("../../tests/fixtures/hdr/invalid-matrix.png").as_slice(),
    ] {
        assert!(ImagePreviewPixels::decode(bytes, &ImagePreviewLimits::default()).is_err());
    }
}

#[test]
fn float_input_moves_owned_storage_strips_stride_and_rejects_invalid_samples() {
    let bytes = half_bytes(&[[-0.25, 12.5, 2.5375, 0.5]]);
    let pointer = bytes.as_ptr();
    let preview = ImagePreviewPixels::from_rgba16f(1, 1, 8, bytes, ImagePreviewColorSpace::ScRgb)
        .unwrap()
        .into_preview(&ImagePreviewLimits::default(), "float")
        .unwrap();
    assert_eq!(preview.image().as_bytes(0).unwrap().as_ptr(), pointer);
    assert_eq!(
        floats(preview.image().as_bytes(0).unwrap())[0],
        [-0.25, 12.5, 2.5371094, 0.5]
    );
    let mut padded = half_bytes(&[[1., 2., 3., 1.]]);
    padded.extend_from_slice(&[255; 8]);
    let input =
        ImagePreviewPixels::from_rgba16f(1, 1, 16, padded, ImagePreviewColorSpace::ScRgb).unwrap();
    assert_eq!(input.as_bytes().len(), 8);
    assert!(input.into_rgba8().is_err());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 70000.] {
        assert!(
            ImagePreviewPixels::from_rgba16f(
                1,
                1,
                8,
                half_bytes(&[[value, 0., 0., 1.]]),
                ImagePreviewColorSpace::ScRgb
            )
            .is_err()
        );
    }
    for alpha in [-0.1, 1.1] {
        assert!(
            ImagePreviewPixels::from_rgba16f(
                1,
                1,
                8,
                half_bytes(&[[0., 0., 0., alpha]]),
                ImagePreviewColorSpace::ScRgb
            )
            .is_err()
        );
    }
    for (w, h, stride, len) in [
        (0, 1, 8, 8),
        (1, 1, 7, 8),
        (1, 1, 16, 8),
        (1, 2, usize::MAX, 8),
        (u32::MAX, 1, 8, 8),
    ] {
        assert!(
            ImagePreviewPixels::from_rgba16f(
                w,
                h,
                stride,
                vec![0; len],
                ImagePreviewColorSpace::ScRgb
            )
            .is_err()
        );
    }
}

#[test]
fn float_proxy_uses_eight_byte_budget_and_associated_linear_filtering() {
    let pixels = half_bytes(&[[100., 0., 0., 0.], [0., 0., 12.5, 1.]]);
    let preview = ImagePreviewPixels::from_rgba16f(2, 1, 16, pixels, ImagePreviewColorSpace::ScRgb)
        .unwrap()
        .into_preview(&ImagePreviewLimits::default().texture_budget(1, 8), "alpha")
        .unwrap();
    assert_eq!(preview.texture_size(), [1, 1]);
    assert_eq!(
        floats(preview.image().as_bytes(0).unwrap()),
        [[0., 0., 12.5, 0.5]]
    );
    assert_eq!(preview.image().peak_nits(), Some(1000.));
    let pixels = half_bytes(&[[0., 0., 0., 1.], [12.5, 12.5, 12.5, 1.]]);
    let reduced = ImagePreviewPixels::from_rgba16f(2, 1, 16, pixels, ImagePreviewColorSpace::ScRgb)
        .unwrap()
        .into_preview(&ImagePreviewLimits::default().texture_budget(1, 8), "peak")
        .unwrap();
    assert_eq!(
        floats(reduced.image().as_bytes(0).unwrap()),
        [[6.25, 6.25, 6.25, 1.]]
    );
    assert_eq!(reduced.image().peak_nits(), Some(1000.));
    assert_eq!(
        ImagePreviewLimits::default()
            .texture_budget(2, 16)
            .texture_size_for([2, 2], ImagePreviewColorSpace::ScRgb)
            .unwrap(),
        [1, 1]
    );
    assert!(
        ImagePreviewLimits::default()
            .texture_budget(1, 4)
            .texture_size_for([1, 1], ImagePreviewColorSpace::ScRgb)
            .is_err()
    );
    let preview = ImagePreviewPixels::from_rgba8(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255])
        .unwrap()
        .into_preview(
            &ImagePreviewLimits::default().texture_budget(1, 4),
            "linear",
        )
        .unwrap();
    assert_eq!(preview.image().as_bytes(0).unwrap(), [188, 188, 188, 255]);
}

#[test]
fn png_reader_matches_byte_decode_and_obeys_explicit_limits() {
    let encoded = include_bytes!("../../tests/fixtures/hdr/pq-gray.png");
    let a = ImagePreviewPixels::decode(encoded, &ImagePreviewLimits::default()).unwrap();
    let b = ImagePreviewPixels::from_png_reader(
        std::io::Cursor::new(encoded),
        &ImagePreviewLimits::default(),
    )
    .unwrap();
    assert_eq!(a.as_bytes(), b.as_bytes());
    assert!(
        ImagePreviewPixels::from_png_reader(
            std::io::Cursor::new(encoded),
            &ImagePreviewLimits::default().max_encoded_bytes(1)
        )
        .is_err()
    );
    assert!(
        ImagePreviewPixels::decode(encoded, &ImagePreviewLimits::default().max_decode_bytes(47))
            .is_err()
    );
    let sdr = a.into_sdr_rgba8();
    assert_eq!(sdr.len(), 24);
    assert_eq!(&sdr[20..], [255, 255, 255, 255]);
}

#[test]
fn render_image_float_contract_is_not_inferred_from_bytes() {
    use gpui_kit::{RenderImage, RenderImageFormat, size};
    let data = half_bytes(&[[-0.25, 12.5, 0., 0.5]]);
    let pointer = data.as_ptr();
    let image = RenderImage::from_rgba16f(
        size(1.into(), 1.into()),
        data,
        RenderImageFormat::Rgba16FloatScRgb,
    )
    .unwrap();
    assert_eq!(image.as_bytes(0).unwrap().as_ptr(), pointer);
    assert_eq!(image.peak_nits(), Some(1000.));
    assert_eq!(image.frame_count(), 1);
    assert!(image.as_bytes(1).is_none());
    assert_eq!(image.size(1), Default::default());
    for (size_, bytes, format) in [
        (
            size(0.into(), 1.into()),
            vec![],
            RenderImageFormat::Rgba16FloatScRgb,
        ),
        (
            size(1.into(), 1.into()),
            vec![0; 4],
            RenderImageFormat::Rgba16FloatScRgb,
        ),
        (
            size(1.into(), 1.into()),
            vec![0; 8],
            RenderImageFormat::Bgra8Srgb,
        ),
        (
            size(1.into(), 1.into()),
            half_bytes(&[[0., 0., 0., 2.]]),
            RenderImageFormat::Rgba16FloatScRgb,
        ),
    ] {
        assert!(RenderImage::from_rgba16f(size_, bytes, format).is_err());
    }
}

fn png(width: u32, height: u32, pixels: Vec<u8>) -> Vec<u8> {
    let image = image::RgbaImage::from_raw(width, height, pixels).unwrap();
    let mut encoded = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    encoded.into_inner()
}

#[test]
fn pixel_constructors_agree_on_bgra_and_preserve_straight_alpha() {
    let rgba = vec![180, 70, 17, 0, 45, 21, 93, 127];
    let bgra = vec![17, 70, 180, 0, 93, 21, 45, 127];
    let a = ImagePreviewImage::from_rgba8(2, 1, rgba, "rgba").unwrap();
    let b = ImagePreviewImage::from_bgra8(2, 1, bgra.clone(), "bgra").unwrap();
    assert_eq!(a.image().as_bytes(0), Some(bgra.as_slice()));
    assert_eq!(a.image().as_bytes(0), b.image().as_bytes(0));
    assert_eq!(a.pixel_size(), [2, 1]);
    assert_eq!(a.texture_size(), [2, 1]);
    assert!(!a.is_proxy());
}

#[test]
fn owned_native_pixels_move_once_and_borrowed_pixels_remain_unchanged() {
    let pixels = vec![17, 70, 180, 127];
    let pointer = pixels.as_ptr();
    let image = ImagePreviewImage::from_bgra8(1, 1, pixels, "owned").unwrap();
    assert_eq!(image.image().as_bytes(0).unwrap().as_ptr(), pointer);
    let source = [180, 70, 17, 127];
    let borrowed = ImagePreviewImage::from_rgba8(1, 1, source.as_slice(), "borrowed").unwrap();
    assert_eq!(source, [180, 70, 17, 127]);
    assert_eq!(borrowed.image().as_bytes(0).unwrap(), [17, 70, 180, 127]);
}

#[test]
fn pixel_input_rejects_empty_overflow_and_non_tight_buffers() {
    for (width, height, bytes) in [
        (0, 1, 0),
        (1, 0, 0),
        (1, 1, 3),
        (1, 1, 8),
        (u32::MAX, u32::MAX, 0),
    ] {
        assert!(ImagePreviewPixels::from_rgba8(width, height, vec![0; bytes]).is_err());
        assert!(ImagePreviewPixels::from_bgra8(width, height, vec![0; bytes]).is_err());
    }
}

#[test]
fn encoded_roundtrip_preserves_rgba_and_bounds_before_decoding() {
    let rgba = vec![180, 70, 17, 0, 45, 21, 93, 127];
    let encoded = png(2, 1, rgba.clone());
    let limits = ImagePreviewLimits::default();
    let decoded = ImagePreviewPixels::decode(&encoded, &limits).unwrap();
    assert_eq!(decoded.pixel_size(), [2, 1]);
    assert_eq!(decoded.into_rgba8().unwrap(), rgba);
    assert!(ImagePreviewPixels::decode(b"not an image", &limits).is_err());
    assert!(ImagePreviewPixels::decode(&encoded, &limits.clone().max_encoded_bytes(8)).is_err());
    assert!(ImagePreviewPixels::decode(&encoded, &limits.clone().max_pixels(1)).is_err());
    assert!(ImagePreviewPixels::decode(&encoded, &limits.clone().max_source_edge(1)).is_err());
    assert!(ImagePreviewPixels::decode(&encoded, &limits.clone().max_decode_bytes(4)).is_err());
}

#[test]
fn proxy_keeps_source_dimensions_and_obeys_edge_and_byte_budgets() {
    let limits = ImagePreviewLimits::default().texture_budget(32, 256 * 4);
    let original =
        ImagePreviewPixels::from_bgra8(128, 64, [17, 70, 180, 127].repeat(128 * 64)).unwrap();
    let preview = original.into_preview(&limits, "proxy").unwrap();
    assert_eq!(preview.pixel_size(), [128, 64]);
    assert!(preview.is_proxy());
    let [w, h] = preview.texture_size();
    assert!(w <= 32 && h <= 32);
    assert!(u64::from(w) * u64::from(h) * 4 <= 256 * 4);
    assert!(
        preview
            .image()
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|v| *v == [17, 70, 180, 127])
    );
}

#[test]
fn proxy_metadata_must_describe_the_same_full_frame() {
    let image = || ImagePreviewImage::from_rgba8(4, 2, vec![255; 32], "test").unwrap();
    assert!(image().with_source_size([8, 4]).unwrap().is_proxy());
    assert!(image().with_source_size([0, 4]).is_err());
    assert!(image().with_source_size([2, 1]).is_err());
    assert!(image().with_source_size([100, 100]).is_err());
}

#[test]
fn extreme_aspect_ratios_fit_the_texture_budget_without_zero_axes() {
    assert!(
        ImagePreviewLimits::default()
            .max_source_edge(i32::MAX as u32)
            .max_pixels(u64::MAX)
            .max_decode_bytes(u64::MAX)
            .texture_size_for([i32::MAX as u32; 2], ImagePreviewColorSpace::ScRgb)
            .is_err()
    );
    let limits = ImagePreviewLimits::default().texture_budget(1024, 4 * 1024);
    for size in [[1, 32768], [32768, 1], [8000, 8000], [3840, 2160]] {
        let [w, h] = limits.texture_size(size).unwrap();
        assert!(w > 0 && h > 0 && w <= 1024 && h <= 1024);
        assert!(u64::from(w) * u64::from(h) <= 1024);
    }
    assert!(
        ImagePreviewLimits::default()
            .texture_budget(0, 4)
            .texture_size([1, 1])
            .is_err()
    );
    assert!(
        ImagePreviewLimits::default()
            .texture_budget(1, 3)
            .texture_size([1, 1])
            .is_err()
    );
}

#[test]
fn decoding_applies_exif_orientation_before_reporting_dimensions() {
    use image::ImageEncoder as _;
    // Little-endian TIFF with a single SHORT Orientation=6 (90° clockwise).
    let exif = vec![
        b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let rgba = [
        [1, 0, 0, 255],
        [2, 0, 0, 255],
        [3, 0, 0, 255],
        [4, 0, 0, 255],
        [5, 0, 0, 255],
        [6, 0, 0, 255],
    ]
    .concat();
    let mut bytes = vec![];
    let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder.set_exif_metadata(exif).unwrap();
    encoder
        .write_image(&rgba, 2, 3, image::ExtendedColorType::Rgba8)
        .unwrap();
    let decoded = ImagePreviewPixels::decode(&bytes, &ImagePreviewLimits::default()).unwrap();
    assert_eq!(decoded.pixel_size(), [3, 2]);
    let order: Vec<_> = decoded
        .into_rgba8()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0])
        .collect();
    assert_eq!(order, [5, 3, 1, 6, 4, 2]);
}

#[test]
fn malformed_icc_is_rejected_instead_of_publishing_unmanaged_pixels() {
    use image::ImageEncoder as _;
    let mut bytes = vec![];
    let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder.set_icc_profile(vec![0; 128]).unwrap();
    encoder
        .write_image(&[1, 2, 3, 127], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    assert!(
        ImagePreviewImage::from_encoded(&bytes, &ImagePreviewLimits::default(), "icc").is_err()
    );
}

#[test]
fn animated_containers_are_rejected_instead_of_showing_a_static_first_frame() {
    // Container chunks only; rejection must precede attempting to decode pixels.
    for bytes in [
        b"\x89PNG\r\n\x1a\n\0\0\0\x08acTL".as_slice(),
        b"RIFF\0\0\0\0WEBPANIM\0\0\0\0",
    ] {
        assert_eq!(
            ImagePreviewPixels::decode(bytes, &ImagePreviewLimits::default()).unwrap_err(),
            ImagePreviewError::InvalidImage
        );
    }
}

#[test]
fn proxy_resampling_does_not_mix_hidden_transparent_rgb_into_visible_edges() {
    let pixels = ImagePreviewPixels::from_rgba8(2, 1, vec![255, 0, 0, 0, 0, 0, 255, 255]).unwrap();
    let preview = pixels
        .into_preview(
            &ImagePreviewLimits::default().texture_budget(1, 4),
            "alpha edge",
        )
        .unwrap();
    assert_eq!(preview.image().as_bytes(0).unwrap(), [255, 0, 0, 128]);
}
