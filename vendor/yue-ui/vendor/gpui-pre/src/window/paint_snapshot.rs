use super::*;
use crate::scene::PaintOperation;

/// Frozen paint commands from one window, without focus, input, accessibility,
/// tooltips, deferred overlays, element state or frame callbacks.
///
/// Capture during paint and replay only in the same window and at the same
/// display scale. Use for short exit transitions, not as a general view cache.
/// Images retain their source and are repainted through the atlas API, so image
/// cache eviction cannot leave a snapshot pointing at a recycled atlas tile.
pub struct PaintSnapshot {
    window: AnyWindowHandle,
    scale: f32,
    origin: Point<Pixels>,
    operations: Vec<SnapshotOperation>,
}

enum SnapshotOperation {
    Scene(PaintOperation),
    Image(SnapshotImage),
}

#[derive(Clone)]
pub(super) struct SnapshotImage {
    pub(super) index: usize,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) image_bounds: Bounds<Pixels>,
    pub(super) corner_radii: Corners<Pixels>,
    pub(super) data: Arc<RenderImage>,
    pub(super) frame_index: usize,
    pub(super) grayscale: bool,
    pub(super) sampling: u32,
    pub(super) mask: ContentMask<Pixels>,
    pub(super) opacity: f32,
}

impl Window {
    /// Runs paint normally and also records its scene commands. This never
    /// captures event handlers or deferred draws outside the closure.
    pub fn capture_paint<R>(
        &mut self,
        origin: Point<Pixels>,
        paint: impl FnOnce(&mut Self) -> R,
    ) -> (R, PaintSnapshot) {
        self.invalidator.debug_assert_paint();
        let start = self.next_frame.scene.len();
        let result = paint(self);
        let mut image_iter = self
            .next_frame
            .snapshot_images
            .iter()
            .filter(|image| image.index >= start)
            .peekable();
        let operations = self.next_frame.scene.paint_operations[start..]
            .iter()
            .enumerate()
            .map(|(offset, operation)| {
                if image_iter
                    .peek()
                    .is_some_and(|image| image.index == start + offset)
                {
                    SnapshotOperation::Image(image_iter.next().unwrap().clone())
                } else {
                    SnapshotOperation::Scene(operation.clone())
                }
            })
            .collect();
        (
            result,
            PaintSnapshot {
                window: self.handle,
                scale: self.scale_factor(),
                origin,
                operations,
            },
        )
    }

    /// Paints a frozen frame at a new origin, intersecting its recorded masks
    /// with the current content mask. Returns false for a different window or
    /// scale; snapshots must be invalidated when layout/style policy changes.
    pub fn paint_snapshot(&mut self, snapshot: &PaintSnapshot, origin: Point<Pixels>) -> bool {
        self.invalidator.debug_assert_paint();
        if snapshot.window != self.handle || snapshot.scale != self.scale_factor() {
            return false;
        }
        let delta = origin - snapshot.origin;
        let scaled_delta = delta.scale(self.scale_factor());
        let clip = self.snapped_content_mask();
        for operation in &snapshot.operations {
            match operation {
                SnapshotOperation::Scene(PaintOperation::Primitive(primitive)) => {
                    let mut primitive = primitive.clone();
                    primitive.translate_and_clip(scaled_delta, clip);
                    self.next_frame.scene.insert_primitive(primitive);
                }
                SnapshotOperation::Scene(PaintOperation::StartLayer(bounds)) => {
                    let mut bounds = *bounds;
                    bounds.origin += scaled_delta;
                    self.next_frame
                        .scene
                        .push_layer(bounds.intersect(&clip.bounds));
                }
                SnapshotOperation::Scene(PaintOperation::EndLayer) => {
                    self.next_frame.scene.pop_layer();
                }
                SnapshotOperation::Image(image) => {
                    let mut bounds = image.bounds;
                    let mut image_bounds = image.image_bounds;
                    let mut mask = image.mask;
                    bounds.origin += delta;
                    image_bounds.origin += delta;
                    mask.bounds.origin += delta;
                    let opacity = self.element_opacity;
                    self.element_opacity = image.opacity;
                    self.with_content_mask(Some(mask), |window| {
                        let _ = window.paint_image_sampled(
                            bounds,
                            image_bounds,
                            image.corner_radii,
                            image.data.clone(),
                            image.frame_index,
                            image.grayscale,
                            image.sampling,
                        );
                    });
                    self.element_opacity = opacity;
                }
            }
        }
        true
    }
}
