//! A face at a place where no person has ever been seen is not a person
//! (#763).
//!
//! lamp-4ace's eyes reported a face all night in an empty office: 55 of 55
//! detections from 01:00 to 05:59 at one spot, about 0.7 across and 0.6
//! down, 0.09 to 0.12 of the picture's height, at 0.60 to 0.79. Its box is
//! not still: it moves 0.01 to 0.02 of the picture look to look, as much as
//! a person sitting at the desk, so stillness alone cannot tell them apart
//! (a box held within two pixels for two minutes left out 4 of the 55, and
//! a centre held within 0.02 for five minutes left out a third of the
//! morning's real faces). What does: in fifteen hours that spot never gave
//! a face of 0.14 of the height or more, nor one scored 0.8 or more, while
//! people at the desk did within seconds, and the people at the picture's
//! edge, small too, scored up to 0.90.
//!
//! So every place a face is seen is remembered (its centre and height,
//! within [`NEAR`]), with how many faces were seen there, since when, and
//! the largest and most certain of them. A face at a place seen for
//! [`LEARNT_AFTER_S`] or more, [`LEARNT_FROM`] times or more, where every
//! face so far was smaller than [`PERSON_HEIGHT`] and scored under
//! [`PERSON_SCORE`], is left out: of what the eyes publish, of what the
//! body is told and of recognition. One face there that is large or
//! certain enough and the place counts as a person's for as long as it is
//! remembered.
//!
//! Boxes are as the bus has them: edges as fractions of the picture,
//! (xmin, ymin, xmax, ymax).

/// How far a centre, across or down, and a height may be from a place's
/// and still be at it, as a fraction of the picture.
pub const NEAR: f32 = 0.03;
/// A place is learnt as nobody's only after faces have come from it for
/// this long, and this many of them.
pub const LEARNT_AFTER_S: u64 = 1_800;
pub const LEARNT_FROM: u32 = 10;
/// A face this tall, or scored this high, is a person's.
pub const PERSON_HEIGHT: f32 = 0.14;
pub const PERSON_SCORE: f32 = 0.8;
/// A place nothing has been seen at for this long is forgotten: the night's
/// false face came back after at most 52 minutes.
const FORGET_US: u64 = 7_200_000_000;
/// Places remembered at once; the one seen longest ago goes first.
const MOST: usize = 32;

#[derive(Debug)]
struct Place {
    x: f32,
    y: f32,
    height: f32,
    since_us: u64,
    seen_us: u64,
    faces: u32,
    /// A face here was a person's.
    somebody: bool,
}

/// The places where a face has been seen, and what was seen at each.
#[derive(Debug, Default)]
pub struct Still {
    places: Vec<Place>,
    /// Faces left out so far, for the journal.
    pub left_out: u64,
}

impl Still {
    /// One face seen now with this box and score: true when it is to be
    /// left out.
    pub fn leave_out(&mut self, edges: [f32; 4], score: f32, now_us: u64) -> bool {
        self.places
            .retain(|place| now_us.saturating_sub(place.seen_us) <= FORGET_US);
        let (x, y) = ((edges[0] + edges[2]) / 2.0, (edges[1] + edges[3]) / 2.0);
        let height = edges[3] - edges[1];
        let at = self.places.iter().position(|place| {
            (x - place.x).abs() <= NEAR
                && (y - place.y).abs() <= NEAR
                && (height - place.height).abs() <= NEAR
        });
        let at = at.unwrap_or_else(|| {
            if self.places.len() >= MOST
                && let Some(oldest) = (0..self.places.len()).min_by_key(|i| self.places[*i].seen_us)
            {
                self.places.swap_remove(oldest);
            }
            self.places.push(Place {
                x,
                y,
                height,
                since_us: now_us,
                seen_us: now_us,
                faces: 0,
                somebody: false,
            });
            self.places.len() - 1
        });
        let place = &mut self.places[at];
        place.seen_us = now_us;
        place.faces = place.faces.saturating_add(1);
        place.somebody |= height >= PERSON_HEIGHT || score >= PERSON_SCORE;
        let out = !place.somebody
            && place.faces >= LEARNT_FROM
            && now_us.saturating_sub(place.since_us) >= LEARNT_AFTER_S * 1_000_000;
        self.left_out += u64::from(out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000;
    /// The night's false face: small, faint, wandering by 0.015.
    const SPOT: [f32; 4] = [0.65, 0.55, 0.71, 0.65];

    fn nudged(edges: [f32; 4], by: f32) -> [f32; 4] {
        edges.map(|edge| edge + by)
    }

    #[test]
    fn a_small_faint_face_at_one_place_for_half_an_hour_is_left_out() {
        let mut still = Still::default();
        // Once a minute for an hour, wandering as the night's did.
        let out: Vec<bool> = (0..60u64)
            .map(|minute| {
                let by = [0.0, 0.015, -0.01][minute as usize % 3];
                still.leave_out(nudged(SPOT, by), 0.65, minute * 60 * SECOND)
            })
            .collect();
        assert!(out[..30].iter().all(|out| !out));
        assert!(out[30..].iter().all(|out| *out));
        assert_eq!(still.left_out, 30);
    }

    #[test]
    fn a_place_where_a_person_was_seen_is_never_left_out() {
        // As small and faint, but once scored as a person's.
        let mut still = Still::default();
        assert!(!still.leave_out(SPOT, 0.85, 0));
        assert!((1..120u64).all(|minute| !still.leave_out(SPOT, 0.65, minute * 60 * SECOND)));
        // Or once as tall as somebody at the lamp.
        let mut still = Still::default();
        let tall = [SPOT[0], SPOT[1], SPOT[2], SPOT[1] + 0.15];
        assert!(!still.leave_out(tall, 0.65, 0));
        assert!((1..120u64).all(|minute| !still.leave_out(
            [SPOT[0], SPOT[1] - 0.02, SPOT[2], SPOT[1] + 0.13],
            0.65,
            minute * 60 * SECOND
        )));
    }

    #[test]
    fn a_person_sitting_still_for_hours_is_kept() {
        // Four looks a second for three hours, within a pixel, at the desk.
        let mut still = Still::default();
        let desk = [0.3, 0.2, 0.45, 0.4];
        assert!((0..43_200u64).all(|look| !still.leave_out(desk, 0.87, look * SECOND / 4)));
    }

    /// lamp-4ace's eyes, 14 Oct 18:23 to 15 Oct 13:17: every face it
    /// published, replayed in order (docs/verification/
    /// still-face-lamp-4ace-20261014). The night's false faces are all left
    /// out; no face of the people at the picture's edge in the evening, nor
    /// of the morning at the desk, is.
    #[test]
    fn lamp_4ace_s_night_is_left_out_and_its_people_are_kept() {
        const DAY: &str =
            include_str!("../../../docs/verification/still-face-lamp-4ace-20261014/faces.csv");
        // Seen and left out, for: the night (01:00 to 05:59), the false spot
        // the rest of the time, people at the edge, the morning (10:00 to
        // 13:59), the rest, and all.
        let mut counts = [[0u32; 2]; 6];
        let mut still = Still::default();
        for line in DAY.lines().skip(1) {
            let field: Vec<f64> = line.split(',').map(|f| f.parse().unwrap()).collect();
            let edges = [field[1], field[2], field[3], field[4]].map(|e| e as f32);
            let out = still.leave_out(edges, field[5] as f32, field[0] as u64 * 1_000);
            let hour = (field[0] as u64 / 1_000 + 7 * 3_600) % 86_400 / 3_600;
            let (x, y) = ((edges[0] + edges[2]) / 2.0, (edges[1] + edges[3]) / 2.0);
            let height = edges[3] - edges[1];
            let spot = (0.62..0.78).contains(&x)
                && (0.52..0.68).contains(&y)
                && (0.08..0.14).contains(&height);
            let group = if (1..=5).contains(&hour) {
                0
            } else if spot {
                1
            } else if x >= 0.9 {
                2
            } else if (10..=13).contains(&hour) {
                3
            } else {
                4
            };
            for counted in [group, 5] {
                counts[counted][0] += 1;
                counts[counted][1] += u32::from(out);
            }
        }
        let [night, spot, edge, morning, _, all] = counts;
        eprintln!(
            "seen, left out: night {night:?} spot {spot:?} edge {edge:?} morning {morning:?} all {all:?}"
        );
        assert_eq!(night, [55, 55]);
        assert_eq!(edge[1], 0);
        assert_eq!(morning[1], 0);
        assert!(spot[1] * 4 >= spot[0] * 3, "{spot:?}");
    }

    #[test]
    fn a_place_not_seen_for_two_hours_is_learnt_again() {
        let mut still = Still::default();
        for minute in 0..40u64 {
            still.leave_out(SPOT, 0.65, minute * 60 * SECOND);
        }
        assert!(still.leave_out(SPOT, 0.65, 40 * 60 * SECOND));
        // Gone for over two hours: a face there again is kept until learnt.
        assert!(!still.leave_out(SPOT, 0.65, (40 + 121) * 60 * SECOND));
    }
}
