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
//! [`STILL_FOR_US`] with every head joint within [`ARRIVED_RADIANS`] of
//! where it was asked to be (the body's own arrival rule, 6 counts held for
//! 15 steps at 50 Hz), and no longer than [`SETTLE_AT_MOST_US`]. With no
//! body on the bus the still is taken at once.
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

/// Exactly `{"look_full":true}`: anything else asks nothing.
pub fn asked(datagram: &[u8]) -> bool {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Ask {
        look_full: bool,
    }
    serde_json::from_slice::<Ask>(datagram).is_ok_and(|ask| ask.look_full)
}

/// Whether the head is where it was asked to be. A body that does not say
/// where its joints were asked to be cannot be waited for: still.
pub fn head_still(joints: &JointState) -> bool {
    joints.commanded.len() != joints.name.len()
        || joints.position.len() != joints.name.len()
        || joints.name.iter().enumerate().all(|(at, name)| {
            !HEAD.contains(&name.as_str())
                || (joints.commanded[at] - joints.position[at]).abs() <= ARRIVED_RADIANS
        })
}

/// One full look asked for: when the head has settled, and the first frame
/// after that is the still.
#[derive(Debug)]
pub struct Wanted {
    asked_us: u64,
    still_since_us: Option<u64>,
    /// When it settled, and how many frames there had been by then.
    settled: Option<(u64, u64)>,
}

impl Wanted {
    pub fn new(now_us: u64) -> Self {
        Self {
            asked_us: now_us,
            still_since_us: None,
            settled: None,
        }
    }

    /// What the body says of the head now: `None` when there is no body to
    /// wait for. `frames` is how many frames the camera has given so far.
    pub fn body(&mut self, head_still: Option<bool>, now_us: u64, frames: u64) {
        if self.settled.is_some() {
            return;
        }
        let settled = match head_still {
            None => true,
            Some(true) => {
                let since = *self.still_since_us.get_or_insert(now_us);
                now_us.saturating_sub(since) >= STILL_FOR_US
            }
            Some(false) => {
                self.still_since_us = None;
                false
            }
        };
        if settled || now_us.saturating_sub(self.asked_us) >= SETTLE_AT_MOST_US {
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

    fn joints(off: f64) -> JointState {
        let name = ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"];
        JointState {
            name: name.map(str::to_owned).to_vec(),
            position: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            velocity: vec![],
            effort: vec![],
            commanded: vec![0.1 + off, 0.2 + 1.0, 0.3, 0.4, 0.5],
        }
    }

    #[test]
    fn only_the_exact_ask_is_one() {
        assert!(asked(br#"{"look_full":true}"#));
        for not in [
            &br#"{"look_full":false}"#[..],
            br#"{"camera":true}"#,
            br#"{"look_full":true,"keep":true}"#,
            b"",
        ] {
            assert!(!asked(not));
        }
    }

    #[test]
    fn the_head_is_still_when_its_joints_are_where_they_were_asked_to_be() {
        // The shoulder far from its goal does not move the camera's aim
        // as the head joints do, and is not waited for.
        assert!(head_still(&joints(0.0)));
        assert!(head_still(&joints(ARRIVED_RADIANS * 0.9)));
        assert!(!head_still(&joints(ARRIVED_RADIANS * 2.0)));
        let mut unknown = joints(1.0);
        unknown.commanded.clear();
        assert!(head_still(&unknown));
    }

    #[test]
    fn the_still_is_the_first_frame_after_the_head_has_turned_and_settled() {
        let mut wanted = Wanted::new(0);
        // Turning toward the person for half a second: no still.
        for at in (0..500).step_by(20) {
            wanted.body(Some(false), at * MS, at / 100);
            assert!(!wanted.take(at / 100 + 1));
        }
        // Still from 500 ms: settled at 800, after 8 frames.
        for at in (500..800).step_by(20) {
            wanted.body(Some(true), at * MS, 8);
            assert!(!wanted.take(8));
        }
        wanted.body(Some(true), 800 * MS, 8);
        assert_eq!(wanted.waited_us(), Some(800 * MS));
        // The frame it had is from before; the next one is the still.
        assert!(!wanted.take(8));
        assert!(wanted.take(9));
        // A movement after settling changes nothing.
        wanted.body(Some(false), 820 * MS, 9);
        assert!(wanted.take(9));
    }

    #[test]
    fn a_head_that_never_settles_or_no_body_still_gets_a_still() {
        let mut moving = Wanted::new(0);
        for at in (0..SETTLE_AT_MOST_US).step_by(20_000) {
            moving.body(Some(false), at, 1);
        }
        assert!(!moving.take(2));
        moving.body(Some(false), SETTLE_AT_MOST_US, 1);
        assert!(moving.take(2));
        // Moving, then still but not long enough: the bound holds.
        let mut late = Wanted::new(0);
        late.body(Some(true), SETTLE_AT_MOST_US - 100 * MS, 1);
        late.body(Some(true), SETTLE_AT_MOST_US, 1);
        assert_eq!(late.waited_us(), Some(SETTLE_AT_MOST_US));
        // No body: at once.
        let mut alone = Wanted::new(5 * MS);
        alone.body(None, 5 * MS, 3);
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
