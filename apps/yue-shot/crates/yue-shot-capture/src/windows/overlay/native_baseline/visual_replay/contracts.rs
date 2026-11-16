//! Demo event vectors injected at the qualified-result boundary. The controller,
//! geometry, wheel walking, source qualification and confirmation are production code.
use super::*;
use serde_json::Value;

fn manifest() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/visual-candidate-v4.1/manifest.json"
    ))
    .unwrap()
}

fn rectangle(v: &Value) -> Rect {
    let a: Vec<i32> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|v| i32::try_from(v.as_i64().unwrap()).unwrap())
        .collect();
    assert_eq!(a.len(), 4);
    assert!(a[2] > 0 && a[3] > 0);
    Rect::new(a[0], a[1], a[0] + a[2], a[1] + a[3])
}

fn contract_replay() -> Replay {
    replay_from_frame(
        crate::windows::providers::test_gpu_frozen_frame(
            640,
            640,
            [32, 64, 96, 255].repeat(640 * 640),
        ),
        Point::new(100, 80),
    )
}

fn publish(replay: &mut Replay, rectangles: Vec<Rect>) {
    publish_for_owner(replay, replay.hover_target.unwrap().owner, rectangles);
}

fn publish_for_owner(replay: &mut Replay, owner: WindowCandidate, mut rectangles: Vec<Rect>) {
    rectangles.sort_unstable_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
    replay.visual.as_mut().unwrap().published =
        Some(crate::window_detection::visual::VisualSnapshot {
            source: crate::window_detection::SelectionSource {
                capture_generation: replay.current_generation,
                pixels: replay.frozen.as_ref().unwrap().frame.source,
            },
            owner,
            point: replay.cursor_screen().unwrap(),
            rectangles,
        });
    replay.refresh_preview_for_cursor();
}

#[test]
#[ignore = "L3 controller with qualified-result injection: frozen Demo T13/T14/T23 events, no detector qualification"]
fn visual_replay_demo_t13_t14_t23_controller_event_contracts() {
    let manifest = manifest();
    let mut cases = 0;
    for id in ["T13", "T14", "T23"] {
        let case = manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap();
        let mut replay = contract_replay();
        let owner = replay.hover_target.unwrap().owner;
        let mut requested = None;
        let mut locked = None;
        let mut wheel_steps = 0;
        for event in case["input"]["events"].as_array().unwrap() {
            match event["type"].as_str().unwrap() {
                "preview" => {
                    let rect = rectangle(&event["rect"]);
                    replay.preview(SelectionTarget::window(owner), rect);
                    requested = Some(rect);
                }
                "pointer_down" => replay.on_left_down(POINT { x: 100, y: 80 }),
                "request_confirm" => {
                    // Use the real generated request id. Demo's 7 is its symbolic
                    // identity; never overwrite the product request counter.
                    assert_eq!(event["id"], 7);
                    assert!(replay.confirm_snap_preview());
                }
                "publish_visual" => publish(&mut replay, vec![rectangle(&event["rect"])]),
                "confirm_result" => {
                    let request = replay.confirm_request.unwrap();
                    let target = replay.confirming_preview.unwrap().target;
                    replay.confirmation(request, target, event["valid"].as_bool().unwrap());
                }
                "publish_chain" => publish(
                    &mut replay,
                    event["rects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(rectangle)
                        .collect(),
                ),
                "wheel_select" => {
                    let expected = rectangle(&event["rect"]);
                    let mut steps = 0;
                    while replay.gesture.snap_preview().unwrap().selection != expected {
                        assert!(replay.step_deep_level(-1));
                        steps += 1;
                        assert!(steps < 24);
                    }
                    locked = Some(expected);
                }
                "checkpoint" => {
                    assert_eq!(event["name"], "locked");
                    assert_eq!(
                        replay.gesture.snap_preview().unwrap().selection,
                        locked.unwrap()
                    );
                }
                "pointer_move" => {
                    let point = Point::new(
                        event["point"][0].as_i64().unwrap() as i32,
                        event["point"][1].as_i64().unwrap() as i32,
                    );
                    replay.cursor = point;
                    replay.drive_refinement(point, Some(SelectionTarget::window(owner)));
                    assert!(replay.manual_level.is_none());
                }
                "wheel_walk_all" => {
                    let path = replay.display_target().unwrap().path.clone();
                    assert!(
                        path.windows(2)
                            .all(|p| p[0].rect != p[1].rect && p[0].rect.contains_rect(p[1].rect))
                    );
                    let close = Rect::new(11, 11, 209, 169);
                    let mut visited = vec![replay.gesture.snap_preview().unwrap().selection];
                    while replay.step_deep_level(-1) {
                        visited.push(replay.gesture.snap_preview().unwrap().selection);
                        wheel_steps += 1;
                        assert!(wheel_steps < 24);
                    }
                    assert!(visited.contains(&close), "one-pixel visual layer skipped");
                    assert_eq!(
                        visited
                            .iter()
                            .filter(|r| **r == Rect::new(10, 10, 210, 170))
                            .count(),
                        1
                    );
                    let chosen = replay.gesture.snap_preview().unwrap().selection;
                    publish(&mut replay, vec![Rect::new(20, 20, 200, 150)]);
                    assert_eq!(replay.gesture.snap_preview().unwrap().selection, chosen);
                }
                event => panic!("UnsupportedContract event={event}"),
            }
        }
        match id {
            "T13" => assert_eq!(replay.session.selection(), requested.unwrap()),
            "T14" => assert!(replay.manual_level.is_none()),
            "T23" => assert!(wheel_steps >= 3),
            _ => unreachable!(),
        }
        println!("DEMO_EVENTS case={id} wheel_steps={wheel_steps} boundary=qualified_result");
        cases += 1;
    }
    assert_eq!(cases, 3);

    // T13's declared variants: the three late sources and confirmation may
    // arrive in any order after the request (4! = 24). A refreshed epoch with
    // the same owner/bounds must not erase the requested rectangle.
    let mut permutations = 0;
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let order = [a, b, c, d];
                    if (0..4).any(|i| (i + 1..4).any(|j| order[i] == order[j])) {
                        continue;
                    }
                    let mut replay = contract_replay();
                    let owner = replay.hover_target.unwrap().owner;
                    let requested = Rect::new(40, 40, 200, 160);
                    replay.preview(SelectionTarget::window(owner), requested);
                    replay.on_left_down(POINT { x: 100, y: 80 });
                    assert!(replay.confirm_snap_preview());
                    let request = replay.confirm_request.unwrap();
                    let target = replay.confirming_preview.unwrap().target;
                    for event in order {
                        match event {
                            0 => publish_for_owner(
                                &mut replay,
                                owner,
                                vec![Rect::new(55, 50, 155, 130)],
                            ),
                            1 => answer(
                                &mut replay,
                                vec![
                                    PathLevel::window(owner.screen_bounds),
                                    PathLevel::new(Rect::new(30, 30, 220, 180), LevelKind::Custom),
                                ],
                            ),
                            2 => {
                                let snapshot_request =
                                    replay.detector.request_refresh(&Exclusions::default());
                                replay.snapshot_request = Some(snapshot_request);
                                let refreshed = WindowCandidate {
                                    snapshot_epoch: 18,
                                    ..owner
                                };
                                replay
                                    .detector
                                    .publish_for_replay(DetectionResult::Refreshed {
                                        request: snapshot_request,
                                        snapshot: Ok(WindowSnapshot::new(
                                            18,
                                            vec![refreshed],
                                            crate::monitor_cache::MonitorCache::from_bounds([
                                                owner.screen_bounds,
                                            ]),
                                        )),
                                    });
                                replay.on_detection_ready();
                            }
                            3 => replay.confirmation(request, target, true),
                            _ => unreachable!(),
                        }
                    }
                    assert_eq!(
                        replay.session.state(),
                        CaptureState::Selected,
                        "T13 order={order:?}"
                    );
                    assert_eq!(replay.session.selection(), requested, "T13 order={order:?}");
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
                    permutations += 1;
                }
            }
        }
    }
    assert_eq!(permutations, 24);
    println!("DEMO_EVENTS T13_arrival_permutations={permutations} source=qualified_result");
    for fault in 0..8 {
        let mut replay = contract_replay();
        let owner = replay.hover_target.unwrap().owner;
        let target = SelectionTarget::window(owner).bind_source(
            replay.current_generation,
            replay.frozen.as_ref().unwrap().frame.source,
        );
        replay.preview(target, Rect::new(40, 40, 200, 160));
        assert!(replay.confirm_snap_preview());
        let request = replay.confirm_request.unwrap();
        let mut changed = WindowCandidate {
            snapshot_epoch: 18,
            ..owner
        };
        let mut candidates = vec![changed];
        match fault {
            0 => candidates.clear(),
            1 => candidates[0].screen_bounds.right -= 1,
            2 => candidates[0].identity.process_id += 1,
            3 => candidates[0].identity.class_name_hash += 1,
            4 => {
                changed.z_order = 1;
                candidates[0] = changed;
                candidates.push(WindowCandidate {
                    identity: WindowIdentity::new(102, 7, 11, 1),
                    screen_bounds: Rect::new(60, 60, 120, 120),
                    z_order: 0,
                    ..changed
                });
            }
            5 => replay.current_generation += 1,
            6 => {
                replay.frozen.as_mut().unwrap().frame.source = yue_media::pixels::PixelSource::new(
                    target.source.unwrap().pixels.descriptor(),
                    target.source.unwrap().pixels.generation() + 1,
                )
            }
            7 => candidates[0].identity.creation_token += 1,
            _ => unreachable!(),
        }
        candidates.sort_unstable_by_key(|candidate| candidate.z_order);
        let snapshot_request = replay.detector.request_refresh(&Exclusions::default());
        replay.snapshot_request = Some(snapshot_request);
        replay
            .detector
            .publish_for_replay(DetectionResult::Refreshed {
                request: snapshot_request,
                snapshot: Ok(WindowSnapshot::new(
                    18,
                    candidates,
                    crate::monitor_cache::MonitorCache::from_bounds([owner.screen_bounds]),
                )),
            });
        replay.on_detection_ready();
        replay.confirmation(request, target, true);
        assert_eq!(
            replay.session.state(),
            CaptureState::Selecting,
            "unsafe refresh accepted fault={fault}"
        );
        assert!(replay.session.selection().is_empty());
        assert!(replay.shared.lock().unwrap().selection.is_none());
    }
    println!("DEMO_EVENTS T13_unsafe_refresh_rejections=7");
}

#[test]
#[ignore = "L3 actual controller/GPU; injects Windows display/power notifications, never suspends the physical computer"]
fn visual_replay_display_and_power_messages_retire_inflight_sources() {
    let messages = [
        (WM_DISPLAYCHANGE, 0),
        (WM_DEVICECHANGE, 0),
        (WM_DPICHANGED, 0),
        (
            windows_sys::Win32::UI::WindowsAndMessaging::WM_POWERBROADCAST,
            4,
        ),
        (
            windows_sys::Win32::UI::WindowsAndMessaging::WM_POWERBROADCAST,
            7,
        ),
        (
            windows_sys::Win32::UI::WindowsAndMessaging::WM_POWERBROADCAST,
            18,
        ),
    ];
    for (message, wparam) in messages {
        let mut replay = fixture_replay();
        replay.request_visual();
        assert!(!replay.visual.as_ref().unwrap().idle());
        unsafe {
            replay.handle(message, wparam, 0);
        }
        assert_eq!(replay.session.state(), CaptureState::Idle);
        assert!(replay.frozen.is_none() && replay.renderer.is_none());
        assert!(replay.combined_target.is_none() && replay.gesture.snap_preview().is_none());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !replay.visual.as_ref().unwrap().idle() {
            replay.on_visual_ready();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_micros(100));
        }
        assert!(replay.visual.as_ref().unwrap().published.is_none());
    }
}

#[test]
#[ignore = "L3 real controller/GPU: cancel/re-arm with retired visual and native confirmation answers; synthetic source bytes"]
fn visual_replay_demo_e08_second_session_rejects_old_answers_and_remains_operable() {
    let mut replay = fixture_replay();
    analyze(&mut replay);
    let old_snapshot = replay.visual.as_ref().unwrap().published.clone().unwrap();
    let old_target = replay.gesture.snap_preview().unwrap().target;
    assert!(replay.confirm_snap_preview());
    let old_request = replay.confirm_request.unwrap();
    let frame = replay.frozen.take().unwrap();
    replay.cancel("E08-between-sessions");
    assert_eq!(replay.session.state(), CaptureState::Idle);
    // Re-arm immutable bytes as a new logical capture. Capture-worker/native-F5
    // qualification separately proves fresh OS pixels; this is the session gate.
    replay.prepare_overlay(&frame.monitor, &frame).unwrap();
    replay.session = CaptureSession::new("E08-second");
    replay
        .session
        .arm(frame.frame.clone(), &frame.monitor.layout)
        .unwrap();
    replay.session.overlay_ready().unwrap();
    replay.current_generation = 2;
    let owner = WindowCandidate {
        snapshot_epoch: 18,
        ..old_snapshot.owner
    };
    replay.snapshot = WindowSnapshot::new(
        18,
        vec![owner],
        crate::monitor_cache::MonitorCache::from_bounds([owner.screen_bounds]),
    );
    replay.cursor = Point::new(91, 66);
    replay.cursor_visible = true;
    replay.hover_target = Some(SelectionTarget::window(owner));
    replay.frozen = Some(frame);
    replay.visual.as_mut().unwrap().published = Some(old_snapshot);
    replay.refresh_preview_for_cursor();
    assert_ne!(
        replay.gesture.snap_preview().unwrap().target.origin,
        TargetOrigin::Visual
    );
    replay.confirmation(old_request, old_target, true);
    assert_eq!(replay.session.state(), CaptureState::Selecting);
    assert!(replay.session.selection().is_empty());
    analyze(&mut replay);
    let current = replay.gesture.snap_preview().unwrap();
    assert_eq!(current.target.source.unwrap().capture_generation, 2);
    assert!(replay.confirm_snap_preview());
    let request = replay.confirm_request.unwrap();
    replay.confirmation(request, current.target, true);
    assert_eq!(replay.session.selection(), Rect::new(41, 31, 141, 101));
}
