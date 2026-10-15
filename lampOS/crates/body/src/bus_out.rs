//! What the body says on the lamp's bus (`docs/bus.md`): where every joint
//! is and where it was last asked to be (`body/joint_states`, the type
//! `lamp_msgs::JointState`), and once a second how the body is
//! (`diagnostics`, `lamp_msgs::Diagnostic`).
//!
//! The body's loop reads and writes the servos every 20 ms and must never
//! wait for anything else. So nothing is published from it: it offers a
//! step to a thread of this module's own, without waiting, and a step that
//! thread has no room for is let go.
use crate::{
    body::{Glimpse, Stop},
    gesture::Mood,
};
use lamp_bus::{Bus, Publisher};
use lamp_msgs::{Diagnostic, JointState, KeyValue, Level};
use std::{
    f64::consts::TAU,
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// Where the body reads whether to publish. `on` only.
pub const ENVIRONMENT: &str = "LAMP_BUS";
/// The name the body publishes under.
pub const SOURCE: &str = "body";
/// The joints, in the order of their servos' numbers, 1 to 5. Every list
/// of a `JointState` from here is in this order.
pub const JOINTS: [&str; 5] = ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"];
/// One step in this many is offered: a step is 20 ms, so 12.5 messages a
/// second, against the ten the topic promises.
pub const EVERY_STEPS: u64 = 4;
/// A step of the body, in seconds.
const STEP_S: f64 = 0.02;
/// The body's report goes with one offered step in this many: every
/// 0.96 s, inside the second the topic promises.
const REPORT_EVERY: u32 = 12;
/// How long a message of where the joints are means anything. A body that
/// has stopped must not go on being read as where the lamp is.
pub const LASTS: Duration = Duration::from_secs(1);
/// A servo's turn is 4,096 counts, and its middle is the angle nothing.
const COUNTS_A_TURN: f64 = 4096.0;
const MIDDLE: f64 = 2048.0;
/// What a servo reports as its load is in thousandths of all it can exert.
const FULL_LOAD: f64 = 1000.0;
/// A joint this warm is said to be warm. The body ends its run at 62 on a
/// joint it commands and sends the arm down at the same reading; this is
/// far enough under to be told before that.
pub const WARM: u8 = 55;
/// Resistance met is said for this long after it happened.
const RESISTANCE_SAID_FOR: Duration = Duration::from_secs(60);

fn radians(counts: u16) -> f64 {
    (f64::from(counts) - MIDDLE) * TAU / COUNTS_A_TURN
}

/// The five joints of a glimpse in the order of [`JOINTS`]: each one's
/// position and its goal, in counts.
fn in_order(glimpse: &Glimpse) -> ([u16; 5], [u16; 5]) {
    // A glimpse has pitch, roll, yaw, and then shoulder, elbow.
    let [pitch, roll, yaw] = glimpse.positions;
    let [shoulder, elbow] = glimpse.arm_positions;
    let [pitch_goal, roll_goal, yaw_goal] = glimpse.goals;
    let [shoulder_goal, elbow_goal] = glimpse.arm_goals;
    (
        [yaw, shoulder, elbow, roll, pitch],
        [yaw_goal, shoulder_goal, elbow_goal, roll_goal, pitch_goal],
    )
}

/// Where the joints are at this step, as the message.
///
/// Angles are in radians from the middle of each servo's turn, the way
/// its own counts grow. `effort` is not ROS's newton metres: it is the
/// share of its full strength each servo reports, from 0 to 1, without a
/// direction. `velocity` is worked out from where the joints were at
/// `before` (a step's number and its positions in counts), and is left
/// empty when there is no such step.
pub fn joint_state(glimpse: &Glimpse, before: Option<(u64, [u16; 5])>) -> JointState {
    let (positions, goals) = in_order(glimpse);
    let [pitch_load, roll_load, yaw_load] = glimpse.loads;
    let [shoulder_load, elbow_load] = glimpse.arm_loads;
    let loads = [yaw_load, shoulder_load, elbow_load, roll_load, pitch_load];
    let velocity = match before {
        Some((step, was)) if glimpse.step > step => {
            let seconds = (glimpse.step - step) as f64 * STEP_S;
            positions
                .iter()
                .zip(was)
                .map(|(now, was)| (radians(*now) - radians(was)) / seconds)
                .collect()
        }
        _ => Vec::new(),
    };
    JointState {
        name: JOINTS.map(str::to_owned).to_vec(),
        position: positions.map(radians).to_vec(),
        velocity,
        effort: loads.map(|load| f64::from(load) / FULL_LOAD).to_vec(),
        commanded: goals.map(radians).to_vec(),
    }
}

fn mood_word(mood: Mood) -> &'static str {
    match mood {
        Mood::Resting => "resting",
        Mood::Listening => "listening",
        Mood::Thinking => "thinking",
        Mood::Speaking => "speaking",
    }
}

fn fact(key: &str, value: impl ToString) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: value.to_string(),
    }
}

/// How the body is at this step, as its report. `resisted` is how many
/// times a joint has met resistance within the last minute.
///
/// It is well unless a joint is warm, or one lately met something and
/// stopped pushing. Neither stops the body: both are what a person would
/// want to know before it does.
pub fn health(glimpse: &Glimpse, resisted: u32) -> Diagnostic {
    // Pitch, roll, yaw, and then shoulder, elbow, as a glimpse has them.
    let warmth = [
        ("head_pitch", glimpse.temperatures[0]),
        ("head_roll", glimpse.temperatures[1]),
        ("base_yaw", glimpse.temperatures[2]),
        ("shoulder", glimpse.arm_temperatures[0]),
        ("elbow", glimpse.arm_temperatures[1]),
    ];
    let (warmest, reading) = warmth
        .into_iter()
        .max_by_key(|(_, reading)| *reading)
        .unwrap_or(("head_pitch", 0));
    let (level, message) = if reading >= WARM {
        (
            Level::Warn,
            format!("{warmest} is warm: it reads {reading}, and the body stops at 62"),
        )
    } else if resisted > 0 {
        (
            Level::Warn,
            format!(
                "a joint met resistance and stopped pushing, {resisted} time{} in the last minute",
                if resisted == 1 { "" } else { "s" }
            ),
        )
    } else {
        (Level::Ok, String::new())
    };
    Diagnostic {
        level,
        name: SOURCE.to_owned(),
        message,
        values: vec![
            fact("mood", mood_word(glimpse.mood)),
            fact("standing", format!("{:.2}", glimpse.stand)),
            fact("warmest_joint", warmest),
            fact("warmest_reading", reading),
            fact("resisted_last_minute", resisted),
            fact(
                "resisted_since_start",
                glimpse.eased.iter().map(|n| u32::from(*n)).sum::<u32>(),
            ),
            // A step has run: the start checks are behind it.
            fact(lamp_keeper::STATE, lamp_keeper::PartState::Ready.as_str()),
        ],
    }
}

/// The state said in `report` becomes `paused`.
fn said_paused(report: &mut Diagnostic) {
    for value in &mut report.values {
        if value.key == lamp_keeper::STATE {
            value.value = lamp_keeper::PartState::Paused.as_str().to_owned();
        }
    }
}

/// A joint by its servo's number, for a person.
fn joint_word(id: u8) -> &'static str {
    usize::from(id)
        .checked_sub(1)
        .and_then(|index| JOINTS.get(index))
        .copied()
        .unwrap_or("a joint")
}

/// Why the body stopped, as its last report. Only being asked to is not a
/// fault, and even then the lamp no longer moves: a warning.
pub fn stopped(stop: &Stop) -> Diagnostic {
    let (level, message) = match stop {
        Stop::Cancelled => (Level::Warn, "stopped, as it was asked to".to_owned()),
        Stop::Refused { why } => (Level::Error, format!("would not start: {why}")),
        Stop::Load { joint, load } => (
            Level::Error,
            format!(
                "stopped: {} was pushed against (load {load})",
                joint_word(*joint)
            ),
        ),
        Stop::Tracking {
            joint,
            goal,
            position,
        } => (
            Level::Error,
            format!(
                "stopped: {} did not follow (asked {goal}, at {position})",
                joint_word(*joint)
            ),
        ),
        Stop::Status { joint, found, now } => (
            Level::Error,
            format!(
                "stopped: the servo of {} reports a fault (status {found:#04x} became {now:#04x})",
                joint_word(*joint)
            ),
        ),
        Stop::Drift { joint, found, now } => (
            Level::Error,
            format!(
                "stopped: {} was moved by something else (from {found} to {now})",
                joint_word(*joint)
            ),
        ),
        Stop::Temperature { joint, temperature } => (
            Level::Error,
            format!(
                "stopped: {} is too warm (it reads {temperature})",
                joint_word(*joint)
            ),
        ),
        Stop::Bus { .. } => (
            Level::Error,
            "stopped: the servos did not answer as expected".to_owned(),
        ),
        Stop::Overload {
            joint,
            load,
            detail,
        } => (
            Level::Error,
            format!(
                "stopped with its torque off: {} was overloaded (load {load}); {detail}",
                joint_word(*joint)
            ),
        ),
    };
    Diagnostic {
        level,
        name: SOURCE.to_owned(),
        message,
        values: vec![fact(
            lamp_keeper::STATE,
            lamp_keeper::PartState::Stopping.as_str(),
        )],
    }
}

/// What the thread is given.
enum Out {
    /// A step, and whether the lamp is paused.
    Step(Glimpse, bool),
    Stopped(Diagnostic),
}

/// The way to the thread that publishes.
pub struct BusOut {
    steps: mpsc::SyncSender<Out>,
    thread: Option<thread::JoinHandle<()>>,
    /// Steps the thread had no room for.
    pub let_go: u64,
    /// The lamp is paused (`lamp/pause`): the state it says is `paused`.
    pub paused: bool,
}

/// What the publishing thread holds between two steps.
struct Publishing {
    joints: Publisher,
    health: Publisher,
    started: Instant,
    before: Option<(u64, [u16; 5])>,
    /// Steps offered since the body's report was last sent, and each time
    /// a joint met resistance, with how many such times there had been in
    /// all.
    since_report: Option<u32>,
    resisted: Vec<Instant>,
    resisted_in_all: u32,
}

impl Publishing {
    fn on(bus: &Bus) -> io::Result<Self> {
        let mut joints =
            bus.publisher_declared(JointState::TOPIC, &JointState::declaration(), SOURCE)?;
        joints.expires_after(Some(LASTS));
        let health =
            bus.publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), SOURCE)?;
        Ok(Self {
            joints,
            health,
            started: Instant::now(),
            before: None,
            since_report: None,
            resisted: Vec::new(),
            resisted_in_all: 0,
        })
    }

    fn at_us(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    fn step(&mut self, glimpse: &Glimpse, paused: bool) {
        let now = Instant::now();
        let state = joint_state(glimpse, self.before);
        self.before = Some((glimpse.step, in_order(glimpse).0));
        let _ = self.joints.publish_json(self.at_us(), &state);
        let in_all: u32 = glimpse.eased.iter().map(|n| u32::from(*n)).sum();
        for _ in self.resisted_in_all..in_all {
            self.resisted.push(now);
        }
        self.resisted_in_all = in_all;
        self.resisted
            .retain(|at| now.saturating_duration_since(*at) < RESISTANCE_SAID_FOR);
        let since = self.since_report.map_or(REPORT_EVERY, |steps| steps + 1);
        self.since_report = Some(since);
        if since >= REPORT_EVERY {
            self.since_report = Some(0);
            let mut report = health(
                glimpse,
                u32::try_from(self.resisted.len()).unwrap_or(u32::MAX),
            );
            if paused {
                said_paused(&mut report);
            }
            let _ = self.health.publish_json(self.at_us(), &report);
        }
    }
}

impl BusOut {
    /// The publisher, if the environment says `on` and the bus can be used.
    /// A bus that cannot is said once by `say` and the body goes on
    /// without.
    pub fn from_environment(say: impl FnOnce(&str)) -> Option<Self> {
        if std::env::var(ENVIRONMENT).ok().as_deref() != Some("on") {
            return None;
        }
        match Bus::from_environment().and_then(|bus| Self::on(&bus)) {
            Ok(out) => Some(out),
            Err(error) => {
                say(&format!("nothing is published on the bus: {error}"));
                None
            }
        }
    }

    pub fn on(bus: &Bus) -> io::Result<Self> {
        let mut publishing = Publishing::on(bus)?;
        // Room for a little over a second of offered steps.
        let (steps, offered) = mpsc::sync_channel::<Out>(16);
        let thread = thread::Builder::new().name("bus".into()).spawn(move || {
            for out in offered {
                match out {
                    Out::Step(glimpse, paused) => publishing.step(&glimpse, paused),
                    Out::Stopped(report) => {
                        let at_us = publishing.at_us();
                        let _ = publishing.health.publish_json(at_us, &report);
                    }
                }
            }
        })?;
        Ok(Self {
            steps,
            thread: Some(thread),
            let_go: 0,
            paused: false,
        })
    }

    /// A step of the body. One in [`EVERY_STEPS`] is passed on, and never
    /// waited for.
    pub fn offer(&mut self, glimpse: &Glimpse) {
        if !glimpse.step.is_multiple_of(EVERY_STEPS) {
            return;
        }
        if self
            .steps
            .try_send(Out::Step(*glimpse, self.paused))
            .is_err()
        {
            self.let_go += 1;
        }
    }

    /// The body has stopped: say why, and wait for that to be said.
    pub fn stopped(mut self, stop: &Stop) {
        let _ = self.steps.send(Out::Stopped(stopped(stop)));
        self.end();
    }

    fn end(&mut self) {
        // The thread ends when nothing more can be offered.
        let (closed, _) = mpsc::sync_channel(1);
        drop(std::mem::replace(&mut self.steps, closed));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for BusOut {
    fn drop(&mut self) {
        self.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_msgs::Level;

    fn glimpse(step: u64) -> Glimpse {
        Glimpse {
            step,
            mood: Mood::Listening,
            // Pitch, roll, yaw.
            goals: [1_968, 2_069, 2_015],
            positions: [1_950, 2_043, 2_015],
            loads: [120, 0, 40],
            temperatures: [38, 36, 41],
            eased: [0, 0, 0],
            kept_where_put: [0, 0, 0],
            stalls: [0; 5],
            stands: true,
            stand: 1.0,
            // Shoulder, elbow.
            arm_goals: [2_657, 2_602],
            arm_positions: [2_650, 2_602],
            arm_loads: [310, 250],
            arm_temperatures: [44, 43],
            look: [0.0, 0.0],
            face: None,
            gesture: None,
            released: 0,
            limp: false,
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_joints_are_named_in_servo_order_in_radians_from_the_middle_of_the_turn() {
        let state = joint_state(&glimpse(8), None);
        assert_eq!(state.check(), Ok(()));
        assert_eq!(
            state.name,
            ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"]
        );
        let count = TAU / 4096.0;
        let expected = [
            (2_015.0 - 2_048.0) * count,
            (2_650.0 - 2_048.0) * count,
            (2_602.0 - 2_048.0) * count,
            (2_043.0 - 2_048.0) * count,
            (1_950.0 - 2_048.0) * count,
        ];
        for (is, should) in state.position.iter().zip(expected) {
            assert!(near(*is, should), "{is} {should}");
        }
        // The middle of the turn is nothing, and 1,024 counts a quarter turn.
        assert!(near(radians(2_048), 0.0));
        assert!(near(radians(3_072), TAU / 4.0));
        assert!(near(radians(1_024), -TAU / 4.0));
        // Where each was asked to be, in the same order.
        assert!(near(state.commanded[0], (2_015.0 - 2_048.0) * count));
        assert!(near(state.commanded[1], (2_657.0 - 2_048.0) * count));
        assert!(near(state.commanded[3], (2_069.0 - 2_048.0) * count));
        assert!(near(state.commanded[4], (1_968.0 - 2_048.0) * count));
        // The load as a share of all the servo can exert.
        assert_eq!(state.effort, [0.04, 0.31, 0.25, 0.0, 0.12]);
        // No step before this one: no speed is made up.
        assert!(state.velocity.is_empty());
    }

    #[test]
    fn how_fast_each_joint_turns_is_worked_out_from_the_step_before() {
        let was = glimpse(8);
        let mut now = glimpse(12);
        // In four steps, 80 ms: the head lifted 10 counts and the base
        // turned back 4.
        now.positions = [1_960, 2_043, 2_011];
        let state = joint_state(&now, Some((was.step, in_order(&was).0)));
        assert_eq!(state.check(), Ok(()));
        let count = TAU / 4096.0;
        assert!(near(state.velocity[4], 10.0 * count / 0.08));
        assert!(near(state.velocity[0], -4.0 * count / 0.08));
        assert!(near(state.velocity[1], 0.0));
        // A step that is not later than the one before has no speed.
        assert!(
            joint_state(&was, Some((12, in_order(&now).0)))
                .velocity
                .is_empty()
        );
    }

    #[test]
    fn a_paused_body_says_paused_and_nothing_else_changes() {
        let well = health(&glimpse(50), 0);
        let mut paused = well.clone();
        said_paused(&mut paused);
        let state = |report: &Diagnostic| {
            report
                .values
                .iter()
                .find(|value| value.key == lamp_keeper::STATE)
                .map(|value| value.value.clone())
        };
        assert_eq!(state(&well).as_deref(), Some("ready"));
        assert_eq!(state(&paused).as_deref(), Some("paused"));
        assert_eq!((paused.level, &paused.message), (well.level, &well.message));
    }

    #[test]
    fn the_body_is_well_unless_a_joint_is_warm_or_one_lately_met_resistance() {
        let well = health(&glimpse(50), 0);
        assert_eq!((well.level, well.message.as_str()), (Level::Ok, ""));
        assert_eq!(well.name, "body");
        let facts: Vec<(&str, &str)> = well
            .values
            .iter()
            .map(|fact| (fact.key.as_str(), fact.value.as_str()))
            .collect();
        assert_eq!(
            facts,
            [
                ("mood", "listening"),
                ("standing", "1.00"),
                ("warmest_joint", "shoulder"),
                ("warmest_reading", "44"),
                ("resisted_last_minute", "0"),
                ("resisted_since_start", "0"),
                ("state", "ready"),
            ]
        );
        // One under warm is well, warm is not.
        let mut warm = glimpse(50);
        warm.arm_temperatures = [WARM - 1, 40];
        assert_eq!(health(&warm, 0).level, Level::Ok);
        warm.arm_temperatures = [WARM, 40];
        let report = health(&warm, 0);
        assert_eq!(report.level, Level::Warn);
        assert_eq!(
            report.message,
            "shoulder is warm: it reads 55, and the body stops at 62"
        );
        // A joint the body commands, too.
        let mut head = glimpse(50);
        head.temperatures = [58, 36, 41];
        assert!(health(&head, 0).message.starts_with("head_pitch is warm"));
        // Resistance, once and more than once. Warmth is said first.
        let mut pushed = glimpse(50);
        pushed.eased = [2, 0, 1];
        let report = health(&pushed, 1);
        assert_eq!(report.level, Level::Warn);
        assert_eq!(
            report.message,
            "a joint met resistance and stopped pushing, 1 time in the last minute"
        );
        assert!(
            health(&pushed, 3)
                .message
                .ends_with("3 times in the last minute")
        );
        assert_eq!(report.values[5].value, "3");
        assert!(health(&head, 3).message.starts_with("head_pitch is warm"));
    }

    #[test]
    fn why_the_body_stopped_is_said_in_words_and_only_being_asked_is_not_a_fault() {
        let asked = stopped(&Stop::Cancelled);
        assert_eq!(
            (asked.level, asked.message.as_str()),
            (Level::Warn, "stopped, as it was asked to")
        );
        for (stop, says) in [
            (
                Stop::Load {
                    joint: 5,
                    load: 260,
                },
                "stopped: head_pitch was pushed against (load 260)",
            ),
            (
                Stop::Tracking {
                    joint: 1,
                    goal: 2_050,
                    position: 2_015,
                },
                "stopped: base_yaw did not follow (asked 2050, at 2015)",
            ),
            (
                Stop::Status {
                    joint: 3,
                    found: 0,
                    now: 0x20,
                },
                "stopped: the servo of elbow reports a fault (status 0x00 became 0x20)",
            ),
            (
                Stop::Drift {
                    joint: 2,
                    found: 2_048,
                    now: 2_060,
                },
                "stopped: shoulder was moved by something else (from 2048 to 2060)",
            ),
            (
                Stop::Temperature {
                    joint: 4,
                    temperature: 63,
                },
                "stopped: head_roll is too warm (it reads 63)",
            ),
            (
                Stop::Bus {
                    detail: "41 bytes of a late reply".into(),
                },
                "stopped: the servos did not answer as expected",
            ),
            (
                Stop::Refused {
                    why: "head pitch is at 2095".into(),
                },
                "would not start: head pitch is at 2095",
            ),
            (
                Stop::Load { joint: 9, load: 1 },
                "stopped: a joint was pushed against (load 1)",
            ),
        ] {
            let report = stopped(&stop);
            assert_eq!(report.level, Level::Error, "{stop:?}");
            assert_eq!(report.message, says);
            assert_eq!(report.name, "body");
        }
    }

    #[test]
    fn steps_offered_reach_the_bus_and_the_last_word_is_why_it_stopped() {
        use lamp_msgs::JointState;
        let root = std::path::PathBuf::from(format!("/tmp/lb-bus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut joints = bus
            .subscribe_typed(JointState::TOPIC, "test", JointState::TYPE)
            .unwrap();
        let mut reports = bus
            .subscribe_typed(Diagnostic::TOPIC, "test", Diagnostic::TYPE)
            .unwrap();
        let mut out = BusOut::on(&bus).unwrap();
        // Twelve steps: three of them are passed on.
        for step in 0..12 {
            let mut seen = glimpse(step);
            seen.positions[0] = 1_950 + step as u16;
            out.offer(&seen);
        }
        let wait = Duration::from_secs(5);
        let states: Vec<JointState> = (0..3)
            .map(|_| {
                joints
                    .recv(wait)
                    .unwrap()
                    .expect("a state")
                    .parse()
                    .unwrap()
            })
            .collect();
        assert!(states[0].velocity.is_empty());
        assert_eq!(states[1].velocity.len(), 5);
        let count = TAU / 4096.0;
        assert!(near(states[2].position[4], (1_958.0 - 2_048.0) * count));
        assert!(near(states[2].velocity[4], 4.0 * count / 0.08));
        // One report of how it is, with the first step.
        let report: Diagnostic = reports
            .recv(wait)
            .unwrap()
            .expect("a report")
            .parse()
            .unwrap();
        assert_eq!((report.level, report.name.as_str()), (Level::Ok, "body"));
        assert_eq!(
            (
                report.values[6].key.as_str(),
                report.values[6].value.as_str()
            ),
            ("state", "ready")
        );
        // What is kept for whoever comes later is the newest. A message
        // reaches its subscribers a moment before it is the kept one.
        let since = Instant::now();
        loop {
            let kept: JointState = bus
                .latest(JointState::TOPIC)
                .unwrap()
                .expect("kept")
                .parse()
                .unwrap();
            if kept == states[2] {
                break;
            }
            assert!(since.elapsed() < wait, "{kept:?}");
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(out.let_go, 0);
        out.stopped(&Stop::Temperature {
            joint: 2,
            temperature: 62,
        });
        let last: Diagnostic = reports
            .recv(wait)
            .unwrap()
            .expect("the last")
            .parse()
            .unwrap();
        assert_eq!(last.level, Level::Error);
        assert_eq!(last.message, "stopped: shoulder is too warm (it reads 62)");
        assert_eq!(last.values[0].value, "stopping");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_thread_with_no_room_costs_the_loop_a_step_and_never_a_wait() {
        let root = std::path::PathBuf::from(format!("/tmp/lb-full-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut out = BusOut::on(&bus).unwrap();
        // Far more than the thread can hold, as fast as they can be made.
        let started = Instant::now();
        for step in 0..40_000u64 {
            out.offer(&glimpse(step * EVERY_STEPS));
        }
        let each = started.elapsed() / 40_000;
        // A step is 20 ms; offering one must be nothing beside that.
        assert!(each < Duration::from_millis(1), "{each:?}");
        drop(out);
        std::fs::remove_dir_all(root).unwrap();
    }
}
