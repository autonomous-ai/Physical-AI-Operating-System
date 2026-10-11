//! Opt-in, host-read-domain turn taking. This is not acoustic echo detection.
//! Positive speaker acceptance opens a mask; actual cursor retirement closes it.
//! This module never grants authority, completes replies, or changes DSP state.
//!
//! The duplex mask is the same mask with ears. While it is given echo
//! evidence for every capture block it lets blocks through during the lamp's
//! own output, scored so that the detector can start on a person and not on
//! the lamp, and it holds a candidate that began over the lamp until the
//! lamp's own sound has gone and speech is still there. Without evidence for
//! a block it is the conservative mask again, block by block.
use crate::{
    activity::{ObservedAudio, START_BLOCKS},
    admission::Candidate,
    options::TurnTaking,
};
use lamp_audio::{
    duplex::{Confirmation, Heard, Hearing, Interruption, Margin, Step},
    evidence::EchoEvidence,
    ownership::{PlaybackCursor, QueuedRange},
};
use lamp_interaction::TurnOwner;
use std::{collections::VecDeque, io};

pub const TAIL_US: u64 = 100_000;
const HISTORY_US: u64 = 600_000;
const MAX_INTERVALS: usize = 16;
const MAX_READS: usize = 80;
/// Blocks whose hearing is remembered: longer than any candidate is held.
const MAX_HEARD: usize = 128;
/// Yields in one reply that came to nothing before the mask stops yielding
/// for the rest of that reply and masks as the conservative baseline does.
pub const FRUITLESS_YIELDS: u8 = 2;
/// Hearing older than this withholds nothing. Capture blocks arrive every
/// 10 ms; a muted or stalled microphone must not hold a reply back.
pub const HEARING_STALE_US: u64 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Disposition {
    Pass,
    Suppress,
    AwaitWriteEvidence,
}

#[derive(Clone, Copy)]
struct Read {
    sequence: u64,
    first: u64,
    completed: u64,
}
#[derive(Clone, Copy)]
struct Dispatch {
    owner: TurnOwner,
    sequence: u64,
    at_us: u64,
    frames: usize,
    accepted: usize,
    end: Option<PlaybackCursor>,
}
#[derive(Clone, Copy)]
struct Span {
    owner: TurnOwner,
    start: u64,
    end: Option<u64>,
    sequence: u64,
    cursor: PlaybackCursor,
    accepted_at: u64,
}

/// What became of the block that made the lamp yield.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Yielded {
    Undecided,
    Committed,
    Resumed,
}

struct Duplex {
    hearing: Hearing,
    interruption: Interruption,
    margin: Margin,
    heard: VecDeque<(u64, Heard)>,
    last_yield: Option<(u64, Yielded)>,
    /// The latest block came without evidence: mask it conservatively.
    blind: bool,
    /// Yields since the reply began that ended in a resume.
    fruitless: u8,
    /// When the latest block was heard.
    listened_at_us: u64,
}

/// One capture block as the duplex mask heard it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Listened {
    pub heard: Option<Heard>,
    /// The score to give the turn detector for this block.
    pub score: f32,
    pub step: Step,
}

pub(crate) struct PlaybackMask {
    policy: TurnTaking,
    duplex: Option<Duplex>,
    pending: VecDeque<Dispatch>,
    spans: VecDeque<Span>,
    reads: VecDeque<Read>,
    epoch: u64,
    last_reset_at: u64,
    last_dispatch: u64,
    last_now: u64,
    suppressed_blocks: u64,
}
impl PlaybackMask {
    pub fn new(policy: TurnTaking) -> Self {
        Self {
            policy,
            duplex: None,
            pending: VecDeque::with_capacity(8),
            spans: VecDeque::with_capacity(MAX_INTERVALS),
            reads: VecDeque::with_capacity(MAX_READS),
            epoch: 1,
            last_reset_at: 0,
            last_dispatch: 0,
            last_now: 0,
            suppressed_blocks: 0,
        }
    }
    /// The conservative mask with ears. See the module documentation.
    pub fn duplex() -> Self {
        Self {
            duplex: Some(Duplex {
                hearing: Hearing::EVALUATED,
                interruption: Interruption::new(Confirmation::EVALUATED),
                margin: Margin::default(),
                heard: VecDeque::with_capacity(MAX_HEARD),
                last_yield: None,
                blind: true,
                fruitless: 0,
                listened_at_us: 0,
            }),
            ..Self::new(TurnTaking::V1Baseline)
        }
    }
    pub fn is_duplex(&self) -> bool {
        self.duplex.is_some()
    }
    /// Hear one capture block. Call for every block, in order, before asking
    /// for its disposition. `speaking` is whether a reply has audio that
    /// would otherwise be going to the speaker, or sound still in the air.
    /// Without evidence the block is not judged and keeps its own score; the
    /// conservative mask covers it.
    pub fn listen(
        &mut self,
        sequence: u64,
        probability: f32,
        evidence: Option<&EchoEvidence>,
        speaking: bool,
        at_us: u64,
    ) -> io::Result<Listened> {
        let unjudged = Listened {
            heard: None,
            score: probability,
            step: Step::Carry,
        };
        let Some(duplex) = self.duplex.as_mut() else {
            return Ok(unjudged);
        };
        if at_us < duplex.listened_at_us {
            return Err(invalid("hearing time moved backwards"));
        }
        duplex.listened_at_us = at_us;
        if duplex
            .heard
            .back()
            .is_some_and(|(last, _)| last.checked_add(1) != Some(sequence))
        {
            // A gap in capture: what was heard before it says nothing now.
            duplex.heard.clear();
            duplex.interruption.clear();
            duplex.last_yield = None;
        }
        let Some(evidence) = evidence else {
            duplex.blind = true;
            // A yield in force still runs to its deadline.
            let step = duplex.interruption.block(Heard::Unknown, speaking);
            Self::settle(duplex, sequence, step);
            return Ok(Listened { step, ..unjudged });
        };
        duplex.blind = false;
        let heard = duplex.hearing.heard(probability, evidence);
        duplex.margin.observe(heard, evidence);
        // A reply that keeps yielding to nothing stops yielding.
        let may_yield = speaking && duplex.fruitless < FRUITLESS_YIELDS;
        let step = duplex.interruption.block(heard, may_yield);
        if duplex.heard.len() == MAX_HEARD {
            duplex.heard.pop_front();
        }
        duplex.heard.push_back((sequence, heard));
        Self::settle(duplex, sequence, step);
        let score = if speaking && duplex.fruitless >= FRUITLESS_YIELDS && heard == Heard::Over {
            // Fallen back for this reply: a person over the lamp is not let
            // through either, as in the conservative baseline.
            0.0
        } else {
            Hearing::detector_score(heard, probability)
        };
        Ok(Listened {
            heard: Some(heard),
            score,
            step,
        })
    }
    fn settle(duplex: &mut Duplex, sequence: u64, step: Step) {
        match step {
            Step::Carry => {}
            Step::Yield => duplex.last_yield = Some((sequence, Yielded::Undecided)),
            Step::Commit => {
                if let Some((_, state)) = duplex.last_yield.as_mut() {
                    *state = Yielded::Committed;
                }
                duplex.fruitless = 0;
            }
            Step::Resume => {
                if let Some((_, state)) = duplex.last_yield.as_mut() {
                    *state = Yielded::Resumed;
                }
                duplex.fruitless = duplex.fruitless.saturating_add(1);
            }
        }
    }
    /// Whether reply audio is being withheld from the speaker for a person.
    /// Only while hearing is current: see [`HEARING_STALE_US`].
    pub fn yielded(&self, now: u64) -> bool {
        self.duplex.as_ref().is_some_and(|duplex| {
            duplex.interruption.yielded()
                && now.saturating_sub(duplex.listened_at_us) <= HEARING_STALE_US
        })
    }
    /// A reply ended or was replaced: the next one starts with a clean
    /// record. A yield in force is left to be settled by what is heard: a
    /// candidate may be waiting on it, and with no reply it withholds nothing.
    pub fn reply_closed(&mut self) {
        if let Some(duplex) = self.duplex.as_mut() {
            duplex.fruitless = 0;
        }
    }
    /// The person's level against the lamp's own at the microphone, once
    /// both have been heard. See `lamp_audio::duplex::Margin`.
    pub fn hearing_margin_db(&self) -> Option<f32> {
        self.duplex.as_ref().and_then(|duplex| duplex.margin.db())
    }
    /// Yields, commits and resumes so far, and whether this reply has fallen
    /// back to the conservative mask.
    pub fn duplex_summary(&self) -> Option<((u32, u32, u32), bool)> {
        self.duplex.as_ref().map(|duplex| {
            (
                duplex.interruption.counts(),
                duplex.fruitless >= FRUITLESS_YIELDS,
            )
        })
    }
    /// Hearing, when the latest block was judged.
    fn hearing(&self) -> Option<&Duplex> {
        self.duplex.as_ref().filter(|duplex| !duplex.blind)
    }
    /// A candidate whose six onset blocks end at `trigger`: one that began
    /// over the lamp waits for its yield to be settled. `None` when not all
    /// of the onset was heard; the conservative rules then decide it.
    fn heard_disposition(duplex: &Duplex, first: u64, trigger: u64) -> Option<Disposition> {
        let mut over = false;
        let mut found = 0;
        for (_, heard) in duplex
            .heard
            .iter()
            .filter(|(sequence, _)| (first..=trigger).contains(sequence))
        {
            found += 1;
            over |= *heard == Heard::Over;
        }
        if trigger < first || found != trigger - first + 1 {
            return None;
        }
        if !over {
            return Some(Disposition::Pass);
        }
        Some(match duplex.last_yield {
            Some((at, Yielded::Undecided)) if at >= first => Disposition::AwaitWriteEvidence,
            Some((at, Yielded::Resumed)) if at >= first => Disposition::Suppress,
            // Committed, or a person over a sound that was no reply.
            _ => Disposition::Pass,
        })
    }
    /// Whether this capture block was judged with evidence.
    fn was_heard(&self, sequence: u64) -> bool {
        self.hearing()
            .is_some_and(|duplex| duplex.heard.iter().any(|(heard, _)| *heard == sequence))
    }
    pub fn suppressed(&mut self) -> io::Result<()> {
        self.suppressed_blocks = self
            .suppressed_blocks
            .checked_add(1)
            .ok_or_else(|| invalid("suppression counter overflow"))?;
        Ok(())
    }
    pub fn suppressed_blocks(&self) -> u64 {
        self.suppressed_blocks
    }
    pub fn discarded(&mut self, owner: TurnOwner, observed: u64, now: u64) -> io::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.advance(now)?;
        if observed == 0 || observed > now {
            return Err(invalid("invalid playback discard time"));
        }
        for span in &mut self.spans {
            if span.owner == owner && span.end.is_none() {
                if observed < span.accepted_at {
                    return Err(invalid("discard predates accepted speech"));
                }
                span.end = Some(
                    observed
                        .checked_add(TAIL_US)
                        .ok_or_else(|| invalid("tail overflow"))?,
                );
            }
        }
        Ok(())
    }
    /// A successful physical drop/reset is not playback retirement. Preserve
    /// the closed old interval for delayed capture and accept only its exact
    /// next clock epoch. Pending command rejection obligations remain intact.
    pub fn clock_reset(
        &mut self,
        previous: QueuedRange,
        next_epoch: u64,
        observed: u64,
        now: u64,
    ) -> io::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.advance(now)?;
        if previous.epoch == 0
            || previous.epoch.checked_add(1) != Some(next_epoch)
            || previous.retired_through > previous.accepted_through
            || previous.accepted_through - previous.retired_through
                > lamp_audio::ownership::MAX_QUEUED_FRAMES as u64
            || self.epoch != previous.epoch
            || observed == 0
            || observed > now
            || observed < self.last_reset_at
        {
            return Err(invalid("invalid or duplicate physical playback reset"));
        }
        for span in self
            .spans
            .iter_mut()
            .filter(|span| span.cursor.epoch == previous.epoch)
        {
            if span.cursor.frame > previous.accepted_through || observed < span.accepted_at {
                return Err(invalid("playback reset does not cover accepted speech"));
            }
            if span.end.is_none() {
                span.end = Some(
                    observed
                        .checked_add(TAIL_US)
                        .ok_or_else(|| invalid("tail overflow"))?,
                );
            }
        }
        self.epoch = next_epoch;
        self.last_reset_at = observed;
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.policy == TurnTaking::V1Baseline
    }
    fn advance(&mut self, now: u64) -> io::Result<()> {
        if now < self.last_now {
            return Err(invalid("turn-taking clock regressed"));
        }
        self.last_now = now;
        if self
            .pending
            .iter()
            .any(|p| now.saturating_sub(p.at_us) >= crate::admission::DECISION_BUDGET_US)
        {
            return Err(invalid("speaker write evidence expired"));
        }
        while self.spans.front().is_some_and(|s| {
            s.end
                .is_some_and(|end| now.saturating_sub(end) > HISTORY_US)
        }) {
            self.spans.pop_front();
        }
        Ok(())
    }
    pub fn dispatched(
        &mut self,
        owner: TurnOwner,
        sequence: u64,
        at_us: u64,
        frames: usize,
    ) -> io::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.advance(at_us)?;
        if (self.pending.len() == 8 || self.pending.iter().any(|p| p.owner == owner))
            || sequence <= self.last_dispatch
            || at_us == 0
            || frames == 0
            || frames > 240
        {
            return Err(invalid("invalid or unresolved speaker dispatch"));
        }
        self.last_dispatch = sequence;
        self.pending.push_back(Dispatch {
            owner,
            sequence,
            at_us,
            frames,
            accepted: 0,
            end: None,
        });
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn accepted(
        &mut self,
        owner: TurnOwner,
        sequence: u64,
        first: PlaybackCursor,
        end: PlaybackCursor,
        frames: usize,
        completed: u64,
        queue_observed: u64,
        now: u64,
    ) -> io::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.advance(now)?;
        if self.epoch != first.epoch {
            return Err(invalid(
                "accepted speech changed epoch without physical reset",
            ));
        }
        let index = self
            .pending
            .iter()
            .position(|p| p.sequence == sequence)
            .ok_or_else(|| invalid("write has no original dispatch"))?;
        let pending = &mut self.pending[index];
        if owner != pending.owner
            || sequence != pending.sequence
            || first.epoch == 0
            || first.epoch != end.epoch
            || end.frame.checked_sub(first.frame) != Some(frames as u64)
            || frames == 0
            || completed < pending.at_us
            || completed > queue_observed
            || queue_observed > now
            || pending.end.is_some_and(|previous| previous != first)
            || pending
                .accepted
                .checked_add(frames)
                .is_none_or(|n| n > pending.frames)
        {
            return Err(invalid("invalid accepted speech receipt"));
        }
        let start = pending.at_us;
        pending.accepted += frames;
        pending.end = Some(end);
        let done = pending.accepted == pending.frames;
        if let Some(last) = self.spans.back_mut() {
            if (first.epoch == last.cursor.epoch && first.frame < last.cursor.frame)
                || completed < last.accepted_at
            {
                return Err(invalid("accepted speech cursor regressed"));
            }
            if first.epoch == last.cursor.epoch
                && last.owner == owner
                && last.end.is_none_or(|until| start <= until)
            {
                last.end = None;
                last.sequence = sequence;
                last.cursor = end;
                last.accepted_at = completed;
            } else {
                if last.end.is_none() {
                    return Err(invalid("new speech owner before old speech drained"));
                }
                self.push_span(Span {
                    owner,
                    start,
                    end: None,
                    sequence,
                    cursor: end,
                    accepted_at: completed,
                })?;
            }
        } else {
            self.push_span(Span {
                owner,
                start,
                end: None,
                sequence,
                cursor: end,
                accepted_at: completed,
            })?;
        }
        if done {
            self.pending.remove(index);
        }
        Ok(())
    }
    fn push_span(&mut self, span: Span) -> io::Result<()> {
        if self.spans.len() == MAX_INTERVALS {
            return Err(invalid("turn-taking interval history full"));
        }
        self.spans.push_back(span);
        Ok(())
    }
    /// A nonfinal gap receipt is local queue evidence, never conversation completion.
    pub fn drained(
        &mut self,
        owner: TurnOwner,
        sequence: u64,
        cursor: PlaybackCursor,
        observed: u64,
        now: u64,
    ) -> io::Result<bool> {
        if !self.enabled() {
            return Ok(false);
        }
        self.advance(now)?;
        if cursor.epoch == 0 || cursor.frame == 0 || observed == 0 || observed > now {
            return Err(invalid("invalid speech-drained time/cursor"));
        }
        let known_owner = self.spans.iter().any(|s| s.owner == owner);
        let last = self
            .spans
            .back_mut()
            .ok_or_else(|| invalid("drain without accepted speech"))?;
        if owner != last.owner || sequence != last.sequence || cursor != last.cursor {
            // An older receipt cannot close a resumed write. It must still name
            // a known original owner and a cursor strictly behind the latest one.
            if known_owner
                && cursor.epoch == last.cursor.epoch
                && cursor.frame < last.cursor.frame
                && sequence <= last.sequence
            {
                return Ok(false);
            }
            return Err(invalid(
                "speech-drained receipt mismatched latest accepted cursor",
            ));
        }
        if last.end.is_some() || observed < last.accepted_at {
            return Err(invalid("duplicate or regressing speech drain"));
        }
        last.end = Some(
            observed
                .checked_add(TAIL_US)
                .ok_or_else(|| invalid("tail overflow"))?,
        );
        Ok(true)
    }
    /// Existing immutable cancelled-tail evidence closes only its original mask.
    pub fn cancelled_tail(
        &mut self,
        owner: TurnOwner,
        through: PlaybackCursor,
        observed: u64,
        now: u64,
    ) -> io::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.advance(now)?;
        let Some(last) = self.spans.back_mut().filter(|s| s.owner == owner) else {
            return Ok(());
        };
        if through.epoch != last.cursor.epoch
            || through.frame < last.cursor.frame
            || observed < last.accepted_at
            || observed > now
        {
            return Err(invalid("cancelled tail does not cover accepted speech"));
        }
        if last.end.is_none() {
            last.end = Some(
                observed
                    .checked_add(TAIL_US)
                    .ok_or_else(|| invalid("tail overflow"))?,
            );
        }
        Ok(())
    }
    pub fn rejected(&mut self, sequence: u64, now: u64) -> io::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.advance(now)?;
        if let Some(index) = self.pending.iter().position(|p| p.sequence == sequence) {
            self.pending.remove(index);
            Ok(())
        } else {
            Err(invalid("speaker rejection has no matching dispatch"))
        }
    }
    pub fn remember_read(
        &mut self,
        sequence: u64,
        first: u64,
        completed: u64,
        now: u64,
    ) -> io::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        self.advance(now)?;
        if sequence == 0
            || first == 0
            || first > completed
            || completed > now
            || now.saturating_sub(first) > HISTORY_US
            || self
                .reads
                .back()
                .is_some_and(|r| r.sequence.checked_add(1) != Some(sequence) || first < r.completed)
        {
            return Err(invalid("invalid original microphone read interval"));
        }
        if self.reads.len() == MAX_READS {
            self.reads.pop_front();
        }
        self.reads.push_back(Read {
            sequence,
            first,
            completed,
        });
        Ok(())
    }
    pub fn clear_read_lineage(&mut self) {
        self.reads.clear();
        if let Some(duplex) = self.duplex.as_mut() {
            // Another capture stream: what was heard on the old one is gone.
            duplex.heard.clear();
            duplex.interruption.clear();
            duplex.last_yield = None;
            duplex.blind = true;
        }
    }
    fn read_disposition(&self, read: Read) -> Disposition {
        if self
            .spans
            .iter()
            .any(|s| read.completed >= s.start && s.end.is_none_or(|end| read.first < end))
        {
            return Disposition::Suppress;
        }
        if self.pending.iter().any(|p| read.completed >= p.at_us) {
            Disposition::AwaitWriteEvidence
        } else {
            Disposition::Pass
        }
    }
    pub fn frame(&self) -> Disposition {
        if self.hearing().is_some() {
            // Heard blocks reach the detector with the score `listen` gave.
            return Disposition::Pass;
        }
        self.reads
            .back()
            .map_or(Disposition::Pass, |r| self.read_disposition(*r))
    }
    /// Recheck retained pre-roll after delayed output evidence. Use the exact
    /// original read interval, never infer its beginning from receipt time.
    pub fn retained_frame(&self, frame: &ObservedAudio) -> io::Result<Disposition> {
        if !self.enabled() || self.was_heard(frame.sequence) {
            // Duplex keeps what the person said over the lamp.
            return Ok(Disposition::Pass);
        }
        let read = self
            .reads
            .iter()
            .find(|read| read.sequence == frame.sequence)
            .filter(|read| read.completed == frame.captured_at_us)
            .ok_or_else(|| invalid("retained prefix original read interval unavailable"))?;
        if self.last_now.saturating_sub(read.first) > HISTORY_US {
            return Err(invalid("retained prefix original read interval expired"));
        }
        Ok(self.read_disposition(*read))
    }
    pub fn candidate(&self, candidate: Candidate) -> io::Result<Disposition> {
        if !self.enabled() {
            return Ok(Disposition::Pass);
        }
        let first_sequence = candidate
            .trigger_sequence
            .saturating_sub(u64::from(START_BLOCKS) - 1)
            .max(1);
        if let Some(disposition) = self.hearing().and_then(|duplex| {
            Self::heard_disposition(duplex, first_sequence, candidate.trigger_sequence)
        }) {
            return Ok(disposition);
        }
        let mut disposition = Disposition::Pass;
        let mut found = false;
        for read in self
            .reads
            .iter()
            .filter(|r| (first_sequence..=candidate.trigger_sequence).contains(&r.sequence))
        {
            found |= read.sequence == candidate.trigger_sequence;
            match self.read_disposition(*read) {
                Disposition::Suppress => return Ok(Disposition::Suppress),
                Disposition::AwaitWriteEvidence => disposition = Disposition::AwaitWriteEvidence,
                Disposition::Pass => {}
            }
        }
        if !found {
            return Err(invalid("candidate original read interval unavailable"));
        }
        Ok(disposition)
    }
    pub fn available(&self, now: u64) -> bool {
        !self.enabled()
            || (self.pending.is_empty()
                && self
                    .spans
                    .back()
                    .is_none_or(|s| s.end.is_some_and(|end| now >= end)))
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::other(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_interaction::{
        AdmissionState, AdmittedInput, BootId, CaptureState, Controller, MonoTime, Permission,
    };
    fn owners() -> (TurnOwner, TurnOwner) {
        let t = MonoTime::from_micros(1_000_000);
        let mut c = Controller::new(BootId::new([8; 16]).unwrap(), t);
        c.set_microphone_permission(t, Permission::Allowed).unwrap();
        c.set_capture(
            t,
            CaptureState::RetainingUntil(MonoTime::from_micros(1_900_000)),
        )
        .unwrap();
        c.set_admission(
            t,
            AdmissionState::OpenUntil(MonoTime::from_micros(1_900_000)),
        )
        .unwrap();
        let a = c.admit(t, AdmittedInput::NewTurn).unwrap();
        let b = c.admit(t, AdmittedInput::Interruption).unwrap();
        (a, b)
    }
    fn cursor(frame: u64) -> PlaybackCursor {
        PlaybackCursor { epoch: 1, frame }
    }
    fn mask() -> PlaybackMask {
        PlaybackMask::new(TurnTaking::V1Baseline)
    }
    #[test]
    fn partial_writes_and_nonfinal_gap_preserve_exact_cursors_and_tail() {
        let (owner, _) = owners();
        let mut m = mask();
        m.dispatched(owner, 1, 1_000_000, 240).unwrap();
        m.accepted(
            owner,
            1,
            cursor(480),
            cursor(600),
            120,
            1_001_000,
            1_001_000,
            1_001_000,
        )
        .unwrap();
        m.accepted(
            owner,
            1,
            cursor(600),
            cursor(720),
            120,
            1_002_000,
            1_002_000,
            1_002_000,
        )
        .unwrap();
        assert!(
            !m.drained(owner, 1, cursor(600), 1_003_000, 1_003_000)
                .unwrap()
        );
        assert!(!m.available(1_090_000));
        m.drained(owner, 1, cursor(720), 1_020_000, 1_020_000)
            .unwrap();
        assert!(!m.available(1_119_999));
        assert!(m.available(1_120_000));
        // Provider owes more speech; intervening real zeros advance cursor.
        m.dispatched(owner, 2, 1_200_000, 240).unwrap();
        m.accepted(
            owner,
            2,
            cursor(5040),
            cursor(5280),
            240,
            1_201_000,
            1_201_000,
            1_201_000,
        )
        .unwrap();
        assert!(
            !m.drained(owner, 1, cursor(720), 1_021_000, 1_202_000)
                .unwrap()
        );
        assert!(!m.available(1_300_000));
    }
    #[test]
    fn delayed_retirement_uses_source_time_and_preserves_closed_history() {
        let (owner, _) = owners();
        let mut m = mask();
        m.dispatched(owner, 1, 1_000_000, 240).unwrap();
        m.accepted(
            owner,
            1,
            cursor(480),
            cursor(720),
            240,
            1_001_000,
            1_001_000,
            1_001_000,
        )
        .unwrap();
        m.drained(owner, 1, cursor(720), 1_020_000, 1_200_000)
            .unwrap();
        assert!(m.available(1_200_000));
        m.remember_read(1, 1_050_000, 1_060_000, 1_200_001).unwrap();
        assert_eq!(m.frame(), Disposition::Suppress);
        m.remember_read(2, 1_120_000, 1_130_000, 1_200_002).unwrap();
        assert_eq!(m.frame(), Disposition::Pass);
        assert!(
            m.drained(owner, 1, cursor(720), 1_020_000, 1_200_003)
                .is_err()
        );
    }
    #[test]
    fn late_original_write_settles_without_borrowing_successor_identity() {
        let (old, new) = owners();
        let mut m = mask();
        m.dispatched(old, 1, 1_000_000, 240).unwrap();
        m.dispatched(new, 2, 1_010_000, 240).unwrap();
        m.accepted(
            old,
            1,
            cursor(480),
            cursor(720),
            240,
            1_001_000,
            1_001_000,
            1_020_000,
        )
        .unwrap();
        m.cancelled_tail(old, cursor(720), 1_015_000, 1_021_000)
            .unwrap();
        m.accepted(
            new,
            2,
            cursor(720),
            cursor(960),
            240,
            1_022_000,
            1_022_000,
            1_022_000,
        )
        .unwrap();
        assert!(!m.available(1_030_000));
        assert!(
            !m.drained(old, 1, cursor(720), 1_015_000, 1_030_000)
                .unwrap()
        );
    }
    #[test]
    fn invalid_receipts_do_not_certify_mask_or_acceptance() {
        let (owner, wrong) = owners();
        for bad in 0..5 {
            let mut m = mask();
            m.dispatched(owner, 1, 1_000_000, 240).unwrap();
            let result = m.accepted(
                if bad == 0 { wrong } else { owner },
                1,
                cursor(480),
                cursor(if bad == 1 { 721 } else { 720 }),
                240,
                1_001_000,
                if bad == 2 {
                    1_000_000
                } else if bad == 3 {
                    1_020_000
                } else {
                    1_001_000
                },
                if bad == 4 { 999_999 } else { 1_002_000 },
            );
            assert!(result.is_err(), "case {bad}");
        }
        let mut m = mask();
        m.dispatched(owner, 1, 1_000_000, 240).unwrap();
        assert!(m.remember_read(1, 1_190_000, 1_200_000, 1_200_000).is_err());
    }
    #[test]
    fn original_read_start_is_preserved_and_clock_regression_is_rejected() {
        let (owner, _) = owners();
        let mut m = mask();
        m.dispatched(owner, 1, 1_000_000, 240).unwrap();
        m.accepted(
            owner,
            1,
            cursor(480),
            cursor(720),
            240,
            1_001_000,
            1_001_000,
            1_001_000,
        )
        .unwrap();
        m.drained(owner, 1, cursor(720), 1_020_000, 1_020_000)
            .unwrap();
        m.remember_read(1, 1_119_999, 1_130_000, 1_130_000).unwrap();
        assert_eq!(
            m.frame(),
            Disposition::Suppress,
            "completion alone would falsely pass this block"
        );
        assert!(m.remember_read(2, 1_120_000, 1_140_000, 1_129_999).is_err());
    }
    #[test]
    fn overlapping_read_intervals_and_sequence_overflow_are_invalid() {
        let mut m = mask();
        m.remember_read(1, 1_000_000, 1_010_000, 1_010_000).unwrap();
        assert!(m.remember_read(2, 1_009_999, 1_020_000, 1_020_000).is_err());
        let mut m = mask();
        m.remember_read(u64::MAX, 1_000_000, 1_010_000, 1_010_000)
            .unwrap();
        assert!(m.remember_read(1, 1_010_000, 1_020_000, 1_020_000).is_err());
    }
    #[test]
    fn explicit_reset_allows_only_exact_next_epoch_and_keeps_pending_rejection() {
        let (owner, _) = owners();
        let mut m = mask();
        m.dispatched(owner, 1, 1_000_000, 240).unwrap();
        m.accepted(
            owner,
            1,
            cursor(480),
            cursor(600),
            120,
            1_001_000,
            1_001_000,
            1_001_000,
        )
        .unwrap();
        let previous = QueuedRange {
            epoch: 1,
            retired_through: 480,
            accepted_through: 600,
        };
        m.clock_reset(previous, 2, 1_010_000, 1_010_000).unwrap();
        m.rejected(1, 1_011_000).unwrap(); // A partial command retains its explicit obligation.
        m.remember_read(1, 1_020_000, 1_030_000, 1_030_000).unwrap();
        assert_eq!(m.frame(), Disposition::Suppress);
        assert!(m.clock_reset(previous, 2, 1_010_000, 1_040_000).is_err());
        m.dispatched(owner, 2, 1_130_000, 240).unwrap();
        m.accepted(
            owner,
            2,
            PlaybackCursor {
                epoch: 2,
                frame: 480,
            },
            PlaybackCursor {
                epoch: 2,
                frame: 720,
            },
            240,
            1_131_000,
            1_131_000,
            1_131_000,
        )
        .unwrap();
        assert!(!m.available(1_140_000));
    }
    #[test]
    fn arbitrary_or_future_reset_cannot_authorize_new_epoch() {
        let (owner, _) = owners();
        for case in 0..3 {
            let mut m = mask();
            m.dispatched(owner, 1, 1_000_000, 240).unwrap();
            m.accepted(
                owner,
                1,
                cursor(480),
                cursor(720),
                240,
                1_001_000,
                1_001_000,
                1_001_000,
            )
            .unwrap();
            let previous = QueuedRange {
                epoch: if case == 0 { 2 } else { 1 },
                retired_through: 480,
                accepted_through: 720,
            };
            assert!(
                m.clock_reset(
                    previous,
                    if case == 0 {
                        3
                    } else if case == 1 {
                        4
                    } else {
                        2
                    },
                    if case == 2 { 1_020_000 } else { 1_010_000 },
                    1_011_000
                )
                .is_err()
            );
        }
        let mut m = mask();
        m.dispatched(owner, 1, 1_000_000, 240).unwrap();
        m.accepted(
            owner,
            1,
            cursor(480),
            cursor(720),
            240,
            1_001_000,
            1_001_000,
            1_001_000,
        )
        .unwrap();
        m.drained(owner, 1, cursor(720), 1_002_000, 1_002_000)
            .unwrap();
        m.dispatched(owner, 2, 1_150_000, 240).unwrap();
        assert!(
            m.accepted(
                owner,
                2,
                PlaybackCursor {
                    epoch: 2,
                    frame: 480
                },
                PlaybackCursor {
                    epoch: 2,
                    frame: 720
                },
                240,
                1_151_000,
                1_151_000,
                1_151_000
            )
            .is_err()
        );
    }
    #[test]
    fn late_rejection_cannot_erase_the_original_dispatch_deadline() {
        let (owner, _) = owners();
        let mut m = mask();
        m.dispatched(owner, 1, 1_000_000, 240).unwrap();
        assert!(m.rejected(1, 1_200_000).is_err());
    }
    #[test]
    fn retained_prefix_requires_matching_unexpired_original_read_evidence() {
        let mut m = mask();
        m.remember_read(1, 1_000_000, 1_010_000, 1_010_000).unwrap();
        let mut frame = ObservedAudio {
            sequence: 1,
            captured_at_us: 1_010_000,
            samples: [0; 160],
        };
        assert_eq!(m.retained_frame(&frame).unwrap(), Disposition::Pass);
        frame.captured_at_us += 1;
        assert!(m.retained_frame(&frame).is_err());
        frame.captured_at_us -= 1;
        frame.sequence = 2;
        assert!(m.retained_frame(&frame).is_err());
        frame.sequence = 1;
        m.advance(1_600_001).unwrap();
        assert!(m.retained_frame(&frame).is_err());
    }

    use lamp_audio::evidence::{EchoMatch, Level};

    /// Evidence for a block: render blocks since the lamp last sounded, and
    /// the shares the observer reports when it has an alignment.
    fn evidence(quiet: u8, shares: Option<(u8, u8)>) -> EchoEvidence {
        let level = Level::from_tenths_of_db(-200);
        EchoEvidence {
            blocks: 1_000,
            render_active_blocks: if quiet == 0 { 50 } else { 0 },
            render_quiet_blocks: quiet,
            capture_level: level,
            residual_level: level,
            echo: shares.map(|(capture_explained, residual_playback_floor)| EchoMatch {
                render_level: level,
                capture_explained,
                residual_explained: 10,
                residual_playback_floor,
                alignment_age_blocks: 0,
            }),
        }
    }
    const OWN: Option<(u8, u8)> = Some((97, 10));
    const OVER: Option<(u8, u8)> = Some((40, 0));

    /// Hear `count` blocks alike, from `*sequence` on. Returns the steps that
    /// were not `Carry`, with the sequence each came at.
    fn hear(
        mask: &mut PlaybackMask,
        sequence: &mut u64,
        count: u64,
        probability: f32,
        evidence: &EchoEvidence,
        speaking: bool,
    ) -> Vec<(u64, Step)> {
        let mut steps = Vec::new();
        for _ in 0..count {
            let listened = mask
                .listen(
                    *sequence,
                    probability,
                    Some(evidence),
                    speaking,
                    *sequence * 10_000,
                )
                .unwrap();
            if listened.step != Step::Carry {
                steps.push((*sequence, listened.step));
            }
            *sequence += 1;
        }
        steps
    }

    fn disposition(mask: &PlaybackMask, trigger: u64) -> Option<Disposition> {
        PlaybackMask::heard_disposition(mask.duplex.as_ref().unwrap(), trigger - 5, trigger)
    }

    #[test]
    fn the_duplex_mask_scores_the_lamp_as_silence_and_a_person_as_speech() {
        let mut mask = PlaybackMask::duplex();
        assert!(mask.is_duplex() && mask.enabled());
        let own = mask
            .listen(1, 0.99, Some(&evidence(0, OWN)), true, 10_000)
            .unwrap();
        assert_eq!((own.heard, own.score), (Some(Heard::Own), 0.0));
        let over = mask
            .listen(2, 0.55, Some(&evidence(0, OVER)), true, 2 * 10_000)
            .unwrap();
        assert_eq!((over.heard, over.score), (Some(Heard::Over), 1.0));
        let clear = mask
            .listen(3, 0.62, Some(&evidence(40, None)), false, 3 * 10_000)
            .unwrap();
        assert_eq!((clear.heard, clear.score), (Some(Heard::Clear), 0.62));
        // Heard blocks are never masked: the score is the protection.
        assert_eq!(mask.frame(), Disposition::Pass);
        // The plain mask hears nothing and changes no score.
        let mut plain = PlaybackMask::new(TurnTaking::V1Baseline);
        let unjudged = plain
            .listen(1, 0.99, Some(&evidence(0, OVER)), true, 10_000)
            .unwrap();
        assert_eq!(
            (unjudged.heard, unjudged.score, unjudged.step),
            (None, 0.99, Step::Carry)
        );
        assert!(!plain.yielded(10_000) && plain.duplex_summary().is_none());
    }

    #[test]
    fn without_evidence_the_duplex_mask_is_the_conservative_mask() {
        let mut mask = PlaybackMask::duplex();
        let blind = mask.listen(1, 0.99, None, true, 10_000).unwrap();
        assert_eq!(
            (blind.heard, blind.score, blind.step),
            (None, 0.99, Step::Carry)
        );
        assert!(mask.hearing().is_none(), "the baseline rules decide");
        // Evidence for the next block and the mask hears again.
        mask.listen(2, 0.99, Some(&evidence(0, OWN)), true, 2 * 10_000)
            .unwrap();
        assert!(mask.hearing().is_some());
        mask.listen(3, 0.99, None, true, 3 * 10_000).unwrap();
        assert!(mask.hearing().is_none());
    }

    #[test]
    fn a_candidate_that_began_over_the_lamp_waits_and_passes_when_the_person_goes_on() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        assert_eq!(
            hear(&mut mask, &mut sequence, 10, 0.9, &evidence(0, OWN), true),
            []
        );
        let steps = hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
        assert_eq!(steps, [(16, Step::Yield)]);
        assert!(mask.yielded((sequence - 1) * 10_000));
        assert_eq!(
            disposition(&mask, 16).unwrap(),
            Disposition::AwaitWriteEvidence
        );
        // The lamp's sound dies away with the person still speaking.
        assert_eq!(
            hear(&mut mask, &mut sequence, 30, 0.9, &evidence(10, None), true),
            []
        );
        assert_eq!(
            disposition(&mask, 16).unwrap(),
            Disposition::AwaitWriteEvidence
        );
        let steps = hear(&mut mask, &mut sequence, 6, 0.9, &evidence(40, None), true);
        assert_eq!(steps, [(52, Step::Commit)]);
        assert!(!mask.yielded((sequence - 1) * 10_000));
        assert_eq!(disposition(&mask, 16).unwrap(), Disposition::Pass);
        assert_eq!(mask.duplex_summary(), Some(((1, 1, 0), false)));
    }

    #[test]
    fn a_candidate_over_the_lamp_is_dropped_when_nobody_goes_on() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
        assert_eq!(
            disposition(&mask, 6).unwrap(),
            Disposition::AwaitWriteEvidence
        );
        hear(&mut mask, &mut sequence, 30, 0.1, &evidence(10, None), true);
        let steps = hear(&mut mask, &mut sequence, 15, 0.1, &evidence(40, None), true);
        assert_eq!(steps, [(51, Step::Resume)]);
        assert_eq!(disposition(&mask, 6).unwrap(), Disposition::Suppress);
        assert_eq!(mask.duplex_summary(), Some(((1, 0, 1), false)));
    }

    #[test]
    fn a_reply_that_twice_yields_to_nothing_stops_yielding_until_it_closes() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        for _ in 0..FRUITLESS_YIELDS {
            let steps = hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
            assert_eq!(steps.len(), 1);
            hear(&mut mask, &mut sequence, 30, 0.1, &evidence(10, None), true);
            let steps = hear(&mut mask, &mut sequence, 15, 0.1, &evidence(40, None), true);
            assert_eq!(steps.last().unwrap().1, Step::Resume);
        }
        assert!(mask.duplex_summary().unwrap().1);
        // The same evidence again: no yield, and the detector is told nothing.
        let before = sequence;
        assert_eq!(
            hear(&mut mask, &mut sequence, 12, 0.9, &evidence(0, OVER), true),
            []
        );
        let listened = mask
            .listen(
                sequence,
                0.9,
                Some(&evidence(0, OVER)),
                true,
                sequence * 10_000,
            )
            .unwrap();
        assert_eq!((listened.heard, listened.score), (Some(Heard::Over), 0.0));
        assert!(sequence > before && !mask.yielded((sequence - 1) * 10_000));
        // The next reply starts clean.
        mask.reply_closed();
        sequence += 1;
        let steps = hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].1, Step::Yield);
    }

    #[test]
    fn a_person_over_a_sound_that_is_no_reply_is_an_ordinary_candidate() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        // Nothing to yield: no reply has audio waiting.
        assert_eq!(
            hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), false),
            []
        );
        assert_eq!(disposition(&mask, 6).unwrap(), Disposition::Pass);
        // And in the clear a candidate has nothing to wait for.
        hear(&mut mask, &mut sequence, 6, 0.9, &evidence(40, None), false);
        assert_eq!(disposition(&mask, 12).unwrap(), Disposition::Pass);
    }

    #[test]
    fn a_gap_in_capture_forgets_what_was_heard_and_any_yield() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
        assert!(mask.yielded((sequence - 1) * 10_000));
        // Block 7 is missing.
        mask.listen(8, 0.9, Some(&evidence(0, OWN)), true, 8 * 10_000)
            .unwrap();
        assert!(!mask.yielded(8 * 10_000));
        assert_eq!(disposition(&mask, 6), None, "the baseline rules decide");
    }

    #[test]
    fn a_yield_whose_hearing_has_stopped_withholds_nothing() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
        let heard_at = 6 * 10_000;
        assert!(mask.yielded(heard_at));
        assert!(mask.yielded(heard_at + HEARING_STALE_US));
        // A muted or stalled microphone: the reply is not held back.
        assert!(!mask.yielded(heard_at + HEARING_STALE_US + 1));
        // Hearing returns in order and the yield is still to be settled.
        mask.listen(7, 0.9, Some(&evidence(1, None)), true, heard_at + 300_000)
            .unwrap();
        assert!(mask.yielded(heard_at + 300_000));
        assert!(mask.listen(8, 0.9, None, true, heard_at).is_err());
    }

    #[test]
    fn a_reply_that_closes_during_a_yield_leaves_its_candidate_to_be_settled() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
        // The reply had nothing more to say and ends while the lamp waits.
        mask.reply_closed();
        assert_eq!(
            disposition(&mask, 6).unwrap(),
            Disposition::AwaitWriteEvidence
        );
        hear(
            &mut mask,
            &mut sequence,
            30,
            0.9,
            &evidence(10, None),
            false,
        );
        let steps = hear(&mut mask, &mut sequence, 6, 0.9, &evidence(40, None), false);
        assert_eq!(steps, [(42, Step::Commit)]);
        assert_eq!(disposition(&mask, 6).unwrap(), Disposition::Pass);
    }

    #[test]
    fn another_capture_stream_forgets_what_was_heard_and_any_yield() {
        let mut mask = PlaybackMask::duplex();
        let mut sequence = 1;
        hear(&mut mask, &mut sequence, 6, 0.9, &evidence(0, OVER), true);
        assert!(mask.yielded(60_000));
        mask.clear_read_lineage();
        assert!(!mask.yielded(60_000));
        assert!(mask.hearing().is_none(), "the baseline rules decide");
        // The new stream may number its blocks from anywhere.
        let first = mask
            .listen(1, 0.9, Some(&evidence(0, OWN)), true, 70_000)
            .unwrap();
        assert_eq!(first.heard, Some(Heard::Own));
    }
}
