use super::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Explicit, offline reproduction of a supplied screenshot. This is a diagnostic,
/// not a geometry assertion or an acceptance result. Never loads images implicitly.
#[test]
#[ignore = "requires YUESHOT_VISUAL_PROBE_PNG and YUESHOT_VISUAL_PROBE_POINTS=x,y;x,y"]
fn visual_diagnose_supplied_png() {
    let path = std::env::var_os("YUESHOT_VISUAL_PROBE_PNG").expect("explicit PNG path required");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(std::fs::read(path).unwrap()));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    pixels.truncate(info.buffer_size());
    let size = [info.width as usize, info.height as usize];
    let read = |rect: Rect| {
        let mut bytes = Vec::with_capacity(rect.area() as usize * 4);
        for y in rect.top..rect.bottom {
            let at = (y as usize * size[0] + rect.left as usize) * 4;
            bytes.extend_from_slice(&pixels[at..at + rect.width() as usize * 4]);
        }
        bytes
    };
    for pair in std::env::var("YUESHOT_VISUAL_PROBE_POINTS")
        .unwrap()
        .split(';')
    {
        let (x, y) = pair.split_once(',').expect("x,y required");
        let point = Point::new(x.parse().unwrap(), y.parse().unwrap());
        let roi = progressive::Query::initial_region(size, point).unwrap();
        let mut query = progressive::Query::new(size, point, roi, read(roi)).unwrap();
        let start = Instant::now();
        let result = loop {
            match query.step() {
                progressive::Step::Read(regions) => query
                    .apply(regions.into_iter().map(|r| (r, read(r))).collect())
                    .unwrap(),
                progressive::Step::Complete(result) => break serde_json::to_value(result).unwrap(),
                progressive::Step::Unavailable(reason) => break json!({"unavailable":reason}),
            }
        };
        println!(
            "VISUAL_PNG_PROBE {}",
            json!({"size":size,"point":[point.x,point.y],"result":result,"passes":query.passes,"extraBytes":query.extra_bytes,"stageUs":query.stage_us,"elapsedUs":start.elapsed().as_micros()})
        );
    }
}

#[test]
fn visual_progressive_embedded_targets_change_with_seed() {
    let (tile_size, tile) = decode("T01.png");
    for (size, origin) in [([1440, 1000], [628, 343]), ([3840, 2160], [1600, 600])] {
        let mut pixels = [245, 247, 251, 255].repeat(size[0] * size[1]);
        for y in 0..tile_size[1] {
            let to = ((origin[1] + y) * size[0] + origin[0]) * 4;
            let from = y * tile_size[0] * 4;
            pixels[to..to + tile_size[0] * 4].copy_from_slice(&tile[from..from + tile_size[0] * 4]);
        }
        let read = |rect: Rect| {
            let mut bytes = Vec::with_capacity(rect.area() as usize * 4);
            for y in rect.top..rect.bottom {
                let at = (y as usize * size[0] + rect.left as usize) * 4;
                bytes.extend_from_slice(&pixels[at..at + rect.width() as usize * 4]);
            }
            bytes
        };
        let offset = |r: Rect| {
            Rect::new(
                r.left + origin[0] as i32,
                r.top + origin[1] as i32,
                r.right + origin[0] as i32,
                r.bottom + origin[1] as i32,
            )
        };
        let blue = offset(Rect::new(41, 31, 141, 101));
        let light = offset(Rect::new(240, 40, 340, 110));
        for (x, y, expected) in [
            (91, 66, Some(blue)),
            (290, 75, Some(light)),
            (20, 20, None),
            (210, 170, None),
        ] {
            let point = Point::new(origin[0] as i32 + x, origin[1] as i32 + y);
            let roi = progressive::Query::initial_region(size, point).unwrap();
            let mut query = progressive::Query::new(size, point, roi, read(roi)).unwrap();
            let result = loop {
                match query.step() {
                    progressive::Step::Read(regions) => query
                        .apply(regions.into_iter().map(|r| (r, read(r))).collect())
                        .unwrap(),
                    progressive::Step::Complete(result) => break result,
                    progressive::Step::Unavailable(reason) => {
                        panic!("embedded target unavailable: {size:?}/{point:?}: {reason:?}")
                    }
                }
            };
            assert_eq!(result.completion, Completion::Complete);
            if let Some(expected) = expected {
                assert_eq!(result.candidates.first().map(|c| c.rect), Some(expected));
            } else {
                assert!(
                    !result
                        .candidates
                        .iter()
                        .any(|c| c.rect == blue || c.rect == light)
                );
            }
        }
    }
}

pub(crate) fn distant_clutter_scene() -> ([usize; 2], Vec<u8>, Point, Rect) {
    let size = [3840, 2160];
    let point = Point::new(1900, 1050);
    let roi = progressive::Query::initial_region(size, point).unwrap();
    // Dense distant alternating structure is unrelated to the complete local box.
    let mut pixels = Vec::with_capacity(size[0] * size[1] * 4);
    for y in 0..size[1] as i32 {
        for x in 0..size[0] as i32 {
            let value = if roi.contains(Point::new(x, y)) {
                232
            } else if (x / 8 + y / 8) % 2 == 0 {
                16
            } else {
                240
            };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let target = Rect::new(1850, 1015, 1950, 1085);
    for y in target.top..target.bottom {
        for x in target.left..target.right {
            let at = (y as usize * size[0] + x as usize) * 4;
            pixels[at..at + 4].copy_from_slice(&[40, 80, 200, 255]);
        }
    }
    (size, pixels, point, target)
}

#[test]
fn visual_progressive_distant_clutter_must_not_erase_a_local_target() {
    let (size, pixels, point, target) = distant_clutter_scene();
    let roi = progressive::Query::initial_region(size, point).unwrap();
    let read = |r: Rect| {
        let mut bytes = Vec::new();
        for y in r.top..r.bottom {
            let at = (y as usize * size[0] + r.left as usize) * 4;
            bytes.extend_from_slice(&pixels[at..at + r.width() as usize * 4]);
        }
        bytes
    };
    let mut query = progressive::Query::new(size, point, roi, read(roi)).unwrap();
    let local = query.local().expect("complete owned ROI result");
    assert_eq!(local.completion, Completion::Complete);
    assert_eq!(local.candidates.first().map(|c| c.rect), Some(target));
    let completion = loop {
        match query.step() {
            progressive::Step::Read(regions) => query
                .apply(regions.into_iter().map(|r| (r, read(r))).collect())
                .unwrap(),
            progressive::Step::Complete(result) => break result.completion,
            progressive::Step::Unavailable(reason) => break reason,
        }
    };
    // Full-source expansion still exceeds the original budget. Its independent
    // failure must not prevent a fully validated local result from being used.
    assert_eq!(completion, Completion::BudgetExceeded);
    assert_eq!(local.candidates.first().map(|c| c.rect), Some(target));
}

#[test]
fn visual_local_missing_probe_halo_cannot_be_treated_as_source_boundary() {
    let size = [1024, 1024];
    let point = Point::new(700, 450);
    let roi = progressive::Query::initial_region(size, point).unwrap();
    let target = Rect::new(450, 400, 900, 500);
    let mut pixels = [232, 232, 232, 255].repeat(size[0] * size[1]);
    for y in target.top..target.bottom {
        for x in target.left..target.right {
            let at = (y as usize * size[0] + x as usize) * 4;
            pixels[at..at + 4].copy_from_slice(&[40, 80, 200, 255]);
        }
    }
    let full = proposals::seed(Image::new(size, &pixels).unwrap(), point);
    assert!(full.candidates.iter().any(|c| c.rect == target));
    let mut initial = Vec::new();
    for y in roi.top..roi.bottom {
        let at = (y as usize * size[0] + roi.left as usize) * 4;
        initial.extend_from_slice(&pixels[at..at + roi.width() as usize * 4]);
    }
    let mut query = progressive::Query::new(size, point, roi, initial).unwrap();
    assert!(
        query.local().unwrap().candidates.is_empty(),
        "missing outer samples were treated as verified evidence"
    );
}

#[test]
fn visual_replay_each_algorithm_allocation_site_fails_without_partial_publication() {
    let (size, pixels) = decode("T21.png");
    let point = Point::new(450, 340);
    let read = |rect: Rect| {
        let mut bytes = Vec::new();
        for y in rect.top..rect.bottom {
            let at = (y as usize * size[0] + rect.left as usize) * 4;
            bytes.extend_from_slice(&pixels[at..at + rect.width() as usize * 4]);
        }
        bytes
    };
    for site in [
        "tile_index",
        "requested_regions",
        "missing_tiles",
        "scan_peaks",
        "peak_positions",
        "edge_spans",
        "corner_rectangles",
        "horizontal_runs",
        "merged_runs",
        "run_index",
        "pending_regions",
        "tile_pixels",
        "validation_positions",
        "verified_candidates",
    ] {
        let (failed, hit) = allocation::fail_at(site, || {
            let roi = progressive::Query::initial_region(size, point).unwrap();
            let mut query = match progressive::Query::new(size, point, roi, read(roi)) {
                Ok(q) => q,
                Err(reason) => return reason == Completion::Failed,
            };
            loop {
                match query.step() {
                    progressive::Step::Read(regions) => {
                        if let Err(reason) =
                            query.apply(regions.into_iter().map(|r| (r, read(r))).collect())
                        {
                            return reason == Completion::Failed;
                        }
                    }
                    progressive::Step::Unavailable(reason) => return reason == Completion::Failed,
                    progressive::Step::Complete(_) => return false,
                }
            }
        });
        assert!(hit, "unexercised allocation site: {site}");
        assert!(
            failed,
            "partial/complete result escaped failed allocation: {site}"
        );
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/visual-candidate-v4.1")
}

fn manifest() -> Value {
    let mut main: Value =
        serde_json::from_slice(&std::fs::read(fixture_root().join("manifest.json")).unwrap())
            .unwrap();
    let extra: Value = serde_json::from_slice(
        &std::fs::read(
            fixture_root()
                .with_file_name("visual-candidate-8k-v4")
                .join("manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    main["cases"]
        .as_array_mut()
        .unwrap()
        .extend(extra["cases"].as_array().unwrap().iter().cloned());
    main
}

fn decode(name: &str) -> ([usize; 2], Vec<u8>) {
    assert!(!name.contains(['/', '\\']));
    let root = if name == "T10-8k.png" {
        fixture_root().with_file_name("visual-candidate-8k-v4")
    } else {
        fixture_root()
    };
    let bytes = std::fs::read(root.join(name)).unwrap();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // Browser PNGs can be RGB when opaque; expand that losslessly to RGBA.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut rgba).unwrap();
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    assert_eq!(frame.bit_depth, png::BitDepth::Eight);
    rgba.truncate(frame.buffer_size());
    ([frame.width as usize, frame.height as usize], rgba)
}

fn rectangle(value: &Value) -> Rect {
    let v: Vec<i32> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as i32)
        .collect();
    Rect::new(v[0], v[1], v[0] + v[2], v[1] + v[3])
}

fn case_nodes(manifest: &Value) -> Vec<&Value> {
    let mut result = Vec::new();
    for case in manifest["cases"].as_array().unwrap() {
        result.push(case);
        if let Some(members) = case["members"].as_array() {
            result.extend(members);
        }
    }
    result
}

#[test]
fn visual_detection_signed_rgb_residual_cancels_affine_ramps() {
    for slope in [-7.0, -0.25, 0.0, 0.18, 9.0] {
        let samples = [-3.0, -1.0, 1.0, 3.0].map(|t| [40.0 + slope * t, 50.0 - slope * t, 80.0]);
        assert!(jump(samples) < 0.0001, "slope={slope}");
        let with_step = std::array::from_fn(|i| {
            std::array::from_fn(|k| {
                samples[i][k] + if i >= 2 { [80.0, -30.0, 0.0][k] } else { 0.0 }
            })
        });
        assert!((jump(with_step) - 80.0).abs() < 0.0001);
    }
    // Taking absolute values before subtraction would invalidate these cases.
    assert_eq!(jump([[0.0; 3], [10.0; 3], [20.0; 3], [30.0; 3]]), 0.0);
}

#[test]
fn visual_detection_distinct_sampling_and_input_limits() {
    for length in [0, 1, 7, 8, 63, 64, 65, 4097] {
        let positions = sample_positions(13, 13 + length);
        assert!(positions.len() <= 64);
        assert!(positions.windows(2).all(|p| p[0] < p[1]));
        assert!(positions.iter().all(|&p| p >= 13 && p < 13 + length));
        if length <= 8 {
            assert!(positions.len() < 8);
        }
    }
    assert!(Image::new([usize::MAX, usize::MAX], &[]).is_err());
    assert!(Image::new([16385, 16385], &[]).is_err());
    assert!(Image::new([4, 2], &[0; 31]).is_err());
    assert!(Image::new([0, 0], &[]).is_ok());
    let image = Image::new([1, 1], &[0, 0, 0, 0]).unwrap();
    assert!(image.pixel(Point::new(0, 0)).is_none());
    assert!(image.with_coverage(Rect::new(-1, 0, 1, 1)).is_err());
}

#[test]
fn visual_detection_boundary_location_exact_coverage_vectors() {
    let manifest = manifest();
    let mut count = 0;
    let mut errors = [0i32; 4];
    let mut rows = Vec::new();
    for case in case_nodes(&manifest) {
        let v = &case["stage_vectors"];
        if v["entry"] != "boundary_location" {
            continue;
        }
        let (size, rgba) = decode(v["image"].as_str().unwrap());
        let output = locate(
            Image::new(size, &rgba).unwrap(),
            rectangle(&v["proposal"]),
            Parameters::default(),
        );
        let expected = rectangle(&v["expected_rect"]);
        let actual = output.rect;
        let signed = [
            actual.left - expected.left,
            actual.top - expected.top,
            actual.right - expected.right,
            actual.bottom - expected.bottom,
        ];
        for i in 0..4 {
            errors[i] += signed[i];
        }
        rows.push(json!({"id":case["id"],"actual":output,"signed_error":signed}));
        count += 1;
    }
    println!(
        "VISUAL_LOCATION {}",
        json!({"rows":rows,"signed_error_sum":errors,"count":count})
    );
    assert_eq!(count, 7);
    assert!(
        rows.iter()
            .all(|r| r["actual"]["qualified"] == true && r["signed_error"] == json!([0, 0, 0, 0])),
        "location vectors failed"
    );
    assert_eq!(errors, [0; 4]);
}

#[test]
fn visual_detection_bordered_fills_take_outer_rim() {
    let manifest = manifest();
    for case in manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| ["T07", "T09", "T21"].contains(&c["id"].as_str().unwrap()))
    {
        let (size, rgba) = decode(case["input"]["image"].as_str().unwrap());
        let image = Image::new(size, &rgba).unwrap();
        for a in case["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["role"] == "target")
        {
            let r = rectangle(&a["rect"]);
            let inner = Rect::new(r.left + 2, r.top + 2, r.right - 2, r.bottom - 2);
            let c = canonicalise(
                image,
                validate(image, inner, Parameters::default()),
                Parameters::default(),
            );
            println!(
                "RIM {} {:?} q={} side={:?}",
                a["id"],
                c.rect,
                c.qualified,
                c.sides
                    .iter()
                    .map(|e| (e.supported, e.stroke, e.inner_stroke, e.outer_offsets))
                    .collect::<Vec<_>>()
            );
            assert!(c.qualified);
            assert_eq!(
                c.rect, r,
                "bordered fill must retain its outer rim: {}",
                a["id"]
            );
        }
    }
}

#[test]
fn visual_detection_rectangle_validation_fixed_stage_vectors() {
    let manifest = manifest();
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    let mut hard = 0;
    for case in case_nodes(&manifest) {
        let vector = &case["stage_vectors"];
        if vector["entry"] != "rectangle_validation" {
            continue;
        }
        let (size, rgba) = decode(vector["image"].as_str().unwrap());
        let image = Image::new(size, &rgba).unwrap();
        for proposal in vector["proposals"].as_array().unwrap() {
            let evidence = validate(image, rectangle(proposal), Parameters::default());
            if let Some(expected) = vector["expected_qualified"].as_bool() {
                hard += 1;
                if evidence.qualified != expected {
                    failures.push(case["id"].as_str().unwrap().to_owned());
                }
            }
            rows.push(json!({"id":case["id"],"expected":vector["expected_qualified"],"evidence":evidence}));
        }
    }
    assert!(hard > 0);
    println!(
        "VISUAL_STAGE {}",
        json!({"scope":"rectangle_validation_only","hard_vectors":hard,"rows":rows,"failures":failures})
    );
    assert!(failures.is_empty(), "stage failures: {failures:?}");
}

fn matches(rect: Rect, truth: Rect, tolerance: i64) -> bool {
    [rect.left, rect.top, rect.right, rect.bottom]
        .into_iter()
        .zip([truth.left, truth.top, truth.right, truth.bottom])
        .all(|(a, b)| (i64::from(a) - i64::from(b)).abs() <= tolerance)
}

fn one_to_one(expected: &[(String, Rect, i64)], actual: &[Candidate]) -> Vec<Option<usize>> {
    fn assign(
        i: usize,
        expected: &[(String, Rect, i64)],
        actual: &[Candidate],
        used: &mut [Option<usize>],
        seen: &mut [bool],
    ) -> bool {
        for (j, c) in actual.iter().enumerate() {
            if seen[j] || !matches(c.rect, expected[i].1, expected[i].2) {
                continue;
            }
            seen[j] = true;
            if used[j].is_none_or(|previous| assign(previous, expected, actual, used, seen)) {
                used[j] = Some(i);
                return true;
            }
        }
        false
    }
    let mut used = vec![None; actual.len()];
    for i in 0..expected.len() {
        assign(
            i,
            expected,
            actual,
            &mut used,
            &mut vec![false; actual.len()],
        );
    }
    let mut result = vec![None; expected.len()];
    for (j, i) in used.into_iter().enumerate() {
        if let Some(i) = i {
            result[i] = Some(j);
        }
    }
    result
}

#[test]
fn visual_detection_sampling_stage_contracts_and_single_output_matching() {
    let manifest = manifest();
    let mut count = 0;
    for case in case_nodes(&manifest) {
        let v = &case["stage_vectors"];
        if v["entry"] != "edge_sampling_qualification" {
            continue;
        }
        let length = v["edge_length"].as_i64().unwrap() as i32;
        let positions = sample_positions(0, length);
        assert_eq!(
            positions.len(),
            v["expected_distinct_positions"].as_u64().unwrap() as usize
        );
        assert_eq!(
            positions.len() >= 8,
            v["expected_eligible"].as_bool().unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 5);
    let a = Rect::new(20, 20, 40, 40);
    let b = Rect::new(21, 21, 41, 41);
    let candidate = Candidate {
        rect: a,
        sides: std::array::from_fn(|_| SideEvidence::default()),
        qualified: true,
        corner_extensions: 0,
    };
    let result = one_to_one(&[("a".into(), a, 1), ("b".into(), b, 1)], &[candidate]);
    assert_eq!(
        result.iter().filter(|r| r.is_some()).count(),
        1,
        "one output cannot satisfy two truths"
    );
}

#[test]
fn visual_detection_injected_geometry_chains_and_arrival_permutations() {
    let manifest = manifest();
    let group = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "Q07")
        .unwrap();
    for case in group["members"].as_array().unwrap() {
        let v = &case["stage_vectors"];
        let mut rectangles = v["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["qualified"] == true)
            .map(|c| rectangle(&c["rect"]))
            .collect::<Vec<_>>();
        rectangles.sort_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
        let selected = rectangles[0];
        let chain = contained_chain(&rectangles, selected);
        let mut reversed = rectangles.clone();
        reversed.reverse();
        assert_eq!(
            chain,
            contained_chain(&reversed, selected),
            "arrival order drift"
        );
        if let Some(expected) = v["expected_chain_rects"].as_array() {
            assert_eq!(chain, expected.iter().map(rectangle).collect::<Vec<_>>());
            assert_eq!(
                chain.len(),
                v["expected_stop_count"].as_u64().unwrap() as usize
            );
        } else {
            assert_eq!(v["expected_relation"], "crossing_alternatives");
            assert_eq!(chain.len(), 1, "crossing rectangles must not form ancestry");
        }
    }
}

#[test]
fn visual_detection_isolated_capacity_stage_vectors() {
    let manifest = manifest();
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "E10")
        .unwrap();
    let mut count = 0;
    for v in case["input"]["vectors"].as_array().unwrap() {
        let mut counters = if let Some(other) = v["other_counters"].as_object() {
            other
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_u64().unwrap() as usize))
                .collect::<Vec<_>>()
        } else {
            v["counters"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_u64().unwrap() as usize))
                .collect::<Vec<_>>()
        };
        if let Some(resource) = v["resource"].as_str() {
            counters.push((resource, v["value"].as_u64().unwrap() as usize));
        }
        let algorithm = v["algorithm"].as_str().unwrap();
        let violations = proposals::budget_violations(algorithm, &counters).unwrap();
        assert_eq!(
            if violations.is_empty() {
                "Complete"
            } else {
                "BudgetExceeded"
            },
            v["expected"].as_str().unwrap(),
            "{}",
            v["id"]
        );
        if let Some(expected) = v["expected_violations"].as_array() {
            assert_eq!(serde_json::to_value(&violations).unwrap(), json!(expected));
        }
        count += 1;
    }
    assert_eq!(count, 32);
    assert!(proposals::budget_violations("A", &[("unknown", 1)]).is_err());
    assert!(proposals::budget_violations("unknown", &[]).is_err());
}

#[test]
fn visual_detection_isolated_mask_uses_the_real_component_guard() {
    let m = manifest();
    let c = m["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "E09")
        .unwrap();
    let v = &c["stage_vectors"];
    let (size, rgba) = decode(v["image"].as_str().unwrap());
    let mask = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0] == 255)
        .collect::<Vec<_>>();
    for diagonal in [false, true] {
        let mut labels = 0;
        assert_eq!(
            proposals::walk_components(
                &mask,
                size[0],
                size[1],
                true,
                diagonal,
                usize::MAX,
                |_, _| {
                    labels += 1;
                    true
                }
            ),
            Completion::Complete
        );
        assert_eq!(labels, 76800);
        let mut labels = 0;
        assert_eq!(
            proposals::walk_components(&mask, size[0], size[1], true, diagonal, 65536, |_, _| {
                labels += 1;
                true
            }),
            Completion::BudgetExceeded
        );
        assert_eq!(labels, 65536, "stop before creating label 65537");
    }
}

#[test]
fn visual_detection_refinement_rejects_inversion_boundary_peaks_and_stale_evidence() {
    let size = [200, 160];
    let mut rgba = vec![255; size[0] * size[1] * 4];
    for y in 40..120 {
        for x in 56..156 {
            rgba[(y * 200 + x) * 4..(y * 200 + x) * 4 + 3].fill(80);
        }
    }
    let image = Image::new(size, &rgba).unwrap();
    assert!(!validate(image, Rect::new(61, 40, 59, 60), Parameters::default()).qualified);
    assert!(
        !locate(image, Rect::new(40, 40, 140, 120), Parameters::default()).qualified,
        "a peak at the search limit needs requery, not acceptance"
    );
    let truth = Rect::new(56, 40, 156, 120);
    assert!(validate(image, truth, Parameters::default()).qualified);
    for x in 56..156 {
        rgba[(40 * 200 + x) * 4..(40 * 200 + x) * 4 + 3].fill(255);
    }
    assert!(
        !validate(
            Image::new(size, &rgba).unwrap(),
            truth,
            Parameters::default()
        )
        .qualified,
        "old quality cannot survive a missing edge"
    );
}

#[test]
fn visual_detection_metamorphic_frozen_pairs() {
    let manifest = manifest();
    let base = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "T01")
        .unwrap();
    let mut count = 0;
    for case in manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| ["M01", "M02", "M03", "M04"].contains(&c["id"].as_str().unwrap()))
    {
        let (size, rgba) = decode(case["input"]["transformed_image"].as_str().unwrap());
        let image = Image::new(size, &rgba).unwrap();
        for q in case["queries"].as_array().unwrap() {
            let point = Point::new(
                q["point"][0].as_i64().unwrap() as i32,
                q["point"][1].as_i64().unwrap() as i32,
            );
            let output = proposals::seed(image, point);
            assert_eq!(output.completion, Completion::Complete);
            let ids = q["expected_annotation_ids"].as_array().unwrap();
            for id in ids {
                let expected = if let Some(geometry) = case["expected_geometry"].as_array() {
                    rectangle(
                        &geometry
                            .iter()
                            .find(|a| a["base_annotation"] == *id)
                            .unwrap()["rect"],
                    )
                } else {
                    rectangle(
                        &base["annotations"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|a| a["id"] == *id)
                            .unwrap()["rect"],
                    )
                };
                assert_eq!(
                    output.candidates.first().map(|c| c.rect),
                    Some(expected),
                    "{}",
                    case["id"]
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 12);
}

#[test]
fn visual_detection_read_coverage_and_extreme_dimensions() {
    let manifest = manifest();
    let group = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "Q05")
        .unwrap();
    let member = group["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "Q05-crop")
        .unwrap();
    let (size, rgba) = decode(member["input"]["image"].as_str().unwrap());
    let coverage = rectangle(&member["input"]["region_access"]["initial_roi"]);
    let image = Image::new(size, &rgba)
        .unwrap()
        .with_coverage(coverage)
        .unwrap();
    let truth = rectangle(&member["annotations"][0]["rect"]);
    assert!(!validate(image, truth, Parameters::default()).qualified);
    assert!(
        proposals::seed(image, Point::new(128, 96))
            .candidates
            .is_empty()
    );
    assert!(
        !validate(image, coverage, Parameters::default()).qualified,
        "ROI edge cannot supply missing pixels"
    );
    for size in [[0, 0], [1, 1], [2, 100], [100, 2], [1, 4097], [4097, 1]] {
        let bytes = vec![255; size[0] * size[1] * 4];
        let image = Image::new(size, &bytes).unwrap();
        assert!(
            proposals::seed(image, Point::new(0, 0))
                .candidates
                .is_empty()
        );
        assert!(
            proposals::components(image, Point::new(0, 0))
                .candidates
                .is_empty()
        );
    }
}

#[test]
fn visual_comparison_fixed_inputs_report() {
    let manifest = manifest();
    let mut rows = Vec::new();
    let mut queries = 0;
    for case in case_nodes(&manifest) {
        let Some(name) = case["input"]["image"].as_str() else {
            continue;
        };
        let Some(case_queries) = case["queries"].as_array() else {
            continue;
        };
        let (size, rgba) = decode(name);
        let image = Image::new(size, &rgba).unwrap();
        let original = blake3::hash(&rgba);
        let image = if case["input"]["region_access"]["additional_source_access"] == false {
            image
                .with_coverage(rectangle(&case["input"]["region_access"]["initial_roi"]))
                .unwrap()
        } else {
            image
        };
        let global_started = Instant::now();
        let global_pixels = proposals::coarse(image);
        let global_prepare_us = global_started.elapsed().as_micros();
        let global = global_pixels
            .as_ref()
            .map(|(size, bytes)| Image::new(*size, bytes).unwrap());
        for query in case_queries {
            let point = Point::new(
                query["point"][0].as_i64().unwrap() as i32,
                query["point"][1].as_i64().unwrap() as i32,
            );
            for route in ["A", "B"] {
                let started = Instant::now();
                let detection = if route == "A" {
                    proposals::seed(image, point)
                } else {
                    proposals::components_with_global(image, global, point)
                };
                let elapsed_us = started.elapsed().as_micros();
                assert!(detection.validations <= if route == "A" { 64 } else { 320 });
                assert!(detection.candidates.len() <= 24);
                assert!(
                    detection
                        .candidates
                        .iter()
                        .all(|c| c.qualified && c.rect.contains(point))
                );
                let annotations = case["annotations"].as_array().unwrap();
                let lookup = |id: &Value| {
                    rectangle(&annotations.iter().find(|a| a["id"] == *id).unwrap()["rect"])
                };
                let tolerance = query["edge_tolerance_px"].as_i64().unwrap_or(1);
                let selected = detection.candidates.first().map(|c| c.rect);
                let default_ok = match query["expected_default"]["kind"].as_str().unwrap() {
                    "one_of_annotations" => Some(selected.is_some_and(|r| {
                        query["expected_default"]["ids"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|id| matches(r, lookup(id), tolerance))
                    })),
                    "no_visual_target_in_region" => Some(selected.is_none()),
                    "record_outcome" => None,
                    _ => None,
                };
                let mut expected = query["required_candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| (id.as_str().unwrap().to_owned(), lookup(id), tolerance))
                    .collect::<Vec<_>>();
                expected.sort_by(|a, b| a.0.cmp(&b.0));
                let matching = one_to_one(&expected, &detection.candidates);
                let required=expected.iter().zip(&matching).map(|((id,_,_),found)|json!({"id":id,"found":found.is_some(),"output_index":found})).collect::<Vec<_>>();
                let forbidden_ok = query["forbidden_exact_candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|id| {
                        detection
                            .candidates
                            .iter()
                            .all(|c| !matches(c.rect, lookup(id), tolerance))
                    });
                let chain_ok = query["expected_chain"].as_array().map(|ids| {
                    let chain = ids
                        .iter()
                        .map(|id| {
                            detection
                                .candidates
                                .iter()
                                .find(|c| matches(c.rect, lookup(id), tolerance))
                                .map(|c| c.rect)
                        })
                        .collect::<Option<Vec<_>>>();
                    chain.is_some_and(|r| {
                        r.windows(2)
                            .all(|p| p[0] != p[1] && p[0].contains_rect(p[1]))
                    })
                });
                let evaluation = case["comparison_member"]["evaluation"]
                    .as_str()
                    .unwrap_or("hard_assertion");
                let applicable = query["mode"] == "assert"
                    && !["calibration", "stage_assertion"].contains(&evaluation);
                let query_ok = default_ok.unwrap_or(false)
                    && required.iter().all(|r| r["found"] == true)
                    && forbidden_ok
                    && chain_ok.unwrap_or(true);
                rows.push(json!({"case":case["id"],"query":query["id"],"mode":query["mode"],"evaluation":case["comparison_member"]["evaluation"],"route":route,"elapsed_us":elapsed_us,"global_prepare_once_us":if route=="B" {global_prepare_us} else {0},"default_ok":default_ok,"applicable":applicable,"query_ok":query_ok,"forbidden_ok":forbidden_ok,"chain_ok":chain_ok,"required":required,"detection":detection}));
            }
            queries += 1;
        }
        assert_eq!(original, blake3::hash(&rgba));
    }
    assert!(queries > 0);
    let failed = rows
        .iter()
        .filter(|r| {
            r["route"] == "A"
                && r["applicable"] == true
                && (r["query_ok"] != true || r["detection"]["completion"] != "Complete")
        })
        .map(|r| format!("{}/{}", r["case"], r["query"]))
        .collect::<Vec<_>>();
    println!(
        "VISUAL_COMPARISON {}",
        json!({"scope":"proposal_experiments_on_synthetic_inputs_only; observations do not certify precision or stage completion","parameters":Parameters::default(),"queries":queries,"rows":rows,"not_executed":["fusion","events","backend lifecycle","non-rectangle stage vectors","independent real qualification"]})
    );
    assert!(
        failed.is_empty(),
        "seed route hard contracts failed: {failed:?}"
    );
}

#[test]
#[ignore = "P2 release-profile development timing; opt-in 100 repetitions per representative scene"]
fn visual_detection_cpu_development_timings() {
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "timing qualification requires --release"
    );
    let manifest = manifest();
    let mut rows = Vec::new();
    for case in case_nodes(&manifest).into_iter().filter(|c| {
        [
            "T01", "T03", "T06", "T07", "T09", "T21", "T10", "T10-8k", "E09",
        ]
        .contains(&c["id"].as_str().unwrap())
    }) {
        let (size, rgba) = decode(case["input"]["image"].as_str().unwrap());
        let image = Image::new(size, &rgba).unwrap();
        let q = case["queries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["mode"] == "assert")
            .unwrap();
        let p = Point::new(
            q["point"][0].as_i64().unwrap() as i32,
            q["point"][1].as_i64().unwrap() as i32,
        );
        let mut samples = Vec::new();
        let mut first = 0;
        let mut reference = None;
        for i in 0..106 {
            let started = Instant::now();
            let result = proposals::seed(image, p);
            let elapsed = started.elapsed().as_micros();
            if i == 0 {
                first = elapsed;
            }
            if i >= 6 {
                samples.push(elapsed);
            }
            let geometry = result.candidates.iter().map(|c| c.rect).collect::<Vec<_>>();
            if let Some(expected) = &reference {
                assert_eq!(&geometry, expected, "repeat geometry drift");
            } else {
                reference = Some(geometry);
            }
            assert_eq!(
                result.completion,
                Completion::Complete,
                "timing must not hide overflow"
            );
        }
        assert_eq!(samples.len(), 100);
        rows.push(
            json!({"case":case["id"],"size":size,"first_use_us":first,"warm_samples_us":samples}),
        );
    }
    assert_eq!(rows.len(), 9);
    println!(
        "VISUAL_CPU_TIMING {}",
        json!({"scope":"pure_CPU_owned_input_development_only; no transfer/queue/UI/independent-statistical qualification","warmup":5,"first_use_separate":true,"rows":rows})
    );
}

#[test]
fn visual_detection_demand_transfer_matches_owned_input_and_enforces_bytes() {
    use std::cell::RefCell;
    let manifest = manifest();
    let mut rows = Vec::new();
    for case in case_nodes(&manifest) {
        let Some(name) = case["input"]["image"].as_str() else {
            continue;
        };
        let Some(queries) = case["queries"].as_array() else {
            continue;
        };
        if case["input"]["region_access"]["additional_source_access"] == false {
            continue;
        }
        let (size, rgba) = decode(name);
        let full = Image::new(size, &rgba).unwrap();
        for query in queries.iter().filter(|q| q["mode"] == "assert") {
            let point = Point::new(
                query["point"][0].as_i64().unwrap() as i32,
                query["point"][1].as_i64().unwrap() as i32,
            );
            let state = RefCell::new(
                reads::DemandPixels::new(size, point, |rect: Rect| {
                    let mut data = Vec::with_capacity(rect.area() as usize * 4);
                    for y in rect.top..rect.bottom {
                        let start = (y as usize * size[0] + rect.left as usize) * 4;
                        data.extend_from_slice(&rgba[start..start + rect.width() as usize * 4]);
                    }
                    Ok(data)
                })
                .unwrap(),
            );
            let read = |p| state.borrow_mut().pixel(p);
            let result = proposals::seed(Image::from_reader(size, &read).unwrap(), point);
            let expected = proposals::seed(full, point);
            let state = state.borrow();
            let geometry =
                |d: &proposals::Detection| d.candidates.iter().map(|c| c.rect).collect::<Vec<_>>();
            let same = geometry(&result) == geometry(&expected)
                && result.completion == expected.completion;
            rows.push(json!({"case":case["id"],"query":query["id"],"same":same,"initial_bytes":state.initial_bytes(),"extra_bytes":state.extra_bytes,"calls":state.calls,"failure":state.failure}));
        }
    }
    println!(
        "VISUAL_DEMAND {}",
        json!({"scope":"synchronous demand-read reference; small calls deliberately included, no product batching claim","rows":rows})
    );
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|r| r["same"] == true && r["failure"].is_null()),
        "bounded demand transfer differs; inspect VISUAL_DEMAND"
    );
}

#[test]
fn visual_detection_demand_reads_fail_closed_before_overallocation() {
    use std::cell::Cell;
    let calls = Cell::new(0usize);
    let mut source = reads::DemandPixels::new([4096, 4096], Point::new(0, 0), |r: Rect| {
        calls.set(calls.get() + 1);
        Ok(vec![255; r.area() as usize * 4])
    })
    .unwrap();
    assert!(source.pixel(Point::new(-1, 0)).is_none());
    assert_eq!(calls.get(), 1);
    for y in (512..4096).step_by(4) {
        for x in (0..4096).step_by(4) {
            let _ = source.pixel(Point::new(x, y));
            if source.failure.is_some() {
                break;
            }
        }
        if source.failure.is_some() {
            break;
        }
    }
    assert_eq!(source.failure, Some(Completion::BudgetExceeded));
    assert_eq!(source.extra_bytes, 1_048_576);
    assert_eq!(source.calls, 1 + 1_048_576 / 64);
    assert_eq!(source.calls, calls.get());
    assert!(
        source.pixel(Point::new(0, 0)).is_none(),
        "a failed query cannot publish previously cached candidates"
    );
    assert!(reads::DemandPixels::new([20, 20], Point::new(0, 0), |_| Ok(vec![0])).is_err());
    let mut malformed = reads::DemandPixels::new([1024, 1024], Point::new(0, 0), |r: Rect| {
        Ok(if r.width() == 512 {
            vec![255; r.area() as usize * 4]
        } else {
            vec![]
        })
    })
    .unwrap();
    assert!(malformed.pixel(Point::new(800, 800)).is_none());
    assert_eq!(malformed.failure, Some(Completion::Failed));
}

#[test]
fn visual_detection_existing_pixel_contracts_reject_invalid_layout_and_unmapped_hdr() {
    use yue_media::PixelFormat;
    use yue_media::pixels::{AlphaMode, PixelBuffer, PixelDescriptor, PixelEncoding};
    assert!(PixelBuffer::from_bytes(4, 2, 15, PixelDescriptor::bgra8(), vec![0; 32]).is_err());
    assert!(PixelBuffer::from_bytes(4, 2, 16, PixelDescriptor::bgra8(), vec![0; 31]).is_err());
    assert!(
        PixelDescriptor::new(
            PixelFormat::Rgba16Float,
            PixelEncoding::Srgb,
            AlphaMode::Straight
        )
        .is_err()
    );
    let hdr = PixelDescriptor::new(
        PixelFormat::Rgba16Float,
        PixelEncoding::ScRgb,
        AlphaMode::Straight,
    )
    .unwrap();
    for first in [0x7e00u16, 0x7c00] {
        let bytes = [first, 0, 0, 0x3c00]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(PixelBuffer::from_tight(1, 1, hdr, bytes).is_err());
    }
    let bytes: Vec<_> = [0xb800u16, 0x3c00, 0, 0x3c00]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let negative = PixelBuffer::from_tight(1, 1, hdr, bytes.clone()).unwrap();
    assert!(matches!(
        Image::from_pixels(&negative),
        Err(Completion::UnsupportedSource)
    ));
    assert_eq!(negative.as_bytes(), bytes);
    let transparent = PixelDescriptor::new(
        PixelFormat::Bgra8Unorm,
        PixelEncoding::Srgb,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let pixels = PixelBuffer::from_tight(1, 1, transparent, vec![0, 0, 0, 0]).unwrap();
    assert!(
        Image::from_pixels(&pixels)
            .unwrap()
            .pixel(Point::new(0, 0))
            .is_none()
    );
    let pixels =
        PixelBuffer::from_tight(1, 1, PixelDescriptor::bgra8(), vec![64, 32, 16, 255]).unwrap();
    let before = pixels.as_bytes().to_vec();
    assert!(
        Image::from_pixels(&pixels)
            .unwrap()
            .pixel(Point::new(0, 0))
            .is_some()
    );
    assert_eq!(pixels.as_bytes(), before);
    // Padding is validated/removed by the existing shared contract once.
    let padded = PixelBuffer::from_bytes(
        2,
        2,
        12,
        PixelDescriptor::bgra8(),
        [vec![255; 8], vec![17; 4], vec![255; 8], vec![19; 4]].concat(),
    )
    .unwrap();
    assert_eq!(Image::from_pixels(&padded).unwrap().bytes, &[255; 16]);
}

#[test]
fn visual_detection_channel_permutations_need_no_bgra_conversion_copy() {
    for name in ["T01.png", "T03.png", "T06.png", "T21.png"] {
        let (size, rgba) = decode(name);
        let point = Point::new(size[0] as i32 / 2, size[1] as i32 / 2);
        let expected = proposals::seed(Image::new(size, &rgba).unwrap(), point);
        for order in [[2, 1, 0], [1, 2, 0], [2, 0, 1]] {
            let mut swapped = rgba.clone();
            for (input, out) in rgba
                .as_chunks::<4>()
                .0
                .iter()
                .zip(swapped.as_chunks_mut::<4>().0)
            {
                out[..3].copy_from_slice(&order.map(|k| input[k]));
            }
            let actual = proposals::seed(Image::new(size, &swapped).unwrap(), point);
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "channel permutation changed evidence: {name}"
            );
        }
    }
}

#[test]
fn visual_detection_odd_large_dimensions_do_not_mean_empty_or_upscale() {
    for size in [[1281usize, 721usize], [4097, 2305]] {
        let mut rgba = vec![255; size[0] * size[1] * 4];
        let rect = Rect::new(40, 40, 180, 140);
        for y in rect.top..rect.bottom {
            for x in rect.left..rect.right {
                let at = (y as usize * size[0] + x as usize) * 4;
                rgba[at..at + 4].copy_from_slice(&[24, 80, 142, 255]);
            }
        }
        let image = Image::new(size, &rgba).unwrap();
        let result = proposals::seed(image, Point::new(100, 80));
        assert_eq!(result.completion, Completion::Complete);
        assert_eq!(result.candidates.first().unwrap().rect, rect);
        if let Some((coarse_size, _)) = proposals::coarse(image) {
            assert!(coarse_size[0] <= size[0] && coarse_size[1] <= size[1]);
        } else {
            assert!(size[0] * size[1] <= 1_048_576);
        }
    }
}

#[test]
fn visual_progressive_batches_preserve_full_reference_and_report_all_costs() {
    let manifest = manifest();
    let mut rows = Vec::new();
    for case in case_nodes(&manifest) {
        let Some(name) = case["input"]["image"].as_str() else {
            continue;
        };
        let Some(queries) = case["queries"].as_array() else {
            continue;
        };
        if case["input"]["region_access"]["additional_source_access"] == false {
            continue;
        }
        let (size, rgba) = decode(name);
        let read = |rect: Rect| {
            let mut bytes = Vec::with_capacity(rect.area() as usize * 4);
            for y in rect.top..rect.bottom {
                let at = (y as usize * size[0] + rect.left as usize) * 4;
                bytes.extend_from_slice(&rgba[at..at + rect.width() as usize * 4]);
            }
            bytes
        };
        for q in queries.iter().filter(|q| q["mode"] == "assert") {
            let point = Point::new(
                q["point"][0].as_i64().unwrap() as i32,
                q["point"][1].as_i64().unwrap() as i32,
            );
            let roi = progressive::Query::initial_region(size, point).unwrap();
            let mut query = progressive::Query::new(size, point, roi, read(roi)).unwrap();
            let expected = proposals::seed(Image::new(size, &rgba).unwrap(), point);
            if let Some(local) = query.local() {
                assert_eq!(local.completion, Completion::Complete);
                assert!(
                    local.candidates.iter().all(|candidate| expected
                        .candidates
                        .iter()
                        .any(|full| full.rect == candidate.rect)),
                    "local query invented geometry: {} / {}",
                    case["id"],
                    q["id"]
                );
            }
            let started = Instant::now();
            let mut copies = 1;
            let mut batches = 1;
            let mut max_boxes = 0;
            let (same, status) = loop {
                match query.step() {
                    progressive::Step::Read(regions) => {
                        max_boxes = max_boxes.max(regions.len());
                        copies += regions.len();
                        batches += 1;
                        query
                            .apply(regions.into_iter().map(|r| (r, read(r))).collect())
                            .unwrap();
                    }
                    progressive::Step::Complete(actual) => {
                        break (
                            serde_json::to_value(actual).unwrap()
                                == serde_json::to_value(&expected).unwrap(),
                            Completion::Complete,
                        );
                    }
                    progressive::Step::Unavailable(reason) => break (false, reason),
                }
            };
            rows.push(json!({"case":case["id"],"query":q["id"],"same":same,"status":status,"passes":query.passes,"batches":batches,"copies":copies,"max_batch_boxes":max_boxes,"extra_bytes":query.extra_bytes,"stage_us":query.stage_us,"elapsed_us":started.elapsed().as_micros()}));
        }
    }
    println!(
        "VISUAL_PROGRESSIVE {}",
        json!({"scope":"owned-input progressive transfer experiment; all repeated CPU passes included","rows":rows})
    );
    assert_eq!(rows.len(), 137);
    assert!(
        rows.iter().all(|r| r["same"] == true),
        "progressive plan differs or exceeds budget; inspect report"
    );
}

#[test]
fn visual_progressive_incomplete_wrong_batches_and_large_indices_fail_closed() {
    for region in [
        Rect::new(-1, 0, 10, 10),
        Rect::new(0, 0, 1025, 1),
        Rect::new(1, 1, 10, 10),
        Rect::default(),
        Rect::new(0, 0, 1024, 1024),
    ] {
        assert!(
            progressive::Query::new([1024, 1024], Point::new(0, 0), region, Vec::new()).is_err(),
            "invalid initial read region accepted: {region:?}"
        );
    }
    assert!(matches!(
        progressive::Query::new(
            [16384, 16384],
            Point::new(0, 0),
            Rect::new(0, 0, 512, 512),
            vec![255; 512 * 512 * 4]
        ),
        Err(Completion::BudgetExceeded)
    ));
    assert!(
        progressive::Query::new(
            [1024, 1024],
            Point::new(0, 0),
            Rect::new(0, 0, 512, 512),
            vec![0; 1]
        )
        .is_err()
    );
    let mut query = progressive::Query::new(
        [1024, 1024],
        Point::new(0, 0),
        Rect::new(0, 0, 512, 512),
        vec![255; 512 * 512 * 4],
    )
    .unwrap();
    let progressive::Step::Read(regions) = query.step() else {
        panic!("missing source requires explicit evidence request");
    };
    assert!(!regions.is_empty());
    assert!(
        matches!(
            query.step(),
            progressive::Step::Unavailable(Completion::Failed)
        ),
        "cannot proceed while owned pixels are pending"
    );
    assert_eq!(query.apply(vec![]), Err(Completion::Failed));
    assert!(matches!(
        query.step(),
        progressive::Step::Unavailable(Completion::Failed)
    ));
}
