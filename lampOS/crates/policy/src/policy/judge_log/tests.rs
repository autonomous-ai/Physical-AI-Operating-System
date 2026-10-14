use super::*;
use serde::{Serializer, ser::SerializeSeq};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    sync::atomic::AtomicUsize,
};

const WAIT: Duration = Duration::from_secs(3);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        for _ in 0..100 {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("lamp-policy-judge-log-{}-{id}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("test directory: {error}"),
            }
        }
        panic!("no fresh test directory");
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn lines(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn accounted(stats: Stats) {
    assert_eq!(
        stats.queued,
        stats.written + stats.failed + stats.abandoned + stats.pending,
        "{stats:?}"
    );
}

#[test]
fn real_records_keep_kinds_days_order_and_private_modes() {
    let temp = Temp::new();
    let dir = temp.0.join("private/log");
    let mut writer = Writer::start(&dir).unwrap();
    let first = json!({"heard":"a line\nand a second: café", "number":1});
    assert!(writer.write(Kind::Models, 0, &first));
    assert!(writer.write(Kind::Models, 1, &json!({"number":2})));
    assert!(writer.write(Kind::Judges, 0, &json!({"local":true})));
    assert!(writer.write(Kind::Outcomes, 0, &json!({"choice":"look_up"})));
    assert!(writer.write(Kind::Models, 86_400_000, &json!({"number":3})));
    let finish = writer.finish(WAIT);
    assert!(finish.joined && finish.drained, "{finish:?}");
    assert_eq!(
        finish.stats,
        Stats {
            queued: 5,
            written: 5,
            ..Stats::default()
        }
    );
    assert_eq!(mode(&dir), 0o700);
    let models = dir.join("models-1970-01-01.jsonl");
    let judges = dir.join("judges-1970-01-01.jsonl");
    let outcomes = dir.join("outcomes-1970-01-01.jsonl");
    let tomorrow = dir.join("models-1970-01-02.jsonl");
    for path in [&models, &judges, &outcomes, &tomorrow] {
        assert_eq!(mode(path), 0o600);
    }
    assert_eq!(lines(&models), [first, json!({"number":2})]);
    assert_eq!(lines(&judges), [json!({"local":true})]);
    assert_eq!(lines(&outcomes), [json!({"choice":"look_up"})]);
    assert_eq!(lines(&tomorrow), [json!({"number":3})]);
}

#[test]
fn invalid_or_exposed_directory_fails_before_a_worker_is_returned() {
    let temp = Temp::new();
    let file = temp.0.join("file");
    fs::write(&file, b"not a directory").unwrap();
    assert!(Writer::start(&file).is_err());
    let exposed = temp.0.join("exposed");
    fs::create_dir(&exposed).unwrap();
    fs::set_permissions(&exposed, fs::Permissions::from_mode(0o750)).unwrap();
    assert!(matches!(
        Writer::start(&exposed),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied
    ));
    assert_eq!(fs::read_dir(&exposed).unwrap().count(), 0);
}

#[test]
fn jsonl_byte_limit_includes_the_newline_and_rejects_before_acceptance() {
    let temp = Temp::new();
    let mut writer = Writer::start(&temp.0).unwrap();
    // Two JSON quotes and Record's newline make this exactly 64 KiB.
    let exact = "x".repeat(MAX_RECORD_BYTES - 3);
    assert!(writer.write(Kind::Judges, 0, &exact));
    assert!(!writer.write(Kind::Judges, 0, &format!("{exact}x")));
    let finish = writer.finish(WAIT);
    assert!(finish.joined && finish.drained, "{finish:?}");
    assert_eq!(finish.stats.queued, 1);
    assert_eq!(finish.stats.written, 1);
    assert_eq!(finish.stats.dropped, 1);
    accounted(finish.stats);
    let path = temp.0.join("judges-1970-01-01.jsonl");
    assert_eq!(fs::metadata(&path).unwrap().len(), MAX_RECORD_BYTES as u64);
    assert_eq!(lines(&path), [Value::String(exact)]);
}

struct Many<'a>(&'a AtomicUsize);

impl Serialize for Many<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(1_000_000))?;
        for _ in 0..1_000_000 {
            self.0.fetch_add(1, Ordering::Relaxed);
            sequence.serialize_element(&0u8)?;
        }
        sequence.end()
    }
}

#[test]
fn streaming_serialization_stops_at_the_bound_without_building_the_whole_record() {
    let temp = Temp::new();
    let mut writer = Writer::start(&temp.0).unwrap();
    let visited = AtomicUsize::new(0);
    assert!(!writer.write(Kind::Models, 0, &Many(&visited)));
    assert!(visited.load(Ordering::Relaxed) <= MAX_RECORD_BYTES);
    let finish = writer.finish(WAIT);
    assert!(finish.joined && finish.drained);
    assert_eq!(finish.stats.queued, 0);
    assert_eq!(finish.stats.dropped, 1);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

struct SecretFailure;

impl Serialize for SecretFailure {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("private utterance in an error"))
    }
}

#[test]
fn serialization_errors_are_rejected_and_stats_contain_no_error_text() {
    let temp = Temp::new();
    let mut writer = Writer::start(&temp.0).unwrap();
    assert!(!writer.write(Kind::Models, 0, &SecretFailure));
    let finish = writer.finish(WAIT);
    assert_eq!(finish.stats.dropped, 1);
    assert_eq!(finish.stats.failed, 0);
    assert_eq!(finish.stats.queued, 0);
    assert!(!serde_json::to_string(&finish).unwrap().contains("private"));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn actual_append_failure_is_counted_and_does_not_block_other_record_kinds() {
    let temp = Temp::new();
    fs::create_dir(temp.0.join("models-1970-01-01.jsonl")).unwrap();
    let mut writer = Writer::start(&temp.0).unwrap();
    assert!(writer.write(Kind::Models, 0, &json!({"private":"heard"})));
    assert!(writer.write(Kind::Judges, 0, &json!({"local":true})));
    assert!(writer.write(Kind::Outcomes, 0, &json!({"choice":"look_up"})));
    let finish = writer.finish(WAIT);
    assert!(finish.joined && !finish.drained, "{finish:?}");
    assert_eq!(finish.stats.queued, 3);
    assert_eq!(finish.stats.written, 2);
    assert_eq!(finish.stats.failed, 1);
    assert_eq!(finish.stats.dropped, 0);
    assert_eq!(finish.stats.pending, 0);
    accounted(finish.stats);
    assert_eq!(
        lines(&temp.0.join("judges-1970-01-01.jsonl")),
        [json!({"local":true})]
    );
    assert!(!serde_json::to_string(&finish).unwrap().contains("heard"));
}

struct Stalled {
    entered: SyncSender<()>,
    release: Receiver<()>,
    exited: SyncSender<()>,
    calls: Arc<AtomicUsize>,
}

impl Sink for Stalled {
    fn append(&mut self, _: &Entry) -> io::Result<()> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let _ = self.entered.try_send(());
        self.release
            .recv_timeout(WAIT)
            .map_err(|_| io::Error::other("injected sink did not receive its release"))
    }
}

impl Drop for Stalled {
    fn drop(&mut self) {
        let _ = self.exited.try_send(());
    }
}

struct Release(Option<SyncSender<()>>);

impl Drop for Release {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.try_send(());
        }
    }
}

fn stalled() -> (
    Writer,
    Receiver<()>,
    Release,
    Receiver<()>,
    Arc<AtomicUsize>,
) {
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let (exited, exit) = mpsc::sync_channel(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let writer = Writer::spawn(Stalled {
        entered,
        release: released,
        exited,
        calls: Arc::clone(&calls),
    })
    .unwrap();
    (writer, entry, Release(Some(release)), exit, calls)
}

#[test]
fn a_full_stalled_queue_is_bounded_and_acceptance_never_claims_a_write() {
    let (mut writer, entered, release, exited, calls) = stalled();
    assert!(writer.write(Kind::Models, 0, &json!({"first":true})));
    entered.recv_timeout(WAIT).unwrap();
    for index in 0..CAPACITY {
        assert!(writer.write(Kind::Judges, 0, &index));
    }
    assert!(!writer.write(Kind::Outcomes, 0, &"full"));
    let stats = writer.stats();
    assert_eq!(stats.queued, CAPACITY as u64 + 1);
    assert_eq!(stats.written, 0);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.dropped, 1);
    assert_eq!(stats.pending, CAPACITY as u64 + 1);
    assert!(stats.alive);
    accounted(stats);
    let shared = Arc::clone(&writer.shared);
    let finish = writer.finish(Duration::ZERO);
    assert!(!finish.joined && !finish.drained, "{finish:?}");
    assert_eq!(finish.stats.pending, CAPACITY as u64 + 1);
    drop(release);
    exited.recv_timeout(WAIT).unwrap();
    let after = shared.stats(stats.queued, stats.dropped);
    assert_eq!(after.written, 1);
    assert_eq!(after.abandoned, CAPACITY as u64);
    assert_eq!(after.failed, 0);
    assert_eq!(after.pending, 0);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    accounted(after);
}

#[test]
fn finish_returns_without_waiting_for_a_stalled_sink_and_never_acknowledges_it() {
    let (mut writer, entered, release, exited, _) = stalled();
    assert!(writer.write(Kind::Models, 0, &json!({"first":true})));
    entered.recv_timeout(WAIT).unwrap();
    let (report, result) = mpsc::sync_channel(1);
    let finishing = thread::spawn(move || {
        let finish = writer.finish(Duration::from_millis(10));
        let _ = report.try_send(finish);
    });
    // A handshake, not a performance measurement: completion must arrive
    // while the sink is still explicitly held, with generous scheduling slack.
    let finish = result.recv_timeout(Duration::from_secs(1));
    drop(release);
    finishing.join().unwrap();
    let finish = finish.expect("finish waited for the stalled sink");
    assert!(!finish.joined && !finish.drained, "{finish:?}");
    assert_eq!(finish.stats.queued, 1);
    assert_eq!(finish.stats.written, 0);
    assert_eq!(finish.stats.pending, 1);
    exited.recv_timeout(WAIT).unwrap();
}

#[test]
fn dropping_without_finish_cancels_queued_work_without_joining() {
    let (mut writer, entered, release, exited, calls) = stalled();
    assert!(writer.write(Kind::Models, 0, &1));
    entered.recv_timeout(WAIT).unwrap();
    assert!(writer.write(Kind::Judges, 0, &2));
    let shared = Arc::clone(&writer.shared);
    drop(writer);
    // The held sink is still alive after the owner returned from Drop.
    assert!(shared.alive.load(Ordering::Acquire));
    drop(release);
    exited.recv_timeout(WAIT).unwrap();
    let stats = shared.stats(2, 0);
    assert_eq!(stats.written, 1);
    assert_eq!(stats.abandoned, 1);
    assert_eq!(stats.pending, 0);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    accounted(stats);
}
