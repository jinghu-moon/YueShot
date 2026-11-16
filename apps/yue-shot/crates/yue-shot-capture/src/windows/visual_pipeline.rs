//! Owner-thread transfer scheduling; the worker receives only owned pixel bytes.
use super::{
    providers::FrozenFrame,
    visual_worker::{Key, Reply, VisualWorker, Work},
    win::d3d11::visual_transfer::BatchReadback,
};
use crate::{
    geometry::{Point, Rect},
    visual_detection::{
        Completion,
        progressive::{Query, Step},
    },
    window_detection::{SelectionSource, WindowCandidate, visual::VisualSnapshot},
};
use std::time::{Duration, Instant};

struct Active {
    key: Key,
    started: Instant,
    initial: bool,
}

pub(crate) struct VisualPipeline {
    metrics: crate::diagnostics::WindowDetectionMetrics,
    worker: VisualWorker,
    transfer: Option<BatchReadback>,
    atlas_source: Option<SelectionSource>,
    active: Option<Active>,
    queued: Option<Key>,
    awaiting: bool,
    cache: Option<(SelectionSource, Rect)>,
    next: u64,
    disabled: Option<SelectionSource>,
    pub published: Option<VisualSnapshot>,
}

impl VisualPipeline {
    #[cfg(test)]
    pub(super) fn idle(&self) -> bool {
        self.active.is_none()
            && self.queued.is_none()
            && !self.awaiting
            && self.transfer.as_ref().is_none_or(|t| !t.pending())
    }

    pub fn retire_for_cursor(
        &mut self,
        point: Point,
        screen: Point,
        owner: Option<crate::window_detection::WindowIdentity>,
    ) {
        let old_key = self.active.as_ref().map(|a| a.key).or(self.queued);
        if old_key.is_some_and(|key| key.point != point || Some(key.owner.identity) != owner)
            || self
                .published
                .as_ref()
                .is_some_and(|s| Some(s.owner.identity) != owner)
        {
            self.invalidate();
        } else if self.published.as_ref().is_some_and(|s| s.point != screen) {
            // Seed-specific results expire immediately; completed source pixels remain reusable.
            self.published = None;
        }
    }

    pub fn new(
        thread: u32,
        metrics: crate::diagnostics::WindowDetectionMetrics,
    ) -> Result<Self, String> {
        Ok(Self {
            metrics,
            worker: VisualWorker::new(thread)?,
            transfer: None,
            atlas_source: None,
            active: None,
            queued: None,
            awaiting: false,
            cache: None,
            next: 0,
            disabled: None,
            published: None,
        })
    }

    pub fn request(&mut self, source: SelectionSource, owner: WindowCandidate, point: Point) {
        if owner.identity.creation_token == 0 {
            self.invalidate();
            return;
        }
        if self.disabled == Some(source) {
            return;
        }
        let same = |key: Key| key.source == source && key.owner == owner && key.point == point;
        if self.active.as_ref().is_some_and(|a| same(a.key)) || self.queued.is_some_and(same) {
            return;
        }
        let Some(next) = self.next.checked_add(1) else {
            self.disabled = Some(source);
            self.invalidate();
            return;
        };
        self.next = next;
        let key = Key {
            request: self.next,
            source,
            owner,
            point,
            submitted: Instant::now(),
        };
        if self.active.is_some()
            || self.awaiting
            || self.transfer.as_ref().is_some_and(|t| t.pending())
        {
            self.invalidate();
        } else {
            self.published = None;
        }
        self.metrics.record_visual_submitted();
        self.queued = Some(key);
    }

    /// Stop publication immediately, but keep a submitted atlas until Map is complete.
    pub fn invalidate(&mut self) {
        if let Some(key) = self.active.as_ref().map(|a| a.key).or(self.queued) {
            self.metrics
                .record_visual_retired(self.disabled == Some(key.source));
        }
        self.published = None;
        self.queued = None;
        self.active = None;
        self.cache = None;
        self.worker.select(0);
        if !self.awaiting {
            self.send(Work::Reset);
        }
        if let Some(transfer) = &mut self.transfer {
            transfer.cancel();
        }
        if self.transfer.as_ref().is_none_or(|t| !t.pending()) {
            self.transfer = None;
            self.atlas_source = None;
        }
    }

    fn unavailable(&mut self) {
        if self.active.take().is_some() || self.queued.is_some() {
            self.metrics.record_visual_retired(true);
        }
        self.queued = None;
        self.invalidate();
    }

    fn send(&mut self, work: Work) -> bool {
        if self.awaiting || !self.worker.submit(work) {
            return false;
        }
        self.awaiting = true;
        true
    }

    pub fn pump(&mut self, frame: Option<&FrozenFrame>, capture_generation: u64) -> bool {
        // Check before consuming completion: a late Complete is still a timeout.
        if let Some(active) = &self.active
            && active.started.elapsed() >= Duration::from_millis(100)
        {
            self.disabled = Some(active.key.source);
            self.invalidate();
        }
        let mut changed = false;
        if let Some(reply) = self.worker.take() {
            self.awaiting = false;
            match reply {
                Reply::Step {
                    key,
                    step,
                    coverage,
                    buffer_bound,
                } if self.active.as_ref().is_some_and(|a| a.key == key) => {
                    // Query + 2MiB explicit worker stack + conservative owner/atlas metadata.
                    if buffer_bound + 4 * 1_048_576 > 16 * 1_048_576 {
                        self.disabled = Some(key.source);
                        self.invalidate();
                    } else {
                        match step {
                            Step::Read(regions) => {
                                let result = frame
                                    .filter(|f| {
                                        f.frame.source == key.source.pixels
                                            && capture_generation == key.source.capture_generation
                                    })
                                    .and_then(FrozenFrame::render_source)
                                    .ok_or_else(|| "visual source retired".to_owned())
                                    .and_then(|source| {
                                        self.transfer
                                            .as_mut()
                                            .ok_or_else(|| "missing visual atlas".to_owned())?
                                            .begin(source, regions)
                                    });
                                if result.is_err() {
                                    self.invalidate();
                                }
                            }
                            Step::Complete(result) if result.completion == Completion::Complete => {
                                if let Some(frame) = frame.filter(|f| {
                                    f.frame.source == key.source.pixels
                                        && capture_generation == key.source.capture_generation
                                }) {
                                    let origin = frame.monitor.layout.to_screen(Point::new(0, 0));
                                    let mut rectangles = Vec::new();
                                    if !crate::visual_detection::allocation::reserve(
                                        &mut rectangles,
                                        result.candidates.len(),
                                        "published_rectangles",
                                    ) {
                                        self.invalidate();
                                        return false;
                                    }
                                    rectangles.extend(result.candidates.into_iter().map(|c| {
                                        Rect::new(
                                            c.rect.left + origin.x,
                                            c.rect.top + origin.y,
                                            c.rect.right + origin.x,
                                            c.rect.bottom + origin.y,
                                        )
                                    }));
                                    self.published = Some(VisualSnapshot {
                                        source: key.source,
                                        owner: key.owner,
                                        point: Point::new(
                                            key.point.x + origin.x,
                                            key.point.y + origin.y,
                                        ),
                                        rectangles,
                                    });
                                    self.metrics.record_visual_completed(
                                        key.submitted.elapsed(),
                                        self.published.as_ref().unwrap().rectangles.is_empty(),
                                        buffer_bound + 4 * 1_048_576,
                                    );
                                    self.cache = Some((key.source, coverage));
                                    self.active = None;
                                    changed = true;
                                } else {
                                    self.invalidate();
                                }
                            }
                            Step::Unavailable(reason) => {
                                log::debug!("visual query unavailable: {reason:?}");
                                self.unavailable();
                            }
                            _ => {
                                self.unavailable();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(active) = &self.active
            && active.started.elapsed() >= Duration::from_millis(100)
        {
            // Four times the 25ms qualification budget is a stall cut-off, not a relaxed latency gate.
            self.disabled = Some(active.key.source);
            self.invalidate();
        }
        if !self.awaiting && self.transfer.as_ref().is_some_and(|t| t.pending()) {
            let transfer = self.transfer.as_mut().unwrap();
            match transfer.submit_slice() {
                Err(_) => {
                    self.invalidate();
                }
                Ok(false) => {
                    self.send(Work::Pace {
                        micros: if self.active.is_some() { 250 } else { 15_000 },
                    });
                    return changed;
                }
                Ok(true) => match transfer.poll() {
                    None => {
                        self.send(Work::Pace {
                            micros: if self.active.is_some() { 250 } else { 15_000 },
                        });
                        return changed;
                    }
                    Some(Err(_)) => {
                        if let Some(active) = &self.active {
                            self.disabled = Some(active.key.source);
                            self.invalidate();
                        } else {
                            // A cancelled batch just drained. Keep the newest queued query.
                            self.transfer = None;
                            self.atlas_source = None;
                        }
                    }
                    Some(Ok(mut regions)) => {
                        if let Some(active) = &mut self.active {
                            let key = active.key;
                            let work = if active.initial {
                                active.initial = false;
                                let size = frame
                                    .map(|f| [f.frame.width as usize, f.frame.height as usize]);
                                match (size, regions.pop()) {
                                    (Some(size), Some((region, pixels))) => Some(Work::Start {
                                        key,
                                        size,
                                        region,
                                        pixels,
                                    }),
                                    _ => None,
                                }
                            } else {
                                Some(Work::Apply { key, regions })
                            };
                            if work.is_none_or(|work| !self.send(work)) {
                                self.invalidate();
                            }
                        }
                    }
                },
            }
        }
        if self.active.is_none()
            && !self.awaiting
            && self.transfer.as_ref().is_none_or(|t| !t.pending())
            && let Some(key) = self.queued.take()
        {
            let Some(frame) = frame.filter(|f| {
                f.frame.source == key.source.pixels
                    && capture_generation == key.source.capture_generation
            }) else {
                return changed;
            };
            let descriptor = frame.frame.source.descriptor();
            if descriptor.format() != yue_media::PixelFormat::Bgra8Unorm
                || descriptor.encoding() != yue_media::pixels::PixelEncoding::Srgb
            {
                self.disabled = Some(key.source);
                return changed;
            }
            self.worker.select(key.request);
            // The source key is immutable; reuse bytes only, never another seed's chain.
            if self
                .cache
                .is_some_and(|(source, rect)| source == key.source && rect.contains(key.point))
            {
                self.active = Some(Active {
                    key,
                    started: key.submitted,
                    initial: false,
                });
                if !self.send(Work::Restart(key)) {
                    self.invalidate();
                }
            } else {
                let size = [frame.frame.width as usize, frame.frame.height as usize];
                let Ok(roi) = Query::initial_region(size, key.point) else {
                    self.disabled = Some(key.source);
                    return changed;
                };
                if self.atlas_source != Some(key.source) {
                    self.transfer = frame.device().and_then(|d| BatchReadback::new(d).ok());
                    self.atlas_source = Some(key.source);
                }
                let mut regions = Vec::new();
                if !crate::visual_detection::allocation::reserve(&mut regions, 1, "roi_request") {
                    self.invalidate();
                    return false;
                }
                regions.push(roi);
                let result = frame
                    .render_source()
                    .zip(self.transfer.as_mut())
                    .ok_or_else(|| "missing visual source/atlas".to_owned())
                    .and_then(|(source, transfer)| transfer.begin(source, regions));
                if result.is_err() {
                    self.disabled = Some(key.source);
                    self.invalidate();
                    return changed;
                }
                self.active = Some(Active {
                    key,
                    started: key.submitted,
                    initial: true,
                });
                self.send(Work::Pace {
                    micros: if self.active.is_some() { 250 } else { 15_000 },
                });
            }
        }
        if self.active.is_none()
            && self.queued.is_none()
            && self.cache.is_none()
            && !self.awaiting
            && self.transfer.as_ref().is_some_and(|t| !t.pending())
        {
            self.transfer = None;
            self.atlas_source = None;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_detection::WindowIdentity;

    fn fixture() -> (FrozenFrame, SelectionSource, WindowCandidate) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/visual-candidate-v4.1/T01.png");
        let mut decoder = png::Decoder::new(std::io::Cursor::new(std::fs::read(path).unwrap()));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let frame = super::super::providers::test_gpu_frozen_frame(info.width, info.height, pixels);
        let source = SelectionSource {
            capture_generation: 1,
            pixels: frame.frame.source,
        };
        let owner = WindowCandidate::new(
            WindowIdentity::new(101, 103, 107, 1),
            frame.monitor.layout.bounds,
            0,
            1,
        );
        (frame, source, owner)
    }

    fn finish(pipeline: &mut VisualPipeline, frame: &FrozenFrame, generation: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            pipeline.pump(Some(frame), generation);
            if pipeline.published.is_some()
                || (pipeline.active.is_none() && pipeline.queued.is_none() && !pipeline.awaiting)
            {
                return;
            }
            assert!(Instant::now() < deadline, "product pipeline stalled");
            std::thread::sleep(Duration::from_micros(100));
        }
    }

    #[test]
    #[ignore = "L3 real GPU and product worker; fixed fixture only, no visible window or external writes"]
    fn visual_replay_product_pipeline_queries_reuses_pixels_and_rejects_retired_source() {
        let (frame, source, owner) = fixture();
        let mut pipeline =
            VisualPipeline::new(0, crate::diagnostics::WindowDetectionMetrics::new()).unwrap();
        pipeline.request(source, owner, Point::new(80, 60));
        finish(&mut pipeline, &frame, 1);
        let first = pipeline
            .published
            .as_ref()
            .expect("first query unavailable");
        assert!(first.rectangles.contains(&Rect::new(41, 31, 141, 101)));
        assert_eq!(first.source, source);
        let cache = pipeline.cache;
        pipeline.retire_for_cursor(
            Point::new(290, 75),
            Point::new(290, 75),
            Some(owner.identity),
        );
        assert!(pipeline.published.is_none());
        pipeline.request(source, owner, Point::new(290, 75));
        assert_eq!(
            pipeline.cache, cache,
            "changing seed must preserve eligible pixels"
        );
        finish(&mut pipeline, &frame, 1);
        assert_eq!(
            pipeline.published.as_ref().unwrap().point,
            Point::new(290, 75)
        );
        assert!(
            pipeline
                .published
                .as_ref()
                .unwrap()
                .rectangles
                .contains(&Rect::new(240, 40, 340, 110))
        );
        pipeline.invalidate();
        finish(&mut pipeline, &frame, 1);
        assert!(pipeline.published.is_none());
        pipeline.request(source, owner, Point::new(80, 60));
        finish(&mut pipeline, &frame, 2);
        assert!(
            pipeline.published.is_none(),
            "source generation cannot cross sessions"
        );
    }

    #[test]
    #[ignore = "L3 real GPU and worker; 60s completed-query cache idle, no visible UI qualification"]
    fn visual_replay_cached_selection_idle_resource_probe() {
        let (frame, source, owner) = fixture();
        let metrics = crate::diagnostics::WindowDetectionMetrics::new();
        let mut pipeline = VisualPipeline::new(0, metrics.clone()).unwrap();
        pipeline.request(source, owner, Point::new(80, 60));
        finish(&mut pipeline, &frame, 1);
        let expected = pipeline
            .published
            .as_ref()
            .expect("positive query required")
            .rectangles
            .clone();
        assert!(!expected.is_empty());
        assert!(pipeline.idle());
        let initial_polls = pipeline.transfer.as_ref().unwrap().poll_count;
        let initial_submitted = metrics.reading().visual_submitted;
        pipeline.retire_for_cursor(
            Point::new(290, 75),
            Point::new(290, 75),
            Some(owner.identity),
        );
        pipeline.request(source, owner, Point::new(290, 75));
        finish(&mut pipeline, &frame, 1);
        let snapshot = pipeline.published.as_ref().expect("new seed must complete");
        assert_eq!(snapshot.point, Point::new(290, 75));
        assert!(snapshot.rectangles.contains(&Rect::new(240, 40, 340, 110)));
        assert_eq!(metrics.reading().visual_submitted, initial_submitted + 1);
        assert_eq!(
            pipeline.transfer.as_ref().unwrap().poll_count,
            initial_polls,
            "covered seed triggered another GPU readback"
        );
        let expected = snapshot.rectangles.clone();
        assert!(pipeline.idle());
        let jobs = pipeline.worker.submitted_jobs();
        let polls = pipeline.transfer.as_ref().unwrap().poll_count;
        let submitted = metrics.reading().visual_submitted;
        let completed = metrics.reading().visual_completed;
        let started = Instant::now();
        for second in 0..=60 {
            if second > 0 {
                std::thread::sleep(Duration::from_secs(1));
            }
            assert!(!pipeline.pump(Some(&frame), 1));
            assert!(pipeline.idle());
            assert_eq!(pipeline.worker.submitted_jobs(), jobs, "new idle CPU job");
            assert_eq!(
                pipeline.transfer.as_ref().unwrap().poll_count,
                polls,
                "new idle GPU poll"
            );
            assert_eq!(pipeline.published.as_ref().unwrap().rectangles, expected);
            assert_eq!(frame.frame.source, source.pixels);
            assert_eq!(metrics.reading().visual_submitted, submitted);
            assert_eq!(metrics.reading().visual_completed, completed);
            println!("VISUAL_CACHED_IDLE second={second} jobs={jobs} polls={polls} idle=true");
        }
        assert!(started.elapsed() >= Duration::from_secs(60));
        pipeline.invalidate();
        finish(&mut pipeline, &frame, 1);
        assert!(pipeline.idle() && pipeline.published.is_none() && pipeline.transfer.is_none());
    }

    #[test]
    #[ignore = "L3 real GPU, injected 200ms Map delay, thousand moves and 60s idle"]
    fn visual_replay_delayed_graphics_latest_query_idle_resource_probe() {
        let (frame, source, owner) = fixture();
        let metrics = crate::diagnostics::WindowDetectionMetrics::new();
        let mut pipeline = VisualPipeline::new(0, metrics.clone()).unwrap();
        pipeline.request(source, owner, Point::new(91, 66));
        pipeline.pump(Some(&frame), 1);
        pipeline.transfer.as_mut().unwrap().injected_map =
            Some(::windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING.0);
        assert!(pipeline.transfer.as_mut().unwrap().submit_slice().unwrap());
        let started = Instant::now();
        for index in 0..1000 {
            let point = if index % 2 == 0 {
                Point::new(80, 60)
            } else {
                Point::new(290, 75)
            };
            pipeline.request(source, owner, point);
            assert!(pipeline.active.is_none());
            assert_eq!(pipeline.queued.unwrap().point, point);
            assert!(pipeline.transfer.as_ref().unwrap().pending());
            assert!(pipeline.published.is_none());
        }
        // Keep the actual submitted atlas busy for the complete delay. Pace uses
        // the product worker timer; owner pumping does not call Map each millisecond.
        while started.elapsed() < Duration::from_millis(200) {
            pipeline.pump(Some(&frame), 1);
            assert!(pipeline.active.is_none() && pipeline.queued.is_some());
            assert!(pipeline.published.is_none());
            assert_eq!(pipeline.queued.unwrap().point, Point::new(290, 75));
            std::thread::sleep(Duration::from_millis(1));
        }
        let polls = pipeline.transfer.as_ref().unwrap().poll_count;
        assert!(polls <= 20, "cancelled GPU batch busy-polled: {polls}");
        pipeline.transfer.as_mut().unwrap().injected_map = None;
        finish(&mut pipeline, &frame, 1);
        // The newest queued intent is already 200ms old. It may fail closed
        // under the unchanged 100ms stall policy; it must never revive a retired
        // seed or bypass the qualification latency budget to force a positive.
        if let Some(snapshot) = &pipeline.published {
            assert_eq!(snapshot.point, Point::new(290, 75));
            assert!(snapshot.rectangles.contains(&Rect::new(240, 40, 340, 110)));
        }
        let reading = metrics.reading();
        assert_eq!(reading.visual_completed + reading.visual_unavailable, 1);
        assert!(reading.visual_completed <= 1, "a retired query published");
        println!(
            "VISUAL_DELAYED_RELEASE completed={} unavailable={} stale_published=0",
            reading.visual_completed, reading.visual_unavailable
        );
        let jobs = pipeline.worker.submitted_jobs();
        let polls = pipeline
            .transfer
            .as_ref()
            .map(|transfer| transfer.poll_count);
        for second in 0..=60 {
            if second > 0 {
                std::thread::sleep(Duration::from_secs(1));
            }
            assert!(!pipeline.pump(Some(&frame), 1));
            assert!(pipeline.idle());
            assert_eq!(pipeline.worker.submitted_jobs(), jobs);
            assert_eq!(
                pipeline
                    .transfer
                    .as_ref()
                    .map(|transfer| transfer.poll_count),
                polls
            );
            println!("VISUAL_DELAYED_IDLE second={second} jobs={jobs} polls={polls:?} idle=true");
        }
        pipeline.invalidate();
        finish(&mut pipeline, &frame, 1);
        assert!(pipeline.transfer.is_none() && pipeline.idle());
    }

    #[test]
    fn visual_replay_latest_query_stays_bounded_across_thousand_moves_and_cancel() {
        let mut pipeline =
            VisualPipeline::new(0, crate::diagnostics::WindowDetectionMetrics::new()).unwrap();
        let frame = super::super::providers::test_frozen_frame(640, 640);
        let source = SelectionSource {
            capture_generation: 1,
            pixels: frame.frame.source,
        };
        let owner = WindowCandidate::new(
            WindowIdentity::new(101, 103, 107, 1),
            frame.monitor.layout.bounds,
            0,
            1,
        );
        for i in 0..1000 {
            pipeline.request(source, owner, Point::new(i % 600, i % 400));
            assert_eq!(pipeline.queued.unwrap().point, Point::new(i % 600, i % 400));
            assert!(pipeline.active.is_none());
            assert!(pipeline.transfer.is_none());
        }
        pipeline.invalidate();
        finish(&mut pipeline, &frame, 1);
        assert!(pipeline.queued.is_none() && pipeline.published.is_none() && !pipeline.awaiting);
        for _ in 0..100 {
            assert!(!pipeline.pump(None, 1));
            assert!(!pipeline.awaiting);
        }
    }

    #[test]
    fn visual_replay_request_exhaustion_and_late_reply_keys_fail_closed() {
        let frame = super::super::providers::test_frozen_frame(640, 640);
        let source = SelectionSource {
            capture_generation: 1,
            pixels: frame.frame.source,
        };
        let owner = WindowCandidate::new(
            WindowIdentity::new(101, 103, 107, 1),
            frame.monitor.layout.bounds,
            0,
            1,
        );
        let metrics = crate::diagnostics::WindowDetectionMetrics::new();
        let mut pipeline = VisualPipeline::new(0, metrics.clone()).unwrap();
        pipeline.next = u64::MAX;
        pipeline.request(source, owner, Point::new(80, 80));
        assert_eq!(pipeline.disabled, Some(source));
        assert!(pipeline.queued.is_none() && pipeline.published.is_none());
        let mut unverified = owner;
        unverified.identity.creation_token = 0;
        let mut unverified_pipeline = VisualPipeline::new(0, metrics.clone()).unwrap();
        unverified_pipeline.request(source, unverified, Point::new(80, 80));
        assert!(unverified_pipeline.queued.is_none() && unverified_pipeline.active.is_none());
        for mutate in 0..9 {
            let mut pipeline = VisualPipeline::new(0, metrics.clone()).unwrap();
            let key = Key {
                request: 1,
                source,
                owner,
                point: Point::new(80, 80),
                submitted: Instant::now(),
            };
            pipeline.active = Some(Active {
                key,
                started: key.submitted,
                initial: false,
            });
            let mut late = key;
            match mutate {
                0 => late.request += 1,
                1 => late.source.capture_generation += 1,
                2 => {
                    late.source.pixels = yue_media::pixels::PixelSource::new(
                        source.pixels.descriptor(),
                        source.pixels.generation() + 1,
                    )
                }
                3 => late.owner.identity.process_id += 1,
                4 => late.owner.identity.class_name_hash += 1,
                5 => late.owner.snapshot_epoch += 1,
                6 => late.owner.screen_bounds.right -= 1,
                7 => late.point.x += 1,
                8 => late.owner.identity.creation_token += 1,
                _ => unreachable!(),
            }
            pipeline.worker.publish_for_replay(Reply::Step {
                key: late,
                step: Step::Complete(crate::visual_detection::proposals::Detection {
                    completion: Completion::Complete,
                    proposals: 0,
                    validations: 0,
                    coarse_tests: 0,
                    candidates: Vec::new(),
                }),
                coverage: Rect::default(),
                buffer_bound: 0,
            });
            pipeline.pump(Some(&frame), 1);
            assert!(
                pipeline.active.is_some() && pipeline.published.is_none(),
                "late key accepted: {mutate}"
            );
            pipeline.invalidate();
        }
    }

    #[test]
    fn visual_replay_query_budget_failure_does_not_disable_other_points_in_same_frame() {
        let frame = super::super::providers::test_frozen_frame(640, 640);
        let source = SelectionSource {
            capture_generation: 1,
            pixels: frame.frame.source,
        };
        let owner = WindowCandidate::new(
            WindowIdentity::new(101, 103, 107, 1),
            frame.monitor.layout.bounds,
            0,
            1,
        );
        let metrics = crate::diagnostics::WindowDetectionMetrics::new();
        let mut pipeline = VisualPipeline::new(0, metrics.clone()).unwrap();
        let key = Key {
            request: 1,
            source,
            owner,
            point: Point::new(80, 80),
            submitted: Instant::now(),
        };
        pipeline.active = Some(Active {
            key,
            started: key.submitted,
            initial: false,
        });
        pipeline.worker.publish_for_replay(Reply::Step {
            key,
            step: Step::Unavailable(Completion::BudgetExceeded),
            coverage: Rect::default(),
            buffer_bound: 0,
        });
        pipeline.pump(Some(&frame), 1);
        assert!(pipeline.published.is_none() && pipeline.disabled.is_none());
        assert_eq!(metrics.reading().visual_unavailable, 1);
        pipeline.request(source, owner, Point::new(200, 200));
        assert_eq!(pipeline.queued.unwrap().point, Point::new(200, 200));
        pipeline.invalidate();
    }

    #[test]
    #[ignore = "L3 real GPU; named owner allocation faults, fixed fixture only"]
    fn visual_replay_owner_allocation_failures_never_publish_partial_results() {
        use crate::visual_detection::allocation;
        let (frame, source, owner) = fixture();
        let (worker, hit) = allocation::fail_at("worker_mailbox", || {
            VisualPipeline::new(0, crate::diagnostics::WindowDetectionMetrics::new())
        });
        assert!(hit && worker.is_err());
        for site in [
            "staging_atlas",
            "atlas_pieces",
            "roi_request",
            "readback_regions",
            "readback_pixels",
            "published_rectangles",
        ] {
            let mut pipeline =
                VisualPipeline::new(0, crate::diagnostics::WindowDetectionMetrics::new()).unwrap();
            let ((), hit) = allocation::fail_at(site, || {
                pipeline.request(source, owner, Point::new(91, 66));
                finish(&mut pipeline, &frame, 1);
            });
            assert!(hit, "owner allocation site unexercised: {site}");
            assert!(
                pipeline.published.is_none(),
                "result escaped owner allocation failure: {site}"
            );
            assert!(pipeline.transfer.as_ref().is_none_or(|t| !t.pending()));
        }
    }

    #[test]
    #[ignore = "L3 real GPU; StillDrawing injection into actual Map branch, no physical device removal"]
    fn visual_replay_timeout_cancel_drain_blocks_reuse_until_safe_completion() {
        let (frame, source, owner) = fixture();
        let mut pipeline =
            VisualPipeline::new(0, crate::diagnostics::WindowDetectionMetrics::new()).unwrap();
        pipeline.request(source, owner, Point::new(91, 66));
        pipeline.pump(Some(&frame), 1);
        pipeline.transfer.as_mut().unwrap().injected_map =
            Some(::windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING.0);
        assert!(pipeline.transfer.as_mut().unwrap().submit_slice().unwrap());
        pipeline.active.as_mut().unwrap().started = Instant::now() - Duration::from_millis(101);
        pipeline.pump(Some(&frame), 1);
        assert_eq!(pipeline.disabled, Some(source));
        assert!(pipeline.published.is_none());
        let new_source = SelectionSource {
            capture_generation: 2,
            ..source
        };
        pipeline.request(new_source, owner, Point::new(91, 66));
        let deadline = Instant::now() + Duration::from_millis(60);
        while Instant::now() < deadline {
            pipeline.pump(Some(&frame), 2);
            assert!(
                pipeline.active.is_none(),
                "in-use atlas was replaced before safe completion"
            );
            assert_eq!(pipeline.atlas_source, Some(source));
            assert!(pipeline.published.is_none());
            std::thread::sleep(Duration::from_millis(1));
        }
        pipeline.transfer.as_mut().unwrap().injected_map = None;
        finish(&mut pipeline, &frame, 2);
        assert_eq!(pipeline.published.as_ref().unwrap().source, new_source);
        assert!(
            pipeline
                .published
                .as_ref()
                .unwrap()
                .rectangles
                .contains(&Rect::new(41, 31, 141, 101))
        );
        pipeline.invalidate();
        finish(&mut pipeline, &frame, 2);
        assert!(
            pipeline.transfer.is_none() && pipeline.cache.is_none() && pipeline.active.is_none()
        );
    }

    #[test]
    #[ignore = "L3 actual submitted GPU batch with injected StillDrawing/device-removed outcome; no physical hang or device removal"]
    fn visual_replay_hung_map_cancel_and_shutdown_do_not_wait_for_gpu_completion() {
        let (frame, source, owner) = fixture();
        let workers =
            super::super::visual_worker::LIVE_WORKERS.load(std::sync::atomic::Ordering::SeqCst);
        for fault in [
            ::windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING.0,
            ::windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED.0,
        ] {
            let mut pipeline =
                VisualPipeline::new(0, crate::diagnostics::WindowDetectionMetrics::new()).unwrap();
            pipeline.request(source, owner, Point::new(91, 66));
            pipeline.pump(Some(&frame), 1);
            let transfer = pipeline.transfer.as_mut().unwrap();
            assert!(transfer.submit_slice().unwrap());
            transfer.injected_map = Some(fault);
            let cancelled = Instant::now();
            pipeline.invalidate();
            let cancel_us = cancelled.elapsed().as_micros();
            assert!(cancel_us < 1000, "owner cancel blocked: {cancel_us}us");
            assert!(pipeline.published.is_none());
            // Never release the injected 10s/indefinite outcome. Teardown must
            // release COM ownership without a blocking Map/graphics fence wait.
            let shutdown = Instant::now();
            drop(pipeline);
            let shutdown_us = shutdown.elapsed().as_micros();
            println!(
                "VISUAL_GRAPHICS_SHUTDOWN fault={fault} cancel_us={cancel_us} cpu_join_and_drop_us={shutdown_us}"
            );
            assert!(
                shutdown_us < 100_000,
                "shutdown waited for graphics completion"
            );
            assert_eq!(
                super::super::visual_worker::LIVE_WORKERS.load(std::sync::atomic::Ordering::SeqCst),
                workers
            );
        }
    }
}
