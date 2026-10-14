//! Reset asks each owner to freeze and empty its own store. It never walks or
//! deletes another component's files, and partial reset is never success.
use crate::atomic;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

pub const MAX_STATE: usize = 32_768;
pub const RECEIPT_AGE_US: u64 = 5_000_000;
pub const REQUEST_TIMEOUT_US: u64 = 30_000_000;
pub const QUIET_TIMEOUT_US: u64 = 120_000_000;
pub const MAX_OPERATIONS: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    Voice,
    Memory,
    Schedule,
    Tasks,
    Settings,
    Configuration,
    Network,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preparing,
    Emptying,
    Complete,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Prepare,
    Empty,
}

/// Voice's prepare can accumulate the required 45-second quiet window after
/// admission. Every erase and other owner's prepare keeps the 30-second cap.
pub const fn timeout_us(store: Store, kind: Kind) -> u64 {
    match (store, kind) {
        (Store::Voice, Kind::Prepare) => QUIET_TIMEOUT_US,
        _ => REQUEST_TIMEOUT_US,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    Unavailable,
    Refused,
    Timeout,
    OwnerChanged,
    ClockChanged,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub operation_id: String,
    pub coordinator_session: String,
    pub clock_id: String,
    pub request_id: String,
    pub store: Store,
    pub kind: Kind,
    /// Empty must be refused before erasing if the prepared owner changed.
    /// Prepare discovers the owner and therefore has no expected session yet.
    pub expected_owner_session: Option<String>,
    pub issued_us: u64,
    pub not_after_us: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub coordinator_session: String,
    pub clock_id: String,
    pub stores: Vec<Store>,
    pub phase: Phase,
    pub completed: Vec<Store>,
    pub prepared: BTreeMap<Store, String>,
    pub pending: Option<Action>,
    pub failure: Option<Failure>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema_version: u32,
    pub revision: u64,
    /// Never evict identities automatically: owners deduplicate erase requests
    /// by these IDs, so A/B/A reuse could otherwise report an old erase as new.
    #[serde(default)]
    pub used_operation_ids: Vec<String>,
    pub operation: Option<Operation>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            used_operation_ids: vec![],
            operation: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    /// The owner holds a durable reset barrier. Voice additionally certifies
    /// its 45-second quiet check, closed inputs and all output workers gone.
    Prepared {
        writes_frozen: bool,
        durable_barrier: bool,
        voice_quiet_and_closed: bool,
    },
    /// The owner's empty command is idempotent by operation/request ID, reads
    /// back emptiness, and keeps writes frozen until explicitly reprovisioned.
    Empty {
        read_back_empty: bool,
        writes_frozen: bool,
    },
    Failed {
        failure: Failure,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub operation_id: String,
    pub coordinator_session: String,
    pub clock_id: String,
    pub request_id: String,
    pub owner_session: String,
    pub observed_us: u64,
    pub outcome: Outcome,
}

pub struct Reset {
    enabled: bool,
    path: PathBuf,
    clock: String,
    session: String,
    now_us: u64,
    state: State,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

impl State {
    pub fn check(&self) -> io::Result<()> {
        if self.schema_version != 1
            || self.revision == u64::MAX
            || self.used_operation_ids.len() > MAX_OPERATIONS
        {
            return Err(invalid("unknown reset record version"));
        }
        for (index, id) in self.used_operation_ids.iter().enumerate() {
            if !valid_id(id) || self.used_operation_ids[..index].contains(id) {
                return Err(invalid("invalid or reused reset operation identity"));
            }
        }
        if let Some(op) = &self.operation {
            let mut sorted = op.stores.clone();
            sorted.sort();
            sorted.dedup();
            if !valid_id(&op.id)
                || !valid_id(&op.coordinator_session)
                || !valid_id(&op.clock_id)
                || self.used_operation_ids.last() != Some(&op.id)
                || op.stores.is_empty()
                || op.stores.len() > 7
                || sorted.len() != op.stores.len()
                || op.stores.first() != Some(&Store::Voice)
                || !op.stores.starts_with(&op.completed)
                || op
                    .prepared
                    .iter()
                    .any(|(s, id)| !op.stores.contains(s) || !valid_id(id))
            {
                return Err(invalid("invalid reset record"));
            }
            if !op
                .stores
                .iter()
                .take(op.prepared.len())
                .all(|store| op.prepared.contains_key(store))
            {
                return Err(invalid(
                    "prepared owners must be a prefix of the configured store order",
                ));
            }
            let complete = op.completed == op.stores;
            let all_prepared = op.prepared.len() == op.stores.len();
            let valid_phase = match op.phase {
                Phase::Preparing => !complete && op.failure.is_none(),
                Phase::Emptying => !complete && all_prepared && op.failure.is_none(),
                Phase::Complete => {
                    complete && all_prepared && op.failure.is_none() && op.pending.is_none()
                }
                Phase::Incomplete => !complete && op.failure.is_some() && op.pending.is_none(),
            };
            if !valid_phase {
                return Err(invalid(
                    "reset phase contradicts completion, barriers or failure",
                ));
            }
            if let Some(action) = &op.pending {
                let (kind, expected_store, expected_owner) = match op.phase {
                    Phase::Preparing => (Kind::Prepare, op.stores.get(op.prepared.len()), None),
                    Phase::Emptying => {
                        let store = op.stores.get(op.completed.len());
                        (
                            Kind::Empty,
                            store,
                            store.and_then(|store| op.prepared.get(store)),
                        )
                    }
                    _ => return Err(invalid("terminal reset has a pending intent")),
                };
                if action.operation_id != op.id
                    || Some(&action.store) != expected_store
                    || action.kind != kind
                    || action.coordinator_session != op.coordinator_session
                    || action.clock_id != op.clock_id
                    || action.expected_owner_session.as_ref() != expected_owner
                    || action.request_id != request_id(&op.id, action.store, kind)
                    || action.not_after_us.checked_sub(action.issued_us)
                        != Some(timeout_us(action.store, kind))
                {
                    return Err(invalid("invalid reset intent"));
                }
            }
        } else if !self.used_operation_ids.is_empty() {
            return Err(invalid("reset identity history has no latest operation"));
        }
        Ok(())
    }
}

impl Reset {
    /// Caller holds DirectoryLock for the state directory. An interrupted reset
    /// re-prepares every owner; persisted old receipts cannot authorize a wipe.
    pub fn open(
        enabled: bool,
        path: &Path,
        clock: &str,
        session: &str,
        now_us: u64,
    ) -> io::Result<Self> {
        if !valid_id(clock) || !valid_id(session) {
            return Err(invalid("invalid reset clock/session"));
        }
        let mut state: State = match atomic::read(path, MAX_STATE)? {
            Some(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| invalid("corrupt reset record"))?
            }
            None => State::default(),
        };
        state.check()?;
        let mut changed = false;
        if enabled
            && let Some(op) = &mut state.operation
            && op.phase != Phase::Complete
        {
            if op.coordinator_session == session {
                return Err(invalid(
                    "interrupted reset needs a fresh coordinator session",
                ));
            }
            let failure = if op.clock_id != clock {
                Failure::ClockChanged
            } else {
                op.failure.unwrap_or(Failure::Unavailable)
            };
            op.coordinator_session = session.into();
            op.clock_id = clock.into();
            op.pending = None;
            op.prepared.clear();
            op.phase = Phase::Incomplete;
            op.failure = Some(failure);
            changed = true;
        }
        let mut out = Self {
            enabled,
            path: path.into(),
            clock: clock.into(),
            session: session.into(),
            now_us,
            state: state.clone(),
        };
        if changed {
            out.save(state)?;
        }
        Ok(out)
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    fn save(&mut self, mut state: State) -> io::Result<()> {
        state.revision = self
            .state
            .revision
            .max(state.revision)
            .checked_add(1)
            .ok_or_else(|| invalid("reset revision exhausted"))?;
        state.check()?;
        let bytes = serde_json::to_vec(&state).map_err(io::Error::other)?;
        atomic::write(&self.path, &bytes, MAX_STATE)?;
        self.state = state;
        Ok(())
    }

    fn clock(&mut self, now_us: u64) -> io::Result<()> {
        if now_us < self.now_us {
            let was_enabled = self.enabled;
            self.enabled = false;
            if was_enabled {
                let mut state = self.state.clone();
                if let Some(op) = &mut state.operation
                    && !matches!(op.phase, Phase::Complete | Phase::Incomplete)
                {
                    op.phase = Phase::Incomplete;
                    op.failure = Some(Failure::ClockChanged);
                    op.pending = None;
                    self.save(state)?;
                }
            }
            return Err(invalid("reset clock moved backwards; restart required"));
        }
        self.now_us = now_us;
        Ok(())
    }

    /// The operator must confirm the displayed store list. Runtime configuration
    /// supplies that exact list; it is never accepted as arbitrary file paths.
    pub fn begin(
        &mut self,
        id: &str,
        stores: Vec<Store>,
        confirmed: bool,
        now_us: u64,
    ) -> io::Result<()> {
        self.clock(now_us)?;
        if !self.enabled || !confirmed {
            return Err(invalid("reset needs enablement and explicit confirmation"));
        }
        if !valid_id(id)
            || self.state.used_operation_ids.len() == MAX_OPERATIONS
            || self.state.used_operation_ids.iter().any(|old| old == id)
            || self
                .state
                .operation
                .as_ref()
                .is_some_and(|op| op.phase != Phase::Complete || op.id == id)
        {
            return Err(invalid("reset already active or operation identity reused"));
        }
        let mut state = self.state.clone();
        state.used_operation_ids.push(id.into());
        state.operation = Some(Operation {
            id: id.into(),
            coordinator_session: self.session.clone(),
            clock_id: self.clock.clone(),
            stores,
            phase: Phase::Preparing,
            completed: vec![],
            prepared: BTreeMap::new(),
            pending: None,
            failure: None,
        });
        self.save(state)
    }

    /// Retry is explicit, retains completed stores and re-establishes every
    /// owner's barrier. No erase is undone and no hidden automatic retry runs.
    pub fn resume(&mut self, now_us: u64) -> io::Result<()> {
        self.clock(now_us)?;
        if !self.enabled {
            return Err(invalid("reset is disabled"));
        }
        let mut state = self.state.clone();
        let op = state
            .operation
            .as_mut()
            .ok_or_else(|| invalid("no reset to resume"))?;
        if op.phase != Phase::Incomplete {
            return Err(invalid("reset is not incomplete"));
        }
        op.phase = Phase::Preparing;
        op.prepared.clear();
        op.pending = None;
        op.failure = None;
        self.save(state)
    }

    /// The returned intent has already been synced. Repeated calls return the
    /// same request; adapters and owners must deduplicate it, not launch twice.
    pub fn next(&mut self, now_us: u64) -> io::Result<Option<Action>> {
        self.clock(now_us)?;
        if !self.enabled {
            return Ok(None);
        }
        let mut state = self.state.clone();
        let Some(op) = &mut state.operation else {
            return Ok(None);
        };
        if matches!(op.phase, Phase::Complete | Phase::Incomplete) {
            return Ok(None);
        }
        if let Some(pending) = &op.pending {
            if now_us >= pending.not_after_us {
                op.phase = Phase::Incomplete;
                op.failure = Some(Failure::Timeout);
                op.pending = None;
                self.save(state)?;
                return Ok(None);
            }
            return Ok(Some(pending.clone()));
        }
        let (kind, store) =
            if let Some(store) = op.stores.iter().find(|s| !op.prepared.contains_key(s)) {
                (Kind::Prepare, *store)
            } else if let Some(store) = op.stores.iter().find(|s| !op.completed.contains(s)) {
                op.phase = Phase::Emptying;
                (Kind::Empty, *store)
            } else {
                op.phase = Phase::Complete;
                self.save(state)?;
                return Ok(None);
            };
        let request_id = request_id(&op.id, store, kind);
        let action = Action {
            operation_id: op.id.clone(),
            coordinator_session: self.session.clone(),
            clock_id: self.clock.clone(),
            request_id,
            store,
            kind,
            expected_owner_session: if kind == Kind::Empty {
                op.prepared.get(&store).cloned()
            } else {
                None
            },
            issued_us: now_us,
            not_after_us: now_us
                .checked_add(timeout_us(store, kind))
                .ok_or_else(|| invalid("reset deadline overflow"))?,
        };
        op.pending = Some(action.clone());
        self.save(state)?;
        Ok(Some(action))
    }

    pub fn observe(&mut self, now_us: u64, receipt: Receipt) -> io::Result<bool> {
        self.clock(now_us)?;
        if !self.enabled {
            return Ok(false);
        }
        let mut state = self.state.clone();
        let Some(op) = &mut state.operation else {
            return Ok(false);
        };
        let Some(action) = &op.pending else {
            return Ok(false);
        };
        if receipt.operation_id != op.id
            || receipt.request_id != action.request_id
            || receipt.coordinator_session != self.session
            || receipt.clock_id != self.clock
            || !valid_id(&receipt.owner_session)
            || receipt.observed_us < action.issued_us
            || receipt.observed_us > now_us
            || now_us - receipt.observed_us >= RECEIPT_AGE_US
            || now_us >= action.not_after_us
        {
            return Ok(false);
        }
        let store = action.store;
        let mut failure = None;
        match receipt.outcome {
            Outcome::Prepared {
                writes_frozen: true,
                durable_barrier: true,
                voice_quiet_and_closed,
            } if action.kind == Kind::Prepare
                && (store != Store::Voice || voice_quiet_and_closed) =>
            {
                op.prepared.insert(store, receipt.owner_session);
            }
            Outcome::Empty {
                read_back_empty: true,
                writes_frozen: true,
            } if action.kind == Kind::Empty => {
                if op.prepared.get(&store) == Some(&receipt.owner_session) {
                    op.completed.push(store);
                } else {
                    failure = Some(Failure::OwnerChanged);
                }
            }
            Outcome::Failed { failure: f } => failure = Some(f),
            _ => failure = Some(Failure::Refused),
        }
        op.pending = None;
        if let Some(failure) = failure {
            op.phase = Phase::Incomplete;
            op.failure = Some(failure);
        } else if op.completed == op.stores {
            op.phase = Phase::Complete;
        }
        self.save(state)?;
        Ok(true)
    }
}

fn request_id(operation_id: &str, store: Store, kind: Kind) -> String {
    format!("{operation_id}:{store:?}:{kind:?}")
}
