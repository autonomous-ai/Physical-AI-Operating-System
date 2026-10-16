//! The asleep face (#712): the one picture the ring shows without a turn's
//! authority. A faint warm glow at the bottom of the ring that breathes
//! slowly, as a sleeping thing does. It says only that the lamp is on and
//! cannot hear (muted, or the privacy switch closed), so it needs no permit:
//! [`AsleepRing`] can show this picture and black, and nothing else.

use crate::{EncodedFrame, FrameSink, PIXEL_COUNT, Rgb, encode};
use std::io;

/// The pixel at the bottom of the ring: pixel 0 is at the top and they
/// count clockwise seen from in front (measured on lamp-4ace, #712).
pub const BOTTOM: usize = PIXEL_COUNT / 2;
/// The brightest the asleep face gets, at the bottom pixel and the top of a
/// breath: about a twentieth of full, and warm.
pub const PEAK: Rgb = Rgb::new(12, 5, 1);
/// From the bottom pixel outward, how much of [`PEAK`] each neighbour gets.
const SPREAD: [f32; 3] = [1.0, 0.5, 0.2];
/// The least of a breath, as a share of its top.
pub const BREATH_LOW: f32 = 0.3;

/// The asleep face `breath` of the way (0 to 1) from the bottom of a breath
/// to its top. Only the five pixels around [`BOTTOM`] are lit; the rest are
/// dark. Every channel stays at or under [`PEAK`]'s.
pub fn asleep_pixels(breath: f32) -> [Rgb; PIXEL_COUNT] {
    let level = BREATH_LOW + (1.0 - BREATH_LOW) * breath.clamp(0.0, 1.0);
    let mut pixels = [Rgb::BLACK; PIXEL_COUNT];
    for (away, share) in SPREAD.iter().enumerate() {
        let scale = |channel: u8| (f32::from(channel) * share * level).round() as u8;
        let colour = Rgb::new(scale(PEAK.red), scale(PEAK.green), scale(PEAK.blue));
        pixels[BOTTOM + away] = colour;
        pixels[BOTTOM - away] = colour;
    }
    pixels
}

/// A ring writer that shows only the asleep face or black. Opening it sends
/// black; [`AsleepRing::shutdown`] sends black again. It holds no authority
/// and grants none: the voice uses it only while no turn can run, and closes
/// it before its ring worker opens the device again.
pub struct AsleepRing<S: FrameSink> {
    sink: S,
}

impl<S: FrameSink> AsleepRing<S> {
    pub fn new(mut sink: S) -> io::Result<Self> {
        sink.write_frame(&EncodedFrame::black())?;
        Ok(Self { sink })
    }

    /// Show the asleep face `breath` of the way up a breath, under `ceiling`.
    pub fn show(&mut self, breath: f32, ceiling: crate::ChannelCeiling) -> io::Result<()> {
        self.sink
            .write_frame(&encode(&asleep_pixels(breath), ceiling))
    }

    /// Black, then the writer is gone.
    pub fn shutdown(mut self) -> io::Result<()> {
        self.sink.write_frame(&EncodedFrame::black())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelCeiling;

    #[test]
    fn only_the_bottom_five_pixels_are_lit_warm_and_faint() {
        for breath in [0.0, 0.25, 0.5, 1.0] {
            let pixels = asleep_pixels(breath);
            for (index, pixel) in pixels.iter().enumerate() {
                if index.abs_diff(BOTTOM) > 2 {
                    assert_eq!(*pixel, Rgb::BLACK, "pixel {index} at {breath}");
                } else {
                    assert!(pixel.red >= pixel.green && pixel.green >= pixel.blue);
                    assert!(pixel.red <= PEAK.red && pixel.green <= PEAK.green);
                }
            }
        }
        assert_eq!(asleep_pixels(1.0)[BOTTOM], PEAK);
    }

    #[test]
    fn a_breath_rises_from_its_low_to_its_top_and_stays_lit() {
        let low = asleep_pixels(0.0)[BOTTOM].red;
        let high = asleep_pixels(1.0)[BOTTOM].red;
        assert!(low > 0 && low < high, "{low} {high}");
        assert_eq!(asleep_pixels(-1.0), asleep_pixels(0.0));
        assert_eq!(asleep_pixels(2.0), asleep_pixels(1.0));
    }

    struct Frames(Vec<EncodedFrame>);
    impl FrameSink for &mut Frames {
        fn write_frame(&mut self, frame: &EncodedFrame) -> io::Result<()> {
            self.0.push(frame.clone());
            Ok(())
        }
    }

    #[test]
    fn the_writer_begins_and_ends_black_and_shows_nothing_but_the_face() {
        let mut frames = Frames(Vec::new());
        let ceiling = ChannelCeiling::new(48).unwrap();
        let mut ring = AsleepRing::new(&mut frames).unwrap();
        ring.show(0.5, ceiling).unwrap();
        ring.shutdown().unwrap();
        assert_eq!(frames.0.len(), 3);
        assert_eq!(frames.0[0], EncodedFrame::black());
        assert_eq!(frames.0[1], encode(&asleep_pixels(0.5), ceiling));
        assert_eq!(frames.0[2], EncodedFrame::black());
    }

    #[test]
    fn a_low_ceiling_still_caps_it() {
        let ceiling = ChannelCeiling::new(4).unwrap();
        let capped = PEAK.capped(ceiling);
        assert!(capped.red <= 4 && capped.green <= 4);
        assert_eq!(
            encode(&asleep_pixels(1.0), ceiling),
            encode(
                &asleep_pixels(1.0).map(|pixel| pixel.capped(ceiling)),
                ChannelCeiling::new(120).unwrap()
            )
        );
    }
}
