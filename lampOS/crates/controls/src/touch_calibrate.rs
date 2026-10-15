//! The hardware team's touch calibration for the base strip (#711), run by
//! hand at the lamp: the chip calibrates itself with nobody touching, each
//! pad's noise is measured, a person swipes each pad, and per-pad touch and
//! release thresholds are chosen between the two and saved.
//!
//! Nothing here runs unless `lamp-controls calibrate-touch` is started. It
//! serves nothing and publishes nothing. The threshold rule is authored, not
//! a datasheet value; see `docs/controls.md`.

use crate::mpr121::{self, Charge, Profile, Signals};
use serde::Serialize;

/// How long each step lasts, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// Hands away while the chip starts and auto-calibrates. Nothing measured.
    pub settle: u64,
    /// Hands away still: each pad's noise is measured.
    pub quiet: u64,
    /// Each working pad is swiped a few times: each pad's peak is kept.
    pub swipe: u64,
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
    };

    pub fn check(&self) -> Result<(), String> {
        if !(1..=600).contains(&self.settle)
            || !(1..=600).contains(&self.quiet)
            || !(1..=600).contains(&self.swipe)
        {
            return Err("each calibration step lasts 1..600 seconds".into());
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
    /// Thresholds were chosen.
    Ok,
    /// The swipe never rose above the noise: the pad was not touched, or
    /// does not sense through the shell.
    NotTouched,
    /// The swipe rose, but less than twice the noise floor: no threshold
    /// would keep a clear margin from both.
    TooCloseToNoise,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pad {
    pub electrode: u8,
    /// Largest |baseline - filtered| while nobody touched.
    pub noise: u16,
    /// Largest baseline - filtered during the swipe.
    pub peak: u16,
    pub verdict: Verdict,
    /// Chosen thresholds; with any other verdict, the old ones, unchanged.
    pub touch_threshold: u8,
    pub release_threshold: u8,
    /// The baseline when the noise step began and when the swipe ended.
    pub baseline_quiet: u16,
    pub baseline_end: u16,
    pub cdc: u8,
    pub cdt: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    pub plan: Plan,
    pub every_ms: u64,
    pub quiet_readings: u64,
    pub swipe_readings: u64,
    /// The readings in the swipe step that the chip itself counted as touched
    /// with the old thresholds, per electrode.
    pub swipe_touched_old: [u64; 12],
    pub pads: Vec<Pad>,
    pub rule: &'static str,
}

impl Report {
    pub fn all_ok(&self) -> bool {
        self.pads.iter().all(|pad| pad.verdict == Verdict::Ok)
    }

    /// The profile with the new thresholds of the working pads. Excluded
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

pub const RULE: &str = "floor = noise + 2; not touched if peak <= floor; too close to noise if peak < 2 * floor; else touch = min(peak / 2, 255) and release = max(touch * 6 / 10, noise + 1)";

/// The touch and release thresholds for one pad, from the largest swing
/// seen with nobody touching and the largest delta seen while swiped.
pub fn thresholds(noise: u16, peak: u16) -> Result<(u8, u8), Verdict> {
    let floor = u32::from(noise) + 2;
    let peak = u32::from(peak);
    if peak <= floor {
        return Err(Verdict::NotTouched);
    }
    if peak < 2 * floor {
        return Err(Verdict::TooCloseToNoise);
    }
    // peak >= 2 * floor, so peak / 2 >= floor > noise + 1.
    let touch = (peak / 2).min(255);
    let release = (touch * 6 / 10).max(u32::from(noise) + 1);
    if release >= touch {
        return Err(Verdict::TooCloseToNoise);
    }
    Ok((touch as u8, release as u8))
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
    let mut peak = [0_u16; 12];
    let mut swipe_touched_old = [0_u64; 12];
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
            let delta = signals.delta(e).clamp(0, i32::from(u16::MAX)) as u16;
            peak[e] = peak[e].max(delta);
            if signals.touched & (1 << e) != 0 {
                swipe_touched_old[e] += 1;
            }
        }
    })?;
    let charge = chip.charge()?;
    let baseline_quiet = baseline_quiet.unwrap_or([0; 12]);
    let pads = pads
        .iter()
        .map(|&electrode| {
            let e = usize::from(electrode);
            let (verdict, touch_threshold, release_threshold) = match thresholds(noise[e], peak[e])
            {
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
                peak: peak[e],
                verdict,
                touch_threshold,
                release_threshold,
                baseline_quiet: baseline_quiet[e],
                baseline_end: baseline_end[e],
                cdc: charge.cdc[e],
                cdt: charge.cdt[e],
            }
        })
        .collect();
    Ok(Report {
        plan,
        every_ms,
        quiet_readings,
        swipe_readings,
        swipe_touched_old,
        pads,
        rule: RULE,
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
