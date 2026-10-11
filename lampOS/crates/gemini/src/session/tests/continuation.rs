//! The barrier's quiet bound counted from where something is waiting on it
//! (`BarrierWait::AtCommit`), and the report of how a barrier ended. The
//! policy is `Require` throughout: nothing here assumes a settlement.
use super::*;
use crate::{BarrierOutcome, BarrierReport, BarrierWait};

/// A barrier bound short enough to outlast in a test, reports on.
fn at_commit(wait: BarrierWait) -> SessionConfig {
    configuration(Timeouts {
        barrier: Duration::from_millis(60),
        ..Timeouts::default()
    })
    .barrier_wait(wait)
    .barrier_reports(true)
}

/// Request 1 committed and request 2 started behind it, its opening audio
/// sent. Returns request 2.
async fn continued(connection: &mut Connection, server: &mut MockSocket) -> RequestId {
    ended_turn(connection, server, 1).await;
    let new = request(2);
    let input = connection.input();
    input.try_start(new).unwrap();
    input.try_audio(new, 0, Instant::now(), &[2; 160]).unwrap();
    sent(server, "activityStart").await;
    sent(server, "audio").await;
    assert!(matches!(
        event(connection).await,
        Event::InputStarted { lineage, waiting_for_barrier: true } if lineage.request == new
    ));
    new
}

async fn barrier(connection: &mut Connection) -> (BarrierOutcome, BarrierReport) {
    match event(connection).await {
        Event::Barrier {
            outcome, report, ..
        } => (outcome, report),
        other => panic!("expected a barrier report, got {other:?}"),
    }
}

#[tokio::test]
async fn a_person_still_speaking_is_not_cut_off_by_the_quiet_bound() {
    let (mut connection, mut server) = mock_with(at_commit(BarrierWait::AtCommit)).await;
    let new = continued(&mut connection, &mut server).await;
    let input = connection.input();
    // Four times the bound, the earlier response silent throughout.
    for sequence in 1..=24 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        input
            .try_audio(new, sequence, Instant::now(), &[2; 160])
            .unwrap();
        sent(&mut server, "audio").await;
    }
    assert!(!matches!(connection.state(), State::Disconnected(_)));
    // Their End is withheld, and only now does the strict wait run.
    input.try_end(new).unwrap();
    silent(&mut server).await;
    let (outcome, report) = barrier(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::TimedOut);
    assert_eq!(
        (report.superseded, report.successor),
        (Some(request(1)), Some(new))
    );
    assert!(!report.successor_input_open);
    let withheld = report.end_withheld.unwrap();
    assert!(
        (Duration::from_millis(60)..Duration::from_millis(200)).contains(&withheld),
        "{withheld:?}"
    );
    assert!(report.waited >= Duration::from_millis(300), "{report:?}");
    // Nothing arrived for the earlier response at all.
    assert_eq!(
        (
            report.audio_frames,
            report.text_frames,
            report.generation_complete,
            report.interrupted,
            report.completions_not_idle
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(disconnected(&connection).await, Error::BarrierTimeout);
    // No activityEnd was ever written for the request that could not be owned.
    assert!(connection.next_event().await.is_none());
}

#[tokio::test]
async fn the_same_person_is_cut_off_as_before_without_the_setting() {
    let (mut connection, mut server) = mock_with(at_commit(BarrierWait::FromInterruption)).await;
    let new = continued(&mut connection, &mut server).await;
    let (outcome, report) = barrier(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::TimedOut);
    assert!(report.successor_input_open, "cut off mid-utterance");
    assert_eq!(report.end_withheld, None);
    assert_eq!(report.successor, Some(new));
    assert_eq!(disconnected(&connection).await, Error::BarrierTimeout);
}

#[tokio::test]
async fn a_terminal_during_the_next_utterance_settles_and_its_end_commits_at_once() {
    let (mut connection, mut server) = mock_with(at_commit(BarrierWait::AtCommit)).await;
    let new = continued(&mut connection, &mut server).await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    // Old output, then its interruption and idle completion: dropped,
    // counted, and reported under the old request's own name.
    send_json(&mut server, audio(7)).await;
    send_json(&mut server, audio(8)).await;
    send_json(&mut server, json!({"serverContent":{"interrupted":true}})).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Interrupted { lineage } if lineage.request == request(1)
    ));
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    let (outcome, report) = barrier(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    assert_eq!((report.audio_frames, report.audio_samples), (2, 2));
    assert_eq!((report.interrupted, report.completions_not_idle), (1, 0));
    assert!(report.successor_input_open);
    assert_eq!(report.end_withheld, None);
    // Settled: the End is not withheld when it comes.
    connection.input().try_end(new).unwrap();
    sent(&mut server, "activityEnd").await;
    send_json(&mut server, audio(222)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, pcm, .. } if lineage.request == new && pcm == [222]
    ));
}

#[tokio::test]
async fn a_terminal_while_the_end_is_withheld_commits_it_and_old_output_is_never_the_new_requests()
{
    let (mut connection, mut server) = mock_with(
        at_commit(BarrierWait::AtCommit)
            .timeouts(Timeouts {
                barrier: Duration::from_millis(400),
                ..Timeouts::default()
            })
            .unwrap(),
    )
    .await;
    let new = continued(&mut connection, &mut server).await;
    connection.input().try_end(new).unwrap();
    silent(&mut server).await;
    // The earlier response is still producing after the End was asked for.
    send_json(&mut server, audio(9)).await;
    quiet(&mut connection).await;
    send_json(
        &mut server,
        json!({"serverContent":{"turnComplete":true,"interactionStatus":"IN_PROGRESS"}}),
    )
    .await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: false } if lineage.request == request(1)
    ));
    silent(&mut server).await;
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    let (outcome, report) = barrier(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::Settled);
    assert_eq!((report.audio_frames, report.completions_not_idle), (1, 1));
    assert!(report.end_withheld.is_some() && !report.successor_input_open);
    sent(&mut server, "activityEnd").await;
    send_json(&mut server, audio(333)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, pcm, .. } if lineage.request == new && pcm == [333]
    ));
}

#[tokio::test]
async fn the_absolute_bound_still_ends_a_barrier_nothing_settles() {
    // However long the next person speaks, an earlier response unsettled for
    // `Timeouts::response` ends the connection.
    let (mut connection, mut server) = mock_with(
        at_commit(BarrierWait::AtCommit)
            .timeouts(Timeouts {
                barrier: Duration::from_millis(60),
                response: Duration::from_millis(250),
                ..Timeouts::default()
            })
            .unwrap(),
    )
    .await;
    let new = continued(&mut connection, &mut server).await;
    let (outcome, report) = barrier(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::TimedOut);
    assert!(report.successor_input_open);
    assert!(report.waited >= Duration::from_millis(250), "{report:?}");
    assert_eq!(report.successor, Some(new));
    assert_eq!(disconnected(&connection).await, Error::BarrierTimeout);
}

#[tokio::test]
async fn a_cancelled_successor_leaves_nothing_waiting_and_a_later_one_waits_strictly() {
    let (mut connection, mut server) = mock_with(at_commit(BarrierWait::AtCommit)).await;
    let second = continued(&mut connection, &mut server).await;
    let input = connection.input();
    // The second request is cancelled before it ended: its End drops it,
    // nothing of it is committed, and nothing waits on the earlier response.
    input.retire(second);
    input.try_end(second).unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!matches!(connection.state(), State::Disconnected(_)));
    silent(&mut server).await;
    // A third request does end, and then the strict wait runs for it.
    let third = request(3);
    input.try_start(third).unwrap();
    assert!(matches!(
        event(&mut connection).await,
        Event::InputStarted { lineage, waiting_for_barrier: true } if lineage.request == third
    ));
    input
        .try_audio(third, 0, Instant::now(), &[3; 160])
        .unwrap();
    sent(&mut server, "audio").await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!matches!(connection.state(), State::Disconnected(_)));
    input.try_end(third).unwrap();
    let (outcome, report) = barrier(&mut connection).await;
    assert_eq!(outcome, BarrierOutcome::TimedOut);
    assert_eq!(
        (report.superseded, report.successor),
        (Some(request(1)), Some(third))
    );
    assert!(report.end_withheld.is_some());
    assert_eq!(disconnected(&connection).await, Error::BarrierTimeout);
}

#[tokio::test]
async fn a_stop_under_a_withheld_end_is_the_stop_and_not_a_timeout() {
    let (mut connection, mut server) = mock_with(
        at_commit(BarrierWait::AtCommit)
            .timeouts(Timeouts {
                barrier: Duration::from_secs(2),
                ..Timeouts::default()
            })
            .unwrap(),
    )
    .await;
    let new = continued(&mut connection, &mut server).await;
    connection.input().try_end(new).unwrap();
    silent(&mut server).await;
    connection.shutdown();
    // Stopped by its owner while the End was withheld: the stop is what
    // ended it, no barrier outcome is invented, and no activityEnd is
    // written on the way out.
    while let Some(event) = timeout(Duration::from_secs(2), connection.next_event())
        .await
        .unwrap()
    {
        assert!(!matches!(event, Event::Barrier { .. }), "{event:?}");
    }
    assert_eq!(connection.state(), State::Closed);
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
async fn without_reports_the_event_stream_is_what_it_was() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let new = continued(&mut connection, &mut server).await;
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));
    connection.input().try_end(new).unwrap();
    sent(&mut server, "activityEnd").await;
    // The next event is the new request's own, with no report before it.
    send_json(&mut server, audio(5)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, .. } if lineage.request == new
    ));
}
