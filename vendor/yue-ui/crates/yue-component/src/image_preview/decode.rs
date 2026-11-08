//! Bounded preparation: no full-frame F32 copy for PNG. File I/O and task
//! cancellation remain the host's responsibility.
use super::*;
use image::metadata::Orientation;
use moxcms::{ColorProfile, Layout, TransformF32Executor, TransformOptions};

enum Conversion {
    Srgb,
    Pq2020,
    Icc(Arc<TransformF32Executor>),
}
impl Conversion {
    fn from_cicp([primaries, transfer, matrix, full]: [u8; 4]) -> Result<Self, ImagePreviewError> {
        match [primaries, transfer, matrix, full] {
            [1, 13, 0, 1] => Ok(Self::Srgb),
            [9, 16, 0, 1] => Ok(Self::Pq2020),
            values => Err(decode_error(format!(
                "unsupported CICP {values:?}; expected full-range RGB sRGB or Rec.2020/PQ"
            ))),
        }
    }
    fn from_icc(bytes: &[u8]) -> Result<Self, ImagePreviewError> {
        let source = ColorProfile::new_from_slice(bytes).map_err(decode_error)?;
        Self::from_profile(source)
    }
    fn from_profile(source: ColorProfile) -> Result<Self, ImagePreviewError> {
        if source.color_space != moxcms::DataColorSpace::Rgb
            || source.red_trc.is_none()
            || source.green_trc.is_none()
            || source.blue_trc.is_none()
            || source.lut_a_to_b_perceptual.is_some()
            || source.lut_a_to_b_colorimetric.is_some()
            || source.lut_a_to_b_saturation.is_some()
        {
            return Err(decode_error(
                "only validated RGB matrix/TRC ICC profiles are supported; LUT/other models require separate validation",
            ));
        }
        // An ICC luminance tag or profile name alone does not identify HDR.
        if let Some(cicp) = source.cicp {
            if cicp.transfer_characteristics as u8 == 16 {
                return Self::from_cicp([
                    cicp.color_primaries as u8,
                    cicp.transfer_characteristics as u8,
                    cicp.matrix_coefficients as u8,
                    u8::from(cicp.full_range),
                ]);
            }
            if !matches!(
                cicp.transfer_characteristics as u8,
                1 | 4 | 5 | 6 | 7 | 8 | 13
            ) {
                return Err(decode_error("unsupported ICC transfer characteristics"));
            }
        }
        let options = TransformOptions {
            prefer_fixed_point: false,
            allow_extended_range_rgb_xyz: true,
            ..TransformOptions::default()
        };
        let transform = source
            .create_transform_f32(
                Layout::Rgba,
                &color::linear_srgb_profile(),
                Layout::Rgba,
                options,
            )
            .map_err(decode_error)?;
        Ok(Self::Icc(transform))
    }
    fn encoding(&self, sixteen_bit: bool) -> ImagePreviewColorSpace {
        match self {
            Self::Srgb if !sixteen_bit => ImagePreviewColorSpace::Srgb,
            Self::Pq2020 => ImagePreviewColorSpace::ScRgb,
            _ => ImagePreviewColorSpace::LinearSrgb,
        }
    }
    fn row(&self, rgba: &mut [f32]) -> Result<(), ImagePreviewError> {
        match self {
            Self::Srgb => {
                for pixel in rgba.as_chunks_mut::<4>().0 {
                    for channel in &mut pixel[..3] {
                        *channel = color::srgb_to_linear(f64::from(*channel)) as f32;
                    }
                }
            }
            Self::Pq2020 => {
                for pixel in rgba.as_chunks_mut::<4>().0 {
                    let rgb = color::rec2020_to_rec709(
                        [pixel[0], pixel[1], pixel[2]]
                            .map(|v| color::pq_to_nits(f64::from(v)) / 80.0),
                    );
                    for (channel, value) in pixel[..3].iter_mut().zip(rgb) {
                        *channel = value as f32;
                    }
                }
            }
            Self::Icc(transform) => {
                let input = rgba.to_vec(); // One row, not another full frame.
                transform.transform(&input, rgba).map_err(decode_error)?;
                // Alpha is coverage, never color-managed.
                for (source, result) in input
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(rgba.as_chunks_mut::<4>().0)
                {
                    result[3] = source[3];
                }
            }
        }
        Ok(())
    }
}

pub(super) fn managed_image(
    image: image::DynamicImage,
    icc: &[u8],
) -> Result<ImagePreviewPixels<'static>, ImagePreviewError> {
    let conversion = Conversion::from_icc(icc)?;
    let encoding = conversion.encoding(true);
    let width = image.width();
    let height = image.height();
    let rgba = image.into_rgba16();
    let mut output = Vec::with_capacity(pixel_bytes(width, height)? * 2);
    let mut row = Vec::with_capacity(width as usize * 4);
    for source in rgba.as_raw().chunks_exact(width as usize * 4) {
        row.clear();
        row.extend(source.iter().map(|v| f32::from(*v) / 65535.0));
        conversion.row(&mut row)?;
        append_float(&row, &mut output)?;
    }
    ImagePreviewPixels::from_rgba16f(width, height, width as usize * 8, output, encoding)
}

fn append_float(values: &[f32], output: &mut Vec<u8>) -> Result<(), ImagePreviewError> {
    for value in values {
        let half = half::f16::from_f32(*value);
        if !half.is_finite() {
            return Err(decode_error("color conversion exceeds finite FP16 range"));
        }
        output.extend_from_slice(&half.to_bits().to_le_bytes());
    }
    Ok(())
}

pub(super) fn png(
    reader: impl std::io::BufRead + std::io::Seek,
    limits: &ImagePreviewLimits,
) -> Result<ImagePreviewPixels<'static>, ImagePreviewError> {
    let mut decoder = png::Decoder::new(reader);
    let decoder_budget = limits.decode_bytes.min(8 * 1024 * 1024) as usize;
    decoder.set_limits(png::Limits {
        bytes: decoder_budget,
    });
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(decode_error)?;
    let info = reader.info();
    if info.animation_control.is_some() {
        return Err(ImagePreviewError::InvalidImage);
    }
    let [width, height] = [info.width, info.height];
    limits.check_source([width, height])?;
    let orientation = info
        .exif_metadata
        .as_deref()
        .and_then(Orientation::from_exif_chunk)
        .unwrap_or(Orientation::NoTransforms);
    let conversion = if let Some(cicp) = info.coding_independent_code_points {
        // PNG Third Edition: real cICP precedes iCCP. The decoder's default
        // sRGB interpretation is not evidence that the file contains cICP.
        Conversion::from_cicp([
            cicp.color_primaries,
            cicp.transfer_function,
            cicp.matrix_coefficients,
            u8::from(cicp.is_video_full_range_image),
        ])?
    } else if let Some(icc) = info.icc_profile.as_deref() {
        Conversion::from_icc(icc)?
    } else if info.srgb.is_some()
        || (info.source_gamma.is_none() && info.source_chromaticities.is_none())
    {
        Conversion::Srgb
    } else {
        return Err(decode_error(
            "PNG gAMA/cHRM without supported sRGB/cICP/ICC is not yet supported",
        ));
    };
    let (channels, depth) = reader.output_color_type();
    let encoding = conversion.encoding(depth == png::BitDepth::Sixteen);
    let bpp = encoding.format().bytes_per_pixel();
    let length = (pixel_bytes(width, height)? / 4)
        .checked_mul(bpp)
        .ok_or(ImagePreviewError::InvalidPixels)?;
    // Output, two F32 conversion rows and the decoder's bounded row working set.
    let working = width as u64 * 64;
    let interlaced = reader.info().interlaced;
    let frame_bytes = if interlaced {
        reader
            .output_buffer_size()
            .ok_or(ImagePreviewError::InvalidPixels)?
    } else {
        0
    };
    let orientation_bytes = if orientation == Orientation::NoTransforms {
        0
    } else {
        length as u64
    };
    if (length as u64)
        .saturating_add(working)
        .saturating_add(frame_bytes as u64)
        .saturating_add(orientation_bytes)
        .saturating_add(decoder_budget as u64)
        > limits.decode_bytes
    {
        return Err(ImagePreviewError::ResourceLimit(
            "PNG conversion working set exceeds decoding budget".into(),
        ));
    }
    let mut output = Vec::with_capacity(length);
    let mut row = Vec::with_capacity(width as usize * 4);
    let mut convert = |source: &[u8]| -> Result<(), ImagePreviewError> {
        if depth != png::BitDepth::Eight && depth != png::BitDepth::Sixteen {
            return Err(decode_error("unexpanded PNG bit depth"));
        }
        row.clear();
        let samples = channels.samples();
        let bytes_per_sample = if depth == png::BitDepth::Sixteen {
            2
        } else {
            1
        };
        for pixel in source.chunks_exact(samples * bytes_per_sample) {
            let sample = |channel| {
                if bytes_per_sample == 2 {
                    f32::from(u16::from_be_bytes([
                        pixel[channel * 2],
                        pixel[channel * 2 + 1],
                    ])) / 65535.0
                } else {
                    f32::from(pixel[channel]) / 255.0
                }
            };
            let rgba = match channels {
                png::ColorType::Grayscale => [sample(0), sample(0), sample(0), 1.0],
                png::ColorType::GrayscaleAlpha => [sample(0), sample(0), sample(0), sample(1)],
                png::ColorType::Rgb => [sample(0), sample(1), sample(2), 1.0],
                png::ColorType::Rgba => [sample(0), sample(1), sample(2), sample(3)],
                png::ColorType::Indexed => return Err(decode_error("unexpanded PNG palette")),
            };
            row.extend_from_slice(&rgba);
        }
        if encoding == ImagePreviewColorSpace::Srgb {
            output.extend(row.iter().map(|v| (v * 255.0).round() as u8));
        } else {
            conversion.row(&mut row)?;
            append_float(&row, &mut output)?;
        }
        Ok(())
    };
    if interlaced {
        let mut frame = vec![0; frame_bytes];
        let result = reader.next_frame(&mut frame).map_err(decode_error)?;
        for source in frame[..result.buffer_size()].chunks_exact(result.line_size) {
            convert(source)?;
        }
    } else {
        while let Some(source) = reader.next_row().map_err(decode_error)? {
            convert(source.data())?;
        }
    }
    reader.finish().map_err(decode_error)?;
    if output.len() != length {
        return Err(decode_error("PNG row count does not match dimensions"));
    }
    let (size, output) = orient([width, height], output, bpp, orientation);
    if encoding == ImagePreviewColorSpace::Srgb {
        ImagePreviewPixels::from_rgba8(size[0], size[1], output)
    } else {
        ImagePreviewPixels::from_rgba16f(size[0], size[1], size[0] as usize * 8, output, encoding)
    }
}

fn orient(
    [w, h]: [u32; 2],
    bytes: Vec<u8>,
    bpp: usize,
    orientation: Orientation,
) -> ([u32; 2], Vec<u8>) {
    if orientation == Orientation::NoTransforms {
        return ([w, h], bytes);
    }
    let rotated = matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    );
    let size = if rotated { [h, w] } else { [w, h] };
    let mut output = vec![0; bytes.len()];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = match orientation {
                Orientation::NoTransforms => (x, y),
                Orientation::Rotate90 => (h - 1 - y, x),
                Orientation::Rotate180 => (w - 1 - x, h - 1 - y),
                Orientation::Rotate270 => (y, w - 1 - x),
                Orientation::FlipHorizontal => (w - 1 - x, y),
                Orientation::FlipVertical => (x, h - 1 - y),
                Orientation::Rotate90FlipH => (y, x),
                Orientation::Rotate270FlipH => (h - 1 - y, w - 1 - x),
            };
            let src = (y as usize * w as usize + x as usize) * bpp;
            let dst = (dy as usize * size[0] as usize + dx as usize) * bpp;
            output[dst..dst + bpp].copy_from_slice(&bytes[src..src + bpp]);
        }
    }
    (size, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icc_rejects_a_missing_channel_curve_before_color_conversion() {
        let mut profile = ColorProfile::new_srgb();
        profile.blue_trc = None;
        assert!(Conversion::from_profile(profile).is_err());
    }
}
