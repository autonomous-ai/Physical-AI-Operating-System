//! The hardware team's touch calibration for the base strip (#711), run by
//! hand at the lamp: the chip calibrates itself with nobody touching, each
//! pad's noise is measured, a person swipes each pad a few times, and per-pad
//! touch and release thresholds are set between the noise and the weakest
//! swipe, and saved.
//!
//! Nothing here runs unless `lamp-controls calibrate-touch` is started. It
//! serves nothing and publishes nothing. The rule is authored, not a
//! datasheet value; its numbers are in every report. See `docs/controls.md`.

use crate::mpr121::{self, Charge, Profile, Signals};
use serde::Serialize;

/// A delta must pass the pad's noise by more than this to count as touch.
pub const CONTACT_ABOVE_NOISE: u16 = 2;
/// A swipe over a pad is at least this many readings in a row above that.
pub const CONTACT_READINGS: u32 = 3;
/// The weakest swipe must pass the noise by at least this much, so that the
/// release threshold is above the noise and the touch threshold above both.
pub const MIN_GAP: u16 = 5;

/// How long each step lasts, and where the thresholds go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// Seconds, hands away, while the chip starts and auto-calibrates.
    pub settle: u64,
    /// Seconds, hands away still: each pad's noise is measured.
    pub quiet: u64,
    /// Seconds in which each working pad is swiped a few times.
    pub swipe: u64,
    /// Touch at this percent of the way from the noise to the weakest swipe.
    pub touch_percent: u8,
    /// Release at this percent of the same way. Below `touch_percent`.
    pub release_percent: u8,
}

impl Default for Plan {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Plan {
    pub const DEFAULT: Self = Self {
        settle: 10,
        quiet: 5,
        swipe: 30,
        touch_percent: 40,
        release_percent: 20,
    };

    pub fn check(&self) -> Result<(), String> {
        if !(1..=600).contains(&self.settle)
            || !(1..=600).contains(&self.quiet)
            || !(1..=600).contains(&self.swipe)
        {
            return Err("each calibration step lasts 1..600 seconds".into());
        }
        if !(10..=90).contains(&self.touch_percent) || self.release_percent >= self.touch_percent {
            return Err("touch is at 10..90 percent, and release below it".into());
        }
        Ok(())
    }
}

/// What the calibration reads from the chip.
pub trait Chip {
    fn signals(&mut self) -> Result<Signals, mpr121::Error>;
    fn charge(&mut self) -> Result<Charge, mpr121::Error>;
}

impl<T: mpr121::RegisterIo> Chip for mpr121::Device<T> {
    fn signals(&mut self) -> Result<Signals, mpr121::Error> {
        self.read_signals()
    }
    fn charge(&mut self) -> Result<Charge, mpr121::Error> {
        mpr121::Device::charge(self)
    }
}

/// The monotonic clock and the wait between readings. Injected for tests.
pub trait Clock {
    fn now_us(&mut self) -> u64;
    fn sleep_us(&mut self, microseconds: u64);
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_us(&mut self) -> u64 {
        lamp_ipc::monotonic_us()
    }
    fn sleep_us(&mut self, microseconds: u64) {
        std::thread::sleep(std::time::Duration::from_micros(microseconds));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Thresholds were set.
    Ok,
    /// No swipe was seen on the pad: it was not touched, or it does not
    /// sense through the shell.
    NotTouched,
    /// The weakest swipe passed the noise by less than [`MIN_GAP`].
    TooCloseToNoise,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pad {
    pub electrode: u8,
    /// Largest |baseline - filtered| while nobody touched.
    pub noise: u16,
    /// Swipes seen: runs of [`CONTACT_READINGS`] or more readings above
    /// noise + [`CONTACT_ABOVE_NOISE`].
    pub swipes: u32,
    /// The lowest and highest peak (baseline - filtered) among those swipes.
    pub weakest: u16,
    pub strongest: u16,
    pub verdict: Verdict,
    /// Set thresholds; with any other verdict, the old ones, unchanged.
    pub touch_threshold: u8,
    pub release_threshold: u8,
    /// The baseline when the noise step began and when the swipe step ended.
    pub baseline_quiet: u16,
    pub baseline_end: u16,
    /// The charge current (µA) and time code the chip chose for the pad.
    pub cdc: u8,
    pub cdt: u8,
    /// Readings in the swipe step that the old thresholds counted as touch.
    pub touched_old: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Rule {
    pub touch_percent: u8,
    pub release_percent: u8,
    pub contact_above_noise: u16,
    pub contact_readings: u32,
    pub min_gap: u16,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    pub plan: Plan,
    pub rule: Rule,
    pub every_ms: u64,
    pub quiet_readings: u64,
    pub swipe_readings: u64,
    /// The working pads, in the pins file's order.
    pub pads: Vec<Pad>,
    /// Electrodes left out of the working set in the pins file: not working
    /// on this unit (E4 and E8 on lamp-4ace). Their thresholds are kept.
    pub not_working: Vec<u8>,
}

impl Report {
    pub fn all_ok(&self) -> bool {
        self.pads.iter().all(|pad| pad.verdict == Verdict::Ok)
    }

    /// The profile with the new thresholds of the working pads. Not-working
    /// electrodes keep theirs. None unless every working pad is Ok.
    pub fn apply(&self, profile: &Profile) -> Option<Profile> {
        if !self.all_ok() {
            return None;
        }
        let mut profile = profile.clone();
        for pad in &self.pads {
            let electrode = usize::from(pad.electrode);
            profile.touch_thresholds[electrode] = pad.touch_threshold;
            profile.release_thresholds[electrode] = pad.release_threshold;
        }
        profile.check().ok()?;
        Some(profile)
    }
}

fn rule(plan: &Plan) -> Rule {
    Rule {
        touch_percent: plan.touch_percent,
        release_percent: plan.release_percent,
        contact_above_noise: CONTACT_ABOVE_NOISE,
        contact_readings: CONTACT_READINGS,
        min_gap: MIN_GAP,
        text: format!(
            "touch at {}% and release at {}% of the way from the pad's noise to its weakest swipe; \
             a swipe is {} or more readings in a row more than {} above the noise; \
             the weakest swipe must be at least {} above the noise",
            plan.touch_percent,
            plan.release_percent,
            CONTACT_READINGS,
            CONTACT_ABOVE_NOISE,
            MIN_GAP
        ),
    }
}

/// The touch and release thresholds for one pad, from the largest swing
/// seen with nobody touching and the peak of its weakest swipe.
pub fn thresholds(noise: u16, weakest: u16, plan: &Plan) -> Result<(u8, u8), Verdict> {
    let noise = u32::from(noise);
    let gap = u32::from(weakest).saturating_sub(noise);
    if gap < u32::from(MIN_GAP) || noise + 2 > 255 {
        return Err(Verdict::TooCloseToNoise);
    }
    let at = |percent: u8| noise + (gap * u32::from(percent) + 50) / 100;
    let touch = at(plan.touch_percent).clamp(noise + 2, 255);
    let release = at(plan.release_percent).clamp(noise + 1, touch - 1);
    Ok((touch as u8, release as u8))
}

/// One pad's swipes, found as they happen.
#[derive(Default)]
struct Swipes {
    run: u32,
    run_peak: u16,
    count: u32,
    weakest: Option<u16>,
    strongest: u16,
}

impl Swipes {
    fn reading(&mut self, delta: i32, noise: u16) {
        if delta > i32::from(noise) + i32::from(CONTACT_ABOVE_NOISE) {
            self.run += 1;
            self.run_peak = self.run_peak.max(delta.min(i32::from(u16::MAX)) as u16);
        } else {
            self.end();
        }
    }

    fn end(&mut self) {
        if self.run >= CONTACT_READINGS {
            self.count += 1;
            self.weakest = Some(self.weakest.map_or(self.run_peak, |w| w.min(self.run_peak)));
            self.strongest = self.strongest.max(self.run_peak);
        }
        self.run = 0;
        self.run_peak = 0;
    }
}

/// Run the three steps on `chip`, reading every `every_ms`. `say` tells the
/// person at the lamp what to do at the start of each step.
pub fn measure(
    chip: &mut impl Chip,
    clock: &mut impl Clock,
    pads: &[u8],
    profile: &Profile,
    every_ms: u64,
    plan: Plan,
    mut say: impl FnMut(&str),
) -> Result<Report, mpr121::Error> {
    plan.check().map_err(mpr121::Error::InvalidConfig)?;
    let every_us = every_ms.max(1) * 1_000;
    let mut noise = [0_u16; 12];
    let mut swipes: [Swipes; 12] = Default::default();
    let mut touched_old = [0_u64; 12];
    let mut baseline_quiet = None;
    let mut baseline_end = [0_u16; 12];
    let mut quiet_readings = 0;
    let mut swipe_readings = 0;

    say(&format!(
        "hands away: the chip calibrates itself for {} s",
        plan.settle
    ));
    step(chip, clock, plan.settle, every_us, |_| {})?;
    say(&format!(
        "still hands away: measuring each pad's noise for {} s",
        plan.quiet
    ));
    step(chip, clock, plan.quiet, every_us, |signals| {
        quiet_readings += 1;
        baseline_quiet.get_or_insert(signals.baseline);
        for &electrode in pads {
            let e = usize::from(electrode);
            let swing = signals.delta(e).unsigned_abs().min(u32::from(u16::MAX)) as u16;
            noise[e] = noise[e].max(swing);
        }
    })?;
    say(&format!(
        "now swipe each working pad a few times, end to end, for {} s",
        plan.swipe
    ));
    step(chip, clock, plan.swipe, every_us, |signals| {
        swipe_readings += 1;
        baseline_end = signals.baseline;
        for &electrode in pads {
            let e = usize::from(electrode);
            swipes[e].reading(signals.delta(e), noise[e]);
            if signals.touched & (1 << e) != 0 {
                touched_old[e] += 1;
            }
        }
    })?;
    let charge = chip.charge()?;
    let baseline_quiet = baseline_quiet.unwrap_or([0; 12]);
    let pads_report = pads
        .iter()
        .map(|&electrode| {
            let e = usize::from(electrode);
            swipes[e].end();
            let found = &swipes[e];
            let result = match found.weakest {
                None => Err(Verdict::NotTouched),
                Some(weakest) => thresholds(noise[e], weakest, &plan),
            };
            let (verdict, touch_threshold, release_threshold) = match result {
                Ok((touch, release)) => (Verdict::Ok, touch, release),
                Err(verdict) => (
                    verdict,
                    profile.touch_thresholds[e],
                    profile.release_thresholds[e],
                ),
            };
            Pad {
                electrode,
                noise: noise[e],
                swipes: found.count,
                weakest: found.weakest.unwrap_or(0),
                strongest: found.strongest,
                verdict,
                touch_threshold,
                release_threshold,
                baseline_quiet: baseline_quiet[e],
                baseline_end: baseline_end[e],
                cdc: charge.cdc[e],
                cdt: charge.cdt[e],
                touched_old: touched_old[e],
            }
        })
        .collect();
    Ok(Report {
        plan,
        rule: rule(&plan),
        every_ms,
        quiet_readings,
        swipe_readings,
        pads: pads_report,
        not_working: (0..12).filter(|e| !pads.contains(e)).collect(),
    })
}

/// Read every `every_us` for `seconds`, handing each reading to `keep`.
/// A chip fault ends the calibration: nothing is saved from a faulted chip.
fn step(
    chip: &mut impl Chip,
    clock: &mut impl Clock,
    seconds: u64,
    every_us: u64,
    mut keep: impl FnMut(&Signals),
) -> Result<(), mpr121::Error> {
    let start = clock.now_us();
    let end = start.saturating_add(seconds.saturating_mul(1_000_000));
    let mut next = start;
    while clock.now_us() < end {
        keep(&chip.signals()?);
        next = next.saturating_add(every_us);
        let now = clock.now_us();
        if next > now {
            clock.sleep_us(next - now);
        } else {
            // Behind: do not try to catch up with a burst of reads.
            next = now;
        }
    }
    Ok(())
}

/// Hold the chip as the served path does and run the calibration on it.
#[cfg(target_os = "linux")]
pub fn run(
    base: &mpr121::Config,
    plan: Plan,
    say: impl FnMut(&str),
) -> Result<Report, Box<dyn std::error::Error>> {
    let _claim = crate::strip_worker::DeviceClaim::acquire(base)?;
    let io = mpr121::LinuxIo::open(base)?;
    let mut device = mpr121::Device::initialize(io, base.clone(), &mut mpr121::ThreadDelay)?;
    Ok(measure(
        &mut device,
        &mut SystemClock,
        &base.pads,
        &base.profile,
        base.every_ms,
        plan,
        say,
    )?)
}

#[cfg(not(target_os = "linux"))]
pub fn run(
    _: &mpr121::Config,
    _: Plan,
    _: impl FnMut(&str),
) -> Result<Report, Box<dyn std::error::Error>> {
    Err("calibrate-touch reads the MPR121, which this system does not have".into())
}
