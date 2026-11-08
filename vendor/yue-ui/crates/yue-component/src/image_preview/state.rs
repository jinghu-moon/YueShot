use super::{
    geometry::{Camera, Viewport, aligned, snap_split},
    *,
};
use crate::{
    YueTheme,
    tokens::{duration, easing},
};
use gpui_kit::{
    App, Context, ElementId, EventEmitter, FocusHandle, Focusable, HitboxId, Subscription, Window,
};
use std::time::{Duration, Instant};

struct ViewMotion {
    start: Camera,
    end: Camera,
    at: Instant,
    duration: Duration,
    anchor: Option<[f32; 2]>,
    ease: Box<dyn Fn(f32) -> f32>,
}
impl ViewMotion {
    fn sample(&self, now: Instant, image: [f32; 2], vp: Viewport) -> Camera {
        let t = elapsed(now, self.at, self.duration);
        let t = (self.ease)(t);
        let c = self.start.interpolate(self.end, t);
        if let Some(anchor) = self.anchor {
            self.start
                .anchored(c.width, anchor, image, vp)
                .constrained(image, vp)
        } else {
            c
        }
    }
}
struct SplitMotion {
    start: f32,
    end: f32,
    at: Instant,
    duration: Duration,
    ease: Box<dyn Fn(f32) -> f32>,
}
fn elapsed(now: Instant, start: Instant, duration: Duration) -> f32 {
    if duration.is_zero() {
        1.
    } else {
        (now.saturating_duration_since(start).as_secs_f32() / duration.as_secs_f32()).clamp(0., 1.)
    }
}

#[derive(Clone, Copy)]
pub(super) enum Gesture {
    Pan {
        at: [f32; 2],
        camera: Camera,
        moved: bool,
        hitbox: HitboxId,
    },
    Split {
        offset: f32,
        hitbox: HitboxId,
    },
}
impl Gesture {
    fn hitbox(self) -> HitboxId {
        match self {
            Self::Pan { hitbox, .. } | Self::Split { hitbox, .. } => hitbox,
        }
    }
}

/// Retain one State per mounted viewport. Setters notify and are silent;
/// user operations emit semantic events after committing logical state.
pub struct YueImagePreviewState {
    pub(super) id: ElementId,
    pub(super) document: Option<ImagePreviewDocument>,
    gallery: Vec<ImagePreviewDocument>,
    gallery_index: usize,
    filmstrip_visible: bool,
    filmstrip_pointer_active: bool,
    filmstrip_generation: u64,
    pub(super) result: Option<ImagePreviewImage>,
    result_revision: Option<u64>,
    pub(super) mode: ImagePreviewMode,
    pub(super) fit: ImagePreviewFit,
    initial_fit: ImagePreviewFit,
    pub(super) background: ImagePreviewBackground,
    pub(super) pixel_grid: bool,
    pub(super) sampling: ImagePreviewSampling,
    pub(super) camera: Camera,
    pub(super) viewport: Viewport,
    pub(super) split: f32,
    last_middle: f32,
    view_motion: Option<ViewMotion>,
    split_motion: Option<SplitMotion>,
    pub(super) gesture: Option<Gesture>,
    pub(super) focus: FocusHandle,
    pub(super) grip_focus: FocusHandle,
    pub(super) animated: bool,
    pub(super) disabled: bool,
    pub(super) theme: YueTheme,
    needs_fit: bool,
    pub(super) mounted: bool,
    pub(super) hitbox: Option<HitboxId>,
    render_errors: Vec<(gpui_kit::ImageId, gpui_kit::SharedString)>,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<ImagePreviewEvent> for YueImagePreviewState {}
impl Focusable for YueImagePreviewState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl YueImagePreviewState {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |s, w, cx| {
            if !w.is_window_active() {
                s.cancel_gesture(w);
                s.finish_motion();
                cx.notify();
            }
        });
        let visibility = cx.observe_window_visibility(window, |s, visibility, w, cx| {
            if !visibility.is_visible() {
                s.cancel_gesture(w);
                s.finish_motion();
                cx.notify();
            }
        });
        let focus = cx.focus_handle().tab_stop(true);
        let grip_focus = cx.focus_handle().tab_stop(true);
        let mut subscriptions = vec![activation, visibility];
        for handle in [&focus, &grip_focus] {
            subscriptions.push(cx.on_focus(handle, window, |_, _, cx| cx.notify()));
            subscriptions.push(cx.on_blur(handle, window, |s, w, cx| {
                s.cancel_gesture(w);
                cx.notify();
            }));
        }
        Self {
            id: id.into(),
            document: None,
            gallery: Vec::new(),
            gallery_index: 0,
            filmstrip_visible: false,
            filmstrip_pointer_active: false,
            filmstrip_generation: 0,
            result: None,
            result_revision: None,
            mode: ImagePreviewMode::Source,
            fit: ImagePreviewFit::FitAll,
            initial_fit: ImagePreviewFit::FitAll,
            background: ImagePreviewBackground::CheckerDark,
            pixel_grid: false,
            sampling: ImagePreviewSampling::Smooth,
            camera: Camera::default(),
            viewport: Viewport::default(),
            split: 1.,
            last_middle: 0.5,
            view_motion: None,
            split_motion: None,
            gesture: None,
            focus,
            grip_focus,
            animated: true,
            disabled: false,
            theme: crate::current_theme(cx),
            needs_fit: true,
            mounted: false,
            hitbox: None,
            render_errors: Vec::new(),
            _subscriptions: subscriptions,
        }
    }
    pub fn with_initial_fit(mut self, fit: ImagePreviewFit) -> Self {
        assert!(
            fit != ImagePreviewFit::Manual,
            "initial preview fit must be automatic"
        );
        self.initial_fit = fit;
        self.fit = fit;
        self
    }
    pub fn id(&self) -> &ElementId {
        &self.id
    }
    pub fn snapshot(&self) -> ImagePreviewSnapshot {
        let image = self.reference();
        ImagePreviewSnapshot {
            document_id: self.document.as_ref().map(|d| d.id.clone()),
            revision: self.document.as_ref().map_or(0, |d| d.revision),
            result_revision: self.result_revision,
            mode: self.mode,
            fit: self.fit,
            zoom: image.map_or(1., |i| self.camera.zoom(i.dimensions())),
            center: self.camera.center,
            split: self.split,
            background: self.background,
            pixel_grid: self.pixel_grid,
            reference_size: image.map(|i| i.pixel_size()),
            texture_size: image.map(|i| i.texture_size()),
            proxy: self.has_proxy(),
            pixel_grid_available: self.is_pixel_grid_available(),
            sampling: self.sampling,
        }
    }
    pub fn set_document(
        &mut self,
        document: ImagePreviewDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        if document.id.trim().is_empty() {
            return Err(ImagePreviewError::EmptyDocumentId);
        }
        let same = self.document.as_ref().is_some_and(|d| d.id == document.id);
        if same
            && self
                .document
                .as_ref()
                .is_some_and(|d| document.revision < d.revision)
        {
            return Err(ImagePreviewError::StaleRevision);
        }
        if self.document.as_ref() == Some(&document) {
            return Ok(());
        }
        self.gallery.clear();
        self.gallery_index = 0;
        self.filmstrip_visible = false;
        self.filmstrip_pointer_active = false;
        self.filmstrip_generation = self.filmstrip_generation.wrapping_add(1);
        let same_source = self
            .source()
            .zip(document.source.ready())
            .is_some_and(|(a, b)| a.image.id == b.image.id && a.pixel_size() == b.pixel_size());
        self.cancel_gesture(window);
        self.finish_motion();
        self.render_errors.clear();
        if !same {
            self.result = None;
            self.result_revision = None;
            self.mode = ImagePreviewMode::Source;
            self.fit = self.initial_fit;
            self.camera = Camera::default();
            self.needs_fit = true;
            self.split = 1.;
            self.last_middle = 0.5;
        }
        match &document.result {
            ImagePreviewLoad::Ready(i) => {
                self.result = Some(i.clone());
                self.result_revision = Some(document.revision);
            }
            ImagePreviewLoad::Loading | ImagePreviewLoad::Failed(_)
                if same
                    && same_source
                    && document.full_frame
                    && self.document.as_ref().is_some_and(|d| d.full_frame) => {}
            _ => {
                self.result = None;
                self.result_revision = None;
            }
        }
        self.document = Some(document);
        if !self.mode_available(self.mode) {
            self.mode = ImagePreviewMode::Source;
        }
        if self.mode != ImagePreviewMode::Compare && self.grip_focus.is_focused(window) {
            self.focus.focus(window, cx);
        }
        if let Some(image) = self
            .reference()
            .map(ImagePreviewImage::dimensions)
            .filter(|_| self.viewport.is_valid())
        {
            if self.needs_fit || self.fit != ImagePreviewFit::Manual {
                self.camera = self.viewport.fit(image, self.fit);
                self.needs_fit = false;
            } else {
                self.camera = self.camera.constrained(image, self.viewport);
            }
        }
        cx.notify();
        Ok(())
    }
    /// Replace the current document with a host-owned image collection.
    /// Each item keeps its own source/result pair and view can be restored by the host.
    pub fn set_gallery(
        &mut self,
        items: Vec<ImagePreviewDocument>,
        active_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        let Some(active) = items.get(active_index).cloned() else {
            return Err(ImagePreviewError::InvalidValue);
        };
        self.set_document(active, window, cx)?;
        self.gallery = items;
        self.gallery_index = active_index;
        cx.notify();
        Ok(())
    }
    pub fn gallery(&self) -> &[ImagePreviewDocument] {
        &self.gallery
    }
    pub fn gallery_index(&self) -> usize {
        self.gallery_index
    }
    pub(super) fn filmstrip_visible(&self) -> bool {
        self.gallery.len() > 1 && self.filmstrip_visible
    }
    pub(super) fn update_filmstrip_pointer(
        &mut self,
        inside_reveal_area: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gallery.len() < 2 {
            self.filmstrip_visible = false;
            self.filmstrip_pointer_active = false;
            return;
        }
        if inside_reveal_area {
            self.filmstrip_pointer_active = true;
            self.filmstrip_generation = self.filmstrip_generation.wrapping_add(1);
            if !self.filmstrip_visible {
                self.filmstrip_visible = true;
                cx.notify();
            }
            return;
        }
        if !self.filmstrip_pointer_active {
            return;
        }
        self.filmstrip_pointer_active = false;
        self.filmstrip_generation = self.filmstrip_generation.wrapping_add(1);
        let generation = self.filmstrip_generation;
        let weak = cx.weak_entity();
        cx.spawn_in(window, async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1000))
                .await;
            let _ = weak.update(cx, |s, cx| {
                if s.filmstrip_generation == generation && !s.filmstrip_pointer_active {
                    s.filmstrip_visible = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub fn select_gallery_item(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        let Some(document) = self.gallery.get(index).cloned() else {
            return Err(ImagePreviewError::InvalidValue);
        };
        if self.gallery_index == index {
            return Ok(());
        }
        let items = self.gallery.clone();
        let filmstrip_visible = self.filmstrip_visible;
        let filmstrip_pointer_active = self.filmstrip_pointer_active;
        self.set_document(document, window, cx)?;
        self.gallery = items;
        self.gallery_index = index;
        self.filmstrip_visible = filmstrip_visible;
        self.filmstrip_pointer_active = filmstrip_pointer_active;
        cx.emit(ImagePreviewEvent::GalleryChanged(
            self.snapshot(),
            index,
            self.gallery.len(),
            ImagePreviewChangeSource::Pointer,
        ));
        cx.notify();
        Ok(())
    }
    pub fn set_mode(
        &mut self,
        mode: ImagePreviewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        self.change_mode(mode, window, cx)?;
        cx.notify();
        Ok(())
    }
    pub fn set_background(&mut self, background: ImagePreviewBackground, cx: &mut Context<Self>) {
        if self.background != background {
            self.background = background;
            cx.notify();
        }
    }
    pub fn set_pixel_grid_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.pixel_grid != enabled {
            self.pixel_grid = enabled;
            cx.notify();
        }
    }
    pub fn set_sampling(&mut self, sampling: ImagePreviewSampling, cx: &mut Context<Self>) {
        if self.sampling != sampling {
            self.sampling = sampling;
            cx.notify();
        }
    }
    pub fn is_pixel_grid_available(&self) -> bool {
        cfg!(target_os = "windows")
            && !self.has_proxy()
            && self
                .reference()
                .is_some_and(|i| self.camera.zoom(i.dimensions()) >= 4.)
    }
    pub(super) fn has_proxy(&self) -> bool {
        self.reference().is_some_and(ImagePreviewImage::is_proxy)
            || (self.mode == ImagePreviewMode::Compare
                && self.source().is_some_and(ImagePreviewImage::is_proxy))
    }
    pub fn capture_view(&self) -> ImagePreviewView {
        ImagePreviewView {
            document: self.document.clone(),
            result: self.result.clone(),
            gallery: self.gallery.clone(),
            gallery_index: self.gallery_index,
            snapshot: self.snapshot(),
            last_middle: self.last_middle,
        }
    }
    /// Silently restore an independent view, including a retained older result.
    /// Automatic fits use this viewport; Manual retains physical zoom and center.
    pub fn restore_view(
        &mut self,
        view: &ImagePreviewView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        if view.document.is_none() || view.snapshot.reference_size.is_none() {
            return Err(ImagePreviewError::UnavailableMode);
        }
        self.cancel_gesture(window);
        self.finish_motion();
        self.render_errors.clear();
        self.document = view.document.clone();
        self.result = view.result.clone();
        self.gallery = view.gallery.clone();
        self.gallery_index = view.gallery_index;
        self.filmstrip_visible = false;
        self.filmstrip_pointer_active = false;
        self.filmstrip_generation = self.filmstrip_generation.wrapping_add(1);
        let snapshot = &view.snapshot;
        self.result_revision = snapshot.result_revision;
        self.mode = snapshot.mode;
        self.fit = snapshot.fit;
        self.background = snapshot.background;
        self.pixel_grid = snapshot.pixel_grid;
        self.sampling = snapshot.sampling;
        self.split = snapshot.split;
        self.last_middle = view.last_middle;
        self.camera = Camera {
            center: snapshot.center,
            width: snapshot.zoom * snapshot.reference_size.unwrap()[0] as f32,
        };
        self.needs_fit = self.fit != ImagePreviewFit::Manual;
        if self.viewport.is_valid() {
            self.measure(self.viewport);
            if self.fit == ImagePreviewFit::Manual {
                self.camera = self
                    .camera
                    .constrained(self.reference().unwrap().dimensions(), self.viewport);
            }
        }
        cx.notify();
        Ok(())
    }
    pub fn fit_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fit_to(ImagePreviewFit::FitAll, window, cx);
    }
    pub fn fit_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fit_to(ImagePreviewFit::FitWidth, window, cx);
    }
    pub fn actual_size(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        self.zoom_at(1., self.viewport.midpoint(), true, window, cx)
    }
    /// Absolute physical image-pixel scale, anchored in logical window coordinates.
    pub fn zoom_at(
        &mut self,
        zoom: f32,
        anchor: [f32; 2],
        animated: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        if !zoom.is_finite() || zoom <= 0. || anchor.iter().any(|v| !v.is_finite()) {
            return Err(ImagePreviewError::InvalidValue);
        }
        let Some(image) = self
            .reference()
            .map(ImagePreviewImage::dimensions)
            .filter(|_| self.viewport.is_valid())
        else {
            return Err(ImagePreviewError::UnavailableMode);
        };
        self.cancel_gesture(window);
        let current = self.display_camera(Instant::now());
        let limits = self.viewport.zoom_limits(image);
        let width = zoom.clamp(limits[0], limits[1]) * image[0];
        let next = current
            .anchored(width, anchor, image, self.viewport)
            .constrained(image, self.viewport);
        self.fit = ImagePreviewFit::Manual;
        self.start_view(
            current,
            next,
            animated.then_some(anchor),
            animated,
            window,
            cx,
        );
        cx.notify();
        Ok(())
    }
    pub fn set_split(
        &mut self,
        split: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        if !split.is_finite() {
            return Err(ImagePreviewError::InvalidValue);
        }
        if !self.mode_available(ImagePreviewMode::Compare) {
            return Err(ImagePreviewError::UnavailableMode);
        }
        self.cancel_gesture(window);
        self.mode = ImagePreviewMode::Compare;
        self.split_motion = None;
        self.assign_split(split.clamp(0., 1.));
        cx.notify();
        Ok(())
    }
    pub fn is_mode_available(&self, mode: ImagePreviewMode) -> bool {
        self.mode_available(mode)
    }
    pub(super) fn source(&self) -> Option<&ImagePreviewImage> {
        self.document.as_ref()?.source.ready()
    }
    pub(super) fn reference(&self) -> Option<&ImagePreviewImage> {
        if self.mode == ImagePreviewMode::Source {
            self.source()
        } else {
            self.result.as_ref()
        }
    }
    pub(super) fn mode_available(&self, mode: ImagePreviewMode) -> bool {
        match mode {
            ImagePreviewMode::Source => self.source().is_some(),
            ImagePreviewMode::Result => self.result.is_some(),
            ImagePreviewMode::Compare => {
                self.document.as_ref().is_some_and(|d| d.full_frame)
                    && self
                        .source()
                        .zip(self.result.as_ref())
                        .is_some_and(|(a, b)| aligned(a.dimensions(), b.dimensions()))
            }
        }
    }
    pub(super) fn child_id(&self, name: &'static str) -> ElementId {
        ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), name.into())
    }
    pub(super) fn display_camera(&self, now: Instant) -> Camera {
        self.view_motion
            .as_ref()
            .zip(self.reference())
            .map_or(self.camera, |(m, i)| {
                m.sample(now, i.dimensions(), self.viewport)
            })
    }
    pub(super) fn display_split(&self, now: Instant) -> f32 {
        self.split_motion.as_ref().map_or(self.split, |m| {
            m.start + (m.end - m.start) * (m.ease)(elapsed(now, m.at, m.duration))
        })
    }
    pub(super) fn is_animating(&self, now: Instant) -> bool {
        self.view_motion
            .as_ref()
            .is_some_and(|m| elapsed(now, m.at, m.duration) < 1.)
            || self
                .split_motion
                .as_ref()
                .is_some_and(|m| elapsed(now, m.at, m.duration) < 1.)
    }
    pub(super) fn is_comparing(&self, now: Instant) -> bool {
        self.mode_available(ImagePreviewMode::Compare)
            && (self.mode == ImagePreviewMode::Compare
                || self
                    .split_motion
                    .as_ref()
                    .is_some_and(|m| elapsed(now, m.at, m.duration) < 1.))
    }
    pub(super) fn bind_hitbox(&mut self, hitbox: HitboxId, w: &mut Window) {
        if let Some(g) = self.gesture
            && w.captured_hitbox() == Some(g.hitbox())
        {
            self.gesture = Some(match g {
                Gesture::Pan {
                    at, camera, moved, ..
                } => Gesture::Pan {
                    at,
                    camera,
                    moved,
                    hitbox,
                },
                Gesture::Split { offset, .. } => Gesture::Split { offset, hitbox },
            });
            w.capture_pointer(hitbox);
        }
        self.hitbox = Some(hitbox);
    }
    pub(super) fn finish_motion(&mut self) {
        self.view_motion = None;
        self.split_motion = None;
    }
    pub(super) fn measure(&mut self, vp: Viewport) -> bool {
        if !vp.is_valid() {
            return false;
        }
        let changed = vp != self.viewport;
        self.viewport = vp;
        if changed {
            self.finish_motion();
        }
        if (changed || self.needs_fit)
            && let Some(image) = self.reference().map(ImagePreviewImage::dimensions)
        {
            self.camera = if self.needs_fit || self.fit != ImagePreviewFit::Manual {
                vp.fit(image, self.fit)
            } else {
                self.camera.constrained(image, vp)
            };
            self.needs_fit = false;
            return true;
        }
        false
    }
    fn start_view(
        &mut self,
        current: Camera,
        next: Camera,
        anchor: Option<[f32; 2]>,
        animated: bool,
        window: &Window,
        cx: &App,
    ) {
        self.camera = next;
        self.view_motion = if animated
            && self.animated
            && !cx.reduce_motion()
            && window.is_visible()
            && current != next
        {
            Some(ViewMotion {
                start: current,
                end: next,
                anchor,
                at: Instant::now(),
                duration: duration(self.theme, "--image-preview-view-duration"),
                ease: easing(self.theme, "--image-preview-view-ease"),
            })
        } else {
            None
        };
    }
    fn fit_to(&mut self, fit: ImagePreviewFit, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_gesture(window);
        self.fit = fit;
        if let Some(image) = self
            .reference()
            .map(ImagePreviewImage::dimensions)
            .filter(|_| self.viewport.is_valid())
        {
            let current = self.display_camera(Instant::now());
            self.start_view(
                current,
                self.viewport.fit(image, fit),
                None,
                true,
                window,
                cx,
            );
        } else {
            self.needs_fit = true;
        }
        cx.notify();
    }
    fn change_mode(
        &mut self,
        mode: ImagePreviewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ImagePreviewError> {
        if !self.mode_available(mode) {
            return Err(ImagePreviewError::UnavailableMode);
        }
        if mode == self.mode {
            return Ok(());
        }
        self.cancel_gesture(window);
        let displayed = self.display_camera(Instant::now());
        let from = if self.mode_available(ImagePreviewMode::Compare) {
            self.display_split(Instant::now())
        } else {
            0.
        };
        self.camera = displayed;
        self.view_motion = None;
        self.mode = mode;
        if mode != ImagePreviewMode::Compare && self.grip_focus.is_focused(window) {
            self.focus.focus(window, cx);
        }
        if !self.mode_available(ImagePreviewMode::Compare) {
            self.split_motion = None;
            self.fit_to(ImagePreviewFit::FitAll, window, cx);
            self.view_motion = None;
        } else {
            self.split = match mode {
                ImagePreviewMode::Source => 1.,
                ImagePreviewMode::Compare => self.last_middle,
                ImagePreviewMode::Result => 0.,
            };
            self.split_motion = if self.animated && !cx.reduce_motion() && from != self.split {
                Some(SplitMotion {
                    start: from,
                    end: self.split,
                    at: Instant::now(),
                    duration: duration(self.theme, "--image-preview-compare-duration"),
                    ease: easing(self.theme, "--image-preview-compare-ease"),
                })
            } else {
                None
            };
            if let Some(image) = self
                .reference()
                .map(ImagePreviewImage::dimensions)
                .filter(|_| self.viewport.is_valid())
            {
                self.camera = self.camera.constrained(image, self.viewport);
            }
        }
        Ok(())
    }
    pub(super) fn user_mode(
        &mut self,
        mode: ImagePreviewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled || mode == self.mode {
            return;
        }
        let before = self.snapshot();
        if self.change_mode(mode, window, cx).is_ok() {
            self.emit_changes(before, ImagePreviewChangeSource::Toolbar, cx);
            cx.notify();
        }
    }
    pub(super) fn user_background(&mut self, bg: ImagePreviewBackground, cx: &mut Context<Self>) {
        if self.disabled || bg == self.background {
            return;
        }
        let before = self.snapshot();
        self.background = bg;
        self.emit_changes(before, ImagePreviewChangeSource::Toolbar, cx);
        cx.notify();
    }
    pub(super) fn user_zoom(
        &mut self,
        factor: f32,
        anchor: [f32; 2],
        animated: bool,
        source: ImagePreviewChangeSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let before = self.snapshot();
        let Some(image) = self.reference().map(ImagePreviewImage::dimensions) else {
            return;
        };
        let zoom = if animated {
            self.camera.zoom(image)
        } else {
            self.display_camera(Instant::now()).zoom(image)
        };
        if self
            .zoom_at(zoom * factor, anchor, animated, window, cx)
            .is_ok()
        {
            self.emit_changes(before, source, cx);
        }
    }
    pub(super) fn user_fit(
        &mut self,
        fit: ImagePreviewFit,
        actual: bool,
        source: ImagePreviewChangeSource,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let before = self.snapshot();
        if actual {
            let _ = self.actual_size(w, cx);
        } else {
            self.fit_to(fit, w, cx);
        }
        self.emit_changes(before, source, cx);
    }
    pub(super) fn user_pan(&mut self, delta: [f32; 2], w: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if let Some(image) = self
            .reference()
            .map(ImagePreviewImage::dimensions)
            .filter(|_| self.viewport.is_valid())
        {
            let before = self.snapshot();
            self.cancel_gesture(w);
            self.camera = self
                .display_camera(Instant::now())
                .pan(delta, image, self.viewport);
            self.view_motion = None;
            self.fit = ImagePreviewFit::Manual;
            self.emit_changes(before, ImagePreviewChangeSource::Keyboard, cx);
            cx.notify();
        }
    }
    pub(super) fn user_split(
        &mut self,
        v: f32,
        source: ImagePreviewChangeSource,
        cx: &mut Context<Self>,
    ) {
        if self.disabled || !v.is_finite() || !self.mode_available(ImagePreviewMode::Compare) {
            return;
        }
        let before = self.snapshot();
        self.mode = ImagePreviewMode::Compare;
        self.split_motion = None;
        self.assign_split(snap_split(v.clamp(0., 1.)));
        self.emit_changes(before, source, cx);
        cx.notify();
    }
    fn assign_split(&mut self, v: f32) {
        self.split = v;
        if v > 0. && v < 1. {
            self.last_middle = v;
        }
    }
    fn emit_changes(
        &self,
        before: ImagePreviewSnapshot,
        source: ImagePreviewChangeSource,
        cx: &mut Context<Self>,
    ) {
        let next = self.snapshot();
        if before.mode != next.mode {
            cx.emit(ImagePreviewEvent::ModeChanged(next.clone(), source));
        }
        if before.zoom != next.zoom || before.center != next.center || before.fit != next.fit {
            cx.emit(ImagePreviewEvent::ViewChanged(next.clone(), source));
        }
        if before.split != next.split {
            cx.emit(ImagePreviewEvent::SplitChanged(next.clone(), source));
        }
        if before.background != next.background {
            cx.emit(ImagePreviewEvent::BackgroundChanged(next.clone(), source));
        }
        if before.pixel_grid != next.pixel_grid {
            cx.emit(ImagePreviewEvent::PixelGridChanged(next, source));
        }
    }
    pub(super) fn user_pixel_grid(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.disabled || self.pixel_grid == enabled {
            return;
        }
        let before = self.snapshot();
        self.pixel_grid = enabled;
        self.emit_changes(before, ImagePreviewChangeSource::Toolbar, cx);
        cx.notify();
    }
    pub(super) fn user_sampling(&mut self, sampling: ImagePreviewSampling, cx: &mut Context<Self>) {
        if self.disabled || self.sampling == sampling {
            return;
        }
        self.set_sampling(sampling, cx);
        cx.emit(ImagePreviewEvent::SamplingChanged(
            self.snapshot(),
            ImagePreviewChangeSource::Toolbar,
        ));
    }
    pub(super) fn cancel_gesture(&mut self, w: &mut Window) {
        if let Some(g) = self.gesture.take()
            && w.captured_hitbox() == Some(g.hitbox())
        {
            w.release_pointer();
        }
    }
    pub(super) fn begin(
        &mut self,
        position: [f32; 2],
        split: bool,
        hitbox: HitboxId,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled || self.reference().is_none() {
            return;
        }
        self.cancel_gesture(w);
        self.camera = self.display_camera(Instant::now());
        self.view_motion = None;
        if split {
            let current = self.display_split(Instant::now());
            self.user_split(current, ImagePreviewChangeSource::Pointer, cx);
            self.grip_focus.focus(w, cx);
            self.gesture = Some(Gesture::Split {
                offset: position[0] - self.viewport.origin[0] - current * self.viewport.size[0],
                hitbox,
            });
        } else {
            self.focus.focus(w, cx);
            self.gesture = Some(Gesture::Pan {
                at: position,
                camera: self.camera,
                moved: false,
                hitbox,
            });
        }
        w.capture_pointer(hitbox);
        cx.notify();
    }
    pub(super) fn drag(&mut self, position: [f32; 2], w: &mut Window, cx: &mut Context<Self>) {
        let Some(g) = self.gesture else {
            return;
        };
        if self.disabled || w.captured_hitbox() != Some(g.hitbox()) {
            self.cancel_gesture(w);
            return;
        }
        match g {
            Gesture::Split { offset, .. } => self.user_split(
                (position[0] - self.viewport.origin[0] - offset) / self.viewport.size[0],
                ImagePreviewChangeSource::Pointer,
                cx,
            ),
            Gesture::Pan {
                at,
                camera,
                moved,
                hitbox,
            } => {
                let delta = [position[0] - at[0], position[1] - at[1]];
                if !moved && delta[0].hypot(delta[1]) <= 2. {
                    return;
                }
                if let Some(image) = self.reference().map(ImagePreviewImage::dimensions) {
                    let before = self.snapshot();
                    let next = camera.pan(delta, image, self.viewport);
                    if next != self.camera {
                        self.camera = next;
                        self.fit = ImagePreviewFit::Manual;
                        self.emit_changes(before, ImagePreviewChangeSource::Pointer, cx);
                    }
                    self.gesture = Some(Gesture::Pan {
                        at,
                        camera,
                        moved: true,
                        hitbox,
                    });
                    cx.notify();
                }
            }
        }
    }
    pub(super) fn retry(&self, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if let Some(d) = &self.document {
            cx.emit(ImagePreviewEvent::RetryRequested {
                document_id: d.id.clone(),
                revision: d.revision,
                result: !matches!(d.source, ImagePreviewLoad::Failed(_))
                    && !self
                        .source()
                        .is_some_and(|i| self.render_failed(i.image.id)),
            });
        }
    }
    pub(super) fn status(&self) -> (String, bool, bool) {
        let Some(d) = &self.document else {
            return ("暂无图片".into(), false, false);
        };
        match &d.source {
            ImagePreviewLoad::Empty => return ("暂无图片".into(), false, false),
            ImagePreviewLoad::Loading => return ("正在加载图片".into(), true, false),
            ImagePreviewLoad::Failed(e) => return (format!("图片加载失败：{e}"), false, true),
            ImagePreviewLoad::Ready(_) => {}
        }
        if let Some((_, error)) = self.render_errors.iter().find(|(id, _)| {
            self.reference().is_some_and(|i| i.image.id == *id)
                || (self.mode == ImagePreviewMode::Compare
                    && self.source().is_some_and(|i| i.image.id == *id))
        }) {
            return (format!("图片绘制失败：{error}"), false, true);
        }
        let old = self.result.is_some();
        match &d.result {
            ImagePreviewLoad::Loading => (
                if old {
                    "正在更新 · 当前为上一版"
                } else {
                    "正在生成预览"
                }
                .into(),
                true,
                false,
            ),
            ImagePreviewLoad::Failed(e) => (
                format!(
                    "{}：{e}",
                    if old {
                        "更新失败 · 当前为上一版"
                    } else {
                        "结果生成失败"
                    }
                ),
                false,
                true,
            ),
            ImagePreviewLoad::Ready(_) if !self.mode_available(ImagePreviewMode::Compare) => (
                "两图未声明对齐或画面比例不同，仅支持单图查看".into(),
                false,
                false,
            ),
            _ => (String::new(), false, false),
        }
    }
    pub(super) fn render_failed(&self, id: gpui_kit::ImageId) -> bool {
        self.render_errors.iter().any(|(failed, _)| *failed == id)
    }
    pub(super) fn record_render_error(&mut self, id: gpui_kit::ImageId, message: String) -> bool {
        if self.render_failed(id)
            || !self
                .source()
                .into_iter()
                .chain(self.result.iter())
                .any(|i| i.image.id == id)
        {
            return false;
        }
        self.render_errors.push((id, message.into()));
        true
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
