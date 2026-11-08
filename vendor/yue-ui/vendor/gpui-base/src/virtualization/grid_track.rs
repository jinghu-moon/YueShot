//! Constraint-driven auto tracks, independent of cell intrinsic measurement.
use super::*;

/// A finite explicit track constraint. Zero weight keeps the minimum; positive
/// weights share remaining viewport space until their respective maxima.
#[derive(Clone, Copy, Debug)]
pub struct VirtualGridTrack {
    minimum: Pixels,
    maximum: Pixels,
    weight: f32,
}
impl VirtualGridTrack {
    pub fn new(minimum: Pixels, maximum: Pixels, weight: f32) -> Self {
        Self {
            minimum,
            maximum,
            weight,
        }
    }
    pub fn minimum(&self) -> Pixels {
        self.minimum
    }
    pub fn maximum(&self) -> Pixels {
        self.maximum
    }
    pub fn weight(&self) -> f32 {
        self.weight
    }
    /// Resolve an explicit list of tracks in O(T log T), without reading or
    /// measuring cells. Insufficient space preserves minima (and scrolling);
    /// surplus after all maxima stays unused. Feed the result to Known sizing
    /// or a coherent update_sizes batch when the available constraint changes.
    pub fn resolve(
        tracks: &[Self],
        available: Pixels,
        gap: Pixels,
        padding_start: Pixels,
        padding_end: Pixels,
    ) -> Result<Vec<Pixels>, VirtualizerError> {
        if !valid_nonnegative(available)
            || !valid_nonnegative(gap)
            || !valid_nonnegative(padding_start)
            || !valid_nonnegative(padding_end)
            || tracks.iter().any(|t| {
                !valid_positive(t.minimum)
                    || !valid_positive(t.maximum)
                    || t.maximum < t.minimum
                    || !t.weight.is_finite()
                    || t.weight < 0.
            })
        {
            return Err(VirtualizerError::InvalidGeometry("auto track constraints"));
        }
        let budget = (f64::from(available)
            - f64::from(padding_start)
            - f64::from(padding_end)
            - tracks.len().saturating_sub(1) as f64 * f64::from(gap))
        .max(0.);
        let minima: f64 = tracks.iter().map(|t| f64::from(t.minimum)).sum();
        let mut remaining = (budget - minima).max(0.);
        let mut stops: Vec<_> = tracks
            .iter()
            .filter(|t| t.weight > 0. && t.maximum > t.minimum)
            .map(|t| {
                (
                    (f64::from(t.maximum) - f64::from(t.minimum)) / f64::from(t.weight),
                    f64::from(t.weight),
                )
            })
            .collect();
        stops.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut weight: f64 = stops.iter().map(|(_, w)| w).sum();
        let mut level = 0.;
        for (stop, w) in stops {
            if weight <= 0. {
                break;
            }
            let cost = (stop - level) * weight;
            if remaining <= cost {
                level += remaining / weight;
                break;
            }
            remaining -= cost;
            level = stop;
            weight = (weight - w).max(0.);
        }
        // Unused space after every maximum is deliberately left unallocated.
        Ok(tracks
            .iter()
            .map(|t| {
                Pixels::from(
                    (f64::from(t.minimum) + level * f64::from(t.weight)).min(f64::from(t.maximum)),
                )
            })
            .collect())
    }
}
