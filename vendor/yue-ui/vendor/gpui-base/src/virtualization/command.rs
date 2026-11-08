//! Bounded convergence shared by one-dimensional and masonry positioning.
use super::{VirtualizerMotion, VirtualizerScrollOptions};
use std::time::{Duration, Instant};

/// Sample once per frame time, even when measurement re-resolves geometry in
/// that frame. Callers keep the native owner as the only applied position.
pub(super) fn sample_offset(
    options: VirtualizerScrollOptions,
    elapsed: Duration,
    from: f64,
    destination: f64,
    current: f64,
    extent: f64,
) -> (f64, bool) {
    if let VirtualizerMotion::Smooth {
        duration,
        max_distance,
    } = options.motion
        && !options.reduced_motion
        && (destination - current).abs() > 0.01
        && (destination - from).abs() <= f64::from(max_distance).min(extent)
    {
        let t = (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        (from + (destination - from) * eased, t >= 1.0)
    } else {
        (destination, true)
    }
}

#[derive(Default)]
pub(super) struct CommandProgress {
    started: Option<Instant>,
    attempts: u8,
    last: Option<f64>,
    stalled: u8,
}
impl CommandProgress {
    pub fn advance(&mut self, now: Instant) -> bool {
        self.advance_with_motion(now, false)
    }
    /// Timed animation samples are not failed geometry retries. The wall-clock
    /// deadline still bounds them, independently of the display refresh rate.
    pub fn advance_with_motion(&mut self, now: Instant, animating: bool) -> bool {
        let started = *self.started.get_or_insert(now);
        self.attempts = self.attempts.saturating_add(u8::from(!animating));
        self.attempts > 32 || now.saturating_duration_since(started) > Duration::from_secs(2)
    }
    pub fn started(&self) -> Option<Instant> {
        self.started
    }
    pub fn confirm(&mut self, applied: f64, settled: bool) -> bool {
        if !settled && self.last.is_some_and(|old| (old - applied).abs() < 0.01) {
            self.stalled = self.stalled.saturating_add(1);
        } else {
            self.stalled = 0;
        }
        self.last = Some(applied);
        self.stalled >= 8
    }
}

/// Optional state only for Measured smooth requests. Re-resolution within one
/// frame returns one sample. Geometry changes rebase at the actual owner offset
/// with the original deadline, rather than restarting from an obsolete origin.
#[derive(Default)]
pub(super) struct MeasuredMotion {
    sampled_at: Option<Instant>,
    segment_start: Option<Instant>,
    deadline: Option<Instant>,
    from: f64,
    destination: f64,
    sample: f64,
    complete: bool,
    disabled: bool,
    tail_pending: bool,
    advanced: bool,
    geometry_revision: u64,
}
pub(super) struct MotionGeometry {
    pub current: f64,
    pub destination: f64,
    pub extent: f64,
    pub known: bool,
    pub revision: u64,
}
impl MeasuredMotion {
    pub fn is_animating(&self, now: Instant) -> bool {
        !self.disabled && (self.tail_pending || self.deadline.is_none_or(|end| now <= end))
    }
    pub fn sample(
        &mut self,
        options: VirtualizerScrollOptions,
        now: Instant,
        geometry: MotionGeometry,
    ) -> (f64, bool, bool) {
        let MotionGeometry {
            current,
            destination,
            extent,
            known,
            revision,
        } = geometry;
        if options.reduced_motion || (destination - current).abs() <= 0.01 {
            self.disabled = true;
            return (destination, true, false);
        }
        let (duration, max_distance, jump) = match options.motion {
            VirtualizerMotion::Smooth {
                duration,
                max_distance,
            } => (duration, f64::from(max_distance).min(extent), false),
            VirtualizerMotion::JumpThenSmooth {
                duration,
                max_distance,
            } => (duration, f64::from(max_distance).min(extent), true),
            VirtualizerMotion::Instant => return (destination, true, false),
        };
        if self.disabled {
            return (destination, true, false);
        }
        if self.sampled_at == Some(now) {
            // Animation sampling is frozen, final alignment is not. A second
            // measurement pass at the deadline can expose a new actual target.
            if self.complete {
                self.sample = destination;
            }
            return (self.sample, self.complete, self.advanced);
        }
        let first = self.sampled_at.is_none();
        self.advanced = self.sampled_at.is_some_and(|old| now > old);
        self.sampled_at = Some(now);
        if first && (destination - current).abs() > max_distance && jump && max_distance > 0. {
            self.tail_pending = true;
            self.sample = destination - (destination - current).signum() * max_distance * 0.5;
            self.complete = false;
            return (self.sample, false, false);
        }
        // The short measured path is a per-sample constraint: resize or
        // invalidation can revoke it after a valid animation has started.
        if !known || extent <= 0. || (destination - current).abs() > max_distance {
            self.disabled = true;
            return (destination, true, false);
        }
        if first || self.tail_pending {
            self.tail_pending = false;
            self.from = current;
            self.destination = destination;
            self.segment_start = Some(now);
            self.deadline = Some(now + duration);
        } else if (destination - self.destination).abs() > 0.01
            || revision != self.geometry_revision && (current - self.sample).abs() > 0.5
        {
            self.from = current;
            self.destination = destination;
            self.segment_start = Some(now);
        }
        self.geometry_revision = revision;
        let end = self.deadline.expect("motion deadline");
        self.complete = now >= end;
        if self.complete {
            self.sample = destination;
        } else {
            let start = self.segment_start.unwrap();
            let t = (now.saturating_duration_since(start).as_secs_f64()
                / end.saturating_duration_since(start).as_secs_f64())
            .clamp(0., 1.);
            self.sample = self.from + (destination - self.from) * (1. - (1. - t).powi(3));
        }
        (self.sample, self.complete, self.advanced && !self.complete)
    }
}
