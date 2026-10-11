//! Visual context: one bounded image slot beside the audio queue.
use super::*;
use crate::{
    ImageOffer, ImageRefusal, MAX_IMAGE_AGE, MAX_IMAGE_BYTES, MAX_IMAGES_PER_REQUEST, testing::jpeg,
};

/// A request that is speaking: started, with one block already sent.
async fn speaking(connection: &mut Connection, server: &mut MockSocket, id: u64) -> RequestId {
    let request = request(id);
    let input = connection.input();
    input.try_start(request).unwrap();
    sent(server, "activityStart").await;
    assert!(matches!(
        event(connection).await,
        Event::InputStarted { .. }
    ));
    input
        .try_audio(request, 0, Instant::now(), &[100; 160])
        .unwrap();
    sent(server, "audio").await;
    request
}
fn picture(message: &Value) -> Vec<u8> {
    let video = &message["realtimeInput"]["video"];
    assert_eq!(video["mimeType"], "image/jpeg", "{message}");
    STANDARD.decode(video["data"].as_str().unwrap()).unwrap()
}
/// First sample of an audio message, which the tests use as a block label.
fn label(message: &Value) -> i16 {
    let pcm = STANDARD
        .decode(
            message["realtimeInput"]["audio"]["data"]
                .as_str()
                .unwrap_or_else(|| panic!("expected audio, received {message}")),
        )
        .unwrap();
    i16::from_le_bytes([pcm[0], pcm[1]])
}

#[tokio::test]
async fn image_is_sent_once_inside_its_request_after_audio_already_queued() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = speaking(&mut connection, &mut server, 1).await;
    let input = connection.input();
    // The largest image allowed, offered between two audio blocks.
    let image = jpeg(MAX_IMAGE_BYTES, 7);
    input
        .try_audio(request, 1, Instant::now(), &[101; 160])
        .unwrap();
    assert_eq!(
        input.try_image(request, 41, Instant::now(), &image),
        Ok(ImageOffer::Queued)
    );
    input
        .try_audio(request, 2, Instant::now(), &[102; 160])
        .unwrap();
    // Audio that was queued goes first; the image does not jump it.
    assert_eq!(label(&receive(&mut server).await), 101);
    assert_eq!(label(&receive(&mut server).await), 102);
    assert_eq!(picture(&receive(&mut server).await), image);
    input.try_end(request).unwrap();
    sent(&mut server, "activityEnd").await;
    silent(&mut server).await;
    let ledger = input.image_ledger();
    assert_eq!(
        (
            ledger.accepted,
            ledger.sent,
            ledger.replaced,
            ledger.dropped
        ),
        (1, 1, 0, 0)
    );
    let receipt = ledger.last_sent.unwrap();
    assert_eq!(
        (receipt.request, receipt.frame, receipt.bytes),
        (request, 41, MAX_IMAGE_BYTES as u32)
    );
    assert_eq!(connection.state(), State::Ready);
}

#[tokio::test]
async fn a_refused_image_changes_nothing_else() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let input = connection.input();
    let image = jpeg(2_000, 3);
    let now = Instant::now;
    assert_eq!(
        input.try_image(request(1), 0, now(), &image),
        Err(ImageRefusal::NotCurrent),
        "nobody is speaking"
    );
    let speaker = speaking(&mut connection, &mut server, 1).await;
    for (wrong, reason) in [
        (jpeg(MAX_IMAGE_BYTES + 1, 3), ImageRefusal::TooLarge),
        (vec![0x89, b'P', b'N', b'G', 0, 0], ImageRefusal::NotJpeg),
        // Cut off before its end marker.
        (image[..image.len() - 2].to_vec(), ImageRefusal::NotJpeg),
    ] {
        assert_eq!(input.try_image(speaker, 0, now(), &wrong), Err(reason));
    }
    assert_eq!(
        input.try_image(speaker, 0, now() - MAX_IMAGE_AGE, &image),
        Err(ImageRefusal::TooOld)
    );
    assert_eq!(
        input.try_image(speaker, 0, now() + Duration::from_secs(1), &image),
        Err(ImageRefusal::TooOld),
        "a capture time in the future is not fresh"
    );
    assert_eq!(
        input.try_image(request(2), 0, now(), &image),
        Err(ImageRefusal::NotCurrent)
    );
    // Up to the limit, then no more for this request.
    for frame in 0..u64::from(MAX_IMAGES_PER_REQUEST) {
        assert_eq!(
            input.try_image(speaker, frame, now(), &image),
            Ok(ImageOffer::Queued)
        );
        assert_eq!(picture(&receive(&mut server).await), image);
    }
    assert_eq!(
        input.try_image(speaker, 9, now(), &image),
        Err(ImageRefusal::LimitReached)
    );
    // None of that disturbed the request's audio or the connection.
    input.try_audio(speaker, 1, now(), &[101; 160]).unwrap();
    assert_eq!(label(&receive(&mut server).await), 101);
    input.try_end(speaker).unwrap();
    sent(&mut server, "activityEnd").await;
    assert_eq!(
        input.try_image(speaker, 9, now(), &image),
        Err(ImageRefusal::NotCurrent),
        "its input has ended"
    );
    silent(&mut server).await;
    assert_eq!(connection.state(), State::Ready);
    let ledger = input.image_ledger();
    assert_eq!((ledger.accepted, ledger.sent), (2, 2));
    input.shutdown();
    assert_eq!(
        input.try_image(speaker, 9, now(), &image),
        Err(ImageRefusal::NotConnected)
    );
}

#[tokio::test]
async fn a_newer_image_replaces_one_not_yet_sent_and_counts_once() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = speaking(&mut connection, &mut server, 1).await;
    let input = connection.input();
    let (older, newer, last) = (jpeg(3_000, 1), jpeg(3_000, 2), jpeg(3_000, 3));
    // Both offers land before the connection has had a turn to send.
    assert_eq!(
        input.try_image(request, 1, Instant::now(), &older),
        Ok(ImageOffer::Queued)
    );
    assert_eq!(
        input.try_image(request, 2, Instant::now(), &newer),
        Ok(ImageOffer::ReplacedUnsent)
    );
    assert_eq!(picture(&receive(&mut server).await), newer);
    // The replaced one did not use up the request's allowance.
    assert_eq!(
        input.try_image(request, 3, Instant::now(), &last),
        Ok(ImageOffer::Queued)
    );
    assert_eq!(picture(&receive(&mut server).await), last);
    assert_eq!(
        input.try_image(request, 4, Instant::now(), &last),
        Err(ImageRefusal::LimitReached)
    );
    silent(&mut server).await;
    let ledger = input.image_ledger();
    assert_eq!(
        (
            ledger.accepted,
            ledger.sent,
            ledger.replaced,
            ledger.dropped
        ),
        (3, 2, 1, 0)
    );
    assert_eq!(ledger.last_sent.unwrap().frame, 3);
}

#[tokio::test]
async fn an_image_whose_request_ended_or_was_cancelled_first_is_dropped_unsent() {
    // The person finished speaking before the image could be sent.
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = speaking(&mut connection, &mut server, 1).await;
    let input = connection.input();
    let image = jpeg(3_000, 5);
    assert_eq!(
        input.try_image(request, 1, Instant::now(), &image),
        Ok(ImageOffer::Queued)
    );
    input.try_end(request).unwrap();
    sent(&mut server, "activityEnd").await;
    silent(&mut server).await;
    let ledger = input.image_ledger();
    assert_eq!((ledger.sent, ledger.dropped), (0, 1));

    // The request was cancelled locally with its image still waiting.
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = speaking(&mut connection, &mut server, 1).await;
    let input = connection.input();
    assert_eq!(
        input.try_image(request, 1, Instant::now(), &image),
        Ok(ImageOffer::Queued)
    );
    input.retire(request);
    silent(&mut server).await;
    let ledger = input.image_ledger();
    assert_eq!((ledger.sent, ledger.dropped), (0, 1));
    assert_eq!(connection.state(), State::Ready);
}

/// A connection whose peer has room for only `capacity` unread bytes, so a
/// large write does not complete until the peer reads.
async fn narrow(capacity: usize) -> (Connection, MockSocket) {
    let (client, mut server) = pair(capacity).await;
    let connect = tokio::spawn(setup_and_spawn(
        client,
        configuration(Timeouts::default()),
        SessionId::new(17).unwrap(),
    ));
    receive(&mut server).await;
    server
        .send(Message::Binary(br#"{"setupComplete":{}}"#.to_vec().into()))
        .await
        .unwrap();
    (connect.await.unwrap().unwrap(), server)
}

#[tokio::test]
async fn audio_offered_during_a_slow_image_write_is_held_and_sent_in_order() {
    let (mut connection, mut server) = narrow(4_096).await;
    let request = speaking(&mut connection, &mut server, 1).await;
    let input = connection.input();
    let image = jpeg(MAX_IMAGE_BYTES, 7);
    assert_eq!(
        input.try_image(request, 1, Instant::now(), &image),
        Ok(ImageOffer::Queued)
    );
    // The image is far larger than the peer has room for, so its write is
    // still in progress. More audio arrives than the command queue alone
    // could hold; none of it may be refused or lost.
    const BLOCKS: i16 = 60;
    assert!(BLOCKS as usize > crate::INPUT_QUEUE_CAPACITY);
    for block in 1..=BLOCKS {
        tokio::task::yield_now().await;
        input
            .try_audio(request, block as u64, Instant::now(), &[100 + block; 160])
            .unwrap_or_else(|error| panic!("block {block} was refused: {error}"));
    }
    assert_eq!(picture(&receive(&mut server).await), image);
    for block in 1..=BLOCKS {
        assert_eq!(label(&receive(&mut server).await), 100 + block);
    }
    input.try_end(request).unwrap();
    sent(&mut server, "activityEnd").await;
    assert_eq!(connection.state(), State::Ready);
    let receipt = input.image_ledger().last_sent.unwrap();
    assert!(receipt.written_in <= IMAGE_WRITE, "{receipt:?}");
    println!(
        "MEASURED image_write bytes={} held_audio_blocks={BLOCKS} written_in_ms={:.1} waited_ms={:.1} (in-memory peer with 4 KiB of room, host)",
        receipt.bytes,
        receipt.written_in.as_secs_f64() * 1e3,
        receipt.waited.as_secs_f64() * 1e3
    );
}

#[tokio::test(start_paused = true)]
async fn an_image_the_link_cannot_take_fails_the_connection_within_its_write_bound() {
    let (mut connection, mut server) = narrow(4_096).await;
    let request = speaking(&mut connection, &mut server, 1).await;
    let input = connection.input();
    let offered = Clock::now();
    assert_eq!(
        input.try_image(request, 1, Instant::now(), &jpeg(MAX_IMAGE_BYTES, 7)),
        Ok(ImageOffer::Queued)
    );
    // The peer never reads.
    assert_eq!(disconnected(&connection).await, Error::WriteTimeout);
    let took = offered.elapsed();
    assert!(
        took >= IMAGE_WRITE && took <= IMAGE_WRITE + Duration::from_millis(50),
        "{took:?}"
    );
    // A write timeout is a link failure a supervisor reconnects from.
    assert_eq!(Error::WriteTimeout.recovery(), crate::Recovery::Reconnect);
    drop(server);
}
