//! Pure color arithmetic, with no image, window, codec or platform dependency.
//! Linear Rec.709/scRGB uses 1.0 = 80 nit; SDR mapping targets 100 nit.
//! Callers validate finite channels, nonnegative PQ input and positive peaks.

pub fn srgb_to_linear(value: f64) -> f64 {
    let magnitude = value.abs();
    let result = if magnitude <= 0.04045 {
        magnitude / 12.92
    } else {
        ((magnitude + 0.055) / 1.055).powf(2.4)
    };
    result.copysign(value)
}

pub fn linear_to_srgb(value: f64) -> f64 {
    let magnitude = value.abs();
    let result = if magnitude <= 0.0031308 {
        magnitude * 12.92
    } else {
        1.055 * magnitude.powf(1.0 / 2.4) - 0.055
    };
    result.copysign(value)
}

// BT.2100 / ST 2084. Input is full-range normalized code, output absolute nit.
pub fn pq_to_nits(code: f64) -> f64 {
    let p = code.powf(32.0 / 2523.0);
    let numerator = (p - 3424.0 / 4096.0).max(0.0);
    let denominator = 2413.0 / 128.0 - 2392.0 / 128.0 * p;
    10000.0 * (numerator / denominator).powf(16384.0 / 2610.0)
}

pub fn nits_to_pq(nits: f64) -> f64 {
    let p = (nits / 10000.0).powf(2610.0 / 16384.0);
    ((3424.0 / 4096.0 + 2413.0 / 128.0 * p) / (1.0 + 2392.0 / 128.0 * p)).powf(2523.0 / 32.0)
}

// D65 to D65 matrix, derived from the BT.2020 and BT.709 chromaticities.
// Values outside [0, 1] intentionally preserve extended color gamut.
pub fn rec2020_to_rec709(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb;
    [
        1.6604910021084345 * r - 0.5876411387885495 * g - 0.0728498633198848 * b,
        -0.1245504745215907 * r + 1.1328998971259598 * g - 0.0083494226043692 * b,
        -0.0181507633549053 * r - 0.1005788980080074 * g + 1.1187296613629127 * b,
    ]
}

pub fn rec709_to_rec2020(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb;
    [
        0.627403895934699 * r + 0.329283038377883 * g + 0.043313065687418 * b,
        0.069097289358232 * r + 0.919540395075459 * g + 0.011362315566309 * b,
        0.016391438875150 * r + 0.088013307877226 * g + 0.895595253247624 * b,
    ]
}

/// BT.2390 EETF with zero source/target black. Hermite roll-off in normalized
/// PQ space; no exposure change below the knee. This is a display derivative,
/// never a mutation of the absolute source. Inputs are validated by the caller.
pub fn map_luminance(nits: f64, source_peak: f64, target_peak: f64) -> f64 {
    if source_peak <= target_peak || nits <= 0.0 {
        return nits.max(0.0);
    }
    let black = nits_to_pq(0.0);
    let source_range = nits_to_pq(source_peak.min(10000.0)) - black;
    let maximum = (nits_to_pq(target_peak.min(10000.0)) - black) / source_range;
    let knee = (1.5 * maximum - 0.5).clamp(0.0, 1.0);
    let input = ((nits_to_pq(nits.min(source_peak)) - black) / source_range).clamp(0.0, 1.0);
    let output = if input <= knee || knee >= 1.0 {
        input
    } else {
        let t = (input - knee) / (1.0 - knee);
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * knee
            + (t3 - 2.0 * t2 + t) * (1.0 - knee)
            + (-2.0 * t3 + 3.0 * t2) * maximum
    };
    pq_to_nits(output * source_range + black)
}

/// Absolute scRGB to relative linear SDR. The target's logical white/peak is
/// fixed at 100 nit. Compress chroma toward the same luminance only when RGB
/// leaves the target cube; per-channel clipping would shift hues.
pub fn scrgb_to_sdr(rgb: [f64; 3], source_peak: f64) -> [f64; 3] {
    let luminance = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    if luminance <= 0.0 {
        return [0.0; 3];
    }
    // PQ round-trip arithmetic may put the mapped peak a few ulps above white.
    // Bound the target luminance before computing chroma intersections.
    let mapped =
        (map_luminance(luminance * 80.0, source_peak.max(100.0), 100.0) / 100.0).clamp(0.0, 1.0);
    let scaled = rgb.map(|v| v / luminance * mapped);
    let mut chroma = 1_f64;
    for value in scaled {
        if value < 0.0 {
            chroma = chroma.min(mapped / (mapped - value));
        } else if value > 1.0 {
            chroma = chroma.min((1.0 - mapped) / (value - mapped));
        }
    }
    scaled.map(|v| (mapped + (v - mapped) * chroma.clamp(0.0, 1.0)).clamp(0.0, 1.0))
}
