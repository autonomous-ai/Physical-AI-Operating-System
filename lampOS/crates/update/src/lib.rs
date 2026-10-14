//! Updates: a lamp keeps its lampOS current by itself (docs/updates.md).
//!
//! [`manifest`] is what a build is and its signature; [`publish`] makes
//! one where it is built; [`fetch`] reads a channel; [`unit`] reads and
//! changes a part's unit; [`quiet`] says when the voice may be switched;
//! [`judge`] whether a part is well after it; [`records`] is what is kept;
//! [`world`] is systemd, the bus, the disk and the clock behind traits; and
//! [`round`] is one round of an update, with all of them.
pub mod fetch;
pub mod judge;
pub mod manifest;
pub mod publish;
pub mod quiet;
pub mod records;
pub mod round;
pub mod unit;
pub mod world;

pub use round::{Config, Outcome, World, round};
