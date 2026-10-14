//! Runtime integration for a directed conversation qualification session.
//!
//! Speech activity is an observation, not evidence of who is being addressed.
//! The initial harness requires an explicitly opened, finite directed session;
//! this does not qualify open-desk addressee recognition or background restraint.
pub mod activity;
pub mod admission;
pub mod air_told;
pub mod attention;
pub mod body_link;
pub mod bus_out;
pub mod called;
pub mod camera_config;
pub mod camera_inspect;
pub mod camera_worker;
pub mod choreography;
pub mod config;
pub mod diagnostic_control;
pub mod diagnostics;
pub mod expression_out;
pub mod fixture_provider;
pub mod hand_controls;
pub mod head_qualification;
pub mod local_time;
pub mod not_speech;
pub mod options;
#[cfg(target_os = "linux")]
pub mod physical_privacy;
#[cfg(any(target_os = "linux", test))]
mod playback;
pub mod privacy;
pub mod process;
pub(crate) mod provider_flow;
pub mod provider_link;
pub mod provider_worker;
pub mod reference;
pub mod remember;
pub mod ring_face;
pub mod ring_light;
pub mod ring_wire;
pub mod ring_worker;
pub mod room_floor;
pub mod room_told;
pub mod second_look;
pub mod session_fact;
pub mod sight;
pub mod speak_up;
mod termination;
pub mod thread;
pub mod tools;
pub mod transport;
mod turn_taking;
pub mod visual_context;
pub mod wire;

#[cfg(target_os = "linux")]
pub mod audio_workers;
pub mod coordinator;

mod receipt_history;
mod service_trace;
mod session_lifetime;
mod session_state;

mod service_privacy;
