//! The engine against scripted services: no network.
use super::*;
use futures_util::stream::{self, BoxStream};
use net::ToolCall;
use serde_json::json;
use std::sync::atomic::AtomicUsize;

/// Scripted services. Speech of a sentence is one 1,500-sample packet
/// whose every sample is the sentence's length, so order can be read
/// back; a sentence containing "FAIL" cannot be spoken.
#[derive(Default)]
struct Script {
    listen_error: Option<net::Error>,
    heard: Mutex<VecDeque<net::Result<String>>>,
    samples_heard: Arc<AtomicUsize>,
    chats: Mutex<VecDeque<Vec<net::Result<ChatEvent>>>>,
    asked: Mutex<Vec<Vec<Message>>>,
    /// A chat stream that, after its script, never ends.
    hold_chat: bool,
    /// Speech of this sentence starts late.
    slow: Option<&'static str>,
    /// Speech of this sentence never ends.
    endless: Option<&'static str>,
    /// Held by every live stream: dropped streams let go of it.
    token: Arc<()>,
}

struct ScriptListener {
    heard: net::Result<String>,
    samples: Arc<AtomicUsize>,
}
impl Listener for ScriptListener {
    fn send(&self, samples: &[i16]) -> net::Result<()> {
        self.samples.fetch_add(samples.len(), Ordering::Relaxed);
        Ok(())
    }
    fn finish(self: Box<Self>) -> BoxFuture<'static, net::Result<String>> {
        Box::pin(async move { self.heard })
    }
}

impl Services for Script {
    fn listen(&self) -> BoxFuture<'static, net::Result<Box<dyn Listener>>> {
        let result = match &self.listen_error {
            Some(error) => Err(error.clone()),
            None => Ok(Box::new(ScriptListener {
                heard: self.heard.lock().unwrap().pop_front().unwrap(),
                samples: self.samples_heard.clone(),
            }) as Box<dyn Listener>),
        };
        Box::pin(async move { result })
    }

    fn chat(&self, messages: Vec<Message>) -> BoxStream<'static, net::Result<ChatEvent>> {
        self.asked.lock().unwrap().push(messages);
        let events = self.chats.lock().unwrap().pop_front().unwrap();
        let token = self.token.clone();
        let scripted = stream::iter(events);
        if self.hold_chat {
            scripted
                .chain(stream::pending().map(move |never: ()| {
                    let _ = &token;
                    Ok(ChatEvent::Text(format!("{never:?}")))
                }))
                .boxed()
        } else {
            scripted.boxed()
        }
    }

    fn speak(&self, text: String) -> BoxStream<'static, net::Result<Vec<i16>>> {
        let token = self.token.clone();
        if self.endless == Some(text.as_str()) {
            return stream::repeat_with(move || {
                let _ = &token;
                Ok(vec![7; PACKET_SAMPLES])
            })
            .then(|packet| async move {
                tokio::time::sleep(Duration::from_millis(1)).await;
                packet
            })
            .boxed();
        }
        let delay = if self.slow == Some(text.as_str()) {
            30
        } else {
            0
        };
        stream::once(async move {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            if text.contains("FAIL") {
                Err(net::Error::Status(503))
            } else {
                Ok(vec![text.len() as i16; 1_500])
            }
        })
        .boxed()
    }
}

fn text(piece: &str) -> net::Result<ChatEvent> {
    Ok(ChatEvent::Text(piece.to_owned()))
}

fn call(id: &str, name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        args,
    }
}

type Calls = Arc<Mutex<Vec<(String, Value)>>>;

/// An engine on `script` whose tools answer `{"ok": true}`, with a clock
/// the test moves.
fn scripted(script: Arc<Script>) -> (Engine, Calls, Arc<Mutex<Instant>>) {
    let calls: Calls = Arc::default();
    let seen = calls.clone();
    let tools: ToolRunner = Arc::new(move |_, name, args| {
        seen.lock().unwrap().push((name, args));
        Box::pin(async { json!({"ok": true}) })
    });
    let now = Arc::new(Mutex::new(Instant::now()));
    let clock = now.clone();
    let engine = Engine::with_clock(
        script,
        Arc::new(|| "SYSTEM".to_owned()),
        tools,
        Arc::new(move || *clock.lock().unwrap()),
    );
    (engine, calls, now)
}

fn start(request: u64) -> ProviderInput {
    ProviderInput::Start {
        request,
        session: 1,
        privacy_generation: 1,
        continues: false,
    }
}
fn audio(request: u64, sequence: u64) -> ProviderInput {
    ProviderInput::Audio {
        request,
        privacy_generation: 1,
        sequence,
        read_completed_at_us: 1,
        samples: vec![0; lamp_audio::CAPTURE_SAMPLES],
    }
}
fn end(request: u64) -> ProviderInput {
    ProviderInput::End {
        request,
        privacy_generation: 1,
        sound: false,
    }
}
fn written(request: u64, words: &str) -> ProviderInput {
    ProviderInput::Written {
        request,
        session: 1,
        privacy_generation: 1,
        text: words.into(),
        look: false,
    }
}

/// Every output of the turn, through its last event, as short labels; a
/// run of audio packets is one `audio:<sample>` label. Packets are checked
/// against the coordinator's bound on the way.
async fn turn(engine: &mut Engine) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), engine.next())
            .await
            .expect("the turn ends");
        let (label, last) = match event {
            ProviderOutput::Started { request, .. } => (format!("started:{request}"), false),
            ProviderOutput::Transcript {
                request: None,
                text,
                finished,
            } => {
                assert!(finished);
                (format!("heard:{text}"), false)
            }
            ProviderOutput::Transcript {
                request: Some(_),
                text,
                ..
            } => (format!("said:{text}"), false),
            ProviderOutput::Audio {
                samples,
                source_frames,
                source_offset,
                ..
            } => {
                assert!(!samples.is_empty() && samples.len() <= PACKET_SAMPLES);
                assert_eq!((source_frames, source_offset), (samples.len(), 0));
                (format!("audio:{}", samples[0]), false)
            }
            ProviderOutput::GenerationComplete { .. } => ("generated".into(), false),
            ProviderOutput::TurnComplete { idle, .. } => {
                assert!(idle);
                ("complete".into(), true)
            }
            ProviderOutput::TurnFailed { stage, reason, .. } => {
                (format!("failed:{stage:?}:{reason:?}"), true)
            }
            ProviderOutput::ReminderDeferred { reason, .. } => {
                (format!("deferred:{reason:?}"), true)
            }
            other => panic!("the second route never sends {other:?}"),
        };
        if labels.last() != Some(&label) || !label.starts_with("audio:") {
            labels.push(label);
        }
        if last {
            return labels;
        }
    }
}

const SAID: usize = ANNOUNCEMENT.len();

#[tokio::test]
async fn a_spoken_turn_is_heard_then_said_sentence_by_sentence_in_order() {
    let script = Arc::new(Script {
        heard: Mutex::new([Ok("What time is it?".to_owned())].into()),
        chats: Mutex::new(
            [vec![
                text("It is "),
                text("noon. Enjoy "),
                text("lunch!"),
                Ok(ChatEvent::Done),
            ]]
            .into(),
        ),
        // The first sentence's speech comes after the second's: still first.
        slow: Some("It is noon."),
        ..Script::default()
    });
    let (mut engine, _, _) = scripted(script.clone());
    engine.submit(start(1));
    for sequence in 1..=3 {
        engine.submit(audio(1, sequence));
    }
    engine.submit(end(1));
    assert_eq!(
        turn(&mut engine).await,
        [
            "started:1".to_owned(),
            "heard:What time is it?".into(),
            format!("said:{ANNOUNCEMENT}"),
            format!("audio:{SAID}"),
            "said:It is noon.".into(),
            "audio:11".into(),
            "said:Enjoy lunch!".into(),
            "audio:12".into(),
            "generated".into(),
            "complete".into(),
        ]
    );
    assert_eq!(
        script.samples_heard.load(Ordering::Relaxed),
        3 * lamp_audio::CAPTURE_SAMPLES
    );
    assert_eq!(
        script.asked.lock().unwrap()[0],
        [Message::system("SYSTEM"), Message::user("What time is it?")]
    );
}

#[tokio::test]
async fn a_written_turn_is_answered_without_listening() {
    let script = Arc::new(Script {
        chats: Mutex::new([vec![text("Here you are."), Ok(ChatEvent::Done)]].into()),
        ..Script::default()
    });
    let (mut engine, _, _) = scripted(script.clone());
    engine.submit(written(4, "Say something."));
    assert_eq!(
        turn(&mut engine).await,
        [
            "started:4".to_owned(),
            format!("said:{ANNOUNCEMENT}"),
            format!("audio:{SAID}"),
            "said:Here you are.".into(),
            "audio:13".into(),
            "generated".into(),
            "complete".into(),
        ]
    );
    assert_eq!(
        script.asked.lock().unwrap()[0][1],
        Message::user("Say something.")
    );
}

#[tokio::test]
async fn a_tool_call_is_run_and_its_result_answered() {
    let asked = call("c1", "set_timer", json!({"minutes": 5}));
    let script = Arc::new(Script {
        chats: Mutex::new(
            [
                vec![
                    Ok(ChatEvent::ToolCalls(vec![asked.clone()])),
                    Ok(ChatEvent::Done),
                ],
                vec![text("Timer set."), Ok(ChatEvent::Done)],
            ]
            .into(),
        ),
        ..Script::default()
    });
    let (mut engine, calls, _) = scripted(script.clone());
    engine.submit(written(1, "Five minute timer."));
    let labels = turn(&mut engine).await;
    assert_eq!(
        labels[labels.len() - 4..],
        ["said:Timer set.", "audio:10", "generated", "complete"]
    );
    assert_eq!(
        *calls.lock().unwrap(),
        [("set_timer".to_owned(), json!({"minutes": 5}))]
    );
    let asked_again = &script.asked.lock().unwrap()[1];
    assert_eq!(
        asked_again[2..],
        [
            Message::assistant_calls(None, &[asked]),
            Message::tool("c1", r#"{"ok":true}"#),
        ]
    );
}

#[tokio::test]
async fn not_for_me_ends_the_turn_in_silence() {
    let script = Arc::new(Script {
        heard: Mutex::new([Ok("No, the other one".to_owned())].into()),
        chats: Mutex::new(
            [vec![
                text("Sure"),
                Ok(ChatEvent::ToolCalls(vec![call(
                    "c1",
                    NOT_FOR_ME,
                    json!({}),
                )])),
                Ok(ChatEvent::Done),
            ]]
            .into(),
        ),
        ..Script::default()
    });
    let (mut engine, calls, _) = scripted(script);
    engine.submit(start(1));
    engine.submit(audio(1, 1));
    engine.submit(end(1));
    assert_eq!(
        turn(&mut engine).await,
        ["started:1", "heard:No, the other one", "complete"]
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn nothing_heard_is_a_quiet_complete_turn() {
    let script = Arc::new(Script {
        heard: Mutex::new([Ok(String::new())].into()),
        ..Script::default()
    });
    let (mut engine, _, _) = scripted(script.clone());
    engine.submit(start(1));
    engine.submit(end(1));
    assert_eq!(turn(&mut engine).await, ["started:1", "heard:", "complete"]);
    assert!(script.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_announcement_is_said_once_per_quiet_spell() {
    let script = Arc::new(Script {
        chats: Mutex::new(
            (0..3)
                .map(|_| vec![text("Yes."), Ok(ChatEvent::Done)])
                .collect(),
        ),
        ..Script::default()
    });
    let (mut engine, _, now) = scripted(script);
    let announced = |labels: &[String]| labels.contains(&format!("said:{ANNOUNCEMENT}"));
    engine.submit(written(1, "One?"));
    assert!(announced(&turn(&mut engine).await));
    *now.lock().unwrap() += Duration::from_secs(119);
    engine.submit(written(2, "Two?"));
    assert!(!announced(&turn(&mut engine).await));
    *now.lock().unwrap() += ANNOUNCE_AFTER;
    engine.submit(written(3, "Three?"));
    assert!(announced(&turn(&mut engine).await));
}

#[tokio::test]
async fn a_turn_spoken_over_stops_its_chat_and_speech_and_sends_nothing_more() {
    let script = Arc::new(Script {
        chats: Mutex::new([vec![text("Long story. ")]].into()),
        hold_chat: true,
        endless: Some("Long story."),
        ..Script::default()
    });
    let (mut engine, _, now) = scripted(script.clone());
    // Within the spell: no announcement, the story is the first speech.
    *engine.shared.answered_at.lock().unwrap() = Some(*now.lock().unwrap());
    engine.submit(written(1, "Tell me a story."));
    loop {
        if let ProviderOutput::Audio { request: 1, .. } = engine.next().await {
            break;
        }
    }
    engine.retire_through(1);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(engine.try_output().is_none());
    // The chat stream and the speech download were both dropped.
    assert_eq!(Arc::strong_count(&script.token), 1);
}

#[tokio::test]
async fn each_failing_service_fails_the_turn_at_its_stage() {
    // Speech-to-text cannot be opened while the person is talking.
    let script = Arc::new(Script {
        listen_error: Some(net::Error::Transport("speech-to-text connect")),
        ..Script::default()
    });
    let (mut engine, _, _) = scripted(script);
    engine.submit(start(1));
    engine.submit(audio(1, 1));
    assert_eq!(
        turn(&mut engine).await,
        ["started:1", "failed:InputOpen:Transport"]
    );

    // No final transcript after the end.
    let script = Arc::new(Script {
        heard: Mutex::new([Err(net::Error::Timeout("speech-to-text final transcript"))].into()),
        ..Script::default()
    });
    let (mut engine, _, _) = scripted(script);
    engine.submit(start(1));
    engine.submit(end(1));
    assert_eq!(
        turn(&mut engine).await,
        ["started:1", "failed:AwaitingResponse:ResponseTimeout"]
    );

    // The chat refuses: the announcement already downloading is not said.
    let script = Arc::new(Script {
        chats: Mutex::new([vec![Err(net::Error::Status(429))]].into()),
        ..Script::default()
    });
    let (mut engine, _, _) = scripted(script);
    engine.submit(written(1, "Hello?"));
    assert_eq!(
        turn(&mut engine).await,
        ["started:1", "failed:AwaitingResponse:RateLimited"]
    );

    // Speech fails after the announcement was heard: part of the answer
    // was delivered.
    let script = Arc::new(Script {
        chats: Mutex::new([vec![text("This will FAIL."), Ok(ChatEvent::Done)]].into()),
        ..Script::default()
    });
    let (mut engine, _, _) = scripted(script);
    engine.submit(written(1, "Hello?"));
    assert_eq!(
        turn(&mut engine).await,
        [
            "started:1".to_owned(),
            format!("said:{ANNOUNCEMENT}"),
            format!("audio:{SAID}"),
            "said:This will FAIL.".into(),
            "failed:Responding:ServerUnavailable".into(),
        ]
    );
}

#[tokio::test]
async fn a_reminder_is_deferred_as_the_fixture_does() {
    let (mut engine, _, _) = scripted(Arc::default());
    engine.submit(ProviderInput::Reminder {
        request: 1,
        session: 1,
        privacy_generation: 1,
        text: "Tea".into(),
        late: false,
        late_by_ms: None,
    });
    assert_eq!(turn(&mut engine).await, ["deferred:Disabled"]);
}

#[tokio::test]
async fn the_next_chat_is_given_the_last_exchange_until_it_is_five_minutes_old() {
    let script = Arc::new(Script {
        chats: Mutex::new(
            [
                vec![text("Paris."), Ok(ChatEvent::Done)],
                vec![text("About two million."), Ok(ChatEvent::Done)],
                vec![text("Banana."), Ok(ChatEvent::Done)],
            ]
            .into(),
        ),
        ..Script::default()
    });
    let (mut engine, _, clock) = scripted(script.clone());
    engine.submit(written(1, "Capital of France?"));
    turn(&mut engine).await;
    engine.submit(written(2, "How many live there?"));
    turn(&mut engine).await;
    *clock.lock().unwrap() += HISTORY_WITHIN;
    engine.submit(written(3, "A yellow fruit?"));
    turn(&mut engine).await;
    let asked = script.asked.lock().unwrap();
    assert_eq!(
        asked[1][1..],
        [
            Message::user("Capital of France?"),
            Message::assistant("Paris."),
            Message::user("How many live there?"),
        ]
    );
    assert_eq!(asked[2][1..], [Message::user("A yellow fruit?")]);
}
