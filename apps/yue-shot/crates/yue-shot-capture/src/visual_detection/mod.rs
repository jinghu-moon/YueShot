//! P2 CPU reference. Input is immutable pixels and explicit read coverage only.
//! No DOM, fixture id, annotations, native semantics or UI state reaches detection.

use crate::geometry::{Point, Rect};
use serde::Serialize;
pub(crate) mod allocation;

pub(super) mod progressive;
pub(super) mod proposals;
#[cfg(test)]
pub(super) mod reads;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Completion {
    Complete,
    BudgetExceeded,
    UnsupportedSource,
    Failed,
}

/// Test reference uses fixed sRGB 8-bit channels. RGB/BGR permutations preserve
/// all channel-symmetric evidence; alpha remains the fourth channel.
#[derive(Clone, Copy)]
pub struct Image<'a> {
    size: [usize; 2],
    bytes: &'a [u8],
    coverage: Rect,
    reader: Option<&'a dyn Fn(Point) -> Option<[u8; 4]>>,
    prefetch: Option<&'a dyn Fn(Rect)>,
    pending: Option<&'a dyn Fn() -> bool>,
    failure: Option<&'a dyn Fn(Completion)>,
}

impl<'a> Image<'a> {
    #[cfg(test)]
    pub fn new(size: [usize; 2], bytes: &'a [u8]) -> Result<Self, Completion> {
        if size.iter().any(|&v| v > 16384) {
            return Err(Completion::UnsupportedSource);
        }
        let required = size[0]
            .checked_mul(size[1])
            .and_then(|n| n.checked_mul(4))
            .ok_or(Completion::Failed)?;
        if required != bytes.len() {
            return Err(Completion::Failed);
        }
        Ok(Self {
            size,
            bytes,
            coverage: Rect::new(0, 0, size[0] as i32, size[1] as i32),
            reader: None,
            prefetch: None,
            pending: None,
            failure: None,
        })
    }

    /// Test-only demand-read oracle. The reader must own/cache each transferred
    /// pixel; absent bytes cannot be inferred from geometry or annotations.
    pub fn from_reader(
        size: [usize; 2],
        reader: &'a dyn Fn(Point) -> Option<[u8; 4]>,
    ) -> Result<Self, Completion> {
        if size.iter().any(|&v| v > 16384) {
            return Err(Completion::UnsupportedSource);
        }
        Ok(Self {
            size,
            bytes: &[],
            coverage: Rect::new(0, 0, size[0] as i32, size[1] as i32),
            reader: Some(reader),
            prefetch: None,
            pending: None,
            failure: None,
        })
    }

    pub fn with_prefetch(
        mut self,
        prefetch: &'a dyn Fn(Rect),
        pending: &'a dyn Fn() -> bool,
        failure: &'a dyn Fn(Completion),
    ) -> Self {
        self.prefetch = Some(prefetch);
        self.pending = Some(pending);
        self.failure = Some(failure);
        self
    }

    fn fail(self, reason: Completion) {
        if let Some(failure) = self.failure {
            failure(reason);
        }
    }
    fn reserve<T>(self, values: &mut Vec<T>, additional: usize, site: &'static str) -> bool {
        if allocation::reserve(values, additional, site) {
            true
        } else {
            self.fail(Completion::Failed);
            false
        }
    }

    #[cfg(test)]
    fn pending_reads(self) -> bool {
        self.pending.is_some_and(|pending| pending())
    }

    fn request(self, region: Rect) {
        if let Some(prefetch) = self.prefetch {
            let clipped = region.intersect(self.coverage);
            if !clipped.is_empty() {
                prefetch(clipped);
            }
        }
    }

    #[cfg(test)]
    pub fn with_coverage(self, coverage: Rect) -> Result<Self, Completion> {
        if coverage.is_empty() || !self.coverage.contains_rect(coverage) {
            return Err(Completion::Failed);
        }
        Ok(Self { coverage, ..self })
    }

    #[cfg(test)]
    pub fn from_pixels(pixels: &'a yue_media::pixels::PixelBuffer) -> Result<Self, Completion> {
        if pixels.descriptor().format() != yue_media::PixelFormat::Bgra8Unorm
            || pixels.descriptor().encoding() != yue_media::pixels::PixelEncoding::Srgb
        {
            return Err(Completion::UnsupportedSource);
        }
        Self::new(pixels.size().map(|v| v as usize), pixels.as_bytes())
    }

    fn pixel(self, point: Point) -> Option<[f32; 3]> {
        if !self.coverage.contains(point) {
            return None;
        }
        let pixel: [u8; 4] = if let Some(reader) = self.reader {
            reader(point)?
        } else {
            let at = (point.y as usize * self.size[0] + point.x as usize) * 4;
            self.bytes[at..at + 4].try_into().unwrap()
        };
        // Transparent source has no unique displayed background. The generated
        // composited PNGs are opaque; unmapped alpha is not silently interpreted.
        if pixel[3] != 255 {
            return None;
        }
        Some([pixel[0], pixel[1], pixel[2]].map(f32::from))
    }

    /// Coordinates are physical pixel geometry: pixel centres at (x+0.5,y+0.5).
    fn sample(self, x: f32, y: f32) -> Option<[f32; 3]> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let x = x - 0.5;
        let y = y - 0.5;
        let left = x.floor() as i32;
        let top = y.floor() as i32;
        let fx = x - left as f32;
        let fy = y - top as f32;
        // Axis-aligned probes keep one coordinate at a pixel centre. Avoid the
        // general four-neighbour loop for that measured hot path, retaining
        // the same weights and addition order (including zero-weight reads).
        if fx == 0.0 && fy == 0.0 {
            return self.pixel(Point::new(left, top));
        }
        if fy == 0.0 {
            let a = self.pixel(Point::new(left, top))?;
            let b = self.pixel(Point::new(left + 1, top))?;
            return Some(std::array::from_fn(|k| (1.0 - fx) * a[k] + fx * b[k]));
        }
        if fx == 0.0 {
            let a = self.pixel(Point::new(left, top))?;
            let b = self.pixel(Point::new(left, top + 1))?;
            return Some(std::array::from_fn(|k| (1.0 - fy) * a[k] + fy * b[k]));
        }
        let mut out = [0.0; 3];
        // Zero-weight neighbours are deliberately not read. A valid source-edge
        // centre is not rejected just because its unused bilinear neighbour is out.
        for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
            for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
                let weight = wx * wy;
                if weight > 0.0 {
                    let value = self.pixel(Point::new(left + dx, top + dy))?;
                    for k in 0..3 {
                        out[k] += weight * value[k];
                    }
                }
            }
        }
        Some(out)
    }
}

fn norm(value: [f32; 3]) -> f32 {
    value.into_iter().map(f32::abs).fold(0.0, f32::max)
}

fn difference(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| a[k] - b[k])
}

fn jump(samples: [[f32; 3]; 4]) -> f32 {
    let [a, b, c, d] = samples;
    norm(std::array::from_fn(|k| {
        (c[k] - b[k]) - ((b[k] - a[k]) + (d[k] - c[k])) * 0.5
    }))
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Parameters {
    threshold: f32,
    support: f32,
    max_gap: f32,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            threshold: 5.0,
            support: 0.65,
            max_gap: 0.20,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct SideEvidence {
    requested: usize,
    valid: usize,
    supported: usize,
    max_unsupported_run: usize,
    longest_supported_run: usize,
    fill: usize,
    stroke: usize,
    inner_stroke: usize,
    tangent_roughness: f32,
    outer_offsets: [usize; 9],
    multiscale_peak: [f32; 3],
    positions: Vec<i32>,
}

impl SideEvidence {
    fn qualifies(&self, parameters: Parameters) -> bool {
        self.valid >= 8
            && self.valid == self.requested
            && self.supported as f32 >= self.valid as f32 * parameters.support
            && self.max_unsupported_run as f32 <= self.requested as f32 * parameters.max_gap
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    #[serde(serialize_with = "serialize_rect")]
    pub(crate) rect: Rect,
    sides: [SideEvidence; 4],
    qualified: bool,
    corner_extensions: usize,
}

fn serialize_rect<S: serde::Serializer>(rect: &Rect, serializer: S) -> Result<S::Ok, S::Error> {
    [
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    ]
    .serialize(serializer)
}

/// Distinct integer tangent positions, corner exclusion fixed before sampling.
#[cfg(test)]
fn sample_positions(start: i32, end: i32) -> Vec<i32> {
    sample_positions_limit(Image::new([0, 0], &[]).unwrap(), start, end, 64)
}

fn sample_positions_limit(image: Image<'_>, start: i32, end: i32, maximum: usize) -> Vec<i32> {
    let length = end.saturating_sub(start);
    if length <= 0 {
        return Vec::new();
    }
    let corner = (length + 9) / 10;
    let available = (length - 2 * corner).max(0) as usize;
    let count = available.min(maximum);
    if count == 0 {
        return Vec::new();
    }
    let mut positions = Vec::new();
    if !image.reserve(&mut positions, count, "validation_positions") {
        return positions;
    }
    positions.extend((0..count).map(|i| {
        start
            + corner
            + if count == 1 {
                0
            } else {
                (i * (available - 1) / (count - 1)) as i32
            }
    }));
    positions
}

fn request_probe(image: Image<'_>, position: [f32; 2], inward: [f32; 2]) {
    let x = position[0].floor() as i32;
    let y = position[1].floor() as i32;
    image.request(if inward[0] != 0.0 {
        Rect::new(x - 13, y - 1, x + 14, y + 2)
    } else {
        Rect::new(x - 1, y - 13, x + 2, y + 14)
    });
}

fn request_validation(image: Image<'_>, rect: Rect) {
    for side in 0..4 {
        let vertical = side < 2;
        let (edge, start, end, normal) = match side {
            0 => (rect.left, rect.top, rect.bottom, [1., 0.]),
            1 => (rect.right, rect.top, rect.bottom, [-1., 0.]),
            2 => (rect.top, rect.left, rect.right, [0., 1.]),
            _ => (rect.bottom, rect.left, rect.right, [0., -1.]),
        };
        for tangent in sample_positions_limit(image, start, end, 64) {
            let p = if vertical {
                [edge as f32, tangent as f32 + 0.5]
            } else {
                [tangent as f32 + 0.5, edge as f32]
            };
            request_probe(image, p, normal);
        }
    }
}

fn probe(
    image: Image<'_>,
    position: [f32; 2],
    inward: [f32; 2],
    parameters: Parameters,
) -> Option<(bool, bool, bool, [f32; 3], usize)> {
    // A request describes evidence needed by this calculation, never a result.
    // It lets a worker collect one batch instead of requiring a blocking read
    // at every sample. Direct byte/oracle inputs ignore this hint.
    request_probe(image, position, inward);
    let sample = |offset: f32| {
        image.sample(
            position[0] + offset * inward[0],
            position[1] + offset * inward[1],
        )
    };
    let outside = sample(-0.5)?;
    let inside = sample(0.5)?;
    let entry = difference(inside, outside);
    let direct = norm(entry);
    let mut peaks = [0.0; 3];
    let mut scales = 0;
    for (i, d) in [1.0, 2.0, 4.0].into_iter().enumerate() {
        if let (Some(a), Some(b), Some(c), Some(e)) =
            (sample(-3.0 * d), sample(-d), sample(d), sample(3.0 * d))
        {
            peaks[i] = jump([a, b, c, e]);
            scales += 1;
        }
    }
    if scales == 0 {
        return None;
    }
    // Look for the first opposite transition on each side, not an arbitrary
    // texture response further away. This distinguishes an outline's outer rim
    // from the equally strong inner rim without merging nearby nested boxes.
    let mut paired = [false; 2];
    let mut outer_offset = 0;
    if direct >= parameters.threshold {
        for (side, sign) in [-1.0, 1.0].into_iter().enumerate() {
            for offset in 1..=8 {
                let (Some(before), Some(after)) = (
                    sample(sign * offset as f32 - 0.5),
                    sample(sign * offset as f32 + 0.5),
                ) else {
                    break;
                };
                let delta = difference(after, before);
                if norm(delta) < parameters.threshold {
                    continue;
                }
                let opposite = (0..3)
                    .any(|k| entry[k].abs() >= parameters.threshold && delta[k] * entry[k] < 0.0);
                let same_background = norm(difference(
                    if sign > 0.0 { after } else { before },
                    if sign > 0.0 {
                        sample(-8.5).unwrap_or(outside)
                    } else {
                        sample(8.5).unwrap_or(inside)
                    },
                )) <= (parameters.threshold * 2.0).max(direct * 0.5);
                if opposite && same_background {
                    paired[side] = true;
                    if side == 0 {
                        outer_offset = offset as usize;
                    }
                    break;
                }
            }
        }
    }
    let stroke = paired[1];
    let inner_stroke = paired[0] && !stroke;
    let fill = peaks.iter().any(|&v| v >= parameters.threshold) && direct >= parameters.threshold;
    // A paired transition outside ONE side can also be the real gap between
    // adjacent filled objects. Keep that evidence; reject an all-inner outline
    // only after the four sides have been considered together.
    Some((
        fill,
        stroke,
        inner_stroke,
        peaks,
        if inner_stroke { outer_offset } else { 0 },
    ))
}

pub fn validate(image: Image<'_>, rect: Rect, parameters: Parameters) -> Candidate {
    validate_limit(image, rect, parameters, 64)
}

fn validate_limit(
    image: Image<'_>,
    rect: Rect,
    parameters: Parameters,
    maximum: usize,
) -> Candidate {
    let mut sides: [SideEvidence; 4] = std::array::from_fn(|_| SideEvidence::default());
    if rect.is_empty() || !image.coverage.contains_rect(rect) {
        return Candidate {
            rect,
            sides,
            qualified: false,
            corner_extensions: 0,
        };
    }
    for (side, evidence) in sides.iter_mut().enumerate() {
        *evidence = measure_side_limit(image, rect, side, parameters, maximum);
    }
    let qualified = sides
        .iter()
        .all(|e| e.qualifies(parameters) && e.tangent_roughness <= parameters.threshold * 2.0)
        && !sides.iter().all(|e| e.inner_stroke * 2 > e.valid);
    let mut corner_extensions = 0;
    for side in 0..4 {
        let (edge, start, end, normal, vertical) = match side {
            0 => (rect.left, rect.top, rect.bottom, [1., 0.], true),
            1 => (rect.right, rect.top, rect.bottom, [-1., 0.], true),
            2 => (rect.top, rect.left, rect.right, [0., 1.], false),
            _ => (rect.bottom, rect.left, rect.right, [0., -1.], false),
        };
        for direction in [-1, 1] {
            let continuous = (0..4).all(|offset| {
                let t = if direction < 0 {
                    start - 1 - offset
                } else {
                    end + offset
                };
                let p = if vertical {
                    [edge as f32, t as f32 + 0.5]
                } else {
                    [t as f32 + 0.5, edge as f32]
                };
                probe(image, p, normal, parameters)
                    .is_some_and(|(fill, stroke, _, _, _)| fill || stroke)
            });
            corner_extensions += usize::from(continuous);
        }
    }
    Candidate {
        rect,
        sides,
        qualified,
        corner_extensions,
    }
}

/// Canonicalise the four rims as ONE outline. A paired edge on just one
/// side is also a possible gap between filled neighbours and is left intact.
#[cfg(test)]
pub fn locate(image: Image<'_>, mut rect: Rect, parameters: Parameters) -> Candidate {
    let mut initial = validate(image, rect, parameters);
    let paired = |e: &SideEvidence| {
        e.valid >= 8 && (e.stroke + e.inner_stroke) as f32 >= e.valid as f32 * parameters.support
    };
    if !initial.qualified && !initial.sides.iter().all(paired) {
        for side in 0..4 {
            let mut best = None;
            for delta in -16i32..=16 {
                let mut trial = rect;
                match side {
                    0 => trial.left += delta,
                    1 => trial.right += delta,
                    2 => trial.top += delta,
                    _ => trial.bottom += delta,
                }
                if trial.is_empty() || !image.coverage.contains_rect(trial) {
                    continue;
                }
                let e = measure_side(image, trial, side, parameters);
                if !e.qualifies(parameters) {
                    continue;
                }
                let support = (e.supported as f32 / e.valid as f32).to_bits();
                let key = (
                    support,
                    e.stroke,
                    std::cmp::Reverse(e.inner_stroke),
                    std::cmp::Reverse(delta.abs()),
                );
                if best.as_ref().is_none_or(|b: &(_, Rect)| key > b.0) {
                    best = Some((key, trial));
                }
            }
            if let Some((key, trial)) = best {
                if key.3.0 == 16 {
                    initial.qualified = false;
                    return initial;
                }
                rect = trial;
            }
        }
        initial = validate(image, rect, parameters);
    }
    canonicalise(image, initial, parameters)
}

/// Geometry only. Native ownership and source policy belong to the P4 gate.
#[cfg(test)]
fn contained_chain(rectangles: &[Rect], selected: Rect) -> Vec<Rect> {
    let mut outer = rectangles
        .iter()
        .copied()
        .filter(|r| !r.is_empty() && r.contains_rect(selected))
        .collect::<Vec<_>>();
    outer.sort_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
    outer.dedup();
    let mut chain = vec![selected];
    for rect in outer {
        if rect != *chain.last().unwrap() && rect.contains_rect(*chain.last().unwrap()) {
            chain.push(rect);
        }
    }
    chain.reverse();
    chain
}

fn canonicalise(image: Image<'_>, initial: Candidate, parameters: Parameters) -> Candidate {
    let mut rect = initial.rect;
    let paired = |e: &SideEvidence| {
        e.valid >= 8 && (e.stroke + e.inner_stroke) as f32 >= e.valid as f32 * parameters.support
    };
    if !initial.sides.iter().all(paired) {
        return initial;
    }
    for (side, e) in initial.sides.iter().enumerate() {
        let offset = e
            .outer_offsets
            .iter()
            .enumerate()
            .max_by_key(|&(i, n)| (*n, std::cmp::Reverse(i)))
            .unwrap()
            .0 as i32;
        match side {
            0 => rect.left -= offset,
            1 => rect.right += offset,
            2 => rect.top -= offset,
            _ => rect.bottom += offset,
        }
    }
    validate(image, rect, parameters)
}

#[cfg(test)]
fn measure_side(image: Image<'_>, rect: Rect, side: usize, parameters: Parameters) -> SideEvidence {
    measure_side_limit(image, rect, side, parameters, 64)
}

fn measure_side_limit(
    image: Image<'_>,
    rect: Rect,
    side: usize,
    parameters: Parameters,
    maximum: usize,
) -> SideEvidence {
    let mut evidence = SideEvidence::default();
    let vertical = side < 2;
    evidence.positions = sample_positions_limit(
        image,
        if vertical { rect.top } else { rect.left },
        if vertical { rect.bottom } else { rect.right },
        maximum,
    );
    evidence.requested = evidence.positions.len();
    let mut gap = 0;
    let mut run = 0;
    for &tangent in &evidence.positions {
        let (point, normal) = match side {
            0 => ([rect.left as f32, tangent as f32 + 0.5], [1.0, 0.0]),
            1 => ([rect.right as f32, tangent as f32 + 0.5], [-1.0, 0.0]),
            2 => ([tangent as f32 + 0.5, rect.top as f32], [0.0, 1.0]),
            _ => ([tangent as f32 + 0.5, rect.bottom as f32], [0.0, -1.0]),
        };
        let mut supported = false;
        if let (Some(a), Some(b)) = (
            image.sample(point[0] - 0.5 * normal[0], point[1] - 0.5 * normal[1]),
            image.sample(point[0] + 0.5 * normal[0], point[1] + 0.5 * normal[1]),
        ) {
            let entry = difference(b, a);
            let previous = if vertical {
                [point[0], point[1] - 1.0]
            } else {
                [point[0] - 1.0, point[1]]
            };
            if let (Some(a), Some(b)) = (
                image.sample(previous[0] - 0.5 * normal[0], previous[1] - 0.5 * normal[1]),
                image.sample(previous[0] + 0.5 * normal[0], previous[1] + 0.5 * normal[1]),
            ) {
                evidence.tangent_roughness += norm(difference(entry, difference(b, a)));
            }
        }
        if let Some((fill, stroke, inner, peaks, outer_offset)) =
            probe(image, point, normal, parameters)
        {
            evidence.valid += 1;
            evidence.fill += usize::from(fill);
            evidence.stroke += usize::from(stroke);
            evidence.inner_stroke += usize::from(inner);
            evidence.outer_offsets[outer_offset] += 1;
            for (saved, peak) in evidence.multiscale_peak.iter_mut().zip(peaks) {
                *saved = saved.max(peak);
            }
            supported = fill || stroke;
        }
        if supported {
            evidence.supported += 1;
            run += 1;
            gap = 0;
        } else {
            gap += 1;
            run = 0;
        }
        evidence.max_unsupported_run = evidence.max_unsupported_run.max(gap);
        evidence.longest_supported_run = evidence.longest_supported_run.max(run);
    }
    evidence.tangent_roughness /= evidence.valid.max(1) as f32;

    evidence
}
