use super::*;
use gpui_kit::size;
use yue_component::{VirtualMasonryState, VirtualizerSizeUpdate};

#[test]
fn unordered_feedback_batches_keep_noop_duplicates_and_late_failures_atomic() {
    for axis in [Axis::Vertical, Axis::Horizontal] {
        for mode in 0..3 {
            for count in [2, 31, 32, 33, 256, 4_096] {
                let sizing = if mode == 0 {
                    VirtualizerSizing::known(vec![px(20.); 5_000])
                } else {
                    VirtualizerSizing::measured(px(20.))
                };
                let mut state = VirtualizerState::new(axis, source(0, 5_000), sizing).unwrap();
                state.commit(&viewport(&state, 1, 0.)).unwrap();
                let mut keys: Vec<_> = (0..count).collect();
                for ix in (1..count).rev() {
                    keys.swap(ix, (ix * 177 + 31) % (ix + 1));
                }
                let updates: Vec<_> = keys
                    .iter()
                    .map(|&ix| VirtualizerSizeUpdate::new(ix, px(30.)))
                    .collect();
                if mode == 2 {
                    state.update_size_hints(&updates).unwrap();
                } else {
                    state.update_sizes(&updates).unwrap();
                }
                let frozen = state.snapshot(&viewport(&state, 1, 0.)).unwrap();
                let mut duplicate = updates.clone();
                duplicate.push(updates[0].clone());
                let apply = |s: &mut VirtualizerState, u: &[VirtualizerSizeUpdate]| {
                    if mode == 2 {
                        s.update_size_hints(u)
                    } else {
                        s.update_sizes(u)
                    }
                };
                assert_eq!(
                    apply(&mut state, &duplicate),
                    Err(VirtualizerError::InvalidChange)
                );
                state.validate_snapshot(&frozen).unwrap();
                let mut invalid = updates.clone();
                invalid[0] = VirtualizerSizeUpdate::new(keys[0], px(40.));
                invalid.push(VirtualizerSizeUpdate::new(4_999usize, px(f32::NAN)));
                assert!(apply(&mut state, &invalid).is_err());
                state.validate_snapshot(&frozen).unwrap();
                apply(&mut state, &updates).unwrap();
                state.validate_snapshot(&frozen).unwrap();
                for ix in [0, 127.min(count - 1), count - 1] {
                    assert_eq!(frozen.item(ix).unwrap().extent(), px(30.));
                    assert_eq!(frozen.item(ix).unwrap().is_measured(), mode != 2);
                }
            }
        }
    }
}

#[test]
fn masonry_shared_directory_boundaries_keep_all_frozen_versions_and_lane_slots() {
    for count in [8_191, 8_192, 8_193, 24_575, 24_576, 24_577] {
        let mut state =
            VirtualMasonryState::new(source(0, count), VirtualizerSizing::fixed(px(20.)), 3)
                .unwrap();
        let mut frozen = Vec::new();
        for pass in 0..8 {
            let old_count = count + pass * 33;
            frozen.push(state.snapshot(0., size(px(300.), px(100.))).unwrap());
            state
                .append_source(source(0, old_count + 33), VirtualizerSizing::fixed(px(20.)))
                .unwrap();
            let current = state.snapshot(0., size(px(300.), px(100.))).unwrap();
            for (generation, old) in frozen.iter().enumerate() {
                let n = count + generation * 33;
                assert!(old.item(n).is_err());
                for ix in [0, 255, 256, 8_190, n - 1] {
                    let old_item = old.item(ix).unwrap();
                    let new_item = current.item(ix).unwrap();
                    assert_eq!(old_item.lane(), ix % 3);
                    assert_eq!(old_item.lane(), new_item.lane());
                    assert_eq!(old_item.item().start(), new_item.item().start());
                    assert_eq!(old_item.item().extent(), px(20.));
                }
            }
            assert_eq!(
                current.item(old_count + 32).unwrap().lane(),
                (old_count + 32) % 3
            );
        }
    }
}
