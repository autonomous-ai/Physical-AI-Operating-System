//! Whether a face's mouth is moving: the sign that someone is speaking that
//! the lamp's own voice cannot fake. A mouth that moves while the lamp talks
//! is not the lamp's echo.
//!
//! No model beyond the face detector. The detector gives the nose and the
//! two corners of the mouth; between frames, how much the lower face
//! changes, over how much the eyes change, is the mouth's own movement:
//! a head that turns or a light that flickers moves both alike. It is the
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
/// This many grey levels of change in the lower face, over the eyes' own,
/// is a mouth moving fully. Not measured: a first guess, for the
/// recordings with people to set.
pub const FULL: f32 = 6.0;
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

    /// A region read as `GRID` points; none if it leaves the picture.
    fn read(&self, region: [f32; 4]) -> Option<Vec<f32>> {
        let [left, top, right, bottom] = region;
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
    // From the nose down past the chin's start, a little wider than the lips.
    let lower = [mouth.0 - wide, nose.1, mouth.0 + wide, mouth.1 + drop * 1.5];
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

fn overlap(a: &Face, b: &Face) -> f32 {
    let across = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let down = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    let shared = across.max(0.0) * down.max(0.0);
    shared / (a.width * a.height + b.width * b.height - shared).max(1e-6)
}

struct Track {
    face: Face,
    regions: Option<([f32; 4], [f32; 4])>,
    before: Option<(Vec<f32>, Vec<f32>)>,
    /// When each frame was taken, and how far its mouth moved, 0 to 1.
    moved: VecDeque<(u64, f32)>,
}

/// A face's score as it leaves the process.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    pub moving: f32,
    pub frames: usize,
    /// When the newest frame in it was taken.
    pub taken_us: u64,
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
            let now = grey.read(lower).zip(grey.read(eyes));
            if let (Some((lower_now, eyes_now)), Some((lower_before, eyes_before))) =
                (&now, &track.before)
            {
                let own = change(lower_now, lower_before) - change(eyes_now, eyes_before);
                track
                    .moved
                    .push_back((taken_us, (own / FULL).clamp(0.0, 1.0)));
            }
            track.before = now;
            while track
                .moved
                .front()
                .is_some_and(|(at, _)| taken_us.saturating_sub(*at) > WINDOW_US)
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
                let &(newest, _) = track.moved.back()?;
                let frames = track.moved.len();
                (frames >= LEAST_FRAMES && now_us.saturating_sub(newest) <= WINDOW_US).then(|| {
                    Score {
                        moving: track.moved.iter().map(|(_, moved)| moved).sum::<f32>()
                            / frames as f32,
                        frames,
                        taken_us: newest,
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
        let (width, height) = (320, 240);
        let mut pixels = vec![0u8; width * height];
        for row in 0..height {
            for column in 0..width {
                let lower = (104..150).contains(&row) && (110..170).contains(&column);
                let shift = head + if lower { lips } else { 0 };
                // Skin-like texture: no two neighbours alike.
                pixels[row * width + column] =
                    (((column + shift) * 7 + row * 13) % 5 * 40 + 40) as u8;
            }
        }
        Grey::from_luma(pixels, width, height).unwrap()
    }

    const MS: u64 = 1_000;

    #[test]
    fn a_moving_mouth_scores_high_a_still_face_low_and_a_turning_head_not_as_speech() {
        let mut talking = Mouths::default();
        talking.faces(&[face(100.0)]);
        for frame in 0..6u64 {
            talking.frame(&picture(frame as usize % 2, 0), frame * 100 * MS);
        }
        let score = talking.scores(500 * MS)[0].unwrap();
        assert!(score.moving > 0.8, "{score:?}");
        assert_eq!(score.taken_us, 500 * MS);

        let mut still = Mouths::default();
        still.faces(&[face(100.0)]);
        for frame in 0..6u64 {
            still.frame(&picture(0, 0), frame * 100 * MS);
        }
        assert!(still.scores(500 * MS)[0].unwrap().moving < 0.01);

        // The whole head moves: lower face and eyes alike.
        let mut turning = Mouths::default();
        turning.faces(&[face(100.0)]);
        for frame in 0..6u64 {
            turning.frame(&picture(0, frame as usize % 2), frame * 100 * MS);
        }
        let turning = turning.scores(500 * MS)[0].unwrap().moving;
        assert!(turning < 0.2, "{turning}");
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
        assert_eq!(mouths.scores(600 * MS)[0].unwrap().frames, 6);
        // Moved further, it keeps its frames and is read afresh.
        mouths.faces(&[face(130.0)]);
        mouths.frame(&picture(1, 0), 650 * MS);
        assert_eq!(mouths.scores(650 * MS)[0].unwrap().frames, 6);
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
                said.push(mouths.scores(taken + 190 * MS)[0]);
                look_at += 250 * MS;
            }
            mouths.frame(&picture(frame as usize % 2, 0), taken);
        }
        // After the first second, every look has a score, of 7 to 9 frames.
        for score in &said[5..] {
            let score = score.unwrap();
            assert!((7..=9).contains(&score.frames), "{score:?}");
            assert!(score.moving > 0.8, "{score:?}");
        }
    }

    #[test]
    fn regions_come_from_the_marks_and_a_face_off_the_picture_is_not_read() {
        let (lower, eyes) = regions(&face(100.0).marks).unwrap();
        assert_eq!(lower, [116.0, 106.0, 164.0, 146.0]);
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
    fn the_message_is_exactly_this_text() {
        let said = MouthsMoving {
            mouths: vec![Mouth {
                id: "f3".into(),
                moving: 0.5,
                window_ms: 1200,
                frames: 8,
                age_ms: 40,
            }],
        };
        assert_eq!(
            serde_json::to_string(&said).unwrap(),
            r#"{"mouths":[{"id":"f3","moving":0.5,"window_ms":1200,"frames":8,"age_ms":40}]}"#
        );
    }
}
