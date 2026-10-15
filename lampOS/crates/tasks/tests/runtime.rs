use lamp_bus::Bus;
use lamp_msgs::{AgentState, Diagnostic, Sign, TaskStatus};
use lamp_tasks::{
    engine::Clock as Sample,
    harness::{EngineKind, Launch, LaunchOutcome, Listed, Observation, Target},
    runtime::{Adapter, Clock, Config, request, serve_with_adapter},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs, io,
    io::Write,
    os::unix::{fs::PermissionsExt, net::UnixStream},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const AGENT: &str = "11111111-1111-4111-8111-111111111111";
#[derive(Clone)]
struct Injected(Arc<Mutex<Sample>>);
impl Clock for Injected {
    fn sample(&self) -> io::Result<Sample> {
        Ok(self.0.lock().unwrap().clone())
    }
}
#[derive(Default)]
struct Counts {
    launches: AtomicUsize,
    reads: AtomicUsize,
    listings: AtomicUsize,
    /// What `ps` lists next: the agent's progress line.
    about: Mutex<Option<String>>,
    /// `ps` fails while set.
    ps_fails: AtomicBool,
}
struct Script {
    counts: Arc<Counts>,
    stop: Arc<AtomicBool>,
    slow: bool,
}
impl Adapter for Script {
    fn launch(&self, _: &Launch) -> LaunchOutcome {
        self.counts.launches.fetch_add(1, Ordering::SeqCst);
        if self.slow {
            while !self.stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
            }
            return LaunchOutcome::Uncertain {
                code: "cancelled".into(),
            };
        }
        LaunchOutcome::Created {
            agent_id: AGENT.into(),
        }
    }
    fn observe(&self, machine: &str, id: &str) -> Result<Observation, String> {
        self.counts.reads.fetch_add(1, Ordering::SeqCst);
        // Read on the machine the task was started on.
        assert_eq!(machine, "0123456789abcdef0123456789abcdef");
        assert_eq!(id, AGENT);
        Ok(Observation {
            agent_id: AGENT.into(),
            state: "needs-input".into(),
            ask: Some("An earlier question".into()),
            answer: "Private scripted answer".into(),
            truncated: false,
            tools: 2,
        })
    }
    fn status(&self, machine: &str) -> Result<Vec<Listed>, String> {
        self.counts.listings.fetch_add(1, Ordering::SeqCst);
        assert_eq!(machine, "0123456789abcdef0123456789abcdef");
        if self.counts.ps_fails.load(Ordering::SeqCst) {
            return Err("timeout".into());
        }
        Ok(vec![Listed {
            agent_id: AGENT.into(),
            state: "working".into(),
            last_active_unix_ms: Some(1_792_022_712_345),
            about: self.counts.about.lock().unwrap().clone(),
        }])
    }
}

struct Running {
    root: tempfile::TempDir,
    config: Config,
    clock: Injected,
    stop: Arc<AtomicBool>,
    counts: Arc<Counts>,
    handle: Option<JoinHandle<io::Result<()>>>,
    slow: bool,
}
impl Running {
    fn new(slow: bool) -> Self {
        Self::with(slow, false)
    }
    fn with(slow: bool, progress: bool) -> Self {
        let root = tempfile::Builder::new()
            .prefix("tsk-")
            .tempdir_in("/private/tmp")
            .or_else(|_| tempfile::Builder::new().prefix("tsk-").tempdir())
            .unwrap();
        let config = Config {
            enabled: true,
            dir: root.path().join("data"),
            socket: root.path().join("run/public"),
            voice_socket: root.path().join("run/voice"),
            bus_root: root.path().join("b"),
            targets: BTreeMap::from([(
                "coding".into(),
                Target {
                    machine_id: "0123456789abcdef0123456789abcdef".into(),
                    project: "project".into(),
                    engine: EngineKind::Codex,
                },
            )]),
            run_for: None,
            watch: false,
            progress,
        };
        let mut running = Self {
            root,
            config,
            clock: Injected(Arc::new(Mutex::new(Sample {
                boot_id: "test-boot".into(),
                monotonic_ms: 0,
            }))),
            stop: Arc::new(AtomicBool::new(false)),
            counts: Arc::new(Counts::default()),
            handle: None,
            slow,
        };
        running.start();
        running
    }
    fn start(&mut self) {
        self.stop.store(false, Ordering::Release);
        let config = self.config.clone();
        let clock = self.clock.clone();
        let stop = self.stop.clone();
        let script = Script {
            counts: self.counts.clone(),
            stop: stop.clone(),
            slow: self.slow,
        };
        self.handle = Some(thread::spawn(move || {
            serve_with_adapter(config, clock, stop, move |_| Ok(script))
        }));
        let start = Instant::now();
        loop {
            if request(
                &self.config.socket,
                &json!({"op":"status"}),
                Duration::from_secs(2),
            )
            .is_ok_and(|s| s["ok"] == true)
            {
                break;
            }
            if self.handle.as_ref().unwrap().is_finished() {
                panic!(
                    "service failed: {:?}",
                    self.handle.take().unwrap().join().unwrap()
                );
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap().unwrap();
        }
    }
    fn ask(&self, value: Value) -> Value {
        request(&self.config.socket, &value, Duration::from_secs(2)).unwrap()
    }
    fn trusted(&self, value: Value) -> Value {
        request(&self.config.voice_socket, &value, Duration::from_secs(2)).unwrap()
    }
    fn status(&self) -> Value {
        self.ask(json!({"op":"status"}))
    }
    fn guard(&self) -> Value {
        let s = self.status();
        json!({"store_id":s["store_id"],"revision":s["revision"],"request_id":format!("request-{}",s["revision"])})
    }
    fn prepare(&self) -> Value {
        let r=self.ask(json!({"op":"prepare","guard":self.guard(),"text":"Private scripted task","target":"coding"}));
        assert_eq!(r["ok"], true, "{r}");
        r
    }
    fn confirmation(&self, prepared: &Value) -> Value {
        json!({"op":"confirm","guard":self.guard(),"id":prepared["id"],"prepared_digest":prepared["prepared_digest"],"session_id":self.status()["summary"]["session_id"]})
    }
    fn wait(&self, id: &Value, predicate: impl Fn(&Value) -> bool) -> Value {
        let start = Instant::now();
        loop {
            let value = self.ask(json!({"op":"get_task","id":id}));
            if predicate(&value) {
                return value;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "task did not reach expected state: {value}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

#[test]
fn model_cannot_confirm_and_restart_keeps_exact_dispatch_without_resending() {
    let mut s = Running::new(false);
    let prepared = s.prepare();
    thread::sleep(Duration::from_millis(50));
    assert_eq!(s.counts.launches.load(Ordering::SeqCst), 0);
    let confirm = s.confirmation(&prepared);
    assert_eq!(s.ask(confirm.clone())["error"], "untrusted");
    assert_eq!(s.trusted(confirm.clone())["ok"], true);
    let accepted = s.wait(&prepared["id"], |v| {
        v["task"]["dispatch"]["state"] == "accepted"
    });
    assert_eq!(accepted["text"], "Private scripted task");
    assert_eq!(
        accepted["target"]["machine_id"],
        "0123456789abcdef0123456789abcdef"
    );
    assert_eq!(accepted["task"]["completion"], "unverified");
    s.stop();
    assert!(!s.config.socket.exists());
    assert!(!s.config.voice_socket.exists());
    s.start();
    s.wait(&prepared["id"], |v| {
        v["task"]["dispatch"]["state"] == "accepted"
    });
    assert_eq!(s.trusted(confirm)["error"], "stale_session");
    assert_eq!(s.counts.launches.load(Ordering::SeqCst), 1);
}

#[test]
fn who_can_be_given_work_is_read_on_the_request_socket_and_runs_nothing() {
    let s = Running::new(false);
    let told = s.ask(json!({"op":"targets"}));
    assert_eq!(told["ok"], true, "{told}");
    assert_eq!(
        told["targets"],
        json!([{"alias":"coding","engine":"codex"}])
    );
    assert_eq!(s.status()["revision"], told["revision"]);
    assert_eq!(s.counts.launches.load(Ordering::SeqCst), 0);
}

#[test]
fn observed_output_and_reported_playout_remain_separate_and_survive_restart() {
    let mut s = Running::new(false);
    let prepared = s.prepare();
    assert_eq!(s.trusted(s.confirmation(&prepared))["ok"], true);
    s.wait(&prepared["id"], |v| {
        v["task"]["dispatch"]["state"] == "accepted"
    });
    assert_eq!(
        s.ask(json!({"op":"observe","guard":s.guard(),"id":prepared["id"]}))["ok"],
        true
    );
    let task = s.wait(&prepared["id"], |v| {
        v["observations"].as_array().is_some_and(|v| !v.is_empty())
    });
    let observation = task["observations"][0]["id"].clone();
    let observed =
        s.ask(json!({"op":"get_observation","id":prepared["id"],"observation_id":observation}));
    assert_eq!(observed["data"]["answer"], "Private scripted answer");
    assert_eq!(observed["data"]["state"], "needs-input");
    assert_eq!(observed["completion"], "unverified");
    assert!(observed["speech"].is_null());
    assert_eq!(s.status()["summary"]["speech_spoken"], 0);
    let session = s.status()["summary"]["session_id"].clone();
    let claim = json!({"op":"claim_speech","guard":s.guard(),"id":prepared["id"],"observation_id":observation,"text":"The agent needs attention in Harness.","session_id":session});
    assert_eq!(s.ask(claim.clone())["error"], "untrusted");
    let granted = s.trusted(claim);
    assert_eq!(granted["ok"], true, "{granted}");
    assert_eq!(s.status()["summary"]["speech_spoken"], 0);
    let ack = json!({"op":"ack_speech","guard":s.guard(),"id":prepared["id"],"observation_id":observation,"claim_id":granted["claim_id"],"text_digest":granted["text_digest"],"session_id":session,"played":true});
    assert_eq!(s.trusted(ack)["reported_spoken"], true);
    s.stop();
    s.start();
    let observed =
        s.ask(json!({"op":"get_observation","id":prepared["id"],"observation_id":observation}));
    assert_eq!(observed["speech"]["state"], "spoken");
    assert_eq!(
        observed["speech"]["text"],
        "The agent needs attention in Harness."
    );
    assert_eq!(s.counts.launches.load(Ordering::SeqCst), 1);
    assert_eq!(s.counts.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn a_slow_cli_does_not_block_status_diagnostics_or_bounded_shutdown() {
    let mut s = Running::new(true);
    let prepared = s.prepare();
    assert_eq!(s.trusted(s.confirmation(&prepared))["ok"], true);
    s.wait(&prepared["id"], |v| {
        v["task"]["dispatch"]["state"] == "attempting"
    });
    let start = Instant::now();
    let status = s.status();
    assert_eq!(status["summary"]["attempting"], 1);
    assert!(start.elapsed() < Duration::from_secs(1));
    let bus = Bus::new(&s.config.bus_root).unwrap();
    let mut diagnostic = bus
        .subscribe_typed(Diagnostic::TOPIC, "tasks_test", Diagnostic::TYPE)
        .unwrap();
    let start = Instant::now();
    let mut found = false;
    while start.elapsed() < Duration::from_secs(3) {
        if let Some(message) = diagnostic.recv(Duration::from_millis(50)).unwrap() {
            let value: Value = message.parse().unwrap();
            let text = value.to_string();
            assert!(!text.contains("Private scripted"));
            if text.contains("cli_busy") && text.contains("true") {
                found = true;
                break;
            }
        }
    }
    assert!(found, "diagnostics stopped during CLI work");
    let stop = Instant::now();
    s.stop();
    assert!(stop.elapsed() < Duration::from_secs(2));
    s.slow = false;
    s.start();
    s.wait(&prepared["id"], |v| {
        v["task"]["dispatch"]["state"] == "uncertain"
    });
    assert_eq!(s.counts.launches.load(Ordering::SeqCst), 1);
}

#[test]
fn a_silent_client_uses_only_one_bounded_worker() {
    let s = Running::new(false);
    let _silent = UnixStream::connect(&s.config.socket).unwrap();
    let began = Instant::now();
    assert_eq!(s.status()["ok"], true);
    assert!(began.elapsed() < Duration::from_secs(1));
    for path in [&s.config.socket, &s.config.voice_socket] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(s.root.path().join("data/tasks.sqlite3").is_file());
}

#[test]
fn expired_and_restarted_preparations_never_reach_harness() {
    let mut s = Running::new(false);
    let old = s.prepare();
    let confirm = s.confirmation(&old);
    s.clock.0.lock().unwrap().monotonic_ms = 120_000;
    assert_eq!(s.trusted(confirm)["ok"], false);
    let recent = s.prepare();
    let confirm = s.confirmation(&recent);
    s.stop();
    s.start();
    assert_eq!(s.trusted(confirm)["error"], "stale_session");
    assert_eq!(s.counts.launches.load(Ordering::SeqCst), 0);
}

#[test]
fn command_line_is_off_without_loading_any_provisioned_paths() {
    let output = Command::new(env!("CARGO_BIN_EXE_lamp-tasks"))
        .args([
            "serve",
            "--targets",
            "/does-not-exist",
            "--harness",
            "/does-not-exist",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["enabled"], false);
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_lamp-tasks"))
            .args(["serve", "--enable", "--enable"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn command_line_rejects_duplicate_fields_before_connecting() {
    for input in [
        r#"{"op":"prepare","op":"status"}"#,
        r#"{"op":"cancel","guard":{"store_id":"a","revision":0,"revision":1,"request_id":"r"},"id":1}"#,
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lamp-tasks"))
            .args(["request", "--socket", "/no-tasks-test-socket"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("bad_request"));
    }
}

/// The next status on the bus, checked.
fn next_status(watch: &mut lamp_bus::Subscriber) -> TaskStatus {
    let message = watch
        .recv(Duration::from_secs(5))
        .unwrap()
        .expect("a status on the bus");
    assert_eq!(message.header.source, "tasks");
    let text = String::from_utf8(message.payload.clone()).unwrap();
    // Typed state only: no word an agent wrote, no task text.
    for words in [
        "Reading the brief",
        "tests pass",
        "Fixed",
        "Private scripted",
    ] {
        assert!(!text.contains(words), "{text}");
    }
    let status: TaskStatus = message.parse().unwrap();
    status.check().unwrap();
    status
}

#[test]
fn with_progress_a_value_from_the_start_each_task_read_at_once_then_on_schedule_and_the_words_stay_here()
 {
    let mut s = Running::with(false, true);
    let bus = Bus::new(&s.config.bus_root).unwrap();
    // A value from the start, before anything is handed over: no task, so
    // "nothing being worked on" is told apart from "not there".
    let begun = Instant::now();
    let start = loop {
        if let Some(message) = bus.latest(TaskStatus::TOPIC).unwrap() {
            break message.parse::<TaskStatus>().unwrap();
        }
        assert!(
            begun.elapsed() < Duration::from_secs(5),
            "no value at start"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert!(start.tasks.is_empty());
    assert_eq!(start.store_id, s.status()["store_id"]);
    assert_eq!(s.counts.listings.load(Ordering::SeqCst), 0);
    let mut watch = bus
        .subscribe_typed(TaskStatus::TOPIC, "tasks_test", TaskStatus::TYPE)
        .unwrap();
    *s.counts.about.lock().unwrap() = Some("Reading the brief".into());
    let prepared = s.prepare();
    assert_eq!(s.trusted(s.confirmation(&prepared))["ok"], true);
    s.wait(&prepared["id"], |v| {
        v["task"]["dispatch"]["state"] == "accepted"
    });
    // Read at once, with the clock where it was handed over.
    let first = next_status(&mut watch);
    assert_eq!(s.counts.listings.load(Ordering::SeqCst), 1);
    let task = &first.tasks[0];
    assert_eq!(first.store_id, start.store_id);
    assert_eq!(
        (
            task.task,
            task.engine.as_str(),
            task.state,
            task.changes,
            task.changed
        ),
        (
            prepared["id"].as_u64().unwrap(),
            "codex",
            AgentState::Working,
            0,
            false
        )
    );
    assert_eq!(task.working_s, Some(0));
    assert_eq!(task.last_active_unix_ms, Some(1_792_022_712_345));
    // The line changes; the next read is half a minute later.
    *s.counts.about.lock().unwrap() = Some("Fixed it; all tests pass".into());
    s.clock.0.lock().unwrap().monotonic_ms = 29_999;
    thread::sleep(Duration::from_millis(100));
    assert_eq!(s.counts.listings.load(Ordering::SeqCst), 1);
    s.clock.0.lock().unwrap().monotonic_ms = 30_000;
    let changed = next_status(&mut watch);
    let task = &changed.tasks[0];
    assert_eq!((task.changes, task.changed), (1, true));
    assert_eq!(task.signs, vec![Sign::TestsPass, Sign::Fixed]);
    assert_eq!(task.new_share, 1.0);
    // The words are handed over on the socket, for the lamp about to say them.
    let words = s.ask(json!({"op":"get_progress","id":prepared["id"]}));
    assert_eq!(words["ok"], true, "{words}");
    assert_eq!(words["progress"]["about"], "Fixed it; all tests pass");
    assert_eq!(words["progress"]["changes"], 1);
    assert_eq!(words["progress"]["run"], changed.run);
    assert_eq!(words["store_id"], changed.store_id);
    assert_eq!(
        s.ask(json!({"op":"get_progress","id":99}))["error"],
        "not_found"
    );
    // The reads are counted in the diagnostics.
    let mut diagnostic = bus
        .subscribe_typed(Diagnostic::TOPIC, "tasks_test", Diagnostic::TYPE)
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(message) = diagnostic.recv(Duration::from_millis(50)).unwrap() {
            let value: Value = message.parse().unwrap();
            if value.to_string().contains("status_reads") {
                assert!(!value.to_string().contains("tests pass"));
                break;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(3), "no status_reads");
    }
    // A restart keeps the store and the task's number and counts again
    // from nothing, as another run: a count names a change only with it.
    assert_eq!(first.run, changed.run);
    s.stop();
    *s.counts.about.lock().unwrap() = Some("Another line altogether".into());
    s.start();
    s.clock.0.lock().unwrap().monotonic_ms = 120_000;
    // The restarted run's value at start has no task; its read at once does.
    let again = loop {
        let status = next_status(&mut watch);
        if !status.tasks.is_empty() {
            break status;
        }
    };
    assert_eq!(again.store_id, changed.store_id);
    assert_eq!(
        (again.tasks[0].task, again.tasks[0].changes),
        (prepared["id"].as_u64().unwrap(), 0)
    );
    assert!(
        again.run > changed.run,
        "{} after {}",
        again.run,
        changed.run
    );
    let words = s.ask(json!({"op":"get_progress","id":prepared["id"]}));
    assert_eq!(words["progress"]["run"], again.run);
    // Settled (the agent asks the person): it leaves the watch, and the
    // bus, at once rather than at the next read, which with the clock held
    // at 120 s is not due. A read already under way may come first.
    assert_eq!(
        s.ask(json!({"op":"observe","guard":s.guard(),"id":prepared["id"]}))["ok"],
        true
    );
    let mut seen = 0;
    let left = loop {
        let status = next_status(&mut watch);
        seen += 1;
        if status.tasks.is_empty() || seen == 3 {
            break status;
        }
    };
    assert!(left.tasks.is_empty(), "{left:?}");
}

/// The reviewer on #793 (16:46): with every `ps` failing, a task handed
/// over, and again after a restart of the persisted store, was on the bus
/// as nothing, and `get_progress` said there was no such task. It is
/// `unread` on `tasks/status`, and `get_progress` says it has not been read.
#[test]
fn a_watched_task_whose_reads_fail_is_unread_not_gone_and_after_a_restart_too() {
    let mut s = Running::with(false, true);
    s.counts.ps_fails.store(true, Ordering::SeqCst);
    let bus = Bus::new(&s.config.bus_root).unwrap();
    let mut watch = bus
        .subscribe_typed(TaskStatus::TOPIC, "tasks_test", TaskStatus::TYPE)
        .unwrap();
    let prepared = s.prepare();
    assert_eq!(s.trusted(s.confirmation(&prepared))["ok"], true);
    let id = prepared["id"].as_u64().unwrap();
    let unread = |watch: &mut lamp_bus::Subscriber| loop {
        let status = next_status(watch);
        if status.unread == [id] {
            break status;
        }
    };
    let first = unread(&mut watch);
    assert!(first.tasks.is_empty());
    let asked = s.ask(json!({"op":"get_progress","id":id}));
    assert_eq!(asked["error"], "not_read", "{asked}");
    // Restarted, the store kept, reads still failing: unread again.
    s.stop();
    s.start();
    let again = unread(&mut watch);
    assert!(again.tasks.is_empty() && again.run > first.run);
    // Reads come back: read, no longer unread.
    s.counts.ps_fails.store(false, Ordering::SeqCst);
    s.clock.0.lock().unwrap().monotonic_ms = 30_000;
    let read = loop {
        let status = next_status(&mut watch);
        if !status.tasks.is_empty() {
            break status;
        }
    };
    assert!(read.unread.is_empty());
    assert_eq!(read.tasks[0].task, id);
}

#[test]
fn without_progress_nothing_is_listed_the_topic_is_untouched_and_the_report_is_as_before() {
    let s = Running::new(false);
    let prepared = s.prepare();
    assert_eq!(s.trusted(s.confirmation(&prepared))["ok"], true);
    s.wait(&prepared["id"], |v| {
        v["task"]["dispatch"]["state"] == "accepted"
    });
    s.clock.0.lock().unwrap().monotonic_ms = 3 * 60_000;
    let bus = Bus::new(&s.config.bus_root).unwrap();
    let mut diagnostic = bus
        .subscribe_typed(Diagnostic::TOPIC, "tasks_test", Diagnostic::TYPE)
        .unwrap();
    let message = loop {
        if let Some(message) = diagnostic.recv(Duration::from_secs(3)).unwrap() {
            break message;
        }
    };
    let report: Value = message.parse().unwrap();
    let keys: Vec<String> = report["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| pair["key"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        keys,
        [
            "store_ready",
            "cli_busy",
            "requests",
            "publish_errors",
            "clients_refused",
            "completion_supported",
            "retained",
            "queued",
            "attempting",
            "accepted",
            "pending",
            "uncertain",
            "observations",
            "speech_spoken",
            "speech_uncertain"
        ]
    );
    assert_eq!(s.counts.listings.load(Ordering::SeqCst), 0);
    // The topic was never made: no publisher, no declaration, no value.
    assert!(!s.config.bus_root.join("tasks").exists());
    assert!(bus.latest(TaskStatus::TOPIC).unwrap().is_none());
    // Asked for anyway, the words are not there.
    assert_eq!(
        s.ask(json!({"op":"get_progress","id":prepared["id"]}))["error"],
        "progress_off"
    );
}
