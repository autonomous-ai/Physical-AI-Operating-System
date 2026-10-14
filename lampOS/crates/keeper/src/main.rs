//! `lamp-keeper`: watches the lamp's health and, with `--act`, restarts a
//! part that stays unwell; and one place to ask what runs and to start,
//! stop or restart a part or the lamp.
use lamp_bus::{Bus, Publisher};
use lamp_keeper::{
    Keeper, NAME, Notify, PartState, Rules, STATE,
    units::{self, Systemctl, Systemd},
};
use lamp_msgs::{Diagnostic, Health, Level, Voice, VoiceState};
use std::{
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

const USAGE: &str = "\
lamp-keeper [serve] [--act] [--unwell-seconds N] [--most N]
lamp-keeper status
lamp-keeper start|stop|restart PART|lamp

serve watches lamp/health on the bus (LAMP_BUS_ROOT, or /run/lampos-bus).
Without --act it only says, in its journal, which part it would restart.
With --act it restarts a part in error or stale for --unwell-seconds
(default 30), at most --most times in ten minutes (default 3), and then
leaves it alone and reports it. The voice is restarted only when it says
it is resting: never in a turn.
status prints every lampOS unit: its state, its build and its health.
start, stop and restart act on lamp-PART.service, or lampos.target for lamp.
";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["--help"] => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        ["status"] => status(),
        [verb @ ("start" | "stop" | "restart"), part] if units::is_part_name(part) => {
            match Systemctl.command(verb, &units::unit_of(part)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(why) => {
                    eprintln!("lamp-keeper: {verb} {part}: {why}");
                    ExitCode::FAILURE
                }
            }
        }
        ["serve", rest @ ..] => serve_with(rest),
        rest => serve_with(rest),
    }
}

fn usage() -> ExitCode {
    eprint!("{USAGE}");
    ExitCode::from(2)
}

fn serve_with(options: &[&str]) -> ExitCode {
    let mut rules = Rules::default();
    let mut options = options.iter();
    while let Some(option) = options.next() {
        let mut number = || options.next().and_then(|value| value.parse::<u64>().ok());
        match *option {
            "--act" => rules.act = true,
            "--unwell-seconds" => match number().filter(|n| (1..=3600).contains(n)) {
                Some(seconds) => rules.unwell_for = Duration::from_secs(seconds),
                None => return usage(),
            },
            "--most" => match number().filter(|n| (1..=100).contains(n)) {
                Some(most) => rules.most = most as usize,
                None => return usage(),
            },
            _ => return usage(),
        }
    }
    match Bus::from_environment() {
        Ok(bus) => serve(&bus, rules),
        Err(error) => {
            eprintln!("lamp-keeper: the bus cannot be used: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The lamp's health, if someone keeps it fresh.
fn health(bus: &Bus) -> Option<Health> {
    let message = bus.latest(Health::TOPIC).ok().flatten()?;
    (!message.expired).then(|| message.parse().ok()).flatten()
}

/// True only when the voice says it is resting: a voice that says nothing,
/// or anything else, may be in someone's turn.
fn voice_resting(bus: &Bus) -> bool {
    bus.latest(VoiceState::TOPIC)
        .ok()
        .flatten()
        .and_then(|message| message.parse::<VoiceState>().ok())
        .is_some_and(|said| said.state == Voice::Resting)
}

fn serve(bus: &Bus, rules: Rules) -> ExitCode {
    // ponytail: no signal handling (the standard library has none, as for
    // lamp-health): SIGTERM ends it where it stands, between two looks.
    let notify = Notify::from_environment();
    let mut keeper = Keeper::new(rules);
    let mut out: Option<Publisher> = None;
    let started = Instant::now();
    eprintln!(
        "lamp-keeper: watching lamp/health; {}",
        if rules.act { "acting" } else { "watching only" }
    );
    notify.ready();
    loop {
        let now = Instant::now();
        let health = health(bus);
        if let Some(health) = &health {
            for done in keeper.look(health, voice_resting(bus), now, &mut Systemctl) {
                eprintln!("lamp-keeper: {done}");
            }
        }
        // Its own report, as every part's.
        let given_up = keeper.given_up(now);
        let (level, message) = if !given_up.is_empty() {
            (
                Level::Warn,
                format!("will not stay up: {}", given_up.join(", ")),
            )
        } else if health.is_none() {
            (Level::Warn, "lamp/health is not heard".to_owned())
        } else {
            (Level::Ok, String::new())
        };
        let report = Diagnostic::new(level, NAME, &message)
            .with(STATE, PartState::Ready.as_str())
            .with("act", rules.act)
            .with("restarts_last_ten_minutes", keeper.restarts(now));
        if out.is_none() {
            out = bus
                .publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), NAME)
                .ok();
        }
        let at_us = started.elapsed().as_micros() as u64;
        if out
            .as_mut()
            .is_some_and(|out| out.publish_json(at_us, &report).is_err())
        {
            // Made again next time round.
            out = None;
        }
        notify.alive();
        thread::sleep(Duration::from_secs(1));
    }
}

fn status() -> ExitCode {
    let text = match Systemctl.show(&units::PATTERNS) {
        Ok(text) => text,
        Err(why) => {
            eprintln!("lamp-keeper: systemctl show: {why}");
            return ExitCode::FAILURE;
        }
    };
    let health = Bus::from_environment().ok().and_then(|bus| health(&bus));
    print!(
        "{}",
        units::status_lines(&units::parse_show(&text), health.as_ref())
    );
    ExitCode::SUCCESS
}
