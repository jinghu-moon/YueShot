//! Bounded, explicit-color PNG derivatives of the immutable pixel master.

use std::io::Write;
pub(crate) use yue_image_codec::{ColorExport, encode_pixels, write_png};
use yue_media::pixels::BandSource;
use yue_media::pixels::{PixelEncoding, PixelSource};

/// Stream a checked historical original. scRGB masters become Rec.2020/PQ
/// PNG16; relative/SDR masters become explicitly SDR PNG8. Existing PNG8 sources
/// are copied losslessly with their encoded identity checked before commit.
/// The caller owns the atomic destination; an error must discard partial bytes.
pub fn write_capture_source_png(
    source: &yue_shot_model::CaptureSource,
    mut output: impl Write,
) -> Result<(), String> {
    if source.artifact.dimensions.width != source.metadata.width
        || source.artifact.dimensions.height != source.metadata.height
    {
        return Err("history source geometry differs from its reference".into());
    }
    if source.artifact.mime == "image/png" {
        use std::io::Read as _;
        if source.metadata.source.descriptor() != yue_media::pixels::PixelDescriptor::bgra8() {
            return Err("history PNG is not the recorded SDR source".into());
        }
        let mut input =
            std::fs::File::open(&source.artifact.absolute_path).map_err(|e| e.to_string())?;
        if input.metadata().map_err(|e| e.to_string())?.len() != source.artifact.byte_len {
            return Err("history PNG length differs from its reference".into());
        }
        let mut hash = blake3::Hasher::new();
        let mut scratch = [0; 64 * 1024];
        loop {
            let n = input.read(&mut scratch).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&scratch[..n]);
            output.write_all(&scratch[..n]).map_err(|e| e.to_string())?;
        }
        if hash.finalize().to_hex().as_str() != source.artifact.content_fingerprint {
            return Err("history PNG fingerprint differs from its reference".into());
        }
        return Ok(());
    }
    let master = yue_shot_storage::pixel_master::PixelMaster::open(&source.artifact)
        .map_err(|e| e.to_string())?;
    if master.metadata() != &source.metadata {
        return Err("history master metadata differs from its reference".into());
    }
    let policy = if source.metadata.source.descriptor().encoding() == PixelEncoding::ScRgb {
        ColorExport::PreserveHdr
    } else {
        ColorExport::Sdr {
            source_peak_nits: 100.,
        }
    };
    write_png(
        &mut MasterBands(master),
        source.metadata.width,
        policy,
        output,
    )
}

#[cfg(feature = "avif")]
pub(crate) use yue_image_codec::PixelBands;

pub(crate) struct MasterBands(pub(crate) yue_shot_storage::pixel_master::PixelMaster);
impl BandSource for MasterBands {
    fn source(&self) -> PixelSource {
        self.0.metadata().source
    }
    fn rows(&self) -> u32 {
        self.0.metadata().height
    }
    fn read_rows(&mut self, first: u32, count: u32, out: &mut Vec<u8>) -> bool {
        match self.0.read_rows(first, count) {
            Ok(pixels) => {
                *out = pixels.into_bytes();
                true
            }
            Err(_) => false,
        }
    }
}
