//! How the air is, told to the voice.
//!
//! The lamp has an air sensor (`lamp-air`, `air/state`). A stuffy room is
//! worth one short remark from a creature that lives on the desk, and never
//! a nag: the research in `docs/creature.md` (a CO2 bird that droops, opt-in
//! air announcements) says show it quietly first and speak only when the
//! person is already talking to it. So the model is told, beside something
//! a person is saying to the lamp, that the air has been stuffy for a while,
//! and may suggest a window once if it fits. Only after the air has been
//! stuffy for ten minutes, only in a room that is not busy, and not again
//! for three hours.
//!
//! Off unless the service's environment says `on`.
use std::time::Duration;

/// Where the coordinator reads whether the air is told. `on` only.
pub const ENVIRONMENT: &str = "LAMP_AIR_FACTS";
/// The provider worker's flag for it.
pub const FLAG: &str = "--air-facts";

/// Stuffy for this long before it is worth a word.
const STUFFY_FOR: Duration = Duration::from_secs(600);
/// And not again for this long once said.
const AGAIN_AFTER: Duration = Duration::from_secs(3 * 3600);
/// A reading older than this says nothing of now.
pub const FRESH_FOR: Duration = Duration::from_secs(30);

/// What the provider worker knows of the air, from `air/state`.
#[derive(Debug, Default)]
pub struct AirTold {
    stuffy_since_us: Option<u64>,
    ppm: Option<u16>,
    told_at_us: Option<u64>,
}

impl AirTold {
    /// The newest reading: whether the air feels stuffy (or very stuffy),
    /// and its carbon dioxide. A reading that is not stuffy, or none,
    /// starts the wait again.
    pub fn seen(&mut self, stuffy: bool, ppm: Option<u16>, now_us: u64) {
        if stuffy {
            self.stuffy_since_us.get_or_insert(now_us);
            self.ppm = ppm;
        } else {
            self.stuffy_since_us = None;
            self.ppm = None;
        }
    }

    /// The note, if one is due now.
    pub fn due(&self, now_us: u64) -> Option<String> {
        let since = self.stuffy_since_us?;
        let stuffy_us = now_us.saturating_sub(since);
        if stuffy_us < STUFFY_FOR.as_micros() as u64 {
            return None;
        }
        if self
            .told_at_us
            .is_some_and(|at| now_us.saturating_sub(at) < AGAIN_AFTER.as_micros() as u64)
        {
            return None;
        }
        Some(note(stuffy_us / 60_000_000, self.ppm))
    }

    /// The note was given.
    pub fn told(&mut self, now_us: u64) {
        self.told_at_us = Some(now_us);
    }
}

/// What the model is told.
pub fn note(minutes: u64, ppm: Option<u16>) -> String {
    let level = ppm.map_or(String::new(), |ppm| {
        format!(
            ", carbon dioxide about {}",
            (u32::from(ppm) + 50) / 100 * 100
        )
    });
    format!(
        "(For you to know, not to answer: the air at this desk has been stuffy for about {minutes} minutes{level}. If it fits what you are saying, you may suggest opening a window, once and briefly; otherwise say nothing about it.)"
    )
}

/// The provider worker's argument for the switch in the environment.
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

/// Take a leading `--air-facts` off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000_000;

    #[test]
    fn stuffy_for_ten_minutes_is_told_once_then_not_for_three_hours() {
        let mut air = AirTold::default();
        air.seen(true, Some(1_871), 0);
        assert_eq!(air.due(9 * MIN), None);
        let note = air.due(12 * MIN).unwrap();
        assert!(
            note.contains("about 12 minutes") && note.contains("carbon dioxide about 1900"),
            "{note}"
        );
        assert!(note.starts_with("(For you to know, not to answer"));
        air.told(12 * MIN);
        air.seen(true, Some(1_900), 60 * MIN);
        assert_eq!(air.due(60 * MIN), None);
        assert!(air.due(12 * MIN + 180 * MIN).is_some());
    }

    #[test]
    fn fresh_air_starts_the_wait_again() {
        let mut air = AirTold::default();
        air.seen(true, Some(1_500), 0);
        air.seen(false, Some(700), 8 * MIN);
        air.seen(true, Some(1_300), 9 * MIN);
        assert_eq!(air.due(15 * MIN), None);
        assert!(air.due(20 * MIN).is_some());
        assert_eq!(AirTold::default().due(100 * MIN), None);
    }

    #[test]
    fn the_switch_is_on_only_when_set_and_its_flag_is_taken_off() {
        assert_eq!(arguments(Some("on")), [FLAG]);
        assert!(arguments(None).is_empty() && arguments(Some("yes")).is_empty());
        let flags = vec![FLAG.to_owned(), "--other".to_owned()];
        assert_eq!(take(&flags), (true, &flags[1..]));
        assert_eq!(take(&flags[1..]), (false, &flags[1..]));
        assert!(note(10, None).contains("about 10 minutes."));
    }
}
