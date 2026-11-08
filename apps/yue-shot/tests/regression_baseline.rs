//! YueShot-owned capture regression inventory; no SnapClip test/source dependency.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

struct Frozen {
    behaviour: &'static str,
    file: &'static str,
    test: &'static str,
}
const BEHAVIOURS: &[&str] = &[
    "f5_full_flow",
    "region_capture",
    "window_capture",
    "browser_window",
    "hotkey",
    "cancel",
    "hit_test_budget",
    "capture_path_unchanged",
    "capture_state_coverage",
];
const FROZEN: &[Frozen] = &[
    Frozen {
        behaviour: "f5_full_flow",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "happy_path_walks_through_every_state",
    },
    Frozen {
        behaviour: "f5_full_flow",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "preparing_accepts_the_frame_and_arms_the_session",
    },
    Frozen {
        behaviour: "f5_full_flow",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "chrome_is_hidden_until_a_selection_exists",
    },
    Frozen {
        behaviour: "region_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "a_press_alone_never_creates_a_zero_size_selection",
    },
    Frozen {
        behaviour: "region_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "dragging_inside_moves_the_existing_selection",
    },
    Frozen {
        behaviour: "region_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "snap_to_adopts_a_usable_rectangle_and_relies_on_the_clip_for_edges",
    },
    Frozen {
        behaviour: "region_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/providers.rs",
        test: "read_region_only_transfers_the_selection",
    },
    Frozen {
        behaviour: "region_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/providers.rs",
        test: "read_region_clips_a_selection_hanging_off_the_right_and_bottom",
    },
    Frozen {
        behaviour: "region_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/providers.rs",
        test: "read_region_clips_a_negative_origin_to_the_frame",
    },
    Frozen {
        behaviour: "window_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/win/wgc.rs",
        test: "support_probe_never_panics",
    },
    Frozen {
        behaviour: "window_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/providers.rs",
        test: "wgc_is_tried_before_the_bitblt_fallback",
    },
    Frozen {
        behaviour: "window_capture",
        file: "apps/yue-shot/crates/yue-shot-capture/src/window_detection/hit_test.rs",
        test: "hit_test_picks_the_frontmost_overlapping_window",
    },
    Frozen {
        behaviour: "browser_window",
        file: "apps/yue-shot/crates/yue-shot-capture/src/window_detection/uia.rs",
        test: "a_text_run_inside_an_element_is_not_a_target",
    },
    Frozen {
        behaviour: "browser_window",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/overlay/tests.rs",
        test: "the_preview_label_names_the_element_it_snapped_to",
    },
    Frozen {
        behaviour: "hotkey",
        file: "apps/yue-shot/src/shortcuts.rs",
        test: "the_second_hotkey_is_registered_with_the_same_discipline",
    },
    Frozen {
        behaviour: "hotkey",
        file: "apps/yue-shot/src/shortcuts.rs",
        test: "escape_and_enter_are_not_global_hotkeys",
    },
    Frozen {
        behaviour: "hotkey",
        file: "apps/yue-shot/src/shortcuts.rs",
        test: "each_conflict_message_names_its_own_key",
    },
    Frozen {
        behaviour: "cancel",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "esc_from_every_active_state_returns_to_idle",
    },
    Frozen {
        behaviour: "cancel",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "window_destroy_and_device_removal_use_the_same_cleanup",
    },
    Frozen {
        behaviour: "hit_test_budget",
        file: "apps/yue-shot/crates/yue-shot-capture/src/window_detection/hit_test.rs",
        test: "hit_test_and_nearest_target_stay_inside_the_latency_budget",
    },
    Frozen {
        behaviour: "capture_path_unchanged",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/providers.rs",
        test: "wgc_is_tried_before_the_bitblt_fallback",
    },
    Frozen {
        behaviour: "capture_state_coverage",
        file: "apps/yue-shot/crates/yue-shot-model/src/capture.rs",
        test: "capture_state_reports_activity",
    },
    Frozen {
        behaviour: "capture_state_coverage",
        file: "apps/yue-shot/crates/yue-shot-model/src/capture.rs",
        test: "capture_state_names_are_the_frozen_contract_strings",
    },
    Frozen {
        behaviour: "capture_state_coverage",
        file: "crates/yue-media/src/lib.rs",
        test: "pixel_format_strings_and_sizes_are_stable",
    },
    Frozen {
        behaviour: "capture_state_coverage",
        file: "apps/yue-shot/crates/yue-shot-capture/src/session.rs",
        test: "every_capture_state_variant_is_reachable",
    },
];
const DESKTOP_ONLY: &[Frozen] = &[
    Frozen {
        behaviour: "desktop_probe",
        file: "apps/yue-shot/crates/yue-shot-capture/src/ring_contrast.rs",
        test: "ring_contrast_probe",
    },
    Frozen {
        behaviour: "desktop_probe",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/uia_provider/tests/probes.rs",
        test: "browser_element_probe",
    },
    Frozen {
        behaviour: "desktop_probe",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/uia_provider/tests/probes.rs",
        test: "dump_uia_names_under_the_cursor",
    },
    Frozen {
        behaviour: "desktop_probe",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/uia_provider/tests/probes.rs",
        test: "explorer_rule_probe",
    },
    Frozen {
        behaviour: "desktop_probe",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/win/d2d/tests.rs",
        test: "write_drawn_text_for_the_font_subset",
    },
    Frozen {
        behaviour: "desktop_probe",
        file: "apps/yue-shot/crates/yue-shot-capture/src/windows/win/d2d/tests.rs",
        test: "font_cost_probe",
    },
    Frozen {
        behaviour: "desktop_probe",
        file: "apps/yue-shot/src/capture_host.rs",
        test: "the_shell_can_host_the_capture_overlay",
    },
];
const GPU_ONLY: &[Frozen] = &[Frozen {
    behaviour: "browser_window",
    file: "apps/yue-shot/crates/yue-shot-capture/src/windows/win/d2d/tests.rs",
    test: "an_element_preview_keeps_its_own_pixels_and_the_window_fallback_does_not",
}];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the app crate lives two levels below the repository root")
        .to_path_buf()
}
fn source(row: &Frozen) -> String {
    fs::read_to_string(repo_root().join(row.file))
        .unwrap_or_else(|error| panic!("cannot read `{}`: {error}", row.file))
}
fn defines(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}
fn ignored(source: &str, name: &str) -> bool {
    let needle = format!("fn {name}(");
    let mut previous: Vec<&str> = Vec::new();
    for line in source.lines() {
        if line.contains(&needle) {
            return previous.iter().any(|line| line.contains("#[ignore"));
        }
        previous.push(line);
        if previous.len() > 4 {
            previous.remove(0);
        }
    }
    false
}

#[test]
fn the_yueshot_regression_inventory_covers_every_owned_capture_behaviour() {
    let declared: BTreeSet<_> = BEHAVIOURS.iter().copied().collect();
    let named: BTreeSet<_> = FROZEN.iter().map(|row| row.behaviour).collect();
    assert_eq!(
        declared, named,
        "YueShot behaviour inventory changed without updating its evidence"
    );
    for row in FROZEN {
        let source = source(row);
        assert!(
            defines(&source, row.test),
            "`{}` no longer defines `{}`",
            row.file,
            row.test
        );
        assert!(
            !ignored(&source, row.test),
            "`{}` must remain a runnable regression",
            row.test
        );
    }
    for row in DESKTOP_ONLY {
        let source = source(row);
        assert!(
            defines(&source, row.test),
            "desktop probe `{}` disappeared",
            row.test
        );
        assert!(
            ignored(&source, row.test),
            "desktop probe `{}` must remain ignored",
            row.test
        );
    }
    for row in GPU_ONLY {
        let source = source(row);
        assert!(
            defines(&source, row.test),
            "GPU probe `{}` disappeared",
            row.test
        );
        assert!(
            ignored(&source, row.test),
            "GPU probe `{}` must remain opt-in",
            row.test
        );
    }
}
