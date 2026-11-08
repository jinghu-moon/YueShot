/// Reading position relative to an item's content, in logical pixels.
/// Blank space has its own meaning: shrinking content only clamps Within.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VirtualizerAnchorPosition {
    Before(f64),
    Within(f64),
    After(f64),
}
impl VirtualizerAnchorPosition {
    pub(super) fn is_valid(self) -> bool {
        let (Self::Before(distance) | Self::Within(distance) | Self::After(distance)) = self;
        distance.is_finite() && distance >= 0.0
    }
    pub(super) fn resolve(self, extent: f64, before: f64, after: f64) -> f64 {
        match self {
            Self::Before(distance) => -distance.min(before),
            Self::Within(distance) => distance.min(extent),
            Self::After(distance) => extent + distance.min(after),
        }
    }
}
