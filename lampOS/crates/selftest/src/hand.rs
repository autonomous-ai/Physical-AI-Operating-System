//! Touch by hand, at the factory only: a person is asked to swipe the base
//! strip and touch the head and the button, and `controls/touch_state` is
//! watched for each source feeling the hand, and which electrodes did.
//! Never at first boot or when asked at home: nobody is told to touch then.
use crate::{Check, listen};
use lamp_bus::Bus;
use lamp_msgs::{PartResult, TouchState, Verdict};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

/// How long the person has.
pub const GIVEN: Duration = Duration::from_secs(20);
/// How often the touch state is read while waiting.
const LOOK_EVERY: Duration = Duration::from_millis(50);

/// What each touch source felt while the person was asked to touch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Felt {
    /// Every source, by name, and the electrodes seen active on it.
    pub sources: BTreeMap<String, Option<BTreeSet<String>>>,
}

impl Felt {
    /// Adds one reading of the touch state.
    pub fn add(&mut self, state: &TouchState) {
        for source in &state.sources {
            let felt = self.sources.entry(source.name.clone()).or_default();
            if source.active == Some(true) {
                felt.get_or_insert_with(BTreeSet::new)
                    .extend(source.active_names.iter().cloned());
            }
        }
    }

    /// Every source has felt the hand.
    pub fn all(&self) -> bool {
        !self.sources.is_empty() && self.sources.values().all(Option::is_some)
    }
}

/// Judges what was felt.
pub fn by_hand(felt: &Felt) -> PartResult {
    let part = "touch_by_hand";
    if felt.sources.is_empty() {
        return PartResult::new(
            part,
            Verdict::NotChecked,
            &format!(
                "nothing was said on {}: is lamp-controls running with touch?",
                TouchState::TOPIC
            ),
        );
    }
    let mut result = if felt.all() {
        PartResult::new(part, Verdict::Pass, "")
    } else if felt.sources.values().all(Option::is_none) {
        PartResult::new(
            part,
            Verdict::NotChecked,
            &format!("nobody touched the lamp within {} s", GIVEN.as_secs()),
        )
    } else {
        let missed: Vec<&str> = felt
            .sources
            .iter()
            .filter(|(_, names)| names.is_none())
            .map(|(name, _)| name.as_str())
            .collect();
        PartResult::new(
            part,
            Verdict::NotChecked,
            &format!("not felt: {}", missed.join(", ")),
        )
    };
    // The electrodes each source felt, so one that never fires shows.
    for (name, names) in &felt.sources {
        if let Some(names) = names {
            let list: Vec<&str> = names.iter().map(String::as_str).collect();
            result = result.with(name, list.join(","));
        }
    }
    result
}

/// Asks the person to touch, then watches for up to [`GIVEN`].
pub struct ByHand<'a> {
    pub bus: &'a Bus,
    /// Tells the person what to do.
    pub ask: Box<dyn Fn(&str) + 'a>,
}

impl Check for ByHand<'_> {
    fn part(&self) -> &str {
        "touch_by_hand"
    }

    fn run(&mut self, deadline: Instant) -> PartResult {
        (self.ask)(&format!(
            "swipe the base strip, touch the head and press the button now ({} s)",
            GIVEN.as_secs()
        ));
        let until = (Instant::now() + GIVEN).min(deadline);
        let mut felt = Felt::default();
        while Instant::now() < until {
            if let Ok(Some(seen)) = listen::newest::<TouchState>(self.bus, TouchState::TOPIC)
                && seen.age <= listen::FAST_FRESH
            {
                felt.add(&seen.value);
                if felt.all() {
                    break;
                }
            }
            std::thread::sleep(LOOK_EVERY);
        }
        by_hand(&felt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(sources: &[(&str, bool, &[&str])]) -> TouchState {
        let sources: Vec<serde_json::Value> = sources
            .iter()
            .map(|(name, active, names)| {
                serde_json::json!({
                    "source": "strip", "name": name, "reader_session_id": null,
                    "ready": true, "fault": null, "observed_us": null, "armed": true,
                    "active": active, "contact_id": null, "began_us": null,
                    "held_us": null, "active_mask": null, "active_names": names,
                    "position": null, "toward_pad": null, "distance": 0.0, "speed": 0.0
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "stamp": {"clock_id": "controls", "at_us": 1, "sequence": 1},
            "session_id": "s1", "enabled": true, "sources": sources,
            "latest": [], "event_sequence": 0, "rejected_inputs": 0
        }))
        .unwrap()
    }

    #[test]
    fn every_source_felt_passes_and_names_the_electrodes_that_fired() {
        let mut felt = Felt::default();
        felt.add(&state(&[
            ("strip", true, &["E1", "E2"]),
            ("head", false, &[]),
        ]));
        assert!(!felt.all());
        felt.add(&state(&[("strip", true, &["E3"]), ("head", true, &["H1"])]));
        let result = by_hand(&felt);
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert!(
            result
                .values
                .iter()
                .any(|v| v.key == "strip" && v.value == "E1,E2,E3")
        );
    }

    #[test]
    fn a_source_never_felt_or_nobody_touching_is_not_checked_and_says_which() {
        let mut felt = Felt::default();
        felt.add(&state(&[("strip", true, &["E1"]), ("head", false, &[])]));
        let partly = by_hand(&felt);
        assert_eq!(partly.verdict, Verdict::NotChecked);
        assert_eq!(partly.reason, "not felt: head");
        let mut nobody = Felt::default();
        nobody.add(&state(&[("strip", false, &[]), ("head", false, &[])]));
        assert!(by_hand(&nobody).reason.starts_with("nobody touched"));
        assert!(by_hand(&Felt::default()).reason.contains("lamp-controls"));
    }

    #[test]
    fn the_person_is_asked_once_and_the_check_ends_when_all_is_felt() {
        let root = std::path::PathBuf::from(format!("/tmp/ls-hand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut controls = bus
            .publisher_declared(TouchState::TOPIC, &TouchState::declaration(), "controls")
            .unwrap();
        controls
            .publish_json(
                1,
                &state(&[("strip", true, &["E5"]), ("head", true, &["H2"])]),
            )
            .unwrap();
        let asked = std::cell::Cell::new(0);
        let started = Instant::now();
        let result = ByHand {
            bus: &bus,
            ask: Box::new(|_| asked.set(asked.get() + 1)),
        }
        .run(Instant::now() + Duration::from_secs(60));
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert_eq!(asked.get(), 1);
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(controls);
        std::fs::remove_dir_all(root).unwrap();
    }
}
