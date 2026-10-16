//! The self-test's tone in the service (`crate::speaker_check`, #812):
//! requests taken from `selftest/speaker/request`, the tone played as a
//! turn of the lamp's own with no provider, the voice microphone's evidence
//! kept around it, and the answer on `selftest/speaker`.
//!
//! Off unless `LAMP_SELFTEST_SPEAKER=on`. It is a check, not the
//! conversation: a request that cannot be read or an answer that cannot be
//! written is said once in the log and switches the check off for the rest
//! of the service; the voice goes on. The turn shows no face and moves
//! nothing: the loop treats it as no turn for the ring, the expression, the
//! body, the room and the bus's voice state.
use super::*;
use crate::speaker_check::{self, Block, Heard};
use lamp_bus::{Bus, Message, Publisher, Subscriber};
use lamp_msgs::{SpeakerCheck, SpeakerCheckRequest};

/// The switch.
pub(super) const ENVIRONMENT: &str = "LAMP_SELFTEST_SPEAKER";
/// Blocks kept before a tone, for its baseline: a little over the half
/// second it needs.
const KEPT_BLOCKS: usize = 80;

/// One tone being played and listened to.
struct Playing {
    id: String,
    turn: TurnOwner,
    /// When the speaker first accepted frames of it; the delay is from here.
    started_us: Option<u64>,
    /// Whether it was played to its end: its final sample retired.
    whole: bool,
    /// Once it has settled, the answer is given at this time.
    answer_at_us: Option<u64>,
}

pub(super) struct SpeakerTone {
    requests: Subscriber,
    answers: Publisher,
    blocks: VecDeque<Block>,
    playing: Option<Playing>,
    /// Switched off after a bus failure of its own.
    off: bool,
}

impl SpeakerTone {
    /// The tone's requests and answers, if the switch says `on` and the
    /// bus can be used.
    pub(super) fn from_environment() -> Option<Self> {
        if std::env::var(ENVIRONMENT).ok().as_deref() != Some("on") {
            return None;
        }
        match Bus::from_environment().and_then(|bus| Self::on(&bus)) {
            Ok(tone) => Some(tone),
            Err(error) => {
                eprintln!("lamp-voice: the self-test's tone is not offered: {error}");
                None
            }
        }
    }

    pub(super) fn on(bus: &Bus) -> io::Result<Self> {
        Ok(Self {
            requests: bus.subscribe_typed(
                SpeakerCheckRequest::TOPIC,
                "voice",
                SpeakerCheckRequest::TYPE,
            )?,
            answers: bus.publisher_declared(
                SpeakerCheck::TOPIC,
                &SpeakerCheck::declaration(),
                "voice",
            )?,
            blocks: VecDeque::with_capacity(KEPT_BLOCKS + 100),
            playing: None,
            off: false,
        })
    }

    /// Each capture block's levels, kept while nothing plays and for as
    /// long as a tone is listened to.
    pub(super) fn observe(&mut self, frame: &MicrophoneFrame) {
        let Some(echo) = &frame.echo else {
            return;
        };
        self.blocks
            .push_back(Block::of(frame.read_completed_at_us, echo));
        let keep = if self.playing.is_some() {
            KEPT_BLOCKS + 100
        } else {
            KEPT_BLOCKS
        };
        while self.blocks.len() > keep {
            self.blocks.pop_front();
        }
    }

    /// The next request, if one is waiting and none is being played. One
    /// that is not for the factory or on demand is answered at once.
    pub(super) fn next_request(&mut self, at_us: u64) -> Option<String> {
        if self.off || self.playing.is_some() {
            return None;
        }
        let received = self.requests.recv(Duration::ZERO);
        self.take(received, at_us)
    }

    /// What one look at the requests gave.
    pub(super) fn take(
        &mut self,
        received: io::Result<Option<Message>>,
        at_us: u64,
    ) -> Option<String> {
        let message = match received {
            Ok(Some(message)) if !message.expired => message,
            Ok(_) => return None,
            Err(error) => {
                self.fail("its requests cannot be read", &error);
                return None;
            }
        };
        let request = match message.parse::<SpeakerCheckRequest>() {
            Ok(request) => request,
            Err(error) => {
                eprintln!("lamp-voice: a speaker check request that is not one: {error}");
                return None;
            }
        };
        if let Err(why) = request.check() {
            self.answer(&request.id, "not_checked", &why.0, None, at_us);
            return None;
        }
        (!self.off).then_some(request.id)
    }

    /// Says why the tone was not played for `id`.
    pub(super) fn refuse(&mut self, id: &str, why: &str, at_us: u64) {
        self.answer(id, "not_checked", why, None, at_us);
    }

    /// Whether a failure of its own has switched the check off.
    #[cfg(test)]
    pub(super) fn is_off(&self) -> bool {
        self.off
    }

    fn fail(&mut self, what: &str, error: &io::Error) {
        if !self.off {
            eprintln!(
                "lamp-voice: the self-test's speaker check is off for this service: {what}: {error}"
            );
        }
        self.off = true;
        self.playing = None;
    }

    /// The tone's turn has begun.
    pub(super) fn began(&mut self, id: String, turn: TurnOwner) {
        self.playing = Some(Playing {
            id,
            turn,
            started_us: None,
            whole: false,
            answer_at_us: None,
        });
    }

    /// Call when a turn settles, before it is let go: the tone's own turn
    /// is then known to have played to its end. A turn cancelled is let go
    /// without this, and its tone is said cut short.
    pub(super) fn finished(&mut self, current: &Reply) {
        if let Some(playing) = self.playing.as_mut()
            && playing.turn == current.owner
        {
            playing.started_us = current.notice.first_accepted_at_us();
            playing.whole = current.notice.retired();
        }
    }

    /// Call each pass with the reply, if any. Notes when the tone's first
    /// chunk left, and when it is over; answers once the microphone has had
    /// its time to hear it.
    pub(super) fn tick(&mut self, reply: Option<&Reply>, at_us: u64) {
        let Some(playing) = self.playing.as_mut() else {
            return;
        };
        let ours = reply.filter(|current| current.owner == playing.turn);
        if let Some(current) = ours {
            playing.started_us = current.notice.first_accepted_at_us();
            playing.whole = current.notice.retired();
            return;
        }
        // The turn is over. Only a tone played to its end is judged: one cut
        // short (a person speaking, a hand, the microphone closing) says so.
        let (Some(started_us), true) = (playing.started_us, playing.whole) else {
            let id = playing.id.clone();
            let why = if playing.started_us.is_none() {
                "the tone's turn ended before the speaker took any of it"
            } else {
                "the tone was cut short before its end: its turn was cancelled"
            };
            self.playing = None;
            self.answer(&id, "not_checked", why, None, at_us);
            return;
        };
        let due = *playing
            .answer_at_us
            .get_or_insert(started_us + speaker_check::LISTEN_US);
        if at_us < due {
            return;
        }
        let id = playing.id.clone();
        let blocks: Vec<Block> = self.blocks.iter().copied().collect();
        self.playing = None;
        let heard = speaker_check::heard(&blocks, started_us);
        let (verdict, reason) = verdict(&heard);
        self.answer(&id, verdict, &reason, Some(&heard), at_us);
    }

    fn answer(&mut self, id: &str, verdict: &str, reason: &str, heard: Option<&Heard>, at_us: u64) {
        let (rise_db, delay_ms, cancelled_db) = match heard {
            Some(Heard::Yes {
                rise_db,
                delay_ms,
                cancelled_db,
            }) => (Some(*rise_db), Some(*delay_ms), Some(*cancelled_db)),
            _ => (None, None, None),
        };
        let answer = SpeakerCheck {
            id: id.to_owned(),
            verdict: verdict.to_owned(),
            reason: reason.to_owned(),
            rise_db,
            delay_ms,
            cancelled_db,
        };
        if let Err(error) = self.answers.publish_json(at_us, &answer) {
            self.fail("its answer cannot be written", &error);
        }
    }
}

/// The self-test's verdict on what the voice microphone heard.
pub(super) fn verdict(heard: &Heard) -> (&'static str, String) {
    match heard {
        Heard::Yes { cancelled_db, .. } if *cancelled_db < speaker_check::CANCELLED_DB => (
            "fail",
            format!(
                "the tone was heard but the echo canceller took only {cancelled_db:.1} dB off it"
            ),
        ),
        Heard::Yes { .. } => ("pass", String::new()),
        Heard::No {
            baseline_db,
            loudest_db,
        } => (
            "fail",
            format!(
                "the voice microphone did not hear the tone: {loudest_db:.0} dB at most over a room at {baseline_db:.0} dB"
            ),
        ),
        Heard::NoBaseline => (
            "not_checked",
            "too little of the microphone's evidence before the tone".to_owned(),
        ),
    }
}

/// Begins the tone: the ring is told to show nothing before the turn
/// exists, so no frame of the tone's turn is ever prepared, not even in
/// the pass that admits it; then the turn is opened.
pub(super) fn begin_tone(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    choreography: Option<&mut RingChoreographer>,
    playout_start: PlayoutStart,
    trace: &mut SessionState,
    publish: impl FnMut(Snapshot) -> io::Result<()>,
) -> Result<(Snapshot, TurnOwner)> {
    if let Some(choreography) = choreography {
        choreography.set_unshown(true);
    }
    commit_tone_turn(owner, reply, 0, playout_start, trace, publish)
}

/// Opens the tone's turn: the lamp's own, with no provider request, ended
/// as soon as it began, its PCM the tone. Like a written turn
/// (`commit_written_turn`) but nothing is sent to the provider.
pub(super) fn commit_tone_turn(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    provider_session: u64,
    playout_start: PlayoutStart,
    trace: &mut SessionState,
    publish: impl FnMut(Snapshot) -> io::Result<()>,
) -> Result<(Snapshot, TurnOwner)> {
    commit_sound_turn(
        owner,
        reply,
        NoticeDelivery::tone(speaker_check::tone()),
        "selftest_tone",
        "the self-test's tone: nothing heard, nothing sent to the provider",
        provider_session,
        playout_start,
        trace,
        publish,
    )
}

/// Opens a turn that is only a short sound of the lamp's own (`sound`):
/// the self-test's tone, or the head tap's "hm?" (#711). No provider
/// request; ended as soon as it began.
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_sound_turn(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    sound: NoticeDelivery,
    basis: &'static str,
    boundary: &'static str,
    provider_session: u64,
    playout_start: PlayoutStart,
    trace: &mut SessionState,
    mut publish: impl FnMut(Snapshot) -> io::Result<()>,
) -> Result<(Snapshot, TurnOwner)> {
    let turn = owner.admit(now(), AdmittedInput::NewTurn)?;
    let plan = owner.plan_output(now(), turn, OutputKind::Speech, None)?;
    let mut snapshot = owner.snapshot(now())?;
    publish(snapshot)?;
    let admitted_at_us = monotonic_us();
    trace
        .receipts
        .admitted(turn, snapshot.issued_at().as_micros())?;
    record(
        trace,
        json!({"kind":"input_admitted","owner":turn,"turn":turn.turn(),"at_us":admitted_at_us,"authority_issued_at_us":snapshot.issued_at().as_micros(),
            "admission_basis":basis,"provider_session":null,
            "boundary":boundary}),
    )?;
    let current = Reply {
        owner: turn,
        // Complete from the start: no request was or will be made, so no
        // link change can abandon it and no failure notice can follow it.
        cloud: CloudRequest {
            session: provider_session,
            start_submitted: true,
            started: false,
            input_end_submitted: true,
            input_abandoned: false,
            outcome: Some(RequestOutcome::Complete),
            movable: false,
        },
        local_input_ended: true,
        plan,
        playback: None,
        pcm: VecDeque::new(),
        generation_done: true,
        provider_idle: true,
        input_sequence: 0,
        had_audio: false,
        voice_level: 0,
        final_chunk: None,
        playback_gaps: 0,
        notice: sound,
        reminder: None,
        last_provider_audio_at_us: None,
        start: playout_start.gate(),
        need: StartNeed::default(),
    };
    owner.input_ended(now(), current.owner)?;
    snapshot = owner.snapshot(now())?;
    publish(snapshot)?;
    *reply = Some(current);
    Ok((snapshot, turn))
}

/// What the voice knows now, for [`speaker_check::may_play`].
pub(super) fn local_minutes() -> Option<u32> {
    let offset = crate::local_time::offset_here()?;
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let local = utc + i64::from(offset);
    Some((local.rem_euclid(86_400) / 60) as u32)
}
