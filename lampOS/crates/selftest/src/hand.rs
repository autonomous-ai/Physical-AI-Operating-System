//! Touch by hand, at the factory only: a person is asked to swipe the base
//! strip and touch the head and the button, and `controls/touch_state` is
//! watched for the whole time given: each source feeling the hand, and
//! which electrodes did. The strip's are the bits of its `active_mask`
//! (`E0` to `E11`, as lamp-controls names them); the head's are its pad
//! names.
//! Never at first boot or when asked at home: nobody is told to touch then.
use crate::{Check, listen};
use lamp_bus::Bus;
use lamp_msgs::{PartResult, TouchSource, TouchState, Verdict};
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
            let electrodes: Vec<String> = match source.source {
                TouchSource::Strip => {
                    let mask = source.active_mask.unwrap_or(0);
                    (0..12)
                        .filter(|bit| mask & (1 << bit) != 0)
                        .map(|bit| format!("E{bit}"))
                        .collect()
                }
                _ => source.active_names.clone(),
            };
            if source.active == Some(true) || !electrodes.is_empty() {
                felt.get_or_insert_with(BTreeSet::new).extend(electrodes);
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
            let mut list: Vec<&str> = names.iter().map(String::as_str).collect();
            // E2 before E10.
            list.sort_by_key(|name| (name.len(), *name));
            result = result.with(name, list.join(","));
        }
    }
    result
}

/// Asks the person to touch, then watches for up to [`GIVEN`].
pub struct ByHand<'a> {
    pub bus: &'a Bus,
    /// How long the person has; watched to the end, so a swipe that
    /// began is read whole ([`GIVEN`] in the program).
    pub given: Duration,
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
            self.given.as_secs()
        ));
        let until = (Instant::now() + self.given).min(deadline);
        let mut felt = Felt::default();
        while Instant::now() < until {
            if let Ok(Some(seen)) = listen::newest::<TouchState>(self.bus, TouchState::TOPIC)
                && seen.age <= listen::FAST_FRESH
            {
                felt.add(&seen.value);
            }
            std::thread::sleep(LOOK_EVERY);
        }
        by_hand(&felt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source as lamp-controls says it: the strip with its mask and no
    /// names, the head with its pad names, the button with neither.
    fn source(
        kind: &str,
        name: &str,
        active: bool,
        mask: u16,
        names: &[&str],
    ) -> serde_json::Value {
        serde_json::json!({
            "source": kind, "name": name, "reader_session_id": "reader",
            "ready": true, "fault": null, "observed_us": 1000, "armed": true,
            "active": active, "contact_id": null, "began_us": null,
            "held_us": null, "active_mask": mask, "active_names": names,
            "position": null, "toward_pad": null, "distance": 0.0, "speed": 0.0
        })
    }

    fn state(strip: u16, head: &[&str], button: bool) -> TouchState {
        let head_mask = (0..head.len()).fold(0u16, |mask, n| mask | 1 << n);
        let state: TouchState = serde_json::from_value(serde_json::json!({
            "stamp": {"clock_id": "controls", "at_us": 1000, "sequence": 1},
            "session_id": "s1", "enabled": true,
            "sources": [
                source("primary", "button", button, u16::from(button), &[]),
                source("head", "head", !head.is_empty(), head_mask, head),
                source("strip", "strip", strip != 0, strip, &[]),
            ],
            "latest": [], "event_sequence": 0, "rejected_inputs": 0
        }))
        .unwrap();
        assert_eq!(state.check(), Ok(()), "a fixture lamp-controls could send");
        state
    }

    fn value<'a>(result: &'a PartResult, key: &str) -> Option<&'a str> {
        result
            .values
            .iter()
            .find(|v| v.key == key)
            .map(|v| v.value.as_str())
    }

    #[test]
    fn the_strip_electrodes_are_read_from_its_mask_and_listed_in_order() {
        let mut felt = Felt::default();
        felt.add(&state(1, &["front"], true));
        felt.add(&state(4, &[], false));
        felt.add(&state(1 << 10, &[], false));
        let result = by_hand(&felt);
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert_eq!(value(&result, "strip"), Some("E0,E2,E10"));
        assert_eq!(value(&result, "head"), Some("front"));
    }

    #[test]
    fn a_source_never_felt_or_nobody_touching_is_not_checked_and_says_which() {
        let mut felt = Felt::default();
        felt.add(&state(1, &[], true));
        let partly = by_hand(&felt);
        assert_eq!(partly.verdict, Verdict::NotChecked);
        assert_eq!(partly.reason, "not felt: head");
        let mut nobody = Felt::default();
        nobody.add(&state(0, &[], false));
        assert!(by_hand(&nobody).reason.starts_with("nobody touched"));
        assert!(by_hand(&Felt::default()).reason.contains("lamp-controls"));
    }

    #[test]
    fn the_whole_window_is_watched_so_a_swipe_is_read_to_its_end() {
        let root = std::path::PathBuf::from(format!("/tmp/ls-hand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut controls = bus
            .publisher_declared(TouchState::TOPIC, &TouchState::declaration(), "controls")
            .unwrap();
        // Every source felt at once, the swipe's later pads after that.
        let swipe = std::thread::spawn(move || {
            controls
                .publish_json(1, &state(1, &["front"], true))
                .unwrap();
            for (n, bit) in [1u16, 2, 3].into_iter().enumerate() {
                std::thread::sleep(Duration::from_millis(120));
                controls
                    .publish_json(2 + n as u64, &state(1 << bit, &[], false))
                    .unwrap();
            }
            controls
        });
        let asked = std::cell::Cell::new(0);
        let started = Instant::now();
        let result = ByHand {
            bus: &bus,
            given: Duration::from_millis(600),
            ask: Box::new(|_| asked.set(asked.get() + 1)),
        }
        .run(Instant::now() + Duration::from_secs(60));
        let _controls = swipe.join().unwrap();
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert_eq!(value(&result, "strip"), Some("E0,E1,E2,E3"), "{result:?}");
        assert_eq!(asked.get(), 1);
        assert!(started.elapsed() >= Duration::from_millis(600));
        std::fs::remove_dir_all(root).unwrap();
    }
}
