use super::*;
use crate::window_detection::{SelectionSource, TargetOrigin};

impl OverlayController {
    pub(super) fn display_target(&self) -> Option<&DeepTarget> {
        self.combined_target.as_ref().or(self.deep_target.as_ref())
    }

    pub(super) fn invalidate_visual(&mut self) {
        if let Some(visual) = &mut self.visual {
            visual.invalidate();
        }
        self.combined_target = None;
    }

    pub(super) fn request_visual(&mut self) {
        if self.session.state() != CaptureState::Selecting
            || self.shift_down
            || !self.gesture.gesture().allows_hover()
            || self.confirming_preview.is_some()
            || self.manual_level.is_some()
        {
            return;
        }
        let Some(frame) = &self.frozen else {
            return;
        };
        let Some(owner) = self.hover_target.map(|t| t.owner).filter(|o| {
            o.screen_bounds
                .contains(frame.monitor.layout.to_screen(self.cursor))
        }) else {
            return;
        };
        let Some(visual) = &mut self.visual else {
            return;
        };
        visual.request(
            SelectionSource {
                capture_generation: self.current_generation,
                pixels: frame.frame.source,
            },
            owner,
            self.cursor,
        );
        self.on_visual_ready();
    }

    pub(super) fn on_visual_ready(&mut self) {
        let changed = self
            .visual
            .as_mut()
            .is_some_and(|visual| visual.pump(self.frozen.as_ref(), self.current_generation));
        let retired_preview = self
            .gesture
            .snap_preview()
            .is_some_and(|p| p.target.origin == TargetOrigin::Visual)
            && self
                .visual
                .as_ref()
                .is_none_or(|visual| visual.published.is_none());
        if (changed || retired_preview)
            && self.session.state() == CaptureState::Selecting
            && self.gesture.gesture().allows_hover()
            && self.confirming_preview.is_none()
            && self.manual_level.is_none()
            && !self.shift_down
        {
            self.refresh_preview_for_cursor();
            self.invalidate_all();
        }
    }

    pub(super) fn rebuild_combined_target(&mut self) {
        if self.manual_level.is_some()
            || self.confirming_preview.is_some()
            || !self.gesture.gesture().allows_hover()
        {
            return;
        }
        let next = self
            .frozen
            .as_ref()
            .zip(self.visual.as_ref())
            .and_then(|(frame, visual)| {
                let source = SelectionSource {
                    capture_generation: self.current_generation,
                    pixels: frame.frame.source,
                };
                let screen = frame.monitor.layout.to_screen(self.cursor);
                let snapshot = visual
                    .published
                    .as_ref()
                    .filter(|s| s.current(source, &self.snapshot, screen))?;
                snapshot.fuse(
                    self.deep_target.as_ref(),
                    &self.snapshot,
                    frame.monitor.layout.bounds,
                )
            });
        if next != self.combined_target {
            self.combined_target = next;
            if let Some(combined) = &self.combined_target {
                self.deep_levels = Some(LevelChain::new(combined.path.len()));
            }
        }
    }

    pub(super) fn preview_is_visual(&self) -> bool {
        self.selected_deep_level()
            .or_else(|| self.display_target()?.path.last().copied())
            .is_some_and(|l| l.origin == TargetOrigin::Visual)
    }
}
