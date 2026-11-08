//! Framework-free domain values owned by YueShot.

pub mod artifact;
pub mod capture;
pub mod error;
pub mod time;

pub use artifact::{ArtifactRef, CaptureMetadata, CaptureOutput, CaptureSource};
pub use capture::{CaptureArtifact, CapturePayload, CaptureState};
pub use error::ErrorCode;
pub use time::unix_time_ms;
pub use yue_geometry as geometry;
pub use yue_geometry::{Point, Rect};
pub use yue_media::ImageDimensions;
