//! Process-level simulation: the real runtime on kernel-simulated hardware.
//!
//! `room` is the portable mixing core and its control protocol; `linux` runs
//! it on the ALSA loopback card; `player` delivers runner stimuli to it.
//! Evidence from this path is the `process_simulation` stratum: real process
//! graph and real code, digital room, no acoustic or device claim.
#[cfg(target_os = "linux")]
pub mod linux;
pub mod player;
pub mod room;
