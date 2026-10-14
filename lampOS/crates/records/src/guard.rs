//! Fail-closed identification of live voice output directories.
//!
//! The caller must inventory and pin candidate directories BEFORE this call,
//! and must never add or revisit candidates during deletion. The trusted voice
//! producer exclusively creates NEW_OUTPUT, so a later start cannot acquire an
//! existing candidate. This is not protection against arbitrary privileged
//! writers or a producer that changes that creation contract.

use std::{collections::BTreeSet, io, path::Path};

/// Return protected direct-child session names, or refuse if ownership cannot
/// be established. One successful call protects one fixed, pinned inventory;
/// retain its entire returned set throughout that pass. Never reuse its result
/// with a refreshed inventory. No environment/config contents are inspected.
pub fn live_sessions(unit: &str, root: &Path) -> io::Result<BTreeSet<String>> {
    #[cfg(target_os = "linux")]
    {
        implementation::live(unit, root)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (unit, root);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "live session protection requires Linux systemd and host procfs",
        ))
    }
}

#[cfg(any(target_os = "linux", test))]
mod implementation {
    use rustix::fs::{Dir, Mode, OFlags, openat, readlinkat_raw};
    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::{OsStr, OsString},
        fs::{self, File},
        io::{self, Read},
        os::{
            fd::{AsFd, OwnedFd},
            unix::ffi::{OsStrExt, OsStringExt},
        },
        path::{Component, Path, PathBuf},
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    const PROPERTIES: &str = "Id,LoadState,ActiveState,SubState,InvocationID,MainPID,ControlPID,Job,ExecStart,ControlGroup,NeedDaemonReload,Type";
    const MAX_PROCESSES: usize = 65_536;
    const MAX_PROCESS_BYTES: usize = 65_536;
    const MAX_FDS: usize = 65_536;
    const MAX_LINK_BYTES: usize = 4096;
    const MAX_SHOW_BYTES: usize = 65_536;
    const TOTAL_BUDGET: Duration = Duration::from_secs(10);
    const COMMAND_BUDGET: Duration = Duration::from_secs(2);

    fn refused(message: &'static str) -> io::Error {
        io::Error::other(message)
    }

    fn check_deadline(deadline: Instant) -> io::Result<()> {
        if Instant::now() >= deadline {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "live session protection deadline exceeded",
            ))
        } else {
            Ok(())
        }
    }

    fn session_id(value: &str) -> bool {
        value.len() == 32
            && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            && value.bytes().any(|byte| byte != b'0')
    }

    fn valid_unit(unit: &str) -> bool {
        unit.len() > ".service".len()
            && unit.len() <= 255
            && unit.ends_with(".service")
            && !unit.starts_with('-')
            && !unit.ends_with("@.service")
            && unit
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.@:-".contains(&byte))
    }

    fn validate_root(root: &Path) -> io::Result<()> {
        if !root.is_absolute()
            || root.parent().is_none()
            || root.to_str().is_none()
            || root != fs::canonicalize(root)?
            || !root.is_dir()
        {
            return Err(refused(
                "session root is not an absolute canonical directory",
            ));
        }
        Ok(())
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Snapshot {
        fields: BTreeMap<String, String>,
        invocation: String,
        main_pid: u32,
        executable: PathBuf,
        control_group: String,
    }

    impl Snapshot {
        fn parse(bytes: &[u8], unit: &str, root: &Path) -> io::Result<Self> {
            if !valid_unit(unit) || bytes.len() > MAX_SHOW_BYTES {
                return Err(refused("invalid unit or oversized manager response"));
            }
            let text =
                std::str::from_utf8(bytes).map_err(|_| refused("manager response is not UTF-8"))?;
            let expected: BTreeSet<_> = PROPERTIES.split(',').collect();
            let mut fields = BTreeMap::new();
            for line in text.lines().filter(|line| !line.is_empty()) {
                let (key, value) = line
                    .split_once('=')
                    .ok_or_else(|| refused("malformed manager property"))?;
                if !expected.contains(key)
                    || fields.insert(key.to_owned(), value.to_owned()).is_some()
                {
                    return Err(refused("unknown or duplicate manager property"));
                }
            }
            if fields.len() != expected.len() {
                return Err(refused("missing manager property"));
            }
            let get = |key: &str| fields.get(key).map(String::as_str).unwrap_or_default();
            if get("Id") != unit
                || get("LoadState") != "loaded"
                || get("Type") != "exec"
                || get("NeedDaemonReload") != "no"
            {
                return Err(refused(
                    "unit identity or loaded configuration is unverified",
                ));
            }
            if !matches!(
                get("ActiveState"),
                "active" | "reloading" | "activating" | "deactivating" | "inactive" | "failed"
            ) || get("SubState").is_empty()
            {
                return Err(refused("unknown service state"));
            }
            let invocation = get("InvocationID");
            if !session_id(invocation) || invocation.bytes().any(|byte| byte.is_ascii_uppercase()) {
                return Err(refused("manager did not provide a valid invocation ID"));
            }
            let main_pid = parse_pid(get("MainPID"), true)?;
            let _ = parse_pid(get("ControlPID"), true)?;
            // systemctl v255 prints an empty Job value for ID zero. Accept
            // numeric zero too, but never mistake arbitrary text for no job.
            if !get("Job").is_empty() {
                let _ = parse_pid(get("Job"), true)?;
            }
            if main_pid == 0 && get("ActiveState") == "active" {
                return Err(refused("active voice service has no main process"));
            }
            let control_group = get("ControlGroup");
            if (!control_group.is_empty() && !control_group.starts_with('/'))
                || control_group == "/"
                || Path::new(control_group)
                    .components()
                    .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
                || (main_pid != 0 && control_group.is_empty())
            {
                return Err(refused("invalid service control group"));
            }
            let executable = loaded_executable(get("ExecStart"), root)?;
            Ok(Self {
                invocation: invocation.to_owned(),
                main_pid,
                executable,
                control_group: control_group.to_owned(),
                fields,
            })
        }
    }

    fn parse_pid(value: &str, zero_allowed: bool) -> io::Result<u32> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(refused("invalid process identifier"));
        }
        let pid = value
            .parse::<u32>()
            .map_err(|_| refused("invalid process identifier"))?;
        if pid > i32::MAX as u32 || (!zero_allowed && pid == 0) {
            return Err(refused("invalid process identifier"));
        }
        Ok(pid)
    }

    // systemctl's ExecStart rendering is not a general shell language. Accept
    // only its unambiguous, single-command form used by the deployed unit.
    fn loaded_executable(value: &str, root: &Path) -> io::Result<PathBuf> {
        let value = value
            .strip_prefix("{ path=")
            .and_then(|value| value.strip_suffix(" }"))
            .ok_or_else(|| refused("unsupported loaded ExecStart format"))?;
        let (binary, rest) = value
            .split_once(" ; argv[]=")
            .ok_or_else(|| refused("loaded ExecStart has no argv"))?;
        let (arguments, status) = rest
            .split_once(" ; ")
            .ok_or_else(|| refused("unsupported loaded ExecStart argv"))?;
        if status.contains("argv[]=") || status.contains("{ path=") {
            return Err(refused(
                "multiple loaded ExecStart commands are unsupported",
            ));
        }
        let arguments: Vec<_> = arguments.split_ascii_whitespace().collect();
        let expected = format!("{}/${{INVOCATION_ID}}", root.display());
        let executable = PathBuf::from(binary);
        if !executable.is_absolute()
            || executable.file_name() != Some(OsStr::new("lamp-voice"))
            || arguments.len() < 4
            || arguments[0] != binary
            || arguments[1] != "serve"
            || !Path::new(arguments[2]).is_absolute()
            || arguments[3] != expected
            || arguments.iter().any(|argument| {
                argument
                    .bytes()
                    .any(|byte| matches!(byte, b'\\' | b'\'' | b'"' | b';'))
            })
        {
            return Err(refused(
                "loaded voice command does not use this invocation root",
            ));
        }
        Ok(executable)
    }

    fn show_command(unit: &str) -> io::Result<Command> {
        if !valid_unit(unit) {
            return Err(refused("an explicit service unit name is required"));
        }
        let mut command = Command::new("/usr/bin/systemctl");
        command
            .env_clear()
            .env("LC_ALL", "C")
            .env("SYSTEMD_COLORS", "0")
            .args([
                "--system",
                "--no-pager",
                "--no-ask-password",
                "show",
                "--all",
            ])
            .arg(format!("--property={PROPERTIES}"))
            .arg(unit);
        Ok(command)
    }

    struct Reap {
        child: Child,
        reaped: bool,
    }

    impl Drop for Reap {
        fn drop(&mut self) {
            if !self.reaped {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }

    // No reader threads or shell. Nonblocking stdout is drained with a cap;
    // every error/timeout kills and reaps the command. As with any local I/O,
    // an uninterruptible kernel wait can exceed the userspace deadline.
    fn run_bounded(command: &mut Command, deadline: Instant) -> io::Result<Vec<u8>> {
        check_deadline(deadline)?;
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| refused("cannot start manager query"))?;
        let mut child = Reap {
            child,
            reaped: false,
        };
        let mut stdout = child
            .child
            .stdout
            .take()
            .ok_or_else(|| refused("manager query has no output pipe"))?;
        let flags = rustix::fs::fcntl_getfl(&stdout)?;
        rustix::fs::fcntl_setfl(&stdout, flags | OFlags::NONBLOCK)?;
        let mut output = Vec::new();
        let mut status = None;
        loop {
            check_deadline(deadline)?;
            let mut buffer = [0u8; 4096];
            let eof = loop {
                match stdout.read(&mut buffer) {
                    Ok(0) => break true,
                    Ok(count) => {
                        if output.len() + count > MAX_SHOW_BYTES {
                            return Err(refused("manager query output exceeds its limit"));
                        }
                        output.extend_from_slice(&buffer[..count]);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break false,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => return Err(refused("cannot read manager query output")),
                }
                check_deadline(deadline)?;
            };
            if status.is_none() {
                status = child.child.try_wait()?;
                child.reaped = status.is_some();
            }
            if eof && let Some(status) = status {
                return if status.success() {
                    Ok(output)
                } else {
                    Err(refused("manager query did not succeed"))
                };
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn directory_at(fd: impl AsFd, name: &OsStr) -> io::Result<OwnedFd> {
        Ok(openat(
            fd,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?)
    }

    fn read_at(fd: impl AsFd, name: &str, limit: usize) -> io::Result<Vec<u8>> {
        let descriptor = openat(
            fd,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let mut bytes = Vec::new();
        File::from(descriptor)
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err(refused("process metadata exceeds its limit"));
        }
        Ok(bytes)
    }

    fn link_at(fd: impl AsFd, name: &OsStr) -> io::Result<PathBuf> {
        let mut bytes = [0u8; MAX_LINK_BYTES];
        let count = readlinkat_raw(fd, name, &mut bytes[..])?;
        if count == bytes.len() {
            return Err(refused("process link exceeds its limit"));
        }
        Ok(PathBuf::from(OsString::from_vec(bytes[..count].to_vec())))
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Identity {
        pid: u32,
        start: u64,
        kernel_thread: bool,
        zombie: bool,
    }

    fn identity(bytes: &[u8], expected_pid: u32) -> io::Result<Identity> {
        // comm is parenthesized and may itself contain spaces or ')'. Fields
        // following the LAST ')' have the stable proc stat field positions.
        let close = bytes
            .iter()
            .rposition(|byte| *byte == b')')
            .ok_or_else(|| refused("invalid process stat"))?;
        let open = bytes
            .iter()
            .position(|byte| *byte == b'(')
            .ok_or_else(|| refused("invalid process stat"))?;
        let prefix =
            std::str::from_utf8(&bytes[..open]).map_err(|_| refused("invalid process stat"))?;
        let pid = parse_pid(prefix.trim(), false)?;
        let rest = std::str::from_utf8(&bytes[close + 1..])
            .map_err(|_| refused("invalid process stat"))?;
        let fields: Vec<_> = rest.split_ascii_whitespace().collect();
        if pid != expected_pid || fields.len() < 20 {
            return Err(refused("process identity changed or stat is truncated"));
        }
        let flags = fields[6]
            .parse::<u64>()
            .map_err(|_| refused("invalid process flags"))?;
        let start = fields[19]
            .parse::<u64>()
            .map_err(|_| refused("invalid process start time"))?;
        Ok(Identity {
            pid,
            start,
            kernel_thread: flags & 0x20_0000 != 0,
            zombie: fields[0] == "Z" || fields[0] == "X",
        })
    }

    fn arguments(bytes: &[u8]) -> io::Result<Vec<OsString>> {
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        if !bytes.ends_with(&[0]) {
            return Err(refused("process arguments are truncated"));
        }
        let arguments: Vec<_> = bytes[..bytes.len() - 1]
            .split(|byte| *byte == 0)
            .map(|word| OsString::from_vec(word.to_vec()))
            .collect();
        if arguments.len() > 1024 || arguments[0].is_empty() {
            return Err(refused("process arguments are ambiguous"));
        }
        Ok(arguments)
    }

    fn executable_path(path: PathBuf) -> PathBuf {
        let bytes = path.as_os_str().as_bytes();
        PathBuf::from(OsStr::from_bytes(
            bytes.strip_suffix(b" (deleted)").unwrap_or(bytes),
        ))
    }

    fn in_cgroup(bytes: &[u8], group: &str) -> io::Result<bool> {
        let text =
            std::str::from_utf8(bytes).map_err(|_| refused("invalid process control group"))?;
        let mut found = false;
        for line in text.lines() {
            let (_, rest) = line
                .split_once(':')
                .ok_or_else(|| refused("invalid process control group"))?;
            let (_, path) = rest
                .split_once(':')
                .ok_or_else(|| refused("invalid process control group"))?;
            if !group.is_empty()
                && (path == group
                    || path
                        .strip_prefix(group)
                        .is_some_and(|suffix| suffix.starts_with('/')))
            {
                found = true;
            }
        }
        Ok(found)
    }

    fn resolve_output(path: &Path, cwd: &Path) -> io::Result<PathBuf> {
        if path.as_os_str().as_bytes().len() > MAX_LINK_BYTES {
            return Err(refused("voice output path exceeds its limit"));
        }
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            if !cwd.is_absolute() {
                return Err(refused("voice working directory is not absolute"));
            }
            cwd.join(path)
        };
        // A new session may not yet exist. Resolve the nearest existing parent
        // so an alias to the configured root cannot hide a live session.
        let mut parent = absolute.as_path();
        let mut missing = Vec::new();
        loop {
            match fs::canonicalize(parent) {
                Ok(mut resolved) => {
                    for name in missing.into_iter().rev() {
                        resolved.push(name);
                    }
                    return Ok(resolved);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if missing.len() >= 64 {
                        return Err(refused("voice output path has too many missing ancestors"));
                    }
                    let name = parent
                        .file_name()
                        .ok_or_else(|| refused("voice output path cannot be resolved"))?;
                    missing.push(name);
                    parent = parent
                        .parent()
                        .ok_or_else(|| refused("voice output path cannot be resolved"))?;
                }
                Err(_) => return Err(refused("voice output path cannot be resolved")),
            }
        }
    }

    fn protect_path(path: &Path, root: &Path, sessions: &mut BTreeSet<String>) -> io::Result<()> {
        let Ok(relative) = path.strip_prefix(root) else {
            return Ok(());
        };
        let Some(Component::Normal(name)) = relative.components().next() else {
            return Err(refused("live process references the whole sessions root"));
        };
        let name = name
            .to_str()
            .ok_or_else(|| refused("live process uses an unknown session name"))?;
        if !session_id(name) {
            return Err(refused("live process uses an unknown session name"));
        }
        // Preserve the actual pathname's case, unlike a normalized UUID parser.
        sessions.insert(name.to_owned());
        Ok(())
    }

    fn voice_paths(arguments: &[OsString]) -> io::Result<Vec<&Path>> {
        let command = arguments
            .get(1)
            .and_then(|word| word.to_str())
            .ok_or_else(|| refused("voice process command is ambiguous"))?;
        let output = match command {
            "serve" => Some(3),
            "directed" => Some(4),
            "directed-fixture" => Some(5),
            "camera-inspect" => Some(4),
            "provider-check" => None,
            "worker" => {
                let role = arguments.get(2).and_then(|word| word.to_str());
                if arguments.len() < 6
                    || !matches!(
                        role,
                        Some(
                            "capture"
                                | "speaker"
                                | "provider"
                                | "camera"
                                | "ring"
                                | "privacy"
                                | "probe"
                        )
                    )
                {
                    return Err(refused("unknown voice worker command"));
                }
                let mut paths = Vec::new();
                if matches!(role, Some("capture" | "speaker")) {
                    for (index, value) in arguments.iter().enumerate().skip(6) {
                        if value == "--diagnostics" {
                            let value = arguments
                                .get(index + 1)
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| refused("voice diagnostics path is missing"))?;
                            paths.push(Path::new(value));
                        }
                    }
                    if paths.len() > 1 {
                        return Err(refused("voice diagnostics path is ambiguous"));
                    }
                }
                return Ok(paths);
            }
            _ => return Err(refused("unknown voice process command")),
        };
        output
            .map(|index| {
                arguments
                    .get(index)
                    .filter(|value| !value.is_empty())
                    .map(|value| vec![Path::new(value)])
                    .ok_or_else(|| refused("voice process output argument is missing"))
            })
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    struct Scan {
        sessions: BTreeSet<String>,
        main_found: bool,
        fds: usize,
    }

    fn inspect_fds(
        process: &OwnedFd,
        root: &Path,
        scan: &mut Scan,
        deadline: Instant,
    ) -> io::Result<()> {
        let fd_directory = directory_at(process, OsStr::new("fd"))?;
        for entry in Dir::read_from(&fd_directory)? {
            check_deadline(deadline)?;
            let entry = entry?;
            let name = entry.file_name().to_bytes();
            if matches!(name, b"." | b"..") {
                continue;
            }
            scan.fds += 1;
            if scan.fds > MAX_FDS {
                return Err(refused("process descriptor inventory exceeds its limit"));
            }
            let path = match link_at(&fd_directory, OsStr::from_bytes(name)) {
                Ok(path) => executable_path(path),
                // A descriptor can close between readdir and readlink. Its
                // disappearance cannot keep a file alive; argv protection also
                // covers the writer opening descriptors during startup.
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(_) => return Err(refused("cannot inspect live process descriptors")),
            };
            if path.is_absolute() {
                protect_path(&path, root, &mut scan.sessions)?;
            }
        }
        Ok(())
    }

    fn inspect_process(
        process: &OwnedFd,
        pid: u32,
        snapshot: &Snapshot,
        root: &Path,
        scan: &mut Scan,
        deadline: Instant,
    ) -> io::Result<()> {
        let before = identity(&read_at(process, "stat", 8192)?, pid)?;
        let argv_bytes = read_at(process, "cmdline", MAX_PROCESS_BYTES)?;
        if before.zombie || before.kernel_thread {
            if pid == snapshot.main_pid {
                return Err(refused("voice main process is not live"));
            }
            return Ok(());
        }
        let argv = arguments(&argv_bytes)?;
        if argv.is_empty() {
            return Err(refused("live process has no inspectable arguments"));
        }
        let executable = executable_path(link_at(process, OsStr::new("exe"))?);
        let groups = read_at(process, "cgroup", 8192)?;
        let same_group = in_cgroup(&groups, &snapshot.control_group)?;
        let named_voice = executable.file_name() == Some(OsStr::new("lamp-voice"))
            || Path::new(&argv[0]).file_name() == Some(OsStr::new("lamp-voice"))
            || executable == snapshot.executable;
        // Many independent lamp components have a `serve` command. Only an
        // identified voice executable gets the voice argv interpretation.
        let relevant = named_voice || same_group || pid == snapshot.main_pid;
        let mut direct_reference = false;
        for argument in &argv {
            let path = Path::new(argument);
            if path.is_absolute() && path.starts_with(root) {
                protect_path(path, root, &mut scan.sessions)?;
                direct_reference = true;
            }
        }
        if relevant {
            if !named_voice {
                return Err(refused("unrecognized process shares voice ownership"));
            }
            let cwd = executable_path(link_at(process, OsStr::new("cwd"))?);
            let paths = voice_paths(&argv)?;
            let mut resolved = Vec::new();
            for path in paths {
                let path = resolve_output(path, &cwd)?;
                protect_path(&path, root, &mut scan.sessions)?;
                resolved.push(path);
            }
            if pid == snapshot.main_pid {
                if executable != snapshot.executable
                    || argv.get(1) != Some(&OsString::from("serve"))
                    || resolved != [root.join(&snapshot.invocation)]
                    || !same_group
                {
                    return Err(refused(
                        "main process disagrees with manager invocation mapping",
                    ));
                }
                scan.main_found = true;
            }
        }
        if relevant || direct_reference {
            // A renamed manual binary or a reader can be protected through its
            // explicit root reference without assuming any particular argv
            // layout. Unrelated services are not parsed as voice processes.
            inspect_fds(process, root, scan, deadline)?;
        }
        let after = identity(&read_at(process, "stat", 8192)?, pid)?;
        if before != after
            || argv_bytes != read_at(process, "cmdline", MAX_PROCESS_BYTES)?
            || executable != executable_path(link_at(process, OsStr::new("exe"))?)
            || groups != read_at(process, "cgroup", 8192)?
        {
            return Err(refused(
                "process identity or command changed during protection check",
            ));
        }
        Ok(())
    }

    fn scan_processes(
        proc_root: &Path,
        snapshot: &Snapshot,
        root: &Path,
        deadline: Instant,
    ) -> io::Result<BTreeSet<String>> {
        let proc_root = directory_at(rustix::fs::CWD, proc_root.as_os_str())?;
        let mut scan = Scan {
            sessions: BTreeSet::from([snapshot.invocation.clone()]),
            main_found: snapshot.main_pid == 0,
            fds: 0,
        };
        let mut count = 0;
        for entry in Dir::read_from(&proc_root)? {
            check_deadline(deadline)?;
            let entry = entry?;
            let name = entry.file_name().to_bytes();
            if name.is_empty() || !name.iter().all(u8::is_ascii_digit) {
                continue;
            }
            count += 1;
            if count > MAX_PROCESSES {
                return Err(refused("process inventory exceeds its limit"));
            }
            let pid = parse_pid(
                std::str::from_utf8(name).map_err(|_| refused("invalid process directory name"))?,
                false,
            )?;
            let process = match directory_at(&proc_root, OsStr::from_bytes(name)) {
                Ok(process) => process,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(_) => return Err(refused("cannot pin a process directory")),
            };
            inspect_process(&process, pid, snapshot, root, &mut scan, deadline).map_err(
                |error| {
                    io::Error::new(
                        error.kind(),
                        format!("process {pid} protection is incomplete: {error}"),
                    )
                },
            )?;
        }
        if !scan.main_found {
            return Err(refused("manager main process was not found in host procfs"));
        }
        Ok(scan.sessions)
    }

    // The closures make ordering, restart races, and query failures testable
    // without systemctl, a Linux process tree, or a lamp.
    fn guard_with(
        mut snapshot: impl FnMut() -> io::Result<Snapshot>,
        mut inspect: impl FnMut(&Snapshot) -> io::Result<BTreeSet<String>>,
    ) -> io::Result<BTreeSet<String>> {
        let before = snapshot()?;
        let mut sessions = inspect(&before)?;
        // A finite diagnostic child can be born while /proc is enumerated and
        // outlive its coordinator. A second pass catches a surviving child
        // missed by the moving PID iterator. These trusted audio workers do
        // not spawn further record-writing processes. Keep both passes' sets.
        sessions.extend(inspect(&before)?);
        let after = snapshot()?;
        if before != after {
            return Err(refused("voice unit changed during protection check"));
        }
        sessions.insert(before.invocation);
        Ok(sessions)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn live(unit: &str, root: &Path) -> io::Result<BTreeSet<String>> {
        let deadline = Instant::now() + TOTAL_BUDGET;
        validate_root(root)?;
        if !rustix::process::geteuid().is_root() {
            return Err(refused(
                "live session protection requires root process visibility",
            ));
        }
        let proc_root = directory_at(rustix::fs::CWD, OsStr::new("/proc"))?;
        let init = directory_at(&proc_root, OsStr::new("1"))?;
        let own_name = std::process::id().to_string();
        let own = directory_at(&proc_root, OsStr::new(&own_name))?;
        if read_at(&init, "comm", 128)? != b"systemd\n"
            || link_at(&init, OsStr::new("ns/pid"))? != link_at(&own, OsStr::new("ns/pid"))?
        {
            return Err(refused("host systemd process namespace is not established"));
        }
        // Deployment must supply the host procfs. Matching PID namespaces does
        // not distinguish a whole systemd container from the physical host.
        guard_with(
            || {
                let mut command = show_command(unit)?;
                let bytes =
                    run_bounded(&mut command, deadline.min(Instant::now() + COMMAND_BUDGET))?;
                Snapshot::parse(&bytes, unit, root)
            },
            |snapshot| scan_processes(Path::new("/proc"), snapshot, root, deadline),
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            os::unix::fs::symlink,
            sync::atomic::{AtomicU64, Ordering},
        };

        const CURRENT: &str = "11111111111111111111111111111111";
        const OLD: &str = "22222222222222222222222222222222";
        const MANUAL: &str = "33333333333333333333333333333333";
        const BINARY: &str = "/opt/releases/example/lamp-voice";
        static NEXT: AtomicU64 = AtomicU64::new(0);

        struct Tree(PathBuf);

        impl Tree {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "lamp-records-guard-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(fs::canonicalize(path).unwrap())
            }

            fn directory(&self, name: &str) -> PathBuf {
                let path = self.0.join(name);
                fs::create_dir_all(&path).unwrap();
                path
            }
        }

        impl Drop for Tree {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        fn response(root: &Path) -> String {
            format!(
                "Id=lamp-voice.service\nLoadState=loaded\nActiveState=active\nSubState=running\nInvocationID={CURRENT}\nMainPID=123\nControlPID=0\nJob=\nExecStart={{ path={BINARY} ; argv[]={BINARY} serve /private/config.json {}/${{INVOCATION_ID}} --microphone mic2 ; ignore_errors=no ; pid=123 ; code=(null) ; status=0/0 }}\nControlGroup=/system.slice/lamp-voice.service\nNeedDaemonReload=no\nType=exec\n",
                root.display()
            )
        }

        fn snapshot(root: &Path) -> Snapshot {
            Snapshot::parse(response(root).as_bytes(), "lamp-voice.service", root).unwrap()
        }

        fn stat(pid: u32, start: u64, state: &str) -> String {
            let mut fields = vec!["0".to_owned(); 20];
            fields[0] = state.to_owned();
            fields[19] = start.to_string();
            format!("{pid} (lamp ) voice) {}\n", fields.join(" "))
        }

        fn process(tree: &Tree, pid: u32, words: &[&OsStr], group: &str) -> PathBuf {
            let path = tree.directory(&format!("proc/{pid}"));
            fs::write(path.join("stat"), stat(pid, 44, "S")).unwrap();
            let mut argv = Vec::new();
            for word in words {
                argv.extend_from_slice(word.as_bytes());
                argv.push(0);
            }
            fs::write(path.join("cmdline"), argv).unwrap();
            fs::write(path.join("cgroup"), format!("0::{group}\n")).unwrap();
            symlink(BINARY, path.join("exe")).unwrap();
            symlink(&tree.0, path.join("cwd")).unwrap();
            fs::create_dir(path.join("fd")).unwrap();
            path
        }

        fn serve(tree: &Tree, pid: u32, output: &Path, group: &str) -> PathBuf {
            process(
                tree,
                pid,
                &[
                    OsStr::new(BINARY),
                    OsStr::new("serve"),
                    OsStr::new("/private/config.json"),
                    output.as_os_str(),
                ],
                group,
            )
        }

        #[test]
        fn manager_identity_and_invocation_are_strict() {
            let root = Path::new("/sessions");
            let source = response(root);
            let parsed = snapshot(root);
            assert_eq!(parsed.invocation, CURRENT);
            for broken in [
                source.replace(CURRENT, "00000000000000000000000000000000"),
                source.replace(CURRENT, "../session"),
                source.replace("LoadState=loaded", "LoadState=not-found"),
                source.replace("MainPID=123", "MainPID=0"),
                source.replace("NeedDaemonReload=no", "NeedDaemonReload=yes"),
                source.replace("ActiveState=active", "ActiveState=new-unknown-state"),
                source.replace("Type=exec", "Type=forking"),
                source.replace("Job=\n", "Job=unknown\n"),
                source.replace("InvocationID=", "Missing="),
                format!("{source}MainPID=123\n"),
                source.replace("/${INVOCATION_ID}", "/fixed-directory"),
            ] {
                assert!(Snapshot::parse(broken.as_bytes(), "lamp-voice.service", root).is_err());
            }
        }

        #[test]
        fn retained_invocation_is_protected_in_all_known_states() {
            let root = Path::new("/sessions");
            for state in [
                "inactive",
                "failed",
                "activating",
                "deactivating",
                "reloading",
            ] {
                let text = response(root)
                    .replace("ActiveState=active", &format!("ActiveState={state}"))
                    .replace("MainPID=123", "MainPID=0");
                let parsed = Snapshot::parse(text.as_bytes(), "lamp-voice.service", root).unwrap();
                let result = guard_with(|| Ok(parsed.clone()), |_| Ok(BTreeSet::new())).unwrap();
                assert!(result.contains(CURRENT));
            }
        }

        #[test]
        fn any_manager_change_or_failure_refuses() {
            let first = snapshot(Path::new("/sessions"));
            let mut next = first.clone();
            next.invocation = OLD.to_owned();
            let mut calls = 0;
            assert!(
                guard_with(
                    || {
                        calls += 1;
                        Ok(if calls == 1 {
                            first.clone()
                        } else {
                            next.clone()
                        })
                    },
                    |_| Ok(BTreeSet::new()),
                )
                .is_err()
            );
            assert!(guard_with(|| Err(refused("unavailable")), |_| unreachable!()).is_err());
            assert!(guard_with(|| Ok(first.clone()), |_| Err(refused("unreadable"))).is_err());
        }

        #[test]
        fn both_process_passes_keep_their_protected_paths() {
            let snapshot = snapshot(Path::new("/sessions"));
            let mut calls = 0;
            let got = guard_with(
                || Ok(snapshot.clone()),
                |_| {
                    calls += 1;
                    Ok(BTreeSet::from([
                        if calls == 1 { OLD } else { MANUAL }.to_owned()
                    ]))
                },
            )
            .unwrap();
            assert_eq!(calls, 2);
            assert_eq!(
                got,
                BTreeSet::from([CURRENT.to_owned(), OLD.to_owned(), MANUAL.to_owned()])
            );
        }

        #[test]
        fn unit_argument_cannot_be_a_pattern_or_option() {
            for unit in [
                "*",
                "*.service",
                "--help",
                "../x.service",
                "lamp@.service",
                "",
            ] {
                assert!(show_command(unit).is_err());
            }
            let command = show_command("lamp-voice.service").unwrap();
            let arguments: Vec<_> = command.get_args().collect();
            assert!(arguments.contains(&OsStr::new("--system")));
            assert!(arguments.contains(&OsStr::new("--all")));
            assert_eq!(arguments.last(), Some(&OsStr::new("lamp-voice.service")));
        }

        #[test]
        fn scan_keeps_current_old_survivor_manual_and_diagnostic_child() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            let current = root.join(CURRENT);
            let old = tree.directory(&format!("sessions/{OLD}"));
            let manual = tree.directory(&format!("sessions/{MANUAL}"));
            // Current startup has not created its output yet.
            serve(&tree, 123, &current, "/system.slice/lamp-voice.service");
            let survivor = serve(&tree, 124, &old, "/system.slice/lamp-voice.service");
            symlink(old.join("metadata/events-0.jsonl"), survivor.join("fd/3")).unwrap();
            process(
                &tree,
                125,
                &[
                    OsStr::new(BINARY),
                    OsStr::new("directed"),
                    OsStr::new("/config"),
                    OsStr::new("60"),
                    manual.as_os_str(),
                ],
                "/user.slice/manual.scope",
            );
            let diagnostic = manual.join("capture-audio");
            process(
                &tree,
                126,
                &[
                    OsStr::new(BINARY),
                    OsStr::new("worker"),
                    OsStr::new("capture"),
                    OsStr::new("/tmp/ipc"),
                    OsStr::new(CURRENT),
                    OsStr::new(OLD),
                    OsStr::new("hw:1"),
                    OsStr::new("--diagnostics"),
                    diagnostic.as_os_str(),
                ],
                "/user.slice/manual.scope",
            );
            let got = scan_processes(
                &tree.0.join("proc"),
                &snapshot(&root),
                &root,
                Instant::now() + TOTAL_BUDGET,
            )
            .unwrap();
            assert_eq!(
                got,
                BTreeSet::from([CURRENT.to_owned(), OLD.to_owned(), MANUAL.to_owned()])
            );
        }

        #[test]
        fn relative_and_alias_outputs_are_resolved() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            let old = tree.directory(&format!("sessions/{OLD}"));
            symlink(&root, tree.0.join("alias")).unwrap();
            assert_eq!(
                resolve_output(Path::new(&format!("alias/{OLD}")), &tree.0).unwrap(),
                old
            );
            assert_eq!(
                resolve_output(&tree.0.join(format!("alias/{CURRENT}")), Path::new("/")).unwrap(),
                root.join(CURRENT)
            );
            validate_root(&root).unwrap();
            assert!(validate_root(&tree.0.join("alias")).is_err());
        }

        #[test]
        fn main_process_must_match_the_manager_and_be_present() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            tree.directory("proc");
            let check = || {
                scan_processes(
                    &tree.0.join("proc"),
                    &snapshot(&root),
                    &root,
                    Instant::now() + TOTAL_BUDGET,
                )
            };
            assert!(check().is_err());
            serve(
                &tree,
                123,
                &root.join(OLD),
                "/system.slice/lamp-voice.service",
            );
            assert!(check().is_err());
        }

        #[test]
        fn main_executable_mismatch_refuses_even_with_the_right_binary_name() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            let current = serve(
                &tree,
                123,
                &root.join(CURRENT),
                "/system.slice/lamp-voice.service",
            );
            fs::remove_file(current.join("exe")).unwrap();
            symlink("/opt/another-release/lamp-voice", current.join("exe")).unwrap();
            assert!(
                scan_processes(
                    &tree.0.join("proc"),
                    &snapshot(&root),
                    &root,
                    Instant::now() + TOTAL_BUDGET
                )
                .is_err()
            );
        }

        #[test]
        fn failed_service_still_protects_an_old_live_coordinator() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            let old = tree.directory(&format!("sessions/{OLD}"));
            serve(&tree, 124, &old, "/system.slice/lamp-voice.service");
            let text = response(&root)
                .replace("ActiveState=active", "ActiveState=failed")
                .replace("SubState=running", "SubState=failed")
                .replace("MainPID=123", "MainPID=0");
            let snapshot = Snapshot::parse(text.as_bytes(), "lamp-voice.service", &root).unwrap();
            let got = scan_processes(
                &tree.0.join("proc"),
                &snapshot,
                &root,
                Instant::now() + TOTAL_BUDGET,
            )
            .unwrap();
            assert_eq!(got, BTreeSet::from([CURRENT.to_owned(), OLD.to_owned()]));
        }

        #[test]
        fn unknown_same_cgroup_process_refuses() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            serve(
                &tree,
                123,
                &root.join(CURRENT),
                "/system.slice/lamp-voice.service",
            );
            let unknown = process(
                &tree,
                124,
                &[OsStr::new("/opt/unknown"), OsStr::new("worker")],
                "/system.slice/lamp-voice.service",
            );
            fs::remove_file(unknown.join("exe")).unwrap();
            symlink("/opt/unknown", unknown.join("exe")).unwrap();
            assert!(
                scan_processes(
                    &tree.0.join("proc"),
                    &snapshot(&root),
                    &root,
                    Instant::now() + TOTAL_BUDGET
                )
                .is_err()
            );
        }

        #[test]
        fn unrelated_serve_commands_do_not_use_the_voice_argument_layout() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            serve(
                &tree,
                123,
                &root.join(CURRENT),
                "/system.slice/lamp-voice.service",
            );
            let health = process(
                &tree,
                200,
                &[OsStr::new("/opt/lamp-health"), OsStr::new("serve")],
                "/system.slice/lamp-health.service",
            );
            fs::remove_file(health.join("exe")).unwrap();
            symlink("/opt/lamp-health", health.join("exe")).unwrap();
            let memory = process(
                &tree,
                201,
                &[
                    OsStr::new("/opt/lamp-memory"),
                    OsStr::new("serve"),
                    OsStr::new("--socket"),
                    OsStr::new("/run/lamp-memory/memory.sock"),
                ],
                "/system.slice/lamp-memory.service",
            );
            fs::remove_file(memory.join("exe")).unwrap();
            symlink("/opt/lamp-memory", memory.join("exe")).unwrap();
            let got = scan_processes(
                &tree.0.join("proc"),
                &snapshot(&root),
                &root,
                Instant::now() + TOTAL_BUDGET,
            )
            .unwrap();
            assert_eq!(got, BTreeSet::from([CURRENT.to_owned()]));
        }

        #[test]
        fn renamed_manual_binary_uses_direct_root_evidence_without_voice_parsing() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            let old = tree.directory(&format!("sessions/{OLD}"));
            serve(
                &tree,
                123,
                &root.join(CURRENT),
                "/system.slice/lamp-voice.service",
            );
            // This argv does not satisfy the voice `serve` layout. Its direct
            // path and open record still establish conservative protection.
            let manual = process(
                &tree,
                202,
                &[
                    OsStr::new("/opt/renamed-reader"),
                    OsStr::new("serve"),
                    old.as_os_str(),
                ],
                "/user.slice/manual.scope",
            );
            fs::remove_file(manual.join("exe")).unwrap();
            symlink("/opt/renamed-reader", manual.join("exe")).unwrap();
            symlink(old.join("metadata/events-0.jsonl"), manual.join("fd/3")).unwrap();
            let got = scan_processes(
                &tree.0.join("proc"),
                &snapshot(&root),
                &root,
                Instant::now() + TOTAL_BUDGET,
            )
            .unwrap();
            assert_eq!(got, BTreeSet::from([CURRENT.to_owned(), OLD.to_owned()]));
        }

        #[test]
        fn incomplete_process_evidence_and_unknown_workers_refuse() {
            let tree = Tree::new();
            let root = tree.directory("sessions");
            let current = serve(
                &tree,
                123,
                &root.join(CURRENT),
                "/system.slice/lamp-voice.service",
            );
            let check = || {
                scan_processes(
                    &tree.0.join("proc"),
                    &snapshot(&root),
                    &root,
                    Instant::now() + TOTAL_BUDGET,
                )
            };
            fs::remove_file(current.join("cmdline")).unwrap();
            assert!(check().is_err());
            assert!(
                voice_paths(&[
                    OsString::from(BINARY),
                    OsString::from("worker"),
                    OsString::from("unknown")
                ])
                .is_err()
            );
            assert!(voice_paths(&[OsString::from(BINARY), OsString::from("serve")]).is_err());
        }

        #[test]
        fn pid_reuse_and_parenthesized_comm_do_not_confuse_identity() {
            let before = identity(stat(42, 100, "S").as_bytes(), 42).unwrap();
            let reused = identity(stat(42, 101, "S").as_bytes(), 42).unwrap();
            assert_ne!(before, reused);
            assert!(identity(stat(43, 100, "S").as_bytes(), 42).is_err());
            assert!(arguments(b"lamp-voice\0serve").is_err());
        }

        #[test]
        fn pinned_proc_directory_does_not_switch_to_reused_name() {
            let tree = Tree::new();
            let path = tree.directory("proc/42");
            fs::write(path.join("stat"), stat(42, 100, "S")).unwrap();
            let pinned = directory_at(rustix::fs::CWD, path.as_os_str()).unwrap();
            fs::rename(&path, tree.0.join("old-process")).unwrap();
            fs::create_dir(&path).unwrap();
            fs::write(path.join("stat"), stat(42, 200, "S")).unwrap();
            let got = identity(&read_at(&pinned, "stat", 8192).unwrap(), 42).unwrap();
            assert_eq!(got.start, 100);
        }

        #[test]
        fn deadline_and_metadata_limits_refuse() {
            assert!(check_deadline(Instant::now()).is_err());
            let tree = Tree::new();
            fs::write(tree.0.join("large"), [1u8; 10]).unwrap();
            let fd = directory_at(rustix::fs::CWD, tree.0.as_os_str()).unwrap();
            assert!(read_at(&fd, "large", 5).is_err());
            let mut sessions = BTreeSet::new();
            assert!(
                protect_path(
                    Path::new("/sessions/manual"),
                    Path::new("/sessions"),
                    &mut sessions
                )
                .is_err()
            );
        }

        #[test]
        fn fake_manager_success_failure_timeout_and_output_cap() {
            let mut success = Command::new("/bin/sh");
            success.args(["-c", "printf 'known-output'"]);
            assert_eq!(
                run_bounded(&mut success, Instant::now() + COMMAND_BUDGET).unwrap(),
                b"known-output"
            );
            let mut failed = Command::new("/bin/sh");
            failed.args(["-c", "exit 3"]);
            assert!(run_bounded(&mut failed, Instant::now() + COMMAND_BUDGET).is_err());
            let mut stalled = Command::new("/bin/sh");
            stalled.args(["-c", "exec sleep 60"]);
            let started = Instant::now();
            assert_eq!(
                run_bounded(&mut stalled, started + Duration::from_millis(50))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::TimedOut
            );
            assert!(started.elapsed() < Duration::from_secs(2));
            let mut large = Command::new("/bin/sh");
            large.args(["-c", "exec /usr/bin/yes oversized"]);
            assert!(run_bounded(&mut large, Instant::now() + COMMAND_BUDGET).is_err());
        }
    }
}

#[cfg(all(test, not(target_os = "linux")))]
mod platform_tests {
    #[test]
    fn unsupported_platform_never_claims_an_empty_live_set() {
        assert_eq!(
            super::live_sessions("lamp-voice.service", std::path::Path::new("/sessions"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Unsupported
        );
    }
}
