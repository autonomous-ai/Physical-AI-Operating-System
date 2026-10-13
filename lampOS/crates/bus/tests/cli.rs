//! The `lamp-bus` binary, run the way a shell runs it, with the test as the
//! other end of the bus.
use lamp_bus::{Bus, Kind, Published};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::{Duration, Instant},
};

const BINARY: &str = env!("CARGO_BIN_EXE_lamp-bus");
/// Long enough for a process to have started, on a machine doing other
/// things as well.
const WAIT: Duration = Duration::from_secs(20);

/// A directory of one test's own, removed when the test ends however it
/// ends. The name is short: a socket's whole path has to fit in 103 bytes.
struct Root(PathBuf);

/// A number in base 36, the shortest a file name can carry it.
fn short(mut number: u32) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(char::from_digit(number % 36, 36).unwrap());
        number /= 36;
        if number == 0 {
            break digits.iter().rev().collect();
        }
    }
}

impl Root {
    fn new() -> Self {
        static MADE: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "c{}_{}",
            short(std::process::id()),
            short(MADE.fetch_add(1, Ordering::Relaxed))
        ));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    fn bus(&self) -> Bus {
        Bus::new(&self.0).unwrap()
    }

    /// The subscribers a publisher would find: a socket has its `.sock`
    /// name from the moment it can be sent to, and another before.
    fn sockets(&self, topic: &str) -> Vec<String> {
        fs::read_dir(self.0.join(topic).join("subs"))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .filter(|name| name.ends_with(".sock"))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A `lamp-bus` still running, killed if the test ends before it does.
struct Running(Option<Child>);

impl Running {
    fn start(root: &Root, arguments: &[&str]) -> Self {
        Self(Some(
            command(root, arguments)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ))
    }

    fn output(mut self) -> Output {
        self.0.take().unwrap().wait_with_output().unwrap()
    }

    fn kill(mut self) {
        let mut child = self.0.take().unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn command(root: &Root, arguments: &[&str]) -> Command {
    let mut command = Command::new(BINARY);
    command
        .arg("--root")
        .arg(&root.0)
        .args(arguments)
        .env_remove("LAMP_BUS_ROOT");
    command
}

fn run(root: &Root, arguments: &[&str]) -> Output {
    command(root, arguments)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

fn until(what: &str, mut there: impl FnMut() -> bool) {
    let started = Instant::now();
    while !there() {
        assert!(started.elapsed() < WAIT, "{what} did not come");
        thread::sleep(Duration::from_millis(5));
    }
}

/// A printed message: the header, and what follows it on the line.
fn printed(line: &str) -> (Value, &str) {
    let mut values = serde_json::Deserializer::from_str(line).into_iter::<Value>();
    let header = values.next().expect("a header").expect("of JSON");
    (header, line[values.byte_offset()..].trim_start())
}

fn lines(path: &Path) -> Vec<Value> {
    let Ok(recording) = fs::read_to_string(path) else {
        return Vec::new();
    };
    assert!(
        recording.is_empty() || recording.ends_with('\n'),
        "half a line"
    );
    recording
        .lines()
        .map(|line| serde_json::from_str(line).expect("a line of JSON"))
        .collect()
}

fn sound() -> Vec<u8> {
    (0..640u32).map(|index| (index * 7) as u8).collect()
}

#[test]
fn pub_then_latest_prints_what_was_published_and_a_topic_that_keeps_nothing_is_status_1() {
    let root = Root::new();
    let nothing = run(&root, &["latest", "servos"]);
    assert_eq!(nothing.status.code(), Some(1));
    assert!(nothing.stdout.is_empty());
    assert!(text(&nothing.stderr).contains("servos keeps no value"));
    assert!(!text(&nothing.stderr).contains("usage:"));

    let value = r#"{"yaw": 1990, "note": "two\nlines"}"#;
    let published = run(&root, &["pub", "servos", "latest", value, "--at-us", "42"]);
    assert!(published.status.success(), "{}", text(&published.stderr));
    assert_eq!(text(&published.stdout), "delivered 0 dropped 0 removed 0\n");

    let latest = run(&root, &["latest", "servos"]);
    assert!(latest.status.success());
    let shown = text(&latest.stdout);
    assert_eq!(shown.matches('\n').count(), 1, "one line: {shown:?}");
    let (header, payload) = printed(shown.trim_end());
    assert_eq!(header["topic"], "servos");
    assert_eq!(header["kind"], "latest");
    assert_eq!(header["enc"], "json");
    assert_eq!(header["source"], "lamp_bus_cli");
    assert_eq!(header["seq"], 1);
    assert_eq!(header["at_us"], 42);
    assert_eq!(header["boot"].as_str().unwrap().len(), 16);
    assert_eq!(
        serde_json::from_str::<Value>(payload).unwrap(),
        json!({"yaw": 1990, "note": "two\nlines"})
    );
    // What the tool published is what the library reads.
    let kept = root.bus().latest("servos").unwrap().unwrap();
    assert_eq!(kept.json().unwrap()["yaw"], 1990);

    // The root from the environment when none is given.
    let by_environment = Command::new(BINARY)
        .args(["latest", "servos"])
        .env("LAMP_BUS_ROOT", &root.0)
        .output()
        .unwrap();
    assert_eq!(by_environment.stdout, latest.stdout);

    // An event is over when it has been sent: nothing of it is kept.
    assert!(run(&root, &["pub", "mic2", "event", "1"]).status.success());
    assert_eq!(run(&root, &["latest", "mic2"]).status.code(), Some(1));
}

#[test]
fn tap_prints_each_message_published_while_it_listens_and_ends_at_its_count() {
    let root = Root::new();
    let topic = "gemini/transcript";
    let tap = Running::start(
        &root,
        &[
            "tap",
            topic,
            "--count",
            "2",
            "--seconds",
            "60",
            "--name",
            "ear",
        ],
    );
    until("the tap's socket", || root.sockets(topic).len() == 1);
    assert!(root.sockets(topic)[0].starts_with("ear."));
    let mut publisher = root.bus().publisher(topic, Kind::Event, "voice").unwrap();
    let said = json!({"said": "hello\nlamp"});
    assert_eq!(publisher.publish_json(7, &said).unwrap().delivered, 1);
    let outcome = publisher.publish_bytes(8, "pcm_s16le_16000_mono", &sound());
    assert_eq!(outcome.unwrap().delivered, 1);
    let output = tap.output();
    assert!(output.status.success(), "{}", text(&output.stderr));
    let shown: Vec<&str> = text(&output.stdout).lines().collect();
    assert_eq!(shown.len(), 2, "{shown:?}");
    let (header, payload) = printed(shown[0]);
    assert_eq!(header["topic"], topic);
    assert_eq!(header["source"], "voice");
    assert_eq!((&header["seq"], &header["at_us"]), (&json!(1), &json!(7)));
    assert_eq!(serde_json::from_str::<Value>(payload).unwrap(), said);
    let (header, payload) = printed(shown[1]);
    assert_eq!(header["enc"], "pcm_s16le_16000_mono");
    assert_eq!(header["seq"], 2);
    assert_eq!(payload, "<640 bytes>");
    // It unsubscribed on its way out.
    assert!(root.sockets(topic).is_empty());

    // One asked for and none come in the time: status 1 and nothing printed.
    let silent = run(&root, &["tap", topic, "--count", "1", "--seconds", "0.3"]);
    assert_eq!(silent.status.code(), Some(1));
    assert!(silent.stdout.is_empty());
    // With no count, the time running out is the ordinary end.
    assert!(
        run(&root, &["tap", topic, "--seconds", "0.1"])
            .status
            .success()
    );
    assert!(root.sockets(topic).is_empty());
}

#[test]
fn record_then_replay_gives_a_subscriber_the_messages_again_at_their_spacing() {
    let root = Root::new();
    let kept = Root::new();
    let bus = root.bus();
    let directory = kept.0.to_str().unwrap();
    let recorder = Running::start(
        &root,
        &[
            "record",
            directory,
            "servos",
            "gemini/transcript",
            "--seconds",
            "2.5",
        ],
    );
    until("the recorder's sockets", || {
        root.sockets("servos").len() == 1 && root.sockets("gemini/transcript").len() == 1
    });
    let mut pose = bus.publisher("servos", Kind::Latest, "body").unwrap();
    pose.expires_after(Some(Duration::from_millis(500)));
    let mut words = bus
        .publisher("gemini/transcript", Kind::Event, "voice")
        .unwrap();
    // Forty milliseconds apart by the publisher's clock, whatever the time
    // between them here.
    let at = |n: u64| 1_000_000 + n * 40_000;
    for n in 0..6u64 {
        let outcome = pose.publish_json(at(n), &json!({"yaw": n})).unwrap();
        assert_eq!(outcome.delivered, 1);
    }
    let every_byte: Vec<u8> = (0..=255).collect();
    let text_of = json!({"text": "hello"});
    assert_eq!(words.publish_json(5, &text_of).unwrap().delivered, 1);
    assert_eq!(
        words
            .publish_bytes(6, "x-test", &every_byte)
            .unwrap()
            .delivered,
        1
    );
    assert_eq!(words.publish_bytes(7, "none", b"").unwrap().delivered, 1);
    let output = recorder.output();
    let said = text(&output.stderr);
    assert!(output.status.success(), "{said}");
    assert!(
        said.contains("6 of servos in ") && said.contains(", 0 missed"),
        "{said}"
    );
    assert!(said.contains("3 of gemini/transcript in "), "{said}");
    assert!(root.sockets("servos").is_empty());

    // A line is the header and the payload in base64, and nothing else.
    let pose_file = kept.0.join("servos.jsonl");
    let words_file = kept.0.join("gemini_transcript.jsonl");
    let recorded = lines(&pose_file);
    assert_eq!(recorded.len(), 6);
    for (n, line) in recorded.iter().enumerate() {
        assert_eq!(line.as_object().unwrap().len(), 2);
        assert_eq!(line["header"]["seq"], n as u64 + 1);
        assert_eq!(line["header"]["source"], "body");
        assert_eq!(line["header"]["kind"], "latest");
        assert_eq!(line["header"]["expires_ms"], 500);
    }
    assert_eq!(recorded[0]["payload_b64"], "eyJ5YXciOjB9");
    let recorded = lines(&words_file);
    assert_eq!(recorded.len(), 3);
    assert_eq!(recorded[0]["payload_b64"], "eyJ0ZXh0IjoiaGVsbG8ifQ==");
    let all = recorded[1]["payload_b64"].as_str().unwrap();
    assert!(
        all.starts_with("AAECAwQFBgcICQoL") && all.ends_with("+fr7/P3+/w=="),
        "{all}"
    );
    assert_eq!(all.len(), 344);
    assert_eq!(recorded[2]["payload_b64"], "");

    // Played back to a subscriber. The topic's kept value is taken first,
    // so that what follows is the replay and nothing else.
    let mut follow = bus.subscribe("servos", "follow").unwrap();
    assert_eq!(follow.latest().unwrap().unwrap().header.seq, 6);
    let started = Instant::now();
    let replayed = run(
        &root,
        &["replay", pose_file.to_str().unwrap(), "--speed", "2"],
    );
    let took = started.elapsed();
    let said = text(&replayed.stderr);
    assert!(replayed.status.success(), "{said}");
    assert!(
        said.contains("6 messages, 6 delivered, 0 dropped"),
        "{said}"
    );
    // Five spaces of 40 ms at twice the speed.
    assert!(took >= Duration::from_millis(100), "{took:?}");
    for n in 0..6u64 {
        let message = follow.recv(WAIT).unwrap().expect("a replayed message");
        assert_eq!(message.header.source, "replay");
        assert_eq!(message.header.kind, Kind::Latest);
        assert_eq!((message.header.seq, message.gap), (n + 1, 0));
        assert_eq!(message.header.at_us, at(n));
        assert_eq!(message.header.expires_ms, Some(500));
        assert_eq!(message.json().unwrap(), json!({"yaw": n}));
    }
    let now = bus.latest("servos").unwrap().unwrap();
    assert_eq!((now.header.source.as_str(), now.header.seq), ("replay", 6));

    // At once, and bytes that are not text come back as they were.
    let mut ear = bus.subscribe("gemini/transcript", "ear").unwrap();
    let replayed = run(
        &root,
        &["replay", words_file.to_str().unwrap(), "--speed", "0"],
    );
    assert!(replayed.status.success(), "{}", text(&replayed.stderr));
    let message = ear.recv(WAIT).unwrap().expect("the words");
    assert_eq!(
        (message.header.kind, message.header.at_us),
        (Kind::Event, 5)
    );
    assert_eq!(message.json().unwrap(), text_of);
    let message = ear.recv(WAIT).unwrap().expect("the bytes");
    assert_eq!(message.header.enc, "x-test");
    assert_eq!(message.payload, every_byte);
    let message = ear.recv(WAIT).unwrap().expect("the bare one");
    assert_eq!(
        (message.header.enc.as_str(), message.payload.len()),
        ("none", 0)
    );
    assert_eq!((message.header.seq, message.header.expires_ms), (3, None));

    // A file that is not a recording is said to be wrong where it is wrong.
    fs::write(&words_file, "{\"header\":{},\"payload_b64\":\"\"}\n").unwrap();
    let wrong = run(&root, &["replay", words_file.to_str().unwrap()]);
    assert_eq!(wrong.status.code(), Some(1));
    assert!(text(&wrong.stderr).contains("gemini_transcript.jsonl line 1: "));
    let missing = run(
        &root,
        &["replay", kept.0.join("none.jsonl").to_str().unwrap()],
    );
    assert_eq!(missing.status.code(), Some(1));
}

#[test]
fn a_killed_recorder_leaves_whole_lines_and_its_socket_goes_with_the_next_publish() {
    let root = Root::new();
    let kept = Root::new();
    let directory = kept.0.to_str().unwrap();
    let file = kept.0.join("mic2.jsonl");
    let recorder = Running::start(&root, &["record", directory, "mic2"]);
    until("the recorder's socket", || root.sockets("mic2").len() == 1);
    let mut publisher = root.bus().publisher("mic2", Kind::Event, "voice").unwrap();
    for n in 0..5u64 {
        let outcome = publisher.publish_bytes(n, "pcm_s16le_16000_mono", &sound());
        assert_eq!(outcome.unwrap().delivered, 1);
        // Each is on the disk before the next: nothing waits in the process.
        until("the line", || lines(&file).len() as u64 == n + 1);
    }
    recorder.kill();
    let recorded = lines(&file);
    assert_eq!(recorded.len(), 5);
    for (n, line) in recorded.iter().enumerate() {
        assert_eq!(line["header"]["seq"], n as u64 + 1);
    }
    // Killed, it could not take its socket with it, and the tool says so.
    assert_eq!(root.sockets("mic2").len(), 1);
    let listed = run(&root, &["topics"]);
    assert_eq!(
        text(&listed.stdout),
        "mic2 subscribers=0 stale=1 latest=no\n"
    );
    let outcome = publisher.publish_bytes(9, "pcm_s16le_16000_mono", &sound());
    assert_eq!(
        outcome.unwrap(),
        Published {
            delivered: 0,
            dropped: 0,
            removed: 1
        }
    );
    assert!(root.sockets("mic2").is_empty());
    // Recording to the same place again adds to what is there.
    let again = run(&root, &["record", directory, "mic2", "--seconds", "0.2"]);
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert!(text(&again.stderr).contains("0 of mic2 in "));
    assert_eq!(lines(&file).len(), 5);
}

#[test]
fn a_recording_stops_at_its_size_and_says_so() {
    let root = Root::new();
    let kept = Root::new();
    let directory = kept.0.to_str().unwrap();
    // 2,621 bytes: two lines of sound, a little over a thousand bytes
    // each, and not a third.
    let recorder = Running::start(
        &root,
        &[
            "record",
            directory,
            "mic2",
            "--max-mb",
            "0.0025",
            "--seconds",
            "60",
        ],
    );
    until("the recorder's socket", || root.sockets("mic2").len() == 1);
    let mut publisher = root.bus().publisher("mic2", Kind::Event, "voice").unwrap();
    for n in 0..3u64 {
        let outcome = publisher.publish_bytes(n, "pcm_s16le_16000_mono", &sound());
        assert_eq!(outcome.unwrap().delivered, 1);
        if n < 2 {
            until("the line", || {
                lines(&kept.0.join("mic2.jsonl")).len() as u64 == n + 1
            });
        }
    }
    // Its one topic full, it has nothing left to do and ends by itself.
    let started = Instant::now();
    let output = recorder.output();
    assert!(started.elapsed() < Duration::from_secs(30));
    let said = text(&output.stderr);
    assert!(output.status.success(), "{said}");
    assert!(
        said.contains("mic2.jsonl has reached 0.0025 MiB: no more of mic2 is recorded"),
        "{said}"
    );
    assert!(
        said.contains("2 of mic2 in ") && said.contains(", file full"),
        "{said}"
    );
    assert_eq!(lines(&kept.0.join("mic2.jsonl")).len(), 2);
    assert!(fs::metadata(kept.0.join("mic2.jsonl")).unwrap().len() <= 2_621);
    assert!(root.sockets("mic2").is_empty());
}

#[test]
fn topics_lists_each_topic_with_who_listens_and_whether_a_value_is_kept() {
    let root = Root::new();
    let empty = run(&root, &["topics"]);
    assert!(empty.status.success() && empty.stdout.is_empty());
    let bus = root.bus();
    let listeners = (
        bus.subscribe("servos", "follow").unwrap(),
        bus.subscribe("servos", "eyes").unwrap(),
        bus.subscribe("gemini/transcript", "memory").unwrap(),
    );
    let mut pose = bus.publisher("servos", Kind::Latest, "body").unwrap();
    assert_eq!(pose.publish_json(1, &1).unwrap().delivered, 2);
    let mut state = bus.publisher("room/state", Kind::Latest, "eyes").unwrap();
    state.publish_json(1, &json!({"people": 1})).unwrap();
    let listed = run(&root, &["topics"]);
    assert!(listed.status.success());
    assert_eq!(
        text(&listed.stdout),
        "gemini/transcript subscribers=1 latest=no\n\
         room/state subscribers=0 latest=yes\n\
         servos subscribers=2 latest=yes\n"
    );
    drop(listeners);
}

#[test]
fn bad_input_prints_the_usage_and_is_status_2_with_nothing_touched() {
    let root = Root::new();
    let elsewhere = root.0.join("recordings");
    let elsewhere = elsewhere.to_str().unwrap();
    let cases: [&[&str]; 24] = [
        &[],
        &["frobnicate"],
        &["topics", "extra"],
        &["tap"],
        &["tap", "servos", "mic2"],
        &["tap", "../x"],
        &["tap", "/tmp/x"],
        &["tap", "servos", "--name", "A.b"],
        &["tap", "room.state"],
        &["tap", "servos", "--name", "lamp-tap"],
        &["tap", "servos", "--count", "many"],
        &["tap", "servos", "--seconds"],
        &["tap", "servos", "--seconds", "-1"],
        &["tap", "servos", "--speed", "2"],
        &["tap", "servos", "--colour", "red"],
        &["latest"],
        &["latest", "A"],
        &["pub", "servos", "latest"],
        &["pub", "servos", "sometimes", "1"],
        &["pub", "servos", "latest", "{not json"],
        &["record", elsewhere],
        &["record", elsewhere, "a/b", "a_b"],
        &["replay"],
        &["replay", "recording.jsonl", "--speed", "-1"],
    ];
    for arguments in cases {
        let output = run(&root, arguments);
        let said = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {said}");
        assert!(said.starts_with("lamp-bus: "), "{arguments:?}: {said}");
        assert!(
            said.contains("usage: lamp-bus [--root PATH] COMMAND"),
            "{arguments:?}"
        );
        assert!(output.stdout.is_empty(), "{arguments:?}");
    }
    assert!(!root.0.exists());
    // A root that is not the same place to every process is refused too.
    let relative = Command::new(BINARY)
        .args(["--root", "run/bus", "topics"])
        .output()
        .unwrap();
    assert_eq!(relative.status.code(), Some(2));
    assert!(text(&relative.stderr).contains("not an absolute path"));
    // Asked for, the usage is the answer and not an error.
    let help = run(&root, &["--help"]);
    assert!(help.status.success() && help.stderr.is_empty());
    assert!(text(&help.stdout).starts_with("usage: lamp-bus"));
}
