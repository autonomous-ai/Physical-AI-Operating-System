//! The self-test's tone in the service (`crate::speaker_check`, #812):
//! requests taken from `selftest/speaker/request`, the tone played as a
//! turn of the lamp's own with no provider, the voice microphone's evidence
//! kept around it, and the answer on `selftest/speaker`.
//!
//! Off unless `LAMP_SELFTEST_SPEAKER=on`. The turn shows no face and moves
//! nothing: the loop treats it as no turn for the ring, the expression, the
//! body, the room and the bus's voice state.
use super::*;
use crate::speaker_check::{self, Block, Heard};
use lamp_bus::{Bus, Publisher, Subscriber};
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
    /// When its first chunk went to the speaker; the delay is from here.
    started_us: Option<u64>,
    /// Once it has settled, the answer is given at this time.
    answer_at_us: Option<u64>,
}

pub(super) struct SpeakerTone {
    requests: Subscriber,
    answers: Publisher,
    blocks: VecDeque<Block>,
    playing: Option<Playing>,
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
    pub(super) fn next_request(&mut self, at_us: u64) -> io::Result<Option<String>> {
        if self.playing.is_some() {
            return Ok(None);
        }
        let Some(message) = self.requests.recv(Duration::ZERO)? else {
            return Ok(None);
        };
        if message.expired {
            return Ok(None);
        }
        let request = match message.parse::<SpeakerCheckRequest>() {
            Ok(request) => request,
            Err(error) => {
                eprintln!("lamp-voice: a speaker check request that is not one: {error}");
                return Ok(None);
            }
        };
        if let Err(why) = request.check() {
            self.answer(&request.id, "not_checked", &why.0, None, at_us)?;
            return Ok(None);
        }
        Ok(Some(request.id))
    }

    /// Says why the tone was not played for `id`.
    pub(super) fn refuse(&mut self, id: &str, why: &str, at_us: u64) -> io::Result<()> {
        self.answer(id, "not_checked", why, None, at_us)
    }

    /// The tone's turn has begun.
    pub(super) fn began(&mut self, id: String, turn: TurnOwner) {
        self.playing = Some(Playing {
            id,
            turn,
            started_us: None,
            answer_at_us: None,
        });
    }

    /// Call each pass with the reply, if any. Notes when the tone's first
    /// chunk left, and when it is over; answers once the microphone has had
    /// its time to hear it.
    pub(super) fn tick(&mut self, reply: Option<&Reply>, at_us: u64) -> io::Result<()> {
        let Some(playing) = self.playing.as_mut() else {
            return Ok(());
        };
        let ours = reply.filter(|current| current.owner == playing.turn);
        if let Some(current) = ours {
            if playing.started_us.is_none() {
                playing.started_us = current.notice.attempted_at_us();
            }
            return Ok(());
        }
        // The turn is over: settled, or cancelled before it played.
        let Some(started_us) = playing.started_us else {
            let id = playing.id.clone();
            self.playing = None;
            return self.answer(
                &id,
                "not_checked",
                "the tone's turn ended before it was played",
                None,
                at_us,
            );
        };
        let due = *playing
            .answer_at_us
            .get_or_insert(started_us + speaker_check::LISTEN_US);
        if at_us < due {
            return Ok(());
        }
        let id = playing.id.clone();
        let blocks: Vec<Block> = self.blocks.iter().copied().collect();
        self.playing = None;
        let heard = speaker_check::heard(&blocks, started_us);
        let (verdict, reason) = verdict(&heard);
        self.answer(&id, verdict, &reason, Some(&heard), at_us)
    }

    fn answer(
        &mut self,
        id: &str,
        verdict: &str,
        reason: &str,
        heard: Option<&Heard>,
        at_us: u64,
    ) -> io::Result<()> {
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
        self.answers.publish_json(at_us, &answer)?;
        Ok(())
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

/// Opens the tone's turn: the lamp's own, with no provider request, ended
/// as soon as it began, its PCM the tone. Like a written turn
/// (`commit_written_turn`) but nothing is sent to the provider.
pub(super) fn commit_tone_turn(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
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
            "admission_basis":"selftest_tone","provider_session":null,
            "boundary":"the self-test's tone: nothing heard, nothing sent to the provider"}),
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
        notice: NoticeDelivery::tone(speaker_check::tone()),
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
