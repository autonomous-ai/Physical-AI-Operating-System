//! A face that never moves is not a person (#763).
//!
//! lamp-4ace's eyes reported a face all night in an empty office, 54 of 55
//! times at one spot, its box the same to a pixel or two: something on the
//! wall the detector takes for a face. A person sitting still still moves
//! their box by a few pixels within seconds. So a face whose box stays
//! within [`SLACK`] of where it was first seen, every time anything is seen
//! near there, for [`STILL_FOR_US`], is left out: of what the eyes publish,
//! of what the body is told and of recognition. Any look that finds a box
//! near there that has moved starts the count again.
//!
//! Boxes are as the bus has them: edges as fractions of the picture,
//! (xmin, ymin, xmax, ymax).

/// How far an edge may move, as a fraction of the picture, and still be
/// the same box: about two pixels of the camera's 320 by 240 frame.
pub const SLACK: f32 = 0.007;
/// How long a box must stay that still before it is left out.
pub const STILL_FOR_US: u64 = 120_000_000;
/// A spot nothing has been seen at for this long is forgotten. Long, since
/// the night's false face was seen only every few minutes.
const FORGET_US: u64 = 1_800_000_000;
/// Spots remembered at once; the one seen longest ago goes first.
const MOST: usize = 32;

#[derive(Debug)]
struct Spot {
    /// The box as first seen there, since it last moved.
    first: [f32; 4],
    since_us: u64,
    seen_us: u64,
}

/// The spots where a face has been seen, and how long each has stayed put.
#[derive(Debug, Default)]
pub struct Still {
    spots: Vec<Spot>,
    /// Faces left out so far, for the journal.
    pub left_out: u64,
}

fn centre(edges: &[f32; 4]) -> (f32, f32) {
    ((edges[0] + edges[2]) / 2.0, (edges[1] + edges[3]) / 2.0)
}

impl Still {
    /// One face seen now with this box: true when it is to be left out.
    pub fn leave_out(&mut self, edges: [f32; 4], now_us: u64) -> bool {
        self.spots
            .retain(|spot| now_us.saturating_sub(spot.seen_us) <= FORGET_US);
        // Near: its centre within half the first box's width and height.
        let (x, y) = centre(&edges);
        let near = self.spots.iter_mut().find(|spot| {
            let (sx, sy) = centre(&spot.first);
            (x - sx).abs() <= (spot.first[2] - spot.first[0]) / 2.0
                && (y - sy).abs() <= (spot.first[3] - spot.first[1]) / 2.0
        });
        match near {
            Some(spot)
                if spot
                    .first
                    .iter()
                    .zip(&edges)
                    .all(|(a, b)| (a - b).abs() <= SLACK) =>
            {
                spot.seen_us = now_us;
                let out = now_us.saturating_sub(spot.since_us) >= STILL_FOR_US;
                self.left_out += u64::from(out);
                out
            }
            // It moved: the count starts again from here.
            Some(spot) => {
                *spot = Spot {
                    first: edges,
                    since_us: now_us,
                    seen_us: now_us,
                };
                false
            }
            None => {
                if self.spots.len() >= MOST
                    && let Some(oldest) =
                        (0..self.spots.len()).min_by_key(|i| self.spots[*i].seen_us)
                {
                    self.spots.swap_remove(oldest);
                }
                self.spots.push(Spot {
                    first: edges,
                    since_us: now_us,
                    seen_us: now_us,
                });
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000;
    const SPOT: [f32; 4] = [0.65, 0.55, 0.75, 0.66];

    fn nudged(by: f32) -> [f32; 4] {
        [SPOT[0] + by, SPOT[1] + by, SPOT[2] + by, SPOT[3] + by]
    }

    #[test]
    fn a_box_that_never_moves_is_left_out_after_two_minutes_and_one_that_moves_is_kept() {
        let mut still = Still::default();
        // Four looks a second for three minutes, the box within a pixel.
        let out: Vec<bool> = (0..720)
            .map(|look| {
                still.leave_out(
                    nudged(if look % 2 == 0 { 0.0 } else { 0.003 }),
                    look * SECOND / 4,
                )
            })
            .collect();
        assert!(out[..479].iter().all(|out| !out));
        assert!(out[480..].iter().all(|out| *out));
        assert_eq!(still.left_out, 240);

        // A person sitting still: a few pixels within seconds.
        let mut still = Still::default();
        let person = (0..720).any(|look| {
            let sway = [0.0, 0.004, 0.012, 0.006, -0.008][(look / 8) as usize % 5];
            still.leave_out(nudged(sway), look * SECOND / 4)
        });
        assert!(!person);
    }

    #[test]
    fn a_box_seen_only_every_few_minutes_is_left_out_and_comes_back_once_it_moves() {
        let mut still = Still::default();
        // The night's face: once every five minutes, the same box.
        assert!(!still.leave_out(SPOT, 0));
        assert!(still.leave_out(SPOT, 300 * SECOND));
        assert!(still.leave_out(SPOT, 600 * SECOND));
        // Moved: a face again, and the count starts over.
        assert!(!still.leave_out(nudged(0.02), 601 * SECOND));
        assert!(!still.leave_out(nudged(0.02), 700 * SECOND));
        assert!(still.leave_out(nudged(0.02), 722 * SECOND));
        // Nothing near it for over half an hour: forgotten.
        assert!(!still.leave_out(nudged(0.02), 2_600 * SECOND));
    }

    #[test]
    fn a_face_elsewhere_has_its_own_count() {
        let mut still = Still::default();
        assert!(!still.leave_out(SPOT, 0));
        assert!(!still.leave_out([0.1, 0.1, 0.2, 0.25], 200 * SECOND));
        assert!(still.leave_out(SPOT, 200 * SECOND));
    }
}
