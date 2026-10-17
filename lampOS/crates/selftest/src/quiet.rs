//! A passive run waits for the voice to be out of a turn (#812, the first
//! builder: it never runs while the lamp is in a turn).
//!
//! The timed run (`--occasion first-boot`) lights, plays and moves nothing,
//! but it reads the microphone and the bus and takes a little of the lamp's
//! processor, which a turn needs. So it begins only while `voice/state`
//! says `resting`, or says nothing fresh (no voice running). While a turn
//! goes on it looks again each second, for at most two minutes; a turn
//! longer than that leaves this run for the next one. A run a person asked
//! for (the factory's, or on demand) begins at once.
use lamp_bus::Bus;
use lamp_msgs::{Voice, VoiceState};
use std::time::Duration;

/// How long a passive run waits for a turn to end before it leaves the
/// run for the next time.
pub const MOST: Duration = Duration::from_secs(120);
/// How often it looks again.
pub const EVERY: Duration = Duration::from_secs(1);
/// A `voice/state` older than this says nothing: the voice publishes it
/// once a second, and a voice that stopped is in no turn.
pub const FRESH_MS: u64 = 3_000;

/// Whether the voice is in a turn, from its newest state and that state's
/// age: listening, thinking or speaking, said within [`FRESH_MS`].
pub fn in_a_turn(state: Option<(Voice, u64)>) -> bool {
    state.is_some_and(|(voice, age_ms)| age_ms <= FRESH_MS && voice != Voice::Resting)
}

/// Wait until the voice is out of a turn: true when it is (at once if it
/// already was), false when [`MOST`] has passed with the turn going on.
/// `state` reads the voice's newest state; `sleep` waits [`EVERY`].
pub fn wait(
    mut state: impl FnMut() -> Option<(Voice, u64)>,
    mut sleep: impl FnMut(Duration),
) -> bool {
    let mut waited = Duration::ZERO;
    loop {
        if !in_a_turn(state()) {
            return true;
        }
        if waited >= MOST {
            return false;
        }
        sleep(EVERY);
        waited += EVERY;
    }
}

/// The voice's newest state on `bus`, and how old it is.
pub fn read(bus: &Bus) -> Option<(Voice, u64)> {
    let message = bus.latest(VoiceState::TOPIC).ok()??;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let age_ms = now_ms.saturating_sub(message.header.at_unix_ms);
    let state: VoiceState = message.parse().ok()?;
    Some((state.state, age_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_is_a_fresh_state_other_than_resting() {
        assert!(!in_a_turn(None));
        assert!(!in_a_turn(Some((Voice::Resting, 0))));
        for voice in [Voice::Listening, Voice::Thinking, Voice::Speaking] {
            assert!(in_a_turn(Some((voice, 0))), "{voice:?}");
            assert!(in_a_turn(Some((voice, FRESH_MS))), "{voice:?}");
            // A voice that stopped mid-turn is in none.
            assert!(!in_a_turn(Some((voice, FRESH_MS + 1))), "{voice:?}");
        }
    }

    /// Resting at once: no wait. A turn that ends after 5 s: it begins
    /// then. A turn that goes on: it gives up after two minutes of looks,
    /// one a second.
    #[test]
    fn a_passive_run_waits_for_a_turn_to_end_and_no_longer_than_two_minutes() {
        let mut slept = Vec::new();
        assert!(wait(|| Some((Voice::Resting, 0)), |d| slept.push(d)));
        assert!(slept.is_empty());

        let mut looks = 0;
        let mut slept = Vec::new();
        assert!(wait(
            || {
                looks += 1;
                Some((
                    if looks > 5 {
                        Voice::Resting
                    } else {
                        Voice::Speaking
                    },
                    0,
                ))
            },
            |d| slept.push(d)
        ));
        assert_eq!(slept, vec![EVERY; 5]);

        let mut slept = Vec::new();
        assert!(!wait(|| Some((Voice::Thinking, 0)), |d| slept.push(d)));
        assert_eq!(slept.len() as u64, MOST.as_secs());
    }
}
