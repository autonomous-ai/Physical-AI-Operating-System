//! Opt-in operational logs on one event topic. Admission never performs I/O.
//!
//! This transport-metadata schema lives here so the bus CLI and `lamp-msgs`
//! re-export use one definition without a dependency cycle. A log is not a health
//! report, a transcript, a credential store, or durable evidence.
use crate::{Bus, Declaration, JSON, Kind, Message, Publisher, name, wire};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

pub const QUEUE_CAPACITY: usize = 64;
pub const MAX_RECORD_BYTES: usize = 4 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 512;
pub const MAX_CODE_BYTES: usize = 64;
pub const FINISH_WAIT: Duration = Duration::from_millis(500);
const ALIVE_EVERY: Duration = Duration::from_millis(100);

/// Operational severity, deliberately distinct from diagnostic health levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }
}

impl FromStr for LogLevel {
    type Err = io::Error;

    fn from_str(value: &str) -> io::Result<Self> {
        match value {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            "fatal" => Ok(Self::Fatal),
            _ => Err(invalid(
                "log level must be debug, info, warn, error or fatal",
            )),
        }
    }
}

/// One bounded operational summary. Unknown JSON fields remain forward compatible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Log {
    /// The publisher's source name, not a person's or conversation's identity.
    pub part: String,
    pub level: LogLevel,
    /// A stable, plain identifier: `[a-z][a-z0-9_.-]{0,63}`.
    pub code: String,
    /// At most 512 UTF-8 bytes. Never include secrets or conversation contents.
    pub message: String,
    /// Open-logger admission attempts from 1, including invalid and full attempts.
    /// Independent of the bus sequence, which advances only when publishing.
    pub attempt: u64,
    /// Wall time at admission, independent of the bus's publication wall time.
    pub created_unix_ms: u64,
}

impl Log {
    pub const TYPE: &str = "lamp_msgs/Log";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "logs";

    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Event)
    }

    pub fn check(&self) -> io::Result<()> {
        name::source(&self.part)?;
        fields(&self.code, &self.message)?;
        if self.attempt == 0 {
            return Err(invalid("log attempt must be positive"));
        }
        Ok(())
    }

    /// Strict log envelope validation, including the header-to-payload identity.
    /// An undeclared generic JSON message is not accepted as an operational log.
    pub fn from_message(message: &Message) -> io::Result<Self> {
        if message.header.topic != Self::TOPIC
            || message.header.kind != Kind::Event
            || message.header.enc != JSON
            || message.header.type_name.as_deref() != Some(Self::TYPE)
            || message.payload.len() > MAX_RECORD_BYTES
        {
            return Err(invalid("message is not a declared JSON log event"));
        }
        let log: Self = message.parse()?;
        log.check()?;
        if log.part != message.header.source {
            return Err(invalid("log part differs from its publisher"));
        }
        Ok(log)
    }

    pub fn check_part(part: &str) -> io::Result<()> {
        name::source(part)
    }
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, why)
}

fn fields(code: &str, message: &str) -> io::Result<()> {
    if !(1..=MAX_CODE_BYTES).contains(&code.len())
        || !code.as_bytes()[0].is_ascii_lowercase()
        || !code.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
    {
        return Err(invalid("log code must be [a-z][a-z0-9_.-]{0,63}"));
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(invalid("log message exceeds 512 UTF-8 bytes"));
    }
    Ok(())
}

struct Packet {
    bytes: [u8; MAX_RECORD_BYTES],
    len: usize,
    at_us: u64,
}

impl Packet {
    fn encode(log: &Log, at_us: u64) -> io::Result<Self> {
        let mut packet = Self {
            bytes: [0; MAX_RECORD_BYTES],
            len: 0,
            at_us,
        };
        serde_json::to_writer(&mut packet, log)?;
        Ok(packet)
    }
}

impl Write for Packet {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .len
            .checked_add(bytes.len())
            .filter(|end| *end <= MAX_RECORD_BYTES)
            .ok_or_else(|| invalid("serialized log exceeds 4096 bytes"))?;
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Disabled,
    Queued,
    Full,
    Invalid,
    Closed,
}

/// Counters are snapshots while running, final once finish is confirmed.
/// `published` acknowledges bus calls, never subscriber receipt or persistence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub enabled: bool,
    pub closed: bool,
    pub confirmed: bool,
    pub worker_finished: bool,
    pub accepted: u64,
    pub full: u64,
    pub invalid: u64,
    pub rejected_closed: u64,
    pub processed: u64,
    pub published: u64,
    pub publish_errors: u64,
    pub alive_errors: u64,
    pub subscriber_drops: u64,
    pub unconfirmed: u64,
}

#[derive(Default)]
struct Counts {
    accepted: AtomicU64,
    full: AtomicU64,
    invalid: AtomicU64,
    rejected_closed: AtomicU64,
    processed: AtomicU64,
    published: AtomicU64,
    publish_errors: AtomicU64,
    alive_errors: AtomicU64,
    subscriber_drops: AtomicU64,
    finished: AtomicBool,
}

struct Running {
    queue: Option<SyncSender<Packet>>,
    counts: Arc<Counts>,
    done: Receiver<()>,
    confirmed: bool,
}

/// One owner per part/process; not cloneable. Other threads coordinate with that
/// owner. Default/disabled construction starts no worker and touches nothing.
#[derive(Default)]
pub struct Logger {
    part: String,
    attempt: u64,
    started: Option<Instant>,
    running: Option<Running>,
}

impl Logger {
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Explicit opt-in. Declares the topic synchronously before spawning the
    /// worker; call outside interaction handling. A failure leaves no worker.
    pub fn start(bus: &Bus, part: &str) -> io::Result<Self> {
        let publisher = bus.publisher_declared(Log::TOPIC, &Log::declaration(), part)?;
        Self::spawn(part, publisher)
    }

    fn spawn(part: &str, sink: impl Sink + Send + 'static) -> io::Result<Self> {
        name::source(part)?;
        let (queue, incoming) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (acknowledge, done) = mpsc::sync_channel(1);
        let counts = Arc::new(Counts::default());
        let shared = Arc::clone(&counts);
        let worker = thread::Builder::new()
            .name("lamp-log".into())
            .spawn(move || {
                consume(sink, incoming, &shared);
                shared.finished.store(true, Ordering::Release);
                let _ = acknowledge.try_send(());
            })?;
        // Filesystem calls are not preemptible. No caller joins a stalled worker.
        drop(worker);
        Ok(Self {
            part: part.into(),
            attempt: 0,
            started: Some(Instant::now()),
            running: Some(Running {
                queue: Some(queue),
                counts,
                done,
                confirmed: false,
            }),
        })
    }

    /// Bounded encoding and a non-waiting enqueue only: no filesystem, socket,
    /// stdout or stderr operations. Full/invalid records are rejected whole.
    pub fn try_log(&mut self, level: LogLevel, code: &str, message: &str) -> Admission {
        let Some(running) = &mut self.running else {
            return Admission::Disabled;
        };
        let Some(queue) = &running.queue else {
            running
                .counts
                .rejected_closed
                .fetch_add(1, Ordering::Relaxed);
            return Admission::Closed;
        };
        let Some(attempt) = self.attempt.checked_add(1) else {
            running.counts.invalid.fetch_add(1, Ordering::Relaxed);
            return Admission::Invalid;
        };
        self.attempt = attempt;
        if fields(code, message).is_err() {
            running.counts.invalid.fetch_add(1, Ordering::Relaxed);
            return Admission::Invalid;
        }
        let created_unix_ms = wire::unix_ms();
        let at_us = self.started.map_or(0, |at| {
            u64::try_from(at.elapsed().as_micros()).unwrap_or(u64::MAX)
        });
        let log = Log {
            part: self.part.clone(),
            level,
            code: code.into(),
            message: message.into(),
            attempt,
            created_unix_ms,
        };
        let packet = match Packet::encode(&log, at_us) {
            Ok(packet) => packet,
            Err(_) => {
                running.counts.invalid.fetch_add(1, Ordering::Relaxed);
                return Admission::Invalid;
            }
        };
        match queue.try_send(packet) {
            Ok(()) => {
                running.counts.accepted.fetch_add(1, Ordering::Release);
                Admission::Queued
            }
            Err(TrySendError::Full(_)) => {
                running.counts.full.fetch_add(1, Ordering::Relaxed);
                Admission::Full
            }
            Err(TrySendError::Disconnected(_)) => {
                running.queue.take();
                running
                    .counts
                    .rejected_closed
                    .fetch_add(1, Ordering::Relaxed);
                Admission::Closed
            }
        }
    }

    pub fn report(&self) -> Report {
        let Some(running) = &self.running else {
            return Report::default();
        };
        let counts = &running.counts;
        let accepted = counts.accepted.load(Ordering::Acquire);
        let processed = counts.processed.load(Ordering::Acquire);
        Report {
            enabled: true,
            closed: running.queue.is_none(),
            confirmed: running.confirmed,
            worker_finished: counts.finished.load(Ordering::Acquire),
            accepted,
            full: counts.full.load(Ordering::Acquire),
            invalid: counts.invalid.load(Ordering::Acquire),
            rejected_closed: counts.rejected_closed.load(Ordering::Acquire),
            processed,
            published: counts.published.load(Ordering::Acquire),
            publish_errors: counts.publish_errors.load(Ordering::Acquire),
            alive_errors: counts.alive_errors.load(Ordering::Acquire),
            subscriber_drops: counts.subscriber_drops.load(Ordering::Acquire),
            unconfirmed: accepted.saturating_sub(processed),
        }
    }

    /// Close admission, drain accepted records and release the publisher, waiting
    /// at most min(wait, 500 ms). Confirmation does not promise delivery or disk.
    /// A timed-out result stays truthful even if a later finish observes success.
    pub fn finish(&mut self, wait: Duration) -> Report {
        if let Some(running) = &mut self.running {
            running.queue.take();
            if !running.confirmed {
                running.confirmed = running.done.recv_timeout(wait.min(FINISH_WAIT)).is_ok();
            }
        }
        self.report()
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        if let Some(running) = &mut self.running {
            running.queue.take();
        }
    }
}

// Private injection boundary for failure and close-race tests, not a framework.
trait Sink {
    fn publish(&mut self, packet: &Packet) -> (io::Result<()>, u64);
    fn alive(&mut self) -> io::Result<()>;
}

impl Sink for Publisher {
    fn publish(&mut self, packet: &Packet) -> (io::Result<()>, u64) {
        let before = self.totals().dropped;
        let result = self.publish_bytes(packet.at_us, JSON, &packet.bytes[..packet.len]);
        // A root error can be reported after sends occurred. Preserve their drop
        // accounting even when the public publish result is an error.
        (
            result.map(|_| ()),
            self.totals().dropped.saturating_sub(before),
        )
    }

    fn alive(&mut self) -> io::Result<()> {
        Publisher::alive(self)
    }
}

fn consume(mut sink: impl Sink, incoming: Receiver<Packet>, counts: &Counts) {
    let mut alive_at = Instant::now();
    loop {
        match incoming.recv_timeout(ALIVE_EVERY) {
            Ok(packet) => {
                let (result, dropped) = sink.publish(&packet);
                counts
                    .subscriber_drops
                    .fetch_add(dropped, Ordering::Relaxed);
                match result {
                    Ok(()) => {
                        counts.published.fetch_add(1, Ordering::Release);
                    }
                    Err(_) => {
                        counts.publish_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                counts.processed.fetch_add(1, Ordering::Release);
            }
            Err(RecvTimeoutError::Timeout) => {}
            // std's bounded channel drains queued messages before Disconnected.
            // There is no separate empty/closed observation that can lose a tail.
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if alive_at.elapsed() >= ALIVE_EVERY {
            if sink.alive().is_err() {
                counts.alive_errors.fetch_add(1, Ordering::Relaxed);
            }
            alive_at = Instant::now();
        }
    }
}

pub mod keep;

#[cfg(test)]
mod tests;
