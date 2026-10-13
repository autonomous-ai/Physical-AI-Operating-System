//! From the faces in a picture to what the body is told: where the one face
//! the lamp should look at is.
use crate::faces::Face;
use serde::{Deserialize, Serialize};

/// A face smaller than this share of the picture's height is somebody across
/// the room, not somebody at the lamp.
pub const LEAST_SIZE: f32 = 0.10;

/// Where a face is. This is everything that leaves the process.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sight {
    /// Its centre from the middle of the picture: -1 at the left edge to 1
    /// at the right, and -1 at the top to 1 at the bottom.
    pub x: f32,
    pub y: f32,
    /// Its height as a share of the picture's.
    pub size: f32,
    /// Its nose from the point between its eyes, in eye distances: about 0
    /// for a face turned to the camera, toward -1 or 1 for one turned away.
    pub turned: f32,
}

/// One datagram to the body: the face to look at, or that there is none.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seen {
    pub face: Option<Sight>,
}

/// The largest face that is near enough. `filled` is how much of the picture
/// the camera frame fills.
pub fn choose(faces: &[Face], filled: (usize, usize)) -> Option<Sight> {
    let (width, height) = (filled.0.max(1) as f32, filled.1.max(1) as f32);
    faces
        .iter()
        .filter(|face| face.height / height >= LEAST_SIZE)
        .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
        .map(|face| {
            let (first, second, nose) = (face.marks[0], face.marks[1], face.marks[2]);
            let between = ((first.0 + second.0) / 2.0, (first.1 + second.1) / 2.0);
            let apart = ((second.0 - first.0).powi(2) + (second.1 - first.1).powi(2))
                .sqrt()
                .max(1.0);
            Sight {
                x: ((face.x + face.width / 2.0) / width * 2.0 - 1.0).clamp(-1.0, 1.0),
                y: ((face.y + face.height / 2.0) / height * 2.0 - 1.0).clamp(-1.0, 1.0),
                size: (face.height / height).min(1.0),
                turned: ((nose.0 - between.0) / apart).clamp(-1.0, 1.0),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(x: f32, y: f32, size: f32, nose: f32) -> Face {
        // Eyes 40% of the width apart, a third of the way down.
        let (left, right) = (x + size * 0.3, x + size * 0.7);
        Face {
            score: 0.9,
            x,
            y,
            width: size,
            height: size,
            marks: [
                (left, y + size / 3.0),
                (right, y + size / 3.0),
                (x + size * nose, y + size * 0.55),
                (left, y + size * 0.8),
                (right, y + size * 0.8),
            ],
        }
    }

    #[test]
    fn the_largest_near_face_is_the_one_and_its_place_is_from_the_middle() {
        let faces = [
            face(10.0, 10.0, 20.0, 0.5),
            face(200.0, 30.0, 60.0, 0.5),
            face(100.0, 100.0, 40.0, 0.5),
        ];
        let sight = choose(&faces, (320, 240)).unwrap();
        // Centre at (230, 60) of 320 by 240.
        assert!((sight.x - 0.4375).abs() < 1e-4 && (sight.y + 0.5).abs() < 1e-4);
        assert!((sight.size - 0.25).abs() < 1e-4);
        assert!(sight.turned.abs() < 1e-4);
        // Nobody near: the small face alone is not looked at.
        assert_eq!(choose(&faces[..1], (320, 240)), None);
        assert_eq!(choose(&[], (320, 240)), None);
    }

    #[test]
    fn a_face_turned_away_shows_in_where_its_nose_is() {
        let ahead = choose(&[face(100.0, 60.0, 80.0, 0.5)], (320, 240)).unwrap();
        let away = choose(&[face(100.0, 60.0, 80.0, 0.75)], (320, 240)).unwrap();
        assert!(ahead.turned.abs() < 0.01);
        assert!((away.turned - 0.625).abs() < 0.01, "{away:?}");
    }

    #[test]
    fn the_datagram_is_exactly_this_text() {
        let seen = Seen {
            face: Some(Sight {
                x: 0.25,
                y: -0.5,
                size: 0.2,
                turned: 0.0,
            }),
        };
        let text = serde_json::to_string(&seen).unwrap();
        assert_eq!(
            text,
            r#"{"face":{"x":0.25,"y":-0.5,"size":0.2,"turned":0.0}}"#
        );
        assert_eq!(serde_json::from_str::<Seen>(&text).unwrap(), seen);
        assert_eq!(
            serde_json::to_string(&Seen { face: None }).unwrap(),
            r#"{"face":null}"#
        );
        assert!(serde_json::from_str::<Seen>(r#"{"face":null,"frame":"x"}"#).is_err());
    }
}
