//! The voice microphone (Jieli): a second of its blocks on `mic2/audio`,
//! as `lamp-voice` gives them after its echo canceller, and their level.
//! Only listened to: the self-test never opens a microphone.
use crate::Check;
use lamp_bus::Bus;
use lamp_msgs::{Audio, PartResult, Verdict};
use std::time::{Duration, Instant};

/// The topic the voice microphone is heard on.
pub const TOPIC: &str = Audio::TOPICS[1];
/// How much of it is listened to.
pub const LISTEN: Duration = Duration::from_secs(1);
/// With nothing in this long, nothing is coming.
pub const FIRST_WITHIN: Duration = Duration::from_secs(2);
/// A microphone at or under this level gives nothing.
pub const SILENT_DB: f32 = crate::listen::SILENT_DB;

/// What a second of listening came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    pub blocks: usize,
    /// Blocks missing between those heard, by their sequence.
    pub lost: u64,
    pub sum_squares: f64,
    pub samples: usize,
}

impl Heard {
    /// Adds one message's payload: the header line, then S16LE samples.
    pub fn add(&mut self, payload: &[u8]) -> Result<(), String> {
        let (header, samples) = Audio::split(payload).map_err(|e| e.to_string())?;
        if header.sample_format != Audio::S16LE {
            return Err(format!("samples are {}, not S16LE", header.sample_format));
        }
        self.blocks += 1;
        for pair in samples.chunks_exact(2) {
            let sample = f64::from(i16::from_le_bytes([pair[0], pair[1]]));
            self.sum_squares += sample * sample;
            self.samples += 1;
        }
        Ok(())
    }

    /// The level of everything heard, in dBFS.
    pub fn level_db(&self) -> f32 {
        if self.samples == 0 || self.sum_squares == 0.0 {
            return -120.0;
        }
        let rms = (self.sum_squares / self.samples as f64).sqrt() / 32_768.0;
        (20.0 * rms.log10()) as f32
    }
}

/// Judges what was heard. `declared`: whether the voice ever said it would
/// publish this microphone.
pub fn voice_mic(declared: bool, heard: &Result<Heard, String>) -> PartResult {
    let part = "voice_mic";
    let heard = match heard {
        Err(why) => {
            return PartResult::new(
                part,
                Verdict::Fail,
                &format!("{TOPIC} cannot be read: {why}"),
            );
        }
        Ok(heard) => heard,
    };
    if heard.blocks == 0 {
        let why = if declared {
            "no sound for 2 s: the microphone is closed (privacy switch) or lamp-voice stopped giving it"
        } else {
            "nothing was ever said on mic2/audio: lamp-voice gives it only with LAMP_BUS=on and LAMP_BUS_AUDIO=on"
        };
        return PartResult::new(part, Verdict::NotChecked, why);
    }
    let level = heard.level_db();
    let result = if level <= SILENT_DB {
        PartResult::new(
            part,
            Verdict::Fail,
            &format!("silent at {level:.0} dBFS: the microphone gives nothing"),
        )
    } else {
        PartResult::new(part, Verdict::Pass, "")
    };
    result
        .with("level_db", format!("{level:.1}"))
        .with("blocks", heard.blocks)
        .with("lost", heard.lost)
}

/// Listens to the voice microphone for a second.
pub struct VoiceMic<'a> {
    pub bus: &'a Bus,
}

impl VoiceMic<'_> {
    fn listen(&self, deadline: Instant) -> Result<Heard, String> {
        let mut hears = self
            .bus
            .subscribe_typed(TOPIC, "selftest", Audio::TYPE)
            .map_err(|e| e.to_string())?;
        let begun = Instant::now();
        let mut heard = Heard::default();
        let mut last: Option<u64> = None;
        let mut first_at: Option<Instant> = None;
        loop {
            let now = Instant::now();
            let until = match first_at {
                Some(at) => at + LISTEN,
                None => begun + FIRST_WITHIN,
            }
            .min(deadline);
            if now >= until {
                break;
            }
            let Some(message) = hears.recv(until - now).map_err(|e| e.to_string())? else {
                continue;
            };
            if message.expired {
                continue;
            }
            first_at.get_or_insert_with(Instant::now);
            if let Ok((header, _)) = Audio::split(&message.payload) {
                if let Some(before) = last {
                    heard.lost += header.sequence.saturating_sub(before + 1);
                }
                last = Some(header.sequence);
            }
            heard.add(&message.payload)?;
        }
        Ok(heard)
    }
}

impl Check for VoiceMic<'_> {
    fn part(&self) -> &str {
        "voice_mic"
    }

    fn run(&mut self, deadline: Instant) -> PartResult {
        let declared = self.bus.declaration(TOPIC).ok().flatten().is_some();
        voice_mic(declared, &self.listen(deadline))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(sequence: u64, amplitude: i16) -> Vec<u8> {
        let samples: Vec<u8> = (0..320)
            .flat_map(|n| (if n % 2 == 0 { amplitude } else { -amplitude }).to_le_bytes())
            .collect();
        Audio {
            sample_rate: 16_000,
            channels: 1,
            sample_format: Audio::S16LE.into(),
            sequence,
        }
        .block(&samples)
        .unwrap()
    }

    #[test]
    fn a_quiet_room_passes_with_its_level_and_digital_silence_fails() {
        let mut room = Heard::default();
        for n in 0..50 {
            room.add(&block(n, 30)).unwrap();
        }
        let result = voice_mic(true, &Ok(room));
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert!(
            result
                .values
                .iter()
                .any(|v| v.key == "level_db" && v.value == "-60.8"),
            "{result:?}"
        );
        let mut dead = Heard::default();
        dead.add(&block(0, 0)).unwrap();
        let silent = voice_mic(true, &Ok(dead));
        assert_eq!(silent.verdict, Verdict::Fail);
        assert!(silent.reason.contains("silent"));
    }

    #[test]
    fn no_sound_is_not_checked_and_says_whether_it_was_ever_given() {
        let closed = voice_mic(true, &Ok(Heard::default()));
        assert_eq!(closed.verdict, Verdict::NotChecked);
        assert!(closed.reason.contains("privacy"));
        let never = voice_mic(false, &Ok(Heard::default()));
        assert!(never.reason.contains("LAMP_BUS_AUDIO"), "{}", never.reason);
        assert_eq!(voice_mic(true, &Err("gone".into())).verdict, Verdict::Fail);
    }

    #[test]
    fn a_second_of_blocks_is_heard_from_the_bus_with_what_was_lost() {
        let root = std::path::PathBuf::from(format!("/tmp/ls-mic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut voice = bus
            .publisher_declared(TOPIC, &Audio::declaration(), "voice")
            .unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let speaker = std::thread::spawn(move || {
            let mut sequence = 0;
            while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
                // Every tenth block is lost on the way.
                if sequence % 10 != 9 {
                    voice
                        .publish_bytes(sequence, Audio::ENCODING, &block(sequence, 300))
                        .unwrap();
                }
                sequence += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let result = VoiceMic { bus: &bus }.run(Instant::now() + Duration::from_secs(10));
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        speaker.join().unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        let value = |key: &str| {
            result
                .values
                .iter()
                .find(|v| v.key == key)
                .map(|v| v.value.parse::<u64>().unwrap())
                .unwrap()
        };
        assert!(value("blocks") >= 30, "{result:?}");
        assert!(value("lost") >= 3, "{result:?}");
    }
}
