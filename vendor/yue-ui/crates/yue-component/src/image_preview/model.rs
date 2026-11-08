use super::ImagePreviewImage;
use gpui_kit::SharedString;
use std::fmt;

/// Explicit host-supplied state. Refreshing may retain a previously ready result.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ImagePreviewLoad {
    #[default]
    Empty,
    Loading,
    Ready(ImagePreviewImage),
    Failed(SharedString),
}
impl ImagePreviewLoad {
    pub fn ready(&self) -> Option<&ImagePreviewImage> {
        if let Self::Ready(image) = self {
            Some(image)
        } else {
            None
        }
    }
}

/// Immutable presentation input; submit through State, never from render.
#[derive(Clone, Debug, PartialEq)]
pub struct ImagePreviewDocument {
    pub(super) id: SharedString,
    pub(super) revision: u64,
    pub(super) source: ImagePreviewLoad,
    pub(super) result: ImagePreviewLoad,
    pub(super) full_frame: bool,
}
impl ImagePreviewDocument {
    pub fn new(id: impl Into<SharedString>, revision: u64) -> Self {
        Self {
            id: id.into(),
            revision,
            source: ImagePreviewLoad::Empty,
            result: ImagePreviewLoad::Empty,
            full_frame: false,
        }
    }
    pub fn with_source(mut self, source: ImagePreviewLoad) -> Self {
        self.source = source;
        self
    }
    pub fn with_result(mut self, result: ImagePreviewLoad) -> Self {
        self.result = result;
        self
    }
    /// Assert that both images represent the same complete, oriented frame.
    /// Aspect-ratio validation is also required; this does not stretch a crop.
    pub fn full_frame(mut self, aligned: bool) -> Self {
        self.full_frame = aligned;
        self
    }
    pub fn id(&self) -> &SharedString {
        &self.id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn source(&self) -> &ImagePreviewLoad {
        &self.source
    }
    pub fn result(&self) -> &ImagePreviewLoad {
        &self.result
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImagePreviewMode {
    #[default]
    Source,
    Compare,
    Result,
}
impl ImagePreviewMode {
    pub(super) fn value(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Compare => "compare",
            Self::Result => "result",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImagePreviewFit {
    #[default]
    FitAll,
    FitWidth,
    Manual,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImagePreviewBackground {
    #[default]
    CheckerDark,
    CheckerLight,
    White,
    Black,
}
impl ImagePreviewBackground {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::CheckerDark => "深色棋盘格",
            Self::CheckerLight => "浅色棋盘格",
            Self::White => "纯白",
            Self::Black => "纯黑",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImagePreviewToolbar {
    Floating,
    #[default]
    Footer,
}
/// Smooth browsing is the default. Pixel inspection uses nearest sampling on
/// Windows; proxy resources always stay smooth because detail is unavailable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImagePreviewSampling {
    #[default]
    Smooth,
    Nearest,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImagePreviewChangeSource {
    Pointer,
    Keyboard,
    Toolbar,
    Accessibility,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImagePreviewSnapshot {
    pub(super) document_id: Option<SharedString>,
    pub(super) revision: u64,
    pub(super) result_revision: Option<u64>,
    pub(super) mode: ImagePreviewMode,
    pub(super) fit: ImagePreviewFit,
    pub(super) zoom: f32,
    pub(super) center: [f32; 2],
    pub(super) split: f32,
    pub(super) background: ImagePreviewBackground,
    pub(super) pixel_grid: bool,
    pub(super) reference_size: Option<[u32; 2]>,
    pub(super) texture_size: Option<[u32; 2]>,
    pub(super) proxy: bool,
    pub(super) pixel_grid_available: bool,
    pub(super) sampling: ImagePreviewSampling,
}
impl ImagePreviewSnapshot {
    pub fn document_id(&self) -> Option<&SharedString> {
        self.document_id.as_ref()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn result_revision(&self) -> Option<u64> {
        self.result_revision
    }
    pub fn mode(&self) -> ImagePreviewMode {
        self.mode
    }
    pub fn fit(&self) -> ImagePreviewFit {
        self.fit
    }
    pub fn zoom(&self) -> f32 {
        self.zoom
    }
    /// Image-space position at the viewport's safe-area midpoint. Moving a small
    /// image through the surrounding space can place this outside `[0, 1]`.
    pub fn center(&self) -> [f32; 2] {
        self.center
    }
    pub fn split(&self) -> f32 {
        self.split
    }
    pub fn background(&self) -> ImagePreviewBackground {
        self.background
    }
    pub fn is_pixel_grid_enabled(&self) -> bool {
        self.pixel_grid
    }
    pub fn reference_size(&self) -> Option<[u32; 2]> {
        self.reference_size
    }
    pub fn texture_size(&self) -> Option<[u32; 2]> {
        self.texture_size
    }
    /// In Compare, true when either visible resource is a proxy.
    pub fn is_proxy(&self) -> bool {
        self.proxy
    }
    pub fn is_pixel_grid_available(&self) -> bool {
        self.pixel_grid_available
    }
    pub fn sampling(&self) -> ImagePreviewSampling {
        self.sampling
    }
}

/// A captured document and view for opening an independent, larger viewer.
/// Decoded images are shared; focus, gestures, layout and animation are not.
#[derive(Clone, Debug)]
pub struct ImagePreviewView {
    pub(super) document: Option<ImagePreviewDocument>,
    pub(super) result: Option<ImagePreviewImage>,
    pub(super) gallery: Vec<ImagePreviewDocument>,
    pub(super) gallery_index: usize,
    pub(super) snapshot: ImagePreviewSnapshot,
    pub(super) last_middle: f32,
}
impl ImagePreviewView {
    pub fn snapshot(&self) -> &ImagePreviewSnapshot {
        &self.snapshot
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImagePreviewEvent {
    ModeChanged(ImagePreviewSnapshot, ImagePreviewChangeSource),
    ViewChanged(ImagePreviewSnapshot, ImagePreviewChangeSource),
    SplitChanged(ImagePreviewSnapshot, ImagePreviewChangeSource),
    BackgroundChanged(ImagePreviewSnapshot, ImagePreviewChangeSource),
    PixelGridChanged(ImagePreviewSnapshot, ImagePreviewChangeSource),
    SamplingChanged(ImagePreviewSnapshot, ImagePreviewChangeSource),
    GalleryChanged(ImagePreviewSnapshot, usize, usize, ImagePreviewChangeSource),
    RetryRequested {
        document_id: SharedString,
        revision: u64,
        result: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImagePreviewError {
    InvalidImage,
    InvalidPixels,
    InvalidLimits,
    ResourceLimit(String),
    Decode(String),
    Render(String),
    EmptyDocumentId,
    StaleRevision,
    UnavailableMode,
    InvalidValue,
}
impl fmt::Display for ImagePreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceLimit(message) => return write!(f, "image resource limit: {message}"),
            Self::Decode(message) => return write!(f, "image decoding failed: {message}"),
            Self::Render(message) => return write!(f, "image rendering failed: {message}"),
            _ => {}
        }
        f.write_str(match self {
            Self::InvalidImage => "image must contain one non-empty static frame",
            Self::InvalidPixels => {
                "image pixels must have valid dimensions and a tightly packed four-channel buffer"
            }
            Self::InvalidLimits => {
                "image budgets must be positive; texture edge must not exceed 8192"
            }
            Self::ResourceLimit(_) | Self::Decode(_) | Self::Render(_) => unreachable!(),
            Self::EmptyDocumentId => "document identity must not be empty",
            Self::StaleRevision => "document revision is older than the current request",
            Self::UnavailableMode => "requested preview mode is unavailable or unaligned",
            Self::InvalidValue => "preview value must be finite and valid",
        })
    }
}
impl std::error::Error for ImagePreviewError {}
