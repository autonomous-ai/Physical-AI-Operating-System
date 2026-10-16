//! Whether a face's mouth is moving: the sign that someone is speaking that
//! the lamp's own voice cannot fake. A mouth that moves while the lamp talks
//! is not the lamp's echo.
//!
//! No model beyond the face detector. The detector gives the nose and the
//! two corners of the mouth. Each frame is first lined up with the one
//! before by the band round the eyes, which moves only with the head: the
//! shift of a few pixels that makes it change least is how far the face
//! moved in the picture (the lamp's head follows the face, so it always
//! moves a little), and the lower face is read shifted by as much. Then the
//! share of the lower face's points that changed by more than noise is the
//! mouth's movement, in the frames where the eyes, once lined up, held
//! still: a head that turns or nods, or a light that flickers, still
//! changes them, and such a frame is left out. It is the
//! oldest way of seeing who speaks (frame differences in a window round the
//! lips), and the lower face rather than the mouth alone, as Caus et al.
//! (ICVS 2021) found works better. It cannot tell talking from chewing or
//! laughing, and a hand on the chin or a face turned away hides it.
//!
//! Nothing here decides anything: a score from 0 to 1 a face, over the last
//! [`WINDOW_US`].
//
// ponytail: frame differences, no learned model. If the recordings with
// people show it cannot part speech from chewing or smiling, the next rung
// is the visual half of Light-ASD (about 1M parameters) on the lower-face
// crop, on the NPU.
use crate::faces::Face;
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use zune_jpeg::{
    JpegDecoder,
    zune_core::{colorspace::ColorSpace, options::DecoderOptions},
};

/// Where the eyes read whether to look at mouths. `on` only.
pub const ENVIRONMENT: &str = "LAMP_EYES_MOUTH";
/// The score looks this far back from its newest frame. On lamp-52e6 the
/// camera gives about 7 frames a second, so this holds about 8 changes:
/// enough that one frame does not make the score. 500 ms, counted back from
/// when the score was said, held at most 3 there: the newest frame is about
/// 190 ms old by then (the camera's queue and a look), and 3 changes from 3
/// frames came out 0 for most speech (2026-10-15, a person at a metre).
pub const WINDOW_US: u64 = 1_200_000;
/// Fewer frames than this in the window say nothing: over half of what the
/// window holds, so a face only just found waits about 0.7 s.
pub const LEAST_FRAMES: usize = 5;
/// Where the eyes read whether to say each frame's raw changes with the
/// score (`Raw`), for setting the numbers below from a run. `on` only.
pub const RAW_ENVIRONMENT: &str = "LAMP_EYES_MOUTH_RAW";
/// A lower-face point changed if its grey moved by at least this much
/// between frames. First guess, for the raw run to set: camera noise on a
/// still face is a few grey levels, an opening lip against skin tens.
pub const CHANGED: f32 = 12.0;
/// This share of the lower face's points changed is a mouth moving fully.
/// First guess, for the raw run to set: at a metre the lips open by 5 to
/// 14 pixels, a few hundredths to a tenth of the region, plus the jaw.
pub const FULL_SHARE: f32 = 0.15;
/// A frame whose eyes, once lined up with the frame before, still changed
/// by more than this mean grey is a head turning or nodding (or the light)
/// and is left out of the score. First guess, for the next raw run to set:
/// a still face's eyes change by a few grey levels (noise, a blink over a
/// few points); unaligned they changed by 19 to 22 on lamp-52e6 with its
/// head following the face (2026-10-16, `eyes_mean`).
pub const HEAD_MOVED: f32 = 10.0;
/// How far a frame is searched for its line-up with the one before, in
/// the detector's pixels each way. First guess: the gaze moves the head at
/// 50 counts a second at most (`--speed`, docs/body.md), about 4.4 degrees
/// a second, 0.6 degrees a frame at 7 frames a second; at 320 pixels over
/// 60 to 78 degrees that is 2.5 to 3.4 pixels, and 6 leaves as much again
/// for the person's own sway. 169 shifts of 192 points: about 32,000 reads
/// a frame, against the 384 before (the time is in the journal's line).
pub const SEARCH: i32 = 6;
/// The lower face and the eyes are each read as this many points across
/// and down, whatever the face's size.
const GRID: (usize, usize) = (16, 12);
/// A face of the next look is the one before if their boxes overlap by at
/// least this share of their union.
const SAME_OVERLAP: f32 = 0.2;
/// A face that overlaps the one before by this much has not moved: it is
/// read where it was, so that a box that jitters by a pixel from one look
/// to the next does not cost a frame. A head that moves inside it moves
/// the eyes too.
const SAME_PLACE: f32 = 0.6;
const MAX_SIDE: usize = 4_096;

/// A camera frame in grey, as the mouths are read from it, with the scale
/// the detector's picture has (`picture::Picture`): a face's marks are in
/// that picture's pixels.
pub struct Grey {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    scale: f32,
}

impl Grey {
    pub fn from_luma(pixels: Vec<u8>, width: usize, height: usize) -> Result<Self, &'static str> {
        if width == 0
            || height == 0
            || width > MAX_SIDE
            || height > MAX_SIDE
            || pixels.len() != width * height
        {
            return Err("a frame of no size, too large, or not one byte a pixel");
        }
        let scale = (crate::picture::WIDTH as f32 / width as f32)
            .min(crate::picture::HEIGHT as f32 / height as f32)
            .min(1.0);
        Ok(Self {
            pixels,
            width,
            height,
            scale,
        })
    }

    pub fn from_jpeg(bytes: &[u8]) -> Result<Self, String> {
        let options = DecoderOptions::default()
            .set_max_width(MAX_SIDE)
            .set_max_height(MAX_SIDE)
            .jpeg_set_out_colorspace(ColorSpace::Luma);
        let mut decoder = JpegDecoder::new_with_options(bytes, options);
        let pixels = decoder
            .decode()
            .map_err(|error| format!("jpeg: {error:?}"))?;
        let (width, height) = decoder.dimensions().ok_or("a jpeg without a size")?;
        Self::from_luma(pixels, width, height).map_err(Into::into)
    }

    /// The grey at a point of the detector's picture, the nearest pixel.
    fn at(&self, x: f32, y: f32) -> f32 {
        let column = ((x / self.scale) as usize).min(self.width - 1);
        let row = ((y / self.scale) as usize).min(self.height - 1);
        f32::from(self.pixels[row * self.width + column])
    }

    /// A region, shifted by `by` pixels, read as `GRID` points; none if it
    /// leaves the picture.
    fn read(&self, region: [f32; 4], by: (i32, i32)) -> Option<Vec<f32>> {
        let (x, y) = (by.0 as f32, by.1 as f32);
        let [left, top, right, bottom] =
            [region[0] + x, region[1] + y, region[2] + x, region[3] + y];
        let (width, height) = (
            self.width as f32 * self.scale,
            self.height as f32 * self.scale,
        );
        if left < 0.0 || top < 0.0 || right > width || bottom > height {
            return None;
        }
        let mut points = Vec::with_capacity(GRID.0 * GRID.1);
        for row in 0..GRID.1 {
            let y = top + (bottom - top) * (row as f32 + 0.5) / GRID.1 as f32;
            for column in 0..GRID.0 {
                let x = left + (right - left) * (column as f32 + 0.5) / GRID.0 as f32;
                points.push(self.at(x, y));
            }
        }
        Some(points)
    }
}

/// The lower face and the eyes, each `[left, top, right, bottom]` in the
/// detector's pixels, from the face's marks: none if they make no shape.
pub fn regions(marks: &[(f32, f32); 5]) -> Option<([f32; 4], [f32; 4])> {
    let (eye, other_eye, nose, corner, other_corner) =
        (marks[0], marks[1], marks[2], marks[3], marks[4]);
    let mouth = (
        (corner.0 + other_corner.0) / 2.0,
        (corner.1 + other_corner.1) / 2.0,
    );
    let wide = (other_corner.0 - corner.0).hypot(other_corner.1 - corner.1);
    let eyes = ((eye.0 + other_eye.0) / 2.0, (eye.1 + other_eye.1) / 2.0);
    let apart = (other_eye.0 - eye.0).hypot(other_eye.1 - eye.1);
    let drop = mouth.1 - nose.1;
    if !(wide > 1.0 && apart > 1.0 && drop > 0.5) {
        return None;
    }
    // From the nose down to about the chin, a little wider than the lips:
    // a chewing or speaking jaw drops below the lips by more than the nose
    // sits above them.
    let lower = [mouth.0 - wide, nose.1, mouth.0 + wide, mouth.1 + drop * 2.5];
    let around_eyes = [
        eyes.0 - apart,
        eyes.1 - apart * 0.35,
        eyes.0 + apart,
        eyes.1 + apart * 0.35,
    ];
    Some((lower, around_eyes))
}

fn change(now: &[f32], before: &[f32]) -> f32 {
    now.iter()
        .zip(before)
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>()
        / now.len().max(1) as f32
}

/// The share of points that changed by at least `at` grey levels.
fn share(now: &[f32], before: &[f32], at: f32) -> f32 {
    now.iter()
        .zip(before)
        .filter(|(a, b)| (*a - *b).abs() >= at)
        .count() as f32
        / now.len().max(1) as f32
}

/// One frame's changes since the frame before, as numbers only: said with
/// the score when `RAW_ENVIRONMENT` is on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Raw {
    /// When the frame was taken (the camera driver's clock).
    pub taken_us: u64,
    /// How far the face moved in the picture since the frame before, in
    /// the detector's pixels: the shift the lower face was read at.
    pub shift_x: i32,
    pub shift_y: i32,
    /// The lower face's mean change, lined up, in grey levels.
    pub lower_mean: f32,
    /// The share of its points that changed by at least 8, 12 and 16.
    pub lower_share8: f32,
    pub lower_share12: f32,
    pub lower_share16: f32,
    /// The eyes' mean change where they were, and lined up, in grey levels.
    pub eyes_mean: f32,
    pub eyes_after: f32,
}

impl Raw {
    fn of(
        taken_us: u64,
        shift: (i32, i32),
        lower: (&[f32], &[f32]),
        eyes_mean: f32,
        eyes_after: f32,
    ) -> Self {
        Self {
            taken_us,
            shift_x: shift.0,
            shift_y: shift.1,
            lower_mean: change(lower.0, lower.1),
            lower_share8: share(lower.0, lower.1, 8.0),
            lower_share12: share(lower.0, lower.1, 12.0),
            lower_share16: share(lower.0, lower.1, 16.0),
            eyes_mean,
            eyes_after,
        }
    }

    /// Whether the head (or the light) moved: the frame says nothing.
    fn head_moved(&self) -> bool {
        self.eyes_after > HEAD_MOVED
    }

    /// How far the mouth moved, 0 to 1.
    fn moved(&self) -> f32 {
        (self.lower_share12 / FULL_SHARE).clamp(0.0, 1.0)
    }
}

/// One frame's changes against the frame before, read at `offset`: the
/// shift within [`SEARCH`] that makes the eyes change least, and the lower
/// face read at it. None if the eyes leave the picture where they were, or
/// the lower face where they line up.
fn line_up(
    grey: &Grey,
    offset: (i32, i32),
    (lower, eyes): ([f32; 4], [f32; 4]),
    (lower_before, eyes_before): (&[f32], &[f32]),
    taken_us: u64,
) -> Option<Raw> {
    let at = |shift: (i32, i32)| (offset.0 + shift.0, offset.1 + shift.1);
    let eyes_mean = change(&grey.read(eyes, offset)?, eyes_before);
    // Not moving wins a tie, then the first found.
    let mut best = ((0, 0), eyes_mean);
    for y in -SEARCH..=SEARCH {
        for x in -SEARCH..=SEARCH {
            if let Some(now) = grey.read(eyes, at((x, y))) {
                let changed = change(&now, eyes_before);
                if changed < best.1 {
                    best = ((x, y), changed);
                }
            }
        }
    }
    let lower_now = grey.read(lower, at(best.0))?;
    Some(Raw::of(
        taken_us,
        best.0,
        (&lower_now, lower_before),
        eyes_mean,
        best.1,
    ))
}

fn overlap(a: &Face, b: &Face) -> f32 {
    let across = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let down = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    let shared = across.max(0.0) * down.max(0.0);
    shared / (a.width * a.height + b.width * b.height - shared).max(1e-6)
}

struct Track {
    face: Face,
    regions: Option<([f32; 4], [f32; 4])>,
    /// How far the face has moved since its regions were found, added up
    /// frame by frame: where they are read now.
    offset: (i32, i32),
    before: Option<(Vec<f32>, Vec<f32>)>,
    /// Each frame's changes, oldest first.
    moved: VecDeque<Raw>,
}

/// A face's score as it leaves the process.
#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    /// The mean over the frames where the head held still; 0 if it held
    /// still in none (then `head_moved == frames`: not read, not still).
    pub moving: f32,
    pub frames: usize,
    /// How many of the frames were left out: the head moved.
    pub head_moved: usize,
    /// When the newest frame in it was taken.
    pub taken_us: u64,
    /// Every frame in it.
    pub raw: Vec<Raw>,
}

/// The mouths of the faces of the last look, followed frame by frame.
#[derive(Default)]
pub struct Mouths {
    tracks: Vec<Track>,
}

impl Mouths {
    /// The faces of a new look. A face that overlaps one of the last look
    /// keeps what was seen of its mouth. A face that moved is read afresh
    /// from the next frame: its regions moved with it.
    pub fn faces(&mut self, found: &[Face]) {
        let mut old = std::mem::take(&mut self.tracks);
        for face in found {
            let kept = old
                .iter()
                .enumerate()
                .map(|(index, track)| (index, overlap(&track.face, face)))
                .filter(|(_, share)| *share >= SAME_OVERLAP)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, share)| (old.swap_remove(index), share));
            self.tracks.push(match kept {
                Some((track, share)) if share >= SAME_PLACE => track,
                kept => Track {
                    face: *face,
                    regions: regions(&face.marks),
                    offset: (0, 0),
                    before: None,
                    moved: kept.map(|(track, _)| track.moved).unwrap_or_default(),
                },
            });
        }
    }

    /// Whether there is any face to read a mouth of.
    pub fn any(&self) -> bool {
        self.tracks.iter().any(|track| track.regions.is_some())
    }

    /// One frame, taken at `taken_us`.
    pub fn frame(&mut self, grey: &Grey, taken_us: u64) {
        for track in &mut self.tracks {
            let Some((lower, eyes)) = track.regions else {
                continue;
            };
            if let Some((lower_before, eyes_before)) = &track.before
                && let Some(raw) = line_up(
                    grey,
                    track.offset,
                    (lower, eyes),
                    (lower_before, eyes_before),
                    taken_us,
                )
            {
                track.offset.0 += raw.shift_x;
                track.offset.1 += raw.shift_y;
                track.moved.push_back(raw);
            }
            track.before = grey
                .read(lower, track.offset)
                .zip(grey.read(eyes, track.offset));
            while track
                .moved
                .front()
                .is_some_and(|raw| taken_us.saturating_sub(raw.taken_us) > WINDOW_US)
            {
                track.moved.pop_front();
            }
        }
    }

    /// One score for each face of the last look, in its order: none for a
    /// face whose mouth has not been seen in enough frames of late. The
    /// window is counted back from the newest frame (`frame` keeps only
    /// those), not from `now_us`: a frame is always some way behind.
    pub fn scores(&self, now_us: u64) -> Vec<Option<Score>> {
        self.tracks
            .iter()
            .map(|track| {
                let newest = track.moved.back()?.taken_us;
                let frames = track.moved.len();
                (frames >= LEAST_FRAMES && now_us.saturating_sub(newest) <= WINDOW_US).then(|| {
                    let still: Vec<f32> = track
                        .moved
                        .iter()
                        .filter(|raw| !raw.head_moved())
                        .map(Raw::moved)
                        .collect();
                    Score {
                        moving: still.iter().sum::<f32>() / still.len().max(1) as f32,
                        frames,
                        head_moved: frames - still.len(),
                        taken_us: newest,
                        raw: track.moved.iter().copied().collect(),
                    }
                })
            })
            .collect()
    }
}

/// One face's mouth, on `humans/faces/mouths`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mouth {
    /// The face's passing id, as on `humans/faces/tracked`.
    pub id: String,
    /// How much its mouth moved over the window, from 0 (still) to 1.
    pub moving: f32,
    pub window_ms: u32,
    pub frames: u32,
    /// From when the newest frame in it was taken to when this was said.
    pub age_ms: u32,
    /// Of `frames`, those left out because the head moved.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub head_moved: u32,
    /// Each frame's changes, with `RAW_ENVIRONMENT` on only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Vec<Raw>>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// The mouths of the faces in view that could be read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouthsMoving {
    pub mouths: Vec<Mouth>,
}

impl MouthsMoving {
    pub const TYPE: &str = "lamp_msgs/MouthsMoving";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "humans/faces/mouths";

    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A face 80 across at (100, 60) in a 320 by 240 grey picture.
    fn face(x: f32) -> Face {
        let y = 60.0;
        Face {
            score: 0.9,
            x,
            y,
            width: 80.0,
            height: 80.0,
            marks: [
                (x + 24.0, y + 30.0),
                (x + 56.0, y + 30.0),
                (x + 40.0, y + 46.0),
                (x + 28.0, y + 62.0),
                (x + 52.0, y + 62.0),
            ],
        }
    }

    /// A picture with a fine texture, the lower face's part of it shifted
    /// by `lips` and the whole by `head`.
    fn picture(lips: usize, head: usize) -> Grey {
        picture_at(lips, (head, 0), 0)
    }

    /// The same, the whole moved left by `head.0` and up by `head.1`, and
    /// `light` grey levels brighter.
    fn picture_at(lips: usize, head: (usize, usize), light: u8) -> Grey {
        let (width, height) = (320, 240);
        // Skin-like texture: no two neighbours alike, no repeats.
        let texture = |x: usize, y: usize| {
            let hash = ((x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663))
                .wrapping_mul(2_654_435_761)
                >> 24;
            (40 + hash * 175 / 255) as u8 + light
        };
        let mut pixels = vec![0u8; width * height];
        for row in 0..height {
            for column in 0..width {
                let (x, y) = (column + head.0, row + head.1);
                let lower = (104..150).contains(&y) && (110..170).contains(&x);
                pixels[row * width + column] = texture(x + if lower { lips } else { 0 }, y);
            }
        }
        Grey::from_luma(pixels, width, height).unwrap()
    }

    const MS: u64 = 1_000;

    #[test]
    fn a_moving_mouth_scores_high_a_still_face_low_and_a_flickering_light_not_as_speech() {
        let mut talking = Mouths::default();
        talking.faces(&[face(100.0)]);
        for frame in 0..6u64 {
            talking.frame(&picture(frame as usize % 2, 0), frame * 100 * MS);
        }
        let score = talking.scores(500 * MS).remove(0).unwrap();
        assert!(score.moving > 0.8, "{score:?}");
        assert_eq!((score.frames, score.head_moved, score.raw.len()), (5, 0, 5));
        assert_eq!(score.taken_us, 500 * MS);

        let mut still = Mouths::default();
        still.faces(&[face(100.0)]);
        for frame in 0..6u64 {
            still.frame(&picture(0, 0), frame * 100 * MS);
        }
        assert!(still.scores(500 * MS).remove(0).unwrap().moving < 0.01);

        // The light flickers: lined up or not, the eyes change. Every frame
        // is left out, so it reads 0 with all its frames left out, not
        // speech.
        let mut flicker = Mouths::default();
        flicker.faces(&[face(100.0)]);
        for frame in 0..6u64 {
            flicker.frame(
                &picture_at(0, (0, 0), frame as u8 % 2 * 30),
                frame * 100 * MS,
            );
        }
        let flicker = flicker.scores(500 * MS).remove(0).unwrap();
        assert_eq!(flicker.moving, 0.0);
        assert_eq!(flicker.head_moved, flicker.frames);
    }

    #[test]
    fn a_face_that_shifts_in_the_picture_is_lined_up_and_reads_still_unless_its_mouth_moves() {
        // The lamp's head follows the face: it slides 2 pixels left and 1
        // up a frame, 16 and 8 in all, further than one search reaches.
        let mut sliding = Mouths::default();
        sliding.faces(&[face(100.0)]);
        for frame in 0..9u64 {
            let at = frame as usize;
            sliding.frame(&picture_at(0, (at * 2, at), 0), frame * 100 * MS);
        }
        let score = sliding.scores(800 * MS).remove(0).unwrap();
        assert!(score.moving < 0.01, "{score:?}");
        assert_eq!(score.head_moved, 0, "{score:?}");
        for raw in &score.raw {
            assert_eq!((raw.shift_x, raw.shift_y), (-2, -1), "{raw:?}");
            assert!(
                raw.eyes_mean > HEAD_MOVED && raw.eyes_after < 0.01,
                "{raw:?}"
            );
            assert!(raw.lower_mean < 0.01, "{raw:?}");
        }
        assert_eq!(sliding.tracks[0].offset, (-16, -8));

        // The same slide with the lips moving: it reads as a mouth.
        let mut talking = Mouths::default();
        talking.faces(&[face(100.0)]);
        for frame in 0..9u64 {
            let at = frame as usize;
            talking.frame(&picture_at(at % 2, (at * 2, at), 0), frame * 100 * MS);
        }
        let score = talking.scores(800 * MS).remove(0).unwrap();
        assert!(score.moving > 0.8, "{score:?}");
        assert_eq!(score.head_moved, 0, "{score:?}");
    }

    #[test]
    fn too_few_frames_or_old_ones_say_nothing_and_a_face_keeps_its_frames_across_looks() {
        let mut mouths = Mouths::default();
        assert!(mouths.scores(0).is_empty());
        mouths.faces(&[face(100.0)]);
        assert!(mouths.any());
        for frame in 0..5u64 {
            mouths.frame(&picture(frame as usize % 2, 0), frame * 100 * MS);
        }
        // Five frames are four changes: one short.
        assert_eq!(mouths.scores(400 * MS), vec![None]);
        // The next look finds the same face a pixel or two off: it is read
        // where it was, and no frame is lost.
        mouths.faces(&[face(102.0)]);
        mouths.frame(&picture(1, 0), 500 * MS);
        mouths.frame(&picture(0, 0), 600 * MS);
        assert_eq!(mouths.scores(600 * MS).remove(0).unwrap().frames, 6);
        // Moved further, it keeps its frames and is read afresh.
        mouths.faces(&[face(130.0)]);
        mouths.frame(&picture(1, 0), 650 * MS);
        assert_eq!(mouths.scores(650 * MS).remove(0).unwrap().frames, 6);
        // Nothing read for longer than the window: the frames are too old.
        assert_eq!(mouths.scores(600 * MS + WINDOW_US + 1), vec![None]);
        // A face elsewhere starts afresh.
        mouths.faces(&[face(220.0)]);
        assert_eq!(mouths.scores(600 * MS), vec![None]);
        mouths.faces(&[]);
        assert!(!mouths.any());
    }

    #[test]
    fn at_the_lamps_rates_a_face_in_view_always_has_a_score_of_about_eight_frames() {
        // As measured on lamp-52e6: a frame every 143 ms, a look every
        // 250 ms, said about 190 ms after the newest frame was taken.
        let mut mouths = Mouths::default();
        let mut said = Vec::new();
        let mut look_at = 0;
        for frame in 0..60u64 {
            let taken = frame * 143 * MS;
            if look_at <= taken {
                mouths.faces(&[face(100.0)]);
                said.push(mouths.scores(taken + 190 * MS).remove(0));
                look_at += 250 * MS;
            }
            mouths.frame(&picture(frame as usize % 2, 0), taken);
        }
        // After the first second, every look has a score, of 7 to 9 frames.
        for score in &said[5..] {
            let score = score.as_ref().unwrap();
            assert!((7..=9).contains(&score.frames), "{score:?}");
            assert!(score.moving > 0.8, "{score:?}");
        }
    }

    #[test]
    fn regions_come_from_the_marks_and_a_face_off_the_picture_is_not_read() {
        let (lower, eyes) = regions(&face(100.0).marks).unwrap();
        assert_eq!(lower, [116.0, 106.0, 164.0, 162.0]);
        assert!(eyes[1] < lower[1]);
        let mut flat = face(100.0).marks;
        flat[3] = flat[4];
        assert_eq!(regions(&flat), None);
        // Past the right edge.
        let mut mouths = Mouths::default();
        mouths.faces(&[face(290.0)]);
        for frame in 0..5u64 {
            mouths.frame(&picture(frame as usize % 2, 0), frame * 100 * MS);
        }
        assert_eq!(mouths.scores(400 * MS), vec![None]);
        assert!(Grey::from_luma(vec![0; 5], 2, 2).is_err());
        assert!(Grey::from_jpeg(b"not a jpeg").is_err());
    }

    #[test]
    fn the_share_counts_points_changed_by_at_least_the_step() {
        let before = [100.0; 8];
        // Changes of 0, 7, 8, 11, 12, 15, 16 and -20 grey.
        let now = [100.0, 107.0, 108.0, 111.0, 112.0, 115.0, 116.0, 80.0];
        assert_eq!(share(&now, &before, 8.0), 6.0 / 8.0);
        assert_eq!(share(&now, &before, 12.0), 4.0 / 8.0);
        assert_eq!(share(&now, &before, 16.0), 2.0 / 8.0);
        let raw = Raw::of(7, (0, 0), (&now, &before), 10.0, 10.0);
        assert_eq!(raw.lower_mean, 89.0 / 8.0);
        assert_eq!(raw.eyes_mean, 10.0);
        // Half the points past 12 is more than the full share: 1.
        assert_eq!(raw.moved(), 1.0);
        assert!(!raw.head_moved(), "10 grey is still a still head");
        let tenth = [
            100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 120.0,
        ];
        let raw = Raw::of(7, (0, 0), (&tenth, &[100.0; 10]), 0.0, 0.0);
        assert!((raw.moved() - 0.1 / FULL_SHARE).abs() < 1e-6);
    }

    #[test]
    fn a_frame_whose_eyes_moved_is_left_out_of_the_score_but_kept_raw() {
        let mut mouths = Mouths::default();
        mouths.faces(&[face(100.0)]);
        let moving = |eyes| Raw {
            taken_us: 0,
            shift_x: 0,
            shift_y: 0,
            lower_mean: 20.0,
            lower_share8: 0.3,
            lower_share12: 0.15,
            lower_share16: 0.1,
            eyes_mean: eyes,
            eyes_after: eyes,
        };
        // Four still frames with the mouth moving fully, two with the head
        // moving, whose lower face is all change.
        for (frame, eyes) in [2.0, 3.0, 11.0, 2.0, 40.0, 1.0].into_iter().enumerate() {
            let mut raw = moving(eyes);
            raw.taken_us = frame as u64 * 100 * MS;
            if eyes > HEAD_MOVED {
                raw.lower_share12 = 1.0;
            }
            mouths.tracks[0].moved.push_back(raw);
        }
        let score = mouths.scores(500 * MS).remove(0).unwrap();
        assert_eq!((score.frames, score.head_moved, score.raw.len()), (6, 2, 6));
        assert!((score.moving - 1.0).abs() < 1e-6, "{score:?}");
        // With the still frames' mouths half as open: half.
        for raw in &mut mouths.tracks[0].moved {
            if !raw.head_moved() {
                raw.lower_share12 = FULL_SHARE / 2.0;
            }
        }
        let score = mouths.scores(500 * MS).remove(0).unwrap();
        assert!((score.moving - 0.5).abs() < 1e-6, "{score:?}");
    }

    #[test]
    fn the_message_is_exactly_this_text() {
        let said = MouthsMoving {
            mouths: vec![Mouth {
                id: "f3".into(),
                moving: 0.5,
                window_ms: 1200,
                frames: 8,
                age_ms: 40,
                head_moved: 0,
                raw: None,
            }],
        };
        let text =
            r#"{"mouths":[{"id":"f3","moving":0.5,"window_ms":1200,"frames":8,"age_ms":40}]}"#;
        assert_eq!(serde_json::to_string(&said).unwrap(), text);
        // A reader of the first version reads the new fields as absent.
        assert_eq!(serde_json::from_str::<MouthsMoving>(text).unwrap(), said);
        let mut with_raw = said.clone();
        with_raw.mouths[0].head_moved = 1;
        with_raw.mouths[0].raw = Some(vec![Raw {
            taken_us: 5,
            shift_x: -2,
            shift_y: 1,
            lower_mean: 2.5,
            lower_share8: 0.25,
            lower_share12: 0.125,
            lower_share16: 0.0,
            eyes_mean: 9.5,
            eyes_after: 1.5,
        }]);
        assert_eq!(
            serde_json::to_string(&with_raw).unwrap(),
            r#"{"mouths":[{"id":"f3","moving":0.5,"window_ms":1200,"frames":8,"age_ms":40,"head_moved":1,"raw":[{"taken_us":5,"shift_x":-2,"shift_y":1,"lower_mean":2.5,"lower_share8":0.25,"lower_share12":0.125,"lower_share16":0.0,"eyes_mean":9.5,"eyes_after":1.5}]}]}"#
        );
    }
}
