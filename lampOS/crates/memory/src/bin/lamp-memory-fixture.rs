//! Explicit child fixture; the ordinary Memory binary has no control endpoint.
#[path = "../fixture_protocol.rs"]
pub mod fixture_protocol;
#[path = "../fixture_schedule.rs"]
pub mod fixture_schedule;

include!("../application.rs");

fn main() -> ExitCode {
    let control = match fixture_schedule::ChildControl::bootstrap() {
        Ok(control) => control,
        Err(error) => {
            eprintln!("lamp-memory-fixture: {error}");
            return ExitCode::FAILURE;
        }
    };
    let schedule = control.schedule();
    entrypoint(move |root, source| Reporter::start_with_schedule(root, source, schedule))
}
