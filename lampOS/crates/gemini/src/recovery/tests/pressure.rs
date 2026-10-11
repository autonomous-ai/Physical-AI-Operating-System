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

#[tokio::test]
async fn image_is_refused_during_an_outage_and_never_carried_to_the_replacement() {
    use crate::{ImageOffer, ImageRefusal, testing::jpeg};
    let (mut rig, mut first) = connected(quick(), config()).await;
    let picture = jpeg(4_000, 9);
    let now = std::time::Instant::now;
    assert_eq!(
        rig.0.try_image(request(1), 5, now(), &picture),
        Err(ImageRefusal::NotCurrent),
        "nothing is speaking yet"
    );
    speak(&mut rig, &mut first, 1, false).await;
    assert_eq!(
        rig.0.try_image(request(1), 5, now(), &picture),
        Ok(ImageOffer::Queued)
    );
    assert!(
        first.service.receive().await["realtimeInput"]
            .get("video")
            .is_some()
    );
    let ledger = rig.0.image_ledger().unwrap();
    assert_eq!((ledger.accepted, ledger.sent), (1, 1));
    assert_eq!(ledger.last_sent.unwrap().frame, 5);

    first.service.close(1011, "outage").await;
    failed_source(&mut rig.0).await;
    assert_eq!(
        rig.0.try_image(request(1), 6, now(), &picture),
        Err(ImageRefusal::NotConnected)
    );
    let (_, mut second) = replacement_without_output(&mut rig).await;
    // The request it belonged to did not move to the new connection.
    assert_eq!(
        rig.0.try_image(request(1), 6, now(), &picture),
        Err(ImageRefusal::NotCurrent)
    );
    assert_eq!(rig.0.image_ledger().unwrap().accepted, 0);
    assert!(
        second
            .service
            .try_receive(Duration::from_millis(40))
            .await
            .is_none(),
        "nothing of the old request is sent again"
    );
    rig.0.try_start(request(2)).unwrap();
    assert_eq!(
        rig.0.try_image(request(2), 7, now(), &picture),
        Ok(ImageOffer::Queued)
    );
    assert!(
        second.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    assert!(
        second.service.receive().await["realtimeInput"]
            .get("video")
            .is_some()
    );
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

/// An answer of more events than the event queue holds, on a connection
/// whose consumer then takes nothing for longer than the delivery bound.
/// Returns the supervisor with the failure observed and not yet read.
async fn speaker_stopped_on_a_live_connection(with_handle: bool) -> (Rig, i16) {
    const BLOCKS: i16 = 40;
    let (mut rig, mut first) = if with_handle {
        connected(quick(), config()).await
    } else {
        let (dialer, dials) = connector(config());
        let mut rig = (Supervisor::new(dialer, quick()).unwrap(), dials);
        let (_, dial, _) = ready(&mut rig).await;
        (rig, dial)
    };
    speak(&mut rig, &mut first, 1, true).await;
    // Twenty-four seconds of audio in forty messages, generation complete.
    // The idle completion is withheld, as the service does while it assumes
    // playback; it is not due before the delivery bound.
    for value in 1..=BLOCKS {
        first.service.send(audio(value, 14_400)).await;
    }
    first
        .service
        .send(json!({"serverContent":{"generationComplete":true}}))
        .await;
    let until = Instant::now() + Timeouts::default().deliver + Duration::from_secs(2);
    while Instant::now() < until && rig.0.is_connected() {
        first.service.rest(Duration::from_millis(100)).await;
    }
    assert!(!rig.0.is_connected(), "the delivery bound never fired");
    (rig, BLOCKS)
}

/// What the dead connection still delivers: an unbroken beginning of the
/// answer, then its one outcome.
async fn beginning_then_outcome(
    rig: &mut Rig,
    blocks: i16,
) -> (usize, std::result::Result<OutputNotice, Failure>) {
    let mut heard = Vec::new();
    let outcome = loop {
        match timeout(WAIT, rig.0.next_output()).await.unwrap() {
            Ok(OutputNotice::Event(Event::Audio { pcm, .. })) => heard.push(pcm[0]),
            Ok(OutputNotice::Event(_)) => {}
            outcome => break outcome,
        }
    };
    assert!(!heard.is_empty() && heard.len() < blocks as usize);
    assert_eq!(heard, (1..=heard.len() as i16).collect::<Vec<_>>());
    (heard.len(), outcome)
}

#[tokio::test(start_paused = true)]
async fn a_stopped_speaker_past_the_delivery_bound_costs_the_answer_not_the_session() {
    let (mut rig, blocks) = speaker_stopped_on_a_live_connection(true).await;
    let unavailable = update(&mut rig.0).await;
    assert!(matches!(
        unavailable.kind,
        LinkUpdateKind::Unavailable {
            error: Error::Backpressure
        }
    ));
    // Replaced like any lost connection: once, with the handle, in bounds.
    let mut second = next_dial(&mut rig.1).await;
    assert!(second.resumed);
    second.service.accept().await;
    finished_attempt(&rig.0).await;
    let recovered = update(&mut rig.0).await;
    assert!(
        matches!(
            recovered.kind,
            LinkUpdateKind::Ready {
                context: Context::Resumed,
                attempts: 1,
                after: Some(Error::Backpressure),
                ..
            }
        ),
        "{recovered:?}"
    );
    let (delivered, outcome) = beginning_then_outcome(&mut rig, blocks).await;
    assert!(delivered <= crate::EVENT_QUEUE_CAPACITY);
    assert!(
        matches!(
            outcome,
            Ok(OutputNotice::TurnLost {
                lineage,
                stage: Stage::Responding,
                error: Error::Backpressure,
            }) if lineage.request.get() == 1 && lineage.session.get() == 1
        ),
        "{outcome:?}"
    );
    // One report, nothing replayed, and the next request is served.
    assert!(
        timeout(Duration::from_millis(100), rig.0.next_output())
            .await
            .is_err(),
        "a second outcome for the same request"
    );
    assert!(
        second
            .service
            .try_receive(Duration::from_millis(40))
            .await
            .is_none(),
        "the lost request was sent again"
    );
    assert_eq!(rig.0.try_start(request(1)), Err(Error::StaleRequest));
    rig.0.try_start(request(2)).unwrap();
    assert!(
        second.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    assert!(rig.1.try_recv().is_err(), "one outage, one replacement");
    rig.0.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_stopped_speaker_still_ends_a_session_that_cannot_resume_its_context() {
    // No handle was ever issued, and policy requires one.
    let (mut rig, blocks) = speaker_stopped_on_a_live_connection(false).await;
    assert!(matches!(
        update(&mut rig.0).await.kind,
        LinkUpdateKind::Unavailable {
            error: Error::Backpressure
        }
    ));
    let terminal = update(&mut rig.0).await;
    assert!(
        matches!(
            terminal.kind,
            LinkUpdateKind::Terminal {
                failure: Failure {
                    error: Error::Backpressure,
                    reason: FailureReason::NoResumableContext,
                    ..
                }
            }
        ),
        "{terminal:?}"
    );
    assert!(rig.1.try_recv().is_err(), "no attempt without context");
    // The request is still reported, once, before the end.
    let (_, outcome) = beginning_then_outcome(&mut rig, blocks).await;
    assert!(matches!(
        outcome,
        Ok(OutputNotice::TurnLost {
            stage: Stage::Responding,
            ..
        })
    ));
    assert_eq!(
        rig.0.next_output().await.unwrap_err().reason,
        FailureReason::NoResumableContext
    );
}

#[tokio::test(start_paused = true)]
async fn input_that_overruns_its_queue_loses_that_request_and_recovers_the_link() {
    let (mut rig, mut first) = connected(quick(), config()).await;
    speak(&mut rig, &mut first, 1, false).await;
    // More blocks than the queue holds, with no turn given to the connection.
    let mut refused = None;
    for sequence in 1..=crate::INPUT_QUEUE_CAPACITY as u64 + 1 {
        if let Err(error) =
            rig.0
                .try_audio(request(1), sequence, std::time::Instant::now(), &[1; 160])
        {
            refused = Some((sequence, error));
            break;
        }
    }
    assert_eq!(
        refused,
        Some((crate::INPUT_QUEUE_CAPACITY as u64 + 1, Error::Backpressure))
    );
    assert!(matches!(
        update(&mut rig.0).await.kind,
        LinkUpdateKind::Unavailable {
            error: Error::Backpressure
        }
    ));
    let mut second = next_dial(&mut rig.1).await;
    second.service.accept().await;
    finished_attempt(&rig.0).await;
    assert!(matches!(
        update(&mut rig.0).await.kind,
        LinkUpdateKind::Ready { .. }
    ));
    let lost = loop {
        match timeout(WAIT, rig.0.next_output()).await.unwrap().unwrap() {
            OutputNotice::Event(_) => {}
            outcome => break outcome,
        }
    };
    assert!(
        matches!(
            lost,
            OutputNotice::TurnLost {
                lineage,
                stage: Stage::InputOpen,
                error: Error::Backpressure,
            } if lineage.request.get() == 1
        ),
        "{lost:?}"
    );
    // Its remaining input is refused, not carried to the replacement.
    assert_eq!(
        rig.0
            .try_audio(request(1), 99, std::time::Instant::now(), &[1; 160]),
        Err(Error::StaleRequest)
    );
    assert!(
        second
            .service
            .try_receive(Duration::from_millis(40))
            .await
            .is_none()
    );
    rig.0.try_start(request(2)).unwrap();
    rig.0.shutdown();
}
