//! Scheduling boundaries surround real worker work; host time only bounds
//! waiting for its progress and the existing shutdown contract.
use super::*;
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    sync::{
        Mutex,
        atomic::AtomicUsize,
        mpsc::{SyncSender, TryRecvError},
    },
};

const WATCHDOG: Duration = Duration::from_secs(8);

#[derive(Debug)]
struct Observed {
    progress: ReportProgress,
    reads: usize,
}

struct Schedule {
    at: Mutex<Instant>,
    reads: AtomicUsize,
    worker: Mutex<Option<thread::Thread>>,
    progress: SyncSender<Observed>,
}

impl Schedule {
    fn new(at: Instant) -> (Arc<Self>, Trace) {
        let (progress, received) = mpsc::sync_channel(128);
        (
            Arc::new(Self {
                at: Mutex::new(at),
                reads: AtomicUsize::new(0),
                worker: Mutex::new(None),
                progress,
            }),
            Trace(received),
        )
    }

    fn step(&self, at: Instant) {
        {
            let mut current = self.at.lock().unwrap();
            assert!(at >= *current);
            *current = at;
        }
        self.wake();
    }

    fn wake(&self) {
        self.worker.lock().unwrap().as_ref().unwrap().unpark();
    }

    fn instant(&self) -> Instant {
        *self.at.lock().unwrap()
    }
}

impl ReportSchedule for Schedule {
    fn now(&self) -> Instant {
        self.reads.fetch_add(1, SeqCst);
        self.instant()
    }

    fn park(&self, _remaining: Duration) {
        thread::park();
    }

    fn observe(&self, progress: ReportProgress) {
        *self.worker.lock().unwrap() = Some(thread::current());
        self.progress
            .try_send(Observed {
                progress,
                reads: self.reads.load(SeqCst),
            })
            .expect("bounded fixture progress queue must be consumed");
    }
}

struct Trace(Receiver<Observed>);

impl Trace {
    fn next_before(&self, until: Instant) -> Observed {
        self.0
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("reporter did not reach the requested work boundary")
    }

    fn cycle(&self, started: Instant, publication: PublicationOutcome) -> Observed {
        let until = Instant::now() + WATCHDOG;
        loop {
            let seen = self.next_before(until);
            match seen.progress {
                ReportProgress::CycleComplete { .. } => {
                    assert_eq!(
                        seen.progress,
                        ReportProgress::CycleComplete {
                            started,
                            publication,
                        }
                    );
                    return seen;
                }
                // A real park may wake spuriously or a checked wait may have
                // been queued just before the controller advanced the clock.
                ReportProgress::WaitChecked { at, .. } => assert!(at <= started),
            }
        }
    }

    fn wait_at(&self, started: Instant, at: Instant) -> Observed {
        let until = Instant::now() + WATCHDOG;
        loop {
            let seen = self.next_before(until);
            match seen.progress {
                ReportProgress::WaitChecked {
                    started: cycle,
                    at: sampled,
                    remaining,
                } => {
                    assert_eq!(cycle, started);
                    assert!(sampled <= at);
                    assert_eq!(
                        remaining,
                        REPORT_EVERY.saturating_sub(sampled.saturating_duration_since(started))
                    );
                    assert!(!remaining.is_zero());
                    if sampled == at {
                        return seen;
                    }
                }
                ReportProgress::CycleComplete { .. } => {
                    panic!("a cycle ran before the requested wait: {seen:?}");
                }
            }
        }
    }

    fn assert_no_completed_cycle(&self) {
        for seen in self.0.try_iter() {
            assert!(
                matches!(seen.progress, ReportProgress::WaitChecked { .. }),
                "held publication was reported complete: {seen:?}"
            );
        }
    }
}

/// Release a held publication on assertion failure as well as success.
struct Release(Option<SyncSender<()>>);

impl Release {
    fn send(&mut self) {
        if let Some(send) = self.0.take() {
            let _ = send.send(());
        }
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        self.send();
    }
}

#[test]
fn schedule_reads_are_pure_and_the_default_source_uses_host_time() {
    let start = Instant::now();
    let (schedule, _trace) = Schedule::new(start);
    assert_eq!(schedule.now(), start);
    assert_eq!(schedule.now(), start);
    assert_eq!(schedule.instant(), start);
    assert_eq!(schedule.reads.load(SeqCst), 2);

    let before = Instant::now();
    let actual = SystemSchedule.now();
    let after = Instant::now();
    assert!(before <= actual && actual <= after);
}

#[test]
fn the_shared_worker_samples_once_at_cycle_start_and_once_at_the_wait_check() {
    let start = Instant::now();
    let (schedule, trace) = Schedule::new(start);
    let mut reporter =
        Reporter::start_scheduled_with(CountSource::Days, schedule.clone(), |_| Ok(())).unwrap();
    let cycle = trace.cycle(start, PublicationOutcome::NotAttempted);
    assert_eq!(cycle.reads, 1);
    let waiting = trace.wait_at(start, start);
    assert_eq!(waiting.reads, 2);
    assert_eq!(schedule.instant(), start);
    reporter.finish(false);
}

#[test]
fn frozen_spurious_and_predeadline_wakes_do_not_publish_but_the_exact_deadline_does() {
    let start = Instant::now();
    let (schedule, trace) = Schedule::new(start);
    let (published, received) = mpsc::sync_channel(4);
    let mut reporter =
        Reporter::start_scheduled_with(CountSource::Days, schedule.clone(), move |_| {
            published.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    trace.cycle(start, PublicationOutcome::NotAttempted);
    trace.wait_at(start, start);
    reporter.state().serving();

    schedule.wake();
    trace.wait_at(start, start);
    assert_eq!(received.try_recv(), Err(TryRecvError::Empty));
    let before_due = start + REPORT_EVERY - Duration::from_nanos(1);
    schedule.step(before_due);
    let last = trace.wait_at(start, before_due);
    assert_eq!(
        last.progress,
        ReportProgress::WaitChecked {
            started: start,
            at: before_due,
            remaining: Duration::from_nanos(1),
        }
    );
    assert_eq!(received.try_recv(), Err(TryRecvError::Empty));

    let due = start + REPORT_EVERY;
    schedule.step(due);
    trace.cycle(due, PublicationOutcome::ReturnedOk);
    trace.wait_at(due, due);
    assert_eq!(received.try_recv(), Ok(()));
    assert_eq!(received.try_recv(), Err(TryRecvError::Empty));
    reporter.finish(false);
}

#[test]
fn completion_waits_for_actual_publication_and_overrun_has_no_catchup_queue() {
    let start = Instant::now();
    let (schedule, trace) = Schedule::new(start);
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let completed = Arc::clone(&calls);
    let mut reporter =
        Reporter::start_scheduled_with(CountSource::Days, schedule.clone(), move |_| {
            if completed.fetch_add(1, SeqCst) == 0 {
                entered.send(()).unwrap();
                released.recv_timeout(WATCHDOG).unwrap();
            }
            Ok(())
        })
        .unwrap();
    let mut release = Release(Some(release));
    trace.cycle(start, PublicationOutcome::NotAttempted);
    trace.wait_at(start, start);
    reporter.state().serving();
    let due = start + REPORT_EVERY;
    schedule.step(due);
    entry.recv_timeout(WATCHDOG).unwrap();
    trace.assert_no_completed_cycle();

    let overrun = start + Duration::from_secs(10);
    schedule.step(overrun);
    trace.assert_no_completed_cycle();
    release.send();
    trace.cycle(due, PublicationOutcome::ReturnedOk);
    trace.cycle(overrun, PublicationOutcome::ReturnedOk);
    trace.wait_at(overrun, overrun);
    assert_eq!(calls.load(SeqCst), 2);
    schedule.wake();
    trace.wait_at(overrun, overrun);
    assert_eq!(calls.load(SeqCst), 2);
    reporter.finish(false);
}

#[test]
fn stopping_a_frozen_wait_unparks_and_preserves_the_final_best_effort_report() {
    let start = Instant::now();
    let (schedule, trace) = Schedule::new(start);
    let (published, received) = mpsc::sync_channel(1);
    let mut reporter =
        Reporter::start_scheduled_with(CountSource::Days, schedule.clone(), move |report| {
            published.send(report.message.clone()).unwrap();
            Ok(())
        })
        .unwrap();
    trace.cycle(start, PublicationOutcome::NotAttempted);
    trace.wait_at(start, start);

    let stopping = Instant::now();
    reporter.finish(false);
    assert!(stopping.elapsed() < Duration::from_millis(500));
    assert_eq!(received.recv_timeout(WATCHDOG).unwrap(), "memory stopped");
    trace.cycle(start, PublicationOutcome::ReturnedOk);
    assert_eq!(schedule.instant(), start);
    assert!(reporter.thread.is_none());
    reporter.finish(false);
    assert!(received.try_recv().is_err());
}

#[test]
fn stopping_a_held_publication_keeps_the_host_bound_without_false_completion() {
    let start = Instant::now();
    let (schedule, trace) = Schedule::new(start);
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let mut reporter =
        Reporter::start_scheduled_with(CountSource::Days, schedule.clone(), move |_| {
            entered.send(()).unwrap();
            released.recv_timeout(WATCHDOG).unwrap();
            Ok(())
        })
        .unwrap();
    let mut release = Release(Some(release));
    trace.cycle(start, PublicationOutcome::NotAttempted);
    trace.wait_at(start, start);
    reporter.state().serving();
    let due = start + REPORT_EVERY;
    schedule.step(due);
    entry.recv_timeout(WATCHDOG).unwrap();

    let stopping = Instant::now();
    reporter.finish(false);
    assert!(stopping.elapsed() < Duration::from_millis(500));
    trace.assert_no_completed_cycle();
    assert_eq!(reporter.done.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(schedule.instant(), due);
    release.send();
    trace.cycle(due, PublicationOutcome::ReturnedOk);
    reporter.done.recv_timeout(WATCHDOG).unwrap();
    assert!(reporter.thread.is_none());
}

#[test]
fn publication_outcomes_follow_the_actual_error_and_recovery_results() {
    let start = Instant::now();
    let (schedule, trace) = Schedule::new(start);
    let calls = Arc::new(AtomicUsize::new(0));
    let attempted = Arc::clone(&calls);
    let mut reporter =
        Reporter::start_scheduled_with(CountSource::Days, schedule.clone(), move |_| {
            if attempted.fetch_add(1, SeqCst) == 0 {
                Err(io::Error::other("fixture publication failed"))
            } else {
                Ok(())
            }
        })
        .unwrap();
    trace.cycle(start, PublicationOutcome::NotAttempted);
    trace.wait_at(start, start);
    reporter.state().serving();
    let first = start + REPORT_EVERY;
    schedule.step(first);
    trace.cycle(first, PublicationOutcome::ReturnedErr);
    trace.wait_at(first, first);
    assert_eq!(calls.load(SeqCst), 1);
    let second = first + REPORT_EVERY;
    schedule.step(second);
    trace.cycle(second, PublicationOutcome::ReturnedOk);
    trace.wait_at(second, second);
    assert_eq!(calls.load(SeqCst), 2);
    reporter.finish(false);
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = PathBuf::from(format!(
            "/tmp/mrs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, SeqCst)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn successful_publication_is_not_a_receipt_and_the_shared_bus_clock_rescans_real_sockets() {
    let scratch = Scratch::new();
    let root = scratch.0.join("b");
    let start = Instant::now();
    let (schedule, trace) = Schedule::new(start);
    let mut reporter =
        Reporter::start_with_schedule(root.clone(), CountSource::Days, schedule.clone()).unwrap();
    trace.cycle(start, PublicationOutcome::NotAttempted);
    trace.wait_at(start, start);
    assert!(!root.exists());
    reporter.state().serving();
    let first = start + REPORT_EVERY;
    schedule.step(first);
    trace.cycle(first, PublicationOutcome::ReturnedOk);
    trace.wait_at(first, first);
    // The actual Event publication returned successfully with no subscriber.
    let bus = lamp_bus::Bus::new(&root).unwrap();
    let mut subscriber = bus
        .subscribe_typed(Diagnostic::TOPIC, "fixture", Diagnostic::TYPE)
        .unwrap();
    assert!(subscriber.recv(Duration::ZERO).unwrap().is_none());

    let second = first + REPORT_EVERY;
    schedule.step(second);
    trace.cycle(second, PublicationOutcome::ReturnedOk);
    trace.wait_at(second, second);
    let message = subscriber.recv(Duration::ZERO).unwrap().unwrap();
    let report = message.parse::<Diagnostic>().unwrap();
    assert_eq!(report.name, "memory");
    assert_eq!(report.level, Level::Ok);
    assert_eq!(message.header.kind, lamp_bus::Kind::Event);
    assert_eq!(message.header.seq, 2);
    assert!(subscriber.recv(Duration::ZERO).unwrap().is_none());
    reporter.finish(false);
}
