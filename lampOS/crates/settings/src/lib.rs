//! Declared, bounded desired settings with durable compare-and-set updates.
//! A saved value is not evidence that an actuator or another part applied it.
pub mod client;
pub mod engine;
pub mod model;
pub mod protocol;
pub mod runtime;
pub mod store;
