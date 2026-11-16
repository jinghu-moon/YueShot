//! P0: call the real controller with injected worker answers and CPU frozen pixels.
//! Default tests need no screen capture, clipboard, files, hotkeys or renderer.
//! One opt-in test qualifies layout()/preview_for_cursor() using a real renderer.

use super::*;
use crate::ports::{BandSource, NullEventSink, SelectionSpec};
use crate::window_detection::{WindowCandidate, WindowIdentity};
use std::ops::{Deref, DerefMut};
mod visual_replay;

struct NoExternalIo;

impl ClipboardWriter for NoExternalIo {
    fn copy_text(&self, _: &str) {
        panic!("native replay must not write the clipboard");
    }

    fn copy_image(
        &self,
        _: u32,
        _: u32,
        _: &[u8],
        _: Option<&std::path::Path>,
    ) -> Result<(), String> {
        panic!("native replay must not write the clipboard");
    }
}

impl ArtifactWriter for NoExternalIo {
    fn write(
        &self,
        _: &str,
        _: &SelectionPixels,
        _: u32,
        _: Option<String>,
    ) -> CaptureResult<yue_shot_model::CaptureArtifact> {
        panic!("native replay must not write artifacts");
    }

    fn write_bands(
        &self,
        _: &str,
        _: &SelectionSpec,
        _: &mut dyn BandSource,
        _: u32,
        _: Option<String>,
    ) -> CaptureResult<yue_shot_model::CaptureArtifact> {
        panic!("native replay must not write artifacts");
    }
}

/// A message-only HWND gives timers an owned lifetime without presenting any UI.
struct Replay(OverlayController);

impl Replay {
    fn new() -> Self {
        Self::build(false)
    }

    fn build(with_renderer: bool) -> Self {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                if with_renderer {
                    WS_EX_NOREDIRECTIONBITMAP
                } else {
                    0
                },
                class.as_ptr(),
                class.as_ptr(),
                if with_renderer { WS_POPUP } else { 0 },
                0,
                0,
                64,
                48,
                if with_renderer {
                    null_mut()
                } else {
                    windows_sys::Win32::UI::WindowsAndMessaging::HWND_MESSAGE
                },
                null_mut(),
                null_mut(),
                null(),
            )
        };
        assert!(!hwnd.is_null(), "message-only replay window: {}", unsafe {
            GetLastError()
        });
        let (_, rx) = mpsc::channel();
        let controller = OverlayController::new(
            OverlayPorts {
                service: Arc::new(CaptureService::new()),
                sink: Arc::new(NullEventSink),
                clipboard: Arc::new(NoExternalIo),
                writer: Arc::new(NoExternalIo),
            },
            crate::window_detection::DetectionOptions::default(),
            Arc::new(Mutex::new(OverlayShared {
                state: CaptureState::Idle,
                session_id: None,
                preferences: OverlayPreferences::default(),
                selection: None,
                selection_session_id: None,
                pinned_session_id: None,
            })),
            rx,
            hwnd,
            0, // injected answers; workers do not notify a real overlay loop
        );
        // Install teardown before any fallible GPU setup, so failed qualification
        // also destroys the HWND and stops the workers.
        let mut controller = Self(controller);
        // All detection answers are injected; no real HWND is queried, and a
        // native answer cannot race with an injected one in the capacity-one slot.
        controller.detector.shutdown();
        let frame = super::super::providers::test_frozen_frame(64, 48);
        controller
            .session
            .arm(frame.frame.clone(), &frame.monitor.layout)
            .unwrap();
        controller.session.overlay_ready().unwrap();
        if with_renderer {
            let device = Arc::new(super::super::win::d3d11::GraphicsDevice::create().unwrap());
            controller.renderer = Some(
                Win32Renderer::new(
                    ::windows::Win32::Foundation::HWND(hwnd),
                    &frame.monitor.layout,
                    device,
                )
                .unwrap(),
            );
            controller
                .renderer
                .as_mut()
                .unwrap()
                .set_frame(&frame)
                .unwrap();
        }
        controller.frozen = Some(frame);
        controller
    }

    fn preview(&mut self, target: SelectionTarget, rect: Rect) {
        let generation = self.gesture.dwell_generation();
        let before = self.session.selection();
        assert!(
            self.gesture
                .apply_dwell(generation, Some((target, rect, before)))
        );
        self.sync_preview_rect();
    }

    fn confirmation(&mut self, request: RequestId, target: SelectionTarget, valid: bool) {
        self.detector
            .publish_for_replay(DetectionResult::Confirmed {
                request,
                target,
                valid,
            });
        self.on_detection_ready();
    }
}

impl Deref for Replay {
    type Target = OverlayController;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Replay {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for Replay {
    fn drop(&mut self) {
        self.disarm_dwell();
        self.disarm_refinement();
        self.disarm_hover_timer();
        self.disarm_render_tick();
        self.detector.shutdown();
        self.refinement.shutdown();
        self.renderer = None;
        unsafe {
            DestroyWindow(self.window);
        }
    }
}

fn target(hwnd: isize) -> SelectionTarget {
    SelectionTarget::window(WindowCandidate::new(
        WindowIdentity::new(hwnd, 7, 11, 1),
        Rect::new(0, 0, 64, 48),
        0,
        17,
    ))
}

#[test]
fn native_baseline_confirm_publishes_exact_frozen_region() {
    let mut replay = Replay::new();
    let bounds = Rect::new(10, 8, 30, 20);
    let target = target(101);
    replay.preview(target, bounds);
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.confirmation(request, target, true);
    assert!(replay.confirm_request.is_none());
    assert_eq!(replay.session.state(), CaptureState::Selected);
    assert_eq!(replay.session.selection(), bounds);
    assert!(replay.gesture.snap_preview().is_none());
    assert!(replay.preview_target.is_none());
    let shared = replay.shared.lock().unwrap();
    let kept = shared.selection.as_ref().unwrap();
    assert_eq!(kept.region, bounds);
    assert_eq!(
        kept.frame.source,
        replay.frozen.as_ref().unwrap().frame.source
    );
    assert_eq!(shared.selection_session_id.as_deref(), Some("idle"));
    let bytes = kept.pixels.as_bytes();
    assert_eq!(bytes.len(), 20 * 12 * 4);
    for (i, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
        assert_eq!(pixel, &[(10 + i % 20) as u8, (8 + i / 20) as u8, 0, 255]);
    }
}

#[test]
fn native_baseline_other_owner_and_cleared_preview_reject_late_confirmation() {
    let mut replay = Replay::new();
    let bounds = Rect::new(10, 8, 30, 20);
    replay.preview(target(102), Rect::new(1, 1, 40, 40));
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.preview(target(101), bounds);
    replay.confirmation(request, target(102), true);
    assert_eq!(replay.session.state(), CaptureState::Selecting);
    assert_eq!(replay.gesture.snap_preview().unwrap().selection, bounds);
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.clear_snap_preview_for_modifier();
    replay.confirmation(request, target(101), true);
    assert_eq!(replay.session.state(), CaptureState::Selecting);
    assert!(replay.session.selection().is_empty());
    assert!(replay.shared.lock().unwrap().selection.is_none());
}

#[test]
fn native_baseline_confirmation_keeps_requested_same_owner_rectangle() {
    // P4 fixes the P0 K01 reproducer. Original output is retained in P0 evidence.
    let mut replay = Replay::new();
    let requested = Rect::new(10, 8, 30, 20);
    let late = Rect::new(4, 3, 50, 40);
    let target = target(101);
    replay.preview(target, requested);
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.preview(target, late); // a native result/wheel step while confirmation is in flight
    replay.confirmation(request, target, true);
    assert_eq!(replay.session.selection(), requested);
    assert_eq!(
        replay
            .shared
            .lock()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .region,
        requested
    );
}

#[test]
fn native_baseline_pressed_gesture_rejects_late_dwell_before_confirmation() {
    let mut replay = Replay::new();
    let bounds = Rect::new(10, 8, 30, 20);
    let target = target(101);
    replay.preview(target, bounds);
    replay.gesture.press(
        Point::new(12, 10),
        SelectionGeometry::Create,
        Rect::default(),
    );
    let generation = replay.gesture.dwell_generation();
    assert!(!replay.gesture.apply_dwell(
        generation,
        Some((target, Rect::new(0, 0, 60, 40), Rect::default()))
    ));
    assert_eq!(replay.gesture.release(), ReleaseOutcome::Click);
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.confirmation(request, target, true);
    assert_eq!(replay.session.selection(), bounds);
}

#[test]
fn native_baseline_confirmation_gate_cancel_restart_and_closed_owner() {
    let mut replay = Replay::new();
    let bounds = Rect::new(10, 8, 30, 20);
    let target = target(101);
    replay.preview(target, bounds);
    assert!(replay.confirm_snap_preview());
    let first = replay.confirm_request.unwrap();
    assert!(replay.confirm_snap_preview());
    let second = replay.confirm_request.unwrap();
    assert_ne!(first, second);
    replay.confirmation(first, target, true);
    assert_eq!(replay.confirm_request, Some(second));
    assert_eq!(replay.session.state(), CaptureState::Selecting);
    replay.confirmation(second, target, false); // owner closed during validation
    assert!(replay.gesture.snap_preview().is_none());
    assert!(replay.session.selection().is_empty());
    assert!(replay.snapshot_request.is_some());

    replay.preview(target, bounds);
    assert!(replay.confirm_snap_preview());
    let retired = replay.confirm_request.unwrap();
    replay.cancel("native-baseline-restart");
    assert!(replay.frozen.is_none());
    assert!(replay.confirm_request.is_none());
    let mut next = super::super::providers::test_frozen_frame(64, 48);
    next.frame.source = yue_media::pixels::PixelSource::new(next.frame.source.descriptor(), 99);
    replay.session = CaptureSession::new("second-source");
    replay
        .session
        .arm(next.frame.clone(), &next.monitor.layout)
        .unwrap();
    replay.session.overlay_ready().unwrap();
    replay.frozen = Some(next);
    replay.preview(target, bounds);
    replay.confirmation(retired, target, true);
    assert_eq!(replay.session.state(), CaptureState::Selecting);
    assert!(replay.session.selection().is_empty());
    assert!(replay.shared.lock().unwrap().selection.is_none());
    assert!(replay.confirm_snap_preview());
    let current = replay.confirm_request.unwrap();
    replay.confirmation(current, target, true);
    assert_eq!(replay.session.state(), CaptureState::Selected);
    assert_eq!(
        replay
            .shared
            .lock()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .frame
            .source
            .generation(),
        99
    );
}

#[test]
fn native_baseline_restart_cross_worker_completion_permutations() {
    use crate::monitor_cache::MonitorCache;

    fn permutations(order: &mut [usize; 5], at: usize, out: &mut Vec<[usize; 5]>) {
        if at == order.len() {
            out.push(*order);
            return;
        }
        for next in at..order.len() {
            order.swap(at, next);
            permutations(order, at + 1, out);
            order.swap(at, next);
        }
    }

    let mut orders = Vec::new();
    permutations(&mut [0, 1, 2, 3, 4], 0, &mut orders);
    assert_eq!(orders.len(), 120);
    for order in orders {
        let mut replay = Replay::new();
        let target = target(101);
        let old_rect = Rect::new(1, 1, 50, 40);
        replay.preview(target, old_rect);
        assert!(replay.confirm_snap_preview());
        let retired_confirmation = replay.confirm_request.unwrap();
        let retired_snapshot = replay.detector.request_refresh(&replay.exclusions);
        replay.snapshot_request = Some(retired_snapshot);
        let retired_hover = replay.detector.request_revalidate(target);
        replay.hover_request = Some(retired_hover);
        replay
            .refine
            .on_cursor_moved(17, Some(target.identity()), Point::new(15, 12));
        let retired_refinement = replay.refine.on_dwell_due().unwrap();
        replay.cancel("cross-worker-permutations");

        let mut frame = super::super::providers::test_frozen_frame(64, 48);
        frame.frame.source =
            yue_media::pixels::PixelSource::new(frame.frame.source.descriptor(), 99);
        replay.session = CaptureSession::new("second-source");
        replay
            .session
            .arm(frame.frame.clone(), &frame.monitor.layout)
            .unwrap();
        replay.session.overlay_ready().unwrap();
        replay.frozen = Some(frame);
        let current_rect = Rect::new(10, 8, 30, 20);
        replay.preview(target, current_rect);
        assert!(replay.confirm_snap_preview());
        let current_confirmation = replay.confirm_request.unwrap();
        assert_ne!(retired_confirmation, current_confirmation);
        let empty_epoch = replay.snapshot.epoch();
        let mut confirmed = false;
        for event in order {
            match event {
                0 => {
                    replay
                        .detector
                        .publish_for_replay(DetectionResult::Refreshed {
                            request: retired_snapshot,
                            snapshot: Ok(WindowSnapshot::new(
                                17,
                                vec![target.owner],
                                MonitorCache::from_bounds([target.screen_bounds()]),
                            )),
                        });
                    replay.on_detection_ready();
                }
                1 => {
                    replay
                        .detector
                        .publish_for_replay(DetectionResult::Revalidated {
                            request: retired_hover,
                            target,
                            validity: HoverValidity::Invalid,
                        });
                    replay.on_detection_ready();
                }
                2 => replay.confirmation(retired_confirmation, target, true),
                3 => replay.apply_refinement_result(RefinementResult {
                    request: retired_refinement.request,
                    epoch: retired_refinement.epoch,
                    elapsed: Duration::ZERO,
                    outcome: RefinementOutcome::Target(Box::new(DeepTarget {
                        window: target.identity(),
                        screen_bounds: old_rect,
                        path: vec![PathLevel::new(old_rect, LevelKind::Button)],
                        stop_reason: crate::window_detection::deep::StopReason::Complete,
                    })),
                }),
                4 => {
                    replay.confirmation(current_confirmation, target, true);
                    confirmed = true;
                }
                _ => unreachable!(),
            }
            assert_eq!(
                replay.snapshot.epoch(),
                empty_epoch,
                "order={order:?} event={event}"
            );
            assert!(replay.snapshot.is_empty() && replay.deep_target.is_none());
            assert_eq!(
                replay.session.state(),
                if confirmed {
                    CaptureState::Selected
                } else {
                    CaptureState::Selecting
                }
            );
            assert_eq!(
                replay.session.selection(),
                if confirmed {
                    current_rect
                } else {
                    Rect::default()
                }
            );
            if confirmed {
                let shared = replay.shared.lock().unwrap();
                let output = shared.selection.as_ref().unwrap();
                assert_eq!(output.region, current_rect);
                assert_eq!(output.frame.source.generation(), 99);
                assert_eq!(
                    shared.selection_session_id.as_deref(),
                    Some("second-source")
                );
            } else {
                assert_eq!(replay.confirm_request, Some(current_confirmation));
                assert_eq!(
                    replay.gesture.snap_preview().unwrap().selection,
                    current_rect
                );
                assert!(replay.shared.lock().unwrap().selection.is_none());
            }
        }
    }
}

fn answer(replay: &mut Replay, path: Vec<PathLevel>) {
    let screen = replay.cursor_screen().unwrap();
    replay
        .refine
        .on_cursor_moved(17, Some(target(101).identity()), screen);
    let job = replay.refine.on_dwell_due().unwrap();
    let screen_bounds = path.last().unwrap().rect;
    replay.apply_refinement_result(RefinementResult {
        request: job.request,
        epoch: job.epoch,
        elapsed: Duration::ZERO,
        outcome: RefinementOutcome::Target(Box::new(DeepTarget {
            window: job.window,
            screen_bounds,
            path,
            stop_reason: crate::window_detection::deep::StopReason::Complete,
        })),
    });
}

#[test]
#[ignore = "L3: real D3D11/D2D renderer on a hidden window; injected native answers, no desktop capture"]
fn native_baseline_controller_preview_replacement_and_manual_path_trace() {
    use crate::monitor_cache::MonitorCache;
    let mut replay = Replay::build(true);
    let target = target(101);
    let frame = PathLevel::new(target.screen_bounds(), LevelKind::Window);
    let parent = PathLevel::new(Rect::new(4, 3, 60, 44), LevelKind::Pane);
    let a = PathLevel::new(Rect::new(10, 8, 30, 20), LevelKind::Button);
    let b = PathLevel::new(Rect::new(35, 8, 55, 20), LevelKind::Image);
    replay.snapshot = WindowSnapshot::new(
        17,
        vec![target.owner],
        MonitorCache::from_bounds([frame.rect]),
    );
    replay.hover_target = Some(target);
    replay.cursor = Point::new(15, 12);
    replay.refinement_pending = Some(Instant::now());
    replay.refresh_preview_for_cursor();
    assert!(
        replay.gesture.snap_preview().is_none(),
        "first pending query withholds the window frame"
    );
    replay.refinement_pending = None;
    replay.refresh_preview_for_cursor();
    assert_eq!(
        replay.preview_target,
        Some(frame.rect),
        "no result uses window frame"
    );
    answer(&mut replay, vec![frame, parent, a]);
    assert_eq!(replay.preview_target, Some(a.rect));
    assert_eq!(replay.preview_kind(), Some(LevelKind::Button));
    assert!(replay.preview_label_text().unwrap().contains("按钮"));
    replay.cursor = Point::new(40, 12);
    replay.refinement_pending = Some(Instant::now());
    replay.refresh_preview_for_cursor();
    assert_eq!(
        replay.preview_target,
        Some(a.rect),
        "lateral wait keeps verified box"
    );
    answer(&mut replay, vec![frame, parent, b]);
    assert_eq!(replay.preview_target, Some(b.rect));
    replay.cursor = Point::new(40, 30);
    answer(&mut replay, vec![frame, parent]);
    assert_eq!(
        replay.preview_target,
        Some(b.rect),
        "first downgrade must be staged"
    );
    assert_eq!(replay.pending_downgrade.map(|(r, _)| r), Some(parent.rect));
    answer(&mut replay, vec![frame, parent]);
    assert_eq!(
        replay.preview_target,
        Some(parent.rect),
        "second dwell accepts downgrade"
    );
    assert!(replay.pending_downgrade.is_none());
    replay.cursor = Point::new(15, 12);
    answer(&mut replay, vec![frame, parent, a]);
    assert!(replay.step_deep_level(-1));
    assert_eq!(replay.preview_target, Some(parent.rect));
    assert_eq!(replay.preview_kind(), Some(LevelKind::Pane));
    let inserted = PathLevel::new(Rect::new(7, 5, 40, 30), LevelKind::Group);
    answer(&mut replay, vec![frame, parent, inserted, a]);
    assert_eq!(replay.deep_levels.unwrap().index(), 1);
    assert_eq!(
        replay.preview_target,
        Some(parent.rect),
        "insertion below index happens to preserve geometry"
    );
    answer(&mut replay, vec![frame, inserted, a]);
    assert_eq!(
        replay.preview_target,
        Some(parent.rect),
        "P4 preserves the explicit geometry when an ancestor disappears"
    );
    assert_eq!(replay.preview_kind(), Some(LevelKind::Pane));
    assert!(replay.preview_label_text().unwrap().contains("容器"));
    replay.clear_snap_preview_for_modifier();
    assert!(replay.gesture.snap_preview().is_none());
    assert!(replay.deep_target.is_none());
    assert!(
        replay.deep_levels.is_none(),
        "P4 clears both independent fields"
    );
    assert!(replay.manual_level.is_none());
    assert!(replay.session.selection().is_empty());
}

#[test]
#[ignore = "L3: real hidden renderer; native paths injected, no clipboard or files"]
fn selection_target_replay_controller_binds_geometry_source_and_native_levels() {
    use crate::monitor_cache::MonitorCache;
    use crate::window_detection::{SelectionSource, TargetOrigin};

    let mut replay = Replay::build(true);
    let owner = target(101);
    let frame = PathLevel::window(owner.owner.screen_bounds);
    let parent = PathLevel::new(Rect::new(4, 3, 50, 40), LevelKind::Pane);
    let button = PathLevel::new(Rect::new(10, 8, 30, 20), LevelKind::Button);
    replay.current_generation = 41;
    replay.snapshot = WindowSnapshot::new(
        17,
        vec![owner.owner],
        MonitorCache::from_bounds([frame.rect]),
    );
    replay.hover_target = Some(owner);
    replay.cursor = Point::new(15, 12);
    answer(&mut replay, vec![frame, parent, button]);
    let preview = replay.gesture.snap_preview().unwrap();
    assert_eq!(preview.selection, button.rect);
    assert_eq!(preview.target.screen_bounds(), button.rect);
    assert_eq!(preview.target.owner, owner.owner);
    assert_eq!(preview.target.origin, TargetOrigin::Accessibility);
    assert_eq!(preview.target.kind, Some(LevelKind::Button));
    assert_eq!(
        preview.target.source,
        Some(SelectionSource {
            capture_generation: 41,
            pixels: replay.frozen.as_ref().unwrap().frame.source,
        })
    );
    assert!(replay.step_deep_level(-1));
    assert_eq!(
        replay
            .gesture
            .snap_preview()
            .unwrap()
            .target
            .screen_bounds(),
        parent.rect
    );
    assert_eq!(
        replay.gesture.snap_preview().unwrap().target.kind,
        Some(LevelKind::Pane)
    );
    assert!(replay.step_deep_level(-1));
    let walked = replay.gesture.snap_preview().unwrap();
    assert_eq!(walked.target.origin, TargetOrigin::Window);
    assert_eq!(walked.target.screen_bounds(), frame.rect);
    assert_eq!(walked.target.source, preview.target.source);
    assert!(replay.step_deep_level(1));
    assert!(replay.step_deep_level(1));
    let selected = replay.gesture.snap_preview().unwrap();
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.confirmation(request, selected.target, true);
    assert_eq!(replay.session.selection(), button.rect);
    let shared = replay.shared.lock().unwrap();
    let output = shared.selection.as_ref().unwrap();
    assert_eq!(output.region, button.rect);
    assert_eq!(output.frame.source, preview.target.source.unwrap().pixels);
    for (i, pixel) in output
        .pixels
        .as_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
    {
        assert_eq!(pixel, &[(10 + i % 20) as u8, (8 + i / 20) as u8, 0, 255]);
    }
}

#[test]
#[ignore = "L3: captures the desktop for 100 sessions, then observes 60s idle; no visible overlay or external writes"]
fn native_baseline_capture_lifecycle_resource_probe() {
    capture_lifecycle_resource_probe(false, false);
}

#[test]
#[ignore = "L3 actual desktop capture and native owner + product visual worker for 100 sessions and 60s idle"]
fn visual_replay_capture_lifecycle_resource_probe() {
    capture_lifecycle_resource_probe(true, false);
}

#[test]
#[ignore = "L3 visible no-activate controlled D2D host; actual capture, real owner, 100 successful product visual sessions and 60s idle"]
fn visual_replay_controlled_capture_lifecycle_resource_probe() {
    capture_lifecycle_resource_probe(true, true);
}

#[test]
#[ignore = "L3 paired controlled-host capture resource reference: no visual jobs, 100 sessions and 60s idle"]
fn visual_replay_controlled_capture_without_analysis_resource_probe() {
    capture_lifecycle_resource_probe(false, true);
}

fn capture_lifecycle_resource_probe(with_visual: bool, controlled: bool) {
    use ::windows::Win32::Graphics::Dxgi::{
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
        DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter3, IDXGIDevice,
    };
    use ::windows::core::Interface;
    use serde_json::json;

    if with_visual {
        struct ProbeLog;
        impl log::Log for ProbeLog {
            fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                true
            }
            fn log(&self, record: &log::Record<'_>) {
                let text = record.args().to_string();
                if text.contains("visual query unavailable") {
                    println!("VISUAL_FAILURE {text}");
                }
            }
            fn flush(&self) {}
        }
        static LOGGER: ProbeLog = ProbeLog;
        log::set_logger(&LOGGER).expect("run the resource probe in its own test process");
        log::set_max_level(log::LevelFilter::Debug);
    }

    monitor::set_per_monitor_v2_awareness().unwrap();
    let mut host = controlled.then(|| {
        super::super::win::d3d11::visual_transfer::tests::PresentProbe::new(
            &super::super::win::d3d11::GraphicsDevice::create().unwrap(),
        )
    });
    let positive = host.as_mut().map(|host| host.positive_scene());
    let workers_before =
        super::super::visual_worker::LIVE_WORKERS.load(std::sync::atomic::Ordering::SeqCst);
    let mut replay = Replay::build(true);
    replay.cancel("resource-probe-setup");
    let monitor = if controlled {
        monitor::captured_monitor_at(Point::new(0, 0)).unwrap()
    } else {
        monitor::captured_monitor_at_cursor().unwrap()
    };
    let mut adapter: Option<IDXGIAdapter3> = None;
    let gpu_usage = |adapter: &IDXGIAdapter3| {
        [
            DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
            DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
        ]
        .map(|group| {
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            unsafe { adapter.QueryVideoMemoryInfo(0, group, &mut info) }.unwrap();
            info.CurrentUsage
        })
    };
    println!(
        "RESOURCE {}",
        json!({"event":"begin", "pid":std::process::id(), "warmups":5, "sessions":100, "idle_seconds":60, "with_visual":with_visual,"controlled_host":controlled})
    );
    for cycle in 0..105 {
        if let Some(host) = &mut host {
            host.pump();
        }
        let started = Instant::now();
        let generation = replay.worker.next_generation();
        replay
            .worker
            .start(StartRequest {
                generation,
                monitor: monitor.clone(),
                cursor_screen: monitor.origin(),
                requested_at: started,
                notify_thread: 0,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let prepared = loop {
            if let Some(frame) = replay.worker.take_ready() {
                break frame.unwrap();
            }
            assert!(
                Instant::now() < deadline,
                "capture timeout at cycle {cycle}"
            );
            thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(prepared.generation, generation);
        assert!(!prepared.frozen.pixels_read());
        let capture_us = started.elapsed().as_micros();
        if adapter.is_none() {
            let dxgi: IDXGIDevice = prepared.frozen.device().unwrap().device().cast().unwrap();
            let actual: IDXGIAdapter3 = unsafe { dxgi.GetAdapter() }.unwrap().cast().unwrap();
            let description = unsafe { actual.GetDesc1() }.unwrap();
            let name = String::from_utf16_lossy(&description.Description);
            println!(
                "RESOURCE {}",
                json!({"event":"adapter", "name":name.trim_end_matches('\0'), "vendor":description.VendorId, "device":description.DeviceId, "size":[monitor.width(),monitor.height()], "dpi":monitor.layout.dpi})
            );
            adapter = Some(actual);
        }
        replay
            .prepare_overlay(&prepared.monitor, &prepared.frozen)
            .unwrap();
        replay.session = CaptureSession::new(format!("resource-{cycle}"));
        replay
            .session
            .arm(prepared.frozen.frame.clone(), &prepared.monitor.layout)
            .unwrap();
        replay.session.overlay_ready().unwrap();
        let source = prepared.frozen.frame.source;
        replay.frozen = Some(prepared.frozen);
        if with_visual {
            use crate::window_detection::provider::WindowProvider;
            // Enumeration is a real provider query on a separate thread, never an
            // overlay event. No HWND or owner answer is fabricated in this probe.
            replay.snapshot = thread::spawn(|| {
                super::super::top_level_provider::TopLevelWindowProvider::new()
                    .refresh(&Exclusions::default())
                    .unwrap()
            })
            .join()
            .unwrap();
            let bounds = replay.frozen.as_ref().unwrap().monitor.layout.bounds;
            let owner = *replay
                .snapshot
                .candidates()
                .iter()
                .find(|w| {
                    !w.screen_bounds.intersect(bounds).is_empty()
                        && host
                            .as_ref()
                            .is_none_or(|host| w.identity.hwnd == host.window.0 as isize)
                })
                .expect("no real owner on captured monitor");
            let visible = owner.screen_bounds.intersect(bounds);
            let screen = Point::new(
                visible.left + visible.width() / 2,
                visible.top + visible.height() / 2,
            );
            replay.cursor = Point::new(screen.x - bounds.left, screen.y - bounds.top);
            replay.hover_target = Some(SelectionTarget::window(owner));
            let visual_started = Instant::now();
            replay.request_visual();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !replay.visual.as_ref().unwrap().idle() {
                replay.on_visual_ready();
                assert!(
                    Instant::now() < deadline,
                    "visual lifecycle did not complete"
                );
                thread::sleep(Duration::from_micros(100));
            }
            let metrics = replay.metrics.reading();
            assert_eq!(metrics.visual_submitted, 1);
            assert_eq!(
                metrics.visual_completed + metrics.visual_unavailable,
                1,
                "query silently retired"
            );
            if controlled {
                assert_eq!(
                    metrics.visual_completed, 1,
                    "controlled positive host did not complete visual analysis"
                );
                assert_eq!(metrics.visual_unavailable, 0);
                let target = replay
                    .gesture
                    .snap_preview()
                    .expect("positive host has no preview")
                    .target;
                assert_eq!(
                    target.origin,
                    crate::window_detection::TargetOrigin::Visual,
                    "controlled positive host did not select a visual target"
                );
                let expected = positive.unwrap();
                let origin = monitor.origin();
                assert_eq!(
                    target.screen_bounds(),
                    Rect::new(
                        expected.left + origin.x,
                        expected.top + origin.y,
                        expected.right + origin.x,
                        expected.bottom + origin.y
                    )
                );
            }
            println!(
                "RESOURCE {}",
                json!({"event":"visual", "cycle":cycle,"completed":metrics.visual_completed,"unavailable":metrics.visual_unavailable,"controller_us":visual_started.elapsed().as_micros(),"query_us":metrics.visual_last_us,"descriptor":format!("{:?}", source.descriptor()),"buffer_bound":metrics.visual_peak_buffer_bytes,"private_bytes":super::super::proc_mem::private_bytes(),"working_set":super::super::proc_mem::working_set_bytes()})
            );
            // The ordinary native baseline below remains the exact confirmation
            // contract; retire the visual result before that separate operation.
            replay.invalidate_visual();
            replay.snapshot.release();
        }
        let selected = Rect::new(10, 10, 266, 138);
        replay.preview(target(101), selected);
        replay.run_action(Action::Confirm);
        let request = replay.confirm_request.unwrap();
        replay.confirmation(request, target(101), true);
        assert_eq!(replay.session.selection(), selected);
        {
            let shared = replay.shared.lock().unwrap();
            let kept = shared.selection.as_ref().unwrap();
            assert_eq!(kept.frame.source, source);
            assert_eq!(kept.pixels.as_bytes().len(), 256 * 128 * 4);
        }
        assert!(!replay.frozen.as_ref().unwrap().pixels_read());
        let active_gpu = gpu_usage(adapter.as_ref().unwrap());
        // Exercise both the save handoff and ordinary cancellation. The sink is
        // NullEventSink, so no file/clipboard/UI outside capture is touched.
        if cycle % 2 == 0 {
            replay.run_action(Action::OpenSavePanel);
        }
        replay.cancel("resource-probe-cycle");
        if with_visual {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !replay.visual.as_ref().unwrap().idle() {
                replay.on_visual_ready();
                assert!(
                    Instant::now() < deadline,
                    "cancelled visual transfer did not drain"
                );
                thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                crate::visual_detection::progressive::LIVE_QUERIES
                    .load(std::sync::atomic::Ordering::SeqCst),
                0
            );
            assert_eq!(
                super::super::win::d3d11::visual_transfer::LIVE_TRANSFERS
                    .load(std::sync::atomic::Ordering::SeqCst),
                0
            );
        }
        assert_eq!(replay.session.state(), CaptureState::Idle);
        assert!(replay.frozen.is_none() && replay.renderer.is_none());
        assert!(replay.deep_target.is_none() && replay.deep_levels.is_none());
        assert!(replay.confirm_request.is_none() && replay.gesture.snap_preview().is_none());
        assert!(!replay.render_armed && !replay.refine.is_in_flight());
        println!(
            "RESOURCE {}",
            json!({"event":"cycle", "cycle":cycle, "warmup":cycle<5, "capture_us":capture_us, "total_us":started.elapsed().as_micros(), "working_set":super::super::proc_mem::working_set_bytes().unwrap(), "private_bytes":super::super::proc_mem::private_bytes(), "active_gpu":active_gpu, "released_gpu":gpu_usage(adapter.as_ref().unwrap())})
        );
    }
    for second in 0..=60 {
        assert!(replay.worker.take_ready().is_none());
        assert!(!replay.render_armed && !replay.refine.is_in_flight());
        if with_visual {
            assert!(replay.visual.as_ref().unwrap().idle());
            let metrics = replay.metrics.reading();
            assert_eq!(metrics.visual_submitted, 0, "idle visual analysis job");
        }
        println!(
            "RESOURCE {}",
            json!({"event":"idle", "second":second, "working_set":super::super::proc_mem::working_set_bytes().unwrap(), "private_bytes":super::super::proc_mem::private_bytes(), "gpu":gpu_usage(adapter.as_ref().unwrap())})
        );
        if second < 60 {
            thread::sleep(Duration::from_secs(1));
        }
    }
    replay.worker.shutdown();
    let shutdown_started = Instant::now();
    drop(replay);
    assert_eq!(
        super::super::visual_worker::LIVE_WORKERS.load(std::sync::atomic::Ordering::SeqCst),
        workers_before
    );
    println!(
        "RESOURCE {}",
        json!({"event":"shutdown", "visual_workers_balanced":true,"owner_shutdown_us":shutdown_started.elapsed().as_micros()})
    );
}
