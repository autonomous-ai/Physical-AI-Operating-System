//! What an open activity has been sent is remembered by request, so that a
//! late abandonment reports exactly its own audio. That memory is bounded:
//! [`MAX_OPEN_ACTIVITY_REQUESTS`] entries, an explicit failure before one
//! more, an entry forgotten once it has been reported, and no memory at all
//! where no start can be kept.
//!
//! Through the session itself, against the in-memory service, except for
//! the two tests of the account alone.
use super::*;
use crate::{MAX_OPEN_ACTIVITY_REQUESTS, SuccessorStart};

const BOUND: u64 = MAX_OPEN_ACTIVITY_REQUESTS as u64;
const BLOCK: u64 = 320;

fn keeping() -> SessionConfig {
    configuration(Timeouts::default())
        .successor_start(SuccessorStart::AfterFirstOutput {
            patience: Duration::from_secs(2),
        })
        .unwrap()
}

/// How many blocks request `id` says: one, two or three.
fn blocks(id: u64) -> u64 {
    1 + id % 3
}

/// The samples requests `from..=through` say between them.
fn said_by(from: u64, through: u64) -> u64 {
    (from..=through).map(|id| blocks(id) * BLOCK).sum()
}

/// Request `id` starts in the activity that is open, or opens it; says its
/// blocks, which the service receives; and is superseded before its End.
/// No End is written for it, so the activity stays open behind it.
async fn said_and_superseded(connection: &mut Connection, server: &mut MockSocket, id: u64) {
    let input = connection.input();
    let this = request(id);
    input.try_start(this).unwrap();
    assert!(matches!(
        event(connection).await,
        Event::InputStarted { lineage, waiting_for_barrier: false } if lineage.request == this
    ));
    for sequence in 0..blocks(id) {
        input
            .try_audio(this, sequence, Instant::now(), &[id as i16; BLOCK as usize])
            .unwrap();
        let message = receive(server).await;
        if message["realtimeInput"].get("activityStart").is_some() {
            assert_eq!((id, sequence), (1, 0), "a second activity was opened");
            assert_eq!(heard(&receive(server).await), id as i16);
        } else {
            assert_eq!(heard(&message), id as i16);
        }
    }
    input.retire(this);
    input.try_end(this).unwrap();
}

/// An audio block of twenty milliseconds; its first sample.
fn heard(message: &Value) -> i16 {
    let data = message["realtimeInput"]["audio"]["data"]
        .as_str()
        .unwrap_or_else(|| panic!("expected audio, received {message}"));
    let bytes = STANDARD.decode(data).unwrap();
    assert_eq!(bytes.len(), 2 * BLOCK as usize);
    i16::from_le_bytes([bytes[0], bytes[1]])
}

async fn reported(connection: &mut Connection) -> (u64, u64, u64) {
    match event(connection).await {
        Event::InputDiscarded {
            through,
            dropped_samples,
            sent_samples,
            ..
        } => (through.get(), dropped_samples, sent_samples),
        other => panic!("expected a discard report, got {other:?}"),
    }
}

#[tokio::test]
async fn one_request_more_than_the_account_holds_fails_the_connection_before_its_audio_is_sent() {
    let (mut connection, mut server) = mock_with(keeping()).await;
    for id in 1..=BOUND {
        said_and_superseded(&mut connection, &mut server, id).await;
    }
    // Full, and still serving: the newest request can go on speaking.
    assert_eq!(connection.state(), State::Ready);
    let input = connection.input();
    let over = request(BOUND + 1);
    input.try_start(over).unwrap();
    assert!(matches!(
        event(&mut connection).await,
        Event::InputStarted { lineage, .. } if lineage.request == over
    ));
    // A request that says nothing is not accounted for and costs nothing.
    assert_eq!(connection.state(), State::Ready);
    input
        .try_audio(over, 0, Instant::now(), &[-1; BLOCK as usize])
        .unwrap();
    // Its first block is refused, by name, and never reaches the service.
    assert_eq!(disconnected(&connection).await, Error::OpenActivityOverrun);
    no_input_was_written(&mut server).await;
    // The session has ended. Input is refused from here, as after any end.
    assert_eq!(
        connection.state(),
        State::Disconnected(Error::OpenActivityOverrun)
    );
    assert_eq!(
        input.try_audio(over, 1, Instant::now(), &[-1; BLOCK as usize]),
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn a_full_account_reports_every_request_exactly_and_is_emptied_by_the_report() {
    let (mut connection, mut server) = mock_with(keeping()).await;
    for id in 1..=BOUND {
        said_and_superseded(&mut connection, &mut server, id).await;
    }
    let input = connection.input();
    // A late abandonment of the older half, long after the newer half
    // spoke into the same activity: the older half's own samples, exactly.
    let half = BOUND / 2;
    input.discard(request(half));
    assert_eq!(reported(&mut connection).await, (half, 0, said_by(1, half)));
    // Then of the rest: the rest alone, the older half not counted twice.
    input.discard(request(BOUND));
    assert_eq!(
        reported(&mut connection).await,
        (BOUND, 0, said_by(half + 1, BOUND))
    );
    // Reported is forgotten: as many requests again fit in the same
    // activity, on the same connection, and their report is theirs alone.
    for id in BOUND + 1..=2 * BOUND {
        said_and_superseded(&mut connection, &mut server, id).await;
    }
    assert_eq!(connection.state(), State::Ready);
    input.discard(request(2 * BOUND));
    assert_eq!(
        reported(&mut connection).await,
        (2 * BOUND, 0, said_by(BOUND + 1, 2 * BOUND))
    );
}

/// Ten times the bound, in one activity that never ends: short requests in
/// runs of one to seven, each run continued request by request and then
/// abandoned as a whole. Nothing accumulates, and no report is ever off by
/// a sample.
#[tokio::test]
async fn an_endless_run_of_short_requests_that_are_abandoned_in_time_never_reaches_the_bound() {
    let (mut connection, mut server) = mock_with(keeping()).await;
    let input = connection.input();
    let (mut id, mut run) = (0, 0);
    while id < 10 * BOUND {
        run = run % 7 + 1;
        let first = id + 1;
        for _ in 0..run {
            id += 1;
            said_and_superseded(&mut connection, &mut server, id).await;
        }
        input.discard(request(id));
        assert_eq!(reported(&mut connection).await, (id, 0, said_by(first, id)));
    }
    assert_eq!(connection.state(), State::Ready);
    silent(&mut server).await;
}

/// The default: no start is ever kept, an abandonment is a retirement and
/// reports nothing, and so nothing is remembered. Four times the bound and
/// one more, none committed and none abandoned, in one open activity.
#[tokio::test]
async fn by_default_nothing_is_remembered_and_no_number_of_requests_fails() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    for id in 1..=4 * BOUND + 1 {
        said_and_superseded(&mut connection, &mut server, id).await;
    }
    connection.input().discard(request(4 * BOUND + 1));
    quiet(&mut connection).await;
    assert_eq!(connection.state(), State::Ready);
}

#[test]
fn the_account_refuses_one_entry_more_than_the_bound_and_is_left_as_it_was() {
    let mut account = OpenActivity::default();
    for request in 1..=BOUND {
        account.note(request, request).unwrap();
        // More from the same request is the same entry.
        account.note(request, 1).unwrap();
    }
    assert_eq!(account.0.len() as u64, BOUND);
    assert_eq!(account.note(BOUND + 1, 5), Err(Error::OpenActivityOverrun));
    assert_eq!(account.0.len() as u64, BOUND);
    // The newest entry still takes more: only growth is refused.
    account.note(BOUND, 7).unwrap();
    let all: u64 = (1..=BOUND).map(|request| request + 1).sum();
    assert_eq!(account.take_through(BOUND), all + 7);
    assert!(account.0.is_empty());
}

#[test]
fn the_account_counts_a_request_once_and_forgets_what_it_has_reported() {
    let mut account = OpenActivity::default();
    for request in [3, 4, 9] {
        account.note(request, 100 * request).unwrap();
    }
    // Nothing through the second request: nothing reported, nothing lost.
    assert_eq!(account.take_through(2), 0);
    assert_eq!(account.0.len(), 3);
    assert_eq!(account.take_through(4), 700);
    assert_eq!(account.take_through(4), 0);
    assert_eq!(account.take_through(8), 0);
    // One entry freed is one entry available, whatever the bound.
    assert_eq!(account.0.len(), 1);
    assert_eq!(account.take_through(u64::MAX), 900);
    account.note(10, 1).unwrap();
    account.clear();
    assert_eq!(account.take_through(u64::MAX), 0);
}
