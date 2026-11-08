//! Seeded mixed transactions checked against a linear geometry oracle.
use super::*;
use std::collections::HashMap;
use yue_component::VirtualizerSizeUpdate;

#[derive(Clone)]
struct Record {
    key: usize,
    revision: u64,
    size: f32,
    measured: bool,
}
struct TraceSource {
    records: Vec<Record>,
    lookup: HashMap<ElementId, usize>,
}
impl TraceSource {
    fn new(records: &[Record]) -> Rc<Self> {
        Rc::new(Self {
            records: records.to_vec(),
            lookup: records
                .iter()
                .enumerate()
                .map(|(ix, r)| (r.key.into(), ix))
                .collect(),
        })
    }
}
impl VirtualizerSource for TraceSource {
    fn len(&self) -> usize {
        self.records.len()
    }
    fn key_at(&self, ix: usize) -> ElementId {
        self.records[ix].key.into()
    }
    fn revision_at(&self, ix: usize) -> u64 {
        self.records[ix].revision
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        self.lookup.get(key).copied()
    }
}
fn sizing(records: &[Record], measured: bool) -> VirtualizerSizing {
    if measured {
        let hints: Vec<_> = records.iter().map(|r| px(r.size)).collect();
        VirtualizerSizing::measured_with_estimates(px(20.), move |ix| hints[ix])
    } else {
        VirtualizerSizing::known(records.iter().map(|r| px(r.size)).collect::<Vec<_>>())
    }
}
fn random(seed: &mut u64) -> usize {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*seed >> 32) as usize
}

#[test]
fn mixed_traces_preserve_geometry_snapshots_and_scroll_targets() {
    for seed in 0..8 {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            for measured in [false, true] {
                let mut rng = seed + 1;
                let mut records: Vec<_> = (0..64)
                    .map(|key| Record {
                        key,
                        revision: 0,
                        size: 10. + (key % 43) as f32,
                        measured: !measured,
                    })
                    .collect();
                let mut next_key = 64;
                let mut state = VirtualizerState::new(
                    axis,
                    TraceSource::new(&records),
                    sizing(&records, measured),
                )
                .unwrap();
                let (mut gap, mut padding) = (0., 0.);
                for step in 0..200 {
                    let frozen = state.snapshot(&viewport(&state, step + 1, 0.)).unwrap();
                    let old = frozen.item(0).unwrap();
                    let ix = random(&mut rng) % records.len();
                    match random(&mut rng) % 8 {
                        0 => {
                            let start = records.len();
                            records.push(Record {
                                key: next_key,
                                revision: 0,
                                size: 5. + (random(&mut rng) % 70) as f32,
                                measured: !measured,
                            });
                            next_key += 1;
                            state
                                .replace_source(
                                    TraceSource::new(&records),
                                    sizing(&records, measured),
                                    VirtualizerChange::Splice {
                                        old: start..start,
                                        inserted: 1,
                                    },
                                )
                                .unwrap();
                        }
                        1 if records.len() > 4 => {
                            records.remove(ix);
                            state
                                .replace_source(
                                    TraceSource::new(&records),
                                    sizing(&records, measured),
                                    VirtualizerChange::Splice {
                                        old: ix..ix + 1,
                                        inserted: 0,
                                    },
                                )
                                .unwrap();
                        }
                        2 => {
                            records.rotate_left(ix);
                            state
                                .replace_source(
                                    TraceSource::new(&records),
                                    sizing(&records, measured),
                                    VirtualizerChange::Reorder,
                                )
                                .unwrap();
                        }
                        3 => {
                            records[ix].size = 5. + (random(&mut rng) % 75) as f32;
                            records[ix].measured = true;
                            state
                                .update_sizes(&[VirtualizerSizeUpdate::new(
                                    records[ix].key,
                                    px(records[ix].size),
                                )])
                                .unwrap();
                        }
                        4 => {
                            records[ix].revision += 1;
                            records[ix].size = 5. + (random(&mut rng) % 75) as f32;
                            records[ix].measured = !measured;
                            state
                                .replace_source(
                                    TraceSource::new(&records),
                                    sizing(&records, measured),
                                    VirtualizerChange::Reorder,
                                )
                                .unwrap();
                        }
                        5 => {
                            state.invalidate_items(ix..ix + 1).unwrap();
                            records[ix].measured = !measured;
                        }
                        6 => {
                            gap = (random(&mut rng) % 9) as f32;
                            padding = (random(&mut rng) % 8) as f32;
                            state
                                .set_layout(
                                    VirtualizerLayout::default()
                                        .gap(px(gap))
                                        .padding(px(padding), px(padding)),
                                )
                                .unwrap();
                        }
                        _ => {
                            state
                                .set_retained_keys(vec![records[ix].key.into()])
                                .unwrap();
                        }
                    }
                    assert_eq!(frozen.item(0).unwrap().key(), old.key());
                    assert_eq!(frozen.item(0).unwrap().start(), old.start());
                    assert_eq!(frozen.item(0).unwrap().extent(), old.extent());
                    let mut start = f64::from(padding);
                    let positions: Vec<_> = records
                        .iter()
                        .map(|r| {
                            let pos = start;
                            start += f64::from(r.size + gap);
                            pos
                        })
                        .collect();
                    let total = start - f64::from(gap) + f64::from(padding);
                    let offset = (random(&mut rng) % (total as usize + 50)) as f64;
                    let v = viewport(&state, step + 1, offset);
                    let snapshot = state.snapshot(&v).unwrap();
                    assert_eq!(
                        snapshot.total_extent(),
                        total,
                        "seed {seed}, step {step}, {axis:?}, measured={measured}"
                    );
                    let expected: Vec<_> = records
                        .iter()
                        .enumerate()
                        .filter(|(ix, r)| {
                            positions[*ix] < offset + 100.
                                && positions[*ix] + f64::from(r.size) > offset
                        })
                        .map(|(ix, _)| ix)
                        .collect();
                    assert_eq!(
                        snapshot.visible_range().collect::<Vec<_>>(),
                        expected,
                        "seed {seed}, step {step}"
                    );
                    for (ix, record) in records.iter().enumerate() {
                        let item = snapshot.item(ix).unwrap();
                        assert_eq!(item.key(), &ElementId::from(record.key));
                        assert_eq!(
                            item.start(),
                            positions[ix],
                            "seed {seed}, step {step}, item {ix}"
                        );
                        assert_eq!(item.extent(), px(record.size));
                        assert_eq!(
                            item.is_measured(),
                            record.measured,
                            "seed {seed}, step {step}, item {ix}, measured={measured}"
                        );
                    }
                    assert!(
                        snapshot.working_indices().len() <= snapshot.rendered_range().len() + 1
                    );
                    if step % 11 == 0 {
                        let ix = random(&mut rng) % records.len();
                        state
                            .scroll_to_key(
                                records[ix].key.into(),
                                VirtualizerScrollOptions::default().align(VirtualizerAlign::Start),
                            )
                            .unwrap();
                        let (_, request) = state.prepare(&v, Instant::now()).unwrap();
                        let request = request.unwrap();
                        assert_eq!(
                            request.offset(),
                            positions[ix].clamp(0., (total - 100.).max(0.))
                        );
                        state.confirm(&request, request.offset(), &v).unwrap();
                        state.cancel_scroll();
                    }
                }
            }
        }
    }
}
