//! Optional integration helper for the owners. Accepting a request is not
//! evidence that sound stopped, motion stopped, or input permission changed.
use crate::mute::MuteRecord;
use lamp_msgs::{ControlGesture, ControlIntent, ControlsState};
use std::{collections::BTreeSet, io, path::Path};

const STATE_FRESH_US: u64 = 300_000;
const MAX_RETIRED: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub stop: bool,
    /// Revoke immediately, then persist the record before acknowledging mute.
    pub mute_changed: bool,
    pub software_muted: bool,
}

pub struct Receiver {
    record: MuteRecord,
    session: Option<String>,
    retired: BTreeSet<String>,
    state_sequence: Option<u64>,
    event_sequence: u64,
    pet_sequence: u64,
    state_at_us: Option<u64>,
    last_now: Option<u64>,
    exhausted: bool,
}

fn invalid(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}

impl Receiver {
    pub fn new(record: MuteRecord) -> io::Result<Self> {
        record.check()?;
        Ok(Self {
            record,
            session: None,
            retired: BTreeSet::new(),
            state_sequence: None,
            event_sequence: 0,
            pet_sequence: 0,
            state_at_us: None,
            last_now: None,
            exhausted: false,
        })
    }

    pub fn mute_record(&self) -> &MuteRecord {
        &self.record
    }

    /// Must only be invoked for a separate explicit user request. This does
    /// not inspect or clear physical privacy and cannot return a capture permit.
    /// This explicit disk transaction belongs outside the urgent event loop.
    /// A save failure leaves the live inhibit intact.
    pub fn clear_after_explicit_user_request(
        &mut self,
        now_us: u64,
        path: &Path,
    ) -> io::Result<()> {
        self.clock(now_us)?;
        let mut proposed = self.record.clone();
        proposed.clear_after_explicit_user_request(now_us)?;
        proposed.save(path)?;
        self.record = proposed;
        Ok(())
    }

    fn clock(&mut self, now_us: u64) -> io::Result<()> {
        if self.exhausted || self.last_now.is_some_and(|last| now_us < last) {
            self.exhausted = true;
            return Err(invalid(
                "controls receiver clock/session fence failed; restart with restored mute",
            ));
        }
        self.last_now = Some(now_us);
        Ok(())
    }

    /// Consume the retained state as well as subscribing to gesture events.
    /// Pet cannot overwrite its independent stop and mute records. A recent
    /// state can carry an older one-way mute after a lost event; a Stop expires.
    pub fn observe(&mut self, now_us: u64, state: &ControlsState) -> io::Result<Effects> {
        self.clock(now_us)?;
        state.check().map_err(|e| invalid(e.to_string()))?;
        if !state.enabled
            || state.stamp.clock_id != self.record.clock_id
            || state.stamp.at_us > now_us
            || now_us - state.stamp.at_us >= STATE_FRESH_US
        {
            return Err(invalid(
                "disabled, foreign-clock, future or stale controls state",
            ));
        }
        if self.state_at_us.is_some_and(|at| state.stamp.at_us < at) {
            return Err(invalid("controls state observation time moved backwards"));
        }
        if self.session.as_deref() != Some(&state.session_id) {
            if self.retired.contains(&state.session_id) || self.retired.len() == MAX_RETIRED {
                if self.retired.len() == MAX_RETIRED {
                    self.exhausted = true;
                }
                return Err(invalid("retired controls session or full session fence"));
            }
            if let Some(old) = self.session.replace(state.session_id.clone()) {
                self.retired.insert(old);
            }
            self.state_sequence = None;
            self.event_sequence = 0;
            self.pet_sequence = 0;
        }
        let mut effects = Effects {
            software_muted: self.record.muted,
            ..Effects::default()
        };
        if let Some(last) = self.state_sequence {
            if state.stamp.sequence < last {
                return Err(invalid("controls state sequence moved backwards"));
            }
            if state.stamp.sequence == last {
                return Ok(effects);
            }
        }
        if state.event_sequence < self.event_sequence {
            return Err(invalid(
                "controls cumulative event sequence moved backwards",
            ));
        }
        self.state_sequence = Some(state.stamp.sequence);
        self.state_at_us = Some(state.stamp.at_us);
        if let Some(mute) = &state.latest_mute
            && self.record.request_mute(mute.released_us)
        {
            effects.mute_changed = true;
            effects.stop = true;
        }
        if let Some(stop) = &state.latest_stop
            && stop.stamp.sequence > self.event_sequence
            && stop.fresh_at(&self.record.clock_id, now_us)
        {
            effects.stop = true;
        }
        // State is cumulative. Advancing across a missing event prevents a
        // late duplicate from causing another action.
        self.event_sequence = self.event_sequence.max(state.event_sequence);
        effects.software_muted = self.record.muted;
        Ok(effects)
    }

    /// Pet is advisory only and requires a previously accepted owner state.
    /// Stop/mute are consumed through observe(), so event loss is recoverable.
    pub fn pet(&mut self, now_us: u64, event: &ControlGesture) -> io::Result<bool> {
        self.clock(now_us)?;
        if event.intent != ControlIntent::Pet
            || !event.fresh_at(&self.record.clock_id, now_us)
            || self.session.as_deref() != Some(&event.session_id)
            || self
                .state_at_us
                .is_none_or(|at| now_us - at >= STATE_FRESH_US)
        {
            return Err(invalid(
                "pet is not a fresh event from the accepted controls owner",
            ));
        }
        // Event and state have independent delivery. A pet included only in
        // state's event_sequence is still not evidence that it was delivered.
        // Use a separate event consumer for optional pet reactions; state
        // intentionally contains no latched pet that could replay on startup.
        if event.stamp.sequence <= self.pet_sequence {
            return Ok(false);
        }
        self.pet_sequence = event.stamp.sequence;
        Ok(true)
    }
}
