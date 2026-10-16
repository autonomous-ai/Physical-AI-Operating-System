//! Deterministic tests of the production observation/decision/commit functions.
use super::*;
use crate::{activity::START_BLOCKS, turn_taking::TAIL_US};
use lamp_audio::ownership::PlaybackCursor;

struct Harness {
    owner: Controller,
    reply: Option<Reply>,
    mask: PlaybackMask,
    detector: TurnDetector,
    admission: InputAdmission,
    trace: SessionState,
    lineage: CaptureLineage,
    base: u64,
    sequence: u64,
    /// Why each sound let go while an answer waited was let go (#1123).
    let_go: Vec<&'static str>,
    /// The microphone's and the admission's leases renewed with each block.
    renew: bool,
    /// Each block heard by the mask with echo evidence, nothing playing, as
    /// the loop hears it (`hear_capture_block`), and whose reply it heard.
    echo: bool,
    /// The blocks come without echo evidence, as before the evidence starts.
    echo_missing: bool,
    /// The speaker plays the lamp's sound, not yet matched by the echo
    /// evidence: the mask cannot hear a person over it yet.
    render_active: bool,
    heard_reply: Option<TurnOwner>,
    /// The wait's state as the loop keeps it, and the candidate that last
    /// waited (#1123).
    wait: AnswerWait,
    kept_for: Option<crate::admission::CandidateId>,
    /// The candidate decided at once when the answer began to sound.
    sounded_for: Option<crate::admission::CandidateId>,
    /// The last block heard as speech, as the loop keeps it.
    last_speech_us: u64,
}
impl Harness {
    /// The product's turn-taking and mask with `LAMP_WAIT_KEEPS_ANSWER=on`:
    /// the duplex mask hearing every block with echo evidence, with the
    /// memory the loop gives it for the wait (`keep_waited_sound`), and the
    /// duplex hold. The answer to the question that ended at the base time
    /// has not arrived: no audio of it yet. Its leases renewed with each
    /// block, as the loop renews them, and its clock `lag` back: a commit
    /// reads the real clock, which must come after the last block and
    /// within its 0.9 s lease.
    fn duplex(lag: u64) -> Self {
        Self::product(lag, true)
    }
    /// [`Self::duplex`], with the mask's usual memory or the wait's.
    fn product(lag: u64, waited_memory: bool) -> Self {
        let mut h = Self::with_lag(TurnTaking::Duplex, lag);
        h.admission = InputAdmission::with_hold(crate::admission::Hold::DUPLEX);
        h.renew = true;
        h.mask = PlaybackMask::duplex();
        if waited_memory {
            h.mask.keep_waited_sound();
        }
        h.echo = true;
        h.reply.as_mut().unwrap().pcm.clear();
        h.wait = AnswerWait {
            on: true,
            asked_until_us: Some(h.base),
            ..AnswerWait::default()
        };
        h
    }
    fn new(policy: TurnTaking) -> Self {
        Self::with_lag(policy, 500_000)
    }
    fn with_lag(policy: TurnTaking, lag: u64) -> Self {
        let base = monotonic_us() - lag;
        let time = MonoTime::from_micros(base);
        let mut owner = Controller::new(new_boot().unwrap(), time);
        owner
            .set_microphone_permission(time, Permission::Allowed)
            .unwrap();
        let until = MonoTime::from_micros(base + 900_000);
        owner
            .set_capture(time, CaptureState::RetainingUntil(until))
            .unwrap();
        owner
            .set_admission(time, AdmissionState::OpenUntil(until))
            .unwrap();
        let turn = owner.admit(time, AdmittedInput::NewTurn).unwrap();
        let plan = owner
            .plan_output(time, turn, OutputKind::Speech, None)
            .unwrap();
        owner.input_ended(time, turn).unwrap();
        let lineage = CaptureLineage {
            worker: new_boot().unwrap(),
            epoch: 1,
            dsp_epoch: 1,
            privacy_generation: owner.snapshot(time).unwrap().microphone_generation(),
        };
        let reply = Reply {
            owner: turn,
            cloud: submitted_request(),
            local_input_ended: true,
            plan,
            playback: None,
            pcm: vec![7; 480].into(),
            generation_done: false,
            provider_idle: false,
            input_sequence: 1,
            had_audio: false,
            voice_level: 0,
            told_stop: None,
            final_chunk: None,
            playback_gaps: 0,
            notice: Default::default(),
            local_only: None,
            reminder: None,
            say: None,
            last_provider_audio_at_us: None,
            start: lamp_audio::playout::StartGate::new(0),
            need: lamp_audio::playout::StartNeed::default(),
        };
        Self {
            owner,
            reply: Some(reply),
            mask: PlaybackMask::new(policy),
            detector: TurnDetector::default(),
            admission: InputAdmission::default(),
            trace: SessionState::default(),
            lineage,
            base,
            sequence: 0,
            let_go: Vec::new(),
            renew: false,
            echo: false,
            echo_missing: false,
            render_active: false,
            heard_reply: None,
            wait: AnswerWait::default(),
            kept_for: None,
            sounded_for: None,
            last_speech_us: 0,
        }
    }
    fn old(&self) -> TurnOwner {
        self.reply.as_ref().unwrap().owner
    }
    fn dispatch(&mut self, offset: u64) {
        self.mask
            .dispatched(self.old(), 1, self.base + offset, 240)
            .unwrap();
    }
    fn accepted(&mut self, written: u64, received: u64) {
        self.mask
            .accepted(
                self.old(),
                1,
                PlaybackCursor {
                    epoch: 1,
                    frame: 480,
                },
                PlaybackCursor {
                    epoch: 1,
                    frame: 720,
                },
                240,
                self.base + written,
                self.base + written,
                self.base + received,
            )
            .unwrap();
    }
    fn drain(&mut self, offset: u64) {
        self.mask
            .drained(
                self.old(),
                1,
                PlaybackCursor {
                    epoch: 1,
                    frame: 720,
                },
                self.base + offset,
                self.base + offset,
            )
            .unwrap();
    }
    fn feed(&mut self, offset: u64, score: f32) -> Result<Option<Accepted>> {
        self.feed_received(offset, offset, score)
    }
    fn feed_received(
        &mut self,
        offset: u64,
        received: u64,
        score: f32,
    ) -> Result<Option<Accepted>> {
        self.step(offset, received, score, false)
    }
    /// [`Self::feed`], with #1123's wait applied as the loop applies it.
    fn feed_waiting(&mut self, offset: u64, score: f32) -> Result<Option<Accepted>> {
        self.step(offset, offset, score, true)
    }
    fn step(
        &mut self,
        offset: u64,
        received: u64,
        score: f32,
        wait: bool,
    ) -> Result<Option<Accepted>> {
        self.sequence += 1;
        let at_us = self.base + received;
        if self.renew {
            let time = MonoTime::from_micros(at_us);
            let until = MonoTime::from_micros(at_us + 900_000);
            self.owner
                .set_capture(time, CaptureState::RetainingUntil(until))?;
            self.owner
                .set_admission(time, AdmissionState::OpenUntil(until))?;
        }
        let read_us = self.base + offset;
        if score >= 0.5 {
            self.last_speech_us = read_us;
        }
        let context = AdmissionContext {
            capture: self.lineage,
            authority: self.owner.snapshot(MonoTime::from_micros(at_us))?,
        };
        let observation = ObservedAudio {
            sequence: self.sequence,
            captured_at_us: read_us,
            samples: [self.sequence as i16; 160],
        };
        let quiet = super::duplex::evidence(if self.render_active { 0 } else { 200 }, None);
        let evidence = (!self.echo_missing).then_some(&quiet);
        let heard = if self.echo {
            let (heard, yielded) = hear_capture_block(
                &mut self.mask,
                &self.reply,
                &mut self.heard_reply,
                self.sequence,
                read_us,
                score,
                evidence,
                at_us,
                &mut self.trace,
            )?;
            if yielded {
                self.detector.reach_back();
            }
            heard
        } else {
            score
        };
        let (update, _) = observe_prospective_heard(
            &mut self.mask,
            &mut self.detector,
            &mut self.admission,
            context,
            observation,
            read_us - 10_000,
            heard,
            evidence.is_some().then_some(score),
            at_us,
        )?;
        if let Some(r) = update.rejection {
            record_input_rejection(&mut self.trace, r)?;
        }
        let candidate = match update.step {
            AdmissionStep::Candidate(c) => Some(c),
            AdmissionStep::None => self
                .admission
                .pending_evidence(at_us, AcceptanceBasis::DirectedSessionVadOnly)
                .map(|(c, _)| c),
            AdmissionStep::Rejected(r) => {
                record_input_rejection(&mut self.trace, r)?;
                None
            }
            _ => None,
        };
        if wait && let Some(c) = candidate.as_ref() {
            let state = AnswerWait {
                waited: self.kept_for == Some(c.id),
                sounding: reply_sounding(&self.mask, &self.reply, at_us),
                decided: self.sounded_for == Some(c.id),
                heard: !self.mask.is_duplex() || (self.echo && !self.echo_missing),
                ..self.wait
            };
            match while_answer_waits(
                state,
                self.reply.as_ref(),
                c,
                self.admission.pending_ended(),
                self.detector.pause_blocks(),
                at_us,
            ) {
                WhileAnswerWaits::Decide => {
                    if state.waited && state.sounding {
                        self.sounded_for = Some(c.id);
                    }
                }
                WhileAnswerWaits::Wait => {
                    self.kept_for = Some(c.id);
                    return Ok(None);
                }
                WhileAnswerWaits::LetGo(why) => {
                    if let Some(r) = self.admission.release_prospective(at_us)? {
                        record_input_rejection(&mut self.trace, r)?;
                    }
                    self.wait.let_go_ended_us = Some(self.last_speech_us);
                    self.detector.clear_prospective();
                    self.let_go.push(why);
                    return Ok(None);
                }
            }
        }
        match candidate {
            Some(c) => decide_prospective_input(
                &self.mask,
                &mut self.detector,
                &mut self.admission,
                c,
                context,
                at_us,
                &mut self.trace,
            ),
            None => Ok(None),
        }
    }
    fn commit(&mut self, accepted: Accepted) -> VecDeque<ProviderInput> {
        self.commit_with(accepted, false)
    }
    fn commit_with(&mut self, accepted: Accepted, cut_in: bool) -> VecDeque<ProviderInput> {
        self.commit_told(accepted, cut_in, false)
    }
    fn commit_told(
        &mut self,
        accepted: Accepted,
        cut_in: bool,
        after_stop: bool,
    ) -> VecDeque<ProviderInput> {
        let mut queue = VecDeque::new();
        commit_admitted_input(
            &mut self.owner,
            &mut self.reply,
            &mut None,
            &mut queue,
            accepted,
            1,
            false,
            cut_in,
            after_stop.then_some((1, 1)),
            PlayoutStart::default(),
            &mut self.trace,
            |_| Ok(()),
        )
        .unwrap();
        queue
    }
}

#[test]
fn v1_baseline_full_reply_echo_never_replaces_owner_or_queues_start() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    let old = h.old();
    h.dispatch(10_000);
    h.accepted(11_000, 11_000);
    for n in 1..=80 {
        let accepted = h.feed(20_000 + n * 10_000, 0.99).unwrap();
        assert!(
            accepted.is_none(),
            "own reply became a destructive candidate"
        );
    }
    assert_eq!(h.old(), old);
    assert!(h.reply.as_ref().unwrap().pcm.len() == 480);
    assert!(!h.trace.iter().any(|e| e["kind"] == "turn_finished"));
}

#[test]
fn v1_baseline_first_write_race_waits_then_discards_original_overlapping_input() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    h.dispatch(10_000); // First control drain saw no receipt; write is independently in flight.
    for n in 1..=u64::from(START_BLOCKS) {
        assert!(
            h.feed(10_000 + n * 10_000, 0.99).unwrap().is_none(),
            "unsettled write was assumed silent"
        );
    }
    assert!(
        h.admission
            .pending_evidence(h.base + 70_000, AcceptanceBasis::DirectedSessionVadOnly)
            .is_some()
    );
    h.accepted(11_000, 71_000); // Actual source time, not arrival, covers retained onset.
    assert!(h.feed(80_000, 0.99).unwrap().is_none());
    assert!(
        h.admission
            .pending_evidence(h.base + 80_000, AcceptanceBasis::DirectedSessionVadOnly)
            .is_none()
    );
    assert!(!h.admission.owns_stream());
}

#[test]
fn v1_baseline_after_reply_starts_fresh_without_suppressed_prefix() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    h.dispatch(10_000);
    h.accepted(11_000, 11_000);
    h.drain(20_000);
    let until = 20_000 + TAIL_US;
    let mut accepted = None;
    for n in 1..=30 {
        let offset = 20_000 + n * 10_000;
        if let Some(a) = h.feed(offset, 0.99).unwrap() {
            accepted = Some(a);
            break;
        }
    }
    let accepted = accepted.expect("fresh post-tail onset was lost");
    assert!(
        accepted
            .audio
            .iter()
            .all(|f| f.captured_at_us - h.base > until)
    );
    assert_eq!(accepted.audio.len(), usize::from(START_BLOCKS));
    let queue = h.commit(accepted);
    assert_eq!(
        queue
            .iter()
            .filter(|e| matches!(e, ProviderInput::Start { .. }))
            .count(),
        1
    );
    assert_eq!(
        h.trace
            .iter()
            .filter(|e| e["outcome"] == "user_interrupted")
            .count(),
        1
    );
}

/// Feeds `blocks` of 10 ms at `score` from `*t`, and keeps the first input
/// admitted.
fn sound(
    h: &mut Harness,
    t: &mut u64,
    blocks: u32,
    score: f32,
    wait: bool,
    accepted: &mut Option<Accepted>,
) {
    for _ in 0..blocks {
        *t += 10_000;
        let admitted = if wait {
            h.feed_waiting(*t, score)
        } else {
            h.feed(*t, score)
        }
        .unwrap();
        if accepted.is_none() {
            *accepted = admitted;
        }
    }
}

/// Quiet before a sound: more than the 30 blocks of pre-roll, so that a
/// candidate holds its whole pre-roll, as on the lamp.
const QUIET_FIRST: u32 = 50;

/// What a fresh product harness (`LAMP_WAIT_KEEPS_ANSWER=on`) does with
/// `pieces` of 10 ms blocks at a score after [`QUIET_FIRST`], and 1.5 s of
/// quiet after: the input taken, if any, why each sound was let go, and the
/// harness's base time.
fn waited(pieces: &[(u32, f32)]) -> (Option<Accepted>, Vec<&'static str>, u64) {
    let mut h = Harness::duplex(4_000_000);
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    for &(blocks, score) in pieces {
        sound(&mut h, &mut t, blocks, score, true, &mut accepted);
    }
    sound(&mut h, &mut t, 150, 0.0, true, &mut accepted);
    (accepted, h.let_go, h.base)
}

/// #1123, `LAMP_WAIT_KEEPS_ANSWER=on`, on the product's mask and hold:
/// while an answer to someone is awaited and has not sounded, a short sound
/// with no words (0.4 s) does not take the turn. It waits, ends and is let
/// go, and the answer goes on; a second one 1.5 s later the same, with no
/// error. Without the wait, or with the switch off on the same path, the
/// same sound takes the turn at once (the reviewer on #1130).
#[test]
fn while_an_answer_waits_a_short_sound_is_let_go_and_the_answer_goes_on() {
    let mut h = Harness::duplex(4_000_000);
    let old = h.old();
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    for _ in 0..2 {
        sound(&mut h, &mut t, 40, 0.99, true, &mut accepted);
        sound(&mut h, &mut t, 150, 0.0, true, &mut accepted);
    }
    assert!(accepted.is_none(), "a short sound took the turn");
    assert_eq!(h.old(), old);
    assert_eq!(h.let_go.len(), 2, "{:?}", h.let_go);
    assert!(h.let_go.iter().all(|why| ["quiet", "ended"].contains(why)));
    for off in [false, true] {
        let mut h = Harness::duplex(4_000_000);
        h.wait.on = !off;
        let (mut t, mut accepted) = (0, None);
        sound(&mut h, &mut t, QUIET_FIRST, 0.0, off, &mut accepted);
        sound(&mut h, &mut t, 40, 0.99, off, &mut accepted);
        assert!(accepted.is_some(), "taken at once, switch off: {off}");
        assert!(h.let_go.is_empty());
    }
}

/// #1123: only `on` turns the wait on.
#[test]
fn the_wait_is_on_only_with_on() {
    for value in [None, Some(""), Some("off"), Some("ON"), Some("1")] {
        assert!(!wait_keeps_answer(value), "{value:?}");
    }
    assert!(wait_keeps_answer(Some("on")));
}

/// #1123: a sound that goes on (1.4 s) takes the turn once it has gone on
/// for 1 s after its trigger, with its whole pre-roll, on the product's
/// mask, and the waiting answer is replaced as before. A follow-up with a
/// short pause in it (0.5 s, 0.2 s of quiet, 0.8 s) is taken too.
#[test]
fn while_an_answer_waits_a_sound_that_goes_on_takes_the_turn_from_its_start() {
    let mut h = Harness::duplex(2_400_000);
    let old = h.old();
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    let began = h.base + t;
    sound(&mut h, &mut t, 140, 0.99, true, &mut accepted);
    let accepted = accepted.expect("a sound that goes on takes the turn");
    let first = accepted.audio.first().unwrap().captured_at_us;
    assert!(
        first <= began,
        "its start is kept: {} ms before it",
        began.saturating_sub(first) / 1_000
    );
    let last = accepted.audio.last().unwrap().captured_at_us;
    assert!(
        last >= accepted.candidate.trigger_read_at_us + 1_000_000,
        "taken {} ms after its trigger",
        (last - accepted.candidate.trigger_read_at_us) / 1_000
    );
    assert!(h.let_go.is_empty());
    let queue = h.commit(accepted);
    assert_ne!(h.old(), old);
    assert!(matches!(queue.front(), Some(ProviderInput::Start { .. })));
    let (accepted, let_go, _) = waited(&[(50, 0.99), (20, 0.0), (80, 0.99)]);
    assert!(accepted.is_some(), "a follow-up with a pause was let go");
    assert!(let_go.is_empty(), "{let_go:?}");
}

/// The reviewer and the first builder on #1130: at its decision a waited
/// sound holds its pre-roll and 1 s more, about 130 blocks. With the mask's
/// usual memory (128) its oldest blocks are gone and it is rejected as
/// `retained_history_unavailable`, losing what was said; with the memory
/// the loop gives the mask for the wait (`keep_waited_sound`) it is taken.
#[test]
fn a_waited_sound_needs_the_memory_the_wait_gives_the_mask() {
    for memory in [false, true] {
        let mut h = Harness::product(4_000_000, memory);
        let (mut t, mut accepted) = (0, None);
        sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
        sound(&mut h, &mut t, 140, 0.99, true, &mut accepted);
        let evicted = h
            .trace
            .iter()
            .any(|e| e["reason"] == "retained_history_unavailable");
        assert_eq!(accepted.is_some(), memory, "memory {memory}");
        assert_eq!(evicted, !memory, "memory {memory}");
    }
}

/// The reviewer on #1130, measured on the product's mask: what the wait
/// lets go and what it takes, by the sound's length. A sound of 0.75 s or
/// less is let go (it has ended, or gone quiet, by its decision); 0.8 s or
/// more is taken. Of the sixth's 0.6 to 1.4 s sounds on lamp-52e6, the
/// longer ones still take the turn.
#[test]
fn what_the_wait_lets_go_and_takes_by_length() {
    for blocks in [20, 40, 50, 60, 70, 75, 80, 90, 100, 120, 140] {
        let (accepted, let_go, _) = waited(&[(blocks, 0.99)]);
        assert_eq!(
            accepted.is_none(),
            blocks <= 75,
            "{} ms: let go {let_go:?}",
            blocks * 10
        );
        assert_eq!(
            let_go.len(),
            usize::from(blocks <= 75),
            "{} ms",
            blocks * 10
        );
    }
}

/// The reviewer on #1130: a follow-up with a pause across its 1 s mark
/// (0.7 s, 0.4 s of quiet, 0.6 s; or 0.6 s, 0.5 s, 0.5 s) has its first
/// piece let go. The next piece, begun within 1 s of that, is taken at once
/// (`AFTER_LET_GO_US`): only the first piece is lost, not the whole.
#[test]
fn a_follow_up_with_a_pause_loses_only_its_first_piece() {
    for pieces in [
        [(70, 0.99), (40, 0.0), (60, 0.99)],
        [(60, 0.99), (50, 0.0), (50, 0.99)],
    ] {
        let (accepted, let_go, base) = waited(&pieces);
        let accepted = accepted.expect("the second piece is taken");
        assert_eq!(let_go, ["quiet"], "{pieces:?}");
        // What was taken begins after the first piece: that piece is lost.
        let first_piece_ended = base + u64::from(QUIET_FIRST + pieces[0].0) * 10_000;
        let started = accepted.audio.first().unwrap().captured_at_us;
        assert!(
            started > first_piece_ended,
            "{pieces:?}: begins {} ms in",
            (started - base) / 1_000
        );
    }
}

/// The answer's audio arrives and plays: the mask hears the lamp sounding,
/// and the echo evidence has not matched it yet, so a person over it is
/// scored 0.0 (`Heard::Unknown`).
fn answer_sounds(h: &mut Harness) {
    let reply = h.reply.as_mut().unwrap();
    reply.had_audio = true;
    reply.pcm = vec![9; 480].into();
    h.render_active = true;
}

/// The first builder's re-check on #1130: once the answer sounds, the mask
/// scores a person's speech 0.0 until it has matched the lamp's sound, so a
/// waited sound's pause from then on is the mask's silence. A sound still
/// going when the answer begins to sound (1.4 s, the answer at 0.5 s) is
/// decided at once and taken, not let go as quiet; one that had stopped
/// 0.4 s before (a 0.4 s cough) is let go and the answer plays. The
/// answer's audio arriving with nothing yet to sound keeps the wait (the
/// first builder on #1130). A sound that begins after the answer has
/// sounded is decided as before.
#[test]
fn once_the_answer_sounds_a_waited_sound_is_judged_by_its_pause_then() {
    let mut h = Harness::duplex(4_000_000);
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    sound(&mut h, &mut t, 50, 0.99, true, &mut accepted);
    answer_sounds(&mut h);
    sound(&mut h, &mut t, 90, 0.99, true, &mut accepted);
    sound(&mut h, &mut t, 100, 0.0, true, &mut accepted);
    assert!(
        accepted.is_some(),
        "a sound running into the answer was lost: {:?}",
        h.let_go
    );
    assert!(h.let_go.is_empty(), "{:?}", h.let_go);

    let mut h = Harness::duplex(4_000_000);
    let old = h.old();
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    sound(&mut h, &mut t, 40, 0.99, true, &mut accepted);
    sound(&mut h, &mut t, 40, 0.0, true, &mut accepted);
    answer_sounds(&mut h);
    sound(&mut h, &mut t, 100, 0.0, true, &mut accepted);
    assert!(accepted.is_none(), "a cough over before the answer cut it");
    assert_eq!(h.old(), old);
    assert_eq!(h.let_go.len(), 1, "{:?}", h.let_go);

    let mut h = Harness::duplex(4_000_000);
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    sound(&mut h, &mut t, 30, 0.99, true, &mut accepted);
    h.reply.as_mut().unwrap().had_audio = true;
    sound(&mut h, &mut t, 20, 0.99, true, &mut accepted);
    sound(&mut h, &mut t, 150, 0.0, true, &mut accepted);
    assert!(
        accepted.is_none(),
        "the cough cut the answer once its audio came"
    );
    assert_eq!(h.let_go.len(), 1, "{:?}", h.let_go);

    let mut h = Harness::duplex(4_000_000);
    h.reply.as_mut().unwrap().had_audio = true;
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    sound(&mut h, &mut t, 10, 0.99, true, &mut accepted);
    assert!(
        accepted.is_some(),
        "an answer already sounding is cut as before"
    );
}

/// The first builder's re-check on #1130: a block with no echo evidence is
/// kept only among the mask's last reads (80, and 0.6 s), so a sound heard
/// without it does not wait: it is decided as before, and never lost.
#[test]
fn a_sound_heard_without_echo_evidence_does_not_wait() {
    let mut h = Harness::duplex(4_000_000);
    h.echo_missing = true;
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    sound(&mut h, &mut t, 140, 0.99, true, &mut accepted);
    assert!(accepted.is_some(), "a sound without evidence was lost");
    assert!(
        h.let_go.is_empty() && h.kept_for.is_none(),
        "{:?}",
        h.let_go
    );
}

/// The reviewer on #1130: a sound in which the lamp's name was heard, from
/// its start on, takes the turn ("Lamp, … stop" with its pause at the 1 s
/// mark); the name heard in the question, before it, does not.
#[test]
fn a_sound_with_the_lamps_name_in_it_takes_the_turn() {
    for (began, taken) in [(500_000, true), (0, false)] {
        let mut h = Harness::duplex(4_000_000);
        h.wait.name_began_us = Some(h.base + began);
        let (mut t, mut accepted) = (0, None);
        sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
        sound(&mut h, &mut t, 70, 0.99, true, &mut accepted);
        sound(&mut h, &mut t, 150, 0.0, true, &mut accepted);
        assert_eq!(accepted.is_some(), taken, "name at {began}");
    }
}

/// The first builder and the reviewer on #1130: nothing waits for the
/// lamp's own turn (a hello, a look, a reminder: no input), for an answer
/// awaited more than 3 s, or on the standard hold (200 ms), where the wait
/// ends 100 ms before the deadline: a short sound is taken as before.
#[test]
fn the_wait_is_only_for_an_answer_to_someone_awaited_a_short_while() {
    let mut h = Harness::duplex(4_000_000);
    h.reply.as_mut().unwrap().input_sequence = 0;
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    sound(&mut h, &mut t, 40, 0.99, true, &mut accepted);
    assert!(accepted.is_some(), "the lamp's own turn waited");
    let mut h = Harness::duplex(4_000_000);
    h.wait.asked_until_us = Some(h.base - 2_800_000);
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, QUIET_FIRST, 0.0, true, &mut accepted);
    sound(&mut h, &mut t, 40, 0.99, true, &mut accepted);
    assert!(accepted.is_some(), "an answer awaited 3.3 s waited");
    let mut h = Harness::new(TurnTaking::V1Baseline);
    h.wait = AnswerWait {
        on: true,
        asked_until_us: Some(h.base),
        ..AnswerWait::default()
    };
    let (mut t, mut accepted) = (0, None);
    sound(&mut h, &mut t, 40, 0.99, true, &mut accepted);
    assert!(accepted.is_some(), "the standard hold's sound was lost");
    assert!(h.let_go.is_empty());
}

#[test]
fn v1_baseline_cloud_wait_without_any_write_still_accepts_resume() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    let old = h.old();
    let mut accepted = None;
    for n in 1..=6 {
        accepted = h.feed(n * 10_000, 0.99).unwrap().or(accepted);
    }
    let queue = h.commit(accepted.unwrap());
    assert_ne!(h.old(), old);
    assert!(matches!(queue.front(), Some(ProviderInput::Start { .. })));
}

/// #926, the reviewer: a candidate that had already ended when it was
/// admitted queues its End in the commit itself; a cut-in's says so, an
/// ordinary turn's does not.
#[test]
fn an_input_ended_at_admission_carries_whether_it_cut_in() {
    for cut_in in [true, false] {
        let mut h = Harness::new(TurnTaking::V1Baseline);
        let mut accepted = None;
        for n in 1..=6 {
            accepted = h.feed(n * 10_000, 0.99).unwrap().or(accepted);
        }
        let mut accepted = accepted.unwrap();
        accepted.ended = true;
        let queue = h.commit_with(accepted, cut_in);
        let end = queue
            .iter()
            .find(|input| matches!(input, ProviderInput::End { .. }))
            .expect("an input ended at admission queues its End");
        assert!(
            matches!(end, ProviderInput::End { cut_in: marked, sound: false, .. } if *marked == cut_in),
            "{end:?}"
        );
    }
}

/// #829, the first builder at 16:33: the first request after a hand or
/// the head stopped the lamp says so in its Start, for the model to be
/// told, and in its record; any other says nothing.
#[test]
fn the_first_request_after_a_stop_says_so_in_its_start() {
    for after_stop in [true, false] {
        let mut h = Harness::new(TurnTaking::V1Baseline);
        let mut accepted = None;
        for n in 1..=6 {
            accepted = h.feed(n * 10_000, 0.99).unwrap().or(accepted);
        }
        let queue = h.commit_told(accepted.unwrap(), false, after_stop);
        assert!(
            matches!(queue.front(), Some(ProviderInput::Start { after_stop: told, .. }) if *told == after_stop),
            "{queue:?}"
        );
        let admitted = h
            .trace
            .iter()
            .find(|row| row["kind"] == "input_admitted")
            .unwrap();
        assert_eq!(
            admitted.get("after_stop").is_some(),
            after_stop,
            "{admitted}"
        );
    }
}

#[test]
fn vad_only_keeps_existing_experiment_semantics() {
    let mut h = Harness::new(TurnTaking::VadOnly);
    h.dispatch(10_000);
    h.accepted(11_000, 11_000);
    let mut accepted = None;
    for n in 1..=6 {
        accepted = h.feed(20_000 + n * 10_000, 0.99).unwrap().or(accepted);
    }
    assert!(accepted.is_some());
}

#[test]
fn v1_baseline_delayed_rejection_uses_latest_retained_evidence_without_renewing_deadline() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    h.dispatch(10_000);
    for n in 2..=18 {
        assert!(h.feed(n * 10_000, 0.99).unwrap().is_none());
    }
    let (original, _) = h
        .admission
        .pending_evidence(h.base + 180_000, AcceptanceBasis::DirectedSessionV1Baseline)
        .unwrap();
    assert_eq!(original.trigger_read_at_us, h.base + 70_000);
    h.mask.rejected(1, h.base + 181_000).unwrap();
    let accepted = h
        .feed(190_000, 0.99)
        .unwrap()
        .expect("fresh retained evidence must replace stale trigger evidence");
    assert_eq!(accepted.candidate.id, original.id);
    assert_eq!(
        accepted.candidate.decision_deadline_us,
        original.decision_deadline_us
    );
    assert_eq!(
        accepted.audio.last().unwrap().captured_at_us,
        h.base + 190_000
    );
    assert!(
        accepted.audio.last().unwrap().captured_at_us - accepted.candidate.trigger_read_at_us
            > 100_000
    );
}

#[test]
fn v1_baseline_unresolved_dispatch_expires_without_replacing_reply() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    let original = h.old();
    h.dispatch(10_000);
    for n in 2..=20 {
        assert!(h.feed(n * 10_000, 0.99).unwrap().is_none());
    }
    assert!(
        h.feed(210_000, 0.99)
            .unwrap_err()
            .to_string()
            .contains("write evidence expired")
    );
    assert_eq!(h.old(), original);
    assert!(!h.trace.iter().any(|e| e["kind"] == "input_admitted"));
}

#[test]
fn v1_baseline_does_not_suppress_previously_accepted_input_or_owned_outage_drain() {
    for outage in [false, true] {
        let mut h = Harness::new(TurnTaking::V1Baseline);
        let mut accepted = None;
        for n in 1..=6 {
            accepted = h.feed(n * 10_000, 0.99).unwrap().or(accepted);
        }
        assert!(accepted.is_some());
        if outage {
            h.admission
                .discard_stream_until_end(h.base + 60_000)
                .unwrap();
        }
        h.dispatch(61_000);
        h.accepted(62_000, 62_000);
        assert!(h.feed(70_000, 0.99).unwrap().is_none());
        assert!(h.admission.owns_stream());
        assert_eq!(
            h.mask.suppressed_blocks(),
            0,
            "accepted input cannot silently disappear"
        );
    }
}

#[test]
fn v1_baseline_long_output_gap_reopens_without_completing_reply() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    let old = h.old();
    h.dispatch(10_000);
    h.accepted(11_000, 11_000);
    h.drain(20_000);
    assert!(!h.reply.as_ref().unwrap().generation_done);
    assert!(h.reply.as_ref().unwrap().pcm.len() == 480);
    let mut accepted = None;
    for n in 1..=6 {
        accepted = h.feed(120_000 + n * 10_000, 0.99).unwrap().or(accepted);
    }
    assert_eq!(h.old(), old);
    assert!(!h.trace.iter().any(|e| e["kind"] == "turn_finished"));
    let queue = h.commit(accepted.unwrap());
    assert_ne!(h.old(), old);
    assert!(matches!(queue.front(), Some(ProviderInput::Start { .. })));
}

#[test]
fn v1_baseline_privacy_loss_and_capture_lineage_change_never_reopen_old_candidate() {
    let mut h = Harness::new(TurnTaking::V1Baseline);
    h.dispatch(10_000);
    for n in 2..=7 {
        assert!(h.feed(n * 10_000, 0.99).unwrap().is_none());
    }
    let (old, _) = h
        .admission
        .pending_evidence(h.base + 70_000, AcceptanceBasis::DirectedSessionV1Baseline)
        .unwrap();
    h.lineage.dsp_epoch += 1;
    h.detector.discard_inactive_prefix();
    h.mask.clear_read_lineage();
    assert!(h.feed(80_000, 0.99).unwrap().is_none());
    assert!(h.trace.iter().any(|e| e["reason"] == "capture_changed"));
    let context = AdmissionContext {
        capture: h.lineage,
        authority: h
            .owner
            .snapshot(MonoTime::from_micros(h.base + 80_000))
            .unwrap(),
    };
    assert!(matches!(
        h.admission
            .decide(
                Evidence {
                    candidate: old.id,
                    through_sequence: old.trigger_sequence,
                    produced_at_us: h.base + 80_000,
                    verdict: Verdict::Accept(AcceptanceBasis::DirectedSessionV1Baseline)
                },
                context,
                h.base + 80_000
            )
            .unwrap(),
        Decision::Ignored
    ));
    h.accepted(11_000, 81_000);
    h.owner
        .set_microphone_permission(MonoTime::from_micros(h.base + 82_000), Permission::Denied)
        .unwrap();
    assert!(h.feed(90_000, 0.99).is_err());
    assert!(
        !h.owner
            .snapshot(MonoTime::from_micros(h.base + 90_000))
            .unwrap()
            .listening_ready(MonoTime::from_micros(h.base + 90_000))
    );
}

struct SocketSpeaker {
    channels: WorkerChannels,
    published: Vec<Snapshot>,
}
impl SpeakerControlIo for SocketSpeaker {
    fn next_speaker_event(&mut self) -> io::Result<Option<WorkerEvent>> {
        self.channels.control.receive()
    }
    fn publish_speaker_snapshot(&mut self, snapshot: Snapshot) -> io::Result<()> {
        self.published.push(snapshot);
        Ok(())
    }
}
fn socket_pending(h: &mut Harness) -> Option<PendingSpeaker> {
    let permit = h
        .owner
        .issue_output(
            MonoTime::from_micros(h.base + 1),
            h.reply.as_ref().unwrap().plan,
            200_000,
        )
        .unwrap();
    Some(PendingSpeaker {
        origin: SpeechOrigin::Provider,
        sequence: 1,
        permit,
        accepted: 0,
        final_chunk: false,
        report_due_us: u64::MAX,
    })
}
fn write_receipt(h: &Harness, frames: usize, first: u64, completed: u64) -> WorkerEvent {
    WorkerEvent::Playback {
        owner: h.old(),
        turn: h.old().turn(),
        sequence: 1,
        accepted_frames: frames,
        queued_frames: 240,
        first_sample: PlaybackCursor {
            epoch: 1,
            frame: first,
        },
        end_sample: PlaybackCursor {
            epoch: 1,
            frame: first + frames as u64,
        },
        observed_at_us: h.base + completed,
        queue_observed_at_us: h.base + completed,
    }
}
fn service_socket(
    h: &mut Harness,
    io: &mut SocketSpeaker,
    pending: &mut Option<PendingSpeaker>,
    budget: &mut usize,
) -> bool {
    let mut snapshot = h.owner.snapshot(now()).unwrap();
    service_speaker_control(
        io,
        &mut h.owner,
        &mut h.reply,
        pending,
        &mut h.mask,
        &mut snapshot,
        &mut 0,
        budget,
        &mut h.trace,
    )
    .unwrap()
}
#[test]
fn independent_speaker_socket_second_drain_catches_first_write_after_empty_check() {
    let directory = SessionDirectory::create().unwrap();
    let (channels, mut peer) = pair(&directory, "speaker");
    let mut io = SocketSpeaker {
        channels,
        published: vec![],
    };
    let mut h = Harness::with_lag(TurnTaking::V1Baseline, 100_000);
    let old = h.old();
    let mut pending = socket_pending(&mut h);
    h.dispatch(10_000);
    let mut budget = 32;
    // Use the actual channel read to prove the first priority check was empty.
    // Snapshot at the same source time here avoids moving the test controller
    // beyond the original capture observations about to be retained.
    let mut snapshot = h
        .owner
        .snapshot(MonoTime::from_micros(h.base + 10_000))
        .unwrap();
    assert!(
        service_speaker_control(
            &mut io,
            &mut h.owner,
            &mut h.reply,
            &mut pending,
            &mut h.mask,
            &mut snapshot,
            &mut 0,
            &mut budget,
            &mut h.trace
        )
        .unwrap()
    );
    for n in 2..=7 {
        assert!(h.feed(n * 10_000, 0.99).unwrap().is_none());
    }
    peer.control
        .send(write_receipt(&h, 240, 480, 11_000))
        .unwrap();
    assert!(service_socket(&mut h, &mut io, &mut pending, &mut budget));
    let context = AdmissionContext {
        capture: h.lineage,
        authority: h.owner.snapshot(now()).unwrap(),
    };
    let at = monotonic_us();
    let (candidate, _) = h
        .admission
        .pending_evidence(at, AcceptanceBasis::DirectedSessionV1Baseline)
        .unwrap();
    assert!(
        decide_prospective_input(
            &h.mask,
            &mut h.detector,
            &mut h.admission,
            candidate,
            context,
            at,
            &mut h.trace
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(h.old(), old);
    assert!(pending.is_none());
    assert!(io.published.last().unwrap().playback().is_some());
    assert!(!h.trace.iter().any(|e| e["outcome"] == "user_interrupted"));
}

#[test]
fn speaker_control_budget_exhaustion_defers_pending_candidate_and_fault_keeps_priority() {
    let directory = SessionDirectory::create().unwrap();
    let (channels, mut peer) = pair(&directory, "speaker");
    let mut io = SocketSpeaker {
        channels,
        published: vec![],
    };
    let mut h = Harness::with_lag(TurnTaking::V1Baseline, 100_000);
    let old = h.old();
    let mut pending = socket_pending(&mut h);
    h.dispatch(10_000);
    for n in 2..=7 {
        assert!(h.feed(n * 10_000, 0.99).unwrap().is_none());
    }
    peer.control
        .send(write_receipt(&h, 120, 480, 11_000))
        .unwrap();
    peer.control
        .send(WorkerEvent::Fault {
            code: "test-only speaker failure".into(),
        })
        .unwrap();
    let mut budget = 1;
    assert!(!service_socket(&mut h, &mut io, &mut pending, &mut budget));
    assert_eq!(budget, 0);
    assert!(!service_socket(&mut h, &mut io, &mut pending, &mut budget));
    assert!(
        h.admission
            .pending_evidence(monotonic_us(), AcceptanceBasis::DirectedSessionV1Baseline)
            .is_some()
    );
    assert_eq!(h.old(), old);
    let mut snapshot = h.owner.snapshot(now()).unwrap();
    budget = 32;
    assert!(
        service_speaker_control(
            &mut io,
            &mut h.owner,
            &mut h.reply,
            &mut pending,
            &mut h.mask,
            &mut snapshot,
            &mut 0,
            &mut budget,
            &mut h.trace
        )
        .unwrap_err()
        .to_string()
        .contains("test-only speaker failure")
    );
    assert!(!h.trace.iter().any(|e| e["outcome"] == "user_interrupted"));
}

#[test]
fn actual_reset_receipt_preserves_old_mask_then_accepts_fresh_successor_epoch() {
    let directory = SessionDirectory::create().unwrap();
    let (channels, mut peer) = pair(&directory, "speaker");
    let mut io = SocketSpeaker {
        channels,
        published: vec![],
    };
    let mut h = Harness::new(TurnTaking::V1Baseline);
    h.dispatch(10_000);
    h.accepted(11_000, 11_000);
    let old = h.old();
    // The old nonfinal cursor already retired: no queued ownership-failure
    // record exists, but successful stop still reports its exact old range.
    peer.control
        .send(WorkerEvent::PlaybackClockReset {
            previous: lamp_audio::ownership::QueuedRange {
                epoch: 1,
                retired_through: 720,
                accepted_through: 720,
            },
            next_epoch: 2,
            observed_at_us: h.base + 330_000,
        })
        .unwrap();
    let mut budget = 32;
    let mut pending = None;
    assert!(service_socket(&mut h, &mut io, &mut pending, &mut budget));
    let received = monotonic_us() - h.base;
    assert!(h.feed_received(430_000, received, 0.99).unwrap().is_none());
    assert_eq!(h.mask.suppressed_blocks(), 1);
    let mut accepted = None;
    for n in 1..=6 {
        let received = monotonic_us() - h.base;
        accepted = h
            .feed_received(430_000 + n * 10_000, received, 0.99)
            .unwrap()
            .or(accepted);
    }
    let _queue = h.commit(accepted.expect("fresh input after reset tail was lost"));
    assert_ne!(h.old(), old);
    h.owner.input_ended(now(), h.old()).unwrap();
    h.reply.as_mut().unwrap().local_input_ended = true;
    h.reply.as_mut().unwrap().cloud = submitted_request();
    h.reply.as_mut().unwrap().pcm = vec![9; 480].into();
    let mut sequence = 1;
    let mut snapshot = h.owner.snapshot(now()).unwrap();
    dispatch_speaker_chunk(
        &mut h.owner,
        &mut h.reply,
        &mut io.channels.data,
        &mut pending,
        &mut sequence,
        &mut snapshot,
        &mut h.mask,
    )
    .unwrap();
    let sent = peer.data.receive::<SpeakerChunk>().unwrap().unwrap();
    let written = monotonic_us();
    peer.control
        .send(WorkerEvent::Playback {
            owner: sent.permit.owner(),
            turn: sent.permit.owner().turn(),
            sequence: sent.chunk_sequence,
            accepted_frames: 240,
            queued_frames: 240,
            first_sample: PlaybackCursor {
                epoch: 2,
                frame: 480,
            },
            end_sample: PlaybackCursor {
                epoch: 2,
                frame: 720,
            },
            observed_at_us: written,
            queue_observed_at_us: written,
        })
        .unwrap();
    assert!(service_socket(&mut h, &mut io, &mut pending, &mut budget));
    assert_eq!(h.old(), sent.permit.owner());
    assert!(h.reply.as_ref().unwrap().playback.is_some());
    assert!(
        !h.trace
            .iter()
            .any(|e| e["kind"] == "speech_final_sample_retired")
    );
}

#[test]
fn pre_dispatch_input_can_replace_reply_and_late_old_write_cannot_fault_successor() {
    let directory = SessionDirectory::create().unwrap();
    let (channels, mut peer) = pair(&directory, "speaker");
    let mut io = SocketSpeaker {
        channels,
        published: vec![],
    };
    let mut h = Harness::new(TurnTaking::V1Baseline);
    let old = h.old();
    h.dispatch(465_000);
    let mut accepted = None;
    for n in 1..=6 {
        accepted = h
            .feed_received(390_000 + n * 10_000, 475_000 + n, 0.99)
            .unwrap()
            .or(accepted);
    }
    let queue = h.commit(accepted.expect("strictly pre-dispatch onset was incorrectly masked"));
    assert!(matches!(queue.front(), Some(ProviderInput::Start { .. })));
    let successor = h.old();
    assert_ne!(old, successor);
    peer.control
        .send(WorkerEvent::Playback {
            owner: old,
            turn: old.turn(),
            sequence: 1,
            accepted_frames: 240,
            queued_frames: 240,
            first_sample: PlaybackCursor {
                epoch: 1,
                frame: 480,
            },
            end_sample: PlaybackCursor {
                epoch: 1,
                frame: 720,
            },
            observed_at_us: h.base + 467_000,
            queue_observed_at_us: h.base + 467_000,
        })
        .unwrap();
    let mut budget = 32;
    assert!(service_socket(&mut h, &mut io, &mut None, &mut budget));
    assert_eq!(h.old(), successor);
    assert!(h.reply.as_ref().unwrap().playback.is_none());
    assert!(h.admission.owns_stream());
    assert!(!h.trace.iter().any(|e| e["kind"] == "speaker_first_write"));
}

#[test]
fn valid_control_receipts_and_short_read_gap_never_forward_masked_preroll() {
    use crate::wire::{Envelope, ReceiveOrder, decode, encode};
    let mut h = Harness::new(TurnTaking::V1Baseline);
    h.dispatch(10_000);
    for n in 2..=11 {
        assert!(h.feed(n * 10_000, 0.0).unwrap().is_none());
    }
    let worker = new_boot().unwrap();
    let mut order = ReceiveOrder::new(worker);
    let acceptance = Envelope {
        boot: worker,
        sequence: 1,
        sent_at_us: h.base + 12_000,
        payload: write_receipt(&h, 240, 480, 11_000),
    };
    let packet: Envelope<WorkerEvent> = decode(&encode(&acceptance).unwrap()).unwrap();
    order.accept(&packet, h.base + 111_000, 100_000).unwrap();
    h.accepted(11_000, 111_000);
    let drain = Envelope {
        boot: worker,
        sequence: 2,
        sent_at_us: h.base + 21_000,
        payload: WorkerEvent::SpeechDrained {
            owner: h.old(),
            sequence: 1,
            last_sample: PlaybackCursor {
                epoch: 1,
                frame: 720,
            },
            observed_at_us: h.base + 20_000,
        },
    };
    let packet: Envelope<WorkerEvent> = decode(&encode(&drain).unwrap()).unwrap();
    order.accept(&packet, h.base + 111_000, 100_000).unwrap();
    h.mask
        .drained(
            h.old(),
            1,
            PlaybackCursor {
                epoch: 1,
                frame: 720,
            },
            h.base + 20_000,
            h.base + 111_000,
        )
        .unwrap();
    // The next actual read starts after the tail. Its 21 ms completion gap is
    // within the existing 50 ms continuity rule; no PCM sequence was dropped.
    let mut accepted = None;
    for n in 0..6 {
        accepted = h.feed(131_000 + n * 10_000, 0.99).unwrap().or(accepted);
    }
    let accepted = accepted.expect("six fresh onset blocks must remain usable");
    println!(
        "acceptance_age_us=99000 drain_age_us=90000 tail_end_offset_us=120000 next_first_read_offset_us=121000 capture_completion_gap_us=21000 accepted_prefix_offsets={:?}",
        accepted
            .audio
            .iter()
            .map(|f| f.captured_at_us - h.base)
            .collect::<Vec<_>>()
    );
    assert!(
        accepted
            .audio
            .iter()
            .all(|f| f.captured_at_us >= h.base + 131_000),
        "old masked quiet prefix was sent with a fresh user onset"
    );
    assert_eq!(accepted.audio.len(), 6);
    assert_eq!(accepted.candidate.first_read_at_us, h.base + 20_000);
    assert_eq!(accepted.candidate.trigger_read_at_us, h.base + 181_000);
    assert_eq!(accepted.candidate.decision_deadline_us, h.base + 381_000);
    assert_eq!(
        h.trace
            .iter()
            .find(|e| e["kind"] == "input_prefix_sanitized")
            .unwrap()["removed_blocks"],
        10
    );
    let queued = h.commit(accepted);
    assert!(matches!(queued.front(), Some(ProviderInput::Start { .. })));
    let fresh_samples: Vec<_> = queued
        .iter()
        .filter_map(|input| match input {
            ProviderInput::Audio { samples, .. } => Some(samples[0]),
            _ => None,
        })
        .collect();
    assert_eq!(fresh_samples, vec![11, 12, 13, 14, 15, 16]);
}
