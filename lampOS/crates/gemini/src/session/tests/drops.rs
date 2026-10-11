//! What a connection drops instead of delivering, counted by what it carried.
use super::*;
use crate::DropLedger;

#[tokio::test]
async fn dropped_messages_are_counted_by_what_they_carried() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let input = connection.input();
    assert_eq!(input.drop_ledger(), DropLedger::default());

    // Request 1 is committed and cut off before any answer. What the service
    // still sends for it is dropped, and there is no event to say so.
    ended_turn(&mut connection, &mut server, 1).await;
    input.try_start(request(2)).unwrap();
    sent(&mut server, "activityStart").await;
    assert!(matches!(
        event(&mut connection).await,
        Event::InputStarted {
            waiting_for_barrier: true,
            ..
        }
    ));
    send_json(&mut server, audio(7)).await;
    send_json(&mut server, audio(7)).await;
    // An interruption is reported under the request it belongs to. It is not
    // a drop, and it is not the idle completion the barrier waits for.
    send_json(&mut server, json!({"serverContent":{"interrupted":true}})).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Interrupted { lineage } if lineage.request == request(1)
    ));
    assert_eq!(
        input.drop_ledger(),
        DropLedger {
            retired_output: 2,
            ..DropLedger::default()
        }
    );
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request(1)
    ));

    // Request 2 is answered whole. Nothing of it is a drop.
    input
        .try_audio(request(2), 0, Instant::now(), &[2; 160])
        .unwrap();
    input.try_end(request(2)).unwrap();
    sent(&mut server, "audio").await;
    sent(&mut server, "activityEnd").await;
    for message in [audio(9), generated(), idle()] {
        send_json(&mut server, message).await;
    }
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, .. } if lineage.request == request(2)
    ));
    assert!(matches!(
        event(&mut connection).await,
        Event::GenerationComplete { .. }
    ));
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { idle: true, .. }
    ));
    assert_eq!(input.drop_ledger().retired_output, 2);
    assert_eq!(input.drop_ledger().unowned_output, 0);
    assert_eq!(input.drop_ledger().unowned_markers, 0);

    // After the answer: a completion with nothing in it, then audio nobody
    // asked for. Both are discarded for the same reason. Only the ledger
    // tells a stray marker from a second answer.
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Discarded {
            reason: Discard::UnownedOutput,
            ..
        }
    ));
    assert_eq!(
        input.drop_ledger(),
        DropLedger {
            retired_output: 2,
            unowned_output: 0,
            unowned_markers: 1,
        }
    );
    send_json(&mut server, audio(5)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Discarded {
            reason: Discard::UnownedOutput,
            ..
        }
    ));
    assert_eq!(
        input.drop_ledger(),
        DropLedger {
            retired_output: 2,
            unowned_output: 1,
            unowned_markers: 1,
        }
    );
}
