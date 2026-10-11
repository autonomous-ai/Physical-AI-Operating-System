//! Duplex hearing through the production hearing, observation, decision,
//! commit and speaker-dispatch functions, one capture block at a time.
//!
//! Time here is the host's own and the blocks follow one another at once, so
//! every assertion is about order and counts of blocks. The bounds that are
//! measured in time are tested where they live: the decision budget in
//! `admission`, the tail and the staleness of hearing in `turn_taking`.
use super::*;
use crate::{activity::START_BLOCKS, admission::Hold, turn_taking::HEARING_STALE_US};
use lamp_audio::{
    evidence::{EchoEvidence, EchoMatch, Level},
    ownership::PlaybackCursor,
};

/// The render explains the microphone: the lamp's own voice.
const OWN: Option<(u8, u8)> = Some((97, 10));
/// The render does not: someone is speaking over the lamp.
const OVER: Option<(u8, u8)> = Some((40, 0));

fn evidence(render_quiet_blocks: u8, shares: Option<(u8, u8)>) -> EchoEvidence {
    let level = Level::from_tenths_of_db(-200);
    EchoEvidence {
        blocks: 1_000,
        render_active_blocks: if render_quiet_blocks == 0 { 12 } else { 0 },
        render_quiet_blocks,
        capture_level: level,
        residual_level: Level::from_tenths_of_db(-400),
        echo: shares.map(|(capture_explained, residual_playback_floor)| EchoMatch {
            render_level: level,
            capture_explained,
            residual_explained: 10,
            residual_playback_floor,
            alignment_age_blocks: 0,
        }),
    }
}

/// What one capture block led to.
#[derive(Debug, Default)]
struct Tick {
    /// Reply samples handed to the speaker after this block was heard.
    dispatched: Option<Vec<i16>>,
    accepted: Option<Admitted>,
}

/// An input the admission accepted and the coordinator committed.
#[derive(Debug)]
struct Admitted {
    basis: AcceptanceBasis,
    trigger_sequence: u64,
    /// Capture blocks that became the start of the request, in order.
    audio: Vec<u64>,
}

struct Harness {
    owner: Controller,
    reply: Option<Reply>,
    mask: PlaybackMask,
    detector: TurnDetector,
    admission: InputAdmission,
    trace: SessionState,
    lineage: CaptureLineage,
    heard_reply: Option<TurnOwner>,
    sequence: u64,
    last_read: u64,
    _directory: SessionDirectory,
    parent: WorkerChannels,
    speaker: WorkerChannels,
    pending: Option<PendingSpeaker>,
    chunk: u64,
    snapshot: Snapshot,
    provider_queue: VecDeque<ProviderInput>,
}

impl Harness {
    /// A reply of `blocks` speaker blocks whose samples count up from zero,
    /// so that what reaches the speaker can be checked for order and loss.
    fn new(policy: TurnTaking, blocks: usize) -> Self {
        let (mut owner, mut reply, _) = active_reply();
        reply.pcm = (0..blocks * 240).map(|n| (n % 30_000) as i16).collect();
        let directory = SessionDirectory::create().unwrap();
        let (parent, speaker) = pair(&directory, "speaker");
        let snapshot = owner.snapshot(now()).unwrap();
        let duplex = policy == TurnTaking::Duplex;
        Self {
            lineage: CaptureLineage {
                worker: new_boot().unwrap(),
                epoch: 1,
                dsp_epoch: 1,
                privacy_generation: snapshot.microphone_generation(),
            },
            owner,
            reply: Some(reply),
            mask: if duplex {
                PlaybackMask::duplex()
            } else {
                PlaybackMask::new(policy)
            },
            detector: TurnDetector::v1_baseline(),
            admission: InputAdmission::with_hold(if duplex {
                Hold::DUPLEX
            } else {
                Hold::STANDARD
            }),
            trace: SessionState::default(),
            heard_reply: None,
            sequence: 0,
            last_read: monotonic_us(),
            _directory: directory,
            parent,
            speaker,
            pending: None,
            chunk: 0,
            snapshot,
            provider_queue: VecDeque::new(),
        }
    }
    fn old(&self) -> TurnOwner {
        self.reply.as_ref().unwrap().owner
    }
    fn events(&self, kind: &str) -> Vec<Value> {
        self.trace
            .iter()
            .filter(|event| event["kind"] == kind)
            .cloned()
            .collect()
    }
    fn at(&self) -> u64 {
        // Strictly after the previous block's read.
        loop {
            let at = monotonic_us();
            if at > self.last_read {
                return at;
            }
            std::hint::spin_loop();
        }
    }
    /// One pass of the session loop for one capture block: hear it, let
    /// admission see it, decide a candidate, commit an accepted one, then
    /// offer the speaker its next chunk and let it accept that chunk.
    fn block(&mut self, score: f32, echo: Option<EchoEvidence>) -> Tick {
        self.sequence += 1;
        let at = self.at();
        let context = AdmissionContext {
            capture: self.lineage,
            // Issued no later than the block it judges.
            authority: self.owner.snapshot(MonoTime::from_micros(at)).unwrap(),
        };
        let heard = hear_capture_block(
            &mut self.mask,
            &self.reply,
            &mut self.heard_reply,
            self.sequence,
            at,
            score,
            echo.as_ref(),
            at,
            &mut self.trace,
        )
        .unwrap();
        let observation = ObservedAudio {
            sequence: self.sequence,
            captured_at_us: at,
            samples: [self.sequence as i16; 160],
        };
        let (update, _) = observe_prospective_input(
            &mut self.mask,
            &mut self.detector,
            &mut self.admission,
            context,
            observation,
            self.last_read,
            heard,
            at,
        )
        .unwrap();
        self.last_read = at;
        if let Some(rejection) = update.rejection {
            record_input_rejection(&mut self.trace, rejection).unwrap();
        }
        if let AdmissionStep::Candidate(candidate) = &update.step {
            record(
                &mut self.trace,
                json!({"kind":"input_candidate","candidate":candidate,"at_us":at}),
            )
            .unwrap();
        }
        let candidate = match update.step {
            AdmissionStep::Candidate(candidate) => Some(candidate),
            AdmissionStep::None => self
                .admission
                .pending_evidence(at, AcceptanceBasis::DirectedSessionVadOnly)
                .map(|(candidate, _)| candidate),
            AdmissionStep::Rejected(rejection) => {
                record_input_rejection(&mut self.trace, rejection).unwrap();
                None
            }
            AdmissionStep::Audio(audio) | AdmissionStep::End(audio) => {
                // The person goes on: more of the request for the provider.
                queue_input(
                    &mut self.provider_queue,
                    self.reply.as_mut().unwrap(),
                    self.snapshot.microphone_generation(),
                    audio,
                    provider_queue_bound(Hold::DUPLEX),
                )
                .unwrap();
                None
            }
        };
        let mut tick = Tick::default();
        if let Some(candidate) = candidate
            && let Some(accepted) = decide_prospective_input(
                &self.mask,
                &mut self.detector,
                &mut self.admission,
                candidate,
                context,
                at,
                &mut self.trace,
            )
            .unwrap()
        {
            tick.accepted = Some(Admitted {
                basis: accepted.basis,
                trigger_sequence: accepted.candidate.trigger_sequence,
                audio: accepted.audio.iter().map(|frame| frame.sequence).collect(),
            });
            self.snapshot = commit_admitted_input(
                &mut self.owner,
                &mut self.reply,
                &mut self.pending,
                &mut self.provider_queue,
                accepted,
                1,
                PlayoutStart::default(),
                &mut self.trace,
                |_| Ok(()),
            )
            .unwrap();
        }
        tick.dispatched = self.offer_speaker();
        tick
    }
    /// The loop's dispatch call, and a speaker that accepts what it is sent.
    fn offer_speaker(&mut self) -> Option<Vec<i16>> {
        dispatch_speaker_chunk(
            &mut self.owner,
            &mut self.reply,
            &mut self.parent.data,
            &mut self.pending,
            &mut self.chunk,
            &mut self.snapshot,
            &mut self.mask,
        )
        .unwrap();
        let chunk = self.speaker.data.receive::<SpeakerChunk>().unwrap()?;
        let pending = self.pending.take().unwrap();
        assert_eq!(pending.sequence, chunk.chunk_sequence);
        let frame = 480 + 240 * (chunk.chunk_sequence - 1);
        let at = monotonic_us();
        self.mask
            .accepted(
                pending.permit.owner(),
                chunk.chunk_sequence,
                PlaybackCursor { epoch: 1, frame },
                PlaybackCursor {
                    epoch: 1,
                    frame: frame + 240,
                },
                240,
                at,
                at,
                at,
            )
            .unwrap();
        Some(chunk.samples)
    }
    /// `count` blocks alike. Returns what each led to.
    fn blocks(&mut self, count: usize, score: f32, echo: Option<EchoEvidence>) -> Vec<Tick> {
        (0..count).map(|_| self.block(score, echo)).collect()
    }
}

fn dispatched(ticks: &[Tick]) -> usize {
    ticks
        .iter()
        .filter(|tick| tick.dispatched.is_some())
        .count()
}

#[test]
fn the_lamps_own_voice_through_a_whole_reply_neither_yields_nor_becomes_a_turn() {
    let mut h = Harness::new(TurnTaking::Duplex, 400);
    let old = h.old();
    // The canceller lets the lamp through and the detector is sure of speech.
    let ticks = h.blocks(300, 0.99, Some(evidence(0, OWN)));
    assert_eq!(dispatched(&ticks), 300, "the speaker was kept waiting");
    assert!(ticks.iter().all(|tick| tick.accepted.is_none()));
    assert_eq!(h.old(), old);
    assert!(h.events("input_candidate").is_empty());
    assert!(h.events("duplex_yield").is_empty());
    assert_eq!(h.mask.duplex_summary(), Some(((0, 0, 0), false)));
    assert_eq!(h.mask.suppressed_blocks(), 0, "heard, not masked");
}

#[test]
fn a_person_over_the_lamp_stops_the_speaker_then_interrupts_with_their_opening_words() {
    let mut h = Harness::new(TurnTaking::Duplex, 400);
    let old = h.old();
    assert_eq!(dispatched(&h.blocks(20, 0.99, Some(evidence(0, OWN)))), 20);
    // Blocks 21 to 26: someone speaks over the lamp.
    let onset = h.blocks(6, 0.9, Some(evidence(0, OVER)));
    assert_eq!(dispatched(&onset[..5]), 5, "one block is not a person");
    assert!(
        onset[5].dispatched.is_none(),
        "the block that established the person still fed the speaker"
    );
    let yielded = h.events("duplex_yield");
    assert_eq!(yielded.len(), 1);
    assert_eq!(yielded[0]["frame_sequence"], 26);
    assert_eq!(yielded[0]["owner"], serde_json::to_value(old).unwrap());
    assert_eq!(h.events("input_candidate").len(), 1);
    // What the speaker already has plays out; the lamp's sound dies away
    // while the person goes on. Nothing more is handed over, nothing is
    // cancelled yet, and the reply keeps every sample it had.
    let queued = h.reply.as_ref().unwrap().pcm.len();
    let mut held = h.blocks(3, 0.9, Some(evidence(0, OVER)));
    for quiet in 1..=29 {
        held.push(h.block(0.9, Some(evidence(quiet, OVER))));
    }
    assert_eq!(dispatched(&held), 0);
    assert!(held.iter().all(|tick| tick.accepted.is_none()));
    assert_eq!(h.old(), old);
    assert_eq!(h.reply.as_ref().unwrap().pcm.len(), queued);
    assert!(h.events("turn_finished").is_empty());
    // The lamp is silent and the person is still speaking: an interruption.
    let clear = h.blocks(6, 0.9, Some(evidence(30, None)));
    assert!(clear[..5].iter().all(|tick| tick.accepted.is_none()));
    let accepted = clear[5]
        .accepted
        .as_ref()
        .expect("the person was not let in");
    assert_eq!(accepted.basis, AcceptanceBasis::DirectedSessionDuplex);
    let committed = h.events("duplex_commit");
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0]["frame_sequence"], 64);
    // The opening words, spoken over the lamp, are the start of the request:
    // one unbroken run from before the first block of the person to now.
    let heard = &accepted.audio;
    assert!(heard[0] <= 21 && *heard.last().unwrap() == 64);
    assert!(heard.windows(2).all(|pair| pair[1] == pair[0] + 1));
    assert_eq!(
        accepted.trigger_sequence, 26,
        "the same candidate, not a later one"
    );
    // The old reply went through the ordinary cancellation and a new request
    // began with that audio.
    let finished = h.events("turn_finished");
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0]["outcome"], "user_interrupted");
    assert_eq!(finished[0]["owner"], serde_json::to_value(old).unwrap());
    assert_ne!(h.old(), old);
    assert!(matches!(
        h.provider_queue.front(),
        Some(ProviderInput::Start { .. })
    ));
    // Nothing of the old reply can reach the speaker now.
    assert!(h.offer_speaker().is_none());
    assert_eq!(h.mask.duplex_summary(), Some(((1, 1, 0), false)));
    // The request is the start and every retained block. That is more than
    // the standing queue bound allows, which is why duplex widens it, and the
    // person's next words still fit behind it.
    assert_eq!(h.provider_queue.len(), 1 + heard.len());
    assert!(h.provider_queue.len() > PROVIDER_QUEUE_BOUND);
    let more = h.blocks(8, 0.9, Some(evidence(40, None)));
    assert!(more.iter().all(|tick| tick.accepted.is_none()));
    assert_eq!(h.provider_queue.len(), 1 + heard.len() + 8);
    assert!(h.provider_queue.len() < provider_queue_bound(Hold::DUPLEX));
    assert_eq!(provider_queue_bound(Hold::STANDARD), PROVIDER_QUEUE_BOUND);
    assert_eq!(h.events("turn_finished").len(), 1, "one interruption");
}

#[test]
fn a_brief_acknowledgment_costs_a_pause_and_none_of_the_reply() {
    let mut h = Harness::new(TurnTaking::Duplex, 400);
    let old = h.old();
    let mut ticks = h.blocks(20, 0.99, Some(evidence(0, OWN)));
    // "Mm-hm", 80 ms over the lamp.
    ticks.extend(h.blocks(8, 0.9, Some(evidence(0, OVER))));
    assert_eq!(h.events("duplex_yield")[0]["frame_sequence"], 26);
    // Then nobody speaks while the lamp's sound dies away, and after it.
    for quiet in 1..=29 {
        ticks.push(h.block(0.1, Some(evidence(quiet, None))));
    }
    let still = h.blocks(15, 0.1, Some(evidence(40, None)));
    assert!(still[..14].iter().all(|tick| tick.dispatched.is_none()));
    assert!(
        still[14].dispatched.is_some(),
        "the reply did not go on in the block that settled it"
    );
    let resumed = h.events("duplex_resume");
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0]["frame_sequence"], 72);
    ticks.extend(still);
    ticks.extend(h.blocks(40, 0.99, Some(evidence(0, OWN))));
    // The pause, in blocks during which the speaker was offered nothing.
    assert_eq!(ticks.len() - dispatched(&ticks), 46);
    // Same reply, same owner, nothing admitted, nothing cancelled.
    assert_eq!(h.old(), old);
    assert!(ticks.iter().all(|tick| tick.accepted.is_none()));
    assert!(h.events("turn_finished").is_empty());
    assert!(!h.admission.owns_stream());
    assert!(
        h.admission
            .pending_evidence(monotonic_us(), AcceptanceBasis::DirectedSessionDuplex)
            .is_none(),
        "the acknowledgment stayed a candidate"
    );
    // And the speaker got every sample of the reply, in order, none twice.
    let played: Vec<i16> = ticks
        .iter()
        .filter_map(|tick| tick.dispatched.clone())
        .flatten()
        .collect();
    assert!(
        played
            .iter()
            .enumerate()
            .all(|(index, sample)| usize::try_from(*sample).unwrap() == index % 30_000)
    );
    assert_eq!(h.mask.duplex_summary(), Some(((1, 0, 1), false)));
}

#[test]
fn a_yield_that_is_never_settled_is_given_up_at_its_deadline() {
    let mut h = Harness::new(TurnTaking::Duplex, 400);
    let old = h.old();
    h.blocks(6, 0.9, Some(evidence(0, OVER)));
    assert_eq!(h.events("duplex_yield").len(), 1);
    // The evidence goes on saying the lamp is sounding: no silence to confirm
    // a person in, so no commitment can be made.
    let waiting = h.blocks(59, 0.9, Some(evidence(0, OVER)));
    assert_eq!(dispatched(&waiting), 0);
    assert!(h.events("duplex_resume").is_empty());
    let last = h.block(0.9, Some(evidence(0, OVER)));
    assert!(last.dispatched.is_some() && last.accepted.is_none());
    assert_eq!(h.events("duplex_resume")[0]["frame_sequence"], 66);
    assert_eq!(h.old(), old);
    assert!(h.events("turn_finished").is_empty());
    // The held candidate ended as a policy rejection inside its own budget,
    // not as a missed deadline that would drain the microphone.
    assert!(!h.admission.owns_stream());
    let rejected = h.events("input_candidate_rejected");
    assert!(
        rejected
            .iter()
            .all(|event| event["reason"] != "deadline" && event["reason"] != "capacity"),
        "{rejected:?}"
    );
}

#[test]
fn the_standard_hold_cannot_keep_a_candidate_through_a_yield() {
    // Why duplex widens the decision budget: with the standard one the same
    // person is turned away for capacity before the lamp has fallen silent.
    let mut h = Harness::new(TurnTaking::Duplex, 400);
    h.admission = InputAdmission::default();
    let old = h.old();
    h.blocks(20, 0.99, Some(evidence(0, OWN)));
    h.blocks(9, 0.9, Some(evidence(0, OVER)));
    let mut ticks = Vec::new();
    for quiet in 1..=29 {
        ticks.push(h.block(0.9, Some(evidence(quiet, OVER))));
    }
    ticks.extend(h.blocks(6, 0.9, Some(evidence(30, None))));
    assert!(ticks.iter().all(|tick| tick.accepted.is_none()));
    assert_eq!(h.old(), old);
    assert!(
        h.events("input_candidate_rejected")
            .iter()
            .any(|event| event["reason"] == "capacity")
    );
}

#[test]
fn a_reply_that_twice_yields_to_nothing_masks_like_the_baseline_until_it_ends() {
    let mut h = Harness::new(TurnTaking::Duplex, 600);
    let old = h.old();
    for _ in 0..2 {
        h.blocks(8, 0.9, Some(evidence(0, OVER)));
        for quiet in 1..=29 {
            h.block(0.1, Some(evidence(quiet, None)));
        }
        h.blocks(15, 0.1, Some(evidence(40, None)));
        h.blocks(10, 0.99, Some(evidence(0, OWN)));
    }
    assert_eq!(h.mask.duplex_summary(), Some(((2, 0, 2), true)));
    // The same evidence a third time: the reply plays on and no one is let in.
    let candidates = h.events("input_candidate").len();
    let ticks = h.blocks(40, 0.9, Some(evidence(0, OVER)));
    assert_eq!(dispatched(&ticks), 40);
    assert!(ticks.iter().all(|tick| tick.accepted.is_none()));
    assert_eq!(h.events("input_candidate").len(), candidates);
    assert_eq!(h.events("duplex_yield").len(), 2);
    assert_eq!(h.old(), old);
}

#[test]
fn without_echo_evidence_duplex_decides_as_the_conservative_baseline_does() {
    // The same blocks to both policies, from a worker that reports nothing.
    let mut outcomes = Vec::new();
    for policy in [TurnTaking::Duplex, TurnTaking::V1Baseline] {
        let mut h = Harness::new(policy, 400);
        let old = h.old();
        let mut ticks = h.blocks(20, 0.2, None);
        ticks.extend(h.blocks(40, 0.99, None));
        assert!(ticks.iter().all(|tick| tick.accepted.is_none()));
        assert_eq!(dispatched(&ticks), 60);
        assert_eq!(h.old(), old);
        assert!(h.events("duplex_yield").is_empty());
        outcomes.push((
            h.mask.suppressed_blocks(),
            h.events("input_candidate").len(),
            h.events("input_candidate_rejected").len(),
        ));
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert!(outcomes[0].0 > 0, "nothing was masked: {outcomes:?}");
}

#[test]
fn a_microphone_that_stops_arriving_does_not_hold_the_reply_back() {
    let mut h = Harness::new(TurnTaking::Duplex, 400);
    h.blocks(6, 0.9, Some(evidence(0, OVER)));
    assert!(h.offer_speaker().is_none(), "yielded");
    // Muted or stalled: no further block is heard.
    std::thread::sleep(Duration::from_micros(HEARING_STALE_US + 20_000));
    assert!(h.offer_speaker().is_some());
}

#[test]
fn a_person_who_speaks_once_the_lamp_is_silent_is_an_ordinary_turn() {
    let mut h = Harness::new(TurnTaking::Duplex, 20);
    // The reply plays out and its sound dies away.
    h.blocks(20, 0.99, Some(evidence(0, OWN)));
    for quiet in 1..=29 {
        h.block(0.1, Some(evidence(quiet, None)));
    }
    h.blocks(10, 0.1, Some(evidence(40, None)));
    // Then speech in the clear: six blocks and it is admitted, as ever.
    let ticks = h.blocks(usize::from(START_BLOCKS), 0.95, Some(evidence(50, None)));
    assert!(ticks[..5].iter().all(|tick| tick.accepted.is_none()));
    let accepted = ticks[5].accepted.as_ref().unwrap();
    assert_eq!(accepted.basis, AcceptanceBasis::DirectedSessionDuplex);
    assert!(h.events("duplex_yield").is_empty());
}
