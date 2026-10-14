//! The end sent early (`--early-end-ms`; docs/faster-answers.md, way 2).
//!
//! The provider cannot be asked for an answer without being told that the
//! person's turn is over, and it then takes a second or more to answer.
//! The lamp waits for quiet before it tells it: 600 ms on lamp-4ace, or
//! the end-of-turn model's verdict. With this the request is ended to the
//! provider after a shorter quiet, while the lamp goes on listening exactly
//! as before. Its own end of speech, the confirming moment, is still what
//! lets the answer be played; until then the answer's sound is taken in and
//! held. If the person speaks again first, the request is given up as a
//! continued utterance is, and what they go on to say is its successor.
//!
//! This is the bookkeeping alone, with every time given to it: which
//! request was ended early and when, the blocks heard since (the opening of
//! a successor, if one comes), and the provider's words for the answer
//! while they are not yet the lamp's to say. The coordinator acts on it.
use crate::activity::{ObservedAudio, PRE_ROLL_BLOCKS, START_BLOCKS, START_PROBABILITY};
use std::collections::VecDeque;

/// Pieces of the provider's text held for one answer, at most. The window
/// between the end sent early and the confirming moment is under 1.5 s.
const HELD_WORDS: usize = 64;

/// One request's end, sent early: what is kept until the lamp is sure.
#[derive(Debug)]
pub(super) struct Sent {
    pub(super) turn: u64,
    /// When the end was handed on, by the coordinator's clock.
    pub(super) at_us: u64,
    /// Capture time of the block that made the quiet long enough.
    pub(super) last_block_host_read_us: u64,
    /// Blocks heard since, newest last, at most a pre-roll's worth.
    heard: VecDeque<ObservedAudio>,
    /// Blocks in a row, to the newest, scoring as the start of speech.
    speaking: u16,
    /// The provider's words for the answer, not yet the lamp's.
    words: VecDeque<(String, bool)>,
    /// The answer opened with no word in it (`Some(false)`) or with a
    /// function's name said aloud (`Some(true)`): it is to end unheard,
    /// once the lamp is sure that the person has finished.
    nothing_said: Option<bool>,
}

impl Sent {
    /// The provider's words held for the answer, oldest first.
    pub(super) fn take_words(&mut self) -> VecDeque<(String, bool)> {
        std::mem::take(&mut self.words)
    }
    pub(super) fn nothing_said(&self) -> Option<bool> {
        self.nothing_said
    }
    /// How long the request has been ended, at `now_us`.
    pub(super) fn ago_us(&self, now_us: u64) -> u64 {
        now_us.saturating_sub(self.at_us)
    }
}

/// What a block heard after the end was sent comes to.
#[derive(Debug)]
pub(super) enum Heard {
    /// Quiet, or not yet someone speaking again: kept.
    Kept,
    /// Someone is speaking again, by the detector's own bar for the start
    /// of an utterance. The request is to be given up and continued, these
    /// blocks the opening of its successor.
    Resumed {
        given_up: Sent,
        opening: Vec<ObservedAudio>,
    },
}

#[derive(Debug)]
pub(super) struct EarlyEnd {
    blocks: u16,
    sent: Option<Sent>,
}

impl EarlyEnd {
    /// Send the end after this many blocks of quiet.
    pub(super) fn new(blocks: u16) -> Self {
        Self { blocks, sent: None }
    }

    pub(super) fn blocks(&self) -> u16 {
        self.blocks
    }

    /// Whether `turn`'s end is due now: the pause it is in has just reached
    /// the setting, and its end has not been sent. Once a pause, so a pause
    /// in which it was held back is not asked about again.
    pub(super) fn due(&self, turn: u64, pause_blocks: u16) -> bool {
        pause_blocks == self.blocks && !self.holds(turn)
    }

    /// `turn`'s end was handed on at `at_us`.
    pub(super) fn send(&mut self, turn: u64, at_us: u64, last_block_host_read_us: u64) {
        self.sent = Some(Sent {
            turn,
            at_us,
            last_block_host_read_us,
            heard: VecDeque::with_capacity(PRE_ROLL_BLOCKS),
            speaking: 0,
            words: VecDeque::new(),
            nothing_said: None,
        });
    }

    /// Whether `turn`'s end was sent and the lamp is not yet sure.
    pub(super) fn holds(&self, turn: u64) -> bool {
        self.sent.as_ref().is_some_and(|sent| sent.turn == turn)
    }

    #[cfg(test)]
    pub(super) fn sent(&self) -> Option<&Sent> {
        self.sent.as_ref()
    }

    /// A block heard after the end was sent, scored as the detector was
    /// given it. Nothing of it goes to the provider for the request ended.
    pub(super) fn hear(&mut self, audio: ObservedAudio, probability: f32) -> Heard {
        let Some(sent) = self.sent.as_mut() else {
            return Heard::Kept;
        };
        if sent.heard.len() == PRE_ROLL_BLOCKS {
            sent.heard.pop_front();
        }
        sent.heard.push_back(audio);
        sent.speaking = if probability >= START_PROBABILITY {
            sent.speaking.saturating_add(1)
        } else {
            0
        };
        if sent.speaking < START_BLOCKS {
            return Heard::Kept;
        }
        let mut given_up = self.sent.take().expect("sent above");
        let opening = given_up.heard.drain(..).collect();
        Heard::Resumed { given_up, opening }
    }

    /// The lamp is sure `turn` has finished: what was kept for it.
    pub(super) fn confirm(&mut self, turn: u64) -> Option<Sent> {
        if self.holds(turn) {
            self.sent.take()
        } else {
            None
        }
    }

    /// Whatever was kept goes: the request it was for is over, or its
    /// utterance ended without a word more for it.
    pub(super) fn forget(&mut self) -> Option<Sent> {
        self.sent.take()
    }

    /// Only the request in hand can be held. One that has gone, by any
    /// path, takes what was kept for it.
    pub(super) fn keep_current(&mut self, turn: Option<u64>) {
        if self
            .sent
            .as_ref()
            .is_some_and(|sent| Some(sent.turn) != turn)
        {
            self.sent = None;
        }
    }

    /// The provider's words for `turn`'s answer, held while the lamp is not
    /// sure. False if they are not held: they are the lamp's already.
    pub(super) fn hold_words(&mut self, turn: u64, text: &str, finished: bool) -> bool {
        let Some(sent) = self.sent.as_mut().filter(|sent| sent.turn == turn) else {
            return false;
        };
        if sent.words.len() == HELD_WORDS {
            sent.words.pop_front();
        }
        sent.words.push_back((text.to_owned(), finished));
        true
    }

    /// `turn`'s answer opened with nothing said (`named`: a function's name
    /// said aloud). It ends unheard once the lamp is sure; false if the
    /// lamp is sure already.
    pub(super) fn nothing_said(&mut self, turn: u64, named: bool) -> bool {
        let Some(sent) = self.sent.as_mut().filter(|sent| sent.turn == turn) else {
            return false;
        };
        sent.nothing_said.get_or_insert(named);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{Activity, TurnDetector};
    use lamp_audio::playout::StartGate;

    const SPEECH: f32 = 0.95;
    const QUIET: f32 = 0.05;

    fn block(sequence: u64) -> ObservedAudio {
        ObservedAudio {
            sequence,
            captured_at_us: sequence * 10_000,
            samples: [sequence as i16; 160],
        }
    }

    #[test]
    fn the_end_is_due_once_a_pause_and_never_again_for_a_request_it_was_sent_for() {
        let mut early = EarlyEnd::new(20);
        assert!(!early.due(1, 19));
        assert!(early.due(1, 20));
        assert!(!early.due(1, 21));
        early.send(1, 200_000, 190_000);
        assert!(early.holds(1));
        assert!(!early.due(1, 20));
        // Its successor's own pause.
        assert!(early.due(2, 20));
    }

    #[test]
    fn a_click_or_a_breath_is_kept_and_six_blocks_of_speech_give_the_request_up() {
        let mut early = EarlyEnd::new(20);
        early.send(1, 0, 0);
        // A breath that the detector would not take as the start of speech.
        for sequence in 1..=5 {
            assert!(matches!(early.hear(block(sequence), SPEECH), Heard::Kept));
        }
        assert!(matches!(early.hear(block(6), QUIET), Heard::Kept));
        assert!(matches!(early.hear(block(7), 0.6), Heard::Kept));
        assert!(early.holds(1));
        // Then someone speaking: the sixth block in a row gives it up.
        for sequence in 8..=12 {
            assert!(matches!(early.hear(block(sequence), SPEECH), Heard::Kept));
        }
        let Heard::Resumed { given_up, opening } = early.hear(block(13), SPEECH) else {
            panic!("six blocks of speech did not give the request up");
        };
        assert_eq!(given_up.turn, 1);
        // Everything heard since the end, in order.
        assert_eq!(
            opening.iter().map(|b| b.sequence).collect::<Vec<_>>(),
            (1..=13).collect::<Vec<_>>()
        );
        assert!(!early.holds(1));
    }

    #[test]
    fn the_opening_of_a_successor_is_at_most_a_pre_roll() {
        let mut early = EarlyEnd::new(20);
        early.send(1, 0, 0);
        for sequence in 1..=100 {
            early.hear(block(sequence), QUIET);
        }
        for sequence in 101..106 {
            early.hear(block(sequence), SPEECH);
        }
        let Heard::Resumed { opening, .. } = early.hear(block(106), SPEECH) else {
            panic!("not resumed");
        };
        assert_eq!(opening.len(), PRE_ROLL_BLOCKS);
        assert_eq!(opening.first().unwrap().sequence, 106 - 29);
        assert_eq!(opening.last().unwrap().sequence, 106);
    }

    #[test]
    fn words_are_held_until_the_lamp_is_sure_and_go_with_a_request_given_up() {
        let mut early = EarlyEnd::new(20);
        assert!(!early.hold_words(1, "Yes?", true));
        early.send(1, 0, 0);
        assert!(early.hold_words(1, "It's ", false));
        assert!(early.hold_words(1, "three.", true));
        assert!(!early.hold_words(2, "Other", true));
        assert!(early.nothing_said(1, false));
        let mut sent = early.confirm(1).unwrap();
        assert_eq!(sent.nothing_said(), Some(false));
        assert_eq!(
            sent.take_words(),
            [("It's ".to_owned(), false), ("three.".to_owned(), true)]
        );
        assert!(early.confirm(1).is_none());
        // A request that is replaced takes its words with it.
        early.send(2, 0, 0);
        early.hold_words(2, "Hi", true);
        early.keep_current(Some(3));
        assert!(early.sent().is_none());
        for n in 0..(HELD_WORDS + 5) {
            early.send(4, 0, 0);
            early.hold_words(4, &n.to_string(), false);
        }
        early.send(4, 0, 0);
        for n in 0..(HELD_WORDS + 5) {
            early.hold_words(4, &n.to_string(), false);
        }
        let words = early.forget().unwrap().take_words();
        assert_eq!(words.len(), HELD_WORDS);
        assert_eq!(words.front().unwrap().0, "5");
    }

    /// The path from a person's last sound to the first block handed to the
    /// speaker, with every clock injected: the real detector (600 ms of
    /// quiet, as lamp-4ace), the real bookkeeping above, and the real
    /// playout start gate (350 ms, as lamp-4ace). The service's first sound
    /// comes `service_ms` after the end reaches it, then 2 s of answer in
    /// 40 ms packets as fast as it plays; the coordinator's own rule for a
    /// first block is that the lamp is sure the person has finished and
    /// the start gate is open. Returns when the end was sent, when the lamp
    /// was sure, and when the first block was handed over, each in ms after
    /// the end of the last block of speech.
    fn path(early_ms: Option<u16>, service_ms: u64) -> (u64, u64, u64) {
        const LAST_SPEECH: u64 = 150;
        let mut detector = TurnDetector::default().with_end_silence(60).unwrap();
        let mut early = early_ms.map(|ms| EarlyEnd::new(ms / 10));
        let mut gate = StartGate::new(350_000);
        let (mut end_sent, mut sure, mut first) = (None, None, None);
        let mut buffered = 0usize;
        let mut arrived = 0usize;
        let block_end_us = |sequence: u64| sequence * 10_000;
        let last_sound_us = block_end_us(LAST_SPEECH);
        for sequence in 1..1_000u64 {
            let now = block_end_us(sequence);
            let probability = if (50..=LAST_SPEECH).contains(&sequence) {
                SPEECH
            } else {
                QUIET
            };
            match detector.push(block(sequence), probability) {
                Activity::Continue(audio) => {
                    if let Some(early) = early.as_mut() {
                        if early.holds(1) {
                            early.hear(audio, probability);
                        } else if early.due(1, detector.pause_blocks()) {
                            early.send(1, now, now);
                            end_sent = Some(now);
                        }
                    }
                }
                Activity::End(_) => {
                    if early.as_mut().and_then(|e| e.confirm(1)).is_none() {
                        end_sent = Some(now);
                    }
                    sure = Some(now);
                }
                _ => {}
            }
            // The service's packets: 960 frames of 24 kHz, every 40 ms.
            if let Some(sent) = end_sent {
                let first_at = sent + service_ms * 1_000;
                while arrived < 50 && first_at + arrived as u64 * 40_000 <= now {
                    gate.arrival(first_at + arrived as u64 * 40_000, 960);
                    buffered += 960;
                    arrived += 1;
                }
            }
            if first.is_none()
                && sure.is_some()
                && buffered > 240
                && gate.open(now, buffered, false)
            {
                first = Some(now);
            }
            if first.is_some() {
                break;
            }
        }
        let ms = |at: Option<u64>| (at.expect("reached") - last_sound_us) / 1_000;
        (ms(end_sent), ms(sure), ms(first))
    }

    #[test]
    fn the_answer_begins_sooner_by_the_time_saved_and_never_before_the_lamp_is_sure() {
        // The lamp's median service time, 1.15 s. In this model the start
        // gate holds the first sound 160 ms: 350 ms are covered by the time
        // waited and the sound in hand together.
        const HOLD: u64 = 160;
        let off = path(None, 1_150);
        let on = path(Some(200), 1_150);
        assert_eq!(off, (600, 600, 600 + 1_150 + HOLD));
        assert_eq!(on, (200, 600, 200 + 1_150 + HOLD));
        assert_eq!(off.2 - on.2, 400);
        // The quickest the lamp has seen, about 0.85 s.
        let (off, on) = (path(None, 850), path(Some(200), 850));
        assert_eq!(off.2 - on.2, 400);
        // An answer that is ready before the lamp is sure waits for it, and
        // begins at that moment.
        assert_eq!(path(Some(200), 0), (200, 600, 600));
        assert_eq!(path(Some(200), 100), (200, 600, 600));
        // One that is not yet ready then starts on the gate alone.
        assert_eq!(path(Some(200), 300), (200, 600, 200 + 300 + HOLD));
        // Whatever the service's time and the setting: never before the
        // lamp is sure, never later than without the option, and sooner by
        // all the time saved once the service takes longer than the gap.
        for service_ms in [0, 50, 150, 250, 400, 850, 1_150, 1_500] {
            for early_ms in [100, 200, 300, 400, 500] {
                let (sent, sure, first) = path(Some(early_ms), service_ms);
                assert_eq!((sent, sure), (u64::from(early_ms), 600));
                assert_eq!(
                    first,
                    sure.max(sent + service_ms + HOLD),
                    "{early_ms} {service_ms}"
                );
                let (_, _, without) = path(None, service_ms);
                assert_eq!(without, 600 + service_ms + HOLD);
                assert!(first <= without, "{early_ms} {service_ms}");
            }
        }
    }
}
