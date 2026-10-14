//! Gestures: short recorded moves of the whole lamp, played on top of the
//! pose a standing lamp is in. A nod, a shake of the head, a greeting.
//!
//! A clip is frames of offsets in encoder counts for head pitch, head roll,
//! base yaw, shoulder and elbow, a fixed time apart. It begins and ends at
//! nothing, so it can be laid over whatever the lamp is doing and leaves it
//! where it was. The clips here were recorded on the old runtime
//! (`hal/recordings/`) and made smaller and slower for this body; `data.rs`
//! is generated from them. Pure data and arithmetic: when one may play, how
//! far a joint may be taken and what is watched while it plays belong to
//! the engine that asks.

#[rustfmt::skip]
mod data;
pub use data::*;

/// One recorded gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    pub name: &'static str,
    /// Time from one frame to the next.
    pub step_ms: u16,
    /// Head pitch, head roll, base yaw, shoulder, elbow: counts from the
    /// pose the lamp is in. The first and the last frame are nothing.
    pub frames: &'static [[i16; 5]],
}

impl Clip {
    pub fn duration_us(&self) -> u64 {
        (self.frames.len().saturating_sub(1) as u64) * u64::from(self.step_ms) * 1_000
    }

    /// The offsets this long after the clip began, between its frames, or
    /// none once it is over.
    pub fn at(&self, elapsed_us: u64) -> Option<[f32; 5]> {
        if elapsed_us >= self.duration_us() {
            return None;
        }
        let step_us = u64::from(self.step_ms) * 1_000;
        let frame = (elapsed_us / step_us) as usize;
        let share = (elapsed_us % step_us) as f32 / step_us as f32;
        let (from, to) = (self.frames[frame], self.frames[frame + 1]);
        Some(std::array::from_fn(|joint| {
            f32::from(from[joint]) + (f32::from(to[joint]) - f32::from(from[joint])) * share
        }))
    }
}

/// A glance for a call with nobody in view: the base turns a little to one
/// side and a little to the other and back, in about a second, as a person
/// called from somewhere they cannot see looks round once and lets it go.
/// Some 60 counts (five degrees) each way. Not one of [`ALL`]: the body
/// plays it for a call (`Limits::look_at_name`), nobody asks for it by name.
pub const CALLED: Clip = Clip {
    name: "called",
    step_ms: 100,
    frames: &[
        [0, 0, 0, 0, 0],
        [0, 0, 25, 0, 0],
        [0, 0, 50, 0, 0],
        [0, 0, 60, 0, 0],
        [0, 0, 50, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, -50, 0, 0],
        [0, 0, -60, 0, 0],
        [0, 0, -50, 0, 0],
        [0, 0, -25, 0, 0],
        [0, 0, 0, 0, 0],
    ],
};

/// The clip of this name.
pub fn named(name: &str) -> Option<&'static Clip> {
    ALL.into_iter().find(|clip| clip.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fastest any clip asks each joint to go, counts a second, and the
    /// farthest it takes each from the pose: what the generator promises.
    const FASTEST: [f32; 5] = [900.0, 1_300.0, 1_000.0, 800.0, 800.0];
    const FARTHEST: [i16; 5] = [200, 200, 150, 350, 350];

    #[test]
    fn every_clip_begins_and_ends_at_nothing_and_stays_inside_its_speed_and_size() {
        assert!(ALL.len() >= 4);
        for clip in ALL {
            assert_eq!(named(clip.name), Some(clip));
            assert!(
                clip.frames.len() >= 2 && clip.step_ms >= 40,
                "{}",
                clip.name
            );
            assert_eq!(clip.frames[0], [0; 5], "{}", clip.name);
            assert_eq!(clip.frames[clip.frames.len() - 1], [0; 5], "{}", clip.name);
            assert!(
                (1_500_000..=10_000_000).contains(&clip.duration_us()),
                "{}",
                clip.name
            );
            let step = f32::from(clip.step_ms) / 1_000.0;
            for pair in clip.frames.windows(2) {
                for joint in 0..5 {
                    // A count of rounding either way at each end.
                    let speed =
                        (f32::from(pair[1][joint]) - f32::from(pair[0][joint])).abs() / step;
                    assert!(
                        speed <= FASTEST[joint] + 2.0 / step,
                        "{} joint {joint}: {speed}",
                        clip.name
                    );
                    assert!(
                        pair[1][joint].abs() <= FARTHEST[joint],
                        "{} joint {joint}",
                        clip.name
                    );
                }
            }
        }
        assert_eq!(named("dance"), None);
    }

    #[test]
    fn a_clip_is_read_between_its_frames_and_is_over_at_its_end() {
        const TWO: Clip = Clip {
            name: "two",
            step_ms: 100,
            frames: &[[0, 0, 0, 0, 0], [10, -20, 0, 40, 0], [0, 0, 0, 0, 0]],
        };
        assert_eq!(TWO.duration_us(), 200_000);
        assert_eq!(TWO.at(0), Some([0.0; 5]));
        assert_eq!(TWO.at(50_000), Some([5.0, -10.0, 0.0, 20.0, 0.0]));
        assert_eq!(TWO.at(100_000), Some([10.0, -20.0, 0.0, 40.0, 0.0]));
        assert_eq!(TWO.at(150_000), Some([5.0, -10.0, 0.0, 20.0, 0.0]));
        assert_eq!(TWO.at(200_000), None);
        assert_eq!(TWO.at(u64::MAX), None);
    }
}
