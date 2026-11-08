//! Embedded static image viewing and aligned before/after comparison.
//! Hosts retain State and supply versioned resources. CPU helpers prepare raw
//! pixels or static encoded bytes on a host worker. No window, encoder, file
//! service, background task or image cache is owned by this component.
//!
//! ```
//! use yue_component::{ImagePreviewImage, ImagePreviewLimits, ImagePreviewPixels};
//! // Owned pixels move; borrowed pixels copy only when a full-size resource is
//! // needed. Decode/resize belongs on the host's background executor.
//! let preview = ImagePreviewImage::from_rgba8(1, 1, vec![255, 0, 0, 127], "红色")?;
//! assert_eq!(preview.image().as_bytes(0), Some([0, 0, 255, 127].as_slice()));
//! let source = vec![255; 16 * 16 * 4];
//! let proxy = ImagePreviewPixels::from_bgra8(16, 16, source.as_slice())?
//!     .into_preview(&ImagePreviewLimits::default().texture_budget(8, 256), "代理预览")?;
//! assert_eq!(proxy.pixel_size(), [16, 16]);
//! assert_eq!(proxy.texture_size(), [8, 8]);
//! assert!(proxy.is_proxy());
//! # Ok::<(), yue_component::ImagePreviewError>(())
//! ```
//!
//! ```no_run
//! use std::sync::Arc;
//! use gpui_kit::{App, AppContext as _, Entity, RenderImage, Window};
//! use yue_component::{ImagePreviewDocument, ImagePreviewError, ImagePreviewImage,
//!     ImagePreviewLoad, YueImagePreviewState};
//!
//! fn create_preview(image: Arc<RenderImage>, window: &mut Window, cx: &mut App)
//!     -> Result<Entity<YueImagePreviewState>, ImagePreviewError>
//! {
//!     let state = cx.new(|cx| YueImagePreviewState::new("preview", window, cx));
//!     let document = ImagePreviewDocument::new("document-1", 1)
//!         .with_source(ImagePreviewLoad::Ready(ImagePreviewImage::new(image, "截图")?));
//!     state.update(cx, |state, cx| state.set_document(document, window, cx))?;
//!     // Retain this Entity in the host; render YueImagePreview::new(&state)
//!     // inside a container with a definite height.
//!     Ok(state)
//! }
//! ```
mod color;
mod chrome;
mod element;
mod geometry;
mod model;
mod paint;
mod resource;
mod state;
pub use model::*;
pub use resource::{
    ImagePreviewColorSpace, ImagePreviewImage, ImagePreviewLimits, ImagePreviewPixels,
};
pub use state::YueImagePreviewState;

use gpui_kit::{AnyElement, App, Entity, IntoElement, RenderOnce, SharedString, Window};
use std::rc::Rc;

type FooterRenderer = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;
type OverlayRenderer = Rc<dyn Fn(ImagePreviewOverlay, &mut Window, &mut App)>;
type ExpandHandler = Rc<dyn Fn(&ImagePreviewView, &mut Window, &mut App)>;

/// Read-only, logical-window geometry for non-interactive paint overlays.
/// Draw only within this viewport; image positions are normalized to the frame.
#[derive(Clone, Copy, Debug)]
pub struct ImagePreviewOverlay {
    viewport: gpui_kit::Bounds<gpui_kit::Pixels>,
    image_bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
}
impl ImagePreviewOverlay {
    pub fn viewport(&self) -> gpui_kit::Bounds<gpui_kit::Pixels> {
        self.viewport
    }
    pub fn image_bounds(&self) -> gpui_kit::Bounds<gpui_kit::Pixels> {
        self.image_bounds
    }
    pub fn point(&self, u: f32, v: f32) -> gpui_kit::Point<gpui_kit::Pixels> {
        self.image_bounds.origin
            + gpui_kit::point(
                self.image_bounds.size.width * u,
                self.image_bounds.size.height * v,
            )
    }
}

#[derive(IntoElement)]
pub struct YueImagePreview {
    state: Entity<YueImagePreviewState>,
    theme: Option<crate::YueTheme>,
    animated: bool,
    disabled: bool,
    modes: bool,
    background_picker: bool,
    toolbar: ImagePreviewToolbar,
    toolbar_visible: bool,
    result_label: SharedString,
    footer: Option<FooterRenderer>,
    overlay: Option<OverlayRenderer>,
    on_expand: Option<ExpandHandler>,
}
impl YueImagePreview {
    pub fn new(state: &Entity<YueImagePreviewState>) -> Self {
        Self {
            state: state.clone(),
            theme: None,
            animated: true,
            disabled: false,
            modes: true,
            background_picker: true,
            toolbar: ImagePreviewToolbar::Footer,
            toolbar_visible: true,
            result_label: "处理后".into(),
            footer: None,
            overlay: None,
            on_expand: None,
        }
    }
    pub fn theme(mut self, theme: crate::YueTheme) -> Self {
        self.theme = Some(theme);
        self
    }
    pub fn animated(mut self, value: bool) -> Self {
        self.animated = value;
        self
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }
    pub fn show_modes(mut self, value: bool) -> Self {
        self.modes = value;
        self
    }
    pub fn show_background_picker(mut self, value: bool) -> Self {
        self.background_picker = value;
        self
    }
    /// Show a larger-preview command. The host opens its own Dialog/window and
    /// restores the supplied view into a separately retained preview State.
    pub fn on_expand(
        mut self,
        handler: impl Fn(&ImagePreviewView, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_expand = Some(Rc::new(handler));
        self
    }
    /// Footer (default) shares the viewer shell with its title and viewport.
    /// Floating explicitly overlays the viewport instead.
    pub fn toolbar(mut self, position: ImagePreviewToolbar) -> Self {
        self.toolbar = position;
        self
    }
    /// Floating toolbar visibility does not change image fitting or the camera.
    pub fn show_toolbar(mut self, value: bool) -> Self {
        self.toolbar_visible = value;
        self
    }
    pub fn result_label(mut self, label: impl Into<SharedString>) -> Self {
        self.result_label = label.into();
        self
    }
    pub fn footer(
        mut self,
        renderer: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.footer = Some(Box::new(renderer));
        self
    }
    /// Paint-only slot. Do not register input handlers or perform side effects.
    pub fn image_overlay(
        mut self,
        renderer: impl Fn(ImagePreviewOverlay, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.overlay = Some(Rc::new(renderer));
        self
    }
}

gpui_kit::actions!(
    yue_image_preview,
    [
        ZoomIn,
        ZoomOut,
        ActualSize,
        FitAll,
        FitWidth,
        Fullscreen,
        ExitFullscreen,
        PanLeft,
        PanRight,
        PanUp,
        PanDown
    ]
);
pub(crate) fn init(cx: &mut App) {
    use gpui_kit::KeyBinding;
    cx.bind_keys([
        KeyBinding::new("+", ZoomIn, Some("YueImagePreview")),
        KeyBinding::new("=", ZoomIn, Some("YueImagePreview")),
        KeyBinding::new("-", ZoomOut, Some("YueImagePreview")),
        KeyBinding::new("0", FitAll, Some("YueImagePreview")),
        KeyBinding::new("1", ActualSize, Some("YueImagePreview")),
        KeyBinding::new("w", FitWidth, Some("YueImagePreview")),
        KeyBinding::new("f", Fullscreen, Some("YueImagePreview")),
        KeyBinding::new("escape", ExitFullscreen, Some("YueImagePreview")),
        KeyBinding::new("left", PanLeft, Some("YueImagePreview")),
        KeyBinding::new("right", PanRight, Some("YueImagePreview")),
        KeyBinding::new("up", PanUp, Some("YueImagePreview")),
        KeyBinding::new("down", PanDown, Some("YueImagePreview")),
        KeyBinding::new("shift-left", PanLeft, Some("YueImagePreview")),
        KeyBinding::new("shift-right", PanRight, Some("YueImagePreview")),
        KeyBinding::new("shift-up", PanUp, Some("YueImagePreview")),
        KeyBinding::new("shift-down", PanDown, Some("YueImagePreview")),
    ]);
}

#[cfg(test)]
mod resource_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn image_preview_defaults_to_the_integrated_footer_toolbar() {
        let toolbar = std::mem::discriminant(&ImagePreviewToolbar::Footer);
        let default = std::mem::discriminant(&ImagePreviewToolbar::default());
        assert_eq!(toolbar, default);
    }
}
