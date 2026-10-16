//! Observations of actual command handling and completed maintenance, not idleness.
use crate::{RequestId, Result, SessionId};
use futures_util::task::AtomicWaker;
use std::{
    future::poll_fn,
    sync::{Arc, Mutex, MutexGuard},
    task::Poll,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkFault {
    Overflow,
    Poisoned,
    WrongIdentity,
    AlreadyInstalled,
    InvalidState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkCommandKind {
    Start,
    Audio,
    Note,
    Written,
    Context,
    End,
    ToolResponse,
}

/// Fixed-size metadata only: no PCM, text, image or tool payload is retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkCommand {
    pub kind: WorkCommandKind,
    pub request: Option<RequestId>,
    pub sequence: Option<u64>,
}

/// Facts copied at an actual handler return or completed maintenance boundary.
/// Held input is intentionally unsent; these values are not transport receipts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkBuffers {
    pub kept_start: bool,
    pub held_blocks: usize,
    pub held_samples: usize,
    pub outbox_events: usize,
    pub output_gated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkReceipt {
    pub ordinal: u64,
    pub command: WorkCommand,
    pub returned: Result<()>,
    pub buffers: WorkBuffers,
}

/// All original maintenance operations returned before the next select begins.
/// Coverage is the returned-command/discard frontier captured at pass entry;
/// commands returned during the pass need a later completed pass for coverage.
/// Buffer facts are sampled at completion, not at that entry frontier.
/// This does not mean any select branch was polled or any due timer was handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkMaintenance {
    pub ordinal: u64,
    pub covers_returned_commands: u64,
    pub covers_post_stop_discards: u64,
    pub buffers: WorkBuffers,
}

#[derive(Clone, Debug)]
pub struct WorkSnapshot {
    identity: Arc<()>,
    revision: u64,
    pub session: SessionId,
    pub installed: bool,
    pub publications_in_flight: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub abandoned_publications: u64,
    pub commands_started: u64,
    pub commands_returned: u64,
    pub commands_abandoned: u64,
    pub active_command: Option<WorkCommand>,
    pub last_returned: Option<WorkReceipt>,
    pub post_stop_discards: u64,
    pub maintenance_started: u64,
    pub maintenance_completed: u64,
    pub maintenance_incomplete: u64,
    pub maintenance_active: bool,
    pub last_maintenance: Option<WorkMaintenance>,
    pub fault: Option<WorkFault>,
}
impl WorkSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

struct Shared {
    state: Mutex<WorkSnapshot>,
    wake: AtomicWaker,
}
impl Shared {
    fn state(&self) -> MutexGuard<'_, WorkSnapshot> {
        self.state.lock().unwrap_or_else(|error| {
            let mut state = error.into_inner();
            state.fault.get_or_insert(WorkFault::Poisoned);
            state
        })
    }
    fn update(&self, change: impl FnOnce(&mut WorkSnapshot)) {
        {
            let mut state = self.state();
            change(&mut state);
            let state = &mut *state;
            increment(&mut state.revision, &mut state.fault);
        }
        // No observer lock is held across a wake or any original operation.
        self.wake.wake();
    }
}
fn increment(value: &mut u64, fault: &mut Option<WorkFault>) {
    if let Some(next) = value.checked_add(1) {
        *value = next;
    } else {
        fault.get_or_insert(WorkFault::Overflow);
    }
}
fn decrement(value: &mut u64, fault: &mut Option<WorkFault>) {
    if let Some(next) = value.checked_sub(1) {
        *value = next;
    } else {
        fault.get_or_insert(WorkFault::InvalidState);
    }
}

/// One fixture reader, installed before the actual Actor and InputSender escape.
/// It owns no task, no clock and no unbounded event history. Dropping it does not
/// stop or alter the session. The one waiter needs mutable access to this handle.
pub struct ActorWork(Arc<Shared>);
impl ActorWork {
    pub(crate) fn new(session: SessionId) -> (Self, Recorder) {
        let shared = Arc::new(Shared {
            state: Mutex::new(WorkSnapshot {
                identity: Arc::new(()),
                revision: 0,
                session,
                installed: false,
                publications_in_flight: 0,
                accepted: 0,
                rejected: 0,
                abandoned_publications: 0,
                commands_started: 0,
                commands_returned: 0,
                commands_abandoned: 0,
                active_command: None,
                last_returned: None,
                post_stop_discards: 0,
                maintenance_started: 0,
                maintenance_completed: 0,
                maintenance_incomplete: 0,
                maintenance_active: false,
                last_maintenance: None,
                fault: None,
            }),
            wake: AtomicWaker::new(),
        });
        (Self(shared.clone()), Recorder(shared))
    }
    /// Historical facts only. In particular, an old completed maintenance pass
    /// says nothing about current remote frames, queued events or timer work.
    pub fn snapshot(&self) -> WorkSnapshot {
        self.0.state().clone()
    }
    /// Wait for a real observation after this allocation's earlier snapshot.
    /// The fixture must apply its own single immutable host failure deadline.
    /// Cancellation leaves the recorder and all original work untouched.
    pub async fn changed(
        &mut self,
        previous: &WorkSnapshot,
    ) -> std::result::Result<WorkSnapshot, WorkFault> {
        poll_fn(|cx| {
            self.0.wake.register(cx.waker());
            let current = self.snapshot();
            if !Arc::ptr_eq(&previous.identity, &current.identity) {
                return Poll::Ready(Err(WorkFault::WrongIdentity));
            }
            if let Some(fault) = current.fault {
                return Poll::Ready(Err(fault));
            }
            if previous.revision > current.revision {
                return Poll::Ready(Err(WorkFault::InvalidState));
            }
            if current.revision != previous.revision {
                Poll::Ready(Ok(current))
            } else {
                Poll::Pending
            }
        })
        .await
    }
    #[cfg(test)]
    pub(crate) fn overflow_revision_for_test(&self) {
        self.0.state().revision = u64::MAX;
    }
    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        // Capture only the Mutex, not AtomicWaker or an assertion-safety wrapper.
        let state = &self.0.state;
        let result = std::panic::catch_unwind(|| {
            let _guard = state.lock().unwrap();
            panic!("intentional Actor work bookkeeping poison");
        });
        assert!(result.is_err());
    }
}

#[derive(Clone)]
pub(crate) struct Recorder(Arc<Shared>);
impl Recorder {
    pub(crate) fn install(&self, session: SessionId) {
        self.0.update(|s| {
            if s.session != session {
                s.fault.get_or_insert(WorkFault::WrongIdentity);
            }
            if s.installed {
                s.fault.get_or_insert(WorkFault::AlreadyInstalled);
            }
            s.installed = true;
        });
    }
    pub(crate) fn submission(&self) -> Submission {
        self.0.update(|s| {
            if !s.installed {
                s.fault.get_or_insert(WorkFault::InvalidState);
            }
            increment(&mut s.publications_in_flight, &mut s.fault);
        });
        Submission {
            recorder: self.clone(),
            returned: false,
        }
    }
    pub(crate) fn command(&self, command: WorkCommand) -> Handling {
        let mut ordinal = 0;
        self.0.update(|s| {
            if !s.installed || s.active_command.is_some() {
                s.fault.get_or_insert(WorkFault::InvalidState);
            }
            increment(&mut s.commands_started, &mut s.fault);
            ordinal = s.commands_started;
            s.active_command = Some(command);
        });
        Handling {
            recorder: self.clone(),
            ordinal,
            command,
            returned: false,
        }
    }
    pub(crate) fn discarded_after_stop(&self) {
        self.0.update(|s| {
            if !s.installed {
                s.fault.get_or_insert(WorkFault::InvalidState);
            }
            increment(&mut s.post_stop_discards, &mut s.fault);
        });
    }
    pub(crate) fn maintenance(&self) -> Maintenance {
        let mut ordinal = 0;
        let mut returned_commands_at_entry = 0;
        let mut post_stop_discards_at_entry = 0;
        self.0.update(|s| {
            if !s.installed || s.maintenance_active {
                s.fault.get_or_insert(WorkFault::InvalidState);
            }
            increment(&mut s.maintenance_started, &mut s.fault);
            ordinal = s.maintenance_started;
            returned_commands_at_entry = s.commands_returned;
            post_stop_discards_at_entry = s.post_stop_discards;
            s.maintenance_active = true;
        });
        Maintenance {
            recorder: self.clone(),
            ordinal,
            returned_commands_at_entry,
            post_stop_discards_at_entry,
            completed: false,
        }
    }
}

pub(crate) struct Submission {
    recorder: Recorder,
    returned: bool,
}
impl Submission {
    pub(crate) fn returned(mut self, result: Result<()>) {
        self.recorder.0.update(|s| {
            decrement(&mut s.publications_in_flight, &mut s.fault);
            if result.is_ok() {
                increment(&mut s.accepted, &mut s.fault);
            } else {
                increment(&mut s.rejected, &mut s.fault);
            }
        });
        self.returned = true;
    }
}
impl Drop for Submission {
    fn drop(&mut self) {
        if !self.returned {
            self.recorder.0.update(|s| {
                decrement(&mut s.publications_in_flight, &mut s.fault);
                increment(&mut s.abandoned_publications, &mut s.fault);
            });
        }
    }
}

pub(crate) struct Handling {
    recorder: Recorder,
    ordinal: u64,
    command: WorkCommand,
    returned: bool,
}
impl Handling {
    pub(crate) fn returned(mut self, result: Result<()>, buffers: WorkBuffers) {
        self.recorder.0.update(|s| {
            if s.active_command != Some(self.command) || s.commands_started != self.ordinal {
                s.fault.get_or_insert(WorkFault::InvalidState);
            }
            s.active_command = None;
            increment(&mut s.commands_returned, &mut s.fault);
            s.last_returned = Some(WorkReceipt {
                ordinal: self.ordinal,
                command: self.command,
                returned: result,
                buffers,
            });
        });
        self.returned = true;
    }
}
impl Drop for Handling {
    fn drop(&mut self) {
        if !self.returned {
            self.recorder.0.update(|s| {
                s.active_command = None;
                increment(&mut s.commands_abandoned, &mut s.fault);
            });
        }
    }
}

pub(crate) struct Maintenance {
    recorder: Recorder,
    ordinal: u64,
    returned_commands_at_entry: u64,
    post_stop_discards_at_entry: u64,
    completed: bool,
}
impl Maintenance {
    pub(crate) fn completed(mut self, buffers: WorkBuffers) {
        self.recorder.0.update(|s| {
            if !s.maintenance_active || s.maintenance_started != self.ordinal {
                s.fault.get_or_insert(WorkFault::InvalidState);
            }
            s.maintenance_active = false;
            increment(&mut s.maintenance_completed, &mut s.fault);
            s.last_maintenance = Some(WorkMaintenance {
                ordinal: self.ordinal,
                covers_returned_commands: self.returned_commands_at_entry,
                covers_post_stop_discards: self.post_stop_discards_at_entry,
                buffers,
            });
        });
        self.completed = true;
    }
}
impl Drop for Maintenance {
    fn drop(&mut self) {
        if !self.completed {
            self.recorder.0.update(|s| {
                s.maintenance_active = false;
                increment(&mut s.maintenance_incomplete, &mut s.fault);
            });
        }
    }
}
