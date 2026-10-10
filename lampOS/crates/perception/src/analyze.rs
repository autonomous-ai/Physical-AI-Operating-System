//! One frame in, one bounded visual analysis out.
use crate::{
    attention::{ATTENTION_CONE_DEG, MIN_ATTENTION_FACE_PIXELS, head_yaw_deg, landmarks_in_frame},
    image::{
        ExposureVerdict, ImageError, decode_jpeg, detector_input, detector_width, input_size,
        measure_exposure,
    },
    model::{ModelError, ModelSpec, sha256_hex},
    observation::{
        Attention, FaceEvidence, MAX_FACES, ModelRef, NORMALIZED, Presence, UnknownReason,
    },
    yunet::{Detection, FaceDetector, SCORE_THRESHOLD},
};
use std::{fmt, time::Instant};

/// Detector input width (V1 used 320x320 for the same model).
pub const INPUT_WIDTH: u32 = 320;

#[derive(Debug)]
pub enum AnalysisError {
    Decode(ImageError),
    Inference(ModelError),
}
impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => error.fmt(f),
            Self::Inference(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for AnalysisError {}

#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub exposure: crate::image::Exposure,
    pub presence: Presence,
    /// Largest face first.
    pub faces: Vec<FaceEvidence>,
    pub attention: Attention,
    pub best_score: Option<f32>,
    pub decode_us: u32,
    pub inference_us: u32,
    pub model: ModelRef,
}

pub struct Analyzer {
    model: Vec<u8>,
    id: &'static str,
    sha256_prefix: String,
    input_width: u32,
    detector: Option<FaceDetector>,
}

fn micros(since: Instant) -> u32 {
    u32::try_from(since.elapsed().as_micros()).unwrap_or(u32::MAX)
}

fn normalize(value: f32, extent: u32) -> u16 {
    (value / extent as f32 * NORMALIZED)
        .clamp(0.0, NORMALIZED)
        .round() as u16
}

impl Analyzer {
    /// `model` must already be verified against `spec` (see `model::read_verified`).
    pub fn new(model: Vec<u8>, spec: &ModelSpec, input_width: u32) -> Self {
        Self {
            sha256_prefix: sha256_hex(&model)[..16].to_owned(),
            model,
            id: spec.id,
            input_width,
            detector: None,
        }
    }

    /// Build the plan for frames of this size now rather than on the first
    /// frame. Called at startup so `Ready` implies a runnable model.
    pub fn prepare(&mut self, frame_width: u32, frame_height: u32) -> Result<(), ModelError> {
        let width = detector_width(frame_width, self.input_width);
        let (width, height, _) = input_size(frame_width, frame_height, width);
        if self
            .detector
            .as_ref()
            .is_some_and(|d| (d.width, d.height) == (width as usize, height as usize))
        {
            return Ok(());
        }
        self.detector = None;
        self.detector = Some(FaceDetector::load(
            &self.model,
            width as usize,
            height as usize,
        )?);
        Ok(())
    }

    fn model_ref(&self) -> ModelRef {
        let (input_width, input_height) = self
            .detector
            .as_ref()
            .map_or((0, 0), |d| (d.width as u32, d.height as u32));
        ModelRef {
            id: self.id.to_owned(),
            sha256_prefix: self.sha256_prefix.clone(),
            input_width,
            input_height,
        }
    }

    pub fn analyze(&mut self, jpeg: &[u8]) -> Result<Analysis, AnalysisError> {
        let started = Instant::now();
        let image = decode_jpeg(jpeg).map_err(AnalysisError::Decode)?;
        let exposure = measure_exposure(&image);
        let decode_us = micros(started);
        let unusable = match exposure.verdict {
            ExposureVerdict::Usable => None,
            ExposureVerdict::TooDark => Some(UnknownReason::TooDark),
            ExposureVerdict::Overexposed => Some(UnknownReason::Overexposed),
        };
        if let Some(reason) = unusable {
            // No inference on a frame that cannot show a face reliably.
            return Ok(Analysis {
                exposure,
                presence: Presence::Unknown { reason },
                faces: Vec::new(),
                attention: Attention::Unknown { reason },
                best_score: None,
                decode_us,
                inference_us: 0,
                model: self.model_ref(),
            });
        }
        let inference = Instant::now();
        let input = detector_input(&image, self.input_width);
        self.prepare(image.width, image.height)
            .map_err(AnalysisError::Inference)?;
        let detector = self.detector.as_ref().expect("prepared");
        if (detector.width, detector.height) != (input.width as usize, input.height as usize) {
            return Err(AnalysisError::Inference(ModelError::Load(
                "detector input size mismatch".into(),
            )));
        }
        let mut detections: Vec<Detection> = detector
            .detect(&input.data, SCORE_THRESHOLD)
            .map_err(AnalysisError::Inference)?;
        let inference_us = micros(inference);
        detections.sort_by(|a, b| b.area().total_cmp(&a.area()));
        let best_score = detections.iter().map(|d| d.score).reduce(f32::max);
        let presence = match best_score {
            None => Presence::NoFaceDetected,
            Some(best) => Presence::Faces {
                count: detections.len().min(usize::from(u8::MAX)) as u8,
                best_per_mille: (best * 1000.0).round().clamp(0.0, 1000.0) as u16,
            },
        };
        let to_frame = |v: f32| v / input.scale;
        let faces: Vec<FaceEvidence> = detections
            .iter()
            .take(MAX_FACES)
            .map(|d| FaceEvidence {
                score_per_mille: (d.score * 1000.0).round().clamp(0.0, 1000.0) as u16,
                bounds: [
                    normalize(to_frame(d.x), image.width),
                    normalize(to_frame(d.y), image.height),
                    normalize(to_frame(d.w), image.width),
                    normalize(to_frame(d.h), image.height),
                ],
                landmarks: d.landmarks.map(|[x, y]| {
                    [
                        normalize(to_frame(x), image.width),
                        normalize(to_frame(y), image.height),
                    ]
                }),
                yaw_tenths: head_yaw_deg(&d.landmarks).map(|yaw| (yaw * 10.0).round() as u16),
            })
            .collect();
        let attention = match detections.first() {
            None => Attention::Unknown {
                reason: UnknownReason::NoFace,
            },
            Some(face) if face.h < MIN_ATTENTION_FACE_PIXELS => Attention::Unknown {
                reason: UnknownReason::FaceTooSmall,
            },
            Some(face)
                if !landmarks_in_frame(
                    &face.landmarks,
                    input.content_width as f32,
                    input.content_height as f32,
                ) =>
            {
                Attention::Unknown {
                    reason: UnknownReason::LandmarksOutOfFrame,
                }
            }
            Some(face) => match head_yaw_deg(&face.landmarks) {
                None => Attention::Unknown {
                    reason: UnknownReason::LandmarksDegenerate,
                },
                Some(yaw) => {
                    let yaw_tenths = (yaw * 10.0).round() as u16;
                    if yaw <= ATTENTION_CONE_DEG {
                        Attention::Toward {
                            face: 0,
                            yaw_tenths,
                        }
                    } else {
                        Attention::Away {
                            face: 0,
                            yaw_tenths,
                        }
                    }
                }
            },
        };
        Ok(Analysis {
            exposure,
            presence,
            faces,
            attention,
            best_score,
            decode_us,
            inference_us,
            model: self.model_ref(),
        })
    }
}
