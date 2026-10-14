//! What the voice says of its own timing, taken by the part it is said
//! for: Expression's planner (`lamp_expression::Engine`), which refuses
//! anything out of order, stale, or belonging to a turn that is over.
//!
//! One made-up minute of the voice's loop, passed through
//! `lamp_voice::expression_out::Timing` every 10 ms and into the planner
//! as it would arrive on the bus. Nothing may be refused, and the planner
//! must make of it the behaviours it was written for.
use lamp_expression::{Catalogue, Engine, Input};
use lamp_voice::{
    expression_out::{Said, Seen, Timing, sounds},
    ring_wire::RingPhase,
};
use serde_json::json;

const MS: u64 = 1_000;
const BOOT: &str = "this-boot";

/// What the room, the light and the body would say, as the planner takes
/// them from the bus.
fn context(engine: &mut Engine, now_us: u64, sequence: u64) {
    for input in [
        json!({"kind":"room","stamp":{"clock_id":BOOT,"at_us":now_us,"sequence":sequence},"value":{"state":"quiet"}}),
        json!({"kind":"light","stamp":{"clock_id":BOOT,"at_us":now_us,"sequence":sequence},"value":{"on":true,"colour":"warm","level":3}}),
        json!({"kind":"joints","stamp":{"clock_id":BOOT,"at_us":now_us,"sequence":sequence},
            "value":{"name":["base_yaw","shoulder","elbow","head_roll","head_pitch"],"position":[0,0,0,0,0],"velocity":[],"effort":[],"commanded":[]}}),
    ] {
        let input: Input = serde_json::from_value(input).unwrap();
        engine.ingest(now_us, input).unwrap();
    }
}

/// The voice at `at` milliseconds of the scene: the turn in hand, how far
/// it has got, whether the person is speaking, whether the lamp is
/// holding back for someone, when its first sound is expected, and
/// whether the speaker is being fed.
struct Moment {
    turn: Option<u64>,
    phase: Option<RingPhase>,
    person_speaking: bool,
    yielding: bool,
    first_sound_ms: Option<u64>,
    playing: bool,
}

fn scene(at: u64) -> Moment {
    let quiet = Moment {
        turn: None,
        phase: None,
        person_speaking: false,
        yielding: false,
        first_sound_ms: None,
        playing: false,
    };
    match at {
        // A question with a pause in the middle of it.
        1_000..2_200 => Moment {
            turn: Some(1),
            phase: Some(RingPhase::Listening),
            person_speaking: !(1_500..1_900).contains(&at),
            ..quiet
        },
        // The answer is awaited; from 2.9 s its first sound is known to
        // be due at 3.2 s.
        2_200..3_200 => Moment {
            turn: Some(1),
            phase: Some(RingPhase::Waiting),
            first_sound_ms: (at >= 2_900).then_some(3_200),
            ..quiet
        },
        // A second of the lamp speaking, and then it is over.
        3_200..4_200 => Moment {
            turn: Some(1),
            phase: Some(RingPhase::Speaking),
            playing: true,
            ..quiet
        },
        // A second question, thrown out as a sound: no answer.
        6_000..6_400 => Moment {
            turn: Some(2),
            phase: Some(RingPhase::Listening),
            person_speaking: true,
            ..quiet
        },
        // A third, answered, and the answer cut off by someone speaking:
        // the lamp holds its sound back from 9.4 s and their turn begins
        // at 9.7 s.
        8_000..8_600 => Moment {
            turn: Some(3),
            phase: Some(RingPhase::Listening),
            person_speaking: true,
            ..quiet
        },
        8_600..9_000 => Moment {
            turn: Some(3),
            phase: Some(RingPhase::Waiting),
            first_sound_ms: (at >= 8_800).then_some(9_000),
            ..quiet
        },
        9_000..9_700 => Moment {
            turn: Some(3),
            phase: Some(RingPhase::Speaking),
            playing: at < 9_400,
            yielding: at >= 9_400,
            person_speaking: at >= 9_400,
            ..quiet
        },
        9_700..10_500 => Moment {
            turn: Some(4),
            phase: Some(RingPhase::Listening),
            person_speaking: true,
            ..quiet
        },
        _ => quiet,
    }
}

#[test]
fn the_planner_takes_everything_the_voice_says_and_plans_what_it_was_written_for() {
    let mut engine = Engine::new(Catalogue::builtin(), true, BOOT).unwrap();
    let mut timing = Timing::new(BOOT, "voice-run-1");
    let mut refused = Vec::new();
    let mut behaviours: Vec<(u64, String)> = Vec::new();
    let mut said = [0usize; 3];
    let mut faces: Vec<(u64, String)> = Vec::new();
    let mut warned: Vec<(u64, String)> = Vec::new();
    for step in 0..1_200u64 {
        let at = step * 10;
        let now_us = at * MS;
        if step % 10 == 0 {
            context(&mut engine, now_us, step / 10 + 1);
        }
        let moment = scene(at);
        let mut out = timing.tick(Seen {
            now_us,
            turn: moment.turn,
            phase: moment.phase,
            accepting_input: true,
            yielding: moment.yielding,
            person_speaking: moment.person_speaking,
            first_sound_us: moment.first_sound_ms.map(|ms| ms * MS),
        });
        if moment.playing {
            // The speaker took 10 ms; its queue then holds 40 ms.
            let (start_us, end_us) = sounds(240, 960, now_us);
            out.extend(timing.played(moment.turn.unwrap(), start_us, end_us, 150, now_us));
        }
        for message in out {
            let (stream, input) = match message {
                Said::Timing(value) => (0, Input::Voice(value)),
                Said::Activity(value) => (1, Input::Activity(value)),
                Said::Playout(value) => (2, Input::Playout(value)),
            };
            said[stream] += 1;
            let shown = format!("{input:?}");
            if let Err(why) = engine.ingest(now_us, input) {
                refused.push(format!("at {at} ms: {why}: {shown}"));
            }
        }
        if step % 2 == 0 {
            let plan = engine.tick(now_us);
            for warning in plan.state.warnings {
                if !warned.iter().any(|(_, seen)| *seen == warning) {
                    warned.push((at, warning));
                }
            }
            for behaviour in plan.state.behaviours {
                if !behaviours.iter().any(|(_, seen)| *seen == behaviour) {
                    behaviours.push((at, behaviour));
                }
            }
            if let Some(light) = plan.light {
                let face = format!("{:?}", light.face);
                if faces.last().is_none_or(|(_, last)| *last != face) {
                    faces.push((at, face));
                }
            }
        }
    }
    assert!(
        refused.is_empty(),
        "the planner refused {} of what the voice said:\n{}",
        refused.len(),
        refused[..refused.len().min(12)].join("\n")
    );
    assert_eq!(engine.rejected_inputs(), 0);
    assert_eq!(timing.held_back, 0);
    // Something of each kind was said: the state every 80 ms and on each
    // change, the person's speech while they were heard, and the lamp's
    // sound in stretches of 20 ms for the 1.4 s it played.
    assert!(said[0] >= 150, "{said:?}");
    assert!(said[1] >= 20, "{said:?}");
    assert!((60..=75).contains(&said[2]), "{said:?}");
    // And the planner made of it what it was written to make.
    let planned: Vec<&str> = behaviours.iter().map(|(_, name)| name.as_str()).collect();
    for wanted in [
        "nod in a pause",
        "look away and back",
        "follow played sound",
        "yield when interrupted",
    ] {
        assert!(
            planned.contains(&wanted),
            "{wanted} was never planned; planned: {behaviours:?}"
        );
    }
    // The nod came in the pause, the glance while the answer was awaited,
    // and the yield when the lamp was cut off, not before.
    let first = |name: &str| behaviours.iter().find(|(_, seen)| seen == name).unwrap().0;
    assert!(
        (1_500..1_900).contains(&first("nod in a pause")),
        "{behaviours:?}"
    );
    assert!(
        (2_200..3_200).contains(&first("look away and back")),
        "{behaviours:?}"
    );
    assert!(
        (3_200..4_300).contains(&first("follow played sound")),
        "{behaviours:?}"
    );
    assert!(
        (9_700..10_000).contains(&first("yield when interrupted")),
        "{behaviours:?}"
    );
    // After the turn nothing was said to (6.0 to 6.4 s) the planner goes on
    // asking, for a lamp at rest: an unanswered turn is no fault to it.
    let shown: Vec<(u64, &str)> = faces
        .iter()
        .map(|(at, face)| (*at, face.as_str()))
        .collect();
    assert!(
        shown
            .windows(2)
            .any(|pair| pair[0] == (6_000, "Listening") && pair[1] == (6_400, "Resting")),
        "{faces:?}"
    );
    // The planner's one note: the third answer's first sound was foretold
    // 200 ms ahead, under the 290 ms it wants for an unhurried look back.
    assert_eq!(
        warned,
        [(
            8_800,
            "short audio notice; return shortened, arrival unverified".to_owned()
        )]
    );
    println!("behaviours first planned at (ms): {behaviours:?}");
    println!("faces asked for at (ms): {faces:?}");
    println!(
        "said: {} timing, {} activity, {} playout",
        said[0], said[1], said[2]
    );
}
