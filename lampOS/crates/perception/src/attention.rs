//! Head yaw toward the camera from YuNet's five landmarks.
//!
//! Ported from V1 `hal/drivers/tracking/gaze.py::head_yaw_deg` (legacy commit
//! `d5efe9d7`): the nose tip's offset along the eye axis, relative to half the
//! eye distance, read as a sine. V1 used a 25° cone (`HAL_GAZE_MAX_YAW_DEG`).
//! This is attention toward the camera, not evidence of being addressed, and
//! the cone has not been qualified on Lamp's camera.

/// V1's gaze cone. Unqualified on V2 hardware.
pub const ATTENTION_CONE_DEG: f32 = 25.0;
/// Below this face height (in detector-input pixels) landmarks are too coarse
/// for a yaw estimate. An unqualified starting value.
pub const MIN_ATTENTION_FACE_PIXELS: f32 = 24.0;

/// Absolute head yaw in degrees, or `None` for degenerate landmarks.
pub fn head_yaw_deg(landmarks: &[[f32; 2]; 5]) -> Option<f32> {
    let [right, left, nose, ..] = *landmarks;
    if !right
        .iter()
        .chain(left.iter())
        .chain(nose.iter())
        .all(|v| v.is_finite())
    {
        return None;
    }
    let (ex, ey) = (left[0] - right[0], left[1] - right[1]);
    let eye_distance = ex.hypot(ey);
    if eye_distance < 1e-6 {
        return None;
    }
    let (mid_x, mid_y) = ((right[0] + left[0]) / 2.0, (right[1] + left[1]) / 2.0);
    let offset = ((nose[0] - mid_x) * ex + (nose[1] - mid_y) * ey) / eye_distance;
    // Beyond the eyes the model has nothing left to say; clamp like V1.
    let ratio = (offset / (eye_distance / 2.0)).clamp(-1.0, 1.0);
    Some(ratio.asin().to_degrees().abs())
}

/// Whether the eyes and nose the yaw is built from lie inside the image.
pub fn landmarks_in_frame(landmarks: &[[f32; 2]; 5], width: f32, height: f32) -> bool {
    landmarks[..3].iter().all(|[x, y]| {
        x.is_finite() && y.is_finite() && *x >= 0.0 && *y >= 0.0 && *x <= width && *y <= height
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(nose_x: f32) -> [[f32; 2]; 5] {
        [
            [40.0, 50.0],
            [60.0, 50.0],
            [nose_x, 60.0],
            [42.0, 70.0],
            [58.0, 70.0],
        ]
    }

    #[test]
    fn a_centred_nose_is_frontal_and_offsets_turn_symmetrically() {
        assert!(head_yaw_deg(&face(50.0)).unwrap() < 1e-4);
        // Offset 5 over half-distance 10: asin(0.5) = 30 degrees, either side.
        assert!((head_yaw_deg(&face(55.0)).unwrap() - 30.0).abs() < 1e-3);
        assert!((head_yaw_deg(&face(45.0)).unwrap() - 30.0).abs() < 1e-3);
        // Past an eye the estimate saturates at 90 degrees, as in V1.
        assert!((head_yaw_deg(&face(80.0)).unwrap() - 90.0).abs() < 1e-3);
    }

    #[test]
    fn degenerate_or_partial_landmarks_give_no_estimate() {
        let mut same = face(50.0);
        same[1] = same[0];
        assert!(head_yaw_deg(&same).is_none());
        let mut nan = face(50.0);
        nan[2][0] = f32::NAN;
        assert!(head_yaw_deg(&nan).is_none());
        assert!(landmarks_in_frame(&face(50.0), 100.0, 100.0));
        assert!(!landmarks_in_frame(&face(50.0), 55.0, 100.0));
    }

    #[test]
    fn rotation_in_the_image_plane_does_not_change_yaw() {
        // A tilted head: the same face rotated 30 degrees about (50, 50).
        let rotate = |[x, y]: [f32; 2]| {
            let (s, c) = 30_f32.to_radians().sin_cos();
            let (dx, dy) = (x - 50.0, y - 50.0);
            [50.0 + dx * c - dy * s, 50.0 + dx * s + dy * c]
        };
        let tilted = face(55.0).map(rotate);
        assert!((head_yaw_deg(&tilted).unwrap() - 30.0).abs() < 1e-3);
    }
}
