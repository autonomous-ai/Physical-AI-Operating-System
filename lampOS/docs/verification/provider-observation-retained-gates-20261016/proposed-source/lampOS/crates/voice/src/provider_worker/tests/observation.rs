//! Handler, body and task boundaries are exercised separately. None of these
//! observations establishes completion of a Gemini Actor or real child.
use super::*;
use crate::provider_worker::observation::{self as observed, BodyState, StopCause};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::Arc,
    task::Poll,
};
use tokio::{sync::oneshot, task::JoinHandle};

struct Deadline(Instant);

impl Deadline {
    fn new() -> Self {
        Self(Instant::now() + Duration::from_secs(10))
    }

    async fn within<T>(&self, future: impl Future<Output = T>) -> T {
        tokio::time::timeout_at(tokio::time::Instant::from_std(self.0), future)
            .await
            .expect("provider observation fixture host deadline")
    }
}

/// A failed assertion requests cancellation too. Only `join` below establishes
/// task completion; the Drop fallback is never reported as a successful join.
struct OwnedTask<T>(Option<JoinHandle<T>>);

impl<T> OwnedTask<T> {
    fn new(task: JoinHandle<T>) -> Self {
        Self(Some(task))
    }

    fn abort(&self) {
        self.0.as_ref().unwrap().abort();
    }

    async fn join(
        &mut self,
        deadline: &Deadline,
    ) -> std::result::Result<T, tokio::task::JoinError> {
        let result = deadline.within(self.0.as_mut().unwrap()).await;
        self.0 = None;
        result
    }
}

impl<T> Drop for OwnedTask<T> {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

struct AttemptGone(Option<oneshot::Sender<()>>);

impl Drop for AttemptGone {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

struct HeldConnector(Option<oneshot::Sender<()>>);

impl Connect for HeldConnector {
    fn connect(
        &mut self,
        _: SessionId,
        _: Option<lamp_gemini::ResumptionHandle>,
    ) -> lamp_gemini::Attempt {
        // Capture the guard before polling, so cancellation also observes Drop
        // of an attempt that never entered. No setup succeeds and no Actor exists.
        let guard = AttemptGone(Some(self.0.take().expect("one held attempt")));
        Box::pin(async move {
            let _guard = guard;
            std::future::pending().await
        })
    }
}

fn held_provider() -> (Supervisor<HeldConnector>, oneshot::Receiver<()>) {
    let (gone, receiver) = oneshot::channel();
    (
        Supervisor::new(HeldConnector(Some(gone)), RecoveryPolicy::default()).unwrap(),
        receiver,
    )
}

fn boot() -> BootId {
    BootId::new([9; 16]).unwrap()
}

async fn pending_once<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn queued_real_stop_is_not_handled_until_the_original_body_runs() {
    let deadline = Deadline::new();
    let (_directory, mut parent, worker) = intake_channels();
    let (provider, attempt_gone) = held_provider();
    let (future, probe) = observed::coordinated(worker, boot(), provider);
    parent.control.send(Control::Stop).unwrap();
    let before = probe.snapshot().unwrap();
    assert_eq!(before.boot, boot());
    assert_eq!(before.body, BodyState::NotPolled);
    assert_eq!(before.stop, None);
    deadline.within(future).await.unwrap();
    let after = probe.snapshot().unwrap();
    assert_eq!(after.worker, before.worker);
    assert_eq!(after.stop, Some(StopCause::ExplicitStop));
    assert_eq!(after.body, BodyState::ReturnedOk);
    assert!(matches!(
        parent.control.receive::<WorkerEvent>().unwrap(),
        Some(WorkerEvent::Ready)
    ));
    // This is actual attempt-future Drop, not its private task's JoinHandle.
    deadline.within(attempt_gone).await.unwrap();
}

#[tokio::test]
async fn unpolled_real_worker_drop_never_claims_a_queued_stop_or_return() {
    let deadline = Deadline::new();
    let (_directory, mut parent, worker) = intake_channels();
    let (provider, attempt_gone) = held_provider();
    let (future, probe) = observed::coordinated(worker, boot(), provider);
    parent.control.send(Control::Stop).unwrap();
    assert!(parent.control.receive::<WorkerEvent>().unwrap().is_none());
    drop(future);
    let state = probe.snapshot().unwrap();
    assert_eq!(state.body, BodyState::DroppedBeforePoll);
    assert_eq!(state.stop, None);
    match parent.control.receive::<WorkerEvent>() {
        Ok(None) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("unexpected unpolled worker receive result: {result:?}"),
    }
    deadline.within(attempt_gone).await.unwrap();
}

#[tokio::test]
async fn actual_intake_stop_precedes_the_component_future_return() {
    let deadline = Deadline::new();
    let (_directory, mut parent, mut worker) = intake_channels();
    parent.control.send(Control::Stop).unwrap();
    let (handled, mut received) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let (future, probe) = observed::observe(boot(), |writer| async move {
        let mut intake = ProviderIntake::new(boot(), NOW);
        intake.observation.attach(writer, boot());
        let mut budget = CONTROL_SLICE;
        let result =
            intake.drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget)?;
        assert_eq!(result, ControlDrain::Stopped);
        assert!(intake.stopped);
        assert!(intake.retained.is_none());
        handled.send(result).unwrap();
        released.await.unwrap();
        Ok(())
    });
    let mut future = Box::pin(future);
    // The withheld oneshot makes Pending causal; it is not an idle/progress test.
    pending_once(future.as_mut()).await;
    assert_eq!(received.try_recv().unwrap(), ControlDrain::Stopped);
    let handled = probe.snapshot().unwrap();
    assert_eq!(handled.stop, Some(StopCause::ExplicitStop));
    assert_eq!(handled.body, BodyState::InFlight);
    release.send(()).unwrap();
    deadline.within(future).await.unwrap();
    assert_eq!(probe.snapshot().unwrap().body, BodyState::ReturnedOk);
}

#[tokio::test]
async fn full_control_slice_leaves_the_real_stop_unhandled_until_the_next_drain() {
    let deadline = Deadline::new();
    let (_directory, mut parent, mut worker) = intake_channels();
    let mut commands: VecDeque<_> = (0..CONTROL_SLICE)
        .map(|_| Control::ProviderOutputCapacity { through: 2 })
        .chain(std::iter::once(Control::Stop))
        .collect();
    parent.control.send(commands.pop_front().unwrap()).unwrap();
    let (deferred, mut first_result) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let (future, probe) = observed::observe(boot(), |writer| async move {
        let mut intake = ProviderIntake::new(boot(), NOW);
        intake.observation.attach(writer, boot());
        let mut budget = CONTROL_SLICE;
        let result = intake.drain_control(
            &mut || NOW,
            &mut || {
                let result = worker.control.receive()?;
                if result.is_some()
                    && let Some(next) = commands.pop_front()
                {
                    parent.control.send(next)?;
                }
                Ok(result)
            },
            &mut budget,
        )?;
        assert_eq!(result, ControlDrain::Deferred);
        assert_eq!(budget, 0);
        assert!(
            commands.is_empty(),
            "the Stop was sent after actual receive 16"
        );
        assert!(!intake.stopped);
        deferred.send(result).unwrap();
        released.await.unwrap();
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake.drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget)?,
            ControlDrain::Stopped
        );
        assert_eq!(budget, CONTROL_SLICE - 1);
        Ok(())
    });
    let mut future = Box::pin(future);
    pending_once(future.as_mut()).await;
    assert_eq!(first_result.try_recv().unwrap(), ControlDrain::Deferred);
    assert_eq!(probe.snapshot().unwrap().stop, None);
    assert_eq!(probe.snapshot().unwrap().body, BodyState::InFlight);
    release.send(()).unwrap();
    deadline.within(future).await.unwrap();
    assert_eq!(
        probe.snapshot().unwrap().stop,
        Some(StopCause::ExplicitStop)
    );
}

#[tokio::test]
async fn initial_denied_authority_is_not_a_revocation_stop() {
    let (_directory, mut parent, mut worker) = intake_channels();
    let mut owner = Controller::new(boot(), time());
    owner
        .set_microphone_permission(time(), Permission::Denied)
        .unwrap();
    parent
        .control
        .send(Control::Authority {
            snapshot: owner.snapshot(time()).unwrap(),
        })
        .unwrap();
    let (future, probe) = observed::observe(boot(), |writer| async move {
        let mut intake = ProviderIntake::new(boot(), NOW);
        intake.observation.attach(writer, boot());
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake.drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget)?,
            ControlDrain::Empty
        );
        assert!(!intake.stopped);
        assert!(!intake.saw_allowed);
        Ok(())
    });
    future.await.unwrap();
    assert_eq!(probe.snapshot().unwrap().stop, None);
    assert_eq!(probe.snapshot().unwrap().body, BodyState::ReturnedOk);
}

async fn revoked_authority(reopen: bool) {
    let (_directory, mut parent, mut worker) = intake_channels();
    let mut owner = controller();
    parent
        .control
        .send(Control::Authority {
            snapshot: owner.snapshot(time()).unwrap(),
        })
        .unwrap();
    let (future, probe) = observed::observe(boot(), |writer| async move {
        let mut intake = ProviderIntake::new(boot(), NOW);
        intake.observation.attach(writer, boot());
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake.drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget)?,
            ControlDrain::Empty
        );
        assert!(intake.saw_allowed);
        owner
            .set_microphone_permission(time(), Permission::Denied)
            .unwrap();
        if reopen {
            owner
                .set_microphone_permission(time(), Permission::Allowed)
                .unwrap();
        }
        parent
            .control
            .send(Control::Authority {
                snapshot: owner.snapshot(time()).unwrap(),
            })
            .unwrap();
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake.drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget)?,
            ControlDrain::Stopped
        );
        assert!(intake.stopped);
        assert!(intake.retained.is_none());
        Ok(())
    });
    future.await.unwrap();
    assert_eq!(
        probe.snapshot().unwrap().stop,
        Some(StopCause::AuthorityRevoked)
    );
    assert_eq!(probe.snapshot().unwrap().body, BodyState::ReturnedOk);
}

#[tokio::test]
async fn actual_permission_revocation_has_its_own_stop_cause() {
    revoked_authority(false).await;
}

#[tokio::test]
async fn actual_generation_change_is_revocation_even_after_permission_reopens() {
    revoked_authority(true).await;
}

#[tokio::test]
async fn original_worker_returns_after_actual_authority_revocation() {
    let deadline = Deadline::new();
    let (_directory, mut parent, worker) = intake_channels();
    let mut owner = Controller::new(boot(), real_time());
    owner
        .set_microphone_permission(real_time(), Permission::Allowed)
        .unwrap();
    parent
        .control
        .send(Control::Authority {
            snapshot: owner.snapshot(real_time()).unwrap(),
        })
        .unwrap();
    owner
        .set_microphone_permission(real_time(), Permission::Denied)
        .unwrap();
    parent
        .control
        .send(Control::Authority {
            snapshot: owner.snapshot(real_time()).unwrap(),
        })
        .unwrap();
    let (provider, attempt_gone) = held_provider();
    let (future, probe) = observed::coordinated(worker, boot(), provider);
    deadline.within(future).await.unwrap();
    assert_eq!(
        probe.snapshot().unwrap().stop,
        Some(StopCause::AuthorityRevoked)
    );
    assert_eq!(probe.snapshot().unwrap().body, BodyState::ReturnedOk);
    deadline.within(attempt_gone).await.unwrap();
}

#[tokio::test]
async fn original_worker_error_is_returned_without_claiming_a_stop_command() {
    let deadline = Deadline::new();
    let (_directory, mut parent, worker) = intake_channels();
    let (provider, attempt_gone) = held_provider();
    parent.control.send(Control::StartCapture).unwrap();
    let (future, probe) = observed::coordinated(worker, boot(), provider);
    let error = deadline.within(future).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().to_string(),
        "invalid provider command"
    );
    let state = probe.snapshot().unwrap();
    assert_eq!(state.body, BodyState::ReturnedErr);
    assert_eq!(state.stop, None);
    deadline.within(attempt_gone).await.unwrap();
}

#[tokio::test]
async fn error_cleanup_clears_intake_without_recording_a_stop_cause() {
    let (_directory, mut parent, mut worker) = intake_channels();
    parent.control.send(Control::StartCapture).unwrap();
    let (future, probe) = observed::observe(boot(), |writer| async move {
        let mut intake = ProviderIntake::new(boot(), NOW);
        intake.observation.attach(writer, boot());
        let mut budget = CONTROL_SLICE;
        let result =
            intake.drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget);
        assert!(intake.stopped);
        assert!(intake.retained.is_none());
        result.map(|_| ())
    });
    assert_eq!(
        future.await.unwrap_err().to_string(),
        "invalid provider command"
    );
    assert_eq!(probe.snapshot().unwrap().stop, None);
    assert_eq!(probe.snapshot().unwrap().body, BodyState::ReturnedErr);
}

#[tokio::test]
async fn ordinary_intake_and_equal_boot_workers_do_not_share_observations() {
    let (first, first_probe) = observed::observe(boot(), |_| async { Ok(()) });
    let (second, second_probe) = observed::observe(boot(), |_| async { Ok(()) });
    let first_identity = first_probe.snapshot().unwrap().worker;
    let second_before = second_probe.snapshot().unwrap();
    assert_ne!(first_identity, second_before.worker);
    let (_directory, mut parent, mut worker) = intake_channels();
    parent.control.send(Control::Stop).unwrap();
    let mut ordinary = ProviderIntake::new(boot(), NOW);
    let mut budget = CONTROL_SLICE;
    assert_eq!(
        ordinary
            .drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget)
            .unwrap(),
        ControlDrain::Stopped
    );
    assert_eq!(first_probe.snapshot().unwrap().body, BodyState::NotPolled);
    assert_eq!(first_probe.snapshot().unwrap().stop, None);
    drop(first);
    drop(first_probe);
    assert_eq!(second_probe.snapshot().unwrap(), second_before);
    second.await.unwrap();
    assert_eq!(second_probe.snapshot().unwrap().body, BodyState::ReturnedOk);
    let (third, third_probe) = observed::observe(boot(), |_| async { Ok(()) });
    assert_ne!(third_probe.snapshot().unwrap().worker, first_identity);
    drop(third);
}

#[tokio::test]
async fn requested_task_cancellation_is_joined_and_never_a_body_return() {
    let deadline = Deadline::new();
    let (entered, entry) = oneshot::channel();
    let (future, probe) = observed::observe(boot(), |_| async move {
        entered.send(()).unwrap();
        std::future::pending().await
    });
    let mut task = OwnedTask::new(tokio::spawn(future));
    deadline.within(entry).await.unwrap();
    assert_eq!(probe.snapshot().unwrap().body, BodyState::InFlight);
    task.abort();
    assert!(task.join(&deadline).await.unwrap_err().is_cancelled());
    assert_eq!(probe.snapshot().unwrap().body, BodyState::DroppedAfterPoll);
    assert_eq!(probe.snapshot().unwrap().stop, None);
}

#[tokio::test]
async fn actual_task_panic_preserves_its_payload_without_a_successful_receipt() {
    let deadline = Deadline::new();
    let payload = Arc::new(String::from("provider observation component panic"));
    let mut sent = Some(payload.clone());
    let (future, probe) = observed::observe(boot(), |_| {
        poll_fn(move |_| -> Poll<Result<()>> {
            std::panic::panic_any(sent.take().unwrap());
        })
    });
    let mut task = OwnedTask::new(tokio::spawn(future));
    let panic = task.join(&deadline).await.unwrap_err();
    assert!(panic.is_panic());
    let original = panic.into_panic().downcast::<Arc<String>>().unwrap();
    assert!(Arc::ptr_eq(&payload, &original));
    assert_eq!(probe.snapshot().unwrap().body, BodyState::DroppedAfterPoll);
    assert_eq!(probe.snapshot().unwrap().stop, None);
}
