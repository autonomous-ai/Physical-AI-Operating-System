//! The hello question in the policy's loop, on a real bus of the test's
//! own, with stand-ins for the decision models: the test plays the voice
//! and the eyes, and reads what the policy publishes and writes down. No
//! real model is asked.
use lamp_bus::{Bus, Publisher, Subscriber};
use lamp_msgs::{
    Decision, Diagnostic, Face, Faces, Room, RoomState, Voice, VoiceArrival, VoiceState,
};
use lamp_policy::{
    attention,
    decide::{Answer, Asked, Decider, Failed, Question},
    hello::{self, Hello},
    policy::{self, Note, Options},
    progress::{self, Choice, Progress},
};
use serde_json::Value;
use std::{
    fs, io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn asked(names: Vec<(String, Question)>, yes: &[f32]) -> Asked {
    Asked {
        answers: names
            .into_iter()
            .zip(yes)
            .map(|((name, _), yes)| (name, Answer::Noul(*yes)))
            .collect(),
        model: "stand-in-1.0-20261015".into(),
        took: ms(1),
        usage: None,
        connection: None,
    }
}

/// Never asked: nothing is heard in these tests.
struct Attention;
impl Decider for Attention {
    fn ask(&mut self, _: &str, _: &[(String, Question)], _: Duration) -> Result<Asked, Failed> {
        Ok(asked(attention::questions(), &[0.1; 7]))
    }
    fn name(&self) -> &str {
        "attention-stand-in"
    }
}

/// The hello judge, played by the test.
struct Judge {
    shown: mpsc::Sender<Value>,
    replies: mpsc::Receiver<Result<Asked, Failed>>,
}
impl Decider for Judge {
    fn ask(
        &mut self,
        state: &str,
        questions: &[(String, Question)],
        deadline: Duration,
    ) -> Result<Asked, Failed> {
        assert_eq!(questions, hello::questions());
        // Never longer than the voice can wait, whatever was given.
        assert_eq!(deadline, hello::MOST_ASKING);
        let _ = self
            .shown
            .send(serde_json::from_str(state).expect("the state is JSON"));
        self.replies
            .recv_timeout(WAIT)
            .unwrap_or(Err(Failed::TimedOut))
    }
    fn name(&self) -> &str {
        "hello-stand-in"
    }
}

struct Rig {
    root: PathBuf,
    log_dir: PathBuf,
    bus: Bus,
    stop: Arc<AtomicBool>,
    running: Option<thread::JoinHandle<io::Result<policy::Totals>>>,
    saying: mpsc::Receiver<Note>,
    arrivals: Publisher,
    voice: Publisher,
    decisions: Subscriber,
    shown: mpsc::Receiver<Value>,
    replies: mpsc::Sender<Result<Asked, Failed>>,
}

impl Rig {
    fn new(name: &str, act: bool) -> Self {
        let root = PathBuf::from(format!("/tmp/lph-{name}-{}", std::process::id()));
        let log_dir =
            std::env::temp_dir().join(format!("lamp-policy-hello-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&log_dir);
        fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap().rescan_every(ms(20));
        let declared = |topic: &str, declaration, source| {
            bus.publisher_declared(topic, &declaration, source).unwrap()
        };
        let (shown_to, shown) = mpsc::channel();
        let (replies, replied) = mpsc::channel();
        let (said_so, saying) = mpsc::channel();
        let mut eyes = declared(Faces::TOPIC, Faces::declaration(), "eyes");
        let mut room = declared(RoomState::TOPIC, RoomState::declaration(), "voice");
        let mut rig = Self {
            decisions: bus
                .subscribe_typed(Decision::TOPIC, "test", Decision::TYPE)
                .unwrap(),
            arrivals: declared(VoiceArrival::TOPIC, VoiceArrival::declaration(), "voice"),
            voice: declared(VoiceState::TOPIC, VoiceState::declaration(), "voice"),
            stop: Arc::new(AtomicBool::new(false)),
            running: None,
            saying,
            shown,
            replies,
            root,
            log_dir,
            bus,
        };
        let options = Options {
            log_dir: rig.log_dir.clone(),
            deadline: ms(900),
            pause: ms(300),
            act_attention: false,
        };
        let hello = hello::Options {
            quiet_for: ms(400),
            at_most_every: Duration::from_secs(20 * 60),
            act,
            // No hour: the rule's answer must not hang on the time of day
            // the test is run at.
            ..hello::Options::new(Duration::from_secs(3), None)
        };
        let judge = Judge {
            shown: shown_to,
            replies: replied,
        };
        let (bus, stop) = (rig.bus.clone(), Arc::clone(&rig.stop));
        rig.running = Some(thread::spawn(move || {
            let hello = Hello::start(judge, hello)?;
            policy::run_with_questions(
                &bus,
                Attention,
                &options,
                &stop,
                &mut |note| {
                    let _ = said_so.send(note);
                },
                None,
                Some(hello),
            )
        }));
        let started = Instant::now();
        while !rig
            .bus
            .subscribers(VoiceArrival::TOPIC)
            .unwrap()
            .iter()
            .any(|listener| listener.socket.starts_with("policy.") && listener.listening)
        {
            assert!(started.elapsed() < WAIT, "the policy did not subscribe");
            thread::sleep(ms(5));
        }
        thread::sleep(ms(60));
        rig.state(Voice::Resting);
        room.publish_json(0, &RoomState { state: Room::Quiet })
            .unwrap();
        let stop = Arc::clone(&rig.stop);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let near = Face {
                    id: "f1".into(),
                    xmin: 0.3,
                    ymin: 0.2,
                    xmax: 0.6,
                    ymax: 0.6,
                    confidence: 0.9,
                    yaw: None,
                    facing: Some(true),
                };
                let _ = eyes.publish_json(0, &Faces { faces: vec![near] });
                thread::sleep(ms(100));
            }
        });
        thread::sleep(ms(150));
        rig
    }

    fn state(&mut self, state: Voice) {
        self.voice.publish_json(0, &VoiceState { state }).unwrap();
    }

    /// The voice holds a hello for arrival `number`.
    fn arrive(&mut self, number: u64, since_hello_min: Option<u32>) {
        let arrival = VoiceArrival {
            arrival: number,
            away_min: 42,
            there_s: 3,
            quiet_s: 310,
            since_hello_min,
        };
        self.arrivals.publish_json(0, &arrival).unwrap();
    }

    fn shown(&self) -> Value {
        self.shown.recv_timeout(WAIT).expect("a hello question")
    }
    fn not_asked(&self) {
        if let Ok(state) = self.shown.recv_timeout(ms(300)) {
            panic!("asked about {state}");
        }
    }
    fn answer(&self, two: [f32; 2]) {
        self.replies
            .send(Ok(asked(hello::questions(), &two)))
            .unwrap();
    }

    /// The next hello decision on the bus, and how long it took to come
    /// after `since`.
    fn decision(&mut self) -> Decision {
        let started = Instant::now();
        loop {
            let left = WAIT.saturating_sub(started.elapsed());
            let message = self.decisions.recv(left).unwrap().expect("a decision");
            let decision: Decision = message.parse().unwrap();
            decision.check().unwrap();
            if decision.about == "hello" {
                return decision;
            }
        }
    }

    fn log(&self, lines: usize) -> Vec<Value> {
        let started = Instant::now();
        loop {
            let found: Vec<Value> = fs::read_dir(&self.log_dir)
                .into_iter()
                .flatten()
                .map(|file| file.unwrap().path())
                .filter(|path| path.to_string_lossy().contains("decisions-"))
                .flat_map(|path| {
                    fs::read_to_string(path)
                        .unwrap()
                        .lines()
                        .map(|line| serde_json::from_str::<Value>(line).unwrap())
                        .collect::<Vec<_>>()
                })
                .filter(|line| line["about"] == "hello")
                .collect();
            if found.len() >= lines {
                assert_eq!(found.len(), lines);
                return found;
            }
            assert!(started.elapsed() < WAIT, "{} of {lines} lines", found.len());
            thread::sleep(ms(5));
        }
    }

    fn report(&self) -> Vec<(String, String)> {
        let mut reports = self
            .bus
            .subscribe_typed(Diagnostic::TOPIC, "test", Diagnostic::TYPE)
            .unwrap();
        loop {
            let message = reports.recv(WAIT).unwrap().expect("a report");
            let report: Diagnostic = message.parse().unwrap();
            if report.name == "policy" {
                return report
                    .values
                    .into_iter()
                    .map(|pair| (pair.key, pair.value))
                    .collect();
            }
        }
    }

    fn stop(mut self) -> Vec<Note> {
        self.stop.store(true, Ordering::Relaxed);
        self.running.take().unwrap().join().unwrap().unwrap();
        let notes = self.saying.try_iter().collect();
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.log_dir);
        notes
    }
}

#[test]
fn an_arrival_is_asked_about_and_decided_for_its_number_in_shadow() {
    let mut rig = Rig::new("shadow", false);
    rig.arrive(1_000_001, Some(95));
    let state = rig.shown();
    assert_eq!(state["arrival"]["minutes_away"], 42);
    assert_eq!(
        state["arrival"]["minutes_since_the_lamp_last_said_hello"],
        95
    );
    assert_eq!(state["room"]["someone_at_the_lamp"], true);
    assert_eq!(state["room"]["facing_the_lamp"], true);
    assert_eq!(state["room"]["room"], "quiet: nobody else has been talking");
    assert!(state["room"]["hour_of_day"].is_null());
    rig.answer([0.9, 0.1]);
    let decided = rig.decision();
    assert_eq!(
        (decided.choice.as_str(), decided.shadow, decided.arrival),
        ("say_now", true, Some(1_000_001))
    );
    assert!(decided.task.is_none());
    assert_eq!(decided.answers.len(), 2);
    let line = &rig.log(1)[0];
    assert_eq!(line["context"]["arrival"], 1_000_001);
    assert_eq!(line["state"], state);
    assert_eq!(line["shadow"], true);
    assert_eq!(line["asked"], "hello-stand-in");
    let report = rig.report();
    for (key, value) in [("hello_decided", "1"), ("hello_said", "1")] {
        assert!(report.contains(&(key.into(), value.into())), "{report:?}");
    }
    assert!(report.iter().all(|(key, _)| key != "hello_acts"));
    let notes = rig.stop();
    let said = notes
        .iter()
        .find(|note| matches!(note, Note::HelloDecided { .. }))
        .unwrap()
        .to_string();
    assert!(
        said.starts_with("hello decision 1 arrival=1000001 choice=say_now"),
        "{said}"
    );
}

#[test]
fn acting_a_say_now_goes_out_for_real_and_none_follows_within_twenty_minutes() {
    let mut rig = Rig::new("act", true);
    rig.arrive(2_000_001, None);
    let _ = rig.shown();
    rig.answer([0.9, 0.1]);
    let decided = rig.decision();
    assert_eq!(
        (decided.choice.as_str(), decided.shadow),
        ("say_now", false)
    );
    assert_eq!(rig.log(1)[0]["shadow"], false);
    // The next, a minute later by the voice's count: the rule says wait,
    // and no model is asked.
    rig.arrive(2_000_002, Some(1));
    let held = rig.decision();
    assert_eq!(
        (held.choice.as_str(), held.by.as_str(), held.arrival),
        ("wait", "rule", Some(2_000_002))
    );
    rig.not_asked();
    assert_eq!(rig.log(2)[1]["guard"], "greeted_recently");
    // And so does the policy's own memory of the hello it made happen,
    // whatever the voice says.
    rig.arrive(2_000_003, None);
    assert_eq!(rig.decision().choice, "wait");
    rig.not_asked();
    let report = rig.report();
    for (key, value) in [
        ("hello_acts", "true"),
        ("hello_held_recent", "2"),
        ("shadow", "true"),
    ] {
        assert!(report.contains(&(key.into(), value.into())), "{report:?}");
    }
    rig.stop();
}

#[test]
fn never_while_anyone_is_talking_to_the_lamp() {
    let mut rig = Rig::new("talking", true);
    rig.state(Voice::Listening);
    thread::sleep(ms(100));
    rig.arrive(3_000_001, None);
    let held = rig.decision();
    assert_eq!((held.choice.as_str(), held.by.as_str()), ("wait", "rule"));
    rig.not_asked();
    assert_eq!(rig.log(1)[0]["guard"], "talking");
    // Someone begins to talk while the model is asked: its yes is a wait.
    rig.state(Voice::Resting);
    thread::sleep(ms(500));
    rig.arrive(3_000_002, None);
    let _ = rig.shown();
    rig.state(Voice::Listening);
    thread::sleep(ms(100));
    rig.answer([1.0, 0.0]);
    let turned = rig.decision();
    assert_eq!((turned.choice.as_str(), turned.probability), ("wait", 1.0));
    assert_eq!(rig.log(2)[1]["guard"], "talking");
    let notes = rig.stop();
    assert!(notes.iter().any(|note| matches!(
        note,
        Note::HelloDecided {
            arrival: 3_000_002,
            choice: Choice::Wait,
            guard: Some(hello::Hold::Talking),
            ..
        }
    )));
}

#[test]
fn a_failed_model_leaves_it_to_the_rule_in_time_for_the_voice() {
    let mut rig = Rig::new("rule", true);
    rig.arrive(4_000_001, None);
    let _ = rig.shown();
    rig.replies.send(Err(Failed::TimedOut)).unwrap();
    let ruled = rig.decision();
    assert_eq!(
        (ruled.choice.as_str(), ruled.by.as_str(), ruled.probability),
        ("say_now", "rule", hello::RULE_YES)
    );
    assert_eq!(rig.log(1)[0]["failed"], "timed_out");
    rig.stop();
}

#[test]
fn without_the_switch_arrivals_are_not_listened_to_and_the_report_is_as_before() {
    struct Never;
    impl Decider for Never {
        fn ask(&mut self, _: &str, _: &[(String, Question)], _: Duration) -> Result<Asked, Failed> {
            Err(Failed::TimedOut)
        }
        fn name(&self) -> &str {
            "never"
        }
    }
    let root = PathBuf::from(format!("/tmp/lph-off-{}", std::process::id()));
    let log_dir =
        std::env::temp_dir().join(format!("lamp-policy-hello-off-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let bus = Bus::new(&root).unwrap().rescan_every(ms(20));
    let mut reports = bus
        .subscribe_typed(Diagnostic::TOPIC, "test", Diagnostic::TYPE)
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    // The progress question on, the hello question off.
    let running = {
        let (bus, stop) = (bus.clone(), Arc::clone(&stop));
        let options = Options {
            log_dir: log_dir.clone(),
            deadline: ms(900),
            pause: ms(300),
            act_attention: false,
        };
        thread::spawn(move || {
            let progress = Progress::start(Never, progress::Options::new(ms(900), None)).unwrap();
            policy::run_with_questions(
                &bus,
                Attention,
                &options,
                &stop,
                &mut |_| {},
                Some(progress),
                None,
            )
        })
    };
    let report: Diagnostic = loop {
        let message = reports.recv(WAIT).unwrap().expect("a report");
        let report: Diagnostic = message.parse().unwrap();
        if report.name == "policy" {
            break report;
        }
    };
    let keys: Vec<&str> = report.values.iter().map(|pair| pair.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "decided",
            "by_rule",
            "shadow",
            "progress_decided",
            "progress_by_rule",
            "progress_said",
            "progress_held_talking",
            "progress_held_recent"
        ]
    );
    assert!(
        bus.subscribers(VoiceArrival::TOPIC)
            .unwrap_or_default()
            .is_empty()
    );
    assert!(!root.join("voice/arrival").exists());
    stop.store(true, Ordering::Relaxed);
    running.join().unwrap().unwrap();
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&log_dir);
}
