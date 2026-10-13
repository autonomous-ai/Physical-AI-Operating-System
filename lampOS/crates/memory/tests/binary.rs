//! The built `lamp-memory` itself: what it refuses to start with, and what
//! a process that is killed leaves for the next one.
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::Duration,
};

const BINARY: &str = env!("CARGO_BIN_EXE_lamp-memory");
const USAGE: &str = "usage: lamp-memory --dir ABSOLUTE_DIR --socket ABSOLUTE_SOCKET_PATH [--keep-days N] [--utc-offset-minutes M]";

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("lm-{}-b-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(scratch: &Scratch, args: &[&str]) -> Output {
    Command::new(BINARY)
        .args(args)
        .current_dir(&scratch.0)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// A running `lamp-memory`, killed when the test lets go of it.
struct Process(Child);

impl Process {
    /// Start it with its log in a file: a pipe nobody reads would fill.
    fn start(scratch: &Scratch, dir: &str, socket: &str, log: &str) -> Self {
        let log = fs::File::create(scratch.0.join(log)).unwrap();
        let child = Command::new(BINARY)
            .args([
                "--dir",
                &scratch.path(dir),
                "--socket",
                &scratch.path(socket),
            ])
            .args(["--utc-offset-minutes", "420"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap();
        Self(child)
    }

    fn kill(mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// One request on a connection of its own, once somebody answers on the
/// socket. Waiting here is only for the process to have started.
fn ask(socket: &Path, request: Value) -> Value {
    let mut tries = 0;
    let stream = loop {
        match UnixStream::connect(socket) {
            Ok(stream) => break stream,
            Err(error) if tries == 1000 => panic!("nobody answers on the socket: {error}"),
            Err(_) => {
                tries += 1;
                thread::sleep(Duration::from_millis(10));
            }
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    reader
        .get_mut()
        .write_all(format!("{request}\n").as_bytes())
        .unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[test]
fn a_relative_path_or_an_unknown_option_gets_the_usage_line_and_exit_code_two() {
    let scratch = Scratch::new("usage");
    let (dir, socket) = (scratch.path("store"), scratch.path("m.sock"));
    for args in [
        &["--dir", "store", "--socket", &socket][..],
        &["--dir", &dir, "--socket", "m.sock"],
        &["--dir", &dir, "--socket", &socket, "--verbose"],
        &["--dir", &dir, "--socket", &socket, "--keep-days", "0"],
        &[
            "--dir",
            &dir,
            "--socket",
            &socket,
            "--utc-offset-minutes",
            "east",
        ],
        &["--dir", &dir],
        &[],
    ] {
        let output = run(&scratch, args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let said = String::from_utf8(output.stderr).unwrap();
        assert!(said.starts_with("lamp-memory: "), "{said}");
        assert!(said.ends_with(&format!("{USAGE}\n")), "{said}");
        assert!(output.stdout.is_empty());
    }
    // Nothing was made, by the paths given or by the names in them.
    assert_eq!(scratch.names(), [""; 0]);

    let help = run(&scratch, &["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8(help.stdout).unwrap().starts_with(USAGE));
    assert!(help.stderr.is_empty());
}

#[test]
fn a_directory_others_can_enter_is_refused_with_the_reason_and_exit_code_two() {
    let scratch = Scratch::new("open");
    let (dir, socket) = (scratch.path("store"), scratch.path("m.sock"));
    fs::create_dir(&dir).unwrap();
    for open in [0o750, 0o701, 0o777] {
        fs::set_permissions(&dir, fs::Permissions::from_mode(open)).unwrap();
        let output = run(&scratch, &["--dir", &dir, "--socket", &socket]);
        assert_eq!(output.status.code(), Some(2), "{open:o}");
        let said = String::from_utf8(output.stderr).unwrap();
        assert!(
            said.contains("can be entered by its group or by everyone"),
            "{said}"
        );
        assert!(said.contains(&format!("(mode {open:o})")), "{said}");
        assert_eq!(mode(Path::new(&dir)), open);
    }
    // Something that is not a directory at all.
    fs::write(scratch.0.join("file"), "").unwrap();
    let output = run(
        &scratch,
        &["--dir", &scratch.path("file"), "--socket", &socket],
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(scratch.names(), ["file", "store"]);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
}

#[test]
fn a_killed_process_leaves_a_store_the_next_one_loads_and_a_socket_it_replaces() {
    let scratch = Scratch::new("killed");
    let socket = scratch.0.join("m.sock");
    let at = 1_791_892_800_000u64;
    let first = Process::start(&scratch, "made/store", "m.sock", "log1");
    let added = ask(
        &socket,
        json!({"op":"append","at_unix_ms":at,"heard":"where is the orchid","said":"On the sill."}),
    );
    assert_eq!(added, json!({"ok":true,"id":1}));
    let kept = ask(
        &socket,
        json!({"op":"remember","at_unix_ms":at,"text":"the orchid wants no sun"}),
    );
    assert_eq!(kept, json!({"ok":true,"id":2}));
    // What it made is its owner's alone.
    assert_eq!(mode(&scratch.0.join("made/store")), 0o700);
    assert_eq!(mode(&scratch.0.join("made/store/everyone")), 0o700);
    assert_eq!(
        mode(&scratch.0.join("made/store/everyone/kept.jsonl")),
        0o600
    );
    assert_eq!(mode(&socket), 0o600);
    assert_eq!(
        fs::read_to_string(scratch.0.join("ready")).unwrap(),
        "ready\n"
    );

    // While it runs, a second one is turned away from its socket, and one
    // given another socket is turned away from its directory. Neither
    // leaves anything behind or disturbs the first.
    let dir = scratch.path("made/store");
    let same = run(
        &scratch,
        &["--dir", &dir, "--socket", &scratch.path("m.sock")],
    );
    assert_eq!(same.status.code(), Some(1));
    assert!(
        String::from_utf8(same.stderr)
            .unwrap()
            .contains("another process is answering")
    );
    fs::create_dir(scratch.0.join("other")).unwrap();
    let other = run(
        &scratch,
        &["--dir", &dir, "--socket", &scratch.path("other/m.sock")],
    );
    assert_eq!(other.status.code(), Some(1));
    assert!(
        String::from_utf8(other.stderr)
            .unwrap()
            .contains("is open in another lamp-memory")
    );
    assert_eq!(fs::read_dir(scratch.0.join("other")).unwrap().count(), 0);
    assert_eq!(ask(&socket, json!({"op":"status"}))["people"][0]["kept"], 1);

    // Killed, with no chance to tidy: the socket and the word stay, and
    // nobody answers.
    first.kill();
    assert_eq!(
        scratch.names(),
        ["log1", "m.sock", "made", "other", "ready"]
    );
    assert!(UnixStream::connect(&socket).is_err());
    // A power cut in the middle of a line, too.
    let journal = fs::read_dir(scratch.0.join("made/store/everyone"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_str().unwrap().contains("journal-"))
        .unwrap();
    let mut cut = fs::OpenOptions::new().append(true).open(&journal).unwrap();
    cut.write_all(b"{\"id\":3,\"at_unix_ms\":17918928").unwrap();
    drop(cut);

    let second = Process::start(&scratch, "made/store", "m.sock", "log2");
    let found = ask(&socket, json!({"op":"recall","query":"orchid"}));
    assert_eq!(
        found,
        json!({"ok":true,"entries":[
            {"kind":"exchange","id":1,"at_unix_ms":at,"heard":"where is the orchid","said":"On the sill."},
            {"kind":"kept","id":2,"at_unix_ms":at,"text":"the orchid wants no sun"}
        ]})
    );
    let next = ask(
        &socket,
        json!({"op":"append","at_unix_ms":at,"heard":"and the fern","said":"In the hall."}),
    );
    assert_eq!(next, json!({"ok":true,"id":3}));
    let status = ask(&socket, json!({"op":"status"}));
    assert_eq!(status["people"][0]["exchanges"], 2);
    assert_eq!(status["people"][0]["skipped_lines"], 1);
    assert_eq!(
        fs::read_to_string(scratch.0.join("ready")).unwrap(),
        "ready\n"
    );
    second.kill();

    // Each said what it found as it started and one line for each request,
    // with none of the text in any of them.
    let (log1, log2) = (
        fs::read_to_string(scratch.0.join("log1")).unwrap(),
        fs::read_to_string(scratch.0.join("log2")).unwrap(),
    );
    assert!(log1.contains(" people=0 skipped_lines=0 "), "{log1}");
    assert!(
        log1.contains("lamp-memory: append person=everyone ok id=1\n"),
        "{log1}"
    );
    assert!(
        log2.contains(" utc_offset_minutes=420 people=1 skipped_lines=1 "),
        "{log2}"
    );
    assert!(
        log2.contains("lamp-memory: recall person=everyone ok entries=2 bytes=54\n"),
        "{log2}"
    );
    assert_eq!((log1.lines().count(), log2.lines().count()), (4, 4));
    for log in [log1, log2] {
        assert!(!log.contains("orchid") && !log.contains("fern") && !log.contains("sill"));
    }
}
