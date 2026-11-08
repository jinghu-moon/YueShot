//! Explicit, per-operation timing. Normal encoding never reads the clock.
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum EncodeStage {
    SourceRead,
    ColorConvert,
    Provider,
    Encode,
    Parse,
    Scratch,
    Header,
    Publish,
}
impl EncodeStage {
    pub const ALL: [Self; 8] = [
        Self::SourceRead,
        Self::ColorConvert,
        Self::Provider,
        Self::Encode,
        Self::Parse,
        Self::Scratch,
        Self::Header,
        Self::Publish,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::SourceRead => "source_read",
            Self::ColorConvert => "color_convert",
            Self::Provider => "provider_inclusive",
            Self::Encode => "encode",
            Self::Parse => "parse",
            Self::Scratch => "scratch",
            Self::Header => "header",
            Self::Publish => "publish",
        }
    }
}

/// One synchronous encoding operation. Provider time includes nested source
/// reads and color conversion; those two measurements must not be summed twice.
/// Not shared globally or across workers. Timings also include failed work.
#[derive(Default)]
pub struct EncodeProfile {
    elapsed: [Cell<Duration>; 8],
    calls: [Cell<u64>; 8],
}
impl EncodeProfile {
    pub fn elapsed(&self, stage: EncodeStage) -> Duration {
        self.elapsed[stage as usize].get()
    }
    pub fn calls(&self, stage: EncodeStage) -> u64 {
        self.calls[stage as usize].get()
    }
}
pub(crate) fn measured<T>(
    profile: Option<&EncodeProfile>,
    stage: EncodeStage,
    action: impl FnOnce() -> T,
) -> T {
    let start = profile.map(|_| Instant::now());
    let result = action();
    if let (Some(profile), Some(start)) = (profile, start) {
        let ix = stage as usize;
        profile.elapsed[ix].set(profile.elapsed[ix].get() + start.elapsed());
        profile.calls[ix].set(profile.calls[ix].get() + 1);
    }
    result
}
