//! CPU preparation probe; no window/GPU upload. Run in release, not as a timing
//! assertion in CI. Reports actual retained raster bytes (not process/VRAM peak).
use std::{hint::black_box, time::Instant};
use yue_component::{ImagePreviewImage, ImagePreviewLimits};

fn main() {
    for (name, [width, height]) in [
        ("4k", [3840, 2160]),
        ("8k", [7680, 4320]),
        ("long", [1024, 32768]),
        ("thin", [1, 32768]),
    ] {
        let pixels = [17, 70, 180, 128].repeat(width as usize * height as usize);
        let mut samples = Vec::new();
        let mut texture = [0; 2];
        let mut raster_bytes = 0;
        for _ in 0..3 {
            let start = Instant::now();
            let preview =
                ImagePreviewImage::from_bgra8(width, height, pixels.as_slice(), name).unwrap();
            samples.push(start.elapsed().as_secs_f64() * 1000.);
            texture = preview.texture_size();
            raster_bytes = preview.image().as_bytes(0).unwrap().len();
            assert_eq!(preview.pixel_size(), [width, height]);
            assert_eq!(
                texture,
                ImagePreviewLimits::default()
                    .texture_size([width, height])
                    .unwrap()
            );
            assert!(raster_bytes <= 64 * 1024 * 1024 && texture.iter().all(|n| *n <= 8192));
            black_box(preview);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{name}: source={width}x{height} texture={}x{} source_bytes={} raster_bytes={raster_bytes} prepare_ms_median={:.3}",
            texture[0],
            texture[1],
            pixels.len(),
            samples[1]
        );
    }
}
