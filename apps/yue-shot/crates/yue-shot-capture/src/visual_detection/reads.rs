//! Demand-transfer reference, intentionally synchronous and test-only. Establishes
//! bytes/calls before choosing a batching strategy; it is not a product cache.
use super::*;
use std::collections::HashMap;

pub struct DemandPixels<F> {
    size: [usize; 2],
    roi: Rect,
    initial: Vec<u8>,
    blocks: HashMap<(i32, i32), Vec<u8>>,
    read: F,
    pub extra_bytes: usize,
    pub calls: usize,
    pub failure: Option<Completion>,
}

impl<F: FnMut(Rect) -> Result<Vec<u8>, Completion>> DemandPixels<F> {
    pub fn new(size: [usize; 2], point: Point, mut read: F) -> Result<Self, Completion> {
        if size.contains(&0) || size.iter().any(|&v| v > 16384) {
            return Err(Completion::UnsupportedSource);
        }
        let bounds = Rect::new(0, 0, size[0] as i32, size[1] as i32);
        if !bounds.contains(point) {
            return Err(Completion::Failed);
        }
        let left = (point.x - 256).clamp(0, (bounds.right - 512).max(0));
        let top = (point.y - 256).clamp(0, (bounds.bottom - 512).max(0));
        let roi = Rect::new(
            left,
            top,
            (left + 512).min(bounds.right),
            (top + 512).min(bounds.bottom),
        );
        let initial = read(roi)?;
        if initial.len() != roi.area() as usize * 4 {
            return Err(Completion::Failed);
        }
        Ok(Self {
            size,
            roi,
            initial,
            blocks: HashMap::new(),
            read,
            extra_bytes: 0,
            calls: 1,
            failure: None,
        })
    }

    pub fn initial_bytes(&self) -> usize {
        self.initial.len()
    }

    pub fn pixel(&mut self, point: Point) -> Option<[u8; 4]> {
        if self.failure.is_some() {
            return None;
        }
        if point.x < 0
            || point.y < 0
            || point.x as usize >= self.size[0]
            || point.y as usize >= self.size[1]
        {
            return None;
        }
        if self.roi.contains(point) {
            let at = ((point.y - self.roi.top) * self.roi.width() + point.x - self.roi.left)
                as usize
                * 4;
            return Some(self.initial[at..at + 4].try_into().unwrap());
        }
        // Four pixels are an experimental transfer granularity, not a claimed
        // optimum. Charge actual bytes including overlap with the initial ROI.
        let key = (point.x / 4 * 4, point.y / 4 * 4);
        let rect = Rect::new(
            key.0,
            key.1,
            (key.0 + 4).min(self.size[0] as i32),
            (key.1 + 4).min(self.size[1] as i32),
        );
        if !self.blocks.contains_key(&key) {
            let bytes = rect.area() as usize * 4;
            if self.extra_bytes + bytes > proposals::A_LIMITS[3].1 {
                self.failure = Some(Completion::BudgetExceeded);
                return None;
            }
            self.calls += 1;
            self.extra_bytes += bytes;
            match (self.read)(rect) {
                Ok(data) if data.len() == bytes => {
                    self.blocks.insert(key, data);
                }
                _ => {
                    self.failure = Some(Completion::Failed);
                    return None;
                }
            }
        }
        let at = ((point.y - rect.top) * rect.width() + point.x - rect.left) as usize * 4;
        Some(self.blocks[&key][at..at + 4].try_into().unwrap())
    }
}
