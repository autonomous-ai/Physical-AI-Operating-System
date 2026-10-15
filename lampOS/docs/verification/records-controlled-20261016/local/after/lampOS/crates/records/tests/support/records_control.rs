//! Parent-side Records fixture control. Checked is not an Event or an exit.
#[path = "../../src/fixture_protocol.rs"]
pub mod protocol;

use protocol::{
    Bootstrap, Channel, Checked, HOST_WAIT, PERIOD_NS, Step, VERSION, invalid, remaining,
};
use std::{
    io::{self, Read, Write},
    os::fd::AsFd,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

#[derive(Clone, Copy)]
pub struct Window {
    deadline: Instant,
    start_ns: u64,
    end_ns: u64,
}

impl Window {
    pub fn new(deadline: Instant, start_ns: u64) -> io::Result<Self> {
        remaining(deadline)?;
        let span = u64::try_from(HOST_WAIT.as_nanos())
            .map_err(|_| invalid("records fixture window overflow"))?;
        Ok(Self {
            deadline,
            start_ns,
            end_ns: start_ns
                .checked_add(span)
                .ok_or_else(|| invalid("records fixture window overflow"))?,
        })
    }

    fn contains(&self, at_ns: u64) -> io::Result<()> {
        remaining(self.deadline)?;
        if at_ns < self.start_ns || at_ns > self.end_ns {
            return Err(invalid(
                "records fixture step is outside its original window",
            ));
        }
        Ok(())
    }

    pub fn receiving(&self, at_ns: u64) -> io::Result<()> {
        self.contains(at_ns)?;
        if at_ns == self.end_ns {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "records fixture logical status deadline",
            ));
        }
        Ok(())
    }
}

pub struct Controller<R, W> {
    channel: Channel<R, W>,
    nonce: String,
    checked: Option<Checked>,
    failure: Option<(io::ErrorKind, String)>,
}

impl<R: Read + AsFd, W: Write + AsFd> Controller<R, W> {
    pub fn new(channel: Channel<R, W>, nonce: String) -> io::Result<Self> {
        Bootstrap {
            version: VERSION,
            nonce: nonce.clone(),
        }
        .validate()?;
        Ok(Self {
            channel,
            nonce,
            checked: None,
            failure: None,
        })
    }

    pub fn elapsed_ns(&self) -> io::Result<u64> {
        self.live()?;
        Ok(self.checked.as_ref().map_or(0, |checked| checked.at_ns))
    }

    pub fn settle_current(&mut self, window: &Window) -> io::Result<Checked> {
        self.step_to(self.elapsed_ns()?, window)
    }

    pub fn step_due(&mut self, window: &Window) -> io::Result<Checked> {
        self.live()?;
        let due = self
            .checked
            .as_ref()
            .map_or(PERIOD_NS, |checked| checked.next_due_ns);
        self.step_to(due, window)
    }

    pub fn step_to(&mut self, at_ns: u64, window: &Window) -> io::Result<Checked> {
        self.live()?;
        let result = self.exchange(at_ns, window);
        if let Err(error) = &result {
            self.failure = Some((error.kind(), error.to_string()));
            self.channel.close();
        }
        result
    }

    fn exchange(&mut self, at_ns: u64, window: &Window) -> io::Result<Checked> {
        window.contains(self.elapsed_ns()?)?;
        window.contains(at_ns)?;
        let round = match &self.checked {
            None if at_ns == 0 => 1,
            Some(previous) if previous.at_ns <= at_ns && at_ns <= previous.next_due_ns => previous
                .round
                .checked_add(1)
                .ok_or_else(|| invalid("records fixture round overflow"))?,
            _ => return Err(invalid("invalid parent records fixture step")),
        };
        if self.checked.is_none() {
            self.channel.write_until(
                &Bootstrap {
                    version: VERSION,
                    nonce: self.nonce.clone(),
                },
                window.deadline,
            )?;
        }
        self.channel.write_until(
            &Step {
                nonce: self.nonce.clone(),
                round,
                at_ns,
            },
            window.deadline,
        )?;
        let checked: Checked = self.channel.read_until(window.deadline)?;
        self.validate(&checked, round, at_ns)?;
        remaining(window.deadline)?;
        // Commit only after the entire actual response has been validated.
        self.checked = Some(checked.clone());
        Ok(checked)
    }

    fn validate(&self, checked: &Checked, round: u64, at_ns: u64) -> io::Result<()> {
        if checked.version != VERSION
            || checked.nonce != self.nonce
            || checked.round != round
            || checked.at_ns != at_ns
        {
            return Err(invalid("records fixture response identity mismatch"));
        }
        let valid = match &self.checked {
            None => {
                checked.completed_cycles == 1
                    && checked.cycle_started_ns == 0
                    && checked.next_due_ns == PERIOD_NS
            }
            Some(previous) if at_ns < previous.next_due_ns => {
                checked.completed_cycles == previous.completed_cycles
                    && checked.cycle_started_ns == previous.cycle_started_ns
                    && checked.next_due_ns == previous.next_due_ns
                    && checked.construction_or_publish == previous.construction_or_publish
            }
            Some(previous) => {
                Some(checked.completed_cycles) == previous.completed_cycles.checked_add(1)
                    && checked.cycle_started_ns == at_ns
                    && Some(checked.next_due_ns) == at_ns.checked_add(PERIOD_NS)
            }
        };
        if !valid {
            return Err(invalid("records fixture response cycle mismatch"));
        }
        Ok(())
    }

    fn live(&self) -> io::Result<()> {
        match &self.failure {
            Some((kind, message)) => Err(io::Error::new(*kind, message.clone())),
            None => Ok(()),
        }
    }

    pub fn close(&mut self) {
        if self.failure.is_none() {
            self.failure = Some((
                io::ErrorKind::BrokenPipe,
                "records fixture controller closed".into(),
            ));
        }
        self.channel.close();
    }
}

pub struct RunningReporter(pub Child, Option<Controller<ChildStdout, ChildStdin>>);

impl RunningReporter {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        Self::from_child(child)
    }

    pub fn from_child(child: Child) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Establish cleanup ownership before either descriptor or fcntl step.
        let mut owned = Self(child, None);
        let input = owned
            .0
            .stdin
            .take()
            .ok_or_else(|| invalid("missing fixture stdin"))?;
        let output = owned
            .0
            .stdout
            .take()
            .ok_or_else(|| invalid("missing fixture stdout"))?;
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                old.checked_add(1)
            })
            .map_err(|_| invalid("records fixture nonce exhausted"))?;
        let nonce = format!("r{:x}c{id:x}", std::process::id());
        owned.1 = Some(Controller::new(Channel::new(output, input)?, nonce)?);
        Ok(owned)
    }

    fn controller(&mut self) -> io::Result<&mut Controller<ChildStdout, ChildStdin>> {
        self.1
            .as_mut()
            .ok_or_else(|| invalid("records fixture control is unavailable"))
    }

    pub fn elapsed_ns(&self) -> io::Result<u64> {
        self.1
            .as_ref()
            .ok_or_else(|| invalid("records fixture control is unavailable"))?
            .elapsed_ns()
    }

    pub fn settle_current(&mut self, window: &Window) -> io::Result<Checked> {
        self.controller()?.settle_current(window)
    }

    pub fn step_due(&mut self, window: &Window) -> io::Result<Checked> {
        self.controller()?.step_due(window)
    }

    pub fn close(&mut self) {
        if let Some(controller) = &mut self.1 {
            controller.close();
        }
    }
}

impl Drop for RunningReporter {
    fn drop(&mut self) {
        self.close();
        // Reap this test's own child on success and during panic unwinding.
        // Its independent 30-second lifetime also bounds a failed test run.
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
