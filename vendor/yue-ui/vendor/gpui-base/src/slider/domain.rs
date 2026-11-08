use super::SliderValue;

/// An atomic numeric domain. Stepping is anchored at `min`; `max`
/// remains reachable even when the final interval is shorter than a step.
/// Bounds are checked by `new`; step representability is checked by `step`
/// and before stepping or attaching the domain to a SliderState.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderDomain {
    min: f32,
    max: f32,
    step: f32,
}

impl Default for SliderDomain {
    fn default() -> Self {
        Self::new(0., 100.)
    }
}

impl SliderDomain {
    /// # Panics
    /// Panics unless both bounds are finite and `min < max`.
    pub fn new(min: f32, max: f32) -> Self {
        assert!(
            min.is_finite() && max.is_finite(),
            "slider bounds must be finite"
        );
        assert!(min < max, "slider min must be less than max");
        Self { min, max, step: 1. }
    }

    /// # Panics
    /// Panics if the step is non-positive, non-finite, or cannot express distinct
    /// adjacent values in this f32 domain. Steps larger than the span are valid.
    pub fn step(mut self, step: f32) -> Self {
        assert!(
            step.is_finite() && step > 0.,
            "slider step must be finite and positive"
        );
        let (a, b, s) = (decimal(self.min), decimal(self.max), decimal(step));
        let count = ((b - a) / s).floor();
        if count >= 1. {
            assert!(
                (a + s) as f32 > self.min,
                "slider step cannot express distinct values near min"
            );
            assert!(
                (a + count * s) as f32 > (a + (count - 1.) * s) as f32,
                "slider step cannot express distinct values near max"
            );
        }
        self.step = step;
        self
    }

    pub fn min(self) -> f32 {
        self.min
    }
    pub fn max(self) -> f32 {
        self.max
    }
    pub fn step_value(self) -> f32 {
        self.step
    }

    /// Clamp and order a programmatic value, preserving its precision.
    /// # Panics
    /// Panics for NaN or infinite input.
    pub fn clamp(self, value: impl Into<SliderValue>) -> SliderValue {
        let value = value.into();
        assert!(
            value.start().is_finite() && value.end().is_finite(),
            "slider value must be finite"
        );
        value.clamp(self.min, self.max)
    }

    /// Snap a finite value to the nearest legal step or endpoint. Ties go up.
    /// # Panics
    /// Panics for NaN or infinite input, or an unrepresentable implicit default step.
    pub fn snap(self, value: f32) -> f32 {
        assert!(value.is_finite(), "slider value must be finite");
        self.step(self.step);
        let (a, b, s) = (decimal(self.min), decimal(self.max), decimal(self.step));
        let x = decimal(value).clamp(a, b);
        let lower = (a + ((x - a) / s).floor() * s).clamp(a, b);
        let upper = (lower + s).min(b);
        // Account only for f64 arithmetic roundoff, not a fixed application
        // epsilon that could erase valid f32 steps. Decimal midpoint ties go up.
        // Grid arithmetic accumulates roundoff across the bounded domain. An
        // oversize step can be arbitrarily larger than that domain; scaling
        // tolerance by it could otherwise turn even an exact min into max.
        let roundoff = f64::EPSILON * 4. * (b - a);
        (if x - lower + roundoff < upper - x {
            lower
        } else {
            upper
        }) as f32
    }

    /// Snap, clamp and order a value using the same rule as pointer input.
    pub fn normalize(self, value: impl Into<SliderValue>) -> SliderValue {
        match value.into() {
            SliderValue::Single(v) => SliderValue::Single(self.snap(v)),
            SliderValue::Range(a, b) => {
                SliderValue::Range(self.snap(a), self.snap(b)).clamp(self.min, self.max)
            }
        }
    }

    /// The next distinct legal value, including a non-divisible max endpoint.
    pub fn adjacent(self, value: f32, increase: bool) -> f32 {
        self.step(self.step);
        let value = self.clamp(value).end();
        let (a, b, s, x) = (
            decimal(self.min),
            decimal(self.max),
            decimal(self.step),
            decimal(value),
        );
        let q = (x - a) / s;
        if increase {
            let mut k = q.floor() + 1.;
            if ((a + k * s).min(b) as f32) <= value {
                k += 1.;
            }
            (a + k * s).min(b) as f32
        } else {
            let mut k = q.ceil() - 1.;
            if ((a + k * s).max(a) as f32) >= value {
                k -= 1.;
            }
            (a + k * s).max(a) as f32
        }
    }
}

// Interpret the public f32's shortest round-trip decimal before grid arithmetic.
// Widening binary 0.1f32 directly would make the ninth point 0.90000004f32.
pub(super) fn decimal(value: f32) -> f64 {
    value
        .to_string()
        .parse()
        .expect("finite f32 has a decimal representation")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonzero_origin_and_final_endpoint() {
        assert_eq!(SliderDomain::new(3., 13.).step(2.).snap(6.), 7.);
        let d = SliderDomain::new(0., 10.).step(3.);
        assert_eq!(d.snap(9.8), 10.);
        assert_eq!(d.adjacent(9., true), 10.);
        assert_eq!(d.adjacent(10., false), 9.);
        assert_eq!(SliderDomain::new(-5., 5.).step(2.).snap(-2.), -1.);
    }
    #[test]
    fn decimal_and_oversize_steps_do_not_stick() {
        let d = SliderDomain::new(0., 1.).step(0.1);
        assert_eq!(d.snap(0.26), 0.3);
        assert_eq!(d.snap(0.35), 0.4);
        assert_eq!(d.adjacent(0.7, true), 0.8);
        assert_eq!(d.adjacent(0.3, false), 0.2);
        let d = SliderDomain::new(0., 10.).step(20.);
        assert_eq!(d.snap(6.), 10.);
        assert_eq!(d.adjacent(0., true), 10.);
        assert_eq!(d.adjacent(10., false), 0.);
        let d = SliderDomain::new(0., 10.).step(f32::MAX);
        assert_eq!(d.snap(0.), 0.);
        assert_eq!(d.snap(4.), 0.);
        assert_eq!(d.snap(5.), 10.);
        assert_eq!(d.snap(10.), 10.);
    }
}
