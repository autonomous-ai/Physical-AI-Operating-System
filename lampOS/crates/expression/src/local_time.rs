//! The lamp's local time (#820): a fixed offset east of UTC, as before, or a
//! time zone whose offset is taken at each moment, so the hour of the day and
//! the night stay right across a daylight-saving change in a home outside
//! Vietnam. `--zone system` reads the zone the lamp's setup wrote
//! (`/etc/localtime`, `docs/setup.md`).

use jiff::{Timestamp, tz::TimeZone};
use std::io;

#[derive(Clone, Debug)]
pub enum LocalTime {
    /// Minutes east of UTC, the same all year (`--utc-offset-minutes`).
    Offset(i32),
    /// A zone (`--zone system` or `--zone Asia/Ho_Chi_Minh`).
    Zone(TimeZone),
}

impl LocalTime {
    /// `system`, or an IANA zone name. The zone data is bundled, so a lamp
    /// missing its own still knows the zones. No silent UTC: a zone that
    /// cannot be found is an error.
    pub fn zone(name: &str) -> io::Result<Self> {
        let zone = if name == "system" {
            TimeZone::try_system()
                .or_else(|_| {
                    // /etc/localtime unreadable: the name /etc/timezone keeps.
                    let named = std::fs::read_to_string("/etc/timezone")?;
                    TimeZone::get(named.trim()).map_err(io::Error::other)
                })
                .map_err(|error| {
                    io::Error::other(format!("the system's time zone is unknown: {error}"))
                })?
        } else {
            TimeZone::get(name).map_err(|error| {
                io::Error::other(format!("time zone {name} is unknown: {error}"))
            })?
        };
        Ok(Self::Zone(zone))
    }

    /// Minutes east of UTC at the moment `unix_ms`.
    pub fn offset_minutes_at(&self, unix_ms: u64) -> i32 {
        match self {
            Self::Offset(minutes) => *minutes,
            Self::Zone(zone) => i64::try_from(unix_ms)
                .ok()
                .and_then(|ms| Timestamp::from_millisecond(ms).ok())
                .map_or(0, |at| zone.to_offset(at).seconds() / 60),
        }
    }

    /// The local minute of the day at `unix_ms`.
    pub fn minute_of_day(&self, unix_ms: u64) -> u16 {
        crate::runtime::minute_of_day(unix_ms, self.offset_minutes_at(unix_ms))
    }
}
