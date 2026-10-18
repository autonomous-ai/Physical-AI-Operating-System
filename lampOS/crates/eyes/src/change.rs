//! With nobody in view, the detector is not run on a frame like the last
//! one it ran on (`--look-on-change on`, off unless given; #705, the lamp's
//! heat).
//!
//! With nobody in view the eyes look every 600 ms, and each look costs
//! lamp-4ace 104 to 130 ms of a core (the 10 s lines, 02:00 to 05:00 on
//! 18 Oct): about 20% of one core all night for an empty room. A person
//! who comes into view changes the picture, so a frame like the last one
//! the detector ran on, with nobody in that one, holds nobody either. Such
//! a frame is said to hold no face without the detector, and the detector
//! still runs at least every [`LOOK_AT_LEAST_US`], whatever the picture.
//!
//! A frame is compared as a grid of [`ACROSS`] by [`DOWN`] cells, each the
//! mean grey of its pixels: a sensor's noise averages out over a cell's
//! hundred pixels, a face does not. It changed when [`CELLS`] cells or more
//! are [`GREYS`] or more brighter or darker than in the last frame the
//! detector ran on, not the last one seen, so a slow change adds up.
use crate::picture::{HEIGHT, Picture, WIDTH};

pub const ACROSS: usize = 32;
pub const DOWN: usize = 24;
/// A cell this much brighter or darker, of 255, has changed.
pub const GREYS: f32 = 12.0;
/// This many changed cells are a changed frame: a face at 0.14 of the
/// picture's height, the smallest a person's was on lamp-4ace (`still.rs`),
/// covers about nine of the working frame's ten-pixel cells.
pub const CELLS: usize = 3;
/// The detector runs at least this often, changed or not.
pub const LOOK_AT_LEAST_US: u64 = 3_000_000;

/// A frame as the cells' mean greys.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid(Vec<f32>);

impl Grid {
    /// The part of `picture` the frame fills, cut into the cells.
    pub fn of(picture: &Picture) -> Self {
        let (wide, tall) = picture.filled;
        let plane = WIDTH * HEIGHT;
        let mut sums = vec![0f32; ACROSS * DOWN];
        let mut counts = vec![0u32; ACROSS * DOWN];
        for y in 0..tall {
            let row = (y * DOWN / tall.max(1)).min(DOWN - 1) * ACROSS;
            for x in 0..wide {
                let at = y * WIDTH + x;
                let grey = (picture.planes[at]
                    + picture.planes[plane + at]
                    + picture.planes[2 * plane + at])
                    / 3.0;
                let cell = row + (x * ACROSS / wide.max(1)).min(ACROSS - 1);
                sums[cell] += grey;
                counts[cell] += 1;
            }
        }
        Self(
            sums.iter()
                .zip(&counts)
                .map(|(sum, count)| sum / (*count).max(1) as f32)
                .collect(),
        )
    }

    fn changed_from(&self, before: &Self) -> bool {
        self.0
            .iter()
            .zip(&before.0)
            .filter(|(now, then)| (*now - *then).abs() >= GREYS)
            .count()
            >= CELLS
    }
}

/// Nobody has been found for this long: the room counts as empty.
pub const EMPTY_AFTER_US: u64 = 4_000_000;

/// Whether the room counts as empty: no face found for
/// [`EMPTY_AFTER_US`] (or ever), no full look asked for and nobody being
/// remembered. Anything else runs the detector at every glance.
pub fn empty_room(asked: bool, remembering: bool, face_ago_us: Option<u64>) -> bool {
    !asked && !remembering && face_ago_us.is_none_or(|ago| ago >= EMPTY_AFTER_US)
}

/// When the detector last ran with nobody in view, and on what.
#[derive(Default)]
pub struct Change {
    last: Option<(Grid, u64)>,
    /// Looks said to hold no face without the detector, since the last
    /// [`Change::take_skipped`].
    skipped: u32,
}

impl Change {
    /// Whether the detector runs on this frame. `empty_room`: nobody was in
    /// view for a while and nothing is asked of the eyes; otherwise it
    /// always runs.
    pub fn detect(&mut self, grid: Grid, empty_room: bool, now_us: u64) -> bool {
        let detect = !empty_room
            || self.last.as_ref().is_none_or(|(last, at)| {
                now_us.saturating_sub(*at) >= LOOK_AT_LEAST_US || grid.changed_from(last)
            });
        if detect {
            self.last = Some((grid, now_us));
        } else {
            self.skipped += 1;
        }
        detect
    }

    pub fn take_skipped(&mut self) -> u32 {
        std::mem::take(&mut self.skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A room at 320 by 240: a grey wall with a darker desk, the sensor's
    /// noise of up to 4 greys on every pixel, and a box drawn in.
    fn room(seed: u32, drift: f32, face: Option<(usize, usize, usize)>) -> Picture {
        let mut noise = seed;
        let mut rgb = Vec::with_capacity(320 * 240 * 3);
        for y in 0..240usize {
            for x in 0..320usize {
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let wobble = ((noise >> 24) % 9) as f32 - 4.0;
                let mut grey = if y > 170 { 70.0 } else { 150.0 } + drift + wobble;
                if let Some((left, top, side)) = face
                    && (left..left + side * 3 / 4).contains(&x)
                    && (top..top + side).contains(&y)
                {
                    grey = 205.0 + wobble;
                }
                let grey = grey.clamp(0.0, 255.0) as u8;
                rgb.extend([grey, grey, grey]);
            }
        }
        Picture::from_rgb(&rgb, 320, 240).unwrap()
    }

    const GLANCE_US: u64 = 600_000;

    /// (a) The sensor's noise is no change; a face at 0.2 of the height
    /// coming into a corner is, and so is the smallest person's, 0.14; so
    /// is a slow change, added up over glances against the last frame the
    /// detector ran on.
    #[test]
    fn noise_is_no_change_and_a_face_or_a_slow_drift_is() {
        let empty = Grid::of(&room(1, 0.0, None));
        for seed in 2..20 {
            assert!(
                !Grid::of(&room(seed, 0.0, None)).changed_from(&empty),
                "{seed}"
            );
        }
        for (corner, side) in [((0, 0), 48), ((270, 190), 48), ((150, 60), 34)] {
            let face = Grid::of(&room(3, 0.0, Some((corner.0, corner.1, side))));
            assert!(face.changed_from(&empty), "{corner:?} {side}");
        }
        // The light rising five greys a glance: never a change from the
        // glance before, a change from where the detector last ran.
        let mut change = Change::default();
        assert!(change.detect(empty.clone(), true, 0));
        let mut detected = Vec::new();
        for glance in 1..=5u64 {
            let grid = Grid::of(&room(glance as u32 + 5, 5.0 * glance as f32, None));
            let before = Grid::of(&room(glance as u32 + 4, 5.0 * (glance - 1) as f32, None));
            assert!(!grid.changed_from(&before), "{glance}");
            if change.detect(grid, true, glance * GLANCE_US / 10) {
                detected.push(glance);
            }
        }
        assert_eq!(detected, [3]);
    }

    /// (b) An empty, still room is detected once every 3 s; a changed frame
    /// at the next glance; and a room that is not empty at every glance.
    #[test]
    fn an_empty_still_room_is_detected_every_three_seconds_and_a_change_at_once() {
        let mut change = Change::default();
        let mut detected = Vec::new();
        for glance in 0..20u64 {
            let face = (glance == 12).then_some((100, 50, 48));
            let grid = Grid::of(&room(glance as u32, 0.0, face));
            if change.detect(grid, true, glance * GLANCE_US) {
                detected.push(glance);
            }
        }
        // 0 s, 3 s, 6 s; the face at 7.2 s; back to empty at 7.8 s; then
        // 3 s after that.
        assert_eq!(detected, [0, 5, 10, 12, 13, 18]);
        assert_eq!(change.take_skipped(), 14);
        assert_eq!(change.take_skipped(), 0);
        for glance in 20..30u64 {
            let grid = Grid::of(&room(glance as u32, 0.0, None));
            assert!(change.detect(grid, false, glance * GLANCE_US));
        }
        assert_eq!(change.take_skipped(), 0);
    }

    /// (b) A face found under 4 s ago, a full look asked for or someone
    /// being remembered is never an empty room.
    #[test]
    fn a_face_an_ask_or_a_remember_is_never_an_empty_room() {
        assert!(empty_room(false, false, None));
        assert!(empty_room(false, false, Some(EMPTY_AFTER_US)));
        assert!(!empty_room(false, false, Some(EMPTY_AFTER_US - 1)));
        assert!(!empty_room(true, false, None));
        assert!(!empty_room(false, true, None));
    }

    /// A frame the working size is not (a full look's working frame is
    /// shrunk into the corner) is cut by what it fills.
    #[test]
    fn the_grid_is_of_what_the_frame_fills() {
        let rgb = vec![90u8; 320 * 180 * 3];
        let grid = Grid::of(&Picture::from_rgb(&rgb, 320, 180).unwrap());
        assert!(grid.0.iter().all(|grey| (*grey - 90.0).abs() < 1e-3));
    }
}
