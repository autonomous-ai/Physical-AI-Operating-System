//! The idle layer, sleep and waking (task 36, #714), through the engine,
//! with the catalogue offered for lamp-4ace (`alive-behaviours.json`).
use lamp_expression::{Catalogue, Engine, Input, Output};
use lamp_msgs::{
    BodyOffsets, ClockStamp, ControlGesture, ControlIntent, ControlSource, Face, Faces, Gesture,
    JointState, Light, NameHeard, Room, RoomState, ScheduleDue, ScheduleNotice, SuddenSound, Voice,
    VoiceEvent, VoiceProgress, VoiceTiming,
};
use std::f64::consts::PI;

const CLOCK: &str = "alive-test-boot";
const TICK: u64 = 20_000;
const SECOND: u64 = 1_000_000;
const MINUTE: u64 = 60 * SECOND;
/// Well into the boot clock's first hour, where the ring's phase is plain.
const START: u64 = 1_000 * SECOND;

fn catalogue() -> Catalogue {
    Catalogue::from_json(include_str!("../alive-behaviours.json")).unwrap()
}

/// The same, falling asleep after one minute of nobody, to reach sleep soon.
fn drowsy() -> Catalogue {
    let mut value = catalogue();
    value
        .alive
        .as_mut()
        .unwrap()
        .sleep
        .as_mut()
        .unwrap()
        .nobody_us = MINUTE;
    value.check().unwrap();
    value
}

/// The ring's resting breath at `now_us` (`crates/voice/src/ring_face.rs`).
fn ring(now_us: u64) -> f64 {
    let seconds = (now_us % 3_600_000_000) as f64 / 1e6;
    0.5 - 0.5 * (2.0 * PI * seconds / 5.2).cos()
}

fn face(xmin: f32, ymin: f32, side: f32) -> Face {
    Face {
        id: "face-one".into(),
        xmin,
        ymin,
        xmax: xmin + side,
        ymax: ymin + side,
        confidence: 0.9,
        yaw: None,
        facing: None,
    }
}

struct Lamp {
    engine: Engine,
    sequence: u64,
    now: u64,
    state: Voice,
    turn: u64,
    started: u64,
    /// What the eyes report every 600 ms; `None` while they are silent.
    faces: Option<Vec<Face>>,
    /// The local minute of the day at `START`, told every minute.
    minute: Option<u16>,
}

impl Lamp {
    fn new(catalogue: Catalogue) -> Self {
        Self::at(catalogue, START)
    }

    /// Started at `now`, with the voice, the joints and the room said once.
    fn at(catalogue: Catalogue, now: u64) -> Self {
        let mut lamp = Self {
            engine: Engine::new(catalogue, true, CLOCK).unwrap(),
            sequence: 0,
            now,
            started: now,
            state: Voice::Resting,
            turn: 1,
            faces: Some(vec![]),
            minute: None,
        };
        let stamp = lamp.stamp();
        lamp.send(Input::Light {
            stamp,
            value: Light::new(true, "warm", 1).unwrap(),
        });
        lamp.say_parts();
        lamp
    }

    fn at_night(mut self) -> Self {
        self.minute = Some(23 * 60 + 30);
        self
    }

    fn stamp(&mut self) -> ClockStamp {
        self.sequence += 1;
        ClockStamp {
            clock_id: CLOCK.into(),
            at_us: self.now,
            sequence: self.sequence,
        }
    }

    fn send(&mut self, input: Input) {
        self.engine.ingest(self.now, input).unwrap();
    }

    /// A new turn of the voice in `state`, said at once.
    fn voice(&mut self, state: Voice) {
        self.state = state;
        self.turn += 1;
        self.say_voice();
    }

    fn say_voice(&mut self) {
        let timing = VoiceTiming {
            stamp: self.stamp(),
            session_id: "voice-one".into(),
            turn: self.turn,
            request: self.turn * 10 + 1,
            state: self.state,
            accepting_input: true,
            event: VoiceEvent::State,
            first_audio_us: None,
        };
        self.send(Input::Progress(VoiceProgress {
            timing,
            audio_generation: 1,
            held: false,
        }));
    }

    /// One 20 ms tick, with what the lamp's parts say at their own pace.
    fn step(&mut self) -> Output {
        self.now += TICK;
        if self.now.is_multiple_of(100_000) {
            self.say_parts();
        }
        if self.now.is_multiple_of(600_000)
            && let Some(faces) = self.faces.clone()
        {
            let stamp = self.stamp();
            self.send(Input::Faces {
                stamp,
                value: Faces { faces },
            });
        }
        if self.now.is_multiple_of(MINUTE)
            && let Some(minute) = self.minute
        {
            let since = (self.now - self.started) / MINUTE;
            let stamp = self.stamp();
            self.send(Input::DayTime {
                stamp,
                minute: ((u64::from(minute) + since) % 1_440) as u16,
            });
        }
        self.engine.tick(self.now)
    }

    /// The voice, fresh joints and a quiet room, as said every 100 ms.
    fn say_parts(&mut self) {
        self.say_voice();
        let stamp = self.stamp();
        self.send(Input::Joints {
            stamp,
            value: JointState {
                name: ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"]
                    .map(str::to_owned)
                    .to_vec(),
                position: vec![0.0; 5],
                commanded: vec![],
                velocity: vec![],
                effort: vec![],
            },
        });
        let stamp = self.stamp();
        self.send(Input::Room {
            stamp,
            value: RoomState { state: Room::Quiet },
        });
    }

    fn run(&mut self, micros: u64, mut each: impl FnMut(&Output, u64)) {
        let until = self.now + micros;
        while self.now < until {
            let output = self.step();
            each(&output, self.now);
        }
    }

    fn last(&mut self, micros: u64) -> Output {
        let mut last = None;
        self.run(micros, |output, _| last = Some(output.clone()));
        last.unwrap()
    }

    fn name(&mut self) {
        let stamp = self.stamp();
        self.send(Input::Name {
            stamp,
            value: NameHeard {
                score: 0.97,
                ago_ms: 400,
            },
        });
    }

    fn touch(&mut self, source: ControlSource, gesture: Gesture, enabled: bool) {
        let stamp = self.stamp();
        let (intent, duration_us) = match gesture {
            Gesture::Pet => (ControlIntent::Pet, 600_000),
            Gesture::LongPress => (ControlIntent::SoftwareMute, 2_100_000),
            _ => (ControlIntent::Stop, 120_000),
        };
        let value = ControlGesture {
            stamp: stamp.clone(),
            session_id: "controls-one".into(),
            source,
            name: "base".into(),
            gesture,
            intent,
            duration_us,
            released_us: self.now - 10_000,
            enabled,
        };
        self.send(Input::Touch { stamp, value });
    }

    fn due(&mut self, id: u64) {
        let stamp = self.stamp();
        let value = ScheduleDue {
            stamp: stamp.clone(),
            store_id: "0123456789abcdef0123456789abcdef".into(),
            revision: id,
            enabled: true,
            ready: true,
            pending: 1,
            waiting_for_clock: 0,
            oldest: Some(ScheduleNotice {
                id,
                delivery: 1,
                due_unix_ms: 1_760_000_000_000,
                late_by_ms: None,
                late: false,
            }),
        };
        self.send(Input::Due { stamp, value });
    }

    fn sudden(&mut self) {
        let stamp = self.stamp();
        self.send(Input::Sudden {
            stamp,
            value: SuddenSound {
                peak_db: -18.0,
                floor_db: -52.0,
                loud_ms: 60,
                short: true,
                ago_ms: 20,
            },
        });
    }

    /// Run until it is asleep, at most `micros`.
    fn sleep_within(&mut self, micros: u64) -> bool {
        let until = self.now + micros;
        while self.now < until {
            if named(&self.step(), "asleep") {
                return true;
            }
        }
        false
    }
}

fn named(output: &Output, name: &str) -> bool {
    output
        .state
        .behaviours
        .iter()
        .any(|planned| planned == name)
}

fn asked(output: &Output) -> BodyOffsets {
    output.body.as_ref().expect("a body request").offsets
}

#[test]
fn a_catalogue_without_alive_asks_nothing_of_a_resting_lamp() {
    let mut without = catalogue();
    without.alive = None;
    let mut lamp = Lamp::new(without);
    lamp.run(3 * MINUTE, |output, _| {
        assert_eq!(asked(output), BodyOffsets::ZERO);
        for name in ["breathing", "drifting", "asleep", "waking"] {
            assert!(!named(output, name));
        }
    });
}

#[test]
fn at_rest_it_breathes_with_the_ring_and_only_lifts_its_head() {
    let mut lamp = Lamp::new(catalogue());
    lamp.run(SECOND, |_, _| {});
    // The first drift comes no sooner than 30 s after the layer came in.
    let mut lowest: f64 = 0.0;
    lamp.run(20 * SECOND, |output, now| {
        let offsets = asked(output);
        assert!(
            (offsets.head_pitch - -0.03 * ring(now)).abs() < 1e-9,
            "{offsets:?} at {now}"
        );
        assert_eq!(offsets.head_roll, 0.0);
        assert_eq!(offsets.base_yaw, 0.0);
        assert!(named(output, "breathing"));
        lowest = lowest.min(offsets.head_pitch);
    });
    // A positive pitch dips the head; a resting lamp's head only lifts.
    assert!((lowest - -0.03).abs() < 1e-4, "{lowest}");
}

#[test]
fn the_layer_comes_in_and_goes_out_within_its_blend() {
    let mut lamp = Lamp::new(catalogue());
    let first = lamp.step();
    assert_eq!(asked(&first).head_pitch, 0.0, "it comes in from nothing");
    let came_in = lamp.now;
    lamp.run(SECOND, |output, now| {
        if now >= came_in + 350_000 {
            let wanted = -0.03 * ring(now);
            assert!((asked(output).head_pitch - wanted).abs() < 1e-9);
        }
    });
    // Someone speaks to it: the layer is gone within 350 ms, and what is
    // left is exactly the listening posture.
    lamp.voice(Voice::Listening);
    let left = lamp.now;
    let listening = catalogue().postures.listening;
    lamp.run(SECOND, |output, now| {
        if now >= left + 350_000 {
            assert_eq!(asked(output), listening);
            assert!(!named(output, "breathing"));
        }
    });
}

#[test]
fn a_drift_comes_now_and_then_within_its_bounds_and_never_as_a_twitch() {
    let mut lamp = Lamp::new(catalogue());
    // Past the layer's coming in, which is quicker by design (0.35 s).
    lamp.run(SECOND, |_, _| {});
    let mut starts = Vec::new();
    let mut drifting = false;
    let mut last: Option<BodyOffsets> = None;
    let mut largest = [0.0_f64; 2];
    lamp.run(15 * MINUTE, |output, now| {
        let offsets = asked(output);
        // The breath lifts up to 0.03 and a drift up to 0.035; a drift
        // tilts up to 0.02 either way.
        assert!(
            (-0.065 - 1e-9..=0.0).contains(&offsets.head_pitch),
            "{offsets:?}"
        );
        assert!(offsets.head_roll.abs() <= 0.02 + 1e-9, "{offsets:?}");
        if let Some(last) = last {
            largest[0] = largest[0].max((offsets.head_pitch - last.head_pitch).abs());
            largest[1] = largest[1].max((offsets.head_roll - last.head_roll).abs());
        }
        last = Some(offsets);
        let now_drifting = named(output, "drifting");
        if now_drifting && !drifting {
            starts.push(now);
        }
        drifting = now_drifting;
    });
    // From one start to the next: the drift's 3 s and then 30 to 90 s.
    let gaps: Vec<u64> = starts.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(starts.len() >= 9, "{starts:?}");
    for gap in &gaps {
        assert!(
            (33 * SECOND - TICK..=93 * SECOND + TICK).contains(gap),
            "{gaps:?}"
        );
    }
    // The breath's fastest is pi * 0.03 / 5.2 = 0.018 rad/s and a drift's
    // 1.5 * 0.035 / 3 = 0.0175 rad/s of pitch and 0.02 of roll: under
    // 0.75 and 0.4 milliradians a tick. No leap anywhere.
    assert!(largest[0] <= 0.00075, "{largest:?}");
    assert!(largest[1] <= 0.00041, "{largest:?}");
}

#[test]
fn where_the_ring_jumps_on_the_hour_the_head_does_not() {
    let mut lamp = Lamp::at(catalogue(), 3_590 * SECOND);
    lamp.run(2 * SECOND, |_, _| {});
    let mut last: Option<f64> = None;
    let mut largest: f64 = 0.0;
    lamp.run(20 * SECOND, |output, now| {
        let pitch = asked(output).head_pitch;
        if let Some(last) = last {
            largest = largest.max((pitch - last).abs());
        }
        last = Some(pitch);
        // Some seconds after the hour it is with the ring again.
        if now >= 3_605 * SECOND {
            assert!((pitch - -0.03 * ring(now)).abs() < 1e-9, "{pitch} at {now}");
        }
    });
    assert!(largest <= 0.00075, "{largest}");
}

#[test]
fn someone_working_close_and_still_under_it_has_a_still_lamp() {
    let mut lamp = Lamp::new(catalogue());
    lamp.run(2 * SECOND, |_, _| {});
    // A face close (0.09 of the picture) and still.
    lamp.faces = Some(vec![face(0.35, 0.3, 0.3)]);
    lamp.run(5 * SECOND + 600_000, |_, _| {});
    let mut still = 0;
    lamp.run(5 * SECOND, |output, _| {
        if named(output, "still for work") {
            still += 1;
        }
    });
    let output = lamp.last(TICK);
    assert_eq!(asked(&output), BodyOffsets::ZERO);
    assert!(!named(&output, "breathing"));
    assert!(still > 200, "{still}");
    // Bent over the work, no face at all: still still for 30 s, then the
    // breath comes back.
    lamp.faces = Some(vec![]);
    lamp.run(28 * SECOND, |output, _| {
        assert_eq!(asked(output), BodyOffsets::ZERO);
    });
    let output = lamp.last(5 * SECOND);
    assert!(named(&output, "breathing"));
}

#[test]
fn a_close_face_that_moves_is_not_someone_working() {
    let mut lamp = Lamp::new(catalogue());
    let mut x = 0.1;
    for _ in 0..30 {
        lamp.faces = Some(vec![face(x, 0.3, 0.3)]);
        x = if x > 0.5 { 0.1 } else { x + 0.1 };
        lamp.run(SECOND, |output, _| {
            assert!(!named(output, "still for work"));
        });
    }
    assert!(named(&lamp.last(TICK), "breathing"));
}

#[test]
fn late_at_night_with_nobody_there_it_falls_asleep_into_a_slow_breath() {
    let mut lamp = Lamp::new(catalogue()).at_night();
    lamp.run(14 * MINUTE, |output, _| {
        assert!(!named(output, "asleep") && !named(output, "falling asleep"));
    });
    let mut falling = 0;
    let mut asleep_at = None;
    // Nodding off, the head goes no faster than the pose and the fading
    // breath and drift together allow: under a milliradian a tick.
    let mut last: Option<BodyOffsets> = None;
    let mut largest: f64 = 0.0;
    lamp.run(2 * MINUTE, |output, now| {
        let offsets = asked(output);
        if let Some(last) = last {
            for (now, then) in offsets.values().iter().zip(last.values()) {
                largest = largest.max((now - then).abs());
            }
        }
        last = Some(offsets);
        if named(output, "falling asleep") {
            falling += 1;
        }
        if named(output, "asleep") {
            asleep_at.get_or_insert(now);
        }
    });
    // 15 minutes of nobody, then 4 s into the pose.
    let asleep_at = asleep_at.expect("asleep");
    assert!(largest <= 0.001, "{largest}");
    assert!((15 * MINUTE..15 * MINUTE + 5 * SECOND).contains(&(asleep_at - START)));
    assert_eq!(falling, 200, "four seconds of 20 ms ticks");
    // Asleep: the head a little bowed, lifting only with a slow breath,
    // nothing else.
    lamp.run(MINUTE, |output, _| {
        let offsets = asked(output);
        assert!((0.008 - 1e-9..=0.023 + 1e-9).contains(&offsets.head_pitch));
        assert_eq!(offsets.head_roll, 0.0);
        assert!(named(output, "asleep"));
        assert!(!named(output, "breathing"));
    });
}

#[test]
fn asleep_a_bang_or_talk_in_the_room_does_not_wake_it() {
    let mut lamp = Lamp::new(drowsy()).at_night();
    assert!(lamp.sleep_within(2 * MINUTE));
    lamp.run(5 * SECOND, |_, _| {});
    lamp.sudden();
    lamp.run(2 * SECOND, |output, _| {
        assert!(named(output, "asleep"));
        assert!(!named(output, "glance at a sound"));
        assert!(asked(output).head_pitch >= 0.008 - 1e-9);
    });
    // Someone talks, not to it: the voice listens, the lamp sleeps on and
    // Expression asks for no listening posture.
    lamp.voice(Voice::Listening);
    lamp.run(3 * SECOND, |output, _| {
        assert!(named(output, "asleep"));
        assert!(asked(output).head_pitch >= 0.008 - 1e-9);
    });
    lamp.voice(Voice::Resting);
    assert!(named(&lamp.last(SECOND), "asleep"));
}

#[test]
fn its_name_wakes_it_with_a_stretch_and_no_leap() {
    let mut lamp = Lamp::new(drowsy()).at_night();
    assert!(lamp.sleep_within(2 * MINUTE));
    lamp.run(10 * SECOND, |_, _| {});
    lamp.name();
    lamp.voice(Voice::Listening);
    let woken = lamp.now;
    let mut highest: f64 = 0.0;
    let mut last: Option<BodyOffsets> = None;
    let mut largest: f64 = 0.0;
    let mut waking = 0;
    lamp.run(4 * SECOND, |output, _| {
        let offsets = asked(output);
        if named(output, "waking") {
            waking += 1;
        }
        highest = highest.min(offsets.head_pitch);
        if let Some(last) = last {
            for (now, then) in offsets.values().iter().zip(last.values()) {
                largest = largest.max((now - then).abs());
            }
        }
        last = Some(offsets);
    });
    // The stretch: up to 0.065 radians of lift at energy 0.5, over 2.7 s.
    // From the tick it is woken to the one 2.7 s later, both counted.
    assert_eq!(waking, 136, "2.7 s of 20 ms ticks and the first");
    assert!(highest < -0.05, "{highest}");
    // A stretch at most 0.35 rad/s, the bow and the posture crossed over:
    // under 0.01 radians a tick.
    assert!(largest < 0.01, "{largest}");
    // Awake and listening: the listening posture exactly.
    let output = lamp.last(TICK);
    assert!(!named(&output, "asleep") && !named(&output, "waking"));
    assert_eq!(asked(&output), catalogue().postures.listening);
    assert!(lamp.now - woken > 2_700_000);
}

#[test]
fn a_tap_a_stroke_a_reminder_or_its_own_answer_wake_it() {
    let wakes = |act: &dyn Fn(&mut Lamp)| {
        let mut lamp = Lamp::new(drowsy()).at_night();
        assert!(lamp.sleep_within(2 * MINUTE));
        act(&mut lamp);
        named(&lamp.last(2 * TICK), "waking")
    };
    assert!(wakes(&|lamp| lamp.touch(
        ControlSource::Base,
        Gesture::Tap,
        true
    )));
    assert!(wakes(&|lamp| lamp.touch(
        ControlSource::Headpad,
        Gesture::Pet,
        true
    )));
    assert!(wakes(&|lamp| lamp.due(7)));
    assert!(wakes(&|lamp| lamp.voice(Voice::Speaking)));
    // A held base is the mute today and Sleep once #711 says so; a touch
    // read in shadow is no touch at all.
    assert!(!wakes(&|lamp| lamp.touch(
        ControlSource::Base,
        Gesture::LongPress,
        true
    )));
    assert!(!wakes(&|lamp| lamp.touch(
        ControlSource::Base,
        Gesture::Tap,
        false
    )));
}

#[test]
fn the_same_reminder_wakes_it_once() {
    let mut lamp = Lamp::new(drowsy()).at_night();
    assert!(lamp.sleep_within(2 * MINUTE));
    lamp.due(7);
    assert!(named(&lamp.last(2 * TICK), "waking"));
    assert!(lamp.sleep_within(3 * MINUTE));
    // Still pending, said again every second: it sleeps on.
    for _ in 0..5 {
        lamp.due(7);
        assert!(named(&lamp.last(SECOND), "asleep"));
    }
    lamp.due(8);
    assert!(named(&lamp.last(2 * TICK), "waking"));
}

#[test]
fn a_face_at_night_lets_it_sleep_and_the_first_face_of_the_morning_wakes_it() {
    let mut lamp = Lamp::new(drowsy());
    // 05:57: asleep a minute later, still night.
    lamp.minute = Some(5 * 60 + 57);
    assert!(lamp.sleep_within(2 * MINUTE));
    lamp.faces = Some(vec![face(0.1, 0.1, 0.1)]);
    lamp.run(5 * SECOND, |output, _| assert!(named(output, "asleep")));
    // Nobody again, past 06:00 (told on the minute), then the first person
    // of the day.
    lamp.faces = Some(vec![]);
    lamp.run(3 * MINUTE, |output, _| assert!(named(output, "asleep")));
    lamp.faces = Some(vec![face(0.1, 0.1, 0.1)]);
    assert!(named(&lamp.last(SECOND), "waking"));
}

#[test]
fn it_does_not_sleep_by_day_without_eyes_or_without_the_time() {
    let stays_awake = |lamp: &mut Lamp| !lamp.sleep_within(5 * MINUTE);
    // By day.
    let mut day = Lamp::new(drowsy());
    day.minute = Some(10 * 60);
    assert!(stays_awake(&mut day));
    // At night, but the eyes are silent: nobody cannot be said to be gone.
    let mut blind = Lamp::new(drowsy()).at_night();
    blind.faces = None;
    assert!(stays_awake(&mut blind));
    // At night with nobody, but no time of day was ever told.
    let mut timeless = Lamp::new(drowsy());
    assert!(stays_awake(&mut timeless));
    // At night, with someone passing every ten minutes.
    let mut visited = Lamp::new(catalogue()).at_night();
    for _ in 0..3 {
        visited.faces = Some(vec![face(0.1, 0.1, 0.1)]);
        visited.run(SECOND, |_, _| {});
        visited.faces = Some(vec![]);
        assert!(!visited.sleep_within(10 * MINUTE));
    }
}

#[test]
fn energy_changes_the_size_and_pace_of_the_same_moves() {
    let breath_at = |energy: f32| {
        let mut value = catalogue();
        value.alive.as_mut().unwrap().energy = energy;
        value.check().unwrap();
        let mut lamp = Lamp::new(value);
        lamp.run(SECOND, |_, _| {});
        // The breath alone, until the first drift.
        let mut lowest: f64 = 0.0;
        let mut first_drift = None;
        lamp.run(2 * MINUTE, |output, now| {
            if named(output, "drifting") {
                first_drift.get_or_insert(now - START);
            }
            if first_drift.is_none() {
                lowest = lowest.min(asked(output).head_pitch);
            }
        });
        (lowest, first_drift.unwrap())
    };
    let (quiet, quiet_drift) = breath_at(0.0);
    let (lively, lively_drift) = breath_at(1.0);
    // 0.7 and 1.3 times the breath's 0.03.
    assert!((quiet - -0.021).abs() < 1e-4, "{quiet}");
    assert!((lively - -0.039).abs() < 1e-4, "{lively}");
    // The same drift, waited for 1.25 and 0.75 times as long.
    let ratio = quiet_drift as f64 / lively_drift as f64;
    assert!(
        (ratio - 1.25 / 0.75).abs() < 0.02,
        "{quiet_drift} {lively_drift}"
    );
}

#[test]
fn the_catalogue_refuses_a_layer_that_could_twitch_or_reach_too_far() {
    let refused = |change: &dyn Fn(&mut Catalogue)| {
        let mut value = catalogue();
        change(&mut value);
        value.check().is_err()
    };
    assert!(!refused(&|_| {}));
    assert!(refused(&|v| v
        .alive
        .as_mut()
        .unwrap()
        .states
        .push(Voice::Speaking)));
    assert!(refused(&|v| v.alive.as_mut().unwrap().states.clear()));
    assert!(refused(&|v| v.alive.as_mut().unwrap().blend_us = 50_000));
    assert!(refused(&|v| v.alive.as_mut().unwrap().energy = 1.5));
    // A breath too large, too quick, or of the arm.
    assert!(refused(&|v| v
        .alive
        .as_mut()
        .unwrap()
        .breath
        .inhale
        .head_pitch = -0.05));
    assert!(refused(
        &|v| v.alive.as_mut().unwrap().breath.period_us = 2_000_000
    ));
    assert!(refused(&|v| v
        .alive
        .as_mut()
        .unwrap()
        .breath
        .inhale
        .shoulder = 0.01));
    // A drift with zero outside its range, too far, too quick.
    assert!(refused(&|v| v
        .alive
        .as_mut()
        .unwrap()
        .drift
        .low
        .head_pitch = 0.01));
    assert!(refused(&|v| v
        .alive
        .as_mut()
        .unwrap()
        .drift
        .low
        .head_pitch = -0.05));
    assert!(refused(
        &|v| v.alive.as_mut().unwrap().drift.move_us = 1_000_000
    ));
    assert!(refused(
        &|v| v.alive.as_mut().unwrap().drift.every_us = [90_000_000, 30_000_000]
    ));
    assert!(refused(&|v| v
        .alive
        .as_mut()
        .unwrap()
        .drift
        .high
        .base_yaw = 0.01));
    // Sleep: no night, too soon, a pose too far.
    assert!(refused(&|v| {
        v.alive.as_mut().unwrap().sleep.as_mut().unwrap().night = [600, 600]
    }));
    assert!(refused(&|v| {
        v.alive.as_mut().unwrap().sleep.as_mut().unwrap().nobody_us = 10_000_000
    }));
    assert!(refused(&|v| {
        v.alive
            .as_mut()
            .unwrap()
            .sleep
            .as_mut()
            .unwrap()
            .pose
            .head_pitch = 0.1
    }));
    // A stretch with no sleep, not back where it began, or a snap.
    assert!(refused(&|v| v.alive.as_mut().unwrap().sleep = None));
    assert!(refused(&|v| {
        let wake = v.alive.as_mut().unwrap().wake.as_mut().unwrap();
        wake.keyframes.last_mut().unwrap().offsets.head_pitch = -0.01;
    }));
    assert!(refused(&|v| {
        let wake = v.alive.as_mut().unwrap().wake.as_mut().unwrap();
        wake.keyframes[1].at_us = 100_000;
    }));
    assert!(refused(&|v| {
        let wake = v.alive.as_mut().unwrap().wake.as_mut().unwrap();
        wake.keyframes[2].offsets.elbow = 0.01;
    }));
}
