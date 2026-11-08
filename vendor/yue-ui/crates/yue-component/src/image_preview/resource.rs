//! CPU-only static raster preparation. Run decoding/resizing on the host's
//! background executor; this module owns neither I/O nor a GPUI task/cache.
use super::{ImagePreviewError, color, geometry::aligned};
use gpui_kit::{RenderImage, RenderImageFormat, SharedString};
use image::ImageDecoder as _;
use std::{borrow::Cow, io::Cursor, sync::Arc};

#[path = "decode.rs"]
mod decode;

/// Prepared pixel encoding, independent of the monitor and source file tags.
/// Alpha is straight (unassociated) in every encoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImagePreviewColorSpace {
    #[default]
    Srgb,
    /// Linear Rec.709 RGB relative to SDR white; preserves high precision.
    LinearSrgb,
    /// Absolute linear Rec.709 scRGB: 1.0 = 80 nit, negative RGB is valid.
    ScRgb,
}

impl ImagePreviewColorSpace {
    fn format(self) -> RenderImageFormat {
        match self {
            Self::Srgb => RenderImageFormat::Bgra8Srgb,
            Self::LinearSrgb => RenderImageFormat::Rgba16FloatLinearSrgb,
            Self::ScRgb => RenderImageFormat::Rgba16FloatScRgb,
        }
    }
}

/// Per-image preparation budgets, not a process/VRAM ceiling. Hosts also own a
/// total live-image budget and may choose tighter limits for their use case.
#[derive(Clone, Debug)]
pub struct ImagePreviewLimits {
    encoded_bytes: usize,
    pixels: u64,
    source_edge: u32,
    decode_bytes: u64,
    texture_edge: u32,
    texture_bytes: u64,
}
impl Default for ImagePreviewLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: 64 * 1024 * 1024,
            pixels: 64_000_000,
            source_edge: 32768,
            decode_bytes: 512 * 1024 * 1024,
            texture_edge: 8192,
            texture_bytes: 64 * 1024 * 1024,
        }
    }
}
impl ImagePreviewLimits {
    pub fn max_encoded_bytes(mut self, bytes: usize) -> Self {
        self.encoded_bytes = bytes;
        self
    }
    pub fn max_pixels(mut self, pixels: u64) -> Self {
        self.pixels = pixels;
        self
    }
    pub fn max_source_edge(mut self, edge: u32) -> Self {
        self.source_edge = edge;
        self
    }
    pub fn max_decode_bytes(mut self, bytes: u64) -> Self {
        self.decode_bytes = bytes;
        self
    }
    /// Bound both texture axes and actual storage bytes. An oversized source becomes an
    /// explicitly marked proxy; it is not rejected solely for its texture size.
    /// The edge is capped at 8192 (below the Windows atlas's physical ceiling).
    pub fn texture_budget(mut self, edge: u32, bytes: u64) -> Self {
        self.texture_edge = edge;
        self.texture_bytes = bytes;
        self
    }
    fn validate(&self) -> Result<(), ImagePreviewError> {
        if self.encoded_bytes == 0
            || self.pixels == 0
            || self.source_edge == 0
            || self.source_edge > i32::MAX as u32
            || self.decode_bytes < 4
            || self.texture_edge == 0
            || self.texture_edge > 8192
            || self.texture_bytes < 4
        {
            return Err(ImagePreviewError::InvalidLimits);
        }
        Ok(())
    }
    fn check_source(&self, [w, h]: [u32; 2]) -> Result<(), ImagePreviewError> {
        self.validate()?;
        let bytes = pixel_bytes(w, h)? as u64;
        if w > self.source_edge || h > self.source_edge {
            return Err(ImagePreviewError::ResourceLimit(format!(
                "source edge exceeds {}",
                self.source_edge
            )));
        }
        if u64::from(w) * u64::from(h) > self.pixels {
            return Err(ImagePreviewError::ResourceLimit(format!(
                "decoded pixels exceed {}",
                self.pixels
            )));
        }
        if bytes > self.decode_bytes {
            return Err(ImagePreviewError::ResourceLimit(
                "RGBA buffer exceeds decoding budget".into(),
            ));
        }
        Ok(())
    }
    /// Planned texture dimensions; useful for resource accounting before any
    /// allocation. Downsampling is bounded by edge AND bytes, including 1px axes.
    pub fn texture_size(&self, source: [u32; 2]) -> Result<[u32; 2], ImagePreviewError> {
        self.texture_size_for(source, ImagePreviewColorSpace::Srgb)
    }
    /// Plan a texture using its actual four- or eight-byte encoding.
    pub fn texture_size_for(
        &self,
        source: [u32; 2],
        encoding: ImagePreviewColorSpace,
    ) -> Result<[u32; 2], ImagePreviewError> {
        self.check_source(source)?;
        let bpp = encoding.format().bytes_per_pixel() as u64;
        let source_bytes = (pixel_bytes(source[0], source[1])? as u64 / 4)
            .checked_mul(bpp)
            .ok_or(ImagePreviewError::InvalidPixels)?;
        if self.texture_bytes < bpp || source_bytes > self.decode_bytes {
            return Err(ImagePreviewError::ResourceLimit(
                "pixel encoding exceeds byte budget".into(),
            ));
        }
        let [w, h] = source.map(f64::from);
        let max_pixels = self.texture_bytes / bpp;
        let ratio = (f64::from(self.texture_edge) / w)
            .min(f64::from(self.texture_edge) / h)
            .min((max_pixels as f64 / (w * h)).sqrt())
            .min(1.);
        let mut size = source.map(|n| (f64::from(n) * ratio).floor().max(1.) as u32);
        // Clamping a very thin axis to 1 can invalidate the area ratio.
        if u64::from(size[0]) * u64::from(size[1]) > max_pixels {
            let axis = usize::from(size[1] > size[0]);
            size[axis] = (max_pixels / u64::from(size[1 - axis])).max(1) as u32;
        }
        Ok(size)
    }
}

fn pixel_bytes(w: u32, h: u32) -> Result<usize, ImagePreviewError> {
    if w == 0 || h == 0 || w > i32::MAX as u32 || h > i32::MAX as u32 {
        return Err(ImagePreviewError::InvalidPixels);
    }
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(4))
        .ok_or(ImagePreviewError::InvalidPixels)
}

/// Tightly packed RGBA8/BGRA8 or RGBA16F pixels, owned or borrowed. A CPU boundary for callers
/// needing the original pixels for copy/export as well as a preview texture.
/// No stride padding; raw inputs are already oriented, use the declared encoding
/// and carry straight alpha.
#[derive(Debug)]
pub struct ImagePreviewPixels<'a> {
    size: [u32; 2],
    bytes: Cow<'a, [u8]>,
    bgra: bool,
    color_space: ImagePreviewColorSpace,
    peak_nits: Option<f32>,
}
impl<'a> ImagePreviewPixels<'a> {
    pub fn from_rgba8(
        w: u32,
        h: u32,
        bytes: impl Into<Cow<'a, [u8]>>,
    ) -> Result<Self, ImagePreviewError> {
        Self::from_pixels(w, h, bytes.into(), false)
    }
    pub fn from_bgra8(
        w: u32,
        h: u32,
        bytes: impl Into<Cow<'a, [u8]>>,
    ) -> Result<Self, ImagePreviewError> {
        Self::from_pixels(w, h, bytes.into(), true)
    }
    fn from_pixels(
        w: u32,
        h: u32,
        bytes: Cow<'a, [u8]>,
        bgra: bool,
    ) -> Result<Self, ImagePreviewError> {
        if pixel_bytes(w, h)? != bytes.len() {
            return Err(ImagePreviewError::InvalidPixels);
        }
        Ok(Self {
            size: [w, h],
            bytes,
            bgra,
            color_space: ImagePreviewColorSpace::Srgb,
            peak_nits: None,
        })
    }
    /// Absolute scRGB or relative linear SDR, little-endian FP16, straight alpha.
    /// A padded source row is copied once into tight storage; tight owned input
    /// moves without a copy. Finite RGB (including negative/>1) and alpha [0,1]
    /// are required. The final row must include the declared stride padding.
    pub fn from_rgba16f(
        w: u32,
        h: u32,
        row_stride: usize,
        bytes: impl Into<Cow<'a, [u8]>>,
        encoding: ImagePreviewColorSpace,
    ) -> Result<Self, ImagePreviewError> {
        let length = pixel_bytes(w, h)?
            .checked_mul(2)
            .ok_or(ImagePreviewError::InvalidPixels)?;
        let row = (w as usize)
            .checked_mul(8)
            .ok_or(ImagePreviewError::InvalidPixels)?;
        let bytes = bytes.into();
        if encoding == ImagePreviewColorSpace::Srgb
            || row_stride < row
            || row_stride.checked_mul(h as usize) != Some(bytes.len())
        {
            return Err(ImagePreviewError::InvalidPixels);
        }
        let bytes = if row_stride == row {
            bytes
        } else {
            let mut tight = Vec::with_capacity(length);
            for source in bytes.chunks_exact(row_stride) {
                tight.extend_from_slice(&source[..row]);
            }
            Cow::Owned(tight)
        };
        let mut peak = 0_f32;
        for pixel in bytes.as_chunks::<8>().0 {
            let mut rgb_peak = 0_f32;
            let mut visible = true;
            for (channel, value) in pixel.as_chunks::<2>().0.iter().enumerate() {
                let value = half::f16::from_bits(u16::from_le_bytes(*value)).to_f32();
                if !value.is_finite() || (channel == 3 && !(0.0..=1.0).contains(&value)) {
                    return Err(ImagePreviewError::InvalidPixels);
                }
                if channel < 3 {
                    rgb_peak = rgb_peak.max(value);
                } else {
                    visible = value > 0.;
                }
            }
            if visible {
                peak = peak.max(rgb_peak);
            }
        }
        Ok(Self {
            size: [w, h],
            bytes,
            bgra: false,
            color_space: encoding,
            peak_nits: (encoding == ImagePreviewColorSpace::ScRgb).then_some(peak * 80.),
        })
    }
    /// Decode static PNG/JPEG/WebP/BMP bytes on a worker. EXIF orientation is
    /// applied before publishing dimensions. Recognized PQ is decoded to absolute
    /// scRGB; SDR ICC is converted to linear Rec.709. Unsupported color metadata
    /// is an error, never an unmanaged success relabeled as sRGB.
    pub fn decode(bytes: &[u8], limits: &ImagePreviewLimits) -> Result<Self, ImagePreviewError> {
        limits.validate()?;
        if bytes.len() > limits.encoded_bytes {
            return Err(ImagePreviewError::ResourceLimit(
                "encoded bytes exceed loading budget".into(),
            ));
        }
        reject_animation(bytes)?;
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return decode::png(Cursor::new(bytes), limits);
        }
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(decode_error)?;
        if !matches!(
            reader.format(),
            Some(
                image::ImageFormat::Png
                    | image::ImageFormat::Jpeg
                    | image::ImageFormat::WebP
                    | image::ImageFormat::Bmp
            )
        ) {
            return Err(ImagePreviewError::Decode(
                "supported static formats: PNG, JPEG, WebP, BMP".into(),
            ));
        }
        let mut decode_limits = image::Limits::default();
        decode_limits.max_alloc = Some(limits.decode_bytes);
        decode_limits.max_image_width = Some(limits.source_edge);
        decode_limits.max_image_height = Some(limits.source_edge);
        // In particular PNG must receive limits BEFORE constructing its decoder.
        reader.limits(decode_limits);
        let mut decoder = reader.into_decoder().map_err(decode_error)?;
        let (w, h) = decoder.dimensions();
        limits.check_source([w, h])?;
        if decoder.total_bytes() > limits.decode_bytes {
            return Err(ImagePreviewError::ResourceLimit(
                "decoded buffer exceeds decoding budget".into(),
            ));
        }
        let orientation = decoder.orientation().map_err(decode_error)?;
        let icc = decoder.icc_profile().map_err(decode_error)?;
        let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(decode_error)?;
        decoded.apply_orientation(orientation);
        if let Some(icc) = icc {
            // Decoder allocation and output coexist. Do not count only RGBA8.
            if u64::from(w)
                .checked_mul(u64::from(h))
                .and_then(|n| n.checked_mul(16))
                .is_none_or(|bytes| bytes > limits.decode_bytes)
            {
                return Err(ImagePreviewError::ResourceLimit(
                    "ICC conversion working set exceeds decoding budget".into(),
                ));
            }
            return decode::managed_image(decoded, &icc);
        }
        if decoded.color_space() != image::metadata::Cicp::SRGB {
            return Err(decode_error("unsupported color metadata"));
        }
        let rgba = decoded.into_rgba8();
        Self::from_rgba8(rgba.width(), rgba.height(), rgba.into_raw())
    }
    pub fn pixel_size(&self) -> [u32; 2] {
        self.size
    }
    /// Bounded PNG reader entry for files larger than the default encoded-byte
    /// limit. The host opens the file and explicitly supplies a budget. Ordinary
    /// PNG decodes row by row; interlaced PNG reserves a bounded encoded frame.
    pub fn from_png_reader(
        mut reader: impl std::io::BufRead + std::io::Seek,
        limits: &ImagePreviewLimits,
    ) -> Result<Self, ImagePreviewError> {
        limits.validate()?;
        let length = reader
            .seek(std::io::SeekFrom::End(0))
            .map_err(decode_error)?;
        if length > limits.encoded_bytes as u64 {
            return Err(ImagePreviewError::ResourceLimit(
                "encoded bytes exceed loading budget".into(),
            ));
        }
        reader
            .seek(std::io::SeekFrom::Start(0))
            .map_err(decode_error)?;
        decode::png(reader, limits)
    }
    pub fn color_space(&self) -> ImagePreviewColorSpace {
        self.color_space
    }
    /// Tight source bytes. `is_bgra8` defines byte order; `color_space` defines
    /// transfer/white and sample width. Never assume four-byte RGBA.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consume the original bytes without changing their channel order or color
    /// space. Owned decode results move without copying; borrowed inputs copy.
    /// Read `color_space`, `is_bgra8` and `pixel_size` before consuming this value.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.into_owned()
    }
    /// Whether these are BGRA8 bytes. Every other prepared buffer is RGBA.
    pub fn is_bgra8(&self) -> bool {
        self.bgra
    }
    /// Return lossless RGBA8 only. Float resources require an explicit SDR
    /// derivative (`into_sdr_rgba8`); implicit HDR truncation is rejected.
    pub fn into_rgba8(self) -> Result<Vec<u8>, ImagePreviewError> {
        if self.color_space != ImagePreviewColorSpace::Srgb {
            return Err(decode_error(
                "high-precision pixels require explicit SDR conversion",
            ));
        }
        let mut bytes = self.bytes.into_owned();
        if self.bgra {
            swap_channels(&mut bytes);
        }
        Ok(bytes)
    }
    /// Explicit 8-bit sRGB derivative. Absolute HDR uses the frozen BT.2390
    /// zero-black SDR policy; the caller retains the original for HDR export.
    pub fn into_sdr_rgba8(self) -> Vec<u8> {
        if self.color_space == ImagePreviewColorSpace::Srgb {
            return self.into_rgba8().unwrap();
        }
        let peak = f64::from(self.peak_nits.unwrap_or(0.));
        let mut output = Vec::with_capacity(self.bytes.len() / 2);
        for pixel in self.bytes.as_chunks::<8>().0 {
            let values: [f64; 4] = std::array::from_fn(|c| {
                f64::from(
                    half::f16::from_bits(u16::from_le_bytes([pixel[c * 2], pixel[c * 2 + 1]]))
                        .to_f32(),
                )
            });
            let rgb = if self.color_space == ImagePreviewColorSpace::ScRgb {
                color::scrgb_to_sdr([values[0], values[1], values[2]], peak)
            } else {
                [values[0], values[1], values[2]]
            };
            output.extend(
                rgb.map(|v| (color::linear_to_srgb(v).clamp(0.0, 1.0) * 255.0).round() as u8),
            );
            output.push((values[3] * 255.0).round() as u8);
        }
        output
    }
    /// Prepare a bounded texture, sharing the returned Arc across every frame.
    /// Owned buffers move without copying. Borrowed native-size buffers copy once;
    /// borrowed proxies allocate only output pixels, never a full-source copy.
    pub fn into_preview(
        self,
        limits: &ImagePreviewLimits,
        description: impl Into<SharedString>,
    ) -> Result<ImagePreviewImage, ImagePreviewError> {
        let target = limits.texture_size_for(self.size, self.color_space)?;
        let mut bytes = if target == self.size {
            self.bytes.into_owned()
        } else {
            thumbnail(&self.bytes, self.size, target, self.color_space)
        };
        if self.color_space != ImagePreviewColorSpace::Srgb {
            let image = RenderImage::from_rgba16f_with_source_peak(
                gpui_kit::size(target[0].into(), target[1].into()),
                bytes,
                self.color_space.format(),
                self.peak_nits,
            )
            .map_err(decode_error)?;
            return ImagePreviewImage::new(Arc::new(image), description)?
                .with_source_size(self.size);
        }
        if !self.bgra {
            swap_channels(&mut bytes);
        }
        let buffer = image::RgbaImage::from_raw(target[0], target[1], bytes)
            .ok_or(ImagePreviewError::InvalidPixels)?;
        let mut preview = ImagePreviewImage::new(
            Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
            description,
        )?
        .with_source_size(self.size)?;
        preview.color_space = self.color_space;
        Ok(preview)
    }
}
fn swap_channels(bytes: &mut [u8]) {
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
}
// Alpha-weighted area sampling in linear color. Unlike channel-independent
// averaging, invisible RGB cannot stain a transparent edge. No full-source
// premultiplied/floating-point copy is allocated; only the output is owned.
fn thumbnail(
    source: &[u8],
    [w, h]: [u32; 2],
    [width, height]: [u32; 2],
    encoding: ImagePreviewColorSpace,
) -> Vec<u8> {
    let bpp = encoding.format().bytes_per_pixel();
    let mut output = vec![0; width as usize * height as usize * bpp];
    let step_x = f64::from(w) / f64::from(width);
    let step_y = f64::from(h) / f64::from(height);
    for y in 0..height {
        let top = f64::from(y) * step_y;
        let bottom = f64::from(y + 1) * step_y;
        for x in 0..width {
            let left = f64::from(x) * step_x;
            let right = f64::from(x + 1) * step_x;
            let mut sum = [0.; 4];
            for sy in top.floor() as u32..(bottom.ceil() as u32).min(h) {
                let wy = (bottom.min(f64::from(sy + 1)) - top.max(f64::from(sy))).max(0.);
                for sx in left.floor() as u32..(right.ceil() as u32).min(w) {
                    let weight =
                        wy * (right.min(f64::from(sx + 1)) - left.max(f64::from(sx))).max(0.);
                    let ix = (sy as usize * w as usize + sx as usize) * bpp;
                    let values = if bpp == 8 {
                        std::array::from_fn(|c| {
                            f64::from(
                                half::f16::from_bits(u16::from_le_bytes([
                                    source[ix + c * 2],
                                    source[ix + c * 2 + 1],
                                ]))
                                .to_f32(),
                            )
                        })
                    } else {
                        [
                            color::srgb_to_linear(f64::from(source[ix]) / 255.0),
                            color::srgb_to_linear(f64::from(source[ix + 1]) / 255.0),
                            color::srgb_to_linear(f64::from(source[ix + 2]) / 255.0),
                            f64::from(source[ix + 3]) / 255.0,
                        ]
                    };
                    let alpha = values[3] * weight;
                    for channel in 0..3 {
                        sum[channel] += values[channel] * alpha;
                    }
                    sum[3] += alpha;
                }
            }
            let ix = (y as usize * width as usize + x as usize) * bpp;
            if sum[3] > 0. {
                let alpha = sum[3] / (step_x * step_y);
                for channel in 0..3 {
                    let linear = sum[channel] / sum[3];
                    if bpp == 8 {
                        output[ix + channel * 2..ix + channel * 2 + 2]
                            .copy_from_slice(&half::f16::from_f64(linear).to_bits().to_le_bytes());
                    } else {
                        output[ix + channel] =
                            (color::linear_to_srgb(linear).clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
                if bpp == 8 {
                    output[ix + 6..ix + 8]
                        .copy_from_slice(&half::f16::from_f64(alpha).to_bits().to_le_bytes());
                } else {
                    output[ix + 3] = (alpha * 255.0).round() as u8;
                }
            }
        }
    }
    output
}
fn decode_error(error: impl std::fmt::Display) -> ImagePreviewError {
    ImagePreviewError::Decode(error.to_string())
}

// Check actual container chunks, not an accidental signature in compressed
// pixel bytes. Do not silently turn an animated file into a static first frame.
fn reject_animation(bytes: &[u8]) -> Result<(), ImagePreviewError> {
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    let webp = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");
    if !png && !webp {
        return Ok(());
    }
    let mut offset: usize = if png { 8 } else { 12 };
    while let Some(header) = bytes.get(offset..).and_then(|b| b.get(..8)) {
        let length = if png {
            u32::from_be_bytes(header[..4].try_into().unwrap())
        } else {
            u32::from_le_bytes(header[4..8].try_into().unwrap())
        } as usize;
        let kind = if png { &header[4..8] } else { &header[..4] };
        if kind == b"acTL" || kind == b"ANIM" || kind == b"ANMF" {
            return Err(ImagePreviewError::InvalidImage);
        }
        let overhead = if png { 12 } else { 8 + length % 2 };
        let Some(next) = offset
            .checked_add(length)
            .and_then(|v| v.checked_add(overhead))
        else {
            break;
        };
        if next > bytes.len() {
            break;
        }
        offset = next;
    }
    Ok(())
}

/// A static full-frame resource. Source dimensions drive the camera and 100%;
/// texture dimensions describe actual available detail, never implied detail.
#[derive(Clone, Debug, PartialEq)]
pub struct ImagePreviewImage {
    pub(super) image: Arc<RenderImage>,
    pub(super) description: SharedString,
    source_size: [u32; 2],
    color_space: ImagePreviewColorSpace,
}
impl ImagePreviewImage {
    /// Advanced entry: a validated RenderImage carries its own color encoding.
    /// Zero/animated frames are rejected; this does not copy/resize the resource.
    pub fn new(
        image: Arc<RenderImage>,
        description: impl Into<SharedString>,
    ) -> Result<Self, ImagePreviewError> {
        let s = image.size(0);
        if image.frame_count() != 1 || s.width.0 <= 0 || s.height.0 <= 0 {
            return Err(ImagePreviewError::InvalidImage);
        }
        let description = description.into();
        let color_space = match image.format() {
            RenderImageFormat::Bgra8Srgb => ImagePreviewColorSpace::Srgb,
            RenderImageFormat::Rgba16FloatLinearSrgb => ImagePreviewColorSpace::LinearSrgb,
            RenderImageFormat::Rgba16FloatScRgb => ImagePreviewColorSpace::ScRgb,
        };
        Ok(Self {
            source_size: [s.width.0 as u32, s.height.0 as u32],
            image,
            description: if description.trim().is_empty() {
                "图片预览".into()
            } else {
                description
            },
            color_space,
        })
    }
    pub fn from_rgba8<'a>(
        w: u32,
        h: u32,
        pixels: impl Into<Cow<'a, [u8]>>,
        description: impl Into<SharedString>,
    ) -> Result<Self, ImagePreviewError> {
        ImagePreviewPixels::from_rgba8(w, h, pixels)?
            .into_preview(&ImagePreviewLimits::default(), description)
    }
    pub fn from_bgra8<'a>(
        w: u32,
        h: u32,
        pixels: impl Into<Cow<'a, [u8]>>,
        description: impl Into<SharedString>,
    ) -> Result<Self, ImagePreviewError> {
        ImagePreviewPixels::from_bgra8(w, h, pixels)?
            .into_preview(&ImagePreviewLimits::default(), description)
    }
    pub fn from_encoded(
        bytes: &[u8],
        limits: &ImagePreviewLimits,
        description: impl Into<SharedString>,
    ) -> Result<Self, ImagePreviewError> {
        ImagePreviewPixels::decode(bytes, limits)?.into_preview(limits, description)
    }
    /// Declare that this texture is a reduced proxy for the SAME whole frame.
    /// Rejects upscaled/incompatible dimensions; the host must guarantee that
    /// pixels are not a crop (content identity cannot be inferred from sizes).
    pub fn with_source_size(mut self, size: [u32; 2]) -> Result<Self, ImagePreviewError> {
        pixel_bytes(size[0], size[1])?;
        let texture = self.texture_size();
        if texture.iter().zip(size).any(|(t, s)| *t > s)
            || !aligned(size.map(|v| v as f32), texture.map(|v| v as f32))
        {
            return Err(ImagePreviewError::InvalidPixels);
        }
        self.source_size = size;
        Ok(self)
    }
    /// Original, oriented dimensions, even when the texture is a proxy.
    pub fn pixel_size(&self) -> [u32; 2] {
        self.source_size
    }
    pub fn texture_size(&self) -> [u32; 2] {
        let s = self.image.size(0);
        [s.width.0 as u32, s.height.0 as u32]
    }
    pub fn is_proxy(&self) -> bool {
        self.source_size != self.texture_size()
    }
    pub fn color_space(&self) -> ImagePreviewColorSpace {
        self.color_space
    }
    pub fn description(&self) -> &SharedString {
        &self.description
    }
    pub fn image(&self) -> &Arc<RenderImage> {
        &self.image
    }
    pub(super) fn dimensions(&self) -> [f32; 2] {
        self.source_size.map(|v| v as f32)
    }
}
