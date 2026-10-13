//! Best-effort metadata storage. No authority or conversation state lives here.
//! Only the writer thread performs filesystem/stderr I/O after construction.
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, Thread},
    time::Duration,
};

pub(crate) const MAX_EVENT_BYTES: usize = 32 * 1024;
const QUEUE_CAPACITY: usize = 64;
const SEGMENTS: usize = 4;
const SEGMENT_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const FINISH_WAIT: Duration = Duration::from_millis(500);

struct Packet {
    bytes: [u8; MAX_EVENT_BYTES],
    len: usize,
}
impl Packet {
    fn encode(event: &Value) -> io::Result<Self> {
        let mut packet = Self {
            bytes: [0; MAX_EVENT_BYTES],
            len: 0,
        };
        serde_json::to_writer(&mut packet, event).map_err(io::Error::other)?;
        packet.write_all(b"\n")?;
        Ok(packet)
    }
}
impl Write for Packet {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .len
            .checked_add(bytes.len())
            .filter(|end| *end <= MAX_EVENT_BYTES)
            .ok_or_else(|| io::Error::other("service metadata event exceeds bound"))?;
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn event_fits(event: &Value) -> bool {
    Packet::encode(event).is_ok()
}

#[derive(Default)]
struct Health {
    closed: AtomicBool,
    failed: AtomicBool,
    dropped: AtomicU64,
    oversized: AtomicU64,
    written: AtomicU64,
    accepted: AtomicU64,
    rotations: AtomicU64,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Report {
    pub acknowledged: bool,
    pub finish_attempted: bool,
    pub degraded: bool,
    pub writer_failed: bool,
    pub initialization_failed: bool,
    pub dropped_events: u64,
    pub oversized_events: u64,
    pub written_events: u64,
    pub unconfirmed_events: u64,
    pub rotations: u64,
}
impl Health {
    fn report(&self, acknowledged: bool, finish_attempted: bool) -> Report {
        let explicit_dropped = self.dropped.load(Ordering::Acquire);
        let written = self.written.load(Ordering::Acquire);
        let unconfirmed = self
            .accepted
            .load(Ordering::Acquire)
            .saturating_sub(written);
        let failed = self.failed.load(Ordering::Acquire);
        let dropped = explicit_dropped.saturating_add(if failed { unconfirmed } else { 0 });
        Report {
            acknowledged,
            finish_attempted,
            degraded: failed || dropped > 0 || (finish_attempted && !acknowledged),
            writer_failed: failed,
            initialization_failed: false,
            dropped_events: dropped,
            oversized_events: self.oversized.load(Ordering::Acquire),
            written_events: written,
            unconfirmed_events: unconfirmed,
            rotations: self.rotations.load(Ordering::Acquire),
        }
    }
}

pub(crate) struct ServiceTrace {
    queue: Option<Producer<Packet>>,
    health: Arc<Health>,
    writer: Thread,
    done: mpsc::Receiver<()>,
}
impl ServiceTrace {
    /// Called before any worker is started. Failure is returned as logging
    /// health, not permission to skip or relax the normal hardware gates.
    pub(crate) fn start(output: &Path) -> io::Result<Self> {
        let store = Files::open(output)?;
        Self::spawn(store)
    }
    fn spawn(store: impl Store + Send + 'static) -> io::Result<Self> {
        let (producer, consumer) = RingBuffer::new(QUEUE_CAPACITY);
        let health = Arc::new(Health::default());
        let shared = Arc::clone(&health);
        let (tx, done) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("lamp-service-trace".into())
            .spawn(move || {
                writer_loop(consumer, store, &shared);
                let _ = tx.try_send(());
            })?;
        let writer = handle.thread().clone();
        // Deliberately detach: filesystem calls cannot be preempted by Rust.
        drop(handle);
        Ok(Self {
            queue: Some(producer),
            health,
            writer,
            done,
        })
    }
    pub(crate) fn lost(&self, oversized: bool) {
        self.health.dropped.fetch_add(1, Ordering::Relaxed);
        if oversized {
            self.health.oversized.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(crate) fn submit(&mut self, event: &Value) {
        if self.health.failed.load(Ordering::Acquire) {
            self.lost(false);
            return;
        }
        let packet = match Packet::encode(event) {
            Ok(packet) => packet,
            Err(_) => {
                self.lost(true);
                return;
            }
        };
        if self
            .queue
            .as_mut()
            .is_none_or(|queue| queue.push(packet).is_err())
        {
            self.lost(false);
        } else {
            self.health.accepted.fetch_add(1, Ordering::Release);
            self.writer.unpark();
        }
    }
    #[cfg(test)]
    pub(crate) fn health(&self) -> Report {
        self.health.report(false, false)
    }
    pub(crate) fn finish(&mut self, wait: Duration) -> Report {
        self.queue.take();
        self.health.closed.store(true, Ordering::Release);
        self.writer.unpark();
        let acknowledged = self.done.recv_timeout(wait.min(FINISH_WAIT)).is_ok();
        self.health.report(acknowledged, true)
    }
}
impl Drop for ServiceTrace {
    fn drop(&mut self) {
        self.queue.take();
        self.health.closed.store(true, Ordering::Release);
        self.writer.unpark();
    }
}

trait Store {
    fn event(&mut self, bytes: &[u8], health: &Health) -> io::Result<()>;
    fn finish(&mut self, health: &Health) -> io::Result<()>;
}
fn writer_loop(mut queue: Consumer<Packet>, mut store: impl Store, health: &Health) {
    let result = (|| -> io::Result<()> {
        let mut warned = false;
        loop {
            let packet = match queue.pop() {
                Ok(packet) => packet,
                Err(_) if health.closed.load(Ordering::Acquire) => {
                    // The original empty observation can precede the producer's
                    // final publication. EOF requires a fresh post-close pop.
                    match queue.pop() {
                        Ok(packet) => packet,
                        Err(_) => break,
                    }
                }
                Err(_) => {
                    thread::park_timeout(Duration::from_millis(10));
                    continue;
                }
            };
            if !warned && health.dropped.load(Ordering::Acquire) > 0 {
                warned = true;
                eprintln!(
                    "lamp-voice serve: metadata events dropped; conversation authority is unaffected"
                );
            }
            store.event(&packet.bytes[..packet.len], health)?;
            health.written.fetch_add(1, Ordering::Release);
        }
        store.finish(health)
    })();
    if result.is_err() {
        health.failed.store(true, Ordering::Release);
        // At most one bounded line; stderr backpressure stalls only this worker.
        eprintln!("lamp-voice serve: metadata writer failed; conversation authority is unaffected");
    }
}

struct Files {
    segments: Vec<File>,
    health_file: File,
    slot: usize,
    generation: u64,
    bytes: u64,
    events: u64,
    limit: u64,
}
impl Files {
    fn open(output: &Path) -> io::Result<Self> {
        let path = output.join("metadata");
        fs::create_dir(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        // All descriptors are opened exclusively before activation and retained.
        // Rotation truncates those descriptors; it never follows a replaced path.
        let mut segments = Vec::with_capacity(SEGMENTS);
        for slot in 0..SEGMENTS {
            segments.push(private_file(&path.join(format!("events-{slot}.jsonl")))?);
        }
        let health_file = private_file(&path.join("health.json"))?;
        Ok(Self {
            segments,
            health_file,
            slot: 0,
            generation: 0,
            bytes: 0,
            events: 0,
            limit: SEGMENT_BYTES,
        })
    }
    fn header(&mut self) -> io::Result<()> {
        let file = &mut self.segments[self.slot];
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        let header = format!(
            "{{\"kind\":\"service_segment\",\"generation\":{},\"first_writer_event\":{},\"evidence\":\"rolling_best_effort_not_benchmark\"}}\n",
            self.generation, self.events
        );
        file.write_all(header.as_bytes())?;
        self.bytes = header.len() as u64;
        Ok(())
    }
    fn health(&mut self, health: &Health, finished: bool) -> io::Result<()> {
        self.health_file.seek(SeekFrom::Start(0))?;
        self.health_file.set_len(0)?;
        serde_json::to_writer(&mut self.health_file, &serde_json::json!({
            "writer_finished":finished,"coordinator_finish_ack_required":true,
            "health":health.report(false, false),"latest_segment":self.slot,"generation":self.generation,
            "maximum_metadata_bytes":SEGMENT_BYTES * SEGMENTS as u64,
            "history_is_rolling":true,"complete_evidence":false
        })).map_err(io::Error::other)?;
        Ok(())
    }
}
fn private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}
impl Store for Files {
    fn event(&mut self, bytes: &[u8], health: &Health) -> io::Result<()> {
        if self.bytes == 0 {
            self.header()?;
        }
        if self.bytes + bytes.len() as u64 > self.limit {
            self.slot = (self.slot + 1) % self.segments.len();
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or_else(|| io::Error::other("trace generation exhausted"))?;
            self.header()?;
            health.rotations.fetch_add(1, Ordering::Release);
        }
        self.segments[self.slot].write_all(bytes)?;
        self.bytes += bytes.len() as u64;
        self.events = self.events.saturating_add(1);
        if self.events % 64 == 1 {
            self.health(health, false)?;
        }
        Ok(())
    }
    fn finish(&mut self, health: &Health) -> io::Result<()> {
        for file in &self.segments {
            file.sync_all()?;
        }
        self.health(health, true)?;
        self.health_file.sync_all()
    }
}

#[cfg(test)]
impl ServiceTrace {
    pub(crate) fn stalled() -> (Self, mpsc::Receiver<()>, mpsc::Sender<()>) {
        struct Stall {
            entered: mpsc::SyncSender<()>,
            release: mpsc::Receiver<()>,
            first: bool,
        }
        impl Store for Stall {
            fn event(&mut self, _: &[u8], _: &Health) -> io::Result<()> {
                if self.first {
                    self.first = false;
                    self.entered.send(()).unwrap();
                    self.release.recv().unwrap();
                }
                Ok(())
            }
            fn finish(&mut self, _: &Health) -> io::Result<()> {
                Ok(())
            }
        }
        let (tx, entered) = mpsc::sync_channel(1);
        let (release, rx) = mpsc::channel();
        (
            Self::spawn(Stall {
                entered: tx,
                release: rx,
                first: true,
            })
            .unwrap(),
            entered,
            release,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Memory {
        events: Arc<Mutex<Vec<Vec<u8>>>>,
        fail: bool,
    }
    impl Store for Memory {
        fn event(&mut self, bytes: &[u8], _: &Health) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("injected full disk"));
            }
            self.events.lock().unwrap().push(bytes.to_vec());
            Ok(())
        }
        fn finish(&mut self, _: &Health) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn closure_flushes_final_accepted_event_without_inventing_complete_evidence() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut writer = ServiceTrace::spawn(Memory {
            events: events.clone(),
            fail: false,
        })
        .unwrap();
        writer.submit(&serde_json::json!({"kind":"final_metadata"}));
        let report = writer.finish(FINISH_WAIT);
        assert!(report.acknowledged && !report.degraded);
        assert_eq!(report.written_events, 1);
        assert_eq!(events.lock().unwrap().len(), 1);
    }
    #[test]
    fn disk_failure_latches_health_without_an_audio_result_error() {
        let mut writer = ServiceTrace::spawn(Memory {
            events: Arc::default(),
            fail: true,
        })
        .unwrap();
        writer.submit(&serde_json::json!({"kind":"event"}));
        let report = writer.finish(FINISH_WAIT);
        assert!(report.acknowledged && report.degraded && report.writer_failed);
        writer.submit(&serde_json::json!({"kind":"after_failure"}));
        assert_eq!(writer.health().dropped_events, 2);
    }
    #[test]
    fn stalled_writer_has_fixed_capacity_and_bounded_finish_without_joining() {
        let (mut writer, entered, release) = ServiceTrace::stalled();
        writer.submit(&serde_json::json!({"kind":"first"}));
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        for n in 0..QUEUE_CAPACITY + 11 {
            writer.submit(&serde_json::json!({"n":n}));
        }
        assert_eq!(writer.health().dropped_events, 11);
        // A zero control-path budget is deterministic; no widened timeout/sleep.
        let report = writer.finish(Duration::ZERO);
        assert!(!report.acknowledged && report.degraded);
        release.send(()).unwrap();
        writer.done.recv_timeout(Duration::from_secs(2)).unwrap();
        // A late writer exit cannot rewrite the already returned timed-out report.
        assert!(!report.acknowledged);
    }
    #[test]
    fn oversized_event_is_dropped_not_truncated_into_a_misleading_record() {
        let mut writer = ServiceTrace::spawn(Memory {
            events: Arc::default(),
            fail: false,
        })
        .unwrap();
        writer.submit(&serde_json::json!({"text":"x".repeat(MAX_EVENT_BYTES)}));
        let report = writer.finish(FINISH_WAIT);
        assert_eq!(report.oversized_events, 1);
        assert_eq!(report.dropped_events, 1);
        assert_eq!(report.written_events, 0);
    }
    #[test]
    fn rotation_uses_only_pinned_files_and_bounded_disk_space() {
        let dir = crate::process::SessionDirectory::create().unwrap();
        let mut files = Files::open(&dir.path).unwrap();
        files.limit = 512;
        let health = Health::default();
        for n in 0..100 {
            let packet =
                Packet::encode(&serde_json::json!({"n":n,"text":"x".repeat(100)})).unwrap();
            files.event(&packet.bytes[..packet.len], &health).unwrap();
        }
        files.finish(&health).unwrap();
        assert!(health.rotations.load(Ordering::Acquire) > SEGMENTS as u64);
        assert!(Files::open(&dir.path).is_err());
        let paths: Vec<_> = fs::read_dir(dir.path.join("metadata")).unwrap().collect();
        assert_eq!(paths.len(), SEGMENTS + 1);
        for slot in 0..SEGMENTS {
            let path = dir.path.join(format!("metadata/events-{slot}.jsonl"));
            assert!(fs::metadata(&path).unwrap().len() <= 512);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            for line in fs::read_to_string(path).unwrap().lines() {
                serde_json::from_str::<Value>(line).unwrap();
            }
        }
    }
}
