//! The lamp's eyes: where a face is in what its camera sees.
//!
//! [`picture`] turns a camera frame into what the detector takes, [`faces`]
//! runs the detector and makes its outputs into boxes, and [`sight`] picks
//! the one face the lamp should look at and says where it is. `lamp-eyes`
//! (`src/main.rs`) owns the camera and sends that to the body. No frame is
//! stored and none leaves the process.
pub mod faces;
pub mod picture;
pub mod sight;
