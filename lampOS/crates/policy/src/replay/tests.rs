use super::*;
use crate::decide::{Answer, Asked, Question};

/// A judge that answers every question with `yes`, as the model `name`,
/// in `took`; or fails, unreachable.
struct Scripted {
    name: &'static str,
    yes: Option<f32>,
    took: Duration,
    states: Vec<String>,
}

impl Decider for Scripted {
    fn ask(
        &mut self,
        state: &str,
        questions: &[(String, Question)],
        _deadline: Duration,
    ) -> Result<Asked, Failed> {
        self.states.push(state.to_owned());
        let Some(yes) = self.yes else {
            return Err(Failed::Unreachable {
                during: "connecting",
                kind: std::io::ErrorKind::ConnectionRefused,
            });
        };
        Ok(Asked {
            answers: questions
                .iter()
                .map(|(name, _)| (name.clone(), Answer::Noul(yes)))
                .collect(),
            model: format!("{}-20261014", self.name),
            took: self.took,
            usage: None,
            connection: None,
        })
    }

    fn name(&self) -> &str {
        self.name
    }
}

fn judge(name: &'static str, yes: Option<f32>, took_ms: u64) -> Scripted {
    Scripted {
        name,
        yes,
        took: Duration::from_millis(took_ms),
        states: Vec::new(),
    }
}

fn context(heard: &str, room: Room) -> Context {
    Context {
        heard: heard.into(),
        room,
        lamp_is_speaking: false,
        lamp_last_said: None,
        seconds_since_lamp_spoke: None,
        name_heard: None,
    }
}

fn logged(id: &str, at: u64, heard: &str, room: Room) -> Logged {
    Logged {
        id: id.into(),
        at_unix_ms: at,
        context: context(heard, room),
        choice: "answer".into(),
        probability: 0.9,
        by: "typesafe/jev-1.13-20260917".into(),
    }
}

/// The day's files are read as they are written: attention lines only, with
/// the line's number, and a line that is not one is counted, not fatal.
#[test]
fn the_logged_attention_decisions_are_read_with_their_place_in_the_day() {
    let dir = std::env::temp_dir().join(format!("policy-replay-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let attention = |at: u64, heard: &str| {
        json!({"at_unix_ms": at, "about": "attention", "shadow": true,
               "context": {"heard": heard, "room": "busy", "lamp_is_speaking": false,
                           "lamp_last_said": null, "seconds_since_lamp_spoke": null},
               "judgement": {"probability": 0.26, "choice": "stay_quiet", "from_rule": false},
               "by": "typesafe/jev-1.13-20260917"})
        .to_string()
    };
    let lines = [
        attention(1, "Hey Lamp, what is the weather like in Hanoi today?"),
        json!({"at_unix_ms": 2, "about": "progress", "context": {}}).to_string(),
        "{\"at_unix_ms\":3,\"about\":\"attention\"".to_owned(),
        json!({"at_unix_ms": 4, "about": "attention", "judgement": {}}).to_string(),
        attention(5, "Voglio mangiare sushi."),
    ];
    fs::write(dir.join("decisions-2026-10-15.jsonl"), lines.join("\n")).unwrap();
    let (read, unread) = read(&dir, &["2026-10-14".into(), "2026-10-15".into()]).unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(unread, 2);
    let ids: Vec<&str> = read.iter().map(|logged| logged.id.as_str()).collect();
    assert_eq!(ids, ["2026-10-15#1", "2026-10-15#5"]);
    assert_eq!(read[0].context.room, Room::Busy);
    assert_eq!(read[1].choice, "stay_quiet");
    assert_eq!(read[1].context.name_heard, None);
}

/// What was heard is kept as kinds: how many words, the lamp named, and
/// likely another language.
#[test]
fn what_was_heard_is_told_as_kinds() {
    let kinds_of = |heard: &str| {
        let kinds = kinds(heard);
        (kinds.words, kinds.names_lamp, kinds.other_language)
    };
    assert_eq!(
        kinds_of("Hey Lamp, how much is one hundred dollars in euros?"),
        (10, true, false)
    );
    assert_eq!(kinds_of("Voglio mangiare sushi."), (3, false, true));
    assert_eq!(kinds_of("¿Qué?"), (1, false, true));
    assert_eq!(kinds_of("Lamp ơi, what time is it?"), (6, true, false));
    assert_eq!(kinds_of("Lamp ơi, mấy giờ rồi?"), (5, true, true));
    assert_eq!(kinds_of("Hey man, lunch?"), (3, true, false));
    assert_eq!(kinds_of("The plan is ready."), (4, true, false));
    assert_eq!(kinds_of("Sounds great."), (2, false, false));
    assert!(!attention::names_the_lamp("lampshade"));
}

/// Each model is asked exactly the text that was sent on the day, and its
/// answers are made a choice by the same weights.
#[test]
fn each_model_is_asked_what_was_sent_and_chooses_by_the_same_weights() {
    let decision = logged("2026-10-15#1", 1, "Hey Lamp, what time is it?", Room::Quiet);
    let mut sure = judge("typesafe/jev-1.14", Some(0.95), 410);
    match ask(&mut sure, &decision, Duration::from_secs(5)) {
        Replayed::Chose {
            by,
            choice,
            took_ms,
            ..
        } => {
            assert_eq!(by, "typesafe/jev-1.14-20261014");
            assert_eq!(took_ms, 410);
            let answers = attention::Answers::of(
                &attention::questions()
                    .into_iter()
                    .map(|(name, _)| (name, Answer::Noul(0.95)))
                    .collect(),
            )
            .unwrap();
            assert_eq!(
                choice,
                attention::combine(&answers, &decision.context)
                    .choice
                    .as_str()
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(sure.states, [attention::state_text(&decision.context)]);
    let mut gone = judge("typesafe/jev-1.13", None, 0);
    assert_eq!(
        ask(&mut gone, &decision, Duration::from_secs(5)),
        Replayed::Failed("unreachable")
    );
}

/// A line names no word that was heard.
#[test]
fn a_line_holds_no_words() {
    let decision = logged("2026-10-15#139", 7, "Voglio mangiare sushi.", Room::Quiet);
    let line = line(
        &decision,
        Some(Should::StayQuiet),
        &[
            (
                "typesafe/jev-1.13".into(),
                Replayed::Chose {
                    by: "typesafe/jev-1.13-20260917".into(),
                    choice: "stay_quiet".into(),
                    probability: 0.26043,
                    took_ms: 531,
                },
            ),
            ("typesafe/jev-1.14".into(), Replayed::Failed("timed_out")),
        ],
    );
    let text = line.to_string();
    for word in ["Voglio", "mangiare", "sushi"] {
        assert!(!text.contains(word), "{text}");
    }
    assert_eq!(line["should"], "stay_quiet");
    assert_eq!(line["other_language"], true);
    assert_eq!(line["judges"][0]["probability"], 0.2604);
    assert_eq!(line["judges"][1]["failed"], "timed_out");
    assert_eq!(line["name_heard"], Value::Null);
}

/// The counts: answered, the time's p50 and p95, what each chose, how often
/// it did what should have happened, overall and for named requests, a busy
/// room and another language; and how often the two chose alike.
#[test]
fn the_summary_counts_each_model_and_each_two_alike() {
    let models = ["old".to_owned(), "new".to_owned()];
    let chose = |choice: &str, took_ms: u64| Replayed::Chose {
        by: "x".into(),
        choice: choice.into(),
        probability: 0.5,
        took_ms,
    };
    let lines = [
        // A named request in a busy room: the old model left it, the new answered.
        line(
            &logged("d#1", 1, "Hey Lamp, what is the weather?", Room::Busy),
            Some(Should::Answer),
            &[
                (models[0].clone(), chose("stay_quiet", 600)),
                (models[1].clone(), chose("answer", 500)),
            ],
        ),
        // Two people talking: both left it.
        line(
            &logged("d#2", 2, "Are you coming to lunch?", Room::Busy),
            Some(Should::StayQuiet),
            &[
                (models[0].clone(), chose("stay_quiet", 700)),
                (models[1].clone(), chose("stay_quiet", 900)),
            ],
        ),
        // Another language, nobody's label; the new model failed.
        line(
            &logged("d#3", 3, "Voglio mangiare sushi.", Room::Quiet),
            None,
            &[
                (models[0].clone(), chose("look_up", 800)),
                (models[1].clone(), Replayed::Failed("timed_out")),
            ],
        ),
    ];
    let summary = summary(&lines, &models);
    let old = &summary["models"]["old"];
    let new = &summary["models"]["new"];
    assert_eq!(old["answered"], 3);
    assert_eq!(new["answered"], 2);
    assert_eq!(new["failed"]["timed_out"], 1);
    assert_eq!(old["took_ms"], json!({"p50": 700, "p95": 800, "max": 800}));
    assert_eq!(new["took_ms"], json!({"p50": 500, "p95": 900, "max": 900}));
    assert_eq!(old["right"], json!({"labelled": 2, "right": 1}));
    assert_eq!(new["right"], json!({"labelled": 2, "right": 2}));
    assert_eq!(old["right_named"], json!({"labelled": 1, "right": 0}));
    assert_eq!(new["right_named"], json!({"labelled": 1, "right": 1}));
    assert_eq!(new["right_busy"], json!({"labelled": 2, "right": 2}));
    assert_eq!(old["chose"], json!({"look_up": 1, "stay_quiet": 2}));
    assert_eq!(old["same_as_logged"], json!({"n": 3, "same": 0}));
    assert_eq!(
        summary["pairs"][0],
        json!({"models": ["old", "new"], "both_answered": 2, "same_choice": 1, "same_side": 1})
    );
    assert!(!summary.to_string().contains("lunch"));
}

#[test]
fn labels_are_read_by_the_moment_and_refused_when_not_one_of_the_two() {
    let read = labels(
        "{\"at_unix_ms\": 5, \"should\": \"answer\"}\n\n{\"at_unix_ms\": 9, \"should\": \"stay_quiet\"}\n",
    )
    .unwrap();
    assert_eq!(read[&5], Should::Answer);
    assert_eq!(read[&9], Should::StayQuiet);
    assert!(labels("{\"at_unix_ms\": 5, \"should\": \"look_up\"}").is_err());
    assert!(labels("{\"should\": \"answer\"}").is_err());
    assert!(labels("not json").is_err());
}

/// A run is labelled after it is seen and counted again, asking nothing.
#[test]
fn an_earlier_run_is_counted_again_with_labels() {
    let models = ["old".to_owned(), "new".to_owned()];
    let chose = |choice: &str| Replayed::Chose {
        by: "x".into(),
        choice: choice.into(),
        probability: 0.5,
        took_ms: 100,
    };
    let lines = [
        line(
            &logged("d#1", 11, "Hey Lamp, what is the weather?", Room::Busy),
            None,
            &[
                (models[0].clone(), chose("stay_quiet")),
                (models[1].clone(), chose("answer")),
            ],
        ),
        line(
            &logged("d#2", 12, "Are you coming to lunch?", Room::Busy),
            None,
            &[
                (models[0].clone(), chose("stay_quiet")),
                (models[1].clone(), chose("answer")),
            ],
        ),
    ];
    let mut text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    text.push_str(&format!("{}\n", summary(&lines, &models)));
    let labels = labels("{\"at_unix_ms\": 11, \"should\": \"answer\"}").unwrap();
    let (again, found) = relabelled(&text, &labels).unwrap();
    assert_eq!(found, models);
    assert_eq!(again.len(), 2);
    assert_eq!(again[0]["should"], "answer");
    assert_eq!(again[1]["should"], Value::Null);
    let counted = summary(&again, &found);
    assert_eq!(
        counted["models"]["new"]["right"],
        json!({"labelled": 1, "right": 1})
    );
    assert_eq!(
        counted["models"]["old"]["right"],
        json!({"labelled": 1, "right": 0})
    );
    assert!(relabelled("{\"status\": \"other\"}", &labels).is_err());
}
