//! Window-owned notifications over Base lifecycle, keyed stack layout and live semantics.
mod card;
mod layer;
mod model;
mod state;
pub(crate) use layer::init;
pub use model::*;
pub use state::{
    ToastEvent, ToastPhase, ToastPlayback, ToastSnapshot, ToastTicket, YueToasterState,
};
gpui_kit::actions!(
    toast,
    [
        FocusToasts,
        CollapseToasts,
        DismissFocusedToast,
        NextToast,
        PreviousToast
    ]
);

/// The Root-installed presentation entry. Never mount a second notification owner.
///
/// Initialize Yue before creating the window's Base Root. Call from an event or
/// update callback, rather than from `Render`.
///
/// ```no_run
/// use gpui_kit::{App, Window};
/// use yue_component::{ToastError, ToastKind, ToastSpec, ToastTicket, YueToaster};
///
/// fn saved(window: &mut Window, cx: &mut App) -> Result<ToastTicket, ToastError> {
///     YueToaster::push(
///         ToastSpec::new("设置已保存").kind(ToastKind::Success).progress(true),
///         window,
///         cx,
///     )
/// }
/// ```
pub struct YueToaster;
impl YueToaster {
    pub fn for_window(
        window: &gpui_kit::Window,
        cx: &gpui_kit::App,
    ) -> Result<gpui_kit::Entity<YueToasterState>, ToastError> {
        YueToasterState::for_window(window, cx)
    }
    pub fn push(
        spec: ToastSpec,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> Result<ToastTicket, ToastError> {
        Self::for_window(window, cx)?.update(cx, |s, cx| s.push(spec, window, cx))
    }
}

#[cfg(all(test, feature = "headless-tests"))]
mod tests;
