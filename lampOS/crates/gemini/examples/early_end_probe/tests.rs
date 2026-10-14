use super::*;
use lamp_gemini::testing::{Dial, audio, connector, sent_audio};
use lamp_gemini::{Credential, GOOGLE_ENDPOINT, Resumption, SessionConfig};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const SECRET: &str = "offline-test-credential";

fn offline() -> SessionConfig {
    SessionConfig::new(GOOGLE_ENDPOINT, Credential::api_key(SECRET).unwrap())
        .unwrap()
        .resumption(Resumption::Request)
}

/// Each utterance is a constant sample, so the scripted service can tell
/// which was said.
fn speech() -> Speech {
    Speech {
        seed: vec![1_000; 16_000],
        piece: vec![2_000; 12_000],
        check: vec![3_000; 12_000],
        half: vec![4_000; 8_000],
        whole: vec![5_000; 12_000],
        rest: vec![6_000; 6_000],
    }
}

/// What the scripted services saw, across every session.
#[derive(Default)]
struct Script {
    /// Each handle given, with what the service remembered when it gave it
    /// and what the last request was.
    handles: BTreeMap<String, (Vec<&'static str>, i16)>,
    /// The handle each resumed setup presented, and what came before it.
    presented: Vec<i16>,
    /// Requests ended, notes written and where each note fell.
    ends: u32,
    notes: Vec<(String, i16)>,
    dials: u32,
}

/// A service with a memory: what it was told is kept with each handle it
/// gives, and a session resumed from a handle knows what was said up to
/// then and no more. That is what option (a) needs the live one to do.
async fn serve(mut dial: Dial, script: Arc<Mutex<Script>>, refuse_resumed: bool, silent: bool) {
    script.lock().unwrap().dials += 1;
    if refuse_resumed && dial.resumed {
        // Dropped without accepting: the attempt fails.
        return;
    }
    let setup = dial.service.accept().await;
    let mut memory = Vec::new();
    if let Some(handle) = setup["setup"]["sessionResumption"]["handle"].as_str() {
        let mut script = script.lock().unwrap();
        let (remembered, last) = script.handles[handle].clone();
        memory = remembered;
        script.presented.push(last);
    }
    let mut said = 0_i16;
    while let Some(message) = dial.service.try_receive(Duration::from_secs(60)).await {
        if let Some(samples) = sent_audio(&message) {
            said = samples[0];
        }
        if let Some(text) = message["realtimeInput"]["text"].as_str() {
            script.lock().unwrap().notes.push((text.to_owned(), said));
        }
        if message["realtimeInput"].get("activityEnd").is_none() {
            continue;
        }
        script.lock().unwrap().ends += 1;
        if silent {
            continue;
        }
        let answer = match said {
            1_000 => {
                memory.push("orange");
                "I will remember orange.".to_owned()
            }
            2_000 => {
                memory.push("purple");
                "I will remember purple too.".to_owned()
            }
            3_000 => format!("You asked me to remember {}.", memory.join(" and ")),
            4_000 => "The capital of what?".to_owned(),
            // "What is the capital of Japan?", or "Japan?" after its half.
            _ => "The capital of Japan is Tokyo.".to_owned(),
        };
        dial.service.send(audio(300, 2_400)).await;
        dial.service
            .send(json!({"serverContent":{"outputTranscription":{"text":answer,"finished":true}}}))
            .await;
        dial.service
            .send(json!({"serverContent":{"turnComplete":true}}))
            .await;
        let handle = {
            let mut script = script.lock().unwrap();
            let handle = format!("scripted-handle-{}", script.handles.len() + 1);
            script
                .handles
                .insert(handle.clone(), (memory.clone(), said));
            handle
        };
        dial.service
            .send(json!({"sessionResumptionUpdate":{"newHandle":handle,"resumable":true}}))
            .await;
    }
}

async fn probe_with(refuse_resumed: bool, silent: bool) -> (Vec<Value>, u32, Script) {
    let (mut connector, mut dials) = connector(offline());
    let script = Arc::new(Mutex::new(Script::default()));
    let serving = Arc::clone(&script);
    tokio::spawn(async move {
        while let Some(dial) = dials.recv().await {
            tokio::spawn(serve(dial, Arc::clone(&serving), refuse_resumed, silent));
        }
    });
    let speech = speech();
    let mut probe = Probe::new(&mut connector, &speech);
    let rows = probe.run(|_| {}).await;
    let requests = probe.requests;
    let script = std::mem::take(&mut *script.lock().unwrap());
    (rows, requests, script)
}

fn row<'a>(rows: &'a [Value], name: &str) -> &'a Value {
    rows.iter().find(|row| row["trial"] == name).unwrap()
}

#[tokio::test(start_paused = true)]
async fn resuming_from_before_the_piece_forgets_it_where_the_service_does() {
    let (rows, requests, script) = probe_with(false, false).await;
    assert_eq!(rows.len(), 5);
    assert_eq!((requests, script.ends), (15, 15));
    // Told the piece as a request of its own, it remembers both words.
    let control = row(&rows, "control");
    assert_eq!(control["answer_mentions"]["orange"], true);
    assert_eq!(control["answer_mentions"]["purple"], true);
    // Resumed from before the piece, it knows only the first.
    let trial = row(&rows, "resume");
    assert_eq!(trial["resumed"], true, "{trial}");
    assert_eq!(trial["answer_mentions"]["orange"], true, "{trial}");
    assert_eq!(trial["answer_mentions"]["purple"], false, "{trial}");
    assert!(trial["resume_setup_ms"].is_u64());
    // The whole question after a resume or with its note, and the rest of
    // it with the voice's note.
    for name in ["whole-resumed", "whole-labelled", "rest-labelled"] {
        assert_eq!(row(&rows, name)["answer_mentions"]["tokyo"], true);
    }
    // Every resumed session presented the handle given just after the
    // first answer (orange), never the newer one given after the piece.
    assert_eq!(script.presented, [1_000, 1_000]);
    // Each note went once, inside its question, before its end.
    assert_eq!(
        script.notes,
        [(NOTE.to_owned(), 5_000), (REST_NOTE.to_owned(), 6_000)]
    );
    // Five sessions and two resumed.
    assert_eq!(script.dials, 7);
}

#[tokio::test(start_paused = true)]
async fn nothing_private_is_printed() {
    let (rows, _, _) = probe_with(false, false).await;
    let printed = serde_json::to_string(&rows).unwrap();
    assert!(!printed.contains("scripted-handle"));
    assert!(!printed.contains(SECRET));
    // The person's words are not in it; the service's are.
    assert!(printed.contains("Tokyo"));
}

#[tokio::test(start_paused = true)]
async fn a_handle_refused_on_resume_is_said_and_not_tried_again() {
    let (rows, _, script) = probe_with(true, false).await;
    for name in ["resume", "whole-resumed"] {
        let trial = row(&rows, name);
        assert_eq!(trial["resumed"], false, "{trial}");
        assert!(trial["resume_refused"].is_string());
    }
    // One session for each trial and one refused resume for two of them.
    assert_eq!(script.dials, 7);
}

#[tokio::test(start_paused = true)]
async fn a_service_that_never_answers_ends_the_run_within_two_minutes() {
    let began = Instant::now();
    let (rows, requests, script) = probe_with(false, true).await;
    assert!(
        began.elapsed() <= DEADLINE + Duration::from_secs(1),
        "{:?}",
        began.elapsed()
    );
    assert!(requests <= MAX_REQUESTS && script.ends <= MAX_REQUESTS);
    assert_eq!(rows.len(), 5);
    assert!(
        rows.iter()
            .any(|row| row.get("error").is_some() || row.get("skipped").is_some()),
        "{rows:?}"
    );
}

#[test]
fn the_dry_run_shows_fifteen_requests_and_the_note() {
    let shown = plan(&speech());
    assert_eq!(shown["requests"], 15);
    assert_eq!(shown["at_most_requests"], 20);
    assert_eq!(shown["trials"].as_array().unwrap().len(), 5);
    let labelled = &shown["trials"][3];
    assert_eq!(labelled["trial"], "whole-labelled");
    assert!(labelled.to_string().contains(NOTE));
    let rest = &shown["trials"][4];
    assert_eq!(rest["trial"], "rest-labelled");
    assert!(rest.to_string().contains(REST_NOTE));
    assert_eq!(
        shown["trials"][1]["steps"][4]["connect"],
        "a new session, resumed from the handle given before the piece"
    );
}

/// The voice's own provider file has fields a probe does not use; it is
/// read as it is, its credential file still held to mode 0600.
#[test]
fn the_voices_own_provider_file_is_read_as_it_is() {
    use std::os::unix::fs::PermissionsExt;
    let folder = std::env::temp_dir().join(format!("early-end-provider-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let key = folder.join("key");
    std::fs::write(&key, "placeholder-key\n").unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    let file = folder.join("provider.json");
    std::fs::write(
        &file,
        json!({"endpoint": GOOGLE_ENDPOINT, "model": "gemini-3.8-live", "voice": "Kore",
            "credential_file": key, "instruction_file": "/etc/lampOS/voice/instruction.txt",
            "resumption": "retain", "search": true})
        .to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let read = crate::provider::session(&file);
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
    let loose = crate::provider::session(&file);
    let _ = std::fs::remove_dir_all(&folder);
    assert!(read.is_ok());
    assert!(loose.is_err());
}
