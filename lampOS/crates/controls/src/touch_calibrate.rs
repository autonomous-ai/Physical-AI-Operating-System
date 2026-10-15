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
use serde::{Deserialize, Serialize};

/// A delta must pass the pad's noise by more than this to count as touch.
pub const CONTACT_ABOVE_NOISE: u16 = 2;
/// A swipe over a pad is at least this many readings in a row above that.
pub const CONTACT_READINGS: u32 = 3;
/// The weakest swipe must pass the noise by at least this much, so that the
/// release threshold is above the noise and the touch threshold above both.
pub const MIN_GAP: u16 = 5;
/// The chip shows only the upper 8 of its 10 baseline bits, so the baseline it
/// compares can be up to this much above the one read (NXP AN3892, 5-6).
pub const BASELINE_HIDDEN: u16 = 3;

/// The largest delta the chip may be comparing, given one read with nobody
/// touching: the read delta, or the read delta plus the hidden baseline bits.
pub fn quiet_swing(read_delta: i32) -> u16 {
    let high = read_delta + i32::from(BASELINE_HIDDEN);
    read_delta
        .unsigned_abs()
        .max(high.unsigned_abs())
        .min(u32::from(u16::MAX)) as u16
}

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
    /// Largest |baseline - filtered| the chip may have compared while nobody
    /// touched, counting the baseline bits it does not show.
    pub noise: u16,
    /// Largest baseline - filtered while another pad was the one touched: a
    /// neighbour's finger reaching this pad through the shell.
    pub coupling: u16,
    /// Swipes on this pad: runs of [`CONTACT_READINGS`] or more readings in
    /// which it was the strongest pad, more than [`CONTACT_ABOVE_NOISE`] above
    /// its noise. A neighbour's swipe is not one of them.
    pub swipes: u32,
    /// The lowest and highest peak (baseline - filtered, as read) among this
    /// pad's own swipes. The chip's own delta is never lower than the one read.
    pub weakest: u16,
    pub strongest: u16,
    /// The touch level the thresholds come from: the lowest of its own swipes
    /// that reached at least [`OWN_SHARE_PERCENT`] of its strongest.
    pub level: u16,
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

/// A pad's touch level is its lowest own swipe that reached at least this
/// share of its strongest, so one swipe that only grazed it does not set it.
pub const OWN_SHARE_PERCENT: u16 = 50;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Rule {
    pub touch_percent: u8,
    pub release_percent: u8,
    pub contact_above_noise: u16,
    pub contact_readings: u32,
    pub own_share_percent: u16,
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
    /// How many working pads were lit at once while a pad was touched, in the
    /// swipe step: index 0 to 4 counts readings with that many, index 5 with
    /// five or more. `footprint_old` is what the chip said with the profile's
    /// thresholds; `footprint_new` replays the same readings with the new
    /// thresholds, the release below the touch and the profile's debounce, as
    /// the chip keeps a pad lit (`docs/controls.md`). It is a replay of the
    /// deltas as read: the chip may compare up to [`BASELINE_HIDDEN`] more,
    /// and it samples between the readings.
    pub footprint_old: [u64; 6],
    pub footprint_new: [u64; 6],
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
        own_share_percent: OWN_SHARE_PERCENT,
        min_gap: MIN_GAP,
        text: format!(
            "touch at {}% and release at {}% of the way from the pad's noise to its touch level, \
             rounded down; touch stays below that level and above release, and release above the \
             noise; the noise counts the {} baseline counts the chip does not show; a swipe on a \
             pad is {} or more readings in a row in which it is the strongest pad, more than {} \
             above its noise; its touch level is the lowest of its own swipes that reached {}% of \
             its strongest, and must be at least {} above the noise",
            plan.touch_percent,
            plan.release_percent,
            BASELINE_HIDDEN,
            CONTACT_READINGS,
            CONTACT_ABOVE_NOISE,
            OWN_SHARE_PERCENT,
            MIN_GAP
        ),
    }
}

/// The touch and release thresholds for one pad, from the largest swing
/// seen with nobody touching and its touch level.
pub fn thresholds(noise: u16, level: u16, plan: &Plan) -> Result<(u8, u8), Verdict> {
    let noise = u32::from(noise);
    let gap = u32::from(level).saturating_sub(noise);
    if gap < u32::from(MIN_GAP) || noise + 2 > 255 {
        return Err(Verdict::TooCloseToNoise);
    }
    // The chip counts a touch only when its delta is strictly above the touch
    // threshold, and a release only when it is strictly below the release one.
    // So: noise < release < touch < level, which MIN_GAP leaves room for.
    let at = |percent: u8| noise + gap * u32::from(percent) / 100;
    let touch = at(plan.touch_percent)
        .clamp(noise + 2, u32::from(level) - 1)
        .min(255);
    let release = at(plan.release_percent).clamp(noise + 1, touch - 1);
    Ok((touch as u8, release as u8))
}

/// The lowest of `peaks` that reached [`OWN_SHARE_PERCENT`] of the highest,
/// compared without rounding: 16 is under half of 33.
pub fn own_level(peaks: &[u16]) -> Option<u16> {
    let strongest = *peaks.iter().max()?;
    let cutoff = u32::from(strongest) * u32::from(OWN_SHARE_PERCENT);
    peaks
        .iter()
        .copied()
        .filter(|&peak| u32::from(peak) * 100 >= cutoff)
        .min()
}

/// One pad's own swipes, found as they happen.
#[derive(Default)]
struct Swipes {
    run: u32,
    run_peak: u16,
    peaks: Vec<u16>,
}

impl Swipes {
    /// A reading in which this pad is the strongest one touched.
    fn strongest(&mut self, delta: i32) {
        self.run += 1;
        self.run_peak = self
            .run_peak
            .max(delta.clamp(0, i32::from(u16::MAX)) as u16);
    }

    fn end(&mut self) {
        if self.run >= CONTACT_READINGS {
            self.peaks.push(self.run_peak);
        }
        self.run = 0;
        self.run_peak = 0;
    }
}

/// One calibration's readings: what [`collect`] gathers from the chip, what
/// `--record` keeps and `--replay` reads back, and what [`analyse`] reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub every_ms: u64,
    pub quiet: Vec<Signals>,
    pub swipe: Vec<Signals>,
    pub charge: Charge,
}

/// Run the three steps on `chip`, reading every `every_ms`. `say` tells the
/// person at the lamp what to do at the start of each step.
pub fn collect(
    chip: &mut impl Chip,
    clock: &mut impl Clock,
    every_ms: u64,
    plan: Plan,
    mut say: impl FnMut(&str),
) -> Result<Session, mpr121::Error> {
    plan.check().map_err(mpr121::Error::InvalidConfig)?;
    let every_us = every_ms.max(1) * 1_000;
    let mut quiet = Vec::new();
    let mut swipe = Vec::new();
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
        quiet.push(*signals)
    })?;
    say(&format!(
        "now swipe each working pad a few times, end to end, for {} s",
        plan.swipe
    ));
    step(chip, clock, plan.swipe, every_us, |signals| {
        swipe.push(*signals)
    })?;
    Ok(Session {
        every_ms,
        quiet,
        swipe,
        charge: chip.charge()?,
    })
}

/// Thresholds for `pads` from one session's readings.
pub fn analyse(session: &Session, pads: &[u8], profile: &Profile, plan: Plan) -> Report {
    let mut noise = [0_u16; 12];
    for signals in &session.quiet {
        for &electrode in pads {
            let e = usize::from(electrode);
            noise[e] = noise[e].max(quiet_swing(signals.delta(e)));
        }
    }
    let mut swipes: [Swipes; 12] = Default::default();
    let mut coupling = [0_u16; 12];
    let mut touched_old = [0_u64; 12];
    // The pad touched at each reading of the swipe step: the one furthest
    // above its own noise, if any is more than CONTACT_ABOVE_NOISE above.
    let active: Vec<Option<usize>> = session
        .swipe
        .iter()
        .map(|signals| {
            pads.iter()
                .map(|&electrode| usize::from(electrode))
                .map(|e| (signals.delta(e) - i32::from(noise[e]), e))
                .filter(|&(excess, _)| excess > i32::from(CONTACT_ABOVE_NOISE))
                .max_by_key(|&(excess, e)| (excess, std::cmp::Reverse(e)))
                .map(|(_, e)| e)
        })
        .collect();
    for (signals, touched) in session.swipe.iter().zip(&active) {
        for &electrode in pads {
            let e = usize::from(electrode);
            let delta = signals.delta(e);
            if *touched == Some(e) {
                swipes[e].strongest(delta);
            } else {
                swipes[e].end();
                if touched.is_some() {
                    coupling[e] = coupling[e].max(delta.clamp(0, i32::from(u16::MAX)) as u16);
                }
            }
            if signals.touched & (1 << e) != 0 {
                touched_old[e] += 1;
            }
        }
    }
    let baseline_quiet = session.quiet.first().map_or([0; 12], |s| s.baseline);
    let baseline_end = session.swipe.last().map_or([0; 12], |s| s.baseline);
    let pads_report: Vec<Pad> = pads
        .iter()
        .map(|&electrode| {
            let e = usize::from(electrode);
            swipes[e].end();
            let peaks = &swipes[e].peaks;
            let level = own_level(peaks);
            let result = match level {
                None => Err(Verdict::NotTouched),
                Some(level) => thresholds(noise[e], level, &plan),
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
                coupling: coupling[e],
                swipes: peaks.len() as u32,
                weakest: peaks.iter().copied().min().unwrap_or(0),
                strongest: peaks.iter().copied().max().unwrap_or(0),
                level: level.unwrap_or(0),
                verdict,
                touch_threshold,
                release_threshold,
                baseline_quiet: baseline_quiet[e],
                baseline_end: baseline_end[e],
                cdc: session.charge.cdc[e],
                cdt: session.charge.cdt[e],
                touched_old: touched_old[e],
            }
        })
        .collect();
    let mut footprint_old = [0_u64; 6];
    let mut footprint_new = [0_u64; 6];
    // Each pad as the chip would hold it with the new thresholds: lit after
    // debounce + 1 readings in a row above its touch threshold, and unlit
    // only after as many below its release threshold, so a pad between the
    // two stays lit (#797 review).
    let (to_touch, to_release) = (
        u32::from(profile.debounce_touch) + 1,
        u32::from(profile.debounce_release) + 1,
    );
    let mut lit = vec![false; pads_report.len()];
    let mut streak = vec![0_u32; pads_report.len()];
    for (signals, touched) in session.swipe.iter().zip(&active) {
        for (index, pad) in pads_report.iter().enumerate() {
            let delta = signals.delta(usize::from(pad.electrode));
            let (crossing, needed) = if lit[index] {
                (delta < i32::from(pad.release_threshold), to_release)
            } else {
                (delta > i32::from(pad.touch_threshold), to_touch)
            };
            streak[index] = if crossing { streak[index] + 1 } else { 0 };
            if streak[index] >= needed {
                lit[index] = !lit[index];
                streak[index] = 0;
            }
        }
        if touched.is_none() {
            continue;
        }
        let old = pads
            .iter()
            .filter(|&&e| signals.touched & (1 << e) != 0)
            .count();
        let new = lit.iter().filter(|&&lit| lit).count();
        footprint_old[old.min(5)] += 1;
        footprint_new[new.min(5)] += 1;
    }
    Report {
        plan,
        rule: rule(&plan),
        every_ms: session.every_ms,
        quiet_readings: session.quiet.len() as u64,
        swipe_readings: session.swipe.len() as u64,
        pads: pads_report,
        not_working: (0..12).filter(|e| !pads.contains(e)).collect(),
        footprint_old,
        footprint_new,
    }
}

/// [`collect`], then [`analyse`].
pub fn measure(
    chip: &mut impl Chip,
    clock: &mut impl Clock,
    pads: &[u8],
    profile: &Profile,
    every_ms: u64,
    plan: Plan,
    say: impl FnMut(&str),
) -> Result<Report, mpr121::Error> {
    let session = collect(chip, clock, every_ms, plan, say)?;
    Ok(analyse(&session, pads, profile, plan))
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
) -> Result<Session, Box<dyn std::error::Error>> {
    let _claim = crate::strip_worker::DeviceClaim::acquire(base)?;
    let io = mpr121::LinuxIo::open(base)?;
    let mut device = mpr121::Device::initialize(io, base.clone(), &mut mpr121::ThreadDelay)?;
    Ok(collect(
        &mut device,
        &mut SystemClock,
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
) -> Result<Session, Box<dyn std::error::Error>> {
    Err("calibrate-touch reads the MPR121, which this system does not have".into())
}
