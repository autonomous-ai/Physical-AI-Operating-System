//! The Policy holds the lamp's actions (#800), off unless
//! `LAMP_POLICY_HOLDS_ACTIONS=on`: a call that changes the lamp (its volume
//! or its light, the executive's functions) or its schedule (`set_timer`,
//! `set_reminder`, `cancel_reminder`) runs only when the Policy's attention
//! decision for the turn is not `stay_quiet`, or the lamp's name was heard.
//!
//! On lamp-4ace on 2026-10-15 at 14:49 the lamp heard "Thailand" in a busy
//! office, ran `volume_up` and stayed 7 dB louder for eleven hours; the
//! Policy's decision in the same second was `stay_quiet` (p = 0.235). The
//! voice's own rule (`crate::attention`, #829) holds a whole answer it
//! judges not for the lamp; this holds only the action, on the Policy's
//! judgement, and speech stays as it is.
//!
//! - **The turn's decision** is an attention decision the Policy publishes
//!   for real (`policy/decisions`, `about` `attention`, `shadow` false:
//!   `lamp-policy --act attention`). The Policy's decisions carry no request
//!   of the voice's yet, so they are paired with the voice's turns in order:
//!   each turn is noted as its sound ends ([`Verdicts::ended`]), and a
//!   decision that comes is that turn's only when it is the one turn waiting
//!   for one. With two or more waiting, which one it judged is not known
//!   (the reviewer on #908: a late decision about the turn before must
//!   never release this turn's action): every turn waiting is then
//!   **ambiguous**, its action runs as without the switch, and the log says
//!   so. A turn waits for its decision at most [`PAIR_WITHIN`]: one with no
//!   decision by then (nothing heard in it to judge) is let go, so that it
//!   never takes a later turn's.
//! - **Waited for** at most [`WAIT`] from the call; past it the action runs
//!   as it did without the switch. While it waits nothing else waits: the
//!   call is kept aside and looked at again on each tick of the worker.
//! - **Held:** the action is not run, the model is told so ([`HELD`]) and
//!   none of the turn's sound is let out.
//! - **Withdrawn while it waits** (the service cancelled the call), or its
//!   turn retired or its session gone: it is let go, neither run nor
//!   answered, and that is looked at again right before it would run (the
//!   reviewer on #908).
//! - Without the bus, or on a turn the lamp wrote itself, nothing is held.
//!   The second voice (`crate::second_provider`) has no bus: its actions run
//!   as before.
//!
//! Nothing of what was heard is read here: the decision's choice only.
use serde_json::{Value, json};
use std::{collections::VecDeque, io, path::Path, time::Duration};

/// Where the coordinator reads the switch. `on` only.
pub const ENVIRONMENT: &str = "LAMP_POLICY_HOLDS_ACTIONS";
/// The provider worker's flag for it, after `--sounds-checked`.
pub const FLAG: &str = "--policy-holds-actions";
/// The longest an action waits for the turn's decision.
pub const WAIT: Duration = Duration::from_secs(2);
/// A hearing of the name at this score or more lets the action run, as the
/// Policy counts the name heard (`lamp_policy::attention::NAME_HEARD_FROM`).
pub const NAME_FROM: f32 = 0.9;
/// What the model is told of an action held.
pub const HELD: &str = "Not done: it was not clear that was said to you. Say nothing at all.";
/// Turns kept, waiting or decided: a few seconds' worth.
const KEPT: usize = 16;
/// A turn waits this long after its sound ended for its decision: the
/// Policy decided 99% of lamp-4ace's 3,698 turns from 13 to 16 October
/// within 1.6 s of asking, and asks some 0.3 s after the end.
pub const PAIR_WITHIN: Duration = Duration::from_secs(3);

/// The worker's argument for the switch in the environment.
pub fn arguments_from_environment() -> Vec<String> {
    arguments(std::env::var(ENVIRONMENT).ok().as_deref())
}

fn arguments(value: Option<&str>) -> Vec<String> {
    if value == Some("on") {
        vec![FLAG.to_owned()]
    } else {
        Vec::new()
    }
}

/// Take a leading [`FLAG`] off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// What the model is told of an action held.
pub fn held() -> Value {
    json!({"done": false, "next": HELD})
}

/// What becomes of an action kept aside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// Run it, as without the switch.
    Run,
    /// Do not run it: the Policy judged the turn not for the lamp.
    Hold,
    /// Wait for the turn's decision.
    Wait,
}

/// What the Policy's decisions say of one turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// `stay_quiet`: not for the lamp.
    StayQuiet,
    /// `answer` or `look_up`: the lamp's.
    TakenUp,
    /// A decision came while this and another turn both waited for one:
    /// which it judged is not known.
    Ambiguous,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StayQuiet => "stay_quiet",
            Self::TakenUp => "taken up",
            Self::Ambiguous => "ambiguous",
        }
    }
}

/// The gate for an action, from its turn's verdict if one has come,
/// whether the name was heard in the turn, and how long the action has
/// waited.
pub fn gate(verdict: Option<Verdict>, name_heard: bool, waited: Duration) -> Gate {
    match verdict {
        _ if name_heard => Gate::Run,
        Some(Verdict::StayQuiet) => Gate::Hold,
        Some(Verdict::TakenUp | Verdict::Ambiguous) => Gate::Run,
        None if waited < WAIT => Gate::Wait,
        None => Gate::Run,
    }
}

/// The Policy's attention decisions paired with the voice's turns.
pub struct Verdicts {
    decisions: lamp_bus::Subscriber,
    /// Turns whose sound ended and that have no decision yet: each request,
    /// and when it ended by this process's clock.
    waiting: VecDeque<(u64, u64)>,
    /// What became of each turn decided, newest last.
    decided: VecDeque<(u64, Verdict)>,
}

impl Verdicts {
    pub fn on(root: &Path) -> io::Result<Self> {
        Ok(Self {
            decisions: lamp_bus::Bus::new(root)?.subscribe_typed(
                lamp_msgs::Decision::TOPIC,
                crate::bus_out::SOURCE,
                lamp_msgs::Decision::TYPE,
            )?,
            waiting: VecDeque::new(),
            decided: VecDeque::new(),
        })
    }

    /// The sound of `request`, a turn someone spoke in, ended at `at_us`:
    /// it waits for its decision. Said once a turn; again, it is nothing.
    pub fn ended(&mut self, request: u64, at_us: u64) {
        if self.waiting.iter().any(|(turn, _)| *turn == request)
            || self.decided.iter().any(|(turn, _)| *turn == request)
        {
            return;
        }
        if self.waiting.len() == KEPT {
            self.waiting.pop_front();
        }
        self.waiting.push_back((request, at_us));
    }

    /// Take what has come, without waiting: attention decisions made for
    /// real only, each dated `now_us`.
    pub fn take(&mut self, now_us: u64) {
        for _ in 0..KEPT {
            let Ok(Some(message)) = self.decisions.recv(Duration::ZERO) else {
                break;
            };
            if let Ok(decision) = message.parse::<lamp_msgs::Decision>() {
                self.took(&decision, now_us);
            }
        }
    }

    fn took(&mut self, decision: &lamp_msgs::Decision, now_us: u64) {
        if decision.about != "attention" || decision.shadow {
            return;
        }
        // A turn that has waited longer than any decision takes had none.
        let within = u64::try_from(PAIR_WITHIN.as_micros()).unwrap_or(u64::MAX);
        self.waiting
            .retain(|(_, ended)| now_us.saturating_sub(*ended) <= within);
        let verdict = match self.waiting.len() {
            0 => return,
            1 if decision.choice == "stay_quiet" => Verdict::StayQuiet,
            1 => Verdict::TakenUp,
            _ => Verdict::Ambiguous,
        };
        for (turn, _) in std::mem::take(&mut self.waiting) {
            if self.decided.len() == KEPT {
                self.decided.pop_front();
            }
            self.decided.push_back((turn, verdict));
        }
    }

    /// What the Policy's decisions say of `request`, if anything yet.
    pub fn verdict(&self, request: u64) -> Option<Verdict> {
        self.decided
            .iter()
            .rev()
            .find(|(turn, _)| *turn == request)
            .map(|(_, verdict)| *verdict)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the Policy publishes one, read as the voice reads it.
    fn decision(about: &str, choice: &str, shadow: bool) -> lamp_msgs::Decision {
        serde_json::from_value(json!({
            "about": about, "choice": choice, "probability": 0.235, "answers": {},
            "by": "typesafe/jev-1.13-20260917", "took_ms": 540, "shadow": shadow
        }))
        .unwrap()
    }

    #[test]
    fn an_action_runs_unless_the_turn_was_judged_not_for_the_lamp_and_the_name_was_not_heard() {
        let short = Duration::from_millis(300);
        assert_eq!(gate(Some(Verdict::StayQuiet), false, short), Gate::Hold);
        assert_eq!(gate(Some(Verdict::StayQuiet), true, short), Gate::Run);
        assert_eq!(gate(Some(Verdict::TakenUp), false, short), Gate::Run);
        // Not known which turn was judged: as without the switch.
        assert_eq!(gate(Some(Verdict::Ambiguous), false, short), Gate::Run);
        assert_eq!(gate(None, false, short), Gate::Wait);
        assert_eq!(gate(None, true, short), Gate::Run);
        // Past the wait it runs as today.
        assert_eq!(gate(None, false, WAIT), Gate::Run);
        assert_eq!(held()["done"], false);
        assert!(HELD.contains("Say nothing"));
    }

    #[test]
    fn the_switch_is_on_only_and_its_flag_is_taken_once() {
        assert_eq!(arguments(Some("on")), [FLAG]);
        for not_on in [None, Some("yes"), Some("ON"), Some("1")] {
            assert!(arguments(not_on).is_empty());
        }
        let flags: Vec<String> = [FLAG, "--next"].map(String::from).into();
        assert_eq!(take(&flags), (true, &flags[1..]));
        assert_eq!(take(&flags[1..]), (false, &flags[1..]));
    }

    fn verdicts(tag: &str) -> (Verdicts, std::path::PathBuf) {
        // Short: a socket's whole path must fit in 103 bytes.
        let dir = std::path::PathBuf::from(format!("/tmp/lphu-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (Verdicts::on(&dir).unwrap(), dir)
    }

    const MS: u64 = 1_000;

    /// One turn at a time: each decision is its turn's. Only attention
    /// decisions made for real count; one with no turn waiting is nobody's.
    #[test]
    fn one_turn_at_a_time_each_decision_is_its_turns() {
        let (mut verdicts, dir) = verdicts("one");
        verdicts.took(&decision("attention", "answer", false), 100 * MS);
        verdicts.ended(1, 1_000 * MS);
        assert_eq!(verdicts.verdict(1), None);
        // A shadow decision, and a progress one, are not the turn's.
        verdicts.took(&decision("attention", "answer", true), 1_300 * MS);
        verdicts.took(&decision("progress", "say_now", false), 1_400 * MS);
        assert_eq!(verdicts.verdict(1), None);
        verdicts.took(&decision("attention", "stay_quiet", false), 1_600 * MS);
        assert_eq!(verdicts.verdict(1), Some(Verdict::StayQuiet));
        // Said again, the turn is not waited for again.
        verdicts.ended(1, 1_000 * MS);
        verdicts.ended(2, 5_000 * MS);
        verdicts.took(&decision("attention", "look_up", false), 5_700 * MS);
        assert_eq!(
            (verdicts.verdict(1), verdicts.verdict(2)),
            (Some(Verdict::StayQuiet), Some(Verdict::TakenUp))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The reviewer on #908: turn A's decision comes late, after turn B's
    /// sound ended. Which turn it judged is not known, so it is neither's:
    /// both are ambiguous, and B's own decision after it changes nothing.
    /// In both orders of answer and stay_quiet.
    #[test]
    fn a_late_decision_of_the_turn_before_never_releases_or_holds_this_turns_action() {
        for (first, second) in [("answer", "stay_quiet"), ("stay_quiet", "answer")] {
            let (mut verdicts, dir) = verdicts("late");
            verdicts.ended(1, 1_000 * MS);
            verdicts.ended(2, 1_800 * MS);
            verdicts.took(&decision("attention", first, false), 2_000 * MS);
            verdicts.took(&decision("attention", second, false), 2_600 * MS);
            for turn in [1, 2] {
                assert_eq!(verdicts.verdict(turn), Some(Verdict::Ambiguous), "{first}");
                assert_eq!(
                    gate(verdicts.verdict(turn), false, Duration::ZERO),
                    Gate::Run
                );
            }
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    /// A turn with nothing in it to judge has no decision: after
    /// [`PAIR_WITHIN`] it is let go, and never takes a later turn's.
    #[test]
    fn a_turn_that_had_no_decision_is_let_go_and_takes_no_later_ones() {
        let (mut verdicts, dir) = verdicts("none");
        verdicts.ended(1, 1_000 * MS);
        verdicts.ended(2, 4_500 * MS);
        verdicts.took(&decision("attention", "stay_quiet", false), 5_000 * MS);
        assert_eq!(verdicts.verdict(1), None);
        assert_eq!(verdicts.verdict(2), Some(Verdict::StayQuiet));
        // Never more than a few seconds' worth is kept.
        for turn in 10..40 {
            verdicts.ended(turn, 10_000 * MS + turn * MS);
        }
        assert_eq!(verdicts.waiting.len(), KEPT);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
