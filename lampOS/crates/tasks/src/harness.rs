//! Bounded adapter for the provisioned Harness CLI, pinned to the inspected
//! 0e6b16d contract. Call only after the caller has durably recorded its attempt.
//! A returned launch or transcript is never proof of task completion or speech.
//! No mutation is retried, and killing this CLI cannot cancel remote work.
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{self, Read},
    os::{
        fd::AsFd,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub const MAX_PROMPT_BYTES: usize = 1000;
pub const MAX_ANSWER_BYTES: usize = 16 * 1024;
pub const MAX_ASK_BYTES: usize = 4 * 1024;
const STDOUT_LIMIT: usize = 64 * 1024;
const STDERR_LIMIT: usize = 16 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const DRAIN_LIMIT: Duration = Duration::from_millis(100);
const REAP_LIMIT: Duration = Duration::from_millis(200);
const ENVIRONMENT: &[&str] = &[
    "HOME",
    "PATH",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TMP",
    "TEMP",
    "USER",
    "LOGNAME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    "ADAPTER_DATA_DIR",
    "HARNESS_AUTH_DIR",
    "PORT",
];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Codex,
    Claude,
}

impl EngineKind {
    fn argument(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub machine_id: String,
    pub project: String,
    pub engine: EngineKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    pub name: String,
    pub prompt: String,
    pub target: Target,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LaunchOutcome {
    Created { agent_id: String },
    Pending { creation_id: String },
    NotSent { code: String },
    Uncertain { code: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub agent_id: String,
    pub state: String,
    pub ask: Option<String>,
    pub answer: String,
    pub truncated: bool,
    pub tools: u64,
}

fn token(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn machine_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}

impl Target {
    pub fn validate(&self) -> Result<(), String> {
        if !machine_id(&self.machine_id) {
            return Err("invalid_machine".into());
        }
        let folder = self.project.starts_with('/') || self.project.starts_with("~/");
        let maximum = if folder { 512 } else { 128 };
        if self.project.is_empty()
            || self.project.len() > maximum
            || self.project.trim() != self.project
            || self.project.chars().any(char::is_control)
            || self.project.starts_with(['-', '@'])
            || (!folder && (self.project.starts_with(['.', '~']) || self.project.contains('/')))
        {
            return Err("invalid_project".into());
        }
        Ok(())
    }
}

impl Launch {
    pub fn validate(&self) -> Result<(), String> {
        self.target.validate()?;
        if !token(&self.name, 80) || !self.name.as_bytes()[0].is_ascii_alphanumeric() {
            return Err("invalid_name".into());
        }
        // The pinned --task parser refuses a value beginning with --. Do not
        // rewrite the task or silently select another CLI permission mode.
        if self.prompt.trim().is_empty()
            || self.prompt.len() > MAX_PROMPT_BYTES
            || self.prompt.starts_with("--")
            || self.prompt.contains('\0')
        {
            return Err("invalid_prompt".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CliConfig {
    pub executable: PathBuf,
    pub launch_timeout: Duration,
    pub read_timeout: Duration,
    pub environment: BTreeMap<String, OsString>,
    pub cancel: Option<Arc<AtomicBool>>,
    /// Call Harness as the person, without marking the caller as an agent
    /// (`--harness-as-person`). Off unless set: marked, Harness refuses to
    /// start a harness until the person lets agents start agents (seen on
    /// lamp-4ace on 2026-10-14). The person's spoken yes to each exact task
    /// is still required either way.
    pub as_person: bool,
}

impl CliConfig {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            launch_timeout: Duration::from_secs(60),
            read_timeout: Duration::from_secs(10),
            environment: ENVIRONMENT
                .iter()
                .filter_map(|name| std::env::var_os(name).map(|value| ((*name).into(), value)))
                .collect(),
            cancel: None,
            as_person: false,
        }
    }
}

pub struct Cli {
    config: CliConfig,
    caller_session: String,
    operation: Mutex<()>,
    poisoned: AtomicBool,
    executable: ExecutableIdentity,
}

fn invalid(code: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, code)
}

#[derive(PartialEq, Eq)]
struct ExecutableIdentity {
    dev: u64,
    ino: u64,
    bytes: u64,
    modified: i64,
    nanos: i64,
    mode: u32,
    uid: u32,
}
fn executable_identity(path: &Path) -> io::Result<ExecutableIdentity> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid("unavailable_executable"))?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
        || metadata.permissions().mode() & 0o6022 != 0
        || ![0, rustix::process::geteuid().as_raw()].contains(&metadata.uid())
    {
        return Err(invalid("invalid_executable"));
    }
    Ok(ExecutableIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        bytes: metadata.len(),
        modified: metadata.mtime(),
        nanos: metadata.mtime_nsec(),
        mode: metadata.mode(),
        uid: metadata.uid(),
    })
}

impl Cli {
    pub fn new(mut config: CliConfig, caller_session: String) -> io::Result<Self> {
        if !token(&caller_session, 80) {
            return Err(invalid("invalid_caller_session"));
        }
        if !config.executable.is_absolute()
            || config.executable.as_os_str().len() > 4096
            || config.executable.as_os_str().as_bytes().contains(&0)
        {
            return Err(invalid("invalid_executable"));
        }
        config.executable = config
            .executable
            .canonicalize()
            .map_err(|_| invalid("unavailable_executable"))?;
        let executable = executable_identity(&config.executable)?;
        if config.launch_timeout.is_zero()
            || config.launch_timeout > Duration::from_secs(60)
            || config.read_timeout.is_zero()
            || config.read_timeout > Duration::from_secs(10)
        {
            return Err(invalid("invalid_timeout"));
        }
        let mut total = 0usize;
        for (name, value) in &config.environment {
            total = total.saturating_add(name.len() + value.len());
            if !ENVIRONMENT.contains(&name.as_str())
                || value.len() > 4096
                || value.as_bytes().contains(&0)
            {
                return Err(invalid("invalid_environment"));
            }
        }
        if total > 32 * 1024 {
            return Err(invalid("invalid_environment"));
        }
        let home = config
            .environment
            .get("HOME")
            .ok_or_else(|| invalid("missing_home"))?;
        if !Path::new(home).is_absolute() {
            return Err(invalid("invalid_home"));
        }
        let path = config
            .environment
            .get("PATH")
            .ok_or_else(|| invalid("missing_path"))?;
        if path.is_empty() || std::env::split_paths(path).any(|path| !path.is_absolute()) {
            return Err(invalid("invalid_path"));
        }
        if let Some(port) = config.environment.get("PORT")
            && port
                .to_str()
                .and_then(|value| value.parse::<u16>().ok())
                .is_none_or(|port| port == 0)
        {
            return Err(invalid("invalid_port"));
        }
        Ok(Self {
            config,
            caller_session,
            operation: Mutex::new(()),
            poisoned: AtomicBool::new(false),
            executable,
        })
    }

    fn cancelled(&self) -> bool {
        self.config
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Acquire))
    }

    pub fn launch(&self, launch: &Launch) -> LaunchOutcome {
        let not_sent = |code: &str| LaunchOutcome::NotSent { code: code.into() };
        let uncertain = |code: &str| LaunchOutcome::Uncertain { code: code.into() };
        if let Err(code) = launch.validate() {
            return not_sent(&code);
        }
        let Ok(_guard) = self.operation.try_lock() else {
            return not_sent("busy");
        };
        let preflight = match self.run(
            &["machines".into(), "--json".into()],
            self.config.read_timeout,
        ) {
            Ok(reply) => reply,
            Err(error) => {
                return not_sent(&if error.code == "cancelled" {
                    "cancelled".into()
                } else {
                    format!("preflight_{}", error.code)
                });
            }
        };
        if !preflight.success {
            return not_sent("preflight_failed");
        }
        if let Err(code) = machines(&preflight.stdout, &launch.target.machine_id) {
            return not_sent(code);
        }
        // This is a preflight, not a reservation. Harness resolves the account
        // list again and can fall back to a name if the ID disappears meanwhile.
        // The machine is named with `--machine` (Harness 0.4.2); the `@id`
        // form before it is refused.
        let args = vec![
            "new".into(),
            launch.target.engine.argument().into(),
            "--machine".into(),
            launch.target.machine_id.clone().into(),
            launch.target.project.clone().into(),
            "--name".into(),
            launch.name.clone().into(),
            // Harness's default, approved by the owner on 2026-10-15: Claude
            // Code works in its own project folder without asking, and still
            // stops for anything risky. With `ask` it asked before every file
            // it wrote, with nobody at the computer to answer: a two-line poem
            // waited on "Do you want to create coffee_poem.txt?" while the
            // lamp said "still working".
            "--mode".into(),
            "auto".into(),
            "--json".into(),
            "--task".into(),
            launch.prompt.clone().into(),
        ];
        match self.run(&args, self.config.launch_timeout) {
            Ok(reply) if reply.success => parse_launch(&reply.stdout, launch),
            Ok(_) => uncertain("new_failed"),
            Err(error) if error.spawned => uncertain(error.code),
            Err(error) => not_sent(error.code),
        }
    }

    /// The agent's last turn, read on the machine it was started on.
    pub fn observe(&self, machine: &str, agent_id: &str) -> Result<Observation, String> {
        if !machine_id(machine) {
            return Err("invalid_machine".into());
        }
        if !uuid(agent_id) {
            return Err("invalid_agent_id".into());
        }
        let _guard = self.operation.try_lock().map_err(|_| "busy".to_string())?;
        let reply = self
            .run(
                &[
                    "show".into(),
                    "--machine".into(),
                    machine.into(),
                    agent_id.into(),
                    "--chars".into(),
                    "4000".into(),
                    "--json".into(),
                ],
                self.config.read_timeout,
            )
            .map_err(|error| error.code.to_string())?;
        if !reply.success {
            return Err("observe_failed".into());
        }
        parse_observation(&reply.stdout, agent_id).map_err(str::to_string)
    }

    fn run(&self, args: &[OsString], timeout: Duration) -> Result<Reply, RunError> {
        let error = |code, spawned| RunError { code, spawned };
        if self.poisoned.load(Ordering::Acquire) {
            return Err(error("cleanup_incomplete", false));
        }
        if self.cancelled() {
            return Err(error("cancelled", false));
        }
        if !executable_identity(&self.config.executable)
            .is_ok_and(|identity| identity == self.executable)
        {
            return Err(error("executable_changed", false));
        }
        let deadline = Instant::now() + timeout;
        let mut command = Command::new(&self.config.executable);
        command
            .args(args)
            .env_clear()
            .envs(&self.config.environment);
        if !self.config.as_person {
            command
                .env("AI_AGENT", "lampos")
                .env("HARNESS_AGENT_SESSION", &self.caller_session);
        }
        command
            .current_dir(&self.config.environment["HOME"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        if self.cancelled() {
            return Err(error("cancelled", false));
        }
        let child = command.spawn().map_err(|_| error("spawn_failed", false))?;
        let mut owned = OwnedChild::new(child);
        let result = self.capture(&mut owned, deadline);
        if !owned.finish() {
            self.poisoned.store(true, Ordering::Release);
            return Err(error("cleanup_incomplete", true));
        }
        result.map_err(|code| error(code, true))
    }

    fn capture(&self, owned: &mut OwnedChild, deadline: Instant) -> Result<Reply, &'static str> {
        let mut stdout = owned.child.stdout.take().ok_or("pipe_failed")?;
        let mut stderr = owned.child.stderr.take().ok_or("pipe_failed")?;
        nonblocking(&stdout).map_err(|_| "pipe_failed")?;
        nonblocking(&stderr).map_err(|_| "pipe_failed")?;
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut exit = None;
        let result = (|| loop {
            let now = Instant::now();
            if self.cancelled() {
                return Err("cancelled");
            }
            if now >= deadline {
                return Err("timeout");
            }
            if !stdout_done {
                stdout_done = drain(&mut stdout, &mut output, STDOUT_LIMIT, "stdout_limit")?;
            }
            if !stderr_done {
                stderr_done = drain(&mut stderr, &mut errors, STDERR_LIMIT, "stderr_limit")?;
            }
            if exit.is_none() {
                match waitid(
                    WaitId::Pid(owned.pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                ) {
                    Ok(Some(status)) => exit = Some((now, status.exit_status() == Some(0))),
                    Ok(None) => {}
                    Err(error) if error == rustix::io::Errno::INTR => {}
                    Err(_) => return Err("wait_failed"),
                }
            }
            if let Some((at, success)) = exit {
                if stdout_done && stderr_done {
                    owned.exited_with_closed_pipes = true;
                    return Ok(Reply {
                        success,
                        stdout: output,
                    });
                }
                if now.saturating_duration_since(at) >= DRAIN_LIMIT {
                    return Err("pipes_open");
                }
            }
            let mut descriptors = Vec::with_capacity(2);
            if !stdout_done {
                descriptors.push(PollFd::new(&stdout, PollFlags::IN));
            }
            if !stderr_done {
                descriptors.push(PollFd::new(&stderr, PollFlags::IN));
            }
            let left = deadline
                .saturating_duration_since(Instant::now())
                .min(POLL_INTERVAL);
            let wait = Timespec::try_from(left).map_err(|_| "poll_failed")?;
            match poll(&mut descriptors, Some(&wait)) {
                Ok(_) => {}
                Err(error) if error == rustix::io::Errno::INTR => {}
                Err(_) => return Err("poll_failed"),
            }
        })();
        if result.is_err() {
            owned.tail = Some(PipeTail {
                stdout,
                stderr,
                stdout_done,
                stderr_done,
            });
        }
        result
    }
}

fn nonblocking(fd: &impl AsFd) -> io::Result<()> {
    fcntl_setfl(fd, fcntl_getfl(fd)? | OFlags::NONBLOCK).map_err(Into::into)
}

fn drain(
    reader: &mut impl Read,
    bytes: &mut Vec<u8>,
    limit: usize,
    overflow: &'static str,
) -> Result<bool, &'static str> {
    // Bound work per pipe per iteration as well as total bytes. A chatty child
    // cannot starve cancellation or the other pipe.
    let mut chunk = [0; 4096];
    match reader.read(&mut chunk) {
        Ok(0) => Ok(true),
        Ok(count) => {
            if bytes.len() + count > limit {
                return Err(overflow);
            }
            bytes.extend_from_slice(&chunk[..count]);
            Ok(false)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(false)
        }
        Err(_) => Err("pipe_read_failed"),
    }
}

struct Reply {
    success: bool,
    stdout: Vec<u8>,
}
struct RunError {
    code: &'static str,
    spawned: bool,
}

struct PipeTail {
    stdout: ChildStdout,
    stderr: ChildStderr,
    stdout_done: bool,
    stderr_done: bool,
}
impl PipeTail {
    fn drain_once(&mut self) -> bool {
        // Cleanup discards at most one chunk per pipe per iteration. The
        // absolute cleanup deadline also bounds a descendant that keeps writing.
        let mut chunk = [0; 4096];
        if !self.stdout_done {
            self.stdout_done = matches!(self.stdout.read(&mut chunk), Ok(0));
        }
        if !self.stderr_done {
            self.stderr_done = matches!(self.stderr.read(&mut chunk), Ok(0));
        }
        self.stdout_done && self.stderr_done
    }
}

struct OwnedChild {
    child: Child,
    pid: Pid,
    cleaned: bool,
    exited_with_closed_pipes: bool,
    tail: Option<PipeTail>,
}
impl OwnedChild {
    fn new(child: Child) -> Self {
        let pid = Pid::from_child(&child);
        Self {
            child,
            pid,
            cleaned: false,
            exited_with_closed_pipes: false,
            tail: None,
        }
    }
    fn finish(&mut self) -> bool {
        if self.cleaned {
            return true;
        }
        // NOWAIT above reserves the child PID until this group signal: a
        // reaped PID must not be reused as an unrelated process-group target.
        let group = kill_process_group(self.pid, Signal::KILL);
        let _ = self.child.kill();
        let until = Instant::now() + REAP_LIMIT;
        let mut reaped = false;
        let mut pipes_closed = self.exited_with_closed_pipes;
        loop {
            if !reaped {
                match self.child.try_wait() {
                    Ok(Some(_)) => reaped = true,
                    Ok(None) => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            if let Some(tail) = &mut self.tail {
                pipes_closed = tail.drain_once();
            }
            if reaped && pipes_closed {
                break;
            }
            if Instant::now() >= until {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        self.exited_with_closed_pipes = reaped && pipes_closed;
        self.cleaned = true;
        // Darwin's killpg1 skips zombies and returns EPERM when a group has
        // no live members. Accept that case only after observing exit and
        // both pipe EOFs; a live or pipe-holding child still fails closed.
        let group_stopped = matches!(group, Ok(()) | Err(rustix::io::Errno::SRCH))
            || (cfg!(target_os = "macos")
                && self.exited_with_closed_pipes
                && group == Err(rustix::io::Errno::PERM));
        reaped && pipes_closed && group_stopped
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.finish();
        }
    }
}

#[derive(Deserialize)]
struct Machine {
    #[serde(rename = "machineId")]
    machine_id: String,
    current: bool,
}

fn machines(bytes: &[u8], target: &str) -> Result<(), &'static str> {
    let text = std::str::from_utf8(bytes).map_err(|_| "preflight_invalid_reply")?;
    let mut ids = BTreeSet::new();
    let mut current = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            return Err("preflight_invalid_reply");
        }
        if ids.len() >= 256 {
            return Err("preflight_invalid_reply");
        }
        let machine: Machine = serde_json::from_str(line).map_err(|_| "preflight_invalid_reply")?;
        if !machine_id(&machine.machine_id) || !ids.insert(machine.machine_id) {
            return Err("preflight_invalid_reply");
        }
        current += usize::from(machine.current);
        if current > 1 {
            return Err("preflight_invalid_reply");
        }
    }
    if ids.contains(target) {
        Ok(())
    } else {
        Err("machine_unavailable")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewReply {
    ok: bool,
    machine_id: Option<String>,
    pending: Option<bool>,
    existing: Option<bool>,
    agent: Option<CreatedAgent>,
    creation_id: Option<String>,
    name: Option<String>,
    error: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedAgent {
    id: String,
    name: String,
    engine: EngineKind,
    permission_mode: String,
    bypass_permission: Value,
}

fn parse_launch(bytes: &[u8], launch: &Launch) -> LaunchOutcome {
    let uncertain = |code: &str| LaunchOutcome::Uncertain { code: code.into() };
    let Ok(reply) = serde_json::from_slice::<NewReply>(bytes) else {
        return uncertain("invalid_reply");
    };
    if !reply.ok {
        return uncertain("new_failed");
    }
    if reply.error.is_some() {
        return uncertain("invalid_reply");
    }
    if reply.machine_id.as_deref() != Some(&launch.target.machine_id) {
        return uncertain("machine_mismatch");
    }
    if reply.existing == Some(true) {
        return uncertain("existing_unbound");
    }
    if reply.pending == Some(true) {
        if reply.agent.is_some() || reply.name.as_deref() != Some(&launch.name) {
            return uncertain("invalid_reply");
        }
        let Some(id) = reply
            .creation_id
            .filter(|id| id.len() >= 16 && token(id, 96))
        else {
            return uncertain("invalid_reply");
        };
        return LaunchOutcome::Pending { creation_id: id };
    }
    if reply.creation_id.is_some() {
        return uncertain("invalid_reply");
    }
    let Some(agent) = reply.agent else {
        return uncertain("invalid_reply");
    };
    if !uuid(&agent.id)
        || agent.name != launch.name
        || agent.engine != launch.target.engine
        || agent.permission_mode != "ask"
        || !(agent.bypass_permission.is_null() || agent.bypass_permission == false)
    {
        return uncertain("invalid_reply");
    }
    LaunchOutcome::Created { agent_id: agent.id }
}

#[derive(Deserialize)]
struct ShowReply {
    id: String,
    state: String,
    ask: Value,
    answer: String,
    truncated: bool,
    tools: u64,
    ok: Option<bool>,
    error: Option<Value>,
}

fn parse_observation(bytes: &[u8], expected: &str) -> Result<Observation, &'static str> {
    let reply: ShowReply = serde_json::from_slice(bytes).map_err(|_| "invalid_observation")?;
    if reply.ok == Some(false) || reply.error.is_some() {
        return Err("invalid_observation");
    }
    if reply.id != expected {
        return Err("agent_mismatch");
    }
    if ![
        "working",
        "idle",
        "needs-input",
        "stopped",
        "offline",
        "unknown",
        "installing",
        "starting",
        "failed",
    ]
    .contains(&reply.state.as_str())
        || reply.answer.len() > MAX_ANSWER_BYTES
        || reply.answer.contains('\0')
        || reply.tools > 1_000_000
    {
        return Err("invalid_observation");
    }
    let ask = match reply.ask {
        Value::Null => None,
        Value::String(ask) if ask.len() <= MAX_ASK_BYTES && !ask.contains('\0') => Some(ask),
        _ => return Err("invalid_observation"),
    };
    Ok(Observation {
        agent_id: reply.id,
        state: reply.state,
        ask,
        answer: reply.answer,
        truncated: reply.truncated,
        tools: reply.tools,
    })
}
