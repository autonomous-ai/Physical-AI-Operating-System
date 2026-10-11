//! A start that names the connection it was chosen for.
use super::{
    availability::{failed_source, replacement_without_output, unread, update},
    *,
};

#[tokio::test(start_paused = true)]
async fn start_chosen_for_a_replaced_connection_is_refused_before_anything_is_sent() {
    // An answer from connection 1 is still unread when that connection dies
    // and connection 2 takes its place.
    let (mut rig, _, expected) = unread(true, quick()).await;
    let (ready, mut second) = replacement_without_output(&mut rig).await;
    let (chosen, live) = (SessionId::new(1).unwrap(), ready.session);
    assert_eq!(live.get(), 2);
    // The decision to start was taken while connection 1 was the ready one,
    // and reaches the supervisor only now.
    assert_eq!(
        rig.0.try_start_on(chosen, request(2)),
        Err(StartRefusal::WrongSession { live })
    );
    assert!(
        second
            .service
            .try_receive(Duration::from_millis(40))
            .await
            .is_none(),
        "the replacement must not hear a request it was never chosen for"
    );
    // Refusing it superseded nothing: the earlier answer is whole.
    let mut actual = Vec::new();
    loop {
        match timeout(WAIT, rig.0.next_output()).await.unwrap().unwrap() {
            OutputNotice::Event(Event::Audio { lineage, pcm, .. }) => {
                assert_eq!((lineage.session.get(), lineage.request.get()), (1, 1));
                actual.extend(pcm);
            }
            OutputNotice::Event(_) => {}
            OutputNotice::Settled { lineage, .. } => {
                assert_eq!(lineage.request.get(), 1);
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(actual, expected);
    // The same request, chosen again for the connection that is live, starts.
    assert_eq!(rig.0.try_start_on(live, request(2)), Ok(()));
    assert!(
        second.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    assert!(matches!(
        timeout(WAIT, rig.0.next_output()).await.unwrap().unwrap(),
        OutputNotice::Event(Event::InputStarted { lineage, .. })
            if lineage.session == live && lineage.request.get() == 2
    ));
    rig.0.shutdown();
}

#[tokio::test(start_paused = true)]
async fn checked_start_never_reaches_a_connection_other_than_the_one_named() {
    let (mut rig, mut first) = connected(quick(), config()).await;
    let one = SessionId::new(1).unwrap();
    // A session that does not exist yet is not the live one either.
    assert_eq!(
        rig.0.try_start_on(SessionId::new(2).unwrap(), request(1)),
        Err(StartRefusal::WrongSession { live: one })
    );
    assert_eq!(rig.0.try_start_on(one, request(1)), Ok(()));
    assert!(
        first.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    // On the right connection the ordinary rules still decide.
    assert_eq!(
        rig.0.try_start_on(one, request(1)),
        Err(StartRefusal::Refused(Error::StaleRequest))
    );
    // During the outage there is no connection to name.
    first.service.close(1011, "outage").await;
    failed_source(&mut rig.0).await;
    assert_eq!(
        rig.0.try_start_on(one, request(2)),
        Err(StartRefusal::NotConnected)
    );
    assert!(matches!(
        update(&mut rig.0).await.kind,
        LinkUpdateKind::Unavailable { .. }
    ));
    assert_eq!(
        rig.0.try_start_on(one, request(2)),
        Err(StartRefusal::NotConnected)
    );
    // None of the refusals spent the request number or the lost request's.
    let mut second = next_dial(&mut rig.1).await;
    second.service.accept().await;
    let ready = loop {
        let update = update(&mut rig.0).await;
        if matches!(update.kind, LinkUpdateKind::Ready { .. }) {
            break update;
        }
    };
    assert_eq!(
        rig.0.try_start_on(one, request(2)),
        Err(StartRefusal::WrongSession {
            live: ready.session
        })
    );
    assert_eq!(rig.0.try_start_on(ready.session, request(2)), Ok(()));
    assert!(
        second.service.receive().await["realtimeInput"]
            .get("activityStart")
            .is_some()
    );
    rig.0.shutdown();
}
