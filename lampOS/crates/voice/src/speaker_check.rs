//! The self-test's speaker check (#812): a short, quiet tone played through
//! the voice's own speaker path, so the echo canceller has it as reference,
//! and what the voice microphone made of it.
//!
//! The cleaned microphone sound cannot carry the tone: the canceller takes
//! out exactly what the lamp plays. So the check reads each block's
//! [`EchoEvidence`]: the level before cancellation rises while the tone
//! plays (the speaker and the microphone work), the time from the first
//! chunk to that rise is the echo path's delay, and the level after
//! cancellation stays well under the rise (the canceller works).
//!
//! Nothing here plays, reads a device or keeps a clock: the coordinator
//! gives it times and levels. It never plays near a conversation, with the
//! microphone not allowed, or at midday ([`may_play`]).
use lamp_audio::evidence::EchoEvidence;

/// Speaker samples a second.
pub const RATE: u32 = 24_000;
/// The tone: well inside the band the canceller's evidence is measured in
/// (4 kHz analysis) and one a small speaker plays.
pub const HERTZ: f64 = 600.0;
/// How long it plays.
pub const PLAYS_MS: u32 = 300;
/// How loud: quiet enough not to startle anyone in the room.
pub const DBFS: f64 = -30.0;
/// Fade in and out, so it starts and ends without a click.
pub const FADE_MS: u32 = 10;
/// Samples in one speaker chunk.
pub const CHUNK: usize = 240;

/// Nothing heard or said for this long before it plays (brain-build.md's
/// rule for anything that could cut across a person).
pub const QUIET_US: u64 = 45_000_000;
/// Not between these local minutes of the day: 12:00 to 14:00.
pub const MIDDAY: (u32, u32) = (12 * 60, 14 * 60);

/// The level before the tone is the mean of this long before its first chunk.
pub const BASELINE_US: u64 = 500_000;
/// The rise over that, in dB, that counts as the tone heard.
pub const HEARD_RISE_DB: f32 = 10.0;
/// Longest wait from the first chunk for the rise: past the canceller's
/// 400 ms delay search and the tone itself.
pub const LISTEN_US: u64 = 800_000;
/// After cancellation the tone must be at least this much under the rise.
pub const CANCELLED_DB: f32 = 6.0;

/// The tone, a whole number of chunks, zero after the end of the sound.
pub fn tone() -> Vec<i16> {
    let length = (RATE * PLAYS_MS / 1_000) as usize;
    let fade = (RATE * FADE_MS / 1_000) as usize;
    let amplitude = 32_767.0 * 10f64.powf(DBFS / 20.0) * std::f64::consts::SQRT_2;
    let mut samples: Vec<i16> = (0..length)
        .map(|n| {
            let edge = n.min(length - 1 - n);
            let gain = if edge < fade {
                0.5 - 0.5 * (std::f64::consts::PI * edge as f64 / fade as f64).cos()
            } else {
                1.0
            };
            let phase = 2.0 * std::f64::consts::PI * HERTZ * n as f64 / f64::from(RATE);
            (amplitude * gain * phase.sin()).round() as i16
        })
        .collect();
    samples.resize(length.div_ceil(CHUNK) * CHUNK, 0);
    samples
}

/// What the voice knows when asked to play the tone.
#[derive(Clone, Copy, Debug, Default)]
pub struct Now {
    pub at_us: u64,
    /// The microphone is allowed by the privacy switch.
    pub allowed: bool,
    /// A turn is open: input, playback, a reply, or the keeper's pause.
    pub busy: bool,
    /// The last time anything was heard (voice activity) or said.
    pub last_heard_us: Option<u64>,
    pub last_said_us: Option<u64>,
    /// Minutes since local midnight, if the local time is known.
    pub local_minutes: Option<u32>,
}

/// Whether the tone may play now, or why not.
pub fn may_play(now: &Now) -> Result<(), String> {
    if !now.allowed {
        return Err("the microphone is not allowed (privacy switch)".into());
    }
    if now.busy {
        return Err("the voice is busy with a turn".into());
    }
    for (what, at) in [("heard", now.last_heard_us), ("said", now.last_said_us)] {
        if let Some(at) = at
            && now.at_us.saturating_sub(at) < QUIET_US
        {
            return Err(format!(
                "something was {what} {:.0} s ago; the tone waits for 45 s of quiet",
                now.at_us.saturating_sub(at) as f64 / 1e6
            ));
        }
    }
    match now.local_minutes {
        None => Err("the local time is not known, so midday cannot be kept free".into()),
        Some(m) if (MIDDAY.0..MIDDAY.1).contains(&m) => Err("not between 12:00 and 14:00".into()),
        Some(_) => Ok(()),
    }
}

/// One capture block's levels, in dB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub at_us: u64,
    pub capture_db: f32,
    pub residual_db: f32,
}

impl Block {
    pub fn of(at_us: u64, echo: &EchoEvidence) -> Self {
        Self {
            at_us,
            capture_db: f32::from(echo.capture_level.tenths_of_db()) / 10.0,
            residual_db: f32::from(echo.residual_level.tenths_of_db()) / 10.0,
        }
    }
}

/// What the voice microphone made of the tone.
#[derive(Clone, Debug, PartialEq)]
pub enum Heard {
    /// The rise over the level before, the delay from the first chunk to
    /// it, and how far under the rise the cancelled level stayed.
    Yes {
        rise_db: f32,
        delay_ms: u32,
        cancelled_db: f32,
    },
    /// No rise within [`LISTEN_US`].
    No { baseline_db: f32, loudest_db: f32 },
    /// Too little before the tone to know the room's level.
    NoBaseline,
}

/// Judges the blocks around a tone whose first chunk was accepted at
/// `started_us`.
pub fn heard(blocks: &[Block], started_us: u64) -> Heard {
    let before: Vec<f32> = blocks
        .iter()
        .filter(|b| b.at_us < started_us && b.at_us + BASELINE_US >= started_us)
        .map(|b| b.capture_db)
        .collect();
    if before.len() < 10 {
        return Heard::NoBaseline;
    }
    let baseline = before.iter().sum::<f32>() / before.len() as f32;
    let during: Vec<&Block> = blocks
        .iter()
        .filter(|b| b.at_us >= started_us && b.at_us < started_us + LISTEN_US)
        .collect();
    let loudest = during
        .iter()
        .map(|b| b.capture_db)
        .fold(f32::NEG_INFINITY, f32::max);
    let Some(onset) = during
        .iter()
        .find(|b| b.capture_db >= baseline + HEARD_RISE_DB)
    else {
        return Heard::No {
            baseline_db: baseline,
            loudest_db: loudest.max(baseline),
        };
    };
    // How far the cancelled level stayed under the rise, at the tone's peak.
    let peak = during
        .iter()
        .max_by(|a, b| a.capture_db.total_cmp(&b.capture_db))
        .copied()
        .copied()
        .unwrap_or(**onset);
    Heard::Yes {
        rise_db: peak.capture_db - baseline,
        delay_ms: ((onset.at_us - started_us) / 1_000) as u32,
        cancelled_db: peak.capture_db - peak.residual_db,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet(at_us: u64) -> Now {
        Now {
            at_us,
            allowed: true,
            busy: false,
            last_heard_us: Some(at_us - 60_000_000),
            last_said_us: None,
            local_minutes: Some(9 * 60),
        }
    }

    #[test]
    fn the_tone_is_quiet_whole_chunks_and_fades() {
        let tone = tone();
        assert_eq!(tone.len() % CHUNK, 0);
        assert_eq!(tone.len(), 7_200);
        assert_eq!(tone[0], 0);
        let peak = tone.iter().map(|s| s.unsigned_abs()).max().unwrap();
        // -30 dBFS RMS of a sine peaks at about 1465.
        assert!((1_400..1_500).contains(&peak), "{peak}");
        assert!(tone[..24].iter().all(|s| s.unsigned_abs() < 1_000));
    }

    #[test]
    fn it_plays_only_after_45_s_of_quiet_with_the_microphone_allowed_and_not_at_midday() {
        let at = 100_000_000;
        assert_eq!(may_play(&quiet(at)), Ok(()));
        let talked = Now {
            last_said_us: Some(at - 10_000_000),
            ..quiet(at)
        };
        assert!(may_play(&talked).unwrap_err().contains("said 10 s ago"));
        assert!(
            may_play(&Now {
                allowed: false,
                ..quiet(at)
            })
            .unwrap_err()
            .contains("privacy")
        );
        assert!(
            may_play(&Now {
                busy: true,
                ..quiet(at)
            })
            .is_err()
        );
        let lunch = Now {
            local_minutes: Some(12 * 60 + 30),
            ..quiet(at)
        };
        assert!(may_play(&lunch).unwrap_err().contains("12:00"));
        assert_eq!(
            may_play(&Now {
                local_minutes: Some(14 * 60),
                ..quiet(at)
            }),
            Ok(())
        );
        assert!(
            may_play(&Now {
                local_minutes: None,
                ..quiet(at)
            })
            .is_err()
        );
    }

    /// Blocks every 10 ms: the room at -60 dB, the tone at -35 dB from
    /// `echo_at_us` for 300 ms, cancelled to `residual` while it plays.
    fn blocks(started_us: u64, echo_at_us: u64, residual: f32) -> Vec<Block> {
        (0..150)
            .map(|n| {
                let at_us = started_us - 600_000 + n * 10_000;
                let playing = at_us >= echo_at_us && at_us < echo_at_us + 300_000;
                Block {
                    at_us,
                    capture_db: if playing { -35.0 } else { -60.0 },
                    residual_db: if playing { residual } else { -62.0 },
                }
            })
            .collect()
    }

    #[test]
    fn a_tone_heard_gives_its_rise_delay_and_how_well_it_was_cancelled() {
        let start = 10_000_000;
        let result = heard(&blocks(start, start + 70_000, -55.0), start);
        assert_eq!(
            result,
            Heard::Yes {
                rise_db: 25.0,
                delay_ms: 70,
                cancelled_db: 20.0
            }
        );
    }

    #[test]
    fn a_silent_speaker_or_a_missing_baseline_is_said() {
        let start = 10_000_000;
        let mut silent = blocks(start, start + 70_000, -55.0);
        for block in &mut silent {
            block.capture_db = -60.0;
        }
        assert!(matches!(heard(&silent, start), Heard::No { .. }));
        let late: Vec<Block> = blocks(start, start + 70_000, -55.0)
            .into_iter()
            .filter(|b| b.at_us >= start)
            .collect();
        assert_eq!(heard(&late, start), Heard::NoBaseline);
    }
}
