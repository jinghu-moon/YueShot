//! Unstyled, crate-private anchored surfaces. Controllers own open, focus and
//! semantics; this module owns measured geometry, presentation and registration.
mod anchor;
mod arrow;
mod geometry;
mod host;
mod runtime;

pub(crate) use anchor::{PopupAnchor, PopupAnchorHandle};
pub(crate) use arrow::{ArrowSpec, ArrowStyle};
pub(crate) use geometry::union;
pub(crate) use host::{
    PopupCandidate, PopupHost, PopupMotion, PopupPlacement, PopupPointerMode, PopupSpec,
};
#[cfg(all(test, feature = "headless-tests"))]
pub(crate) use runtime::PopupSnapshot;
pub(crate) use runtime::{
    PopupDismissReason, PopupDismissRoute, PopupId, PopupRuntime, PopupSnapshotHandle,
};

pub(crate) fn init(cx: &mut gpui_kit::App) {
    runtime::init(cx);
}

#[cfg(test)]
mod tests;
