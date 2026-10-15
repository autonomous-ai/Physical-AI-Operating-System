include!("../application.rs");

#[path = "../fixture_protocol.rs"]
mod fixture_protocol;
#[path = "../fixture_schedule.rs"]
mod fixture_schedule;

fn main() -> ExitCode {
    entrypoint(|command| command_with_reporting(command, fixture_schedule::report))
}
