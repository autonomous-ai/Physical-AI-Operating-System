//! The Policy holds the lamp's actions (#800), off unless
//! `LAMP_POLICY_HOLDS_ACTIONS=on`: a call that changes the lamp (its volume
//! or its light, the executive's functions) runs only when the Policy's
//! attention decision for the turn is not `stay_quiet`, or the lamp's name
//! was heard.
//!
//! On lamp-4ace on 2026-10-15 at 14:49 the lamp heard "Thailand" in a busy
//! office, ran `volume_up` and stayed 7 dB louder for eleven hours; the
//! Policy's decision in the same second was `stay_quiet` (p = 0.235). The
//! voice's own rule (`crate::attention`, #829) holds a whole answer it
//! judges not for the lamp; this holds only the action, on the Policy's
//! judgement, and speech stays as it is.
//!
//! - **The turn's decision** is the first attention decision the Policy
//!   publishes for real (`policy/decisions`, `about` `attention`, `shadow`
//!   false: `lamp-policy --act attention`) after the turn's sound ended.
//!   The Policy's decisions carry no request of the voice's yet, so they
//!   are paired by time: one is published a few hundred milliseconds after
//!   the end of what it judged.
//! - **Waited for** at most [`WAIT`] from the call; past it the action runs
//!   as it did without the switch. While it waits nothing else waits: the
//!   call is kept aside and looked at again on each tick of the worker.
//! - **Held:** the action is not run, the model is told so ([`HELD`]) and
//!   none of the turn's sound is let out.
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
/// The decisions kept to pair with a turn: a few seconds' worth.
const KEPT: usize = 16;

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

/// The gate for an action, from the turn's decision if one has come
/// (`Some(true)` for `stay_quiet`), whether the name was heard in the turn,
/// and how long the action has waited.
pub fn gate(stay_quiet: Option<bool>, name_heard: bool, waited: Duration) -> Gate {
    match stay_quiet {
        _ if name_heard => Gate::Run,
        Some(true) => Gate::Hold,
        Some(false) => Gate::Run,
        None if waited < WAIT => Gate::Wait,
        None => Gate::Run,
    }
}

/// The Policy's attention decisions as they come, each with when it was
/// taken by this process's clock.
pub struct Verdicts {
    decisions: lamp_bus::Subscriber,
    got: VecDeque<(u64, bool)>,
}

impl Verdicts {
    pub fn on(root: &Path) -> io::Result<Self> {
        Ok(Self {
            decisions: lamp_bus::Bus::new(root)?.subscribe_typed(
                lamp_msgs::Decision::TOPIC,
                crate::bus_out::SOURCE,
                lamp_msgs::Decision::TYPE,
            )?,
            got: VecDeque::new(),
        })
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
        if self.got.len() == KEPT {
            self.got.pop_front();
        }
        self.got
            .push_back((now_us, decision.choice == "stay_quiet"));
    }

    /// The turn's decision: the first taken at or after `after_us`, whether
    /// it was `stay_quiet`.
    pub fn after(&self, after_us: u64) -> Option<bool> {
        self.got
            .iter()
            .find(|(at, _)| *at >= after_us)
            .map(|(_, quiet)| *quiet)
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
        assert_eq!(gate(Some(true), false, short), Gate::Hold);
        assert_eq!(gate(Some(true), true, short), Gate::Run);
        assert_eq!(gate(Some(false), false, short), Gate::Run);
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

    /// Only attention decisions made for real are kept, and the turn's is
    /// the first after its sound ended.
    #[test]
    fn the_turns_decision_is_the_first_made_for_real_after_its_sound_ended() {
        // Short: a socket's whole path must fit in 103 bytes.
        let dir = std::path::PathBuf::from(format!("/tmp/lph-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut verdicts = Verdicts::on(&dir).unwrap();
        verdicts.took(&decision("attention", "answer", false), 1_000);
        // A shadow decision, and a progress one, are not the turn's.
        verdicts.took(&decision("attention", "stay_quiet", true), 2_000);
        verdicts.took(&decision("progress", "wait", false), 2_500);
        verdicts.took(&decision("attention", "stay_quiet", false), 3_000);
        verdicts.took(&decision("attention", "look_up", false), 4_000);
        assert_eq!(verdicts.after(500), Some(false));
        assert_eq!(verdicts.after(1_500), Some(true));
        assert_eq!(verdicts.after(3_500), Some(false));
        assert_eq!(verdicts.after(4_001), None);
        for at in 0..20 {
            verdicts.took(&decision("attention", "answer", false), 10_000 + at);
        }
        assert_eq!(verdicts.got.len(), KEPT);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
