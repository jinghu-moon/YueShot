//! Actual controller + product worker + real atlas; only native provider answers are injected.
use super::*;
use crate::window_detection::TargetOrigin;
mod contracts;

#[test]
#[ignore = "L3 controller and real GPU; late broad native answer and failed expansion keep local preview"]
fn visual_replay_distant_clutter_cannot_restore_the_broad_native_preview() {
    let (size, mut pixels, point, target) = crate::visual_detection::tests::distant_clutter_scene();
    for p in pixels.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    let frame =
        crate::windows::providers::test_gpu_frozen_frame(size[0] as u32, size[1] as u32, pixels);
    let mut replay = replay_from_frame(frame, point);
    let owner = replay.hover_target.unwrap().owner;
    let broad = PathLevel::new(Rect::new(1800, 950, 2100, 1200), LevelKind::Unknown);
    answer(
        &mut replay,
        vec![PathLevel::window(owner.screen_bounds), broad],
    );
    replay.request_visual();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut verified = false;
    loop {
        replay.on_visual_ready();
        if replay.preview_is_visual() {
            verified = true;
            assert_eq!(replay.preview_target, Some(target));
            // The same broad native provider completion arrives after local pixels.
            answer(
                &mut replay,
                vec![PathLevel::window(owner.screen_bounds), broad],
            );
        }
        if verified {
            assert_eq!(
                replay.preview_target,
                Some(target),
                "preview jumped back to the native container"
            );
        }
        if replay.visual.as_ref().unwrap().idle() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_micros(100));
    }
    assert!(verified && replay.preview_is_visual());
    let selected = replay.gesture.snap_preview().unwrap();
    assert!(replay.confirm_snap_preview());
    replay.confirmation(replay.confirm_request.unwrap(), selected.target, true);
    assert_eq!(replay.session.selection(), target);
}

fn fixture_replay() -> Replay {
    fixture_named("T01", Point::new(91, 66))
}

fn fixture_named(name: &str, point: Point) -> Replay {
    let directory = if name == "T10-8k" {
        "visual-candidate-8k-v4"
    } else {
        "visual-candidate-v4.1"
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/{directory}/{name}.png"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(std::fs::read(path).unwrap()));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(info.buffer_size());
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let frame = crate::windows::providers::test_gpu_frozen_frame(info.width, info.height, pixels);
    replay_from_frame(frame, point)
}

fn replay_from_frame(frame: crate::windows::providers::FrozenFrame, point: Point) -> Replay {
    let mut replay = Replay::new();
    replay.renderer = Some(
        Win32Renderer::new(
            ::windows::Win32::Foundation::HWND(replay.window),
            &frame.monitor.layout,
            frame.device().unwrap().clone(),
        )
        .unwrap(),
    );
    replay.renderer.as_mut().unwrap().set_frame(&frame).unwrap();
    replay.session = CaptureSession::new("visual-product-replay");
    replay
        .session
        .arm(frame.frame.clone(), &frame.monitor.layout)
        .unwrap();
    replay.session.overlay_ready().unwrap();
    replay.current_generation = 1;
    let owner = WindowCandidate::new(
        WindowIdentity::new(101, 7, 11, 1),
        frame.monitor.layout.bounds,
        0,
        17,
    );
    replay.snapshot = WindowSnapshot::new(
        17,
        vec![owner],
        crate::monitor_cache::MonitorCache::from_bounds([owner.screen_bounds]),
    );
    replay.hover_target = Some(SelectionTarget::window(owner));
    replay.cursor = point;
    replay.cursor_visible = true;
    replay.frozen = Some(frame);
    replay
}

#[test]
#[ignore = "L3 product controller timing on real GPU/worker and message wakeups; hidden fixed-pixel replay, no render-load qualification"]
fn visual_replay_product_controller_latency_diagnostic() {
    use crate::windows::{visual_pipeline::VisualPipeline, visual_worker::VISUAL_READY_MESSAGE};
    use ::windows::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
            QS_ALLINPUT,
        },
    };
    let count = std::env::var("YUESHOT_P4_SAMPLES")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(100);
    let sessions = std::env::var("YUESHOT_P4_SESSIONS")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(1);
    let present = std::env::var("YUESHOT_P4_PRESENT").as_deref() == Ok("1");
    assert!((100..=10000).contains(&count) && (1..=10).contains(&sessions));
    for (name, point) in [
        ("T01", Point::new(91, 66)),
        ("T21", Point::new(410, 302)),
        ("T10", Point::new(1816, 1066)),
        ("T10-8k", Point::new(1816, 1066)),
    ] {
        let mut replay = fixture_named(name, point);
        let device = replay.frozen.as_ref().unwrap().device().unwrap().clone();
        let mut render = present.then(|| {
            crate::windows::win::d3d11::visual_transfer::tests::PresentProbe::new(&device)
        });
        for session in 0..sessions {
            replay.visual = Some(
                VisualPipeline::new(unsafe { GetCurrentThreadId() }, replay.metrics.clone())
                    .unwrap(),
            );
            let mut times = Vec::new();
            let mut owner_times = Vec::new();
            let mut hot_times = Vec::new();
            let mut iteration = 0;
            while iteration < count + 5
                || render.as_ref().is_some_and(|r| {
                    r.displayed_samples() < (session + 1) * 1000usize.div_ceil(sessions)
                })
            {
                let started = Instant::now();
                replay.request_visual();
                if iteration >= 5 {
                    owner_times.push(started.elapsed().as_nanos());
                }
                let mut calls = 0;
                while replay.visual.as_ref().unwrap().published.is_none() {
                    if let Some(render) = &mut render {
                        render.pump();
                    }
                    let mut message = MSG::default();
                    while unsafe {
                        PeekMessageW(
                            &mut message,
                            None,
                            VISUAL_READY_MESSAGE,
                            VISUAL_READY_MESSAGE,
                            PM_REMOVE,
                        )
                    }
                    .as_bool()
                    {}
                    let event = Instant::now();
                    replay.on_visual_ready();
                    if iteration >= 5 {
                        owner_times.push(event.elapsed().as_nanos());
                    }
                    calls += 1;
                    assert!(
                        started.elapsed() < Duration::from_secs(2),
                        "product query unavailable: {name}/{iteration}"
                    );
                    if replay.visual.as_ref().unwrap().published.is_none() {
                        if let Some(render) = &render {
                            render.wait();
                        } else {
                            let _ = unsafe {
                                MsgWaitForMultipleObjectsEx(
                                    None,
                                    10,
                                    QS_ALLINPUT,
                                    MWMO_INPUTAVAILABLE,
                                )
                            };
                        }
                    }
                }
                assert!(calls > 0);
                assert_eq!(
                    replay.gesture.snap_preview().unwrap().target.origin,
                    TargetOrigin::Visual
                );
                if iteration >= 5 {
                    times.push(started.elapsed().as_nanos());
                    let hot = Instant::now();
                    let preview = replay.preview_for_cursor().unwrap();
                    hot_times.push(hot.elapsed().as_nanos());
                    assert_eq!(preview.0.origin, TargetOrigin::Visual);
                }
                // A verified ROI now precedes optional ancestor discovery. Drain
                // that same query before measuring a fresh query, rather than
                // timing a deduplicated request against its existing preview.
                while !replay.visual.as_ref().unwrap().idle() {
                    if let Some(render) = &mut render {
                        render.pump();
                    }
                    replay.on_visual_ready();
                    assert!(
                        started.elapsed() < Duration::from_secs(2),
                        "ancestor continuation stalled"
                    );
                    if let Some(render) = &render {
                        render.wait();
                    } else {
                        std::thread::sleep(Duration::from_micros(100));
                    }
                }
                iteration += 1;
            }
            let percentile = |samples: &[u128]| {
                let mut v = samples.to_vec();
                v.sort_unstable();
                v[(v.len() * 95).div_ceil(100) - 1]
            };
            let p95 = percentile(&times);
            let owner_p95 = percentile(&owner_times);
            let hot_p95 = percentile(&hot_times);
            println!(
                "PRODUCT_LATENCY {}",
                serde_json::json!({"fixture":name,"session":session,"minimum_samples":count,"actual_samples":times.len(),"end_to_end_ns":times,"owner_event_ns":owner_times,"hot_ns":hot_times,"p95_ns":p95,"owner_p95_ns":owner_p95,"hot_p95_ns":hot_p95,"metrics":replay.metrics.summary_line(),"render_competition":present,"scope":"real product controller, GPU transfers and CPU worker; native answers fixed"})
            );
            assert!(p95 <= 25_000_000, "product P95 over 25ms: {name}: {p95}ns");
            assert!(
                owner_p95 <= 1_000_000,
                "owner event P95 over 1ms: {owner_p95}ns"
            );
            assert!(hot_p95 < 100_000, "hot P95 over 0.1ms: {hot_p95}ns");
        }
        if let Some(render) = &mut render {
            render.qualify_product_competition();
        }
    }
}

fn analyze(replay: &mut Replay) {
    replay.request_visual();
    let deadline = Instant::now() + Duration::from_secs(5);
    while replay.visual.as_ref().unwrap().published.is_none() {
        replay.on_visual_ready();
        assert!(
            !replay.visual.as_ref().unwrap().idle()
                || replay.visual.as_ref().unwrap().published.is_some(),
            "controller query unavailable: {}",
            replay.metrics.summary_line()
        );
        assert!(
            Instant::now() < deadline,
            "controller visual publication timed out"
        );
        std::thread::sleep(Duration::from_micros(100));
    }
}

#[test]
#[ignore = "L3 actual controller, hidden renderer, real GPU/worker; no clipboard/files/desktop capture"]
fn visual_replay_controller_confirms_visual_pixels_and_protects_native_text() {
    let mut replay = fixture_replay();
    analyze(&mut replay);
    let expected = Rect::new(41, 31, 141, 101);
    let preview = replay.gesture.snap_preview().unwrap();
    assert_eq!(preview.selection, expected);
    assert_eq!(preview.target.origin, TargetOrigin::Visual);
    assert_eq!(preview.target.kind, None);
    assert!(replay.preview_label_text().unwrap().contains("区域"));
    let owner = preview.target.owner;
    let text = PathLevel::new(Rect::new(50, 40, 130, 90), LevelKind::Text);
    answer(
        &mut replay,
        vec![PathLevel::window(owner.screen_bounds), text],
    );
    assert_eq!(
        replay.gesture.snap_preview().unwrap().target.origin,
        TargetOrigin::Accessibility
    );
    assert_eq!(replay.preview_target, Some(text.rect));
    // Unknown/Custom can refine, while keeping that native ancestor in the mixed chain.
    answer(
        &mut replay,
        vec![
            PathLevel::window(owner.screen_bounds),
            PathLevel::new(Rect::new(20, 20, 180, 140), LevelKind::Custom),
        ],
    );
    answer(
        &mut replay,
        vec![
            PathLevel::window(owner.screen_bounds),
            PathLevel::new(Rect::new(20, 20, 180, 140), LevelKind::Custom),
        ],
    );
    assert_eq!(replay.preview_target, Some(expected));
    let selected = replay.gesture.snap_preview().unwrap();
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.preview(selected.target, Rect::new(0, 0, 400, 200));
    replay.confirmation(request, selected.target, true);
    assert_eq!(replay.session.selection(), expected);
    let shared = replay.shared.lock().unwrap();
    let output = shared.selection.as_ref().unwrap();
    let source_pixels = replay
        .frozen
        .as_ref()
        .unwrap()
        .read_region(expected)
        .unwrap();
    assert_eq!(output.pixels.as_bytes(), source_pixels.as_bytes());
}

#[test]
#[ignore = "L3 actual GPU source and export paths for both frozen T16 analysis variants; no external writes"]
fn visual_replay_demo_t16_analysis_does_not_change_export_or_source() {
    let mut exported = Vec::new();
    for analysis in [false, true] {
        let mut replay = fixture_replay();
        let expected = Rect::new(41, 31, 141, 101);
        let frame = replay.frozen.as_ref().unwrap();
        let descriptor = frame.frame.source.descriptor();
        let entire = Rect::new(0, 0, frame.frame.width as i32, frame.frame.height as i32);
        let before = frame.read_region(entire).unwrap();
        let before_hash = blake3::hash(before.as_bytes());
        if analysis {
            analyze(&mut replay);
        } else {
            let target = replay.hover_target.unwrap();
            replay.preview(target, expected);
        }
        assert_eq!(replay.gesture.snap_preview().unwrap().selection, expected);
        let target = replay.gesture.snap_preview().unwrap().target;
        assert!(replay.confirm_snap_preview());
        let request = replay.confirm_request.unwrap();
        replay.confirmation(request, target, true);
        let shared = replay.shared.lock().unwrap();
        let selected = shared.selection.as_ref().unwrap();
        assert_eq!(selected.frame.source.descriptor(), descriptor);
        exported.push(selected.pixels.as_bytes().to_vec());
        let after = replay.frozen.as_ref().unwrap().read_region(entire).unwrap();
        assert_eq!(blake3::hash(after.as_bytes()), before_hash);
        assert_eq!(before.as_bytes(), after.as_bytes());
    }
    assert_eq!(exported[0], exported[1]);
    assert_eq!(exported[0].len(), 100 * 70 * 4);
}

#[test]
#[ignore = "L3 controller/GPU construction with named visual-worker allocation failure; native and manual input stay usable"]
fn visual_replay_missing_visual_worker_preserves_native_and_manual_controller() {
    let (mut replay, hit) =
        crate::visual_detection::allocation::fail_at("worker_mailbox", fixture_replay);
    assert!(hit && replay.visual.is_none());
    let owner = replay.hover_target.unwrap().owner;
    answer(
        &mut replay,
        vec![
            PathLevel::window(owner.screen_bounds),
            PathLevel::new(Rect::new(30, 20, 170, 120), LevelKind::Button),
        ],
    );
    assert_eq!(
        replay.gesture.snap_preview().unwrap().target.origin,
        TargetOrigin::Accessibility
    );
    replay.clear_snap_preview_for_modifier();
    replay.on_left_down(POINT { x: 10, y: 10 });
    replay.on_mouse_move(POINT { x: 80, y: 70 });
    replay.on_left_up();
    assert_eq!(replay.session.selection(), Rect::new(10, 10, 80, 70));
}

#[test]
#[ignore = "L3 actual hidden controller with fixed pixels; geometry/manual/epoch/Shift replay"]
fn visual_replay_controller_manual_choice_and_changed_owner_cannot_drift() {
    let mut replay = fixture_replay();
    analyze(&mut replay);
    assert!(replay.step_deep_level(-1));
    let chosen = replay.gesture.snap_preview().unwrap();
    let owner = chosen.target.owner;
    let child = PathLevel::new(Rect::new(60, 50, 120, 85), LevelKind::Button);
    answer(
        &mut replay,
        vec![PathLevel::window(owner.screen_bounds), child],
    );
    assert_eq!(
        replay.gesture.snap_preview().unwrap().selection,
        chosen.selection
    );
    replay.clear_snap_preview_for_modifier();
    assert!(
        replay.manual_level.is_none()
            && replay.deep_levels.is_none()
            && replay.combined_target.is_none()
    );
    assert!(replay.visual.as_ref().unwrap().published.is_none());
    replay.cursor = Point::new(91, 66);
    replay.hover_target = Some(SelectionTarget::window(owner));
    analyze(&mut replay);
    let old = replay.gesture.snap_preview().unwrap();
    assert!(replay.confirm_snap_preview());
    let confirm = replay.confirm_request.unwrap();
    replay.snapshot = WindowSnapshot::new(
        18,
        // T13 now deliberately keeps an unchanged owner across enumeration
        // epochs (24 arrival orders in contracts.rs). A changed owner frame
        // remains unsafe and must reject the old confirmation.
        vec![WindowCandidate {
            snapshot_epoch: 18,
            screen_bounds: Rect::new(
                owner.screen_bounds.left + 1,
                owner.screen_bounds.top,
                owner.screen_bounds.right,
                owner.screen_bounds.bottom,
            ),
            ..owner
        }],
        crate::monitor_cache::MonitorCache::from_bounds([owner.screen_bounds]),
    );
    replay.confirmation(confirm, old.target, true);
    assert_eq!(replay.session.state(), CaptureState::Selecting);
    assert!(replay.session.selection().is_empty());
}

#[test]
#[ignore = "L3 actual hidden controller and GPU; cached native path must not suppress new visual seeds"]
fn visual_replay_controller_cached_native_path_requeries_visual_on_new_dwell() {
    let mut replay = fixture_replay();
    let owner = replay.hover_target.unwrap().owner;
    answer(
        &mut replay,
        vec![
            PathLevel::window(owner.screen_bounds),
            PathLevel::new(owner.screen_bounds, LevelKind::Custom),
        ],
    );
    analyze(&mut replay);
    assert_eq!(replay.preview_target, Some(Rect::new(41, 31, 141, 101)));
    replay.cursor = Point::new(290, 75);
    let screen = replay.cursor_screen().unwrap();
    replay.drive_refinement(screen, Some(SelectionTarget::window(owner)));
    // Same native container is cached. The shared dwell entry still issues the new pixel query.
    // Consume the real window timer, through the same controller message route.
    let timer_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut message: windows_sys::Win32::UI::WindowsAndMessaging::MSG =
            unsafe { std::mem::zeroed() };
        let found = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PeekMessageW(
                &mut message,
                replay.window,
                WM_TIMER,
                WM_TIMER,
                windows_sys::Win32::UI::WindowsAndMessaging::PM_REMOVE,
            )
        } != 0;
        if found && message.wParam == REFINEMENT_TIMER_ID {
            unsafe {
                replay.handle(message.message, message.wParam, message.lParam);
            }
            break;
        }
        assert!(
            Instant::now() < timer_deadline,
            "visual dwell timer was not armed"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while replay.visual.as_ref().unwrap().published.is_none() {
        replay.on_visual_ready();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_micros(100));
    }
    assert_eq!(replay.preview_target, Some(Rect::new(240, 40, 340, 110)));
    assert_eq!(
        replay.gesture.snap_preview().unwrap().target.origin,
        TargetOrigin::Visual
    );
}

#[test]
#[ignore = "L3 real controller/GPU: all eight frozen Demo T15 phase/Shift combinations, not physical input"]
fn visual_replay_demo_t15_phase_and_shift_late_completion_matrix() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/visual-candidate-v4.1/manifest.json"
    ))
    .unwrap();
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "T15")
        .unwrap();
    let mut count = 0;
    for scenario in case["input"]["scenarios"].as_array().unwrap() {
        let events = scenario["events"].as_array().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0]["type"], "start_visual");
        assert_eq!(events[1]["type"], "set_phase");
        assert_eq!(events[2]["type"], "set_shift");
        assert_eq!(events[3]["type"], "visual_complete");
        let phase = events[1]["phase"].as_str().unwrap();
        let shift = events[2]["pressed"].as_bool().unwrap();
        let mut replay = fixture_replay();
        replay.request_visual();
        if phase != "Selecting" {
            assert!(replay.session.snap_to(Rect::new(10, 10, 30, 30)));
            match phase {
                "Selected" => {}
                "Annotating" => replay.session.begin_annotating().unwrap(),
                "Exporting" => {
                    replay.session.begin_export().unwrap();
                }
                _ => panic!("UnsupportedContract phase={phase}"),
            }
        }
        replay.shift_down = shift;
        if shift {
            replay.clear_snap_preview_for_modifier();
        }
        let before = replay.session.selection();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !replay.visual.as_ref().unwrap().idle() {
            replay.on_visual_ready();
            assert!(Instant::now() < deadline, "T15 worker did not drain");
            std::thread::sleep(Duration::from_micros(100));
        }
        replay.on_visual_ready();
        let visible = phase == "Selecting" && replay.gesture.snap_preview().is_some();
        let expected = scenario["assertions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["op"] == "preview_visible")
            .unwrap()["equals"]
            .as_bool()
            .unwrap();
        assert_eq!(visible, expected, "{}", scenario["id"]);
        assert_eq!(
            replay.session.selection(),
            before,
            "late visual changed selection"
        );
        count += 1;
    }
    assert_eq!(count, 8);
    println!(
        "DEMO_EVENTS T15_scenarios={count} actual_controller=true completion_from_real_pixels=true"
    );
}

#[test]
#[ignore = "L3 real controller/GPU; injected result-allocation fault with and without native and previous visual results"]
fn visual_replay_demo_t17_failures_preserve_native_and_manual_and_retire_old_preview() {
    for (native, previous_visual) in [(false, false), (true, false), (false, true)] {
        let mut replay = fixture_replay();
        let owner = replay.hover_target.unwrap().owner;
        if native {
            answer(
                &mut replay,
                vec![
                    PathLevel::window(owner.screen_bounds),
                    PathLevel::new(Rect::new(30, 20, 170, 120), LevelKind::Button),
                ],
            );
        }
        replay.refresh_preview_for_cursor();
        if previous_visual {
            analyze(&mut replay);
        }
        let ((), hit) =
            crate::visual_detection::allocation::fail_at("published_rectangles", || {
                replay.request_visual();
                let deadline = Instant::now() + Duration::from_secs(2);
                while !replay.visual.as_ref().unwrap().idle() {
                    replay.on_visual_ready();
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_micros(100));
                }
                replay.on_visual_ready();
            });
        assert!(hit, "selected result allocation was not reached");
        let preview = replay.gesture.snap_preview().unwrap();
        assert_eq!(
            preview.target.origin,
            if native {
                TargetOrigin::Accessibility
            } else {
                TargetOrigin::Window
            }
        );
        // Clear the old dwell and exercise the exact unconstrained manual vector.
        replay.clear_snap_preview_for_modifier();
        replay.on_left_down(POINT { x: 10, y: 10 });
        replay.on_mouse_move(POINT { x: 80, y: 70 });
        replay.on_left_up();
        assert_eq!(replay.session.selection(), Rect::new(10, 10, 80, 70));
        assert!(replay.visual.as_ref().unwrap().published.is_none());
    }
    // Independent native-failure / visual-success situation, then exact manual drag.
    let mut replay = fixture_replay();
    analyze(&mut replay);
    assert_eq!(
        replay.gesture.snap_preview().unwrap().target.origin,
        TargetOrigin::Visual
    );
    replay.clear_snap_preview_for_modifier();
    replay.on_left_down(POINT { x: 10, y: 10 });
    replay.on_mouse_move(POINT { x: 80, y: 70 });
    replay.on_left_up();
    assert_eq!(replay.session.selection(), Rect::new(10, 10, 80, 70));
}

#[test]
#[ignore = "L3 actual product pixels/controller for frozen T19 and all Q13 gap members; synthetic development input only"]
fn visual_replay_demo_t19_and_q13_similar_pixels_reach_product_default() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/visual-candidate-v4.1/manifest.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["id"] == "T19" || c["id"] == "Q13" || c["id"] == "T21")
    {
        let members: Vec<&serde_json::Value> = case["members"]
            .as_array()
            .map(|v| v.iter().collect())
            .unwrap_or_else(|| vec![case]);
        for member in members {
            for query in member["queries"].as_array().unwrap() {
                let point = Point::new(
                    query["point"][0].as_i64().unwrap() as i32,
                    query["point"][1].as_i64().unwrap() as i32,
                );
                let mut replay = fixture_named(member["id"].as_str().unwrap(), point);
                analyze(&mut replay);
                let preview = replay.gesture.snap_preview().unwrap();
                match query["expected_default"]["kind"].as_str().unwrap() {
                    "one_of_annotations" => {
                        let accepted: Vec<Rect> = query["expected_default"]["ids"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|id| {
                                let a = member["annotations"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .find(|a| a["id"] == *id)
                                    .unwrap();
                                let v = &a["rect"];
                                Rect::new(
                                    v[0].as_i64().unwrap() as i32,
                                    v[1].as_i64().unwrap() as i32,
                                    (v[0].as_i64().unwrap() + v[2].as_i64().unwrap()) as i32,
                                    (v[1].as_i64().unwrap() + v[3].as_i64().unwrap()) as i32,
                                )
                            })
                            .collect();
                        assert_eq!(preview.target.origin, TargetOrigin::Visual);
                        // These are crisp integer gaps. Do not spend the manifest's
                        // AA allowance to hide a one-pixel neighbour merge.
                        assert!(
                            accepted.contains(&preview.selection),
                            "{}: {:?}",
                            query["id"],
                            preview.selection
                        );
                        let path = &replay.display_target().unwrap().path;
                        assert!(
                            path.windows(2)
                                .all(|p| p[0].rect != p[1].rect
                                    && p[0].rect.contains_rect(p[1].rect))
                        );
                    }
                    "no_visual_target_in_region" => {
                        assert_ne!(
                            preview.target.origin,
                            TargetOrigin::Visual,
                            "texture false default: {}",
                            query["id"]
                        );
                        assert!(
                            replay
                                .visual
                                .as_ref()
                                .unwrap()
                                .published
                                .as_ref()
                                .unwrap()
                                .rectangles
                                .is_empty()
                        );
                    }
                    unsupported => panic!("UnsupportedContract expected_default={unsupported}"),
                }
                count += 1;
            }
        }
    }
    assert_eq!(count, 16);
    println!("DEMO_PRODUCT T19_T21_Q13_queries={count} detection_and_fusion=true");
}
