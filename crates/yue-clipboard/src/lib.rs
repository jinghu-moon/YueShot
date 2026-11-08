//! Bounded system clipboard I/O. No product models, classification, storage or UI.
#[cfg(windows)]
pub mod windows;
mod writer;
pub use writer::{Channels, ClipboardImage, SystemClipboard};

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 24_000_000;
pub const MAX_FILES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPurpose {
    Monitor,
    ExplicitPin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEncoding {
    Png,
    Dib,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    sequence: u32,
    completed_sequence: u32,
    owner: usize,
    foreground: usize,
    text: Option<String>,
    html: Option<Vec<u8>>,
    rtf: Option<Vec<u8>>,
    files: Vec<String>,
    images: Vec<(ImageEncoding, Vec<u8>)>,
    oversized_image: bool,
    virtual_files: bool,
    has_formats: bool,
    excluded: bool,
}
impl Snapshot {
    pub fn new(sequence: u32) -> Self {
        Self {
            sequence,
            completed_sequence: sequence,
            owner: 0,
            foreground: 0,
            text: None,
            html: None,
            rtf: None,
            files: vec![],
            images: vec![],
            oversized_image: false,
            virtual_files: false,
            has_formats: false,
            excluded: false,
        }
    }
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
    pub fn with_html(mut self, bytes: Vec<u8>) -> Self {
        self.html = Some(bytes);
        self
    }
    pub fn with_rtf(mut self, bytes: Vec<u8>) -> Self {
        self.rtf = Some(bytes);
        self
    }
    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }
    pub fn with_virtual_files(mut self, present: bool) -> Self {
        self.virtual_files = present;
        self
    }
    pub fn with_image(mut self, encoding: ImageEncoding, bytes: Vec<u8>) -> Self {
        self.images.push((encoding, bytes));
        self
    }
    pub fn sequence(&self) -> u32 {
        self.sequence
    }
    pub fn completed_sequence(&self) -> u32 {
        self.completed_sequence
    }
    pub fn owner(&self) -> usize {
        self.owner
    }
    pub fn foreground(&self) -> usize {
        self.foreground
    }
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
    pub fn html(&self) -> Option<&[u8]> {
        self.html.as_deref()
    }
    pub fn rtf(&self) -> Option<&[u8]> {
        self.rtf.as_deref()
    }
    pub fn files(&self) -> &[String] {
        &self.files
    }
    pub fn images(&self) -> &[(ImageEncoding, Vec<u8>)] {
        &self.images
    }
    pub fn has_virtual_files(&self) -> bool {
        self.virtual_files
    }
    pub fn has_oversized_image(&self) -> bool {
        self.oversized_image
    }
    pub fn has_formats(&self) -> bool {
        self.has_formats
    }
    pub fn is_excluded(&self) -> bool {
        self.excluded
    }
    pub fn byte_len(&self) -> usize {
        self.text.as_ref().map_or(0, String::len)
            + self.html.as_ref().map_or(0, Vec::len)
            + self.rtf.as_ref().map_or(0, Vec::len)
            + self.files.iter().map(String::len).sum::<usize>()
            + self
                .images
                .iter()
                .map(|(_, bytes)| bytes.len())
                .sum::<usize>()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    Empty,
    Busy,
    Changed,
    Unsupported,
    VirtualFiles,
    TooLarge,
    InvalidImage,
    InvalidDocument,
    Platform(String),
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "剪贴板中没有可贴出的内容",
            Self::Busy => "无法读取剪贴板，请稍后重试",
            Self::Changed => "剪贴板已变化，请重新按 F3",
            Self::Unsupported => "当前剪贴板格式暂不支持",
            Self::VirtualFiles => "暂不支持直接贴出此附件，请先保存到本地文件",
            Self::TooLarge => "内容超过当前贴图上限",
            Self::InvalidImage => "无法读取图片",
            Self::InvalidDocument => "无法读取文档，且没有可用文字",
            Self::Platform(message) => message,
        })
    }
}
impl std::error::Error for ReadError {}
