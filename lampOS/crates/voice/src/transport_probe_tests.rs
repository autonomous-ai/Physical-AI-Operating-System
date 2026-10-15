use super::super::{Channel, ReferenceChannels, WorkerChannels};
use super::*;
use crate::{
    process::SessionDirectory,
    scenario_clock::{self, Phase, Scope},
    wire::{Envelope, encode},
};
use lamp_ipc::MAX_DATAGRAM_BYTES;
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

const HOST_GUARD: Duration = Duration::from_secs(30);

fn boots() -> (BootId, BootId) {
    (BootId::new([1; 16]).unwrap(), BootId::new([2; 16]).unwrap())
}

fn clock() -> Scope {
    let scope = Scope::enter(HOST_GUARD).unwrap();
    scope.set_pump(|_| Ok(())).unwrap();
    scope
}

fn pair(dir: &std::path::Path) -> (Channel, Probe, Channel, Probe) {
    let (a, b) = boots();
    let mut sender = Channel::bind(&dir.join("s"), a, b, 100_000).unwrap();
    let mut receiver = Channel::bind(&dir.join("r"), b, a, 100_000).unwrap();
    let sent = sender.install_probe().unwrap();
    let received = receiver.install_probe().unwrap();
    sender.connect(&dir.join("r")).unwrap();
    receiver.connect(&dir.join("s")).unwrap();
    (sender, sent, receiver, received)
}

fn raw(sender: &Channel, boot: BootId, sequence: u64, sent_at_us: u64) {
    sender
        .endpoint
        .try_send(
            &encode(&Envelope {
                boot,
                sequence,
                sent_at_us,
                payload: sequence,
            })
            .unwrap(),
        )
        .unwrap();
}

fn receive<T>(receiver: &Receiver<T>) -> T {
    receiver
        .recv_timeout(HOST_GUARD)
        .expect("fixture handshake must finish within the host guard")
}

#[test]
fn successful_send_and_accepted_receive_are_separate_pure_observations() {
    let _scope = clock();
    let before_clock = (scenario_clock::monotonic_us(), scenario_clock::instant());
    let dir = SessionDirectory::create().unwrap();
    let (mut sender, sent, mut receiver, received) = pair(&dir.path);
    let initial = sent.snapshot().unwrap();
    assert_eq!(initial.send.count, 0);
    assert_eq!(initial.receive.count, 0);
    assert_eq!(initial.send.last_sequence, None);
    assert_eq!(initial.send.contiguous_through, 0);
    assert_eq!(initial.channel.name(), "s");
    assert_eq!(initial.send.direction, Direction::Send);
    assert_eq!(initial.receive.direction, Direction::Receive);
    assert_eq!((initial.send.boot, initial.receive.boot), boots());
    assert_eq!(receiver.receive::<u64>().unwrap(), None);
    assert_eq!(received.snapshot().unwrap().revision, 0);
    sender.send(17_u64).unwrap();
    let after_send = sent.snapshot().unwrap();
    assert_eq!(after_send.send.count, 1);
    assert_eq!(after_send.send.last_sequence, Some(1));
    assert_eq!(after_send.send.contiguous_through, 1);
    assert_eq!(after_send.send.first_gap, None);
    assert_eq!(after_send.receive.count, 0);
    assert_eq!(received.snapshot().unwrap().receive.count, 0);
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(17));
    let after_receive = received.snapshot().unwrap();
    assert_eq!(after_receive.receive.count, 1);
    assert_eq!(after_receive.receive.last_sequence, Some(1));
    assert_eq!(after_receive.receive.boot, boots().0);
    assert_eq!(after_receive.send.count, 0);
    assert_ne!(after_send.channel, after_receive.channel);
    for _ in 0..16 {
        assert_eq!(sent.snapshot().unwrap(), after_send);
        assert_eq!(received.snapshot().unwrap(), after_receive);
    }
    assert_eq!(
        (scenario_clock::monotonic_us(), scenario_clock::instant()),
        before_clock
    );
}

#[test]
fn encoding_and_unconnected_send_failures_do_not_commit_sequence_or_evidence() {
    let _scope = clock();
    let dir = SessionDirectory::create().unwrap();
    let (a, b) = boots();
    let mut sender = Channel::bind(&dir.path.join("s"), a, b, 100_000).unwrap();
    let probe = sender.install_probe().unwrap();
    assert_eq!(
        sender
            .send("x".repeat(MAX_DATAGRAM_BYTES))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(sender.send(1_u64).is_err());
    assert_eq!(sender.send_sequence, 0);
    assert_eq!(probe.snapshot().unwrap().send.count, 0);
    assert_eq!(probe.snapshot().unwrap().revision, 0);
    let mut receiver = Channel::bind(&dir.path.join("r"), b, a, 100_000).unwrap();
    sender.connect(&dir.path.join("r")).unwrap();
    receiver.connect(&dir.path.join("s")).unwrap();
    sender.send(2_u64).unwrap();
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(2));
    assert_eq!(sender.send_sequence, 1);
    assert_eq!(probe.snapshot().unwrap().send.last_sequence, Some(1));
    assert_eq!(probe.snapshot().unwrap().send.count, 1);
}

#[test]
fn actual_full_socket_would_block_does_not_count_or_commit_a_send() {
    let _scope = clock();
    let dir = SessionDirectory::create().unwrap();
    let (mut sender, sent, _receiver, received) = pair(&dir.path);
    let payload = "x".repeat(7_000);
    let until = Instant::now() + HOST_GUARD;
    let mut successes = 0;
    let mut full = false;
    // Every successful attempt queues a distinct real datagram. Stop at the
    // first full-queue refusal; this is bounded saturation, not a retry loop.
    for _ in 0..4_096 {
        assert!(
            Instant::now() < until,
            "socket saturation exceeded the host guard"
        );
        match sender.send(&payload) {
            Ok(()) => successes += 1,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                full = true;
                break;
            }
            Err(error) => panic!("unexpected saturation failure: {error}"),
        }
    }
    assert!(full, "bounded real socket queue must reach WouldBlock");
    assert!(successes > 0);
    assert_eq!(sender.send_sequence, successes);
    let snapshot = sent.snapshot().unwrap();
    assert_eq!(snapshot.send.count, successes);
    assert_eq!(snapshot.send.last_sequence, Some(successes));
    assert_eq!(snapshot.send.contiguous_through, successes);
    assert_eq!(snapshot.revision, successes * 2);
    assert_eq!(received.snapshot().unwrap().receive.count, 0);
}

#[test]
fn decode_boot_age_future_and_order_rejections_do_not_count_receives() {
    let scope = Scope::enter(HOST_GUARD).unwrap();
    scope
        .set_pump(|_| scenario_clock::elapse_for_fault(Duration::from_secs(2)))
        .unwrap();
    scenario_clock::step(Phase::Run).unwrap();
    let at = scenario_clock::monotonic_us();
    let dir = SessionDirectory::create().unwrap();
    let (sender, sent, mut receiver, received) = pair(&dir.path);
    sender.endpoint.try_send(b"not an envelope").unwrap();
    assert_eq!(
        receiver.receive::<u64>().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    for (boot, sequence, timestamp) in [
        (boots().1, 1, at),
        (boots().0, 0, at),
        (boots().0, 1, at + 1),
        (boots().0, 1, at - 1_500_000),
    ] {
        raw(&sender, boot, sequence, timestamp);
        assert_eq!(
            receiver.receive::<u64>().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(received.snapshot().unwrap().receive.count, 0);
        assert_eq!(received.snapshot().unwrap().revision, 0);
    }
    raw(&sender, boots().0, 1, at);
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(1));
    let accepted = received.snapshot().unwrap();
    for (sequence, timestamp) in [(1, at), (0, at), (2, at - 1), (2, at + 1)] {
        raw(&sender, boots().0, sequence, timestamp);
        assert_eq!(
            receiver.receive::<u64>().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(received.snapshot().unwrap(), accepted);
    }
    assert_eq!(receiver.receive::<u64>().unwrap(), None);
    assert_eq!(received.snapshot().unwrap(), accepted);
    // Deliberately injected envelopes bypass Channel::send; they are not send
    // observations, while the receiver still performs real full validation.
    assert_eq!(sent.snapshot().unwrap().send.count, 0);
}

#[test]
fn accepted_sequence_gaps_keep_the_first_gap_and_do_not_extend_a_prefix() {
    let _scope = clock();
    let at = scenario_clock::monotonic_us();
    for (sequences, prefix, expected) in [([1, 3, 4], 1, 2), ([3, 4, 6], 0, 1)] {
        let dir = SessionDirectory::create().unwrap();
        let (sender, _sent, mut receiver, received) = pair(&dir.path);
        for sequence in sequences {
            raw(&sender, boots().0, sequence, at);
            assert_eq!(receiver.receive::<u64>().unwrap(), Some(sequence));
        }
        let observed = received.snapshot().unwrap().receive;
        assert_eq!(observed.count, 3);
        assert_eq!(observed.last_sequence, Some(sequences[2]));
        assert_eq!(observed.contiguous_through, prefix);
        assert_eq!(
            observed.first_gap,
            Some(Gap {
                expected,
                observed: 3
            })
        );
        raw(&sender, boots().0, 2, at);
        assert!(receiver.receive::<u64>().is_err());
        assert_eq!(received.snapshot().unwrap().receive, observed);
    }
}

#[test]
fn accepted_grace_and_one_sender_stall_count_only_after_the_original_checks() {
    let scope = Scope::enter(HOST_GUARD).unwrap();
    scope
        .set_pump(|_| scenario_clock::elapse_for_fault(Duration::from_millis(200)))
        .unwrap();
    let dir = SessionDirectory::create().unwrap();
    let (sender, _sent, receiver, received) = pair(&dir.path);
    let mut receiver = receiver.with_first_grace();
    let before_stall = scenario_clock::monotonic_us();
    raw(&sender, boots().0, 1, before_stall);
    scenario_clock::step(Phase::Run).unwrap();
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(1));
    raw(&sender, boots().0, 2, before_stall);
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(2));
    let after_exception = received.snapshot().unwrap();
    assert_eq!(after_exception.receive.count, 2);
    raw(&sender, boots().0, 3, before_stall);
    assert!(receiver.receive::<u64>().is_err());
    assert_eq!(received.snapshot().unwrap(), after_exception);
    raw(&sender, boots().0, 3, scenario_clock::monotonic_us());
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(3));
    assert_eq!(received.snapshot().unwrap().receive.contiguous_through, 3);
}

#[test]
fn snapshots_keep_allocation_identity_after_channel_and_probe_drop() {
    let dir = SessionDirectory::create().unwrap();
    let (a, b) = boots();
    let first = {
        let mut channel = Channel::bind(&dir.path.join("same"), a, b, 100_000).unwrap();
        let probe = channel.install_probe().unwrap();
        probe.snapshot().unwrap()
    };
    let mut channel = Channel::bind(&dir.path.join("same"), a, b, 100_000).unwrap();
    let probe = channel.install_probe().unwrap();
    let second = probe.snapshot().unwrap();
    assert_eq!(first.channel.name(), second.channel.name());
    assert_eq!(first.send, second.send);
    assert_eq!(first.receive, second.receive);
    assert_ne!(first.channel, second.channel);
    let retained = second.clone();
    drop(channel);
    drop(probe);
    assert_eq!(retained.channel, second.channel);
    assert_ne!(retained.channel, first.channel);
}

#[test]
fn late_installation_is_rejected_even_after_failed_or_empty_io() {
    let _scope = clock();
    let (a, b) = boots();
    let dir = SessionDirectory::create().unwrap();
    let mut send_failure =
        Channel::bind(&dir.path.join("unconnected-send"), a, b, 100_000).unwrap();
    assert!(send_failure.send(1_u64).is_err());
    assert!(send_failure.install_probe().is_err());
    let mut encode_failure =
        Channel::bind(&dir.path.join("encode-failure"), a, b, 100_000).unwrap();
    assert!(encode_failure.send("x".repeat(MAX_DATAGRAM_BYTES)).is_err());
    assert!(encode_failure.install_probe().is_err());
    let mut receive_failure =
        Channel::bind(&dir.path.join("unconnected-receive"), a, b, 100_000).unwrap();
    assert!(receive_failure.receive::<u64>().is_err());
    assert!(receive_failure.install_probe().is_err());
    let mut sender = Channel::bind(&dir.path.join("s"), a, b, 100_000).unwrap();
    let mut receiver = Channel::bind(&dir.path.join("r"), b, a, 100_000).unwrap();
    sender.connect(&dir.path.join("r")).unwrap();
    receiver.connect(&dir.path.join("s")).unwrap();
    assert_eq!(receiver.receive::<u64>().unwrap(), None);
    assert!(receiver.install_probe().is_err());
    sender.send(4_u64).unwrap();
    assert!(sender.install_probe().is_err());
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(4));
}

#[test]
fn a_latched_probe_fault_never_changes_actual_send_or_receive_success() {
    let _scope = clock();
    let dir = SessionDirectory::create().unwrap();
    let (mut sender, sent, mut receiver, received) = pair(&dir.path);
    assert!(sender.install_probe().is_err());
    assert_eq!(
        sent.snapshot(),
        Err(SnapshotError::Failed(Fault::AlreadyInstalled))
    );
    // Invalid-state instrumentation affects only the observer's local counters.
    receiver.probe.writer.as_mut().unwrap().receive.count = 1;
    sender.send(8_u64).unwrap();
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(8));
    assert_eq!(
        received.snapshot(),
        Err(SnapshotError::Failed(Fault::InvalidState))
    );
    sender.send(9_u64).unwrap();
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(9));
    assert_eq!(sender.send_sequence, 2);
    assert_eq!(sent.shared.send.count.load(Ordering::SeqCst), 0);
    assert_eq!(received.shared.receive.count.load(Ordering::SeqCst), 0);
    assert_eq!(
        sent.snapshot(),
        Err(SnapshotError::Failed(Fault::AlreadyInstalled))
    );
    assert_eq!(
        received.snapshot(),
        Err(SnapshotError::Failed(Fault::InvalidState))
    );
}

#[test]
fn counter_and_publication_overflow_latch_without_fabricating_success() {
    let _scope = clock();
    for fault in [Fault::CountOverflow, Fault::RevisionOverflow] {
        let dir = SessionDirectory::create().unwrap();
        let (mut sender, sent, mut receiver, _received) = pair(&dir.path);
        let writer = sender.probe.writer.as_mut().unwrap();
        // Boundary instrumentation reaches otherwise impractical counter limits;
        // these assigned values are not evidence that IPC work happened.
        match fault {
            Fault::CountOverflow => {
                writer.send.count = u64::MAX;
                writer.send.last_sequence = Some(u64::MAX);
                writer.send.contiguous_through = u64::MAX;
            }
            Fault::RevisionOverflow => {
                writer.revision = u64::MAX - 1;
                writer
                    .shared
                    .revision
                    .store(writer.revision, Ordering::SeqCst);
            }
            _ => unreachable!(),
        }
        sender.send(42_u64).unwrap();
        assert_eq!(receiver.receive::<u64>().unwrap(), Some(42));
        assert_eq!(sent.snapshot(), Err(SnapshotError::Failed(fault)));
        assert_eq!(sent.shared.send.count.load(Ordering::SeqCst), 0);
        sender.send(43_u64).unwrap();
        assert_eq!(receiver.receive::<u64>().unwrap(), Some(43));
        assert_eq!(sent.snapshot(), Err(SnapshotError::Failed(fault)));
        assert_eq!(sent.shared.send.count.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn boot_order_and_shared_state_faults_are_sticky_and_do_not_publish_more() {
    for fault in [
        Fault::WrongBoot,
        Fault::NonIncreasingSequence,
        Fault::InvalidState,
    ] {
        let (a, b) = boots();
        let mut slot = Slot::new(a, b);
        let probe = slot.install("state-boundary").unwrap();
        slot.record(Direction::Send, a, 1);
        // Private state-machine instrumentation, not an actual IPC observation.
        match fault {
            Fault::WrongBoot => slot.record(Direction::Send, b, 2),
            Fault::NonIncreasingSequence => slot.record(Direction::Send, a, 1),
            Fault::InvalidState => {
                probe.shared.revision.store(4, Ordering::SeqCst);
                slot.record(Direction::Send, a, 2);
            }
            _ => unreachable!(),
        }
        assert_eq!(probe.snapshot(), Err(SnapshotError::Failed(fault)));
        slot.record(Direction::Send, a, 3);
        assert_eq!(probe.snapshot(), Err(SnapshotError::Failed(fault)));
        assert_eq!(probe.shared.send.count.load(Ordering::SeqCst), 1);
        assert_eq!(probe.shared.send.last_sequence.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn a_snapshot_of_corrupt_stable_state_latches_failure() {
    for corruption in 0..3 {
        let (a, b) = boots();
        let mut slot = Slot::new(a, b);
        let probe = slot.install("invalid-snapshot").unwrap();
        // Corruption instrumentation: missing sequence, incomplete gap pair,
        // and revision without an observation are all invalid stable states.
        match corruption {
            0 => probe.shared.send.count.store(1, Ordering::SeqCst),
            1 => probe.shared.send.gap_observed.store(3, Ordering::SeqCst),
            2 => probe.shared.revision.store(2, Ordering::SeqCst),
            _ => unreachable!(),
        }
        let before_count = probe.shared.send.count.load(Ordering::SeqCst);
        let before_revision = probe.shared.revision.load(Ordering::SeqCst);
        assert_eq!(
            probe.snapshot(),
            Err(SnapshotError::Failed(Fault::InvalidState))
        );
        slot.record(Direction::Send, a, 1);
        assert_eq!(
            probe.snapshot(),
            Err(SnapshotError::Failed(Fault::InvalidState))
        );
        assert_eq!(probe.shared.send.count.load(Ordering::SeqCst), before_count);
        assert_eq!(
            probe.shared.revision.load(Ordering::SeqCst),
            before_revision
        );
    }
}

#[test]
fn causally_held_publication_returns_busy_without_waiting_or_claiming_a_receipt() {
    let _scope = clock();
    let before_clock = (scenario_clock::monotonic_us(), scenario_clock::instant());
    let (a, b) = boots();
    let mut slot = Slot::new(a, b);
    let probe = slot.install("held-publication").unwrap();
    // Publication-state instrumentation uses the same begin/finish operations
    // as the writer. This case makes no claim that an IPC operation occurred.
    std::thread::scope(|threads| {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let publisher = threads.spawn(move || {
            let writer = slot.writer.as_mut().unwrap();
            let next = writer.send.next(a, 1).unwrap();
            let revision = writer.shared.begin_publication(writer.revision).unwrap();
            entered_tx.send(()).unwrap();
            match release_rx.recv_timeout(HOST_GUARD) {
                Ok(()) => {}
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => panic!("held publication was not released"),
            }
            writer.send = next;
            writer
                .shared
                .finish_publication(revision, writer.send, writer.receive);
            writer.revision = revision;
        });
        receive(&entered_rx);
        assert_eq!(probe.snapshot(), Err(SnapshotError::Busy));
        assert_eq!(probe.shared.send.count.load(Ordering::SeqCst), 0);
        assert_eq!(probe.shared.failure(), None);
        release_tx.send(()).unwrap();
        publisher.join().unwrap();
    });
    assert_eq!(probe.snapshot().unwrap().revision, 2);
    assert_eq!(probe.snapshot().unwrap().send.count, 1);
    assert_eq!(
        (scenario_clock::monotonic_us(), scenario_clock::instant()),
        before_clock
    );
}

#[test]
fn ordinary_channel_worker_and_reference_constructors_have_no_probe() {
    let _scope = clock();
    let (a, b) = boots();
    let dir = SessionDirectory::create().unwrap();
    let mut sender = Channel::bind(&dir.path.join("s"), a, b, 100_000).unwrap();
    let mut receiver = Channel::bind(&dir.path.join("r"), b, a, 100_000).unwrap();
    sender.connect(&dir.path.join("r")).unwrap();
    receiver.connect(&dir.path.join("s")).unwrap();
    sender.send(5_u64).unwrap();
    assert_eq!(receiver.receive::<u64>().unwrap(), Some(5));
    let worker = WorkerChannels::bind(&dir.path, "provider", a, b, false).unwrap();
    let reference = ReferenceChannels::bind(&dir.path, "speaker", a, b).unwrap();
    for channel in [
        &sender,
        &receiver,
        &worker.control,
        &worker.data,
        &reference.control,
        &reference.data,
    ] {
        assert!(channel.probe.writer.is_none());
    }
}
