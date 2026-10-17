//! A passive run waits for the voice to be out of a turn (#812, the first
//! builder: it never runs while the lamp is in a turn).
//!
//! The timed run (`--occasion first-boot`) lights, plays and moves nothing,
//! but it reads the microphone and the bus and takes a little of the lamp's
//! processor, which a turn needs. So it begins only once `voice/state` has
//! said `resting`, or nothing fresh (no voice running), on [`CALM`] looks in
//! a row, one a second: the voice rests for a moment between the lamp's
//! answer and the person's next words too, and that is still a conversation
//! (the seventh on #1245; `crates/update/src/quiet.rs` waits 5 s the same
//! way). It looks for at most two minutes; a conversation longer than that
//! leaves this run for the next one. A run a person asked for (the
//! factory's, or on demand) begins at once.
use crate::listen;
use lamp_bus::Bus;
use lamp_msgs::{Voice, VoiceState};
use std::time::Duration;

/// How long a passive run waits for a turn to end before it leaves the
/// run for the next time.
pub const MOST: Duration = Duration::from_secs(120);
/// How often it looks again.
pub const EVERY: Duration = Duration::from_secs(1);
/// How many looks in a row must find the voice out of a turn.
pub const CALM: u32 = 5;
/// A `voice/state` older than this says nothing: the voice publishes it
/// once a second, and a voice that stopped is in no turn.
pub const FRESH_MS: u64 = 3_000;

/// Whether the voice is in a turn, from its newest state and that state's
/// age: listening, thinking or speaking, said within [`FRESH_MS`].
pub fn in_a_turn(state: Option<(Voice, u64)>) -> bool {
    state.is_some_and(|(voice, age_ms)| age_ms <= FRESH_MS && voice != Voice::Resting)
}

/// Wait until the voice has been out of a turn for [`CALM`] looks in a
/// row: true when it has, false when [`MOST`] has passed without that.
/// `state` reads the voice's newest state; `sleep` waits [`EVERY`].
pub fn wait(
    mut state: impl FnMut() -> Option<(Voice, u64)>,
    mut sleep: impl FnMut(Duration),
) -> bool {
    let (mut waited, mut calm) = (Duration::ZERO, 0);
    loop {
        calm = if in_a_turn(state()) { 0 } else { calm + 1 };
        if calm >= CALM {
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
    let seen = listen::newest::<VoiceState>(bus, VoiceState::TOPIC).ok()??;
    Some((seen.value.state, seen.age.as_millis() as u64))
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

    /// Resting all along: five looks, four waits. A turn that ends after 5 s:
    /// five calm looks after it. A rest of one look between two turns of one
    /// conversation (the seventh on #1245) is no calm. A turn that goes on:
    /// it gives up after two minutes of looks, one a second.
    #[test]
    fn a_passive_run_waits_for_five_calm_looks_and_no_longer_than_two_minutes() {
        let mut slept = Vec::new();
        assert!(wait(|| Some((Voice::Resting, 0)), |d| slept.push(d)));
        assert_eq!(slept, vec![EVERY; CALM as usize - 1]);

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
        assert_eq!(slept.len(), 5 + CALM as usize - 1);

        // Speaking, one look resting, listening again, then resting.
        let script = [
            Voice::Speaking,
            Voice::Resting,
            Voice::Listening,
            Voice::Resting,
            Voice::Resting,
            Voice::Resting,
            Voice::Resting,
            Voice::Resting,
        ];
        let mut at = 0;
        let mut slept = 0;
        assert!(wait(
            || {
                let voice = script[at.min(script.len() - 1)];
                at += 1;
                Some((voice, 0))
            },
            |_| slept += 1
        ));
        assert_eq!(at, script.len());

        let mut slept = Vec::new();
        assert!(!wait(|| Some((Voice::Thinking, 0)), |d| slept.push(d)));
        assert_eq!(slept.len() as u64, MOST.as_secs());
    }
}
