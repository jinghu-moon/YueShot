//! Two bounded proposal experiments sharing the same validator. No annotations.
use super::*;
#[cfg(test)]
use std::collections::VecDeque;

pub(super) const A_LIMITS: [(&str, usize); 4] = [
    ("peaks_per_side", 48),
    ("validation_requests", 64),
    ("levels", 24),
    ("extra_readback_bytes", 1048576),
];
#[cfg(test)]
pub(super) const B_LIMITS: [(&str, usize); 6] = [
    ("lines_per_direction", 192),
    ("proposals", 2048),
    ("global_validations", 256),
    ("roi_validations", 64),
    ("retained_candidates", 320),
    ("labels", 65536),
];
fn over_budget(value: usize, limit: usize) -> bool {
    value > limit
}

#[cfg(test)]
pub(super) fn budget_violations(
    algorithm: &str,
    counters: &[(&str, usize)],
) -> Result<Vec<&'static str>, Completion> {
    let limits: &[(&'static str, usize)] = match algorithm {
        "A" => &A_LIMITS,
        "B" => &B_LIMITS,
        _ => return Err(Completion::UnsupportedSource),
    };
    if counters
        .iter()
        .any(|(key, _)| !limits.iter().any(|(name, _)| name == key))
    {
        return Err(Completion::Failed);
    }
    Ok(limits
        .iter()
        .filter_map(|&(name, limit)| {
            counters
                .iter()
                .find(|(key, _)| *key == name)
                .and_then(|(_, value)| over_budget(*value, limit).then_some(name))
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct Detection {
    pub completion: Completion,
    pub proposals: usize,
    pub validations: usize,
    pub coarse_tests: usize,
    pub candidates: Vec<Candidate>,
}

pub(super) fn finish(
    image: Image<'_>,
    proposals: impl IntoIterator<Item = Rect, IntoIter: ExactSizeIterator>,
    point: Point,
    limit: usize,
    mut completion: Completion,
) -> Detection {
    let proposals = proposals.into_iter();
    let total = proposals.len();
    let mut candidates = Vec::new();
    let mut validations = 0;
    let mut coarse_tests = 0;
    for rect in proposals {
        coarse_tests += 1;
        let coarse = validate_limit(image, rect, Parameters::default(), 16);
        if !coarse.sides.iter().all(|e| {
            e.qualifies(Parameters::default())
                && e.tangent_roughness
                    <= Parameters::default().threshold * 2.0 * 64.0 / e.valid.max(1) as f32
        }) {
            continue;
        }
        if validations == limit {
            completion = Completion::BudgetExceeded;
            break;
        }
        validations += 1;
        let candidate = canonicalise(
            image,
            validate(image, rect, Parameters::default()),
            Parameters::default(),
        );
        if candidate.qualified && candidate.rect.contains(point) {
            if !image.reserve(&mut candidates, 1, "verified_candidates") {
                candidates.clear();
                completion = Completion::Failed;
                break;
            }
            candidates.push(candidate);
        }
    }
    // Deterministic geometry order; keep all distinct nested rectangles for now.
    // A default must come from this verified list, never an unverified proposal.
    rank(&mut candidates);
    if over_budget(candidates.len(), A_LIMITS[2].1) {
        candidates.truncate(24);
        completion = Completion::BudgetExceeded;
    }
    Detection {
        completion,
        proposals: total,
        validations,
        coarse_tests,
        candidates,
    }
}

fn rank(candidates: &mut Vec<Candidate>) {
    candidates.sort_unstable_by(|a, b| {
        let quality = |c: &Candidate| {
            let weakest = c
                .sides
                .iter()
                .map(|e| e.supported as f32 / e.requested.max(1) as f32)
                .fold(1.0, f32::min);
            let roughness = c.sides.iter().map(|e| e.tangent_roughness).sum::<f32>();
            (weakest, roughness)
        };
        let qa = quality(a);
        let qb = quality(b);
        a.corner_extensions
            .cmp(&b.corner_extensions)
            .then_with(|| qb.0.total_cmp(&qa.0))
            .then_with(|| qa.1.total_cmp(&qb.1))
            .then_with(|| {
                let geometry = |c: &Candidate| {
                    (
                        c.rect.area(),
                        c.rect.left,
                        c.rect.top,
                        c.rect.right,
                        c.rect.bottom,
                    )
                };
                geometry(a).cmp(&geometry(b))
            })
    });
    candidates.dedup_by_key(|c| c.rect);
}

pub(super) fn scan_peaks(image: Image<'_>, point: Point, side: usize) -> (Vec<i32>, bool) {
    let vertical = side < 2;
    let centre = if vertical { point.y } else { point.x };
    let tangent_min = if vertical {
        image.coverage.top
    } else {
        image.coverage.left
    };
    let tangent_max = if vertical {
        image.coverage.bottom
    } else {
        image.coverage.right
    };
    let start = if vertical {
        image.coverage.left
    } else {
        image.coverage.top
    };
    let normal_limit = if vertical {
        image.coverage.right
    } else {
        image.coverage.bottom
    };
    let coordinate = if vertical { point.x } else { point.y };
    let offset = ((tangent_max - tangent_min) / 8).clamp(1, 32);
    let tangents = [centre - offset, centre, centre + offset];
    // Five tangential rows cover the roughness probe without another round trip
    // on smaller sources. Reserve half the append budget for subsequent edge
    // evidence; larger sources keep the exact one-row plan.
    let scan_halo =
        if 3usize * (image.coverage.width() as usize + image.coverage.height() as usize) * 8 * 4
            <= A_LIMITS[3].1 / 2
        {
            2
        } else {
            0
        };
    for tangent in tangents {
        image.request(if vertical {
            Rect::new(
                start,
                tangent - scan_halo,
                normal_limit,
                tangent + 1 + scan_halo,
            )
        } else {
            Rect::new(
                tangent - scan_halo,
                start,
                tangent + 1 + scan_halo,
                normal_limit,
            )
        });
    }
    let mut peaks = Vec::new();
    for at in start + 1..normal_limit {
        if (side.is_multiple_of(2) && at > coordinate)
            || (!side.is_multiple_of(2) && at <= coordinate)
        {
            continue;
        }
        let mut votes = 0;
        let mut strength: f32 = 0.0;
        let mut roughness = 0.0;
        for tangent in tangents {
            if tangent < tangent_min || tangent >= tangent_max {
                continue;
            }
            let sign = if side.is_multiple_of(2) { 1.0 } else { -1.0 };
            let (position, inward) = if vertical {
                ([at as f32, tangent as f32 + 0.5], [sign, 0.0])
            } else {
                ([tangent as f32 + 0.5, at as f32], [0.0, sign])
            };
            let adjacent = if vertical {
                (Point::new(at - 1, tangent), Point::new(at, tangent))
            } else {
                (Point::new(tangent, at - 1), Point::new(tangent, at))
            };
            if !image
                .pixel(adjacent.0)
                .zip(image.pixel(adjacent.1))
                .is_some_and(|(a, b)| norm(difference(a, b)) >= Parameters::default().threshold)
            {
                continue;
            }
            if let Some((fill, stroke, _, scales, _)) =
                probe(image, position, inward, Parameters::default())
                && (fill || stroke)
            {
                votes += 1;
                strength = strength.max(scales.into_iter().fold(0.0, f32::max));
                let mut last = None;
                for t in tangent - 2..=tangent + 2 {
                    let p = if vertical {
                        [at as f32, t as f32 + 0.5]
                    } else {
                        [t as f32 + 0.5, at as f32]
                    };
                    if let (Some(a), Some(b)) = (
                        image.sample(p[0] - 0.5 * inward[0], p[1] - 0.5 * inward[1]),
                        image.sample(p[0] + 0.5 * inward[0], p[1] + 0.5 * inward[1]),
                    ) {
                        let entry = difference(b, a);
                        if let Some(previous) = last {
                            roughness += norm(difference(entry, previous));
                        }
                        last = Some(entry);
                    }
                }
            }
        }
        if votes > 0 && roughness / votes as f32 <= Parameters::default().threshold * 8.0 {
            if !image.reserve(&mut peaks, 1, "scan_peaks") {
                return (Vec::new(), true);
            }
            peaks.push((at, votes, strength, roughness / votes as f32));
        }
    }
    peaks.sort_unstable_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.3.total_cmp(&b.3))
            .then_with(|| (a.0 - coordinate).abs().cmp(&(b.0 - coordinate).abs()))
            .then_with(|| b.2.total_cmp(&a.2))
            .then(a.0.cmp(&b.0))
    });
    // The path admits at most 24 layers; each can expose two outline rims.
    // Keep that derived bound now that unrelated Cartesian products are gone.
    let exceeded = over_budget(peaks.len(), A_LIMITS[0].1);
    peaks.truncate(A_LIMITS[0].1);
    let mut positions = Vec::new();
    if !image.reserve(&mut positions, peaks.len(), "peak_positions") {
        return (positions, true);
    }
    positions.extend(peaks.into_iter().map(|p| p.0));
    (positions, exceeded)
}

/// A: three row/column strips find bounded peaks and trace their corners. Sampling beyond
/// the owned coverage is impossible. Full-frame test input is owned CPU memory;
/// physical strip/edge transfer batching is measured separately by readback probe.
pub fn seed(image: Image<'_>, point: Point) -> Detection {
    if image.coverage.is_empty() {
        return finish(image, Vec::new(), point, 64, Completion::Complete);
    }
    if !image.coverage.contains(point) {
        return Detection {
            completion: Completion::Failed,
            proposals: 0,
            validations: 0,
            coarse_tests: 0,
            candidates: Vec::new(),
        };
    }
    let lists = seed_peaks(image, point);
    if image.pending_reads() {
        return finish(image, Vec::new(), point, 64, Completion::Failed);
    }
    let completion = if lists.iter().any(|l| l.1) {
        Completion::BudgetExceeded
    } else {
        Completion::Complete
    };
    let rectangles = seed_rectangles(image, point, &lists);
    if image.pending_reads() {
        return finish(image, Vec::new(), point, 64, Completion::Failed);
    }
    finish(image, rectangles, point, 64, completion)
}

pub(super) type SeedPeaks = [(Vec<i32>, bool); 4];

pub(super) fn seed_peaks(image: Image<'_>, point: Point) -> SeedPeaks {
    std::array::from_fn(|side| scan_peaks(image, point, side))
}

pub(super) fn seed_rectangles(image: Image<'_>, point: Point, lists: &SeedPeaks) -> Vec<Rect> {
    let mut rectangles = Vec::new();
    // All three seed rays can hit the same long edge. Reuse only a fully read
    // contiguous span inside this proposal pass; missing pixels never cache a
    // shortened edge. This removes repeated O(edge length) traces.
    let mut spans: Vec<(i32, usize, i32, i32)> = Vec::new();
    let mut span = |edge, tangent, side| {
        if let Some(&(_, _, start, end)) = spans
            .iter()
            .find(|&&(e, s, a, b)| e == edge && s == side && a <= tangent && tangent < b)
        {
            return Some((start, end));
        }
        let result = line_span(image, edge, tangent, side)?;
        if !image.reserve(&mut spans, 1, "edge_spans") {
            return None;
        }
        spans.push((edge, side, result.0, result.1));
        Some(result)
    };
    for (side, (edges, _)) in lists.iter().enumerate() {
        let vertical = side < 2;
        let centre = if vertical { point.y } else { point.x };
        let extent = if vertical {
            image.coverage.height()
        } else {
            image.coverage.width()
        };
        let offset = (extent / 8).clamp(1, 32);
        for &edge in edges {
            for tangent in [centre - offset, centre, centre + offset] {
                let Some((start, end)) = span(edge, tangent, side) else {
                    continue;
                };
                // Trace connected edge runs to actual corners instead of making
                // a Cartesian product of unrelated row and column boundaries.
                for (corner, other_side) in [
                    (start, if vertical { 2 } else { 0 }),
                    (end, if vertical { 3 } else { 1 }),
                ] {
                    let inside = if side.is_multiple_of(2) {
                        edge
                    } else {
                        edge - 1
                    };
                    let Some((other_start, other_end)) = span(corner, inside, other_side) else {
                        continue;
                    };
                    let rect = if vertical {
                        Rect::new(other_start, start, other_end, end)
                    } else {
                        Rect::new(start, other_start, end, other_end)
                    };
                    if rect.contains(point) {
                        if !image.reserve(&mut rectangles, 1, "corner_rectangles") {
                            return Vec::new();
                        }
                        rectangles.push(rect);
                    }
                }
            }
        }
    }
    rectangles.sort_unstable_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
    rectangles.dedup();
    rectangles
}

fn line_span(image: Image<'_>, edge: i32, tangent: i32, side: usize) -> Option<(i32, i32)> {
    let vertical = side < 2;
    let (low, high) = if vertical {
        (image.coverage.top, image.coverage.bottom)
    } else {
        (image.coverage.left, image.coverage.right)
    };
    let supported = |t: i32| {
        let (a, b) = if vertical {
            (Point::new(edge - 1, t), Point::new(edge, t))
        } else {
            (Point::new(t, edge - 1), Point::new(t, edge))
        };
        let pixels = image.pixel(a).zip(image.pixel(b));
        if pixels.is_none() {
            let radius = (t - tangent)
                .unsigned_abs()
                .saturating_add(1)
                .max(256)
                .next_power_of_two() as i32;
            image.request(if vertical {
                Rect::new(
                    edge - 1,
                    (tangent - radius).max(low),
                    edge + 1,
                    (tangent + radius).min(high),
                )
            } else {
                Rect::new(
                    (tangent - radius).max(low),
                    edge - 1,
                    (tangent + radius).min(high),
                    edge + 1,
                )
            });
        }
        pixels.map(|(a, b)| norm(difference(a, b)) >= Parameters::default().threshold)
    };
    if tangent < low || tangent >= high || !supported(tangent)? {
        return None;
    }
    let mut start = tangent;
    let mut end = tangent + 1;
    while start > low && supported(start - 1)? {
        start -= 1;
    }
    while end < high && supported(end)? {
        end += 1;
    }
    (end - start >= 10).then_some((start, end))
}

/// B experiment: edge/background components plus directional lines on owned CPU
/// pixels. Does not call A. The comparison harness prepares the coarse image
/// once, then maps proposals back and revalidates original pixels.
#[cfg(test)]
pub fn components(image: Image<'_>, point: Point) -> Detection {
    let image = if image.coverage.area() > 1_048_576 {
        let left = (point.x - 256).clamp(
            image.coverage.left,
            (image.coverage.right - 512).max(image.coverage.left),
        );
        let top = (point.y - 256).clamp(
            image.coverage.top,
            (image.coverage.bottom - 512).max(image.coverage.top),
        );
        let coverage = Rect::new(
            left,
            top,
            (left + 512).min(image.coverage.right),
            (top + 512).min(image.coverage.bottom),
        );
        image.with_coverage(coverage).unwrap()
    } else {
        image
    };
    let width = image.coverage.width() as usize;
    let height = image.coverage.height() as usize;
    if width == 0 || height == 0 {
        return finish(image, Vec::new(), point, 256, Completion::Complete);
    }
    if width * height > 1_048_576 {
        return finish(image, Vec::new(), point, 256, Completion::UnsupportedSource);
    }
    let mut edge = vec![false; width * height];
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let here = image.pixel(Point::new(
                image.coverage.left + x as i32,
                image.coverage.top + y as i32,
            ));
            let left = image.pixel(Point::new(
                image.coverage.left + x as i32 - 1,
                image.coverage.top + y as i32,
            ));
            let top = image.pixel(Point::new(
                image.coverage.left + x as i32,
                image.coverage.top + y as i32 - 1,
            ));
            if let (Some(a), Some(b), Some(c)) = (here, left, top) {
                edge[y * width + x] = norm(difference(a, b)).max(norm(difference(a, c))) >= 5.0;
            }
        }
    }
    let mut proposals = Vec::new();
    let mut completion = Completion::Complete;
    for foreground in [true, false] {
        let result = walk_components(
            &edge,
            width,
            height,
            foreground,
            foreground,
            B_LIMITS[5].1,
            |bounds, count| {
                let bounds = Rect::new(
                    bounds.left + image.coverage.left,
                    bounds.top + image.coverage.top,
                    bounds.right + image.coverage.left,
                    bounds.bottom + image.coverage.top,
                );
                if bounds.right - bounds.left < 10
                    || bounds.bottom - bounds.top < 10
                    || !bounds.contains(point)
                {
                    return true;
                }
                if !foreground && count as f64 / (bounds.area() as f64) < 0.72 {
                    return true;
                }
                // Boundary pixels span the two sides of a mathematical seam. All
                // bounded adjacent geometric hypotheses are revalidated on original
                // pixels; a component's raw bounding box is never itself evidence.
                for left in [bounds.left - 1, bounds.left] {
                    for top in [bounds.top - 1, bounds.top] {
                        for right in [bounds.right - 1, bounds.right, bounds.right + 1] {
                            for bottom in [bounds.bottom - 1, bounds.bottom, bounds.bottom + 1] {
                                if proposals.len() == B_LIMITS[1].1 {
                                    return false;
                                }
                                proposals.push(Rect::new(left, top, right, bottom));
                            }
                        }
                    }
                }
                true
            },
        );
        if result != Completion::Complete {
            completion = result;
            break;
        }
    }
    proposals.sort_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
    let (lines, exceeded) = directional_rectangles(image, point);
    for rect in lines {
        if proposals.len() == B_LIMITS[1].1 {
            completion = Completion::BudgetExceeded;
            break;
        }
        proposals.push(rect);
    }
    if exceeded {
        completion = Completion::BudgetExceeded;
    }
    proposals.sort_by_key(|r| (r.area(), r.left, r.top, r.right, r.bottom));
    proposals.dedup();
    finish(
        image,
        proposals,
        point,
        if image.size[0] * image.size[1] > 1_048_576 {
            64
        } else {
            256
        },
        completion,
    )
}

#[cfg(test)]
pub(super) fn walk_components(
    mask: &[bool],
    width: usize,
    height: usize,
    foreground: bool,
    diagonal: bool,
    label_limit: usize,
    mut visit: impl FnMut(Rect, usize) -> bool,
) -> Completion {
    if width.checked_mul(height) != Some(mask.len()) || width > 16384 || height > 16384 {
        return Completion::Failed;
    }
    let mut seen = vec![false; mask.len()];
    let mut queue = VecDeque::new();
    let mut labels = 0;
    for seed in 0..mask.len() {
        if seen[seed] || mask[seed] != foreground {
            continue;
        }
        labels += 1;
        if over_budget(labels, label_limit) {
            return Completion::BudgetExceeded;
        }
        seen[seed] = true;
        queue.push_back(seed);
        let mut bounds = Rect::new(
            (seed % width) as i32,
            (seed / width) as i32,
            (seed % width + 1) as i32,
            (seed / width + 1) as i32,
        );
        let mut count = 0;
        while let Some(at) = queue.pop_front() {
            count += 1;
            let x = at % width;
            let y = at / width;
            bounds.left = bounds.left.min(x as i32);
            bounds.right = bounds.right.max(x as i32 + 1);
            bounds.top = bounds.top.min(y as i32);
            bounds.bottom = bounds.bottom.max(y as i32 + 1);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx == 0 && dy == 0) || (!diagonal && dx != 0 && dy != 0) {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let next = ny as usize * width + nx as usize;
                    if !seen[next] && mask[next] == foreground {
                        seen[next] = true;
                        queue.push_back(next);
                    }
                }
            }
        }
        if !visit(bounds, count) {
            return Completion::BudgetExceeded;
        }
    }
    Completion::Complete
}

#[cfg(test)]
fn directional_rectangles(image: Image<'_>, point: Point) -> (Vec<Rect>, bool) {
    let mut all = [Vec::new(), Vec::new()];
    let mut exceeded = false;
    for (axis, lines) in all.iter_mut().enumerate() {
        let vertical = axis == 0;
        let (normal_start, normal_end, tangent_start, tangent_end) = if vertical {
            (
                image.coverage.left,
                image.coverage.right,
                image.coverage.top,
                image.coverage.bottom,
            )
        } else {
            (
                image.coverage.top,
                image.coverage.bottom,
                image.coverage.left,
                image.coverage.right,
            )
        };
        'normal: for at in normal_start + 1..normal_end {
            let mut t = tangent_start;
            let supported = |t| {
                let (a, b) = if vertical {
                    (Point::new(at - 1, t), Point::new(at, t))
                } else {
                    (Point::new(t, at - 1), Point::new(t, at))
                };
                image
                    .pixel(a)
                    .zip(image.pixel(b))
                    .is_some_and(|(a, b)| norm(difference(a, b)) >= Parameters::default().threshold)
            };
            while t < tangent_end {
                if !supported(t) {
                    t += 1;
                    continue;
                }
                let start = t;
                t += 1;
                while t < tangent_end && supported(t) {
                    t += 1;
                }
                if t - start >= 25 {
                    if lines.len() == 192 {
                        exceeded = true;
                        break 'normal;
                    }
                    lines.push((at, start, t));
                }
            }
        }
    }
    let mut result = Vec::new();
    for &(x, top, bottom) in &all[0] {
        for &(_, left, right) in all[1]
            .iter()
            .filter(|&&(y, left, right)| (y == top || y == bottom) && left <= x && x <= right)
            .take(3)
        {
            let rect = Rect::new(left, top, right, bottom);
            if rect.contains(point) {
                result.push(rect);
            }
        }
    }
    (result, exceeded)
}

/// Exact box/area integration, built once per source by the comparison harness.
/// The temporary is a B-only experiment; its source acquisition cost is reported
/// separately and cannot be excluded from the product memory/latency budget.
#[cfg(test)]
pub fn coarse(image: Image<'_>) -> Option<([usize; 2], Vec<u8>)> {
    if image.coverage != Rect::new(0, 0, image.size[0] as i32, image.size[1] as i32)
        || image.coverage.area() <= 1_048_576
    {
        return None;
    }
    let [w, h] = image.size;
    let scale = (1280.0 / (w.max(h) as f64))
        .min((1_048_576.0 / (w * h) as f64).sqrt())
        .min(1.0);
    let size = [
        (w as f64 * scale).floor().max(1.0) as usize,
        (h as f64 * scale).floor().max(1.0) as usize,
    ];
    let mut result = vec![0u8; size[0] * size[1] * 4];
    for y in 0..size[1] {
        let y0 = y as f64 * h as f64 / size[1] as f64;
        let y1 = (y + 1) as f64 * h as f64 / size[1] as f64;
        for x in 0..size[0] {
            let x0 = x as f64 * w as f64 / size[0] as f64;
            let x1 = (x + 1) as f64 * w as f64 / size[0] as f64;
            let mut sum = [0.0; 3];
            for sy in y0.floor() as usize..y1.ceil() as usize {
                for sx in x0.floor() as usize..x1.ceil() as usize {
                    let weight = ((sx + 1) as f64).min(x1) - (sx as f64).max(x0);
                    let weight = weight * (((sy + 1) as f64).min(y1) - (sy as f64).max(y0));
                    let pixel = image.pixel(Point::new(sx as i32, sy as i32))?;
                    for k in 0..3 {
                        sum[k] += f64::from(pixel[k]) * weight;
                    }
                }
            }
            let at = (y * size[0] + x) * 4;
            for k in 0..3 {
                result[at + k] = (sum[k] / ((x1 - x0) * (y1 - y0))).round().clamp(0.0, 255.0) as u8;
            }
            result[at + 3] = 255;
        }
    }
    Some((size, result))
}

#[cfg(test)]
pub fn components_with_global(
    image: Image<'_>,
    global: Option<Image<'_>>,
    point: Point,
) -> Detection {
    let mut local = components(image, point);
    let Some(global) = global else {
        return local;
    };
    let mapped_point = Point::new(
        (point.x as i64 * global.size[0] as i64 / image.size[0] as i64) as i32,
        (point.y as i64 * global.size[1] as i64 / image.size[1] as i64) as i32,
    );
    let rough = components(global, mapped_point);
    let mut mapped = Vec::new();
    let global_validation_requests = rough.candidates.len();
    for c in rough.candidates {
        let map_x =
            |x: i32| (f64::from(x) * image.size[0] as f64 / global.size[0] as f64).round() as i32;
        let map_y =
            |y: i32| (f64::from(y) * image.size[1] as f64 / global.size[1] as f64).round() as i32;
        let c = locate(
            image,
            Rect::new(
                map_x(c.rect.left),
                map_y(c.rect.top),
                map_x(c.rect.right),
                map_y(c.rect.bottom),
            ),
            Parameters::default(),
        );
        if c.qualified && c.rect.contains(point) {
            mapped.push(c);
        }
    }
    mapped.extend(local.candidates);
    rank(&mut mapped);
    if mapped.len() > 24 {
        mapped.truncate(24);
        local.completion = Completion::BudgetExceeded;
    }
    local.candidates = mapped;
    local.proposals += rough.proposals;
    local.validations += global_validation_requests;
    local.coarse_tests += rough.coarse_tests + rough.validations;
    if rough.completion != Completion::Complete {
        local.completion = rough.completion;
    }
    local
}
