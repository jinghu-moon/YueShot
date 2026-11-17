//! One fourth-order superellipse for selection rendering, export and grips.
//! Coordinates are distances from the outer corner of its radius-square.
//! No configurable exponent: removing this shared definition would let the
//! displayed cutout, saved alpha and resize grips disagree about the shape.

/// Normalized centre-to-diagonal distance: 2^(-1/4).
const DIAGONAL: f64 = 0.840_896_415_253_714_5;

/// First half of (x^4 + y^4 = 1), from (1, 0) to the diagonal.
/// Endpoint positions, tangents and signed curvatures match the analytic curve;
/// controls lie on those tangents. Solving the two endpoint curvature equations
/// fixes each pair of control distances. The first two controls have x=1, so
/// curvature starts at zero where the straight edge joins. Mirror/reverse gives
/// the other half with the same curvature at the diagonal.
///
/// Fixed controls avoid trig, root solving or allocation while painting. Tests
/// check G2 joins and analytic boundary error at the product's 128px radius cap.
const HALF: [[[f64; 2]; 4]; 4] = [
    [
        [1., 0.],
        [1., 0.117_210_453_622_620_04],
        [1., 0.187_591_702_074_206_8],
        [0.999_022_003_720_116_7, 0.25],
    ],
    [
        [0.999_022_003_720_116_7, 0.25],
        [0.997_590_384_820_071_6, 0.341_355_049_776_710_7],
        [0.993_927_263_456_288, 0.424_295_122_312_408_46],
        [0.983_994_835_632_715_2, 0.5],
    ],
    [
        [0.983_994_835_632_715_2, 0.5],
        [0.976_083_014_277_034_9, 0.560_303_832_925_552_5],
        [0.964_004_951_888_347_3, 0.617_451_816_685_833_4],
        [0.945_294_662_840_581_6, 0.67],
    ],
    [
        [0.945_294_662_840_581_6, 0.67],
        [0.922_074_601_352_050_7, 0.735_213_960_326_885_2],
        [0.888_745_195_356_930_4, 0.793_047_635_150_498_7],
        [DIAGONAL, DIAGONAL],
    ],
];

pub(crate) fn corner_cubics() -> [[[f32; 2]; 4]; 8] {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            let [x, y] = if i < 4 {
                HALF[i][j]
            } else {
                HALF[7 - i][3 - j]
            };
            if i < 4 {
                [x as f32, y as f32]
            } else {
                [y as f32, x as f32]
            }
        })
    })
}

pub(crate) fn diagonal_inset(radius: i32) -> i32 {
    (f64::from(radius) * (1. - DIAGONAL)).round() as i32
}

pub(crate) fn contains_corner(x: f32, y: f32, radius: f32) -> bool {
    let dx = ((radius - x) / radius).max(0.);
    let dy = ((radius - y) / radius).max(0.);
    let dx2 = dx * dx;
    let dy2 = dy * dy;
    dx2 * dx2 + dy2 * dy2 <= 1.
}

/// Analytic scanline boundary for GDI's binary window regions. The native
/// region cannot carry alpha, so it includes pixels whose centres are inside.
pub(crate) fn boundary_inset(y: f64, radius: f64) -> f64 {
    let dy = (radius - y) / radius;
    let dy2 = dy * dy;
    radius * (1. - (1. - dy2 * dy2).max(0.).sqrt().sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curvature(points: [[f32; 2]; 4], end: bool) -> f64 {
        let p = if end {
            [points[3], points[2], points[1]]
        } else {
            [points[0], points[1], points[2]]
        };
        let u = [f64::from(p[1][0] - p[0][0]), f64::from(p[1][1] - p[0][1])];
        let v = [f64::from(p[2][0] - p[1][0]), f64::from(p[2][1] - p[1][1])];
        let k = (2. / 3.) * (u[0] * v[1] - u[1] * v[0]) / (u[0] * u[0] + u[1] * u[1]).powf(1.5);
        if end { -k } else { k }
    }

    #[test]
    fn smooth_corner_cubics_are_monotone_symmetric_and_g2() {
        let segments = corner_cubics();
        assert_eq!(segments[0][0], [1., 0.]);
        assert_eq!(segments[7][3], [0., 1.]);
        assert_eq!(curvature(segments[0], false), 0.);
        assert_eq!(curvature(segments[7], true), 0.);
        for (i, segment) in segments.iter().enumerate() {
            for j in 0..4 {
                assert_eq!(
                    segment[j],
                    [segments[7 - i][3 - j][1], segments[7 - i][3 - j][0]]
                );
                if j > 0 {
                    assert!(
                        segment[j][0] <= segment[j - 1][0] && segment[j][1] >= segment[j - 1][1]
                    );
                }
            }
            if i > 0 {
                let previous = segments[i - 1];
                assert_eq!(previous[3], segment[0]);
                let a = [
                    previous[3][0] - previous[2][0],
                    previous[3][1] - previous[2][1],
                ];
                let b = [segment[1][0] - segment[0][0], segment[1][1] - segment[0][1]];
                assert!((a[0] * b[1] - a[1] * b[0]).abs() < 1e-7);
                assert!((curvature(previous, true) - curvature(*segment, false)).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn smooth_corner_cubics_follow_the_analytic_boundary_with_subpixel_error() {
        let mut maximum_error = 0f64;
        for segment in corner_cubics() {
            for i in 0..=10000 {
                let t = f64::from(i) / 10000.;
                let u = 1. - t;
                let weights = [u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t];
                let p: [f64; 2] = std::array::from_fn(|axis| {
                    segment
                        .iter()
                        .zip(weights)
                        .map(|(p, w)| f64::from(p[axis]) * w)
                        .sum()
                });
                // Project along the nearer axis, avoiding the derivative singularity
                // at an endpoint. Each maximum coordinate error is a distance bound.
                let (x, y) = if p[0] > p[1] {
                    (p[0], p[1])
                } else {
                    (p[1], p[0])
                };
                let error = (x - (1. - y.powi(4)).max(0.).powf(0.25)).abs() * 128.;
                maximum_error = maximum_error.max(error);
            }
        }
        println!("SMOOTH_CORNER maximum_coordinate_error_at_128px={maximum_error}");
        assert!(maximum_error < 0.01, "{maximum_error}");
    }
}
