//! Source/owner qualification and one deterministic native/visual containment path.
use super::{
    DeepTarget, LevelKind, PathLevel, SelectionSource, StopReason, TargetOrigin, WindowCandidate,
    WindowSnapshot,
};
use crate::geometry::{Point, Rect};

#[derive(Debug, Clone)]
pub(crate) struct VisualSnapshot {
    pub source: SelectionSource,
    pub owner: WindowCandidate,
    pub point: Point,
    /// Qualified rectangles in detector evidence order, in screen physical coordinates.
    pub rectangles: Vec<Rect>,
}

impl VisualSnapshot {
    pub fn current(&self, source: SelectionSource, windows: &WindowSnapshot, point: Point) -> bool {
        self.owner.identity.creation_token != 0
            && self.source == source
            && self.point == point
            && self.owner.snapshot_epoch == windows.epoch()
            && windows.candidates().contains(&self.owner)
    }

    pub fn fuse(
        &self,
        native: Option<&DeepTarget>,
        windows: &WindowSnapshot,
        frozen: Rect,
    ) -> Option<DeepTarget> {
        if self.owner.identity.creation_token == 0
            || self.owner.snapshot_epoch != windows.epoch()
            || !windows.candidates().contains(&self.owner)
        {
            return None;
        }
        let native = native.filter(|n| n.window == self.owner.identity && n.covers(self.point));
        let protected = native.and_then(|n| n.path.last()).is_some_and(|l| {
            !matches!(
                l.kind,
                LevelKind::Custom
                    | LevelKind::Unknown
                    | LevelKind::Window
                    | LevelKind::Pane
                    | LevelKind::Group
                    | LevelKind::Document
            )
        });
        let valid = |r: Rect| {
            !r.is_empty()
                && r.contains(self.point)
                && frozen.contains_rect(r)
                && self.owner.screen_bounds.contains_rect(r)
                && !windows.candidates().iter().any(|w| {
                    w.z_order < self.owner.z_order && !w.screen_bounds.intersect(r).is_empty()
                })
        };
        let selected = if protected {
            native?.screen_bounds
        } else {
            self.rectangles
                .iter()
                .copied()
                .find(|&r| valid(r) && native.is_none_or(|n| n.screen_bounds.contains_rect(r)))
                .or_else(|| native.map(|n| n.screen_bounds))?
        };
        let mut levels = Vec::new();
        let count = 1 + native.map_or(0, |n| n.path.len()) + self.rectangles.len();
        if !crate::visual_detection::allocation::reserve(&mut levels, count, "fusion_levels") {
            return None;
        }
        levels.push(PathLevel::window(self.owner.screen_bounds));
        if let Some(native) = native {
            levels.extend(native.path.iter().copied());
        }
        levels.extend(
            self.rectangles
                .iter()
                .copied()
                .filter(|&r| valid(r))
                .map(PathLevel::visual),
        );
        levels.retain(|l| l.rect.contains_rect(selected));
        // Equal geometry keeps native semantics. Only actual containment is a layer.
        levels.sort_unstable_by_key(|l| {
            (
                std::cmp::Reverse(l.rect.area()),
                l.rect.left,
                l.rect.top,
                l.rect.right,
                l.rect.bottom,
                match l.origin {
                    TargetOrigin::Accessibility => 0,
                    TargetOrigin::Window => 1,
                    TargetOrigin::Visual => 2,
                },
            )
        });
        levels.dedup_by_key(|l| l.rect);
        let mut path: Vec<PathLevel> = Vec::new();
        if !crate::visual_detection::allocation::reserve(&mut path, levels.len(), "fusion_path") {
            return None;
        }
        for level in levels {
            if path
                .last()
                .is_none_or(|previous| previous.rect.contains_rect(level.rect))
            {
                path.push(level);
            }
        }
        // Preserve native semantic nodes and outer frame before visual context when capped.
        while path.len() > super::MAX_PATH_LEN {
            let index = path
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, l)| l.origin == TargetOrigin::Visual && l.rect != selected)
                .map(|(i, _)| i)?;
            path.remove(index);
        }
        Some(DeepTarget {
            window: self.owner.identity,
            screen_bounds: selected,
            path,
            stop_reason: StopReason::Complete,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod contracts;
    use crate::{
        monitor_cache::MonitorCache,
        window_detection::{WindowIdentity, next_visible_stop},
    };
    fn scene() -> (VisualSnapshot, WindowSnapshot, Rect) {
        let bounds = Rect::new(-640, 0, 0, 640);
        let owner = WindowCandidate::new(WindowIdentity::new(7, 11, 13, 1), bounds, 1, 17);
        let source = SelectionSource {
            capture_generation: 19,
            pixels: yue_media::pixels::PixelSource::new(
                yue_media::pixels::PixelDescriptor::bgra8(),
                23,
            ),
        };
        let snapshot = VisualSnapshot {
            source,
            owner,
            point: Point::new(-320, 320),
            rectangles: vec![
                Rect::new(-400, 240, -240, 400),
                Rect::new(-500, 100, -100, 500),
            ],
        };
        (
            snapshot,
            WindowSnapshot::new(17, vec![owner], MonitorCache::from_bounds([bounds])),
            bounds,
        )
    }

    #[test]
    fn visual_replay_semantic_defaults_and_equal_geometry_keep_native_kind() {
        let (visual, windows, bounds) = scene();
        for kind in LevelKind::ALL {
            let native = DeepTarget {
                window: visual.owner.identity,
                screen_bounds: Rect::new(-520, 80, -80, 550),
                path: vec![
                    PathLevel::window(bounds),
                    PathLevel::new(Rect::new(-520, 80, -80, 550), kind),
                ],
                stop_reason: StopReason::Complete,
            };
            let result = visual.fuse(Some(&native), &windows, bounds).unwrap();
            let refine = matches!(
                kind,
                LevelKind::Unknown
                    | LevelKind::Custom
                    | LevelKind::Window
                    | LevelKind::Pane
                    | LevelKind::Group
                    | LevelKind::Document
            );
            assert_eq!(
                result.screen_bounds,
                if refine {
                    visual.rectangles[0]
                } else {
                    native.screen_bounds
                },
                "{kind:?}"
            );
            assert!(
                result
                    .path
                    .windows(2)
                    .all(|p| p[0].rect != p[1].rect && p[0].rect.contains_rect(p[1].rect))
            );
        }
        let mut native = DeepTarget {
            window: visual.owner.identity,
            screen_bounds: visual.rectangles[0],
            path: vec![
                PathLevel::window(bounds),
                PathLevel::new(visual.rectangles[0], LevelKind::Custom),
            ],
            stop_reason: StopReason::Complete,
        };
        let result = visual.fuse(Some(&native), &windows, bounds).unwrap();
        assert_eq!(
            result.path.last().unwrap().origin,
            TargetOrigin::Accessibility
        );
        native.path.last_mut().unwrap().kind = LevelKind::Text;
        assert_eq!(
            visual
                .fuse(Some(&native), &windows, bounds)
                .unwrap()
                .path
                .last()
                .unwrap()
                .kind,
            LevelKind::Text
        );
    }

    #[test]
    fn visual_replay_source_owner_occlusion_and_frozen_bounds_fail_closed() {
        let (mut visual, mut windows, bounds) = scene();
        assert!(visual.current(visual.source, &windows, visual.point));
        let mut source = visual.source;
        source.capture_generation += 1;
        assert!(!visual.current(source, &windows, visual.point));
        source = visual.source;
        source.pixels = yue_media::pixels::PixelSource::new(source.pixels.descriptor(), 24);
        assert!(!visual.current(source, &windows, visual.point));
        assert!(!visual.current(visual.source, &windows, Point::new(-319, 320)));
        assert!(visual.fuse(None, &windows, bounds).is_some());
        assert!(
            visual
                .fuse(None, &windows, Rect::new(-350, 300, -250, 350))
                .is_none()
        );
        let mut occluder = visual.owner;
        occluder.identity.hwnd = 8;
        occluder.z_order = 0;
        occluder.screen_bounds = Rect::new(-405, 240, -399, 400);
        windows = WindowSnapshot::new(
            17,
            vec![occluder, visual.owner],
            MonitorCache::from_bounds([bounds]),
        );
        // Even an occluder away from the query point invalidates the rectangle it intersects.
        assert!(visual.fuse(None, &windows, bounds).is_none());
        let mut reincarnated = visual.owner;
        reincarnated.identity.creation_token += 1;
        let replacement =
            WindowSnapshot::new(17, vec![reincarnated], MonitorCache::from_bounds([bounds]));
        assert!(!visual.current(visual.source, &replacement, visual.point));
        assert!(visual.fuse(None, &replacement, bounds).is_none());
        let mut unverified = visual.clone();
        unverified.owner.identity.creation_token = 0;
        let unverified_windows = WindowSnapshot::new(
            17,
            vec![unverified.owner],
            MonitorCache::from_bounds([bounds]),
        );
        assert!(!unverified.current(visual.source, &unverified_windows, visual.point));
        assert!(unverified.fuse(None, &unverified_windows, bounds).is_none());
        let empty = WindowSnapshot::empty();
        assert!(visual.fuse(None, &empty, bounds).is_none());
        visual.owner.identity.class_name_hash += 1;
        assert!(!visual.current(visual.source, &windows, visual.point));
        assert!(visual.fuse(None, &windows, bounds).is_none());
    }

    #[test]
    fn visual_replay_nested_single_pixel_layers_reachable_crossings_not_in_chain() {
        let (mut visual, windows, bounds) = scene();
        let inner = visual.rectangles[0];
        let fine = Rect::new(
            inner.left - 1,
            inner.top - 1,
            inner.right + 1,
            inner.bottom + 1,
        );
        let crossing = Rect::new(
            inner.left - 10,
            inner.top + 10,
            inner.right + 10,
            inner.bottom + 10,
        );
        visual.rectangles = vec![inner, inner, fine, crossing, visual.rectangles[1]];
        let result = visual.fuse(None, &windows, bounds).unwrap();
        assert_eq!(result.screen_bounds, inner);
        assert_eq!(result.path.iter().filter(|p| p.rect == inner).count(), 1);
        assert!(!result.path.iter().any(|p| p.rect == crossing));
        let last = result.path.len() - 1;
        let stop = next_visible_stop(&result.path, last, -1, 3);
        assert_eq!(result.path[stop].rect, fine);
        assert_eq!(next_visible_stop(&result.path, stop, 1, 3), last);
        // Candidate order only chooses among qualified alternatives; containment cannot invent one.
        visual.rectangles.reverse();
        let result = visual.fuse(None, &windows, bounds).unwrap();
        assert!(visual.rectangles.contains(&result.screen_bounds));
    }

    #[test]
    fn visual_replay_fusion_allocation_failure_keeps_native_fallback_available() {
        let (visual, windows, bounds) = scene();
        for site in ["fusion_levels", "fusion_path"] {
            let (result, hit) = crate::visual_detection::allocation::fail_at(site, || {
                visual.fuse(None, &windows, bounds)
            });
            assert!(hit && result.is_none(), "{site}");
            assert!(windows.nearest_target(visual.point, 0).is_some());
        }
    }
}
