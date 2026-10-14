//! Static balance arithmetic for five lamp joints. This library has no I/O,
//! actuator, thread, environment switch, or default measured geometry.
//!
//! Inputs are encoder-angle radians in increasing-count direction, ordered
//! by servo IDs 1..5. The geometry gives their physical zero frames and axes.
//! A result describes this model on a horizontal support plane, not dynamic
//! stability, collision clearance, friction, or the mechanism's actual motion.

mod data;
mod footprint;
mod model;
mod path;

pub use data::{Angles, Base, Geometry, Joint, Link, Measurement};
pub use model::{Model, PoseCheck};
pub use path::{Clip, MovementCheck, OffsetFrame, PoseAt, Step, Verdict};

/// A numerical reserve beyond the configured physical margin. It is not an
/// allowance for uncertain mass, dimensions, tracking or measurement error.
pub const NUMERICAL_RESERVE_M: f64 = 1e-9;
pub const JOINT_COUNT: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

fn invalid(message: impl Into<String>) -> Error {
    Error(message.into())
}
