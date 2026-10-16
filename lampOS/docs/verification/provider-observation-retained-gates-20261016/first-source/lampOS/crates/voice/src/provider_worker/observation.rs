//! Explicit observations of the original worker body and its intake Stop causes.
//! A body result is not a task join, Actor exit, runtime exit or child reap.
use super::{Connect, Options, Reporting, Result, Supervisor, WorkerChannels};
use lamp_turn_owner::BootId;
use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    task::{Context, Poll},
};

#[derive(Debug)]
struct Identity {
    boot: BootId,
}

/// Owns its identity allocation, so equal boots and reused addresses cannot
/// identify another worker lifetime.
#[derive(Clone, Debug)]
pub(crate) struct WorkerId(Arc<Identity>);

impl PartialEq for WorkerId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for WorkerId {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum StopCause {
    ExplicitStop = 1,
    AuthorityRevoked = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum BodyState {
    NotPolled = 0,
    InFlight = 1,
    ReturnedOk = 2,
    ReturnedErr = 3,
    DroppedBeforePoll = 4,
    DroppedAfterPoll = 5,
}

/// A coherent historical observation, never a lease on future progress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Snapshot {
    pub(crate) worker: WorkerId,
    pub(crate) boot: BootId,
    pub(crate) stop: Option<StopCause>,
    pub(crate) body: BodyState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Fault {
    InvalidState = 1,
    InvalidTransition = 2,
    AlreadyStopped = 3,
    AlreadyAttached = 4,
    WrongBoot = 5,
    ConcurrentPublication = 6,
}

impl Fault {
    fn decode(value: u8) -> Option<Self> {
        match value {
            0 => None,
            2 => Some(Self::InvalidTransition),
            3 => Some(Self::AlreadyStopped),
            4 => Some(Self::AlreadyAttached),
            5 => Some(Self::WrongBoot),
            6 => Some(Self::ConcurrentPublication),
            _ => Some(Self::InvalidState),
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "provider body observation failed: {self:?}")
    }
}
impl std::error::Error for Fault {}

#[derive(Clone, Copy)]
struct State {
    body: BodyState,
    stop: Option<StopCause>,
}

impl State {
    fn decode(value: u8) -> std::result::Result<Self, Fault> {
        let body = match value & 7 {
            0 => BodyState::NotPolled,
            1 => BodyState::InFlight,
            2 => BodyState::ReturnedOk,
            3 => BodyState::ReturnedErr,
            4 => BodyState::DroppedBeforePoll,
            5 => BodyState::DroppedAfterPoll,
            _ => return Err(Fault::InvalidState),
        };
        let stop = match value >> 3 {
            0 => None,
            1 => Some(StopCause::ExplicitStop),
            2 => Some(StopCause::AuthorityRevoked),
            _ => return Err(Fault::InvalidState),
        };
        if stop.is_some() && matches!(body, BodyState::NotPolled | BodyState::DroppedBeforePoll) {
            return Err(Fault::InvalidState);
        }
        Ok(Self { body, stop })
    }

    fn encode(self) -> u8 {
        self.body as u8 | self.stop.map_or(0, |cause| (cause as u8) << 3)
    }

    fn transition(mut self, event: Event) -> std::result::Result<Self, Fault> {
        match event {
            Event::Enter if self.body == BodyState::NotPolled => {
                self.body = BodyState::InFlight;
            }
            Event::Stop(cause) if self.body == BodyState::InFlight => {
                if self.stop.is_some() {
                    return Err(Fault::AlreadyStopped);
                }
                self.stop = Some(cause);
            }
            Event::Returned(ok) if self.body == BodyState::InFlight => {
                self.body = if ok {
                    BodyState::ReturnedOk
                } else {
                    BodyState::ReturnedErr
                };
            }
            Event::Dropped(false) if self.body == BodyState::NotPolled => {
                self.body = BodyState::DroppedBeforePoll;
            }
            Event::Dropped(true) if self.body == BodyState::InFlight => {
                self.body = BodyState::DroppedAfterPoll;
            }
            _ => return Err(Fault::InvalidTransition),
        }
        Ok(self)
    }
}

enum Event {
    Enter,
    Stop(StopCause),
    Returned(bool),
    Dropped(bool),
}

struct Shared {
    identity: WorkerId,
    state: AtomicU8,
    failure: AtomicU8,
}

impl Shared {
    fn failure(&self) -> Option<Fault> {
        Fault::decode(self.failure.load(Ordering::SeqCst))
    }

    fn fail(&self, fault: Fault) {
        let _ = self
            .failure
            .compare_exchange(0, fault as u8, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// The two writers belong to one future and execute sequentially. A single
    /// compare/exchange detects misuse without spinning or changing its Result.
    fn record(&self, event: Event) {
        if self.failure().is_some() {
            return;
        }
        let before = self.state.load(Ordering::SeqCst);
        let next = State::decode(before).and_then(|state| state.transition(event));
        match next {
            Ok(next) => {
                if self
                    .state
                    .compare_exchange(before, next.encode(), Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    self.fail(Fault::ConcurrentPublication);
                }
            }
            Err(fault) => self.fail(fault),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Probe {
    shared: Arc<Shared>,
}

impl Probe {
    /// No waiting, retries, clocks or work pumping. A later fault does not
    /// retroactively turn an earlier historical snapshot into a health lease.
    pub(crate) fn snapshot(&self) -> std::result::Result<Snapshot, Fault> {
        if let Some(fault) = self.shared.failure() {
            return Err(fault);
        }
        let state = State::decode(self.shared.state.load(Ordering::SeqCst));
        if let Some(fault) = self.shared.failure() {
            return Err(fault);
        }
        let state = state.inspect_err(|fault| self.shared.fail(*fault))?;
        Ok(Snapshot {
            worker: self.shared.identity.clone(),
            boot: self.shared.identity.0.boot,
            stop: state.stop,
            body: state.body,
        })
    }
}

/// A capability bound before the worker is polled; never exposed by its factory.
pub(super) struct IntakeWriter {
    shared: Arc<Shared>,
}

/// Ordinary intakes allocate and publish nothing. The original body's sole
/// internal attachment happens immediately after constructing its intake.
#[derive(Default)]
pub(super) struct IntakeSlot {
    writer: Option<IntakeWriter>,
}

impl IntakeSlot {
    pub(super) fn attach(&mut self, writer: IntakeWriter, boot: BootId) {
        if let Some(existing) = &self.writer {
            existing.shared.fail(Fault::AlreadyAttached);
            writer.shared.fail(Fault::AlreadyAttached);
            return;
        }
        if writer.shared.identity.0.boot != boot {
            writer.shared.fail(Fault::WrongBoot);
            return;
        }
        if !matches!(
            State::decode(writer.shared.state.load(Ordering::SeqCst)),
            Ok(State {
                body: BodyState::InFlight,
                stop: None
            })
        ) {
            writer.shared.fail(Fault::InvalidTransition);
            return;
        }
        self.writer = Some(writer);
    }

    pub(super) fn stop(&self, cause: StopCause) {
        if let Some(writer) = &self.writer {
            writer.shared.record(Event::Stop(cause));
        }
    }
}

struct BodyWriter {
    shared: Arc<Shared>,
    polled: bool,
    returned: bool,
}

impl BodyWriter {
    fn enter(&mut self) {
        if self.returned {
            self.shared.fail(Fault::InvalidTransition);
            return;
        }
        if !self.polled {
            self.polled = true;
            self.shared.record(Event::Enter);
        }
    }

    fn returned(&mut self, ok: bool) {
        self.returned = true;
        self.shared.record(Event::Returned(ok));
    }
}

impl Drop for BodyWriter {
    fn drop(&mut self) {
        if !self.returned {
            // Drop does not establish why polling ended. Only the actual task
            // owner can classify its awaited cancellation or panic result.
            self.shared.record(Event::Dropped(self.polled));
        }
    }
}

pub(super) struct Observed<F> {
    // Drop the actual body before its guard publishes DroppedBefore/AfterPoll.
    future: Pin<Box<F>>,
    body: BodyWriter,
}

impl<F: Future<Output = Result<()>>> Future for Observed<F> {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        this.body.enter();
        match this.future.as_mut().poll(cx) {
            Poll::Ready(result) => {
                this.body.returned(result.is_ok());
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

// Private construction also supports focused component tests of the wrapper.
// The crate-facing entry below is the only factory for an observed real worker.
pub(super) fn observe<F: Future<Output = Result<()>>>(
    boot: BootId,
    build: impl FnOnce(IntakeWriter) -> F,
) -> (Observed<F>, Probe) {
    let shared = Arc::new(Shared {
        identity: WorkerId(Arc::new(Identity { boot })),
        state: AtomicU8::new(BodyState::NotPolled as u8),
        failure: AtomicU8::new(0),
    });
    let future = build(IntakeWriter {
        shared: shared.clone(),
    });
    (
        Observed {
            future: Box::pin(future),
            body: BodyWriter {
                shared: shared.clone(),
                polled: false,
                returned: false,
            },
        },
        Probe { shared },
    )
}

/// Install observation before any worker poll without changing its real clocks,
/// channels, coordinator options or ticker. Supervisor construction is earlier
/// caller work and is not observed; this does not claim to own its nested tasks.
pub(crate) fn coordinated<C: Connect>(
    channels: WorkerChannels,
    boot: BootId,
    provider: Supervisor<C>,
) -> (impl Future<Output = Result<()>>, Probe) {
    observe(boot, |writer| {
        super::serve_with_visual_inner(
            channels,
            boot,
            provider,
            Options::reporting(Reporting::Coordinated),
            None,
            None,
            Some(writer),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot() -> BootId {
        BootId::new([9; 16]).unwrap()
    }

    #[test]
    fn malformed_state_is_a_latched_fault_and_never_a_receipt() {
        let (future, probe) = observe(boot(), |_| std::future::pending());
        probe.shared.state.store(0xff, Ordering::SeqCst);
        assert_eq!(probe.snapshot(), Err(Fault::InvalidState));
        probe
            .shared
            .state
            .store(BodyState::NotPolled as u8, Ordering::SeqCst);
        assert_eq!(probe.snapshot(), Err(Fault::InvalidState));
        drop(future);
        assert_eq!(probe.snapshot(), Err(Fault::InvalidState));
    }

    #[test]
    fn duplicate_stop_and_terminal_mutations_are_latched_faults() {
        let (future, probe) = observe(boot(), |_| std::future::pending());
        probe.shared.record(Event::Enter);
        probe.shared.record(Event::Stop(StopCause::ExplicitStop));
        probe.shared.record(Event::Stop(StopCause::ExplicitStop));
        assert_eq!(probe.snapshot(), Err(Fault::AlreadyStopped));
        drop(future);
        assert_eq!(probe.snapshot(), Err(Fault::AlreadyStopped));

        let (future, probe) = observe(boot(), |_| std::future::pending());
        probe.shared.record(Event::Enter);
        probe.shared.record(Event::Returned(true));
        probe.shared.record(Event::Stop(StopCause::ExplicitStop));
        assert_eq!(probe.snapshot(), Err(Fault::InvalidTransition));
        drop(future);
    }

    #[test]
    fn attachment_refuses_an_unentered_body_or_another_boot() {
        let mut slot = IntakeSlot::default();
        let (future, probe) = observe(boot(), |writer| {
            slot.attach(writer, boot());
            std::future::pending()
        });
        assert_eq!(probe.snapshot(), Err(Fault::InvalidTransition));
        drop(future);

        let (future, probe) = observe(boot(), |writer| {
            slot.attach(writer, BootId::new([7; 16]).unwrap());
            std::future::pending()
        });
        assert_eq!(probe.snapshot(), Err(Fault::WrongBoot));
        drop(future);
    }

    #[tokio::test]
    async fn polling_after_a_return_faults_observation_without_replacing_results() {
        let (mut future, probe) =
            observe(boot(), |_| std::future::poll_fn(|_| Poll::Ready(Ok(()))));
        std::future::poll_fn(|cx| Pin::new(&mut future).poll(cx))
            .await
            .unwrap();
        assert_eq!(probe.snapshot().unwrap().body, BodyState::ReturnedOk);
        // Intentional Future-contract misuse in this wrapper unit test only.
        std::future::poll_fn(|cx| Pin::new(&mut future).poll(cx))
            .await
            .unwrap();
        assert_eq!(probe.snapshot(), Err(Fault::InvalidTransition));
        drop(future);
        assert_eq!(probe.snapshot(), Err(Fault::InvalidTransition));
    }

    #[test]
    fn reattachment_faults_both_owners_without_replacing_the_first_writer() {
        let mut slot = IntakeSlot::default();
        let (first, first_probe) = observe(boot(), |writer| {
            // Private publication setup, not evidence of a real body poll.
            writer.shared.record(Event::Enter);
            slot.attach(writer, boot());
            std::future::pending()
        });
        let (second, second_probe) = observe(boot(), |writer| {
            writer.shared.record(Event::Enter);
            slot.attach(writer, boot());
            std::future::pending()
        });
        assert_eq!(first_probe.snapshot(), Err(Fault::AlreadyAttached));
        assert_eq!(second_probe.snapshot(), Err(Fault::AlreadyAttached));
        assert!(Arc::ptr_eq(
            &slot.writer.as_ref().unwrap().shared,
            &first_probe.shared
        ));
        drop((first, second));
    }

    #[tokio::test]
    async fn observation_failure_preserves_a_component_futures_actual_success() {
        let (future, probe) = observe(boot(), |writer| async move {
            let mut slot = IntakeSlot::default();
            slot.attach(writer, boot());
            slot.stop(StopCause::ExplicitStop);
            slot.stop(StopCause::ExplicitStop);
            Ok(())
        });
        future.await.unwrap();
        assert_eq!(probe.snapshot(), Err(Fault::AlreadyStopped));
    }

    #[tokio::test]
    async fn observation_failure_preserves_the_actual_result_and_error_object() {
        let error = Box::new(std::io::Error::other("original observation-unit error"));
        let address = std::ptr::from_ref(error.as_ref());
        let (future, probe) = observe(boot(), |writer| async move {
            let mut slot = IntakeSlot::default();
            slot.attach(writer, boot());
            slot.stop(StopCause::ExplicitStop);
            slot.stop(StopCause::ExplicitStop);
            Err(error as Box<dyn std::error::Error + Send + Sync>)
        });
        let error = future.await.unwrap_err();
        assert_eq!(
            std::ptr::from_ref(error.downcast_ref::<std::io::Error>().unwrap()),
            address
        );
        assert_eq!(probe.snapshot(), Err(Fault::AlreadyStopped));
    }
}
