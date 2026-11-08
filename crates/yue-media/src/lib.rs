//! Shared image value types and color transforms, without product or UI policy.

pub mod pixel_color;
pub mod pixels;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Bgra8Unorm,
    Rgba16Float,
}

impl PixelFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bgra8Unorm => "bgra8_unorm",
            Self::Rgba16Float => "rgba16_float",
        }
    }

    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Bgra8Unorm => 4,
            Self::Rgba16Float => 8,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::{ImageDimensions, PixelFormat};

    #[test]
    fn image_dimensions_keep_the_wire_format() {
        let json = serde_json::to_string(&ImageDimensions {
            width: 1920,
            height: 1080,
        })
        .unwrap();
        assert_eq!(json, r#"{"width":1920,"height":1080}"#);
    }

    #[test]
    fn pixel_format_strings_and_sizes_are_stable() {
        assert_eq!(PixelFormat::Bgra8Unorm.as_str(), "bgra8_unorm");
        assert_eq!(PixelFormat::Bgra8Unorm.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::Rgba16Float.bytes_per_pixel(), 8);
    }
}
