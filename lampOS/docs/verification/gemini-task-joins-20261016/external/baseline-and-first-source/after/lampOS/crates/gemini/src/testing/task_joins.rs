//! Fixture ownership of actual Gemini task joins. No task is spawned here.
use crate::{Error, Result, SessionId};
use futures_util::task::AtomicWaker;
use std::{
    future::{Future, poll_fn},
    num::NonZeroUsize,
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU8, Ordering},
    },
    task::{Context, Poll},
};
use tokio::task::{Id, JoinError, JoinHandle};

/// Maximum lifetime task registrations in one fixture, without slot reuse.
pub const MAX_JOINED_TASKS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKind {
    Actor,
    Dialing,
}

/// Classification copied only from an actual JoinHandle's Ready result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoinOutcome {
    Returned,
    Cancelled,
    Panicked,
    OtherError,
}

/// The original consumer keeps its exact value/error. Only an abandoned handle
/// can give the fixture its owning JoinError and panic payload.
#[derive(Debug)]
pub enum TaskCompletion {
    Consumer(JoinOutcome),
    Fixture(std::result::Result<(), JoinError>),
}

#[derive(Debug)]
pub struct JoinedTask {
    pub kind: TaskKind,
    pub session: SessionId,
    pub task: Id,
    pub abort_requested: bool,
    pub completion: TaskCompletion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JoinFault {
    Capacity = 1,
    Closed = 2,
    InvalidState = 3,
    Poisoned = 4,
    AlreadyFinished = 5,
}

#[derive(Debug)]
pub struct JoinReport {
    pub tasks: Vec<JoinedTask>,
    pub fault: Option<JoinFault>,
}
impl JoinReport {
    /// All registered tasks were actually joined and no registration was lost.
    /// This says nothing about protocol success, event consumption or children.
    pub fn complete(&self) -> bool {
        self.fault.is_none()
    }
}

enum SlotState {
    Reserved,
    Owned,
    Released(JoinHandle<()>),
    Joining,
    Consumer(JoinOutcome),
    Fixture,
    Missing,
}
struct Slot {
    kind: TaskKind,
    session: SessionId,
    task: Option<Id>,
    abort_requested: bool,
    state: SlotState,
}
struct State {
    slots: Vec<Slot>,
    owner_created: bool,
    owner_released: bool,
    closed: bool,
}
struct Shared {
    limit: usize,
    state: Mutex<State>,
    fault: AtomicU8,
    wake: AtomicWaker,
}
impl Shared {
    fn fault(&self, fault: JoinFault) {
        let _ = self
            .fault
            .compare_exchange(0, fault as u8, Ordering::AcqRel, Ordering::Acquire);
    }
    fn recorded_fault(&self) -> Option<JoinFault> {
        match self.fault.load(Ordering::Acquire) {
            0 => None,
            1 => Some(JoinFault::Capacity),
            2 => Some(JoinFault::Closed),
            3 => Some(JoinFault::InvalidState),
            4 => Some(JoinFault::Poisoned),
            _ => Some(JoinFault::InvalidState),
        }
    }
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| {
            self.fault(JoinFault::Poisoned);
            error.into_inner()
        })
    }
}

#[derive(Clone)]
pub(crate) struct Registrar(Arc<Shared>);
pub(crate) struct OwnerLease(Arc<Shared>);
impl OwnerLease {
    pub(crate) fn source(&self) -> super::JoinSource {
        super::JoinSource::Retained(Registrar(self.0.clone()))
    }
}
impl Drop for OwnerLease {
    fn drop(&mut self) {
        self.0.state().owner_released = true;
        self.0.wake.wake();
    }
}
impl Registrar {
    pub(crate) fn owner(&self) -> OwnerLease {
        let mut state = self.0.state();
        if state.owner_created {
            self.0.fault(JoinFault::InvalidState);
        }
        state.owner_created = true;
        OwnerLease(self.0.clone())
    }
    pub(crate) fn reserve(&self, kind: TaskKind, session: SessionId) -> Option<Registration> {
        let mut state = self.0.state();
        if state.closed || (state.owner_released && kind == TaskKind::Dialing) {
            self.0.fault(JoinFault::Closed);
            return None;
        }
        if state.slots.len() == self.0.limit {
            self.0.fault(JoinFault::Capacity);
            return None;
        }
        let index = state.slots.len();
        state.slots.push(Slot {
            kind,
            session,
            task: None,
            abort_requested: false,
            state: SlotState::Reserved,
        });
        Some(Registration {
            shared: self.0.clone(),
            index,
        })
    }
}
impl super::JoinSource {
    pub(crate) fn reserve(&self, kind: TaskKind, session: SessionId) -> Option<Registration> {
        match self {
            Self::Plain => None,
            Self::Retained(registrar) => registrar.reserve(kind, session),
        }
    }
    pub(crate) fn owner(&self) -> Option<OwnerLease> {
        match self {
            Self::Plain => None,
            Self::Retained(registrar) => Some(registrar.owner()),
        }
    }
}

pub(crate) struct Registration {
    shared: Arc<Shared>,
    index: usize,
}
impl Registration {
    fn bind(&self, id: Id) {
        let mut state = self.shared.state();
        let slot = &mut state.slots[self.index];
        if !matches!(slot.state, SlotState::Reserved) || slot.task.is_some() {
            self.shared.fault(JoinFault::InvalidState);
            return;
        }
        slot.task = Some(id);
        slot.state = SlotState::Owned;
    }
    fn abort_requested(&self) {
        self.shared.state().slots[self.index].abort_requested = true;
    }
    fn consumed(&self, result: &std::result::Result<(), JoinError>) {
        let outcome = match result {
            Ok(()) => JoinOutcome::Returned,
            Err(error) if error.is_panic() => JoinOutcome::Panicked,
            Err(error) if error.is_cancelled() => JoinOutcome::Cancelled,
            Err(_) => JoinOutcome::OtherError,
        };
        let mut state = self.shared.state();
        let slot = &mut state.slots[self.index];
        if matches!(slot.state, SlotState::Owned) {
            slot.state = SlotState::Consumer(outcome);
        } else {
            self.shared.fault(JoinFault::InvalidState);
        }
        drop(state);
        self.shared.wake.wake();
    }
    fn release(&self, task: JoinHandle<()>) {
        let mut task = Some(task);
        let mut state = self.shared.state();
        if state.closed {
            self.shared.fault(JoinFault::Closed);
        } else {
            let slot = &mut state.slots[self.index];
            if matches!(slot.state, SlotState::Owned) {
                slot.state = SlotState::Released(task.take().expect("one handle"));
            } else {
                self.shared.fault(JoinFault::InvalidState);
            }
        }
        drop(state);
        // A rejected transfer drops only after releasing the registry lock.
        drop(task);
        self.shared.wake.wake();
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        let mut state = self.shared.state();
        let slot = &mut state.slots[self.index];
        if matches!(slot.state, SlotState::Reserved) {
            slot.state = SlotState::Missing;
            self.shared.fault(JoinFault::InvalidState);
        }
        drop(state);
        self.shared.wake.wake();
    }
}

/// Keeps exactly one actual handle with its original owner until that owner
/// either harvests it or releases it. It never adds an abort or another task.
pub(crate) struct UnitTask {
    task: Option<JoinHandle<()>>,
    registration: Option<Registration>,
    joined: bool,
}
impl UnitTask {
    pub(crate) fn new(task: JoinHandle<()>, registration: Option<Registration>) -> Self {
        if let Some(registration) = &registration {
            registration.bind(task.id());
        }
        Self {
            task: Some(task),
            registration,
            joined: false,
        }
    }
    pub(crate) fn abort(&self) {
        self.task.as_ref().expect("owned task").abort();
        if let Some(registration) = &self.registration {
            registration.abort_requested();
        }
    }
    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.task.as_ref().expect("owned task").is_finished()
    }
}
impl Future for UnitTask {
    type Output = std::result::Result<(), JoinError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = Pin::new(self.task.as_mut().expect("owned task")).poll(cx);
        if let Poll::Ready(result) = &result {
            self.joined = true;
            if let Some(registration) = &self.registration {
                registration.consumed(result);
            }
        }
        result
    }
}
impl Drop for UnitTask {
    fn drop(&mut self) {
        if !self.joined
            && let Some(registration) = &self.registration
        {
            registration.release(self.task.take().expect("owned task"));
        }
    }
}

/// Sole fixture collector. Cancelling `finish` leaves every handle owned here.
/// Finish observes original shutdown; it neither requests it nor consumes PCM.
pub struct TaskJoins {
    shared: Arc<Shared>,
    pending: Vec<Option<JoinHandle<()>>>,
    completed: Vec<Option<std::result::Result<(), JoinError>>>,
    finished: bool,
}
impl TaskJoins {
    pub(crate) fn new(limit: NonZeroUsize) -> Result<(Registrar, Self)> {
        let limit = limit.get();
        if limit > MAX_JOINED_TASKS {
            return Err(Error::InvalidConfiguration);
        }
        let shared = Arc::new(Shared {
            limit,
            state: Mutex::new(State {
                slots: Vec::with_capacity(limit),
                owner_created: false,
                owner_released: false,
                closed: false,
            }),
            fault: AtomicU8::new(0),
            wake: AtomicWaker::new(),
        });
        Ok((
            Registrar(shared.clone()),
            Self {
                shared,
                pending: (0..limit).map(|_| None).collect(),
                completed: (0..limit).map(|_| None).collect(),
                finished: false,
            },
        ))
    }
    /// Wait for owner release and real joins. The caller supplies one immutable
    /// host deadline outside this future; no clock is read or advanced here.
    pub async fn finish(&mut self) -> std::result::Result<JoinReport, JoinFault> {
        if self.finished {
            return Err(JoinFault::AlreadyFinished);
        }
        Ok(poll_fn(|cx| self.poll_finish(cx)).await)
    }
    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<JoinReport> {
        self.shared.wake.register(cx.waker());
        {
            let mut state = self.shared.state();
            for (index, slot) in state.slots.iter_mut().enumerate() {
                if matches!(slot.state, SlotState::Released(_)) {
                    let SlotState::Released(task) =
                        std::mem::replace(&mut slot.state, SlotState::Joining)
                    else {
                        unreachable!()
                    };
                    self.pending[index] = Some(task);
                }
            }
        }
        for index in 0..self.pending.len() {
            let Some(task) = self.pending[index].as_mut() else {
                continue;
            };
            if let Poll::Ready(result) = Pin::new(task).poll(cx) {
                self.pending[index] = None;
                self.completed[index] = Some(result);
                let mut state = self.shared.state();
                if matches!(state.slots[index].state, SlotState::Joining) {
                    state.slots[index].state = SlotState::Fixture;
                } else {
                    self.shared.fault(JoinFault::InvalidState);
                }
            }
        }
        let mut state = self.shared.state();
        // Actor admission happens inside a Dialing task before its real join.
        // All slots must be joined, not merely the currently released handles.
        if !state.owner_created
            || !state.owner_released
            || !state.slots.iter().all(|slot| {
                matches!(
                    slot.state,
                    SlotState::Consumer(_) | SlotState::Fixture | SlotState::Missing
                )
            })
        {
            return Poll::Pending;
        }
        state.closed = true;
        // Keep immutable slot identities until the registry itself is dropped:
        // an original owner may still drop its already-consumed registration.
        let terminal: Vec<_> = state
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let outcome = match slot.state {
                    SlotState::Consumer(outcome) => Some(outcome),
                    SlotState::Fixture => None,
                    SlotState::Missing => return None,
                    _ => unreachable!("only completed slots leave the collector"),
                };
                Some((
                    index,
                    slot.kind,
                    slot.session,
                    slot.task.expect("bound real task"),
                    slot.abort_requested,
                    outcome,
                ))
            })
            .collect();
        drop(state);
        let tasks = terminal
            .into_iter()
            .map(|(index, kind, session, task, abort_requested, outcome)| {
                let completion = match outcome {
                    Some(outcome) => TaskCompletion::Consumer(outcome),
                    None => TaskCompletion::Fixture(
                        self.completed[index].take().expect("actual joined result"),
                    ),
                };
                JoinedTask {
                    kind,
                    session,
                    task,
                    abort_requested,
                    completion,
                }
            })
            .collect();
        self.finished = true;
        Poll::Ready(JoinReport {
            tasks,
            fault: self.shared.recorded_fault(),
        })
    }
}
impl Drop for TaskJoins {
    fn drop(&mut self) {
        let mut abandoned = Vec::new();
        {
            let mut state = self.shared.state();
            state.closed = true;
            for slot in &mut state.slots {
                if matches!(slot.state, SlotState::Released(_)) {
                    let SlotState::Released(task) =
                        std::mem::replace(&mut slot.state, SlotState::Missing)
                    else {
                        unreachable!()
                    };
                    abandoned.push(task);
                }
            }
        }
        // Best effort only: dropping this controller never certifies a join.
        for task in self
            .pending
            .iter_mut()
            .filter_map(Option::take)
            .chain(abandoned)
        {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests;
