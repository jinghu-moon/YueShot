//! Frozen Demo vectors at the production fusion boundary, not pixel-detection results.
use super::*;
use serde_json::Value;

fn manifest() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/visual-candidate-v4.1/manifest.json"
    ))
    .unwrap()
}

fn rect(v: &Value) -> Rect {
    let a = v.as_array().unwrap();
    let a: Vec<i32> = a
        .iter()
        .map(|x| i32::try_from(x.as_i64().unwrap()).unwrap())
        .collect();
    assert_eq!(a.len(), 4);
    assert!(a[2] > 0 && a[3] > 0);
    Rect::new(a[0], a[1], a[0] + a[2], a[1] + a[3])
}

fn scene_at(point: Point) -> (VisualSnapshot, WindowSnapshot, Rect) {
    let (mut visual, _, _) = scene();
    let bounds = Rect::new(0, 0, 640, 640);
    visual.owner.screen_bounds = bounds;
    visual.point = point;
    let windows = WindowSnapshot::new(17, vec![visual.owner], MonitorCache::from_bounds([bounds]));
    (visual, windows, bounds)
}

fn kind(v: &Value) -> LevelKind {
    LevelKind::ALL
        .into_iter()
        .find(|k| format!("{k:?}") == v.as_str().unwrap())
        .unwrap()
}

#[test]
fn visual_replay_demo_t11_t20_q09_native_policy_vectors() {
    let manifest = manifest();
    let mut count = 0;
    for case in manifest["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let vectors = match id {
            "T11" => &case["input"]["variants"],
            "T20" => &case["input"]["variants"],
            "Q09" => &case["members"],
            _ => continue,
        };
        for variant in vectors.as_array().unwrap() {
            let v = if id == "Q09" {
                &variant["stage_vectors"]
            } else {
                variant
            };
            let (mut visual, windows, bounds) = scene_at(Point::new(100, 80));
            let no_owner = (id == "T20" || id == "Q09") && v["owner"].is_null();
            let expected = v["expected_default"]
                .as_str()
                .or(v["expected"].as_str())
                .unwrap();
            if no_owner {
                // A source without a real owner is never made into a SelectionTarget.
                assert!(!visual.current(visual.source, &WindowSnapshot::empty(), visual.point));
                assert!(
                    visual
                        .fuse(None, &WindowSnapshot::empty(), bounds)
                        .is_none()
                );
                assert_eq!(expected, "no_visual_default");
            } else {
                let vr = v.get("visual_rect").or(v.get("visual")).unwrap();
                visual.rectangles = vec![rect(vr)];
                if id == "Q09" {
                    visual.point = Point::new(128, 96);
                }
                let k = v.get("native_kind").or(v.get("kind")).map(kind);
                let native = k.filter(|_| v["native_adopted"] != false).map(|k| {
                    let r = rect(v.get("native_rect").or(v.get("rect")).unwrap());
                    DeepTarget {
                        window: visual.owner.identity,
                        screen_bounds: r,
                        path: vec![PathLevel::window(bounds), PathLevel::new(r, k)],
                        stop_reason: StopReason::Complete,
                    }
                });
                let result = visual.fuse(native.as_ref(), &windows, bounds).unwrap();
                let selected = result
                    .path
                    .iter()
                    .find(|p| p.rect == result.screen_bounds)
                    .unwrap();
                assert_eq!(format!("{:?}", selected.origin), expected, "{id}: {v}");
            }
            count += 1;
        }
    }
    assert_eq!(count, 38);
    println!("DEMO_FUSION native_policy_vectors={count} stage_only=true");
}

#[test]
fn visual_replay_demo_q07_nested_duplicate_crossing_stage_vectors() {
    let manifest = manifest();
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "Q07")
        .unwrap();
    let mut count = 0;
    for member in case["members"].as_array().unwrap() {
        let v = &member["stage_vectors"];
        assert_eq!(v["entry"], "candidate_fusion");
        let (mut visual, windows, bounds) = scene_at(Point::new(128, 96));
        let mut candidates: Vec<Rect> = v["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                assert_eq!(c["qualified"], true);
                assert_eq!(c["owner"], "fixture-window-1");
                rect(&c["rect"])
            })
            .collect();
        // Vectors have equal evidence. Resolve that tie before passing ranked evidence
        // to the production snapshot, just as the detector's complete comparator does.
        candidates.sort_unstable_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
        visual.rectangles = candidates.clone();
        let result = visual.fuse(None, &windows, bounds).unwrap();
        let path: Vec<Rect> = result
            .path
            .iter()
            .filter(|l| l.origin == TargetOrigin::Visual)
            .map(|l| l.rect)
            .collect();
        if let Some(expected) = v["expected_chain_rects"].as_array() {
            assert_eq!(
                path,
                expected.iter().map(rect).collect::<Vec<_>>(),
                "{}",
                member["id"]
            );
            assert_eq!(
                path.len(),
                v["expected_stop_count"].as_u64().unwrap() as usize
            );
            let mut index = result.path.len() - 1;
            for expected in path.iter().rev().skip(1) {
                index = next_visible_stop(&result.path, index, -1, 3);
                assert_eq!(result.path[index].rect, *expected);
            }
        } else {
            assert_eq!(v["expected_relation"], "crossing_alternatives");
            assert_eq!(path.len(), 1);
            candidates.reverse();
            candidates.sort_unstable_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
            visual.rectangles = candidates;
            assert_eq!(visual.fuse(None, &windows, bounds).unwrap(), result);
        }
        count += 1;
    }
    assert_eq!(count, 4);
    println!("DEMO_FUSION q07_vectors={count} stage_only=true");
}

#[test]
fn visual_replay_demo_t12_physical_dpi_origin_and_occlusion_vectors() {
    let manifest = manifest();
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "T12")
        .unwrap();
    let mut count = 0;
    for v in case["input"]["vectors"].as_array().unwrap() {
        if let Some(dpi) = v["dpi"].as_u64() {
            let origin = &v["monitor_origin"];
            let x = origin[0].as_i64().unwrap() as i32;
            let y = origin[1].as_i64().unwrap() as i32;
            let layout = crate::geometry::MonitorLayout {
                bounds: Rect::new(x, y, x + 1920, y + 1080),
                work_area: Rect::new(x, y, x + 1920, y + 1080),
                dpi: dpi as u32,
                primary: false,
            };
            let r = rect(&v["rect_local"]);
            let a = layout.to_screen(Point::new(r.left, r.top));
            let b = layout.to_screen(Point::new(r.right, r.bottom));
            let actual = Rect::new(a.x, a.y, b.x, b.y);
            assert_eq!(actual, rect(&v["expected_screen"]));
            assert_eq!(crate::geometry::window_rect_to_local(actual, &layout), r);
        } else {
            let (mut visual, _, _) = scene_at(Point::new(130, 120));
            let windows = v["windows"].as_array().unwrap();
            let front = windows.iter().find(|w| w["id"] == "front").unwrap();
            let back = windows.iter().find(|w| w["id"] == "back").unwrap();
            let bounds = rect(&back["rect"]);
            visual.owner.screen_bounds = bounds;
            visual.owner.z_order = back["z"].as_u64().unwrap() as u32;
            visual.rectangles = vec![rect(&v["candidate"]["rect"])];
            let mut occluder = visual.owner;
            occluder.identity.hwnd += 1;
            occluder.z_order = front["z"].as_u64().unwrap() as u32;
            occluder.screen_bounds = rect(&front["rect"]);
            let windows = WindowSnapshot::new(
                17,
                vec![occluder, visual.owner],
                MonitorCache::from_bounds([bounds]),
            );
            assert_eq!(v["expected"], "reject");
            assert!(visual.fuse(None, &windows, bounds).is_none());
        }
        count += 1;
    }
    assert_eq!(count, 4);
}
