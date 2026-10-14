//! Optional durable work tracking through the provisioned Harness CLI.
//! Tasks never opens an output device or implements Harness authentication.
pub mod engine;
pub mod harness;
pub mod protocol;
pub mod runtime;
pub mod store;
