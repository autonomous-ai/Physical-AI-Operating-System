//! Local visual evidence for the interaction owner: face presence and head
//! attention from fresh camera frames, with original frame lineage.
//!
//! Frames arrive over the camera's bounded frame link (`lamp_camera::frames`).
//! Decoding uses `zune-jpeg`; detection runs the pinned YuNet model through
//! `tract-onnx`, a maintained pure-Rust ONNX runtime with no native library.
//! Darkness, overexposure and missing frames are reported as unknown, never as
//! "nobody is there". Facing the camera is attention evidence, not proof that
//! speech addressed Lamp. This crate has no audio dependency, so a perception
//! fault cannot block hearing, admission or playback.
pub mod analyze;
pub mod attention;
pub mod image;
pub mod model;
pub mod observation;
pub mod worker;
pub mod yunet;
