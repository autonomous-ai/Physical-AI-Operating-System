//! Private inherited-pipe control for the explicit Records fixture.
//! Checked means a completed reporting wait, never delivery or process exit.
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::{self, Read, Write},
    os::fd::AsFd,
    time::{Duration, Instant},
};

pub const VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 4096;
pub const HOST_WAIT: Duration = Duration::from_secs(4);
pub const PERIOD_NS: u64 = 1_000_000_000;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    pub version: u32,
    pub nonce: String,
}

impl Bootstrap {
    pub fn validate(&self) -> io::Result<()> {
        if self.version != VERSION || !valid_nonce(&self.nonce) {
            return Err(invalid("invalid records fixture bootstrap"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub nonce: String,
    pub round: u64,
    pub at_ns: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    ReturnedOk,
    ReturnedErr,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Checked {
    pub version: u32,
    pub nonce: String,
    pub round: u64,
    pub at_ns: u64,
    pub completed_cycles: u64,
    pub cycle_started_ns: u64,
    pub next_due_ns: u64,
    pub construction_or_publish: Outcome,
}

pub fn valid_nonce(nonce: &str) -> bool {
    !nonce.is_empty() && nonce.len() <= 32 && nonce.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

pub fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "records fixture host deadline"))
}

/// Exactly one caller owns each endpoint. Dropping/closing the channel drops
/// both owned handles; no reader thread or buffered read-ahead outlives it.
pub struct Channel<R, W> {
    reader: Option<R>,
    writer: Option<W>,
}

impl<R: Read + AsFd, W: Write + AsFd> Channel<R, W> {
    pub fn new(reader: R, writer: W) -> io::Result<Self> {
        for descriptor in [reader.as_fd(), writer.as_fd()] {
            let flags = fcntl_getfl(descriptor)?;
            fcntl_setfl(descriptor, flags | OFlags::NONBLOCK)?;
        }
        Ok(Self {
            reader: Some(reader),
            writer: Some(writer),
        })
    }

    pub fn read_until<T: DeserializeOwned>(&mut self, deadline: Instant) -> io::Result<T> {
        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| invalid("records fixture channel is closed"))?;
        let mut frame = Vec::with_capacity(256);
        loop {
            remaining(deadline)?;
            let mut byte = [0];
            match reader.read(&mut byte) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "records fixture frame ended before LF",
                    ));
                }
                Ok(_) => {
                    frame.push(byte[0]);
                    if frame.len() > MAX_FRAME_BYTES {
                        return Err(invalid("records fixture frame exceeds bound"));
                    }
                    if byte[0] == b'\n' {
                        return serde_json::from_slice(&frame[..frame.len() - 1])
                            .map_err(io::Error::from);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_ready(reader, PollFlags::IN, deadline)?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn write_until<T: Serialize>(&mut self, value: &T, deadline: Instant) -> io::Result<()> {
        let mut frame = serde_json::to_vec(value)?;
        frame.push(b'\n');
        if frame.len() > MAX_FRAME_BYTES {
            return Err(invalid("records fixture frame exceeds bound"));
        }
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| invalid("records fixture channel is closed"))?;
        let mut sent = 0;
        while sent < frame.len() {
            remaining(deadline)?;
            match writer.write(&frame[sent..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(count) => sent += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_ready(writer, PollFlags::OUT, deadline)?;
                }
                Err(error) => return Err(error),
            }
        }
        // All users pass unbuffered File/ChildStdin endpoints. No hidden
        // buffering or separate flush can escape the deadline above.
        remaining(deadline)?;
        Ok(())
    }
}

impl<R, W> Channel<R, W> {
    pub fn close(&mut self) {
        self.reader.take();
        self.writer.take();
    }
}

fn wait_ready(fd: &impl AsFd, flags: PollFlags, deadline: Instant) -> io::Result<()> {
    loop {
        let left = remaining(deadline)?.min(Duration::from_millis(i32::MAX as u64));
        let timeout = Timespec::try_from(left)
            .map_err(|_| invalid("records fixture poll timeout overflow"))?;
        let mut descriptors = [PollFd::new(fd, flags)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => {}
            Ok(_) => {
                let events = descriptors[0].revents();
                if events.contains(PollFlags::NVAL) {
                    return Err(invalid("records fixture invalid descriptor"));
                }
                // HUP is not EOF: try reading complete queued bytes first.
                if events.intersects(flags | PollFlags::HUP | PollFlags::ERR) {
                    return Ok(());
                }
            }
            Err(error) if error == rustix::io::Errno::INTR => {}
            Err(error) => return Err(error.into()),
        }
    }
}
