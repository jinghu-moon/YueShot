use crate::{DevicePixels, Pixels, Result, SharedString, Size, size};
use smallvec::SmallVec;

use image::{Delay, Frame};
use std::{
    borrow::Cow,
    fmt,
    hash::Hash,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

/// A source of assets for this app to use.
pub trait AssetSource: 'static + Send + Sync {
    /// Load the given asset from the source path.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>>;

    /// List the assets at the given path.
    fn list(&self, path: &str) -> Result<Vec<SharedString>>;
}

impl AssetSource for () {
    fn load(&self, _path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(None)
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

/// A unique identifier for the image cache
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ImageId(pub usize);

#[derive(PartialEq, Eq, Hash, Clone)]
#[expect(missing_docs)]
pub struct RenderImageParams {
    pub image_id: ImageId,
    pub frame_index: usize,
    pub format: RenderImageFormat,
}

/// Storage and color interpretation are one validated contract. Float samples
/// use little-endian IEEE binary16 and straight alpha; SDR linear values are
/// relative to the current SDR white, scRGB values are absolute (1 = 80 nit).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RenderImageFormat {
    #[default]
    /// Four-byte encoded sRGB pixels in BGRA order.
    Bgra8Srgb,
    /// Eight-byte linear Rec.709 pixels, 1.0 = SDR display white.
    Rgba16FloatLinearSrgb,
    /// Eight-byte absolute scRGB pixels, 1.0 = 80 nit.
    Rgba16FloatScRgb,
}

impl RenderImageFormat {
    /// Size of a tightly packed pixel in this format.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Bgra8Srgb => 4,
            Self::Rgba16FloatLinearSrgb | Self::Rgba16FloatScRgb => 8,
        }
    }
}

enum RenderImageData {
    Bgra8(SmallVec<[Frame; 1]>),
    Float {
        size: Size<DevicePixels>,
        bytes: Vec<u8>,
        format: RenderImageFormat,
        peak_nits: f32,
    },
}

/// A tightly packed render-target readback, retaining its actual encoding.
/// RGB is associated with alpha, as stored in the composited render target;
/// this is deliberately distinct from a straight-alpha `RenderImage` source.
#[derive(Debug)]
pub struct RenderTargetReadback {
    size: Size<DevicePixels>,
    format: RenderImageFormat,
    bytes: Vec<u8>,
}

impl RenderTargetReadback {
    /// Validate dimensions and exact byte count without copying storage.
    pub fn new(
        size: Size<DevicePixels>,
        format: RenderImageFormat,
        bytes: Vec<u8>,
    ) -> Result<Self> {
        anyhow::ensure!(
            size.width.0 > 0 && size.height.0 > 0,
            "empty render-target readback"
        );
        let expected = (size.width.0 as usize)
            .checked_mul(size.height.0 as usize)
            .and_then(|v| v.checked_mul(format.bytes_per_pixel()))
            .ok_or_else(|| anyhow::anyhow!("render-target readback size overflow"))?;
        anyhow::ensure!(
            expected == bytes.len(),
            "invalid render-target readback byte count"
        );
        Ok(Self {
            size,
            format,
            bytes,
        })
    }
    /// Physical dimensions, excluding row-pitch padding.
    pub fn size(&self) -> Size<DevicePixels> {
        self.size
    }
    /// Storage and output color encoding, never guessed from byte length.
    pub fn format(&self) -> RenderImageFormat {
        self.format
    }
    /// Associated-alpha pixels in the declared encoding.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Move the tightly packed pixel storage out without copying.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// An immutable cached image with explicit pixel storage and color encoding.
pub struct RenderImage {
    /// The ID associated with this image
    pub id: ImageId,
    /// The scale factor of this image on render.
    pub(crate) scale_factor: f32,
    data: RenderImageData,
}

static NEXT_IMAGE_ID: AtomicUsize = AtomicUsize::new(0);

impl PartialEq for RenderImage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RenderImage {}

impl RenderImage {
    /// Create a new image from the given data.
    pub fn new(data: impl Into<SmallVec<[Frame; 1]>>) -> Self {
        Self {
            id: ImageId(NEXT_IMAGE_ID.fetch_add(1, SeqCst)),
            scale_factor: 1.0,
            data: RenderImageData::Bgra8(data.into()),
        }
    }

    /// Construct a static float texture without changing its values or copying
    /// the allocation. Rejects invalid dimensions, nonfinite values, invalid
    /// alpha and nonfloat formats. Negative and >1 RGB are valid.
    pub fn from_rgba16f(
        size: Size<DevicePixels>,
        bytes: Vec<u8>,
        format: RenderImageFormat,
    ) -> Result<Self> {
        Self::from_rgba16f_with_source_peak(size, bytes, format, None)
    }

    /// Construct a float proxy retaining the full source's measured peak.
    /// Supplied peak must be finite, representable and at least the proxy's
    /// visible MaxRGB. SDR resources must not carry absolute peak metadata.
    pub fn from_rgba16f_with_source_peak(
        size: Size<DevicePixels>,
        bytes: Vec<u8>,
        format: RenderImageFormat,
        source_peak_nits: Option<f32>,
    ) -> Result<Self> {
        anyhow::ensure!(size.width.0 > 0 && size.height.0 > 0, "empty float image");
        anyhow::ensure!(
            format != RenderImageFormat::Bgra8Srgb,
            "float pixels require a float encoding"
        );
        let expected = (size.width.0 as usize)
            .checked_mul(size.height.0 as usize)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| anyhow::anyhow!("float image size overflow"))?;
        anyhow::ensure!(bytes.len() == expected, "invalid float image byte length");
        let mut peak = 0_f32;
        for pixel in bytes.as_chunks::<8>().0 {
            let mut rgb_peak = 0_f32;
            let mut visible = true;
            for (ix, sample) in pixel.as_chunks::<2>().0.iter().enumerate() {
                let value = half::f16::from_bits(u16::from_le_bytes(*sample)).to_f32();
                anyhow::ensure!(value.is_finite(), "nonfinite float image sample");
                if ix == 3 {
                    anyhow::ensure!((0.0..=1.0).contains(&value), "invalid float image alpha");
                    visible = value > 0.;
                } else {
                    rgb_peak = rgb_peak.max(value);
                }
            }
            if visible {
                peak = peak.max(rgb_peak);
            }
        }
        let peak_nits = if let Some(source_peak) = source_peak_nits {
            anyhow::ensure!(
                format == RenderImageFormat::Rgba16FloatScRgb
                    && source_peak.is_finite()
                    && source_peak >= peak * 80.
                    && source_peak <= 65504. * 80.,
                "invalid absolute source peak"
            );
            source_peak
        } else {
            peak * 80.
        };
        Ok(Self {
            id: ImageId(NEXT_IMAGE_ID.fetch_add(1, SeqCst)),
            scale_factor: 1.0,
            data: RenderImageData::Float {
                size,
                bytes,
                format,
                peak_nits,
            },
        })
    }

    /// Pixel storage and color encoding; never inferred from the byte count.
    pub fn format(&self) -> RenderImageFormat {
        match &self.data {
            RenderImageData::Bgra8(_) => RenderImageFormat::Bgra8Srgb,
            RenderImageData::Float { format, .. } => *format,
        }
    }

    /// MaxRGB of an absolute scRGB image, computed once on construction.
    /// SDR images have no absolute peak. This is not display capability.
    pub fn peak_nits(&self) -> Option<f32> {
        match &self.data {
            RenderImageData::Float {
                format: RenderImageFormat::Rgba16FloatScRgb,
                peak_nits,
                ..
            } => Some(*peak_nits),
            _ => None,
        }
    }

    /// Convert this image into a byte slice.
    pub fn as_bytes(&self, frame_index: usize) -> Option<&[u8]> {
        match &self.data {
            RenderImageData::Bgra8(frames) => frames
                .get(frame_index)
                .map(|frame| frame.buffer().as_raw().as_slice()),
            RenderImageData::Float { bytes, .. } => (frame_index == 0).then_some(bytes.as_slice()),
        }
    }

    /// Get the size of this image, in pixels.
    pub fn size(&self, frame_index: usize) -> Size<DevicePixels> {
        match &self.data {
            RenderImageData::Bgra8(frames) => frames
                .get(frame_index)
                .map(|frame| {
                    let (width, height) = frame.buffer().dimensions();
                    size(width.into(), height.into())
                })
                .unwrap_or_default(),
            RenderImageData::Float { size, .. } => {
                if frame_index == 0 {
                    *size
                } else {
                    Size::default()
                }
            }
        }
    }

    /// Get the size of this image, in pixels for display, adjusted for the scale factor.
    pub(crate) fn render_size(&self, frame_index: usize) -> Size<Pixels> {
        self.size(frame_index)
            .map(|v| (v.0 as f32 / self.scale_factor).into())
    }

    /// Get the delay of this frame from the previous
    pub fn delay(&self, frame_index: usize) -> Delay {
        match &self.data {
            RenderImageData::Bgra8(frames) => frames.get(frame_index).map(|frame| frame.delay()),
            RenderImageData::Float { .. } => None,
        }
        .unwrap_or(Delay::from_numer_denom_ms(100, 1))
    }

    /// Get the number of frames for this image.
    pub fn frame_count(&self) -> usize {
        match &self.data {
            RenderImageData::Bgra8(frames) => frames.len(),
            RenderImageData::Float { .. } => 1,
        }
    }
}

impl fmt::Debug for RenderImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("id", &self.id)
            .field("size", &self.size(0))
            .field("format", &self.format())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    #[test]
    fn empty_render_image_does_not_panic() {
        let image = RenderImage::new(SmallVec::new());
        assert_eq!(image.frame_count(), 0);
        assert_eq!(image.size(0), Size::default());
        assert_eq!(image.as_bytes(0), None);
        assert_eq!(image.render_size(0), Size::default());
        assert_eq!(image.delay(0), Delay::from_numer_denom_ms(100, 1));
        let _ = format!("{image:?}");
    }
}
