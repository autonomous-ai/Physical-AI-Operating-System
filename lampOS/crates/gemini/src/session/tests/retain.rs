//! A next request started while a committed request's response has produced
//! nothing: its `activityStart` and its audio are kept here until the earlier
//! response produces or completes (`SuccessorStart::AfterFirstOutput`).
//!
//! The policy is `Require` throughout and nothing is assumed. Real time,
//! with input paced as it would be spoken where the pacing matters.
use super::*;
use crate::{
    BarrierOutcome, BarrierReport, MAX_RETAINED_SAMPLES, MAX_SUCCESSOR_PATIENCE, StartRelease,
    SuccessorStart,
};

/// Reports on, the start kept for at most `patience`. Production's two
/// second barrier bound, so that a busy host cannot time a test out.
fn keeping(patience: Duration) -> SessionConfig {
    keeping_with(patience, Timeouts::default())
}

fn keeping_with(patience: Duration, timeouts: Timeouts) -> SessionConfig {
    configuration(timeouts)
        .barrier_reports(true)
        .successor_start(SuccessorStart::AfterFirstOutput { patience })
        .unwrap()
}

/// Request 1 committed with no output yet, then `next` started behind it.
/// Nothing is written for it.
async fn kept(connection: &mut Connection, server: &mut MockSocket, next: u64) -> RequestId {
    ended_turn(connection, server, 1).await;
    started(connection, server, next).await
}

async fn started(connection: &mut Connection, server: &mut MockSocket, next: u64) -> RequestId {
    let next = request(next);
    connection.input().try_start(next).unwrap();
    assert!(matches!(
        event(connection).await,
        Event::InputStarted { lineage, waiting_for_barrier: true } if lineage.request == next
    ));
    silent(server).await;
    next
}

/// One block of twenty milliseconds whose every sample is `value`.
fn speak(connection: &Connection, request: RequestId, sequence: u64, value: i16) {
    connection
        .input()
        .try_audio(request, sequence, Instant::now(), &[value; 320])
        .unwrap();
}

/// The next message is an audio block; its first sample.
async fn heard(server: &mut MockSocket) -> i16 {
    let message = receive(server).await;
    let data = message["realtimeInput"]["audio"]["data"]
        .as_str()
        .unwrap_or_else(|| panic!("expected audio, received {message}"));
    let bytes = STANDARD.decode(data).unwrap();
    assert_eq!(bytes.len(), 640);
    i16::from_le_bytes([bytes[0], bytes[1]])
}

async fn heard_in_order(server: &mut MockSocket, values: impl IntoIterator<Item = i16>) {
    for value in values {
        assert_eq!(heard(server).await, value);
    }
}

async fn report(connection: &mut Connection) -> (BarrierOutcome, BarrierReport) {
    match event(connection).await {
        Event::Barrier {
            outcome, report, ..
        } => (outcome, report),
        other => panic!("expected a barrier report, got {other:?}"),
    }
}

/// The earlier response is confirmed stopped: `interrupted`, then idle.
async fn confirmed(connection: &mut Connection, server: &mut MockSocket) -> BarrierReport {
    send_json(server, json!({"serverContent":{"interrupted":true}})).await;
    assert!(matches!(
        event(connection).await,
        Event::Interrupted { lineage } if lineage.request == request(1)
    ));
    send_json(server, idle()).await;
    assert!(matches!(
        event(connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    let (outcome, report) = report(connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    report
}

#[tokio::test]
async fn the_opening_words_wait_for_the_earlier_answer_to_begin_and_then_follow_in_order() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let next = kept(&mut connection, &mut server, 2).await;
    // A third of a second of speech, as it is spoken. None of it is written.
    for sequence in 0..15 {
        speak(&connection, next, sequence, sequence as i16);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    silent(&mut server).await;
    // The earlier answer begins: now there is something to interrupt.
    send_json(&mut server, audio(7)).await;
    sent(&mut server, "activityStart").await;
    heard_in_order(&mut server, 0..15).await;
    // Its audio is never the next request's.
    quiet(&mut connection).await;
    // What is spoken from here goes straight out, behind what was kept.
    for sequence in 15..20 {
        speak(&connection, next, sequence, sequence as i16);
    }
    heard_in_order(&mut server, 15..20).await;
    connection.input().try_end(next).unwrap();
    silent(&mut server).await;
    let report = confirmed(&mut connection, &mut server).await;
    assert_eq!(report.start_released_by, Some(StartRelease::FirstOutput));
    assert!(
        report.start_held.unwrap() >= Duration::from_millis(300),
        "{report:?}"
    );
    assert_eq!(report.retained_samples, 15 * 320);
    assert_eq!((report.audio_frames, report.interrupted), (1, 1));
    sent(&mut server, "activityEnd").await;
    send_json(&mut server, audio(222)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, pcm, .. } if lineage.request == next && pcm == [222]
    ));
    silent(&mut server).await;
}

#[tokio::test]
async fn an_earlier_answer_that_completes_with_nothing_lets_the_next_request_start_normally() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let next = kept(&mut connection, &mut server, 2).await;
    for sequence in 0..10 {
        speak(&connection, next, sequence, sequence as i16);
    }
    connection.input().try_end(next).unwrap();
    silent(&mut server).await;
    // No output at all: an idle completion. Nothing was interrupted.
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    let (outcome, report) = report(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    assert_eq!(report.start_released_by, Some(StartRelease::Completion));
    assert_eq!(report.retained_samples, 10 * 320);
    assert_eq!((report.audio_frames, report.interrupted), (0, 0));
    // The start, every kept block, and only then the End.
    sent(&mut server, "activityStart").await;
    heard_in_order(&mut server, 0..10).await;
    sent(&mut server, "activityEnd").await;
    send_json(&mut server, audio(5)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, pcm, .. } if lineage.request == next && pcm == [5]
    ));
}

#[tokio::test]
async fn two_continuations_are_kept_in_order_behind_one_start_and_one_end() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let second = kept(&mut connection, &mut server, 2).await;
    for sequence in 0..5 {
        speak(&connection, second, sequence, sequence as i16);
    }
    connection.input().try_end(second).unwrap();
    silent(&mut server).await;
    // The person goes on again before anything has been answered.
    let third = started(&mut connection, &mut server, 3).await;
    for sequence in 0..5 {
        speak(&connection, third, sequence, 100 + sequence as i16);
    }
    silent(&mut server).await;
    send_json(&mut server, audio(7)).await;
    sent(&mut server, "activityStart").await;
    heard_in_order(&mut server, (0..5).chain(100..105)).await;
    let report = confirmed(&mut connection, &mut server).await;
    assert_eq!(
        (report.superseded, report.successor),
        (Some(request(1)), Some(third))
    );
    assert_eq!(report.retained_samples, 10 * 320);
    // The second request's End was never written; the third's is the one.
    silent(&mut server).await;
    connection.input().try_end(third).unwrap();
    sent(&mut server, "activityEnd").await;
    send_json(&mut server, audio(33)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, pcm, .. } if lineage.request == third && pcm == [33]
    ));
    silent(&mut server).await;
}

#[tokio::test]
async fn a_long_next_utterance_is_kept_only_until_the_answer_begins_and_loses_nothing() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let next = kept(&mut connection, &mut server, 2).await;
    // A second and a half of speech at the rate it is spoken. The earlier
    // answer begins 400 ms in.
    let speaking = async {
        for sequence in 0..75 {
            speak(&connection, next, sequence, sequence as i16);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let service = async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        send_json(&mut server, audio(7)).await;
        sent(&mut server, "activityStart").await;
        // Interrupted while producing, the service confirms at once.
        send_json(&mut server, json!({"serverContent":{"interrupted":true}})).await;
        send_json(&mut server, idle()).await;
        // Every block exactly once and in order: the kept ones at once,
        // the rest as they are spoken.
        heard_in_order(&mut server, 0..75).await;
    };
    tokio::join!(speaking, service);
    assert!(matches!(
        event(&mut connection).await,
        Event::Interrupted { lineage } if lineage.request == request(1)
    ));
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    let (outcome, report) = report(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    // Still speaking when it settled: the End is not withheld when it comes.
    assert!(report.successor_input_open);
    silent(&mut server).await;
    connection.input().try_end(next).unwrap();
    assert_eq!(report.start_released_by, Some(StartRelease::FirstOutput));
    let held = report.start_held.unwrap();
    assert!(
        (Duration::from_millis(350)..Duration::from_millis(900)).contains(&held),
        "{held:?}"
    );
    sent(&mut server, "activityEnd").await;
}

#[tokio::test]
async fn patience_runs_out_and_the_start_is_written_with_the_strict_wait_from_there() {
    let (mut connection, mut server) = mock_with(keeping_with(
        Duration::from_millis(150),
        Timeouts {
            barrier: Duration::from_millis(300),
            ..Timeouts::default()
        },
    ))
    .await;
    let next = kept(&mut connection, &mut server, 2).await;
    for sequence in 0..5 {
        speak(&connection, next, sequence, sequence as i16);
    }
    let started = Instant::now();
    // Nothing from the earlier response. After the patience the start and
    // the kept audio go out anyway: the default behavior from here.
    sent(&mut server, "activityStart").await;
    assert!(started.elapsed() >= Duration::from_millis(100));
    heard_in_order(&mut server, 0..5).await;
    connection.input().try_end(next).unwrap();
    // And its failure, said truthfully: no confirmation, no End, no guess.
    let (outcome, report) = report(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::TimedOut);
    assert_eq!(report.start_released_by, Some(StartRelease::Patience));
    assert!(report.start_held.unwrap() >= Duration::from_millis(150));
    assert_eq!(disconnected(&connection).await, Error::BarrierTimeout);
    while let Ok(Some(Ok(message))) = timeout(Duration::from_millis(40), server.next()).await {
        if let Ok(value) = serde_json::from_slice::<Value>(&message.into_data()) {
            assert!(
                value["realtimeInput"].get("activityEnd").is_none(),
                "{value}"
            );
        }
    }
}

#[tokio::test]
async fn more_than_may_be_kept_is_written_at_once_and_in_order() {
    let (mut connection, mut server) = mock_with(keeping(MAX_SUCCESSOR_PATIENCE)).await;
    let next = kept(&mut connection, &mut server, 2).await;
    let input = connection.input();
    // Faster than speech, as a retained prefix arrives: one block past
    // what may be kept.
    let blocks = MAX_RETAINED_SAMPLES / 320 + 1;
    for sequence in 0..blocks {
        speak(
            &connection,
            next,
            sequence as u64,
            (sequence % 30_000) as i16,
        );
        if sequence % 16 == 15 {
            timeout(Duration::from_secs(2), async {
                while input.commands.capacity() != INPUT_QUEUE_CAPACITY {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        }
    }
    sent(&mut server, "activityStart").await;
    heard_in_order(
        &mut server,
        (0..blocks).map(|sequence| (sequence % 30_000) as i16),
    )
    .await;
    let report = confirmed(&mut connection, &mut server).await;
    assert_eq!(report.start_released_by, Some(StartRelease::Overflow));
    assert_eq!(report.retained_samples, (blocks * 320) as u64);
}

#[tokio::test]
async fn events_of_the_earlier_answer_that_are_not_output_do_not_release_the_start() {
    let (mut connection, mut server) =
        mock_with(keeping(Duration::from_secs(2)).resumption(Resumption::Retain)).await;
    let next = kept(&mut connection, &mut server, 2).await;
    speak(&connection, next, 0, 1);
    // An interruption nobody asked for, and a completion that is not idle:
    // reported under the earlier request's name, and nothing is written.
    send_json(&mut server, json!({"serverContent":{"interrupted":true}})).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Interrupted { lineage } if lineage.request == request(1)
    ));
    send_json(
        &mut server,
        json!({"serverContent":{"turnComplete":true,"interactionStatus":"IN_PROGRESS"}}),
    )
    .await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: false } if lineage.request == request(1)
    ));
    // The documented `waitingForInput`, and a resumption update: counted.
    send_json(
        &mut server,
        json!({"serverContent":{"waitingForInput":true}}),
    )
    .await;
    send_json(
        &mut server,
        json!({"sessionResumptionUpdate":{"resumable":false}}),
    )
    .await;
    silent(&mut server).await;
    // Output of the earlier response is what releases it.
    send_json(&mut server, audio(9)).await;
    sent(&mut server, "activityStart").await;
    heard_in_order(&mut server, [1]).await;
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    let (outcome, report) = report(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    assert_eq!(report.start_released_by, Some(StartRelease::FirstOutput));
    assert_eq!(
        (
            report.interrupted,
            report.completions_not_idle,
            report.waiting_for_input,
            report.unresumable_updates,
            report.resumable_updates
        ),
        (1, 1, 1, 1, 0)
    );
}

#[tokio::test]
async fn a_kept_request_that_is_cancelled_is_never_committed_and_what_was_kept_stays_in_order() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let next = kept(&mut connection, &mut server, 2).await;
    for sequence in 0..5 {
        speak(&connection, next, sequence, sequence as i16);
    }
    let input = connection.input();
    // Taken by the session, and so kept, before the cancellation. Audio
    // handed in after a cancellation is dropped, as it always was.
    timeout(Duration::from_secs(1), async {
        while input.commands.capacity() != INPUT_QUEUE_CAPACITY {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    input.retire(next);
    input.try_end(next).unwrap();
    silent(&mut server).await;
    // The earlier answer begins and is interrupted all the same. What was
    // kept follows in order, as on the default path, where it would
    // already be on the wire: the session cannot tell a cancellation from
    // a continuation, so it does not drop words on that guess.
    send_json(&mut server, audio(7)).await;
    sent(&mut server, "activityStart").await;
    heard_in_order(&mut server, 0..5).await;
    let report = confirmed(&mut connection, &mut server).await;
    assert_eq!(
        (report.superseded, report.successor),
        (Some(request(1)), None)
    );
    // No End for a request nobody owns.
    silent(&mut server).await;
    assert_eq!(connection.state(), State::Ready);
}

#[tokio::test]
async fn a_stop_while_the_opening_is_kept_writes_none_of_it() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let next = kept(&mut connection, &mut server, 2).await;
    for sequence in 0..20 {
        speak(&connection, next, sequence, sequence as i16);
    }
    connection.input().try_end(next).unwrap();
    silent(&mut server).await;
    connection.shutdown();
    while let Some(event) = timeout(Duration::from_secs(2), connection.next_event())
        .await
        .unwrap()
    {
        assert!(!matches!(event, Event::Barrier { .. }), "{event:?}");
    }
    assert_eq!(connection.state(), State::Closed);
    // Kept, and then dropped: not a start, not a block, not an End.
    no_input_was_written(&mut server).await;
}

#[tokio::test]
async fn a_stop_while_kept_audio_is_being_written_ends_the_writing() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let next = kept(&mut connection, &mut server, 2).await;
    let input = connection.input();
    for sequence in 0..240_u64 {
        speak(&connection, next, sequence, (sequence % 30_000) as i16);
        if sequence % 16 == 15 {
            timeout(Duration::from_secs(2), async {
                while input.commands.capacity() != INPUT_QUEUE_CAPACITY {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        }
    }
    silent(&mut server).await;
    send_json(&mut server, audio(7)).await;
    sent(&mut server, "activityStart").await;
    // Writing has begun. The stop ends it between two blocks.
    connection.shutdown();
    timeout(Duration::from_secs(2), &mut connection.task)
        .await
        .unwrap()
        .unwrap();
    let mut written = 0;
    while let Ok(Some(Ok(message))) = timeout(Duration::from_millis(40), server.next()).await {
        if let Ok(value) = serde_json::from_slice::<Value>(&message.into_data()) {
            assert!(value["realtimeInput"].get("audio").is_some(), "{value}");
            written += 1;
        }
    }
    assert!(written < 240, "all of it was written after the stop");
}

#[tokio::test]
async fn the_service_closing_for_quota_while_the_opening_is_kept_ends_as_that_and_writes_nothing() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    let next = kept(&mut connection, &mut server, 2).await;
    for sequence in 0..5 {
        speak(&connection, next, sequence, sequence as i16);
    }
    silent(&mut server).await;
    server
        .close(Some(tungstenite::protocol::CloseFrame {
            code: tungstenite::protocol::frame::coding::CloseCode::Policy,
            reason: "quota exceeded".into(),
        }))
        .await
        .unwrap();
    assert_eq!(disconnected(&connection).await, Error::QuotaExceeded);
    // That is what ended it. No barrier outcome is invented, and the
    // request whose opening was kept is given nothing.
    while let Some(event) = timeout(Duration::from_secs(2), connection.next_event())
        .await
        .unwrap()
    {
        assert!(
            !matches!(
                event,
                Event::Barrier { .. } | Event::Audio { .. } | Event::TurnComplete { .. }
            ),
            "{event:?}"
        );
    }
    no_input_was_written(&mut server).await;
}

#[tokio::test]
async fn a_consumer_that_takes_no_events_does_not_keep_the_opening_back() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    ended_turn(&mut connection, &mut server, 1).await;
    let next = request(2);
    connection.input().try_start(next).unwrap();
    for sequence in 0..10 {
        speak(&connection, next, sequence, sequence as i16);
    }
    connection.input().try_end(next).unwrap();
    silent(&mut server).await;
    // Not one event has been taken since the first request ended. The
    // release, the confirmation and the commit happen all the same.
    send_json(&mut server, audio(7)).await;
    sent(&mut server, "activityStart").await;
    heard_in_order(&mut server, 0..10).await;
    send_json(&mut server, json!({"serverContent":{"interrupted":true}})).await;
    send_json(&mut server, idle()).await;
    sent(&mut server, "activityEnd").await;
    send_json(&mut server, audio(222)).await;
    // Taken late, the events are all there and in order.
    assert!(matches!(
        event(&mut connection).await,
        Event::InputStarted { lineage, waiting_for_barrier: true } if lineage.request == next
    ));
    assert!(matches!(
        event(&mut connection).await,
        Event::Interrupted { lineage } if lineage.request == request(1)
    ));
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    let (outcome, report) = report(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    assert_eq!(report.start_released_by, Some(StartRelease::FirstOutput));
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, pcm, .. } if lineage.request == next && pcm == [222]
    ));
}

#[tokio::test]
async fn an_answer_that_has_already_begun_is_interrupted_at_once_as_before() {
    let (mut connection, mut server) = mock_with(keeping(Duration::from_secs(2))).await;
    ended_turn(&mut connection, &mut server, 1).await;
    send_json(&mut server, audio(1)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, .. } if lineage.request == request(1)
    ));
    let next = request(2);
    connection.input().try_start(next).unwrap();
    // Something to interrupt: nothing is kept.
    sent(&mut server, "activityStart").await;
    speak(&connection, next, 0, 3);
    heard_in_order(&mut server, [3]).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::InputStarted { lineage, waiting_for_barrier: true } if lineage.request == next
    ));
    let report = confirmed(&mut connection, &mut server).await;
    assert_eq!((report.start_held, report.start_released_by), (None, None));
}

#[tokio::test]
async fn without_the_setting_the_start_is_written_at_once_and_reports_say_nothing_was_kept() {
    let (mut connection, mut server) =
        mock_with(configuration(Timeouts::default()).barrier_reports(true)).await;
    ended_turn(&mut connection, &mut server, 1).await;
    let next = request(2);
    connection.input().try_start(next).unwrap();
    sent(&mut server, "activityStart").await;
    assert!(matches!(
        event(&mut connection).await,
        Event::InputStarted { lineage, waiting_for_barrier: true } if lineage.request == next
    ));
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { idle: true, .. }
    ));
    let (outcome, report) = report(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    assert_eq!(
        (
            report.start_held,
            report.start_released_by,
            report.retained_samples
        ),
        (None, None, 0)
    );
}

#[test]
fn the_patience_is_bounded_and_off_is_the_default() {
    let base = || configuration(Timeouts::default());
    assert_eq!(base().successor_start, SuccessorStart::AtOnce);
    for patience in [Duration::from_millis(1), MAX_SUCCESSOR_PATIENCE] {
        assert!(
            base()
                .successor_start(SuccessorStart::AfterFirstOutput { patience })
                .is_ok()
        );
    }
    for patience in [
        Duration::ZERO,
        MAX_SUCCESSOR_PATIENCE + Duration::from_millis(1),
    ] {
        assert!(
            base()
                .successor_start(SuccessorStart::AfterFirstOutput { patience })
                .is_err()
        );
    }
}

/// The reviewer's path. The earlier response's idle completion takes the
/// barrier down, the kept start and audio are written, and the withheld End
/// follows. Those writes are awaited: a retirement recorded while one of
/// them waits on the transport must still be honored before the End.
async fn retired_while_the_kept_start_is_being_written(supersede: bool) {
    use super::stop_fence::controlled_with;
    let (mut connection, mut server, control) = controlled_with(keeping_with(
        Duration::from_secs(2),
        Timeouts {
            write: Duration::from_secs(1),
            ..Timeouts::default()
        },
    ))
    .await;
    let next = kept(&mut connection, &mut server, 2).await;
    for sequence in 0..3 {
        speak(&connection, next, sequence, sequence as i16);
    }
    let input = connection.input();
    input.try_end(next).unwrap();
    silent(&mut server).await;
    // The transport holds the next write: the kept start, which the idle
    // completion releases.
    control.hold.store(true, Ordering::Release);
    send_json(&mut server, idle()).await;
    timeout(Duration::from_secs(1), control.held.notified())
        .await
        .unwrap();
    // While that write waits, the request is retired: cancelled, or
    // superseded by a third.
    if supersede {
        input.try_start(request(3)).unwrap();
    } else {
        input.retire(next);
    }
    control.release();
    // The start and the kept audio go out, as documented for a request
    // retired while kept. Its End does not: it would commit a turn that
    // was retired before any byte of that End was written.
    sent(&mut server, "activityStart").await;
    heard_in_order(&mut server, 0..3).await;
    if let Ok(Some(Ok(message))) = timeout(Duration::from_millis(60), server.next()).await {
        let written: Value = serde_json::from_slice(&message.into_data()).unwrap();
        panic!("written for a request retired before it: {written}");
    }
    assert!(!matches!(connection.state(), State::Disconnected(_)));
    if supersede {
        // The third request continues the activity and is the one committed.
        speak(&connection, request(3), 0, 9);
        heard_in_order(&mut server, [9]).await;
        input.try_end(request(3)).unwrap();
        sent(&mut server, "activityEnd").await;
        silent(&mut server).await;
    }
}

#[tokio::test]
async fn a_request_cancelled_while_its_kept_start_is_being_written_is_not_committed() {
    retired_while_the_kept_start_is_being_written(false).await;
}

#[tokio::test]
async fn a_request_superseded_while_its_kept_start_is_being_written_is_not_committed() {
    retired_while_the_kept_start_is_being_written(true).await;
}
