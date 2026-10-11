use super::*;
use lamp_gemini::testing::{FakeService, audio, pair};
use lamp_gemini::{Credential, GOOGLE_ENDPOINT};

fn offline() -> SessionConfig {
    SessionConfig::new(
        GOOGLE_ENDPOINT,
        Credential::api_key("offline-test").unwrap(),
    )
    .unwrap()
}

async fn mock() -> (Connection, FakeService) {
    mock_with(BARRIER).await
}

async fn mock_with(quiet: Duration) -> (Connection, FakeService) {
    let config = config(offline(), quiet, BarrierWait::FromInterruption).unwrap();
    let (connecting, mut service) = pair(config, SessionId::new(1).unwrap(), 1_048_576).await;
    let connecting = tokio::spawn(connecting);
    service.accept().await;
    (connecting.await.unwrap().unwrap(), service)
}

async fn until(service: &mut FakeService, name: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    loop {
        let message = service.receive().await;
        let found = message["realtimeInput"].get(name).is_some();
        messages.push(message);
        if found {
            return messages;
        }
    }
}

fn idle() -> Value {
    json!({"serverContent":{"turnComplete":true}})
}

#[tokio::test(start_paused = true)]
async fn strict_terminal_is_observed_while_distinct_continuation_is_still_sending() {
    let (connection, mut service) = mock().await;
    let first = vec![111; 8_000];
    let second = vec![222; 16_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        until(&mut service, "activityStart").await;
        service
            .send(json!({"serverContent":{"interrupted":true}}))
            .await;
        service
            .send(json!({"serverContent":{"turnComplete":true,"interactionStatus":"IN_PROGRESS"}}))
            .await;
        // Audio keeps flowing while the End barrier is still unresolved.
        for _ in 0..10 {
            let message = service.receive().await;
            assert!(message["realtimeInput"].get("audio").is_some());
            use base64::{Engine, engine::general_purpose::STANDARD};
            let bytes = STANDARD
                .decode(message["realtimeInput"]["audio"]["data"].as_str().unwrap())
                .unwrap();
            assert_eq!(bytes, [222_i16.to_le_bytes(); 160].concat());
        }
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        service.send(audio(333, 240)).await;
        service.send(json!({"serverContent":{"outputTranscription":{"text":"A reply to the continuation.","finished":true}}})).await;
        service.send(idle()).await;
        service.rest(Duration::from_secs(9)).await;
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "strict_barrier_confirmed_and_answered");
    assert_eq!(report["inputs"][0]["accepted_samples"], first.len());
    assert_eq!(report["inputs"][1]["accepted_samples"], second.len());
    assert!(
        report["idle_observed_us"][0].as_u64().unwrap()
            < report["inputs"][1]["end_queued_us"].as_u64().unwrap()
    );
    assert_eq!(report["received_audio_samples"], json!([0, 240]));
    assert_eq!(report["aftermath"]["window_complete"], true);
    assert_eq!(report["aftermath"]["observed_ms"], 8_000);
    assert_eq!(report["absolute_before_output_order_proven"], false);
}

#[tokio::test(start_paused = true)]
async fn interrupted_and_nonidle_do_not_clear_require_two_second_barrier_or_erase_partial_send() {
    let (connection, mut service) = mock().await;
    let first = vec![1; 8_000];
    let second = vec![2; 64_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        until(&mut service, "activityStart").await;
        service.send(json!({"serverContent":{"interrupted":true,"turnComplete":true,"interactionStatus":"IN_PROGRESS"}})).await;
        while let Some(message) = service.try_receive(Duration::from_secs(3)).await {
            assert!(message["realtimeInput"].get("activityEnd").is_none());
        }
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["barrier_ms"], 2_000);
    assert_eq!(report["outcome"], "strict_barrier_timeout");
    assert_eq!(report["final_state"], "Disconnected(BarrierTimeout)");
    let samples = report["inputs"][1]["accepted_samples"].as_u64().unwrap();
    assert!(samples > 0 && samples < second.len() as u64);
    assert!(report["inputs"][1]["end_queued_us"].is_null());
    assert!(report["idle_observed_us"][0].is_null());
    assert!(report["aftermath"]["observed_ms"].is_null());
    assert_eq!(report["aftermath"]["window_complete"], false);
}

#[tokio::test(start_paused = true)]
async fn early_close_after_answer_does_not_claim_eight_seconds_of_observation() {
    let (connection, mut service) = mock().await;
    let first = vec![1; 8_000];
    let second = vec![2; 8_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        until(&mut service, "activityStart").await;
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        service.send(audio(3, 240)).await;
        service.send(idle()).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        service.close(1000, "test complete").await;
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(
        report["outcome"],
        "answer_completed_but_observation_incomplete"
    );
    assert_eq!(report["stop_reason"], "connection_ended");
    assert_eq!(report["aftermath"]["window_complete"], false);
    assert!(report["aftermath"]["observed_ms"].as_u64().unwrap() < 500);
    assert_eq!(report["received_audio_samples"][1], 240);
}

#[tokio::test(start_paused = true)]
async fn prior_audio_and_a_completed_first_turn_are_not_the_requested_case() {
    for complete in [false, true] {
        let (connection, mut service) = mock().await;
        let first = vec![1; 8_000];
        let second = vec![2; 8_000];
        let client = trial(
            connection,
            &first,
            &second,
            Duration::from_millis(300),
            "continue-before-output",
        );
        let server = async {
            until(&mut service, "activityEnd").await;
            service.send(audio(10, 240)).await;
            if complete {
                service.send(idle()).await;
            }
            until(&mut service, "activityStart").await;
            if !complete {
                service.send(idle()).await;
            }
            until(&mut service, "activityEnd").await;
            service.send(audio(20, 240)).await;
            service.send(idle()).await;
            service.rest(Duration::from_secs(9)).await;
        };
        let (report, ()) = tokio::join!(client, server);
        assert_eq!(report["outcome"], "answered_other_case");
        assert_eq!(report["first_audio_observed_before_second_queue"], true);
        assert_eq!(
            report["case"],
            if complete {
                "no_barrier_at_second_start"
            } else {
                "barrier_wait_observed_after_audio"
            }
        );
    }
}

#[tokio::test(start_paused = true)]
async fn no_terminal_stays_incomplete_and_total_deadline_never_renews_on_output() {
    let (connection, mut service) = mock().await;
    let first = vec![1; 8_000];
    let second = vec![2; 8_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        until(&mut service, "activityStart").await;
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        for _ in 0..8 {
            service.send(audio(3, 240)).await;
            // Answer keepalive pings; no new input is expected.
            assert!(service.rest(Duration::from_secs(5)).await.is_none());
        }
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "incomplete");
    assert_eq!(report["stop_reason"], "trial_deadline");
    assert_eq!(report["observed_ms"], 40_000);
    assert!(report["idle_observed_us"][1].is_null());
    assert_eq!(report["aftermath"]["window_complete"], false);
}

#[test]
fn transcripts_keep_original_tokens_or_explicitly_have_no_request() {
    let mut seen = Observed::new();
    seen.event(Event::UncorrelatedInputTranscript {
        session: SessionId::new(1).unwrap(),
        text: "first or second: unknown".into(),
        finished: true,
    })
    .unwrap();
    assert!(seen.events[0].get("request").is_none());
    seen.event(Event::OutputTranscript {
        lineage: Lineage {
            session: SessionId::new(9).unwrap(),
            request: RequestId::new(77).unwrap(),
        },
        text: "foreign".into(),
        finished: true,
    })
    .unwrap();
    assert_eq!(seen.events[1]["session"], 9);
    assert_eq!(seen.events[1]["request"], 77);
    assert!(seen.foreign_lineage);
    assert_eq!(seen.audio_samples, [0, 0]);
}

#[test]
fn event_and_text_storage_is_bounded_without_synthesizing_success() {
    let mut seen = Observed::new();
    for _ in 0..MAX_EVENTS {
        seen.event(Event::Discarded {
            session: SessionId::new(1).unwrap(),
            reason: lamp_gemini::Discard::LateTerminal,
        })
        .unwrap();
    }
    assert_eq!(
        seen.event(Event::Discarded {
            session: SessionId::new(1).unwrap(),
            reason: lamp_gemini::Discard::LateTerminal
        }),
        Err("event_or_text_limit")
    );
    assert_eq!(seen.events.len(), MAX_EVENTS);
    let mut seen = Observed::new();
    assert_eq!(
        seen.event(Event::UncorrelatedInputTranscript {
            session: SessionId::new(1).unwrap(),
            text: "a".repeat(MAX_TEXT_BYTES + 1),
            finished: false
        }),
        Err("event_or_text_limit")
    );
    assert!(seen.events.is_empty());
}

fn wav(samples: usize) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(((samples * 2 + 36) as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend([1, 0, 1, 0, 128, 62, 0, 0, 0, 125, 0, 0, 2, 0, 16, 0]);
    bytes.extend(b"data");
    bytes.extend((samples as u32 * 2).to_le_bytes());
    bytes.extend([123_i16.to_le_bytes()].repeat(samples).concat());
    bytes
}

#[test]
fn strict_fixture_is_exact_and_rejects_truncation_extra_bytes_wrong_format_and_duration() {
    assert_eq!(pcm(&wav(321)).unwrap(), vec![123; 321]);
    assert!(pcm(&wav(MAX_SAMPLES)).is_ok());
    assert!(pcm(&wav(MAX_SAMPLES + 1)).is_err());
    assert!(pcm(&wav(159)).is_err());
    let good = wav(320);
    assert!(pcm(&good[..good.len() - 1]).is_err());
    let mut wrong = good.clone();
    wrong.push(0);
    assert!(pcm(&wrong).is_err());
    let mut wrong = good;
    wrong[22] = 2;
    assert!(pcm(&wrong).is_err());
}

#[tokio::test(start_paused = true)]
async fn idle_without_audio_is_not_an_answer() {
    let (connection, mut service) = mock().await;
    let first = vec![1; 8_000];
    let second = vec![2; 8_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        until(&mut service, "activityStart").await;
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        service.send(idle()).await;
        service.rest(Duration::from_secs(9)).await;
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "completed_without_audio");
    assert_eq!(report["received_audio_samples"][1], 0);
    assert_eq!(report["aftermath"]["window_complete"], true);
}

#[tokio::test]
async fn duplicated_continuation_is_refused_before_any_provider_configuration_is_opened() {
    let root = std::env::temp_dir().join(format!(
        "lamp-strict-fixture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let first = root.join("first.wav");
    let second = root.join("second.wav");
    std::fs::write(&first, wav(320)).unwrap();
    std::fs::write(&second, wav(320)).unwrap();
    let result = run(&[
        root.join("does-not-exist.json")
            .to_string_lossy()
            .into_owned(),
        first.to_string_lossy().into_owned(),
        "continue-before-output".into(),
        second.to_string_lossy().into_owned(),
    ])
    .await;
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("continuation must contain distinct PCM")
    );
}

#[tokio::test(start_paused = true)]
async fn during_output_the_successor_starts_only_on_the_first_requests_actual_audio() {
    let (connection, mut service) = mock().await;
    let first = vec![111; 8_000];
    let second = vec![222; 16_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::ZERO,
        "continue-during-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        // No successor while the first request has produced nothing,
        // however long that takes.
        assert!(service.try_receive(Duration::from_secs(3)).await.is_none());
        service.send(audio(111, 240)).await;
        until(&mut service, "activityStart").await;
        // Interrupted while producing: the terminal the barrier waits for.
        service.send(audio(112, 240)).await;
        service
            .send(json!({"serverContent":{"interrupted":true}}))
            .await;
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        service.send(audio(333, 240)).await;
        service.send(idle()).await;
        service.rest(Duration::from_secs(9)).await;
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(
        report["outcome"],
        "strict_barrier_confirmed_after_audio_and_answered"
    );
    assert_eq!(report["case"], "barrier_wait_observed_after_audio");
    assert_eq!(report["first_audio_observed_before_second_queue"], true);
    // The second start followed the first audio, not a clock.
    assert!(
        report["inputs"][1]["start_queued_us"].as_u64().unwrap()
            >= report["first_audio_observed_us"][0].as_u64().unwrap()
    );
    assert!(report["inputs"][1]["start_queued_us"].as_u64().unwrap() >= 3_000_000);
    // The first request's audio was received before its retirement only:
    // what came after the interrupting start is not counted to either.
    assert_eq!(report["received_audio_samples"], json!([240, 240]));
    assert!(report["interrupted_observed_us"][0].as_u64().is_some());
    assert!(
        report["idle_observed_us"][0].as_u64().unwrap()
            < report["inputs"][1]["end_queued_us"].as_u64().unwrap()
    );
    assert_eq!(report["inputs"][1]["accepted_samples"], second.len());
}

#[tokio::test(start_paused = true)]
async fn during_output_without_a_terminal_is_a_strict_timeout_and_no_end_is_written() {
    let (connection, mut service) = mock().await;
    let first = vec![1; 8_000];
    let second = vec![2; 64_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::ZERO,
        "continue-during-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        service.send(audio(5, 240)).await;
        until(&mut service, "activityStart").await;
        // The service goes on producing for a while and never says it stopped.
        service.send(audio(6, 240)).await;
        while let Some(message) = service.try_receive(Duration::from_secs(3)).await {
            assert!(message["realtimeInput"].get("activityEnd").is_none());
        }
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "strict_barrier_timeout");
    assert_eq!(report["case"], "barrier_wait_observed_after_audio");
    assert_eq!(report["inputs"][1]["end_queued_us"], Value::Null);
    assert_eq!(report["interrupted_observed_us"], json!([null, null]));
}

#[test]
fn a_longer_barrier_bound_is_parsed_bounded_and_removed() {
    let arguments = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    let mut plain = arguments(&["p", "a.wav", "continue-during-output", "b.wav"]);
    assert_eq!(observed_barrier(&mut plain).unwrap(), BARRIER);
    assert_eq!(plain.len(), 4);
    let mut longer = arguments(&[
        "p",
        "a.wav",
        "continue-before-output",
        "b.wav",
        "300",
        "barrier-ms=5000",
    ]);
    assert_eq!(observed_barrier(&mut longer).unwrap(), LONGEST_BARRIER);
    assert_eq!(longer.len(), 5);
    for bad in [
        "barrier-ms=1999",
        "barrier-ms=5001",
        "barrier-ms=10000",
        "barrier-ms=soon",
    ] {
        let mut arguments = arguments(&["p", "a.wav", "continue-before-output", "b.wav", bad]);
        assert!(observed_barrier(&mut arguments).is_err(), "{bad}");
    }
}

#[test]
fn only_a_confirmed_answer_under_the_production_bound_passes() {
    let result = |outcome: &str, observation_only: bool| json!({"outcome": outcome, "observation_only": observation_only});
    // Both confirmed outcomes pass: before output, and after it.
    assert!(passed(&result(
        "strict_barrier_confirmed_and_answered",
        false
    )));
    assert!(passed(&result(
        "strict_barrier_confirmed_after_audio_and_answered",
        false
    )));
    // A longer bound than production's observes and passes nothing.
    assert!(!passed(&result(
        "strict_barrier_confirmed_and_answered",
        true
    )));
    assert!(!passed(&result(
        "strict_barrier_confirmed_after_audio_and_answered",
        true
    )));
    for outcome in [
        "strict_barrier_timeout",
        "answered_other_case",
        "completed_without_audio",
        "answer_completed_but_observation_incomplete",
        "incomplete",
        "setup_failed",
    ] {
        assert!(!passed(&result(outcome, false)), "{outcome}");
    }
    // A result that does not say which bound it ran under does not pass.
    assert!(!passed(
        &json!({"outcome": "strict_barrier_confirmed_and_answered"})
    ));
}

/// The first real attempt at a longer bound asked for ten seconds: the
/// probe's parser took it and the library refused the configuration, so the
/// run never reached the service. The parser was tested and the
/// configuration it feeds was not.
#[test]
fn every_bound_the_probe_accepts_builds_a_configuration_the_library_accepts() {
    for milliseconds in [2_000_u64, 2_001, 3_500, 4_999, 5_000] {
        let mut arguments = vec![format!("barrier-ms={milliseconds}")];
        let quiet = observed_barrier(&mut arguments).unwrap();
        assert_eq!(quiet, Duration::from_millis(milliseconds));
        assert!(
            config(offline(), quiet, BarrierWait::FromInterruption).is_ok(),
            "{milliseconds}"
        );
    }
    // The probe's limit is the library's: one millisecond more is refused
    // there, so the two cannot drift apart without this failing.
    assert!(
        config(
            offline(),
            LONGEST_BARRIER + Duration::from_millis(1),
            BarrierWait::FromInterruption
        )
        .is_err()
    );
}

#[tokio::test(start_paused = true)]
async fn the_longest_bound_sees_a_terminal_that_two_seconds_would_have_missed() {
    let (connection, mut service) = mock_with(LONGEST_BARRIER).await;
    let first = vec![111; 8_000];
    let second = vec![222; 16_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        until(&mut service, "activityStart").await;
        // Four seconds with nothing for the earlier response. The successor
        // is uploaded meanwhile and its End is not written.
        let late = Instant::now() + Duration::from_secs(4);
        while let Some(left) = late.checked_duration_since(Instant::now()) {
            if left.is_zero() {
                break;
            }
            if let Some(message) = service.try_receive(left).await {
                assert!(message["realtimeInput"].get("activityEnd").is_none());
            }
        }
        service
            .send(json!({"serverContent":{"interrupted":true}}))
            .await;
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        service.send(audio(333, 240)).await;
        service.send(idle()).await;
        service.rest(Duration::from_secs(9)).await;
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "strict_barrier_confirmed_and_answered");
    let start = report["inputs"][1]["start_queued_us"].as_u64().unwrap();
    assert!(report["idle_observed_us"][0].as_u64().unwrap() >= start + 4_000_000);
    assert_eq!(report["inputs"][1]["accepted_samples"], second.len());
}

#[tokio::test(start_paused = true)]
async fn the_longest_bound_still_times_out_when_no_terminal_comes() {
    let (connection, mut service) = mock_with(LONGEST_BARRIER).await;
    let first = vec![1; 8_000];
    let second = vec![2; 112_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        while let Some(message) = service.try_receive(Duration::from_secs(3)).await {
            assert!(message["realtimeInput"].get("activityEnd").is_none());
        }
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "strict_barrier_timeout");
    assert_eq!(report["inputs"][1]["end_queued_us"], Value::Null);
    let start = report["inputs"][1]["start_queued_us"].as_u64().unwrap();
    // Not before five seconds of nothing, and not a full upload later.
    let ended = report["observed_ms"].as_u64().unwrap() * 1_000;
    assert!(ended >= start + 5_000_000, "{ended} {start}");
    assert!(ended < start + 6_000_000, "{ended} {start}");
    assert!((report["inputs"][1]["accepted_samples"].as_u64().unwrap() as usize) < second.len());
}

#[test]
fn where_the_bound_starts_is_parsed_last_and_removed() {
    let arguments = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    let mut plain = arguments(&["p", "a.wav", "continue-during-output", "b.wav"]);
    assert_eq!(
        barrier_wait(&mut plain).unwrap(),
        BarrierWait::FromInterruption
    );
    assert_eq!(plain.len(), 4);
    let mut both = arguments(&[
        "p",
        "a.wav",
        "continue-before-output",
        "b.wav",
        "300",
        "barrier-ms=5000",
        "barrier-wait=at-commit",
    ]);
    assert_eq!(barrier_wait(&mut both).unwrap(), BarrierWait::AtCommit);
    assert_eq!(observed_barrier(&mut both).unwrap(), LONGEST_BARRIER);
    assert_eq!(both.len(), 5);
    let mut unknown = arguments(&["p", "a.wav", "repeat-before-output", "barrier-wait=later"]);
    assert!(barrier_wait(&mut unknown).is_err());
}

fn barrier_rows(report: &Value) -> Vec<&Value> {
    report["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "barrier")
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_settled_barrier_is_reported_with_counts_of_what_it_withheld() {
    let (connection, mut service) = mock().await;
    let first = vec![111; 8_000];
    let second = vec![222; 16_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::ZERO,
        "continue-during-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        service.send(audio(111, 240)).await;
        until(&mut service, "activityStart").await;
        // Two frames of the earlier response arrive after it was superseded.
        service.send(audio(112, 240)).await;
        service.send(audio(113, 480)).await;
        service
            .send(json!({"serverContent":{"interrupted":true}}))
            .await;
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        service.send(audio(333, 240)).await;
        service.send(idle()).await;
        service.rest(Duration::from_secs(9)).await;
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(
        report["outcome"],
        "strict_barrier_confirmed_after_audio_and_answered"
    );
    let rows = barrier_rows(&report);
    assert_eq!(rows.len(), 1);
    let row = rows[0];
    assert_eq!(row["outcome"], "Settled");
    assert_eq!(row["superseded"], 1);
    assert_eq!(row["successor"], 2);
    assert_eq!(row["audio_frames"], 2);
    assert_eq!(row["audio_samples"], 720);
    assert_eq!(row["interrupted"], 1);
    assert_eq!(row["completions_not_idle"], 0);
    // The withheld audio is counted here and nowhere else.
    assert_eq!(report["received_audio_samples"], json!([240, 240]));
}

#[tokio::test(start_paused = true)]
async fn a_barrier_that_times_out_reports_what_arrived_before_it_did() {
    let (connection, mut service) = mock().await;
    let first = vec![1; 8_000];
    let second = vec![2; 64_000];
    let client = trial(
        connection,
        &first,
        &second,
        Duration::ZERO,
        "continue-during-output",
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        service.send(audio(5, 240)).await;
        until(&mut service, "activityStart").await;
        service.send(audio(6, 240)).await;
        while let Some(message) = service.try_receive(Duration::from_secs(3)).await {
            assert!(message["realtimeInput"].get("activityEnd").is_none());
        }
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "strict_barrier_timeout");
    let rows = barrier_rows(&report);
    assert_eq!(rows.len(), 1);
    let row = rows[0];
    assert_eq!(row["outcome"], "TimedOut");
    assert_eq!(row["superseded"], 1);
    assert_eq!(row["successor"], 2);
    assert_eq!(row["audio_frames"], 1);
    assert_eq!(row["audio_samples"], 240);
    assert_eq!(row["interrupted"], 0);
    // Nothing arrived for two seconds: that, and no terminal, ended it.
    assert!(row["quiet_ms"].as_u64().unwrap() >= 2_000);
}

async fn mock_at_commit() -> (Connection, FakeService) {
    let config = config(offline(), BARRIER, BarrierWait::AtCommit).unwrap();
    let (connecting, mut service) = pair(config, SessionId::new(1).unwrap(), 1_048_576).await;
    let connecting = tokio::spawn(connecting);
    service.accept().await;
    (connecting.await.unwrap().unwrap(), service)
}

#[test]
fn a_held_end_is_parsed_last_bounded_and_removed() {
    let arguments = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    let mut plain = arguments(&["p", "a.wav", "continue-before-output", "b.wav"]);
    assert_eq!(end_hold(&mut plain).unwrap(), Duration::ZERO);
    assert_eq!(plain.len(), 4);
    let mut held = arguments(&[
        "p",
        "a.wav",
        "continue-before-output",
        "b.wav",
        "barrier-wait=at-commit",
        "hold-end-ms=8000",
    ]);
    assert_eq!(end_hold(&mut held).unwrap(), Duration::from_secs(8));
    assert_eq!(barrier_wait(&mut held).unwrap(), BarrierWait::AtCommit);
    assert_eq!(held.len(), 4);
    for bad in ["hold-end-ms=0", "hold-end-ms=20001", "hold-end-ms=long"] {
        let mut arguments = arguments(&["p", "a.wav", "repeat-before-output", bad]);
        assert!(end_hold(&mut arguments).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn a_held_end_without_the_at_commit_wait_is_refused_before_any_file_is_opened() {
    let arguments = [
        "/nonexistent/provider.json",
        "/nonexistent/a.wav",
        "continue-before-output",
        "/nonexistent/b.wav",
        "hold-end-ms=8000",
    ]
    .map(String::from);
    let error = run(&arguments).await.unwrap_err().to_string();
    assert!(error.contains("barrier-wait=at-commit"), "{error}");
}

#[tokio::test(start_paused = true)]
async fn a_held_end_keeps_watching_and_sees_a_terminal_later_than_any_quiet_bound() {
    let (connection, mut service) = mock_at_commit().await;
    let first = vec![111; 8_000];
    let second = vec![222; 16_000];
    let client = trial_holding(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
        Duration::from_millis(6_500),
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        until(&mut service, "activityStart").await;
        // Seven seconds with nothing for the earlier response: longer than
        // the longest quiet bound. No End is written meanwhile.
        let late = Instant::now() + Duration::from_secs(7);
        while let Some(left) = late.checked_duration_since(Instant::now()) {
            if left.is_zero() {
                break;
            }
            if let Some(message) = service.try_receive(left).await {
                assert!(message["realtimeInput"].get("activityEnd").is_none());
            }
        }
        service
            .send(json!({"serverContent":{"interrupted":true}}))
            .await;
        service.send(idle()).await;
        until(&mut service, "activityEnd").await;
        service.send(audio(333, 240)).await;
        service.send(idle()).await;
        service.rest(Duration::from_secs(9)).await;
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "strict_barrier_confirmed_and_answered");
    let start = report["inputs"][1]["start_queued_us"].as_u64().unwrap();
    assert!(report["idle_observed_us"][0].as_u64().unwrap() >= start + 7_000_000);
    // The End was the probe's to keep back: one second of upload, then six
    // and a half held.
    assert!(report["inputs"][1]["end_queued_us"].as_u64().unwrap() >= start + 7_500_000);
    let rows = barrier_rows(&report);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["outcome"], "Settled");
    assert_eq!(rows[0]["successor_input_open"], true);
}

#[tokio::test(start_paused = true)]
async fn a_held_end_with_no_terminal_times_out_only_after_the_end_is_asked_for() {
    let (connection, mut service) = mock_at_commit().await;
    let first = vec![1; 8_000];
    let second = vec![2; 16_000];
    let client = trial_holding(
        connection,
        &first,
        &second,
        Duration::from_millis(300),
        "continue-before-output",
        Duration::from_secs(8),
    );
    let server = async {
        until(&mut service, "activityEnd").await;
        while let Some(message) = service.try_receive(Duration::from_secs(12)).await {
            assert!(message["realtimeInput"].get("activityEnd").is_none());
        }
    };
    let (report, ()) = tokio::join!(client, server);
    assert_eq!(report["outcome"], "strict_barrier_timeout");
    let rows = barrier_rows(&report);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["outcome"], "TimedOut");
    assert_eq!(rows[0]["audio_frames"], 0);
    assert_eq!(rows[0]["interrupted"], 0);
    // One second of upload, eight held, then the two second bound.
    let waited = rows[0]["waited_ms"].as_u64().unwrap();
    assert!((11_000..12_000).contains(&waited), "{waited}");
    let withheld = rows[0]["end_withheld_ms"].as_u64().unwrap();
    assert!((2_000..2_100).contains(&withheld), "{withheld}");
}
