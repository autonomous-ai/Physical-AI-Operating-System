//! A dedicated fixture's pure clock and observed Reporter boundaries.
use crate::fixture_protocol::{
    Bootstrap, Channel, Checked, Connected, HOST_WAIT, Outcome, Step, VERSION, invalid, marker,
    read_frame, remaining,
};
use lamp_memory::health::{PublicationOutcome, REPORT_EVERY, ReportProgress, ReportSchedule};
use std::{
    fs::File,
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::{fs::FileTypeExt, net::UnixStream},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering::SeqCst},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle, Thread},
    time::{Duration, Instant},
};

const PROGRESS_CAPACITY: usize = 128;
const OBSERVER_OVERFLOW: u8 = 1;
const WRONG_WORKER: u8 = 2;
const CONTROL_FAILED: u8 = 3;

enum Observation {
    Progress(ReportProgress),
    Closed,
}

pub struct FixtureSchedule {
    origin: Instant,
    at_us: AtomicU64,
    worker: OnceLock<Thread>,
    observations: SyncSender<Observation>,
    failure: AtomicU8,
    closed: AtomicBool,
}

impl FixtureSchedule {
    pub fn pair(nonce: String) -> io::Result<(Arc<Self>, ProgressDriver)> {
        marker(&nonce, 1)?;
        let (send, receive) = mpsc::sync_channel(PROGRESS_CAPACITY);
        let schedule = Arc::new(Self {
            origin: Instant::now(),
            at_us: AtomicU64::new(0),
            worker: OnceLock::new(),
            observations: send,
            failure: AtomicU8::new(0),
            closed: AtomicBool::new(false),
        });
        let driver = ProgressDriver {
            schedule: Arc::clone(&schedule),
            observations: receive,
            nonce,
            round: 0,
            cycles: 0,
            attempts: 0,
            last_outcome: Outcome::NotAttempted,
            cycle_started_us: None,
            checked_at_us: None,
        };
        Ok((schedule, driver))
    }

    fn check(&self) -> io::Result<()> {
        match self.failure.load(SeqCst) {
            OBSERVER_OVERFLOW => Err(invalid("memory fixture observation overflow")),
            WRONG_WORKER => Err(invalid("memory fixture has more than one Reporter worker")),
            CONTROL_FAILED => Err(invalid("memory fixture control failed")),
            _ if self.closed.load(SeqCst) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "memory fixture control closed",
            )),
            _ => Ok(()),
        }
    }

    fn fail(&self, reason: u8) {
        let _ = self.failure.compare_exchange(0, reason, SeqCst, SeqCst);
        self.close();
    }

    pub fn close(&self) {
        if !self.closed.swap(true, SeqCst) {
            let _ = self.observations.try_send(Observation::Closed);
            self.wake();
        }
    }

    fn wake(&self) {
        if let Some(worker) = self.worker.get() {
            worker.unpark();
        }
    }

    fn set(&self, at_us: u64) -> io::Result<()> {
        self.check()?;
        self.origin
            .checked_add(Duration::from_micros(at_us))
            .ok_or_else(|| invalid("memory fixture Instant overflow"))?;
        self.at_us.store(at_us, SeqCst);
        self.wake();
        Ok(())
    }

    fn relative(&self, at: Instant) -> io::Result<u64> {
        let elapsed = at
            .checked_duration_since(self.origin)
            .ok_or_else(|| invalid("memory fixture observation predates origin"))?;
        let micros = u64::try_from(elapsed.as_micros())
            .map_err(|_| invalid("memory fixture observation overflow"))?;
        if elapsed != Duration::from_micros(micros) {
            return Err(invalid(
                "memory fixture observation is not at a clock instant",
            ));
        }
        Ok(micros)
    }
}

impl ReportSchedule for FixtureSchedule {
    fn now(&self) -> Instant {
        // set checks this addition before publishing the new value.
        self.origin + Duration::from_micros(self.at_us.load(SeqCst))
    }

    fn park(&self, _remaining: Duration) {
        // Even a closed controller must not turn a frozen Reporter into a spin.
        // Reporter::finish retains its own real unpark and 100 ms host bound.
        thread::park();
    }

    fn observe(&self, progress: ReportProgress) {
        if self.closed.load(SeqCst) {
            return;
        }
        let current = thread::current();
        let worker = self.worker.get_or_init(|| current.clone());
        if worker.id() != current.id() {
            self.fail(WRONG_WORKER);
            return;
        }
        if self
            .observations
            .try_send(Observation::Progress(progress))
            .is_err()
        {
            self.fail(OBSERVER_OVERFLOW);
        }
    }
}

/// One owner validates every observation and every consecutive parent step.
pub struct ProgressDriver {
    schedule: Arc<FixtureSchedule>,
    observations: Receiver<Observation>,
    nonce: String,
    round: u64,
    cycles: u64,
    attempts: u64,
    last_outcome: Outcome,
    cycle_started_us: Option<u64>,
    checked_at_us: Option<u64>,
}

impl ProgressDriver {
    pub fn settle(&mut self, step: &Step, deadline: Instant) -> io::Result<Checked> {
        let result = self.settle_inner(step, deadline);
        if result.is_err() {
            self.schedule.fail(CONTROL_FAILED);
        }
        result
    }

    fn settle_inner(&mut self, step: &Step, deadline: Instant) -> io::Result<Checked> {
        remaining(deadline)?;
        self.schedule.check()?;
        self.drain(deadline)?;
        let current = self.schedule.at_us.load(SeqCst);
        if step.nonce != self.nonce
            || self.round.checked_add(1) != Some(step.round)
            || step.at_us < current
            || (self.round == 0 && step.at_us != 0)
            || (self.round != 0 && step.at_us > self.next_due()?)
        {
            return Err(invalid("invalid memory fixture step"));
        }
        if step.at_us != current {
            self.schedule.set(step.at_us)?;
        }
        while self.checked_at_us != Some(step.at_us) {
            self.schedule.check()?;
            let observation = self
                .observations
                .recv_timeout(remaining(deadline)?)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => io::Error::new(
                        io::ErrorKind::TimedOut,
                        "memory fixture Reporter boundary deadline",
                    ),
                    mpsc::RecvTimeoutError::Disconnected => {
                        invalid("memory fixture Reporter observer disconnected")
                    }
                })?;
            self.record(observation)?;
        }
        self.drain(deadline)?;
        self.schedule.check()?;
        remaining(deadline)?;
        self.round = step.round;
        Ok(Checked {
            nonce: self.nonce.clone(),
            round: self.round,
            at_us: step.at_us,
            cycles: self.cycles,
            attempts: self.attempts,
            last_outcome: self.last_outcome,
            cycle_started_us: self
                .cycle_started_us
                .ok_or_else(|| invalid("memory fixture checked before a cycle"))?,
            next_due_us: self.next_due()?,
            marker: marker(&self.nonce, self.round)?,
        })
    }

    fn next_due(&self) -> io::Result<u64> {
        self.cycle_started_us
            .and_then(|start| start.checked_add(REPORT_EVERY.as_micros() as u64))
            .ok_or_else(|| invalid("memory fixture has no representable next deadline"))
    }

    fn drain(&mut self, deadline: Instant) -> io::Result<()> {
        loop {
            remaining(deadline)?;
            self.schedule.check()?;
            match self.observations.try_recv() {
                Ok(observation) => self.record(observation)?,
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    return Err(invalid("memory fixture Reporter observer disconnected"));
                }
            }
        }
    }

    fn record(&mut self, observation: Observation) -> io::Result<()> {
        self.schedule.check()?;
        let Observation::Progress(progress) = observation else {
            return Err(invalid("memory fixture controller closed"));
        };
        match progress {
            ReportProgress::CycleComplete {
                started,
                publication,
            } => {
                let started = self.schedule.relative(started)?;
                let expected = if self.cycles == 0 {
                    0
                } else {
                    self.next_due()?
                };
                if started != expected || started != self.schedule.at_us.load(SeqCst) {
                    return Err(invalid("memory fixture unexpected completed cycle"));
                }
                self.cycles = self
                    .cycles
                    .checked_add(1)
                    .ok_or_else(|| invalid("cycle overflow"))?;
                self.last_outcome = match publication {
                    PublicationOutcome::NotAttempted => Outcome::NotAttempted,
                    PublicationOutcome::ReturnedOk => Outcome::ReturnedOk,
                    PublicationOutcome::ReturnedErr => Outcome::ReturnedErr,
                };
                if publication != PublicationOutcome::NotAttempted {
                    self.attempts = self
                        .attempts
                        .checked_add(1)
                        .ok_or_else(|| invalid("attempt overflow"))?;
                }
                self.cycle_started_us = Some(started);
                self.checked_at_us = None;
            }
            ReportProgress::WaitChecked {
                started,
                at,
                remaining,
            } => {
                let started = self.schedule.relative(started)?;
                let at = self.schedule.relative(at)?;
                let next = self.next_due()?;
                if self.cycle_started_us != Some(started)
                    || at < started
                    || at >= next
                    || at > self.schedule.at_us.load(SeqCst)
                    || self.checked_at_us.is_some_and(|checked| at < checked)
                    || remaining != Duration::from_micros(next - at)
                {
                    return Err(invalid("memory fixture invalid positive wait boundary"));
                }
                self.checked_at_us = Some(at);
            }
        }
        Ok(())
    }
}

impl Drop for ProgressDriver {
    fn drop(&mut self) {
        self.schedule.close();
    }
}

pub struct ChildControl {
    schedule: Arc<FixtureSchedule>,
    cancel: UnixStream,
    worker: Option<JoinHandle<()>>,
}

impl ChildControl {
    /// Only this dedicated process reads bootstrap stdin. A blocked reader is
    /// ended by this process's exit, never joined without a completion proof.
    pub fn bootstrap() -> io::Result<Self> {
        let deadline = Instant::now() + HOST_WAIT;
        let (send, receive) = mpsc::sync_channel(1);
        let bootstrap = thread::Builder::new()
            .name("memory-fixture-bootstrap".into())
            .spawn(move || {
                let result = connect(deadline);
                let _ = send.send(result);
            })?;
        let received = receive.recv_timeout(remaining(deadline)?);
        if bootstrap.is_finished() {
            let _ = bootstrap.join();
        }
        let (bootstrap, mut channel) = received.map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => {
                io::Error::new(io::ErrorKind::TimedOut, "memory fixture bootstrap deadline")
            }
            mpsc::RecvTimeoutError::Disconnected => invalid("memory fixture bootstrap stopped"),
        })??;
        let cancel = channel.try_clone_stream()?;
        let (schedule, mut driver) = FixtureSchedule::pair(bootstrap.nonce)?;
        let shared = Arc::clone(&schedule);
        let worker = thread::Builder::new()
            .name("memory-fixture-control".into())
            .spawn(move || {
                let result = (|| -> io::Result<()> {
                    loop {
                        let deadline = Instant::now() + HOST_WAIT;
                        let step: Step = channel.read_until(deadline)?;
                        let checked = driver.settle(&step, deadline)?;
                        shared.check()?;
                        // This actual stderr write shares the application's
                        // installed relay. The parent must observe the line.
                        write_marker(&mut io::stderr(), &checked)?;
                        shared.check()?;
                        channel.write_until(&checked, deadline)?;
                    }
                })();
                if result.is_err() {
                    shared.fail(CONTROL_FAILED);
                }
            })?;
        Ok(Self {
            schedule,
            cancel,
            worker: Some(worker),
        })
    }

    pub fn schedule(&self) -> Arc<FixtureSchedule> {
        Arc::clone(&self.schedule)
    }
}

impl Drop for ChildControl {
    fn drop(&mut self) {
        self.schedule.close();
        let _ = self.cancel.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
        // A Reporter blocked in real I/O or a control writer blocked in a
        // full host stderr pipe ends with the owned child process. Neither is
        // evidence of completion; the parent retains kill/try_wait/reap.
    }
}

fn connect(deadline: Instant) -> io::Result<(Bootstrap, Channel)> {
    let mut stdin = File::open("/dev/stdin")?;
    if !stdin.metadata()?.file_type().is_fifo() {
        return Err(invalid("memory fixture bootstrap requires piped stdin"));
    }
    let bootstrap: Bootstrap = read_frame(&mut stdin)?;
    let mut extra = [0];
    if stdin.read(&mut extra)? != 0 {
        return Err(invalid(
            "memory fixture bootstrap requires one frame then EOF",
        ));
    }
    remaining(deadline)?;
    bootstrap.validate()?;
    let mut channel = Channel::new(UnixStream::connect(&bootstrap.control)?)?;
    channel.write_until(
        &Connected {
            version: VERSION,
            nonce: bootstrap.nonce.clone(),
        },
        deadline,
    )?;
    Ok((bootstrap, channel))
}

pub fn write_marker(writer: &mut impl Write, checked: &Checked) -> io::Result<()> {
    if checked.marker != marker(&checked.nonce, checked.round)? {
        return Err(invalid("memory fixture marker does not match receipt"));
    }
    writer.write_all(checked.marker.as_bytes())?;
    writer.flush()
}
