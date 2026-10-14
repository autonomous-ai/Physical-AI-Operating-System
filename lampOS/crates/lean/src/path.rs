//! Whole planned paths use conservative bounds, not just safe keyframes.
use crate::{Angles, Error, Model, NUMERICAL_RESERVE_M, PoseCheck, invalid};
use lamp_msgs::BodyOffsets;
use serde::Serialize;

pub const MAX_STEPS: usize = 10_000;
pub const MAX_EVALUATIONS: usize = 100_000;
pub const MAX_DEPTH: usize = 20;
pub const MAX_MOVEMENT_US: u64 = 600_000_000;
pub const MAX_CLIP_US: u64 = 10_000_000;
pub const MAX_KEYFRAMES: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub at_us: u64,
    pub angles: Angles,
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetFrame {
    pub at_us: u64,
    pub offsets: BodyOffsets,
}

/// A single authored Expression clip on a fixed base pose. Posture and clip
/// offsets are added, weighted and clamped in Expression's order. The mask
/// selects exactly the joints the eventual owner applies. It is never guessed.
/// Concurrent behaviours, gaze, transitions and a moving base pose must instead
/// be composed by their owner and passed as final steps to check_movement.
#[derive(Clone, Copy, Debug)]
pub struct Clip<'a> {
    pub posture: BodyOffsets,
    pub frames: &'a [OffsetFrame],
    pub weight: f64,
    pub limits: BodyOffsets,
    pub applied: [bool; 5],
    pub repeat: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    WithinMargin,
    OutsideMargin,
    /// No outside sample was found, but the work/numerical bound did not
    /// establish that the whole authored path stays within the margin.
    Unresolved,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PoseAt {
    /// May be fractional during adaptive checks between authored microseconds.
    pub at_us: f64,
    pub angles: Angles,
    pub check: PoseCheck,
}

#[derive(Clone, Debug, Serialize)]
pub struct MovementCheck {
    pub verdict: Verdict,
    /// Includes every supplied step (or clip grid/keyframe time).
    pub checked_steps: usize,
    /// Includes additional midpoint evaluations, bounded by MAX_EVALUATIONS.
    pub evaluated_poses: usize,
    pub worst_sample: PoseAt,
    /// A conservative lower bound for the entire declared interpolation.
    /// A negative bound alone is not evidence of an actual outside pose.
    pub minimum_clearance_bound_m: f64,
}

impl MovementCheck {
    pub fn within_margin(&self) -> bool {
        self.verdict == Verdict::WithinMargin
    }
}

struct Audit<'a> {
    model: &'a Model,
    evaluated: usize,
    worst: PoseAt,
    outside: bool,
}

impl<'a> Audit<'a> {
    fn new(model: &'a Model, first: PoseAt) -> Self {
        Self {
            model,
            evaluated: 1,
            worst: first,
            outside: first.check.clearance_m < 0.0,
        }
    }

    fn observe(&mut self, at_us: f64, angles: Angles) -> PoseAt {
        let sample = at(self.model, at_us, angles);
        self.evaluated += 1;
        if sample.check.clearance_m < self.worst.check.clearance_m {
            self.worst = sample;
        }
        self.outside |= sample.check.clearance_m < 0.0;
        sample
    }

    fn interval(
        &mut self,
        start: PoseAt,
        end: PoseAt,
        depth: usize,
        evaluate: &impl Fn(f64) -> Result<Angles, Error>,
    ) -> Result<f64, Error> {
        let a = start.angles.radians();
        let b = end.angles.radians();
        let travel: f64 = a
            .iter()
            .zip(b)
            .zip(self.model.radii)
            .map(|((&a, b), radius)| radius * (b - a).abs())
            .sum();
        // Every point in a coordinatewise-monotone endpoint box is within
        // half the total weighted L1 travel of one endpoint. Rotation moves
        // a downstream COM no faster than its radius * angle; signed distance
        // to the inset polygon is 1-Lipschitz. This bounds its entire path.
        let bound = start.check.clearance_m.min(end.check.clearance_m) - travel * 0.5;
        if bound > NUMERICAL_RESERVE_M
            || self.outside
            || travel == 0.0
            || depth == MAX_DEPTH
            || self.evaluated >= MAX_EVALUATIONS
        {
            return Ok(bound);
        }
        let middle = start.at_us + (end.at_us - start.at_us) * 0.5;
        if middle == start.at_us || middle == end.at_us {
            return Ok(bound);
        }
        // The true time midpoint is essential for a clamped clip. Averaging
        // its endpoint joint angles can invent a different trajectory.
        let sample = self.observe(middle, evaluate(middle)?);
        let left = self.interval(start, sample, depth + 1, evaluate)?;
        let right = self.interval(sample, end, depth + 1, evaluate)?;
        Ok(left.min(right))
    }

    fn finish(self, checked_steps: usize, bound: f64) -> MovementCheck {
        let verdict = if self.outside {
            Verdict::OutsideMargin
        } else if bound > NUMERICAL_RESERVE_M {
            Verdict::WithinMargin
        } else {
            Verdict::Unresolved
        };
        MovementCheck {
            verdict,
            checked_steps,
            evaluated_poses: self.evaluated,
            worst_sample: self.worst,
            minimum_clearance_bound_m: bound,
        }
    }
}

fn at(model: &Model, at_us: f64, angles: Angles) -> PoseAt {
    PoseAt {
        at_us,
        angles,
        check: model.check_pose(angles),
    }
}

fn blend(a: Angles, b: Angles, share: f64) -> Result<Angles, Error> {
    let a = a.radians();
    let b = b.radians();
    Angles::new(std::array::from_fn(|i| a[i] * (1.0 - share) + b[i] * share))
}

impl Model {
    /// Checks every final planned step and the joint-linear paths between them.
    /// A common monotone easing of all five joints has the same geometric path.
    /// This is not a claim about independently lagging servos or unsampled
    /// commands; the hardware owner must check their actual final composition.
    pub fn check_movement(&self, steps: &[Step]) -> Result<MovementCheck, Error> {
        if steps.is_empty()
            || steps.len() > MAX_STEPS
            || steps[0].at_us != 0
            || steps.last().unwrap().at_us > MAX_MOVEMENT_US
            || steps.windows(2).any(|pair| pair[0].at_us >= pair[1].at_us)
        {
            return Err(invalid(
                "movement needs 1..10000 steps, increasing from zero, at most 600 seconds",
            ));
        }
        let first = at(self, 0.0, steps[0].angles);
        let mut audit = Audit::new(self, first);
        // Validate all requested poses before adaptive work. A sampled outside
        // pose never prevents the remaining prescribed steps from being checked.
        let mut samples = Vec::with_capacity(steps.len());
        samples.push(first);
        for step in &steps[1..] {
            samples.push(audit.observe(step.at_us as f64, step.angles));
        }
        let mut bound = audit.worst.check.clearance_m;
        for pair in samples.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            let evaluate =
                |time: f64| blend(a.angles, b.angles, (time - a.at_us) / (b.at_us - a.at_us));
            bound = bound.min(audit.interval(a, b, 0, &evaluate)?);
        }
        Ok(audit.finish(steps.len(), bound))
    }

    /// Checks a single clip's complete smoothstep curve, not only its keys.
    /// Every grid step and keyframe is included. Each interval stays within
    /// one authored keyframe span, where every composed/clamped joint is
    /// monotone, allowing the analytic bound above. A repeated clip must close.
    pub fn check_clip(
        &self,
        base: Angles,
        clip: Clip<'_>,
        sample_every_us: u64,
    ) -> Result<MovementCheck, Error> {
        clip.validate()?;
        if !(1..=100_000).contains(&sample_every_us) {
            return Err(invalid("clip sample step must be 1..100000 microseconds"));
        }
        let duration = clip.frames.last().unwrap().at_us;
        let grid_count = duration.div_ceil(sample_every_us) + 1;
        if grid_count + clip.frames.len() as u64 > MAX_STEPS as u64 {
            return Err(invalid(
                "clip grid and keyframes exceed the 10000-step work bound",
            ));
        }
        let mut times: Vec<_> = (0..grid_count)
            .map(|n| (n * sample_every_us).min(duration))
            .chain(clip.frames.iter().map(|frame| frame.at_us))
            .collect();
        times.sort_unstable();
        times.dedup();
        let evaluate = |time: f64| clip.angles_at(base, time);
        let first = at(self, 0.0, evaluate(0.0)?);
        let mut audit = Audit::new(self, first);
        let mut samples = Vec::with_capacity(times.len());
        samples.push(first);
        for time in &times[1..] {
            samples.push(audit.observe(*time as f64, evaluate(*time as f64)?));
        }
        let mut bound = audit.worst.check.clearance_m;
        for pair in samples.windows(2) {
            bound = bound.min(audit.interval(pair[0], pair[1], 0, &evaluate)?);
        }
        Ok(audit.finish(times.len(), bound))
    }
}

impl Clip<'_> {
    fn validate(&self) -> Result<(), Error> {
        for offsets in [self.posture, self.limits] {
            offsets
                .check()
                .map_err(|error| invalid(error.to_string()))?;
        }
        if self.limits.values().iter().any(|limit| *limit < 0.0)
            || !self.weight.is_finite()
            || !(0.0..=1.0).contains(&self.weight)
        {
            return Err(invalid(
                "clip limits must be nonnegative and weight must be 0..1",
            ));
        }
        if self.frames.len() < 2
            || self.frames.len() > MAX_KEYFRAMES
            || self.frames[0].at_us != 0
            || self.frames.last().unwrap().at_us > MAX_CLIP_US
            || self
                .frames
                .windows(2)
                .any(|pair| pair[0].at_us >= pair[1].at_us)
        {
            return Err(invalid(
                "clip needs 2..64 increasing keyframes from zero, at most 10 seconds",
            ));
        }
        for frame in self.frames {
            frame
                .offsets
                .check()
                .map_err(|error| invalid(error.to_string()))?;
        }
        if self.repeat && self.frames[0].offsets != self.frames.last().unwrap().offsets {
            return Err(invalid("a repeating clip must end at its initial offsets"));
        }
        Ok(())
    }

    fn angles_at(&self, base: Angles, time: f64) -> Result<Angles, Error> {
        let index = self
            .frames
            .partition_point(|frame| (frame.at_us as f64) < time);
        let offsets = if index == 0 {
            self.frames[0].offsets.values()
        } else if index >= self.frames.len() {
            self.frames.last().unwrap().offsets.values()
        } else {
            let a = self.frames[index - 1];
            let b = self.frames[index];
            let share = (time - a.at_us as f64) / (b.at_us - a.at_us) as f64;
            let eased = share * share * (3.0 - 2.0 * share);
            let a = a.offsets.values();
            let b = b.offsets.values();
            std::array::from_fn(|i| a[i] * (1.0 - eased) + b[i] * eased)
        };
        let base = base.radians();
        let posture = self.posture.values();
        let limits = self.limits.values();
        Angles::new(std::array::from_fn(|i| {
            let offset = if self.applied[i] {
                (posture[i] + offsets[i] * self.weight).clamp(-limits[i], limits[i])
            } else {
                0.0
            };
            base[i] + offset
        }))
    }
}
