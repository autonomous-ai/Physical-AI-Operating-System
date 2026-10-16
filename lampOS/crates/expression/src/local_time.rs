//! The lamp's local time (#820), one reader shared with Memory and the policy:
//! a fixed offset, a named zone, or `--zone system`, which follows
//! `/etc/localtime` as setup changes it. See `lamp_local_time`.

pub use lamp_local_time::{LocalTime, SystemZone};
