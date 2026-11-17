//! One product query owns bounded pixels, asks for missing evidence in
//! batches, and never publishes a partial pass. No GPU handles or fixture truth.
use super::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

const TILE: usize = 4;
const TILE_BYTES: usize = TILE * TILE * 4;
#[cfg(test)]
pub(crate) static LIVE_QUERIES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub enum Step {
    Read(Vec<Rect>),
    Complete(proposals::Detection),
    Unavailable(Completion),
}

enum Stage {
    Peaks,
    Rectangles(proposals::SeedPeaks),
    Validation(Vec<Rect>, Completion),
}

pub struct Query {
    size: [usize; 2],
    point: Point,
    roi: Rect,
    initial: Vec<u8>,
    tiles: Vec<u16>,
    data: Vec<[u8; TILE_BYTES]>,
    missing: Vec<usize>,
    requested: HashSet<[i32; 4]>,
    pending: Vec<Rect>,
    pub extra_bytes: usize,
    pub passes: usize,
    pub stage_us: [u128; 4],
    pub peak_buffer_bound: usize,
    failure: Option<Completion>,
    stage: Stage,
}

impl Query {
    /// A complete, independently validated query over the already owned ROI.
    /// No read is requested outside it, and a clipped edge cannot qualify.
    /// Full-source ancestor discovery remains a separate continuation.
    pub fn local(&mut self) -> Option<proposals::Detection> {
        if self.roi == Rect::new(0, 0, self.size[0] as i32, self.size[1] as i32) {
            return None;
        }
        let size = self.size;
        let roi = self.roi;
        let point = self.point;
        let cell = RefCell::new(&mut *self);
        let pixel = |p| {
            let q = cell.borrow();
            if !roi.contains(p) {
                return None;
            }
            let at = ((p.y - roi.top) * roi.width() + p.x - roi.left) as usize * 4;
            Some(q.initial[at..at + 4].try_into().unwrap())
        };
        let no_read = |_: Rect| {};
        let no_pending = || false;
        let failed = |reason| cell.borrow_mut().failure = Some(reason);
        let image = Image::from_reader(size, &pixel)
            .unwrap()
            .with_coverage(roi)
            .unwrap()
            .with_prefetch(&no_read, &no_pending, &failed);
        let mut result = proposals::seed(image, point);
        // A local image edge is not a frozen-source edge. Require the entire
        // probe halo to be owned, rather than validating with truncated scales.
        let source_bounds = Rect::new(0, 0, size[0] as i32, size[1] as i32);
        result.candidates.retain(|c| {
            roi.contains_rect(
                Rect::new(
                    c.rect.left - PROBE_RADIUS,
                    c.rect.top - PROBE_RADIUS,
                    c.rect.right + PROBE_RADIUS + 1,
                    c.rect.bottom + PROBE_RADIUS + 1,
                )
                .intersect(source_bounds),
            )
        });
        cell.borrow_mut().record_buffers(0);
        if cell.borrow().failure.is_some() || result.completion != Completion::Complete {
            None
        } else {
            Some(result)
        }
    }

    /// Reuse owned pixels for a new seed, never a previous seed's candidate chain.
    pub fn restart(&mut self, point: Point) -> bool {
        if !self.roi.contains(point) || !self.pending.is_empty() || self.failure.is_some() {
            return false;
        }
        self.point = point;
        self.passes = 0;
        self.stage_us = [0; 4];
        self.stage = Stage::Peaks;
        true
    }

    pub fn coverage(&self) -> Rect {
        self.roi
    }

    pub fn initial_region(size: [usize; 2], point: Point) -> Result<Rect, Completion> {
        if size.contains(&0) || size.iter().any(|&v| v > 16384) {
            return Err(Completion::UnsupportedSource);
        }
        let bounds = Rect::new(0, 0, size[0] as i32, size[1] as i32);
        if !bounds.contains(point) {
            return Err(Completion::Failed);
        }
        let left = (point.x - 256).clamp(0, (bounds.right - 512).max(0));
        let top = (point.y - 256).clamp(0, (bounds.bottom - 512).max(0));
        Ok(Rect::new(
            left,
            top,
            (left + 512).min(bounds.right),
            (top + 512).min(bounds.bottom),
        ))
    }

    /// Actual initial read coverage accompanies its owned bytes; missing source
    /// pixels remain unknown. The graphics owner normally chooses initial_region,
    /// but the query must not silently infer coverage from a byte length or seed.
    pub fn new(
        size: [usize; 2],
        point: Point,
        roi: Rect,
        initial: Vec<u8>,
    ) -> Result<Self, Completion> {
        Self::initial_region(size, point)?;
        let bounds = Rect::new(0, 0, size[0] as i32, size[1] as i32);
        if roi.is_empty()
            || !bounds.contains_rect(roi)
            || !roi.contains(point)
            || roi.area() > 512 * 512
            || initial.len() != roi.area() as usize * 4
        {
            return Err(Completion::Failed);
        }
        let count = size[0].div_ceil(TILE) * size[1].div_ceil(TILE);
        // Bound the index before allocation; the remaining budget includes the
        // worker stack, owner, pixels, hash growth and concurrent transfer bytes.
        // Keep room for pixels, hash growth, transfer output, worker stack and owner metadata.
        if count * std::mem::size_of::<u16>() > 5 * 1_048_576 {
            return Err(Completion::BudgetExceeded);
        }
        let mut tiles = Vec::new();
        if !allocation::reserve(&mut tiles, count, "tile_index") {
            return Err(Completion::Failed);
        }
        tiles.resize(count, 0);
        #[cfg(test)]
        LIVE_QUERIES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Self {
            size,
            point,
            roi,
            initial,
            tiles,
            data: Vec::new(),
            missing: Vec::new(),
            requested: HashSet::new(),
            pending: Vec::new(),
            extra_bytes: 0,
            passes: 0,
            stage_us: [0; 4],
            peak_buffer_bound: 0,
            failure: None,
            stage: Stage::Peaks,
        })
    }

    #[cfg(test)]
    pub fn live_count() -> usize {
        LIVE_QUERIES.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Conservative capacity accounting for this prototype, not process RSS.
    /// Hash tables include control bytes, alignment and a second table during
    /// growth. The 2MiB temporary allowance covers <=1MiB tile reallocation,
    /// <=128KiB missing keys, <=256KiB rectangle runs and <=512KiB validation.
    fn record_buffers(&mut self, external: usize) {
        let hash = if self.requested.capacity() == 0 {
            0
        } else {
            2 * ((self.requested.capacity() + 1).next_power_of_two() * 17 + 32)
        };
        let stage = match &self.stage {
            Stage::Peaks => 0,
            Stage::Rectangles(peaks) => peaks
                .iter()
                .map(|p| p.0.capacity() * size_of::<i32>())
                .sum(),
            Stage::Validation(rects, _) => rects.capacity() * size_of::<Rect>(),
        };
        let bytes = size_of::<Self>()
            + self.initial.capacity()
            + self.tiles.capacity() * size_of::<u16>()
            + self.data.capacity() * TILE_BYTES
            + self.missing.capacity() * size_of::<usize>()
            + self.pending.capacity() * size_of::<Rect>()
            + hash
            + stage
            + external
            + 2 * 1_048_576;
        self.peak_buffer_bound = self.peak_buffer_bound.max(bytes);
        if bytes + 4 * 1_048_576 > 16 * 1_048_576 {
            self.failure = Some(Completion::BudgetExceeded);
        }
    }

    fn columns(&self) -> usize {
        self.size[0].div_ceil(TILE)
    }
    fn tile_rect(&self, key: usize) -> Rect {
        let x = (key % self.columns()) as i32 * TILE as i32;
        let y = (key / self.columns()) as i32 * TILE as i32;
        Rect::new(
            x,
            y,
            (x + TILE as i32).min(self.size[0] as i32),
            (y + TILE as i32).min(self.size[1] as i32),
        )
    }

    fn request(&mut self, rect: Rect) {
        if self.failure.is_some() {
            return;
        }
        let rect = rect.intersect(Rect::new(0, 0, self.size[0] as i32, self.size[1] as i32));
        if rect.is_empty() {
            return;
        }
        if self.roi.contains_rect(rect) {
            return;
        }
        let region = [rect.left, rect.top, rect.right, rect.bottom];
        if self.requested.contains(&region) {
            return;
        }
        if self.requested.len() == 65536 {
            self.failure = Some(Completion::BudgetExceeded);
            return;
        }
        if self.requested.len() == self.requested.capacity() {
            // Account the next doubled table before its allocation; current accounting
            // already includes the old table and all bounded temporary vector growth.
            let projected = (self.requested.capacity() + 1).next_power_of_two() * 34 + 64;
            self.record_buffers(projected);
            if self.failure.is_some() {
                return;
            }
        }
        if !allocation::permit("requested_regions") || self.requested.try_reserve(1).is_err() {
            self.failure = Some(Completion::Failed);
            return;
        }
        self.requested.insert(region);
        for y in rect.top / TILE as i32..=(rect.bottom - 1) / TILE as i32 {
            for x in rect.left / TILE as i32..=(rect.right - 1) / TILE as i32 {
                let key = y as usize * self.columns() + x as usize;
                if !self.roi.contains_rect(self.tile_rect(key)) && self.tiles[key] == 0 {
                    if !allocation::permit("missing_tiles") || self.missing.try_reserve(1).is_err()
                    {
                        self.failure = Some(Completion::Failed);
                        return;
                    }
                    self.missing.push(key);
                    self.tiles[key] = u16::MAX;
                    // Coarse upper bound before growing the request set. Real
                    // clipped transfer bytes are checked again before issuing.
                    if (self.data.len() + self.missing.len()) * TILE_BYTES
                        > proposals::A_LIMITS[3].1
                    {
                        self.failure = Some(Completion::BudgetExceeded);
                        return;
                    }
                }
            }
        }
    }

    fn pixel(&mut self, point: Point) -> Option<[u8; 4]> {
        if self.failure.is_some() {
            return None;
        }
        if self.roi.contains(point) {
            let at = ((point.y - self.roi.top) * self.roi.width() + point.x - self.roi.left)
                as usize
                * 4;
            return Some(self.initial[at..at + 4].try_into().unwrap());
        }
        let key = point.y as usize / TILE * self.columns() + point.x as usize / TILE;
        if self.tiles[key] != 0 && self.tiles[key] != u16::MAX {
            let tile = &self.data[self.tiles[key] as usize - 1];
            let at = ((point.y as usize % TILE) * TILE + point.x as usize % TILE) * 4;
            return Some(tile[at..at + 4].try_into().unwrap());
        }
        self.request(Rect::new(point.x, point.y, point.x + 1, point.y + 1));
        None
    }

    pub fn step(&mut self) -> Step {
        if let Some(failure) = self.failure {
            return Step::Unavailable(failure);
        }
        if !self.pending.is_empty() {
            return Step::Unavailable(Completion::Failed);
        }
        // Bounded replay is a reference scheduling choice; it must be measured,
        // including every repeated CPU pass, before any product promotion.
        if self.passes == 16 {
            return Step::Unavailable(Completion::BudgetExceeded);
        }
        self.passes += 1;
        loop {
            let size = self.size;
            let point = self.point;
            let stage = std::mem::replace(&mut self.stage, Stage::Peaks);
            let stage_index = match &stage {
                Stage::Peaks => 0,
                Stage::Rectangles(_) => 1,
                Stage::Validation(_, _) => 2,
            };
            let stage_start = std::time::Instant::now();
            let cell = RefCell::new(&mut *self);
            let pixel = |p| cell.borrow_mut().pixel(p);
            let request = |r| cell.borrow_mut().request(r);
            let pending = || !cell.borrow().missing.is_empty();
            let failure = |reason| cell.borrow_mut().failure = Some(reason);
            let image = Image::from_reader(size, &pixel)
                .unwrap()
                .with_prefetch(&request, &pending, &failure);
            let next = match stage {
                Stage::Peaks => {
                    let peaks = proposals::seed_peaks(image, point);
                    if pending() {
                        Stage::Peaks
                    } else {
                        Stage::Rectangles(peaks)
                    }
                }
                Stage::Rectangles(peaks) => {
                    let rectangles = proposals::seed_rectangles(image, point, &peaks);
                    if pending() {
                        Stage::Rectangles(peaks)
                    } else {
                        Stage::Validation(
                            rectangles,
                            if peaks.iter().any(|p| p.1) {
                                Completion::BudgetExceeded
                            } else {
                                Completion::Complete
                            },
                        )
                    }
                }
                Stage::Validation(rectangles, completion) => {
                    for &rect in &rectangles {
                        request_validation(image, rect);
                    }
                    let output = if pending() {
                        None
                    } else {
                        // No clone of a proposal vector at the allocation/failure boundary.
                        Some(proposals::finish(
                            image,
                            rectangles.iter().copied(),
                            point,
                            64,
                            completion,
                        ))
                    };
                    if let Some(failure) = cell.borrow().failure {
                        return Step::Unavailable(failure);
                    }
                    if !pending()
                        && let Some(output) = output
                    {
                        cell.borrow_mut().stage_us[stage_index] +=
                            stage_start.elapsed().as_micros();
                        cell.borrow_mut().record_buffers(0);
                        if let Some(failure) = cell.borrow().failure {
                            return Step::Unavailable(failure);
                        }
                        return Step::Complete(output);
                    }
                    Stage::Validation(rectangles, completion)
                }
            };
            self.stage = next;
            self.stage_us[stage_index] += stage_start.elapsed().as_micros();
            if let Some(failure) = self.failure {
                return Step::Unavailable(failure);
            }
            if !self.missing.is_empty() {
                break;
            }
        }
        // Consecutive tiles form horizontal runs; equal adjacent runs merge
        // vertically. No bounding-box fill over unrelated holes is permitted.
        let mut runs: Vec<Rect> = Vec::new();
        if !allocation::reserve(&mut runs, self.missing.len(), "horizontal_runs") {
            self.failure = Some(Completion::Failed);
            return Step::Unavailable(Completion::Failed);
        }
        self.missing.sort_unstable();
        for &key in &self.missing {
            let rect = self.tile_rect(key);
            if let Some(last) = runs.last_mut()
                && last.top == rect.top
                && last.bottom == rect.bottom
                && last.right == rect.left
            {
                last.right = rect.right;
            } else {
                runs.push(rect);
            }
        }
        let mut merged: Vec<Rect> = Vec::new();
        if !allocation::reserve(&mut merged, runs.len(), "merged_runs") {
            self.failure = Some(Completion::Failed);
            return Step::Unavailable(Completion::Failed);
        }
        let runs_bytes = runs.capacity() * size_of::<Rect>();
        let mut previous: HashMap<(i32, i32), usize> = HashMap::new();
        for rect in runs {
            let k = (rect.left, rect.right);
            if let Some(&index) = previous.get(&k)
                && merged[index].bottom == rect.top
            {
                merged[index].bottom = rect.bottom;
            } else {
                if !allocation::permit("run_index") || previous.try_reserve(1).is_err() {
                    self.failure = Some(Completion::Failed);
                    return Step::Unavailable(Completion::Failed);
                }
                previous.insert(k, merged.len());
                merged.push(rect);
            }
        }
        let bytes = merged.iter().map(|r| r.area() as usize * 4).sum::<usize>();
        if self.extra_bytes + bytes > proposals::A_LIMITS[3].1 {
            self.failure = Some(Completion::BudgetExceeded);
            return Step::Unavailable(Completion::BudgetExceeded);
        }
        self.extra_bytes += bytes;
        self.pending.clear();
        if !allocation::reserve(&mut self.pending, merged.len(), "pending_regions") {
            self.failure = Some(Completion::Failed);
            return Step::Unavailable(Completion::Failed);
        }
        self.pending.extend_from_slice(&merged);
        let hash_bytes = if previous.capacity() == 0 {
            0
        } else {
            2 * ((previous.capacity() + 1).next_power_of_two() * 17 + 32)
        };
        self.record_buffers(runs_bytes + merged.capacity() * size_of::<Rect>() + hash_bytes);
        Step::Read(merged)
    }

    pub fn apply(&mut self, regions: Vec<(Rect, Vec<u8>)>) -> Result<(), Completion> {
        let started = std::time::Instant::now();
        let valid = regions.len() == self.pending.len()
            && regions
                .iter()
                .zip(&self.pending)
                .all(|((rect, data), expected)| {
                    rect == expected && data.len() == rect.area() as usize * 4
                });
        if !valid {
            self.failure = Some(Completion::Failed);
            return Err(Completion::Failed);
        }
        let external = regions.capacity() * size_of::<(Rect, Vec<u8>)>()
            + regions
                .iter()
                .map(|(_, bytes)| bytes.capacity())
                .sum::<usize>();
        if !allocation::permit("tile_pixels") || self.data.try_reserve(self.missing.len()).is_err()
        {
            self.failure = Some(Completion::Failed);
            return Err(Completion::Failed);
        }
        self.record_buffers(external);
        for (rect, data) in regions {
            for y in (rect.top..rect.bottom).step_by(TILE) {
                for x in (rect.left..rect.right).step_by(TILE) {
                    let key = y as usize / TILE * self.columns() + x as usize / TILE;
                    let tile_rect = self.tile_rect(key);
                    let mut tile = [0; TILE_BYTES];
                    for row in 0..tile_rect.height() as usize {
                        let start = ((y - rect.top) as usize + row) * rect.width() as usize * 4
                            + (x - rect.left) as usize * 4;
                        tile[row * TILE * 4..row * TILE * 4 + tile_rect.width() as usize * 4]
                            .copy_from_slice(&data[start..start + tile_rect.width() as usize * 4]);
                    }
                    if self.tiles[key] == 0 || self.tiles[key] == u16::MAX {
                        self.data.push(tile);
                        self.tiles[key] = self.data.len() as u16;
                    } else {
                        self.data[self.tiles[key] as usize - 1] = tile;
                    }
                }
            }
        }
        self.missing.clear();
        self.pending.clear();
        self.stage_us[3] += started.elapsed().as_micros();
        Ok(())
    }
}

#[cfg(test)]
impl Drop for Query {
    fn drop(&mut self) {
        LIVE_QUERIES.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
