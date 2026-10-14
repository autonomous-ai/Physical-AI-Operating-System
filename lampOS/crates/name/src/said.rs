//! When a run of scores is one hearing of the name.
//!
//! The models give a score every 80 ms, and a name that is said raises it
//! for several steps in a row. That is one hearing, said once, with the
//! highest score it reached. Nothing here is slow: a hearing is said when
//! the score falls again, or after [`LONGEST`] if it does not.
use lamp_msgs::NameHeard;
use std::time::{Duration, Instant};

/// A hearing is said after this long even if the score has not fallen.
pub const LONGEST: Duration = Duration::from_millis(800);
/// After a hearing, scores are passed over for this long: the same call
/// is not heard twice.
pub const HOLD_OFF: Duration = Duration::from_secs(1);

pub struct Hearings {
    least: f32,
    /// A hearing going on: when it began and the highest score so far.
    open: Option<(Instant, f32)>,
    quiet_until: Option<Instant>,
}

impl Hearings {
    /// Scores of `least` or more are the name being heard.
    pub fn new(least: f32) -> Self {
        Self {
            least,
            open: None,
            quiet_until: None,
        }
    }

    fn close(&mut self, now: Instant) -> Option<NameHeard> {
        let (began, score) = self.open.take()?;
        self.quiet_until = Some(now + HOLD_OFF);
        Some(NameHeard {
            score,
            ago_ms: u32::try_from(now.saturating_duration_since(began).as_millis())
                .unwrap_or(u32::MAX),
        })
    }

    /// A score that came at `now`. A hearing, if one is over with it.
    pub fn score(&mut self, score: f32, now: Instant) -> Option<NameHeard> {
        if self.quiet_until.is_some_and(|until| now < until) {
            return None;
        }
        match (&mut self.open, score >= self.least) {
            (Some((began, highest)), true) => {
                *highest = highest.max(score.min(1.0));
                (now.saturating_duration_since(*began) >= LONGEST)
                    .then(|| self.close(now))
                    .flatten()
            }
            (Some(_), false) => self.close(now),
            (None, true) => {
                self.open = Some((now, score.min(1.0)));
                None
            }
            (None, false) => None,
        }
    }

    /// The sound stopped coming, or some of it was lost: a hearing that
    /// is open is said as it stands, dated from when it began.
    pub fn cut(&mut self, now: Instant) -> Option<NameHeard> {
        self.close(now)
    }

    /// Time has passed with no score: a hearing that has gone on long
    /// enough, or whose sound stopped coming, is said.
    pub fn tick(&mut self, now: Instant) -> Option<NameHeard> {
        self.open
            .is_some_and(|(began, _)| now.saturating_duration_since(began) >= LONGEST)
            .then(|| self.close(now))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_run_of_high_scores_is_one_hearing_said_when_the_score_falls() {
        let start = Instant::now();
        let mut hearings = Hearings::new(0.9);
        // Talk, then the name over four steps, then talk.
        let scores = [0.01, 0.2, 0.93, 0.999, 0.97, 0.95, 0.3, 0.02];
        let mut said = Vec::new();
        for (step, score) in scores.into_iter().enumerate() {
            if let Some(heard) = hearings.score(score, start + ms(80 * step as u64)) {
                said.push((step, heard));
            }
        }
        // Once, at the step the score fell, with the highest it reached
        // and how long ago it began.
        assert_eq!(said.len(), 1);
        let (step, heard) = said[0];
        assert_eq!(step, 6);
        assert_eq!(heard.score, 0.999);
        assert_eq!(heard.ago_ms, 320);
        assert_eq!(heard.check(), Ok(()));
    }

    #[test]
    fn a_score_that_stays_high_is_said_after_its_longest_and_not_again_for_a_second() {
        let start = Instant::now();
        let mut hearings = Hearings::new(0.9);
        let mut said = Vec::new();
        // Three seconds of nothing but high scores, 80 ms apart.
        for step in 0..38u64 {
            if let Some(heard) = hearings.score(0.95, start + ms(80 * step)) {
                said.push((step, heard.ago_ms));
            }
        }
        // At 0.8 s, then nothing for a second, then a second hearing that
        // begins at 1.84 s and is said at 2.64 s.
        assert_eq!(said, [(10, 800), (33, 800)]);
    }

    /// Sound lost in the middle of a hearing: said at once, and dated from
    /// its first high score, not 0.8 s before it.
    #[test]
    fn a_hearing_cut_short_is_said_then_and_dated_from_when_it_began() {
        let start = Instant::now();
        let mut hearings = Hearings::new(0.9);
        assert_eq!(hearings.cut(start), None);
        assert_eq!(hearings.score(0.97, start + ms(80)), None);
        let heard = hearings.cut(start + ms(240)).unwrap();
        assert_eq!((heard.score, heard.ago_ms), (0.97, 160));
        // Said once, and the same call is not heard again for a second.
        assert_eq!(hearings.cut(start + ms(300)), None);
        assert_eq!(
            hearings.score(1.0, start + ms(240) + HOLD_OFF - ms(1)),
            None
        );
        assert_eq!(hearings.tick(start + ms(5_000)), None);
    }

    #[test]
    fn a_hearing_whose_sound_stops_coming_is_said_by_the_clock() {
        let start = Instant::now();
        let mut hearings = Hearings::new(0.9);
        assert_eq!(hearings.score(0.96, start), None);
        assert_eq!(hearings.tick(start + ms(400)), None);
        let heard = hearings.tick(start + ms(800)).unwrap();
        assert_eq!((heard.score, heard.ago_ms), (0.96, 800));
        // Nothing is open, so the clock says nothing more.
        assert_eq!(hearings.tick(start + ms(5_000)), None);
        // And a score under the least never opens one.
        assert_eq!(hearings.score(0.89, start + ms(6_000)), None);
        assert_eq!(hearings.tick(start + ms(9_000)), None);
    }
}
