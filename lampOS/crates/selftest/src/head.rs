//! The head's small move (#812, as the fifth builder agreed it): at the
//! factory and when asked, the head's pitch and roll are each asked a few
//! counts one way and the other through `expression/body`, the way
//! Expression asks, and `body/joint_states` is watched for the body's goal
//! to move, the joint to follow it and to come back. `lamp-body` takes such
//! a move at rest under `--rest-glance` and cuts it to 50 counts of pitch
//! and 25 of roll; it never moves the base or the arm for it, and neither
//! does this check: those three are only read (`listen::joint`).
//!
//! Only from rest, standing, with no hand on the lamp and nothing asked of
//! the body by anyone else in the last minute. Otherwise it says why and
//! moves nothing; it never stands a lamp up. Never at first boot. A move
//! that fails is reported and not tried again in the run.
use crate::{Check, listen};
use lamp_bus::Bus;
use lamp_msgs::{
    BodyOffsets, Diagnostic, EXPRESSION_LEASE_US, ExpressionBody, ExpressionLease, Hand,
    JointState, Level, Occasion, PartResult, Verdict,
};
use std::{
    f64::consts::{PI, TAU},
    time::{Duration, Instant},
};

/// The part's name in the report.
pub const PART: &str = "head_move";
/// The name the self-test asks under on `expression/body`.
pub const SOURCE: &str = "selftest";
/// How far each move asks, in counts: inside the body's rest reach (50
/// and 25), so the body's own cut leaves it as asked where it can.
pub const PITCH_COUNTS: f64 = 40.0;
pub const ROLL_COUNTS: f64 = 20.0;
/// The body's own take-hold tolerance, in counts; its tracking stop is 30.
pub const NEAR: f64 = 12.0;
/// The goal must be met within this of being asked.
pub const REACH_WITHIN: Duration = Duration::from_secs(2);
/// The joint must be back within this of the move being let go.
pub const BACK_WITHIN: Duration = Duration::from_secs(3);
/// Nothing asked of the body by anyone else for this long before.
pub const QUIET: Duration = Duration::from_secs(60);
/// A body at least this far up is standing (`standing` in its report).
pub const STANDING: f64 = 0.95;
/// The body stops at this temperature reading; a joint this warm is not moved.
pub const WARM: u32 = 62;
/// The body reports how it is once a second.
pub const REPORT_WITHIN: Duration = Duration::from_millis(2_500);
/// Easing out and back, as Expression's moves do.
const EASE: Duration = Duration::from_millis(400);
/// Requests are renewed as often as Expression renews them.
const STEP: Duration = Duration::from_millis(20);
/// A goal unchanged this long has arrived where the body takes it.
const STEADY: Duration = Duration::from_millis(200);
/// A servo's turn is 4,096 counts.
const COUNTS_A_RADIAN: f64 = 4096.0 / TAU;
/// What one move takes at most, with the margin of a renewal or two.
const ONE_MOVE: Duration = Duration::from_millis(5_600);

/// Which joint a move asks of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Joint {
    Pitch,
    Roll,
}

impl Joint {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pitch => "head_pitch",
            Self::Roll => "head_roll",
        }
    }

    fn counts(self) -> f64 {
        match self {
            Self::Pitch => PITCH_COUNTS,
            Self::Roll => ROLL_COUNTS,
        }
    }

    fn offsets(self, counts: f64) -> BodyOffsets {
        let radians = counts / COUNTS_A_RADIAN;
        match self {
            Self::Pitch => BodyOffsets {
                head_pitch: radians,
                ..BodyOffsets::ZERO
            },
            Self::Roll => BodyOffsets {
                head_roll: radians,
                ..BodyOffsets::ZERO
            },
        }
    }
}

/// The moves, in order: the head up and down, then tilted each way. A
/// positive pitch dips the head, as in the body's own poses.
pub const MOVES: [(Joint, f64); 4] = [
    (Joint::Pitch, -1.0),
    (Joint::Pitch, 1.0),
    (Joint::Roll, 1.0),
    (Joint::Roll, -1.0),
];

/// What the body said of itself on `diagnostics`.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyReport {
    pub level: Level,
    pub message: String,
    pub mood: String,
    pub standing: f64,
    pub warmest: u32,
}

impl BodyReport {
    /// The body's report, if `diagnostic` is one.
    pub fn of(diagnostic: &Diagnostic) -> Option<Self> {
        if diagnostic.name != "body" {
            return None;
        }
        let value = |key: &str| {
            diagnostic
                .values
                .iter()
                .find(|value| value.key == key)
                .map(|value| value.value.as_str())
        };
        Some(Self {
            level: diagnostic.level,
            message: diagnostic.message.clone(),
            mood: value("mood").unwrap_or("").to_owned(),
            standing: value("standing")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
            warmest: value("warmest_reading")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        })
    }
}

/// What is known before moving: the body's newest report, whether a hand
/// is on the lamp, and how long ago someone else last asked the body for
/// anything.
#[derive(Clone, Debug, PartialEq)]
pub struct Before {
    pub occasion: Occasion,
    pub body: Option<BodyReport>,
    pub pushed: bool,
    pub others_asked: Option<Duration>,
}

/// Whether the head may be moved now, or the part's result if not.
pub fn may_move(before: &Before) -> Result<(), PartResult> {
    let not = |why: &str| Err(PartResult::new(PART, Verdict::NotChecked, why));
    if before.occasion == Occasion::FirstBoot {
        return not("never moved at first boot: only at the factory and when asked");
    }
    let Some(body) = &before.body else {
        return not("lamp-body said nothing on diagnostics: is it running, with LAMP_BUS=on?");
    };
    if body.warmest >= WARM {
        return not(&format!(
            "a joint is warm (it reads {}, and the body stops at {WARM})",
            body.warmest
        ));
    }
    if body.level != Level::Ok {
        return not(&format!("lamp-body is not well: {}", body.message));
    }
    if body.mood != "resting" {
        return not(&format!("the lamp is not at rest: it is {}", body.mood));
    }
    if body.standing < STANDING {
        return not(&format!(
            "the lamp is not standing (standing {:.2}); it is not stood up for a test",
            body.standing
        ));
    }
    if before.pushed {
        return not("a hand is on the lamp");
    }
    if let Some(ago) = before.others_asked
        && ago < QUIET
    {
        return not(&format!(
            "the body was asked to move {:.0} s ago by someone else",
            ago.as_secs_f64()
        ));
    }
    Ok(())
}

/// One reading of a joint, in counts, at a time since the move began.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub at: Duration,
    pub position: f64,
    pub commanded: f64,
}

/// When the asked goal was met: the body's goal moved the asked way, has
/// held still for a moment, and the joint is within [`NEAR`] of it. The
/// goal's move in counts beside it, which the body may have cut.
pub fn reached(goal_was: f64, sign: f64, samples: &[Sample]) -> Option<(Duration, f64)> {
    for (index, sample) in samples.iter().enumerate() {
        let moved = (sample.commanded - goal_was) * sign;
        if moved < 1.0 || (sample.position - sample.commanded).abs() > NEAR {
            continue;
        }
        let since = sample.at.saturating_sub(STEADY);
        let held = samples[..=index]
            .iter()
            .rev()
            .take_while(|earlier| earlier.at >= since)
            .all(|earlier| (earlier.commanded - sample.commanded).abs() < 0.5);
        let long_enough = samples[..index].iter().any(|earlier| earlier.at <= since);
        if held && long_enough {
            return Some((sample.at, moved));
        }
    }
    None
}

/// How one move went.
#[derive(Clone, Debug, PartialEq)]
pub enum Moved {
    /// The body's goal never moved: the move was not taken.
    NotTaken,
    /// Met, and back within [`NEAR`] of where the joint began.
    Done {
        reached: Duration,
        goal: f64,
        back: Duration,
    },
    /// The goal moved and the joint did not follow it in time.
    NotReached { goal: f64, gap: f64 },
    /// Met, and not back where it began in time.
    NotBack { goal: f64, away: f64 },
}

/// One move and how it went.
#[derive(Clone, Debug, PartialEq)]
pub struct Tried {
    pub joint: Joint,
    pub sign: f64,
    pub moved: Moved,
}

fn way(joint: Joint, sign: f64) -> &'static str {
    match (joint, sign > 0.0) {
        (Joint::Pitch, true) => "down",
        (Joint::Pitch, false) => "up",
        (Joint::Roll, true) => "one way",
        (Joint::Roll, false) => "the other way",
    }
}

/// The part's result from the moves tried and, if it was, what ended the
/// check early.
pub fn judge(tried: &[Tried], interrupted: Option<&str>) -> PartResult {
    let mut values = Vec::new();
    for t in tried {
        let key = format!(
            "{}_{}",
            t.joint.name(),
            if t.sign > 0.0 { "plus" } else { "minus" }
        );
        if let Moved::Done {
            reached,
            goal,
            back,
        } = &t.moved
        {
            values.push((format!("{key}_goal_counts"), format!("{goal:.0}")));
            values.push((
                format!("{key}_ms"),
                format!(
                    "{:.0}/{:.0}",
                    reached.as_secs_f64() * 1e3,
                    back.as_secs_f64() * 1e3
                ),
            ));
        }
    }
    let with_values = |mut result: PartResult| {
        for (key, value) in &values {
            result = result.with(key, value);
        }
        result
    };
    for t in tried {
        let what = format!("{} {}", t.joint.name(), way(t.joint, t.sign));
        let failed = match &t.moved {
            Moved::NotReached { goal, gap } => Some(format!(
                "{what}: the goal moved {goal:.0} counts and the joint stayed {gap:.0} away after {} s",
                REACH_WITHIN.as_secs()
            )),
            Moved::NotBack { goal, away } => Some(format!(
                "{what}: moved {goal:.0} counts and was still {away:.0} from where it began after {} s",
                BACK_WITHIN.as_secs()
            )),
            _ => None,
        };
        if let Some(why) = failed {
            return with_values(PartResult::new(PART, Verdict::Fail, &why));
        }
    }
    if let Some(why) = interrupted {
        return with_values(PartResult::new(
            PART,
            Verdict::NotChecked,
            &format!("stopped: {why}"),
        ));
    }
    let taken = |joint: Joint| {
        tried
            .iter()
            .any(|t| t.joint == joint && matches!(t.moved, Moved::Done { .. }))
    };
    if !taken(Joint::Pitch) && !taken(Joint::Roll) {
        return PartResult::new(
            PART,
            Verdict::NotChecked,
            "the body took no move: it runs without LAMP_EXPRESSION_BODY=on, or with --rest-glance off",
        );
    }
    for joint in [Joint::Pitch, Joint::Roll] {
        if !taken(joint) {
            return with_values(PartResult::new(
                PART,
                Verdict::NotChecked,
                &format!("the body did not move {}'s goal either way", joint.name()),
            ));
        }
    }
    with_values(PartResult::new(PART, Verdict::Pass, ""))
}

/// A joint's position and goal, in counts, from the body's newest reading.
fn read(bus: &Bus, joint: Joint) -> Option<(u64, f64, f64)> {
    let message = bus.latest(JointState::TOPIC).ok()??;
    let joints = message.parse::<JointState>().ok()?;
    let at = joints.name.iter().position(|n| n == joint.name())?;
    Some((
        message.header.seq,
        *joints.position.get(at)? * COUNTS_A_RADIAN,
        *joints.commanded.get(at)? * COUNTS_A_RADIAN,
    ))
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// How long ago someone other than the self-test last asked on
/// `expression/body`, by its kept value.
fn others_asked(bus: &Bus) -> Option<Duration> {
    let message = bus.latest(ExpressionBody::TOPIC).ok()??;
    (message.header.source != SOURCE)
        .then(|| Duration::from_millis(unix_ms().saturating_sub(message.header.at_unix_ms)))
}

fn pushed(bus: &Bus) -> bool {
    matches!(
        listen::newest::<Hand>(bus, Hand::TOPIC),
        Ok(Some(seen)) if seen.value.pushed && seen.age <= listen::FAST_FRESH
    )
}

/// The body's newest report: the first to come within `within`, or with
/// no time given, the newest of those already come.
fn body_report(diagnostics: &mut lamp_bus::Subscriber, within: Duration) -> Option<BodyReport> {
    let until = Instant::now() + within;
    let mut newest = None;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        match diagnostics.recv(left) {
            Ok(Some(message)) => {
                if let Some(report) = message
                    .parse::<Diagnostic>()
                    .ok()
                    .as_ref()
                    .and_then(BodyReport::of)
                {
                    newest = Some(report);
                    if !within.is_zero() {
                        return newest;
                    }
                }
            }
            Ok(None) | Err(_) => return newest,
        }
    }
}

/// Asks on `expression/body`, the way Expression does.
struct Asker {
    out: lamp_bus::Publisher,
    clock_id: String,
    session_id: String,
    sequence: u64,
}

impl Asker {
    fn new(bus: &Bus) -> Result<Self, String> {
        let out = bus
            .publisher_declared(
                ExpressionBody::TOPIC,
                &ExpressionBody::declaration(),
                SOURCE,
            )
            .map_err(|e| e.to_string())?;
        let clock_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map_err(|e| format!("this machine's boot cannot be named: {e}"))?
            .trim()
            .to_owned();
        let session_id = format!(
            "selftest-{}-{}",
            std::process::id(),
            lamp_ipc::monotonic_us()
        );
        Ok(Self {
            out,
            clock_id,
            session_id,
            sequence: 0,
        })
    }

    fn ask(&mut self, offsets: BodyOffsets) -> Result<(), String> {
        let now = lamp_ipc::monotonic_us();
        self.sequence += 1;
        let request = ExpressionBody {
            lease: ExpressionLease {
                clock_id: self.clock_id.clone(),
                session_id: self.session_id.clone(),
                voice_session_id: SOURCE.into(),
                sequence: self.sequence,
                turn: 1,
                request: 1,
                issued_us: now,
                apply_us: now,
                valid_until_us: now + EXPRESSION_LEASE_US,
            },
            offsets,
            hold: false,
            gaze: None,
        };
        request.check().map_err(|why| format!("{why:?}"))?;
        self.out
            .publish_json(now, &request)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn ease(share: f64) -> f64 {
    0.5 - 0.5 * (PI * share.clamp(0.0, 1.0)).cos()
}

/// What can end the check while it moves: a hand on the lamp, someone else
/// asking the body, or the lamp leaving rest.
fn interruption(
    bus: &Bus,
    began: Instant,
    diagnostics: &mut lamp_bus::Subscriber,
) -> Option<String> {
    if pushed(bus) {
        return Some("a hand was put on the lamp".into());
    }
    if others_asked(bus).is_some_and(|ago| ago < began.elapsed()) {
        return Some("someone else asked the body to move".into());
    }
    if let Some(body) = body_report(diagnostics, Duration::ZERO)
        && body.mood != "resting"
    {
        return Some(format!("the lamp left rest: it is {}", body.mood));
    }
    None
}

/// One move: eased out and held until met or [`REACH_WITHIN`], eased back,
/// and watched until back or [`BACK_WITHIN`].
fn one_move(
    bus: &Bus,
    asker: &mut Asker,
    diagnostics: &mut lamp_bus::Subscriber,
    joint: Joint,
    sign: f64,
    checked_since: Instant,
) -> Result<Moved, String> {
    let Some((_, start, goal_was)) = read(bus, joint) else {
        return Err(format!(
            "{} is not read on {}",
            joint.name(),
            JointState::TOPIC
        ));
    };
    let target = joint.counts() * sign;
    let began = Instant::now();
    let mut samples: Vec<Sample> = Vec::new();
    let mut last_seq = None;
    let mut met = None;
    while began.elapsed() < REACH_WITHIN {
        let share = ease(began.elapsed().as_secs_f64() / EASE.as_secs_f64());
        asker.ask(joint.offsets(target * share))?;
        if let Some((seq, position, commanded)) = read(bus, joint)
            && last_seq != Some(seq)
        {
            last_seq = Some(seq);
            samples.push(Sample {
                at: began.elapsed(),
                position,
                commanded,
            });
            met = reached(goal_was, sign, &samples);
            if met.is_some() {
                break;
            }
        }
        if let Some(why) = interruption(bus, checked_since, diagnostics) {
            return Err(why);
        }
        std::thread::sleep(STEP);
    }
    // Let go the way it was asked: eased back.
    let letting_go = Instant::now();
    while letting_go.elapsed() < EASE {
        let share = 1.0 - ease(letting_go.elapsed().as_secs_f64() / EASE.as_secs_f64());
        asker.ask(joint.offsets(target * share))?;
        std::thread::sleep(STEP);
    }
    let Some((reached_at, goal)) = met else {
        let moved = samples
            .iter()
            .map(|s| (s.commanded - goal_was) * sign)
            .fold(0.0f64, f64::max);
        if moved < 1.0 {
            return Ok(Moved::NotTaken);
        }
        let gap = samples
            .last()
            .map_or(f64::NAN, |s| (s.position - s.commanded).abs());
        return Ok(Moved::NotReached { goal: moved, gap });
    };
    let mut away = f64::NAN;
    while letting_go.elapsed() < BACK_WITHIN {
        if let Some((_, position, _)) = read(bus, joint) {
            away = (position - start).abs();
            if away <= NEAR {
                return Ok(Moved::Done {
                    reached: reached_at,
                    goal,
                    back: letting_go.elapsed(),
                });
            }
        }
        if pushed(bus) {
            return Err("a hand was put on the lamp".into());
        }
        std::thread::sleep(STEP);
    }
    Ok(Moved::NotBack { goal, away })
}

pub struct HeadMove<'a> {
    pub bus: &'a Bus,
    pub occasion: Occasion,
}

impl HeadMove<'_> {
    fn moves(&mut self, deadline: Instant) -> PartResult {
        let mut diagnostics =
            match self
                .bus
                .subscribe_typed(Diagnostic::TOPIC, SOURCE, Diagnostic::TYPE)
            {
                Ok(subscriber) => subscriber,
                Err(error) => {
                    return PartResult::new(
                        PART,
                        Verdict::Fail,
                        &format!("{} cannot be read: {error}", Diagnostic::TOPIC),
                    );
                }
            };
        let before = Before {
            occasion: self.occasion,
            body: body_report(&mut diagnostics, REPORT_WITHIN),
            pushed: pushed(self.bus),
            others_asked: others_asked(self.bus),
        };
        if let Err(result) = may_move(&before) {
            return result;
        }
        let mut asker = match Asker::new(self.bus) {
            Ok(asker) => asker,
            Err(why) => return PartResult::new(PART, Verdict::Fail, &why),
        };
        let checked_since = Instant::now();
        let mut tried = Vec::new();
        for (joint, sign) in MOVES {
            if Instant::now() + ONE_MOVE > deadline {
                return judge(&tried, Some("out of time"));
            }
            match one_move(
                self.bus,
                &mut asker,
                &mut diagnostics,
                joint,
                sign,
                checked_since,
            ) {
                Ok(moved) => {
                    let failed = matches!(moved, Moved::NotReached { .. } | Moved::NotBack { .. });
                    tried.push(Tried { joint, sign, moved });
                    // A fail is reported, never tried again or after.
                    if failed {
                        break;
                    }
                }
                Err(why) => return judge(&tried, Some(&why)),
            }
        }
        judge(&tried, None)
    }
}

impl Check for HeadMove<'_> {
    fn part(&self) -> &str {
        PART
    }

    fn run(&mut self, deadline: Instant) -> PartResult {
        self.moves(deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_msgs::KeyValue;

    fn report(mood: &str, standing: f64, warmest: u32) -> Diagnostic {
        Diagnostic {
            level: Level::Ok,
            name: "body".into(),
            message: String::new(),
            values: [
                ("mood", mood.to_owned()),
                ("standing", format!("{standing:.2}")),
                ("warmest_joint", "head_pitch".into()),
                ("warmest_reading", warmest.to_string()),
            ]
            .into_iter()
            .map(|(key, value)| KeyValue {
                key: key.into(),
                value,
            })
            .collect(),
        }
    }

    fn before(body: Option<Diagnostic>) -> Before {
        Before {
            occasion: Occasion::Factory,
            body: body.as_ref().and_then(BodyReport::of),
            pushed: false,
            others_asked: None,
        }
    }

    fn why(result: Result<(), PartResult>) -> String {
        let result = result.unwrap_err();
        assert_eq!(result.verdict, Verdict::NotChecked);
        result.reason
    }

    #[test]
    fn a_standing_lamp_at_rest_may_be_moved() {
        assert_eq!(may_move(&before(Some(report("resting", 1.0, 40)))), Ok(()));
        let mut long_ago = before(Some(report("resting", 0.98, 40)));
        long_ago.others_asked = Some(Duration::from_secs(61));
        assert_eq!(may_move(&long_ago), Ok(()));
    }

    #[test]
    fn nothing_moves_unless_at_rest_standing_untouched_and_quiet() {
        let mut first_boot = before(Some(report("resting", 1.0, 40)));
        first_boot.occasion = Occasion::FirstBoot;
        assert!(why(may_move(&first_boot)).contains("first boot"));
        assert!(why(may_move(&before(None))).contains("lamp-body said nothing"));
        assert!(why(may_move(&before(Some(report("speaking", 1.0, 40))))).contains("speaking"));
        assert!(why(may_move(&before(Some(report("resting", 0.0, 40))))).contains("not standing"));
        assert!(why(may_move(&before(Some(report("resting", 1.0, 62))))).contains("warm"));
        let mut touched = before(Some(report("resting", 1.0, 40)));
        touched.pushed = true;
        assert!(why(may_move(&touched)).contains("hand"));
        let mut surprised = before(Some(report("resting", 1.0, 40)));
        surprised.others_asked = Some(Duration::from_secs(20));
        assert!(why(may_move(&surprised)).contains("20 s ago"));
        let mut unwell = report("resting", 1.0, 40);
        unwell.level = Level::Warn;
        unwell.message = "a joint met resistance".into();
        assert!(why(may_move(&before(Some(unwell)))).contains("resistance"));
        let mut other = report("resting", 1.0, 40);
        other.name = "ears".into();
        assert!(BodyReport::of(&other).is_none());
    }

    fn at(ms: u64, position: f64, commanded: f64) -> Sample {
        Sample {
            at: Duration::from_millis(ms),
            position,
            commanded,
        }
    }

    #[test]
    fn a_goal_is_met_once_it_holds_and_the_joint_is_near() {
        // The goal eases 40 counts down, holds, and the joint follows.
        let samples = [
            at(0, 1700.0, 1700.0),
            at(100, 1702.0, 1710.0),
            at(200, 1712.0, 1725.0),
            at(300, 1726.0, 1738.0),
            at(400, 1735.0, 1740.0),
            at(500, 1738.0, 1740.0),
            at(600, 1739.0, 1740.0),
        ];
        assert_eq!(
            reached(1700.0, 1.0, &samples),
            Some((Duration::from_millis(600), 40.0))
        );
        // The other way it was never met.
        assert_eq!(reached(1700.0, -1.0, &samples), None);
        // A goal cut to one count, as lamp-52e6's roll is, is met.
        let roll = [
            at(0, 2035.0, 2035.0),
            at(100, 2035.0, 2036.0),
            at(400, 2036.0, 2036.0),
        ];
        assert_eq!(
            reached(2035.0, 1.0, &roll),
            Some((Duration::from_millis(400), 1.0))
        );
        // A joint left behind its goal is not.
        let stuck = [
            at(0, 1700.0, 1700.0),
            at(300, 1700.0, 1740.0),
            at(600, 1701.0, 1740.0),
        ];
        assert_eq!(reached(1700.0, 1.0, &stuck), None);
    }

    fn done(joint: Joint, sign: f64) -> Tried {
        Tried {
            joint,
            sign,
            moved: Moved::Done {
                reached: Duration::from_millis(900),
                goal: 40.0,
                back: Duration::from_millis(1_100),
            },
        }
    }

    #[test]
    fn all_four_moves_met_and_back_pass_with_their_times() {
        let tried: Vec<_> = MOVES
            .iter()
            .map(|(joint, sign)| done(*joint, *sign))
            .collect();
        let result = judge(&tried, None);
        assert_eq!(result.verdict, Verdict::Pass);
        assert!(
            result
                .values
                .iter()
                .any(|v| v.key == "head_pitch_minus_ms" && v.value == "900/1100")
        );
    }

    #[test]
    fn a_move_not_followed_or_not_back_fails_and_names_it() {
        let mut tried = vec![done(Joint::Pitch, -1.0)];
        tried.push(Tried {
            joint: Joint::Pitch,
            sign: 1.0,
            moved: Moved::NotReached {
                goal: 40.0,
                gap: 38.0,
            },
        });
        let result = judge(&tried, None);
        assert_eq!(result.verdict, Verdict::Fail);
        assert!(
            result
                .reason
                .starts_with("head_pitch down: the goal moved 40"),
            "{}",
            result.reason
        );
        let back = [Tried {
            joint: Joint::Roll,
            sign: 1.0,
            moved: Moved::NotBack {
                goal: 20.0,
                away: 19.0,
            },
        }];
        assert!(
            judge(&back, None)
                .reason
                .contains("still 19 from where it began")
        );
    }

    #[test]
    fn a_move_never_taken_or_cut_short_is_not_checked() {
        let none: Vec<_> = MOVES
            .iter()
            .map(|(joint, sign)| Tried {
                joint: *joint,
                sign: *sign,
                moved: Moved::NotTaken,
            })
            .collect();
        let result = judge(&none, None);
        assert_eq!(result.verdict, Verdict::NotChecked);
        assert!(result.reason.contains("LAMP_EXPRESSION_BODY"));
        let pitch_only = [done(Joint::Pitch, -1.0), done(Joint::Pitch, 1.0)];
        assert!(judge(&pitch_only, None).reason.contains("head_roll"));
        let cut = judge(&pitch_only, Some("a hand was put on the lamp"));
        assert_eq!(cut.verdict, Verdict::NotChecked);
        assert_eq!(cut.reason, "stopped: a hand was put on the lamp");
    }

    /// How a fake body treats what it is asked.
    #[derive(Clone, Copy)]
    enum Fake {
        Follows,
        PitchStuck,
        TakesNothing,
    }

    /// A body on `bus`: it takes requests on
    /// `expression/body`, moves its goals at 120 counts a second, lets
    /// the joints follow, and says it rests, standing, until stopped.
    fn body(
        root: std::path::PathBuf,
        fake: Fake,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let bus = Bus::new(&root).unwrap();
            let mut joints = bus
                .publisher_declared(JointState::TOPIC, &JointState::declaration(), "body")
                .unwrap();
            let mut diagnostics = bus
                .publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), "body")
                .unwrap();
            let mut asked = bus
                .subscribe_typed(ExpressionBody::TOPIC, "body", ExpressionBody::TYPE)
                .unwrap();
            let rest = [1771.0, 2035.0];
            let (mut goal, mut position) = (rest, rest);
            let mut offsets = None::<(BodyOffsets, u64)>;
            let mut step = 0u64;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                while let Ok(Some(message)) = asked.recv(Duration::ZERO) {
                    if let Ok(request) = message.parse::<ExpressionBody>() {
                        offsets = Some((request.offsets, request.lease.valid_until_us));
                    }
                }
                let now = lamp_ipc::monotonic_us();
                let wanted = match offsets {
                    Some((o, until)) if until > now && !matches!(fake, Fake::TakesNothing) => [
                        rest[0] + (o.head_pitch * COUNTS_A_RADIAN).clamp(-50.0, 50.0),
                        rest[1] + (o.head_roll * COUNTS_A_RADIAN).clamp(-25.0, 25.0),
                    ],
                    _ => rest,
                };
                for joint in 0..2 {
                    goal[joint] += (wanted[joint] - goal[joint]).clamp(-2.4, 2.4);
                    if joint == 1 || !matches!(fake, Fake::PitchStuck) {
                        position[joint] += (goal[joint] - position[joint]) * 0.5;
                    }
                }
                let radians = |counts: f64| counts / COUNTS_A_RADIAN;
                let state = JointState {
                    name: ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"]
                        .map(str::to_owned)
                        .to_vec(),
                    position: [2035.0, 2657.0, 2602.0, position[1], position[0]]
                        .map(radians)
                        .to_vec(),
                    velocity: Vec::new(),
                    effort: Vec::new(),
                    commanded: [2035.0, 2657.0, 2602.0, goal[1], goal[0]]
                        .map(radians)
                        .to_vec(),
                };
                joints.publish_json(now, &state).unwrap();
                if step.is_multiple_of(10) {
                    diagnostics
                        .publish_json(now, &report("resting", 1.0, 40))
                        .unwrap();
                }
                step += 1;
                std::thread::sleep(STEP);
            }
        })
    }

    fn run_against(fake: Fake, name: &str) -> PartResult {
        let root = std::path::PathBuf::from(format!("/tmp/hm-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fake_body = body(root.clone(), fake, stop.clone());
        std::thread::sleep(Duration::from_millis(300));
        let bus = Bus::new(&root).unwrap();
        let mut check = HeadMove {
            bus: &bus,
            occasion: Occasion::OnDemand,
        };
        let result = check.run(Instant::now() + Duration::from_secs(60));
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        fake_body.join().unwrap();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn a_body_that_follows_passes_and_one_that_does_not_fails_or_is_not_checked() {
        let follows = run_against(Fake::Follows, "follows");
        assert_eq!(follows.verdict, Verdict::Pass, "{follows:?}");
        assert!(
            follows
                .values
                .iter()
                .any(|v| v.key == "head_roll_plus_goal_counts" && v.value == "20")
        );
        let stuck = run_against(Fake::PitchStuck, "stuck");
        assert_eq!(stuck.verdict, Verdict::Fail, "{stuck:?}");
        assert!(
            stuck.reason.starts_with("head_pitch up"),
            "{}",
            stuck.reason
        );
        let deaf = run_against(Fake::TakesNothing, "deaf");
        assert_eq!(deaf.verdict, Verdict::NotChecked, "{deaf:?}");
    }
}
