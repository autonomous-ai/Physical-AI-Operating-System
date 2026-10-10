//! YuNet face detection through `tract-onnx` (pure Rust, no native runtime).
//!
//! Decoding follows OpenCV's `FaceDetectorYN` for the 2023mar model: per
//! stride, score = sqrt(clamp(cls) * clamp(obj)); box centre/size and five
//! landmarks are offsets on the stride grid. Greedy NMS follows. Landmark
//! order is the subject's right eye, left eye, nose tip, right mouth corner,
//! left mouth corner.
use crate::model::ModelError;
use std::{io::Cursor, sync::Arc};
use tract_onnx::prelude::*;

pub const STRIDES: [usize; 3] = [8, 16, 32];
/// ONNX graph outputs, in the order this decoder consumes them.
pub const OUTPUTS: [&str; 12] = [
    "cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16", "bbox_32",
    "kps_8", "kps_16", "kps_32",
];
/// V1's detection threshold was 0.35 for tracking recall; presence evidence
/// reports scores from this floor and leaves policy to the interaction owner.
pub const SCORE_THRESHOLD: f32 = 0.5;
pub const NMS_THRESHOLD: f32 = 0.3;
pub const MAX_CANDIDATES: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detection {
    pub score: f32,
    /// Top-left x, y and width, height in detector-input pixels.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub landmarks: [[f32; 2]; 5],
}

impl Detection {
    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }
    fn iou(&self, other: &Detection) -> f32 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let union = self.area() + other.area() - inter;
        if union <= 0.0 { 0.0 } else { inter / union }
    }
}

/// Decode the twelve outputs for an input of `width` x `height` pixels.
/// Non-finite values are skipped rather than propagated.
pub fn decode(
    outputs: &[&[f32]; 12],
    width: usize,
    height: usize,
    threshold: f32,
) -> Vec<Detection> {
    let mut found = Vec::new();
    for (index, stride) in STRIDES.iter().copied().enumerate() {
        let cols = width / stride;
        let rows = height / stride;
        let cells = cols * rows;
        let (cls, obj, bbox, kps) = (
            outputs[index],
            outputs[index + 3],
            outputs[index + 6],
            outputs[index + 9],
        );
        if cls.len() < cells
            || obj.len() < cells
            || bbox.len() < cells * 4
            || kps.len() < cells * 10
        {
            continue;
        }
        for row in 0..rows {
            for col in 0..cols {
                let cell = row * cols + col;
                let score = (cls[cell].clamp(0.0, 1.0) * obj[cell].clamp(0.0, 1.0)).sqrt();
                if !score.is_finite() || score < threshold {
                    continue;
                }
                let s = stride as f32;
                let b = &bbox[cell * 4..cell * 4 + 4];
                let cx = (col as f32 + b[0]) * s;
                let cy = (row as f32 + b[1]) * s;
                let w = b[2].exp() * s;
                let h = b[3].exp() * s;
                let k = &kps[cell * 10..cell * 10 + 10];
                let mut landmarks = [[0.0; 2]; 5];
                for (n, point) in landmarks.iter_mut().enumerate() {
                    *point = [(k[2 * n] + col as f32) * s, (k[2 * n + 1] + row as f32) * s];
                }
                let detection = Detection {
                    score,
                    x: cx - w / 2.0,
                    y: cy - h / 2.0,
                    w,
                    h,
                    landmarks,
                };
                if [detection.x, detection.y, w, h]
                    .iter()
                    .chain(landmarks.iter().flatten())
                    .all(|v| v.is_finite())
                {
                    found.push(detection);
                }
            }
        }
    }
    found
}

/// Greedy non-maximum suppression, highest score first.
pub fn suppress(mut detections: Vec<Detection>, iou_threshold: f32) -> Vec<Detection> {
    detections.sort_by(|a, b| b.score.total_cmp(&a.score));
    detections.truncate(MAX_CANDIDATES);
    let mut kept: Vec<Detection> = Vec::new();
    for detection in detections {
        if kept.iter().all(|k| k.iou(&detection) <= iou_threshold) {
            kept.push(detection);
        }
    }
    kept
}

/// One loaded, optimized plan for a fixed input size.
pub struct FaceDetector {
    plan: Arc<TypedRunnableModel>,
    pub width: usize,
    pub height: usize,
}

impl FaceDetector {
    /// Load verified model bytes for a `width` x `height` input (multiples of 32).
    /// The fixed 640x640 shape annotations inside the file are ignored so the
    /// graph is re-inferred for the actual input, as OpenCV does.
    pub fn load(model: &[u8], width: usize, height: usize) -> Result<Self, ModelError> {
        if width == 0
            || height == 0
            || !width.is_multiple_of(32)
            || !height.is_multiple_of(32)
            || width > 1280
            || height > 1280
        {
            return Err(ModelError::Load(
                "input size must be positive multiples of 32".into(),
            ));
        }
        let load = |e: TractError| ModelError::Load(e.to_string());
        let model = tract_onnx::onnx()
            .with_ignore_value_info(true)
            .with_ignore_output_shapes(true)
            .model_for_read(&mut Cursor::new(model))
            .map_err(load)?;
        let outlets = model.output_outlets().map_err(load)?;
        let labels: Vec<_> = outlets
            .iter()
            .map(|outlet| model.outlet_label(*outlet).unwrap_or_default().to_owned())
            .collect();
        if labels != OUTPUTS {
            return Err(ModelError::Load(format!(
                "unexpected model outputs {labels:?}"
            )));
        }
        let plan = model
            .with_input_fact(0, f32::fact([1, 3, height, width]).into())
            .map_err(load)?
            .into_optimized()
            .map_err(load)?
            .into_runnable()
            .map_err(load)?;
        Ok(Self {
            plan,
            width,
            height,
        })
    }

    /// Run on planar BGR float input of exactly `3 * width * height` values.
    pub fn detect(&self, input: &[f32], threshold: f32) -> Result<Vec<Detection>, ModelError> {
        if input.len() != 3 * self.width * self.height {
            return Err(ModelError::Load("detector input has the wrong size".into()));
        }
        let tensor = Tensor::from_shape(&[1, 3, self.height, self.width], input)
            .map_err(|e| ModelError::Load(e.to_string()))?;
        let outputs = self
            .plan
            .run(tvec!(tensor.into()))
            .map_err(|e| ModelError::Load(e.to_string()))?;
        if outputs.len() != OUTPUTS.len() {
            return Err(ModelError::Load(
                "model returned the wrong number of outputs".into(),
            ));
        }
        let mut slices: Vec<&[f32]> = Vec::with_capacity(OUTPUTS.len());
        for output in outputs.iter() {
            slices.push(
                output
                    .try_as_plain_ram()
                    .and_then(|view| view.as_slice::<f32>())
                    .map_err(|e| ModelError::Load(e.to_string()))?,
            );
        }
        let slices: [&[f32]; 12] = slices.try_into().expect("checked length");
        Ok(suppress(
            decode(&slices, self.width, self.height, threshold),
            NMS_THRESHOLD,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic outputs for a 64x64 input with one face cell per stride-8 grid.
    fn outputs(cells: &[(usize, f32, [f32; 4])]) -> Vec<Vec<f32>> {
        let (w, h) = (64, 64);
        let mut out = Vec::new();
        for kind in 0..4 {
            for stride in STRIDES {
                let n = (w / stride) * (h / stride);
                let per = [1, 1, 4, 10][kind];
                let mut values = vec![0.0; n * per];
                if stride == 8 {
                    for &(cell, score, bbox) in cells {
                        match kind {
                            0 | 1 => values[cell] = score,
                            2 => values[cell * 4..cell * 4 + 4].copy_from_slice(&bbox),
                            _ => {
                                for p in 0..5 {
                                    values[cell * 10 + 2 * p] = 0.1 * p as f32;
                                    values[cell * 10 + 2 * p + 1] = 0.2;
                                }
                            }
                        }
                    }
                }
                out.push(values);
            }
        }
        out
    }

    fn slices(out: &[Vec<f32>]) -> [&[f32]; 12] {
        std::array::from_fn(|i| out[i].as_slice())
    }

    #[test]
    fn decoding_matches_the_opencv_grid_formulas() {
        // Cell 9 on the 8x8 grid is row 1, column 1.
        let out = outputs(&[(9, 0.81, [0.5, 0.25, 0.0, (2.0_f32).ln()])]);
        let found = decode(&slices(&out), 64, 64, 0.5);
        assert_eq!(found.len(), 1);
        let d = found[0];
        // score = sqrt(0.81 * 0.81)
        assert!((d.score - 0.81).abs() < 1e-6);
        // cx = (1 + 0.5) * 8 = 12, w = e^0 * 8 = 8 -> x = 8
        assert!((d.x - 8.0).abs() < 1e-5 && (d.w - 8.0).abs() < 1e-5);
        // cy = (1 + 0.25) * 8 = 10, h = 2 * 8 = 16 -> y = 2
        assert!((d.y - 2.0).abs() < 1e-5 && (d.h - 16.0).abs() < 1e-5);
        // Landmark n: ((0.1 n + 1) * 8, (0.2 + 1) * 8)
        assert!((d.landmarks[2][0] - 9.6).abs() < 1e-5);
        assert!((d.landmarks[2][1] - 9.6).abs() < 1e-5);
    }

    #[test]
    fn scores_below_the_threshold_and_non_finite_values_are_dropped() {
        let mut out = outputs(&[(9, 0.49, [0.0; 4]), (10, 0.9, [0.0, 0.0, f32::NAN, 0.0])]);
        assert!(decode(&slices(&out), 64, 64, 0.5).is_empty());
        // Scores outside 0..1 are clamped before the geometric mean.
        out[0][9] = 4.0;
        out[3][9] = 0.36;
        let found = decode(&slices(&out), 64, 64, 0.5);
        assert!((found[0].score - 0.6).abs() < 1e-6);
    }

    #[test]
    fn suppression_keeps_the_best_of_overlapping_boxes() {
        let base = Detection {
            score: 0.9,
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            landmarks: [[0.0; 2]; 5],
        };
        let overlapping = Detection {
            score: 0.8,
            x: 1.0,
            ..base
        };
        let separate = Detection {
            score: 0.7,
            x: 30.0,
            ..base
        };
        let kept = suppress(vec![overlapping, separate, base], NMS_THRESHOLD);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].score, 0.9);
        assert_eq!(kept[1].score, 0.7);
    }
}
