//! Yue's motion, read the way the web reads it.
//!
//! Durations come from one 50ms-stepped ladder, `--duration-100` to
//! `--duration-400`. On top of it sit the semantic roles — [`MotionDuration`]
//! and [`MotionEase`] — that a component reaches for first. A component whose
//! layout calls for a different pace may name a ladder rung instead, or derive
//! a special value from one, and says why in its token; it never writes a raw
//! literal (`apps/docs/design/motion.md` in the design system).
//!
//! A value that a gesture releases or that is retargeted mid-flight runs on a
//! spring instead: [`settle_spring`] keeps its velocity across the change,
//! which a duration-based transition cannot.
//!
//! Reduced motion is not a token here: GPUI reads the OS setting and settles
//! every Base transition and spring at once, so the roles keep their values.

use std::time::Duration;

use gpui_kit::base::{Easing, LinearStop, Spring, Transition};

use crate::YueTheme;
use crate::tokens::{cubic_bezier, duration, easing, linear_stops};

/// How long a motion runs, by what moves and how much of the screen it covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionDuration {
    /// In-component feedback: hover, press, colour, border, an icon swap.
    Interaction,
    /// A small surface or mark entering: popover, dropdown, a check drawn.
    Enter,
    /// The same leaving — shorter, because the user has already decided.
    Exit,
    /// A surface that covers the page entering: dialog, drawer.
    EnterLarge,
    /// The same leaving.
    ExitLarge,
    /// A route or a full-screen view replacing the page.
    EnterPage,
    /// The same leaving.
    ExitPage,
    /// One turn of an indeterminate indicator; exempt from the 400ms ceiling.
    Loop,
    /// One sweep of an indeterminate progress bar across its track.
    LoopTrack,
    /// How long [`settle_spring`] takes to come to rest.
    Settle,
}

impl MotionDuration {
    pub const ALL: [Self; 10] = [
        Self::Interaction,
        Self::Enter,
        Self::Exit,
        Self::EnterLarge,
        Self::ExitLarge,
        Self::EnterPage,
        Self::ExitPage,
        Self::Loop,
        Self::LoopTrack,
        Self::Settle,
    ];

    pub fn token_name(self) -> &'static str {
        match self {
            Self::Interaction => "--motion-duration-interaction",
            Self::Enter => "--motion-duration-enter",
            Self::Exit => "--motion-duration-exit",
            Self::EnterLarge => "--motion-duration-enter-large",
            Self::ExitLarge => "--motion-duration-exit-large",
            Self::EnterPage => "--motion-duration-enter-page",
            Self::ExitPage => "--motion-duration-exit-page",
            Self::Loop => "--motion-duration-loop",
            Self::LoopTrack => "--motion-duration-loop-track",
            Self::Settle => "--motion-duration-settle",
        }
    }

    pub fn duration(self, theme: YueTheme) -> Duration {
        duration(theme, self.token_name())
    }
}

/// The shape of a motion over its duration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionEase {
    /// Moving or reshaping within the screen.
    Standard,
    /// Entering view: decelerates into place.
    Enter,
    /// Leaving view: accelerates away.
    Exit,
    /// Leaving view but staying recallable — a side panel collapsed to its
    /// rail, an accordion folding — so it does not accelerate off for good.
    Stow,
    /// An opacity ramp that accompanies another motion, at a constant rate.
    Fade,
    /// The expressive tier, for a page arriving or an empty state: harder
    /// acceleration and deceleration than anything opened all day.
    ExpressiveStandard,
    ExpressiveEnter,
    ExpressiveExit,
}

impl MotionEase {
    pub const ALL: [Self; 8] = [
        Self::Standard,
        Self::Enter,
        Self::Exit,
        Self::Stow,
        Self::Fade,
        Self::ExpressiveStandard,
        Self::ExpressiveEnter,
        Self::ExpressiveExit,
    ];

    /// The productive curves a component reaches for by default.
    pub const PRODUCTIVE: [Self; 5] = [
        Self::Standard,
        Self::Enter,
        Self::Exit,
        Self::Stow,
        Self::Fade,
    ];

    /// The expressive tier, paired with the page roles.
    pub const EXPRESSIVE: [Self; 3] = [
        Self::ExpressiveStandard,
        Self::ExpressiveEnter,
        Self::ExpressiveExit,
    ];

    pub fn token_name(self) -> &'static str {
        match self {
            Self::Standard => "--motion-ease-standard",
            Self::Enter => "--motion-ease-enter",
            Self::Exit => "--motion-ease-exit",
            Self::Stow => "--motion-ease-stow",
            Self::Fade => "--motion-ease-fade",
            Self::ExpressiveStandard => "--motion-ease-expressive-standard",
            Self::ExpressiveEnter => "--motion-ease-expressive-enter",
            Self::ExpressiveExit => "--motion-ease-expressive-exit",
        }
    }

    /// The curve's `cubic-bezier()` control points; `linear` reads as
    /// `(0, 0, 1, 1)`.
    pub fn control_points(self, theme: YueTheme) -> (f32, f32, f32, f32) {
        cubic_bezier(theme, self.token_name())
    }

    /// The curve as a progress remapper: elapsed fraction in, progress out.
    pub fn easing(self, theme: YueTheme) -> Box<dyn Fn(f32) -> f32> {
        easing(theme, self.token_name())
    }
}

/// A Base transition that runs `length` along `ease`.
pub fn transition(theme: YueTheme, length: MotionDuration, ease: MotionEase) -> Transition {
    Transition::new(length.duration(theme)).ease(ease.easing(theme))
}

/// Shared surface policy for Dropdown and Select, including collapsed tags.
pub(crate) fn dropdown_popup(theme: YueTheme) -> crate::popup::PopupMotion {
    use crate::theme::keys;
    let enter = Transition::new(duration(theme, keys::POPOVER_DURATION_ENTER))
        .ease(easing(theme, keys::MOTION_EASE_ENTER));
    let exit = Transition::new(duration(theme, keys::POPOVER_DURATION_EXIT))
        .ease(easing(theme, keys::MOTION_EASE_EXIT));
    crate::popup::PopupMotion::new(enter, exit).entrance_distance(gpui_kit::px(4.))
}

/// The decay rate, per settle time, of the critically damped spring both sides
/// share: `1 - (1 + 9.23t)e^(-9.23t)` is within 0.1% of its target at `t = 1`.
pub const SETTLE_RATE: f32 = 9.23;

/// The spring for a value that a gesture releases or that is retargeted while
/// it moves: a slider thumb let go, a sheet dropped, a hover chased across a
/// list. Critically damped, so it never overshoots, and it comes to rest at
/// [`MotionDuration::Settle`].
///
/// Base's `Spring::new` takes the undamped period rather than a settle time;
/// a critically damped spring of angular frequency `ω` reaches 99.9% at
/// `SETTLE_RATE / ω`, so the period is `settle × 2π / SETTLE_RATE`.
pub fn settle_spring(theme: YueTheme) -> Spring {
    Spring::new(settle_response(theme))
}

/// The undamped period [`settle_spring`] hands to `Spring::new`.
pub fn settle_response(theme: YueTheme) -> Duration {
    MotionDuration::Settle
        .duration(theme)
        .mul_f32(std::f32::consts::TAU / SETTLE_RATE)
}

/// The same spring's shape as an easing, read from `--motion-ease-spring`.
///
/// This is what the web runs, since CSS cannot carry velocity into a curve;
/// on this side prefer [`settle_spring`] and keep this for plotting or for a
/// one-shot motion that has nothing to inherit.
pub fn spring_curve(theme: YueTheme) -> Easing {
    Easing::linear_stops(
        linear_stops(theme, crate::theme::keys::MOTION_EASE_SPRING)
            .into_iter()
            .map(LinearStop::new),
    )
    .expect("--motion-ease-spring holds valid linear() stops")
}

/// The steps of the duration ladder, in milliseconds.
pub const LADDER: [u32; 7] = [100, 150, 200, 250, 300, 350, 400];

/// The ladder rung `--duration-{step}`.
///
/// # Panics
///
/// When `step` is not one of [`LADDER`].
pub fn rung(theme: YueTheme, step: u32) -> Duration {
    let name = match step {
        100 => "--duration-100",
        150 => "--duration-150",
        200 => "--duration-200",
        250 => "--duration-250",
        300 => "--duration-300",
        350 => "--duration-350",
        400 => "--duration-400",
        _ => panic!("{step}ms is not a rung of the duration ladder"),
    };
    duration(theme, name)
}

/// Any duration token, such as a component's `--menu-item-duration`.
///
/// # Panics
///
/// If `name` is not a token, or is not a non-negative millisecond value.
pub fn token_duration(theme: YueTheme, name: &'static str) -> Duration {
    duration(theme, name)
}

/// Any curve token's control points, such as a component's `--menu-item-ease`.
///
/// # Panics
///
/// If `name` is not a token, or is not a `cubic-bezier()` / `linear` curve.
pub fn token_curve(theme: YueTheme, name: &'static str) -> (f32, f32, f32, f32) {
    cubic_bezier(theme, name)
}

#[cfg(test)]
mod tests {
    use super::{
        LADDER, MotionDuration, MotionEase, SETTLE_RATE, rung, settle_response, spring_curve,
    };
    use crate::tokens::linear_stops;
    use crate::{Accent, ThemeMode, YueTheme};
    use std::time::Duration;

    fn theme() -> YueTheme {
        YueTheme::new(ThemeMode::Light, Accent::Azure)
    }

    /// The table `motion.md` documents, kept here so a retune on the web fails
    /// until the docs and this side agree.
    #[test]
    fn roles_resolve_to_the_documented_values() {
        let theme = theme();
        let millis: Vec<_> = MotionDuration::ALL
            .into_iter()
            .map(|role| role.duration(theme).as_millis())
            .collect();
        assert_eq!(millis, [100, 200, 100, 250, 150, 400, 250, 900, 1800, 300]);
        let curves: Vec<_> = MotionEase::ALL
            .into_iter()
            .map(|ease| ease.control_points(theme))
            .collect();
        assert_eq!(
            curves,
            [
                (0.2, 0.0, 0.38, 0.9),
                (0.0, 0.0, 0.38, 0.9),
                (0.2, 0.0, 1.0, 0.9),
                (0.2, 0.0, 0.38, 0.9),
                (0.0, 0.0, 1.0, 1.0),
                (0.4, 0.14, 0.3, 1.0),
                (0.0, 0.0, 0.3, 1.0),
                (0.4, 0.14, 1.0, 1.0),
            ]
        );
    }

    #[test]
    fn leaving_is_shorter_than_entering() {
        let theme = theme();
        let pairs = [
            (MotionDuration::Enter, MotionDuration::Exit),
            (MotionDuration::EnterLarge, MotionDuration::ExitLarge),
            (MotionDuration::EnterPage, MotionDuration::ExitPage),
        ];
        for (enter, exit) in pairs {
            let ratio = exit.duration(theme).as_secs_f32() / enter.duration(theme).as_secs_f32();
            assert!(
                (0.5..=0.75).contains(&ratio),
                "{enter:?}/{exit:?} is {ratio}"
            );
        }
        assert!(MotionDuration::EnterLarge.duration(theme) > MotionDuration::Enter.duration(theme));
        assert!(
            MotionDuration::EnterPage.duration(theme) > MotionDuration::EnterLarge.duration(theme)
        );
    }

    #[test]
    fn the_ladder_steps_by_fifty() {
        let theme = theme();
        for step in LADDER {
            assert_eq!(rung(theme, step), Duration::from_millis(step.into()));
        }
    }

    #[test]
    fn fade_is_linear() {
        let fade = MotionEase::Fade.easing(theme());
        for step in 0..=10 {
            let time = step as f32 / 10.0;
            assert!((fade(time) - time).abs() < 1e-4);
        }
    }

    /// The web's `linear()` stops and this side's spring must describe one
    /// motion: the stops sample the critically damped response, and the
    /// spring's period is the one that settles at `--motion-duration-settle`.
    #[test]
    fn the_spring_curve_and_the_spring_agree() {
        let theme = theme();
        let stops = linear_stops(theme, crate::theme::keys::MOTION_EASE_SPRING);
        let last = (stops.len() - 1) as f32;
        for (index, stop) in stops.iter().enumerate() {
            let time = index as f32 / last;
            let expected = 1.0 - (1.0 + SETTLE_RATE * time) * (-SETTLE_RATE * time).exp();
            assert!(
                (stop - expected).abs() < 2e-3,
                "stop {index}: {stop} vs {expected}"
            );
        }
        assert_eq!(stops.last(), Some(&1.0));
        let curve = spring_curve(theme);
        assert!((curve.sample(0.5) - stops[stops.len() / 2]).abs() < 1e-3);

        // ω = 2π / response, and the response settles at SETTLE_RATE / ω.
        let settle = MotionDuration::Settle.duration(theme).as_secs_f32();
        let omega = std::f32::consts::TAU / settle_response(theme).as_secs_f32();
        assert!((SETTLE_RATE / omega - settle).abs() < 1e-4);
    }
}
