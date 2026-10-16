//! Actual task/channel regressions. Each test has one fixed, unpaused host
//! deadline; no sleeps, retries, clock advances or scheduling-count assertions.
use super::*;
use crate::{
    Attempt, Connect, Credential, Error, Event, FailureReason, GOOGLE_ENDPOINT, InputSender,
    Notice, RecoveryPolicy, RequestId, ResumptionHandle, SessionConfig, State, Supervisor,
    testing::{self, CaptureClock, Dial, FakeConnector, FakeService, JoinSource},
};
use futures_util::FutureExt;
use std::{sync::atomic::AtomicBool, time::Duration};
use tokio::sync::{mpsc, oneshot};

const HOST_LIMIT: Duration = Duration::from_secs(5);

async fn bounded<F: Future>(future: F) -> F::Output {
    // These tests do not pause Tokio: its fixed deadline is host monotonic time.
    let deadline = tokio::time::Instant::now() + HOST_LIMIT;
    tokio::time::timeout_at(deadline, future)
        .await
        .expect("one immutable host deadline for the entire test")
}
fn session(id: u64) -> SessionId {
    SessionId::new(id).unwrap()
}
fn request(id: u64) -> RequestId {
    RequestId::new(id).unwrap()
}
fn limit(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).unwrap()
}
fn config() -> SessionConfig {
    SessionConfig::new(
        GOOGLE_ENDPOINT,
        Credential::api_key("task-joins-fixture").unwrap(),
    )
    .unwrap()
}
struct FrozenClock(std::time::Instant);
impl FrozenClock {
    fn new() -> Arc<Self> {
        Arc::new(Self(std::time::Instant::now() + Duration::from_secs(3600)))
    }
}
impl CaptureClock for FrozenClock {
    fn now(&self) -> std::time::Instant {
        self.0
    }
}
type Rig = (
    Supervisor<FakeConnector>,
    mpsc::UnboundedReceiver<Dial>,
    TaskJoins,
);
fn fixture(max_tasks: usize) -> Rig {
    testing::supervisor_with_capture_clock_and_joins(
        config(),
        RecoveryPolicy::DISABLED,
        crate::MAX_WIRE_BYTES * 2,
        FrozenClock::new(),
        limit(max_tasks),
    )
    .unwrap()
}
async fn ready(
    supervisor: &mut Supervisor<FakeConnector>,
    dials: &mut mpsc::UnboundedReceiver<Dial>,
) -> Dial {
    let (notice, dial) = tokio::join!(supervisor.next(), async {
        let mut dial = dials.recv().await.unwrap();
        dial.service.accept().await;
        dial
    });
    assert!(
        matches!(notice.unwrap(), Notice::Ready { session: actual, attempts: 1, after: None, .. } if actual == dial.session)
    );
    dial
}
async fn joined(tasks: &mut TaskJoins) -> JoinReport {
    let report = tasks.finish().await.unwrap();
    assert!(report.complete(), "{report:?}");
    report
}
fn actor(report: &JoinReport, id: u64) -> &JoinedTask {
    report
        .tasks
        .iter()
        .find(|task| task.kind == TaskKind::Actor && task.session == session(id))
        .unwrap()
}
fn dial(report: &JoinReport, id: u64) -> &JoinedTask {
    report
        .tasks
        .iter()
        .find(|task| task.kind == TaskKind::Dialing && task.session == session(id))
        .unwrap()
}
fn returned_to_consumer(task: &JoinedTask) {
    assert!(matches!(
        task.completion,
        TaskCompletion::Consumer(JoinOutcome::Returned)
    ));
}
fn cancelled_in_fixture(task: &JoinedTask) {
    assert!(
        matches!(&task.completion, TaskCompletion::Fixture(Err(error)) if error.is_cancelled())
    );
    assert!(task.abort_requested);
}
async fn pair(
    registrar: &Registrar,
    id: u64,
    clock: Arc<dyn CaptureClock>,
) -> (crate::Connection, FakeService) {
    let (attempt, mut service) = testing::pair_with_sources(
        config(),
        session(id),
        crate::MAX_WIRE_BYTES * 2,
        crate::session::CaptureClockSource::Fixture(clock),
        JoinSource::Retained(registrar.clone()),
    )
    .await;
    let (connection, _) = tokio::join!(attempt, service.accept());
    (connection.unwrap(), service)
}

#[tokio::test]
async fn pending_setup_drop_joins_the_actual_cancelled_dialing_task() {
    bounded(async {
        let (supervisor, mut dials, mut tasks) = fixture(4);
        let mut service = dials.recv().await.unwrap().service;
        assert!(service.receive().await["setup"].is_object());
        assert!(tasks.finish().now_or_never().is_none());
        drop(supervisor);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 1);
        cancelled_in_fixture(dial(&report, 1));
        assert!(tasks.finish().await.is_err());
    })
    .await;
}

#[tokio::test]
async fn ready_drop_keeps_consumed_dial_result_and_actually_joins_actor_abort() {
    bounded(async {
        let (mut supervisor, mut dials, mut tasks) = fixture(4);
        let service = ready(&mut supervisor, &mut dials).await;
        assert!(supervisor.is_connected());
        assert!(tasks.finish().now_or_never().is_none());
        drop(supervisor);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 2);
        returned_to_consumer(dial(&report, 1));
        cancelled_in_fixture(actor(&report, 1));
        assert_ne!(report.tasks[0].task, report.tasks[1].task);
        drop(service);
    })
    .await;
}

struct ReceiptConnector {
    joins: JoinSource,
    dials: mpsc::UnboundedSender<FakeService>,
    input: Option<oneshot::Sender<InputSender>>,
}
impl Connect for ReceiptConnector {
    fn connect(&mut self, session: SessionId, _: Option<ResumptionHandle>) -> Attempt {
        let joins = self.joins.clone();
        let dials = self.dials.clone();
        let input = self.input.take().unwrap();
        Box::pin(async move {
            let (attempt, service) = testing::pair_with_sources(
                config(),
                session,
                crate::MAX_WIRE_BYTES * 2,
                crate::session::CaptureClockSource::System,
                joins,
            )
            .await;
            assert!(dials.send(service).is_ok());
            let connection = attempt.await?;
            assert!(input.send(connection.input()).is_ok());
            // No await follows this receipt. On this current-thread runtime,
            // the attempt's actual task returns before its receiver can resume.
            Ok(connection)
        })
    }
}

#[tokio::test]
async fn unharvested_success_drops_connection_before_fixture_polls_retained_join() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(4)).unwrap();
        let joins = JoinSource::Retained(registrar);
        let (send, mut services) = mpsc::unbounded_channel();
        let (input, received) = oneshot::channel();
        let connector = ReceiptConnector {
            joins: joins.clone(),
            dials: send,
            input: Some(input),
        };
        let supervisor =
            Supervisor::new_with_join_source(connector, RecoveryPolicy::DISABLED, joins).unwrap();
        let mut service = services.recv().await.unwrap();
        service.accept().await;
        let input = received.await.unwrap();
        assert!(input.has_room());
        // Never poll Supervisor: its successful Connection is still buffered.
        drop(supervisor);
        assert_eq!(input.try_start(request(1)), Err(Error::Closed));
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 2);
        assert!(matches!(
            dial(&report, 1).completion,
            TaskCompletion::Fixture(Ok(()))
        ));
        assert!(dial(&report, 1).abort_requested);
        cancelled_in_fixture(actor(&report, 1));
    })
    .await;
}

#[tokio::test]
async fn protocol_failure_keeps_its_result_despite_a_successful_task_join() {
    bounded(async {
        let (mut supervisor, mut dials, mut tasks) = fixture(4);
        let mut service = dials.recv().await.unwrap().service;
        assert!(service.receive().await["setup"].is_object());
        service.close(1011, "fixture setup failure").await;
        let failure = supervisor.next().await.unwrap_err();
        assert_eq!(failure.error, Error::PeerClosed { code: Some(1011) });
        assert_eq!(failure.reason, FailureReason::InitialConnect);
        drop(supervisor);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 1);
        returned_to_consumer(dial(&report, 1));
    })
    .await;
}

struct PanicConnector;
impl Connect for PanicConnector {
    fn connect(&mut self, _: SessionId, _: Option<ResumptionHandle>) -> Attempt {
        Box::pin(async { panic!("actual dialing panic") })
    }
}

#[tokio::test]
async fn dialing_panic_stays_a_join_error_at_the_original_recovery_consumer() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(2)).unwrap();
        let mut supervisor = Supervisor::new_with_join_source(
            PanicConnector,
            RecoveryPolicy::DISABLED,
            JoinSource::Retained(registrar),
        )
        .unwrap();
        let failure = supervisor.next().await.unwrap_err();
        assert_eq!(failure.error, Error::Transport);
        assert_eq!(failure.reason, FailureReason::InitialConnect);
        drop(supervisor);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 1);
        assert!(matches!(
            dial(&report, 1).completion,
            TaskCompletion::Consumer(JoinOutcome::Panicked)
        ));
    })
    .await;
}

#[tokio::test]
async fn original_handle_consumer_keeps_the_identical_panic_payload() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(1)).unwrap();
        let owner = registrar.owner();
        let payload = Arc::new(41_u64);
        let sent = payload.clone();
        let task = tokio::spawn(async move {
            std::panic::panic_any(sent);
        });
        let id = task.id();
        let mut task = UnitTask::new(task, registrar.reserve(TaskKind::Actor, session(7)));
        let error = (&mut task).await.unwrap_err();
        assert_eq!(error.id(), id);
        assert!(error.is_panic());
        let actual = error.into_panic().downcast::<Arc<u64>>().unwrap();
        assert!(Arc::ptr_eq(&payload, &actual));
        drop(owner);
        let report = joined(&mut tasks).await;
        assert!(matches!(
            report.tasks[0].completion,
            TaskCompletion::Consumer(JoinOutcome::Panicked)
        ));
        // A consumed registration may outlive report creation; dropping it is
        // not another join or an access to a removed registry slot.
        drop(task);
    })
    .await;
}

#[tokio::test]
async fn cancelled_finish_retains_actual_handles_until_the_same_collector_joins() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(1)).unwrap();
        let owner = registrar.owner();
        let (release, held) = oneshot::channel();
        let task = tokio::spawn(async {
            held.await.unwrap();
        });
        let id = task.id();
        let task = UnitTask::new(task, registrar.reserve(TaskKind::Actor, session(8)));
        drop(task);
        drop(owner);
        assert!(tasks.finish().now_or_never().is_none());
        release.send(()).unwrap();
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks[0].task, id);
        assert!(!report.tasks[0].abort_requested);
        assert!(matches!(
            report.tasks[0].completion,
            TaskCompletion::Fixture(Ok(()))
        ));
    })
    .await;
}

struct PanicClock {
    now: std::time::Instant,
    armed: AtomicBool,
    payload: Arc<u64>,
}
impl CaptureClock for PanicClock {
    fn now(&self) -> std::time::Instant {
        if self.armed.load(Ordering::Acquire) {
            std::panic::panic_any(self.payload.clone());
        }
        self.now
    }
}

#[tokio::test]
async fn actual_actor_panic_is_joined_with_its_own_payload_after_connection_drop() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(1)).unwrap();
        let owner = registrar.owner();
        let payload = Arc::new(52_u64);
        let clock = Arc::new(PanicClock {
            now: std::time::Instant::now(),
            armed: AtomicBool::new(false),
            payload: payload.clone(),
        });
        let (mut connection, mut service) = pair(&registrar, 12, clock.clone()).await;
        let input = connection.input();
        input.try_start(request(1)).unwrap();
        assert!(service.receive().await["realtimeInput"]["activityStart"].is_object());
        assert!(matches!(
            connection.next_event().await,
            Some(Event::InputStarted { .. })
        ));
        input
            .try_audio(request(1), 0, clock.now, &[3; 160])
            .unwrap();
        // No await between enqueue and arming: only the Actor's next read fails.
        clock.armed.store(true, Ordering::Release);
        assert!(connection.next_event().await.is_none());
        drop(connection);
        drop(owner);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 1);
        let TaskCompletion::Fixture(Err(error)) =
            report.tasks.into_iter().next().unwrap().completion
        else {
            panic!("actual Actor panic must retain its JoinError");
        };
        assert!(error.is_panic());
        let actual = error.into_panic().downcast::<Arc<u64>>().unwrap();
        assert!(Arc::ptr_eq(&payload, &actual));
    })
    .await;
}

#[tokio::test]
async fn remote_terminal_state_keeps_ordered_buffered_pcm_until_actual_actor_join() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(1)).unwrap();
        let owner = registrar.owner();
        let (mut connection, mut service) = pair(&registrar, 13, FrozenClock::new()).await;
        connection.input().try_start(request(1)).unwrap();
        assert!(service.receive().await["realtimeInput"]["activityStart"].is_object());
        assert!(matches!(connection.next_event().await, Some(Event::InputStarted { .. })));
        connection.input().try_end(request(1)).unwrap();
        assert!(service.receive().await["realtimeInput"]["activityEnd"].is_object());
        let chunks = crate::EVENT_QUEUE_CAPACITY + 3;
        service.burst((0..chunks).map(|index| testing::audio(index as i16, 240))).await;
        service.close(1011, "fixture remote loss").await;
        let mut state = connection.subscribe_state();
        state.wait_for(|state| *state != State::Ready).await.unwrap();
        assert_eq!(connection.state(), State::Disconnected(Error::PeerClosed { code: Some(1011) }));
        assert!(tasks.finish().now_or_never().is_none());
        for index in 0..chunks {
            assert!(matches!(connection.next_event().await, Some(Event::Audio { lineage, pcm, .. })
                if lineage.session == session(13) && lineage.request == request(1) && pcm == vec![index as i16; 240]));
        }
        assert!(connection.next_event().await.is_none());
        drop(connection);
        drop(owner);
        let report = joined(&mut tasks).await;
        assert!(matches!(actor(&report, 13).completion, TaskCompletion::Fixture(Ok(()))));
    }).await;
}

#[tokio::test]
async fn capacity_fault_keeps_real_ready_and_never_claims_complete_coverage() {
    bounded(async {
        let (mut supervisor, mut dials, mut tasks) = fixture(1);
        let service = ready(&mut supervisor, &mut dials).await;
        assert!(supervisor.is_connected());
        drop(supervisor);
        let report = tasks.finish().await.unwrap();
        assert_eq!(report.fault, Some(JoinFault::Capacity));
        assert!(!report.complete());
        assert_eq!(report.tasks.len(), 1);
        returned_to_consumer(dial(&report, 1));
        drop(service);
    })
    .await;
}

#[tokio::test]
async fn independent_collectors_keep_task_identity_and_actual_cancellation_separate() {
    bounded(async {
        let (one, mut dials_one, mut tasks_one) = fixture(2);
        let (two, mut dials_two, mut tasks_two) = fixture(2);
        let mut first = dials_one.recv().await.unwrap();
        let mut second = dials_two.recv().await.unwrap();
        first.service.receive().await;
        second.service.receive().await;
        drop(one);
        let report_one = joined(&mut tasks_one).await;
        cancelled_in_fixture(dial(&report_one, 1));
        assert!(tasks_two.finish().now_or_never().is_none());
        drop(two);
        let report_two = joined(&mut tasks_two).await;
        cancelled_in_fixture(dial(&report_two, 1));
        assert_ne!(report_one.tasks[0].task, report_two.tasks[0].task);
    })
    .await;
}

#[tokio::test]
async fn dropped_collector_does_not_change_the_real_setup_or_owner_shutdown() {
    bounded(async {
        let (mut supervisor, mut dials, tasks) = fixture(4);
        drop(tasks);
        let service = ready(&mut supervisor, &mut dials).await;
        assert!(supervisor.is_connected());
        supervisor.shutdown();
        let failure = supervisor.next().await.unwrap_err();
        assert_eq!(failure.reason, FailureReason::Shutdown);
        assert_eq!(failure.error, Error::Closed);
        drop(supervisor);
        drop(service);
    })
    .await;
}

#[test]
fn lifetime_limit_is_validated_without_spawning_and_slots_are_never_reused() {
    assert!(matches!(
        TaskJoins::new(limit(MAX_JOINED_TASKS + 1)),
        Err(Error::InvalidConfiguration)
    ));
    let (registrar, _tasks) = TaskJoins::new(limit(1)).unwrap();
    let reservation = registrar.reserve(TaskKind::Actor, session(1)).unwrap();
    drop(reservation);
    assert!(registrar.reserve(TaskKind::Actor, session(2)).is_none());
    assert_eq!(registrar.0.state().slots.len(), 1);
    assert_eq!(registrar.0.recorded_fault(), Some(JoinFault::InvalidState));
}

struct ReceiptPanicConnector(Option<oneshot::Sender<()>>);
impl Connect for ReceiptPanicConnector {
    fn connect(&mut self, _: SessionId, _: Option<ResumptionHandle>) -> Attempt {
        let receipt = self.0.take().unwrap();
        Box::pin(async move {
            receipt.send(()).unwrap();
            panic!("unharvested actual dialing panic");
        })
    }
}

#[tokio::test]
async fn abandoned_dialing_panic_retains_the_actual_join_error_for_the_fixture() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(1)).unwrap();
        let (receipt, received) = oneshot::channel();
        let supervisor = Supervisor::new_with_join_source(
            ReceiptPanicConnector(Some(receipt)),
            RecoveryPolicy::DISABLED,
            JoinSource::Retained(registrar),
        )
        .unwrap();
        // There is no await after send: its current-thread task has panicked
        // before this receiver resumes; no Supervisor poll harvests the error.
        received.await.unwrap();
        drop(supervisor);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 1);
        let TaskCompletion::Fixture(Err(error)) =
            report.tasks.into_iter().next().unwrap().completion
        else {
            panic!("the fixture must own this actual Dialing JoinError");
        };
        assert!(error.is_panic());
        assert_eq!(
            *error.into_panic().downcast::<&str>().unwrap(),
            "unharvested actual dialing panic"
        );
    })
    .await;
}

#[tokio::test]
async fn original_consumer_receives_actual_cancellation_with_the_same_task_identity() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(1)).unwrap();
        let owner = registrar.owner();
        let (_release, held) = oneshot::channel::<()>();
        let task = tokio::spawn(async {
            held.await.unwrap();
        });
        let id = task.id();
        let mut task = UnitTask::new(task, registrar.reserve(TaskKind::Actor, session(9)));
        task.abort();
        let error = (&mut task).await.unwrap_err();
        assert_eq!(error.id(), id);
        assert!(error.is_cancelled());
        drop(task);
        drop(owner);
        let report = joined(&mut tasks).await;
        assert!(matches!(
            report.tasks[0].completion,
            TaskCompletion::Consumer(JoinOutcome::Cancelled)
        ));
        assert!(report.tasks[0].abort_requested);
    })
    .await;
}

#[tokio::test]
async fn owner_release_cannot_hide_an_actor_admitted_before_its_dialing_task_joins() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(2)).unwrap();
        let owner = registrar.owner();
        let (start, proceed) = oneshot::channel();
        let (admitted, seen) = oneshot::channel();
        let (finish_actor, actor_gate) = oneshot::channel();
        let actor_registrar = registrar.clone();
        let task = tokio::spawn(async move {
            proceed.await.unwrap();
            let registration = actor_registrar
                .reserve(TaskKind::Actor, session(1))
                .unwrap();
            let actor = tokio::spawn(async {
                actor_gate.await.unwrap();
            });
            let actor_id = actor.id();
            drop(UnitTask::new(actor, Some(registration)));
            admitted.send(actor_id).unwrap();
        });
        drop(UnitTask::new(
            task,
            registrar.reserve(TaskKind::Dialing, session(1)),
        ));
        drop(owner);
        assert!(tasks.finish().now_or_never().is_none());
        start.send(()).unwrap();
        let actor_id = seen.await.unwrap();
        assert!(tasks.finish().now_or_never().is_none());
        finish_actor.send(()).unwrap();
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 2);
        assert_eq!(actor(&report, 1).task, actor_id);
        for task in report.tasks {
            assert!(matches!(task.completion, TaskCompletion::Fixture(Ok(()))));
        }
    })
    .await;
}

#[tokio::test]
async fn reconnect_joins_both_sessions_and_preserves_the_earlier_buffered_answer() {
    bounded(async {
        let policy = RecoveryPolicy {
            max_attempts: 1, first_backoff: Duration::ZERO, max_backoff: Duration::ZERO,
            outage_budget: Duration::from_secs(2), require_context: false,
            ..RecoveryPolicy::default()
        };
        let clock = FrozenClock::new();
        let (mut supervisor, mut dials, mut tasks) = testing::supervisor_with_capture_clock_and_joins(
            config(), policy, crate::MAX_WIRE_BYTES * 2, clock.clone(), limit(4),
        ).unwrap();
        let mut first = ready(&mut supervisor, &mut dials).await;
        supervisor.try_start(request(1)).unwrap();
        assert!(first.service.receive().await["realtimeInput"]["activityStart"].is_object());
        assert!(matches!(supervisor.next().await.unwrap(), Notice::Event(Event::InputStarted { .. })));
        supervisor.try_end(request(1)).unwrap();
        assert!(first.service.receive().await["realtimeInput"]["activityEnd"].is_object());
        let chunks = crate::EVENT_QUEUE_CAPACITY + 3;
        first.service.burst((0..chunks).map(|index| testing::audio(index as i16, 240))).await;
        first.service.close(1011, "first session lost").await;
        let (notices, mut replacement) = tokio::join!(async {
            let mut notices = Vec::new();
            for _ in 0..chunks + 4 {
                let notice = supervisor.next().await.unwrap();
                let ready = matches!(notice, Notice::Ready { session: id, after: Some(Error::PeerClosed { code: Some(1011) }), .. } if id == session(2));
                notices.push(notice);
                if ready { return notices; }
            }
            panic!("replacement Ready must follow the finite prior answer");
        }, async {
            let mut replacement = dials.recv().await.unwrap();
            replacement.service.accept().await;
            replacement
        });
        assert_eq!(replacement.session, session(2));
        assert!(!replacement.resumed);
        let mut audio = Vec::new();
        let mut lost = 0;
        let collect = |notice: Notice, audio: &mut Vec<i16>, lost: &mut usize| match notice {
            Notice::Event(Event::Audio { lineage, pcm, .. }) => {
                assert_eq!(lineage.session, session(1));
                assert_eq!(lineage.request, request(1));
                audio.extend(pcm);
            }
            Notice::TurnLost { lineage, stage: crate::Stage::Responding, .. } => {
                assert_eq!(lineage.session, session(1));
                *lost += 1;
            }
            Notice::Ready { .. } | Notice::Unavailable { .. } => {}
            other => panic!("unexpected notice {other:?}"),
        };
        for notice in notices { collect(notice, &mut audio, &mut lost); }
        for _ in 0..chunks + 1 {
            if audio.len() == chunks * 240 && lost == 1 { break; }
            collect(supervisor.next().await.unwrap(), &mut audio, &mut lost);
        }
        assert_eq!(audio, (0..chunks).flat_map(|index| std::iter::repeat_n(index as i16, 240)).collect::<Vec<_>>());
        assert_eq!(lost, 1);
        // The second real session shares the capture source, not host time.
        supervisor.try_start(request(2)).unwrap();
        assert!(replacement.service.receive().await["realtimeInput"]["activityStart"].is_object());
        supervisor.try_audio(request(2), 0, clock.0, &[8; 160]).unwrap();
        assert_eq!(testing::sent_audio(&replacement.service.receive().await), Some(vec![8; 160]));
        drop(supervisor);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 4);
        returned_to_consumer(dial(&report, 1));
        returned_to_consumer(dial(&report, 2));
        assert!(matches!(actor(&report, 1).completion, TaskCompletion::Fixture(Ok(()))));
        cancelled_in_fixture(actor(&report, 2));
    }).await;
}

#[tokio::test]
async fn poisoned_observation_preserves_the_real_consumer_result_and_reports_incomplete() {
    bounded(async {
        let (registrar, mut tasks) = TaskJoins::new(limit(1)).unwrap();
        let owner = registrar.owner();
        let task = tokio::spawn(async {});
        let mut task = UnitTask::new(task, registrar.reserve(TaskKind::Actor, session(14)));
        let state = &registrar.0.state;
        assert!(
            std::panic::catch_unwind(move || {
                let _guard = state.lock().unwrap();
                panic!("poison only the fixture observation mutex");
            })
            .is_err()
        );
        assert!((&mut task).await.is_ok());
        drop(task);
        drop(owner);
        let report = tasks.finish().await.unwrap();
        assert_eq!(report.fault, Some(JoinFault::Poisoned));
        assert!(!report.complete());
        returned_to_consumer(&report.tasks[0]);
    })
    .await;
}

#[derive(Debug, Eq, PartialEq)]
struct SupervisorVisibleRun {
    setup: serde_json::Value,
    ready: (SessionId, crate::Context, u32, Option<Error>),
    input: [serde_json::Value; 3],
    started: (crate::Lineage, bool),
    audio: Vec<(crate::Lineage, u64, Vec<i16>)>,
    unavailable: Error,
    lost: (crate::Lineage, crate::Stage, Error),
    terminal: crate::Failure,
    refused_after_terminal: crate::Result<()>,
}

async fn ordered_supervisor_terminal_run(
    mut supervisor: Supervisor<FakeConnector>,
    mut dials: mpsc::UnboundedReceiver<Dial>,
    acquired: std::time::Instant,
) -> SupervisorVisibleRun {
    let mut dial = dials.recv().await.unwrap();
    assert_eq!(dial.session, session(1));
    assert!(!dial.resumed);
    let setup = dial.service.accept().await;
    assert!(setup["setup"].is_object());
    let Notice::Ready {
        session,
        context,
        attempts,
        elapsed,
        after,
    } = supervisor.next().await.unwrap()
    else {
        panic!("setup must be the first consumer notice");
    };
    // Each real setup measures its own host duration; compare its semantic
    // fields and enforce the same outer bound rather than equating runtimes.
    assert!(elapsed < HOST_LIMIT);
    let ready = (session, context, attempts, after);
    assert_eq!(ready, (dial.session, crate::Context::Initial, 1, None));
    let lineage = crate::Lineage {
        session: dial.session,
        request: request(1),
    };

    supervisor.try_start(request(1)).unwrap();
    let start = dial.service.receive().await;
    assert!(start["realtimeInput"]["activityStart"].is_object());
    let Notice::Event(Event::InputStarted {
        lineage: started_lineage,
        waiting_for_barrier,
    }) = supervisor.next().await.unwrap()
    else {
        panic!("Start must produce InputStarted before answer data");
    };
    let started = (started_lineage, waiting_for_barrier);
    assert_eq!(started, (lineage, false));
    supervisor
        .try_audio(request(1), 0, acquired, &[7; 160])
        .unwrap();
    let input_audio = dial.service.receive().await;
    assert_eq!(testing::sent_audio(&input_audio), Some(vec![7; 160]));
    supervisor.try_end(request(1)).unwrap();
    let end = dial.service.receive().await;
    assert!(end["realtimeInput"]["activityEnd"].is_object());

    dial.service
        .burst([testing::audio(11, 240), testing::audio(22, 240)])
        .await;
    let mut audio = Vec::new();
    for (expected_sequence, value) in [(0, 11), (1, 22)] {
        let Notice::Event(Event::Audio {
            lineage: actual,
            sequence,
            pcm,
        }) = supervisor.next().await.unwrap()
        else {
            panic!("the two answer frames must remain ordered audio notices");
        };
        assert_eq!(actual, lineage);
        assert_eq!(sequence, expected_sequence);
        assert_eq!(pcm, vec![value; 240]);
        audio.push((actual, sequence, pcm));
    }
    dial.service.close(1008, "same terminal sequence").await;
    let Notice::Unavailable { error: unavailable } = supervisor.next().await.unwrap() else {
        panic!("remote close must first make the connection unavailable");
    };
    assert_eq!(unavailable, Error::PeerClosed { code: Some(1008) });
    let Notice::TurnLost {
        lineage: lost_lineage,
        stage,
        error,
    } = supervisor.next().await.unwrap()
    else {
        panic!("the unfinished answer must be reported after its audio");
    };
    let lost = (lost_lineage, stage, error);
    assert_eq!(lost, (lineage, crate::Stage::Responding, unavailable));
    let terminal = supervisor.next().await.unwrap_err();
    assert_eq!(
        terminal,
        crate::Failure {
            error: unavailable,
            reason: FailureReason::NotRecoverable,
            attempts: 0,
        }
    );
    let refused_after_terminal = supervisor.try_start(request(2));
    assert_eq!(refused_after_terminal, Err(Error::NotConnected));
    drop(supervisor);
    drop(dial);
    SupervisorVisibleRun {
        setup,
        ready,
        input: [start, input_audio, end],
        started,
        audio,
        unavailable,
        lost,
        terminal,
        refused_after_terminal,
    }
}

#[tokio::test]
async fn ordinary_and_retained_supervisors_have_the_same_ordered_protocol_and_terminal_result() {
    bounded(async {
        let clock = FrozenClock::new();
        let (connector, dials) = testing::connector_with_capture_clock(
            config(),
            crate::MAX_WIRE_BYTES * 2,
            clock.clone(),
        );
        assert!(matches!(&connector.joins, JoinSource::Plain));
        let ordinary = Supervisor::new(connector, RecoveryPolicy::DISABLED).unwrap();
        let ordinary = ordered_supervisor_terminal_run(ordinary, dials, clock.0).await;

        let (retained, dials, mut tasks) = testing::supervisor_with_capture_clock_and_joins(
            config(),
            RecoveryPolicy::DISABLED,
            crate::MAX_WIRE_BYTES * 2,
            clock.clone(),
            limit(2),
        )
        .unwrap();
        let retained = ordered_supervisor_terminal_run(retained, dials, clock.0).await;
        assert_eq!(retained, ordinary);
        let report = joined(&mut tasks).await;
        assert_eq!(report.tasks.len(), 2);
        returned_to_consumer(dial(&report, 1));
        assert!(matches!(
            actor(&report, 1).completion,
            TaskCompletion::Fixture(Ok(()))
        ));
        assert_ne!(dial(&report, 1).task, actor(&report, 1).task);
    })
    .await;
}
