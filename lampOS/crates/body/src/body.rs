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
//! - It never touches torque unless told to take hold (`Limits::torque_on`),
//!   to let go under overload or, with `Limits::give_way`, to let one
//!   joint of the three go limp under a hand.
//!   A joint whose torque is off, or whose status byte is not clear, is a
//!   reason not to start; told to take hold, it first sets the goal of a
//!   joint found without torque to where it is and then writes torque on.
//! - Every goal is within a fixed reach of its home (a pose given at start,
//!   or else the pose found), and the head's pitch also within a fixed
//!   absolute window short of the base. A head found outside the reach of a
//!   given home is first brought into it, never moved away from it. Told
//!   to wake (`Limits::wake`), a joint found out of where it may start, up
//!   to [`WAKE_REACH`] from that home, is first brought there slowly.
//! - After every step it reads all five joints. A load over its bound, a
//!   joint that does not follow, a status byte that changes, a shoulder or
//!   elbow that moves, a joint that stays hot or a wire that is not exactly
//!   as expected ends the run: the present positions are written as the goals, so that
//!   nothing keeps pushing, and nothing is written after that.
use crate::{
    Joint,
    clips::{self, Clip},
    gaze::{Gaze, Looking, Seen},
    gesture::{Gestures, Mood, Pose},
    inspection::{Interest, PortIo, Runtime},
    protocol::{ReadOnlyRequest, ReadWindow, StatusFlags, StatusParser},
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
/// A joint that met resistance is left where it is for this many steps, two
/// seconds, before it is asked to move again.
const EASE_STEPS: u16 = 100;
/// How often that may happen before the run ends: the fourth time within
/// this many steps, a minute.
const EASES_ALLOWED: usize = 3;
const EASE_MEMORY_STEPS: u64 = 3_000;
const REST_SLACK: u16 = 24;
/// The most a joint it took hold of may have moved when its torque came on:
/// a goal set where it is moves it by nothing a servo would notice.
const TAKE_HOLD_SLACK: u16 = 12;
/// A joint that has arrived is given its own position as its goal, in any
/// mood, and is left there until the pose wanted of it is this many counts
/// from where it is. Asked for less, a servo pushes and does not move: on
/// lamp-4ace the head sat 3 to 8 counts from the goals of its nods while its
/// load climbed from 48 to 178 in a second and a half, three times in a
/// minute. With 12 here it did the same for nods of 11 and 12 counts, four
/// times in ten minutes: from rest that joint does not start for so little.
const WAKE_COUNTS: f32 = 20.0;
/// With `--give-way`, a hand as the old runtime (V1) met it
/// (`robots/lamp/servo_overload.json`, `hal/drivers/motors/animation_service.py`
/// on `main`), the hardware team's choice to protect the servos. A
/// commanded joint over `CONTACT_FLOOR` (head pitch, head roll, base yaw;
/// V1's `contact.load`, 0.1% of full drive) for `CONTACT_STEPS` steps, or
/// over its own load bound as before, stops where it is. Still pushed after
/// that for `HAND_STEPS` steps in a row (V1's `release_after_s`, 1.0 s):
/// its load over the lower of the bound that stopped it and its floor, or
/// held more than `HAND_MOVED` counts (V1's `CONTACT_RELEASE_LAG`, one
/// degree) from where it stopped, and its torque alone goes off. Left
/// alone, moving no more than `LIMP_STILL` counts for `LIMP_STEPS` steps
/// (two seconds), it is taken hold of where it is. On lamp-52e6 on
/// 2026-10-14 a head that only gave way by its goal, torque on, could not
/// be pushed at all: roll 2,019 to 1,958 at a load of 256, yaw 2,035 to
/// 1,986 at 188.
const CONTACT_FLOOR: [u16; 3] = [650, 750, 550];
/// With a force limit the base's floor is this share of it, in percent:
/// V1's 550 is 55% of full drive.
const CONTACT_YAW_SHARE: u16 = 55;
const CONTACT_STEPS: u8 = 3;
const HAND_STEPS: u16 = 50;
const HAND_MOVED: u16 = 11;
const LIMP_STILL: u16 = 3;
const LIMP_STEPS: u16 = 100;
/// With `--give-way`, a joint whose goal is more than `STALL_SLACK` counts
/// from where it is, whose load is over `STALL_LOAD` and which moves no
/// more than `STALL_SLACK` counts for `STALL_STEPS` steps is told to stop
/// pushing: its goal becomes where it is, until what is wanted of it moves
/// `WAKE_COUNTS` or more. Never torque off for that: on V1 a shoulder or an
/// elbow let go dropped the arm with nobody touching it.
///
/// One and a half seconds. Longer than a joint takes to be put to rest by
/// what it already has (arrived for 0.3 s, at rest for 1 s), so that only
/// a joint those leave pushing is caught. Shorter than the servo's own
/// protection (80% load for 2 s, then 20% of its force: on the shoulder or
/// the elbow that is the arm dropping), so that this acts first. Heat comes
/// from pushing for minutes, not seconds (the elbow on lamp-4ace reached
/// 69 degrees one day with the head resting on the base), so a second and a
/// half of it costs nothing. 100 of 1,000 is over what a resting joint
/// reads at its goal on lamp-4ace (68 for the head, 44 to 72 for the base
/// resting short) and under the 100 to 150 lamp-52e6's shoulder pushes with
/// for as long as it stands, 22 to 34 counts short of its goal.
const STALL_STEPS: u16 = 75;
const STALL_LOAD: u16 = 100;
const STALL_SLACK: u16 = 6;
/// Arrived: this close to the wanted pose and this slow, for this many steps.
const ARRIVED_COUNTS: f32 = 6.0;
const ARRIVED_SPEED: f32 = 15.0;
const ARRIVED_STEPS: u8 = 15;
/// Steps in a row on which a read, or a write, may fail before the run
/// ends: half a second. While one is failing nothing new is asked of the
/// joints. Three steps were not enough: on lamp-4ace one stall of the whole
/// machine (the ring's own write overran in the same second) left a late
/// reply on the wire, the next three exchanges tripped over it, and the lamp
/// had no body until someone started it again.
const WIRE_STEPS: u8 = 25;

/// The joints this body commands, in the order of every array of three here.
/// The glance a look away begins with, in counts of the base's turn: a
/// little over three degrees, which the base is back from in a third of a
/// second at the speed lamp-4ace runs with. It is the whole look away of a
/// lamp with the folded reach (nine tenths of 40 counts). More than
/// [`WAKE_COUNTS`], or it would not be made at all.
const GLANCE: f32 = 36.0;

/// Waking (`Limits::wake`): the farthest a commanded joint may be found
/// from its home and still be brought there, about 35 degrees. A head
/// resting on the base was found 145 counts below lamp-4ace's home (2,095
/// for 1,950); lamp-52e6's home is 1,771, so its head on the base would be
/// some 300 away. Beyond this a joint is somewhere no lamp has been seen,
/// and it is refused as before. At the wake's speed the whole of it takes
/// 31 s, inside the 90 s systemd allows a service to say it is ready.
pub const WAKE_REACH: u16 = 400;
/// The fastest a joint moves while it wakes, counts a second: two fifths of
/// the body's own 50, under two degrees a second.
const WAKE_SPEED: f32 = 20.0;
/// A wake begins with a stir of at most this many counts toward home, more
/// than [`WAKE_COUNTS`] so that a servo starts for it, and then a pause.
const WAKE_STIR: f32 = 24.0;
const WAKE_PAUSE: f32 = 1.0;
/// A head that must lift out of a droop does not move for a goal just
/// ahead of it. On lamp-52e6 after a power cut on 2026-10-14 (head pitch
/// gain P 32), a goal 40 counts up moved the drooped head 7 counts and it
/// stuck at a load of 144; on a ramp of 260 counts in 3 s it lagged its
/// goal by 40 to 80 counts at loads of 200 to 320 and stopped 37 short of
/// the end. So the stir of a head that lifts reaches this far ahead of
/// where it was found, the largest lag seen while it lifted, and the rise
/// begins from there with the goal leading it.
const WAKE_LIFT_LEAD: f32 = 80.0;
/// While its goal is still on the way, a waking joint may be this far from
/// it (the most lamp-52e6's head lagged while lifting, 80, and some), for
/// `Limits::tracking_steps` steps, before the run ends.
const WAKE_TRACKING: u16 = 100;
/// The load a head lifting out of a droop may read for
/// `Limits::load_steps` steps: lamp-52e6's read 200 to 320 doing it.
const WAKE_LIFT_LOAD: u16 = 400;
/// Once every goal is home, a joint still farther than `Limits::tracking`
/// from it that has not moved `WAKE_MOVED` counts toward it for this many
/// steps (half a second) ends the run: it has stopped short.
const WAKE_STILL_STEPS: u8 = 25;
const WAKE_MOVED: u16 = 3;
/// A joint that has stopped coming nearer within this many counts of home
/// has woken where it settled. On lamp-52e6 on 2026-10-14, woken from a
/// real droop (found yaw 1,979, roll 2,101, pitch 2,029 for a home of
/// 2,035, 2,035, 1,771), the head lifted 226 counts in 20.8 s and stayed 32
/// short of home, the yaw 11 and the roll 17: from below, these servos
/// (pitch gain P 32, roll and yaw 16) do not close the last 30 to 40
/// counts. Farther than this from home it has not woken, and the run ends.
const WAKE_SETTLE: u16 = 50;
/// How far the shoulder or the elbow may sway while the lamp wakes before
/// that is taken for a move: on lamp-4ace on 2026-10-14 the shoulder moved
/// from 2,050 to 2,057 as the base turned 189 counts of its wake, one over
/// the run's 6. Twice what was seen, about a degree and a third; an arm
/// that falls or is pushed moves far more.
const WAKE_SWAY: u16 = 15;
/// The fastest a shoulder found up by itself is lowered while the lamp
/// wakes, counts a second. On lamp-52e6 on 2026-10-14, unplugged standing,
/// the shoulder stayed up at 2,633 on its gears and the elbow fell folded;
/// eased by hand from 2,633 to 2,053 over 15 s (39 a second on average,
/// about 58 at its fastest) it reached 2,059 at a load of 172 at most. This
/// is a little slower than that at its fastest: 580 counts take about 22 s,
/// and with a head's wake after it the lamp is up well inside the 90 s
/// systemd allows a service to say it is ready. The wake's own 20 would
/// take 44 s for the shoulder alone.
const ARM_LOWER_SPEED: f32 = 40.0;

/// Under `--overload release` (`Limits::overload_release`), the old
/// runtime's cut-off (`hal/drivers/motors/overload.py`,
/// `robots/lamp/servo_overload.json`): any of the five joints that reads a
/// load of at least `OVERLOAD_LOAD` of 1,000 on `OVERLOAD_STEPS` steps in a
/// row (1.5 s) has torque switched off on all five, which are left so for
/// `OVERLOAD_REST` and then taken up again slowly. A second time within
/// `OVERLOAD_AGAIN` of the first ends the run with torque left off.
///
/// A joint held to a force limit (`--force-limit`) cannot read much more
/// than that limit, so 800 would never trip on it: V1 says so of itself
/// ("a force-limited joint can never read the 800 that trips the
/// cut-off"). For such a joint the level is `OVERLOAD_SHARE` percent of its
/// limit, the same 80% of what it may push with that 800 is of a joint
/// without one: 480 at lamp-4ace's 600.
const OVERLOAD_LOAD: u16 = 800;
const OVERLOAD_SHARE: u16 = 80;
const OVERLOAD_STEPS: u16 = 75;
const OVERLOAD_REST: Duration = Duration::from_secs(120);
const OVERLOAD_AGAIN: Duration = Duration::from_secs(600);

pub const COMMANDED: [Joint; 3] = [Joint::WristPitch, Joint::WristRoll, Joint::BaseYaw];
/// A gesture asked for while the lamp is still rising waits this long for it
/// to be standing, and is then forgotten.
const GESTURE_WAIT: Duration = Duration::from_secs(3);
/// The most a gesture's offsets change in one step: head pitch, head roll,
/// base yaw, shoulder, elbow. A little over the fastest a clip asks, so that
/// a clip is followed exactly and one that is dropped goes back no faster.
const GESTURE_STEP: [f32; 5] = [24.0, 34.0, 26.0, 21.0, 21.0];
/// The farthest a gesture takes each of those from where the lamp has it.
const GESTURE_REACH: [f32; 5] = [220.0, 220.0, 170.0, 360.0, 360.0];
/// While a gesture plays, and for half a second after, a joint of the head
/// may be this far from its goal and read this load before the gesture is
/// dropped; the shoulder and the elbow may be this far from theirs. A quick
/// move lags and reverses hard. That is the move, not something in its way,
/// and what gives is the gesture, never the run.
const GESTURE_TRACKING: u16 = 150;
const GESTURE_LOAD: u16 = 700;
const GESTURE_ARM_TRACKING: u16 = 260;
const GESTURE_COOL_STEPS: u16 = 25;
/// Looks in a row with a face in them, about a second of them, after which
/// somebody is taken to be in front of the lamp: it stands up for them and
/// stays up while they are there.
const COMPANY_SIGHTS: u8 = 6;
/// A lamp that may idle and has stood at rest this long shifts a little by
/// itself, and waits this long after one shift before the next.
const IDLE_AFTER: Duration = Duration::from_secs(4);
const IDLE_EVERY: Duration = Duration::from_secs(8);

/// What reaches the body from outside in one step, besides the mood: what
/// its eyes saw, and a gesture it was asked to make.
#[derive(Clone, Copy, Debug, Default)]
pub struct Asked {
    pub seen: Option<Seen>,
    pub gesture: Option<&'static Clip>,
    /// What the part that plans what the lamp shows asks now
    /// (`crate::expression_in`, `docs/expression.md`): encoder counts to
    /// add to the pose wanted of the head's pitch, the head's roll and the
    /// base's turn, in that order. `None` when it asks nothing or what it
    /// asked has run out.
    pub expression: Option<[f32; 3]>,
}

/// The most that may be added to a pose for [`Asked::expression`], in
/// counts, for the head's pitch, its roll and the base's turn: about five,
/// four and six degrees. What is asked is cut to this and then to the
/// reach of each joint like everything else that follows the mood.
pub const EXPRESSION_REACH: [f32; 3] = [60.0, 45.0, 70.0];

/// The shoulder and the elbow, in the order of every array of two here.
/// Read on every step. Written only by a body that was given a pose to
/// stand in.
pub const ARM: [Joint; 2] = [Joint::BasePitch, Joint::ElbowPitch];

/// Standing up. With this the lamp rises out of its folded pose when someone
/// speaks to it and folds down again once it has been left alone. Without
/// it the shoulder and the elbow are never written.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Standing {
    /// Shoulder and elbow when folded.
    pub folded: [u16; 2],
    /// Head pitch, head roll, base yaw, shoulder and elbow when stood up.
    pub pose: [u16; 5],
    /// How much of the way up it goes: 1 is all of it.
    pub share: f32,
    /// Seconds the whole way takes.
    pub seconds: f32,
    /// It folds down this long after the last mood that was not rest.
    pub awake: Duration,
    /// A shoulder or an elbow reading above this for `Limits::load_steps`
    /// steps sends the lamp back down.
    pub load: [u16; 2],
    /// Standing with nothing to do, it shifts a little now and then.
    pub idle: bool,
}

impl Standing {
    /// The farthest the shoulder or the elbow may be asked to travel
    /// between folded and stood up: about 120 degrees.
    pub const TRAVEL_CAP: u16 = 1_400;
    /// A shoulder or an elbow this far from its goal for half a second
    /// sends the lamp back down. Much wider than the head's: these two
    /// carry the lamp. On lamp-4ace the shoulder held 53 counts under its
    /// goal a third of the way up and 65 at six tenths, at loads of 224
    /// and 272, for as long as it stood; the elbow reached its goal.
    pub const TRACKING: u16 = 160;
    pub const TRACKING_STEPS: u8 = 25;
    /// Over its load or off its goal for this long, two seconds, with the
    /// lamp already told to go down: the run ends.
    pub const STUCK_STEPS: u8 = 100;
    /// As warm as this for `Limits::temperature_steps`, likewise sent down.
    pub const TEMPERATURE: u8 = 62;
    /// Sent back down, it stays down this long, and after the third time
    /// for the rest of the run.
    pub const TIRED: Duration = Duration::from_secs(60);
    pub const RETREATS_ALLOWED: u8 = 3;
    /// To start, each of the two must be found within this of the way
    /// between folded and stood up.
    pub const FOUND_SLACK: u16 = 80;

    /// `share` in percent, `seconds` for the whole way, `awake` in seconds.
    pub fn new(
        folded: [u16; 2],
        pose: [u16; 5],
        share: u8,
        seconds: u8,
        awake: u16,
        load: [u16; 2],
    ) -> Result<Self, &'static str> {
        let far = folded
            .iter()
            .zip(&pose[3..])
            .any(|(from, to)| from.abs_diff(*to) > Self::TRAVEL_CAP);
        if far
            || pose.iter().chain(&folded).any(|count| *count > 4_095)
            || !(1..=100).contains(&share)
            || !(2..=20).contains(&seconds)
            || !(5..=3_600).contains(&awake)
            || load.iter().any(|bound| !(50..=1_000).contains(bound))
        {
            return Err("a standing pose outside what this body may be given");
        }
        Ok(Self {
            folded,
            pose,
            share: f32::from(share) / 100.0,
            seconds: f32::from(seconds),
            awake: Duration::from_secs(u64::from(awake)),
            load,
            idle: false,
        })
    }

    /// The same, shifting a little now and then while it stands at rest, or
    /// not.
    pub fn idling(self, idle: bool) -> Self {
        Self { idle, ..self }
    }
}

/// Ease in and out: no speed at either end of the way.
fn smooth(share: f32) -> f32 {
    let share = share.clamp(0.0, 1.0);
    share * share * (3.0 - 2.0 * share)
}

/// The share that `smooth` turns into this much of the way.
fn unsmooth(lift: f32) -> f32 {
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if smooth(middle) < lift {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) / 2.0
}

/// How far the head is toward its standing pose with the arm this far up.
/// It waits for the first quarter of the arm's way and is back at its folded
/// pose before the arm comes down that far: the head leaves the base and
/// returns beside it with the arm already carrying it clear.
fn head_share(stand: f32) -> f32 {
    smooth((stand - 0.25) / 0.75)
}

/// Bounds in encoder counts, about 11.4 to a degree. Fixed in code: nothing
/// outside this process can widen them.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Limits {
    /// The head's pitch must be found inside this window to start at all.
    pub pitch_home: (u16, u16),
    /// And is never commanded outside this one, whatever the reach allows.
    pub pitch_window: (u16, u16),
    /// The head's pitch is never asked to go under this, folded or stood
    /// up, whatever a mood, the eyes and a gesture add up to: the servo's
    /// own limit is just below it and a goal past that is never reached.
    pub pitch_floor: u16,
    /// The farthest a goal may be from the found pose: up, down, roll, yaw.
    pub pitch_up: f32,
    pub pitch_down: f32,
    pub roll: f32,
    pub yaw: f32,
    /// The head's roll reach once the lamp has stood up, where it is more
    /// than `roll`: folded, lamp-4ace's head cannot tilt at all (its roll
    /// servo meets something after four counts); stood up it is free.
    pub stand_roll: f32,
    /// The fastest the commanded pose moves, and how fast that may change.
    pub speed: f32,
    pub acceleration: f32,
    /// The profile written to each commanded servo at start.
    pub servo_speed: u16,
    pub servo_acceleration: u8,
    /// The most force the joints that carry the lamp may push with, of
    /// 1,000 (the servo's register 48), written to each at start and read
    /// back: the base's turn always, the shoulder and the elbow when the
    /// lamp may stand. `None`: nothing is written and each keeps what it
    /// holds, which on lamp-4ace is all of it.
    pub force: Option<u16>,
    /// Write torque on at start to a joint found without it, which this
    /// body would otherwise refuse, its goal first set to where it is: a
    /// servo whose torque went off keeps an old goal, and on lamp-52e6 those
    /// were 18 to 39 counts from where the joints were. Off: torque is
    /// never written to such a joint.
    pub torque_on: bool,
    /// Bring a commanded joint found out of where it may start, up to
    /// [`WAKE_REACH`] from a home given at start, back to that home slowly
    /// before anything else, instead of refusing: see [`wake_pose`]. Off:
    /// such a joint is refused.
    pub wake: bool,
    /// How a joint gives way to a hand (as V1: one still pushed a second
    /// after it stopped goes limp, see `CONTACT_FLOOR`), and whether a
    /// joint that pushes without moving is told to stop (see [`GiveWay`]).
    /// `None`: as before this was chosen, exactly.
    pub give_way: Option<GiveWay>,
    /// Let go under overload as the old runtime did: any joint at 80% of
    /// its full load (or of its force limit) for 1.5 s switches torque off
    /// on all five for two minutes, after which the lamp is taken up again
    /// slowly. Off: torque is never written off.
    pub overload_release: bool,
    /// A load above this on `load_steps` steps in a row makes the joint give
    /// way, and ends the run if it stays or comes back too often.
    pub load: [u16; 3],
    pub load_steps: u8,
    /// A joint found carrying more than this is not started on: at rest it
    /// should be carrying next to nothing, and one that is not is leaning
    /// on something.
    pub start_load: [u16; 3],
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
            acceleration: f32::from(tuning.speed) * 6.0,
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
        // lamp-4ace's head pitch servo has its own lower limit at 1,657. A
        // stretch on top of a look away asked it for 1,568 on 2026-10-13
        // and it sat at 1,656 until the stretch was over.
        pitch_floor: 1_670,
        pitch_up: 45.0,
        pitch_down: 15.0,
        roll: 30.0,
        yaw: 40.0,
        stand_roll: 0.0,
        speed: 50.0,
        acceleration: 400.0,
        servo_speed: 150,
        servo_acceleration: 50,
        // In its first second on lamp-4ace the body read 104, 96 and 88.
        // Six steps, 120 ms: a head that reverses quickly reads over the
        // bound for a few steps (217 on the third, once in eleven minutes
        // of talking) and under it again; something in its way does not.
        // The head's bound was 170 and the base's 150 until 2026-10-13.
        // Starting to lift from rest the head read 173 to 203 for longer
        // than that, and the base 208 turning back in the middle of its look
        // away, with nothing in the way of either; a joint that pushes and
        // does not move is now put to rest within half a second whatever
        // it reads, so the bound only has to catch what is pushed hard.
        load: [240, 150, 220],
        load_steps: 6,
        start_load: [170, 150, 150],
        tracking: 30,
        tracking_steps: 10,
        bystander_drift: 6,
        temperature: 62,
        temperature_steps: 50,
        start_reach: 60,
        force: None,
        torque_on: false,
        wake: false,
        give_way: None,
        overload_release: false,
    };
}

/// What a commanded joint does after it gave way. With either, a joint that
/// pushes at a goal without moving is told to stop pushing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GiveWay {
    /// It comes back two seconds later, as without the option.
    Return,
    /// It comes back from a bump; let go under a hand and then left, it
    /// stays where it was put, and later moods are laid relative to that.
    Stay,
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
    /// What each of the three joints' temperature register reads.
    pub temperatures: [u8; 3],
    /// Times so far that each joint met resistance and was left where it was.
    pub eased: [u16; 3],
    /// Times so far that a hand left each joint where it put it, and that
    /// each joint was told to stop pushing without moving (pitch, roll, yaw,
    /// shoulder, elbow).
    pub kept_where_put: [u16; 3],
    pub stalls: [u16; 5],
    /// Whether this body may stand at all, how far it has stood up, 0
    /// folded to 1, and the shoulder and the elbow: their goals as this
    /// body last wrote them, or else as found.
    pub stands: bool,
    pub stand: f32,
    pub arm_goals: [u16; 2],
    pub arm_positions: [u16; 2],
    pub arm_loads: [u16; 2],
    pub arm_temperatures: [u8; 2],
    /// Where the eyes have it look: yaw and pitch, in counts from the
    /// pose's own direction.
    pub look: [f32; 2],
    /// A face the eyes reported on this step: across, down and size.
    pub face: Option<[f32; 3]>,
    /// The gesture it is making.
    pub gesture: Option<&'static str>,
    /// Under `--overload release`: times so far it let go, and whether its
    /// torque is off now.
    pub released: u16,
    pub limp: bool,
    /// With `--give-way`: which of the three joints a hand has left with
    /// its torque off now.
    pub hand_limp: [bool; 3],
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
    /// Under `--overload release`: it let go a second time within ten
    /// minutes, or could not take the lamp up again. Torque is left as it
    /// was let go, off.
    Overload {
        joint: u8,
        load: u16,
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
    /// Times each commanded joint met resistance and was left where it was.
    pub eased: [u16; 3],
    /// With `--give-way stay`, times a hand left each commanded joint where
    /// it put it.
    pub kept_where_put: [u16; 3],
    /// With `--give-way`, times each joint (pitch, roll, yaw, shoulder,
    /// elbow) pushed without moving and was told to stop.
    pub stalls: [u16; 5],
    /// With `--give-way`, times each commanded joint was still pushed a
    /// second after it stopped under a hand and had its torque switched off.
    pub released_by_hand: [u16; 3],
    /// With `--wake on`, times each commanded joint stopped a little short
    /// of its goal and was let rest there.
    pub rested_short: [u16; 3],
    /// With `--wake on`, the shoulder found up by itself and lowered onto
    /// its folded count before anything else.
    pub lowered: Option<Woke>,
    /// The goals written to stop pushing, when the run ended on a surprise.
    pub held_at: Option<[u16; 3]>,
    pub last: Option<[Sample; 5]>,
    /// The pose it was given to stand in and still had when it ran.
    pub standing: Option<Standing>,
    /// Why it would not stand, though it ran as a folded lamp.
    pub standing_refused: Option<String>,
    /// Writes of torque enable: to clear a latched overload on the shoulder
    /// or the elbow before standing, and, told to take hold, to each joint
    /// found without torque.
    pub torque_writes: u8,
    /// The joints found without torque that it took hold of.
    pub torque_given: Vec<&'static str>,
    /// What waking did, if it had to.
    pub woke: Option<Woke>,
    /// Writes of the force limit, and the limit every joint it was written
    /// to read back with.
    pub force_writes: u8,
    pub force: Option<u16>,
    /// It left its folded pose at least once.
    pub stood: bool,
    pub arm_peak_load: [u16; 2],
    pub arm_peak_temperature: [u8; 2],
    /// Each time the arm was sent back down, and why.
    pub retreats: Vec<String>,
    /// Gestures begun, and how many of them were let go because a joint
    /// did not keep up or strained.
    pub gestures: u32,
    pub gestures_dropped: u32,
    /// Steps in which something asked by Expression was added to the pose.
    pub expression_steps: u64,
    /// Times it shifted by itself while standing at rest.
    pub idles: u32,
    /// Under `--overload release`, each time it let go.
    pub released: Vec<Released>,
    pub stop: Stop,
}

/// One time it let go under overload: the joint and the load that tripped
/// it, seconds into the run, and how it brought the head home afterwards.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Released {
    pub joint: &'static str,
    pub load: u16,
    pub seconds: f32,
    pub woke: Option<Woke>,
}

/// What the wake did: the joints it moved, where each was found and where
/// it brought it, in counts, and how long that took.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Woke {
    pub joints: Vec<&'static str>,
    pub from: Vec<u16>,
    pub to: Vec<u16>,
    /// Where each was when the wake ended: short of `to` by up to
    /// [`WAKE_SETTLE`] when it settled there.
    pub settled: Vec<u16>,
    pub seconds: f32,
}

/// Where a waking lamp has its three joints `elapsed` seconds into the
/// wake from `from` to `to`, and whether the wake is over. First a stir of
/// up to [`WAKE_STIR`] counts ([`WAKE_LIFT_LEAD`] for a head that lifts),
/// then [`WAKE_PAUSE`], then the rest of the way, each eased at both ends
/// and timed so that the joint with the farthest to go moves at
/// [`WAKE_SPEED`] at most. Every joint only ever moves from where it was
/// found toward where it goes.
fn wake_pose(from: [u16; 3], to: [u16; 3], elapsed: f32) -> ([f32; 3], bool) {
    let way: [f32; 3] = std::array::from_fn(|index| f32::from(to[index]) - f32::from(from[index]));
    let stir: [f32; 3] = std::array::from_fn(|index| {
        let most = if leads(from, to, index) {
            WAKE_LIFT_LEAD
        } else {
            WAKE_STIR
        };
        way[index].clamp(-most, most)
    });
    let rest: [f32; 3] = std::array::from_fn(|index| way[index] - stir[index]);
    // An eased move peaks at one and a half times its mean speed.
    let length =
        |moves: [f32; 3]| 1.5 * moves.iter().fold(0.0f32, |m, d| m.max(d.abs())) / WAKE_SPEED;
    let (first, last) = (length(stir), length(rest));
    let share = |time: f32, length: f32| {
        if length > 0.0 {
            smooth(time / length)
        } else {
            1.0
        }
    };
    let (stirred, risen) = (
        share(elapsed, first),
        share(elapsed - first - WAKE_PAUSE, last),
    );
    let pose = std::array::from_fn(|index| {
        f32::from(from[index]) + stir[index] * stirred + rest[index] * risen
    });
    (pose, elapsed >= first + WAKE_PAUSE + last)
}

/// The head's pitch, going to fewer counts: lifting, against its weight.
fn lifts(from: [u16; 3], to: [u16; 3], index: usize) -> bool {
    index == 0 && to[0] < from[0]
}

/// A joint whose goal must lead it to move at all while it wakes: the head
/// lifting, and the base either way. On lamp-4ace on 2026-10-14 (base gain
/// P 16), woken toward a home 286 counts round, the base turned 72 counts
/// and stuck at 2,101, 34 behind a goal moving at the wake's speed.
fn leads(from: [u16; 3], to: [u16; 3], index: usize) -> bool {
    lifts(from, to, index) || (index == 2 && to[2] != from[2])
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
/// The goal position (register 42, two bytes) of one servo. Sent only with
/// its own position: to a shoulder or an elbow found pushing under an old
/// overload, so that it stops, and to a joint it takes hold of.
fn goal_write(joint: Joint, value: u16) -> [u8; 9] {
    let [low, high] = value.to_le_bytes();
    let mut packet = [0xff, 0xff, joint.id(), 5, 0x03, 42, low, high, 0];
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

/// Torque enable (register 40, one byte). Sent only to clear an overload
/// latched on the shoulder or the elbow before this run, and to take hold
/// of a joint found without torque.
fn torque_write(joint: Joint, value: u8) -> [u8; 8] {
    let mut packet = [0xff, 0xff, joint.id(), 4, 0x03, 40, value, 0];
    packet[7] = checksum(&packet[2..7]);
    packet
}
/// The force limit (register 48, two bytes): the most a joint may push
/// with, of 1,000. The servo keeps it until it loses power, and starts
/// again with the limit in its register 16.
fn force_write(joint: Joint, value: u16) -> [u8; 9] {
    let [low, high] = value.to_le_bytes();
    let mut packet = [0xff, 0xff, joint.id(), 5, 0x03, 48, low, high, 0];
    packet[8] = checksum(&packet[2..8]);
    packet
}
/// One sync write of the goal position of all five joints: the three of
/// the head and base, then the shoulder and the elbow.
fn all_goals_write(goals: [u16; 3], arm: [u16; 2]) -> [u8; 23] {
    let mut packet = [0u8; 23];
    packet[..7].copy_from_slice(&[0xff, 0xff, 0xfe, 19, 0x83, 42, 2]);
    let joints = COMMANDED.iter().chain(&ARM);
    for (index, (joint, goal)) in joints.zip(goals.into_iter().chain(arm)).enumerate() {
        let [low, high] = goal.to_le_bytes();
        packet[7 + index * 3..10 + index * 3].copy_from_slice(&[joint.id(), low, high]);
    }
    packet[22] = checksum(&packet[2..22]);
    packet
}

/// One sync write of the goal position of the joints given, in that order.
fn some_goals_write(entries: &[(Joint, u16)]) -> Vec<u8> {
    let mut packet = vec![0xff, 0xff, 0xfe, (entries.len() * 3 + 4) as u8, 0x83, 42, 2];
    for (joint, goal) in entries {
        let [low, high] = goal.to_le_bytes();
        packet.extend([joint.id(), low, high]);
    }
    packet.push(checksum(&packet[2..]));
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
    /// Commanded joints let go under a hand: no goal is written to them,
    /// since a goal written to one of these servos turns its torque on.
    limp: [bool; 3],
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

    /// Torque, acceleration, goal speed and goal of the shoulder and elbow.
    fn arm_profiles(&mut self) -> Result<[(u8, u8, u16, u16); 2], String> {
        let request = ReadOnlyRequest::sync_read(&ARM, ReadWindow::TORQUE_AND_PROFILE)
            .map_err(|error| error.to_string())?;
        let mut profiles = [(0, 0, 0, 0); 2];
        self.read(&request, |reply| {
            let profile = reply
                .torque_and_profile()
                .map_err(|error| error.to_string())?;
            let index = ARM
                .iter()
                .position(|joint| *joint == reply.joint())
                .ok_or("a reply from a joint that was not asked")?;
            profiles[index] = (
                profile.torque_enable_raw(),
                profile.acceleration_raw(),
                profile.goal_speed_raw(),
                profile.goal_position_raw(),
            );
            Ok(())
        })?;
        Ok(profiles)
    }

    /// The force limit each of `joints` holds, in their order.
    fn force_limits(&mut self, joints: &[Joint]) -> Result<Vec<u16>, String> {
        let window = ReadWindow::new(48, 2).map_err(|error| error.to_string())?;
        let request =
            ReadOnlyRequest::sync_read(joints, window).map_err(|error| error.to_string())?;
        let mut limits = vec![0u16; joints.len()];
        self.read(&request, |reply| {
            let index = joints
                .iter()
                .position(|joint| *joint == reply.joint())
                .ok_or("a reply from a joint that was not asked")?;
            let [low, high] = reply.parameters() else {
                return Err("a force limit that is not two bytes".into());
            };
            limits[index] = u16::from_le_bytes([*low, *high]);
            Ok(())
        })?;
        Ok(limits)
    }

    /// The goals of the head and base, and with them the arm's for a body
    /// that may stand.
    fn send_goals(&mut self, goals: [u16; 3], arm: Option<[u16; 2]>) -> Result<(), String> {
        if self.limp.contains(&true) {
            let entries: Vec<(Joint, u16)> = COMMANDED
                .into_iter()
                .zip(goals)
                .zip(self.limp)
                .filter(|(_, limp)| !limp)
                .map(|(entry, _)| entry)
                .chain(arm.into_iter().flat_map(|arm| ARM.into_iter().zip(arm)))
                .collect();
            if entries.is_empty() {
                return Ok(());
            }
            return self.send(&some_goals_write(&entries));
        }
        match arm {
            Some(arm) => self.send(&all_goals_write(goals, arm)),
            None => self.send(&goals_write(goals)),
        }
    }

    /// One register write to one servo and its plain acknowledgement, which
    /// has the form of a ping's.
    fn write_checked(&mut self, joint: Joint, packet: &[u8]) -> Result<(), String> {
        self.write_acknowledged(joint, packet, false)
    }

    /// The same, and with `flagged` an acknowledgement that carries a
    /// status flag is still one: the write that is meant to clear a latched
    /// flag is answered with it.
    fn write_acknowledged(
        &mut self,
        joint: Joint,
        packet: &[u8],
        flagged: bool,
    ) -> Result<(), String> {
        self.send(packet)?;
        let ping = ReadOnlyRequest::ping(joint);
        let mut tracker = if flagged {
            ping.track_replies_reporting_faults()
        } else {
            ping.track_replies()
        };
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

/// Who watches the steps of a wake, and the step they are shown as.
type Watching<'a> = (&'a mut dyn FnMut(&Glimpse), Glimpse);

/// Bring the three joints from `from` to `to` along [`wake_pose`], the arm
/// held at `arm` for a body that may stand. Every step it reads all five
/// joints: a status byte that changes, a shoulder or an elbow that moves, a
/// commanded joint over its load or off its goal for as many steps as in
/// the run, or a wire that fails for half a second ends it. The wake goes
/// on only on steps whose write and read both went through, so a missed
/// step never makes the next goal jump. Returns what the joints read at
/// the end, and what it did. With `watching`, each step read is a step of
/// the run and is watched, as a body that is already running must be.
fn wake<P: PortIo, R: Runtime>(
    bus: &mut Bus<'_, P, R>,
    report: &mut BodyReport,
    limits: &Limits,
    (from, to): ([u16; 3], [u16; 3]),
    arm: Option<[u16; 2]>,
    found: [Sample; 5],
    mut watching: Option<Watching<'_>>,
) -> Result<[Sample; 5], Stop> {
    let started = bus.runtime.now();
    let (mut ticks, mut done) = (0u32, 0u32);
    let mut written = from;
    let (mut misses, mut over, mut off) = (0u8, [0u8; 3], [0u8; 3]);
    // How near each joint has come to its goal since the goal got home, the
    // steps since it last came nearer, and whether it has settled short.
    let (mut closest, mut still, mut settled) = ([u16::MAX; 3], [0u8; 3], [false; 3]);
    loop {
        ticks += 1;
        let due = started + TICK * ticks;
        if !matches!(bus.runtime.cancelled(), Ok(false)) {
            report.woke = Some(woke_from(
                (from, to, written),
                report.last,
                bus.runtime.now().saturating_sub(started),
            ));
            return Err(Stop::Cancelled);
        }
        let (pose, over_now) = wake_pose(from, to, TICK.as_secs_f32() * done as f32);
        let goals = pose.map(|goal| goal.round() as u16);
        let outcome = if goals == written {
            Ok(())
        } else {
            bus.send_goals(goals, arm).map(|()| {
                written = goals;
                report.goal_writes += 1;
                report.moved = true;
            })
        };
        match outcome.and_then(|()| bus.samples()) {
            Err(detail) => {
                report.missed_reads += 1;
                misses += 1;
                if misses >= WIRE_STEPS {
                    report.woke = Some(woke_from(
                        (from, to, written),
                        report.last,
                        bus.runtime.now().saturating_sub(started),
                    ));
                    return Err(Stop::Bus {
                        detail: format!("waking: {detail}"),
                    });
                }
            }
            Ok(now) => {
                misses = 0;
                report.last = Some(now);
                let mut surprise = None;
                if let Some((watch, glimpse)) = watching.as_mut() {
                    report.steps += 1;
                    watch(&glimpse_of(*glimpse, report.steps, &now, written));
                }
                for joint in Joint::ALL {
                    let (was, is) = (found[joint.index()], now[joint.index()]);
                    if is.status != was.status {
                        surprise.get_or_insert(Stop::Status {
                            joint: joint.id(),
                            found: was.status,
                            now: is.status,
                        });
                    }
                }
                // The wake never moves the arm, though it sways as the base
                // turns.
                for joint in ARM {
                    let (was, is) = (found[joint.index()], now[joint.index()]);
                    if was.position.abs_diff(is.position) > limits.bystander_drift.max(WAKE_SWAY) {
                        surprise.get_or_insert(Stop::Drift {
                            joint: joint.id(),
                            found: was.position,
                            now: is.position,
                        });
                    }
                }
                for (index, joint) in COMMANDED.iter().enumerate() {
                    let is = now[joint.index()];
                    report.peak_load[index] = report.peak_load[index].max(is.load);
                    let bound = if lifts(from, to, index) {
                        limits.load[index].max(WAKE_LIFT_LOAD)
                    } else {
                        limits.load[index]
                    };
                    over[index] = if is.load > bound {
                        over[index].saturating_add(1)
                    } else {
                        0
                    };
                    // A head lifting against its weight lags its goal: on
                    // the way it may lag up to WAKE_TRACKING. Once the goal
                    // is home, it must come within the run's own bound, or
                    // at least keep moving toward it.
                    let apart = written[index].abs_diff(is.position);
                    off[index] = if apart > WAKE_TRACKING {
                        off[index].saturating_add(1)
                    } else {
                        0
                    };
                    let nearer = apart + WAKE_MOVED <= closest[index];
                    if nearer || !over_now {
                        closest[index] = apart;
                        still[index] = 0;
                    } else if apart > limits.tracking {
                        still[index] = still[index].saturating_add(1);
                    }
                    // Stopped short, but near enough home: woken there.
                    if still[index] >= WAKE_STILL_STEPS && apart <= WAKE_SETTLE {
                        settled[index] = true;
                        still[index] = 0;
                    }
                    let stop = if over[index] >= limits.load_steps.max(1) {
                        Some(Stop::Load {
                            joint: joint.id(),
                            load: is.load,
                        })
                    } else if off[index] >= limits.tracking_steps
                        || still[index] >= WAKE_STILL_STEPS
                    {
                        Some(Stop::Tracking {
                            joint: joint.id(),
                            goal: written[index],
                            position: is.position,
                        })
                    } else {
                        None
                    };
                    surprise = surprise.or(stop);
                }
                let arrived = (0..3).all(|index| {
                    settled[index]
                        || written[index].abs_diff(now[COMMANDED[index].index()].position)
                            <= limits.tracking
                });
                if surprise.is_some() || (over_now && arrived) {
                    report.woke = Some(woke_from(
                        (from, to, written),
                        report.last,
                        bus.runtime.now().saturating_sub(started),
                    ));
                    return surprise.map_or(Ok(now), Err);
                }
                if !over_now {
                    done += 1;
                }
            }
        }
        bus.sleep_until(due);
    }
}

/// Lower a shoulder found up with its elbow folded onto the folded
/// shoulder count, eased at both ends at [`ARM_LOWER_SPEED`] at most, the
/// head, the base and the elbow held at `head` and `elbow`. Every step it
/// reads all five joints: a changed status byte, the head, the base or
/// the elbow moving more than `Limits::tracking`, the shoulder over its
/// load for `Limits::load_steps` steps or farther than
/// `Standing::TRACKING` from its goal for `Standing::TRACKING_STEPS`, or
/// a wire that fails for half a second ends it. Once its goal is folded
/// it waits for the shoulder to come within `Standing::FOUND_SLACK`; one
/// that stops farther away for half a second ends it too.
#[allow(clippy::too_many_arguments)]
fn lower_shoulder<P: PortIo, R: Runtime>(
    bus: &mut Bus<'_, P, R>,
    report: &mut BodyReport,
    limits: &Limits,
    up: Standing,
    head: [u16; 3],
    elbow: u16,
    found: [Sample; 5],
) -> Result<[Sample; 5], Stop> {
    let shoulder = Joint::BasePitch;
    let (from, to) = (found[shoulder.index()].position, up.folded[0]);
    let way = f32::from(to) - f32::from(from);
    let length = 1.5 * way.abs() / ARM_LOWER_SPEED;
    let started = bus.runtime.now();
    let (mut ticks, mut done, mut misses) = (0u32, 0u32, 0u8);
    let (mut written, mut over, mut off, mut still) = (from, 0u8, 0u8, 0u8);
    let mut closest = u16::MAX;
    let outcome = loop {
        ticks += 1;
        let due = started + TICK * ticks;
        if !matches!(bus.runtime.cancelled(), Ok(false)) {
            break Err(Stop::Cancelled);
        }
        let share = smooth(TICK.as_secs_f32() * done as f32 / length.max(TICK.as_secs_f32()));
        let goal = (f32::from(from) + way * share).round() as u16;
        let sent = if goal == written && done > 0 {
            Ok(())
        } else {
            bus.send(&all_goals_write(head, [goal, elbow])).map(|()| {
                written = goal;
                report.goal_writes += 1;
                report.moved = true;
            })
        };
        let now = match sent.and_then(|()| bus.samples()) {
            Ok(now) => now,
            Err(detail) => {
                report.missed_reads += 1;
                misses += 1;
                if misses >= WIRE_STEPS {
                    break Err(Stop::Bus {
                        detail: format!("lowering the shoulder: {detail}"),
                    });
                }
                bus.sleep_until(due);
                continue;
            }
        };
        misses = 0;
        report.last = Some(now);
        let mut surprise = None;
        for joint in Joint::ALL {
            let (was, is) = (found[joint.index()], now[joint.index()]);
            if is.status != was.status {
                surprise.get_or_insert(Stop::Status {
                    joint: joint.id(),
                    found: was.status,
                    now: is.status,
                });
            }
            if joint != shoulder && was.position.abs_diff(is.position) > limits.tracking {
                surprise.get_or_insert(Stop::Drift {
                    joint: joint.id(),
                    found: was.position,
                    now: is.position,
                });
            }
        }
        let is = now[shoulder.index()];
        report.arm_peak_load[0] = report.arm_peak_load[0].max(is.load);
        over = if is.load > up.load[0] { over + 1 } else { 0 };
        let apart = written.abs_diff(is.position);
        off = if apart > Standing::TRACKING {
            off + 1
        } else {
            0
        };
        let over_now = share >= 1.0;
        if apart + WAKE_MOVED <= closest || !over_now {
            (closest, still) = (apart, 0);
        } else {
            still += 1;
        }
        if over >= limits.load_steps.max(1) {
            surprise.get_or_insert(Stop::Load {
                joint: shoulder.id(),
                load: is.load,
            });
        } else if off >= Standing::TRACKING_STEPS
            || (still >= WAKE_STILL_STEPS && apart > Standing::FOUND_SLACK)
        {
            surprise.get_or_insert(Stop::Tracking {
                joint: shoulder.id(),
                goal: written,
                position: is.position,
            });
        }
        if let Some(surprise) = surprise {
            break Err(surprise);
        }
        if over_now && apart <= Standing::FOUND_SLACK {
            break Ok(now);
        }
        if !over_now {
            done += 1;
        }
        bus.sleep_until(due);
    };
    report.lowered = Some(Woke {
        joints: vec![shoulder.name()],
        from: vec![from],
        to: vec![written],
        settled: report
            .last
            .map_or_else(Vec::new, |last| vec![last[shoulder.index()].position]),
        seconds: bus.runtime.now().saturating_sub(started).as_secs_f32(),
    });
    outcome
}

/// With `--wake on`, one step of a commanded joint that may be held short
/// of its goal by its own weight: farther than `Limits::tracking` from
/// `goal` but within [`WAKE_SETTLE`], under its load bound (`over` is the
/// steps in a row it has read over it), the goal unchanged and the joint
/// not moving [`WAKE_MOVED`] counts. `steps` counts the steps it has been
/// so, from 1, and is 0 while it is not short in this way; after more than
/// [`WAKE_STILL_STEPS`] it rests where it is (true). On lamp-52e6 woken
/// with its shoulder up, the head held 30 to 37 counts short of goals of
/// 1,759 and 1,775 and the run ended on tracking three times in a row.
fn rests_short(
    limits: &Limits,
    (goal, position, over): (u16, u16, u8),
    at: &mut (u16, u16),
    steps: &mut u8,
) -> bool {
    let apart = goal.abs_diff(position);
    if !limits.wake || apart <= limits.tracking || apart > WAKE_SETTLE || over > 0 {
        *steps = 0;
        return false;
    }
    if *steps == 0 || at.0 != goal || at.1.abs_diff(position) >= WAKE_MOVED {
        (*at, *steps) = ((goal, position), 1);
        return false;
    }
    *steps = steps.saturating_add(1);
    if *steps > WAKE_STILL_STEPS {
        *steps = 0;
        return true;
    }
    false
}

/// Where the head and the base are at home with the arm `stand` of its way
/// up: the folded `home`, carried toward the standing pose with the head.
fn home_with(home: [u16; 3], standing: Option<Standing>, stand: f32) -> [f32; 3] {
    let folded = home.map(f32::from);
    match standing {
        Some(up) => {
            let share = head_share(stand);
            std::array::from_fn(|index| {
                folded[index] + (f32::from(up.pose[index]) - folded[index]) * share
            })
        }
        None => folded,
    }
}

/// What the wake did, as far as it got: the joints it set out to move,
/// where each was found, where its goal had got to and where it was last
/// read.
fn woke_from(
    (from, to, written): ([u16; 3], [u16; 3], [u16; 3]),
    last: Option<[Sample; 5]>,
    took: Duration,
) -> Woke {
    let moved: Vec<usize> = (0..3).filter(|index| from[*index] != to[*index]).collect();
    Woke {
        joints: moved.iter().map(|index| COMMANDED[*index].name()).collect(),
        from: moved.iter().map(|index| from[*index]).collect(),
        to: moved.iter().map(|index| written[*index]).collect(),
        settled: last.map_or_else(Vec::new, |last| {
            moved
                .iter()
                .map(|index| last[COMMANDED[*index].index()].position)
                .collect()
        }),
        seconds: took.as_secs_f32(),
    }
}

/// A step to watch that is not one of the run's own: `template` with what
/// the joints read now and the goals of the head and base in force.
fn glimpse_of(template: Glimpse, step: u64, now: &[Sample; 5], goals: [u16; 3]) -> Glimpse {
    Glimpse {
        step,
        mood: Mood::Resting,
        goals,
        positions: COMMANDED.map(|joint| now[joint.index()].position),
        loads: COMMANDED.map(|joint| now[joint.index()].load),
        temperatures: COMMANDED.map(|joint| now[joint.index()].temperature),
        arm_positions: ARM.map(|joint| now[joint.index()].position),
        arm_loads: ARM.map(|joint| now[joint.index()].load),
        arm_temperatures: ARM.map(|joint| now[joint.index()].temperature),
        look: [0.0; 2],
        face: None,
        gesture: None,
        ..template
    }
}

/// Take hold of each of `joints`, found without torque: its goal is
/// written as where it was `found`, then torque on, each acknowledged. A
/// servo whose torque went off keeps whatever goal it had, and torque on
/// alone would send it there. About 100 ms later all five are read; a
/// joint that moved more than `TAKE_HOLD_SLACK` counts is given where it
/// went as its goal, and that is a reason not to go on (`Stop::Refused`).
fn take_hold<P: PortIo, R: Runtime>(
    bus: &mut Bus<'_, P, R>,
    report: &mut BodyReport,
    joints: &[Joint],
    found: &[Sample; 5],
) -> Result<[Sample; 5], Stop> {
    let mut outcome = Ok(());
    for joint in joints {
        let at = found[joint.index()].position;
        // A flag is read and judged with the rest afterwards.
        if outcome.is_ok() {
            outcome = bus.write_acknowledged(*joint, &goal_write(*joint, at), true);
        }
        if outcome.is_ok() {
            report.torque_writes = report.torque_writes.saturating_add(1);
            report.torque_given.push(joint.name());
            outcome = bus.write_acknowledged(*joint, &torque_write(*joint, 1), true);
        }
    }
    // A few steps for a goal that was not taken to show.
    let now = outcome
        .and_then(|()| {
            let due = bus.runtime.now() + TICK * 5;
            bus.sleep_until(due);
            bus.samples()
        })
        .map_err(|detail| Stop::Bus {
            detail: format!("taking hold: {detail}"),
        })?;
    report.last = Some(now);
    let moved: Vec<(Joint, u16)> = joints
        .iter()
        .map(|joint| {
            let index = joint.index();
            (*joint, now[index].position.abs_diff(found[index].position))
        })
        .filter(|(_, by)| *by > TAKE_HOLD_SLACK)
        .collect();
    if let Some((joint, by)) = moved.first() {
        // Nothing keeps pushing a joint that went somewhere.
        for (joint, _) in &moved {
            let at = now[joint.index()].position;
            let _ = bus.write_acknowledged(*joint, &goal_write(*joint, at), true);
        }
        return Err(Stop::Refused {
            why: format!("{joint} moved {by} counts when its torque came on"),
        });
    }
    Ok(now)
}

/// Where a shoulder and an elbow at `arm` are on the way between folded
/// and stood up: how far up, 0 to 1, by the one that travels farther, and
/// how far each is off that point of the way, in counts.
fn on_the_way(up: Standing, arm: [u16; 2]) -> (f32, [f32; 2]) {
    let way = |index: usize| (f32::from(up.folded[index]), f32::from(up.pose[3 + index]));
    let leader = usize::from(up.folded[1].abs_diff(up.pose[4]) > up.folded[0].abs_diff(up.pose[3]));
    let (from, to) = way(leader);
    let lift = if (to - from).abs() < 1.0 {
        0.0
    } else {
        ((f32::from(arm[leader]) - from) / (to - from)).clamp(0.0, 1.0)
    };
    let off = [0, 1].map(|index| {
        let (from, to) = way(index);
        (f32::from(arm[index]) - (from + (to - from) * lift)).abs()
    });
    (lift, off)
}

/// Under `--overload release`: let go. Nothing more is asked of the joints:
/// torque off (register 40, 0) is written to all five, each acknowledged,
/// and one that failed is tried again on the next step. For `rest` from
/// then on it reads all five every step and acts on nothing: moods,
/// gestures and the eyes are taken and dropped, and no goal is written (a
/// goal written to one of these servos turns its torque back on). Returns
/// what the joints read once all five are let go and the rest is over.
/// Asked to stop, or a wire that fails for half a second, ends it with
/// torque off.
fn let_go<P: PortIo, R: Runtime>(
    bus: &mut Bus<'_, P, R>,
    report: &mut BodyReport,
    rest: Duration,
    (news, asked): (
        &mut impl FnMut() -> Option<(Mood, u8)>,
        &mut impl FnMut() -> Asked,
    ),
    watch: &mut impl FnMut(&Glimpse),
    template: Glimpse,
) -> Result<[Sample; 5], Stop> {
    let started = bus.runtime.now();
    let mut holding: Vec<Joint> = Joint::ALL.to_vec();
    let (mut ticks, mut misses) = (0u32, 0u8);
    loop {
        ticks += 1;
        let due = started + TICK * ticks;
        if !matches!(bus.runtime.cancelled(), Ok(false)) {
            return Err(Stop::Cancelled);
        }
        let _ = (news(), asked());
        let mut failed = None;
        holding.retain(|joint| {
            match bus.write_acknowledged(*joint, &torque_write(*joint, 0), true) {
                Ok(()) => {
                    report.torque_writes = report.torque_writes.saturating_add(1);
                    false
                }
                Err(detail) => {
                    failed.get_or_insert(detail);
                    true
                }
            }
        });
        report.steps += 1;
        let read = bus.samples();
        if let Ok(now) = &read {
            report.last = Some(*now);
            for (index, joint) in COMMANDED.iter().enumerate() {
                let is = now[joint.index()];
                report.peak_load[index] = report.peak_load[index].max(is.load);
                report.peak_temperature[index] = report.peak_temperature[index].max(is.temperature);
            }
            watch(&glimpse_of(template, report.steps, now, template.goals));
        }
        match (failed, read) {
            (None, Ok(now)) => {
                misses = 0;
                if holding.is_empty() && bus.runtime.now() - started >= rest {
                    return Ok(now);
                }
            }
            (Some(detail), _) | (None, Err(detail)) => {
                report.missed_reads += 1;
                misses += 1;
                if misses >= WIRE_STEPS {
                    return Err(Stop::Bus {
                        detail: format!("letting go: {detail}"),
                    });
                }
            }
        }
        bus.sleep_until(due);
    }
}

/// A commanded joint let go under a hand: whether its torque off has been
/// acknowledged, where it was last seen moving and the steps it has been
/// still since.
#[derive(Clone, Copy, Debug)]
struct Limp {
    acknowledged: bool,
    at: u16,
    still: u16,
}

/// One joint pushing at a goal it does not reach, under `--give-way`.
#[derive(Clone, Copy, Debug, Default)]
struct Stall {
    /// Steps in a row it has pushed without moving, and where it was when
    /// they began.
    steps: u16,
    at: u16,
    /// Told to stop pushing: the goal wanted of it then, and where it is
    /// held instead.
    hold: Option<(u16, u16)>,
}

impl Stall {
    /// One step of a joint whose goal is `goal` and of which `wanted` is
    /// wanted. True when it has just been told to stop pushing.
    fn step(&mut self, goal: u16, is: Sample, wanted: u16) -> bool {
        let pushing = goal.abs_diff(is.position) > STALL_SLACK && is.load > STALL_LOAD;
        if !pushing || self.steps == 0 || is.position.abs_diff(self.at) > STALL_SLACK {
            self.at = is.position;
            self.steps = u16::from(pushing);
            return false;
        }
        self.steps += 1;
        if self.steps < STALL_STEPS {
            return false;
        }
        self.steps = 0;
        self.hold = Some((wanted, is.position));
        true
    }

    /// Where it is held, while what is wanted of it has not moved
    /// `WAKE_COUNTS` from what was wanted when it stalled.
    fn holding(&mut self, wanted: u16) -> Option<u16> {
        match self.hold {
            Some((was, at)) if f32::from(wanted.abs_diff(was)) < WAKE_COUNTS => Some(at),
            _ => {
                self.hold = None;
                None
            }
        }
    }
}

/// Run the body until it is cancelled or something is not as expected.
/// `news` is asked once a step for the latest mood and never waits. `home`
/// is the pitch, roll and yaw every gesture is an offset from; without one
/// it is the pose found at start. With one, the head first eases there from
/// where it is found, at the same speed as everything else.
#[cfg(test)]
pub(crate) fn run(
    io: &mut impl PortIo,
    runtime: &mut impl Runtime,
    limits: Limits,
    home: Option<[u16; 3]>,
    news: impl FnMut() -> Option<(Mood, u8)>,
    watch: impl FnMut(&Glimpse),
) -> BodyReport {
    run_standing(io, runtime, limits, home, None, news, watch)
}

/// The same for a lamp that may also stand up: see [`Standing`]. It rises
/// when a mood other than rest arrives, stays up while they keep arriving
/// and for `Standing::awake` after the last, and folds down again; asked to
/// stop, it folds down first. A shoulder or an elbow that reads over its
/// load, does not follow or stays warm sends it back down, where it rests
/// for a minute. It needs a home, since where a standing lamp is found is
/// not its folded pose.
#[cfg(test)]
pub(crate) fn run_standing(
    io: &mut impl PortIo,
    runtime: &mut impl Runtime,
    limits: Limits,
    home: Option<[u16; 3]>,
    standing: Option<Standing>,
    news: impl FnMut() -> Option<(Mood, u8)>,
    watch: impl FnMut(&Glimpse),
) -> BodyReport {
    run_looking(
        io,
        runtime,
        limits,
        home,
        standing,
        None,
        news,
        || None,
        watch,
    )
}

/// The same for a lamp that also has eyes: see [`Looking`] and
/// [`crate::gaze`]. `eyes` is asked once a step for what the eyes last saw
/// and never waits. While the lamp listens or speaks, and is not folded or
/// folding, its head turns in small corrections to the face they report,
/// within the same reach as every gesture.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_looking(
    io: &mut impl PortIo,
    runtime: &mut impl Runtime,
    limits: Limits,
    home: Option<[u16; 3]>,
    standing: Option<Standing>,
    looking: Option<Looking>,
    news: impl FnMut() -> Option<(Mood, u8)>,
    mut eyes: impl FnMut() -> Option<Seen>,
    watch: impl FnMut(&Glimpse),
) -> BodyReport {
    let asked = move || Asked {
        seen: eyes(),
        gesture: None,
        expression: None,
    };
    run_alive(
        io, runtime, limits, home, standing, looking, news, asked, watch,
    )
}

/// The same for a lamp that also makes gestures: see [`crate::clips`].
/// `asked` is called once a step for what its eyes last saw and for a
/// gesture it has been asked to make, and never waits. A gesture plays only
/// on a lamp standing still at its full height; asked of one still rising
/// it waits up to three seconds, and of any other it is forgotten. Its
/// offsets are laid over whatever the lamp is doing and leave it where it
/// was. If a joint strains or falls behind while one plays, the gesture is
/// let go and the lamp carries on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_alive(
    io: &mut impl PortIo,
    runtime: &mut impl Runtime,
    limits: Limits,
    home: Option<[u16; 3]>,
    standing: Option<Standing>,
    looking: Option<Looking>,
    mut news: impl FnMut() -> Option<(Mood, u8)>,
    mut asked: impl FnMut() -> Asked,
    mut watch: impl FnMut(&Glimpse),
) -> BodyReport {
    let mut standing = standing;
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
        eased: [0; 3],
        kept_where_put: [0; 3],
        stalls: [0; 5],
        released_by_hand: [0; 3],
        rested_short: [0; 3],
        lowered: None,
        peak_temperature: [0; 3],
        held_at: None,
        last: None,
        standing: None,
        standing_refused: None,
        torque_writes: 0,
        torque_given: Vec::new(),
        woke: None,
        force_writes: 0,
        force: None,
        stood: false,
        arm_peak_load: [0; 2],
        arm_peak_temperature: [0; 2],
        retreats: Vec::new(),
        gestures: 0,
        gestures_dropped: 0,
        expression_steps: 0,
        idles: 0,
        released: Vec::new(),
        stop: Stop::Cancelled,
    };
    let mut bus = Bus {
        io,
        runtime,
        limp: [false; 3],
    };
    let refused = |why: String| Stop::Refused { why };

    // What is there, before anything is written.
    let mut found = match bus.samples() {
        Ok(found) => found,
        Err(detail) => {
            report.stop = refused(format!("first read: {detail}"));
            return report;
        }
    };
    report.found = Some(found);
    report.last = Some(found);
    let mut profiles = match bus.profiles() {
        Ok(profiles) => profiles,
        Err(detail) => {
            report.stop = refused(format!("first read: {detail}"));
            return report;
        }
    };
    if standing.is_some() && home.is_none() {
        standing = None;
        report.standing_refused = Some("a lamp that stands needs a home".into());
    }

    // Told to take hold: a joint found without torque, which would be a
    // reason not to start, is given it. Its goal is set to where it is
    // first, since a servo whose torque went off keeps whatever goal it
    // had, and torque on alone would send it there.
    if limits.torque_on {
        let mut without = Vec::new();
        for (index, joint) in COMMANDED.iter().enumerate() {
            if profiles[index].0 != 1 {
                without.push(*joint);
            }
        }
        // The shoulder and the elbow only of a lamp that may stand: of
        // any other they are not this body's to write to.
        if standing.is_some() {
            match bus.arm_profiles() {
                Ok(arm) => {
                    for (index, joint) in ARM.iter().enumerate() {
                        if arm[index].0 != 1 {
                            without.push(*joint);
                        }
                    }
                }
                Err(detail) => {
                    report.stop = refused(format!("first read: {detail}"));
                    return report;
                }
            }
        }
        if !without.is_empty() {
            found = match take_hold(&mut bus, &mut report, &without, &found) {
                Ok(now) => now,
                Err(stop) => {
                    report.stop = stop;
                    return report;
                }
            };
            report.found = Some(found);
            profiles = match bus.profiles() {
                Ok(profiles) => profiles,
                Err(detail) => {
                    report.stop = refused(format!("first read: {detail}"));
                    return report;
                }
            };
        }
    }
    let found_pose = COMMANDED.map(|joint| found[joint.index()].position);
    // Only a home that was given is one to wake to: without it, the home is
    // wherever the lamp was found.
    let may_wake = limits.wake && home.is_some();
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
        } else if sample.load > limits.start_load[index] {
            Some(format!("{joint} already carries load {}", sample.load))
        } else {
            None
        };
        if let Some(why) = why {
            report.stop = refused(why);
            return report;
        }
    }
    // The shoulder and the elbow of a lamp that may stand: as they are
    // found, and how far up that is.
    let mut stand = 0.0f32;
    let mut arm_written = ARM.map(|joint| found[joint.index()].position);
    let mut arm_profiles = [(0u8, 0u8, 0u16, 0u16); 2];
    if let Some(up) = standing {
        arm_profiles = match bus.arm_profiles() {
            Ok(profiles) => profiles,
            Err(detail) => {
                report.stop = refused(format!("first read: {detail}"));
                return report;
            }
        };
        let (mut lift, mut offs) = on_the_way(up, ARM.map(|joint| found[joint.index()].position));
        // A power cut on a standing lamp leaves the shoulder up on its gears
        // and lets the elbow fall folded: woken, such a shoulder is lowered
        // onto its folded count before anything else, and the lamp may then
        // stand as it would have. Any other arm off its way stays as it is.
        let slack = Standing::FOUND_SLACK;
        let [shoulder, elbow] = ARM.map(|joint| found[joint.index()]);
        let lowered = limits.wake
            && offs.iter().any(|off| *off > f32::from(slack))
            && shoulder.position.abs_diff(up.folded[0]) > slack
            && elbow.position.abs_diff(up.folded[1]) <= slack
            && (0..2).all(|index| {
                let sample = [shoulder, elbow][index];
                arm_profiles[index].0 == 1
                    && sample.status == 0
                    && sample.temperature < Standing::TEMPERATURE
            });
        if lowered {
            let head = COMMANDED.map(|joint| found[joint.index()].position);
            match lower_shoulder(
                &mut bus,
                &mut report,
                &limits,
                up,
                head,
                elbow.position,
                found,
            ) {
                Ok(now) => found = now,
                Err(stop) => {
                    if let Some(last) = report.last {
                        let at = |joint: Joint| last[joint.index()].position;
                        let hold = COMMANDED.map(at);
                        if bus.send(&all_goals_write(hold, ARM.map(at))).is_ok() {
                            report.goal_writes += 1;
                            report.held_at = Some(hold);
                        }
                    }
                    report.stop = stop;
                    return report;
                }
            }
            arm_profiles = match bus.arm_profiles() {
                Ok(profiles) => profiles,
                Err(detail) => {
                    report.stop = Stop::Bus {
                        detail: format!("after lowering the shoulder: {detail}"),
                    };
                    return report;
                }
            };
            (lift, offs) = on_the_way(up, ARM.map(|joint| found[joint.index()].position));
        }
        let mut why = None;
        for (index, joint) in ARM.iter().enumerate() {
            let sample = found[joint.index()];
            let off = offs[index];
            if arm_profiles[index].0 != 1 {
                why.get_or_insert(format!("{joint} torque is not on"));
            } else if sample.status & !StatusFlags::OVERLOAD != 0 {
                why.get_or_insert(format!("{joint} status is {:#04x}", sample.status));
            } else if sample.temperature >= Standing::TEMPERATURE {
                why.get_or_insert(format!("{joint} is at {} already", sample.temperature));
            } else if off > f32::from(Standing::FOUND_SLACK) {
                let mut said =
                    format!("{joint} is {off:.0} counts off the way between folded and stood up");
                // Which way down from there clears the head, the base and
                // the desk is not known, and these two carry the lamp.
                if limits.wake {
                    said.push_str("; waking does not lower an arm found off its way");
                }
                why.get_or_insert(said);
            }
        }
        match why {
            Some(why) => {
                standing = None;
                report.standing_refused = Some(why);
            }
            None => {
                // Under a twentieth of the way it is folded.
                stand = if lift < 0.05 { 0.0 } else { unsmooth(lift) };
                arm_written = [arm_profiles[0].3, arm_profiles[1].3];
            }
        }
    }
    let home_of = |standing: Option<Standing>, stand: f32| home_with(home, standing, stand);
    // The window beside the base is for a head that is beside the base.
    let beside_base = head_share(stand) <= 0.0;
    // Waking: a joint found where it may not start, but within WAKE_REACH
    // of its home, is brought there before anything else (below).
    let mut to_wake = false;
    for (pitch, woken) in [(home[0], false), (found_pose[0], may_wake)] {
        if beside_base && !(limits.pitch_home.0..=limits.pitch_home.1).contains(&pitch) {
            if woken {
                to_wake = true;
                continue;
            }
            report.stop = refused(format!(
                "head pitch {pitch} is outside {}..={}",
                limits.pitch_home.0, limits.pitch_home.1
            ));
            return report;
        }
    }
    let home_found = home_of(standing, stand);
    for (index, joint) in COMMANDED.iter().enumerate() {
        let apart = (f32::from(found_pose[index]) - home_found[index]).abs();
        // Found part of the way up, the head is wherever that way put it.
        let allowed = if beside_base {
            limits.start_reach
        } else {
            limits.start_reach.max(150)
        };
        if may_wake && apart > f32::from(allowed.max(WAKE_REACH)) {
            report.stop = refused(format!(
                "{joint} is {apart:.0} counts from its home, more than waking brings back ({WAKE_REACH})"
            ));
            return report;
        }
        if may_wake && apart > f32::from(allowed) {
            to_wake = true;
        } else if apart > f32::from(allowed) {
            report.stop = refused(format!("{joint} is {apart:.0} counts from its home"));
            return report;
        }
    }

    // An overload latched before this run leaves a servo pushing at its
    // goal with a fraction of its torque: on lamp-4ace the elbow read a
    // load of exactly 200 for a day, three counts short of its goal, and
    // writing torque alone did not end that. Such a joint cannot carry the
    // lamp up. So first it is told to stop pushing, its goal becoming where
    // it is; if the flag is still there after half a second torque is
    // written on, and then off and on. If it stays, the lamp stays folded.
    if standing.is_some() {
        for (index, joint) in ARM.into_iter().enumerate() {
            let flagged =
                |found: &[Sample; 5]| found[joint.index()].status & StatusFlags::OVERLOAD != 0;
            if !flagged(&found) {
                continue;
            }
            let at = found[joint.index()].position;
            // Stop pushing; then torque on; then torque off and on. After
            // each, a few steps for the flag to go.
            let attempts: [(bool, &[u8], u8); 3] =
                [(true, &[], 25), (false, &[1], 10), (false, &[0, 1], 10)];
            for (stop_pushing, torques, waits) in attempts {
                let mut outcome = Ok(());
                if stop_pushing {
                    outcome = bus.write_acknowledged(joint, &goal_write(joint, at), true);
                    arm_written[index] = at;
                }
                for value in torques {
                    if outcome.is_ok() {
                        report.torque_writes += 1;
                        outcome = bus.write_acknowledged(joint, &torque_write(joint, *value), true);
                    }
                }
                // The flag may take a moment to go once nothing is pushed.
                for _ in 0..waits {
                    if outcome.is_err() {
                        break;
                    }
                    let due = bus.runtime.now() + TICK;
                    bus.sleep_until(due);
                    match bus.samples() {
                        Ok(now) => {
                            found = now;
                            report.found = Some(found);
                            report.last = Some(found);
                            if !flagged(&found) {
                                break;
                            }
                        }
                        Err(detail) => outcome = Err(detail),
                    }
                }
                if let Err(detail) = outcome {
                    report.stop = Stop::Bus {
                        detail: format!("clearing the overload of {joint}: {detail}"),
                    };
                    return report;
                }
                if !flagged(&found) {
                    break;
                }
            }
            if found[joint.index()].status != 0 && standing.is_some() {
                standing = None;
                stand = 0.0;
                report.standing_refused = Some(format!(
                    "{joint} keeps status {:#04x}",
                    found[joint.index()].status
                ));
            }
        }
    }
    report.standing = standing;

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
    if let Some(up) = standing {
        // Fast enough for the way at its quickest, and no more than twice that.
        let quickest = up
            .folded
            .iter()
            .zip(&up.pose[3..])
            .map(|(from, to)| f32::from(from.abs_diff(*to)) * 1.5 / up.seconds)
            .fold(0.0f32, f32::max);
        // The way up is slow by itself and a gesture is quick: the servo's
        // own bound is there for neither to be outrun.
        let arm_speed = (quickest * 2.0).clamp(800.0, 1_000.0) as u16;
        for (index, joint) in ARM.iter().enumerate() {
            let (_, acceleration, speed, _) = arm_profiles[index];
            let mut outcome = Ok(());
            if speed != arm_speed {
                report.profile_writes += 1;
                outcome = bus.write_checked(*joint, &speed_write(*joint, arm_speed));
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
        match bus.arm_profiles() {
            Ok(now)
                if now.iter().all(|profile| {
                    (profile.0, profile.1, profile.2) == (1, limits.servo_acceleration, arm_speed)
                }) => {}
            Ok(now) => {
                report.stop = Stop::Bus {
                    detail: format!("arm profile read back as {now:?}"),
                };
                return report;
            }
            Err(detail) => {
                report.stop = Stop::Bus {
                    detail: format!("arm profile read back: {detail}"),
                };
                return report;
            }
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

    // The most the joints that carry the lamp may push with, if one was
    // chosen: a lamp that meets a hand or an edge gives way sooner, and
    // cannot push with more than this while this program is being slow to
    // notice. Written before anything moves, and read back.
    if let Some(force) = limits.force {
        let mut carrying = vec![Joint::BaseYaw];
        if standing.is_some() {
            carrying.extend(ARM);
        }
        for joint in &carrying {
            report.force_writes += 1;
            if let Err(detail) = bus.write_checked(*joint, &force_write(*joint, force)) {
                report.stop = Stop::Bus {
                    detail: format!("force limit of {joint}: {detail}"),
                };
                return report;
            }
        }
        match bus.force_limits(&carrying) {
            Ok(now) if now.iter().all(|held| *held == force) => report.force = Some(force),
            Ok(now) => {
                report.stop = Stop::Bus {
                    detail: format!("force limit read back as {now:?}"),
                };
                return report;
            }
            Err(detail) => {
                report.stop = Stop::Bus {
                    detail: format!("force limit read back: {detail}"),
                };
                return report;
            }
        }
    }

    // Woken from wherever it was found: brought home slowly, the way
    // something alive wakes, before the body takes it as found.
    let mut home = home;
    let found_pose = if to_wake {
        let to = home_found.map(|count| count.round() as u16);
        match wake(
            &mut bus,
            &mut report,
            &limits,
            (found_pose, to),
            standing.map(|_| arm_written),
            found,
            None,
        ) {
            // The run takes over the goals the wake left, home, where the
            // servos have them: a head that still lags its goal by a few
            // counts is not asked to drop back to where it is. A joint that
            // settled farther short than the run's own bound runs from where
            // it settled: its goal becomes where it is, so that nothing keeps
            // pushing, and its home moves there by as much, so that moods
            // are laid on the pose it has and the gap is not tried again.
            Ok(now) => {
                found = now;
                let mut goals = to;
                for (index, joint) in COMMANDED.iter().enumerate() {
                    let at = found[joint.index()].position;
                    if at.abs_diff(to[index]) > limits.tracking {
                        goals[index] = at;
                        let shifted = i32::from(home[index]) + i32::from(at) - i32::from(to[index]);
                        home[index] = shifted.clamp(0, 4_095) as u16;
                    }
                }
                if goals != to {
                    if let Err(detail) = bus.send_goals(goals, standing.map(|_| arm_written)) {
                        report.stop = Stop::Bus {
                            detail: format!("waking: {detail}"),
                        };
                        return report;
                    }
                    report.goal_writes += 1;
                    report.home = Some(home);
                }
                goals
            }
            Err(stop) => {
                // Stop pushing: each goal becomes where its joint is now.
                if let Some(last) = report.last {
                    let hold = COMMANDED.map(|joint| last[joint.index()].position);
                    if bus.send_goals(hold, standing.map(|_| arm_written)).is_ok() {
                        report.goal_writes += 1;
                        report.held_at = Some(hold);
                    }
                }
                report.stop = stop;
                return report;
            }
        }
    } else {
        found_pose
    };
    // Home as the wake left it; letting go under overload may move it again.

    // The reach around home, and the absolute window for the head's pitch
    // while the head is beside the base. A lamp that stands carries its
    // home with it, so these move as it rises. Sideways the eyes may take
    // the head further than any pose does: a person is where they are.
    let yaw_reach = limits
        .yaw
        .max(looking.map_or(0.0, |looking| looking.reach[0]));
    let bounds = |home: [f32; 3], stand: f32| -> ([f32; 3], [f32; 3]) {
        let (floor, ceiling) = if head_share(stand) > 0.0 {
            (f32::from(limits.pitch_floor), 4_095.0)
        } else {
            (
                f32::from(limits.pitch_window.0),
                f32::from(limits.pitch_window.1),
            )
        };
        // Folded, the head can dip only a little before it meets the base.
        // Stood up there is nothing under it, so it may look down as far as
        // it may look up; in between, in proportion to how clear it is.
        let clear = head_share(stand).powi(2);
        let down = limits.pitch_down + (limits.pitch_up - limits.pitch_down).max(0.0) * clear;
        let roll = limits.roll + (limits.stand_roll - limits.roll).max(0.0) * clear;
        (
            [
                (home[0] - limits.pitch_up).max(floor),
                home[1] - roll,
                home[2] - yaw_reach,
            ],
            [
                (home[0] + down).min(ceiling),
                home[1] + roll,
                home[2] + yaw_reach,
            ],
        )
    };
    let mut home_now = home_with(home, standing, stand);
    let (mut low, mut high) = bounds(home_now, stand);
    let mut stand_velocity = 0.0f32;
    let mut awake_until: Option<Duration> = None;
    let mut tired_until: Option<Duration> = None;
    let mut retreats = 0u8;
    let mut arm_over = [0u8; 2];
    let mut arm_off = [0u8; 2];
    let mut arm_hot = [0u8; 2];
    // A folded arm is not pushed at: once it has been down for a second its
    // goals become where its joints are, until it next rises. A goal a few
    // counts past where a folded joint can go is what latched the elbow's
    // overload on lamp-4ace.
    let mut arm_rests = stand <= 0.0;
    let mut down_steps = 0u16;
    // Where the eyes have it look, yaw and pitch in counts, and for how
    // many steps no joint has been asked to move.
    let mut gaze = looking.map(Gaze::new);
    let mut look = [0.0f32; 2];
    let mut still_steps = 0u32;
    // The gesture being made and one waiting to be, each with when; the
    // offsets it has the five joints at; and steps left of the allowance a
    // gesture is given.
    let mut playing: Option<(&'static Clip, Duration)> = None;
    let mut pending: Option<(&'static Clip, Duration)> = None;
    let mut gesture_now = [0.0f32; 5];
    let mut gesture_cooling = 0u16;
    // Looks in a row in which its eyes reported a face.
    let mut faces_running = 0u8;
    // When a lamp standing at rest next shifts by itself.
    let mut idle_due: Option<Duration> = None;
    let step = TICK.as_secs_f32();
    // A look away begins with a glance of GLANCE counts of the base's
    // turn: within the folded reach that is all of it, as it always was.
    let mut gestures = Gestures::new().glancing(GLANCE / limits.yaw.max(1.0));
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
    // With `--wake on`: the goal and the position a joint short of its goal
    // was last seen at, and the steps since, while it stays short.
    let mut short_at = [(0u16, 0u16); 3];
    let mut short_steps = [0u8; 3];
    // Steps left for which a joint that met resistance is not asked to move,
    // and the steps on which any joint last did.
    let mut easing = [0u16; 3];
    let mut eased_at = [0u64; EASES_ALLOWED];
    // Under `--give-way`: where each joint last gave way, steps in a row a
    // hand has pushed it since, the offset from home a hand left each where
    // it put it, and each of the five joints' pushing without moving; each
    // commanded joint's contact floor (V1's), steps in a row over it, and
    // the joints a hand has left limp.
    let mut gave_at = [0u16; 3];
    let mut in_hand = [0u16; 3];
    let mut kept = [0.0f32; 3];
    let mut stalls = [Stall::default(); 5];
    let contact_floor = [
        CONTACT_FLOOR[0],
        CONTACT_FLOOR[1],
        report
            .force
            .map_or(CONTACT_FLOOR[2], |force| force * CONTACT_YAW_SHARE / 100),
    ];
    let mut contact = [0u8; 3];
    let mut limp: [Option<Limp>; 3] = [None; 3];
    let mut misses = 0u8;
    let mut unsent = 0u8;
    let mut leaving: Option<Duration> = None;
    let mut resting_steps = 0u16;
    let mut relaxed = false;
    // A joint that has arrived and is asked nothing until the wanted pose
    // moves far enough from it; the steps each has been arriving for; and
    // the pose last wanted of each.
    let mut asleep = [false; 3];
    let mut arriving = [0u8; 3];
    let mut wanted_of = commanded;
    // Under `--overload release`: the load at which each of the five
    // (in servo order) trips the cut-off, steps in a row each has read
    // it, when it last let go, and whether the joints are to be left as
    // they are when the run ends (torque off, or already held). Taken up
    // again, an arm that slumped stays where it is until it is next asked
    // to stand.
    let trip_at = Joint::ALL.map(|joint| match report.force {
        Some(force) if joint == Joint::BaseYaw || (standing.is_some() && ARM.contains(&joint)) => {
            force * OVERLOAD_SHARE / 100
        }
        _ => OVERLOAD_LOAD,
    });
    let mut overloaded = [0u16; 5];
    let mut let_go_at: Option<Duration> = None;
    let mut left_as_is = false;
    let mut slumped = false;
    let mut started = bus.runtime.now();
    // The step the schedule counts from: it starts again after a rest.
    let mut first_step = 0u64;
    if let Some(up) = standing
        && stand > 0.0
    {
        // Found standing: it stays up as if it had just been spoken to.
        awake_until = Some(started + up.awake);
    }

    let stop = loop {
        report.steps += 1;
        let due = started + TICK * ((report.steps - first_step).min(u64::from(u32::MAX)) as u32);
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
        // What its eyes saw and a gesture asked of it, since the last step.
        let asked_now = asked();
        // Somebody in front of it: a face in several looks running.
        match asked_now.seen {
            Some(Seen { face: Some(_) }) if looking.is_some() => {
                faces_running = faces_running.saturating_add(1);
            }
            Some(Seen { face: None }) => faces_running = 0,
            _ => {}
        }
        let company = faces_running >= COMPANY_SIGHTS;
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
        // The wire is failing: nothing new is asked until it answers again.
        let blind = misses > 0 || unsent > 0;
        // How far up it should be, and this step of the way there. The
        // head's home goes with it, and the head with its home.
        let mut carried = [0.0f32; 3];
        let mut arm_goals = arm_written;
        // Taken up again after it let go, the arm stays where it slumped
        // until something asks the lamp to stand.
        if slumped && leaving.is_none() && (mood != Mood::Resting || company) {
            slumped = false;
        }
        // A lamp that is folded, or folding, looks at nobody.
        let mut awake_now = leaving.is_none() && !slumped;
        if let Some(up) = standing.filter(|_| !slumped) {
            // Spoken to, or with somebody sitting in front of it: awake.
            // It does not fold away from a person who is still there.
            if mood != Mood::Resting || company {
                awake_until = Some(now + up.awake);
            }
            let tired = retreats >= Standing::RETREATS_ALLOWED
                || tired_until.is_some_and(|until| now < until);
            let awake = awake_until.is_some_and(|until| now < until);
            let wanted = if leaving.is_none() && awake && !tired {
                up.share
            } else {
                0.0
            };
            awake_now = wanted > 0.0;
            if !blind {
                let rate = 1.0 / up.seconds;
                let pull = FOLLOW * FOLLOW * (wanted - stand) - 2.0 * FOLLOW * stand_velocity;
                stand_velocity = (stand_velocity + pull.clamp(-rate * 2.0, rate * 2.0) * step)
                    .clamp(-rate, rate);
                stand = (stand + stand_velocity * step).clamp(0.0, 1.0);
                if (wanted - stand).abs() < 0.000_5 && stand_velocity.abs() < 0.005 {
                    stand = wanted;
                    stand_velocity = 0.0;
                }
            }
            if stand > 0.05 {
                report.stood = true;
            }
            let lift = smooth(stand);
            down_steps = if stand <= 0.0 && stand_velocity == 0.0 {
                down_steps.saturating_add(1)
            } else {
                0
            };
            if down_steps == 0 {
                arm_rests = false;
            } else if down_steps >= REST_STEPS
                && !arm_rests
                && !blind
                && let Some(last) = report.last
            {
                arm_rests = true;
                arm_goals = ARM.map(|joint| last[joint.index()].position);
            }
            if !arm_rests {
                arm_goals = std::array::from_fn(|index| {
                    let (from, to) = (f32::from(up.folded[index]), f32::from(up.pose[3 + index]));
                    (from + (to - from) * lift).round() as u16
                });
            }
            let next = home_with(home, standing, stand);
            carried = std::array::from_fn(|index| next[index] - home_now[index]);
            home_now = next;
            (low, high) = bounds(home_now, stand);
            if stand_velocity != 0.0 || wanted != stand {
                // On its way up or down nothing of the head is left resting
                // where it was: it has to come along.
                resting_steps = 0;
                asleep = [false; 3];
            }
        }
        if resting_steps == 0 {
            relaxed = false;
        }
        // A gesture is for a lamp standing still at its full height.
        let upright = leaving.is_none()
            && !blind
            && standing.is_some_and(|up| up.share >= 1.0 && stand >= 1.0 && stand_velocity == 0.0);
        // Anything alive that stands with nothing to do shifts a little
        // now and then. It stops the moment there is something to do.
        let idling = playing.is_some_and(|(clip, _)| clip.name == clips::IDLE.name);
        if !(upright && mood == Mood::Resting && pending.is_none()) {
            idle_due = None;
            if idling {
                playing = None;
            }
        } else if playing.is_none() {
            match idle_due {
                None => idle_due = Some(now + IDLE_AFTER),
                Some(due) if now >= due && standing.is_some_and(|up| up.idle) => {
                    playing = Some((&clips::IDLE, now));
                    report.idles += 1;
                    idle_due =
                        Some(now + Duration::from_micros(clips::IDLE.duration_us()) + IDLE_EVERY);
                }
                Some(_) => {}
            }
        }
        if let Some(clip) = asked_now.gesture
            && (playing.is_none() || idling)
        {
            pending = Some((clip, now));
        }
        if let Some((clip, since)) = pending {
            if upright {
                playing = Some((clip, now));
                pending = None;
                report.gestures += 1;
            } else if now.saturating_sub(since) >= GESTURE_WAIT {
                pending = None;
            }
        }
        let mut gesture_wanted = [0.0f32; 5];
        if let Some((clip, since)) = playing {
            let elapsed = now.saturating_sub(since).as_micros();
            match clip.at(elapsed.try_into().unwrap_or(u64::MAX)) {
                Some(offsets) if upright => gesture_wanted = offsets,
                _ => playing = None,
            }
        }
        for (index, now_at) in gesture_now.iter_mut().enumerate() {
            let wanted = gesture_wanted[index].clamp(-GESTURE_REACH[index], GESTURE_REACH[index]);
            *now_at += (wanted - *now_at).clamp(-GESTURE_STEP[index], GESTURE_STEP[index]);
        }
        let gesturing = playing.is_some() || gesture_now != [0.0; 5];
        gesture_cooling = if gesturing {
            GESTURE_COOL_STEPS
        } else {
            gesture_cooling.saturating_sub(1)
        };
        if gesturing {
            // Nothing of the head is left resting: it has to make the move.
            resting_steps = 0;
            relaxed = false;
            asleep = [false; 3];
            for (goal, offset) in arm_goals.iter_mut().zip(&gesture_now[3..]) {
                *goal = (f32::from(*goal) + offset).round().clamp(0.0, 4_095.0) as u16;
            }
        }
        let mut face = None;
        if let Some(gaze) = gaze.as_mut() {
            let seen = asked_now.seen;
            face = seen
                .and_then(|seen| seen.face)
                .map(|sight| [sight.x, sight.y, sight.size]);
            look = gaze.step(
                seen,
                mood,
                awake_now,
                u64::from(still_steps) * TICK.as_micros() as u64,
                now.as_micros().try_into().unwrap_or(u64::MAX),
            );
        }
        // Each axis of a pose is a share of its reach, and where the eyes
        // have it look is added to that, inside the same reach. The head's
        // tilt has the reach of a lamp that has stood up, as far as it has.
        let roll_reach =
            limits.roll + (limits.stand_roll - limits.roll).max(0.0) * head_share(stand).powi(2);
        // What Expression asks is added to the pose the mood wants, and so
        // goes through everything that pose goes through: the reach of
        // each joint, the speed and the acceleration the pose follows at,
        // rest, and the hold after resistance. Not while a gesture plays:
        // that is the whole lamp's move, and nothing is laid over it.
        let extra = match asked_now.expression {
            Some(asked) if !gesturing => {
                report.expression_steps += 1;
                [0, 1, 2].map(|index| {
                    let asked = if asked[index].is_finite() {
                        asked[index]
                    } else {
                        0.0
                    };
                    asked.clamp(-EXPRESSION_REACH[index], EXPRESSION_REACH[index])
                })
            }
            _ => [0.0; 3],
        };
        let offsets = [
            pitch
                * if pitch < 0.0 {
                    limits.pitch_up
                } else {
                    limits.pitch_down
                }
                + look[1]
                + extra[0],
            roll * roll_reach + extra[1],
            yaw * limits.yaw + look[0] + extra[2],
        ];
        // A joint that met resistance is asked nothing for now, at rest or not.
        let held = easing.map(|left| left > 0);
        for left in &mut easing {
            *left = left.saturating_sub(1);
        }
        // Asked to stop, it goes home: not to where a hand put it.
        let hand = limits.give_way.is_some() && leaving.is_none();
        if leaving.is_some() {
            kept = [0.0; 3];
            stalls.iter_mut().for_each(|stall| stall.hold = None);
        }
        for (index, offset) in offsets.into_iter().enumerate() {
            if blind {
                continue;
            }
            if held[index] {
                velocity[index] = 0.0;
                goals[index] = commanded[index].round() as u16;
                continue;
            }
            if relaxed {
                continue;
            }
            commanded[index] += carried[index];
            let wanted = (home_now[index] + kept[index] + offset).clamp(low[index], high[index]);
            wanted_of[index] = wanted;
            if asleep[index] {
                if (wanted - commanded[index]).abs() < WAKE_COUNTS && leaving.is_none() {
                    continue;
                }
                asleep[index] = false;
            }
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
            // A gesture is laid over the pose, outside what follows the mood.
            let least = if index == 0 {
                f32::from(limits.pitch_floor)
            } else {
                0.0
            };
            // A head that does not tilt when it stands does not tilt in a
            // gesture either: on lamp-4ace that joint turns a round head
            // about its own axis, which nobody sees.
            let over = if index == 1 && limits.stand_roll <= limits.roll {
                0.0
            } else {
                gesture_now[index]
            };
            goals[index] = (commanded[index] + over).round().clamp(least, 4_095.0) as u16;
        }
        // A joint told to stop pushing is held where it was until something
        // else is wanted of it.
        if limits.give_way.is_some() && !blind {
            for index in 0..3 {
                if let Some(at) = stalls[index].holding(wanted_of[index].round() as u16) {
                    goals[index] = at;
                    commanded[index] = f32::from(at);
                    velocity[index] = 0.0;
                }
            }
            if standing.is_some() {
                for index in 0..2 {
                    if let Some(at) = stalls[3 + index].holding(arm_goals[index]) {
                        arm_goals[index] = at;
                    }
                }
            }
        }
        still_steps = if goals != written || arm_goals != arm_written {
            0
        } else {
            still_steps.saturating_add(1)
        };
        if goals != written || arm_goals != arm_written {
            match bus.send_goals(goals, standing.map(|_| arm_goals)) {
                Ok(()) => {
                    unsent = 0;
                    written = goals;
                    arm_written = arm_goals;
                    report.goal_writes += 1;
                    report.moved = true;
                }
                Err(detail) => {
                    report.missed_reads += 1;
                    unsent += 1;
                    // A step not read breaks a run of overloaded ones.
                    overloaded = [0; 5];
                    if unsent >= WIRE_STEPS {
                        break Stop::Bus { detail };
                    }
                    bus.sleep_until(due);
                    continue;
                }
            }
        }
        let samples = match bus.samples() {
            Ok(samples) => {
                misses = 0;
                samples
            }
            Err(detail) => {
                report.missed_reads += 1;
                misses += 1;
                overloaded = [0; 5];
                if misses >= WIRE_STEPS {
                    break Stop::Bus { detail };
                }
                bus.sleep_until(due);
                continue;
            }
        };
        // A write below that fails changes nothing and is tried again.
        let mut failed: Option<String> = None;
        report.last = Some(samples);
        if resting_steps >= REST_STEPS && !relaxed {
            let positions = COMMANDED.map(|joint| samples[joint.index()].position);
            let near = (0..3).all(|index| {
                bus.limp[index]
                    || positions[index].abs_diff(written[index]) <= REST_SLACK
                        && velocity[index].abs() < 3.0
            });
            if near {
                let sent = positions == written
                    || match bus.send_goals(positions, standing.map(|_| arm_written)) {
                        Ok(()) => {
                            written = positions;
                            report.goal_writes += 1;
                            true
                        }
                        Err(detail) => {
                            failed = Some(detail);
                            false
                        }
                    };
                if sent {
                    relaxed = true;
                    commanded = positions.map(f32::from);
                    velocity = [0.0; 3];
                }
            }
        }
        // A joint that has arrived stops being pushed at, whatever the mood.
        let mut rested = written;
        let mut arrived: [Option<f32>; 3] = [None; 3];
        for index in 0..3 {
            let there = !asleep[index]
                && !held[index]
                && !relaxed
                && gesture_cooling == 0
                && leaving.is_none()
                && (wanted_of[index] - commanded[index]).abs() < ARRIVED_COUNTS
                && velocity[index].abs() < ARRIVED_SPEED;
            arriving[index] = if there {
                arriving[index].saturating_add(1)
            } else {
                0
            };
            let position = samples[COMMANDED[index].index()].position;
            if arriving[index] >= ARRIVED_STEPS && position.abs_diff(written[index]) <= REST_SLACK {
                let at = f32::from(position).clamp(
                    low[index].min(commanded[index]),
                    high[index].max(commanded[index]),
                );
                arrived[index] = Some(at);
                rested[index] = at.round() as u16;
            }
        }
        let sent = failed.is_none()
            && (rested == written
                || match bus.send_goals(rested, standing.map(|_| arm_written)) {
                    Ok(()) => {
                        written = rested;
                        report.goal_writes += 1;
                        true
                    }
                    Err(detail) => {
                        failed = Some(detail);
                        false
                    }
                });
        if sent {
            for index in 0..3 {
                if let Some(at) = arrived[index] {
                    asleep[index] = true;
                    arriving[index] = 0;
                    commanded[index] = at;
                    velocity[index] = 0.0;
                }
            }
        }
        match failed {
            Some(detail) => {
                report.missed_reads += 1;
                unsent += 1;
                if unsent >= WIRE_STEPS {
                    break Stop::Bus { detail };
                }
            }
            None => unsent = 0,
        }
        let glimpse = Glimpse {
            step: report.steps,
            mood,
            goals: written,
            positions: COMMANDED.map(|joint| samples[joint.index()].position),
            loads: COMMANDED.map(|joint| samples[joint.index()].load),
            temperatures: COMMANDED.map(|joint| samples[joint.index()].temperature),
            eased: report.eased,
            stands: standing.is_some(),
            stand,
            arm_goals: arm_written,
            arm_positions: ARM.map(|joint| samples[joint.index()].position),
            arm_loads: ARM.map(|joint| samples[joint.index()].load),
            arm_temperatures: ARM.map(|joint| samples[joint.index()].temperature),
            look,
            face,
            gesture: playing.map(|(clip, _)| clip.name),
            kept_where_put: report.kept_where_put,
            stalls: report.stalls,
            released: report.released.len().try_into().unwrap_or(u16::MAX),
            limp: false,
            hand_limp: bus.limp,
        };
        watch(&glimpse);
        // A joint pushing at a goal it does not reach, and not moving, is
        // told to stop: its goal becomes where it is. Never torque off.
        // The joints told so on this step, in servo order.
        let mut stopped_pushing = [false; 5];
        if limits.give_way.is_some() && leaving.is_none() {
            for index in 0..3 {
                let is = samples[COMMANDED[index].index()];
                if stalls[index].step(written[index], is, wanted_of[index].round() as u16) {
                    report.stalls[index] = report.stalls[index].saturating_add(1);
                    stopped_pushing[COMMANDED[index].index()] = true;
                }
            }
            if standing.is_some() {
                for (index, joint) in ARM.iter().enumerate() {
                    let is = samples[joint.index()];
                    if stalls[3 + index].step(arm_written[index], is, arm_written[index]) {
                        report.stalls[3 + index] = report.stalls[3 + index].saturating_add(1);
                        stopped_pushing[joint.index()] = true;
                    }
                }
            }
        }
        let mut surprise = None;
        for joint in Joint::ALL {
            let (was, is) = (found[joint.index()], samples[joint.index()]);
            // Torque off may change what a servo flags: a joint a hand left
            // limp is judged again once it is taken hold of.
            let limp_now = COMMANDED
                .iter()
                .position(|commanded| *commanded == joint)
                .is_some_and(|index| bus.limp[index]);
            if is.status != was.status && !limp_now {
                surprise.get_or_insert(Stop::Status {
                    joint: joint.id(),
                    found: was.status,
                    now: is.status,
                });
            }
        }
        for joint in ARM {
            let (was, is) = (found[joint.index()], samples[joint.index()]);
            if standing.is_none() && was.position.abs_diff(is.position) > limits.bystander_drift {
                surprise.get_or_insert(Stop::Drift {
                    joint: joint.id(),
                    found: was.position,
                    now: is.position,
                });
            }
        }
        if let Some(up) = standing {
            for (index, joint) in ARM.iter().enumerate() {
                let is = samples[joint.index()];
                report.arm_peak_load[index] = report.arm_peak_load[index].max(is.load);
                report.arm_peak_temperature[index] =
                    report.arm_peak_temperature[index].max(is.temperature);
                arm_over[index] = if is.load > up.load[index] {
                    arm_over[index].saturating_add(1)
                } else {
                    0
                };
                let astray = if gesture_cooling > 0 {
                    GESTURE_ARM_TRACKING
                } else {
                    Standing::TRACKING
                };
                arm_off[index] = if arm_written[index].abs_diff(is.position) > astray {
                    arm_off[index].saturating_add(1)
                } else {
                    0
                };
                arm_hot[index] = if is.temperature >= Standing::TEMPERATURE {
                    arm_hot[index].saturating_add(1)
                } else {
                    0
                };
                let why = if arm_over[index] >= limits.load_steps.max(1) {
                    Some(format!("{joint} load {}", is.load))
                } else if arm_off[index] >= Standing::TRACKING_STEPS {
                    Some(format!(
                        "{joint} at {} for goal {}",
                        is.position, arm_written[index]
                    ))
                } else if arm_hot[index] >= limits.temperature_steps.max(1) {
                    Some(format!("{joint} at {} degrees", is.temperature))
                } else {
                    None
                };
                let Some(why) = why else { continue };
                playing = None;
                // A living thing that cannot hold itself up sits down. It
                // goes back the way it came and rests there. Only an arm
                // still strained or astray two seconds on ends the run.
                let tired = retreats >= Standing::RETREATS_ALLOWED
                    || tired_until.is_some_and(|until| now < until);
                if !tired && stand > 0.0 {
                    retreats += 1;
                    if report.retreats.len() < 16 {
                        report
                            .retreats
                            .push(format!("step {}: {why}", report.steps));
                    }
                }
                tired_until = Some(now + Standing::TIRED);
                if arm_over[index] >= Standing::STUCK_STEPS {
                    surprise.get_or_insert(Stop::Load {
                        joint: joint.id(),
                        load: is.load,
                    });
                } else if arm_off[index] >= Standing::STUCK_STEPS {
                    surprise.get_or_insert(Stop::Tracking {
                        joint: joint.id(),
                        goal: arm_written[index],
                        position: is.position,
                    });
                }
            }
        }
        // A joint a hand left limp and then let be, to take hold of again.
        let mut retake: Option<usize> = None;
        for (index, joint) in COMMANDED.iter().enumerate() {
            let is = samples[joint.index()];
            report.peak_load[index] = report.peak_load[index].max(is.load);
            let (strained, astray) = if gesture_cooling > 0 {
                (
                    limits.load[index].max(GESTURE_LOAD),
                    limits.tracking.max(GESTURE_TRACKING),
                )
            } else {
                (limits.load[index], limits.tracking)
            };
            over_load[index] = if is.load > strained {
                over_load[index].saturating_add(1)
            } else {
                0
            };
            off_track[index] = if written[index].abs_diff(is.position) > astray {
                off_track[index].saturating_add(1)
            } else {
                0
            };
            // Let go under a hand: read every step and written nothing, so
            // nothing about it ends the run. Still for two seconds, it is
            // taken hold of where it is (below), but not while leaving.
            if let Some(limp) = limp[index].as_mut() {
                (over_load[index], off_track[index], contact[index]) = (0, 0, 0);
                easing[index] = EASE_STEPS;
                if !limp.acknowledged
                    && bus
                        .write_acknowledged(*joint, &torque_write(*joint, 0), true)
                        .is_ok()
                {
                    limp.acknowledged = true;
                    report.torque_writes = report.torque_writes.saturating_add(1);
                }
                if is.position.abs_diff(limp.at) > LIMP_STILL {
                    (limp.at, limp.still) = (is.position, 0);
                } else {
                    limp.still = limp.still.saturating_add(1);
                }
                if hand && limp.acknowledged && limp.still >= LIMP_STEPS {
                    retake.get_or_insert(index);
                }
                continue;
            }
            // V1's contact floor, for three steps: a hand, whatever the
            // joint's own bound has seen.
            contact[index] = if limits.give_way.is_some() && is.load > contact_floor[index] {
                contact[index].saturating_add(1)
            } else {
                0
            };
            let touched = hand && contact[index] >= CONTACT_STEPS;
            if gesture_cooling > 0
                && !touched
                && (over_load[index] >= limits.load_steps.max(1)
                    || off_track[index] >= limits.tracking_steps)
            {
                // The gesture asked more of this joint than it gave: the
                // gesture is let go, and the lamp goes on as it was.
                if playing.take().is_some() {
                    report.gestures_dropped += 1;
                }
                over_load[index] = 0;
                off_track[index] = 0;
                continue;
            }
            report.peak_temperature[index] = report.peak_temperature[index].max(is.temperature);
            hot[index] = if is.temperature >= limits.temperature {
                hot[index].saturating_add(1)
            } else {
                0
            };
            if hand && held[index] {
                // It gave way, and a hand may be on it. As V1: still pushed
                // a second on, by load over the lower of the bound that
                // stopped it and its contact floor, or held a degree off
                // where it stopped, it lets go of that joint alone. A load
                // and a tracking that would end the run are what a hand
                // looks like, so nothing else is judged meanwhile.
                over_load[index] = 0;
                off_track[index] = 0;
                let pushed = is.load > strained.min(contact_floor[index])
                    || is.position.abs_diff(gave_at[index]) > HAND_MOVED;
                in_hand[index] = if pushed { in_hand[index] + 1 } else { 0 };
                if in_hand[index] >= HAND_STEPS {
                    in_hand[index] = 0;
                    bus.limp[index] = true;
                    let acknowledged = bus
                        .write_acknowledged(*joint, &torque_write(*joint, 0), true)
                        .is_ok();
                    if acknowledged {
                        report.torque_writes = report.torque_writes.saturating_add(1);
                    }
                    limp[index] = Some(Limp {
                        acknowledged,
                        at: is.position,
                        still: 0,
                    });
                    easing[index] = EASE_STEPS;
                    report.released_by_hand[index] =
                        report.released_by_hand[index].saturating_add(1);
                }
                if hot[index] >= limits.temperature_steps.max(1) {
                    surprise.get_or_insert(Stop::Temperature {
                        joint: joint.id(),
                        temperature: is.temperature,
                    });
                }
                continue;
            }
            if over_load[index] >= limits.load_steps.max(1) || touched {
                // Something resists. A living thing stops pushing; it does
                // not faint. The joint's goal becomes where it is and it is
                // left there for two seconds. Loaded still with nothing
                // asked of it, or resisted for the fourth time in a minute,
                // and the run ends as it always did.
                let often = eased_at
                    .iter()
                    .all(|at| *at != 0 && report.steps - *at < EASE_MEMORY_STEPS);
                if held[index] || often {
                    surprise.get_or_insert(Stop::Load {
                        joint: joint.id(),
                        load: is.load,
                    });
                } else {
                    commanded[index] = f32::from(is.position).clamp(
                        low[index].min(commanded[index]),
                        high[index].max(commanded[index]),
                    );
                    velocity[index] = 0.0;
                    easing[index] = EASE_STEPS;
                    gave_at[index] = is.position;
                    in_hand[index] = 0;
                    over_load[index] = 0;
                    contact[index] = 0;
                    eased_at.rotate_left(1);
                    eased_at[EASES_ALLOWED - 1] = report.steps;
                    report.eased[index] = report.eased[index].saturating_add(1);
                }
            } else if rests_short(
                &limits,
                (written[index], is.position, over_load[index]),
                &mut short_at[index],
                &mut short_steps[index],
            ) {
                // Held a little short of its goal by its own weight, still,
                // and not straining: it rests where it is, and later moods
                // are laid from there, as for a joint a hand left.
                commanded[index] = f32::from(is.position).clamp(
                    low[index].min(commanded[index]),
                    high[index].max(commanded[index]),
                );
                kept[index] = commanded[index] - home_now[index] - offsets[index];
                velocity[index] = 0.0;
                off_track[index] = 0;
                report.rested_short[index] = report.rested_short[index].saturating_add(1);
            } else if off_track[index] >= limits.tracking_steps && short_steps[index] == 0 {
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
        // A joint a hand left limp, still for two seconds: taken hold of
        // where it is, its goal first and then its torque. Told to stay, it
        // stays there, and later moods are laid from there; told to return,
        // it is brought home as a lamp wakes, slowly.
        if let Some(index) = retake.filter(|_| surprise.is_none()) {
            let joint = COMMANDED[index];
            (bus.limp[index], limp[index]) = (false, None);
            let now = match take_hold(&mut bus, &mut report, &[joint], &samples) {
                Ok(now) => now,
                Err(stop) => break stop,
            };
            // Torque off and on may change what it flags.
            found[joint.index()].status = now[joint.index()].status;
            written[index] = samples[joint.index()].position;
            (easing[index], in_hand[index], contact[index]) = (0, 0, 0);
            (asleep[index], arriving[index], velocity[index]) = (false, 0, 0.0);
            stalls[index] = Stall::default();
            (resting_steps, relaxed) = (0, false);
            if limits.give_way == Some(GiveWay::Stay) {
                commanded[index] = f32::from(written[index]).clamp(low[index], high[index]);
                kept[index] = commanded[index] - home_now[index] - offsets[index];
                report.kept_where_put[index] = report.kept_where_put[index].saturating_add(1);
            } else {
                let from = written;
                let mut to = from;
                to[index] = home_now[index].clamp(low[index], high[index]).round() as u16;
                // The run's own wake record is kept as it was.
                let woke_at_start = report.woke.take();
                let woken = wake(
                    &mut bus,
                    &mut report,
                    &limits,
                    (from, to),
                    standing.map(|_| arm_written),
                    now,
                    Some((&mut watch, glimpse)),
                );
                report.woke = woke_at_start;
                let now = match woken {
                    Ok(now) => now,
                    Err(stop) => {
                        if let Some(last) = report.last {
                            let hold = COMMANDED.map(|joint| last[joint.index()].position);
                            if bus.send_goals(hold, standing.map(|_| arm_written)).is_ok() {
                                report.goal_writes += 1;
                                report.held_at = Some(hold);
                            }
                        }
                        left_as_is = true;
                        break stop;
                    }
                };
                // Settled short of home: it rests where it is.
                let settled = now[joint.index()].position;
                kept[index] = 0.0;
                if settled.abs_diff(to[index]) > limits.tracking {
                    kept[index] = f32::from(settled) - f32::from(to[index]);
                    to[index] = settled;
                    if let Err(detail) = bus.send_goals(to, standing.map(|_| arm_written)) {
                        break Stop::Bus {
                            detail: format!("taking hold again: {detail}"),
                        };
                    }
                    report.goal_writes += 1;
                }
                written = to;
                commanded = to.map(f32::from);
                velocity = [0.0; 3];
            }
            started = bus.runtime.now();
            first_step = report.steps;
        }
        // Under `--overload release`: a joint at its trip load for 1.5 s.
        // One told to stop pushing on this step is given its chance first:
        // its count starts again, and it trips only if it stays loaded.
        let mut tripped: Option<(Joint, u16)> = None;
        if limits.overload_release {
            for joint in Joint::ALL {
                let (index, is) = (joint.index(), samples[joint.index()]);
                overloaded[index] = if is.load >= trip_at[index] && !stopped_pushing[index] {
                    overloaded[index].saturating_add(1)
                } else {
                    0
                };
                if overloaded[index] >= OVERLOAD_STEPS
                    && tripped.is_none_or(|(_, load)| is.load > load)
                {
                    tripped = Some((joint, is.load));
                }
            }
        }
        // It lets go before anything else this step would end the run with:
        // that is the old runtime's answer to a servo pushed this hard.
        if let Some((joint, load)) = tripped {
            let now = bus.runtime.now();
            let again = let_go_at.is_some_and(|at| now.saturating_sub(at) < OVERLOAD_AGAIN);
            let_go_at = Some(now);
            report.released.push(Released {
                joint: joint.name(),
                load,
                seconds: now.as_secs_f32(),
                woke: None,
            });
            left_as_is = true;
            let template = Glimpse {
                released: report.released.len().try_into().unwrap_or(u16::MAX),
                limp: true,
                ..glimpse
            };
            let rest = if again { Duration::ZERO } else { OVERLOAD_REST };
            let slumped_at = match let_go(
                &mut bus,
                &mut report,
                rest,
                (&mut news, &mut asked),
                &mut watch,
                template,
            ) {
                Ok(now) => now,
                Err(stop) => break stop,
            };
            // Something that keeps fighting should be looked at.
            if again {
                break Stop::Overload {
                    joint: joint.id(),
                    load,
                    detail: "let go a second time within ten minutes; torque left off".into(),
                };
            }
            // Taken up again: every joint held where it is, as `--torque on`
            // takes hold, its goal first and then its torque.
            let all: Vec<Joint> = COMMANDED.into_iter().chain(ARM).collect();
            let now = match take_hold(&mut bus, &mut report, &all, &slumped_at) {
                Ok(now) => now,
                Err(Stop::Refused { why }) => {
                    break Stop::Overload {
                        joint: joint.id(),
                        load,
                        detail: format!("taking the lamp up again: {why}"),
                    };
                }
                Err(stop) => break stop,
            };
            left_as_is = false;
            commanded = COMMANDED.map(|joint| f32::from(now[joint.index()].position));
            velocity = [0.0; 3];
            arm_written = ARM.map(|joint| now[joint.index()].position);
            if let Some(joint) = COMMANDED
                .into_iter()
                .find(|joint| now[joint.index()].status != 0)
            {
                break Stop::Status {
                    joint: joint.id(),
                    found: found[joint.index()].status,
                    now: now[joint.index()].status,
                };
            }
            // Torque off and on may change what a servo flags: what it reads
            // now is what later reads are judged against (the wake's and,
            // below, the run's), and where the arm slumped is where it is
            // now found.
            if let Some(up) = standing {
                let (lift, off) = on_the_way(up, arm_written);
                match (0..2).find(|index| off[*index] > f32::from(Standing::FOUND_SLACK)) {
                    Some(index) => {
                        standing = None;
                        report.standing = None;
                        stand = 0.0;
                        report.standing_refused = Some(format!(
                            "{} is {:.0} counts off the way between folded and stood up after it let go; an arm found off its way is not lowered",
                            ARM[index], off[index]
                        ));
                    }
                    None => stand = if lift < 0.05 { 0.0 } else { unsmooth(lift) },
                }
            }
            stand_velocity = 0.0;
            slumped = true;
            home_now = home_with(home, standing, stand);
            (low, high) = bounds(home_now, stand);
            // The head, its roll and the base brought home as a lamp wakes
            // from a power cut: slowly, never away from home, a head that
            // drooped led enough to lift it, and woken where it settles.
            let from = COMMANDED.map(|joint| now[joint.index()].position);
            let to = home_now.map(|count| count.round() as u16);
            let woke_at_start = report.woke.take();
            let woken = wake(
                &mut bus,
                &mut report,
                &limits,
                (from, to),
                standing.map(|_| arm_written),
                now,
                Some((
                    &mut watch,
                    Glimpse {
                        limp: false,
                        ..template
                    },
                )),
            );
            if let Some(released) = report.released.last_mut() {
                released.woke = report.woke.take();
            }
            report.woke = woke_at_start;
            let now = match woken {
                Ok(now) => now,
                Err(stop) => {
                    // Stop pushing: each goal becomes where its joint is now.
                    if let Some(last) = report.last {
                        let hold = COMMANDED.map(|joint| last[joint.index()].position);
                        if bus.send_goals(hold, standing.map(|_| arm_written)).is_ok() {
                            report.goal_writes += 1;
                            report.held_at = Some(hold);
                        }
                    }
                    left_as_is = true;
                    break stop;
                }
            };
            // As at start: the run takes over the goals the wake left, and a
            // joint that settled farther short than the run's own bound runs
            // from where it settled, its home moved there by as much.
            found = now;
            let mut goals = to;
            for (index, joint) in COMMANDED.iter().enumerate() {
                let at = found[joint.index()].position;
                if at.abs_diff(to[index]) > limits.tracking {
                    goals[index] = at;
                    let shifted = i32::from(home[index]) + i32::from(at) - i32::from(to[index]);
                    home[index] = shifted.clamp(0, 4_095) as u16;
                }
            }
            if goals != to {
                if let Err(detail) = bus.send_goals(goals, standing.map(|_| arm_written)) {
                    break Stop::Bus {
                        detail: format!("waking: {detail}"),
                    };
                }
                report.goal_writes += 1;
                report.home = Some(home);
                home_now = home_with(home, standing, stand);
                (low, high) = bounds(home_now, stand);
            }
            // Everything else starts again from where it is, as at start.
            written = goals;
            commanded = written.map(f32::from);
            wanted_of = commanded;
            (easing, over_load, off_track, hot, in_hand) = ([0; 3], [0; 3], [0; 3], [0; 3], [0; 3]);
            (kept, asleep, arriving) = ([0.0; 3], [false; 3], [0; 3]);
            (arm_over, arm_off, arm_hot) = ([0; 2], [0; 2], [0; 2]);
            stalls = [Stall::default(); 5];
            (contact, limp, bus.limp) = ([0; 3], [None; 3], [false; 3]);
            overloaded = [0; 5];
            (resting_steps, relaxed, still_steps, down_steps) = (0, false, 0, 0);
            arm_rests = stand <= 0.0;
            (playing, pending, idle_due) = (None, None, None);
            (gesture_now, gesture_cooling) = ([0.0; 5], 0);
            (gaze, look, faces_running) = (looking.map(Gaze::new), [0.0; 2], 0);
            (heard, awake_until) = (None, None);
            started = bus.runtime.now();
            first_step = report.steps;
            continue;
        }
        if let Some(surprise) = surprise {
            break surprise;
        }
        if let Some(since) = leaving {
            let settled = (0..3).all(|index| {
                (commanded[index] - home_now[index]).abs() < 0.6 && velocity[index].abs() < 3.0
            }) && (slumped || stand <= 0.0 && stand_velocity == 0.0);
            // A lamp that stands folds down first, and that takes its time.
            let allowed = LEAVE
                + standing.map_or(Duration::ZERO, |up| {
                    Duration::from_secs_f32(up.seconds * 2.0)
                });
            if settled || now.saturating_sub(since) >= allowed {
                break Stop::Cancelled;
            }
        }
        bus.sleep_until(due);
    };

    if left_as_is {
        // Let go under overload, or already held where it is: nothing more.
        // A goal written to a servo whose torque is off turns it back on.
    } else if stop == Stop::Cancelled {
        // Leave the goals exactly at home: of a lamp that is down. One
        // that did not get down in time is left as it is.
        // Its folded arm is left resting where it is, not pushed at.
        let arm_at = report.last.map_or(arm_written, |last| {
            ARM.map(|joint| last[joint.index()].position)
        });
        if stand <= 0.0
            && (written != home || (standing.is_some() && arm_written != arm_at))
            && bus.send_goals(home, standing.map(|_| arm_at)).is_ok()
        {
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
        if bus.send_goals(hold, standing.map(|_| arm_written)).is_ok() {
            report.goal_writes += 1;
            report.held_at = Some(hold);
        }
    }
    report.stop = stop;
    report
}

#[cfg(test)]
mod tests;
