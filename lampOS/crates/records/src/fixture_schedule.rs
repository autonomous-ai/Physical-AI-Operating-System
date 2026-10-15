//! A single synchronous reporting loop owns all fixture clock advancement.
use crate::fixture_protocol::{
    Bootstrap, Channel, Checked, Outcome, PERIOD_NS, Step, VERSION, invalid,
};
use crate::{ReportCycle, ReportSchedule};
use std::{
    io::{self, Read, Write},
    os::fd::AsFd,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Clock {
    epoch: Instant,
    elapsed_ns: Arc<AtomicU64>,
}

impl Clock {
    pub fn now(&self) -> Instant {
        // Only set() writes the offset, after checking this exact addition.
        self.epoch
            .checked_add(Duration::from_nanos(
                self.elapsed_ns.load(Ordering::Acquire),
            ))
            .expect("validated records fixture clock")
    }

    fn elapsed(&self) -> u64 {
        self.elapsed_ns.load(Ordering::Acquire)
    }

    fn set(&self, at_ns: u64) -> io::Result<()> {
        if at_ns < self.elapsed()
            || self
                .epoch
                .checked_add(Duration::from_nanos(at_ns))
                .is_none()
        {
            return Err(invalid("records fixture clock regressed or overflowed"));
        }
        self.elapsed_ns.store(at_ns, Ordering::Release);
        Ok(())
    }
}

pub struct Schedule<R, W> {
    channel: Channel<R, W>,
    nonce: String,
    clock: Clock,
    round: u64,
    cycles: u64,
    pending: Option<Step>,
    host_deadline: Option<Instant>,
    failed: bool,
}

impl<R: Read + AsFd, W: Write + AsFd> Schedule<R, W> {
    pub fn bootstrap(mut channel: Channel<R, W>, deadline: Instant) -> io::Result<Self> {
        let bootstrap: Bootstrap = channel.read_until(deadline)?;
        bootstrap.validate()?;
        Ok(Self {
            channel,
            nonce: bootstrap.nonce,
            clock: Clock {
                epoch: Instant::now(),
                elapsed_ns: Arc::new(AtomicU64::new(0)),
            },
            round: 0,
            cycles: 0,
            pending: None,
            host_deadline: None,
            failed: false,
        })
    }

    pub fn clock(&self) -> Clock {
        self.clock.clone()
    }

    fn wait_inner(&mut self, cycle: ReportCycle, remaining: Duration) -> io::Result<()> {
        let seconds = cycle
            .host_seconds
            .filter(|seconds| (1..=86_400).contains(seconds))
            .ok_or_else(|| invalid("records fixture requires an explicit host lifetime"))?;
        let deadline = cycle
            .host_started
            .checked_add(Duration::from_secs(seconds))
            .ok_or_else(|| invalid("records fixture lifetime overflow"))?;
        if self
            .host_deadline
            .is_some_and(|previous| previous != deadline)
        {
            return Err(invalid("records fixture host lifetime changed"));
        }
        self.host_deadline = Some(deadline);
        let started = cycle
            .started
            .checked_duration_since(self.clock.epoch)
            .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok())
            .ok_or_else(|| invalid("records fixture cycle start is outside its clock"))?;
        let due = started
            .checked_add(PERIOD_NS)
            .ok_or_else(|| invalid("records fixture due time overflow"))?;
        let due_instant = cycle
            .started
            .checked_add(Duration::from_nanos(PERIOD_NS))
            .ok_or_else(|| invalid("records fixture due Instant overflow"))?;
        let after_wait = self
            .clock
            .now()
            .checked_add(remaining)
            .ok_or_else(|| invalid("records fixture wait Instant overflow"))?;
        if remaining.is_zero()
            || started != self.clock.elapsed()
            || due_instant != after_wait
            || (self.cycles == 0 && (started != 0 || self.pending.is_some()))
            || (self.cycles != 0
                && self
                    .pending
                    .as_ref()
                    .is_none_or(|step| step.at_ns != started))
        {
            return Err(invalid(
                "records fixture did not reach its positive due wait",
            ));
        }
        self.cycles = self
            .cycles
            .checked_add(1)
            .ok_or_else(|| invalid("records fixture cycle count overflow"))?;
        let outcome = if cycle.construction_or_publish.is_ok() {
            Outcome::ReturnedOk
        } else {
            Outcome::ReturnedErr
        };
        if let Some(step) = self.pending.take() {
            self.reply(&step, started, due, outcome, deadline)?;
        }
        loop {
            let step: Step = self.channel.read_until(deadline)?;
            let next_round = self
                .round
                .checked_add(1)
                .ok_or_else(|| invalid("records fixture round overflow"))?;
            if step.nonce != self.nonce
                || step.round != next_round
                || step.at_ns < self.clock.elapsed()
                || step.at_ns > due
                || (self.round == 0 && step.at_ns != 0)
            {
                return Err(invalid("invalid records fixture step"));
            }
            self.clock.set(step.at_ns)?;
            self.round = step.round;
            if step.at_ns == due {
                self.pending = Some(step);
                return Ok(());
            }
            self.reply(&step, started, due, outcome, deadline)?;
        }
    }

    fn reply(
        &mut self,
        step: &Step,
        started: u64,
        due: u64,
        outcome: Outcome,
        deadline: Instant,
    ) -> io::Result<()> {
        self.channel.write_until(
            &Checked {
                version: VERSION,
                nonce: self.nonce.clone(),
                round: step.round,
                at_ns: step.at_ns,
                completed_cycles: self.cycles,
                cycle_started_ns: started,
                next_due_ns: due,
                construction_or_publish: outcome,
            },
            deadline,
        )
    }
}

impl<R: Read + AsFd, W: Write + AsFd> ReportSchedule for Schedule<R, W> {
    fn now(&self) -> Instant {
        self.clock.now()
    }

    fn wait(&mut self, cycle: ReportCycle, remaining: Duration) -> io::Result<()> {
        if self.failed {
            return Err(invalid("records fixture schedule already failed"));
        }
        let result = self.wait_inner(cycle, remaining);
        if result.is_err() {
            self.failed = true;
            self.channel.close();
        }
        result
    }
}

#[cfg(not(test))]
pub fn report(
    config: std::path::PathBuf,
    result: std::path::PathBuf,
    seconds: Option<u64>,
) -> io::Result<()> {
    use crate::{Reporter, fixture_protocol::HOST_WAIT, reporting_with};
    use std::fs::File;
    if seconds.is_none() {
        return Err(invalid("records fixture requires --seconds"));
    }
    let reader = File::from(rustix::io::fcntl_dupfd_cloexec(io::stdin(), 0)?);
    let writer = File::from(rustix::io::fcntl_dupfd_cloexec(io::stdout(), 0)?);
    let mut schedule =
        Schedule::bootstrap(Channel::new(reader, writer)?, Instant::now() + HOST_WAIT)?;
    let clock = schedule.clock();
    reporting_with(config, result, seconds, &mut schedule, |root| {
        let clock = clock.clone();
        Reporter::new_with_cadence_clock(root, move || clock.now())
    })
}
