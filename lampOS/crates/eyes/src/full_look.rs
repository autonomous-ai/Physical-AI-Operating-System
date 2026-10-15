//! A full look (#836): one larger picture, taken when the voice service asks
//! because the lamp said it would look, after the head has stopped moving.
//!
//! The owner held something up at lamp-4ace and asked "what do you see?".
//! The picture that went was the eyes' working frame, 320 by 240: the room,
//! the person side-on, and nothing to tell what was held up. With
//! `--full-look WIDTHxHEIGHT` the eyes' stream runs at that size all the
//! time (the detector shrinks every frame to its own 320 by 256 as it always
//! could), no working frame is kept, and the sight file holds only a still
//! made for one request and removed [`KEPT_US`] after.
//!
//! The stream is not switched to a larger mode for the still and back: on
//! UVC that is a stop and a start, and lamp-52e6's camera often fails to
//! start again for minutes (#763). One mode, opened once, costs a larger
//! decode a look and nothing at the moment of asking.
//!
//! The head is the body's: it already turns to the face the eyes see and to
//! a call. The still waits for it to stop, as `body/joint_states` says, for
//! [`STILL_FOR_US`]: every head joint within [`ARRIVED_RADIANS`] of where it
//! was asked to be (the body's own arrival rule, 6 counts held for 15 steps
//! at 50 Hz), turning slower than [`STILL_RADIANS_A_SECOND`], and asked to
//! be in the same place throughout. A head that moves again before the
//! frame comes is waited for again. No longer than [`SETTLE_AT_MOST_US`] in
//! all. With no body on the bus the still is taken at once.
use lamp_msgs::JointState;
use std::f64::consts::TAU;
use zune_jpeg::{
    JpegDecoder,
    zune_core::{colorspace::ColorSpace, options::DecoderOptions},
};

/// The joints that move the camera.
pub const HEAD: [&str; 3] = ["base_yaw", "head_pitch", "head_roll"];
/// Six of a servo's 4096 counts a turn.
pub const ARRIVED_RADIANS: f64 = 6.0 * TAU / 4_096.0;
/// Two counts in the 80 ms over which the body works out how fast a joint
/// turns: more than a count of jitter, less than a head following a slow
/// move at 50 counts a second.
pub const STILL_RADIANS_A_SECOND: f64 = 25.0 * TAU / 4_096.0;
pub const STILL_FOR_US: u64 = 300_000;
pub const SETTLE_AT_MOST_US: u64 = 2_000_000;
/// No still this long after the ask: there is none for it.
pub const GIVE_UP_US: u64 = 2_500_000;
/// A still is on the lamp no longer than this: the voice service reads it
/// for the one request it was taken for.
pub const KEPT_US: u64 = 3_000_000;
/// The still's longer side at most. What a model is shown is scaled to
/// about this anyway, and a picture this size fits the provider's bound.
pub const LONGEST_SIDE: usize = 1_024;
/// Qualities tried in turn until the picture fits.
const QUALITIES: [u8; 4] = [85, 75, 65, 50];
const MAX_SIDE: usize = 4_096;

/// Exactly `{"look_full":ID}`, the ask's id from 1: anything else asks
/// nothing.
pub fn asked(datagram: &[u8]) -> Option<u64> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Ask {
        look_full: u64,
    }
    serde_json::from_slice::<Ask>(datagram)
        .ok()
        .map(|ask| ask.look_full)
        .filter(|id| *id > 0)
}

/// Bytes a still's id takes: what [`tagged`] adds, at most.
pub const TAG_BYTES: usize = 64;

/// The still with the id of the ask it was taken for, as a JPEG comment
/// right after the start of the image: `lamp-look ID`. The voice service
/// takes a still only with its own ask's id (#842), so a still of an
/// earlier ask written late is never taken for a later one. A number, no
/// more.
pub fn tagged(jpeg: &[u8], id: u64) -> Vec<u8> {
    let text = format!("lamp-look {id}");
    let length = (text.len() + 2) as u16;
    let mut out = Vec::with_capacity(jpeg.len() + text.len() + 4);
    out.extend_from_slice(&jpeg[..2.min(jpeg.len())]);
    out.extend_from_slice(&[0xff, 0xfe]);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(jpeg.get(2..).unwrap_or_default());
    out
}

/// What the body says of the head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Head {
    /// No body says anything: nothing to wait for.
    Absent,
    Moving,
    /// Not moving, and where each head joint was asked to be.
    Still([f64; 3]),
}

/// The head from what the body says: still when every head joint is within
/// [`ARRIVED_RADIANS`] of where it was asked to be and turning slower than
/// [`STILL_RADIANS_A_SECOND`]. A small error alone is not enough: a servo
/// that follows a slow move lags its goal by a count or so all the way.
/// What the body does not say (velocity, goals) is not waited for.
pub fn head(joints: &JointState) -> Head {
    let given = |list: &[f64]| list.len() == joints.name.len();
    let mut goals = [0.0; 3];
    for (at, name) in joints.name.iter().enumerate() {
        let Some(which) = HEAD.iter().position(|head| head == name) else {
            continue;
        };
        let off = given(&joints.commanded)
            && given(&joints.position)
            && (joints.commanded[at] - joints.position[at]).abs() > ARRIVED_RADIANS;
        let turning = given(&joints.velocity) && joints.velocity[at].abs() > STILL_RADIANS_A_SECOND;
        if off || turning {
            return Head::Moving;
        }
        if given(&joints.commanded) {
            goals[which] = joints.commanded[at];
        }
    }
    Head::Still(goals)
}

/// One full look asked for: when the head has settled, and the first frame
/// after that is the still.
#[derive(Debug)]
pub struct Wanted {
    asked_us: u64,
    /// Since when the head has been still, and where it was asked to be.
    still_since: Option<(u64, [f64; 3])>,
    /// When it settled, and how many frames there had been by then.
    settled: Option<(u64, u64)>,
}

impl Wanted {
    pub fn new(now_us: u64) -> Self {
        Self {
            asked_us: now_us,
            still_since: None,
            settled: None,
        }
    }

    /// What the body says of the head now. `frames` is how many frames the
    /// camera has given so far. A head that moves again before the still is
    /// taken is waited for again, until [`SETTLE_AT_MOST_US`].
    pub fn body(&mut self, head: Head, now_us: u64, frames: u64) {
        let capped = now_us.saturating_sub(self.asked_us) >= SETTLE_AT_MOST_US;
        let mut moved = false;
        let settled = match head {
            Head::Absent => true,
            Head::Moving => {
                moved = true;
                false
            }
            Head::Still(goals) => {
                // Goals that move are a head still being turned.
                moved = self.still_since.is_some_and(|(_, was)| {
                    was.iter()
                        .zip(goals)
                        .any(|(was, now)| (was - now).abs() > ARRIVED_RADIANS)
                });
                if moved {
                    self.still_since = None;
                }
                let (since, _) = *self.still_since.get_or_insert((now_us, goals));
                now_us.saturating_sub(since) >= STILL_FOR_US
            }
        };
        if moved {
            if matches!(head, Head::Moving) {
                self.still_since = None;
            }
            if !capped {
                self.settled = None;
            }
        }
        if self.settled.is_none() && (settled || capped) {
            self.settled = Some((now_us, frames));
        }
    }

    /// Whether the newest frame, the `frames`th, is the still: the first
    /// one to come after the head settled.
    pub fn take(&self, frames: u64) -> bool {
        self.settled.is_some_and(|(_, by)| frames > by)
    }

    /// How long the head was waited for, once it settled.
    pub fn waited_us(&self) -> Option<u64> {
        self.settled.map(|(at, _)| at.saturating_sub(self.asked_us))
    }

    pub fn given_up(&self, now_us: u64) -> bool {
        now_us.saturating_sub(self.asked_us) >= GIVE_UP_US
    }
}

/// The camera's frame as a picture the provider takes: itself when it is a
/// whole JPEG small enough already, or else decoded, shrunk to
/// [`LONGEST_SIDE`] by averaging and encoded at the best quality that fits
/// in `most_bytes`. With its size, across and down.
pub fn fit(frame: &[u8], most_bytes: usize) -> Result<(Vec<u8>, (usize, usize)), String> {
    let options = DecoderOptions::default()
        .set_max_width(MAX_SIDE)
        .set_max_height(MAX_SIDE)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(frame, options);
    decoder
        .decode_headers()
        .map_err(|error| format!("jpeg: {error:?}"))?;
    let (width, height) = decoder.dimensions().ok_or("a jpeg without a size")?;
    if frame.len() <= most_bytes
        && width.max(height) <= LONGEST_SIDE
        && crate::kept::whole_jpeg(frame)
    {
        return Ok((frame.to_vec(), (width, height)));
    }
    let rgb = decoder
        .decode()
        .map_err(|error| format!("jpeg: {error:?}"))?;
    let (mut rgb, mut size) = shrink(&rgb, (width, height), LONGEST_SIDE);
    loop {
        for quality in QUALITIES {
            let mut out = Vec::new();
            jpeg_encoder::Encoder::new(&mut out, quality)
                .encode(
                    &rgb,
                    size.0 as u16,
                    size.1 as u16,
                    jpeg_encoder::ColorType::Rgb,
                )
                .map_err(|error| format!("encoding: {error}"))?;
            if out.len() <= most_bytes {
                return Ok((out, size));
            }
        }
        if size.0.max(size.1) <= 160 {
            return Err("no size fits".into());
        }
        (rgb, size) = shrink(&rgb, size, size.0.max(size.1) * 3 / 4);
    }
}

/// Red, green and blue bytes shrunk so that the longer side is at most
/// `longest`, each pixel the average of those it covers. Never enlarged.
fn shrink(
    rgb: &[u8],
    (width, height): (usize, usize),
    longest: usize,
) -> (Vec<u8>, (usize, usize)) {
    let scale = (width.max(height) as f64 / longest.max(1) as f64).max(1.0);
    let to = (
        ((width as f64 / scale).round() as usize).clamp(1, width),
        ((height as f64 / scale).round() as usize).clamp(1, height),
    );
    let span = |at: usize, of: usize, into: usize| {
        let first = at * of / into;
        (first, ((at + 1) * of / into).max(first + 1).min(of))
    };
    let mut out = Vec::with_capacity(to.0 * to.1 * 3);
    for y in 0..to.1 {
        let (top, bottom) = span(y, height, to.1);
        for x in 0..to.0 {
            let (left, right) = span(x, width, to.0);
            let mut sum = [0u32; 3];
            for row in top..bottom {
                for pixel in
                    rgb[(row * width + left) * 3..(row * width + right) * 3].chunks_exact(3)
                {
                    for channel in 0..3 {
                        sum[channel] += u32::from(pixel[channel]);
                    }
                }
            }
            let count = ((bottom - top) * (right - left)) as u32;
            out.extend(sum.map(|total| ((total + count / 2) / count) as u8));
        }
    }
    (out, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000;

    const COUNT: f64 = TAU / 4_096.0;
    const STILL: Head = Head::Still([0.1, 0.5, 0.4]);

    /// The body's message: the head's yaw `off` from its goal `goal`,
    /// turning at `velocity`; the shoulder far from its goal throughout.
    fn joints(goal: f64, off: f64, velocity: f64) -> JointState {
        let name = ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"];
        JointState {
            name: name.map(str::to_owned).to_vec(),
            position: vec![goal - off, 0.2, 0.3, 0.4, 0.5],
            velocity: vec![velocity, 0.0, 0.0, 0.0, 0.0],
            effort: vec![],
            commanded: vec![goal, 0.2 + 1.0, 0.3, 0.4, 0.5],
        }
    }

    #[test]
    fn only_the_exact_ask_is_one() {
        assert_eq!(asked(br#"{"look_full":42}"#), Some(42));
        for not in [
            &br#"{"look_full":true}"#[..],
            br#"{"look_full":0}"#,
            br#"{"look_full":-1}"#,
            br#"{"camera":true}"#,
            br#"{"look_full":4,"keep":true}"#,
            b"",
        ] {
            assert_eq!(asked(not), None);
        }
    }

    #[test]
    fn a_still_carries_its_ask_s_id_and_stays_a_whole_jpeg() {
        let (still, _) = fit(&camera_frame(320, 240, 80), 64 * 1024 - TAG_BYTES).unwrap();
        let marked = tagged(&still, 42);
        assert!(marked.len() <= still.len() + TAG_BYTES);
        assert!(crate::kept::whole_jpeg(&marked));
        // The exact bytes the voice service looks for after the start.
        let mark = b"\xff\xd8\xff\xfe\x00\x0elamp-look 42";
        assert_eq!(&marked[..mark.len()], &mark[..]);
        assert_eq!(&marked[mark.len()..], &still[2..]);
        assert!(crate::picture::Picture::from_jpeg(&marked).is_ok());
        assert!(tagged(&still, u64::MAX).len() <= still.len() + TAG_BYTES);
    }

    #[test]
    fn the_head_is_still_when_it_is_where_it_was_asked_to_be_and_not_turning() {
        // The shoulder far from its goal does not move the camera's aim
        // as the head joints do, and is not waited for.
        assert_eq!(head(&joints(0.1, 0.0, 0.0)), STILL);
        assert_eq!(head(&joints(0.1, 5.0 * COUNT, COUNT)), STILL);
        assert_eq!(head(&joints(0.1, 12.0 * COUNT, 0.0)), Head::Moving);
        // A count behind its goal but turning at 50 counts a second.
        assert_eq!(head(&joints(0.1, COUNT, 50.0 * COUNT)), Head::Moving);
        let mut unknown = joints(0.1, 1.0, 0.0);
        unknown.commanded.clear();
        unknown.velocity.clear();
        assert_eq!(head(&unknown), Head::Still([0.0; 3]));
    }

    #[test]
    fn a_truly_still_head_gets_the_first_frame_after_it_settles() {
        let mut wanted = Wanted::new(0);
        // Turning toward the person for half a second: no still.
        for at in (0..500).step_by(20) {
            wanted.body(Head::Moving, at * MS, at / 100);
            assert!(!wanted.take(at / 100 + 1));
        }
        // Still from 500 ms: settled at 800, after 8 frames.
        for at in (500..800).step_by(20) {
            wanted.body(STILL, at * MS, 8);
            assert!(!wanted.take(8));
        }
        wanted.body(STILL, 800 * MS, 8);
        assert_eq!(wanted.waited_us(), Some(800 * MS));
        // The frame it had is from before; the next one is the still.
        assert!(!wanted.take(8));
        assert!(wanted.take(9));
    }

    #[test]
    fn a_head_following_a_slow_move_with_a_count_of_lag_is_not_still() {
        // Reviewer's case on #842: 50 counts a second, one count behind,
        // as the body's own publisher reports it every 20 ms.
        let mut wanted = Wanted::new(0);
        for step in 0..100u64 {
            let goal = step as f64 * COUNT;
            let joints = joints(goal, COUNT, 50.0 * COUNT);
            wanted.body(head(&joints), step * 20 * MS, step);
            assert!(!wanted.take(step + 1), "taken at {} ms", step * 20);
        }
        // Even were its speed not told: the goal moves under it.
        let mut wanted = Wanted::new(0);
        for step in 0..100u64 {
            let goal = step as f64 * COUNT;
            let mut joints = joints(goal, COUNT, 0.0);
            joints.velocity.clear();
            wanted.body(head(&joints), step * 20 * MS, step);
            assert!(!wanted.take(step + 1), "taken at {} ms", step * 20);
        }
        // The 2 s bound still holds.
        wanted.body(Head::Moving, SETTLE_AT_MOST_US, 100);
        assert!(wanted.take(101));
    }

    #[test]
    fn a_head_that_moves_again_before_the_frame_is_waited_for_again() {
        // Reviewer's case on #842: settled at 300 ms, moving at 350, a
        // frame at 400.
        let mut wanted = Wanted::new(0);
        wanted.body(STILL, 0, 0);
        wanted.body(STILL, 300 * MS, 3);
        assert_eq!(wanted.waited_us(), Some(300 * MS));
        wanted.body(Head::Moving, 350 * MS, 3);
        assert!(!wanted.take(4));
        assert_eq!(wanted.waited_us(), None);
        // So does one asked to be somewhere else while it seemed still.
        wanted.body(STILL, 400 * MS, 4);
        wanted.body(Head::Still([0.2, 0.5, 0.4]), 600 * MS, 5);
        wanted.body(Head::Still([0.2, 0.5, 0.4]), 800 * MS, 6);
        assert!(!wanted.take(7));
        wanted.body(Head::Still([0.2, 0.5, 0.4]), 900 * MS, 6);
        assert!(wanted.take(7));
        // After the bound a movement no longer holds the still back.
        let mut late = Wanted::new(0);
        late.body(Head::Moving, SETTLE_AT_MOST_US, 1);
        late.body(Head::Moving, SETTLE_AT_MOST_US + 20 * MS, 1);
        assert!(late.take(2));
    }

    #[test]
    fn a_head_that_never_settles_or_no_body_still_gets_a_still() {
        let mut moving = Wanted::new(0);
        for at in (0..SETTLE_AT_MOST_US).step_by(20_000) {
            moving.body(Head::Moving, at, 1);
        }
        assert!(!moving.take(2));
        moving.body(Head::Moving, SETTLE_AT_MOST_US, 1);
        assert!(moving.take(2));
        // Moving, then still but not long enough: the bound holds.
        let mut late = Wanted::new(0);
        late.body(STILL, SETTLE_AT_MOST_US - 100 * MS, 1);
        late.body(STILL, SETTLE_AT_MOST_US, 1);
        assert_eq!(late.waited_us(), Some(SETTLE_AT_MOST_US));
        // No body: at once.
        let mut alone = Wanted::new(5 * MS);
        alone.body(Head::Absent, 5 * MS, 3);
        assert_eq!(alone.waited_us(), Some(0));
        assert!(alone.take(4));
        assert!(!alone.given_up(5 * MS + GIVE_UP_US - 1));
        assert!(alone.given_up(5 * MS + GIVE_UP_US));
    }

    fn camera_frame(width: usize, height: usize, quality: u8) -> Vec<u8> {
        // A room as a camera gives it: shapes with hard edges, light falling
        // off across them, and a little sensor noise everywhere.
        let mut rgb = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let (u, v) = (x * 64 / width, y * 48 / height);
                let block = ((u * 37 + v * 91) % 7) as u32 * 30;
                let light = (x + y) as u32 * 120 / (width + height) as u32;
                let noise = ((x * 7_919 + y * 104_729) % 13) as u32;
                let value = |shift: u32| ((block + light + noise + shift) % 256) as u8;
                rgb.extend([value(0), value(40), value(90)]);
            }
        }
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, quality)
            .encode(
                &rgb,
                width as u16,
                height as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .unwrap();
        out
    }

    #[test]
    fn a_large_frame_is_made_to_fit_and_a_small_whole_one_goes_as_it_is() {
        let most = 64 * 1024;
        let large = camera_frame(1_280, 960, 90);
        assert!(large.len() > most);
        let started = std::time::Instant::now();
        let (still, size) = fit(&large, most).unwrap();
        eprintln!(
            "1280x960 of {} bytes to {size:?} of {} bytes in {} ms",
            large.len(),
            still.len(),
            started.elapsed().as_millis()
        );
        assert!(still.len() <= most);
        // As large as fits, at most 1024 across, and the same shape.
        assert!(size.0 <= LONGEST_SIDE && size.0 >= 640 && size.0 * 3 == size.1 * 4);
        assert!(crate::kept::whole_jpeg(&still));
        let small = camera_frame(320, 240, 80);
        assert!(small.len() <= most);
        assert_eq!(fit(&small, most).unwrap(), (small.clone(), (320, 240)));
        assert!(fit(b"not a jpeg", most).is_err());
    }

    #[test]
    fn shrinking_averages_what_each_pixel_covers() {
        // Two by two of black and white is one grey pixel.
        let rgb = [0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0];
        assert_eq!(shrink(&rgb, (2, 2), 1), (vec![128, 128, 128], (1, 1)));
        // Never enlarged.
        assert_eq!(shrink(&rgb, (2, 2), 10), (rgb.to_vec(), (2, 2)));
    }
}
