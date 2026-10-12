//! The body against five simulated servos: what it writes, where it stops.
use super::*;
use std::{cell::Cell, collections::VecDeque, rc::Rc};

#[derive(Clone, Copy)]
struct Servo {
    position: f32,
    goal: u16,
    torque: u8,
    acceleration: u8,
    speed: u16,
    status: u8,
    load: u16,
    temperature: u8,
    answers: bool,
}

/// Something that happens to one servo at a given time.
#[derive(Clone, Copy)]
enum Event {
    Flag(u8),
    Push(f32),
    Heat(u8),
    /// One reading of this temperature, then back to what it was.
    HeatOnce(u8),
}

/// Five servos on one wire, moved by simulated time.
struct Lamp {
    time: Rc<Cell<Duration>>,
    servos: [Servo; 5],
    incoming: VecDeque<u8>,
    accepted: Vec<u8>,
    /// Every sync write of goals: (servo id, goal) as sent.
    goals: Vec<Vec<(u8, u16)>>,
    /// Every single-servo register write: (servo id, address, bytes).
    registers: Vec<(u8, u8, Vec<u8>)>,
    /// One servo cannot pass this position and presses on it instead.
    wall: Option<(usize, f32)>,
    silent_from: Option<Duration>,
    event: Option<(Duration, usize, Event)>,
    /// A temperature to put back once it has been read once.
    blip: Option<(usize, u8)>,
    simulated_to: Duration,
}

struct Clock {
    time: Rc<Cell<Duration>>,
    cancel_at: Duration,
}
impl Runtime for Clock {
    fn now(&self) -> Duration {
        self.time.get()
    }
    fn cancelled(&mut self) -> io::Result<bool> {
        Ok(self.time.get() >= self.cancel_at)
    }
}

/// The lamp as it should be after its head is backed off the base: folded,
/// torque on, the head on the slow profile, the elbow's old flag latched.
fn folded(cancel_at: Duration) -> (Lamp, Clock) {
    let servo = |position: f32, acceleration, speed, status, load| Servo {
        position,
        goal: position as u16,
        torque: 1,
        acceleration,
        speed,
        status,
        load,
        temperature: 45,
        answers: true,
    };
    let time = Rc::new(Cell::new(Duration::ZERO));
    (
        Lamp {
            time: time.clone(),
            servos: [
                servo(2_015.0, 0, 0, 0, 0),
                servo(2_050.0, 0, 0, 0, 20),
                servo(2_047.0, 0, 0, 0x20, 30),
                servo(2_044.0, 0, 0, 0, 32),
                servo(2_000.0, 50, 150, 0, 25),
            ],
            incoming: VecDeque::new(),
            accepted: Vec::new(),
            goals: Vec::new(),
            registers: Vec::new(),
            wall: None,
            silent_from: None,
            event: None,
            blip: None,
            simulated_to: Duration::ZERO,
        },
        Clock { time, cancel_at },
    )
}

const PITCH: usize = 4;
const ROLL: usize = 3;
const YAW: usize = 0;
const HOME: [u16; 3] = [2_000, 2_044, 2_015];

impl Lamp {
    fn simulate(&mut self) {
        let now = self.time.get();
        let step = (now - self.simulated_to).as_secs_f32();
        self.simulated_to = now;
        if let Some((at, index, event)) = self.event
            && now >= at
        {
            self.event = None;
            match event {
                Event::Flag(status) => self.servos[index].status = status,
                Event::Push(by) => self.servos[index].position += by,
                Event::Heat(to) => self.servos[index].temperature = to,
                Event::HeatOnce(to) => {
                    self.blip = Some((index, self.servos[index].temperature));
                    self.servos[index].temperature = to;
                }
            }
        }
        for (index, servo) in self.servos.iter_mut().enumerate() {
            if servo.torque != 1 || index == 1 || index == 2 {
                continue;
            }
            // The register's unit is not known; here it is counts a second.
            let fastest = if servo.speed == 0 {
                3_000.0
            } else {
                f32::from(servo.speed)
            };
            let wanted = f32::from(servo.goal);
            let moved = (wanted - servo.position).clamp(-fastest * step, fastest * step);
            servo.position += moved;
            if let Some((walled, at)) = self.wall
                && walled == index
                && servo.position > at
            {
                servo.position = at;
                servo.load = 300;
            }
        }
    }

    fn registers_of(&self, servo: &Servo, address: u8, count: u8) -> Vec<u8> {
        let position = servo.position.round() as u16;
        (address..address + count)
            .map(|register| match register {
                40 => servo.torque,
                41 => servo.acceleration,
                42 => servo.goal.to_le_bytes()[0],
                43 => servo.goal.to_le_bytes()[1],
                46 => servo.speed.to_le_bytes()[0],
                47 => servo.speed.to_le_bytes()[1],
                56 => position.to_le_bytes()[0],
                57 => position.to_le_bytes()[1],
                60 => servo.load.to_le_bytes()[0],
                61 => servo.load.to_le_bytes()[1],
                62 => 122,
                63 => servo.temperature,
                65 => servo.status,
                _ => 0,
            })
            .collect()
    }

    fn answer(&mut self, id: u8, data: &[u8]) {
        let servo = self.servos[usize::from(id) - 1];
        let silent = self.silent_from.is_some_and(|from| self.time.get() >= from);
        if !servo.answers || silent {
            return;
        }
        let mut body = vec![id, data.len() as u8 + 2, servo.status];
        body.extend(data);
        let sum = checksum(&body);
        self.incoming.extend([0xff, 0xff]);
        self.incoming.extend(body);
        self.incoming.push_back(sum);
    }

    fn receive(&mut self) {
        while self.accepted.len() >= 6 {
            let total = usize::from(self.accepted[3]) + 4;
            if self.accepted.len() < total {
                return;
            }
            let packet: Vec<u8> = self.accepted.drain(..total).collect();
            assert_eq!(&packet[..2], &[0xff, 0xff]);
            assert_eq!(checksum(&packet[2..total - 1]), packet[total - 1]);
            let (id, instruction, parameters) = (packet[2], packet[4], &packet[5..total - 1]);
            match instruction {
                0x82 => {
                    let (address, count) = (parameters[0], parameters[1]);
                    for asked in parameters[2..].iter().copied() {
                        let data =
                            self.registers_of(&self.servos[usize::from(asked) - 1], address, count);
                        self.answer(asked, &data);
                    }
                    if address == 56
                        && let Some((index, before)) = self.blip.take()
                    {
                        self.servos[index].temperature = before;
                    }
                }
                0x83 => {
                    assert_eq!(id, 0xfe);
                    assert_eq!((parameters[0], parameters[1]), (42, 2));
                    let mut written = Vec::new();
                    for entry in parameters[2..].chunks(3) {
                        let goal = u16::from_le_bytes([entry[1], entry[2]]);
                        self.servos[usize::from(entry[0]) - 1].goal = goal;
                        written.push((entry[0], goal));
                    }
                    self.goals.push(written);
                }
                0x03 => {
                    let (address, data) = (parameters[0], parameters[1..].to_vec());
                    let servo = &mut self.servos[usize::from(id) - 1];
                    match (address, data.as_slice()) {
                        (41, [value]) => servo.acceleration = *value,
                        (46, [low, high]) => servo.speed = u16::from_le_bytes([*low, *high]),
                        other => panic!("a write this body must never send: {other:?}"),
                    }
                    self.registers.push((id, address, data));
                    self.answer(id, &[]);
                }
                other => panic!("an instruction this body must never send: {other:#04x}"),
            }
        }
    }

    fn pass(&mut self, by: Duration) {
        self.time.set(self.time.get() + by);
        self.simulate();
    }

    fn nothing_was_written(&self) -> bool {
        self.goals.is_empty() && self.registers.is_empty()
    }
}

impl PortIo for Lamp {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.pass(Duration::from_micros(40));
        if self.incoming.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(self.incoming.len());
        for slot in &mut bytes[..count] {
            *slot = self.incoming.pop_front().unwrap();
        }
        Ok(count)
    }
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.pass(Duration::from_micros(40));
        self.accepted.extend(bytes);
        self.receive();
        Ok(bytes.len())
    }
    fn wait(&mut self, interest: Interest, maximum: Duration) -> io::Result<()> {
        if matches!(interest, Interest::Read) && !self.incoming.is_empty() {
            self.pass(Duration::from_micros(40));
        } else {
            self.pass(maximum);
        }
        Ok(())
    }
}

/// A mood for each moment, as the voice service would send it.
fn script(time: &Rc<Cell<Duration>>) -> impl FnMut() -> Option<(Mood, u8)> + use<> {
    let time = time.clone();
    move || {
        let at = time.get().as_millis() as u64;
        Some(match at {
            0..1_000 => (Mood::Resting, 0),
            1_000..3_000 => (Mood::Listening, 120),
            3_000..4_500 => (Mood::Thinking, 0),
            // Syllables: loud for 120 ms of every 300.
            4_500..7_000 => (Mood::Speaking, if at % 300 < 120 { 230 } else { 30 }),
            _ => (Mood::Resting, 0),
        })
    }
}

fn goal_of(write: &[(u8, u16)], joint: Joint) -> u16 {
    write.iter().find(|(id, _)| *id == joint.id()).unwrap().1
}

#[test]
fn the_three_writes_are_exactly_these_bytes() {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(&goals_write(HOME)),
        "fffffe0d832a0205d00704fc0701df077b"
    );
    assert_eq!(
        hex(&speed_write(Joint::WristRoll, 150)),
        "ffff0405032e96002f"
    );
    assert_eq!(
        hex(&acceleration_write(Joint::BaseYaw, 50)),
        "ffff01040329329c"
    );
}

#[test]
fn a_folded_lamp_listens_thinks_speaks_and_comes_home() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.moved);
    assert_eq!(report.home, Some(HOME));
    assert_eq!(report.missed_reads, 0);

    // The servo's own speed bound: written to the two joints that lacked
    // it, not to the head, which had it, and to nothing else.
    assert_eq!(report.profile_writes, 4);
    let mut registers = lamp.registers.clone();
    registers.sort();
    assert_eq!(
        registers,
        vec![
            (1, 41, vec![50]),
            (1, 46, vec![150, 0]),
            (4, 41, vec![50]),
            (4, 46, vec![150, 0])
        ]
    );

    // Every goal write names the three commanded joints and no other.
    assert!(lamp.goals.len() > 100);
    for write in &lamp.goals {
        let ids: Vec<u8> = write.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![5, 4, 1]);
    }
    // Within reach of where it was found, and the head short of the base.
    let limits = Limits::FOLDED;
    for write in &lamp.goals {
        let pitch = f32::from(goal_of(write, Joint::WristPitch));
        let roll = f32::from(goal_of(write, Joint::WristRoll));
        let yaw = f32::from(goal_of(write, Joint::BaseYaw));
        assert!(pitch >= 2_000.0 - limits.pitch_up && pitch <= 2_000.0 + limits.pitch_down);
        assert!(pitch <= f32::from(limits.pitch_window.1));
        assert!((roll - 2_044.0).abs() <= limits.roll);
        assert!((yaw - 2_015.0).abs() <= limits.yaw);
    }
    // Slowly: no joint's goal moves more than two counts in one write.
    let mut previous = HOME.map(i32::from);
    for write in &lamp.goals {
        for (index, joint) in COMMANDED.iter().enumerate() {
            let goal = i32::from(goal_of(write, *joint));
            assert!(
                (goal - previous[index]).abs() <= 2,
                "{joint} {previous:?} {write:?}"
            );
            previous[index] = goal;
        }
    }
    // It did tilt, look away and nod.
    let reach = |joint: Joint, home: u16| {
        lamp.goals
            .iter()
            .map(|write| goal_of(write, joint).abs_diff(home))
            .max()
            .unwrap()
    };
    assert!(reach(Joint::WristRoll, 2_044) >= 20);
    assert!(reach(Joint::BaseYaw, 2_015) >= 25);
    assert!(reach(Joint::WristPitch, 2_000) >= 20);
    // And it ends where it was found, with the shoulder and elbow untouched.
    assert_eq!(lamp.servos[PITCH].goal, 2_000);
    assert_eq!(lamp.servos[ROLL].goal, 2_044);
    assert_eq!(lamp.servos[YAW].goal, 2_015);
    assert!((lamp.servos[PITCH].position - 2_000.0).abs() <= 2.0);
    assert_eq!((lamp.servos[1].goal, lamp.servos[2].goal), (2_050, 2_047));
    assert_eq!(report.held_at, None);
    serde_json::to_string(&report).unwrap();
}

#[test]
fn it_does_not_start_on_a_lamp_that_is_not_as_expected() {
    let refused = |change: &dyn Fn(&mut Lamp)| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(2));
        change(&mut lamp);
        let news = script(&clock.time);
        let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
        assert!(
            matches!(report.stop, Stop::Refused { .. }),
            "{:?}",
            report.stop
        );
        assert!(!report.moved);
        assert!(lamp.nothing_was_written());
        match report.stop {
            Stop::Refused { why } => why,
            _ => unreachable!(),
        }
    };
    // The head pressed on the base, as it was found on 2026-10-12.
    assert!(refused(&|lamp| lamp.servos[PITCH].position = 2_095.0).contains("2095"));
    assert!(refused(&|lamp| lamp.servos[ROLL].torque = 0).contains("torque"));
    assert!(refused(&|lamp| lamp.servos[PITCH].status = 0x20).contains("0x20"));
    assert!(refused(&|lamp| lamp.servos[YAW].temperature = 70).contains("70"));
    assert!(refused(&|lamp| lamp.servos[PITCH].load = 200).contains("200"));
    // A joint that does not answer, commanded or not.
    assert!(refused(&|lamp| lamp.servos[1].answers = false).contains("first read"));
}

#[test]
fn a_load_that_rises_stops_it_where_it_is_and_nothing_is_written_after() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    // Something in the way of the head's tilt, ten counts from home.
    lamp.wall = Some((ROLL, 2_054.0));
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(
        report.stop,
        Stop::Load {
            joint: 4,
            load: 300
        },
        "{report:?}"
    );
    // It stopped within a few steps of meeting it, not at the end of the tilt.
    assert!(
        clock.time.get() < Duration::from_millis(1_700),
        "{:?}",
        clock.time.get()
    );
    let held = report.held_at.unwrap();
    assert_eq!(held[1], 2_054);
    // The last thing written is the hold: every goal is where its joint is.
    assert_eq!(
        lamp.goals.last().unwrap(),
        &vec![(5, held[0]), (4, held[1]), (1, held[2])]
    );
    assert_eq!(lamp.servos[ROLL].goal, 2_054);
    assert!(
        lamp.servos[PITCH]
            .goal
            .abs_diff(lamp.servos[PITCH].position.round() as u16)
            <= 1
    );
}

#[test]
fn a_new_flag_a_moved_elbow_or_a_hot_joint_stops_it() {
    let ended = |index: usize, event: Event| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(8));
        lamp.event = Some((Duration::from_millis(1_500), index, event));
        let news = script(&clock.time);
        let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
        // At once, except for heat, which has to last a second to count.
        let by = if matches!(event, Event::Heat(_)) {
            2_600
        } else {
            1_600
        };
        assert!(clock.time.get() < Duration::from_millis(by));
        assert!(report.held_at.is_some());
        report.stop
    };
    // The shoulder trips its overload flag, though nothing commands it.
    assert_eq!(
        ended(1, Event::Flag(0x20)),
        Stop::Status {
            joint: 2,
            found: 0,
            now: 0x20
        }
    );
    // The elbow's old flag was there at start; a different one is news.
    assert_eq!(
        ended(2, Event::Flag(0x24)),
        Stop::Status {
            joint: 3,
            found: 0x20,
            now: 0x24
        }
    );
    assert_eq!(
        ended(2, Event::Push(9.0)),
        Stop::Drift {
            joint: 3,
            found: 2_047,
            now: 2_056
        }
    );
    assert_eq!(
        ended(PITCH, Event::Heat(63)),
        Stop::Temperature {
            joint: 5,
            temperature: 63
        }
    );
}

#[test]
fn one_hot_reading_while_it_moves_is_not_heat() {
    // As on lamp-4ace: the yaw servo read 72 on one step of its first move.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.event = Some((Duration::from_millis(3_200), YAW, Event::HeatOnce(72)));
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.peak_temperature[2], 72);
}

#[test]
fn a_home_it_is_given_is_reached_slowly_from_where_it_is_found() {
    // Left 43 counts above its home by an earlier stop, and turned a little.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.servos[PITCH].position = 1_973.0;
    lamp.servos[PITCH].goal = 1_973;
    lamp.servos[YAW].position = 2_033.0;
    lamp.servos[YAW].goal = 2_033;
    let home = [1_930, 2_044, 2_015];
    let news = script(&clock.time);
    let report = run(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(home),
        news,
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.home, Some(home));
    // Every write is a small step from the one before, starting where it was.
    let mut previous = [1_973, 2_044, 2_033];
    for write in &lamp.goals {
        for (index, joint) in COMMANDED.iter().enumerate() {
            let goal = i32::from(goal_of(write, *joint));
            assert!(
                (goal - previous[index]).abs() <= 2,
                "{joint} {previous:?} {write:?}"
            );
            previous[index] = goal;
        }
    }
    // The head never moved away from its home on the way in.
    assert!(
        lamp.goals
            .iter()
            .all(|write| goal_of(write, Joint::WristPitch) <= 1_973)
    );
    assert_eq!(
        [
            lamp.servos[PITCH].goal,
            lamp.servos[ROLL].goal,
            lamp.servos[YAW].goal
        ],
        home
    );

    // Too far from the home it is given: nothing is written.
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    let news = script(&clock.time);
    let far = Some([1_900, 2_044, 2_015]);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, far, news, |_| {});
    assert!(
        matches!(&report.stop, Stop::Refused { why } if why.contains("100 counts")),
        "{:?}",
        report.stop
    );
    assert!(lamp.nothing_was_written());
}

#[test]
fn a_wider_reach_and_speed_can_be_chosen_only_inside_their_caps() {
    let tuning = Tuning {
        reach: [150, 15, 80, 200],
        speed: 200,
        servo_speed: 600,
        servo_acceleration: 100,
    };
    let limits = Limits::folded_with(tuning).unwrap();
    assert_eq!(
        (limits.pitch_up, limits.yaw, limits.speed),
        (150.0, 200.0, 200.0)
    );
    // What is not a reach or a speed is exactly as before.
    assert_eq!(limits.pitch_window, Limits::FOLDED.pitch_window);
    assert_eq!(limits.load, Limits::FOLDED.load);
    assert_eq!(limits.bystander_drift, Limits::FOLDED.bystander_drift);
    for refused in [
        Tuning {
            reach: [181, 15, 80, 200],
            ..tuning
        },
        Tuning {
            reach: [150, 26, 80, 200],
            ..tuning
        },
        Tuning {
            reach: [150, 15, 101, 200],
            ..tuning
        },
        Tuning {
            reach: [150, 15, 80, 251],
            ..tuning
        },
        Tuning {
            reach: [0, 15, 80, 200],
            ..tuning
        },
        Tuning {
            speed: 301,
            ..tuning
        },
        Tuning {
            servo_speed: 1_001,
            ..tuning
        },
        Tuning {
            servo_acceleration: 151,
            ..tuning
        },
    ] {
        assert!(Limits::folded_with(refused).is_err(), "{refused:?}");
    }

    // With it the head turns much farther, still in small steps and still
    // never past the pitch window.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.servos[PITCH].position = 1_950.0;
    lamp.servos[PITCH].goal = 1_950;
    for servo in [PITCH, ROLL, YAW] {
        lamp.servos[servo].speed = 600;
        lamp.servos[servo].acceleration = 100;
    }
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, limits, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let farthest = |joint: Joint, home: u16| {
        lamp.goals
            .iter()
            .map(|write| goal_of(write, joint).abs_diff(home))
            .max()
            .unwrap()
    };
    assert!(farthest(Joint::BaseYaw, 2_015) >= 150 && farthest(Joint::BaseYaw, 2_015) <= 200);
    assert!(farthest(Joint::WristPitch, 1_950) >= 90);
    let mut previous = [1_950, 2_044, 2_015];
    for write in &lamp.goals {
        for (index, joint) in COMMANDED.iter().enumerate() {
            let goal = i32::from(goal_of(write, *joint));
            assert!(
                (goal - previous[index]).abs() <= 5,
                "{joint} {previous:?} {write:?}"
            );
            previous[index] = goal;
        }
        assert!(goal_of(write, Joint::WristPitch) <= 2_030);
    }
}

#[test]
fn every_step_can_be_watched_with_the_mood_it_acted_on() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    let news = script(&clock.time);
    let mut seen = Vec::new();
    let report = run(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        None,
        news,
        |glimpse| seen.push(*glimpse),
    );
    assert_eq!(report.stop, Stop::Cancelled);
    assert_eq!(seen.len() as u64, report.steps);
    let moods: Vec<Mood> = seen.iter().map(|glimpse| glimpse.mood).collect();
    for mood in [
        Mood::Resting,
        Mood::Listening,
        Mood::Thinking,
        Mood::Speaking,
    ] {
        assert!(moods.contains(&mood), "{mood:?}");
    }
    // While it looked away its goal and its position were both off home.
    let away = seen
        .iter()
        .find(|glimpse| glimpse.mood == Mood::Thinking && glimpse.goals[2] != 2_015);
    assert!(away.is_some());
    assert!(
        seen.iter()
            .any(|glimpse| glimpse.positions[2].abs_diff(2_015) >= 20)
    );
}

#[test]
fn a_wire_that_goes_silent_stops_it_after_three_steps() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.silent_from = Some(Duration::from_millis(1_500));
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert!(matches!(report.stop, Stop::Bus { .. }), "{:?}", report.stop);
    assert_eq!(report.missed_reads, 3);
    assert!(clock.time.get() < Duration::from_millis(1_600));
}

#[test]
fn a_mood_that_stops_arriving_is_rest() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(6));
    let time = clock.time.clone();
    // Listening for a second and a half, then the sender is gone.
    let news = move || (time.get() < Duration::from_millis(1_500)).then_some((Mood::Listening, 90));
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled);
    let tilted = lamp
        .goals
        .iter()
        .any(|write| goal_of(write, Joint::WristRoll).abs_diff(2_044) >= 20);
    assert!(tilted);
    // Long before it was asked to stop, the goals were back at home.
    assert_eq!(lamp.servos[ROLL].goal, 2_044);
    assert!((lamp.servos[ROLL].position - 2_044.0).abs() <= 1.0);
    let last_change = lamp.goals.len();
    assert!(last_change < 260, "{last_change}");
}

#[test]
fn the_heads_pitch_never_passes_its_window_however_it_is_found() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    // Found near the top of where it may be found: little room to dip.
    lamp.servos[PITCH].position = 2_018.0;
    lamp.servos[PITCH].goal = 2_018;
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let highest = lamp
        .goals
        .iter()
        .map(|write| goal_of(write, Joint::WristPitch))
        .max()
        .unwrap();
    assert!(highest <= 2_030, "{highest}");
}
