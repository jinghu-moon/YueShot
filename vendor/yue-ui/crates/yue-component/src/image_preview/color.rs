//! Pure color math and ICC preparation. No display, file I/O or retained state.
//! Working RGB uses linear Rec.709 primaries; absolute scRGB units are 80 nit.
use moxcms::{ColorProfile, ToneReprCurve};

pub(crate) fn linear_srgb_profile() -> ColorProfile {
    let mut profile = ColorProfile::new_srgb();
    let curve = Some(ToneReprCurve::Parametric(vec![1.0]));
    profile.red_trc = curve.clone();
    profile.green_trc = curve.clone();
    profile.blue_trc = curve;
    profile.cicp = None;
    profile
}

pub(crate) use yue_color::{
    linear_to_srgb, pq_to_nits, rec2020_to_rec709, scrgb_to_sdr, srgb_to_linear,
};
#[cfg(test)]
use yue_color::{map_luminance, nits_to_pq, rec709_to_rec2020};

#[cfg(test)]
mod tests {
    use super::*;
    include!("../../tests/fixtures/hdr/sdr-vectors.rs");

    #[test]
    fn cpu_mapping_matches_independent_quantized_oracle() {
        for &(rgb, peak, expected) in SDR_MAPPING_VECTORS {
            for (actual, expected) in scrgb_to_sdr(rgb, peak).into_iter().zip(expected) {
                close(actual, expected, 1e-9);
            }
        }
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn pq_reference_codes_and_absolute_scrgb_white() {
        for (nits, code) in [
            (100.0, 0.508078421517399),
            (203.0, 0.5806888810416109),
            (1000.0, 0.751827096247041),
            (10000.0, 1.0),
        ] {
            close(nits_to_pq(nits), code, 1e-12);
            close(pq_to_nits(code), nits, 1e-6);
        }
        close(pq_to_nits(0.0), 0.0, 1e-12);
        close(pq_to_nits(nits_to_pq(203.0)) / 80.0, 2.5375, 1e-9);
    }

    #[test]
    fn srgb_uses_piecewise_transfer_and_preserves_extended_signs() {
        close(srgb_to_linear(0.04045), 0.00313080495356, 1e-12);
        close(linear_to_srgb(0.0031308), 0.040449936, 1e-12);
        for v in [-1.3, -0.04, 0.0, 0.2, 0.5, 1.0, 2.5] {
            close(linear_to_srgb(srgb_to_linear(v)), v, 1e-7);
        }
    }

    #[test]
    fn primary_conversion_keeps_negative_and_superwhite_components() {
        let primary = rec2020_to_rec709([1.0, 0.0, 0.0]);
        assert!(primary[0] > 1.0 && primary[1] < 0.0 && primary[2] < 0.0);
        for rgb in [[1.0; 3], [12.5; 3], [0.2, -0.1, 2.0]] {
            for (actual, expected) in rec709_to_rec2020(rec2020_to_rec709(rgb))
                .into_iter()
                .zip(rgb)
            {
                close(actual, expected, 1e-12);
            }
        }
    }

    #[test]
    fn eetf_is_monotonic_maps_the_peak_and_does_not_compress_in_range_content() {
        for (source, target) in [(1000.0, 100.0), (10000.0, 100.0), (1000.0, 600.0)] {
            close(map_luminance(source, source, target), target, 1e-6);
            let mut previous = -1.0;
            for i in 0..=1000 {
                let value = map_luminance(source * f64::from(i) / 1000.0, source, target);
                assert!(value >= previous - 1e-8 && value <= target + 1e-6);
                previous = value;
            }
        }
        close(map_luminance(203.0, 400.0, 1000.0), 203.0, 1e-9);
    }

    #[test]
    fn sdr_mapping_retains_neutrality_and_gamut_compresses_instead_of_clipping() {
        for gray in [0.0, 1.0, 2.5375, 12.5, 125.0] {
            let output = scrgb_to_sdr([gray; 3], 10000.0);
            close(output[0], output[1], 1e-12);
            close(output[1], output[2], 1e-12);
            assert!(output.iter().all(|v| (0.0..=1.0).contains(v)));
        }
        let output = scrgb_to_sdr(rec2020_to_rec709([3.0, 0.1, 0.1]), 1000.0);
        assert!(
            output
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        );
    }

    #[test]
    fn icc_to_linear_f32_keeps_precision_and_alpha() {
        let source = ColorProfile::new_from_slice(include_bytes!(
            "../../tests/fixtures/hdr/srgb-matrix.icc"
        ))
        .unwrap();
        let transform = source
            .create_transform_f32(
                moxcms::Layout::Rgba,
                &linear_srgb_profile(),
                moxcms::Layout::Rgba,
                moxcms::TransformOptions::default(),
            )
            .unwrap();
        let input = [0.25, 0.5, 0.75, 0.37];
        let mut output = [0.0; 4];
        transform.transform(&input, &mut output).unwrap();
        for ix in 0..3 {
            close(
                f64::from(output[ix]),
                srgb_to_linear(f64::from(input[ix])),
                0.0002,
            );
        }
        close(f64::from(output[3]), f64::from(input[3]), 1e-7);
    }
}
