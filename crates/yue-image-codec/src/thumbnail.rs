//! Bounded top-of-capture SDR thumbnails. Source bands remain immutable; filtering
//! is linear-light and alpha-associated. Only a 128×128 accumulator is retained.
use yue_media::{PixelFormat, pixels::*};


pub struct Thumbnail {
    source_size: [u32; 2],
    descriptor: PixelDescriptor,
    scale: u32,
    width: u32,
    height: u32,
    next_row: u32,
    peak_nits: f64,
    sums: Vec<[f64; 5]>,
}
impl Thumbnail {
    pub fn new(source_width: u32, source_height: u32, descriptor: PixelDescriptor) -> Result<Self, &'static str> {
        if source_width == 0 || source_height == 0 {
            return Err("thumbnail source is empty");
        }
        let scale = source_width.div_ceil(128).max(1);
        let width = source_width.div_ceil(scale);
        let height = source_height.div_ceil(scale).min(128);
        Ok(Self {
            source_size: [source_width, source_height],
            descriptor,
            scale,
            width,
            height,
            next_row: 0,
            peak_nits: 100.,
            sums: vec![[0.; 5]; (width * height) as usize],
        })
    }
    pub fn push(&mut self, first: u32, pixels: &PixelBuffer) -> Result<(), &'static str> {
        let [width, height] = pixels.size();
        let end = first
            .checked_add(height)
            .ok_or("thumbnail source row overflow")?;
        if first != self.next_row
            || width != self.source_size[0]
            || end > self.source_size[1]
            || pixels.descriptor() != self.descriptor
        {
            return Err("thumbnail band differs from its frozen source or row order");
        }
        for (ix, [r, g, b, a]) in yue_media::pixel_color::linear_rgba(pixels).enumerate() {
            if a > 0. {
                self.peak_nits = self.peak_nits.max(r.max(g).max(b) * 80.);
            }
            let y = first + (ix / width as usize) as u32;
            let out_y = y / self.scale;
            if out_y >= self.height {
                continue;
            }
            let out_x = (ix % width as usize) as u32 / self.scale;
            let sum = &mut self.sums[(out_y * self.width + out_x) as usize];
            for (sum, value) in sum.iter_mut().zip([r * a, g * a, b * a, a, 1.]) {
                *sum += value;
            }
        }
        self.next_row = end;
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<u8>, String> {
        if self.next_row != self.source_size[1] {
            return Err("thumbnail source ended before its last row".into());
        }
        let mut bytes = Vec::with_capacity(self.sums.len() * 8);
        for [r, g, b, a, count] in self.sums {
            for value in [
                if a > 0. { r / a } else { 0. },
                if a > 0. { g / a } else { 0. },
                if a > 0. { b / a } else { 0. },
                a / count,
            ] {
                let value = half::f16::from_f64(value);
                if !value.is_finite() {
                    return Err("thumbnail exceeds finite FP16 range".into());
                }
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let encoding = if self.descriptor.encoding() == PixelEncoding::ScRgb {
            PixelEncoding::ScRgb
        } else {
            PixelEncoding::LinearSrgb
        };
        let descriptor =
            PixelDescriptor::new(PixelFormat::Rgba16Float, encoding, AlphaMode::Straight)
                .map_err(str::to_owned)?;
        let pixels = PixelBuffer::from_tight(self.width, self.height, descriptor, bytes)
            .map_err(str::to_owned)?;
        let mut bytes = yue_media::pixel_color::to_sdr_bgra8(&pixels, self.peak_nits)
            .map_err(str::to_owned)?
            .into_bytes();
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let image = image::RgbaImage::from_raw(self.width, self.height, bytes)
            .ok_or_else(|| "invalid thumbnail buffer".to_string())?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|error| format!("encode thumbnail png: {error}"))?;
        Ok(png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_bands_make_the_same_bounded_linear_alpha_thumbnail_as_one_buffer() {
        let (source_width, source_height, source_descriptor) = (256, 600, PixelDescriptor::bgra8());
        let bytes: Vec<u8> = (0..source_height)
            .flat_map(|y| {
                (0..source_width).flat_map(move |x| {
                    if y < 256 && x % 2 == 0 {
                        [0, 0, 255, 255]
                    } else {
                        [255, 0, 0, 0]
                    }
                })
            })
            .collect();
        let pixels =
            PixelBuffer::from_tight(source_width, source_height, source_descriptor, bytes.clone())
                .unwrap();
        let mut whole = Thumbnail::new(source_width, source_height, source_descriptor).unwrap();
        whole.push(0, &pixels).unwrap();
        let expected = whole.finish().unwrap();
        let mut bands = Thumbnail::new(source_width, source_height, source_descriptor).unwrap();
        for first in (0..source_height).step_by(100) {
            bands
                .push(
                    first,
                    &PixelBuffer::from_tight(
                        source_width,
                        100,
                        source_descriptor,
                        bytes[first as usize * 256 * 4..(first + 100) as usize * 256 * 4].to_vec(),
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        assert_eq!(bands.finish().unwrap(), expected);
        assert_eq!(pixels.as_bytes(), bytes);
        let decoded = image::load_from_memory_with_format(&expected, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        let image = &decoded;
        assert_eq!(image.dimensions(), (128, 128));
        assert_eq!(
            image.get_pixel(0, 0).0,
            [255, 0, 0, 128],
            "hidden blue must not bleed into red"
        );
    }
    #[test]
    fn incomplete_or_out_of_order_bands_cannot_publish_a_thumbnail() {
        let (source_width, source_height, source_descriptor) = (256, 600, PixelDescriptor::bgra8());
        let mut thumbnail = Thumbnail::new(source_width, source_height, source_descriptor).unwrap();
        let band =
            PixelBuffer::from_tight(256, 1, PixelDescriptor::bgra8(), vec![0; 1024]).unwrap();
        assert!(thumbnail.push(1, &band).is_err());
        assert!(thumbnail.finish().is_err());
    }
}
