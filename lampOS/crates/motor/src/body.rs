//! A small expressive body: the head's pitch and roll and the base's yaw move
//! a few degrees around where they were found, following what the lamp is
//! doing, and stop at the first surprise.
//!
//! This is not the qualified motion worker. It exists so that a lamp folded
//! in its rest pose can tilt, look away and nod while it talks, within limits
//! narrow enough to be checked by eye. What it may do:
//!
//! - It commands three joints only: head pitch, head roll and base yaw. The
//!   shoulder and the elbow are read on every step and never written.
//! - It writes three registers only: the goal position of those joints, and
//!   once at start their goal speed and acceleration, so that the servo
//!   itself is slow whatever this process sends.
//! - It never touches torque. A joint whose torque is off, or whose status
//!   byte is not clear, is a reason not to start.
//! - Every goal is within a fixed reach of its home (a pose given at start,
//!   or else the pose found), and the head's pitch also within a fixed
//!   absolute window short of the base. A head found outside the reach of a
//!   given home is first brought into it, never moved away from it.
//! - After every step it reads all five joints. A load over its bound, a
//!   joint that does not follow, a status byte that changes, a shoulder or
//!   elbow that moves, a joint that stays hot or a wire that is not exactly
//!   as expected ends the run: the present positions are written as the goals, so that
//!   nothing keeps pushing, and nothing is written after that.
use crate::{
    Joint,
    gesture::{Gestures, Mood, Pose},
    inspection::{Interest, PortIo, Runtime},
    protocol::{ReadOnlyRequest, ReadWindow, StatusParser},
};
use serde::Serialize;
use std::{io, time::Duration};

/// One step: goals are written and all five joints read this often.
pub const TICK: Duration = Duration::from_millis(20);
/// One request and its replies, or the step fails.
const EXCHANGE: Duration = Duration::from_millis(12);
const POLL: Duration = Duration::from_millis(1);
/// A mood older than this is rest: a silent sender is not a reason to hold
/// a pose.
const MOOD_STALE: Duration = Duration::from_millis(1_500);
/// The longest the head takes to ease home when asked to stop.
const LEAVE: Duration = Duration::from_secs(3);
/// How stiffly the commanded pose follows the wanted one, in radians a
/// second: it arrives in well under a second without overshoot.
const FOLLOW: f32 = 7.0;
/// At rest for this many steps, a joint within `REST_SLACK` counts of its
/// goal is given its own position as the goal. A servo stops a few counts
/// short of a goal and goes on pushing at it: on lamp-4ace the yaw servo sat
/// 8 counts short at a load of 44 for as long as it rested.
const REST_STEPS: u16 = 50;
const REST_SLACK: u16 = 24;

/// The joints this body commands, in the order of every array of three here.
pub const COMMANDED: [Joint; 3] = [Joint::WristPitch, Joint::WristRoll, Joint::BaseYaw];
const BYSTANDERS: [Joint; 2] = [Joint::BasePitch, Joint::ElbowPitch];

/// Bounds in encoder counts, about 11.4 to a degree. Fixed in code: nothing
/// outside this process can widen them.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Limits {
    /// The head's pitch must be found inside this window to start at all.
    pub pitch_home: (u16, u16),
    /// And is never commanded outside this one, whatever the reach allows.
    pub pitch_window: (u16, u16),
    /// The farthest a goal may be from the found pose: up, down, roll, yaw.
    pub pitch_up: f32,
    pub pitch_down: f32,
    pub roll: f32,
    pub yaw: f32,
    /// The fastest the commanded pose moves, and how fast that may change.
    pub speed: f32,
    pub acceleration: f32,
    /// The profile written to each commanded servo at start.
    pub servo_speed: u16,
    pub servo_acceleration: u8,
    /// A load above this on `load_steps` steps in a row ends the run.
    pub load: [u16; 3],
    pub load_steps: u8,
    /// A position this far from its goal for `tracking_steps` steps ends it.
    pub tracking: u16,
    pub tracking_steps: u8,
    /// A shoulder or elbow this far from where it was found ends it.
    pub bystander_drift: u16,
    /// A commanded joint this warm, by its own register, on every one of
    /// `temperature_steps` steps in a row ends it. One reading is not heat:
    /// on lamp-4ace the yaw servo read 72 on one step while it moved and 44
    /// to 45 on three reads at rest straight after.
    pub temperature: u8,
    pub temperature_steps: u8,
    /// With a home given at start, the farthest a commanded joint may be
    /// found from it.
    pub start_reach: u16,
}

/// What may be chosen at start, each inside a cap fixed here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tuning {
    /// Reach in counts: pitch up, pitch down, roll, yaw.
    pub reach: [u16; 4],
    /// Counts a second.
    pub speed: u16,
    pub servo_speed: u16,
    pub servo_acceleration: u8,
}

impl Tuning {
    /// The largest reach that may be chosen: about 16 degrees up, 2 down,
    /// 9 of roll and 22 of yaw.
    pub const REACH_CAP: [u16; 4] = [180, 25, 100, 250];
    pub const SPEED_CAP: u16 = 300;
    pub const SERVO_SPEED_CAP: u16 = 1_000;
    pub const SERVO_ACCELERATION_CAP: u8 = 150;
}

impl Limits {
    /// The folded limits with another reach and speed, each within its cap.
    /// Whatever the reach, the head's pitch keeps its absolute window.
    pub fn folded_with(tuning: Tuning) -> Result<Self, &'static str> {
        let within = tuning
            .reach
            .iter()
            .zip(Tuning::REACH_CAP)
            .all(|(reach, cap)| *reach >= 1 && *reach <= cap);
        if !within
            || !(10..=Tuning::SPEED_CAP).contains(&tuning.speed)
            || !(50..=Tuning::SERVO_SPEED_CAP).contains(&tuning.servo_speed)
            || !(10..=Tuning::SERVO_ACCELERATION_CAP).contains(&tuning.servo_acceleration)
        {
            return Err("reach or speed outside what this body may be given");
        }
        Ok(Self {
            pitch_up: f32::from(tuning.reach[0]),
            pitch_down: f32::from(tuning.reach[1]),
            roll: f32::from(tuning.reach[2]),
            yaw: f32::from(tuning.reach[3]),
            speed: f32::from(tuning.speed),
            // Reaches its speed in a little over a tenth of a second.
            acceleration: f32::from(tuning.speed) * 8.0,
            servo_speed: tuning.servo_speed,
            servo_acceleration: tuning.servo_acceleration,
            // A joint may be found anywhere inside the reach of its home.
            start_reach: tuning.reach.into_iter().max().unwrap_or(60).max(60),
            ..Self::FOLDED
        })
    }

    /// For a lamp folded in its rest pose. On lamp-4ace the head crossed
    /// 1766 to 2043 with no load and met the base near 2045 to 2056, so its
    /// goal stays at or under 2030 and it dips little. Speed is what the head
    /// was moved at when it was backed off the base on 2026-10-12: 47 counts
    /// a second at most, which it followed within 13 counts on the profile
    /// written here, reading a load of up to 116 on the way and 68 at rest.
    pub const FOLDED: Self = Self {
        pitch_home: (1_760, 2_020),
        pitch_window: (1_740, 2_030),
        pitch_up: 45.0,
        pitch_down: 15.0,
        roll: 30.0,
        yaw: 40.0,
        speed: 50.0,
        acceleration: 400.0,
        servo_speed: 150,
        servo_acceleration: 50,
        // In its first second on lamp-4ace the body read 104, 96 and 88.
        load: [170, 150, 150],
        load_steps: 3,
        tracking: 30,
        tracking_steps: 10,
        bystander_drift: 6,
        temperature: 62,
        temperature_steps: 50,
        start_reach: 60,
    };
}

/// What one joint said on one step.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Sample {
    pub position: u16,
    pub load: u16,
    pub temperature: u8,
    pub status: u8,
}

/// One step as the body saw it, for whoever wants to watch: the mood it
/// acted on, the goals in force and what the three joints read. Pitch, roll,
/// yaw.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Glimpse {
    pub step: u64,
    pub mood: Mood,
    pub goals: [u16; 3],
    pub positions: [u16; 3],
    pub loads: [u16; 3],
}

/// Why the run ended. Only `Cancelled` is an ordinary end.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stop {
    /// Asked to stop: the head eased back to where it was found.
    Cancelled,
    /// A reason not to start. Nothing was written.
    Refused {
        why: String,
    },
    Load {
        joint: u8,
        load: u16,
    },
    Tracking {
        joint: u8,
        goal: u16,
        position: u16,
    },
    Status {
        joint: u8,
        found: u8,
        now: u8,
    },
    Drift {
        joint: u8,
        found: u16,
        now: u16,
    },
    Temperature {
        joint: u8,
        temperature: u8,
    },
    /// The wire was not exactly as expected, on three steps in a row or
    /// while the servo profile was being written.
    Bus {
        detail: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct BodyReport {
    pub limits: Limits,
    /// Goals were written at least once.
    pub moved: bool,
    /// Pitch, roll, yaw of the pose every gesture is an offset from: the one
    /// given at start, or else the one found.
    pub home: Option<[u16; 3]>,
    pub found: Option<[Sample; 5]>,
    pub steps: u64,
    pub goal_writes: u64,
    pub profile_writes: u8,
    pub missed_reads: u64,
    pub peak_load: [u16; 3],
    pub peak_temperature: [u8; 3],
    /// The goals written to stop pushing, when the run ended on a surprise.
    pub held_at: Option<[u16; 3]>,
    pub last: Option<[Sample; 5]>,
    pub stop: Stop,
}

fn checksum(body: &[u8]) -> u8 {
    !body.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

/// The only single-servo writes there are: acceleration (register 41, one
/// byte) and goal speed (register 46, two bytes).
fn acceleration_write(joint: Joint, value: u8) -> [u8; 8] {
    let mut packet = [0xff, 0xff, joint.id(), 4, 0x03, 41, value, 0];
    packet[7] = checksum(&packet[2..7]);
    packet
}
fn speed_write(joint: Joint, value: u16) -> [u8; 9] {
    let [low, high] = value.to_le_bytes();
    let mut packet = [0xff, 0xff, joint.id(), 5, 0x03, 46, low, high, 0];
    packet[8] = checksum(&packet[2..8]);
    packet
}
/// One sync write of the goal position (register 42, two bytes) of the three
/// commanded joints. A broadcast: no servo answers it.
fn goals_write(goals: [u16; 3]) -> [u8; 17] {
    let mut packet = [0u8; 17];
    packet[..7].copy_from_slice(&[0xff, 0xff, 0xfe, 13, 0x83, 42, 2]);
    for (index, (joint, goal)) in COMMANDED.iter().zip(goals).enumerate() {
        let [low, high] = goal.to_le_bytes();
        packet[7 + index * 3..10 + index * 3].copy_from_slice(&[joint.id(), low, high]);
    }
    packet[16] = checksum(&packet[2..16]);
    packet
}

fn nothing_yet(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
    )
}

struct Bus<'a, P, R> {
    io: &'a mut P,
    runtime: &'a mut R,
}

impl<P: PortIo, R: Runtime> Bus<'_, P, R> {
    /// A byte that arrives before a request belongs to nothing sent here.
    fn expect_quiet(&mut self) -> Result<(), String> {
        let mut bytes = [0u8; 64];
        match self.io.read(&mut bytes) {
            Ok(0) => Err("the servo port closed".into()),
            Ok(count) => Err(format!("{count} unexpected bytes before a request")),
            Err(error) if nothing_yet(&error) => Ok(()),
            Err(error) => Err(format!("read: {error}")),
        }
    }

    fn send(&mut self, request: &[u8]) -> Result<(), String> {
        self.expect_quiet()?;
        let deadline = self.runtime.now() + EXCHANGE;
        let mut sent = 0;
        while sent < request.len() {
            match self.io.write(&request[sent..]) {
                Ok(0) => return Err("the servo port accepted nothing".into()),
                Ok(count) => sent += count.min(request.len() - sent),
                Err(error) if nothing_yet(&error) => {
                    let _ = self.io.wait(Interest::Write, POLL);
                }
                Err(error) => return Err(format!("write: {error}")),
            }
            if sent < request.len() && self.runtime.now() >= deadline {
                return Err("the request was not accepted in time".into());
            }
        }
        Ok(())
    }

    /// Send a read and collect one reply from every joint asked.
    fn read(
        &mut self,
        request: &ReadOnlyRequest,
        mut reply: impl FnMut(crate::protocol::MatchedReply) -> Result<(), String>,
    ) -> Result<(), String> {
        self.send(request.bytes())?;
        // A latched flag is information here, not the end of the read: the
        // caller compares every status with the one found at start.
        let mut tracker = request.track_replies_reporting_faults();
        let mut parser = StatusParser::default();
        let deadline = self.runtime.now() + EXCHANGE;
        let mut bytes = [0u8; 64];
        loop {
            match self.io.read(&mut bytes) {
                Ok(0) => return Err("the servo port closed".into()),
                Ok(count) => {
                    let mut failure = None;
                    let fed = parser.feed(&bytes[..count.min(bytes.len())], |event| {
                        if failure.is_some() {
                            return;
                        }
                        match tracker.accept(event) {
                            Ok(matched) => failure = reply(matched).err(),
                            Err(error) => failure = Some(error.to_string()),
                        }
                    });
                    if let Some(failure) = failure {
                        return Err(failure);
                    }
                    if fed.discarded_bytes != 0 {
                        return Err(format!("{} bytes of noise", fed.discarded_bytes));
                    }
                    if tracker.is_complete() {
                        if parser.buffered_bytes() != 0 {
                            return Err("bytes after the last reply".into());
                        }
                        return Ok(());
                    }
                }
                Err(error) if nothing_yet(&error) => {
                    let _ = self.io.wait(Interest::Read, POLL);
                }
                Err(error) => return Err(format!("read: {error}")),
            }
            if self.runtime.now() >= deadline {
                return Err("a reply did not come in time".into());
            }
        }
    }

    fn samples(&mut self) -> Result<[Sample; 5], String> {
        let request = ReadOnlyRequest::sync_read(&Joint::ALL, ReadWindow::TELEMETRY)
            .map_err(|error| error.to_string())?;
        let mut samples = [Sample::default(); 5];
        self.read(&request, |reply| {
            let telemetry = reply.telemetry().map_err(|error| error.to_string())?;
            samples[reply.joint().index()] = Sample {
                position: telemetry.position().raw(),
                load: telemetry.load().magnitude_raw(),
                temperature: telemetry.temperature_raw(),
                // Either place a flag shows: the reply's own byte or the register.
                status: reply.status_flags().bits() | telemetry.status_raw(),
            };
            Ok(())
        })?;
        Ok(samples)
    }

    /// Torque, acceleration and goal speed of the commanded joints.
    fn profiles(&mut self) -> Result<[(u8, u8, u16); 3], String> {
        let request = ReadOnlyRequest::sync_read(&COMMANDED, ReadWindow::TORQUE_AND_PROFILE)
            .map_err(|error| error.to_string())?;
        let mut profiles = [(0, 0, 0); 3];
        self.read(&request, |reply| {
            let profile = reply
                .torque_and_profile()
                .map_err(|error| error.to_string())?;
            let index = COMMANDED
                .iter()
                .position(|joint| *joint == reply.joint())
                .ok_or("a reply from a joint that was not asked")?;
            profiles[index] = (
                profile.torque_enable_raw(),
                profile.acceleration_raw(),
                profile.goal_speed_raw(),
            );
            Ok(())
        })?;
        Ok(profiles)
    }

    /// One register write to one servo and its plain acknowledgement, which
    /// has the form of a ping's.
    fn write_checked(&mut self, joint: Joint, packet: &[u8]) -> Result<(), String> {
        self.send(packet)?;
        let mut tracker = ReadOnlyRequest::ping(joint).track_replies();
        let mut parser = StatusParser::default();
        let deadline = self.runtime.now() + EXCHANGE;
        let mut bytes = [0u8; 16];
        loop {
            match self.io.read(&mut bytes) {
                Ok(0) => return Err("the servo port closed".into()),
                Ok(count) => {
                    let mut failure = None;
                    parser.feed(&bytes[..count.min(bytes.len())], |event| {
                        if let Err(error) = tracker.accept(event) {
                            failure.get_or_insert(error.to_string());
                        };
                    });
                    if let Some(failure) = failure {
                        return Err(failure);
                    }
                    if tracker.is_complete() {
                        return Ok(());
                    }
                }
                Err(error) if nothing_yet(&error) => {
                    let _ = self.io.wait(Interest::Read, POLL);
                }
                Err(error) => return Err(format!("read: {error}")),
            }
            if self.runtime.now() >= deadline {
                return Err(format!("{joint} did not acknowledge a write"));
            }
        }
    }

    fn sleep_until(&mut self, due: Duration) {
        loop {
            let now = self.runtime.now();
            if now >= due {
                return;
            }
            let _ = self.io.wait(Interest::Read, (due - now).min(POLL * 4));
        }
    }
}

/// Run the body until it is cancelled or something is not as expected.
/// `news` is asked once a step for the latest mood and never waits. `home`
/// is the pitch, roll and yaw every gesture is an offset from; without one
/// it is the pose found at start. With one, the head first eases there from
/// where it is found, at the same speed as everything else.
pub(crate) fn run(
    io: &mut impl PortIo,
    runtime: &mut impl Runtime,
    limits: Limits,
    home: Option<[u16; 3]>,
    mut news: impl FnMut() -> Option<(Mood, u8)>,
    mut watch: impl FnMut(&Glimpse),
) -> BodyReport {
    let mut report = BodyReport {
        limits,
        moved: false,
        home: None,
        found: None,
        steps: 0,
        goal_writes: 0,
        profile_writes: 0,
        missed_reads: 0,
        peak_load: [0; 3],
        peak_temperature: [0; 3],
        held_at: None,
        last: None,
        stop: Stop::Cancelled,
    };
    let mut bus = Bus { io, runtime };
    let refused = |why: String| Stop::Refused { why };

    // What is there, before anything is written.
    let found = match bus.samples() {
        Ok(found) => found,
        Err(detail) => {
            report.stop = refused(format!("first read: {detail}"));
            return report;
        }
    };
    report.found = Some(found);
    report.last = Some(found);
    let profiles = match bus.profiles() {
        Ok(profiles) => profiles,
        Err(detail) => {
            report.stop = refused(format!("first read: {detail}"));
            return report;
        }
    };
    let found_pose = COMMANDED.map(|joint| found[joint.index()].position);
    let home = home.unwrap_or(found_pose);
    report.home = Some(home);
    for (index, joint) in COMMANDED.iter().enumerate() {
        let sample = found[joint.index()];
        let why = if profiles[index].0 != 1 {
            Some(format!("{joint} torque is not on"))
        } else if sample.status != 0 {
            Some(format!("{joint} status is {:#04x}", sample.status))
        } else if sample.temperature >= limits.temperature {
            Some(format!("{joint} is at {} already", sample.temperature))
        } else if sample.load > limits.load[index] {
            Some(format!("{joint} already carries load {}", sample.load))
        } else {
            None
        };
        if let Some(why) = why {
            report.stop = refused(why);
            return report;
        }
    }
    for pitch in [home[0], found_pose[0]] {
        if !(limits.pitch_home.0..=limits.pitch_home.1).contains(&pitch) {
            report.stop = refused(format!(
                "head pitch {pitch} is outside {}..={}",
                limits.pitch_home.0, limits.pitch_home.1
            ));
            return report;
        }
    }
    for (index, joint) in COMMANDED.iter().enumerate() {
        let apart = found_pose[index].abs_diff(home[index]);
        if apart > limits.start_reach {
            report.stop = refused(format!("{joint} is {apart} counts from its home"));
            return report;
        }
    }

    // The servo's own speed bound, where it is not already the one wanted.
    for (index, joint) in COMMANDED.iter().enumerate() {
        let (_, acceleration, speed) = profiles[index];
        let mut outcome = Ok(());
        if speed != limits.servo_speed {
            report.profile_writes += 1;
            outcome = bus.write_checked(*joint, &speed_write(*joint, limits.servo_speed));
        }
        if outcome.is_ok() && acceleration != limits.servo_acceleration {
            report.profile_writes += 1;
            outcome = bus.write_checked(
                *joint,
                &acceleration_write(*joint, limits.servo_acceleration),
            );
        }
        if let Err(detail) = outcome {
            report.stop = Stop::Bus {
                detail: format!("profile of {joint}: {detail}"),
            };
            return report;
        }
    }
    match bus.profiles() {
        Ok(now)
            if now
                .iter()
                .all(|profile| *profile == (1, limits.servo_acceleration, limits.servo_speed)) => {}
        Ok(now) => {
            report.stop = Stop::Bus {
                detail: format!("profile read back as {now:?}"),
            };
            return report;
        }
        Err(detail) => {
            report.stop = Stop::Bus {
                detail: format!("profile read back: {detail}"),
            };
            return report;
        }
    }

    // The reach around home, and the absolute window for the head's pitch.
    let low = [
        (f32::from(home[0]) - limits.pitch_up).max(f32::from(limits.pitch_window.0)),
        f32::from(home[1]) - limits.roll,
        f32::from(home[2]) - limits.yaw,
    ];
    let high = [
        (f32::from(home[0]) + limits.pitch_down).min(f32::from(limits.pitch_window.1)),
        f32::from(home[1]) + limits.roll,
        f32::from(home[2]) + limits.yaw,
    ];
    let step = TICK.as_secs_f32();
    let mut gestures = Gestures::new();
    // It starts from where it is, not from where it should be: the way home
    // is travelled like any other move. Until it is inside the reach, the
    // only bound that applies is that it never moves away from it.
    let mut commanded = found_pose.map(f32::from);
    let mut velocity = [0.0f32; 3];
    let mut written = found_pose;
    let mut hot = [0u8; 3];
    let mut heard: Option<(Mood, u8, Duration)> = None;
    let mut over_load = [0u8; 3];
    let mut off_track = [0u8; 3];
    let mut misses = 0u8;
    let mut leaving: Option<Duration> = None;
    let mut resting_steps = 0u16;
    let mut relaxed = false;
    let started = bus.runtime.now();

    let stop = loop {
        report.steps += 1;
        let due = started + TICK * (report.steps.min(u64::from(u32::MAX)) as u32);
        let now = bus.runtime.now();
        match bus.runtime.cancelled() {
            Ok(false) => {}
            // Asked to stop, or unable to tell: ease home and leave.
            Ok(true) | Err(_) => {
                leaving.get_or_insert(now);
            }
        }
        if let Some((mood, level)) = news() {
            heard = Some((mood, level, now));
        }
        let (mood, level) = match (leaving, heard) {
            (None, Some((mood, level, at))) if now.saturating_sub(at) < MOOD_STALE => (mood, level),
            _ => (Mood::Resting, 0),
        };
        let Pose { pitch, roll, yaw } =
            gestures.target(mood, level, now.as_micros().try_into().unwrap_or(u64::MAX));
        let mut goals = written;
        // Rest that has lasted: nothing is asked of the joints until the
        // mood changes, so the goals stay where the joints came to rest.
        resting_steps = if mood == Mood::Resting && leaving.is_none() {
            resting_steps.saturating_add(1)
        } else {
            0
        };
        if resting_steps == 0 {
            relaxed = false;
        }
        // Each axis of a pose is a share of its reach.
        let offsets = [
            pitch
                * if pitch < 0.0 {
                    limits.pitch_up
                } else {
                    limits.pitch_down
                },
            roll * limits.roll,
            yaw * limits.yaw,
        ];
        for (index, offset) in offsets.into_iter().enumerate() {
            if relaxed {
                break;
            }
            let wanted = (f32::from(home[index]) + offset).clamp(low[index], high[index]);
            let pull =
                FOLLOW * FOLLOW * (wanted - commanded[index]) - 2.0 * FOLLOW * velocity[index];
            velocity[index] = (velocity[index]
                + pull.clamp(-limits.acceleration, limits.acceleration) * step)
                .clamp(-limits.speed, limits.speed);
            let (floor, ceiling) = (
                low[index].min(commanded[index]),
                high[index].max(commanded[index]),
            );
            commanded[index] = (commanded[index] + velocity[index] * step).clamp(floor, ceiling);
            goals[index] = commanded[index].round() as u16;
        }
        if goals != written {
            if let Err(detail) = bus.send(&goals_write(goals)) {
                break Stop::Bus { detail };
            }
            written = goals;
            report.goal_writes += 1;
            report.moved = true;
        }
        let samples = match bus.samples() {
            Ok(samples) => {
                misses = 0;
                samples
            }
            Err(detail) => {
                report.missed_reads += 1;
                misses += 1;
                if misses >= 3 {
                    break Stop::Bus { detail };
                }
                bus.sleep_until(due);
                continue;
            }
        };
        report.last = Some(samples);
        if resting_steps >= REST_STEPS && !relaxed {
            let positions = COMMANDED.map(|joint| samples[joint.index()].position);
            let near = (0..3).all(|index| {
                positions[index].abs_diff(written[index]) <= REST_SLACK
                    && velocity[index].abs() < 3.0
            });
            if near {
                relaxed = true;
                if positions != written {
                    if let Err(detail) = bus.send(&goals_write(positions)) {
                        break Stop::Bus { detail };
                    }
                    written = positions;
                    report.goal_writes += 1;
                }
                commanded = positions.map(f32::from);
                velocity = [0.0; 3];
            }
        }
        watch(&Glimpse {
            step: report.steps,
            mood,
            goals: written,
            positions: COMMANDED.map(|joint| samples[joint.index()].position),
            loads: COMMANDED.map(|joint| samples[joint.index()].load),
        });
        let mut surprise = None;
        for joint in Joint::ALL {
            let (was, is) = (found[joint.index()], samples[joint.index()]);
            if is.status != was.status {
                surprise.get_or_insert(Stop::Status {
                    joint: joint.id(),
                    found: was.status,
                    now: is.status,
                });
            }
        }
        for joint in BYSTANDERS {
            let (was, is) = (found[joint.index()], samples[joint.index()]);
            if was.position.abs_diff(is.position) > limits.bystander_drift {
                surprise.get_or_insert(Stop::Drift {
                    joint: joint.id(),
                    found: was.position,
                    now: is.position,
                });
            }
        }
        for (index, joint) in COMMANDED.iter().enumerate() {
            let is = samples[joint.index()];
            report.peak_load[index] = report.peak_load[index].max(is.load);
            over_load[index] = if is.load > limits.load[index] {
                over_load[index].saturating_add(1)
            } else {
                0
            };
            off_track[index] = if written[index].abs_diff(is.position) > limits.tracking {
                off_track[index].saturating_add(1)
            } else {
                0
            };
            report.peak_temperature[index] = report.peak_temperature[index].max(is.temperature);
            hot[index] = if is.temperature >= limits.temperature {
                hot[index].saturating_add(1)
            } else {
                0
            };
            if over_load[index] >= limits.load_steps.max(1) {
                surprise.get_or_insert(Stop::Load {
                    joint: joint.id(),
                    load: is.load,
                });
            } else if off_track[index] >= limits.tracking_steps {
                surprise.get_or_insert(Stop::Tracking {
                    joint: joint.id(),
                    goal: written[index],
                    position: is.position,
                });
            } else if hot[index] >= limits.temperature_steps.max(1) {
                surprise.get_or_insert(Stop::Temperature {
                    joint: joint.id(),
                    temperature: is.temperature,
                });
            }
        }
        if let Some(surprise) = surprise {
            break surprise;
        }
        if let Some(since) = leaving {
            let settled = (0..3).all(|index| {
                (commanded[index] - f32::from(home[index])).abs() < 0.6
                    && velocity[index].abs() < 3.0
            });
            if settled || now.saturating_sub(since) >= LEAVE {
                break Stop::Cancelled;
            }
        }
        bus.sleep_until(due);
    };

    if stop == Stop::Cancelled {
        // Leave the goals exactly at home.
        if written != home && bus.send(&goals_write(home)).is_ok() {
            report.goal_writes += 1;
        }
    } else if let Some(last) = report.last {
        // Stop pushing: each goal becomes where its joint is now, kept
        // inside the same bounds as every other goal.
        let hold: [u16; 3] = std::array::from_fn(|index| {
            let position = f32::from(last[COMMANDED[index].index()].position);
            position
                .clamp(
                    low[index].min(commanded[index]),
                    high[index].max(commanded[index]),
                )
                .round() as u16
        });
        if bus.send(&goals_write(hold)).is_ok() {
            report.goal_writes += 1;
            report.held_at = Some(hold);
        }
    }
    report.stop = stop;
    report
}

#[cfg(test)]
mod tests;
