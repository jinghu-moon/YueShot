//! Explicit large-file CPU probe. No windows, files written or system changes.
use std::{fs::File, io::BufReader, time::Instant};
use yue_component::{ImagePreviewLimits, ImagePreviewPixels};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This diagnostic alone opts into the supplied 45 MP / 200 MiB samples.
    // Neither the component defaults nor application import limits are raised.
    let limits = ImagePreviewLimits::default().max_encoded_bytes(256 * 1024 * 1024);
    for path in std::env::args().skip(1) {
        let started = Instant::now();
        let pixels =
            ImagePreviewPixels::from_png_reader(BufReader::new(File::open(&path)?), &limits)?;
        let decoded_ms = started.elapsed().as_secs_f64() * 1000.;
        let size = pixels.pixel_size();
        let source_bytes = pixels.as_bytes().len();
        let encoded = pixels.color_space();
        let before_proxy = Instant::now();
        let preview = pixels.into_preview(&limits, "HDR resource probe")?;
        println!(
            "file={path} source={size:?} encoding={encoded:?} source_bytes={source_bytes} texture={:?} texture_bytes={} source_peak_nits={:?} decode_ms={decoded_ms:.3} proxy_ms={:.3}",
            preview.texture_size(),
            preview.image().as_bytes(0).unwrap().len(),
            preview.image().peak_nits(),
            before_proxy.elapsed().as_secs_f64() * 1000.
        );
    }
    Ok(())
}
