//! Local sockets and synthetic capture only. These tests measure the injected
//! coordinator-observation to speaker-control-send boundary, not bus delivery,
//! physical input detection, device playback, or acoustic silence.
use super::*;
use crate::activity::{ConfirmPolicy, START_BLOCKS};
use lamp_audio::ownership::PlaybackCursor;
use lamp_turn_end_wire::{Availability, Classifier, Outcome, Submit, Token};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn hand_pats_report_only_accepted_metadata_without_inventing_a_turn() {
    let mut trace = SessionState::default();
    record_hand_pats(&mut trace, 0).unwrap();
    assert!(trace.is_empty());
    record_hand_pats(&mut trace, 2).unwrap();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0]["kind"], "hand_pat_observed");
    assert_eq!(trace[0]["count"], 2);
    assert_eq!(trace[0]["action"], "none");
    assert!(trace[0].get("turn").is_none());
}

fn received_authority(peer: &mut WorkerChannels) -> Snapshot {
    let Some(Control::Authority { snapshot }) = peer.control.receive().unwrap() else {
        panic!("speaker was not sent the authority revocation");
    };
    snapshot
}

fn pending(permit: OutputPermit, at_us: u64) -> PendingSpeaker {
    PendingSpeaker {
        origin: SpeechOrigin::Provider,
        sequence: 1,
        permit,
        accepted: 120,
        final_chunk: false,
        report_due_us: at_us + 100_000,
    }
}

#[test]
fn speaking_stop_sends_revocation_before_clock_receipt_and_drops_both_pcm_queues() {
    for muted in [false, true] {
        let directory = SessionDirectory::create().unwrap();
        let (mut speaker, mut peer) = pair(&directory, "speaker");
        let (mut owner, current, old_permit) = active_reply();
        owner
            .set_camera_permission(now(), Permission::Allowed)
            .unwrap();
        let old_owner = current.owner;
        let mut reply = Some(current);
        let mut queue = VecDeque::from([start_command(&reply), end_command(&reply)]);
        let observed_at_us = monotonic_us();
        let mut held = Some(pending(old_permit, observed_at_us));
        let mut admission = InputAdmission::default();
        let mut detector = TurnDetector::default();
        let mut clock_calls = 0;
        let cancellation = hand::cancel(
            &mut owner,
            &mut reply,
            &mut speaker.control,
            &mut held,
            &mut queue,
            &mut admission,
            &mut detector,
            muted,
            observed_at_us,
            || {
                clock_calls += 1;
                // Inspect the actual datagram inside the clock callback. The
                // receipt timestamp must be sampled after the successful send.
                let snapshot = received_authority(&mut peer);
                assert!(snapshot.owner().is_none());
                let at = MonoTime::from_micros(observed_at_us);
                let mut guard = BoundaryGuard::new(snapshot.boot(), at);
                guard.install(at, snapshot).unwrap();
                assert!(guard.check(at, old_permit, OutputKind::Speech).is_err());
                observed_at_us + 1_500
            },
        )
        .unwrap();
        assert_eq!(clock_calls, 1);
        assert_eq!(cancellation.speaker_told_at_us - observed_at_us, 1_500);
        assert!(cancellation.speaker_told_at_us - observed_at_us <= 2_000);
        assert_eq!(cancellation.cancelled_turn, Some(old_owner.turn()));
        assert!(reply.is_none() && held.is_none() && queue.is_empty());
        let cancelled = cancellation.reply.unwrap();
        assert_eq!(cancelled.owner, old_owner);
        assert!(cancelled.pcm.is_empty());
        assert_eq!(
            cancellation.snapshot.microphone_permission(),
            if muted {
                Permission::Denied
            } else {
                Permission::Allowed
            }
        );
        assert_eq!(
            cancellation.snapshot.camera_permission(),
            if muted {
                Permission::Denied
            } else {
                Permission::Allowed
            }
        );
        let mut snapshot = cancellation.snapshot;
        dispatch_speaker_chunk(
            &mut owner,
            &mut reply,
            &mut speaker.data,
            &mut held,
            &mut 0,
            &mut snapshot,
            &mut PlaybackMask::new(TurnTaking::VadOnly),
        )
        .unwrap();
        assert!(peer.data.receive::<SpeakerChunk>().unwrap().is_none());
    }
}

#[test]
fn thinking_and_idle_stop_are_local_and_repeated_stop_has_no_new_owner_to_cancel() {
    for idle in [false, true] {
        let directory = SessionDirectory::create().unwrap();
        let (mut speaker, mut peer) = pair(&directory, "speaker");
        let (mut owner, mut reply) = current_input();
        let turn = reply.as_ref().unwrap().owner;
        owner.input_ended(now(), turn).unwrap();
        reply.as_mut().unwrap().local_input_ended = true;
        if idle {
            owner.cancel_turn(now(), turn).unwrap();
            reply = None;
        }
        let mut admission = InputAdmission::default();
        let mut detector = TurnDetector::default();
        let mut generation = None;
        for repeated in [false, true] {
            let observed = monotonic_us();
            let cancellation = hand::cancel(
                &mut owner,
                &mut reply,
                &mut speaker.control,
                &mut None,
                &mut VecDeque::new(),
                &mut admission,
                &mut detector,
                false,
                observed,
                || observed,
            )
            .unwrap();
            let received = received_authority(&mut peer);
            assert_eq!(received.generation(), cancellation.snapshot.generation());
            assert!(received.owner().is_none());
            assert_eq!(received.microphone_permission(), Permission::Allowed);
            assert_eq!(cancellation.speaker_told_at_us - observed, 0);
            if idle || repeated {
                assert!(cancellation.cancelled_turn.is_none());
                assert!(cancellation.reply.is_none());
            } else {
                assert_eq!(cancellation.cancelled_turn, Some(turn.turn()));
                assert!(cancellation.reply.is_some());
            }
            if let Some(generation) = generation {
                assert_eq!(received.generation(), generation);
            }
            generation = Some(received.generation());
        }
    }
}

fn input_context(owner: &mut Controller, capture: CaptureLineage, at_us: u64) -> AdmissionContext {
    let at = MonoTime::from_micros(at_us);
    let until = MonoTime::from_micros(at_us + 100_000);
    owner
        .set_capture(at, CaptureState::RetainingUntil(until))
        .unwrap();
    owner
        .set_admission(at, AdmissionState::OpenUntil(until))
        .unwrap();
    AdmissionContext {
        capture,
        authority: owner.snapshot(at).unwrap(),
    }
}

fn audio(sequence: u64, captured_at_us: u64) -> ObservedAudio {
    ObservedAudio {
        sequence,
        captured_at_us,
        samples: [sequence as i16; 160],
    }
}

#[derive(Default)]
struct EndpointCalls {
    cancelled: AtomicUsize,
    submitted: AtomicUsize,
    polled: AtomicUsize,
}

struct EndpointProbe(Arc<EndpointCalls>);
impl Classifier for EndpointProbe {
    fn submit(&mut self, _: Token, _: &[i16]) -> Submit {
        self.0.submitted.fetch_add(1, Ordering::SeqCst);
        Submit::Accepted
    }
    fn poll(&mut self) -> Option<Outcome> {
        self.0.polled.fetch_add(1, Ordering::SeqCst);
        None
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn cancel(&mut self) {
        self.0.cancelled.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn pending_and_streaming_speech_drain_to_silence_even_under_playback_suppression() {
    for streaming in [false, true] {
        for suppressed in [false, true] {
            let directory = SessionDirectory::create().unwrap();
            let (mut speaker, mut peer) = pair(&directory, "speaker");
            let (mut owner, mut reply) = current_input();
            let previous_owner = reply.as_ref().unwrap().owner;
            if !streaming {
                owner.cancel_turn(now(), previous_owner).unwrap();
                reply = None;
            }
            let capture = CaptureLineage {
                worker: new_boot().unwrap(),
                epoch: 1,
                dsp_epoch: 1,
                privacy_generation: owner.snapshot(now()).unwrap().microphone_generation(),
            };
            let calls = Arc::new(EndpointCalls::default());
            let mut detector = TurnDetector::default()
                .with_confirmation(
                    Box::new(EndpointProbe(Arc::clone(&calls))),
                    ConfirmPolicy::responsive(),
                )
                .unwrap();
            let mut admission = InputAdmission::default();
            let mut mask = PlaybackMask::new(if suppressed {
                TurnTaking::V1Baseline
            } else {
                TurnTaking::VadOnly
            });
            let base = monotonic_us() + 10_000;
            let mut first = None;
            for sequence in 1..=u64::from(START_BLOCKS) {
                let at = base + sequence * 10_000;
                let context = input_context(&mut owner, capture, at);
                let (update, ended) = observe_prospective_input(
                    &mut mask,
                    &mut detector,
                    &mut admission,
                    context,
                    audio(sequence, at),
                    at - 1_000,
                    0.99,
                    at,
                )
                .unwrap();
                assert!(!ended);
                if let AdmissionStep::Candidate(candidate) = update.step {
                    first = Some(candidate.id);
                    if streaming {
                        let accepted = admission
                            .decide(
                                Evidence {
                                    candidate: candidate.id,
                                    through_sequence: candidate.trigger_sequence,
                                    produced_at_us: at,
                                    verdict: Verdict::Accept(
                                        AcceptanceBasis::DirectedSessionVadOnly,
                                    ),
                                },
                                context,
                                at,
                            )
                            .unwrap();
                        assert!(matches!(accepted, Decision::Accepted(_)));
                    }
                }
            }
            assert!(first.is_some());
            let stopped_at = base + u64::from(START_BLOCKS) * 10_000 + 1;
            if suppressed {
                // Playback starts after this human candidate already exists.
                // Its real accepted interval remains until a later receipt.
                mask.dispatched(previous_owner, 1, stopped_at, 240).unwrap();
                mask.accepted(
                    previous_owner,
                    1,
                    PlaybackCursor { epoch: 1, frame: 0 },
                    PlaybackCursor {
                        epoch: 1,
                        frame: 240,
                    },
                    240,
                    stopped_at,
                    stopped_at,
                    stopped_at,
                )
                .unwrap();
            }
            let cancellations_before = calls.cancelled.load(Ordering::SeqCst);
            let polls_before = calls.polled.load(Ordering::SeqCst);
            hand::cancel(
                &mut owner,
                &mut reply,
                &mut speaker.control,
                &mut None,
                &mut VecDeque::new(),
                &mut admission,
                &mut detector,
                false,
                stopped_at,
                || stopped_at + 700,
            )
            .unwrap();
            assert!(received_authority(&mut peer).owner().is_none());
            assert!(admission.draining());
            assert!(calls.cancelled.load(Ordering::SeqCst) > cancellations_before);
            // Twenty more speech blocks, then the unchanged 60-block silence
            // boundary. No continuation, endpoint, or fresh candidate escapes.
            for sequence in 7..=86 {
                let at = base + sequence * 10_000;
                let context = input_context(&mut owner, capture, at);
                let (update, ended) = observe_prospective_input(
                    &mut mask,
                    &mut detector,
                    &mut admission,
                    context,
                    audio(sequence, at),
                    at - 1_000,
                    if sequence <= 26 { 0.99 } else { 0.0 },
                    at,
                )
                .unwrap();
                assert!(matches!(update.step, AdmissionStep::None));
                assert_eq!(ended, sequence == 86);
                assert_eq!(admission.draining(), sequence != 86);
                if suppressed {
                    assert_eq!(mask.frame(), Disposition::Suppress);
                }
            }
            assert_eq!(calls.submitted.load(Ordering::SeqCst), 0);
            assert_eq!(calls.polled.load(Ordering::SeqCst), polls_before);
            assert_eq!(detector.last_endpoint().unwrap().silence_ms, 600);
            let ended_at = base + 86 * 10_000;
            if suppressed {
                mask.discarded(previous_owner, ended_at, ended_at).unwrap();
            }
            // Wait beyond the speaker tail, then six genuinely new blocks.
            // The only resulting candidate has a new identity and later onset.
            for sequence in 87..=104 {
                let at = base + sequence * 10_000;
                let context = input_context(&mut owner, capture, at);
                let (update, ended) = observe_prospective_input(
                    &mut mask,
                    &mut detector,
                    &mut admission,
                    context,
                    audio(sequence, at),
                    at - 1_000,
                    if sequence >= 99 { 0.99 } else { 0.0 },
                    at,
                )
                .unwrap();
                assert!(!ended);
                if sequence == 104 {
                    let AdmissionStep::Candidate(candidate) = update.step else {
                        panic!("fresh utterance did not become a new candidate");
                    };
                    assert_ne!(Some(candidate.id), first);
                    assert_eq!(candidate.trigger_sequence, 104);
                    assert!(candidate.first_read_at_us > ended_at);
                } else {
                    assert!(matches!(update.step, AdmissionStep::None));
                }
            }
        }
    }
}

/// #891, the reviewer's 04:30 finding: a hand Stop ends a call's wait, in
/// either order. Stopped after the lamp's name armed the wait, or with a
/// sign of the name heard after the Stop, the abandoned utterance drains to
/// the ordinary 600 ms boundary, not the call's 2 s, and a fresh command
/// right after that boundary is a new candidate.
#[test]
fn a_hand_stop_ends_a_calls_wait_in_either_order_and_a_fresh_command_is_heard() {
    for sign_first in [true, false] {
        let directory = SessionDirectory::create().unwrap();
        let (mut speaker, mut peer) = pair(&directory, "speaker");
        let (mut owner, mut reply) = current_input();
        let previous_owner = reply.as_ref().unwrap().owner;
        owner.cancel_turn(now(), previous_owner).unwrap();
        reply = None;
        let capture = CaptureLineage {
            worker: new_boot().unwrap(),
            epoch: 1,
            dsp_epoch: 1,
            privacy_generation: owner.snapshot(now()).unwrap().microphone_generation(),
        };
        let mut detector = TurnDetector::default().with_call_wait(200).unwrap();
        let mut admission = InputAdmission::default();
        let mut mask = PlaybackMask::new(TurnTaking::VadOnly);
        let base = monotonic_us() + 10_000;
        let mut step = |owner: &mut Controller,
                        detector: &mut TurnDetector,
                        admission: &mut InputAdmission,
                        sequence: u64,
                        score: f32| {
            let at = base + sequence * 10_000;
            let context = input_context(owner, capture, at);
            observe_prospective_input(
                &mut mask,
                detector,
                admission,
                context,
                audio(sequence, at),
                at - 1_000,
                score,
                at,
            )
            .unwrap()
        };
        // "Hey Lamp," 0.35 s.
        let mut first = None;
        for sequence in 1..=35 {
            let (update, _) = step(&mut owner, &mut detector, &mut admission, sequence, 0.99);
            if let AdmissionStep::Candidate(candidate) = update.step {
                first = Some(candidate.id);
            }
        }
        assert!(first.is_some());
        let began_us = base + 10_000;
        if sign_first {
            assert!(detector.name_heard(began_us), "the call did not wait");
        }
        let stopped_at = base + 35 * 10_000 + 1;
        hand::cancel(
            &mut owner,
            &mut reply,
            &mut speaker.control,
            &mut None,
            &mut VecDeque::new(),
            &mut admission,
            &mut detector,
            false,
            stopped_at,
            || stopped_at + 700,
        )
        .unwrap();
        assert!(received_authority(&mut peer).owner().is_none());
        assert!(admission.draining());
        if !sign_first {
            // The sign comes 100 ms after the Stop, as the name's part
            // says a hearing once it falls.
            assert!(
                !detector.name_heard(began_us),
                "the Stop's utterance waited"
            );
        }
        // The ordinary 600 ms boundary ends it, not the call's 2 s.
        for sequence in 36..=95 {
            let (update, ended) = step(&mut owner, &mut detector, &mut admission, sequence, 0.0);
            assert!(matches!(update.step, AdmissionStep::None));
            assert_eq!(ended, sequence == 95, "{sign_first} {sequence}");
        }
        assert!(!admission.draining());
        assert_eq!(detector.last_endpoint().unwrap().silence_ms, 600);
        assert_eq!(detector.last_endpoint().unwrap().call_wait_ms, None);
        // A fresh command, after 200 ms of quiet: a new candidate.
        for sequence in 96..=115 {
            let (update, ended) = step(&mut owner, &mut detector, &mut admission, sequence, 0.0);
            assert!(!ended && matches!(update.step, AdmissionStep::None));
        }
        let mut fresh = None;
        for sequence in 116..=121 {
            let (update, _) = step(&mut owner, &mut detector, &mut admission, sequence, 0.99);
            if let AdmissionStep::Candidate(candidate) = update.step {
                fresh = Some(candidate);
            }
        }
        let fresh = fresh.expect("the fresh command was dropped");
        assert_ne!(Some(fresh.id), first);
        assert_eq!(fresh.trigger_sequence, 121);
    }
}

#[test]
fn idle_stop_fences_already_captured_audio_but_accepts_a_later_utterance() {
    let directory = SessionDirectory::create().unwrap();
    let (mut speaker, mut peer) = pair(&directory, "speaker");
    let (mut owner, current, _) = active_reply();
    owner.cancel_turn(now(), current.owner).unwrap();
    let capture = CaptureLineage {
        worker: new_boot().unwrap(),
        epoch: 1,
        dsp_epoch: 1,
        privacy_generation: owner.snapshot(now()).unwrap().microphone_generation(),
    };
    let mut detector = TurnDetector::default();
    let mut admission = InputAdmission::default();
    let mut mask = PlaybackMask::new(TurnTaking::VadOnly);
    let base = monotonic_us() + 1_000;
    let stopped_at = base + 20_000;
    let cancellation = hand::cancel(
        &mut owner,
        &mut None,
        &mut speaker.control,
        &mut None,
        &mut VecDeque::new(),
        &mut admission,
        &mut detector,
        false,
        stopped_at,
        || stopped_at,
    )
    .unwrap();
    assert!(cancellation.cancelled_turn.is_none());
    assert!(received_authority(&mut peer).owner().is_none());
    for sequence in 1..=26 {
        let captured = if sequence <= 20 {
            base + sequence * 1_000
        } else {
            stopped_at + (sequence - 20) * 10_000
        };
        let at = stopped_at.max(captured) + sequence;
        let context = input_context(&mut owner, capture, at);
        let (update, ended) = observe_prospective_input(
            &mut mask,
            &mut detector,
            &mut admission,
            context,
            audio(sequence, captured),
            captured,
            0.99,
            at,
        )
        .unwrap();
        assert!(!ended);
        if sequence < 26 {
            assert!(matches!(update.step, AdmissionStep::None));
        } else {
            let AdmissionStep::Candidate(candidate) = update.step else {
                panic!("fresh post-Stop speech was not admitted as a candidate");
            };
            assert_eq!(candidate.trigger_sequence, 26);
            assert!(candidate.first_read_at_us > stopped_at);
        }
    }
}

#[test]
fn a_hand_abandoned_pending_utterance_still_fails_on_physical_privacy_loss() {
    let directory = SessionDirectory::create().unwrap();
    let (mut speaker, mut peer) = pair(&directory, "speaker");
    let (mut owner, current, _) = active_reply();
    owner.cancel_turn(now(), current.owner).unwrap();
    let capture = CaptureLineage {
        worker: new_boot().unwrap(),
        epoch: 1,
        dsp_epoch: 1,
        privacy_generation: owner.snapshot(now()).unwrap().microphone_generation(),
    };
    let mut detector = TurnDetector::default();
    let mut admission = InputAdmission::default();
    let mut mask = PlaybackMask::new(TurnTaking::V1Baseline);
    let base = monotonic_us() + 1_000;
    for sequence in 1..=6 {
        let at = base + sequence * 10_000;
        let context = input_context(&mut owner, capture, at);
        let (update, _) = observe_prospective_input(
            &mut mask,
            &mut detector,
            &mut admission,
            context,
            audio(sequence, at),
            at - 1_000,
            0.99,
            at,
        )
        .unwrap();
        if sequence == 6 {
            assert!(matches!(update.step, AdmissionStep::Candidate(_)));
        }
    }
    let stopped_at = base + 60_001;
    hand::cancel(
        &mut owner,
        &mut None,
        &mut speaker.control,
        &mut None,
        &mut VecDeque::new(),
        &mut admission,
        &mut detector,
        false,
        stopped_at,
        || stopped_at,
    )
    .unwrap();
    assert!(received_authority(&mut peer).owner().is_none());
    assert!(admission.draining());
    let at = base + 70_000;
    owner
        .set_microphone_permission(MonoTime::from_micros(at), Permission::Denied)
        .unwrap();
    let context = AdmissionContext {
        capture,
        authority: owner.snapshot(MonoTime::from_micros(at)).unwrap(),
    };
    let error = observe_prospective_input(
        &mut mask,
        &mut detector,
        &mut admission,
        context,
        audio(7, at),
        at - 1_000,
        0.99,
        at,
    )
    .unwrap_err();
    assert!(error.to_string().contains("lost capture authority"));
    assert!(!admission.draining());
    assert!(!admission.owns_stream());
}

#[test]
fn conversation_sends_hand_stop_before_a_provider_fault_queued_in_the_same_tick() {
    use lamp_msgs::{
        ClockStamp, ControlGesture, ControlIntent, ControlSource, ControlsState, Gesture, HoldTier,
    };
    let directory = SessionDirectory::create().unwrap();
    let (speaker, mut speaker_peer, speaker_input) =
        Worker::cooperative_fixture(&directory.path, "speaker");
    let (capture, capture_peer, capture_input) =
        Worker::cooperative_fixture(&directory.path, "capture");
    let (provider, mut provider_peer, provider_input) =
        Worker::cooperative_fixture(&directory.path, "provider");
    let (privacy, privacy_peer, privacy_input) =
        Worker::cooperative_fixture(&directory.path, "privacy");
    let mut rig = Rig {
        second: None,
        microphone: crate::options::Microphone::Mic2,
        end_silence_blocks: None,
        faint_level: None,
        speaker,
        capture,
        provider,
        privacy,
        ring: None.into(),
        camera: None,
        motor: None,
        ring_channel_ceiling: None,
        diagnostic_directory: None,
        noise_suppression: NoiseSuppression::On,
        turn_taking: TurnTaking::VadOnly,
        playout_start: PlayoutStart::default(),
        endpoint: None,
        call_wait_blocks: None,
        cut_keeps_name: false,
        paused: Box::new(|| false),
        early_end_blocks: None,
    };
    let (mut owner, current, permit) = active_reply();
    let old_owner = current.owner;
    let mut reply = Some(current);
    let at_us = monotonic_us();
    let stamp = || ClockStamp {
        clock_id: "test-hand-clock".into(),
        at_us,
        sequence: 1,
    };
    let state = ControlsState {
        stamp: stamp(),
        session_id: "test-controls".into(),
        enabled: true,
        ready: true,
        fault: None,
        buttons: vec![],
        head_active: false,
        base_ready: false,
        base_held_us: None,
        base_tier: HoldTier::None,
        latest_stop: Some(ControlGesture {
            stamp: stamp(),
            session_id: "test-controls".into(),
            source: ControlSource::Button,
            name: "primary".into(),
            gesture: Gesture::Tap,
            intent: ControlIntent::Stop,
            duration_us: 100_000,
            released_us: at_us,
            enabled: true,
        }),
        latest_mute: None,
        event_sequence: 1,
        rejected_inputs: 0,
    };
    state.check().unwrap();
    let mut controls =
        Some(HandControls::with_test_state("test-hand-clock", at_us, state).unwrap());
    provider_peer
        .control
        .send(WorkerEvent::Fault {
            code: "synthetic same-tick provider fault".into(),
        })
        .unwrap();
    let mut trace = SessionState::default();
    let error = conversation_with_controls(
        &mut rig,
        &mut owner,
        Lifetime::Serve,
        &mut trace,
        StartupEvents::default(),
        &mut reply,
        &mut None,
        &Termination::inactive(),
        &FailureNotices::none(),
        &mut controls,
        &mut None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("same-tick provider fault"));
    // Read the real speaker socket before finish_active_run. No teardown
    // revocation can satisfy these assertions: the production loop itself
    // must send its cancellation before returning the already-queued fault.
    assert_eq!(
        received_authority(&mut speaker_peer).owner(),
        Some(old_owner)
    );
    let stopped = received_authority(&mut speaker_peer);
    assert!(stopped.owner().is_none());
    assert!(reply.is_none());
    let at = now();
    let mut guard = BoundaryGuard::new(stopped.boot(), at);
    guard.install(at, stopped).unwrap();
    assert!(guard.check(at, permit, OutputKind::Speech).is_err());
    assert!(trace.iter().any(|row| {
        row["kind"] == "turn_finished"
            && row["turn"] == old_owner.turn()
            && row["outcome"] == "hand_stop"
    }));

    // The mock peers close their own pipes on Stop. Every child has bounded
    // cooperative teardown, and Worker::drop still reaps on assertion failure.
    let peers = [
        (speaker_peer, speaker_input),
        (capture_peer, capture_input),
        (provider_peer, provider_input),
        (privacy_peer, privacy_input),
    ]
    .map(|(mut peer, input)| {
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match peer.control.receive::<Control>().unwrap() {
                    Some(Control::Stop) => {
                        drop(input);
                        return peer;
                    }
                    Some(Control::Authority { .. }) | None => {}
                    Some(_) => panic!("unexpected command during hand-stop fixture cleanup"),
                }
                assert!(Instant::now() < deadline, "mock peer did not receive Stop");
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    });
    let error = finish_active_run(
        &mut rig,
        &mut owner,
        &mut reply,
        &mut trace,
        &mut None,
        Err(error),
    )
    .unwrap_err();
    assert!(error.to_string().contains("same-tick provider fault"));
    assert!(!error.to_string().contains("verified cleanup failed"));
    for peer in peers {
        drop(peer.join().unwrap());
    }
    for worker in [
        &mut rig.speaker,
        &mut rig.capture,
        &mut rig.provider,
        &mut rig.privacy,
    ] {
        assert!(worker.try_exit().unwrap().is_some());
    }
    drop(rig);
    fs::remove_dir(&directory.path).unwrap();
}

#[test]
fn queued_provider_failure_and_full_trace_cannot_preempt_the_local_speaker_send() {
    let directory = SessionDirectory::create().unwrap();
    let (mut speaker, mut speaker_peer) = pair(&directory, "speaker");
    let (mut provider, mut provider_peer) = pair(&directory, "provider");
    provider_peer
        .control
        .send(WorkerEvent::Fault {
            code: "synthetic simultaneous provider link drop".into(),
        })
        .unwrap();
    let (mut owner, current, permit) = active_reply();
    let mut reply = Some(current);
    let mut trace = SessionState::default();
    trace.resize(crate::session_state::MAX_FINITE_EVENTS - 1, Value::Null);
    let observed = monotonic_us();
    let cancellation = hand::cancel(
        &mut owner,
        &mut reply,
        &mut speaker.control,
        &mut None,
        &mut VecDeque::new(),
        &mut InputAdmission::default(),
        &mut TurnDetector::default(),
        false,
        observed,
        || {
            let state = received_authority(&mut speaker_peer);
            assert!(state.owner().is_none());
            observed + 2_000
        },
    )
    .unwrap();
    assert_eq!(cancellation.speaker_told_at_us - observed, 2_000);
    assert!(
        record_cancelled_reply(
            cancellation.reply.unwrap(),
            &mut trace,
            "hand_stop",
            cancellation.speaker_told_at_us,
        )
        .is_err()
    );
    let error = service_provider_control(
        &mut provider.control,
        &mut CloudAvailability::default(),
        &mut 16,
        &mut trace,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("simultaneous provider link drop")
    );
    let at = MonoTime::from_micros(observed);
    let mut guard = BoundaryGuard::new(cancellation.snapshot.boot(), at);
    guard.install(at, cancellation.snapshot).unwrap();
    assert!(guard.check(at, permit, OutputKind::Speech).is_err());
    assert!(reply.is_none());
}

#[test]
fn a_regressed_measurement_clock_is_an_error_after_actual_revocation() {
    let directory = SessionDirectory::create().unwrap();
    let (mut speaker, mut peer) = pair(&directory, "speaker");
    let (mut owner, current, permit) = active_reply();
    let mut reply = Some(current);
    let observed = monotonic_us();
    let error = hand::cancel(
        &mut owner,
        &mut reply,
        &mut speaker.control,
        &mut None,
        &mut VecDeque::new(),
        &mut InputAdmission::default(),
        &mut TurnDetector::default(),
        false,
        observed,
        || observed - 1,
    )
    .err()
    .expect("a regressed clock cannot produce a latency receipt");
    assert!(error.to_string().contains("clock moved backwards"));
    let snapshot = received_authority(&mut peer);
    let at = MonoTime::from_micros(observed);
    let mut guard = BoundaryGuard::new(snapshot.boot(), at);
    guard.install(at, snapshot).unwrap();
    assert!(guard.check(at, permit, OutputKind::Speech).is_err());
    assert!(reply.is_none());
}

#[test]
fn cancelled_request_tails_are_filtered_without_swallowing_new_requests_or_link_events() {
    use crate::provider_worker::{FailedStage, Fault};
    for request in [3, 4, 5] {
        let events = [
            ProviderOutput::Started {
                request,
                session: 1,
                waiting_for_barrier: false,
            },
            ProviderOutput::Audio {
                request,
                sequence: 1,
                provider_event_at_us: 1,
                source_frames: 240,
                source_offset: 0,
                samples: vec![17; 240],
            },
            ProviderOutput::Transcript {
                request: Some(request),
                text: "Let me look".into(),
                finished: false,
            },
            ProviderOutput::GenerationComplete { request },
            ProviderOutput::Interrupted { request },
            ProviderOutput::TurnComplete {
                request,
                idle: true,
            },
            ProviderOutput::TurnFailed {
                request,
                stage: FailedStage::Responding,
                reason: Fault::Transport,
            },
            ProviderOutput::InputDiscarded {
                request,
                dropped_samples: 160,
                sent_samples: 160,
            },
            ProviderOutput::OwnershipAssumed {
                request: Some(request),
                superseded: Some(2),
            },
        ];
        for event in events {
            assert!(!hand::cancelled_output(&event, None));
            assert_eq!(hand::cancelled_output(&event, Some(4)), request <= 4);
        }
    }
    for event in [
        ProviderOutput::Ready { setup_us: 12 },
        ended_output(),
        ProviderOutput::Transcript {
            request: None,
            text: "session-scoped evidence".into(),
            finished: true,
        },
        ProviderOutput::OwnershipAssumed {
            request: None,
            superseded: Some(3),
        },
    ] {
        assert!(!hand::cancelled_output(&event, Some(4)));
    }
}

#[test]
#[ignore = "manual local-host speaker-control latency probe; no lamp or acoustic claim"]
fn local_hand_stop_dispatch_latency() {
    let mut elapsed_us = Vec::with_capacity(100);
    for _ in 0..100 {
        // Each iteration has fresh authority and independent socket endpoints.
        // All setup, allocation, verification, and teardown is outside the
        // measured boundary; the provider has no role in this measurement.
        let directory = SessionDirectory::create().unwrap();
        let (mut speaker, mut peer) = pair(&directory, "speaker");
        let (mut owner, current, permit) = active_reply();
        let mut reply = Some(current);
        let mut held = Some(pending(permit, monotonic_us()));
        let mut queue = VecDeque::from([start_command(&reply), end_command(&reply)]);
        let mut admission = InputAdmission::default();
        let mut detector = TurnDetector::default();
        let observed = monotonic_us();
        let cancellation = hand::cancel(
            &mut owner,
            &mut reply,
            &mut speaker.control,
            &mut held,
            &mut queue,
            &mut admission,
            &mut detector,
            false,
            observed,
            monotonic_us,
        )
        .unwrap();
        elapsed_us.push(cancellation.speaker_told_at_us - observed);
        assert!(received_authority(&mut peer).owner().is_none());
        assert!(reply.is_none() && held.is_none() && queue.is_empty());
    }
    let first_us = elapsed_us[0];
    let mut repeated = elapsed_us[1..].to_vec();
    repeated.sort_unstable();
    let count = repeated.len();
    println!(
        "{}",
        json!({
            "measurement":"local_hand_stop_dispatch",
            "host_os":std::env::consts::OS,
            "host_arch":std::env::consts::ARCH,
            "clock":"actual lamp_ipc::monotonic_us",
            "boundary":"coordinator observation immediately before hand::cancel to clock sampled after successful local speaker Authority datagram send",
            "excludes":["fixture setup and teardown", "bus delivery", "hand gesture detection", "speaker worker scheduling", "device playback", "acoustic silence"],
            "samples":elapsed_us.len(),
            "first_us":first_us,
            "repeated_samples":count,
            "repeated_median_us":repeated[(count - 1) / 2],
            "repeated_p95_order_statistic_us":repeated[(count * 95).div_ceil(100) - 1],
            "repeated_worst_us":repeated[count - 1],
            "worst_us":elapsed_us.into_iter().max(),
            "injected_clock_test_intervals_us":[0, 700, 1500, 2000],
            "injected_clock_target_us":2000,
            "lamp_measurement":false
        })
    );
}
