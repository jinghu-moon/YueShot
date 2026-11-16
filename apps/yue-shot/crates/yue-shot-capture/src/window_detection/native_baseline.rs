//! P0 characterization of the existing native path, before SelectionTarget.
//! These compose production functions; they are not a visual detector or a UI replay.

use super::*;
use crate::geometry::{MonitorLayout, Point, Rect, window_rect_to_local};
use crate::monitor_cache::MonitorCache;
use serde_json::{Value, json};

fn rect_value(rect: Rect) -> Value {
    json!([rect.left, rect.top, rect.right, rect.bottom])
}

fn owner() -> WindowIdentity {
    WindowIdentity::new(101, 7, 11)
}

fn layout(dpi: u32) -> MonitorLayout {
    MonitorLayout {
        bounds: Rect::new(-640, -120, 0, 360),
        work_area: Rect::new(-640, -120, 0, 340),
        dpi,
        primary: false,
    }
}

fn snapshot() -> WindowSnapshot {
    WindowSnapshot::new(
        17,
        vec![
            WindowCandidate::new(owner(), Rect::new(-620, -100, -320, 300), 0, 17),
            WindowCandidate::new(
                WindowIdentity::new(102, 8, 12),
                Rect::new(-320, -100, -20, 300),
                1,
                17,
            ),
        ],
        MonitorCache::from_bounds([layout(96).bounds]),
    )
}

fn deep() -> DeepTarget {
    let bounds = Rect::new(-590, -60, -500, 0);
    DeepTarget {
        window: owner(),
        kind: TargetKind::UiElement,
        screen_bounds: bounds,
        path: vec![
            PathLevel::new(Rect::new(-620, -100, -320, 300), LevelKind::Window),
            PathLevel::new(Rect::new(-610, -90, -330, 290), LevelKind::Pane),
            // A near duplicate must not consume a visible wheel step.
            PathLevel::new(Rect::new(-609, -89, -331, 289), LevelKind::Group),
            PathLevel::new(bounds, LevelKind::Button),
        ],
        stop_reason: super::deep::StopReason::Complete,
    }
}

fn golden(name: &str) -> Value {
    let all: Value = serde_json::from_str(include_str!("native-baseline.json")).unwrap();
    all[name].clone()
}

#[test]
fn native_baseline_hit_mapping_and_epoch_trace() {
    let mut snapshot = snapshot();
    let queries = [
        Point::new(-620, -100),
        Point::new(-320, 0),
        Point::new(4, 0), // exactly 24 physical pixels beyond the right edge
        Point::new(5, 0),
    ];
    let mut trace = Vec::new();
    for dpi in [96, 120, 144, 192] {
        for point in queries {
            let hit = snapshot.hit_test(point).map(|t| t.identity().hwnd);
            let target = snapshot.nearest_target(point, DEFAULT_SNAP_RADIUS_PX);
            trace.push(json!({
                "dpi": dpi, "point": [point.x, point.y], "hit": hit,
                "nearest": target.map(|t| t.identity().hwnd),
                "local": target.map(|t| rect_value(window_rect_to_local(t.screen_bounds(), &layout(dpi)))),
            }));
        }
    }
    assert!(
        !snapshot.apply_candidate_update(&HoverValidity::BoundsChanged {
            epoch: 16,
            identity: owner(),
            new_bounds: Rect::new(0, 0, 20, 20),
        })
    );
    assert!(
        snapshot.apply_candidate_update(&HoverValidity::BoundsChanged {
            epoch: 17,
            identity: owner(),
            new_bounds: Rect::new(-610, -100, -320, 300),
        })
    );
    trace.push(json!({"updated": snapshot.hit_test(Point::new(-615, 0)).is_some()}));
    snapshot.release();
    trace.push(json!({
        "released_epoch": snapshot.epoch(),
        "released_nearest": snapshot.nearest_target(queries[0], 24).is_some(),
        "released_monitors": snapshot.monitors().len(),
    }));
    assert_eq!(json!(trace), golden("hit_mapping"));
}

#[test]
fn native_baseline_pending_preview_and_visible_layer_trace() {
    let snapshot = snapshot();
    let target = snapshot.hit_test(Point::new(-550, -30)).unwrap();
    let deep = deep();
    let point = Point::new(-550, -30);
    let outside = Point::new(-450, 150);
    let mut trace = Vec::new();
    for (name, verified, at, waiting) in [
        ("first_pending", None, point, true),
        ("first_empty", None, point, false),
        ("inside_verified", Some(&deep), point, false),
        ("outside_pending", Some(&deep), outside, true),
        ("outside_empty", Some(&deep), outside, false),
    ] {
        trace.push(json!({"event": name, "preview": preview_bounds(
            verified, None, target.identity(), at, target.screen_bounds(), waiting
        ).map(|r| rect_value(window_rect_to_local(r, &layout(144))))}));
    }
    let mut chain = LevelChain::new(deep.path.len());
    for direction in [-1, -1, -1, 1, 1, 1] {
        let next = next_visible_stop(
            &deep.path,
            chain.index(),
            direction,
            RingOptions::default().collapse_gap_px,
        );
        let moved = chain.jump_to(next);
        let plan = chain_rings(&deep.path, chain.index(), RingOptions::default());
        trace.push(json!({
            "direction": direction, "moved": moved, "index": chain.index(),
            "kind": deep.path[chain.index()].kind.debug_name(),
            "preview": rect_value(window_rect_to_local(chain.current(&deep.path).unwrap().rect, &layout(144))),
            "rings": plan.rings.iter().map(|r| json!([r.index, format!("{:?}", r.role)])).collect::<Vec<_>>(),
            "shallower": stops_from(&deep.path, chain.index(), -1, RingOptions::default().collapse_gap_px),
            "deeper": stops_from(&deep.path, chain.index(), 1, RingOptions::default().collapse_gap_px),
        }));
    }
    assert_eq!(json!(trace), golden("preview_layers"));
}

#[test]
fn native_baseline_press_blocks_late_publish_and_threshold_is_inclusive() {
    use crate::geometry::SelectionGeometry;

    let target = snapshot().hit_test(Point::new(-550, -30)).unwrap();
    let local = window_rect_to_local(deep().screen_bounds, &layout(96));
    let mut gesture = GestureState::new(5);
    let generation = gesture.dwell_generation();
    assert!(gesture.apply_dwell(generation, Some((target, local, Rect::default()))));
    let press = Point::new(90, 90);
    assert_eq!(
        gesture.press(press, SelectionGeometry::Create, Rect::default()),
        PressOutcome::Pending
    );
    assert_eq!(gesture.snap_preview().unwrap().selection, local);
    assert!(!gesture.apply_dwell(generation, None));
    assert!(!gesture.apply_dwell(gesture.dwell_generation(), None));
    assert_eq!(
        gesture.move_cursor(Point::new(92, 92)),
        MoveOutcome::Pending
    );
    assert_eq!(gesture.release(), ReleaseOutcome::Click);
    assert_eq!(gesture.snap_preview().unwrap().selection, local);
    gesture.press(press, SelectionGeometry::Create, Rect::default());
    assert_eq!(
        gesture.move_cursor(Point::new(93, 94)),
        MoveOutcome::ManualDragStarted { press_point: press }
    );
    assert!(gesture.snap_preview().is_none());
    assert_eq!(gesture.release(), ReleaseOutcome::CommitDrag);
    assert!(!gesture.apply_dwell(generation, Some((target, local, Rect::default()))));
}
