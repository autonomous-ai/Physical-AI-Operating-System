//! A digital room for process-level tests of the real runtime on
//! kernel-simulated hardware (ALSA `snd-aloop`, `gpio-sim`).
//!
//! The runtime's speaker plays into the loopback card; the room reads that
//! accepted PCM back, delays and attenuates it as echo, adds any active
//! stimuli, and writes the result into the loopback the runtime's microphone
//! captures. Everything is digital: the echo is a configured gain and delay,
//! not an acoustic path, and stimulus onsets are the host times their first
//! samples were handed to the loopback. Results from this room are labelled
//! `process_simulation` and are never pooled with physical evidence.
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf};

/// The room runs at the runtime's capture rate in 10 ms blocks.
pub const RATE: u32 = 16_000;
pub const BLOCK: usize = 160;
/// Longest stimulus the room accepts (samples at `RATE`).
pub const MAX_STIMULUS_SAMPLES: usize = 60 * RATE as usize;
/// Concurrent stimuli (for example two speakers).
pub const MAX_ACTIVE: usize = 4;
pub const MAX_ECHO_DELAY_MS: u32 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Echo {
    /// Linear gain applied to the speaker signal (0 disables echo).
    pub gain: f32,
    pub delay_ms: u32,
}

impl Echo {
    pub fn validate(self) -> Result<Self, String> {
        if !(0.0..=1.0).contains(&self.gain) || !self.gain.is_finite() {
            return Err("echo gain must be 0..=1".into());
        }
        if self.delay_ms > MAX_ECHO_DELAY_MS {
            return Err(format!("echo delay must be at most {MAX_ECHO_DELAY_MS} ms"));
        }
        Ok(self)
    }
    fn delay_samples(self) -> usize {
        (self.delay_ms * RATE / 1000) as usize
    }
}

/// Room control messages (JSON datagrams on the room's control socket).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Play a WAV (any rate, mono or the first channel) into the microphone.
    /// `reply` is a datagram socket that receives `Notice`s for this id.
    Play {
        id: u64,
        wav: PathBuf,
        reply: Option<PathBuf>,
    },
    Cancel {
        id: u64,
    },
    Echo {
        echo: Echo,
    },
    /// Close (`muted: true`) or open the simulated privacy switch.
    Privacy {
        muted: bool,
    },
    /// Send a signal to a runtime worker found by its `worker ROLE` arguments.
    Signal {
        role: String,
        signal: WorkerSignal,
    },
    /// An empty struct variant, so unknown keys are rejected (internally
    /// tagged unit variants would accept them).
    Stop {},
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerSignal {
    Kill,
    Stop,
    Continue,
}

/// Delivery evidence for one stimulus, sent to its reply socket.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum Notice {
    Started {
        id: u64,
        /// Index of the first stimulus sample in the room's microphone stream.
        mic_sample: u64,
        /// Host monotonic time just after that block was handed to the loopback.
        host_us: u64,
    },
    Finished {
        id: u64,
        mic_sample: u64,
        host_us: u64,
        samples: u64,
        cancelled: bool,
    },
    Rejected {
        id: u64,
        reason: String,
    },
}

impl Notice {
    pub fn id(&self) -> u64 {
        match self {
            Notice::Started { id, .. }
            | Notice::Finished { id, .. }
            | Notice::Rejected { id, .. } => *id,
        }
    }
}

struct Active {
    id: u64,
    pcm: Vec<i16>,
    position: usize,
    reply: Option<PathBuf>,
    started: bool,
}

/// The mixing core, independent of audio I/O so it can be tested anywhere.
pub struct RoomCore {
    echo: Echo,
    /// Speaker samples waiting `delay` samples before they return as echo.
    delay_line: VecDeque<i16>,
    active: Vec<Active>,
    mic_samples: u64,
    pending: Vec<(Option<PathBuf>, Notice)>,
}

impl RoomCore {
    pub fn new(echo: Echo) -> Self {
        let mut core = Self {
            echo,
            delay_line: VecDeque::new(),
            active: Vec::new(),
            mic_samples: 0,
            pending: Vec::new(),
        };
        core.set_echo(echo);
        core
    }

    pub fn echo(&self) -> Echo {
        self.echo
    }

    /// Change the echo path. The delay line restarts silent: no speaker
    /// sample is replayed twice or skipped into the past.
    pub fn set_echo(&mut self, echo: Echo) {
        self.echo = echo;
        self.delay_line.clear();
        self.delay_line
            .extend(std::iter::repeat_n(0, echo.delay_samples()));
    }

    pub fn mic_samples(&self) -> u64 {
        self.mic_samples
    }

    pub fn active(&self) -> usize {
        self.active.len()
    }

    /// Queue a stimulus at `RATE`. It starts in the next mixed block.
    pub fn play(&mut self, id: u64, pcm: Vec<i16>, reply: Option<PathBuf>) {
        let reject = |reason: &str| Notice::Rejected {
            id,
            reason: reason.into(),
        };
        if pcm.is_empty() || pcm.len() > MAX_STIMULUS_SAMPLES {
            self.pending
                .push((reply, reject("stimulus length out of bounds")));
        } else if self.active.len() >= MAX_ACTIVE {
            self.pending
                .push((reply, reject("too many concurrent stimuli")));
        } else if self.active.iter().any(|a| a.id == id) {
            self.pending.push((reply, reject("duplicate stimulus id")));
        } else {
            self.active.push(Active {
                id,
                pcm,
                position: 0,
                reply,
                started: false,
            });
        }
    }

    /// Stop a stimulus early; its `Finished` notice is marked cancelled.
    pub fn cancel(&mut self, id: u64, host_us: u64) {
        if let Some(index) = self.active.iter().position(|a| a.id == id) {
            let active = self.active.swap_remove(index);
            self.pending.push((
                active.reply,
                Notice::Finished {
                    id,
                    mic_sample: self.mic_samples,
                    host_us,
                    samples: active.position as u64,
                    cancelled: true,
                },
            ));
        }
    }

    /// Mix one block: `out[i] = sum(stimuli) + gain * speaker[i - delay]`,
    /// saturating. `host_us` is the time the block is handed to the loopback.
    pub fn mix(&mut self, speaker: &[i16], out: &mut [i16], host_us: u64) {
        debug_assert_eq!(speaker.len(), out.len());
        let block_start = self.mic_samples;
        for (index, sample) in out.iter_mut().enumerate() {
            self.delay_line.push_back(speaker[index]);
            let delayed = self.delay_line.pop_front().unwrap_or(0);
            let mut value = f32::from(delayed) * self.echo.gain;
            for active in &mut self.active {
                if active.position < active.pcm.len() {
                    value += f32::from(active.pcm[active.position]);
                    active.position += 1;
                }
            }
            *sample = value
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        }
        self.mic_samples += out.len() as u64;
        for active in &mut self.active {
            if !active.started {
                active.started = true;
                self.pending.push((
                    active.reply.clone(),
                    Notice::Started {
                        id: active.id,
                        mic_sample: block_start,
                        host_us,
                    },
                ));
            }
        }
        let mut index = 0;
        while index < self.active.len() {
            if self.active[index].position >= self.active[index].pcm.len() {
                let done = self.active.swap_remove(index);
                self.pending.push((
                    done.reply,
                    Notice::Finished {
                        id: done.id,
                        mic_sample: self.mic_samples,
                        host_us,
                        samples: done.pcm.len() as u64,
                        cancelled: false,
                    },
                ));
            } else {
                index += 1;
            }
        }
    }

    /// Notices produced since the last call, with their reply sockets.
    pub fn take_notices(&mut self) -> Vec<(Option<PathBuf>, Notice)> {
        std::mem::take(&mut self.pending)
    }
}

/// Read a bounded WAV and convert it to mono 16-bit PCM at `RATE` (first
/// channel; linear interpolation when the rate differs).
pub fn load_stimulus(path: &std::path::Path) -> Result<Vec<i16>, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err("stimulus WAV exceeds 64 MiB".into());
    }
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err("invalid WAV format".into());
    }
    let channels = usize::from(spec.channels);
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .step_by(channels)
            .map(|s| s.map(f32::from))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .step_by(channels)
            .map(|s| s.map(|v| v * 32767.0))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        _ => return Err("unsupported WAV sample format".into()),
    };
    let ratio = f64::from(spec.sample_rate) / f64::from(RATE);
    let length = (samples.len() as f64 / ratio).floor() as usize;
    if length == 0 || length > MAX_STIMULUS_SAMPLES {
        return Err("stimulus length out of bounds".into());
    }
    Ok((0..length)
        .map(|n| {
            let position = n as f64 * ratio;
            let index = position.floor() as usize;
            let fraction = (position - index as f64) as f32;
            let a = samples[index];
            let b = samples.get(index + 1).copied().unwrap_or(a);
            (a + (b - a) * fraction)
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(value: i16) -> Vec<i16> {
        vec![value; BLOCK]
    }

    #[test]
    fn echo_returns_the_speaker_after_its_delay_at_its_gain() {
        let mut room = RoomCore::new(Echo {
            gain: 0.5,
            delay_ms: 20,
        });
        let mut out = block(0);
        // 20 ms = two blocks of silence before the first speaker block returns.
        room.mix(&block(1000), &mut out, 1);
        assert_eq!(out, block(0));
        room.mix(&block(2000), &mut out, 2);
        assert_eq!(out, block(0));
        room.mix(&block(0), &mut out, 3);
        assert_eq!(out, block(500));
        room.mix(&block(0), &mut out, 4);
        assert_eq!(out, block(1000));
    }

    #[test]
    fn stimuli_mix_saturate_and_report_their_exact_start_and_end() {
        let mut room = RoomCore::new(Echo {
            gain: 0.0,
            delay_ms: 0,
        });
        let mut out = block(0);
        room.mix(&block(0), &mut out, 10);
        room.play(1, vec![30_000; 250], None);
        room.play(2, vec![10_000; 100], None);
        room.mix(&block(0), &mut out, 20);
        // Overlapping stimuli add and saturate rather than wrap.
        assert_eq!(out[0], i16::MAX);
        assert_eq!(out[150], 30_000);
        room.mix(&block(0), &mut out, 30);
        assert_eq!(&out[89..91], &[30_000, 0]);
        let notices: Vec<Notice> = room.take_notices().into_iter().map(|(_, n)| n).collect();
        assert!(notices.contains(&Notice::Started {
            id: 1,
            mic_sample: 160,
            host_us: 20
        }));
        assert!(notices.contains(&Notice::Finished {
            id: 2,
            mic_sample: 320,
            host_us: 20,
            samples: 100,
            cancelled: false
        }));
        assert!(notices.contains(&Notice::Finished {
            id: 1,
            mic_sample: 480,
            host_us: 30,
            samples: 250,
            cancelled: false
        }));
        assert_eq!(room.active(), 0);
    }

    #[test]
    fn cancellation_bounds_and_duplicates_are_explicit() {
        let mut room = RoomCore::new(Echo {
            gain: 0.0,
            delay_ms: 0,
        });
        room.play(1, vec![1; 10_000], None);
        room.play(1, vec![1; 10], None);
        room.play(2, Vec::new(), None);
        let mut out = block(0);
        room.mix(&block(0), &mut out, 5);
        room.cancel(1, 6);
        let notices: Vec<Notice> = room.take_notices().into_iter().map(|(_, n)| n).collect();
        assert!(
            matches!(&notices[0], Notice::Rejected { id: 1, reason } if reason.contains("duplicate"))
        );
        assert!(matches!(&notices[1], Notice::Rejected { id: 2, .. }));
        assert!(notices.contains(&Notice::Finished {
            id: 1,
            mic_sample: 160,
            host_us: 6,
            samples: 160,
            cancelled: true
        }));
        assert!(
            Echo {
                gain: 1.5,
                delay_ms: 0
            }
            .validate()
            .is_err()
        );
        assert!(
            Echo {
                gain: 0.5,
                delay_ms: 501
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn changing_the_echo_path_never_replays_old_speaker_audio() {
        let mut room = RoomCore::new(Echo {
            gain: 1.0,
            delay_ms: 10,
        });
        let mut out = block(0);
        room.mix(&block(700), &mut out, 1);
        room.set_echo(Echo {
            gain: 1.0,
            delay_ms: 10,
        });
        room.mix(&block(0), &mut out, 2);
        assert_eq!(
            out,
            block(0),
            "the pre-change speaker block must not return"
        );
    }

    #[test]
    fn commands_round_trip_as_strict_json() {
        let command = Command::Play {
            id: 3,
            wav: "/sim/a.wav".into(),
            reply: Some("/tmp/r.sock".into()),
        };
        let text = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&text).unwrap(), command);
        assert!(serde_json::from_str::<Command>(r#"{"cmd":"stop","extra":1}"#).is_err());
    }
}
