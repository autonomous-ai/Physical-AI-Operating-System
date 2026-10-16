//! Causal observations through the actual Actor, original queues and WebSocket.
use super::*;
use crate::{
    Credential, GOOGLE_ENDPOINT,
    testing::{
        self, ActorWork, ActorWorkSource, CaptureClock, JoinSource, WorkCommandKind, WorkFault,
        WorkSnapshot,
    },
};
use futures_util::{SinkExt, StreamExt, task::AtomicWaker};
use serde_json::{Value, json};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{DuplexStream, ReadBuf};
use tokio_tungstenite::tungstenite::protocol::Role;

type Service = WebSocketStream<DuplexStream>;

struct Capture {
    at: Instant,
    reads: AtomicU64,
    panic_next: AtomicBool,
}
impl Capture {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            at: Instant::now()
                .checked_add(Duration::from_secs(3_600))
                .unwrap(),
            reads: AtomicU64::new(0),
            panic_next: AtomicBool::new(false),
        })
    }
}
impl CaptureClock for Capture {
    fn now(&self) -> Instant {
        self.reads.fetch_add(1, Ordering::SeqCst);
        assert!(
            !self.panic_next.swap(false, Ordering::SeqCst),
            "original Actor capture panic"
        );
        self.at
    }
}

#[derive(Default)]
struct Gate {
    hold: AtomicBool,
    held: Notify,
    wake: AtomicWaker,
}
impl Gate {
    fn block(&self) {
        self.hold.store(true, Ordering::Release);
    }
    fn release(&self) {
        self.hold.store(false, Ordering::Release);
        self.wake.wake();
    }
}
struct Controlled {
    inner: DuplexStream,
    gate: Arc<Gate>,
}
impl AsyncRead for Controlled {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, bytes)
    }
}
impl AsyncWrite for Controlled {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.gate.hold.load(Ordering::Acquire) {
            self.gate.wake.register(cx.waker());
            if self.gate.hold.load(Ordering::Acquire) {
                self.gate.held.notify_one();
                return Poll::Pending;
            }
        }
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
fn config() -> SessionConfig {
    SessionConfig::new(
        GOOGLE_ENDPOINT,
        Credential::api_key("actor-work-test").unwrap(),
    )
    .unwrap()
}
fn request(id: u64) -> RequestId {
    RequestId::new(id).unwrap()
}
async fn bounded<T>(deadline: Clock, future: impl Future<Output = T>) -> T {
    tokio::time::timeout_at(deadline, future)
        .await
        .expect("fixed Actor work test deadline")
}
async fn receive(service: &mut Service, deadline: Clock) -> Value {
    let message = bounded(deadline, service.next()).await.unwrap().unwrap();
    let Message::Text(text) = message else {
        panic!("expected actual JSON frame: {message:?}")
    };
    serde_json::from_str(&text).unwrap()
}
async fn send(service: &mut Service, deadline: Clock, value: Value) {
    bounded(deadline, service.send(Message::text(value.to_string())))
        .await
        .unwrap();
}
async fn observed(
    work: &mut ActorWork,
    deadline: Clock,
    condition: impl Fn(&WorkSnapshot) -> bool,
) -> WorkSnapshot {
    bounded(deadline, async {
        let mut snapshot = work.snapshot();
        loop {
            assert_eq!(snapshot.fault, None, "{snapshot:?}");
            if condition(&snapshot) {
                return snapshot;
            }
            snapshot = work.changed(&snapshot).await.unwrap();
        }
    })
    .await
}
async fn open(
    configuration: SessionConfig,
) -> (
    Connection,
    Service,
    Arc<Gate>,
    Arc<Capture>,
    ActorWork,
    Clock,
) {
    let deadline = Clock::now() + Duration::from_secs(5);
    let clock = Capture::new();
    let gate = Arc::new(Gate::default());
    let session = SessionId::new(17).unwrap();
    let (work, recorder) = ActorWork::new(session);
    let (client, server) = tokio::io::duplex(MAX_WIRE_BYTES * 2);
    let client = WebSocketStream::from_raw_socket(
        Controlled {
            inner: client,
            gate: gate.clone(),
        },
        Role::Client,
        Some(socket_config()),
    )
    .await;
    let mut service =
        WebSocketStream::from_raw_socket(server, Role::Server, Some(socket_config())).await;
    let attempt = tokio::spawn(setup_and_spawn_with_work_source(
        client,
        configuration,
        session,
        CaptureClockSource::Fixture(clock.clone()),
        JoinSource::Plain,
        ActorWorkSource::Recorded(recorder),
    ));
    assert!(receive(&mut service, deadline).await["setup"].is_object());
    assert!(
        !work.snapshot().installed,
        "setup has not been acknowledged"
    );
    send(&mut service, deadline, json!({"setupComplete": {}})).await;
    let connection = bounded(deadline, attempt).await.unwrap().unwrap();
    assert!(work.snapshot().installed);
    (connection, service, gate, clock, work, deadline)
}
async fn start(
    connection: &Connection,
    service: &mut Service,
    work: &mut ActorWork,
    deadline: Clock,
) {
    connection.input().try_start(request(1)).unwrap();
    assert!(receive(service, deadline).await["realtimeInput"]["activityStart"].is_object());
    observed(work, deadline, |s| {
        s.last_maintenance
            .is_some_and(|m| m.covers_returned_commands == 1)
    })
    .await;
}
async fn joined(mut connection: Connection, deadline: Clock) {
    connection.shutdown();
    bounded(deadline, async {
        while connection.next_event().await.is_some() {}
        (&mut connection.task).await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn public_factory_installs_before_admission_and_acceptance_is_not_handling_or_output_consumption()
 {
    let deadline = Clock::now() + Duration::from_secs(5);
    let clock = Capture::new();
    let (attempt, mut service, mut work) = testing::pair_with_capture_clock_and_work(
        config(),
        SessionId::new(17).unwrap(),
        64,
        clock.clone(),
    )
    .await;
    assert!(!work.snapshot().installed);
    let attempt = tokio::spawn(attempt);
    bounded(deadline, service.accept()).await;
    let mut connection = bounded(deadline, attempt).await.unwrap().unwrap();
    connection.input().try_start(request(1)).unwrap();
    // The current-thread Actor cannot run in this synchronous interval.
    let accepted = work.snapshot();
    assert!(accepted.installed);
    assert_eq!(accepted.accepted, 1);
    assert_eq!(accepted.publications_in_flight, 0);
    assert_eq!(accepted.commands_started, 0);
    assert!(accepted.last_returned.is_none());
    assert!(
        bounded(deadline, service.receive()).await["realtimeInput"]["activityStart"].is_object()
    );
    let handled = observed(&mut work, deadline, |s| {
        s.last_maintenance
            .is_some_and(|m| m.covers_returned_commands == 1)
    })
    .await;
    assert_eq!(handled.commands_returned, 1);
    assert_eq!(handled.last_returned.unwrap().returned, Ok(()));
    assert_eq!(
        connection.events.len(),
        1,
        "event is enqueued but not consumed"
    );
    assert!(matches!(
        bounded(deadline, connection.next_event()).await,
        Some(Event::InputStarted { .. })
    ));
    assert_eq!(
        clock.reads.load(Ordering::SeqCst),
        0,
        "observation never samples capture time"
    );
    joined(connection, deadline).await;
}

#[tokio::test]
async fn actual_blocked_audio_write_has_no_return_receipt_until_transport_accepts_it() {
    let (connection, mut service, gate, clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    gate.block();
    connection
        .input()
        .try_audio(request(1), 0, clock.at, &[42; 160])
        .unwrap();
    bounded(deadline, gate.held.notified()).await;
    let blocked = work.snapshot();
    assert_eq!(blocked.accepted, 2);
    assert_eq!(blocked.commands_returned, 1);
    assert_eq!(blocked.active_command.unwrap().kind, WorkCommandKind::Audio);
    assert_eq!(
        blocked.last_maintenance.unwrap().covers_returned_commands,
        1
    );
    gate.release();
    assert_eq!(
        testing::sent_audio(&receive(&mut service, deadline).await),
        Some(vec![42; 160])
    );
    let complete = observed(&mut work, deadline, |s| {
        s.last_maintenance
            .is_some_and(|m| m.covers_returned_commands == 2)
    })
    .await;
    assert_eq!(complete.last_returned.unwrap().returned, Ok(()));
    assert_eq!(complete.commands_abandoned, 0);
    assert_eq!(clock.reads.load(Ordering::SeqCst), 3);
    joined(connection, deadline).await;
}

#[tokio::test]
async fn original_stop_returns_closed_from_the_handler_but_the_actual_actor_task_returns_unit() {
    let (mut connection, mut service, gate, clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    gate.block();
    connection
        .input()
        .try_audio(request(1), 0, clock.at, &[42; 160])
        .unwrap();
    bounded(deadline, gate.held.notified()).await;
    connection.shutdown();
    bounded(deadline, &mut connection.task).await.unwrap();
    let stopped = work.snapshot();
    assert_eq!(connection.state(), State::Closed);
    assert_eq!(stopped.commands_returned, 2);
    assert_eq!(stopped.last_returned.unwrap().returned, Err(Error::Closed));
    assert_eq!(stopped.commands_abandoned, 0);
    assert_eq!(
        stopped.last_maintenance.unwrap().covers_returned_commands,
        1
    );
}

#[tokio::test]
async fn actual_actor_abort_abandons_entered_handling_without_inventing_a_returned_result() {
    let (mut connection, mut service, gate, clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    gate.block();
    connection
        .input()
        .try_audio(request(1), 0, clock.at, &[42; 160])
        .unwrap();
    bounded(deadline, gate.held.notified()).await;
    connection.task.abort();
    let error = bounded(deadline, &mut connection.task).await.unwrap_err();
    assert!(error.is_cancelled());
    let abandoned = work.snapshot();
    assert_eq!(abandoned.commands_started, 2);
    assert_eq!(abandoned.commands_returned, 1);
    assert_eq!(abandoned.commands_abandoned, 1);
    assert!(abandoned.active_command.is_none());
    assert_eq!(
        abandoned.last_returned.unwrap().command.kind,
        WorkCommandKind::Start
    );
}

#[tokio::test]
async fn actual_actor_panic_keeps_its_join_error_and_never_becomes_a_handler_success() {
    let (mut connection, mut service, _gate, clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    connection
        .input()
        .try_audio(request(1), 0, clock.at, &[42; 160])
        .unwrap();
    clock.panic_next.store(true, Ordering::SeqCst);
    let error = bounded(deadline, &mut connection.task).await.unwrap_err();
    assert!(error.is_panic());
    assert_eq!(
        error.into_panic().downcast_ref::<&str>(),
        Some(&"original Actor capture panic")
    );
    let abandoned = work.snapshot();
    assert_eq!(abandoned.commands_returned, 1);
    assert_eq!(abandoned.commands_abandoned, 1);
    assert_eq!(
        abandoned.last_returned.unwrap().command.kind,
        WorkCommandKind::Start
    );
}

#[tokio::test]
async fn actual_input_sequence_error_is_recorded_as_the_original_result_not_a_task_error() {
    let (mut connection, mut service, _gate, clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    let input = connection.input();
    input.try_audio(request(1), 0, clock.at, &[5; 160]).unwrap();
    assert_eq!(
        testing::sent_audio(&receive(&mut service, deadline).await),
        Some(vec![5; 160])
    );
    observed(&mut work, deadline, |s| s.commands_returned == 2).await;
    input.try_audio(request(1), 0, clock.at, &[6; 160]).unwrap();
    bounded(deadline, &mut connection.task).await.unwrap();
    assert_eq!(
        connection.state(),
        State::Disconnected(Error::InputSequence)
    );
    let failed = work.snapshot();
    assert_eq!(failed.accepted, 3);
    assert_eq!(failed.commands_returned, 3);
    assert_eq!(failed.commands_abandoned, 0);
    assert_eq!(
        failed.last_returned.unwrap().returned,
        Err(Error::InputSequence)
    );
}

#[tokio::test]
async fn kept_start_and_audio_are_actually_handled_while_intentionally_unsent() {
    let configuration = config()
        .successor_start(SuccessorStart::AfterFirstOutput {
            patience: Duration::from_secs(2),
        })
        .unwrap();
    let (connection, mut service, _gate, clock, mut work, deadline) = open(configuration).await;
    start(&connection, &mut service, &mut work, deadline).await;
    let input = connection.input();
    input.try_audio(request(1), 0, clock.at, &[5; 160]).unwrap();
    input.try_end(request(1)).unwrap();
    assert_eq!(
        testing::sent_audio(&receive(&mut service, deadline).await),
        Some(vec![5; 160])
    );
    assert!(receive(&mut service, deadline).await["realtimeInput"]["activityEnd"].is_object());
    observed(&mut work, deadline, |s| s.commands_returned == 3).await;
    input.try_start(request(2)).unwrap();
    input.try_audio(request(2), 0, clock.at, &[6; 160]).unwrap();
    let kept = observed(&mut work, deadline, |s| {
        s.last_maintenance
            .is_some_and(|m| m.covers_returned_commands == 5)
    })
    .await;
    let receipt = kept.last_returned.unwrap();
    assert_eq!(receipt.returned, Ok(()));
    assert!(receipt.buffers.kept_start);
    assert_eq!(receipt.buffers.held_blocks, 1);
    assert_eq!(receipt.buffers.held_samples, 160);
    // Actual first output releases the held input through the unchanged policy.
    send(&mut service, deadline, testing::audio(9, 160)).await;
    assert!(receive(&mut service, deadline).await["realtimeInput"]["activityStart"].is_object());
    assert_eq!(
        testing::sent_audio(&receive(&mut service, deadline).await),
        Some(vec![6; 160])
    );
    observed(&mut work, deadline, |s| {
        s.last_maintenance
            .is_some_and(|m| !m.buffers.kept_start && m.buffers.held_blocks == 0)
    })
    .await;
    joined(connection, deadline).await;
}

async fn blocked_image_with_queued_audio(
    connection: &Connection,
    gate: &Gate,
    clock: &Capture,
    work: &mut ActorWork,
    deadline: Clock,
) {
    gate.block();
    let input = connection.input();
    assert_eq!(
        input.try_image(request(1), 41, 1, clock.at, &testing::jpeg(100, 3)),
        Ok(ImageOffer::Queued)
    );
    input.try_note(request(1), "kept note").unwrap();
    bounded(deadline, gate.held.notified()).await;
    let image = work.snapshot();
    assert_eq!(
        image.commands_returned, 2,
        "Note really returned before image maintenance blocked"
    );
    assert!(image.maintenance_active);
    assert_eq!(image.last_maintenance.unwrap().covers_returned_commands, 1);
    for sequence in 0..INPUT_QUEUE_CAPACITY as u64 {
        input
            .try_audio(request(1), sequence, clock.at, &[7; 160])
            .unwrap();
    }
    assert_eq!(input.commands.capacity(), 0);
    // The actual queue's permit proves at least one command was removed while
    // the transport still holds the image; it does not imply handler completion.
    drop(bounded(deadline, input.commands.reserve()).await.unwrap());
    let held = work.snapshot();
    assert_eq!(held.accepted, 2 + INPUT_QUEUE_CAPACITY as u64);
    assert_eq!(held.commands_returned, 2);
    assert!(held.active_command.is_none());
    assert!(held.maintenance_active);
}

#[tokio::test]
async fn image_held_replay_uses_the_same_handler_and_only_then_completes_maintenance() {
    let (connection, mut service, gate, clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    blocked_image_with_queued_audio(&connection, &gate, &clock, &mut work, deadline).await;
    let image_pass = work.snapshot().maintenance_started;
    gate.release();
    assert_eq!(
        receive(&mut service, deadline).await["realtimeInput"]["video"]["mimeType"],
        "image/jpeg"
    );
    for _ in 0..INPUT_QUEUE_CAPACITY {
        assert_eq!(
            testing::sent_audio(&receive(&mut service, deadline).await),
            Some(vec![7; 160])
        );
    }
    let completed = observed(&mut work, deadline, |s| {
        s.last_maintenance
            .is_some_and(|m| m.covers_returned_commands == 2 + INPUT_QUEUE_CAPACITY as u64)
    })
    .await;
    // Image-held commands return late in that pass. Coverage requires a later
    // whole pass, not just completing the pass which replayed those commands.
    assert!(completed.last_maintenance.unwrap().ordinal > image_pass);
    assert_eq!(completed.commands_returned, 2 + INPUT_QUEUE_CAPACITY as u64);
    assert_eq!(completed.commands_abandoned, 0);
    assert_eq!(
        completed.last_returned.unwrap().command.kind,
        WorkCommandKind::Audio
    );
    assert_eq!(connection.input().image_ledger().sent, 1);
    joined(connection, deadline).await;
}

#[tokio::test]
async fn aborting_image_maintenance_drops_its_private_queue_without_claiming_handlers_returned() {
    let (mut connection, mut service, gate, clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    blocked_image_with_queued_audio(&connection, &gate, &clock, &mut work, deadline).await;
    connection.task.abort();
    assert!(
        bounded(deadline, &mut connection.task)
            .await
            .unwrap_err()
            .is_cancelled()
    );
    let abandoned = work.snapshot();
    assert_eq!(abandoned.accepted, 2 + INPUT_QUEUE_CAPACITY as u64);
    assert_eq!(abandoned.commands_started, 2);
    assert_eq!(abandoned.commands_returned, 2);
    assert_eq!(
        abandoned.commands_abandoned, 0,
        "held commands never entered a handler"
    );
    assert_eq!(abandoned.maintenance_incomplete, 1);
    assert!(!abandoned.maintenance_active);
    assert_eq!(
        abandoned.last_maintenance.unwrap().covers_returned_commands,
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_real_input_clones_reconcile_only_after_both_producers_finish() {
    let (connection, mut service, _gate, _clock, mut work, deadline) = open(config()).await;
    start(&connection, &mut service, &mut work, deadline).await;
    let mut producers = tokio::task::JoinSet::new();
    for name in ["first", "second"] {
        let input = connection.input();
        producers.spawn(async move {
            for _ in 0..8 {
                input.try_note(request(1), name).unwrap();
            }
        });
    }
    while let Some(result) = bounded(deadline, producers.join_next()).await {
        result.unwrap();
    }
    let complete = observed(&mut work, deadline, |s| {
        s.last_maintenance
            .is_some_and(|m| m.covers_returned_commands == 17)
    })
    .await;
    assert_eq!(complete.publications_in_flight, 0);
    assert_eq!(complete.accepted, 17);
    assert_eq!(complete.rejected, 0);
    assert_eq!(complete.commands_returned, 17);
    assert_eq!(complete.commands_abandoned, 0);
    joined(connection, deadline).await;
}

#[tokio::test]
async fn observation_overflow_and_poison_preserve_real_start_result_wire_and_join() {
    for fault in [WorkFault::Overflow, WorkFault::Poisoned] {
        let (mut connection, mut service, _gate, _clock, mut work, deadline) = open(config()).await;
        if fault == WorkFault::Overflow {
            work.overflow_revision_for_test();
        } else {
            work.poison_for_test();
        }
        let before = work.snapshot();
        assert_eq!(connection.input().try_start(request(1)), Ok(()));
        assert!(
            receive(&mut service, deadline).await["realtimeInput"]["activityStart"].is_object()
        );
        assert!(matches!(
            bounded(deadline, connection.next_event()).await,
            Some(Event::InputStarted { .. })
        ));
        assert_eq!(work.snapshot().fault, Some(fault));
        assert_eq!(
            bounded(deadline, work.changed(&before)).await.unwrap_err(),
            fault
        );
        assert_eq!(connection.state(), State::Ready);
        joined(connection, deadline).await;
    }
}

#[tokio::test]
async fn foreign_same_session_snapshot_is_refused_and_cancelled_wait_loses_no_real_receipt() {
    let (connection, mut service, _gate, _clock, mut work, deadline) = open(config()).await;
    let (other, _recorder) = ActorWork::new(SessionId::new(17).unwrap());
    assert_eq!(
        bounded(deadline, work.changed(&other.snapshot()))
            .await
            .unwrap_err(),
        WorkFault::WrongIdentity
    );
    let before = work.snapshot();
    {
        let pending = work.changed(&before);
        tokio::pin!(pending);
        assert!(futures_util::poll!(&mut pending).is_pending());
    }
    start(&connection, &mut service, &mut work, deadline).await;
    let after = bounded(deadline, work.changed(&before)).await.unwrap();
    assert_eq!(after.accepted, 1);
    assert_eq!(after.commands_returned, 1);
    joined(connection, deadline).await;
}

#[tokio::test]
async fn unpolled_setup_drop_has_neither_installation_nor_handler_or_maintenance_receipts() {
    let clock = Capture::new();
    let (attempt, _service, work) =
        testing::pair_with_capture_clock_and_work(config(), SessionId::new(17).unwrap(), 64, clock)
            .await;
    drop(attempt);
    let untouched = work.snapshot();
    assert!(!untouched.installed);
    assert_eq!(untouched.revision(), 0);
    assert_eq!(untouched.commands_started, 0);
    assert_eq!(untouched.commands_abandoned, 0);
    assert_eq!(untouched.maintenance_started, 0);
    assert!(untouched.last_maintenance.is_none());
}

#[tokio::test]
async fn existing_factories_remain_unobserved_with_fixture_support_enabled() {
    let deadline = Clock::now() + Duration::from_secs(5);
    let (attempt, mut service) =
        testing::pair_with_capture_clock(config(), SessionId::new(17).unwrap(), 64, Capture::new())
            .await;
    let attempt = tokio::spawn(attempt);
    bounded(deadline, service.accept()).await;
    let connection = bounded(deadline, attempt).await.unwrap().unwrap();
    assert!(connection.input().shared.work.is_none());
    connection.input().try_start(request(1)).unwrap();
    assert!(
        bounded(deadline, service.receive()).await["realtimeInput"]["activityStart"].is_object()
    );
    joined(connection, deadline).await;
}

#[test]
fn publication_can_follow_handler_return_without_faulting_or_claiming_settled_admission() {
    // Explicit interleaving for the race between mpsc success and its publication;
    // the tests above separately exercise the real concurrent producers and Actor.
    let session = SessionId::new(17).unwrap();
    let (work, recorder) = ActorWork::new(session);
    recorder.install(session);
    let publication = recorder.submission();
    let command = testing::WorkCommand {
        kind: WorkCommandKind::Note,
        request: Some(request(1)),
        sequence: None,
    };
    recorder
        .command(command)
        .returned(Ok(()), testing::WorkBuffers::default());
    let overtaken = work.snapshot();
    assert_eq!(overtaken.accepted, 0);
    assert_eq!(overtaken.commands_returned, 1);
    assert_eq!(overtaken.publications_in_flight, 1);
    assert_eq!(overtaken.fault, None);
    publication.returned(Ok(()));
    assert_eq!(work.snapshot().accepted, 1);
    assert_eq!(work.snapshot().publications_in_flight, 0);
    recorder.discarded_after_stop();
    assert_eq!(work.snapshot().post_stop_discards, 1);
    assert_eq!(work.snapshot().commands_returned, 1);
    recorder.install(session);
    assert_eq!(work.snapshot().fault, Some(WorkFault::AlreadyInstalled));
    let (foreign, wrong_recorder) = ActorWork::new(session);
    wrong_recorder.install(SessionId::new(18).unwrap());
    assert_eq!(foreign.snapshot().fault, Some(WorkFault::WrongIdentity));
}

#[test]
fn maintenance_covers_only_its_entry_frontier_until_a_later_complete_pass() {
    // The real image case above exercises late replay. This explicit state
    // interleaving also verifies the discard counter's entry-snapshot contract;
    // it does not claim the Actor's after-stop select runs during maintenance.
    let session = SessionId::new(17).unwrap();
    let (work, recorder) = ActorWork::new(session);
    recorder.install(session);
    let command = testing::WorkCommand {
        kind: WorkCommandKind::Note,
        request: Some(request(1)),
        sequence: None,
    };
    recorder
        .command(command)
        .returned(Ok(()), testing::WorkBuffers::default());
    recorder.discarded_after_stop();
    let first = recorder.maintenance();
    recorder
        .command(command)
        .returned(Ok(()), testing::WorkBuffers::default());
    recorder.discarded_after_stop();
    let during = work.snapshot();
    assert_eq!(during.commands_returned, 2);
    assert_eq!(during.post_stop_discards, 2);
    assert!(during.maintenance_active);
    assert!(during.last_maintenance.is_none());
    let at_completion = testing::WorkBuffers {
        held_blocks: 2,
        ..testing::WorkBuffers::default()
    };
    first.completed(at_completion);
    let completed = work.snapshot();
    let receipt = completed.last_maintenance.unwrap();
    assert_eq!(completed.commands_returned, 2);
    assert_eq!(completed.post_stop_discards, 2);
    assert_eq!(receipt.ordinal, 1);
    assert_eq!(receipt.covers_returned_commands, 1);
    assert_eq!(receipt.covers_post_stop_discards, 1);
    assert_eq!(receipt.buffers, at_completion);
    assert!(!completed.maintenance_active);
    assert_eq!(completed.fault, None);
    recorder.maintenance().completed(at_completion);
    let later = work.snapshot();
    let receipt = later.last_maintenance.unwrap();
    assert_eq!(receipt.ordinal, 2);
    assert_eq!(receipt.covers_returned_commands, 2);
    assert_eq!(receipt.covers_post_stop_discards, 2);
    assert_eq!(receipt.buffers, at_completion);
    assert_eq!(later.maintenance_completed, 2);
    assert_eq!(later.maintenance_incomplete, 0);
    assert_eq!(later.fault, None);
}
