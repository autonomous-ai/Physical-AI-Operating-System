//! A cut-off request the service never acknowledges, as a device recorded it.
//!
//! Run of 2026-10-11 on `51bd815e9`: one spoken question was split in two by
//! the endpoint detector. The first part ended; 241 ms later the second part
//! was admitted as a new request, before any answer. The service sent one
//! input transcript and nothing else, and two seconds later the session was
//! over. These tests drive that timeline through the supervisor on a paused
//! clock with the production bounds.
use super::*;

const RESUMED_AFTER: Duration = Duration::from_millis(241);
const SECOND_PART: Duration = Duration::from_millis(1_254);

/// Requests 1 and 2 as the device sent them. Returns when the interruption
/// request was made.
async fn question_split_in_two(rig: &mut Rig, dial: &mut Dial) -> tokio::time::Instant {
    speak(rig, dial, 1, true).await;
    tokio::time::advance(RESUMED_AFTER).await;
    rig.0.try_start(request(2)).unwrap();
    let asked = tokio::time::Instant::now();
    // The only interruption request there is, and the second part's audio
    // goes out at once behind it.
    assert!(
        dial.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    assert!(matches!(
        notice(&mut rig.0).await,
        Notice::Event(Event::InputStarted { lineage, waiting_for_barrier: true })
            if lineage.request.get() == 2
    ));
    rig.0
        .try_audio(request(2), 0, std::time::Instant::now(), &[2; 160])
        .unwrap();
    assert!(
        dial.service.receive().await["realtimeInput"]
            .get("audio")
            .is_some()
    );
    // A transcript of the first part. It names no request and settles nothing.
    dial.service
        .send(json!({"serverContent":{"inputTranscription":{"text":"Hola."}}}))
        .await;
    assert!(matches!(
        notice(&mut rig.0).await,
        Notice::Event(Event::UncorrelatedInputTranscript { text, .. }) if text == "Hola."
    ));
    tokio::time::advance(SECOND_PART).await;
    rig.0.try_end(request(2)).unwrap();
    // The End is withheld. Until it is sent the service cannot be answering
    // request 2, so nothing that arrives can be mistaken for its answer.
    assert!(
        dial.service
            .try_receive(Duration::from_millis(40))
            .await
            .is_none()
    );
    asked
}

/// Speak request 3 on `dial` and have the service answer it with `value`.
async fn third_question_answered(rig: &mut Rig, dial: &mut Dial, value: i16) {
    rig.0.try_start(request(3)).unwrap();
    assert!(
        dial.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    rig.0
        .try_audio(request(3), 0, std::time::Instant::now(), &[3; 160])
        .unwrap();
    rig.0.try_end(request(3)).unwrap();
    assert!(
        dial.service.receive().await["realtimeInput"]
            .get("audio")
            .is_some()
    );
    // A new connection owes nobody an answer: committed at once.
    assert!(
        dial.service.receive().await["realtimeInput"]
            .get("activityEnd")
            .is_some()
    );
    dial.service
        .burst([
            audio(value, 480),
            json!({"serverContent":{"generationComplete":true}}),
            idle(),
        ])
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_service_that_confirms_nothing_costs_the_resumed_question_and_one_connection() {
    let (mut rig, mut first) = connected(quick(), config()).await;
    let asked = question_split_in_two(&mut rig, &mut first).await;

    // Two seconds after the interruption request, not after the person
    // stopped: the connection is given up.
    assert!(matches!(
        raw(&mut rig.0).await,
        Ok(Notice::Unavailable {
            error: Error::BarrierTimeout
        })
    ));
    let waited = asked.elapsed().as_millis();
    assert!((2_000..2_030).contains(&waited), "{waited} ms");
    assert_eq!(
        rig.0.try_start(request(3)),
        Err(Error::NotConnected),
        "nothing is accepted for a connection that cannot say whose answer is next"
    );

    // The second part is reported lost once. The first, cut off by the
    // person, is not reported at all. The replacement carries the context.
    let (lost, mut second, setup) = ready(&mut rig).await;
    assert!(matches!(
        lost,
        Notice::TurnLost { lineage, stage: Stage::AwaitingResponse, error: Error::BarrierTimeout }
            if (lineage.session.get(), lineage.request.get()) == (1, 2)
    ));
    assert!(matches!(
        raw(&mut rig.0).await,
        Ok(Notice::Ready { session, context: Context::Resumed, after: Some(Error::BarrierTimeout), .. })
            if session.get() == 2
    ));
    assert_eq!(setup["setup"]["sessionResumption"]["handle"], "HANDLE-ONE");

    // The first connection's service wakes up and answers after all. There
    // is nobody left to hear it.
    let _ = first.service.try_send(audio(7, 480)).await;
    let _ = first.service.try_send(idle()).await;

    third_question_answered(&mut rig, &mut second, 9).await;
    let mut heard = Vec::new();
    loop {
        match raw(&mut rig.0).await.unwrap() {
            Notice::Event(Event::Audio { lineage, pcm, .. }) => {
                assert_eq!((lineage.session.get(), lineage.request.get()), (2, 3));
                heard.extend(pcm);
            }
            Notice::Event(Event::TurnComplete {
                lineage,
                idle: true,
            }) => {
                assert_eq!(lineage.request.get(), 3);
                break;
            }
            Notice::Event(
                Event::InputStarted {
                    waiting_for_barrier: false,
                    ..
                }
                | Event::GenerationComplete { .. },
            ) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(heard, [9; 480]);
    rig.0.shutdown();
}

#[tokio::test(start_paused = true)]
async fn the_same_resumed_question_is_answered_when_the_service_confirms_within_the_bound() {
    let (mut rig, mut dial) = connected(quick(), config()).await;
    let asked = question_split_in_two(&mut rig, &mut dial).await;

    // 1.9 s after it was asked to stop, the service says the first answer is
    // over. That is all the session was waiting for.
    tokio::time::sleep_until(asked + Duration::from_millis(1_900)).await;
    dial.service
        .burst([json!({"serverContent":{"interrupted":true}}), idle()])
        .await;
    assert!(
        dial.service.receive().await["realtimeInput"]
            .get("activityEnd")
            .is_some(),
        "the withheld End goes out as soon as the earlier response settles"
    );
    dial.service
        .burst([
            audio(5, 480),
            json!({"serverContent":{"generationComplete":true}}),
            idle(),
        ])
        .await;
    let mut heard = Vec::new();
    loop {
        match raw(&mut rig.0).await.unwrap() {
            Notice::Event(Event::Audio { lineage, pcm, .. }) => {
                assert_eq!((lineage.session.get(), lineage.request.get()), (1, 2));
                heard.extend(pcm);
            }
            Notice::Event(Event::TurnComplete {
                lineage,
                idle: true,
            }) if lineage.request.get() == 2 => {
                break;
            }
            Notice::Event(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(heard, [5; 480]);
    assert!(rig.0.is_connected());
    rig.0.shutdown();
}
