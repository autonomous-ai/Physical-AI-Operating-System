//! Metadata for the Schedule owner's durable due queue. A bus value is neither
//! a durable delivery record nor permission to speak; the owner serves the
//! exact notice over its private socket and records completion there.
use crate::{ClockStamp, Invalid};
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Pending notices and schedules waiting for a usable clock share this bound.
pub const SCHEDULE_MAX_ITEMS: u32 = 128;
/// Last millisecond of 9999-12-31 UTC.
pub const SCHEDULE_MAX_UNIX_MS: i64 = 253_402_300_799_999;

/// One pending occurrence, without the person's reminder text. The store keeps
/// this identity until an explicit durable acknowledgement, cancellation, or
/// snooze. Renewing a bus heartbeat does not create another delivery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleNotice {
    pub id: u64,
    pub delivery: u64,
    /// Scheduled instant, in milliseconds since 1970 UTC, through year 9999.
    pub due_unix_ms: i64,
    /// Current known lateness in milliseconds; absent when the clock cannot
    /// establish it. Absence must not be interpreted as zero lateness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub late_by_ms: Option<u64>,
    pub late: bool,
}

impl ScheduleNotice {
    pub fn check(&self) -> Result<(), Invalid> {
        if self.id == 0 || self.delivery == 0 {
            return Err(Invalid("schedule id and delivery must be positive".into()));
        }
        if !(0..=SCHEDULE_MAX_UNIX_MS).contains(&self.due_unix_ms) {
            return Err(Invalid(
                "schedule due_unix_ms must be within years 1970 through 9999".into(),
            ));
        }
        Ok(())
    }
}

/// The oldest actionable notice from the Schedule owner's persistent queue.
/// Consumers validate freshness, host boot, publisher identity and ordering,
/// then retrieve this exact store/id/delivery from the private socket before
/// considering speech. Only confirmed completion may be acknowledged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleDue {
    pub stamp: ClockStamp,
    /// Persistent store identity: exactly 32 lowercase hexadecimal characters.
    pub store_id: String,
    pub revision: u64,
    /// False in shadow mode: retained metadata cannot authorize an action.
    pub enabled: bool,
    /// Whether the Schedule owner can currently serve due work, not whether
    /// Voice can speak or retain a person's input.
    pub ready: bool,
    pub pending: u32,
    pub waiting_for_clock: u32,
    /// Present exactly when enabled, ready and pending is positive. Disabled
    /// or unavailable owners may still report counts but offer no notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest: Option<ScheduleNotice>,
}

impl ScheduleDue {
    pub const TYPE: &str = "lamp_msgs/ScheduleDue";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "schedule/due";

    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Latest)
            .every(Duration::from_secs(1))
            .lasting(Duration::from_secs(3))
    }

    /// Structural validation only. The receiver must independently validate
    /// current clock/boot, message age and monotonic publisher ordering.
    pub fn check(&self) -> Result<(), Invalid> {
        self.stamp.check()?;
        if self.store_id.len() != 32
            || !self
                .store_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Invalid(
                "schedule store_id must be 32 lowercase hexadecimal characters".into(),
            ));
        }
        if self.pending > SCHEDULE_MAX_ITEMS
            || self.waiting_for_clock > SCHEDULE_MAX_ITEMS
            || self.pending + self.waiting_for_clock > SCHEDULE_MAX_ITEMS
        {
            return Err(Invalid(
                "schedule pending and waiting_for_clock must total at most 128".into(),
            ));
        }
        if self.oldest.is_some() != (self.pending > 0 && self.enabled && self.ready) {
            return Err(Invalid(
                "schedule oldest requires enabled, ready and positive pending, and is required then"
                    .into(),
            ));
        }
        if let Some(notice) = &self.oldest {
            notice.check()?;
        }
        Ok(())
    }
}

/// Optional Schedule topics, separate from the original seventeen topics.
pub fn schedule_topics() -> Vec<(&'static str, Declaration)> {
    vec![(ScheduleDue::TOPIC, ScheduleDue::declaration())]
}
