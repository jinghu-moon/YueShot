//! Private consumer experiment, not a new intrinsic-layout/Base API.
//! Both paths have the same epoch, key, budget, validation and output contract.
use super::*;
use std::collections::{HashMap, HashSet};
use yue_component::{VirtualGridCellKey, VirtualizerSizeUpdate};

#[derive(Clone)]
pub(super) struct Observation {
    key: VirtualGridCellKey,
    width: f32,
    height: f32,
}
fn observation(row: usize, column: usize, width: f32, height: f32) -> Observation {
    Observation {
        key: VirtualGridCellKey::new(row, column),
        width,
        height,
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Path {
    Cells,
    Axes,
}

/// Application-owned finite track domain. O(R+C+budget) storage is explicit.
pub(super) struct Consumer {
    epoch: u64,
    budget: usize,
    rows: HashMap<ElementId, usize>,
    columns: HashMap<ElementId, usize>,
    row_sizes: Vec<f32>,
    column_sizes: Vec<f32>,
    cells: HashMap<VirtualGridCellKey, (f32, f32)>,
    seen: HashSet<VirtualGridCellKey>,
    path: Path,
}
impl Consumer {
    pub(super) fn new(path: Path, side: usize, budget: usize) -> Self {
        Self {
            epoch: 1,
            budget,
            rows: (0..side).map(|i| (i.into(), i)).collect(),
            columns: (0..side).map(|i| (i.into(), i)).collect(),
            row_sizes: vec![20.; side],
            column_sizes: vec![40.; side],
            cells: HashMap::new(),
            seen: HashSet::new(),
            path,
        }
    }
    pub(super) fn reset(&mut self, epoch: u64) -> Result<(), &'static str> {
        if epoch <= self.epoch {
            return Err("stale epoch");
        }
        self.epoch = epoch;
        self.cells.clear();
        self.seen.clear();
        self.row_sizes.fill(20.);
        self.column_sizes.fill(40.);
        Ok(())
    }
    pub(super) fn observe(
        &mut self,
        epoch: u64,
        batch: &[Observation],
    ) -> Result<(), &'static str> {
        if epoch != self.epoch {
            return Err("stale epoch");
        }
        // Validate the entire batch before any mutation, including duplicate keys.
        let mut added = HashSet::new();
        for item in batch {
            if !item.width.is_finite()
                || item.width <= 0.
                || !item.height.is_finite()
                || item.height <= 0.
            {
                return Err("geometry");
            }
            if !self.rows.contains_key(item.key.row())
                || !self.columns.contains_key(item.key.column())
            {
                return Err("missing key");
            }
            let contains = match self.path {
                Path::Cells => self.cells.contains_key(&item.key),
                Path::Axes => self.seen.contains(&item.key),
            };
            if !contains {
                added.insert(&item.key);
            }
        }
        let count = match self.path {
            Path::Cells => self.cells.len(),
            Path::Axes => self.seen.len(),
        };
        if count + added.len() > self.budget {
            return Err("budget");
        }
        for item in batch {
            match self.path {
                Path::Cells => {
                    let current = self.cells.entry(item.key.clone()).or_insert((0., 0.));
                    current.0 = current.0.max(item.width);
                    current.1 = current.1.max(item.height);
                }
                Path::Axes => {
                    self.seen.insert(item.key.clone());
                    let row = self.rows[item.key.row()];
                    let column = self.columns[item.key.column()];
                    self.row_sizes[row] = self.row_sizes[row].max(item.height);
                    self.column_sizes[column] = self.column_sizes[column].max(item.width);
                }
            }
        }
        Ok(())
    }
    pub(super) fn output(&mut self) -> (Vec<f32>, Vec<f32>) {
        if matches!(self.path, Path::Cells) {
            self.row_sizes.fill(20.);
            self.column_sizes.fill(40.);
            for (key, (width, height)) in &self.cells {
                let row = self.rows[key.row()];
                let column = self.columns[key.column()];
                self.row_sizes[row] = self.row_sizes[row].max(*height);
                self.column_sizes[column] = self.column_sizes[column].max(*width);
            }
        }
        // Both paths materialize the same complete source-order vectors.
        (self.row_sizes.clone(), self.column_sizes.clone())
    }
}

pub(super) fn workload(side: usize) -> Vec<Vec<Observation>> {
    (0..16)
        .map(|frame| {
            (0..side * side)
                .filter(|i| frame == 0 || i % 8 == frame % 8)
                .map(|i| {
                    observation(
                        i / side,
                        i % side,
                        42. + ((i + frame) % 19) as f32,
                        22. + ((i * 3 + frame) % 13) as f32,
                    )
                })
                .collect()
        })
        .collect()
}

#[test]
fn observed_consumer_paths_match_independent_full_output_oracle() {
    // Inspired by TanStack lane equivalence: fixed history, independent oracle,
    // full values, repeated feedback and epoch reset (including shrink).
    for side in [1, 7, 64] {
        let mut consumers =
            [Path::Cells, Path::Axes].map(|path| Consumer::new(path, side, side * side));
        for epoch in 1..=3 {
            if epoch > 1 {
                for consumer in &mut consumers {
                    consumer.reset(epoch).unwrap();
                }
            }
            let mut history = Vec::new();
            for mut batch in workload(side) {
                if epoch == 2 {
                    for item in &mut batch {
                        item.width = 41.;
                        item.height = 21.;
                    }
                }
                history.extend(batch.iter().cloned());
                let mut expected = (vec![20f32; side], vec![40f32; side]);
                for item in &history {
                    // Independent exhaustive scan, deliberately no key->index map.
                    for row in 0..side {
                        if item.key.row() == &ElementId::from(row) {
                            expected.0[row] = expected.0[row].max(item.height);
                        }
                    }
                    for column in 0..side {
                        if item.key.column() == &ElementId::from(column) {
                            expected.1[column] = expected.1[column].max(item.width);
                        }
                    }
                }
                for consumer in &mut consumers {
                    consumer.observe(epoch, &batch).unwrap();
                    assert_eq!(consumer.output(), expected);
                }
            }
        }
    }
}

#[test]
fn observed_consumer_rejects_batches_atomically_and_resets_shrinking_tracks() {
    for path in [Path::Cells, Path::Axes] {
        let mut c = Consumer::new(path, 3, 2);
        c.observe(
            1,
            &[observation(0, 0, 80., 60.), observation(0, 0, 70., 50.)],
        )
        .unwrap();
        let before = c.output();
        for (epoch, batch, error) in [
            (0, vec![observation(1, 1, 90., 90.)], "stale epoch"),
            (
                1,
                vec![observation(1, 1, 90., 90.), observation(2, 2, 90., 90.)],
                "budget",
            ),
            (
                1,
                vec![observation(1, 1, 90., 90.), observation(3, 0, 90., 90.)],
                "missing key",
            ),
            (
                1,
                vec![observation(1, 1, 90., 90.), observation(0, 0, f32::NAN, 1.)],
                "geometry",
            ),
        ] {
            assert_eq!(c.observe(epoch, &batch), Err(error));
            assert_eq!(c.output(), before);
        }
        assert_eq!(c.reset(1), Err("stale epoch"));
        c.reset(2).unwrap();
        c.observe(2, &[observation(0, 0, 45., 25.)]).unwrap();
        assert_eq!(c.output(), (vec![25., 20., 20.], vec![45., 40., 40.]));
        assert_eq!(
            c.observe(1, &[observation(0, 0, 100., 100.)]),
            Err("stale epoch")
        );
        let mut zero = Consumer::new(path, 3, 0);
        assert_eq!(
            zero.observe(1, &[observation(0, 0, 50., 30.)]),
            Err("budget")
        );
    }
}

#[test]
fn observed_consumer_updates_known_axes_and_preserves_reading_anchor_on_growth_and_shrink() {
    for path in [Path::Cells, Path::Axes] {
        let mut consumer = Consumer::new(path, 64, 4096);
        let binding = VirtualizerBinding::new();
        let mut axes = [Axis::Vertical, Axis::Horizontal].map(|axis| {
            VirtualizerState::new_attached(
                axis,
                source(0, 64),
                VirtualizerSizing::known(vec![
                    px(if axis == Axis::Vertical { 20. } else { 40. });
                    64
                ]),
                &binding,
            )
            .unwrap()
        });
        for epoch in 1..=3 {
            let offsets = [205., 405.];
            let old = axes
                .iter_mut()
                .zip(offsets)
                .map(|(s, offset)| {
                    let v = VirtualizerViewport::new(&binding, epoch, offset, px(100.), px(240.));
                    s.commit(&v).unwrap();
                    s.snapshot(&v).unwrap()
                })
                .collect::<Vec<_>>();
            if epoch > 1 {
                consumer.reset(epoch).unwrap();
            }
            consumer
                .observe(
                    epoch,
                    &[observation(
                        0,
                        0,
                        if epoch == 2 { 44. } else { 70. },
                        if epoch == 2 { 24. } else { 50. },
                    )],
                )
                .unwrap();
            let (rows, columns) = consumer.output();
            for ((s, sizes), frozen) in axes.iter_mut().zip([rows, columns]).zip(old) {
                let anchor = frozen.item(frozen.visible_range().start).unwrap();
                let distance = anchor.start() - frozen.viewport().offset();
                let updates = sizes
                    .iter()
                    .enumerate()
                    .map(|(i, extent)| VirtualizerSizeUpdate::new(i, px(*extent)))
                    .collect::<Vec<_>>();
                // Each axis is atomic; this does NOT claim a joint 2-axis transaction.
                s.update_sizes_from_snapshot(&frozen, &updates).unwrap();
                let v = s.viewport().unwrap().clone();
                let (_, request) = s.prepare(&v, Instant::now()).unwrap();
                let request = request.unwrap();
                let next = s
                    .snapshot(&VirtualizerViewport::new(
                        &binding,
                        epoch + 10,
                        request.offset(),
                        px(100.),
                        px(240.),
                    ))
                    .unwrap();
                assert_eq!(
                    next.item_for_key(anchor.key()).unwrap().start() - request.offset(),
                    distance
                );
                assert_eq!(
                    s.update_sizes_from_snapshot(&frozen, &updates),
                    Err(VirtualizerError::StaleSnapshot)
                );
            }
        }
    }
}
