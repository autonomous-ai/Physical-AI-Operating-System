//! The speaker and both microphones (`docs/speaker-check.md`): the voice is
//! asked to play its tone on `selftest/speaker/request` and answers on
//! `selftest/speaker` what its microphone made of it; the room microphone's
//! level (`ears/level`, lamp-ears) is read around it. Only at the factory
//! and when asked: never at first boot, so a lamp in a home does not beep
//! by itself.
use crate::{Check, listen};
use lamp_bus::Bus;
use lamp_msgs::{Occasion, PartResult, RoomSound, SpeakerCheck, SpeakerCheckRequest, Verdict};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The room's level is read this long before asking.
pub const BASELINE: Duration = Duration::from_millis(500);
/// The voice answers within this: the tone, its 0.8 s of listening, and
/// the voice's own loop.
pub const ANSWER_WITHIN: Duration = Duration::from_secs(5);
/// The room microphone's rise that counts as the tone heard. Its levels
/// are 100 ms ones, so less than the voice microphone's 10 dB.
pub const ROOM_RISE_DB: f32 = 6.0;
/// How often the room's level and the answer are looked at.
const LOOK_EVERY: Duration = Duration::from_millis(50);

/// The room microphone around the tone: the mean before, the loudest after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    pub baseline_db: f32,
    pub loudest_db: f32,
}

/// Judges the voice's answer, and the room microphone with it.
pub fn judge(answer: Option<&SpeakerCheck>, room: Option<Room>) -> PartResult {
    let part = "speaker";
    let Some(answer) = answer else {
        return PartResult::new(
            part,
            Verdict::NotChecked,
            "lamp-voice did not answer: does its unit have LAMP_SELFTEST_SPEAKER=on?",
        );
    };
    let verdict = match answer.verdict.as_str() {
        "pass" => Verdict::Pass,
        "fail" => Verdict::Fail,
        _ => Verdict::NotChecked,
    };
    let mut result = PartResult::new(part, verdict, &answer.reason);
    if let Some(rise) = answer.rise_db {
        result = result.with("voice_rise_db", format!("{rise:.1}"));
    }
    if let Some(delay) = answer.delay_ms {
        result = result.with("delay_ms", delay);
    }
    if let Some(cancelled) = answer.cancelled_db {
        result = result.with("cancelled_db", format!("{cancelled:.1}"));
    }
    match room {
        Some(room) => {
            let rise = room.loudest_db - room.baseline_db;
            result = result.with("room_rise_db", format!("{rise:.1}"));
            // Only a tone the voice played and heard says anything of the
            // room microphone.
            if verdict == Verdict::Pass && rise < ROOM_RISE_DB {
                result = PartResult {
                    verdict: Verdict::Fail,
                    reason: format!(
                        "the voice heard the tone but the room microphone rose only {rise:.1} dB"
                    ),
                    ..result
                };
            }
        }
        None => result = result.with("room", "no fresh ears/level"),
    }
    result
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The room's level now, if fresh. Taken whether or not lamp-ears marks it
/// the lamp's own voice: the tone is the lamp's own sound.
fn room_level(bus: &Bus) -> Option<f32> {
    let seen = listen::newest::<RoomSound>(bus, RoomSound::TOPIC).ok()??;
    (seen.age <= listen::FAST_FRESH).then_some(seen.value.level_db)
}

pub struct Speaker<'a> {
    pub bus: &'a Bus,
    pub occasion: Occasion,
    pub answer_within: Duration,
}

impl Check for Speaker<'_> {
    fn part(&self) -> &str {
        "speaker"
    }

    fn run(&mut self, deadline: Instant) -> PartResult {
        let occasion = match self.occasion {
            Occasion::Factory => "factory",
            Occasion::OnDemand => "on_demand",
            Occasion::FirstBoot => {
                return PartResult::new(
                    "speaker",
                    Verdict::NotChecked,
                    "not at first boot: a lamp in a home does not beep by itself",
                );
            }
        };
        let mut before = Vec::new();
        let until = (Instant::now() + BASELINE).min(deadline);
        while Instant::now() < until {
            before.extend(room_level(self.bus));
            std::thread::sleep(LOOK_EVERY);
        }
        let id = format!("selftest-{}", unix_ms());
        let asked_ms = unix_ms();
        let request = SpeakerCheckRequest {
            id: id.clone(),
            occasion: occasion.to_owned(),
        };
        let sent = self
            .bus
            .publisher_declared(
                SpeakerCheckRequest::TOPIC,
                &SpeakerCheckRequest::declaration(),
                "selftest",
            )
            .and_then(|mut asks| asks.publish_json(1, &request).map(|_| asks));
        let _asks = match sent {
            Ok(asks) => asks,
            Err(error) => {
                return PartResult::new(
                    "speaker",
                    Verdict::NotChecked,
                    &format!("{} cannot be asked: {error}", SpeakerCheckRequest::TOPIC),
                );
            }
        };
        let mut loudest: Option<f32> = None;
        let mut answer = None;
        let until = (Instant::now() + self.answer_within).min(deadline);
        while Instant::now() < until {
            if let Some(level) = room_level(self.bus) {
                loudest = Some(loudest.map_or(level, |l: f32| l.max(level)));
            }
            if let Ok(Some(message)) = self.bus.latest(SpeakerCheck::TOPIC)
                && message.header.at_unix_ms >= asked_ms
                && let Ok(said) = message.parse::<SpeakerCheck>()
                && said.id == id
            {
                answer = Some(said);
                break;
            }
            std::thread::sleep(LOOK_EVERY);
        }
        let room = (!before.is_empty())
            .then(|| before.iter().sum::<f32>() / before.len() as f32)
            .zip(loudest)
            .map(|(baseline_db, loudest_db)| Room {
                baseline_db,
                loudest_db,
            });
        judge(answer.as_ref(), room)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(verdict: &str, reason: &str) -> SpeakerCheck {
        SpeakerCheck {
            id: "x".into(),
            verdict: verdict.into(),
            reason: reason.into(),
            rise_db: (verdict == "pass").then_some(61.0),
            delay_ms: (verdict == "pass").then_some(133),
            cancelled_db: (verdict == "pass").then_some(14.8),
        }
    }

    fn value<'a>(result: &'a PartResult, key: &str) -> Option<&'a str> {
        result
            .values
            .iter()
            .find(|v| v.key == key)
            .map(|v| v.value.as_str())
    }

    #[test]
    fn a_tone_both_microphones_heard_passes_with_what_each_heard() {
        let room = Room {
            baseline_db: -60.0,
            loudest_db: -38.0,
        };
        let result = judge(Some(&answer("pass", "")), Some(room));
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert_eq!(value(&result, "delay_ms"), Some("133"));
        assert_eq!(value(&result, "room_rise_db"), Some("22.0"));
    }

    #[test]
    fn a_room_microphone_that_did_not_rise_fails_only_when_the_voice_heard_it() {
        let flat = Room {
            baseline_db: -60.0,
            loudest_db: -58.0,
        };
        let deaf = judge(Some(&answer("pass", "")), Some(flat));
        assert_eq!(deaf.verdict, Verdict::Fail);
        assert!(deaf.reason.contains("room microphone"), "{}", deaf.reason);
        let refused = judge(
            Some(&answer("not_checked", "the voice is busy")),
            Some(flat),
        );
        assert_eq!(refused.verdict, Verdict::NotChecked);
        assert_eq!(refused.reason, "the voice is busy");
        let no_room = judge(Some(&answer("pass", "")), None);
        assert_eq!(no_room.verdict, Verdict::Pass);
        assert_eq!(value(&no_room, "room"), Some("no fresh ears/level"));
    }

    #[test]
    fn no_answer_is_not_checked_and_first_boot_asks_nothing() {
        assert!(judge(None, None).reason.contains("LAMP_SELFTEST_SPEAKER"));
        let root = std::path::PathBuf::from(format!("/tmp/ls-spk-boot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let result = Speaker {
            bus: &bus,
            occasion: Occasion::FirstBoot,
            answer_within: ANSWER_WITHIN,
        }
        .run(Instant::now() + Duration::from_secs(5));
        assert_eq!(result.verdict, Verdict::NotChecked);
        assert!(bus.latest(SpeakerCheckRequest::TOPIC).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_voice_is_asked_and_its_answer_for_that_request_is_taken() {
        let root = std::path::PathBuf::from(format!("/tmp/ls-spk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        // A voice that answers each request it is given, and a room whose
        // level rises once asked.
        let voice_bus = Bus::new(&root).unwrap();
        let mut requests = voice_bus
            .subscribe_typed(
                SpeakerCheckRequest::TOPIC,
                "voice",
                SpeakerCheckRequest::TYPE,
            )
            .unwrap();
        let mut answers = voice_bus
            .publisher_declared(SpeakerCheck::TOPIC, &SpeakerCheck::declaration(), "voice")
            .unwrap();
        let mut ears = voice_bus
            .publisher_declared(RoomSound::TOPIC, &RoomSound::declaration(), "ears")
            .unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let voice = std::thread::spawn(move || {
            let mut asked = false;
            let mut clock = 0;
            while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
                clock += 1;
                let level = if asked { -35.0 } else { -60.0 };
                ears.publish_json(
                    clock,
                    &RoomSound {
                        level_db: level,
                        floor_db: None,
                        own_voice: false,
                    },
                )
                .unwrap();
                if let Ok(Some(message)) = requests.recv(Duration::from_millis(20)) {
                    let request: SpeakerCheckRequest = message.parse().unwrap();
                    assert_eq!(request.occasion, "on_demand");
                    asked = true;
                    // The tone plays, and the room hears it, before the
                    // voice answers.
                    for _ in 0..4 {
                        clock += 1;
                        ears.publish_json(
                            clock,
                            &RoomSound {
                                level_db: -35.0,
                                floor_db: None,
                                own_voice: true,
                            },
                        )
                        .unwrap();
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    answers
                        .publish_json(
                            clock,
                            &SpeakerCheck {
                                id: request.id,
                                ..answer("pass", "")
                            },
                        )
                        .unwrap();
                }
            }
        });
        let result = Speaker {
            bus: &bus,
            occasion: Occasion::OnDemand,
            answer_within: ANSWER_WITHIN,
        }
        .run(Instant::now() + Duration::from_secs(30));
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        voice.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert_eq!(value(&result, "room_rise_db"), Some("25.0"));
    }
}
