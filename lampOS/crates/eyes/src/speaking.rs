//! Whether a face is speaking, from the five landmarks YuNet already gives
//! (#720): the eyes, the nose tip and the two corners of the mouth. No new
//! model, no picture kept: two numbers a look per face.
//!
//! The mouth's shape is its width and how far its middle sits below the eye
//! line, both over the distance between the eyes. Moving, turning, leaning
//! or coming closer scales and shifts all five marks together, so the shape
//! stays; a speaking mouth changes it several times a second. The score is
//! how much the shape changes from one look to the next over the last
//! [`WINDOW_US`], less what the face's own landmark jitter explains, read in
//! the same window from the distance between the eyes, which speaking does
//! not move.
//!
//! YuNet has no lip points: the opening is read only through the corners,
//! which move down and in or out as the jaw opens and the lips round. On a
//! small face that is about as much as the marks jitter, so a face whose eyes
//! are less than [`LEAST_EYES_PX`] apart, turned more than [`MOST_TURN_DEG`],
//! too new, or with jittering marks is `unknown`, never `no`.
use lamp_msgs::{Speaking, SpeakingState};
use std::collections::{HashMap, VecDeque};

/// Where the eyes read whether to judge speaking. `on` only; with
/// `LAMP_BUS=on`, since it is published there.
pub const ENVIRONMENT: &str = "LAMP_EYES_SPEAKING";
/// How far back a face's mouth is read.
pub const WINDOW_US: u64 = 1_200_000;
/// The looks a window needs before it says anything.
pub const LEAST_LOOKS: usize = 5;
/// Eyes closer than this, in the detector's pixels, have mouth corners whose
/// jitter is as large as the lips' movement.
pub const LEAST_EYES_PX: f32 = 24.0;
/// A face turned further than this hides one corner behind the other.
pub const MOST_TURN_DEG: f32 = 30.0;
/// The distance between the eyes changing by more than this share from one
/// look to the next (root mean square) is landmarks jittering, not a head.
pub const MOST_JITTER: f32 = 0.06;
/// The score, in eye distances a look, from which a mouth is speaking.
pub const SPEAKING_AT: f32 = 0.05;
/// The marks' own jitter moves the mouth's shape about this many times as
/// much as it moves the eye distance (squared, from the geometry of the five
/// points; the synthetic faces in the tests agree).
const JITTER_TO_SHAPE: f32 = 4.0;
/// A face not seen for this long is forgotten.
const FORGET_US: u64 = 3_000_000;

/// Whether the environment asks for it.
pub fn from_environment() -> Option<Mouths> {
    (std::env::var(ENVIRONMENT).ok().as_deref() == Some("on")).then(Mouths::default)
}

/// The mouth's shape: its width and the drop of its middle below the eye
/// line, each over the distance between the eyes, and that distance in
/// pixels. `None` if the eyes sit on one point.
pub fn shape(marks: &[(f32, f32); 5]) -> Option<([f32; 2], f32)> {
    let (eye, other, left, right) = (marks[0], marks[1], marks[3], marks[4]);
    let (across, down) = (other.0 - eye.0, other.1 - eye.1);
    let eyes = across.hypot(down);
    if !eyes.is_finite() || eyes < 1e-3 {
        return None;
    }
    let width = (right.0 - left.0).hypot(right.1 - left.1) / eyes;
    let middle = (
        (left.0 + right.0) / 2.0 - eye.0,
        (left.1 + right.1) / 2.0 - eye.1,
    );
    let drop = (across * middle.1 - down * middle.0).abs() / (eyes * eyes);
    Some(([width, drop], eyes))
}

#[derive(Debug)]
struct Mouth {
    /// When, the shape, and the eye distance in pixels.
    looks: VecDeque<(u64, [f32; 2], f32)>,
}

/// Each face's recent mouth, by its passing id.
#[derive(Debug, Default)]
pub struct Mouths {
    faces: HashMap<u32, Mouth>,
}

const UNKNOWN: Speaking = Speaking {
    state: SpeakingState::Unknown,
    score: 0.0,
};

impl Mouths {
    /// This look of the face `id`: its marks in the detector's pixels and
    /// how far it is turned, in degrees, if that was measured.
    pub fn judge(
        &mut self,
        id: u32,
        marks: &[(f32, f32); 5],
        turn_deg: Option<f32>,
        now_us: u64,
    ) -> Speaking {
        self.faces.retain(|_, mouth| {
            mouth
                .looks
                .back()
                .is_some_and(|last| now_us.saturating_sub(last.0) <= FORGET_US)
        });
        let Some((shape, eyes)) = shape(marks) else {
            return UNKNOWN;
        };
        let mouth = self.faces.entry(id).or_insert(Mouth {
            looks: VecDeque::new(),
        });
        mouth.looks.push_back((now_us, shape, eyes));
        while mouth
            .looks
            .front()
            .is_some_and(|first| now_us.saturating_sub(first.0) > WINDOW_US)
        {
            mouth.looks.pop_front();
        }
        let looks = &mouth.looks;
        let turned = turn_deg.is_none_or(|degrees| degrees > MOST_TURN_DEG);
        let small =
            looks.iter().map(|look| look.2).sum::<f32>() / (looks.len() as f32) < LEAST_EYES_PX;
        if turned || small || looks.len() < LEAST_LOOKS {
            return UNKNOWN;
        }
        let pairs = looks.iter().zip(looks.iter().skip(1));
        let steps = (looks.len() - 1) as f32;
        let jitter = (pairs
            .clone()
            .map(|(a, b)| ((b.2 - a.2) / a.2).powi(2))
            .sum::<f32>()
            / steps)
            .sqrt();
        if jitter > MOST_JITTER {
            return UNKNOWN;
        }
        let change = pairs
            .map(|(a, b)| (b.1[0] - a.1[0]).powi(2) + (b.1[1] - a.1[1]).powi(2))
            .sum::<f32>()
            / steps;
        // The eyes do not move as the mouth speaks: how much their distance
        // changes is this face's own jitter, in this window, taken off.
        let score = (change - JITTER_TO_SHAPE * jitter.powi(2)).max(0.0).sqrt();
        let state = if score >= SPEAKING_AT {
            SpeakingState::Yes
        } else {
            SpeakingState::No
        };
        Speaking { state, score }
    }
}

#[cfg(test)]
mod tests {
    //! Synthetic faces: five points of a head in millimetres, turned,
    //! nodded, moved and scaled, seen about seven times a second (the eyes'
    //! look rate on lamp-52e6), with the marks jittering as a detector's do.
    use super::*;
    use crate::facing;
    use std::f32::consts::TAU;

    /// Eye distance of an adult, millimetres.
    const EYES_MM: f32 = 63.0;
    const LOOK_US: u64 = 150_000;

    struct Noise(u64);
    impl Noise {
        fn uniform(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32 + 0.5) / (1u64 << 24) as f32
        }
        fn gauss(&mut self) -> f32 {
            (-2.0 * self.uniform().ln()).sqrt() * (TAU * self.uniform()).cos()
        }
    }

    #[derive(Clone, Copy)]
    struct Scene {
        /// Eye distance in detector pixels, and its growth a second.
        eyes_px: f32,
        grow: f32,
        /// Turn and nod, amplitude in degrees and frequency.
        yaw: (f32, f32),
        pitch: (f32, f32),
        /// Sideways drift, pixels a second.
        drift: f32,
        /// Whether the mouth speaks: jaw at 5 Hz, rounding at 4.3 Hz.
        talk: bool,
        /// Marks' jitter, pixels, one standard deviation.
        jitter: f32,
    }

    const STILL: Scene = Scene {
        eyes_px: 40.0,
        grow: 0.0,
        yaw: (0.0, 0.0),
        pitch: (0.0, 0.0),
        drift: 0.0,
        talk: false,
        jitter: 0.5,
    };

    fn marks(scene: &Scene, t: f32, noise: &mut Noise) -> [(f32, f32); 5] {
        let open = if scene.talk {
            5.0 * (1.0 - (TAU * 5.0 * t).cos())
        } else {
            0.0
        };
        let round = if scene.talk {
            2.5 * (TAU * 4.3 * t).sin()
        } else {
            0.0
        };
        let half = EYES_MM / 2.0;
        let head = [
            [-half, 0.0, 0.0],
            [half, 0.0, 0.0],
            [0.0, 38.0, -18.0],
            [-25.0 + round, 70.0 + 0.3 * open, -2.0],
            [25.0 - round, 70.0 + 0.3 * open, -2.0],
        ];
        let yaw = (scene.yaw.0 * (TAU * scene.yaw.1 * t).sin()
            + if scene.yaw.1 == 0.0 { scene.yaw.0 } else { 0.0 })
        .to_radians();
        let pitch = (scene.pitch.0 * (TAU * scene.pitch.1 * t).sin()).to_radians();
        let scale = (scene.eyes_px + scene.grow * t) / EYES_MM;
        let (cx, cy) = (160.0 + scene.drift * t, 110.0);
        head.map(|[x, y, z]| {
            let (x, z) = (
                x * yaw.cos() + z * yaw.sin(),
                -x * yaw.sin() + z * yaw.cos(),
            );
            let y = y * pitch.cos() - z * pitch.sin();
            (
                cx + scale * x + scene.jitter * noise.gauss(),
                cy + scale * y + scene.jitter * noise.gauss(),
            )
        })
    }

    /// The states judged over 4 s, after the first window.
    fn run(scene: Scene, seed: u64) -> Vec<Speaking> {
        let (mut mouths, mut noise) = (Mouths::default(), Noise(seed));
        (0..27u64)
            .map(|look| {
                let at = look * LOOK_US;
                let seen = marks(&scene, at as f32 / 1e6, &mut noise);
                (at, mouths.judge(1, &seen, facing::yaw_deg(&seen), at))
            })
            .filter(|(at, _)| *at >= WINDOW_US)
            .map(|(_, judged)| judged)
            .collect()
    }

    fn share(scene: Scene, state: SpeakingState) -> f32 {
        let judged: Vec<Speaking> = (1..=5).flat_map(|seed| run(scene, seed)).collect();
        judged.iter().filter(|said| said.state == state).count() as f32 / judged.len() as f32
    }

    fn mostly(scene: Scene, state: SpeakingState, what: &str) {
        let right = share(scene, state);
        assert!(
            right >= 0.9,
            "{what}: {state:?} in {:.0}% of looks",
            right * 100.0
        );
    }

    #[test]
    fn a_still_face_is_not_speaking() {
        mostly(STILL, SpeakingState::No, "still");
    }

    #[test]
    fn a_nodding_face_is_not_speaking() {
        mostly(
            Scene {
                pitch: (12.0, 0.8),
                ..STILL
            },
            SpeakingState::No,
            "nodding",
        );
    }

    #[test]
    fn a_face_turning_from_side_to_side_is_not_speaking() {
        mostly(
            Scene {
                yaw: (25.0, 0.4),
                ..STILL
            },
            SpeakingState::No,
            "turning",
        );
    }

    #[test]
    fn a_talking_face_is_speaking() {
        mostly(
            Scene {
                talk: true,
                ..STILL
            },
            SpeakingState::Yes,
            "talking",
        );
    }

    #[test]
    fn a_face_talking_while_it_moves_is_speaking() {
        let moving = Scene {
            talk: true,
            yaw: (20.0, 0.4),
            pitch: (10.0, 0.7),
            drift: 30.0,
            ..STILL
        };
        mostly(moving, SpeakingState::Yes, "talking and moving");
    }

    /// Fails if the shape is not taken over the eye distance: a still face
    /// coming closer and drifting across changes every distance in pixels.
    #[test]
    fn a_face_coming_closer_is_not_speaking() {
        mostly(
            Scene {
                grow: 15.0,
                drift: 40.0,
                ..STILL
            },
            SpeakingState::No,
            "coming closer",
        );
    }

    #[test]
    fn a_small_turned_or_jittering_face_is_unknown() {
        mostly(
            Scene {
                eyes_px: 18.0,
                talk: true,
                ..STILL
            },
            SpeakingState::Unknown,
            "small",
        );
        mostly(
            Scene {
                yaw: (55.0, 0.0),
                talk: true,
                ..STILL
            },
            SpeakingState::Unknown,
            "turned",
        );
        mostly(
            Scene {
                jitter: 3.0,
                ..STILL
            },
            SpeakingState::Unknown,
            "jittering",
        );
    }

    #[test]
    fn a_new_face_is_unknown_until_its_window_fills() {
        let (mut mouths, mut noise) = (Mouths::default(), Noise(1));
        for look in 0..(LEAST_LOOKS as u64 - 1) {
            let seen = marks(&STILL, look as f32 * 0.15, &mut noise);
            let said = mouths.judge(7, &seen, Some(0.0), look * LOOK_US);
            assert_eq!(said.state, SpeakingState::Unknown);
        }
    }

    /// The margins, by face size and jitter: `cargo test -p lamp-eyes
    /// speaking::tests::margins -- --nocapture` prints the share of looks
    /// judged right for a still face and a talking one.
    #[test]
    fn margins() {
        for jitter in [0.3, 0.5, 1.0] {
            for eyes_px in [24.0, 30.0, 40.0, 60.0] {
                let still = share(
                    Scene {
                        eyes_px,
                        jitter,
                        ..STILL
                    },
                    SpeakingState::No,
                );
                let talk = share(
                    Scene {
                        eyes_px,
                        jitter,
                        talk: true,
                        ..STILL
                    },
                    SpeakingState::Yes,
                );
                eprintln!(
                    "jitter {jitter} px, eyes {eyes_px} px: still no {still:.2}, talking yes {talk:.2}"
                );
            }
        }
    }
}
