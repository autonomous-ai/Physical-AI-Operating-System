//! What the room microphone hears, as levels only (`lamp-ears`,
//! `docs/ears.md`). No sound is in either message.
use crate::Invalid;
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The quietest a level is said to be, in dBFS: digital silence is said as
/// this, never as minus infinity.
pub const EARS_SILENCE_DB: f32 = -120.0;

fn level(name: &str, value: f32) -> Result<(), Invalid> {
    if value.is_finite() && (EARS_SILENCE_DB..=0.0).contains(&value) {
        Ok(())
    } else {
        Err(Invalid(format!(
            "{name} is {value}: dBFS from {EARS_SILENCE_DB} to 0"
        )))
    }
}

/// How loud the room is at the lamp now (`ears/level`): the last 100 ms of
/// the room microphone against the room's own quiet, in dBFS.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoomSound {
    pub level_db: f32,
    /// The room's quiet: the tenth percentile of the last 30 s heard while
    /// the lamp was not speaking. None until two seconds of it were heard.
    pub floor_db: Option<f32>,
    /// The lamp was speaking, or had stopped less than half a second ago:
    /// its own voice is in the level.
    pub own_voice: bool,
}

impl RoomSound {
    pub const TYPE: &str = "lamp_msgs/RoomSound";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "ears/level";

    /// The level now replaces the one before, ten times a second.
    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Latest).every(Duration::from_millis(100))
    }

    pub fn check(&self) -> Result<(), Invalid> {
        level("level_db", self.level_db)?;
        match self.floor_db {
            Some(floor) => level("floor_db", floor),
            None => Ok(()),
        }
    }
}

/// A sound that rose suddenly out of the room's quiet (`ears/sudden`): a
/// knock, a clap, something dropped, a door, or a voice beginning close by.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SuddenSound {
    /// Its loudest 20 ms, in dBFS.
    pub peak_db: f32,
    /// The room's quiet it rose out of, in dBFS.
    pub floor_db: f32,
    /// How long it stayed 10 dB or more over the quiet, in the 300 ms it was
    /// watched.
    pub loud_ms: u32,
    /// Loud for less than 200 ms: a knock, a clap, something dropped. If
    /// not, it went on: a voice, a chair dragged, music.
    pub short: bool,
    /// How long before this was sent it began, in milliseconds.
    pub ago_ms: u32,
}

impl SuddenSound {
    pub const TYPE: &str = "lamp_msgs/SuddenSound";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "ears/sudden";

    /// Each is over when it has been sent. None is promised.
    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Event)
    }

    pub fn check(&self) -> Result<(), Invalid> {
        level("peak_db", self.peak_db)?;
        level("floor_db", self.floor_db)?;
        if self.peak_db < self.floor_db {
            return Err(Invalid(format!(
                "peak_db {} is under floor_db {}",
                self.peak_db, self.floor_db
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_room_level_is_two_levels_and_whether_the_lamp_was_speaking() {
        let heard = RoomSound {
            level_db: -44.5,
            floor_db: Some(-51.0),
            own_voice: false,
        };
        let text = serde_json::to_string(&heard).unwrap();
        assert_eq!(
            text,
            "{\"level_db\":-44.5,\"floor_db\":-51.0,\"own_voice\":false}"
        );
        assert_eq!(serde_json::from_str::<RoomSound>(&text).unwrap(), heard);
        assert_eq!(heard.check(), Ok(()));
        let unknown = RoomSound {
            floor_db: None,
            ..heard
        };
        assert_eq!(unknown.check(), Ok(()));
        let odd = RoomSound {
            level_db: 3.0,
            ..heard
        };
        assert!(odd.check().unwrap_err().0.contains("level_db"));
        let declared = RoomSound::declaration();
        assert_eq!(
            (declared.kind, declared.period()),
            (Kind::Latest, Some(Duration::from_millis(100)))
        );
    }

    #[test]
    fn a_sudden_sound_is_how_loud_out_of_what_quiet_and_for_how_long() {
        let heard = SuddenSound {
            peak_db: -14.0,
            floor_db: -50.5,
            loud_ms: 60,
            short: true,
            ago_ms: 300,
        };
        let text = serde_json::to_string(&heard).unwrap();
        assert_eq!(serde_json::from_str::<SuddenSound>(&text).unwrap(), heard);
        assert_eq!(heard.check(), Ok(()));
        let odd = SuddenSound {
            peak_db: -60.0,
            ..heard
        };
        assert!(odd.check().unwrap_err().0.contains("under"));
        let declared = SuddenSound::declaration();
        assert_eq!((declared.kind, declared.period()), (Kind::Event, None));
    }
}
