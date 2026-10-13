//! Whether a face is turned toward the lamp, and whether someone has been
//! facing it for the last moment: the evidence a person gives, without
//! meaning to, that what they are saying is said to the lamp.
//!
//! V1 had this (`hal/drivers/tracking/gaze.py`) and used it when speech
//! began. It needs no model beyond the face detector: the detector already
//! gives the two eyes and the nose, and how far the nose sits from the
//! point between the eyes, along the line of the eyes, is the sine of how
//! far the head is turned. It says nothing of where the eyes themselves
//! look, nor of a head tipped up or down.
//!
//! Nothing here decides anything. It turns marks into an angle, an angle
//! into "facing or not" for one look, and a run of looks into a vote. The
//! numbers are V1's and were tuned on V1's lamps at twice this picture's
//! width; none has been checked on this lamp.

/// A face turned further than this is not facing the lamp. V1's default
/// was 25 and its lamps were set to 60 after trying them with people: a
/// wide lens and a person looking at a screen beside the lamp.
pub const MAX_YAW_DEG: f32 = 60.0;
/// Toward the edge of the picture the lens shows a face turned even when
/// it looks straight at the lamp, so the allowance grows to this many
/// times `MAX_YAW_DEG` at the very edge (and is held to 90).
pub const EDGE_SCALE: f32 = 1.8;
/// A face narrower than this share of the picture's width has marks too
/// close together to measure a turn from. V1: 48 pixels of 640.
pub const LEAST_WIDTH: f32 = 0.075;
/// The vote looks this far back, needs this many looks, and this share of
/// them facing.
pub const WINDOW_US: u64 = 1_500_000;
pub const LEAST_LOOKS: usize = 2;
pub const LEAST_FACING: f32 = 0.6;

/// How far the head is turned, in degrees from straight at the camera, to
/// either side. `marks` are the eye on the left of the picture, the eye on
/// the right and the nose. None when the eyes are on top of one another or
/// a mark is not a number.
pub fn yaw_deg(marks: &[(f32, f32); 5]) -> Option<f32> {
    let (first, second, nose) = (marks[0], marks[1], marks[2]);
    if [first, second, nose]
        .iter()
        .any(|mark| !mark.0.is_finite() || !mark.1.is_finite())
    {
        return None;
    }
    let (across, down) = (second.0 - first.0, second.1 - first.1);
    let apart = across.hypot(down);
    if apart < 1e-6 {
        return None;
    }
    let between = ((first.0 + second.0) / 2.0, (first.1 + second.1) / 2.0);
    // The nose's distance from the middle, along the line of the eyes.
    let along = ((nose.0 - between.0) * across + (nose.1 - between.1) * down) / apart;
    // Past an eye the marks have nothing more to say.
    Some(
        (along / (apart / 2.0))
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees()
            .abs(),
    )
}

/// Whether the eyes and the nose were all inside the picture: a mark the
/// detector placed outside it was guessed, not seen.
pub fn marks_seen(marks: &[(f32, f32); 5], width: f32, height: f32) -> bool {
    marks[..3].iter().all(|mark| {
        mark.0.is_finite()
            && mark.1.is_finite()
            && (0.0..=width).contains(&mark.0)
            && (0.0..=height).contains(&mark.1)
    })
}

/// The turn allowed to a face this far from the middle of the picture:
/// 0 at the middle, 1 at an edge.
pub fn allowed_deg(from_middle: f32) -> f32 {
    (MAX_YAW_DEG * (1.0 + (EDGE_SCALE - 1.0) * from_middle.clamp(0.0, 1.0))).min(90.0)
}

/// One look at one face: is it facing the lamp. A face too small to
/// measure never counts, for or against, and that is the caller's to
/// leave out of the vote: here it is simply not facing.
pub fn facing(yaw: Option<f32>, width_share: f32, from_middle: f32) -> bool {
    yaw.is_some_and(|yaw| width_share >= LEAST_WIDTH && yaw <= allowed_deg(from_middle))
}

/// The looks of the last moment, for the vote taken when someone begins to
/// speak.
#[derive(Default)]
pub struct Recent {
    looks: std::collections::VecDeque<(u64, bool)>,
}

impl Recent {
    /// One look in which a face large enough to measure was seen.
    pub fn saw(&mut self, at_us: u64, facing: bool) {
        self.looks.push_back((at_us, facing));
        self.forget(at_us);
    }

    fn forget(&mut self, now_us: u64) {
        while self
            .looks
            .front()
            .is_some_and(|(at, _)| now_us.saturating_sub(*at) > WINDOW_US)
        {
            self.looks.pop_front();
        }
    }

    /// Whether someone has been facing the lamp: none when there were too
    /// few looks to say, which is not the same as nobody facing it.
    pub fn vote(&mut self, now_us: u64) -> Option<bool> {
        self.forget(now_us);
        if self.looks.len() < LEAST_LOOKS {
            return None;
        }
        let facing = self.looks.iter().filter(|(_, facing)| *facing).count();
        Some(facing as f32 / self.looks.len() as f32 >= LEAST_FACING)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A face 100 across with its eyes 40 apart and its nose `turn` of the
    /// way from the middle toward the right eye (1) or the left (-1).
    fn marks(turn: f32) -> [(f32, f32); 5] {
        let nose = 50.0 + 20.0 * turn;
        [
            (30.0, 40.0),
            (70.0, 40.0),
            (nose, 60.0),
            (35.0, 80.0),
            (65.0, 80.0),
        ]
    }

    #[test]
    fn the_nose_between_the_eyes_is_straight_and_at_an_eye_is_side_on() {
        assert!(yaw_deg(&marks(0.0)).unwrap() < 0.01);
        assert!((yaw_deg(&marks(0.5)).unwrap() - 30.0).abs() < 0.01);
        assert!((yaw_deg(&marks(-0.5)).unwrap() - 30.0).abs() < 0.01);
        assert!((yaw_deg(&marks(1.0)).unwrap() - 90.0).abs() < 0.01);
        // Past the eye is held at side on.
        assert!((yaw_deg(&marks(1.7)).unwrap() - 90.0).abs() < 0.01);
        // A head tilted over: the turn is measured along the tilted eyes.
        let tilted = [
            (30.0, 30.0),
            (70.0, 50.0),
            (45.0, 60.0),
            (0.0, 0.0),
            (0.0, 0.0),
        ];
        let straight = yaw_deg(&tilted).unwrap();
        assert!(straight < 12.0, "{straight}");
        // No eyes to measure from, or a mark that is no number.
        let mut none = marks(0.0);
        none[1] = none[0];
        assert_eq!(yaw_deg(&none), None);
        let mut broken = marks(0.0);
        broken[2].0 = f32::NAN;
        assert_eq!(yaw_deg(&broken), None);
    }

    #[test]
    fn a_small_face_or_a_guessed_mark_does_not_count_and_the_edge_allows_more() {
        // At the middle: up to sixty degrees is facing.
        assert!(facing(Some(59.0), 0.2, 0.0));
        assert!(!facing(Some(61.0), 0.2, 0.0));
        // At the edge of a wide lens the allowance grows, and stops at 90.
        assert!(facing(Some(80.0), 0.2, 1.0));
        assert!((allowed_deg(0.5) - 84.0).abs() < 0.01);
        assert_eq!(allowed_deg(1.0), 90.0);
        assert_eq!(allowed_deg(7.0), 90.0);
        // Too small to measure, or not measured at all.
        assert!(!facing(Some(0.0), 0.05, 0.0));
        assert!(!facing(None, 0.5, 0.0));
        // A nose the detector put outside the picture was not seen.
        assert!(marks_seen(&marks(0.0), 100.0, 100.0));
        let mut outside = marks(0.0);
        outside[2].0 = 104.0;
        assert!(!marks_seen(&outside, 100.0, 100.0));
        outside[2].0 = -1.0;
        assert!(!marks_seen(&outside, 100.0, 100.0));
        // The mouth is not used, so it may be anywhere.
        let mut mouth = marks(0.0);
        mouth[4] = (500.0, 500.0);
        assert!(marks_seen(&mouth, 100.0, 100.0));
    }

    #[test]
    fn the_vote_needs_two_looks_in_the_last_moment_and_most_of_them_facing() {
        const MS: u64 = 1_000;
        let mut recent = Recent::default();
        assert_eq!(recent.vote(0), None);
        recent.saw(100 * MS, true);
        assert_eq!(recent.vote(200 * MS), None);
        recent.saw(350 * MS, true);
        assert_eq!(recent.vote(400 * MS), Some(true));
        // Three of five facing is enough; two of five is not.
        recent.saw(600 * MS, false);
        recent.saw(850 * MS, false);
        recent.saw(1_100 * MS, true);
        assert_eq!(recent.vote(1_150 * MS), Some(true));
        let mut away = Recent::default();
        for (at, facing) in [
            (0, true),
            (250, false),
            (500, false),
            (750, false),
            (1_000, true),
        ] {
            away.saw(at * MS, facing);
        }
        assert_eq!(away.vote(1_050 * MS), Some(false));
        // Looks older than a second and a half are forgotten: a person who
        // faced the lamp and left is not still facing it.
        assert_eq!(recent.vote(1_100 * MS + WINDOW_US + 1), None);
    }
}
