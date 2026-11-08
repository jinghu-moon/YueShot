//! One-axis virtualization without a mandatory container, theme or role.
//! Native uniform arithmetic, known prefixes and ListState remain the engines.
mod anchor;
mod command;
mod element;
mod geometry;
mod grid;
mod grid_span;
mod grid_track;
mod masonry;
mod state;

pub use anchor::VirtualizerAnchorPosition;
pub use element::VirtualList;
pub use grid_span::{VirtualGridSpan, VirtualGridSpanSet};
pub use grid_track::VirtualGridTrack;
pub use grid::{VirtualGrid, VirtualGridCell, VirtualGridCellIdentity, VirtualGridCellKey, VirtualGridSnapshot};
pub use masonry::{
    VirtualMasonry, VirtualMasonryCache, VirtualMasonryCacheItem, VirtualMasonryCapabilities, VirtualMasonryItem, VirtualMasonrySnapshot,
    VirtualMasonryState,
};
pub use state::*;

use gpui::{Axis, ElementId, Pixels};
use std::{fmt, ops::Range, rc::Rc, time::Duration};

/// Immutable domain index. Implementations own key uniqueness and O(1)/O(log N)
/// reverse lookup; the controller does not clone all records or build a second map.
pub trait VirtualizerSource: 'static {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn key_at(&self, index: usize) -> ElementId;
    /// Bump when prepared or size-affecting content changes under the same key.
    /// This is the portable identity used by measurement caches across
    /// controllers. A live invalidate_items epoch does not replace it.
    fn revision_at(&self, _index: usize) -> u64 {
        0
    }
    fn index_of(&self, key: &ElementId) -> Option<usize>;
}

/// Read-only input to a working-set policy. Returned indexes supplement the
/// default range; consumers own sticky/drag/edit presentation and its state.
#[derive(Clone, Debug)]
pub struct VirtualizerRange {
    visible: Range<usize>,
    rendered: Range<usize>,
    count: usize,
}
impl VirtualizerRange {
    pub fn visible_range(&self) -> Range<usize> {
        self.visible.clone()
    }
    pub fn rendered_range(&self) -> Range<usize> {
        self.rendered.clone()
    }
    pub fn count(&self) -> usize {
        self.count
    }
}
pub type VirtualizerRangeExtractor = Rc<dyn Fn(&VirtualizerRange) -> Vec<usize>>;

#[derive(Clone)]
pub struct VirtualizerSizing(Sizing, Option<Rc<dyn Fn(usize) -> Pixels>>);
#[derive(Clone)]
enum Sizing {
    Fixed(Pixels),
    Known(geometry::KnownGeometry),
    Measured(Pixels),
}

impl VirtualizerSizing {
    pub fn fixed(extent: Pixels) -> Self {
        Self(Sizing::Fixed(extent), None)
    }
    pub fn known(extents: impl Into<Rc<[Pixels]>>) -> Self {
        Self(
            Sizing::Known(geometry::KnownGeometry::new(
                extents.into().iter().copied(),
                0.0,
            )),
            None,
        )
    }
    pub fn measured(estimate: Pixels) -> Self {
        Self(Sizing::Measured(estimate), None)
    }
    /// Per-item initial hints. Retained real measurements take precedence.
    pub fn measured_with_estimates(
        estimate: Pixels,
        estimates: impl Fn(usize) -> Pixels + 'static,
    ) -> Self {
        Self(Sizing::Measured(estimate), Some(Rc::new(estimates)))
    }
    fn estimate_at(&self, ix: usize) -> Pixels {
        self.1.as_ref().map_or_else(
            || match self.0 {
                Sizing::Measured(size) => size,
                _ => unreachable!(),
            },
            |estimate| estimate(ix),
        )
    }
    pub fn kind(&self) -> VirtualizerSizingKind {
        match self.0 {
            Sizing::Fixed(_) => VirtualizerSizingKind::Fixed,
            Sizing::Known(_) => VirtualizerSizingKind::Known,
            Sizing::Measured(_) => VirtualizerSizingKind::Measured,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualizerSizingKind {
    Fixed,
    Known,
    Measured,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualizerAttachment {
    Owned,
    Attached,
}

/// Value-only layout inputs. Typography/density/rem are represented by the
/// consumer's semantic layout revision, not an address or renderer identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualizerLayout {
    gap: Pixels,
    padding_start: Pixels,
    padding_end: Pixels,
    overscan: Pixels,
    reverse_overscan: Option<Pixels>,
    revision: u64,
    measurement_budget: usize,
    scroll_padding_start: Pixels,
    scroll_padding_end: Pixels,
}
impl Default for VirtualizerLayout {
    fn default() -> Self {
        Self {
            gap: gpui::px(0.),
            padding_start: gpui::px(0.),
            padding_end: gpui::px(0.),
            overscan: gpui::px(0.),
            reverse_overscan: None,
            revision: 0,
            measurement_budget: 1024,
            scroll_padding_start: gpui::px(0.),
            scroll_padding_end: gpui::px(0.),
        }
    }
}
impl VirtualizerLayout {
    /// Alignment clearance for sticky/overlay content, separate from content padding.
    pub fn scroll_padding(mut self, start: Pixels, end: Pixels) -> Self {
        self.scroll_padding_start = start;
        self.scroll_padding_end = end;
        self
    }
    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }
    pub fn padding(mut self, start: Pixels, end: Pixels) -> Self {
        self.padding_start = start;
        self.padding_end = end;
        self
    }
    pub fn overscan(mut self, overscan: Pixels) -> Self {
        self.overscan = overscan;
        self
    }
    /// Opt into bounded directional work: preserve `overscan` ahead and keep
    /// this reserve behind. Idle/unknown direction remains symmetric. The
    /// reserve must be finite, nonnegative and no larger than overscan.
    pub fn directional_overscan(mut self, reverse: Pixels) -> Self {
        self.reverse_overscan = Some(reverse);
        self
    }
    fn valid_overscan(&self) -> bool {
        self.reverse_overscan
            .is_none_or(|reverse| valid_nonnegative(reverse) && reverse <= self.overscan)
    }
    fn overscan_edges(&self, direction: Option<VirtualizerScrollDirection>) -> (Pixels, Pixels) {
        match (self.reverse_overscan, direction) {
            (Some(reverse), Some(VirtualizerScrollDirection::Forward)) => (reverse, self.overscan),
            (Some(reverse), Some(VirtualizerScrollDirection::Backward)) => (self.overscan, reverse),
            _ => (self.overscan, self.overscan),
        }
    }
    pub fn revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }
    /// Maximum native item layouts per synchronous measured list pass.
    /// Masonry always lays out its required workset, then budgets this many
    /// additional intrinsic roots exposed by correction. Remaining roots use
    /// estimated slots and can be measured in a progressing subsequent frame.
    /// Raise explicitly for unusually dense/large viewports; never zero.
    pub fn measurement_budget(mut self, count: usize) -> Self {
        self.measurement_budget = count;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualizerCapabilities {
    axis: Axis,
    sizing: VirtualizerSizingKind,
}
impl VirtualizerCapabilities {
    pub fn for_configuration(
        axis: Axis,
        sizing: VirtualizerSizingKind,
    ) -> Result<Self, VirtualizerError> {
        Ok(Self { axis, sizing })
    }
    pub fn axis(&self) -> Axis {
        self.axis
    }
    pub fn sizing(&self) -> VirtualizerSizingKind {
        self.sizing
    }
    pub fn supports_attached(&self) -> bool {
        true
    }
    pub fn supports_follow_tail(&self) -> bool {
        true
    }
    pub fn supports_smooth(&self) -> bool {
        // Measured requires a bounded, already measured path; unknown geometry
        // falls back to instant convergence, never full-path measurement.
        true
    }
    pub fn supports_scroll_to_offset(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualizerError {
    Unsupported(&'static str),
    InvalidGeometry(&'static str),
    InvalidSource,
    InvalidChange,
    BindingConflict,
    AlreadyBound,
    Detached,
    StaleSnapshot,
    StaleRequest,
    RequestAlreadyAccepted,
    MissingKey,
    WrongBackend,
}
impl fmt::Display for VirtualizerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "virtualizer: {self:?}")
    }
}
impl std::error::Error for VirtualizerError {}

#[derive(Clone, Debug)]
pub enum VirtualizerChange {
    /// Caller guarantees identities, content and hints outside the replaced
    /// range are unchanged. Only inserted estimates are evaluated. Use Reorder,
    /// Replace or explicit hint updates when existing hints also change.
    Splice {
        old: Range<usize>,
        inserted: usize,
    },
    Reorder,
    Replace {
        preserve_anchor: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VirtualizerAlign {
    Start,
    Center,
    End,
    #[default]
    Nearest,
}
#[derive(Clone, Copy, Debug, Default)]
pub enum VirtualizerMotion {
    #[default]
    Instant,
    Smooth {
        duration: Duration,
        max_distance: Pixels,
    },
    /// Measured linear lists only: jump near a distant target, then animate a
    /// bounded measured tail. This does not animate the intervening distance.
    JumpThenSmooth {
        duration: Duration,
        max_distance: Pixels,
    },
}
#[derive(Clone, Copy, Debug, Default)]
pub struct VirtualizerScrollOptions {
    align: VirtualizerAlign,
    motion: VirtualizerMotion,
    reduced_motion: bool,
}
impl VirtualizerScrollOptions {
    pub fn align(mut self, align: VirtualizerAlign) -> Self {
        self.align = align;
        self
    }
    pub fn motion(mut self, motion: VirtualizerMotion) -> Self {
        self.motion = motion;
        self
    }
    pub fn reduced_motion(mut self, reduced: bool) -> Self {
        self.reduced_motion = reduced;
        self
    }
}
impl From<VirtualizerAlign> for VirtualizerScrollOptions {
    fn from(align: VirtualizerAlign) -> Self {
        Self::default().align(align)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualizerCommandOutcome {
    Settled,
    Cancelled,
    Missing,
    NoProgress,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualizerScrollDirection {
    Forward,
    Backward,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualizerEvent {
    ScrollStateChanged {
        is_scrolling: bool,
        direction: Option<VirtualizerScrollDirection>,
    },
    RangeChanged {
        generation: u64,
        visible: Range<usize>,
        rendered: Range<usize>,
    },
    ScrollEnd,
    CommandFinished {
        id: u64,
        outcome: VirtualizerCommandOutcome,
    },
    Diagnostic(&'static str),
}

fn valid_positive(value: Pixels) -> bool {
    let value = f64::from(value);
    value.is_finite() && value > 0.0
}
fn valid_nonnegative(value: Pixels) -> bool {
    let value = f64::from(value);
    value.is_finite() && value >= 0.0
}
