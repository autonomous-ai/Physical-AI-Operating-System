//! Recovery under pressure from its own consumer and from a link that keeps
//! failing: an unread notice, a speaker that has stopped, replacements that
//! die on arrival, and input accepted before its connection was announced.
use super::{
    availability::{failed_source, finished_attempt, replacement_without_output, unread, update},
    *,
};

/// Upkeep on a control tick while nothing is read: neither a link update nor
/// output. Answers attempts the way a live service does, when they are made.
async fn tick_unread(rig: &mut Rig, span: Duration, mut serve: impl FnMut(&Dial) -> bool) -> u64 {
    let (mut dials, until) = (0, Instant::now() + span);
    while Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(2)).await;
        rig.0.maintain();
        if let Ok(mut dial) = rig.1.try_recv() {
            dials += 1;
            dial.service.receive().await;
            if serve(&dial) {
                dial.service.send(json!({"setupComplete": {}})).await;
                // Keep the accepted connection open for the rest of the test.
                std::mem::forget(dial);
            } else {
                dial.service.close(1011, "setup failed").await;
            }
        }
    }
    dials
}

#[tokio::test(start_paused = true)]
async fn refused_attempt_is_retried_while_the_outage_notice_is_still_unread() {
    // The speaker is stopped: the consumer takes no output and reads no link
    // update for longer than the whole outage budget. Only upkeep runs.
    let policy = quick();
    let (mut rig, failed_at, expected) = unread(true, policy).await;
    rig.0.maintain();
    let unavailable = rig.0.pending_link.unwrap();
    assert!(matches!(
        unavailable.kind,
        LinkUpdateKind::Unavailable { .. }
    ));
    // The first replacement attempt is refused; the next one is accepted.
    let dials = tick_unread(
        &mut rig,
        policy.outage_budget + Duration::from_secs(1),
        |dial| dial.session.get() > 2,
    )
    .await;
    assert_eq!(dials, 2, "the outage budget ran out on an unread notice");
    assert_eq!(
        rig.0.pending_link,
        Some(unavailable),
        "one update, still unread"
    );
    assert!(
        !rig.0.is_connected(),
        "no input before the outage has been read"
    );

    assert_eq!(rig.0.poll_link(), Some(unavailable));
    let ready = update(&mut rig.0).await;
    let LinkUpdateKind::Ready {
        attempts,
        completed_at,
        outage_started_at,
        ..
    } = ready.kind
    else {
        panic!("a recovery completed inside the budget was lost: {ready:?}")
    };
    assert_eq!((attempts, ready.session.get()), (2, 3));
    assert_eq!(outage_started_at, Some(failed_at));
    assert!(completed_at < failed_at + policy.outage_budget);
    assert!(rig.0.is_connected());
    // Nothing of the answer that was waiting all along is missing.
    let mut actual = Vec::new();
    loop {
        match timeout(WAIT, rig.0.next_output()).await.unwrap().unwrap() {
            OutputNotice::Event(Event::Audio { pcm, .. }) => actual.extend(pcm),
            OutputNotice::Event(Event::GenerationComplete { .. }) => {}
            OutputNotice::Settled { .. } => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(actual, expected);
    rig.0.shutdown();
}

#[tokio::test(start_paused = true)]
async fn unread_outage_notice_does_not_lift_the_attempt_or_budget_bounds() {
    // Every attempt is refused and nothing is ever read.
    let policy = RecoveryPolicy {
        max_attempts: 3,
        ..quick()
    };
    let (mut rig, _, _) = unread(false, policy).await;
    rig.0.maintain();
    let unavailable = rig.0.pending_link.unwrap();
    let dials = tick_unread(&mut rig, Duration::from_secs(5), |_| false).await;
    assert_eq!(dials, 3, "attempts are bounded by policy");
    assert_eq!(rig.0.poll_link(), Some(unavailable));
    let terminal = update(&mut rig.0).await;
    assert!(
        matches!(
            terminal.kind,
            LinkUpdateKind::Terminal {
                failure: Failure {
                    reason: FailureReason::AttemptsExhausted,
                    attempts: 3,
                    ..
                }
            }
        ),
        "{terminal:?}"
    );
    // Stamped when the last attempt failed, not when it was finally read.
    assert!(terminal.observed_at < Instant::now() - Duration::from_secs(4));
    rig.0.shutdown();
}

/// The transport gives up audio it still holds once its consumer has taken
/// nothing for `Timeouts::deliver`, on a failed connection as on a live one.
/// Returns the blocks delivered afterwards and how the request ended.
async fn stopped_speaker(
    stopped_for: Duration,
    blocks: i16,
) -> (Vec<i16>, OutputNotice, Rig, Dial) {
    let (mut rig, mut first) = connected(quick(), config()).await;
    speak(&mut rig, &mut first, 1, true).await;
    // More events than the event queue holds: the rest waits in the transport.
    for value in 1..=blocks {
        first.service.send(audio(value, 240)).await;
    }
    first
        .service
        .send(json!({"serverContent":{"generationComplete":true}}))
        .await;
    first.service.close(1011, "recoverable loss").await;
    failed_source(&mut rig.0).await;
    let (_, mut replacement) = replacement_without_output(&mut rig).await;
    // The speaker takes nothing. The link is still watched on every tick.
    let until = Instant::now() + stopped_for;
    while Instant::now() < until {
        assert!(
            replacement
                .service
                .rest(Duration::from_millis(100))
                .await
                .is_none()
        );
        assert!(rig.0.poll_link().is_none(), "the replacement stays up");
    }
    let mut heard = Vec::new();
    let outcome = loop {
        match timeout(WAIT, rig.0.next_output()).await.unwrap().unwrap() {
            OutputNotice::Event(Event::Audio { lineage, pcm, .. }) => {
                assert_eq!((lineage.session.get(), lineage.request.get()), (1, 1));
                assert!(pcm.len() == 240 && pcm.iter().all(|sample| *sample == pcm[0]));
                heard.push(pcm[0]);
            }
            OutputNotice::Event(_) => {}
            outcome => break outcome,
        }
    };
    (heard, outcome, rig, replacement)
}

#[tokio::test(start_paused = true)]
async fn answer_of_a_dead_connection_survives_a_stopped_speaker_only_up_to_the_delivery_bound() {
    const BLOCKS: i16 = 40;
    let bound = Timeouts::default().deliver;
    let margin = Duration::from_secs(1);

    // Inside the bound nothing is lost, however little was taken meanwhile.
    let (heard, outcome, mut rig, _replacement) = stopped_speaker(bound - margin, BLOCKS).await;
    assert_eq!(heard, (1..=BLOCKS).collect::<Vec<_>>());
    assert!(matches!(outcome, OutputNotice::Settled { lineage, .. } if lineage.request.get() == 1));
    rig.0.shutdown();

    // Past it the audio still held by the transport is gone. What had reached
    // the event queue is delivered as an unbroken beginning, and the request
    // is reported lost: a cut answer is never presented as a complete one.
    let (heard, outcome, mut rig, mut replacement) = stopped_speaker(bound + margin, BLOCKS).await;
    assert!(!heard.is_empty() && heard.len() <= crate::EVENT_QUEUE_CAPACITY);
    assert_eq!(heard, (1..=heard.len() as i16).collect::<Vec<_>>());
    assert!(
        matches!(
            outcome,
            OutputNotice::TurnLost {
                lineage,
                stage: Stage::Responding,
                error: Error::PeerClosed { code: Some(1011) },
            } if lineage.request.get() == 1 && lineage.session.get() == 1
        ),
        "{outcome:?}"
    );
    // The replacement was never part of it and serves the next request.
    assert!(rig.0.is_connected());
    assert_eq!(rig.0.try_start(request(1)), Err(Error::StaleRequest));
    rig.0.try_start(request(2)).unwrap();
    assert!(
        replacement.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    rig.0.shutdown();
}

#[tokio::test(start_paused = true)]
async fn request_accepted_before_its_connection_was_announced_is_lost_exactly_once() {
    let (mut rig, _, _) = unread(false, quick()).await;
    update(&mut rig.0).await;
    let mut second = next_dial(&mut rig.1).await;
    second.service.accept().await;
    finished_attempt(&rig.0).await;
    rig.0.maintain();
    let ready = rig.0.pending_link.unwrap();
    assert!(matches!(ready.kind, LinkUpdateKind::Ready { .. }));
    // The person interrupts the answer that is still playing from the dead
    // connection. The replacement is ready, so its Start is accepted at once,
    // before anyone has read that the replacement exists.
    rig.0.try_start(request(2)).unwrap();
    assert!(
        second.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    second.service.close(1011, "replacement loss").await;
    let failed_at = failed_source(&mut rig.0).await;
    let unavailable = update(&mut rig.0).await;
    assert!(matches!(
        unavailable.kind,
        LinkUpdateKind::Unavailable { .. }
    ));
    assert_eq!(unavailable.session.get(), 2);
    assert_eq!(unavailable.observed_at, failed_at);
    assert!(unavailable.revision > ready.revision);

    let mut third = next_dial(&mut rig.1).await;
    third.service.accept().await;
    finished_attempt(&rig.0).await;
    let recovered = update(&mut rig.0).await;
    // The handle is from session 1; input was accepted on session 2 since.
    assert!(
        matches!(
            recovered.kind,
            LinkUpdateKind::Ready {
                context: Context::ResumedBeforeLatest,
                ..
            }
        ),
        "{recovered:?}"
    );
    let mut lost = Vec::new();
    loop {
        match timeout(Duration::from_millis(100), rig.0.next_output()).await {
            Ok(Ok(OutputNotice::Event(Event::Audio { lineage, .. }))) => {
                panic!("audio of superseded request {lineage:?}")
            }
            Ok(Ok(OutputNotice::Event(_))) => {}
            Ok(Ok(OutputNotice::TurnLost { lineage, stage, .. })) => {
                lost.push((lineage.session.get(), lineage.request.get(), stage));
            }
            Ok(other) => panic!("unexpected {other:?}"),
            Err(_) => break,
        }
    }
    assert_eq!(lost, [(2, 2, Stage::InputOpen)]);
    assert_eq!(rig.0.try_start(request(2)), Err(Error::StaleRequest));
    rig.0.try_start(request(3)).unwrap();
    // The first thing the third session hears is the new request, nothing older.
    assert!(
        third.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    rig.0.shutdown();
}

#[tokio::test(start_paused = true)]
async fn replacements_that_die_on_arrival_end_as_unstable_with_the_first_answer_intact() {
    let policy = quick();
    let (mut rig, _, expected) = unread(true, policy).await;
    let (mut dials, mut outages) = (0, 0);
    let failure = loop {
        let update = update(&mut rig.0).await;
        match update.kind {
            LinkUpdateKind::Unavailable { .. } => {
                outages += 1;
                if let Ok(Some(mut dial)) = timeout(Duration::from_millis(50), rig.1.recv()).await {
                    dials += 1;
                    dial.service.accept().await;
                    dial.service.close(1011, "dead on arrival").await;
                    finished_attempt(&rig.0).await;
                }
            }
            LinkUpdateKind::Ready { .. } => {}
            LinkUpdateKind::Terminal { failure } => break failure,
        }
    };
    assert_eq!(failure.reason, FailureReason::Unstable);
    assert_eq!(dials, policy.max_recoveries);
    assert_eq!(outages, policy.max_recoveries + 1);
    // Three more connections came and went. None of them took the answer.
    let (mut actual, mut settled) = (Vec::new(), 0);
    let ended = loop {
        match timeout(WAIT, rig.0.next_output()).await.unwrap() {
            Ok(OutputNotice::Event(Event::Audio { lineage, pcm, .. })) => {
                assert_eq!(lineage.session.get(), 1);
                actual.extend(pcm);
            }
            Ok(OutputNotice::Event(_)) => {}
            Ok(OutputNotice::Settled { lineage, .. }) => {
                assert_eq!(lineage.request.get(), 1);
                assert_eq!(actual, expected);
                settled += 1;
            }
            Ok(other) => panic!("unexpected {other:?}"),
            Err(failure) => break failure,
        }
    };
    assert_eq!((settled, ended.reason), (1, FailureReason::Unstable));
    assert!(
        rig.1.try_recv().is_err(),
        "no attempt after the link was declared unstable"
    );
}
