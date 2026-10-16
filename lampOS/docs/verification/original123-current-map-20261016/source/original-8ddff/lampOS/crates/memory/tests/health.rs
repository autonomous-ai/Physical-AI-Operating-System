//! The real binary's optional reports, with synthetic records and temporary
//! stores. These are development-host integration checks, not lamp timings.
use lamp_bus::{Bus, Subscriber};
use lamp_memory::{days::DATABASE, health::REPORT_EVERY};
use lamp_msgs::{Diagnostic, Level};
use serde_json::{Value, json};
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BINARY: &str = env!("CARGO_BIN_EXE_lamp-memory");
const WAIT: Duration = Duration::from_secs(8);
const REQUEST_WAIT: Duration = Duration::from_secs(3);
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        // macOS's default temporary directory can exceed Unix socket limits
        // once the bus adds a topic and subscriber name.
        let path = PathBuf::from("/tmp").join(format!(
            "lmh-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn subscribe(&self) -> Subscriber {
        Bus::new(self.path("b"))
            .unwrap()
            .subscribe_typed(Diagnostic::TOPIC, "t", Diagnostic::TYPE)
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Every assertion retains this guard, including startup-failure assertions.
/// Logs go to a file so an unread pipe cannot block the child.
struct Process {
    child: Child,
    log: PathBuf,
}

impl Process {
    fn start(scratch: &Scratch, days: bool, bus: bool, env_bus: bool) -> Self {
        let log = scratch.path("log");
        let mut command = Command::new(BINARY);
        command
            .arg("--dir")
            .arg(scratch.path("store"))
            .arg("--socket")
            .arg(scratch.path("m.sock"))
            .env_remove("LAMP_BUS_ROOT")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap());
        if days {
            command.arg("--memory-days");
        }
        if bus {
            command.arg("--bus-root").arg(scratch.path("b"));
        }
        if env_bus {
            command.env("LAMP_BUS_ROOT", scratch.path("b"));
        }
        Self {
            child: command.spawn().unwrap(),
            log,
        }
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn ready(&mut self, scratch: &Scratch) {
        let until = Instant::now() + WAIT;
        let mut delay = Duration::from_millis(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("memory exited before readiness: {status}; {}", self.log());
            }
            if fs::read(scratch.path("ready")).is_ok_and(|bytes| bytes == b"ready\n") {
                return;
            }
            assert!(
                Instant::now() < until,
                "memory never became ready; {}",
                self.log()
            );
            thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(50));
        }
    }

    fn exited(&mut self) -> ExitStatus {
        let until = Instant::now() + WAIT;
        let mut delay = Duration::from_millis(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < until,
                "memory did not exit; {}",
                self.log()
            );
            thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(50));
        }
    }

    fn logged(&mut self, expected: &str) {
        let until = Instant::now() + WAIT;
        let mut delay = Duration::from_millis(5);
        loop {
            let log = self.log();
            if log.contains(expected) {
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "memory exited before logging {expected}; {log}"
            );
            assert!(Instant::now() < until, "never logged {expected}; {log}");
            thread::sleep(delay);
            delay = (delay * 2).min(Duration::from_millis(50));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn ask_raw(scratch: &Scratch, request: &str) -> Value {
    let stream = UnixStream::connect(scratch.path("m.sock")).unwrap();
    stream.set_read_timeout(Some(REQUEST_WAIT)).unwrap();
    stream.set_write_timeout(Some(REQUEST_WAIT)).unwrap();
    let mut reader = BufReader::new(stream);
    reader.get_mut().write_all(request.as_bytes()).unwrap();
    reader.get_mut().write_all(b"\n").unwrap();
    let mut line = String::new();
    assert!(
        reader.read_line(&mut line).unwrap() > 0,
        "no answer to {request}"
    );
    serde_json::from_str(&line).unwrap()
}

fn ask(scratch: &Scratch, request: Value) -> Value {
    ask_raw(scratch, &request.to_string())
}

fn ok(scratch: &Scratch, request: Value) -> Value {
    let response = ask(scratch, request.clone());
    assert_eq!(response["ok"], true, "{request}: {response}");
    response
}

fn value<'a>(report: &'a Diagnostic, key: &str) -> &'a str {
    report
        .values
        .iter()
        .find(|entry| entry.key == key)
        .unwrap_or_else(|| panic!("missing {key} in {report:?}"))
        .value
        .as_str()
}

fn number(report: &Diagnostic, key: &str) -> u64 {
    value(report, key).parse().unwrap()
}

fn report_when(reports: &mut Subscriber, accept: impl Fn(&Diagnostic) -> bool) -> Diagnostic {
    let until = Instant::now() + WAIT;
    let mut last = None;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "no matching memory report; last={last:?}");
        if let Some(message) = reports.recv(left.min(Duration::from_millis(250))).unwrap() {
            assert_eq!(message.header.type_name.as_deref(), Some(Diagnostic::TYPE));
            let report: Diagnostic = message.parse().unwrap();
            assert_eq!(report.name, "memory");
            if accept(&report) {
                return report;
            }
            last = Some(report);
        }
    }
}

fn retained(reports: &mut Subscriber, exchanges: u64, kept: u64, answered: u64) -> Diagnostic {
    report_when(reports, |report| {
        report.level == Level::Ok
            && value(report, "exchanges") == exchanges.to_string()
            && value(report, "kept") == kept.to_string()
            && number(report, "answered") >= answered
    })
}

fn counts_through_real_requests(days: bool) {
    let scratch = Scratch::new();
    let mut reports = scratch.subscribe();
    let mut process = Process::start(&scratch, days, true, false);
    process.ready(&scratch);
    let initial = retained(&mut reports, 0, 0, 0);
    assert_eq!(value(&initial, "slowest_answer_ms"), "none");
    assert_eq!(number(&initial, "answered"), 0);
    // A second unchanged report proves the publisher continues while the
    // socket owner waits for a client. It does not assert scheduler precision.
    retained(&mut reports, 0, 0, 0);
    let at = now();
    for person in ["everyone", "other"] {
        ok(
            &scratch,
            json!({"op":"append","person":person,"at_unix_ms":at,"heard":"synthetic hello"}),
        );
        ok(
            &scratch,
            json!({"op":"remember","person":person,"at_unix_ms":at,"text":"synthetic coffee"}),
        );
    }
    let added = retained(&mut reports, 2, 2, 4);
    assert!(value(&added, "slowest_answer_ms").parse::<f64>().unwrap() > 0.0);
    assert_eq!(number(&added, "note_budget_ms"), 300);
    assert_eq!(number(&added, "call_budget_ms"), 1500);
    assert_eq!(number(&added, "recent_answers"), 4);
    ok(&scratch, json!({"op":"forget_last","kind":"exchange"}));
    retained(&mut reports, 1, 2, 5);
    ok(&scratch, json!({"op":"forget_all","person":"other"}));
    retained(&mut reports, 0, 1, 6);
    ok(&scratch, json!({"op":"forget_all","person":null}));
    retained(&mut reports, 0, 0, 7);

    for (request, code) in [
        ("not JSON".to_owned(), "bad_json"),
        (json!({"op":"unknown"}).to_string(), "unknown_op"),
        (
            json!({"op":"append","at_unix_ms":at,"heard":" "}).to_string(),
            "empty",
        ),
        (
            json!({"op":"remember","at_unix_ms":at,"text":" "}).to_string(),
            "empty",
        ),
    ] {
        let response = ask_raw(&scratch, &request);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"], code);
    }
    let refused = retained(&mut reports, 0, 0, 11);
    assert!(refused.message.is_empty());
    assert_eq!(number(&refused, "answered"), 11);
}

#[test]
fn legacy_reports_retained_counts_and_stays_well_after_caller_refusals() {
    counts_through_real_requests(false);
}

#[test]
fn sqlite_reports_retained_counts_and_stays_well_after_caller_refusals() {
    counts_through_real_requests(true);
}

#[test]
fn a_failed_legacy_write_stays_warn_through_reads_until_a_write_succeeds() {
    let scratch = Scratch::new();
    let mut reports = scratch.subscribe();
    let mut process = Process::start(&scratch, false, true, false);
    process.ready(&scratch);
    let at = now();
    ok(
        &scratch,
        json!({"op":"append","at_unix_ms":at,"heard":"synthetic hello"}),
    );
    ok(
        &scratch,
        json!({"op":"remember","at_unix_ms":at,"text":"synthetic coffee"}),
    );
    retained(&mut reports, 1, 1, 2);

    let kept = scratch.path("store/everyone/kept.jsonl");
    let saved = scratch.path("saved-kept");
    let mut answered = 2;
    for (recovery, old_exchanges, new_exchanges, new_kept) in [
        (
            json!({"op":"append","at_unix_ms":at,"heard":"synthetic recovery"}),
            1,
            2,
            1,
        ),
        (
            json!({"op":"remember","at_unix_ms":at,"text":"synthetic recovery"}),
            2,
            2,
            2,
        ),
    ] {
        fs::rename(&kept, &saved).unwrap();
        fs::create_dir(&kept).unwrap();
        let refused = ask(
            &scratch,
            json!({"op":"remember","at_unix_ms":at,"text":"synthetic failure"}),
        );
        answered += 1;
        assert_eq!(refused["error"], "storage");
        report_when(&mut reports, |report| {
            report.level == Level::Warn
                && report.message.contains("storage failed")
                && number(report, "answered") >= answered
        });

        fs::remove_dir(&kept).unwrap();
        fs::rename(&saved, &kept).unwrap();
        let read = ok(&scratch, json!({"op":"kept"}));
        answered += 1;
        assert_eq!(read["entries"].as_array().unwrap().len(), 1);
        let warning = report_when(&mut reports, |report| {
            report.level == Level::Warn
                && report.message.contains("awaiting a successful write")
                && number(report, "answered") >= answered
                && value(report, "kept") == "1"
        });
        assert_eq!(number(&warning, "exchanges"), old_exchanges);
        ok(&scratch, recovery);
        answered += 1;
        retained(&mut reports, new_exchanges, new_kept, answered);
    }
}

#[test]
fn the_environment_alone_does_not_enable_reporting_or_create_bus_data() {
    for days in [false, true] {
        let scratch = Scratch::new();
        let mut process = Process::start(&scratch, days, false, true);
        process.ready(&scratch);
        ok(
            &scratch,
            json!({"op":"remember","at_unix_ms":now(),"text":"synthetic offline note"}),
        );
        let _ = ok(&scratch, json!({"op":"status"}));
        // A bounded negative observation spans two report periods. It is
        // test synchronization, not a response-latency measurement.
        let until = Instant::now() + Duration::from_millis(2200);
        while Instant::now() < until {
            assert!(!scratch.path("b").exists());
            assert!(process.child.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(100));
        }
        assert!(!scratch.path("b").exists());
        assert!(!process.log().contains("health delivery"));
    }
}

#[test]
fn an_unusable_bus_does_not_prevent_serving_and_reports_recover_when_it_returns() {
    let scratch = Scratch::new();
    fs::write(scratch.path("b"), b"synthetic bus obstruction").unwrap();
    let mut process = Process::start(&scratch, false, true, false);
    process.ready(&scratch);
    ok(
        &scratch,
        json!({"op":"remember","at_unix_ms":now(),"text":"synthetic note during bus failure"}),
    );
    assert_eq!(
        fs::read(scratch.path("b")).unwrap(),
        b"synthetic bus obstruction"
    );
    process.logged("health delivery failed");
    fs::remove_file(scratch.path("b")).unwrap();
    let mut reports = scratch.subscribe();
    retained(&mut reports, 0, 1, 1);
    let next = retained(&mut reports, 0, 1, 1);
    assert!(next.message.is_empty());
    let read = ok(&scratch, json!({"op":"kept"}));
    assert_eq!(read["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn a_working_bus_that_fails_stays_failed_until_publication_really_recovers() {
    const FAILED: &str = "lamp-memory: health delivery failed";
    const RECOVERED: &str = "lamp-memory: health delivery recovered";
    let scratch = Scratch::new();
    let mut reports = scratch.subscribe();
    let mut process = Process::start(&scratch, false, true, false);
    process.ready(&scratch);
    retained(&mut reports, 0, 0, 0);
    assert!(!process.log().contains(FAILED));
    drop(reports);

    // Keep the old publisher's tree for inspection while its configured root
    // becomes a file. This exercises failure of an already-used Publisher.
    let bus = scratch.path("b");
    fs::rename(&bus, scratch.path("old-b")).unwrap();
    fs::write(&bus, b"synthetic sustained bus obstruction").unwrap();
    ok(
        &scratch,
        json!({"op":"remember","at_unix_ms":now(),"text":"synthetic note while reporting is broken"}),
    );
    let mut answered = 1;
    process.logged(FAILED);

    // After the first observed failure, keep the obstruction through three
    // more report periods and continue answering requests. This window is
    // synchronization for retries, not a claim of exact worker scheduling.
    let until = Instant::now() + REPORT_EVERY * 3;
    loop {
        let read = ok(&scratch, json!({"op":"status"}));
        answered += 1;
        assert_eq!(read["people"][0]["kept"], 1);
        assert_eq!(
            fs::read(&bus).unwrap(),
            b"synthetic sustained bus obstruction"
        );
        let log = process.log();
        assert_eq!(
            log.lines().filter(|line| *line == FAILED).count(),
            1,
            "{log}"
        );
        assert_eq!(
            log.lines().filter(|line| *line == RECOVERED).count(),
            0,
            "{log}"
        );
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        thread::sleep(left.min(Duration::from_millis(200)));
    }

    // Rebuild the bus tree, rather than returning the old publisher's files.
    // create_dir_all also permits the retrying publisher to create it first.
    fs::remove_file(&bus).unwrap();
    fs::create_dir_all(&bus).unwrap();
    let mut reports = scratch.subscribe();
    retained(&mut reports, 0, 1, answered);
    process.logged(RECOVERED);
    retained(&mut reports, 0, 1, answered);
    let log = process.log();
    assert_eq!(
        log.lines().filter(|line| *line == FAILED).count(),
        1,
        "{log}"
    );
    assert_eq!(
        log.lines().filter(|line| *line == RECOVERED).count(),
        1,
        "{log}"
    );
}

fn private_file(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
}

#[test]
fn a_store_that_cannot_open_never_claims_readiness_and_any_delivered_report_is_warn() {
    for days in [false, true] {
        let scratch = Scratch::new();
        DirBuilder::new()
            .mode(0o700)
            .create(scratch.path("store"))
            .unwrap();
        // The same file blocks the legacy opener's backend guard, or is
        // rejected as a corrupt SQLite database by the opt-in opener.
        private_file(
            &scratch.path("store").join(DATABASE),
            b"synthetic invalid database",
        );
        let mut reports = scratch.subscribe();
        let mut process = Process::start(&scratch, days, true, false);
        assert_eq!(process.exited().code(), Some(1), "{}", process.log());
        assert!(!scratch.path("ready").exists());
        // Final reporting is best-effort and shutdown waits only 100 ms.
        // Absence is allowed; a delivered report must not claim health.
        let until = Instant::now() + Duration::from_millis(250);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let Some(message) = reports.recv(left).unwrap() else {
                break;
            };
            let report: Diagnostic = message.parse().unwrap();
            assert_eq!(report.name, "memory");
            assert_eq!(report.level, Level::Warn);
            assert!(!report.message.is_empty());
        }
        assert_eq!(
            fs::read(scratch.path("store").join(DATABASE)).unwrap(),
            b"synthetic invalid database"
        );
    }
}
