//! The bus between a publisher and its subscribers, through real sockets in
//! a directory of the test's own.
//!
//! How much a queue holds is the operating system's business and differs
//! between the lamp and the machine these run on, so nothing here assumes a
//! number: a test that needs a full queue fills one until a send is dropped.
use lamp_bus::{Bus, Kind, MOST_BYTES, Published, Topic, Totals};
use serde_json::json;
use std::{
    fs, io,
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Long enough for a message that has been sent to be there.
const WAIT: Duration = Duration::from_secs(5);
/// Long enough to say that nothing is coming.
const BRIEF: Duration = Duration::from_millis(30);
/// A publisher that looks at its subscribers once and never again, so that
/// a test decides what it knows.
const NEVER: Duration = Duration::from_secs(3_600);

/// A root of one test's own, removed when the test ends however it ends.
struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        static MADE: AtomicU32 = AtomicU32::new(0);
        // Short: a socket's whole path has to fit in 103 bytes, and on
        // macOS the temporary directory alone is 49.
        let path = std::env::temp_dir().join(format!(
            "lb{}-{}",
            std::process::id(),
            MADE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    fn bus(&self) -> Bus {
        Bus::new(&self.0).unwrap()
    }

    /// Everything under the root, directories with a `/` after them.
    fn contents(&self) -> Vec<String> {
        fn walk(directory: &Path, prefix: &str, found: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
                if entry.file_type().unwrap().is_dir() {
                    found.push(format!("{name}/"));
                    walk(&entry.path(), &format!("{name}/"), found);
                } else {
                    found.push(name);
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.0, "", &mut found);
        found.sort();
        found
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sockets(subs: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(subs)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    found.sort();
    found
}

/// The first message waiting, which has to be the publisher's first, and
/// how many bytes it was on the wire.
fn subscriber_bytes(subscriber: &mut lamp_bus::Subscriber) -> usize {
    let message = subscriber.recv(WAIT).unwrap().expect("a held message");
    assert_eq!((message.header.seq, message.gap), (1, 0));
    serde_json::to_vec(&message.header).unwrap().len() + 1 + message.payload.len()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// Twenty milliseconds of sound as the microphones give it, every byte
/// value in it, line ends and zeros too.
fn sound(n: u64) -> Vec<u8> {
    (0..640u64).map(|index| (index * (n + 1)) as u8).collect()
}

const ONE: Published = Published {
    delivered: 1,
    dropped: 0,
    removed: 0,
};

#[test]
fn one_publisher_reaches_two_subscribers_with_every_message_in_order_json_and_bytes() {
    let root = Root::new();
    let bus = root.bus();
    let mut follow = bus.subscribe("servos", "follow").unwrap();
    let mut eyes = bus.subscribe("servos", "eyes").unwrap();
    let mut publisher = bus.publisher("servos", Kind::Event, "lamp-body").unwrap();
    publisher.expires_after(Some(Duration::from_millis(500)));
    let both = Published {
        delivered: 2,
        dropped: 0,
        removed: 0,
    };
    // By turns, so that nothing depends on how much a queue holds.
    for n in 1..=60u64 {
        let outcome = if n % 2 == 1 {
            publisher.publish_json(n * 1_000, &json!({"yaw": n, "note": "a\nb"}))
        } else {
            publisher.publish_bytes(n * 1_000, "pcm_s16le_16000_mono", &sound(n))
        };
        assert_eq!(outcome.unwrap(), both);
        for subscriber in [&mut follow, &mut eyes] {
            let message = subscriber.recv(WAIT).unwrap().expect("a message");
            assert_eq!(message.header.topic, "servos");
            assert_eq!(message.header.kind, Kind::Event);
            assert_eq!(message.header.source, "lamp-body");
            assert_eq!(message.header.boot, publisher.boot());
            assert_eq!(message.header.seq, n);
            assert_eq!(message.header.at_us, n * 1_000);
            assert_eq!(message.header.expires_ms, Some(500));
            assert!(message.header.at_unix_ms > 0);
            assert_eq!((message.gap, message.publisher_restarted), (0, false));
            if n % 2 == 1 {
                assert_eq!(message.header.enc, "json");
                assert_eq!(message.json().unwrap(), json!({"yaw": n, "note": "a\nb"}));
                assert_eq!(message.parse::<serde_json::Value>().unwrap()["yaw"], n);
            } else {
                assert_eq!(message.header.enc, "pcm_s16le_16000_mono");
                assert_eq!(message.payload, sound(n));
                assert!(message.json().is_err());
            }
        }
    }
    // Several waiting at once come out in the order they went in.
    for n in 61..=68u64 {
        assert_eq!(publisher.publish_json(n, &n).unwrap(), both);
    }
    for subscriber in [&mut follow, &mut eyes] {
        for n in 61..=68u64 {
            let message = subscriber.recv(WAIT).unwrap().expect("a message");
            assert_eq!((message.header.seq, message.gap), (n, 0));
            assert_eq!(message.parse::<u64>().unwrap(), n);
        }
        assert!(subscriber.recv(BRIEF).unwrap().is_none());
        assert!(subscriber.recv(Duration::ZERO).unwrap().is_none());
        assert_eq!((subscriber.malformed(), subscriber.repeated()), (0, 0));
    }
    assert_eq!(
        publisher.totals(),
        Totals {
            published: 68,
            delivered: 136,
            dropped: 0
        }
    );
    // A message with nothing to carry, and labels that are not labels.
    publisher.expires_after(None);
    assert_eq!(publisher.publish_bytes(7, "none", b"").unwrap(), both);
    let bare = follow.recv(WAIT).unwrap().unwrap();
    assert_eq!((bare.header.enc.as_str(), bare.payload.len()), ("none", 0));
    assert_eq!(bare.header.expires_ms, None);
    for enc in ["none", "", "PCM", "a b"] {
        let error = publisher.publish_bytes(8, enc, b"x").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{enc:?}");
    }
    assert_eq!(publisher.totals().published, 69);
}

#[test]
fn a_late_subscriber_reads_the_current_value_of_a_latest_topic_and_nothing_of_an_event_topic() {
    let root = Root::new();
    let bus = root.bus().rescan_every(Duration::ZERO);
    let mut pose = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
    let mut words = bus
        .publisher("gemini/transcript", Kind::Event, "voice")
        .unwrap();
    for n in 1..=3u64 {
        pose.publish_json(n, &json!({"yaw": 1_990 + n})).unwrap();
        words.publish_json(n, &json!({"text": "hello"})).unwrap();
    }
    let current = bus.latest("servos").unwrap().expect("the current value");
    assert_eq!(current.header.seq, 3);
    assert_eq!(current.header.kind, Kind::Latest);
    assert_eq!(current.json().unwrap(), json!({"yaw": 1_993}));
    assert_eq!((current.gap, current.publisher_restarted), (0, false));
    assert!(bus.latest("gemini/transcript").unwrap().is_none());
    assert!(bus.latest("mic2").unwrap().is_none());

    let mut late = bus.subscribe("servos", "late").unwrap();
    assert_eq!(late.latest().unwrap().expect("the value").header.seq, 3);
    // Asked for again it is still the value, and it is not handed over by
    // `recv` as if it had just arrived.
    assert_eq!(late.latest().unwrap().expect("the value").header.seq, 3);
    assert!(late.recv(BRIEF).unwrap().is_none());
    let mut late_words = bus.subscribe("gemini/transcript", "late").unwrap();
    assert!(late_words.latest().unwrap().is_none());
    assert!(late_words.recv(BRIEF).unwrap().is_none());

    // From here on both are subscribers like any other.
    assert_eq!(pose.publish_json(4, &json!({"yaw": 2_000})).unwrap(), ONE);
    assert_eq!(
        words.publish_json(4, &json!({"text": "lamp"})).unwrap(),
        ONE
    );
    let next = late.recv(WAIT).unwrap().expect("the next value");
    assert_eq!((next.header.seq, next.gap), (4, 0));
    let word = late_words.recv(WAIT).unwrap().expect("the next word");
    // The first three words were said before it listened: that is not a gap.
    assert_eq!((word.header.seq, word.gap), (4, 0));
    assert_eq!(bus.latest("servos").unwrap().unwrap().header.seq, 4);
}

#[test]
fn a_subscriber_that_never_reads_does_not_slow_the_publisher_or_break_the_one_that_reads() {
    const MESSAGES: u64 = 20_000;
    let root = Root::new();
    let bus = root.bus();
    let slow = bus.subscribe("mic2", "slow").unwrap();
    let mut fast = bus.subscribe("mic2", "fast").unwrap();
    let mut publisher = bus.publisher("mic2", Kind::Event, "voice").unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let reader = thread::spawn({
        let done = Arc::clone(&done);
        move || {
            let mut got = Vec::new();
            loop {
                match fast.recv(BRIEF).unwrap() {
                    Some(message) => {
                        assert_eq!(message.parse::<u64>().unwrap(), message.header.seq);
                        got.push((message.header.seq, message.gap));
                    }
                    // Nothing waiting and nothing more to come.
                    None if done.load(Ordering::SeqCst) => break got,
                    None => {}
                }
            }
        }
    });
    let started = Instant::now();
    for n in 1..=MESSAGES {
        let outcome = publisher.publish_json(n, &n).unwrap();
        assert_eq!(outcome.delivered + outcome.dropped, 2, "message {n}");
        assert_eq!(outcome.removed, 0);
    }
    let took = started.elapsed();
    done.store(true, Ordering::SeqCst);
    let got = reader.join().unwrap();
    assert!(took < Duration::from_secs(5), "publishing took {took:?}");

    // The one that read: in order, and every hole in it announced.
    assert_eq!(got[0], (1, 0));
    let mut last = 0;
    for &(seq, gap) in &got {
        assert!(seq > last, "{seq} after {last}");
        assert_eq!(gap, seq - last - 1, "at {seq}");
        last = seq;
    }
    let missed: u64 = got.iter().map(|&(_, gap)| gap).sum();
    assert_eq!(got.len() as u64 + missed, last);
    assert!(last <= MESSAGES);

    let totals = publisher.totals();
    assert_eq!(totals.published, MESSAGES);
    assert_eq!(totals.delivered + totals.dropped, 2 * MESSAGES);
    let subscribers = publisher.subscribers();
    let of = |name: &str| {
        subscribers
            .iter()
            .find(|subscriber| subscriber.socket.starts_with(name))
            .unwrap()
    };
    // Everything the reader's queue took, the reader got.
    assert_eq!(of("fast.").delivered, got.len() as u64);
    assert_eq!(of("fast.").delivered + of("fast.").dropped, MESSAGES);
    assert!(of("slow.").dropped > 0);
    assert!(of("slow.").delivered > 0);
    assert_eq!(of("slow.").delivered + of("slow.").dropped, MESSAGES);
    println!(
        "{MESSAGES} messages in {took:?}: the reader got {} and was told of {missed} missed; \
         the one that never read was sent {} and dropped {}",
        got.len(),
        of("slow.").delivered,
        of("slow.").dropped
    );
    drop(slow);
}

#[test]
fn a_full_queue_at_one_subscriber_takes_nothing_from_another() {
    let root = Root::new();
    let bus = root.bus();
    let slow = bus.subscribe("mic2", "slow").unwrap();
    let mut fast = bus.subscribe("mic2", "fast").unwrap();
    let mut publisher = bus.publisher("mic2", Kind::Event, "voice").unwrap();
    // One at a time, each read before the next is sent: the reader's queue
    // never holds more than one, and whatever it misses is the other's doing.
    for n in 1..=3_000u64 {
        let outcome = publisher
            .publish_bytes(n, "pcm_s16le_16000_mono", &sound(n))
            .unwrap();
        assert_eq!(outcome.delivered + outcome.dropped, 2);
        let message = fast.recv(WAIT).unwrap().expect("a message");
        assert_eq!((message.header.seq, message.gap), (n, 0));
        assert_eq!(message.payload, sound(n));
    }
    let subscribers = publisher.subscribers();
    for subscriber in &subscribers {
        if subscriber.socket.starts_with("fast.") {
            assert_eq!((subscriber.delivered, subscriber.dropped), (3_000, 0));
        } else {
            assert!(subscriber.dropped > 0, "the queue never filled");
            assert_eq!(subscriber.delivered + subscriber.dropped, 3_000);
            println!(
                "a subscriber that does not read held {} pieces of sound of 640 bytes",
                subscriber.delivered
            );
        }
    }
    assert_eq!(subscribers.len(), 2);
    drop(slow);
}

#[test]
fn messages_lost_to_a_full_queue_show_as_a_gap_on_the_next_one_that_arrives() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("people", "follow").unwrap();
    let mut publisher = bus.publisher("people", Kind::Event, "eyes").unwrap();
    // Nobody reads: at some point the queue takes no more.
    let mut sent = 0u64;
    loop {
        let outcome = publisher.publish_json(7, &json!({"n": 1})).unwrap();
        sent += 1;
        if outcome.dropped == 1 {
            break;
        }
        assert_eq!(outcome, ONE);
        assert!(sent < 1_000_000, "the queue never filled");
    }
    let held = sent - 1;
    assert!(held >= 1);
    for _ in 0..7 {
        let outcome = publisher.publish_json(7, &json!({"n": 1})).unwrap();
        assert_eq!((outcome.delivered, outcome.dropped), (0, 1));
    }
    // What was taken comes out whole and in order, with nothing missing.
    let size = subscriber_bytes(&mut subscriber);
    println!(
        "a subscriber that does not read held {held} messages of {size} bytes before one was dropped"
    );
    for seq in 2..=held {
        let message = subscriber.recv(WAIT).unwrap().expect("a held message");
        assert_eq!((message.header.seq, message.gap), (seq, 0));
    }
    assert!(subscriber.recv(BRIEF).unwrap().is_none());
    // The eight that were dropped are said with the next one, once.
    assert_eq!(publisher.publish_json(7, &json!({"n": 1})).unwrap(), ONE);
    let message = subscriber.recv(WAIT).unwrap().expect("the next message");
    assert_eq!((message.header.seq, message.gap), (held + 9, 8));
    assert!(!message.publisher_restarted);
    assert_eq!(publisher.publish_json(7, &json!({"n": 1})).unwrap(), ONE);
    let message = subscriber.recv(WAIT).unwrap().expect("the one after");
    assert_eq!((message.header.seq, message.gap), (held + 10, 0));
    assert_eq!(
        publisher.totals(),
        Totals {
            published: held + 10,
            delivered: held + 2,
            dropped: 8
        }
    );
    // An event topic keeps nothing: the topic, its subs and one socket.
    assert_eq!(root.contents().len(), 3, "{:?}", root.contents());
}

#[test]
fn a_second_publisher_instance_under_the_same_name_shows_as_a_restart_and_not_as_a_gap() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("servos", "follow").unwrap();
    let mut first = bus.publisher("servos", Kind::Event, "lamp-body").unwrap();
    for seq in 1..=5u64 {
        assert_eq!(first.publish_json(seq, &seq).unwrap(), ONE);
        let message = subscriber.recv(WAIT).unwrap().unwrap();
        assert_eq!((message.header.seq, message.gap), (seq, 0));
        assert!(!message.publisher_restarted);
    }
    let was = first.boot().to_string();
    drop(first);
    // The body's process ended and was started again.
    let mut second = bus.publisher("servos", Kind::Event, "lamp-body").unwrap();
    assert_ne!(second.boot(), was);
    assert_eq!(second.publish_json(1, &1).unwrap(), ONE);
    let message = subscriber.recv(WAIT).unwrap().unwrap();
    assert_eq!(message.header.boot, second.boot());
    assert_eq!((message.header.seq, message.gap), (1, 0));
    assert!(message.publisher_restarted);
    // Said once: the next is an ordinary message of the new instance.
    assert_eq!(second.publish_json(2, &2).unwrap(), ONE);
    let message = subscriber.recv(WAIT).unwrap().unwrap();
    assert_eq!((message.header.seq, message.gap), (2, 0));
    assert!(!message.publisher_restarted);
}

#[test]
fn a_dropped_subscriber_removes_its_socket_and_a_dead_ones_is_removed_by_the_next_publish() {
    let root = Root::new();
    let bus = root.bus().rescan_every(NEVER);
    let subs = root.0.join("servos/subs");
    let subscriber = bus.subscribe("servos", "follow").unwrap();
    let bound = sockets(&subs);
    assert_eq!(bound.len(), 1);
    let name = bound[0].file_name().unwrap().to_str().unwrap();
    assert!(
        name.starts_with(&format!("follow.{}.", std::process::id())) && name.ends_with(".sock"),
        "{name}"
    );
    drop(subscriber);
    assert!(sockets(&subs).is_empty());

    // A subscriber whose process died: the kernel closed its socket and
    // nothing removed the file.
    let ghost = subs.join("ghost.1.0.sock");
    drop(UnixDatagram::bind(&ghost).unwrap());
    assert!(ghost.exists());
    let mut alive = bus.subscribe("servos", "alive").unwrap();
    let mut publisher = bus.publisher("servos", Kind::Event, "lamp-body").unwrap();
    let outcome = publisher.publish_json(1, &1).unwrap();
    assert_eq!(
        outcome,
        Published {
            delivered: 1,
            dropped: 0,
            removed: 1
        }
    );
    assert!(!ghost.exists());
    assert_eq!(alive.recv(WAIT).unwrap().unwrap().header.seq, 1);
    // Forgotten: it is not found dead a second time.
    assert_eq!(publisher.publish_json(2, &2).unwrap(), ONE);
    assert_eq!(publisher.subscribers().len(), 1);

    // One that dies while the publisher knows it, with another already in
    // its place: the publisher has not looked since its first message, and
    // finding the dead one makes it look, in time for this same message.
    let dying = subs.join("dying.1.0.sock");
    let held = UnixDatagram::bind(&dying).unwrap();
    drop(alive);
    let outcome = publisher.publish_json(3, &3).unwrap();
    assert_eq!(
        (outcome.delivered, outcome.removed),
        (1, 1),
        "alive went, dying came"
    );
    drop(held);
    let mut successor = bus.subscribe("servos", "next").unwrap();
    let outcome = publisher.publish_json(4, &4).unwrap();
    assert_eq!(
        outcome,
        Published {
            delivered: 1,
            dropped: 0,
            removed: 1
        }
    );
    assert!(!dying.exists());
    let message = successor.recv(WAIT).unwrap().expect("the very message");
    assert_eq!((message.header.seq, message.gap), (4, 0));
    assert_eq!(sockets(&subs).len(), 1);
    drop(successor);
    assert_eq!(root.contents(), ["servos/", "servos/subs/"]);
}

#[test]
fn publishing_to_nobody_succeeds_and_leaves_nothing_but_the_latest_value() {
    let root = Root::new();
    let bus = root.bus().rescan_every(Duration::ZERO);
    let mut sound_of = bus.publisher("mic2", Kind::Event, "voice").unwrap();
    for n in 1..=100u64 {
        let outcome = sound_of.publish_bytes(n, "pcm_s16le_16000_mono", &sound(n));
        assert_eq!(outcome.unwrap(), Published::default());
    }
    // Events to nobody: not so much as the root is created.
    assert!(!root.0.exists());
    let mut pose = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
    assert!(!root.0.exists());
    for n in 1..=100u64 {
        let outcome = pose.publish_json(n, &json!({"yaw": n}));
        assert_eq!(outcome.unwrap(), Published::default());
        assert_eq!(root.contents(), ["servos/", "servos/latest"]);
    }
    let expected = Totals {
        published: 100,
        delivered: 0,
        dropped: 0,
    };
    assert_eq!((sound_of.totals(), pose.totals()), (expected, expected));
    assert!(pose.subscribers().is_empty());
    let current = bus.latest("servos").unwrap().unwrap();
    assert_eq!(current.header.seq, 100);
    assert_eq!(current.json().unwrap(), json!({"yaw": 100}));
    // The value outlives its publisher: it is what the next subscriber reads.
    drop(pose);
    drop(sound_of);
    assert_eq!(root.contents(), ["servos/", "servos/latest"]);
}

#[test]
fn invalid_topic_and_subscriber_names_are_refused_before_anything_is_created() {
    let outer = Root::new();
    fs::create_dir(&outer.0).unwrap();
    let bus = Bus::new(outer.0.join("r")).unwrap();
    let long = "x".repeat(97);
    let absolute = outer.0.join("elsewhere");
    for topic in [
        "../x",
        "a//b",
        "A",
        long.as_str(),
        absolute.to_str().unwrap(),
        "/x",
        "",
        "..",
        "a/../b",
        "a/",
        "servos/subs",
        "latest",
    ] {
        let refused = [
            bus.publisher(topic, Kind::Latest, "lamp-body").err(),
            bus.subscribe(topic, "follow").err(),
            bus.latest(topic).err(),
        ];
        for error in refused {
            let error = error.unwrap_or_else(|| panic!("{topic:?} was taken"));
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{topic:?}");
            assert!(error.to_string().starts_with("topic "), "{error}");
        }
    }
    let long = "n".repeat(33);
    for name in ["", "a.b", "a/b", "../x", "A", "a b", long.as_str()] {
        let error = bus.subscribe("servos", name).err().expect("refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{name:?}");
        assert!(error.to_string().starts_with("subscriber name "), "{error}");
        let error = bus.publisher("servos", Kind::Latest, name).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{name:?}");
        assert!(error.to_string().starts_with("publisher name "), "{error}");
    }
    // Not under the root, and not beside it where `../x` would have gone.
    assert!(outer.contents().is_empty(), "{:?}", outer.contents());
    // A root has to be a place every process calls the same.
    let error = Bus::new("run/lampos-bus").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("not an absolute path"));
}

#[test]
fn a_message_over_the_limit_is_refused_and_nothing_is_sent_or_kept() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("camera", "follow").unwrap();
    let mut publisher = bus.publisher("camera", Kind::Latest, "eyes").unwrap();
    // The payload alone is the limit: the header takes it over.
    for attempt in [
        publisher.publish_bytes(1, "jpeg", &vec![0xff; MOST_BYTES]),
        publisher.publish_json(1, &"x".repeat(MOST_BYTES)),
        publisher.publish_bytes(1, "jpeg", &vec![0xff; 4 * MOST_BYTES]),
    ] {
        let error = attempt.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(
            error.to_string().contains("over the bus's 60000"),
            "{error}"
        );
    }
    assert!(subscriber.recv(BRIEF).unwrap().is_none());
    assert!(bus.latest("camera").unwrap().is_none());
    assert_eq!(root.contents().len(), 3, "{:?}", root.contents());
    assert_eq!(publisher.totals(), Totals::default());
    // A frame goes by reference, and the count did not move for the three
    // refused: this is the publisher's first message.
    let reference = json!({"file": "/run/lampos-eyes/sight.jpg", "bytes": 61_234});
    assert_eq!(publisher.publish_json(2, &reference).unwrap(), ONE);
    let message = subscriber.recv(WAIT).unwrap().unwrap();
    assert_eq!((message.header.seq, message.gap), (1, 0));
    assert_eq!(message.json().unwrap(), reference);
    assert_eq!(bus.latest("camera").unwrap().unwrap().header.seq, 1);
}

#[test]
fn a_message_of_exactly_the_limit_goes_through_whole_where_the_system_takes_one_that_long() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("mic1", "follow").unwrap();
    let mut publisher = bus.publisher("mic1", Kind::Event, "voice").unwrap();
    assert_eq!(publisher.publish_bytes(1, "x", b"").unwrap(), ONE);
    let first = subscriber.recv(WAIT).unwrap().unwrap();
    // The next header is as long as this one: the count stays one digit.
    let header = serde_json::to_vec(&first.header).unwrap().len() + 1;
    let payload: Vec<u8> = (0..MOST_BYTES - header).map(|index| index as u8).collect();
    let one_more = [&payload[..], &[0]].concat();
    let error = publisher.publish_bytes(1, "x", &one_more).unwrap_err();
    assert!(error.to_string().contains("of 60001 bytes"), "{error}");
    let whole = publisher.publish_bytes(1, "x", &payload);
    if cfg!(target_os = "linux") {
        assert_eq!(whole.unwrap(), ONE);
        let message = subscriber.recv(WAIT).unwrap().expect("the whole message");
        assert_eq!((message.header.seq, message.gap), (2, 0));
        assert_eq!(message.payload.len() + header, MOST_BYTES);
        assert!(message.payload == payload);
    } else {
        // macOS gives a local datagram socket 2,048 bytes to send with and
        // the standard library has no way to ask for more: the refusal is
        // the system's, is said so, and leaves the count where it was.
        let error = whole.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("operating system"), "{error}");
        assert!(subscriber.recv(BRIEF).unwrap().is_none());
        assert_eq!(
            publisher.publish_bytes(1, "x", &payload[..1_500]).unwrap(),
            ONE
        );
        let message = subscriber.recv(WAIT).unwrap().expect("a shorter message");
        assert_eq!((message.header.seq, message.gap), (2, 0));
        assert!(message.payload == payload[..1_500]);
    }
    assert_eq!(publisher.totals().published, 2);
}

#[test]
fn two_publishers_on_one_topic_keep_their_own_count_and_their_own_gaps() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("people", "follow").unwrap();
    let mut eyes = bus.publisher("people", Kind::Event, "eyes").unwrap();
    let mut voice = bus.publisher("people", Kind::Event, "voice").unwrap();
    assert_ne!(eyes.boot(), voice.boot());
    // What was published, in order: by whom, with what count, and whether
    // the subscriber's queue took it.
    let mut log: Vec<(&str, u64, bool)> = Vec::new();
    let mut publish = |publisher: &mut lamp_bus::Publisher, source: &'static str| {
        let outcome = publisher.publish_json(1, &json!({"by": source})).unwrap();
        assert_eq!(outcome.delivered + outcome.dropped, 1);
        log.push((source, publisher.totals().published, outcome.delivered == 1));
        outcome.delivered == 1
    };
    let mut got: Vec<(String, u64, u64)> = Vec::new();
    let mut read = |subscriber: &mut lamp_bus::Subscriber, wait: Duration| {
        let Some(message) = subscriber.recv(wait).unwrap() else {
            return false;
        };
        assert!(!message.publisher_restarted);
        assert_eq!(
            message.json().unwrap()["by"],
            message.header.source.as_str()
        );
        got.push((message.header.source, message.header.seq, message.gap));
        true
    };
    // By turns and read at once, twice as many from the eyes.
    for _ in 0..4 {
        assert!(publish(&mut eyes, "eyes") && read(&mut subscriber, WAIT));
        assert!(publish(&mut voice, "voice") && read(&mut subscriber, WAIT));
        assert!(publish(&mut eyes, "eyes") && read(&mut subscriber, WAIT));
    }
    // Nobody reads until the eyes have lost five, the voice speaking in
    // between. Whether the voice loses any is the system's affair: on Linux
    // each publisher has an allowance of its own.
    let mut lost = 0;
    while lost < 5 {
        if !publish(&mut eyes, "eyes") {
            lost += 1;
        }
        if lost % 2 == 1 {
            publish(&mut voice, "voice");
        }
        assert!(
            eyes.totals().published < 1_000_000,
            "the queue never filled"
        );
    }
    while read(&mut subscriber, BRIEF) {}
    // One more from each, which says what each of them lost.
    assert!(publish(&mut voice, "voice") && read(&mut subscriber, WAIT));
    assert!(publish(&mut eyes, "eyes") && read(&mut subscriber, WAIT));

    // What should have been seen: every message taken, in the order sent,
    // each with the number of its own publisher's dropped since the last.
    let mut expected: Vec<(String, u64, u64)> = Vec::new();
    let mut lost_by = [("eyes", 0u64), ("voice", 0u64)];
    for &(source, seq, taken) in &log {
        let lost = &mut lost_by.iter_mut().find(|(by, _)| *by == source).unwrap().1;
        if taken {
            expected.push((source.into(), seq, *lost));
            *lost = 0;
        } else {
            *lost += 1;
        }
    }
    assert_eq!(got, expected);
    let last_of = |source: &str| got.iter().rev().find(|(by, ..)| by == source).unwrap();
    assert!(last_of("eyes").2 >= 1, "the eyes' loss was not said");
    assert_eq!(last_of("eyes").1, eyes.totals().published);
    assert_eq!(last_of("voice").1, voice.totals().published);
    assert_ne!(eyes.totals().published, voice.totals().published);
}

#[test]
fn a_root_that_cannot_be_used_is_an_error_said_once_and_never_a_panic() {
    let outer = Root::new();
    fs::create_dir(&outer.0).unwrap();
    // A file where the root's parent should be.
    let blocked = outer.0.join("f");
    fs::write(&blocked, b"in the way").unwrap();
    let bus = Bus::new(blocked.join("bus"))
        .unwrap()
        .rescan_every(Duration::ZERO);
    assert!(bus.subscribe("servos", "follow").is_err());
    assert!(bus.latest("servos").is_err());
    assert!(bus.topics().is_err());

    let mut events = bus.publisher("mic2", Kind::Event, "voice").unwrap();
    let error = events.publish_json(1, &1).unwrap_err();
    assert!(error.to_string().starts_with("bus topic "), "{error}");
    for n in 2..=20u64 {
        assert_eq!(events.publish_json(n, &n).unwrap(), Published::default());
    }
    assert_eq!(events.totals().published, 20);
    let mut pose = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
    assert!(pose.publish_json(1, &1).is_err());
    for n in 2..=20u64 {
        assert_eq!(pose.publish_json(n, &n).unwrap(), Published::default());
    }

    // The root becomes usable: the same publishers carry on, without a word.
    fs::remove_file(&blocked).unwrap();
    assert_eq!(pose.publish_json(21, &21).unwrap(), Published::default());
    assert_eq!(bus.latest("servos").unwrap().unwrap().header.seq, 21);
    let mut subscriber = bus.subscribe("mic2", "follow").unwrap();
    assert_eq!(events.publish_json(21, &21).unwrap(), ONE);
    assert_eq!(subscriber.recv(WAIT).unwrap().unwrap().header.seq, 21);
    drop(subscriber);
    assert_eq!(events.publish_json(22, &22).unwrap(), Published::default());

    // And when it breaks again that is said again, once.
    fs::remove_dir_all(&blocked).unwrap();
    fs::write(&blocked, b"in the way again").unwrap();
    assert!(pose.publish_json(22, &22).is_err());
    assert!(events.publish_json(23, &23).is_err());
    for n in 23..=30u64 {
        assert_eq!(pose.publish_json(n, &n).unwrap(), Published::default());
        assert_eq!(events.publish_json(n, &n).unwrap(), Published::default());
    }
    assert_eq!(pose.totals().published, 30);
}

#[test]
fn a_socket_path_too_long_for_the_system_is_refused_in_plain_words_and_nothing_is_created() {
    let root = Root::new();
    let bus = root.bus();
    let topic = ["a".repeat(32), "b".repeat(32), "c".repeat(30)].join("/");
    assert_eq!(topic.len(), lamp_bus::MOST_TOPIC_BYTES);
    let error = bus.subscribe(&topic, "follow").err().expect("refused");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    let text = error.to_string();
    assert!(
        text.contains("a Unix socket's address holds 103") && text.contains("use a shorter root"),
        "{text}"
    );
    assert!(!root.0.exists());
    // Nothing of a publisher is bound, so the same topic can be published
    // and its value kept and read.
    let mut publisher = bus.publisher(&topic, Kind::Latest, "lamp-body").unwrap();
    assert_eq!(publisher.publish_json(1, &1).unwrap(), Published::default());
    assert_eq!(bus.latest(&topic).unwrap().unwrap().header.seq, 1);
    // A topic as long as leaves room for the socket's name is taken.
    let room = lamp_bus::MOST_SOCKET_PATH_BYTES
        - root.0.as_os_str().len()
        - "/".len()
        - "/subs/".len()
        - format!("f.{}.999.sock", std::process::id()).len();
    let fits = "t".repeat(room.min(32));
    let subscriber = bus.subscribe(&fits, "f");
    assert!(subscriber.is_ok(), "{room} bytes of topic");
    let bound = sockets(&root.0.join(&fits).join("subs")).remove(0);
    assert!(bound.as_os_str().len() <= lamp_bus::MOST_SOCKET_PATH_BYTES);
}

#[test]
fn what_is_not_a_message_of_the_topic_is_passed_over_and_counted() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("servos", "follow").unwrap();
    let path = sockets(&root.0.join("servos/subs")).remove(0);
    let stranger = UnixDatagram::unbound().unwrap();
    let mut publisher = bus.publisher("servos", Kind::Event, "lamp-body").unwrap();
    stranger.send_to(b"hello", &path).unwrap();
    stranger.send_to(b"{}\n{}", &path).unwrap();
    stranger.send_to(b"", &path).unwrap();
    assert_eq!(publisher.publish_json(1, &1).unwrap(), ONE);
    let message = subscriber
        .recv(WAIT)
        .unwrap()
        .expect("the message behind them");
    assert_eq!((message.header.seq, message.gap), (1, 0));
    assert_eq!(subscriber.malformed(), 3);

    // A message made by hand, as a publisher in another language would
    // make it: taken where it belongs, and not a message at the socket of
    // another topic.
    let by_hand = b"{\"topic\":\"mic2\",\"kind\":\"event\",\"enc\":\"json\",\"source\":\"voice\",\
        \"boot\":\"0123456789abcdef\",\"seq\":9,\"at_us\":1,\"at_unix_ms\":1}\n{\"level\":3}";
    let mut other = bus.subscribe("mic2", "follow").unwrap();
    let there = sockets(&root.0.join("mic2/subs")).remove(0);
    stranger.send_to(by_hand, &there).unwrap();
    let message = other.recv(WAIT).unwrap().expect("the message made by hand");
    assert_eq!((message.header.seq, message.gap), (9, 0));
    assert_eq!(message.json().unwrap(), json!({"level": 3}));
    assert_eq!(other.malformed(), 0);
    stranger.send_to(by_hand, &path).unwrap();
    // Rubbish alone ends a wait with nothing, in the time asked for.
    let started = Instant::now();
    assert!(subscriber.recv(BRIEF).unwrap().is_none());
    assert!(started.elapsed() < WAIT);
    assert_eq!(subscriber.malformed(), 4);
    assert_eq!(subscriber.repeated(), 0);
}

#[test]
fn directories_get_the_mode_asked_for_and_files_and_sockets_owner_and_group_only() {
    let root = Root::new();
    let bus = root.bus().directory_mode(0o750);
    let subscriber = bus.subscribe("gemini/transcript", "follow").unwrap();
    for directory in ["", "gemini", "gemini/transcript", "gemini/transcript/subs"] {
        assert_eq!(mode(&root.0.join(directory)), 0o750, "{directory:?}");
    }
    let socket = sockets(&root.0.join("gemini/transcript/subs")).remove(0);
    assert_eq!(mode(&socket), 0o660);
    let mut pose = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
    pose.publish_json(1, &1).unwrap();
    assert_eq!(mode(&root.0.join("servos")), 0o750);
    assert_eq!(mode(&root.0.join("servos/latest")), 0o660);
    drop(subscriber);

    // Group-writable unless said otherwise, and a directory that was
    // already there keeps the mode it had.
    let other = Root::new();
    fs::create_dir(&other.0).unwrap();
    fs::set_permissions(&other.0, fs::Permissions::from_mode(0o700)).unwrap();
    let subscriber = other.bus().subscribe("servos", "follow").unwrap();
    assert_eq!(mode(&other.0), 0o700);
    assert_eq!(mode(&other.0.join("servos")), 0o770);
    assert_eq!(mode(&other.0.join("servos/subs")), 0o770);
    drop(subscriber);
}

#[test]
fn the_newest_value_reaches_a_subscriber_whose_queue_missed_it() {
    let root = Root::new();
    // The publisher looks once; the subscriber looks at `latest` whenever
    // it has nothing waiting.
    let seldom = root.bus().rescan_every(NEVER);
    let bus = root.bus().rescan_every(Duration::ZERO);
    let mut publisher = seldom
        .publisher("room.state", Kind::Latest, "eyes")
        .unwrap();
    assert_eq!(
        publisher.publish_json(1, &json!({"people": 0})).unwrap(),
        Published::default()
    );
    // Bound just after the publisher's look: the next values are sent to
    // nobody.
    let mut subscriber = bus.subscribe("room.state", "follow").unwrap();
    assert_eq!(
        publisher.publish_json(2, &json!({"people": 1})).unwrap(),
        Published::default()
    );
    let message = subscriber.recv(BRIEF).unwrap().expect("the kept value");
    assert_eq!((message.header.seq, message.gap), (2, 0));
    assert_eq!(message.json().unwrap(), json!({"people": 1}));
    // Once: it is not news a second time.
    assert!(subscriber.recv(BRIEF).unwrap().is_none());
    assert!(subscriber.recv(Duration::ZERO).unwrap().is_none());
    for people in [2, 3, 4] {
        publisher
            .publish_json(3, &json!({"people": people}))
            .unwrap();
    }
    // The values between are gone, as a latest topic's are, and counted.
    let message = subscriber.recv(BRIEF).unwrap().expect("the kept value");
    assert_eq!((message.header.seq, message.gap), (5, 2));
    assert_eq!(message.json().unwrap(), json!({"people": 4}));
    assert_eq!(subscriber.repeated(), 0);

    // The same when the queue was full: the last value has no later
    // message behind it to show that it was dropped.
    let mut known = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
    let mut follow = bus.subscribe("servos", "follow").unwrap();
    let mut held = 0u64;
    while known.publish_json(1, &json!({"yaw": 1})).unwrap() == ONE {
        held += 1;
        assert!(held < 1_000_000, "the queue never filled");
    }
    for _ in 0..3 {
        assert_eq!(
            known.publish_json(1, &json!({"yaw": 2})).unwrap().dropped,
            1
        );
    }
    for seq in 1..=held {
        let message = follow.recv(WAIT).unwrap().expect("a held value");
        assert_eq!((message.header.seq, message.gap), (seq, 0));
    }
    let message = follow.recv(BRIEF).unwrap().expect("the newest value");
    assert_eq!((message.header.seq, message.gap), (held + 4, 3));
    assert_eq!(message.json().unwrap(), json!({"yaw": 2}));
    assert!(follow.recv(BRIEF).unwrap().is_none());
    // A value that then arrives by the queue as well is passed over.
    assert_eq!(known.publish_json(1, &json!({"yaw": 3})).unwrap(), ONE);
    assert_eq!(follow.latest().unwrap().unwrap().header.seq, held + 5);
    assert!(follow.recv(BRIEF).unwrap().is_none());
    assert_eq!(follow.repeated(), 1);
}

#[test]
fn a_subscriber_that_binds_later_is_found_at_the_publishers_next_look() {
    let root = Root::new();
    let bus = root.bus().rescan_every(Duration::from_millis(40));
    let mut publisher = bus.publisher("mic2", Kind::Event, "voice").unwrap();
    assert_eq!(publisher.publish_json(1, &1).unwrap(), Published::default());
    let mut subscriber = bus.subscribe("mic2", "follow").unwrap();
    let started = Instant::now();
    let mut seq = 1;
    let found = loop {
        seq += 1;
        if publisher.publish_json(seq, &seq).unwrap() == ONE {
            break seq;
        }
        assert!(started.elapsed() < WAIT, "never found");
        thread::sleep(Duration::from_millis(2));
    };
    // What was said before it was found is not a gap: it was not there.
    let message = subscriber
        .recv(WAIT)
        .unwrap()
        .expect("the first it was sent");
    assert_eq!((message.header.seq, message.gap), (found, 0));
    assert!(found >= 2);
}

#[test]
fn topics_are_listed_with_who_listens_and_whether_a_value_is_kept() {
    let root = Root::new();
    let bus = root.bus();
    assert!(bus.topics().unwrap().is_empty());
    let words = bus.subscribe("gemini/transcript", "memory").unwrap();
    let first = bus.subscribe("servos", "follow").unwrap();
    let second = bus.subscribe("servos", "eyes").unwrap();
    drop(UnixDatagram::bind(root.0.join("servos/subs/ghost.1.0.sock")).unwrap());
    let mut state = bus.publisher("room.state", Kind::Latest, "eyes").unwrap();
    state.publish_json(1, &1).unwrap();
    // Put there by hand: a publish to `servos` would remove the dead
    // socket that is there to be listed.
    fs::write(root.0.join("servos/latest"), b"anything").unwrap();
    // What is not a topic is not listed.
    fs::create_dir(root.0.join("Not-a-topic")).unwrap();
    fs::create_dir_all(root.0.join("empty/inside")).unwrap();
    fs::write(root.0.join("notes"), b"a file").unwrap();
    let topic = |name: &str, subscribers, stale, latest| Topic {
        name: name.into(),
        subscribers,
        stale,
        latest,
    };
    assert_eq!(
        bus.topics().unwrap(),
        [
            topic("gemini/transcript", 1, 0, false),
            topic("room.state", 0, 0, true),
            topic("servos", 2, 1, true),
        ]
    );
    drop((words, first, second));
    assert_eq!(bus.topics().unwrap()[2], topic("servos", 0, 1, true));
}

#[test]
fn a_subscriber_that_is_being_bound_is_not_taken_for_a_dead_one() {
    let root = Root::new();
    let bus = root.bus().rescan_every(Duration::ZERO);
    let done = Arc::new(AtomicBool::new(false));
    // A publisher that looks at the directory with every message and sends
    // at once: if a socket's file can be seen before the socket can be sent
    // to, this is who finds it refused and removes it.
    let publishing = thread::spawn({
        let (bus, done) = (bus.clone(), Arc::clone(&done));
        move || {
            let mut publisher = bus.publisher("mic2", Kind::Event, "voice").unwrap();
            let mut published = 0u64;
            while !done.load(Ordering::SeqCst) {
                published += 1;
                publisher.publish_json(published, &published).unwrap();
            }
            published
        }
    });
    let mut unheard = Vec::new();
    for round in 0..400 {
        let mut subscriber = bus.subscribe("mic2", "born").unwrap();
        if subscriber.recv(Duration::from_secs(2)).unwrap().is_none() {
            unheard.push(round);
        }
    }
    done.store(true, Ordering::SeqCst);
    assert!(publishing.join().unwrap() > 400);
    assert!(
        unheard.is_empty(),
        "subscribers that never heard: {unheard:?}"
    );
}

#[test]
fn a_publisher_serves_sixty_four_subscribers_and_says_once_that_there_are_more() {
    let root = Root::new();
    let bus = root.bus().rescan_every(Duration::ZERO);
    let mut subscribers: Vec<_> = (0..=lamp_bus::MOST_SUBSCRIBERS)
        .map(|_| bus.subscribe("room.state", "s").unwrap())
        .collect();
    let mut publisher = bus.publisher("room.state", Kind::Event, "eyes").unwrap();
    let all = Published {
        delivered: 64,
        dropped: 0,
        removed: 0,
    };
    let error = publisher.publish_json(1, &1).unwrap_err();
    assert!(
        error.to_string().contains("more than 64 subscribers"),
        "{error}"
    );
    // Said once, and the sixty-four were served both times.
    assert_eq!(publisher.publish_json(2, &2).unwrap(), all);
    assert_eq!(publisher.subscribers().len(), 64);
    let mut left_out = Vec::new();
    for (index, subscriber) in subscribers.iter_mut().enumerate() {
        match subscriber.recv(BRIEF).unwrap() {
            Some(message) => {
                assert_eq!(message.header.seq, 1);
                assert_eq!(subscriber.recv(WAIT).unwrap().unwrap().header.seq, 2);
            }
            None => left_out.push(index),
        }
    }
    assert_eq!(left_out.len(), 1);
    // One of the sixty-four goes: the one left out is taken on at the next
    // look, and nothing is wrong any more.
    let served = if left_out[0] == 0 { 1 } else { 0 };
    drop(subscribers.swap_remove(served));
    assert_eq!(publisher.publish_json(3, &3).unwrap(), all);
    for subscriber in &mut subscribers {
        let message = subscriber.recv(WAIT).unwrap().expect("the third");
        assert_eq!((message.header.seq, message.gap), (3, 0));
    }
    // One too many again is said again.
    subscribers.push(bus.subscribe("room.state", "s").unwrap());
    assert!(publisher.publish_json(4, &4).is_err());
    assert_eq!(publisher.publish_json(5, &5).unwrap(), all);
    assert_eq!(publisher.totals().published, 5);
}
