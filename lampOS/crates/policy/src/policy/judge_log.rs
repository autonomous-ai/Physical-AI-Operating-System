//! Bounded, best-effort private judge records. Acceptance is only a queue
//! receipt; `written` means an append returned successfully, never disk sync.
use crate::record::{Record, private_dir};
use serde::Serialize;
use std::{
    io::{self, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CAPACITY: usize = 64;
/// Includes the newline added by `Record`.
const MAX_RECORD_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Models,
    Judges,
    Outcomes,
}

/// Cumulative counts, plus a conservative snapshot of unfinished work.
/// `queued = written + failed + abandoned + pending` in every snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(super) struct Stats {
    /// Accepted into the queue, including any record already in flight.
    pub queued: u64,
    /// `Record::write` returned success. No power-loss durability is claimed.
    pub written: u64,
    /// Accepted records whose decoding or append returned an error.
    pub failed: u64,
    /// Rejected before acceptance: oversized, unserializable, full or closed.
    pub dropped: u64,
    /// Accepted records discarded on cancellation or an unconfirmed worker exit.
    pub abandoned: u64,
    /// Accepted records with no known write/failure/abandonment result yet.
    pub pending: u64,
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct Finish {
    pub stats: Stats,
    /// Every accepted record completed a successful append. Pre-acceptance
    /// drops are separate; this is neither a flush nor a durability receipt.
    pub drained: bool,
    /// The one worker has been joined, including an abnormal exit.
    pub joined: bool,
}

#[derive(Default)]
struct Shared {
    written: AtomicU64,
    failed: AtomicU64,
    abandoned: AtomicU64,
    cancelled: AtomicBool,
    alive: AtomicBool,
}

impl Shared {
    fn stats(&self, queued: u64, dropped: u64) -> Stats {
        // Reading a completed lifetime first makes its final counters visible.
        // During work, separate counter reads can only undercount completion.
        let alive = self.alive.load(Ordering::Acquire);
        let written = self.written.load(Ordering::Acquire);
        let failed = self.failed.load(Ordering::Acquire);
        let abandoned = self.abandoned.load(Ordering::Acquire);
        Stats {
            queued,
            written,
            failed,
            dropped,
            abandoned,
            pending: queued.saturating_sub(written + failed + abandoned),
            alive,
        }
    }
}

struct Entry {
    kind: Kind,
    at_unix_ms: u64,
    json: Vec<u8>,
}

/// Serialize directly into bounded storage. `to_vec` followed by a length
/// check would first allocate an arbitrarily large record on the caller.
struct BoundedJson(Vec<u8>);

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > (MAX_RECORD_BYTES - 1).saturating_sub(self.0.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "judge record exceeds its byte limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Entry {
    fn encode<T: Serialize>(kind: Kind, at_unix_ms: u64, line: &T) -> io::Result<Self> {
        let mut bytes = BoundedJson(Vec::with_capacity(MAX_RECORD_BYTES));
        serde_json::to_writer(&mut bytes, line).map_err(io::Error::other)?;
        Ok(Self {
            kind,
            at_unix_ms,
            json: bytes.0,
        })
    }
}

trait Sink: Send + 'static {
    fn append(&mut self, entry: &Entry) -> io::Result<()>;
}

struct Files {
    models: Record,
    judges: Record,
    outcomes: Record,
}

impl Files {
    fn new(dir: &Path) -> Self {
        Self {
            models: Record::of("models", dir),
            judges: Record::of("judges", dir),
            outcomes: Record::of("outcomes", dir),
        }
    }
}

impl Sink for Files {
    fn append(&mut self, entry: &Entry) -> io::Result<()> {
        // Parsing and Record's serialization occur only on the worker. The
        // ordinary JSON recursion limit also bounds decoding of this buffer.
        let value: serde_json::Value = serde_json::from_slice(&entry.json)?;
        let record = match entry.kind {
            Kind::Models => &mut self.models,
            Kind::Judges => &mut self.judges,
            Kind::Outcomes => &mut self.outcomes,
        };
        record.write(entry.at_unix_ms, &value)
    }
}

pub(super) struct Writer {
    sender: Option<SyncSender<Entry>>,
    worker: Option<JoinHandle<()>>,
    done: Receiver<()>,
    shared: Arc<Shared>,
    queued: u64,
    dropped: u64,
}

impl Writer {
    /// Startup may create/validate the private directory. After it returns,
    /// only the one worker performs filesystem operations.
    pub fn start(dir: &Path) -> io::Result<Self> {
        private_dir(dir)?;
        Self::spawn(Files::new(dir))
    }

    fn spawn(sink: impl Sink) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(CAPACITY);
        let (finished, done) = mpsc::sync_channel(1);
        let shared = Arc::new(Shared {
            alive: AtomicBool::new(true),
            ..Shared::default()
        });
        let state = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("lamp-policy-judge-log".into())
            .spawn(move || {
                // Remains alive through the sink's destruction, even if a
                // filesystem close is held up, and clears on unwinding too.
                let _lifetime = Lifetime(&state);
                write_loop(receiver, sink, &state);
                let _ = finished.try_send(());
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            done,
            shared,
            queued: 0,
            dropped: 0,
        })
    }

    /// A nonblocking queue attempt, with bounded serialization and no file I/O.
    /// Success is acceptance only: consult `stats`/`finish` for write results.
    pub fn write<T: Serialize>(&mut self, kind: Kind, at_unix_ms: u64, line: &T) -> bool {
        if !self.shared.alive.load(Ordering::Acquire) || self.sender.is_none() {
            self.dropped += 1;
            return false;
        }
        let entry = match Entry::encode(kind, at_unix_ms, line) {
            Ok(entry) => entry,
            Err(_) => {
                self.dropped += 1;
                return false;
            }
        };
        match self.sender.as_ref().expect("open sender").try_send(entry) {
            Ok(()) => {
                self.queued += 1;
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.dropped += 1;
                false
            }
        }
    }

    pub fn stats(&self) -> Stats {
        self.shared.stats(self.queued, self.dropped)
    }

    /// Close admission and give the worker at most this waiting grace. A
    /// stalled syscall cannot be cancelled; after the grace the worker is
    /// detached and discards further queued entries once its current call ends.
    /// OS scheduling is outside this userspace waiting bound.
    pub fn finish(mut self, within: Duration) -> Finish {
        self.sender.take();
        let started = Instant::now();
        let joined = loop {
            if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                let worker = self.worker.take().expect("one worker");
                let _ = worker.join();
                // A panic may have dropped queued entries before reporting
                // them. Nothing can still write after this confirmed join.
                let pending = self.stats().pending;
                self.shared.abandoned.fetch_add(pending, Ordering::Release);
                break true;
            }
            let remaining = within.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                self.shared.cancelled.store(true, Ordering::Release);
                break false;
            }
            match self.done.recv_timeout(remaining) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // The acknowledgement follows all sink work, but the
                    // closure can still be returning. Never join until the
                    // handle itself confirms completion.
                    let remaining = within.saturating_sub(started.elapsed());
                    if !remaining.is_zero()
                        && self.worker.as_ref().is_some_and(|w| !w.is_finished())
                    {
                        thread::park_timeout(remaining.min(Duration::from_millis(1)));
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        };
        let stats = self.stats();
        Finish {
            drained: stats.pending == 0 && stats.failed == 0 && stats.abandoned == 0,
            stats,
            joined,
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
        self.sender.take();
        // Dropping JoinHandle detaches. No implicit join or replacement worker.
        self.worker.take();
    }
}

struct Lifetime<'a>(&'a Shared);

impl Drop for Lifetime<'_> {
    fn drop(&mut self) {
        self.0.alive.store(false, Ordering::Release);
    }
}

fn write_loop(receiver: Receiver<Entry>, mut sink: impl Sink, shared: &Shared) {
    while let Ok(entry) = receiver.recv() {
        if shared.cancelled.load(Ordering::Acquire) {
            shared.abandoned.fetch_add(1, Ordering::Release);
        } else if sink.append(&entry).is_ok() {
            shared.written.fetch_add(1, Ordering::Release);
        } else {
            // Error text may contain private content or paths. Only this count
            // crosses back to the policy; Record repairs partial lines itself.
            shared.failed.fetch_add(1, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests;
