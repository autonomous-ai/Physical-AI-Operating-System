//! Each person's memory kept apart (#764): Voice's clients, as a known
//! person at the lamp uses them, against the real store on a socket, in both
//! modes. Two known people and the room; neither is ever answered from the
//! other's scope. All content is synthetic.
use lamp_gemini::ToolCall;
use lamp_memory::{
    Options, Person, Server, Store,
    days::Days,
    protocol::Answer,
    server::{Memory, Stopper},
};
use lamp_voice::remember::{DaysClient, FORGET, RECALL, REMEMBER, person_id, run_for};
use serde_json::{Value, json};
use std::{
    fs::{self, DirBuilder},
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BUDGET: Duration = Duration::from_millis(1500);
static NEXT: AtomicU64 = AtomicU64::new(0);

/// The real store, with each request line and its answer traced.
struct Traced<M> {
    store: M,
    trace: Sender<(Value, Value)>,
}

impl<M: Memory> Memory for Traced<M> {
    fn answer(&mut self, line: &[u8], now: u64) -> Answer {
        let answer = self.store.answer(line, now);
        let _ = self.trace.send((
            serde_json::from_slice(line).unwrap(),
            serde_json::from_str(&answer.line).unwrap(),
        ));
        answer
    }
}

struct Fixture {
    root: PathBuf,
    socket: PathBuf,
    now: u64,
    trace: Receiver<(Value, Value)>,
    stop: Stopper,
    worker: Option<JoinHandle<()>>,
}

impl Fixture {
    fn new<M: Memory + Send + 'static>(open: impl FnOnce(Options, u64) -> M) -> Self {
        let root = std::env::temp_dir().join(format!(
            "lamp-mp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        DirBuilder::new().mode(0o700).create(&root).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let store = open(
            Options {
                dir: root.join("store"),
                keep_days: 30,
                utc_offset_minutes: lamp_voice::local_time::offset_here().unwrap_or(0) / 60,
            },
            now,
        );
        let socket = root.join("m.sock");
        let server = Server::bind(&socket).unwrap();
        let stop = server.stopper();
        let (tx, trace) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut memory = Traced { store, trace: tx };
            server.run(&mut memory, &mut || now, &mut |_| {}).unwrap();
        });
        Self {
            root,
            socket,
            now,
            trace,
            stop,
            worker: Some(worker),
        }
    }

    /// The next traced request with this op, and the store's answer.
    fn next(&self, op: &str) -> (Value, Value) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let event = self
                .trace
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .unwrap();
            if event.0["op"] == op {
                return event;
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.stop();
        self.worker.take().unwrap().join().unwrap();
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: "voice-people-test".into(),
        name: name.into(),
        args,
    }
}

/// Every text a recall told the model, with whether it was marked theirs.
fn found(told: &Value) -> Vec<(String, bool)> {
    told["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let text = ["asked_to_remember", "you_said", "recorded_observation"]
                .iter()
                .find_map(|field| item[*field].as_str())
                .unwrap();
            (text.to_owned(), item["theirs"] == true)
        })
        .collect()
}

fn texts(told: &Value) -> Vec<String> {
    found(told).into_iter().map(|(text, _)| text).collect()
}

/// What each of Minh, Sam and nobody known is told from a recall about
/// "coffee" asked through `recall`.
fn recalls(recall: impl Fn(Option<&str>) -> (Value, bool)) -> [Value; 3] {
    let (minh, sam) = (person_id("Minh"), person_id("Sam"));
    [Some(minh.as_str()), Some(sam.as_str()), None].map(|person| {
        let (told, answered) = recall(person);
        assert!(answered, "{told}");
        told
    })
}

#[test]
fn legacy_two_known_people_and_the_room_are_never_answered_from_each_other() {
    let fixture = Fixture::new(|options, now| Store::open(options, now).unwrap());
    let (minh, sam) = (person_id("Minh"), person_id("Sam"));
    for (person, text) in [
        (Some(minh.as_str()), "Minh takes coffee black"),
        (Some(sam.as_str()), "Sam takes coffee with milk"),
        (None, "the coffee machine is on the left"),
    ] {
        let (told, ok) = run_for(
            &fixture.socket,
            &call(REMEMBER, json!({"text":text})),
            BUDGET,
            person,
        );
        assert!(ok && told["remembered"] == true, "{told}");
        assert_eq!(fixture.next("remember").1["ok"], true);
    }
    let [to_minh, to_sam, to_room] = recalls(|person| {
        run_for(
            &fixture.socket,
            &call(RECALL, json!({"about":"coffee"})),
            BUDGET,
            person,
        )
    });
    assert_eq!(
        found(&to_minh),
        [
            ("Minh takes coffee black".into(), true),
            ("the coffee machine is on the left".into(), false)
        ]
    );
    assert_eq!(
        found(&to_sam),
        [
            ("Sam takes coffee with milk".into(), true),
            ("the coffee machine is on the left".into(), false)
        ]
    );
    assert_eq!(texts(&to_room), ["the coffee machine is on the left"]);
    // Minh forgetting the last thing they asked to keep is Minh's alone.
    let (told, ok) = run_for(
        &fixture.socket,
        &call(FORGET, json!({})),
        BUDGET,
        Some(&minh),
    );
    assert!(ok && told["forgotten"] == true, "{told}");
    let [to_minh, to_sam, _] = recalls(|person| {
        run_for(
            &fixture.socket,
            &call(RECALL, json!({"about":"coffee"})),
            BUDGET,
            person,
        )
    });
    assert_eq!(texts(&to_minh), ["the coffee machine is on the left"]);
    assert_eq!(
        texts(&to_sam),
        [
            "Sam takes coffee with milk",
            "the coffee machine is on the left"
        ]
    );
}

#[test]
fn days_two_known_people_and_the_room_are_never_answered_from_each_other() {
    let fixture = Fixture::new(|options, now| {
        let mut store = Days::open(options, now).unwrap();
        store
            .remember(
                &Person::everyone(),
                now,
                "the coffee machine is on the left",
            )
            .unwrap();
        store
    });
    let (minh, sam) = (person_id("Minh"), person_id("Sam"));
    let client = DaysClient::new(fixture.socket.clone()).unwrap();
    for (person, text) in [
        (minh.as_str(), "Minh takes coffee black"),
        (sam.as_str(), "Sam takes coffee with milk"),
    ] {
        let (told, ok) =
            client.run_for(&call(REMEMBER, json!({"text":text})), BUDGET, Some(person));
        assert!(ok && told["remembered"] == true, "{told}");
    }
    // A session opened with Minh at the lamp is told the room's and Minh's.
    let (room, theirs) = client
        .open_session_for(1, Duration::from_millis(300), Some(&minh))
        .unwrap();
    let theirs = theirs.unwrap();
    assert!(room["note"].as_str().unwrap().contains("coffee machine"));
    assert!(theirs["note"].as_str().unwrap().contains("Minh takes"));
    for note in [&room, &theirs] {
        assert!(!note["note"].as_str().unwrap().contains("Sam"), "{note}");
    }
    assert!(!room["note"].as_str().unwrap().contains("Minh"));
    // Each one's exchange goes to their own journal, linked to their own.
    assert!(client.finished_exchange_for(
        fixture.now - 2_000,
        "how do I take my coffee",
        "black, Minh",
        Some(&minh)
    ));
    assert!(client.finished_exchange_for(
        fixture.now - 1_000,
        "and my coffee",
        "with milk, Sam",
        Some(&sam)
    ));
    let (line, answer) = fixture.next("append_linked");
    assert_eq!(
        (line["person"].as_str(), &answer["ok"]),
        (Some(minh.as_str()), &json!(true))
    );
    assert_eq!(
        line["source_ids"],
        json!([1]),
        "Minh's kept thing, in Minh's numbering"
    );
    // Sam coming to the lamp ends Minh's conversation, in Minh's scope.
    let (line, answer) = fixture.next("conversation_end");
    assert_eq!(line["person"].as_str(), Some(minh.as_str()));
    assert_eq!(answer["ok"], true, "{answer} {line}");
    let (line, answer) = fixture.next("append_linked");
    assert_eq!(
        (line["person"].as_str(), &answer["ok"]),
        (Some(sam.as_str()), &json!(true))
    );
    assert_eq!(
        line["source_ids"],
        json!([1]),
        "Sam's own kept thing, in Sam's numbering"
    );
    let [to_minh, to_sam, to_room] =
        recalls(|person| client.run_for(&call(RECALL, json!({"about":"coffee"})), BUDGET, person));
    let minh_said = texts(&to_minh);
    let sam_said = texts(&to_sam);
    assert!(
        minh_said.contains(&"Minh takes coffee black".into()),
        "{to_minh}"
    );
    assert!(minh_said.contains(&"black, Minh".into()), "{to_minh}");
    assert!(
        sam_said.contains(&"Sam takes coffee with milk".into()),
        "{to_sam}"
    );
    assert!(sam_said.contains(&"with milk, Sam".into()), "{to_sam}");
    for (told, other) in [(&to_minh, "Sam"), (&to_sam, "Minh")] {
        assert!(
            texts(told).iter().all(|text| !text.contains(other)),
            "{told}"
        );
        assert!(texts(told).contains(&"the coffee machine is on the left".into()));
        // Theirs first, the room's after, unmarked.
        let marks: Vec<bool> = found(told).into_iter().map(|(_, theirs)| theirs).collect();
        assert!(marks.windows(2).all(|pair| pair[0] >= pair[1]), "{told}");
        assert_eq!(marks.last(), Some(&false));
    }
    assert_eq!(texts(&to_room), ["the coffee machine is on the left"]);
    assert_eq!(to_room["scope"], "said in this room; speaker unknown");
    // Minh forgetting is Minh's alone.
    let (told, ok) = client.run_for(&call(FORGET, json!({})), BUDGET, Some(&minh));
    assert!(ok && told["forgotten"] == true, "{told}");
    let [to_minh, to_sam, _] =
        recalls(|person| client.run_for(&call(RECALL, json!({"about":"coffee"})), BUDGET, person));
    assert!(!texts(&to_minh).contains(&"Minh takes coffee black".into()));
    assert!(texts(&to_sam).contains(&"Sam takes coffee with milk".into()));
}

/// #813: a correction kept in one session is told in the next, to that
/// person only; listing and forgetting are theirs alone; a recall never
/// returns a rule. Voice's Days client against the real store.
#[test]
fn days_a_rule_is_told_in_the_next_session_to_that_person_only_until_it_is_forgotten() {
    use lamp_voice::rules::{FORGET_RULE, KEEP_RULE, LIST_RULES};
    let fixture = Fixture::new(|options, now| Days::open(options, now).unwrap());
    let (minh, sam) = (person_id("Minh"), person_id("Sam"));
    let client = DaysClient::new(fixture.socket.clone()).unwrap();
    let rule = "Do not tell them the weather in the morning.";
    let (told, ok) = client.rule_for(
        &call(
            KEEP_RULE,
            json!({"rule":rule,"speaking_up":"not_first_in_mornings"}),
        ),
        BUDGET,
        Some(&minh),
    );
    assert!(ok && told["kept_rule"] == true, "{told}");
    let (line, answer) = fixture.next("remember");
    assert_eq!(line["person"], format!("r-{}", &minh[2..]));
    let number = answer["id"].as_u64().unwrap();
    // The next session with Minh at the lamp is told it; Sam's and the
    // room's are not, and without the switch nobody is.
    let to_minh = client.told_for_session(1, Some(&minh), true);
    assert!(
        to_minh.contains(&format!("- ({number}) {rule}\n")),
        "{to_minh}"
    );
    assert!(!to_minh.contains("[quiet_mornings]"), "{to_minh}");
    for note in [
        client.told_for_session(2, Some(&sam), true),
        client.told_for_session(3, None, true),
        client.told_for_session(4, Some(&minh), false),
    ] {
        assert!(!note.contains("weather"), "{note}");
    }
    // Asked, Minh hears it back; Sam hears there is none.
    let list = |person: &str| client.rule_for(&call(LIST_RULES, json!({})), BUDGET, Some(person));
    let (told, ok) = list(&minh);
    assert!(ok, "{told}");
    assert_eq!(told["rules"][0]["rule"], rule);
    assert_eq!(told["rules"][0]["number"], number);
    assert_eq!(list(&sam).0["rules"], json!([]));
    // A recall in Minh's memory never returns the rule.
    let (told, ok) = client.run_for(
        &call(RECALL, json!({"about":"weather"})),
        BUDGET,
        Some(&minh),
    );
    assert!(ok && texts(&told).is_empty(), "{told}");
    // Sam cannot take Minh's rule back; Minh can, and it is never told again.
    let forget = |person: &str| {
        client.rule_for(
            &call(FORGET_RULE, json!({"number":number})),
            BUDGET,
            Some(person),
        )
    };
    assert_eq!(forget(&sam).0["forgotten"], false);
    assert_eq!(forget(&minh).0["forgotten"], true);
    let to_minh = client.told_for_session(5, Some(&minh), true);
    assert!(!to_minh.contains("weather"), "{to_minh}");
    assert_eq!(list(&minh).0["rules"], json!([]));
}
