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
    /// Its force limit (register 48), and whether a write of it is kept.
    force: u16,
    keeps_force: bool,
    /// Its own lower and upper position limits (registers 9 and 11).
    range: (u16, u16),
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
    /// When each of `goals` and of `registers` was written.
    goal_times: Vec<Duration>,
    register_times: Vec<Duration>,
    /// One servo cannot pass this position and presses on it instead.
    wall: Option<(usize, f32)>,
    /// One servo stops this many counts short of any goal, as a real one
    /// does against its own friction.
    short: Option<(usize, f32)>,
    /// One servo lifting a drooped head: while above this position and
    /// asked to go lower it stops this many counts short of its goal, and
    /// reads this load while it does, as lamp-52e6's head pitch did after a
    /// power cut. Servo, above, counts, load.
    heavy: Option<(usize, f32, f32, u16)>,
    silent_from: Option<Duration>,
    silent_until: Option<Duration>,
    event: Option<(Duration, usize, Event)>,
    /// Loads one servo reads for a while whatever it is doing: from, until,
    /// servo, load. Afterwards it reads what it read before.
    strains: Vec<(Duration, Duration, usize, u16)>,
    unstrained: Option<[u16; 5]>,
    /// A temperature to put back once it has been read once.
    blip: Option<(usize, u8)>,
    simulated_to: Duration,
    /// The shoulder and the elbow follow their goals like the other three.
    /// Off, they stay where they are whatever is written to them.
    arm_moves: bool,
    /// What takes a latched overload off a servo: 0 nothing, 1 a write of
    /// torque on, 2 only torque off and then on, 3 being told to stop
    /// pushing (a goal that is where it is), a few steps later.
    overload_clears: u8,
    /// Torque and a goal by itself may be written to any joint, as a body
    /// told to take hold does. Off, only to the shoulder or the elbow.
    may_take_hold: bool,
    /// One servo acknowledges the next goal written to it by itself and
    /// keeps the goal it had.
    drops_goal: Option<usize>,
    /// A hand moves one servo toward a position at 100 counts a second,
    /// whatever its goal: from, until, servo, position. Its load is a
    /// strain of the same time.
    hand: Option<(Duration, Duration, usize, f32)>,
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
        force: 1_000,
        keeps_force: true,
        range: (0, 4_095),
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
            goal_times: Vec::new(),
            register_times: Vec::new(),
            wall: None,
            short: None,
            heavy: None,
            silent_from: None,
            silent_until: None,
            event: None,
            strains: Vec::new(),
            unstrained: None,
            blip: None,
            simulated_to: Duration::ZERO,
            arm_moves: false,
            overload_clears: 0,
            may_take_hold: false,
            drops_goal: None,
            hand: None,
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
        if !self.strains.is_empty() {
            let before = *self
                .unstrained
                .get_or_insert_with(|| self.servos.map(|servo| servo.load));
            for (index, servo) in self.servos.iter_mut().enumerate() {
                servo.load = self
                    .strains
                    .iter()
                    .find(|(from, until, strained, _)| {
                        *strained == index && (*from..*until).contains(&now)
                    })
                    .map_or(before[index], |strain| strain.3);
            }
        }
        for (index, servo) in self.servos.iter_mut().enumerate() {
            // An overload that goes once the servo is no longer pushing.
            if self.overload_clears == 3
                && servo.torque == 1
                && (f32::from(servo.goal) - servo.position).abs() < 1.0
            {
                servo.status &= !0x20;
            }
            if let Some((from, until, handled, to)) = self.hand
                && handled == index
                && (from..until).contains(&now)
            {
                servo.position += (to - servo.position).clamp(-100.0 * step, 100.0 * step);
                continue;
            }
            if servo.torque != 1 || ((index == 1 || index == 2) && !self.arm_moves) {
                continue;
            }
            // The register's unit is not known; here it is counts a second.
            let fastest = if servo.speed == 0 {
                3_000.0
            } else {
                f32::from(servo.speed)
            };
            let mut wanted = f32::from(servo.goal);
            if let Some((sticky, by)) = self.short
                && sticky == index
            {
                let gap = wanted - servo.position;
                wanted = if gap.abs() <= by {
                    servo.position
                } else {
                    wanted - by * gap.signum()
                };
            }
            if let Some((lifted, above, by, load)) = self.heavy
                && lifted == index
            {
                let lifting = wanted < servo.position && servo.position > above;
                // At rest it reads what the folded head reads here.
                servo.load = if lifting { load } else { 25 };
                if lifting {
                    wanted = (wanted + by).min(servo.position);
                }
            }
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
                9 => servo.range.0.to_le_bytes()[0],
                10 => servo.range.0.to_le_bytes()[1],
                11 => servo.range.1.to_le_bytes()[0],
                12 => servo.range.1.to_le_bytes()[1],
                40 => servo.torque,
                41 => servo.acceleration,
                42 => servo.goal.to_le_bytes()[0],
                43 => servo.goal.to_le_bytes()[1],
                46 => servo.speed.to_le_bytes()[0],
                47 => servo.speed.to_le_bytes()[1],
                48 => servo.force.to_le_bytes()[0],
                49 => servo.force.to_le_bytes()[1],
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
        let silent = self.silent_from.is_some_and(|from| self.time.get() >= from)
            && self
                .silent_until
                .is_none_or(|until| self.time.get() < until);
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
                    self.goal_times.push(self.time.get());
                }
                0x03 => {
                    let (address, data) = (parameters[0], parameters[1..].to_vec());
                    let clears = self.overload_clears;
                    let arm_or_held = id == 2 || id == 3 || self.may_take_hold;
                    let dropped = address == 42 && self.drops_goal == Some(usize::from(id) - 1);
                    if dropped {
                        self.drops_goal = None;
                    }
                    let servo = &mut self.servos[usize::from(id) - 1];
                    match (address, data.as_slice()) {
                        (41, [value]) => servo.acceleration = *value,
                        (46, [low, high]) => servo.speed = u16::from_le_bytes([*low, *high]),
                        // A goal by itself: only ever to the shoulder or the
                        // elbow, or to a joint the body takes hold of.
                        (42, [low, high]) if arm_or_held => {
                            if !dropped {
                                servo.goal = u16::from_le_bytes([*low, *high]);
                            }
                        }
                        // Torque: the same.
                        (40, [value]) if arm_or_held => {
                            let was = servo.torque;
                            servo.torque = *value;
                            if *value == 1 && (clears == 1 || (clears == 2 && was == 0)) {
                                servo.status &= !0x20;
                            }
                        }
                        // The force limit: only ever to the three joints
                        // that carry the lamp.
                        (48, [low, high]) if (1..=3).contains(&id) => {
                            if servo.keeps_force {
                                servo.force = u16::from_le_bytes([*low, *high]);
                            }
                        }
                        other => panic!("a write this body must never send: {other:?}"),
                    }
                    self.registers.push((id, address, data));
                    self.register_times.push(self.time.get());
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
fn a_joint_that_meets_resistance_stops_pushing_for_two_seconds_and_then_goes_on() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    // A fifth of a second into its tilt, a load like the one the head read on
    // lamp-4ace: 217 for a moment and nothing in its way.
    let ms = Duration::from_millis;
    lamp.strains = vec![(ms(1_200), ms(1_400), ROLL, 217)];
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
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.eased, report.peak_load[1]), ([0, 1, 0], 217));
    // It gave way on the sixth step over the bound, where it was. A step is
    // watched before it is judged, so the next one is the first to show it.
    let strained = seen
        .iter()
        .position(|glimpse| glimpse.loads[1] == 217)
        .unwrap();
    let gave = seen
        .iter()
        .position(|glimpse| glimpse.eased[1] == 1)
        .unwrap();
    assert_eq!(gave, strained + 6);
    let held = seen[gave].goals[1];
    assert!(
        held.abs_diff(seen[gave - 1].positions[1]) <= 1,
        "{:?}",
        seen[gave - 1]
    );
    assert_ne!(held, HOME[1], "it had begun to tilt");
    // For two seconds its goal does not change, while the other joints go
    // on with what the lamp is doing.
    let waiting = &seen[gave..gave + usize::from(EASE_STEPS) - 1];
    assert!(waiting.iter().all(|glimpse| glimpse.goals[1] == held));
    assert!(waiting.iter().any(|glimpse| glimpse.goals[2] != HOME[2]));
    // Then it moves again, from where it stopped.
    let after = &seen[gave + usize::from(EASE_STEPS)..];
    assert!(
        after
            .iter()
            .any(|glimpse| glimpse.goals[1].abs_diff(held) >= 10)
    );
    assert!(
        after
            .windows(2)
            .all(|pair| pair[1].goals[1].abs_diff(pair[0].goals[1]) <= 2),
        "it jumped"
    );
}

#[test]
fn resistance_for_the_fourth_time_in_a_minute_ends_the_run() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(40));
    let ms = Duration::from_millis;
    lamp.strains = [1_200, 6_000, 12_000, 18_000]
        .map(|at| (ms(at), ms(at + 200), YAW, 260))
        .to_vec();
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(
        report.stop,
        Stop::Load {
            joint: 1,
            load: 260
        },
        "{report:?}"
    );
    assert_eq!(report.eased, [0, 0, 3]);
    assert!(clock.time.get() < ms(18_200), "{:?}", clock.time.get());
    // Three in a minute and a fourth later are four separate moments.
    let (mut lamp, mut clock) = folded(Duration::from_secs(90));
    lamp.strains = [1_200, 6_000, 12_000, 75_000]
        .map(|at| (ms(at), ms(at + 200), YAW, 260))
        .to_vec();
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.eased, [0, 0, 4]);
}

#[test]
fn a_joint_still_loaded_after_it_stopped_pushing_ends_the_run() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    let ms = Duration::from_millis;
    // Half a second of it: longer than giving way explains.
    lamp.strains = vec![(ms(1_200), ms(1_700), ROLL, 240)];
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!(
        report.stop,
        Stop::Load {
            joint: 4,
            load: 240
        },
        "{report:?}"
    );
    assert_eq!(report.eased, [0, 1, 0]);
    // Twelve steps: six to give way and six more of the same.
    let ended = clock.time.get();
    assert!(
        (ms(1_200 + 11 * 20)..ms(1_200 + 13 * 20)).contains(&ended),
        "{ended:?}"
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
    // never past the pitch window: a glance when the answer comes soon,
    // and the whole look away only when the lamp goes on thinking.
    let turned = |thinks_until_ms: u64, lasts: u64| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(lasts));
        lamp.servos[PITCH].position = 1_950.0;
        lamp.servos[PITCH].goal = 1_950;
        for servo in [PITCH, ROLL, YAW] {
            lamp.servos[servo].speed = 600;
            lamp.servos[servo].acceleration = 100;
        }
        let time = clock.time.clone();
        let news = move || {
            let at = time.get().as_millis() as u64;
            Some(match at {
                0..1_000 => (Mood::Resting, 0),
                1_000..3_000 => (Mood::Listening, 120),
                at if at < thinks_until_ms => (Mood::Thinking, 0),
                at if at < thinks_until_ms + 2_500 => (Mood::Speaking, 30),
                _ => (Mood::Resting, 0),
            })
        };
        let report = run(&mut lamp, &mut clock, limits, None, news, |_| {});
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        lamp
    };
    // An answer after 1.2 s: the base has glanced aside, 36 counts and a
    // little more, and no farther.
    let lamp = turned(4_200, 9);
    let farthest = |lamp: &Lamp, joint: Joint, home: u16| {
        lamp.goals
            .iter()
            .map(|write| goal_of(write, joint).abs_diff(home))
            .max()
            .unwrap()
    };
    let glanced = farthest(&lamp, Joint::BaseYaw, 2_015);
    assert!((30..=50).contains(&glanced), "{glanced}");
    // Six seconds of thinking: all the way.
    let lamp = turned(9_000, 14);
    let farthest = |joint: Joint, home: u16| farthest(&lamp, joint, home);
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
fn at_rest_a_joint_that_stopped_short_is_no_longer_pushed() {
    // The yaw servo comes to rest eight counts from wherever it is sent.
    let (mut lamp, mut clock) = folded(Duration::from_secs(12));
    lamp.short = Some((YAW, 8.0));
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
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    // From two and a half seconds into the rest that follows speaking until
    // it is asked to stop, the yaw goal is the yaw position: nothing pushes.
    let rested: Vec<&Glimpse> = seen
        .iter()
        .filter(|glimpse| glimpse.step > 9_500 / 20 && glimpse.step < 11_900 / 20)
        .collect();
    assert!(!rested.is_empty());
    assert!(
        rested
            .iter()
            .all(|glimpse| glimpse.goals[2] == glimpse.positions[2])
    );
    // And it still turned away while it thought, a real move from there.
    assert!(seen.iter().any(|glimpse| glimpse.mood == Mood::Thinking
        && glimpse.goals[2].abs_diff(2_015) >= 25));
}

#[test]
fn a_joint_that_has_arrived_is_not_pushed_at_and_a_small_change_does_not_move_it() {
    // The roll servo stops five counts from wherever it is sent, as the
    // head's did on lamp-4ace from the goals of its nods.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.short = Some((ROLL, 5.0));
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
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.eased, [0; 3]);
    let during = |from: u64, until: u64| -> Vec<&Glimpse> {
        seen.iter()
            .filter(|glimpse| (from / 20..until / 20).contains(&glimpse.step))
            .collect()
    };
    // Listening, once the head has tilted: every goal is where its joint
    // is, the one that stopped short included.
    let listening = during(2_600, 2_950);
    assert!(!listening.is_empty());
    for glimpse in &listening {
        assert_eq!(glimpse.goals, glimpse.positions, "{glimpse:?}");
    }
    assert!(listening[0].goals[1].abs_diff(HOME[1]) >= 15, "it did tilt");
    // Speaking: it came down from its thinking pose, and the nods on its
    // voice, a few counts each at this reach, ask nothing of it.
    let speaking = during(5_800, 6_950);
    assert!(
        speaking
            .iter()
            .all(|glimpse| glimpse.mood == Mood::Speaking)
    );
    assert!(
        speaking
            .iter()
            .all(|glimpse| glimpse.goals == speaking[0].goals),
        "a goal changed while it spoke"
    );
    assert!(speaking[0].goals[0] < HOME[0] && speaking[0].goals[0] > HOME[0] - 20);
    // A real move still happens: thinking turned it and lifted it.
    assert!(seen.iter().any(|glimpse| glimpse.mood == Mood::Thinking
        && glimpse.goals[2].abs_diff(HOME[2]) >= 25
        && glimpse.goals[0] + 25 <= HOME[0]));
}

#[test]
fn a_wire_that_goes_silent_stops_it_after_half_a_second() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.silent_from = Some(Duration::from_millis(1_500));
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert!(matches!(report.stop, Stop::Bus { .. }), "{:?}", report.stop);
    assert_eq!(report.missed_reads, u64::from(WIRE_STEPS));
    assert!(clock.time.get() < Duration::from_millis(1_500 + 20 * 27));
    // Nothing was asked of the joints once the wire had gone silent, but
    // the hold: no goal moved further from where the joints were.
    let last = lamp.goals.last().unwrap();
    for (id, goal) in last {
        let servo = lamp.servos[usize::from(*id) - 1];
        assert!(
            goal.abs_diff(servo.position.round() as u16) <= 8,
            "{last:?}"
        );
    }
}

#[test]
fn a_wire_that_fails_for_a_moment_is_ridden_out() {
    // Four steps of nothing, as after a stall of the whole machine, in the
    // middle of the look away.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.silent_from = Some(Duration::from_millis(3_400));
    lamp.silent_until = Some(Duration::from_millis(3_480));
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
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!((3..=6).contains(&report.missed_reads), "{report:?}");
    // It went on with the look away afterwards and came home at the end.
    assert!(seen.iter().any(|glimpse| glimpse.mood == Mood::Thinking
        && glimpse.step > 3_500 / 20
        && glimpse.goals[2].abs_diff(HOME[2]) >= 25));
    assert_eq!(lamp.goals.last().unwrap()[0], (5, HOME[0]));
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

/// A lamp's own window holds the head inside it as lamp-4ace's does, and is
/// taken only inside the head pitch servo's own limits.
#[test]
fn a_lamp_s_own_pitch_window_holds_the_head_and_must_fit_its_servo() {
    let own = Limits {
        pitch_window: (1_670, 1_995),
        ..Limits::FOLDED
    };
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.servos[PITCH].position = 1_990.0;
    lamp.servos[PITCH].goal = 1_990;
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, own, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let pitches: Vec<u16> = lamp
        .goals
        .iter()
        .map(|write| goal_of(write, Joint::WristPitch))
        .collect();
    // Found five counts under the window's top: every goal stays inside it.
    assert!(
        pitches.iter().all(|pitch| (1_670..=1_995).contains(pitch)),
        "{pitches:?}"
    );
    // A servo that may not reach the low end refuses the run, writing nothing.
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    lamp.servos[PITCH].range = (1_700, 3_000);
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, own, None, news, |_| {});
    match &report.stop {
        Stop::Refused { why } => assert!(why.contains("1700..=3000"), "{why}"),
        stop => panic!("{stop:?}"),
    }
    assert!(lamp.nothing_was_written());
}

/// Without a window of its own, a servo's limits are not even read: a lamp
/// whose servo says it can reach nothing runs exactly as before.
#[test]
fn without_a_window_of_its_own_the_lamp_runs_exactly_as_before() {
    let writes = |range: (u16, u16)| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(8));
        lamp.servos[PITCH].range = range;
        let news = script(&clock.time);
        let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        (lamp.goals, lamp.registers)
    };
    assert_eq!(writes((0, 0)), writes((0, 4_095)));
}

/// A pose to stand in for the simulated lamp, whose arm is found folded at
/// 2,050 and 2,047: the whole way in four seconds unless said otherwise.
fn standing(share: u8, awake: u16) -> Standing {
    Standing::new(
        [2_050, 2_047],
        [1_900, 2_044, 2_015, 2_650, 2_600],
        share,
        4,
        awake,
        [950, 950],
    )
    .unwrap()
}

/// Spoken to from one second to seven, then left alone.
fn spoken_to(time: &Rc<Cell<Duration>>) -> impl FnMut() -> Option<(Mood, u8)> + use<> {
    let time = time.clone();
    move || {
        let at = time.get().as_millis() as u64;
        Some(match at {
            1_000..3_000 => (Mood::Listening, 120),
            3_000..4_500 => (Mood::Thinking, 0),
            4_500..7_000 => (Mood::Speaking, 60),
            _ => (Mood::Resting, 0),
        })
    }
}

#[test]
fn the_two_writes_of_a_lamp_that_stands_are_exactly_these_bytes() {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(&all_goals_write(HOME, [2_048, 2_047])),
        "fffffe13832a0205d00704fc0701df0702000803ff0762"
    );
    assert_eq!(hex(&torque_write(Joint::ElbowPitch, 1)), "ffff0304032801cc");
}

/// The force limit: off unless chosen; written to the joints that carry
/// the lamp and no other, before anything moves; and read back.
#[test]
fn a_force_limit_is_written_to_the_joints_that_carry_the_lamp_and_read_back() {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(&force_write(Joint::BasePitch, 600)),
        "ffff0205033058026b"
    );
    let force_writes = |lamp: &Lamp| -> Vec<(u8, Vec<u8>)> {
        lamp.registers
            .iter()
            .filter(|(_, address, _)| *address == 48)
            .map(|(id, _, bytes)| (*id, bytes.clone()))
            .collect()
    };
    let capped = Limits {
        force: Some(600),
        ..Limits::FOLDED
    };

    // A lamp that stays folded: the base's turn alone. The shoulder and
    // the elbow are not this body's to write to unless it may stand.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, capped, None, news, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.force_writes, report.force), (1, Some(600)));
    assert_eq!(force_writes(&lamp), vec![(1, vec![88, 2])]);
    assert_eq!(
        lamp.servos.map(|servo| servo.force),
        [600, 1_000, 1_000, 1_000, 1_000]
    );
    // It was written before the first goal was.
    assert!(report.moved);

    // A lamp that may stand: the base's turn, the shoulder and the elbow,
    // and it stands up as before.
    let (mut lamp, mut clock) = folded(Duration::from_secs(22));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    let news = spoken_to(&clock.time);
    let report = run_standing(
        &mut lamp,
        &mut clock,
        capped,
        Some(HOME),
        Some(standing(100, 5)),
        news,
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.stood && report.retreats.is_empty());
    assert_eq!((report.force_writes, report.force), (3, Some(600)));
    assert_eq!(
        force_writes(&lamp),
        vec![(1, vec![88, 2]), (2, vec![88, 2]), (3, vec![88, 2])]
    );
    assert_eq!(
        lamp.servos.map(|servo| servo.force),
        [600, 600, 600, 1_000, 1_000]
    );

    // With none chosen nothing is written to that register at all.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    let news = script(&clock.time);
    let report = run(&mut lamp, &mut clock, Limits::FOLDED, None, news, |_| {});
    assert_eq!((report.force_writes, report.force), (0, None));
    assert!(force_writes(&lamp).is_empty());
}

/// A servo that does not keep the limit it was given is not moved: the
/// run ends before the first goal, and says what was read back.
#[test]
fn a_force_limit_that_is_not_kept_ends_the_run_before_anything_moves() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.servos[0].keeps_force = false;
    let news = script(&clock.time);
    let capped = Limits {
        force: Some(600),
        ..Limits::FOLDED
    };
    let report = run(&mut lamp, &mut clock, capped, None, news, |_| {});
    assert_eq!(
        report.stop,
        Stop::Bus {
            detail: "force limit read back as [1000]".into()
        },
        "{report:?}"
    );
    assert!(!report.moved && lamp.goals.is_empty());
    assert_eq!(report.force, None);
}

#[test]
fn a_standing_pose_is_taken_only_inside_its_caps() {
    let pose = [1_900, 2_044, 2_015, 2_650, 2_600];
    let new = |folded, pose, share, seconds, awake, load| {
        Standing::new(folded, pose, share, seconds, awake, load)
    };
    let taken = new([2_050, 2_047], pose, 30, 5, 90, [950, 950]).unwrap();
    assert_eq!((taken.share, taken.seconds), (0.3, 5.0));
    assert_eq!(taken.awake, Duration::from_secs(90));
    // Too far for a shoulder, off the encoder, no share, too quick, too
    // briefly awake, a load bound that is no bound.
    assert!(
        new(
            [2_050, 2_047],
            [1_900, 2_044, 2_015, 3_500, 2_600],
            30,
            5,
            90,
            [950, 950]
        )
        .is_err()
    );
    assert!(
        new(
            [2_050, 2_047],
            [4_096, 2_044, 2_015, 2_650, 2_600],
            30,
            5,
            90,
            [950, 950]
        )
        .is_err()
    );
    assert!(new([2_050, 2_047], pose, 0, 5, 90, [950, 950]).is_err());
    assert!(new([2_050, 2_047], pose, 101, 5, 90, [950, 950]).is_err());
    assert!(new([2_050, 2_047], pose, 30, 1, 90, [950, 950]).is_err());
    assert!(new([2_050, 2_047], pose, 30, 5, 4, [950, 950]).is_err());
    assert!(new([2_050, 2_047], pose, 30, 5, 90, [950, 1_001]).is_err());
    // The way up eases in and out, and its inverse finds the same place.
    assert_eq!((smooth(0.0), smooth(0.5), smooth(1.0)), (0.0, 0.5, 1.0));
    for share in [0.1f32, 0.37, 0.8] {
        assert!((unsmooth(smooth(share)) - share).abs() < 0.001);
    }
    assert_eq!(head_share(0.25), 0.0);
    assert_eq!(head_share(1.0), 1.0);
}

#[test]
fn spoken_to_it_stands_up_and_left_alone_it_folds_down_again() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(22));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    let news = spoken_to(&clock.time);
    let up = standing(100, 5);
    let mut highest = 0.0f32;
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(up),
        news,
        |glimpse| highest = highest.max(glimpse.stand),
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.standing, Some(up));
    assert_eq!(report.standing_refused, None);
    assert!(report.stood && report.retreats.is_empty());
    assert_eq!(highest, 1.0);

    // The elbow's old flag: torque written once, to it alone, and it is
    // gone. Then the slow profile on all five.
    assert_eq!(report.torque_writes, 1);
    let torque: Vec<_> = lamp
        .registers
        .iter()
        .filter(|(_, address, _)| *address == 40)
        .collect();
    assert_eq!(torque, vec![&(3, 40, vec![1])]);
    assert_eq!(lamp.servos[2].status, 0);
    for id in [2u8, 3] {
        assert!(
            lamp.registers
                .iter()
                .any(|(to, address, _)| (*to, *address) == (id, 46))
        );
        assert!(
            lamp.registers
                .iter()
                .any(|(to, address, _)| (*to, *address) == (id, 41))
        );
    }

    // Every goal write names all five joints, the arm stays between folded
    // and stood up, reaches the pose, and no goal jumps.
    let mut previous = [2_050i32, 2_047];
    let mut top = [0u16; 2];
    for write in &lamp.goals {
        let ids: Vec<u8> = write.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![5, 4, 1, 2, 3]);
        for (index, joint) in ARM.iter().enumerate() {
            let goal = goal_of(write, *joint);
            assert!(
                (up.folded[index]..=up.pose[3 + index]).contains(&goal),
                "{write:?}"
            );
            assert!(
                (i32::from(goal) - previous[index]).abs() <= 6,
                "{previous:?} {write:?}"
            );
            previous[index] = i32::from(goal);
            top[index] = top[index].max(goal);
        }
    }
    assert_eq!(top, [2_650, 2_600]);
    // The head went with it: its home rose to the standing pose's, and its
    // gestures stayed within their reach of that.
    let lowest = lamp
        .goals
        .iter()
        .map(|write| goal_of(write, Joint::WristPitch))
        .min()
        .unwrap();
    assert!((1_855..=1_900).contains(&lowest), "{lowest}");
    // And it is folded again as it was found, the head at its home.
    assert_eq!((lamp.servos[1].goal, lamp.servos[2].goal), (2_050, 2_047));
    assert!((lamp.servos[1].position - 2_050.0).abs() <= 2.0);
    assert!((lamp.servos[2].position - 2_047.0).abs() <= 2.0);
    assert_eq!(lamp.servos[PITCH].goal, 2_000);
    assert!((lamp.servos[PITCH].position - 2_000.0).abs() <= 2.0);
    serde_json::to_string(&report).unwrap();
}

#[test]
fn the_head_keeps_its_folded_pose_until_the_arm_has_lifted_it_clear() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(9));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    // A pose whose head pitch is toward the base: it must not go there early.
    let up = Standing::new(
        [2_050, 2_047],
        [2_100, 2_044, 2_015, 2_650, 2_600],
        100,
        4,
        60,
        [950, 950],
    )
    .unwrap();
    let news = spoken_to(&clock.time);
    let mut seen = Vec::new();
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(up),
        news,
        |glimpse| seen.push((glimpse.stand, glimpse.goals[0])),
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let limits = Limits::FOLDED;
    for (stand, pitch) in &seen {
        if *stand <= 0.25 {
            // Beside the base: inside the folded reach and window.
            assert!(
                f32::from(*pitch) <= 2_000.0 + limits.pitch_down,
                "{stand} {pitch}"
            );
            assert!(*pitch <= limits.pitch_window.1);
        }
    }
    assert!(
        seen.iter()
            .any(|(stand, pitch)| *stand > 0.99 && *pitch > 2_060)
    );
    assert_eq!(lamp.servos[PITCH].goal, 2_000);
}

#[test]
fn an_arm_that_strains_goes_back_down_and_stays_down_for_a_while() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(12));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    // The shoulder reads over its bound for a second on the way up.
    lamp.strains = vec![(
        Duration::from_millis(2_500),
        Duration::from_millis(3_500),
        1,
        990,
    )];
    let time = clock.time.clone();
    let news = move || Some((Mood::Listening, 100));
    let mut after = Vec::new();
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 60)),
        news,
        |glimpse| {
            if time.get() > Duration::from_secs(9) {
                after.push(glimpse.stand);
            }
        },
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.stood);
    assert_eq!(report.retreats.len(), 1, "{:?}", report.retreats);
    assert!(
        report.retreats[0].contains("base_pitch load 990"),
        "{:?}",
        report.retreats
    );
    assert_eq!(report.arm_peak_load[0], 990);
    // Still spoken to, and it stays down: it is resting.
    assert!(
        !after.is_empty() && after.iter().all(|stand| *stand == 0.0),
        "{after:?}"
    );
    assert_eq!((lamp.servos[1].goal, lamp.servos[2].goal), (2_050, 2_047));
}

#[test]
fn an_arm_still_strained_two_seconds_on_ends_the_run_where_it_is() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(12));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    lamp.strains = vec![(
        Duration::from_millis(2_500),
        Duration::from_secs(60),
        2,
        990,
    )];
    let news = || Some((Mood::Listening, 100));
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 60)),
        news,
        |_| {},
    );
    assert_eq!(
        report.stop,
        Stop::Load {
            joint: 3,
            load: 990
        },
        "{report:?}"
    );
    assert_eq!(report.retreats.len(), 1);
    // Nothing is written after it: the arm holds where it had got to.
    let last = lamp.goals.last().unwrap();
    assert_eq!(goal_of(last, Joint::ElbowPitch), lamp.servos[2].goal);
}

#[test]
fn found_standing_it_is_taken_as_it_is_and_folds_down_when_asked_to_stop() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(1));
    lamp.arm_moves = true;
    for (index, at) in [(1usize, 2_650.0f32), (2, 2_600.0), (PITCH, 1_900.0)] {
        lamp.servos[index].position = at;
        lamp.servos[index].goal = at as u16;
        lamp.servos[index].status = 0;
    }
    let news = || Some((Mood::Resting, 0));
    let mut first = None;
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 60)),
        news,
        |glimpse| {
            first.get_or_insert(glimpse.stand);
        },
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.torque_writes, 0);
    assert!(first.unwrap() > 0.98, "{first:?}");
    // Down again, by its own way, before it left.
    assert_eq!((lamp.servos[1].goal, lamp.servos[2].goal), (2_050, 2_047));
    assert_eq!(lamp.servos[PITCH].goal, 2_000);
    assert!((lamp.servos[1].position - 2_050.0).abs() <= 2.0);

    // Found somewhere that is not on the way, it runs as a folded lamp
    // would have to: it refuses, since its head is not at its home either.
    let (mut lamp, mut clock) = folded(Duration::from_secs(1));
    lamp.servos[1].position = 2_650.0;
    lamp.servos[1].goal = 2_650;
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 60)),
        || None,
        |_| {},
    );
    assert_eq!(report.standing, None);
    assert!(report.standing_refused.unwrap().contains("off the way"));
    assert!(lamp.goals.iter().all(|write| write.len() == 3));
}

#[test]
fn a_flag_that_stays_or_a_missing_home_keeps_it_folded_and_it_runs_as_before() {
    // Torque on, then off and on: the flag is still there.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.arm_moves = true;
    let news = script(&clock.time);
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 60)),
        news,
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.torque_writes, 3);
    assert_eq!(report.standing, None);
    assert!(
        report
            .standing_refused
            .unwrap()
            .contains("elbow_pitch keeps status 0x20")
    );
    assert!(!report.stood);
    assert!(lamp.goals.len() > 100);
    assert!(lamp.goals.iter().all(|write| write.len() == 3));
    assert_eq!((lamp.servos[1].goal, lamp.servos[2].goal), (2_050, 2_047));
    assert_eq!(lamp.servos[2].torque, 1);

    // One that clears only when torque goes off and on again does stand.
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.arm_moves = true;
    lamp.overload_clears = 2;
    let news = spoken_to(&clock.time);
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 60)),
        news,
        |_| {},
    );
    assert_eq!(report.torque_writes, 3);
    assert!(report.stood && report.standing.is_some(), "{report:?}");

    // Without a home there is no telling folded from stood up.
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    let news = script(&clock.time);
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        None,
        Some(standing(100, 60)),
        news,
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled);
    assert_eq!(report.standing, None);
    assert_eq!(report.torque_writes, 0);
    assert!(report.standing_refused.unwrap().contains("needs a home"));
}

/// The eyes: a face at this bearing (counts of yaw and pitch from the home
/// direction), seen by a camera in the simulated head six times a second.
fn face_at(
    lamp_yaw_pitch: Rc<Cell<(f32, f32)>>,
    time: &Rc<Cell<Duration>>,
    bearing: (f32, f32),
) -> impl FnMut() -> Option<Seen> + use<> {
    let time = time.clone();
    let mut last = Duration::ZERO;
    move || {
        let now = time.get();
        if now.saturating_sub(last) < Duration::from_millis(150) {
            return None;
        }
        last = now;
        let (yaw, pitch) = lamp_yaw_pitch.get();
        // The picture is worth 342 counts across and 200 down, each half.
        let x = (bearing.0 - (yaw - f32::from(HOME[2]))) / 342.0;
        let y = (bearing.1 - (pitch - f32::from(HOME[0]))) / 200.0;
        Some(Seen {
            face: (x.abs() <= 1.0 && y.abs() <= 1.0).then_some(crate::gaze::Sight {
                x,
                y,
                size: 0.2,
                turned: 0.0,
            }),
        })
    }
}

#[test]
fn the_eyes_take_the_head_further_round_than_any_pose_does() {
    // A person standing 300 counts to one side, about 26 degrees: further
    // than this body turns for any pose (150), inside what its eyes may ask.
    let turned_for = |gaze_reach: u16| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(16));
        let looking = Looking::new([342, 200], [gaze_reach, 120]).unwrap();
        let limits = Limits::folded_with(Tuning {
            reach: [100, 15, 30, 150],
            speed: 120,
            servo_speed: 300,
            servo_acceleration: 60,
        })
        .unwrap();
        let seen_from = Rc::new(Cell::new((f32::from(HOME[2]), f32::from(HOME[0]))));
        let eyes = face_at(seen_from.clone(), &clock.time, (300.0, -60.0));
        let news = || Some((Mood::Listening, 100));
        let furthest = Cell::new(0.0f32);
        let report = run_looking(
            &mut lamp,
            &mut clock,
            limits,
            None,
            None,
            Some(looking),
            news,
            eyes,
            |glimpse| {
                seen_from.set((
                    f32::from(glimpse.positions[2]),
                    f32::from(glimpse.positions[0]),
                ));
                let yaw = f32::from(glimpse.positions[2]) - f32::from(HOME[2]);
                furthest.set(furthest.get().max(yaw));
            },
        );
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        furthest.get()
    };
    // With the eyes held to the poses' own reach it stops there, short.
    let short = turned_for(150);
    assert!((140.0..=152.0).contains(&short), "{short}");
    // Given more, it comes round to the person, and not past them.
    let round = turned_for(400);
    assert!((250.0..=330.0).contains(&round), "{round}");
}

#[test]
fn with_eyes_the_head_turns_to_the_face_while_it_listens_and_not_beyond_its_reach() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(14));
    let looking = Looking::new([342, 200], [250, 120]).unwrap();
    // A reach wide enough to turn to someone 120 counts to one side and 60
    // above: a third of the way to the edge of the picture.
    let limits = Limits::folded_with(Tuning {
        reach: [100, 15, 30, 150],
        speed: 120,
        servo_speed: 300,
        servo_acceleration: 60,
    })
    .unwrap();
    let seen_from = Rc::new(Cell::new((f32::from(HOME[2]), f32::from(HOME[0]))));
    let eyes = face_at(seen_from.clone(), &clock.time, (120.0, -60.0));
    let time = clock.time.clone();
    let news = move || {
        Some(if time.get() < Duration::from_secs(10) {
            (Mood::Listening, 100)
        } else {
            (Mood::Resting, 0)
        })
    };
    let mut looks = Vec::new();
    let while_listening = Cell::new((0.0f32, 0.0f32));
    let watched = clock.time.clone();
    let report = run_looking(
        &mut lamp,
        &mut clock,
        limits,
        None,
        None,
        Some(looking),
        news,
        eyes,
        |glimpse| {
            // The camera is where the head is.
            seen_from.set((
                f32::from(glimpse.positions[2]),
                f32::from(glimpse.positions[0]),
            ));
            if watched.get() < Duration::from_secs(9) {
                while_listening.set(seen_from.get());
            }
            looks.push(glimpse.look);
        },
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    // It ended up looking toward the face, within what counts as near.
    // (The last looks are of it letting go, once it was asked to stop.)
    let turned = looks
        .iter()
        .copied()
        .max_by(|a, b| a[0].total_cmp(&b[0]))
        .unwrap();
    assert!((turned[0] - 120.0).abs() < 342.0 * 0.15, "{turned:?}");
    // Lifting to listen had already brought the face near the middle up
    // and down, so nothing was added there. Where the camera ended up, the
    // face is near the middle of its picture both ways.
    let (yaw, pitch) = while_listening.get();
    assert!(((120.0 - (yaw - 2_015.0)) / 342.0).abs() < 0.15, "{yaw}");
    assert!(
        ((-60.0 - (pitch - 2_000.0)) / 200.0).abs() < 0.20,
        "{pitch}"
    );
    // The head did go there, and every goal stayed inside the same reach
    // as every gesture.
    let furthest = lamp
        .goals
        .iter()
        .map(|write| goal_of(write, Joint::BaseYaw))
        .max()
        .unwrap();
    assert!(furthest >= 2_015 + 60, "{furthest}");
    for write in &lamp.goals {
        let yaw = f32::from(goal_of(write, Joint::BaseYaw));
        let pitch = f32::from(goal_of(write, Joint::WristPitch));
        assert!((yaw - 2_015.0).abs() <= limits.yaw, "{write:?}");
        assert!(pitch >= 2_000.0 - limits.pitch_up && pitch <= 2_000.0 + limits.pitch_down);
        assert_eq!(write.len(), 3);
    }
    // And it came home when asked to stop.
    assert_eq!(lamp.servos[YAW].goal, 2_015);
    assert_eq!(lamp.servos[PITCH].goal, 2_000);
}

#[test]
fn without_a_face_or_without_eyes_nothing_is_added_to_what_the_mood_wants() {
    let run_with = |looking: Option<Looking>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(8));
        let news = script(&clock.time);
        let report = run_looking(
            &mut lamp,
            &mut clock,
            Limits::FOLDED,
            None,
            None,
            looking,
            news,
            || Some(Seen { face: None }),
            |_| {},
        );
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        lamp.goals
    };
    let plain = run_with(None);
    let eyed = run_with(Some(Looking::new([342, 200], [250, 120]).unwrap()));
    assert!(plain.len() > 100);
    assert_eq!(plain, eyed);
}

#[test]
fn an_elbow_pushing_under_an_old_overload_is_told_to_stop_and_then_carries_the_lamp() {
    // As lamp-4ace's was found: three counts short of its goal, which its
    // own friction keeps it from, flagged, and the flag goes only once it
    // is no longer pushing.
    let (mut lamp, mut clock) = folded(Duration::from_secs(22));
    lamp.arm_moves = true;
    lamp.overload_clears = 3;
    lamp.short = Some((2, 3.0));
    lamp.servos[2].position = 2_044.0;
    lamp.servos[2].goal = 2_047;
    let news = spoken_to(&clock.time);
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 5)),
        news,
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.standing_refused, None);
    // No torque was written: stopping the push was enough.
    assert_eq!(report.torque_writes, 0);
    assert!(lamp.registers.contains(&(3, 42, vec![0xfc, 0x07])));
    assert!(report.stood, "{report:?}");
    assert_eq!(lamp.servos[2].status, 0);
    // Down again, its arm is left resting where it is, not pushed at.
    let (shoulder, elbow) = (lamp.servos[1], lamp.servos[2]);
    assert!((f32::from(shoulder.goal) - shoulder.position).abs() < 1.0);
    assert!((f32::from(elbow.goal) - elbow.position).abs() < 1.0);
}

#[test]
fn stood_up_the_head_may_look_down_as_far_as_up_and_folded_it_may_not() {
    // A face well below the middle of the picture, for a lamp that listens.
    let below = || {
        Some(Seen {
            face: Some(crate::gaze::Sight {
                x: 0.0,
                y: 0.9,
                size: 0.2,
                turned: 0.0,
            }),
        })
    };
    let looking = Looking::new([342, 200], [250, 120]).unwrap();
    // The furthest down the head's pitch was asked to go, folded and at
    // its full height.
    let lowest = |stands: Option<Standing>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(20));
        lamp.arm_moves = true;
        lamp.overload_clears = 1;
        let (mut down, mut up) = (0u16, 0u16);
        let report = run_looking(
            &mut lamp,
            &mut clock,
            Limits::FOLDED,
            Some(HOME),
            stands,
            Some(looking),
            || Some((Mood::Listening, 100)),
            below,
            |glimpse| {
                if glimpse.stand <= 0.0 {
                    down = down.max(glimpse.goals[0]);
                } else if glimpse.stand >= 1.0 {
                    up = up.max(glimpse.goals[0]);
                }
            },
        );
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        (down, up)
    };
    // Folded, with its home at 2,000: fifteen counts under it and no more,
    // whatever is seen.
    assert_eq!(lowest(None), (2_015, 0));
    // Stood up, with its home at 1,900: down by its whole upward reach of 45.
    let (folded_low, stood_low) = lowest(Some(standing(100, 60)));
    assert!(folded_low <= 2_015, "{folded_low}");
    // (It comes to rest within a count of that bound.)
    assert!((1_944..=1_945).contains(&stood_low), "{stood_low}");
}

/// One step of a run as a gesture test watches it: when, the gesture being
/// made, the head's goals and the arm's.
type Step = (Duration, Option<&'static str>, [u16; 3], [u16; 2]);

/// A lamp that stands at once and is spoken to throughout, asked for one
/// gesture at a given moment.
fn gesture_run(
    clip: &'static Clip,
    ask_at: Duration,
    seconds: u64,
    prepare: impl FnOnce(&mut Lamp),
) -> (BodyReport, Lamp, Vec<Step>) {
    let (mut lamp, mut clock) = folded(Duration::from_secs(seconds));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    prepare(&mut lamp);
    let limits = Limits::folded_with(Tuning {
        reach: [150, 15, 1, 250],
        speed: 120,
        servo_speed: 1_000,
        servo_acceleration: 60,
    })
    .unwrap();
    let time = clock.time.clone();
    let seen_at = clock.time.clone();
    let mut asked_once = false;
    let mut seen = Vec::new();
    let report = run_alive(
        &mut lamp,
        &mut clock,
        limits,
        Some(HOME),
        Some(standing(100, 60)),
        None,
        || Some((Mood::Speaking, 60)),
        move || {
            let due = !asked_once && time.get() >= ask_at;
            asked_once |= due;
            Asked {
                seen: None,
                gesture: due.then_some(clip),
                expression: None,
                called: false,
            }
        },
        |glimpse| {
            seen.push((
                seen_at.get(),
                glimpse.gesture,
                glimpse.goals,
                glimpse.arm_goals,
            ));
        },
    );
    (report, lamp, seen)
}

/// A lamp that stands at once and is in one mood throughout, asked by
/// Expression for `offsets` from `from` until `until`. Each step: when,
/// and the head's three goals.
fn expression_run(
    mood: Mood,
    offsets: [f32; 3],
    from: Duration,
    until: Duration,
    seconds: u64,
) -> (BodyReport, Vec<(Duration, [u16; 3])>) {
    expression_run_with(mood, offsets, from, until, seconds, false)
}

fn expression_run_with(
    mood: Mood,
    offsets: [f32; 3],
    from: Duration,
    until: Duration,
    seconds: u64,
    rest_glance: bool,
) -> (BodyReport, Vec<(Duration, [u16; 3])>) {
    let (mut lamp, mut clock) = folded(Duration::from_secs(seconds));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    let limits = Limits::folded_with(Tuning {
        reach: [150, 15, 1, 250],
        speed: 120,
        servo_speed: 1_000,
        servo_acceleration: 60,
    })
    .map(|limits| Limits {
        rest_glance,
        ..limits
    })
    .unwrap();
    let time = clock.time.clone();
    let seen_at = clock.time.clone();
    let mut seen = Vec::new();
    let report = run_alive(
        &mut lamp,
        &mut clock,
        limits,
        Some(HOME),
        Some(standing(100, 60)),
        None,
        move || Some((mood, 60)),
        move || Asked {
            seen: None,
            gesture: None,
            expression: (time.get() >= from && time.get() < until).then_some(offsets),
            called: false,
        },
        |glimpse| seen.push((seen_at.get(), glimpse.goals)),
    );
    (report, seen)
}

/// What Expression asks is added to the pose the lamp listens in: the
/// head dips for as long as it is asked and comes back to where it was,
/// at the speed everything else moves at, and nothing else turns.
#[test]
fn a_listening_lamp_dips_its_head_for_expression_and_comes_back() {
    let (from, until) = (Duration::from_secs(8), Duration::from_millis(8_600));
    // The run goes on for four seconds after what is looked at here: a
    // lamp that is told to stop folds down, and that is not the dip.
    let watched = Duration::from_secs(12);
    let (report, seen) = expression_run(Mood::Listening, [40.0, 0.0, 0.0], from, until, 16);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.expression_steps, 30, "0.6 s of 20 ms steps");
    assert!(report.retreats.is_empty(), "{:?}", report.retreats);
    let at = |when: Duration| seen.iter().rfind(|step| step.0 < when).unwrap().1;
    let before = at(from);
    // It listens with its head lifted, well clear of its home.
    assert!(before[0] + 40 < HOME[0], "{before:?} against {HOME:?}");
    let lowest = seen
        .iter()
        .filter(|step| step.0 >= from && step.0 < watched)
        .map(|step| step.1[0])
        .max()
        .unwrap();
    // A dip of 40 was asked for 0.6 s. The pose follows at 120 counts a
    // second with its own gentle start, so most of it is made.
    let dip = lowest - before[0];
    assert!((24..=41).contains(&dip), "the head dipped {dip} counts");
    // Back where it listened from, and at rest there.
    let after = at(Duration::from_millis(11_500));
    assert!(
        after[0].abs_diff(before[0]) <= 6,
        "{after:?} against {before:?}"
    );
    // Nothing else moved, and no goal leapt.
    for (index, name) in [(1, "roll"), (2, "yaw")] {
        let values: Vec<u16> = seen
            .iter()
            .filter(|step| step.0 >= from && step.0 < watched)
            .map(|step| step.1[index])
            .collect();
        let swing = values.iter().max().unwrap() - values.iter().min().unwrap();
        assert!(swing <= 1, "{name} moved {swing}");
    }
    for pair in seen
        .windows(2)
        .filter(|pair| pair[0].0 >= from && pair[1].0 < watched)
    {
        assert!(pair[1].1[0].abs_diff(pair[0].1[0]) <= 3, "{pair:?}");
    }
}

/// What is asked is cut to a few degrees and then to the joint's own
/// reach, a number that is not one is nothing, and a lamp at rest is not
/// stirred by it.
#[test]
fn expression_is_held_to_its_own_reach_and_the_joints_and_does_not_stir_a_resting_lamp() {
    let (from, until) = (Duration::from_secs(8), Duration::from_secs(10));
    let (report, seen) =
        expression_run(Mood::Listening, [500.0, f32::NAN, -500.0], from, until, 14);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let before = seen.iter().rfind(|step| step.0 < from).unwrap().1;
    let during: Vec<[u16; 3]> = seen
        .iter()
        .filter(|step| step.0 >= from && step.0 < until)
        .map(|step| step.1)
        .collect();
    let lowest = during.iter().map(|goals| goals[0]).max().unwrap();
    let leftmost = during.iter().map(|goals| goals[2]).min().unwrap();
    // Sixty counts of dip at the most, and never past what the head may
    // dip below its home; seventy of turn.
    assert!(lowest - before[0] <= 60, "dipped {}", lowest - before[0]);
    assert!(lowest <= HOME[0] + 15, "{lowest} against {HOME:?}");
    assert!(
        (60..=70).contains(&(before[2] - leftmost)),
        "turned {}",
        before[2] - leftmost
    );
    let roll: Vec<u16> = during.iter().map(|goals| goals[1]).collect();
    assert!(roll.iter().max().unwrap() - roll.iter().min().unwrap() <= 1);
    // At rest for eight seconds, and then asked: nothing is written.
    let (report, seen) = expression_run(Mood::Resting, [40.0, 0.0, 40.0], from, until, 14);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let before = seen.iter().rfind(|step| step.0 < from).unwrap().1;
    for step in seen
        .iter()
        .filter(|step| step.0 >= from && step.0 < Duration::from_secs(11))
    {
        assert_eq!(step.1, before, "at {:?}", step.0);
    }
}

/// Without `rest_glance`, a lamp at rest writes exactly what it writes
/// when nothing is asked of it.
#[test]
fn without_the_switch_a_resting_lamp_writes_exactly_as_when_nothing_is_asked() {
    let (from, until) = (Duration::from_secs(8), Duration::from_millis(9_500));
    let (_, asked) = expression_run(Mood::Resting, [-46.0, 30.0, 40.0], from, until, 14);
    let (_, quiet) = expression_run(Mood::Resting, [0.0; 3], from, from, 14);
    assert_eq!(asked, quiet);
}

/// With `rest_glance`, a lamp at rest lifts its head for as long as it is
/// asked, within the smaller reach, turns no base, and rests again where it
/// was.
#[test]
fn a_resting_lamp_glances_with_the_switch_and_comes_back_to_rest() {
    let (from, until) = (Duration::from_secs(8), Duration::from_millis(9_500));
    let (report, seen) =
        expression_run_with(Mood::Resting, [-500.0, 500.0, 40.0], from, until, 14, true);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let before = seen.iter().rfind(|step| step.0 < from).unwrap().1;
    let during: Vec<[u16; 3]> = seen
        .iter()
        .filter(|step| step.0 >= from && step.0 < until)
        .map(|step| step.1)
        .collect();
    let highest = during.iter().map(|goals| goals[0]).min().unwrap();
    let lift = before[0] - highest;
    // Fifty counts at the most, its goal led forty ahead of that.
    assert!((80..=90).contains(&lift), "lifted {lift}");
    let tilt = during.iter().map(|goals| goals[1]).max().unwrap() - before[1];
    // The roll's own reach here is one count, and it is kept to it.
    assert!(tilt <= 1, "tilted {tilt}");
    assert!(during.iter().all(|goals| goals[2] == before[2]));
    let after = seen
        .iter()
        .rfind(|step| step.0 < Duration::from_millis(13_500))
        .unwrap()
        .1;
    for index in 0..3 {
        assert!(
            after[index].abs_diff(before[index]) <= 6,
            "{after:?} against {before:?}"
        );
    }
}

#[test]
fn a_standing_lamp_nods_and_is_left_exactly_where_it_was() {
    // Standing takes a little over four seconds; the nod is asked at eight.
    let (report, lamp, seen) = gesture_run(&crate::clips::NOD, Duration::from_secs(8), 16, |_| {});
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.gestures, report.gestures_dropped), (1, 0));
    assert!(report.retreats.is_empty(), "{:?}", report.retreats);
    let during: Vec<_> = seen.iter().filter(|step| step.1 == Some("nod")).collect();
    let before = seen
        .iter()
        .rfind(|step| step.0 < Duration::from_secs(8))
        .unwrap();
    let after = seen
        .iter()
        .rfind(|step| step.0 < Duration::from_secs(13))
        .unwrap();
    // It lasted as long as the clip, moved the head's pitch and both joints
    // of the arm by about what the clip says, and nothing else.
    let lasted = during.last().unwrap().0 - during[0].0;
    assert!(
        (lasted.as_millis() as i64 - 3_068).abs() < 100,
        "{lasted:?}"
    );
    let swing = |pick: &dyn Fn(&Step) -> u16| {
        let values: Vec<u16> = during.iter().map(|step| pick(step)).collect();
        values.iter().max().unwrap() - values.iter().min().unwrap()
    };
    assert!(
        (100..=115).contains(&swing(&|step| step.2[0])),
        "{}",
        swing(&|step| step.2[0])
    );
    assert!(
        (135..=150).contains(&swing(&|step| step.3[0])),
        "{}",
        swing(&|step| step.3[0])
    );
    assert!(
        (200..=220).contains(&swing(&|step| step.3[1])),
        "{}",
        swing(&|step| step.3[1])
    );
    assert_eq!(swing(&|step| step.2[2]), 0);
    // No goal jumps: a clip is followed, not leapt to.
    for pair in during.windows(2) {
        assert!(pair[1].2[0].abs_diff(pair[0].2[0]) <= 24, "{pair:?}");
        assert!(pair[1].3[1].abs_diff(pair[0].3[1]) <= 21, "{pair:?}");
    }
    // And afterwards it stands as it stood.
    assert_eq!((before.2, before.3), (after.2, after.3));
    assert_eq!(after.1, None);
    assert_eq!((lamp.servos[1].goal, lamp.servos[2].goal), (2_050, 2_047));
}

#[test]
fn a_gesture_waits_for_a_lamp_that_is_rising_and_is_forgotten_by_one_that_is_not() {
    // Asked three seconds in, while it rises: it begins once it stands.
    let (report, _, seen) = gesture_run(&crate::clips::NOD, Duration::from_secs(3), 12, |_| {});
    assert_eq!(report.gestures, 1, "{report:?}");
    let began = seen.iter().find(|step| step.1.is_some()).unwrap().0;
    assert!(
        began > Duration::from_millis(4_400) && began < Duration::from_secs(6),
        "{began:?}"
    );
    // Asked at once, more than three seconds before it stands: forgotten.
    let (report, _, seen) = gesture_run(&crate::clips::NOD, Duration::from_millis(100), 10, |_| {});
    assert_eq!(report.gestures, 0);
    assert!(seen.iter().all(|step| step.1.is_none()));
    // A folded lamp that may not stand makes none either.
    let (mut lamp, mut clock) = folded(Duration::from_secs(4));
    let mut asked_once = false;
    let report = run_alive(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        None,
        None,
        None,
        || Some((Mood::Speaking, 60)),
        move || Asked {
            seen: None,
            gesture: (!std::mem::replace(&mut asked_once, true)).then_some(&crate::clips::NOD),
            expression: None,
            called: false,
        },
        |_| {},
    );
    assert_eq!((report.stop, report.gestures), (Stop::Cancelled, 0));
    assert!(lamp.goals.iter().all(|write| write.len() == 3));
}

#[test]
fn a_joint_that_cannot_keep_up_ends_the_gesture_and_not_the_run() {
    // The head's pitch meets something a little above where it stands.
    let (report, lamp, seen) =
        gesture_run(&crate::clips::NOD, Duration::from_secs(8), 16, |lamp| {
            lamp.strains = vec![(
                Duration::from_millis(8_600),
                Duration::from_millis(9_200),
                PITCH,
                900,
            )];
        });
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.gestures, report.gestures_dropped), (1, 1));
    // It let go early, went back to where it stood, and stayed standing.
    let during: Vec<_> = seen.iter().filter(|step| step.1 == Some("nod")).collect();
    let lasted = during.last().unwrap().0 - during[0].0;
    assert!(lasted < Duration::from_millis(1_200), "{lasted:?}");
    let before = seen
        .iter()
        .rfind(|step| step.0 < Duration::from_secs(8))
        .unwrap();
    let after = seen
        .iter()
        .rfind(|step| step.0 < Duration::from_secs(13))
        .unwrap();
    assert_eq!((before.2, before.3), (after.2, after.3));
    assert!(report.retreats.is_empty());
    assert_eq!(lamp.servos[2].status, 0);
}

#[test]
fn the_heads_pitch_is_never_asked_under_its_floor_whatever_adds_up() {
    // A stretch lifts the head 112 counts; with a floor just under where it
    // stands, the head stops there and the arm makes its move all the same.
    let (mut lamp, mut clock) = folded(Duration::from_secs(16));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    let limits = Limits {
        pitch_floor: 1_880,
        ..Limits::folded_with(Tuning {
            reach: [150, 15, 1, 250],
            speed: 120,
            servo_speed: 1_000,
            servo_acceleration: 60,
        })
        .unwrap()
    };
    let time = clock.time.clone();
    let mut asked_once = false;
    let (mut lowest, mut highest_arm) = (u16::MAX, 0u16);
    let report = run_alive(
        &mut lamp,
        &mut clock,
        limits,
        Some(HOME),
        Some(standing(100, 60)),
        None,
        || Some((Mood::Thinking, 0)),
        move || {
            let due = !asked_once && time.get() >= Duration::from_secs(8);
            asked_once |= due;
            Asked {
                seen: None,
                gesture: due.then_some(&crate::clips::STRETCHING),
                expression: None,
                called: false,
            }
        },
        |glimpse| {
            lowest = lowest.min(glimpse.goals[0]);
            highest_arm = highest_arm.max(glimpse.arm_goals[0]);
        },
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.gestures, report.gestures_dropped), (1, 0));
    assert_eq!(lowest, 1_880);
    assert!(highest_arm >= 2_650 + 290, "{highest_arm}");
}

#[test]
fn stood_up_the_head_tilts_to_listen_and_folded_it_does_not() {
    // A lamp whose head cannot tilt where it is folded: a roll reach of one
    // count, and more once it has stood up.
    let tilt = |stand_roll: f32| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(12));
        lamp.arm_moves = true;
        lamp.overload_clears = 1;
        let limits = Limits {
            stand_roll,
            ..Limits::folded_with(Tuning {
                reach: [150, 15, 1, 250],
                speed: 120,
                servo_speed: 1_000,
                servo_acceleration: 60,
            })
            .unwrap()
        };
        let (mut folded_tilt, mut stood_tilt) = (0u16, 0u16);
        let report = run_standing(
            &mut lamp,
            &mut clock,
            limits,
            Some(HOME),
            Some(standing(100, 60)),
            || Some((Mood::Listening, 100)),
            |glimpse| {
                let tilt = glimpse.goals[1].abs_diff(HOME[1]);
                if glimpse.stand <= 0.25 {
                    folded_tilt = folded_tilt.max(tilt);
                } else if glimpse.stand >= 1.0 {
                    stood_tilt = stood_tilt.max(tilt);
                }
            },
        );
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        (folded_tilt, stood_tilt)
    };
    // Listening tilts 85% of the reach: of one count folded, and of 110
    // once it stands.
    let (folded_tilt, stood_tilt) = tilt(110.0);
    assert!(folded_tilt <= 1, "{folded_tilt}");
    assert!((90..=94).contains(&stood_tilt), "{stood_tilt}");
    // Without a standing reach it never tilts.
    assert!(tilt(0.0).1 <= 1);
}

#[test]
fn standing_with_nothing_to_do_it_shifts_now_and_then_and_stops_when_spoken_to() {
    // Spoken to for two seconds, then left standing for a minute.
    let run_with = |idle: bool, spoken_again_at: Option<u64>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(40));
        lamp.arm_moves = true;
        lamp.overload_clears = 1;
        let limits = Limits::folded_with(Tuning {
            reach: [150, 15, 1, 250],
            speed: 120,
            servo_speed: 1_000,
            servo_acceleration: 60,
        })
        .unwrap();
        let time = clock.time.clone();
        let watched = clock.time.clone();
        let mut seen: Vec<(Duration, Option<&'static str>, Mood)> = Vec::new();
        let report = run_alive(
            &mut lamp,
            &mut clock,
            limits,
            Some(HOME),
            Some(standing(100, 120).idling(idle)),
            None,
            move || {
                let at = time.get().as_secs();
                let spoken = at < 2 || spoken_again_at.is_some_and(|again| at >= again);
                Some(if spoken {
                    (Mood::Listening, 100)
                } else {
                    (Mood::Resting, 0)
                })
            },
            Asked::default,
            |glimpse| seen.push((watched.get(), glimpse.gesture, glimpse.mood)),
        );
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        (report, seen)
    };
    // It stands by five seconds, rests four, shifts for ten, rests eight,
    // shifts again.
    let (report, seen) = run_with(true, None);
    assert_eq!(
        (report.idles, report.gestures, report.gestures_dropped),
        (2, 0, 0)
    );
    let first = seen.iter().find(|step| step.1 == Some("idle")).unwrap().0;
    assert!(
        first > Duration::from_secs(8) && first < Duration::from_secs(11),
        "{first:?}"
    );
    assert!(report.retreats.is_empty());
    // Not asked to, it stands still.
    let (report, seen) = run_with(false, None);
    assert_eq!(report.idles, 0);
    assert!(seen.iter().all(|step| step.1.is_none()));
    // Spoken to in the middle of a shift, it lets the shift go at once.
    let (report, seen) = run_with(true, Some(14));
    assert_eq!(report.idles, 1);
    let last = seen.iter().rfind(|step| step.1 == Some("idle")).unwrap().0;
    assert!(last < Duration::from_millis(14_100), "{last:?}");
    assert!(
        seen.iter()
            .filter(|step| step.2 == Mood::Listening && step.0 >= Duration::from_secs(14))
            .all(|step| step.1.is_none())
    );
}

#[test]
fn it_stands_up_for_somebody_in_front_of_it_and_stays_up_while_they_are_there() {
    // Nobody speaks. A face is in front of the lamp from two seconds to
    // twenty, seen six times a second, and then gone.
    let company = |from: u64, until: u64| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(34));
        lamp.arm_moves = true;
        lamp.overload_clears = 1;
        let time = clock.time.clone();
        let watched = clock.time.clone();
        let mut last = Duration::ZERO;
        let mut stood: Vec<(Duration, f32)> = Vec::new();
        let report = run_alive(
            &mut lamp,
            &mut clock,
            Limits::FOLDED,
            Some(HOME),
            Some(standing(100, 5)),
            Some(Looking::new([342, 200], [250, 120]).unwrap()),
            || Some((Mood::Resting, 0)),
            move || {
                let now = time.get();
                if now.saturating_sub(last) < Duration::from_millis(150) {
                    return Asked::default();
                }
                last = now;
                let there = (from..until).contains(&now.as_secs());
                Asked {
                    seen: Some(Seen {
                        face: there.then_some(crate::gaze::Sight {
                            x: 0.0,
                            y: 0.0,
                            size: 0.2,
                            turned: 0.0,
                        }),
                    }),
                    gesture: None,
                    expression: None,
                    called: false,
                }
            },
            |glimpse| stood.push((watched.get(), glimpse.stand)),
        );
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        stood
    };
    let stood = company(2, 20);
    let at = |seconds: u64| {
        stood
            .iter()
            .rfind(|step| step.0 <= Duration::from_secs(seconds))
            .unwrap()
            .1
    };
    // Folded until a second after the face appeared, standing by eight
    // seconds, still standing while it is there, and through the five
    // seconds it stays awake after; folded again after that.
    assert_eq!(at(2), 0.0);
    assert_eq!(at(9), 1.0);
    assert_eq!(at(19), 1.0);
    assert_eq!(at(24), 1.0);
    assert_eq!(at(33), 0.0);
    // A face for half a second, somebody walking past, wakes nothing.
    let passed = company(2, 2);
    assert!(passed.iter().all(|step| step.1 == 0.0));
}

/// Told to take hold: what would refuse it for want of torque is given
/// torque, each joint's goal first set to where it is. Without the switch
/// such a lamp is refused as before (see the test of a lamp that is not as
/// expected) and nothing is written.
#[test]
fn a_joint_found_without_torque_is_given_it_goal_first_and_does_not_move() {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(&goal_write(Joint::WristPitch, 2_000)),
        "ffff0505032ad007f1"
    );
    assert_eq!(hex(&torque_write(Joint::WristPitch, 1)), "ffff0504032801ca");
    let holding = Limits {
        torque_on: true,
        ..Limits::FOLDED
    };
    let (mut lamp, mut clock) = folded(Duration::from_secs(1));
    lamp.may_take_hold = true;
    // As on lamp-52e6 after its old runtime stopped: torque off, and each
    // goal some way from where the joint is.
    for (index, stale) in [(PITCH, 2_025), (ROLL, 2_014), (YAW, 2_033)] {
        lamp.servos[index].torque = 0;
        lamp.servos[index].goal = stale;
    }
    let found = lamp.servos.map(|servo| servo.position);
    let report = run(
        &mut lamp,
        &mut clock,
        holding,
        None,
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.torque_writes, 3);
    assert_eq!(
        report.torque_given,
        vec!["wrist_pitch", "wrist_roll", "base_yaw"]
    );
    // Goal, then torque, for each in turn, before anything else.
    let goal = |value: u16| value.to_le_bytes().to_vec();
    assert_eq!(
        lamp.registers[..6],
        [
            (5, 42, goal(2_000)),
            (5, 40, vec![1]),
            (4, 42, goal(2_044)),
            (4, 40, vec![1]),
            (1, 42, goal(2_015)),
            (1, 40, vec![1]),
        ]
    );
    assert!(lamp.servos.iter().all(|servo| servo.torque == 1));
    assert_eq!(lamp.servos.map(|servo| servo.position), found);
}

/// A goal that was not taken shows as a joint that moves when its torque
/// comes on: the run ends there, naming it, with its goal where it went.
#[test]
fn a_joint_that_moves_when_its_torque_comes_on_ends_the_run() {
    let holding = Limits {
        torque_on: true,
        ..Limits::FOLDED
    };
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    lamp.may_take_hold = true;
    lamp.servos[ROLL].torque = 0;
    lamp.servos[ROLL].goal = 2_074;
    lamp.drops_goal = Some(ROLL);
    let report = run(
        &mut lamp,
        &mut clock,
        holding,
        None,
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(
        report.stop,
        Stop::Refused {
            why: "wrist_roll moved 30 counts when its torque came on".into()
        }
    );
    assert!(!report.moved && lamp.goals.is_empty());
    // Goal, torque, and the goal that stops it pushing; nothing else.
    let goal = |value: u16| value.to_le_bytes().to_vec();
    assert_eq!(
        lamp.registers,
        [(4, 42, goal(2_044)), (4, 40, vec![1]), (4, 42, goal(2_074))]
    );
    assert_eq!(lamp.servos[ROLL].goal, 2_074);
}

/// The shoulder and the elbow are taken hold of only on a lamp that may
/// stand; on any other they are never written.
#[test]
fn the_arm_is_taken_hold_of_only_on_a_lamp_that_may_stand() {
    let holding = Limits {
        torque_on: true,
        ..Limits::FOLDED
    };
    let without_torque = || {
        let (mut lamp, clock) = folded(Duration::from_secs(1));
        lamp.may_take_hold = true;
        lamp.arm_moves = true;
        lamp.overload_clears = 1;
        lamp.servos[1].torque = 0;
        lamp.servos[1].goal = 2_070;
        lamp.servos[2].torque = 0;
        lamp.servos[2].goal = 2_022;
        (lamp, clock)
    };

    let (mut lamp, mut clock) = without_torque();
    let report = run_standing(
        &mut lamp,
        &mut clock,
        holding,
        Some(HOME),
        Some(standing(100, 5)),
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.standing.is_some(), "{report:?}");
    assert_eq!(report.torque_given, vec!["base_pitch", "elbow_pitch"]);
    let goal = |value: u16| value.to_le_bytes().to_vec();
    assert_eq!(
        lamp.registers[..4],
        [
            (2, 42, goal(2_050)),
            (2, 40, vec![1]),
            (3, 42, goal(2_047)),
            (3, 40, vec![1]),
        ]
    );
    assert_eq!(
        (lamp.servos[1].position, lamp.servos[2].position),
        (2_050.0, 2_047.0)
    );

    let (mut lamp, mut clock) = without_torque();
    let report = run(
        &mut lamp,
        &mut clock,
        holding,
        None,
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.torque_writes, 0);
    assert!(lamp.registers.iter().all(|(id, _, _)| *id != 2 && *id != 3));
}

/// The folded limits with waking on.
fn waking() -> Limits {
    Limits {
        wake: true,
        ..Limits::FOLDED
    }
}

/// The goals a joint was written, every write in order.
fn goals_of(lamp: &Lamp, joint: Joint) -> Vec<u16> {
    lamp.goals
        .iter()
        .map(|write| goal_of(write, joint))
        .collect()
}

#[test]
fn a_head_found_pressed_on_the_base_rises_out_of_it_slowly_and_then_runs() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(12));
    lamp.servos[PITCH].position = 2_095.0;
    lamp.servos[PITCH].goal = 2_095;
    let mut first = None;
    let report = run(
        &mut lamp,
        &mut clock,
        waking(),
        Some(HOME),
        || Some((Mood::Resting, 0)),
        |glimpse| {
            first.get_or_insert(glimpse.positions);
        },
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let woke = report.woke.unwrap();
    assert_eq!(woke.joints, vec!["wrist_pitch"]);
    assert_eq!((woke.from, woke.to), (vec![2_095], vec![2_000]));
    // A stir of 24 counts, a second's pause and the rest of the 95, each
    // eased at 20 counts a second at most: a little over eight seconds.
    assert!((8.0..9.0).contains(&woke.seconds), "{}", woke.seconds);
    // It ran once it was up, inside its window.
    let first = first.unwrap();
    assert!((1_760..=2_020).contains(&first[0]), "{first:?}");
    // Never toward the base, and never more than a count a write.
    let pitch = goals_of(&lamp, Joint::WristPitch);
    assert_eq!(pitch[0], 2_094);
    let risen = pitch.iter().position(|goal| *goal == 2_000).unwrap();
    assert!(
        pitch[..=risen]
            .windows(2)
            .all(|two| two[1] <= two[0] && two[0] - two[1] <= 1)
    );
    assert!(
        pitch[risen..].iter().all(|goal| *goal <= 2_030),
        "{pitch:?}"
    );

    // Without waking, the same lamp is refused as it always was.
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    lamp.servos[PITCH].position = 2_095.0;
    let report = run(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        || None,
        |_| {},
    );
    assert_eq!(
        report.stop,
        Stop::Refused {
            why: "head pitch 2095 is outside 1760..=2020".into()
        }
    );
    assert!(lamp.nothing_was_written() && report.woke.is_none());

    // Waking needs a home it was given: without one, where it is found is home.
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    lamp.servos[PITCH].position = 2_095.0;
    let report = run(&mut lamp, &mut clock, waking(), None, || None, |_| {});
    assert!(matches!(report.stop, Stop::Refused { .. }), "{report:?}");
    assert!(lamp.nothing_was_written());
}

#[test]
fn a_base_turned_300_counts_comes_home_without_moving_away_or_faster_than_waking() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(28));
    lamp.servos[YAW].position = 2_315.0;
    lamp.servos[YAW].goal = 2_315;
    let report = run(
        &mut lamp,
        &mut clock,
        waking(),
        Some(HOME),
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let woke = report.woke.unwrap();
    assert_eq!(woke.joints, vec!["base_yaw"]);
    // 1.8 s of stir, a second, then 276 counts eased at 20 a second at most.
    assert!((23.0..24.5).contains(&woke.seconds), "{}", woke.seconds);
    let yaw = goals_of(&lamp, Joint::BaseYaw);
    assert!(
        yaw.windows(2)
            .all(|two| two[1] <= two[0] && two[0] - two[1] <= 1)
    );
    assert!(yaw.iter().all(|goal| (2_015..2_315).contains(goal)));
    assert_eq!(lamp.servos[YAW].goal, 2_015);
    // The others were never asked to move.
    assert!(
        goals_of(&lamp, Joint::WristRoll)
            .iter()
            .all(|goal| *goal == 2_044)
    );

    // The way itself, step by step: never away from home and never over
    // the wake's speed.
    let (from, to) = ([2_000, 2_044, 2_315], [2_000, 2_044, 2_015]);
    let mut was = wake_pose(from, to, 0.0).0;
    assert_eq!(was, [2_000.0, 2_044.0, 2_315.0]);
    let mut elapsed = 0.0f32;
    loop {
        elapsed += TICK.as_secs_f32();
        let (now, over) = wake_pose(from, to, elapsed);
        let moved = was[2] - now[2];
        assert!((0.0..=WAKE_SPEED * TICK.as_secs_f32() + 0.001).contains(&moved));
        assert_eq!(&now[..2], &[2_000.0, 2_044.0]);
        was = now;
        if over {
            break;
        }
    }
    assert_eq!(was[2], 2_015.0);
}

#[test]
fn a_joint_beyond_what_waking_brings_back_is_refused() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    lamp.servos[YAW].position = 2_465.0;
    lamp.servos[YAW].goal = 2_465;
    let report = run(&mut lamp, &mut clock, waking(), Some(HOME), || None, |_| {});
    assert_eq!(
        report.stop,
        Stop::Refused {
            why: "base_yaw is 450 counts from its home, more than waking brings back (400)".into()
        }
    );
    assert!(lamp.nothing_was_written());
}

#[test]
fn an_arm_found_off_its_way_is_not_lowered_and_the_lamp_runs_folded() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    lamp.servos[1].position = 2_650.0;
    lamp.servos[1].goal = 2_650;
    let report = run_standing(
        &mut lamp,
        &mut clock,
        waking(),
        Some(HOME),
        Some(standing(100, 60)),
        || Some((Mood::Listening, 120)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.standing, None);
    assert!(
        report
            .standing_refused
            .unwrap()
            .ends_with("waking does not lower an arm found off its way")
    );
    // Not a goal to the arm, and nothing to wake.
    assert!(lamp.goals.iter().all(|write| write.len() == 3));
    assert!(report.woke.is_none());
}

#[test]
fn waking_on_a_lamp_found_as_expected_writes_exactly_what_it_did_without() {
    let writes = |limits: Limits| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(8));
        let news = script(&clock.time);
        let report = run(&mut lamp, &mut clock, limits, Some(HOME), news, |_| {});
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        assert!(report.woke.is_none());
        (lamp.goals, lamp.registers)
    };
    assert_eq!(writes(waking()), writes(Limits::FOLDED));
}

/// The folded limits with room to lift the head 100 counts, and a choice of
/// how it gives way.
fn giving_way(give_way: GiveWay) -> Limits {
    Limits {
        give_way: Some(give_way),
        ..Limits::folded_with(Tuning {
            reach: [100, 15, 30, 40],
            speed: 50,
            servo_speed: 150,
            servo_acceleration: 50,
        })
        .unwrap()
    }
}

/// Resting for six seconds, then listening.
fn rest_then_listen(time: &Rc<Cell<Duration>>) -> impl FnMut() -> Option<(Mood, u8)> + use<> {
    let time = time.clone();
    move || {
        Some(if time.get() < Duration::from_secs(6) {
            (Mood::Resting, 0)
        } else {
            (Mood::Listening, 100)
        })
    }
}

/// The time of a step, from its number.
fn clock_of(glimpse: &Glimpse) -> Duration {
    TICK * glimpse.step as u32
}

/// Told to stay, a bump that does not move the head, and a hand that moves
/// it for under a second, each give way and come back as without it.
#[test]
fn told_to_stay_a_short_push_still_gives_way_and_comes_back() {
    let ms = Duration::from_millis;
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    lamp.strains = vec![(ms(1_200), ms(1_400), ROLL, 217)];
    let news = script(&clock.time);
    let mut seen = Vec::new();
    let report = run(
        &mut lamp,
        &mut clock,
        Limits {
            give_way: Some(GiveWay::Stay),
            ..Limits::FOLDED
        },
        None,
        news,
        |glimpse| seen.push(*glimpse),
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.eased, report.kept_where_put), ([0, 1, 0], [0; 3]));
    let gave = seen.iter().position(|g| g.eased[1] == 1).unwrap();
    let held = seen[gave].goals[1];
    assert!(
        seen[gave + usize::from(EASE_STEPS)..]
            .iter()
            .any(|g| g.goals[1].abs_diff(held) >= 10),
        "it came back"
    );

    // A hand lifts the head 40 counts for half a second, while it listens.
    let (mut lamp, mut clock) = folded(Duration::from_secs(12));
    lamp.hand = Some((ms(7_200), ms(7_700), PITCH, 1_920.0));
    lamp.strains = vec![(ms(7_200), ms(7_700), PITCH, 300)];
    let news = rest_then_listen(&clock.time);
    let mut seen = Vec::new();
    let report = run(
        &mut lamp,
        &mut clock,
        giving_way(GiveWay::Stay),
        None,
        news,
        |glimpse| seen.push(*glimpse),
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.eased, report.kept_where_put), ([1, 0, 0], [0; 3]));
    // Listening lifts it 40 counts; it is back near there once the hold is
    // over, and not where the hand had it, 40 counts further.
    let back = seen
        .iter()
        .rev()
        .find(|g| clock_of(g) < ms(11_900))
        .unwrap();
    assert!(back.goals[0].abs_diff(1_960) <= 12, "{back:?}");
    assert!(back.positions[0].abs_diff(1_960) <= 12, "{back:?}");
}

/// Told to stay, a hand that moves the head for a second and a half leaves
/// it there, and the next mood is laid from where it was put.
#[test]
fn told_to_stay_a_head_moved_by_hand_stays_and_the_next_mood_is_relative_to_it() {
    let ms = Duration::from_millis;
    let (mut lamp, mut clock) = folded(Duration::from_secs(10));
    // Lifted 30 counts, held there, let go.
    lamp.hand = Some((ms(1_500), ms(3_000), PITCH, 1_970.0));
    lamp.strains = vec![(ms(1_500), ms(3_000), PITCH, 300)];
    let news = rest_then_listen(&clock.time);
    let mut seen = Vec::new();
    let report = run(
        &mut lamp,
        &mut clock,
        giving_way(GiveWay::Stay),
        None,
        news,
        |glimpse| seen.push(*glimpse),
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.eased, [1, 0, 0]);
    assert_eq!(report.kept_where_put, [1, 0, 0]);
    // Let go, it stays where it was put while it rests.
    let resting: Vec<_> = seen
        .iter()
        .filter(|g| (ms(3_500)..ms(6_000)).contains(&clock_of(g)))
        .collect();
    assert!(
        resting.iter().all(|g| g.goals[0].abs_diff(1_970) <= 1),
        "{:?}",
        resting.iter().map(|g| g.goals[0]).collect::<Vec<_>>()
    );
    // Listening lifts it 40 counts from there, not from home, and on the way
    // no goal jumps.
    let listening = seen
        .iter()
        .rev()
        .find(|g| (ms(8_500)..ms(10_000)).contains(&clock_of(g)))
        .unwrap();
    assert!(listening.goals[0].abs_diff(1_930) <= 2, "{listening:?}");
    let after: Vec<_> = seen.iter().filter(|g| clock_of(g) > ms(3_500)).collect();
    assert!(
        after
            .windows(2)
            .all(|pair| pair[1].goals[0].abs_diff(pair[0].goals[0]) <= 2),
        "it jumped"
    );
    // Asked to stop, it goes home, not to where it was put.
    assert_eq!(lamp.servos[PITCH].goal, HOME[0]);
}

/// A light head: a hand that lifts it 40 counts reads only 120, under the
/// head's load bound of 240, as lamp-52e6's did. Told to stay, it stays
/// where it was put; told to come back, it comes back.
#[test]
fn a_light_head_lifted_by_hand_stays_when_told_to_and_comes_back_when_not() {
    let ms = Duration::from_millis;
    let ran = |give_way: GiveWay| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(8));
        // It reaches 1,920 in under half a second and is held there for
        // a second and a half.
        lamp.hand = Some((ms(1_500), ms(3_500), PITCH, 1_920.0));
        lamp.strains = vec![(ms(1_500), ms(3_500), PITCH, 120)];
        let news = || Some((Mood::Listening, 100));
        let mut seen = Vec::new();
        let report = run(
            &mut lamp,
            &mut clock,
            giving_way(give_way),
            None,
            news,
            |glimpse| seen.push(*glimpse),
        );
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        let after = *seen.iter().rev().find(|g| clock_of(g) < ms(7_900)).unwrap();
        (report, after)
    };
    let (report, after) = ran(GiveWay::Stay);
    assert_eq!(report.kept_where_put, [1, 0, 0], "{report:?}");
    assert!(after.goals[0].abs_diff(1_920) <= 6, "{after:?}");
    // Listening holds it at 1,960; the same hand, told to come back.
    let (report, after) = ran(GiveWay::Return);
    assert_eq!(report.kept_where_put, [0; 3], "{report:?}");
    assert!(after.goals[0].abs_diff(1_960) <= 6, "{after:?}");
}

/// lamp-52e6, 15 Oct, run 4: a hand lifts the head 60 counts; it gives way
/// as the push begins (that read 192 there), and the hand then holds it for
/// three seconds at a load of only 95, under every load bound, the head kept
/// 30 to 40 counts off the goal it is held at. Told to stay it stays
/// where it was put; told to come back it comes back; without the option
/// the same hand ends the run, as before.
#[test]
fn a_head_held_off_its_goal_by_a_hand_at_a_low_load_is_in_the_hand() {
    let ms = Duration::from_millis;
    let ran = |give_way: Option<GiveWay>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(10));
        lamp.hand = Some((ms(1_500), ms(4_500), PITCH, 1_900.0));
        lamp.strains = vec![
            (ms(1_500), ms(2_100), PITCH, 150),
            (ms(2_100), ms(4_500), PITCH, 95),
        ];
        let news = || Some((Mood::Listening, 100));
        let mut seen = Vec::new();
        let limits = Limits {
            give_way,
            ..giving_way(GiveWay::Return)
        };
        let report = run(&mut lamp, &mut clock, limits, None, news, |glimpse| {
            seen.push(*glimpse)
        });
        let after = *seen.iter().rev().find(|g| clock_of(g) < ms(9_900)).unwrap();
        (report, after)
    };
    let (report, after) = ran(Some(GiveWay::Stay));
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.kept_where_put[0] >= 1, "{report:?}");
    assert!(after.goals[0].abs_diff(1_900) <= 8, "{after:?}");
    // Listening holds it at 1,960.
    let (report, after) = ran(Some(GiveWay::Return));
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.kept_where_put, [0; 3], "{report:?}");
    assert!(after.goals[0].abs_diff(1_960) <= 6, "{after:?}");
    let (report, _) = ran(None);
    assert!(
        matches!(report.stop, Stop::Tracking { joint: 5, .. }),
        "{report:?}"
    );
}

/// A hand that takes the head past its reach leaves it at the edge of it.
#[test]
fn told_to_stay_a_head_pushed_past_its_reach_is_kept_at_the_edge() {
    let ms = Duration::from_millis;
    let (mut lamp, mut clock) = folded(Duration::from_secs(8));
    // 150 counts up where the reach is 100.
    lamp.hand = Some((ms(1_500), ms(3_000), PITCH, 1_850.0));
    lamp.strains = vec![(ms(1_500), ms(3_000), PITCH, 300)];
    let news = rest_then_listen(&clock.time);
    let mut seen = Vec::new();
    let report = run(
        &mut lamp,
        &mut clock,
        giving_way(GiveWay::Stay),
        None,
        news,
        |glimpse| seen.push(*glimpse),
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.kept_where_put[0], 1, "{report:?}");
    assert!(
        lamp.goals
            .iter()
            .all(|write| goal_of(write, Joint::WristPitch) >= 1_900)
    );
    let rested = seen.iter().find(|g| clock_of(g) >= ms(5_500)).unwrap();
    assert_eq!(rested.goals[0], 1_900, "{rested:?}");
    assert!(rested.positions[0].abs_diff(1_900) <= 1, "{rested:?}");
}

/// Told to come back, a hand that tilts the head and holds it there for
/// three seconds, past where it first gave way, does not end the run: it
/// gives way again where the hand has it, and comes back once let go. As on
/// lamp-52e6 on 2026-10-15, where the run ended on tracking 45 counts on.
#[test]
fn told_to_return_a_head_tilted_and_held_by_hand_gives_way_again_and_comes_back() {
    let ms = Duration::from_millis;
    let ran = |give_way: Option<GiveWay>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(10));
        lamp.hand = Some((ms(1_500), ms(4_500), ROLL, f32::from(HOME[1]) + 60.0));
        lamp.strains = vec![(ms(1_500), ms(4_500), ROLL, 240)];
        // Awake: a lamp at rest lies where it is whatever the option.
        let news = || Some((Mood::Listening, 100));
        let mut seen = Vec::new();
        let limits = Limits {
            give_way,
            ..giving_way(GiveWay::Return)
        };
        let report = run(&mut lamp, &mut clock, limits, Some(HOME), news, |glimpse| {
            seen.push(*glimpse)
        });
        (report, seen)
    };
    let (report, seen) = ran(Some(GiveWay::Return));
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.eased[1] >= 1, "{report:?}");
    assert_eq!(report.kept_where_put, [0; 3]);
    // Let go, it is back where it was before the hand, within what counts
    // as arrived (it gave way a few counts into the push), and not where
    // the hand had it, 35 counts on.
    let before = seen.iter().rev().find(|g| clock_of(g) < ms(1_500)).unwrap();
    let back = seen.iter().rev().find(|g| clock_of(g) < ms(9_900)).unwrap();
    assert!(back.goals[1].abs_diff(before.goals[1]) <= 6, "{back:?}");
    assert!(
        back.positions[1].abs_diff(before.positions[1]) <= 6,
        "{back:?}"
    );
    // Without the option it goes as before: the same hand ends the run.
    let (report, _) = ran(None);
    assert_ne!(report.stop, Stop::Cancelled, "{report:?}");
}

/// Told to come back, a base turned by a hand gives way though it reads
/// under the load that ends a run (lamp-52e6's read 120 turned 38 counts),
/// and comes back once let go; without the option the run ends on
/// tracking, as before.
#[test]
fn told_to_return_a_base_turned_by_hand_under_its_load_gives_way_and_comes_back() {
    let ms = Duration::from_millis;
    let ran = |give_way: Option<GiveWay>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(10));
        lamp.hand = Some((ms(1_500), ms(4_500), YAW, f32::from(HOME[2]) + 40.0));
        lamp.strains = vec![(ms(1_500), ms(4_500), YAW, 120)];
        // Awake: a lamp at rest lies where it is whatever the option.
        let news = || Some((Mood::Listening, 100));
        let mut seen = Vec::new();
        let limits = Limits {
            give_way,
            ..giving_way(GiveWay::Return)
        };
        let report = run(&mut lamp, &mut clock, limits, Some(HOME), news, |glimpse| {
            seen.push(*glimpse)
        });
        (report, seen)
    };
    let (report, seen) = ran(Some(GiveWay::Return));
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.eased[2] >= 1, "{report:?}");
    let before = seen.iter().rev().find(|g| clock_of(g) < ms(1_500)).unwrap();
    let back = seen.iter().rev().find(|g| clock_of(g) < ms(9_900)).unwrap();
    assert!(back.goals[2].abs_diff(before.goals[2]) <= 3, "{back:?}");
    assert!(
        back.positions[2].abs_diff(before.positions[2]) <= 6,
        "{back:?}"
    );
    let (report, _) = ran(None);
    assert!(
        matches!(report.stop, Stop::Tracking { joint: 1, .. }),
        "{report:?}"
    );
}

/// A head that pushes at a goal it does not reach, without moving, is told
/// to stop after a second and a half; without the option it goes on.
#[test]
fn a_joint_pushing_without_moving_is_told_to_stop_after_a_second_and_a_half() {
    let ran = |give_way: Option<GiveWay>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(6));
        // It stops 27 counts short of any goal, pushing at 130: past where
        // a joint that has arrived is let rest, short of a tracking stop.
        lamp.short = Some((PITCH, 27.0));
        lamp.strains = vec![(Duration::ZERO, Duration::from_secs(60), PITCH, 130)];
        let news = || Some((Mood::Listening, 100));
        let mut seen = Vec::new();
        let limits = Limits {
            give_way,
            ..giving_way(GiveWay::Return)
        };
        let report = run(&mut lamp, &mut clock, limits, None, news, |glimpse| {
            seen.push(*glimpse)
        });
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        (report, seen)
    };
    let (report, seen) = ran(Some(GiveWay::Return));
    assert_eq!(report.stalls, [1, 0, 0, 0, 0]);
    // Pushing without moving from the step it came within six counts of
    // where it stops...
    let began = seen
        .iter()
        .position(|g| g.positions[0] <= 1_987 + STALL_SLACK)
        .unwrap();
    // ...to the step its goal became where it is.
    let stopped = seen
        .iter()
        .position(|g| g.goals[0] == g.positions[0] && g.stalls[0] == 1)
        .unwrap();
    let steps = stopped - began;
    assert!((75..=78).contains(&steps), "{steps}");
    // And it is not pushed at again while it listens.
    assert!(
        seen[stopped..]
            .iter()
            .filter(|g| clock_of(g) < Duration::from_secs(6))
            .all(|g| g.goals[0] == 1_987)
    );
    // Without the option it pushes for as long as it listens.
    let (report, seen) = ran(None);
    assert_eq!(report.stalls, [0; 5]);
    assert!(
        seen.iter()
            .filter(|g| (Duration::from_secs(3)..Duration::from_secs(6)).contains(&clock_of(g)))
            .all(|g| g.goals[0] == 1_960)
    );
}

/// A standing shoulder that rests short of its goal and pushes is told to
/// stop: its goal becomes where it is, and torque is never written off.
#[test]
fn a_standing_shoulder_pushing_short_of_its_goal_is_told_to_stop_and_keeps_its_torque() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(12));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    // As lamp-52e6's did: 30 counts short of where it stands, at 150.
    lamp.short = Some((1, 30.0));
    lamp.strains = vec![(Duration::ZERO, Duration::from_secs(60), 1, 150)];
    let news = || Some((Mood::Listening, 100));
    let mut seen = Vec::new();
    let report = run_standing(
        &mut lamp,
        &mut clock,
        giving_way(GiveWay::Stay),
        Some(HOME),
        Some(standing(100, 60)),
        news,
        |glimpse| seen.push(*glimpse),
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.stood && report.retreats.is_empty(), "{report:?}");
    assert_eq!(report.stalls, [0, 0, 0, 1, 0]);
    let held = seen.iter().find(|g| g.stalls[3] == 1).unwrap();
    assert_eq!(held.arm_goals[0], 2_620, "{held:?}");
    // Torque was written once, on, to clear the elbow's old flag; never off.
    assert!(
        lamp.registers
            .iter()
            .filter(|(_, address, _)| *address == 40)
            .all(|(_, _, data)| data == &vec![1]),
        "{:?}",
        lamp.registers
    );
    assert!(lamp.servos.iter().all(|servo| servo.torque == 1));
}

/// A folded arm lifted by a hand and held is told to stop pushing, and is
/// held there only for a while: then it goes back down to where it rested,
/// so that the next start does not find it off its way.
#[test]
fn a_folded_elbow_lifted_and_held_by_a_hand_goes_back_down_once_let_go() {
    let ms = Duration::from_millis;
    let ran = |give_way: Option<GiveWay>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(14));
        lamp.arm_moves = true;
        lamp.overload_clears = 1;
        let rest = lamp.servos[2].position;
        // Lifted 200 counts, past what a standing arm may be off its goal,
        // held there for three seconds, let go.
        lamp.hand = Some((ms(3_000), ms(7_000), 2, rest + 200.0));
        lamp.strains = vec![(ms(3_000), ms(7_000), 2, 200)];
        let news = || Some((Mood::Resting, 0));
        let mut seen = Vec::new();
        let limits = Limits {
            give_way,
            ..giving_way(GiveWay::Return)
        };
        let report = run_standing(
            &mut lamp,
            &mut clock,
            limits,
            Some(HOME),
            Some(standing(100, 60)),
            news,
            |glimpse| seen.push(*glimpse),
        );
        (report, seen, rest)
    };
    let (report, seen, rest) = ran(Some(GiveWay::Return));
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(!report.stood, "{report:?}");
    assert!(report.stalls[4] >= 1, "{report:?}");
    let end = seen.last().unwrap();
    assert!(
        (f32::from(end.arm_goals[1]) - rest).abs() <= 2.0
            && (f32::from(end.arm_positions[1]) - rest).abs() <= 6.0,
        "{end:?}"
    );
    // Without the option, as before: no stall rule, and the elbow's goal
    // never leaves where it rested.
    let (report, seen, rest) = ran(None);
    assert_eq!(report.stalls, [0; 5], "{report:?}");
    assert!(
        seen.iter()
            .all(|g| (f32::from(g.arm_goals[1]) - rest).abs() <= 2.0),
        "{report:?}"
    );
}

/// lamp-52e6 after its power was cut and came back on 2026-10-14: every
/// servo with torque off and a goal of 0, the head drooped from 1,771 to
/// 2,034, the base and the roll moved. Its unit's options, with `--wake on`.
#[test]
fn a_head_that_drooped_in_a_power_cut_is_lifted_home_with_lamp_52e6s_options() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(40));
    lamp.may_take_hold = true;
    lamp.arm_moves = true;
    for (index, at) in [
        (YAW, 1_958.0),
        (1, 2_045.0),
        (2, 2_083.0),
        (ROLL, 2_100.0),
        (PITCH, 2_034.0),
    ] {
        let servo = &mut lamp.servos[index];
        (servo.position, servo.goal, servo.torque, servo.status) = (at, 0, 0, 0);
    }
    // Lifting from the droop it lags its goal by 37 counts at a load of
    // 280, and does not move for a goal less far ahead.
    lamp.heavy = Some((PITCH, 1_850.0, 37.0, 320));
    let limits = Limits {
        force: Some(600),
        torque_on: true,
        wake: true,
        ..Limits::folded_with(Tuning {
            reach: [150, 15, 1, 250],
            speed: 120,
            servo_speed: 1_000,
            servo_acceleration: 60,
        })
        .unwrap()
    };
    let up = Standing::new(
        [2_053, 2_069],
        [1_772, 2_035, 2_035, 2_657, 2_602],
        100,
        5,
        90,
        [560, 560],
    )
    .unwrap();
    let report = run_standing(
        &mut lamp,
        &mut clock,
        limits,
        Some([1_771, 2_035, 2_035]),
        Some(up),
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.standing.is_some(), "{report:?}");
    let woke = report.woke.unwrap();
    // All three come home with the head, at its pace.
    assert_eq!(woke.joints, vec!["wrist_pitch", "wrist_roll", "base_yaw"]);
    assert_eq!(
        (woke.from, woke.to),
        (vec![2_034, 2_100, 1_958], vec![1_771, 2_035, 2_035])
    );
    // A stir that reaches 80 counts ahead, a pause, then the rest of 263.
    assert!((19.0..23.0).contains(&woke.seconds), "{}", woke.seconds);
    // Lifting, it read 320 and lagged its goal by 37, and was let.
    assert!(report.peak_load[0] >= 320, "{:?}", report.peak_load);
    // Never toward the base.
    let pitch = goals_of(&lamp, Joint::WristPitch);
    let home = pitch.iter().position(|goal| *goal == 1_771).unwrap();
    assert!(
        pitch[..=home].windows(2).all(|two| two[1] <= two[0]),
        "{pitch:?}"
    );
    assert!(pitch.iter().all(|goal| *goal <= 2_034));
    assert!((lamp.servos[PITCH].position - 1_771.0).abs() <= 30.0);
}

#[test]
fn a_head_that_cannot_lift_ends_the_wake_held_where_it_is_and_says_how_far_it_got() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(40));
    lamp.servos[PITCH].position = 2_095.0;
    lamp.servos[PITCH].goal = 2_095;
    // It never moves for a goal less than 150 counts ahead: its goal gets
    // home, it stays still for half a second, and the run ends.
    lamp.heavy = Some((PITCH, 1_850.0, 150.0, 200));
    let report = run(
        &mut lamp,
        &mut clock,
        waking(),
        Some(HOME),
        || Some((Mood::Resting, 0)),
        |_| panic!("a lamp that did not wake does not run"),
    );
    assert!(
        report.stop
            == Stop::Tracking {
                joint: 5,
                goal: 2_000,
                position: 2_095
            },
        "{:?}",
        report.stop
    );
    let woke = report.woke.unwrap();
    assert_eq!(woke.from, vec![2_095]);
    assert_eq!(woke.to, vec![2_000]);
    assert_eq!(report.held_at.map(|held| held[0]), Some(2_095));
}

/// lamp-52e6 woken from a real droop on 2026-10-14: the head lifted and
/// stayed 32 counts short of home. It has woken there, and runs from there.
#[test]
fn a_head_that_settles_32_short_of_home_wakes_there_and_runs_from_where_it_is() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(40));
    lamp.may_take_hold = true;
    for (index, at) in [(YAW, 1_979.0), (ROLL, 2_101.0), (PITCH, 2_029.0)] {
        let servo = &mut lamp.servos[index];
        (servo.position, servo.goal, servo.torque) = (at, 0, 0);
    }
    lamp.servos[2].status = 0;
    // Lifting, it always stops 32 short of its goal.
    lamp.heavy = Some((PITCH, 0.0, 32.0, 232));
    let limits = Limits {
        torque_on: true,
        wake: true,
        ..Limits::folded_with(Tuning {
            reach: [150, 15, 1, 250],
            speed: 120,
            servo_speed: 1_000,
            servo_acceleration: 60,
        })
        .unwrap()
    };
    let report = run(
        &mut lamp,
        &mut clock,
        limits,
        Some([1_771, 2_035, 2_035]),
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let woke = report.woke.unwrap();
    assert_eq!(woke.to, vec![1_771, 2_035, 2_035]);
    assert_eq!(woke.settled, vec![1_803, 2_035, 2_035]);
    // Its home is where it settled; nothing pushes at the gap.
    assert_eq!(report.home, Some([1_803, 2_035, 2_035]));
    let pitch = goals_of(&lamp, Joint::WristPitch);
    let settled = pitch.iter().position(|goal| *goal == 1_771).unwrap() + 1;
    assert_eq!(pitch[settled], 1_803);
    assert!(
        pitch[settled..].iter().all(|goal| *goal >= 1_803),
        "{pitch:?}"
    );
    assert_eq!(lamp.servos[PITCH].goal, 1_803);
    assert_eq!(lamp.servos[PITCH].position, 1_803.0);
}

/// The folded limits, letting go under overload.
fn releasing() -> Limits {
    Limits {
        overload_release: true,
        ..Limits::FOLDED
    }
}

/// A joint at 800 for a second and a half: torque off to all five, byte
/// for byte; nothing written for two minutes, whatever it is told; then
/// each joint held where it slumped, goal first; a head that drooped 250
/// counts lifted as a lamp wakes from a power cut, its goal leading, and
/// woken where it settles 32 short; the arm left where it slumped.
/// Overloaded again within ten minutes, it lets go and the run ends there.
#[test]
fn a_joint_at_800_for_a_second_and_a_half_lets_go_of_all_five_and_is_taken_up_again_slowly() {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(hex(&torque_write(Joint::BaseYaw, 0)), "ffff0104032800cf");
    let secs = Duration::from_secs_f32;
    let (mut lamp, mut clock) = folded(Duration::from_secs(200));
    lamp.may_take_hold = true;
    // The elbow at 800 for 1.6 s, and again once the lamp is up again.
    lamp.strains = vec![
        (secs(1.0), secs(2.6), 2, 800),
        (secs(150.0), secs(151.6), 2, 800),
    ];
    // Let go, the head droops 250 counts and the elbow falls 40. This head,
    // as lamp-52e6's does, stops 32 counts short of a goal it lifts to.
    lamp.hand = Some((secs(5.0), secs(8.0), PITCH, 2_250.0));
    lamp.short = Some((PITCH, 32.0));
    lamp.event = Some((secs(9.0), 2, Event::Push(-40.0)));
    // Spoken to while it rests: nothing is done for it.
    let time = clock.time.clone();
    let news = move || {
        Some(if (secs(30.0)..secs(60.0)).contains(&time.get()) {
            (Mood::Listening, 120)
        } else {
            (Mood::Resting, 0)
        })
    };
    let mut limp = Vec::new();
    let report = run(&mut lamp, &mut clock, releasing(), Some(HOME), news, |g| {
        if g.limp {
            limp.push(*g);
        }
    });
    assert_eq!(
        report.stop,
        Stop::Overload {
            joint: 3,
            load: 800,
            detail: "let go a second time within ten minutes; torque left off".into()
        },
        "{report:?}"
    );
    assert_eq!(report.released.len(), 2);
    let first = &report.released[0];
    assert_eq!((first.joint, first.load), ("elbow_pitch", 800));
    // 75 steps of 20 ms after the load began at 1 s.
    assert!((2.45..2.6).contains(&first.seconds), "{}", first.seconds);
    let tripped = secs(first.seconds);

    // Torque off to the five, in servo order, before anything else.
    let off: Vec<(u8, u8, Vec<u8>)> = Joint::ALL.iter().map(|j| (j.id(), 40, vec![0])).collect();
    let at = lamp
        .registers
        .iter()
        .position(|(_, address, data)| *address == 40 && data == &vec![0])
        .unwrap();
    assert_eq!(lamp.registers[at..at + 5], off[..]);
    assert!(lamp.register_times[..at].iter().all(|t| *t < tripped));
    // Nothing written for two minutes: no goal, no register.
    let rest = tripped..tripped + OVERLOAD_REST;
    assert!(lamp.goal_times.iter().all(|t| !rest.contains(t)));
    assert!(
        lamp.register_times[at + 5..]
            .iter()
            .all(|t| *t >= tripped + OVERLOAD_REST)
    );
    // It watched every step of it, at rest whatever it was told.
    assert!(limp.len() >= 6_000, "{}", limp.len());
    assert!(
        limp.iter()
            .filter(|g| g.released == 1)
            .all(|g| g.mood == Mood::Resting && g.goals == HOME)
    );

    // Then each joint held where it slumped: its goal, then its torque.
    let goal = |value: u16| value.to_le_bytes().to_vec();
    assert_eq!(
        lamp.registers[at + 5..at + 15],
        [
            (5, 42, goal(2_250)),
            (5, 40, vec![1]),
            (4, 42, goal(2_044)),
            (4, 40, vec![1]),
            (1, 42, goal(2_015)),
            (1, 40, vec![1]),
            (2, 42, goal(2_050)),
            (2, 40, vec![1]),
            (3, 42, goal(2_007)),
            (3, 40, vec![1]),
        ]
    );
    // And the head lifted as a lamp wakes from a power cut: a count a
    // write at most, never away from home, the goal leading the head; it
    // settled 32 short, and runs from there, its home moved by as much.
    let woke = first.woke.as_ref().unwrap();
    assert_eq!(
        (&woke.joints, &woke.from, &woke.to, &woke.settled),
        (
            &vec!["wrist_pitch"],
            &vec![2_250],
            &vec![2_000],
            &vec![2_032]
        )
    );
    // A stir of 80 counts, a second's pause, the other 170, and settling.
    assert!((19.0..22.5).contains(&woke.seconds), "{}", woke.seconds);
    assert_eq!(report.home, Some([2_032, 2_044, 2_015]));
    let taken = lamp.register_times[at + 14];
    let pitch: Vec<u16> = lamp
        .goals
        .iter()
        .zip(&lamp.goal_times)
        .filter(|(_, t)| (taken..secs(150.0)).contains(*t))
        .map(|(write, _)| goal_of(write, Joint::WristPitch))
        .collect();
    assert_eq!(pitch[0], 2_249);
    let home = pitch.iter().position(|goal| *goal == 2_000).unwrap();
    assert!(
        pitch[..=home]
            .windows(2)
            .all(|two| two[1] <= two[0] && two[0] - two[1] <= 1),
        "{pitch:?}"
    );
    assert_eq!(pitch[home + 1], 2_032);
    // Nothing pushes at the gap: a count of the run's own easing at most.
    assert!(
        pitch[home + 1..].iter().all(|goal| *goal >= 2_031),
        "{pitch:?}"
    );
    assert_eq!(lamp.servos[PITCH].position, 2_032.0);
    // The arm of a lamp that does not stand stays where it slumped.
    assert!(lamp.goals.iter().all(|write| write.len() == 3));
    assert_eq!(lamp.servos[2].position, 2_007.0);

    // The second time: let go, and nothing after it.
    let second = secs(report.released[1].seconds);
    assert!((151.4..151.7).contains(&report.released[1].seconds));
    assert_eq!(lamp.registers[lamp.registers.len() - 5..], off[..]);
    assert!(lamp.goal_times.iter().all(|t| *t < second));
    assert!(lamp.servos.iter().all(|servo| servo.torque == 0));
    serde_json::to_string(&report).unwrap();
}

/// 800 for 1.4 s is not an overload, and with the option a run where none
/// comes writes exactly what it writes without it.
#[test]
fn a_load_of_800_for_less_than_a_second_and_a_half_changes_nothing() {
    let writes = |limits: Limits| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(8));
        lamp.strains = vec![(Duration::from_secs(1), Duration::from_millis(2_400), 2, 800)];
        let news = script(&clock.time);
        let report = run(&mut lamp, &mut clock, limits, None, news, |_| {});
        assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
        assert!(report.released.is_empty());
        (lamp.goals, lamp.registers)
    };
    assert_eq!(writes(releasing()), writes(Limits::FOLDED));
}

/// An elbow held to a force limit of 600 trips at 480, 80% of it, where
/// 800 could never be read; without the limit 500 is not an overload.
/// Asked to stop while it rests, it ends with torque left off.
#[test]
fn a_force_limited_arm_lets_go_at_80_percent_of_its_limit() {
    let ran = |force: Option<u16>| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(5));
        lamp.arm_moves = true;
        lamp.overload_clears = 1;
        lamp.may_take_hold = true;
        lamp.strains = vec![(Duration::from_secs(1), Duration::from_secs(3), 2, 500)];
        let limits = Limits {
            force,
            ..releasing()
        };
        let report = run_standing(
            &mut lamp,
            &mut clock,
            limits,
            Some(HOME),
            Some(standing(100, 5)),
            || Some((Mood::Resting, 0)),
            |_| {},
        );
        (report, lamp)
    };
    let (report, lamp) = ran(Some(600));
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.released.len(), 1);
    let released = &report.released[0];
    assert_eq!((released.joint, released.load), ("elbow_pitch", 500));
    assert!((2.45..2.6).contains(&released.seconds));
    assert!(lamp.servos.iter().all(|servo| servo.torque == 0));
    assert!(
        lamp.goal_times
            .iter()
            .all(|t| *t < Duration::from_secs_f32(released.seconds))
    );

    let (report, lamp) = ran(None);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.released.is_empty());
    assert!(lamp.servos.iter().all(|servo| servo.torque == 1));
}

/// lamp-52e6 unplugged while it stood, on 2026-10-14: the shoulder stayed
/// up at 2,633 on its gears and the elbow fell folded, to 2,074; every
/// servo without torque and with a goal of 0; the head drooped.
#[test]
fn a_shoulder_left_up_by_a_power_cut_is_lowered_first_and_the_lamp_may_then_stand() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(70));
    lamp.may_take_hold = true;
    lamp.arm_moves = true;
    for (index, at) in [
        (YAW, 1_979.0),
        (1, 2_633.0),
        (2, 2_074.0),
        (ROLL, 2_101.0),
        (PITCH, 2_029.0),
    ] {
        let servo = &mut lamp.servos[index];
        (servo.position, servo.goal, servo.torque, servo.status) = (at, 0, 0, 0);
    }
    lamp.heavy = Some((PITCH, 1_850.0, 37.0, 232));
    let limits = Limits {
        force: Some(600),
        torque_on: true,
        wake: true,
        ..Limits::folded_with(Tuning {
            reach: [150, 15, 1, 250],
            speed: 120,
            servo_speed: 1_000,
            servo_acceleration: 60,
        })
        .unwrap()
    };
    let up = Standing::new(
        [2_053, 2_069],
        [1_772, 2_035, 2_035, 2_657, 2_602],
        100,
        5,
        90,
        [560, 560],
    )
    .unwrap();
    // Spoken to once it is awake.
    let time = clock.time.clone();
    let news = move || {
        Some(if time.get() > Duration::from_secs(50) {
            (Mood::Listening, 120)
        } else {
            (Mood::Resting, 0)
        })
    };
    let report = run_standing(
        &mut lamp,
        &mut clock,
        limits,
        Some([1_771, 2_035, 2_035]),
        Some(up),
        news,
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.standing_refused, None, "{report:?}");
    let lowered = report.lowered.clone().unwrap();
    assert_eq!((lowered.from, lowered.to), (vec![2_633], vec![2_053]));
    // 580 counts eased at 40 a second at most: a little under 22 s.
    assert!(
        (21.5..23.0).contains(&lowered.seconds),
        "{}",
        lowered.seconds
    );
    assert!(report.woke.is_some());
    assert!(report.stood, "{report:?}");
    // On the way down the shoulder only ever went down, under a count a
    // write, and the elbow was never asked to move.
    let shoulder = goals_of(&lamp, Joint::BasePitch);
    let down = shoulder.iter().position(|goal| *goal == 2_053).unwrap();
    assert!(
        shoulder[..=down]
            .windows(2)
            .all(|two| two[1] <= two[0] && two[0] - two[1] <= 1),
        "{shoulder:?}"
    );
    assert!(
        goals_of(&lamp, Joint::ElbowPitch)[..=down]
            .iter()
            .all(|goal| *goal == 2_074)
    );

    // Without waking it is refused standing, as before.
    let (mut lamp, mut clock) = folded(Duration::from_secs(2));
    lamp.servos[1].position = 2_633.0;
    lamp.servos[1].goal = 2_633;
    lamp.servos[2].status = 0;
    let report = run_standing(
        &mut lamp,
        &mut clock,
        Limits::FOLDED,
        Some(HOME),
        Some(standing(100, 60)),
        || None,
        |_| {},
    );
    assert!(report.lowered.is_none());
    assert!(report.standing_refused.unwrap().contains("off the way"));
}

/// A head held 35 counts short of its goal by its own weight, at rest:
/// with `--wake on` it rests where it is; without, the run ends as before.
#[test]
fn with_waking_a_head_held_35_short_at_rest_rests_where_it_is() {
    let held = |limits: Limits| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(8));
        lamp.servos[PITCH].position = 2_035.0;
        lamp.servos[PITCH].goal = 2_035;
        lamp.servos[PITCH].load = 120;
        lamp.short = Some((PITCH, 35.0));
        let report = run(
            &mut lamp,
            &mut clock,
            limits,
            Some(HOME),
            || Some((Mood::Resting, 0)),
            |_| {},
        );
        (report, lamp.servos[PITCH].position)
    };
    let (report, _) = held(Limits {
        pitch_home: (1_760, 2_040),
        pitch_window: (1_740, 2_040),
        ..Limits::FOLDED
    });
    assert!(
        matches!(report.stop, Stop::Tracking { joint: 5, .. }),
        "{:?}",
        report.stop
    );
    let (report, at) = held(Limits {
        pitch_home: (1_760, 2_040),
        pitch_window: (1_740, 2_040),
        ..waking()
    });
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.rested_short[0] >= 1, "{report:?}");
    assert!(report.woke.is_none());
    assert_eq!(at, 2_035.0);
}

/// lamp-4ace woken toward a base home 286 counts round on 2026-10-14: the
/// base stuck 34 behind its slow goal. Led and given room, it comes round.
#[test]
fn a_base_that_sticks_34_behind_a_slow_goal_is_led_round_and_settles() {
    let (mut lamp, mut clock) = folded(Duration::from_secs(40));
    lamp.servos[YAW].position = 2_029.0;
    lamp.servos[YAW].goal = 2_029;
    lamp.short = Some((YAW, 34.0));
    let home = [2_000, 2_044, 2_315];
    let report = run(
        &mut lamp,
        &mut clock,
        waking(),
        Some(home),
        || Some((Mood::Resting, 0)),
        |_| {},
    );
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    let woke = report.woke.unwrap();
    assert_eq!((woke.from, woke.to), (vec![2_029], vec![2_315]));
    assert_eq!(woke.settled, vec![2_281]);
    // It runs from where it settled, and nothing pushes at the gap.
    assert_eq!(report.home, Some([2_000, 2_044, 2_281]));
    assert_eq!(lamp.servos[YAW].position, 2_281.0);
    // Its goal never went back on the way round.
    let yaw = goals_of(&lamp, Joint::BaseYaw);
    let round = yaw.iter().position(|goal| *goal == 2_315).unwrap();
    assert!(yaw[..=round].windows(2).all(|two| two[1] >= two[0]));
}

/// lamp-4ace woken the other way: as the base turned, the shoulder swayed
/// 7 counts. That is a sway, not a move; 20 is a move.
#[test]
fn an_arm_that_sways_7_counts_as_the_base_wakes_is_let_and_one_that_moves_20_is_not() {
    let swayed = |by: f32| {
        let (mut lamp, mut clock) = folded(Duration::from_secs(40));
        lamp.servos[YAW].position = 2_029.0;
        lamp.servos[YAW].goal = 2_029;
        lamp.event = Some((Duration::from_secs(10), 1, Event::Push(by)));
        run(
            &mut lamp,
            &mut clock,
            waking(),
            Some([2_000, 2_044, 1_729]),
            || Some((Mood::Resting, 0)),
            |_| {},
        )
    };
    let report = swayed(7.0);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!(report.woke.unwrap().settled, vec![1_729]);
    assert_eq!(
        swayed(20.0).stop,
        Stop::Drift {
            joint: 2,
            found: 2_050,
            now: 2_070
        }
    );
}

/// When, the gesture it makes, how far up it is and its base's goal.
type CalledStep = (Duration, Option<&'static str>, f32, u16);

/// A folded lamp at rest, called by its name at `calls` (seconds), with a
/// face in view from `face_from` on if given, in `mood` throughout; once
/// stood, shifting now and then at rest if `idle`.
fn called_run(
    look_at_name: bool,
    idle: bool,
    mood: Mood,
    calls: &'static [u64],
    face_from: Option<u64>,
) -> (BodyReport, Lamp, Vec<CalledStep>) {
    let (mut lamp, mut clock) = folded(Duration::from_secs(20));
    lamp.arm_moves = true;
    lamp.overload_clears = 1;
    let limits = Limits {
        look_at_name,
        ..Limits::folded_with(Tuning {
            reach: [150, 15, 1, 250],
            speed: 120,
            servo_speed: 1_000,
            servo_acceleration: 60,
        })
        .unwrap()
    };
    let time = clock.time.clone();
    let watched = clock.time.clone();
    let mut answered = 0;
    let mut last = Duration::ZERO;
    let mut seen = Vec::new();
    let report = run_alive(
        &mut lamp,
        &mut clock,
        limits,
        Some(HOME),
        Some(standing(100, 30).idling(idle)),
        Some(Looking::new([342, 200], [250, 120]).unwrap()),
        move || Some((mood, 0)),
        move || {
            let now = time.get();
            let called = calls
                .get(answered)
                .is_some_and(|at| now >= Duration::from_secs(*at));
            answered += usize::from(called);
            // The eyes look six times a second.
            let look = now.saturating_sub(last) >= Duration::from_millis(150);
            if look {
                last = now;
            }
            Asked {
                seen: look.then(|| Seen {
                    face: face_from
                        .is_some_and(|from| now >= Duration::from_secs(from))
                        .then_some(crate::gaze::Sight {
                            x: 0.0,
                            y: 0.0,
                            size: 0.2,
                            turned: 0.0,
                        }),
                }),
                gesture: None,
                expression: None,
                called,
            }
        },
        |glimpse| {
            seen.push((
                watched.get(),
                glimpse.gesture,
                glimpse.stand,
                glimpse.goals[2],
            ))
        },
    );
    (report, lamp, seen)
}

/// Without the switch a call changes nothing: the lamp writes exactly
/// what one that was never called writes.
#[test]
fn without_the_switch_a_called_lamp_writes_exactly_as_one_never_called() {
    let (called, called_lamp, _) = called_run(false, false, Mood::Resting, &[1, 12], None);
    let (quiet, quiet_lamp, _) = called_run(false, false, Mood::Resting, &[], None);
    assert_eq!(called.calls, 0);
    assert_eq!(called_lamp.goals, quiet_lamp.goals);
    assert_eq!(called_lamp.registers, quiet_lamp.registers);
    assert_eq!(called.stop, quiet.stop);
}

/// Called with nobody in view: it rises at once, glances a little to one
/// side and the other once it stands, comes back to where it looked, and
/// a second call within ten seconds is let go.
#[test]
fn called_with_nobody_in_view_it_rises_and_glances_once() {
    let (report, _, seen) = called_run(true, false, Mood::Resting, &[1, 6], None);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert_eq!((report.calls, report.gestures), (1, 1), "{report:?}");
    let rising = seen.iter().find(|step| step.2 > 0.0).unwrap().0;
    assert!(rising < Duration::from_millis(1_100), "{rising:?}");
    let glance: Vec<_> = seen
        .iter()
        .filter(|step| step.1 == Some("called"))
        .collect();
    let (first, last) = (glance[0].0, glance[glance.len() - 1].0);
    assert!(first >= Duration::from_secs(5) && last - first <= Duration::from_millis(1_100));
    let yaw: Vec<i32> = glance
        .iter()
        .map(|step| i32::from(step.3) - 2_015)
        .collect();
    let (left, right) = (*yaw.iter().min().unwrap(), *yaw.iter().max().unwrap());
    assert!(
        (-60..=-30).contains(&left) && (30..=60).contains(&right),
        "{yaw:?}"
    );
    // Back where it looked from by the end of the run.
    assert_eq!(seen.last().unwrap().3, 2_015);
}

/// Called with somebody in view: no glance; the gaze has them. Called
/// while it speaks: nothing.
#[test]
fn called_with_a_face_in_view_it_does_not_glance_and_speaking_it_takes_no_call() {
    let (report, _, seen) = called_run(true, false, Mood::Resting, &[1], Some(0));
    assert_eq!((report.calls, report.gestures), (1, 0), "{report:?}");
    assert!(seen.iter().all(|step| step.1.is_none()));
    let (report, _, _) = called_run(true, false, Mood::Speaking, &[1], None);
    assert_eq!(report.calls, 0);
}

/// A lamp that stands and shifts now and then at rest, called again after
/// ten seconds: the shift gives way to the glance.
#[test]
fn a_call_is_glanced_for_over_a_shift_at_rest() {
    let (report, _, seen) = called_run(true, true, Mood::Resting, &[1, 13], None);
    assert_eq!(report.stop, Stop::Cancelled, "{report:?}");
    assert!(report.idles >= 1, "{report:?}");
    assert_eq!((report.calls, report.gestures), (2, 2), "{report:?}");
    let second = seen
        .iter()
        .find(|step| step.0 >= Duration::from_secs(13) && step.1 == Some("called"))
        .unwrap()
        .0;
    assert!(second < Duration::from_millis(13_300), "{second:?}");
}
