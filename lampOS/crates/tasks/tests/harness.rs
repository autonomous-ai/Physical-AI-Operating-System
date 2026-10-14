//! Scripted local executable fixtures. No installed Harness, daemon, account,
//! remote agent, credentials or provider is invoked by these tests.
use lamp_tasks::harness::{
    Cli, CliConfig, EngineKind, Launch, LaunchOutcome, Listed, MOST_ABOUT_BYTES, MOST_LISTED,
    PS_STDOUT_LIMIT, Target, iso_unix_ms, parse_ps,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const MACHINE: &str = "0123456789abcdef0123456789abcdef";
const AGENT: &str = "11111111-1111-4111-8111-111111111111";
const CREATION: &str = "22222222-2222-4222-8222-222222222222";

struct Rig {
    root: tempfile::TempDir,
    config: CliConfig,
}
impl Rig {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("tasks-harness-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let config = CliConfig {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_lamp-tasks-harness-fixture")),
            // The production read bound. The fixture is a fresh process
            // per call, and on a loaded host its first start alone has
            // taken over a second (the fourth builder's run of 2026-10-15,
            // #698). Each timeout case sets its own short limit.
            launch_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(10),
            environment: BTreeMap::from([
                ("HOME".into(), root.path().as_os_str().into()),
                ("PATH".into(), OsString::from("/usr/bin:/bin")),
                ("LANG".into(), OsString::from("C")),
            ]),
            cancel: None,
            as_person: false,
        };
        let rig = Self { root, config };
        rig.plan(
            json!({"machines":{"default":true},"new":{"default":true},"show":{"default":true}}),
        );
        rig
    }
    fn plan(&self, plan: Value) {
        use std::io::Write;
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.root.path().join("plan.json"))
            .unwrap()
            .write_all(serde_json::to_string(&plan).unwrap().as_bytes())
            .unwrap();
    }
    fn new_step(&self, step: Value) {
        self.plan(json!({"machines":{"default":true},"new":step}));
    }
    fn show_step(&self, step: Value) {
        self.plan(json!({"show":step}));
    }
    fn cli(&self) -> Cli {
        Cli::new(self.config.clone(), "stable-session_123".into()).unwrap()
    }
    fn calls(&self) -> Vec<Value> {
        match fs::read_to_string(self.root.path().join("calls.jsonl")) {
            Ok(text) => text
                .split_inclusive('\n')
                .filter(|line| line.ends_with('\n'))
                .map(|line| serde_json::from_str(line).unwrap())
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("fixture call log: {error}"),
        }
    }
    fn wait_operation(&self, operation: &str) {
        let until = Instant::now() + Duration::from_secs(2);
        while !self.calls().iter().any(|call| call["args"][0] == operation) {
            assert!(Instant::now() < until, "fixture did not enter {operation}");
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn copied_binary(&mut self) {
        let copy = self.root.path().join("fixture");
        fs::copy(&self.config.executable, &copy).unwrap();
        fs::set_permissions(&copy, fs::Permissions::from_mode(0o700)).unwrap();
        self.config.executable = copy;
        // A newly copied image can incur the host's first-execution checks.
        // These identity tests use the production read bound, not a cold-start
        // performance assertion. Dedicated timeout cases inject shorter limits.
        self.config.read_timeout = Duration::from_secs(10);
        self.config.launch_timeout = Duration::from_secs(10);
    }
}

fn launch() -> Launch {
    Launch {
        name: "lamp-task-001".into(),
        prompt: "Inspect the authored fixture".into(),
        target: Target {
            machine_id: MACHINE.into(),
            project: "fixture-project".into(),
            engine: EngineKind::Codex,
        },
    }
}
fn created() -> Value {
    json!({"ok":true,"machineId":MACHINE,"agent":{"id":AGENT,"name":"lamp-task-001","engine":"codex","permissionMode":"auto","bypassPermission":true}})
}
fn shown() -> Value {
    json!({"id":AGENT,"state":"idle","ask":"An older unrelated question","answer":"Observed output only","truncated":false,"tools":2})
}
fn step(reply: Value) -> Value {
    json!({"stdout":format!("{reply}\n")})
}
fn uncertain(code: &str) -> LaunchOutcome {
    LaunchOutcome::Uncertain { code: code.into() }
}
fn not_sent(code: &str) -> LaunchOutcome {
    LaunchOutcome::NotSent { code: code.into() }
}

#[test]
fn launch_uses_exact_argv_auto_mode_null_stdin_and_stable_generic_identity() {
    let rig = Rig::new();
    rig.plan(json!({"machines":{"default":true,"stdin_eof":true},"new":{"default":true,"stdin_eof":true}}));
    let mut request = launch();
    request.prompt = "  literal '$HOME' $(touch never) ; --mode full\nnext line  ".into();
    assert_eq!(
        rig.cli().launch(&request),
        LaunchOutcome::Created {
            agent_id: AGENT.into()
        }
    );
    let calls = rig.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["args"], json!(["machines", "--json"]));
    assert_eq!(
        calls[1]["args"],
        json!([
            "new",
            "codex",
            "--machine",
            MACHINE,
            "fixture-project",
            "--name",
            "lamp-task-001",
            "--mode",
            "auto",
            "--json",
            "--task",
            request.prompt
        ])
    );
    for call in calls {
        let environment = call["environment"].as_object().unwrap();
        assert_eq!(environment.len(), 5);
        assert_eq!(environment["AI_AGENT"], "lampos");
        assert_eq!(environment["HARNESS_AGENT_SESSION"], "stable-session_123");
        for forbidden in [
            "TMUX_PANE",
            "CLAUDECODE",
            "CLAUDE_CODE_SESSION_ID",
            "CODEX_THREAD_ID",
        ] {
            assert!(!environment.contains_key(forbidden));
        }
    }
    assert!(!rig.root.path().join("never").exists());
}

#[test]
fn as_the_person_the_call_carries_no_agent_mark_and_is_otherwise_the_same() {
    let mut rig = Rig::new();
    rig.config.as_person = true;
    rig.plan(json!({"machines":{"default":true,"stdin_eof":true},"new":{"default":true,"stdin_eof":true}}));
    assert_eq!(
        rig.cli().launch(&launch()),
        LaunchOutcome::Created {
            agent_id: AGENT.into()
        }
    );
    let calls = rig.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1]["args"][0], "new");
    for call in calls {
        let environment = call["environment"].as_object().unwrap();
        assert_eq!(environment.len(), 3, "{environment:?}");
        assert!(!environment.contains_key("AI_AGENT"));
        assert!(!environment.contains_key("HARNESS_AGENT_SESSION"));
    }
}

#[test]
fn claude_and_explicit_folder_are_preserved_without_shell_expansion() {
    let rig = Rig::new();
    let mut request = launch();
    request.target.engine = EngineKind::Claude;
    request.target.project = "~/code/my project".into();
    assert!(matches!(
        rig.cli().launch(&request),
        LaunchOutcome::Created { .. }
    ));
    assert_eq!(rig.calls()[1]["args"][1], "claude");
    assert_eq!(rig.calls()[1]["args"][4], "~/code/my project");
}

/// OpenCode, as Harness 0.4.2's `new --help` names it, in auto mode like the
/// others. Only a receipt that says auto mode binds the task; one that does
/// not (what a real OpenCode receipt says is not yet seen) leaves it
/// uncertain, never sent again.
#[test]
fn opencode_is_asked_for_by_name_in_auto_mode_and_bound_only_by_its_own_receipt() {
    let rig = Rig::new();
    let mut request = launch();
    request.target.engine = EngineKind::Opencode;
    assert_eq!(
        rig.cli().launch(&request),
        LaunchOutcome::Created {
            agent_id: AGENT.into()
        }
    );
    let args = &rig.calls()[1]["args"];
    assert_eq!(args[1], "opencode");
    assert_eq!((&args[7], &args[8]), (&json!("--mode"), &json!("auto")));
    for reply in [
        json!({"ok":true,"machineId":MACHINE,"agent":{"id":AGENT,"name":"lamp-task-001","engine":"opencode","permissionMode":null,"bypassPermission":null}}),
        json!({"ok":true,"machineId":MACHINE,"agent":{"id":AGENT,"name":"lamp-task-001","engine":"opencode","permissionMode":"ask","bypassPermission":null}}),
    ] {
        rig.new_step(step(reply));
        assert_eq!(rig.cli().launch(&request), uncertain("invalid_reply"));
    }
}

#[test]
fn input_limits_and_parser_ambiguity_are_refused_before_any_process() {
    let rig = Rig::new();
    let cli = rig.cli();
    let mut requests = Vec::new();
    for prompt in [
        "".into(),
        "--mode full".into(),
        "a\0b".into(),
        "é".repeat(501),
    ] {
        let mut request = launch();
        request.prompt = prompt;
        requests.push(request);
    }
    for machine in ["01234567", "0123456789ABCDEF0123456789ABCDEF", "not-an-id"] {
        let mut request = launch();
        request.target.machine_id = machine.into();
        requests.push(request);
    }
    for project in ["", "--full", "@another", "./relative", "a/b", "a\nb"] {
        let mut request = launch();
        request.target.project = project.into();
        requests.push(request);
    }
    for name in ["--name", "space name", "é"] {
        let mut request = launch();
        request.name = name.into();
        requests.push(request);
    }
    for request in requests {
        assert!(matches!(
            cli.launch(&request),
            LaunchOutcome::NotSent { .. }
        ));
    }
    let mut boundary = launch();
    boundary.prompt = "é".repeat(500);
    assert!(boundary.validate().is_ok());
    assert!(rig.calls().is_empty());
}

#[test]
fn missing_or_competing_environment_and_unsafe_sessions_are_refused() {
    let rig = Rig::new();
    for name in [
        "TMUX_PANE",
        "CLAUDECODE",
        "CLAUDE_CODE_SESSION_ID",
        "CODEX_THREAD_ID",
        "AI_AGENT",
        "HARNESS_AGENT_SESSION",
        "NODE_OPTIONS",
        "TOKEN",
    ] {
        let mut config = rig.config.clone();
        config.environment.insert(name.into(), "inherited".into());
        assert!(Cli::new(config, "stable".into()).is_err(), "{name}");
    }
    for name in ["HOME", "PATH"] {
        let mut config = rig.config.clone();
        config.environment.remove(name);
        assert!(Cli::new(config, "stable".into()).is_err());
    }
    for session in [
        "".to_string(),
        " leading".into(),
        "a:b".into(),
        "é".into(),
        "x".repeat(81),
    ] {
        assert!(Cli::new(rig.config.clone(), session).is_err());
    }
    assert!(rig.calls().is_empty());
}

#[test]
fn executable_is_canonicalized_and_replacement_or_unsafe_modes_are_refused() {
    let mut rig = Rig::new();
    rig.copied_binary();
    let real = rig.config.executable.clone();
    let link = rig.root.path().join("selected");
    symlink(&real, &link).unwrap();
    rig.config.executable = link.clone();
    let cli = rig.cli();
    fs::remove_file(link).unwrap();
    let outcome = cli.launch(&launch());
    assert!(
        matches!(outcome, LaunchOutcome::Created { .. }),
        "{outcome:?}"
    );
    fs::set_permissions(&real, fs::Permissions::from_mode(0o722)).unwrap();
    assert_eq!(
        cli.launch(&launch()),
        not_sent("preflight_executable_changed")
    );
    rig.config.executable = real.clone();
    assert!(Cli::new(rig.config.clone(), "stable".into()).is_err());
    fs::set_permissions(real, fs::Permissions::from_mode(0o4700)).unwrap();
    assert!(Cli::new(rig.config.clone(), "stable".into()).is_err());
}

#[test]
fn a_missing_working_directory_is_a_proven_preflight_spawn_failure() {
    let mut rig = Rig::new();
    rig.config.environment.insert(
        "HOME".into(),
        rig.root.path().join("missing-home").into_os_string(),
    );
    assert_eq!(
        rig.cli().launch(&launch()),
        not_sent("preflight_spawn_failed")
    );
    assert!(rig.calls().is_empty());
}

#[test]
fn preflight_requires_exact_unique_full_machine_ids_in_ndjson() {
    let cases = [
        ("".to_string(), "machine_unavailable"),
        (
            format!(
                "{}\n",
                json!({"machineId":"ffffffffffffffffffffffffffffffff","name":MACHINE,"current":true})
            ),
            "machine_unavailable",
        ),
        (
            format!("{}\n", json!([{"machineId":MACHINE,"current":true}])),
            "preflight_invalid_reply",
        ),
        (
            format!("{}\n", json!({"machineId":"01234567","current":true})),
            "preflight_invalid_reply",
        ),
        (
            format!(
                "{}\n{}\n",
                json!({"machineId":MACHINE,"current":true}),
                json!({"machineId":MACHINE,"current":false})
            ),
            "preflight_invalid_reply",
        ),
        ("{malformed}\n".into(), "preflight_invalid_reply"),
    ];
    for (stdout, code) in cases {
        let rig = Rig::new();
        rig.plan(json!({"machines":{"stdout":stdout},"new":{"default":true}}));
        assert_eq!(rig.cli().launch(&launch()), not_sent(code));
        assert_eq!(rig.calls().len(), 1);
    }
}

#[test]
fn a_machine_list_error_or_timeout_cannot_issue_new() {
    for (step, expected) in [
        (
            json!({"exit":1,"stdout":"human secret detail"}),
            "preflight_failed",
        ),
        (json!({"delay_ms":5000}), "preflight_timeout"),
        (json!({"stdout_bytes":65537}), "preflight_stdout_limit"),
    ] {
        let mut rig = Rig::new();
        if expected == "preflight_timeout" {
            rig.config.read_timeout = Duration::from_millis(500);
        }
        rig.plan(json!({"machines":step,"new":{"default":true}}));
        assert_eq!(rig.cli().launch(&launch()), not_sent(expected));
        assert_eq!(rig.calls().len(), 1);
    }
}

#[test]
fn pending_keeps_creation_identity_without_guessing_an_agent() {
    let rig = Rig::new();
    rig.new_step(step(json!({"ok":true,"pending":true,"machineId":MACHINE,"creationId":CREATION,"name":"lamp-task-001","note":"installing"})));
    assert_eq!(
        rig.cli().launch(&launch()),
        LaunchOutcome::Pending {
            creation_id: CREATION.into()
        }
    );
    assert_eq!(rig.calls().len(), 2);
}

#[test]
fn created_requires_every_identity_and_permission_binding() {
    let mut cases = Vec::new();
    for (field, value) in [
        ("id", json!(CREATION.replace('2', "z"))),
        ("name", json!("other")),
        ("engine", json!("claude")),
        ("permissionMode", json!("full")),
        ("permissionMode", json!("ask")),
        ("bypassPermission", json!("yes")),
    ] {
        let mut reply = created();
        reply["agent"][field] = value;
        cases.push(reply);
    }
    for field in ["permissionMode", "bypassPermission", "id"] {
        let mut reply = created();
        reply["agent"].as_object_mut().unwrap().remove(field);
        cases.push(reply);
    }
    let mut contradiction = created();
    contradiction["error"] = json!("contradictory");
    cases.push(contradiction);
    for reply in cases {
        let rig = Rig::new();
        rig.new_step(step(reply));
        assert_eq!(rig.cli().launch(&launch()), uncertain("invalid_reply"));
        assert_eq!(rig.calls().len(), 2);
    }
    // Auto mode with the bypass reported as true, false or null.
    for bypass in [json!(true), json!(false), Value::Null] {
        let rig = Rig::new();
        let mut reply = created();
        reply["agent"]["bypassPermission"] = bypass;
        rig.new_step(step(reply));
        assert!(matches!(
            rig.cli().launch(&launch()),
            LaunchOutcome::Created { .. }
        ));
    }
}

#[test]
fn machine_change_and_existing_name_heuristic_remain_uncertain() {
    let mut wrong = created();
    wrong["machineId"] = json!("ffffffffffffffffffffffffffffffff");
    for (reply, code) in [
        (wrong, "machine_mismatch"),
        (
            json!({"ok":true,"existing":true,"machineId":MACHINE,"name":"lamp-task-001","state":"running","agentId":AGENT}),
            "existing_unbound",
        ),
    ] {
        let rig = Rig::new();
        rig.new_step(step(reply));
        assert_eq!(rig.cli().launch(&launch()), uncertain(code));
        assert_eq!(rig.calls().len(), 2);
    }
}

#[test]
fn errors_and_malformed_success_after_new_are_never_retried() {
    for (step, code) in [
        (
            json!({"exit":1,"stdout":"{\"ok\":false,\"error\":\"private detail\"}\n"}),
            "new_failed",
        ),
        (json!({"stdout":"not JSON"}), "invalid_reply"),
        (json!({"stdout":"{} {}"}), "invalid_reply"),
        (
            json!({"stdout":"{\"ok\":true,\"agent\":{}}"}),
            "invalid_reply",
        ),
    ] {
        let rig = Rig::new();
        rig.new_step(step);
        assert_eq!(rig.cli().launch(&launch()), uncertain(code));
        assert_eq!(
            rig.calls()
                .iter()
                .filter(|call| call["args"][0] == "new")
                .count(),
            1
        );
    }
}

#[test]
fn both_pipe_caps_and_exit_deadline_are_enforced_after_new() {
    for (step, code) in [
        (json!({"stdout_bytes":65537}), "stdout_limit"),
        (json!({"stderr_bytes":16385,"default":true}), "stderr_limit"),
        (json!({"default":true,"after_output_ms":5000}), "timeout"),
    ] {
        let mut rig = Rig::new();
        if code == "timeout" {
            rig.config.launch_timeout = Duration::from_millis(500);
        }
        rig.new_step(step);
        let began = Instant::now();
        assert_eq!(rig.cli().launch(&launch()), uncertain(code));
        assert!(began.elapsed() < Duration::from_secs(2));
        assert_eq!(rig.calls().len(), 2);
    }
}

#[test]
fn a_descendant_holding_pipes_is_killed_without_an_unbounded_drain() {
    let rig = Rig::new();
    rig.new_step(json!({"held_descendant":true,"default":true}));
    let began = Instant::now();
    assert_eq!(rig.cli().launch(&launch()), uncertain("pipes_open"));
    assert!(began.elapsed() < Duration::from_secs(2));
    let heartbeat = fs::read(rig.root.path().join("heartbeat")).unwrap();
    thread::sleep(Duration::from_millis(50));
    assert_eq!(
        fs::read(rig.root.path().join("heartbeat")).unwrap(),
        heartbeat
    );
    // Reaping the direct child is observable: it cannot be waited for again.
    let pid = rig.calls()[1]["pid"].as_u64().unwrap() as i32;
    let error = rustix::process::waitpid(
        rustix::process::Pid::from_raw(pid),
        rustix::process::WaitOptions::NOHANG,
    )
    .unwrap_err();
    assert_eq!(error, rustix::io::Errno::CHILD);
}

#[test]
fn a_live_descendant_with_closed_pipes_is_still_killed_on_normal_exit() {
    let rig = Rig::new();
    rig.new_step(json!({"held_descendant":true,"redirect_descendant":true,"default":true}));
    assert_eq!(
        rig.cli().launch(&launch()),
        LaunchOutcome::Created {
            agent_id: AGENT.into()
        }
    );
    let heartbeat = fs::read(rig.root.path().join("heartbeat")).unwrap();
    thread::sleep(Duration::from_millis(50));
    assert_eq!(
        fs::read(rig.root.path().join("heartbeat")).unwrap(),
        heartbeat
    );
}

#[test]
fn changed_executable_after_preflight_proves_new_was_not_spawned() {
    let mut rig = Rig::new();
    rig.copied_binary();
    rig.plan(json!({"machines":{"default":true,"remove_executable":true},"new":{"default":true}}));
    assert_eq!(rig.cli().launch(&launch()), not_sent("executable_changed"));
    assert_eq!(rig.calls().len(), 1);
}

#[test]
fn cancellation_before_spawn_does_not_run_preflight() {
    let mut rig = Rig::new();
    rig.config.cancel = Some(Arc::new(AtomicBool::new(true)));
    let cli = rig.cli();
    assert_eq!(cli.launch(&launch()), not_sent("cancelled"));
    assert_eq!(cli.observe(MACHINE, AGENT), Err("cancelled".into()));
    assert!(rig.calls().is_empty());
}

#[test]
fn cancellation_during_preflight_is_not_sent_and_never_runs_new() {
    let mut rig = Rig::new();
    let cancel = Arc::new(AtomicBool::new(false));
    rig.config.cancel = Some(cancel.clone());
    rig.plan(json!({"machines":{"delay_ms":5000},"new":{"default":true}}));
    let cli = rig.cli();
    let running = thread::spawn(move || cli.launch(&launch()));
    rig.wait_operation("machines");
    cancel.store(true, Ordering::Release);
    assert_eq!(running.join().unwrap(), not_sent("cancelled"));
    assert_eq!(rig.calls().len(), 1);
}

#[test]
fn cancellation_after_new_and_busy_calls_do_not_start_another_process() {
    let mut rig = Rig::new();
    let cancel = Arc::new(AtomicBool::new(false));
    rig.config.cancel = Some(cancel.clone());
    rig.new_step(json!({"delay_ms":5000}));
    let cli = Arc::new(rig.cli());
    let worker = cli.clone();
    let running = thread::spawn(move || worker.launch(&launch()));
    rig.wait_operation("new");
    assert_eq!(cli.launch(&launch()), not_sent("busy"));
    assert_eq!(cli.observe(MACHINE, AGENT), Err("busy".into()));
    let began = Instant::now();
    cancel.store(true, Ordering::Release);
    assert_eq!(running.join().unwrap(), uncertain("cancelled"));
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(rig.calls().len(), 2);
}

#[test]
fn show_uses_full_agent_id_and_preserves_observation_only_fields() {
    let rig = Rig::new();
    let reply = shown();
    rig.show_step(step(reply));
    let observation = rig.cli().observe(MACHINE, AGENT).unwrap();
    assert_eq!(observation.agent_id, AGENT);
    assert_eq!(observation.state, "idle");
    assert_eq!(
        observation.ask.as_deref(),
        Some("An older unrelated question")
    );
    assert_eq!(observation.answer, "Observed output only");
    assert_eq!(
        rig.calls()[0]["args"],
        json!([
            "show",
            "--machine",
            MACHINE,
            AGENT,
            "--chars",
            "4000",
            "--json"
        ])
    );
    let rig = Rig::new();
    let mut reply = shown();
    reply["state"] = json!("needs-input");
    reply["truncated"] = json!(true);
    rig.show_step(step(reply));
    let observation = rig.cli().observe(MACHINE, AGENT).unwrap();
    assert!(observation.truncated);
    assert_eq!(observation.state, "needs-input");
}

#[test]
fn observation_bounds_wrong_identity_and_invented_completion_are_rejected() {
    let mut cases = Vec::new();
    for state in ["done", "completed", "gone", "closed", "timeout"] {
        let mut reply = shown();
        reply["state"] = json!(state);
        cases.push(reply);
    }
    for (field, value) in [
        ("tools", json!(-1)),
        ("tools", json!(1.5)),
        ("tools", json!(1000001)),
        ("answer", json!("x".repeat(16385))),
        ("ask", json!("x".repeat(4097))),
        ("error", json!("contradicts a successful observation")),
        ("ok", json!(false)),
    ] {
        let mut reply = shown();
        reply[field] = value;
        cases.push(reply);
    }
    let mut absent = shown();
    absent.as_object_mut().unwrap().remove("ask");
    cases.push(absent);
    for reply in cases {
        let rig = Rig::new();
        rig.show_step(step(reply));
        assert_eq!(
            rig.cli().observe(MACHINE, AGENT),
            Err("invalid_observation".into())
        );
    }
    let rig = Rig::new();
    let mut wrong = shown();
    wrong["id"] = json!(CREATION);
    rig.show_step(step(wrong));
    assert_eq!(
        rig.cli().observe(MACHINE, AGENT),
        Err("agent_mismatch".into())
    );
    assert_eq!(
        rig.cli().observe(MACHINE, "111111"),
        Err("invalid_agent_id".into())
    );
    assert_eq!(
        rig.cli().observe("office", AGENT),
        Err("invalid_machine".into())
    );
    assert_eq!(rig.calls().len(), 1);
}

#[test]
fn read_failure_codes_do_not_echo_private_cli_details() {
    let rig = Rig::new();
    rig.show_step(
        json!({"exit":1,"stdout":"{\"error\":\"private account detail\"}","stderr_bytes":100}),
    );
    assert_eq!(
        rig.cli().observe(MACHINE, AGENT),
        Err("observe_failed".into())
    );
}

#[test]
fn read_timeout_and_output_cap_never_retry_show() {
    for (step, code) in [
        (json!({"delay_ms":5000}), "timeout"),
        (json!({"stdout_bytes":65537}), "stdout_limit"),
        (json!({"stderr_bytes":16385}), "stderr_limit"),
    ] {
        let mut rig = Rig::new();
        if code == "timeout" {
            rig.config.read_timeout = Duration::from_millis(500);
        }
        rig.show_step(step);
        let began = Instant::now();
        assert_eq!(rig.cli().observe(MACHINE, AGENT), Err(code.into()));
        assert!(began.elapsed() < Duration::from_secs(2));
        assert_eq!(rig.calls().len(), 1);
    }
}

/// A `harness ps --machine <id> --json` reply, written by hand in the shape
/// Harness 0.4.2's CLI makes (`JSON.stringify(..., null, 1)` of `{you,
/// machine, harnesses}`), read from its source, not from any real session:
/// the ids, names, folders and lines are invented.
fn listing() -> Value {
    json!({"you":null,"machine":"office","harnesses":[
        {"name":"lamp-0a1b","id":AGENT,"agent":"Claude Code","machine":"office","state":"working",
         "folder":"/home/person/harnesses/lamp-tasks","branch":null,"startedBy":"person","yours":false,"self":false,
         "lastActive":"2026-10-15T00:05:12.345Z","asks":["Suggest three names for a desk lamp"],
         "about":"Read the brief / Wrote three names to names.md / All 3 tests pass"},
        {"name":"other","id":"33333333-3333-4333-8333-333333333333","agent":"Codex","machine":"office","state":"idle",
         "folder":null,"branch":null,"startedBy":"person","yours":false,"self":false,"lastActive":null},
        // Still being made: listed by its creation id, which is passed over.
        {"name":"lamp-9z","id":"creation-abcdef0123456789","agent":"Claude Code","machine":"office","state":"starting",
         "folder":null,"branch":null,"startedBy":"you","yours":true,"self":false,"lastActive":"2026-10-15T00:06:00.000Z",
         "about":"lamp-9z has not started yet: it is starting."}
    ]})
}

#[test]
fn a_listing_is_read_for_state_last_activity_and_the_line_and_nothing_else() {
    let text = serde_json::to_string_pretty(&listing()).unwrap();
    let listed = parse_ps(text.as_bytes()).unwrap();
    assert_eq!(
        listed,
        vec![
            Listed {
                agent_id: AGENT.into(),
                state: "working".into(),
                last_active_unix_ms: Some(1_792_022_712_345),
                about: Some(
                    "Read the brief / Wrote three names to names.md / All 3 tests pass".into()
                ),
            },
            Listed {
                agent_id: "33333333-3333-4333-8333-333333333333".into(),
                state: "idle".into(),
                last_active_unix_ms: None,
                about: None,
            },
        ]
    );
    // A long line is cut at a whole character; control characters go.
    let mut long = listing();
    long["harnesses"][0]["about"] = json!(format!("é\n{}", "ü".repeat(MOST_ABOUT_BYTES)));
    let about = parse_ps(long.to_string().as_bytes()).unwrap()[0]
        .about
        .clone()
        .unwrap();
    assert!(
        about.len() <= MOST_ABOUT_BYTES && about.starts_with("é ü"),
        "{}",
        about.len()
    );
    // A time in another form is not known; the rest is read.
    let mut local = listing();
    local["harnesses"][0]["lastActive"] = json!("2026-10-15 07:05:12+07:00");
    let read = parse_ps(local.to_string().as_bytes()).unwrap();
    assert_eq!(
        (read[0].last_active_unix_ms, read[0].state.as_str()),
        (None, "working")
    );
    // Refused whole: not the shape, too many, a state that is no word.
    let mut many = listing();
    many["harnesses"] = json!(vec![listing()["harnesses"][1].clone(); MOST_LISTED + 1]);
    let mut spaced = listing();
    spaced["harnesses"][0]["state"] = json!("hard at work");
    for wrong in [
        json!([]),
        json!({"machine":"office"}),
        json!({"harnesses":[{"id":AGENT}]}),
        many,
        spaced,
    ] {
        assert_eq!(
            parse_ps(wrong.to_string().as_bytes()),
            Err("status_invalid_reply"),
            "{wrong}"
        );
    }
    assert_eq!(parse_ps(b"not json"), Err("status_invalid_reply"));
}

#[test]
fn times_are_read_only_in_the_form_harness_writes() {
    for (text, ms) in [
        ("1970-01-01T00:00:00.000Z", 0),
        ("2026-10-15T00:05:12.345Z", 1_792_022_712_345),
        ("2026-10-14T17:31:02.000Z", 1_791_999_062_000),
        ("2000-02-29T12:00:00Z", 951_825_600_000),
        ("9999-12-31T23:59:59.999Z", 253_402_300_799_999),
    ] {
        assert_eq!(iso_unix_ms(text), Some(ms), "{text}");
    }
    for text in [
        "",
        "2026-10-15",
        "2026-13-01T00:00:00.000Z",
        "2026-10-15T24:00:00.000Z",
        "2026-10-15T00:05:12.345",
        "2026-10-15T00:05:12.34Z",
        "2026-10-15 00:05:12.345Z",
        "1969-12-31T23:59:59.999Z",
        "+2026-10-15T00:05:12Z",
        "2026-10-15T00:05:12.345+07:00",
    ] {
        assert_eq!(iso_unix_ms(text), None, "{text}");
    }
}

#[test]
fn status_is_one_ps_of_the_full_machine_id_bounded_and_never_retried() {
    let rig = Rig::new();
    rig.plan(json!({"ps":step(listing())}));
    let listed = rig.cli().status(MACHINE).unwrap();
    assert_eq!(listed.len(), 2);
    let calls = rig.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0]["args"],
        json!(["ps", "--machine", MACHINE, "--json"])
    );
    // The same caller identity and environment as every other call.
    assert_eq!(calls[0]["environment"]["AI_AGENT"], "lampos");
    assert!(calls[0]["environment"].get("OPENAI_API_KEY").is_none());
    // A listing larger than a read of one agent may be is taken, up to its
    // own bound.
    let rig = Rig::new();
    let mut large = listing();
    large["harnesses"] = json!(vec![listing()["harnesses"][0].clone(); 200]);
    let text = large.to_string();
    assert!(
        text.len() > 64 * 1024 && text.len() < PS_STDOUT_LIMIT,
        "{}",
        text.len()
    );
    rig.plan(json!({"ps":{"stdout":text}}));
    assert_eq!(rig.cli().status(MACHINE).unwrap().len(), 200);
    for (step, code) in [
        (
            json!({"stdout":"x".repeat(PS_STDOUT_LIMIT + 1)}),
            "stdout_limit",
        ),
        (json!({"delay_ms":5000}), "timeout"),
        (
            json!({"exit":1,"stdout":"{\"error\":\"private account detail\"}"}),
            "status_failed",
        ),
        (
            json!({"stdout":"{\"harnesses\":7}"}),
            "status_invalid_reply",
        ),
    ] {
        let mut rig = Rig::new();
        if code == "timeout" {
            rig.config.read_timeout = Duration::from_millis(500);
        }
        rig.plan(json!({"ps":step}));
        let began = Instant::now();
        assert_eq!(rig.cli().status(MACHINE), Err(code.into()));
        assert!(began.elapsed() < Duration::from_secs(2));
        assert_eq!(rig.calls().len(), 1);
    }
    let rig = Rig::new();
    assert_eq!(rig.cli().status("office"), Err("invalid_machine".into()));
    assert!(rig.calls().is_empty());
}
