//! What reading progress costs on this machine (`docs/tasks.md`, "Progress
//! on the bus"): parsing a `harness ps` listing, finding its signs, keeping
//! track of sixteen tasks, and one whole `ps` read through the adapter with
//! this executable standing in for Harness. It never runs the installed
//! Harness: the listing is made up here, in the shape Harness 0.4.2 writes.
//!
//! `cargo run --release -p lamp-tasks --example progress_cost -- --samples 200`
#[path = "../tests/fixtures/harness_cli.rs"]
mod scripted;

use lamp_tasks::{
    engine::Watched,
    harness::{Cli, CliConfig, parse_ps},
    progress::{Tracker, signs_in},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::DirBuilderExt,
    time::{Duration, Instant},
};

const MACHINE: &str = "0123456789abcdef0123456789abcdef";

/// Three events of at most 140 characters, as Harness joins them.
const EVENTS: [&str; 6] = [
    "Read src/parser.rs and the failing test in tests/parse.rs to see where the tokenizer drops the last character of a quoted string",
    "Fixed the off-by-one in the tokenizer: the end index now includes the closing quote, and the escape handling keeps its position",
    "Ran cargo test: all 118 tests pass, including the three new cases for quoted strings with escapes at the very end of the input",
    "Opened a pull request with the fix and the new tests, and wrote a short note on why the old end index was wrong for escapes",
    "Looking at the CI log for the Linux build, which fails in a part of the code this change does not touch; checking whether it is flaky",
    "Updated the README section on quoting so that it matches what the parser now does, with one example of an escaped quote at the end",
];

fn id(index: usize) -> String {
    format!("{index:08x}-1111-4111-8111-111111111111")
}

fn listing(rows: usize) -> Value {
    let harnesses: Vec<Value> = (0..rows)
        .map(|index| {
            let about: Vec<&str> = (0..3).map(|k| EVENTS[(index + k) % EVENTS.len()]).collect();
            json!({"name":format!("lamp-{index:04}"),"id":id(index),"agent":"Claude Code","machine":"office",
                "state":"working","folder":format!("/home/person/harnesses/project-{index}"),"branch":null,
                "startedBy":"person","yours":false,"self":false,"lastActive":"2026-10-15T00:05:12.345Z",
                "asks":["Fix the quoted string bug in the parser"],"about":about.join(" / ")})
        })
        .collect();
    json!({"you":null,"machine":"office","harnesses":harnesses})
}

fn micros(time: Duration) -> f64 {
    (time.as_secs_f64() * 1e7).round() / 10.0
}

fn summary(mut values: Vec<f64>) -> Value {
    let first = values[0];
    values.sort_by(f64::total_cmp);
    json!({"first":first,"median":values[values.len() / 2],"max":values[values.len() - 1],"samples":values.len()})
}

fn measure(samples: usize) -> io::Result<Value> {
    let watched: Vec<Watched> = (0..16)
        .map(|index| Watched {
            task_id: index as u64 + 1,
            machine_id: MACHINE.into(),
            agent_id: id(index),
            engine: "claude".into(),
            accepted_ago_ms: Some(60_000),
        })
        .collect();
    let mut parsing = serde_json::Map::new();
    for rows in [1, 16, 40, 256] {
        // As the CLI prints it: one space of indent.
        let mut text = Vec::new();
        let mut writer = serde_json::Serializer::with_formatter(
            &mut text,
            serde_json::ser::PrettyFormatter::with_indent(b" "),
        );
        serde::Serialize::serialize(&listing(rows), &mut writer).map_err(io::Error::other)?;
        let (mut parse, mut signs, mut track) = (Vec::new(), Vec::new(), Vec::new());
        let mut tracker = Tracker::started(1);
        for sample in 0..samples {
            let began = Instant::now();
            let listed = parse_ps(&text).map_err(io::Error::other)?;
            parse.push(micros(began.elapsed()));
            let began = Instant::now();
            let found: usize = listed
                .iter()
                .filter_map(|row| row.about.as_deref())
                .map(|about| signs_in(about).len())
                .sum();
            signs.push(micros(began.elapsed()));
            std::hint::black_box(found);
            let began = Instant::now();
            tracker.read(MACHINE, Ok(listed), &watched, sample as u64, 1);
            std::hint::black_box(tracker.take(&"0".repeat(32)));
            track.push(micros(began.elapsed()));
        }
        parsing.insert(
            rows.to_string(),
            json!({"bytes":text.len(),"parse_us":summary(parse),"signs_us":summary(signs),
                "track_16_tasks_us":summary(track)}),
        );
    }
    // One whole read through the adapter: spawning this executable as the
    // scripted `harness`, capturing and parsing forty harnesses.
    let home = std::env::temp_dir().join(format!("progress-cost-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::DirBuilder::new().mode(0o700).create(&home)?;
    fs::write(
        home.join("plan.json"),
        serde_json::to_vec(&json!({"ps":{"stdout":listing(40).to_string()}}))?,
    )?;
    let mut config = CliConfig::new(std::env::current_exe()?);
    config.environment = BTreeMap::from([
        ("HOME".into(), home.clone().into_os_string()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("LANG".into(), "C".into()),
    ]);
    let cli = Cli::new(config, "progress-cost".into())?;
    let mut reads = Vec::new();
    for _ in 0..samples.min(50) {
        let began = Instant::now();
        let listed = cli.status(MACHINE).map_err(io::Error::other)?;
        reads.push((began.elapsed().as_secs_f64() * 1e4).round() / 10.0);
        std::hint::black_box(listed);
    }
    fs::remove_dir_all(&home)?;
    Ok(json!({
        "platform":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH},
        "parsing_by_rows":parsing,
        "scripted_ps_read_40_rows_ms":summary(reads),
        "boundaries":"parse: parse_ps on the listing's bytes; signs: signs_in on every about; track: Tracker::read for 16 watched tasks and take of the status; scripted read: Cli::status from spawn to parsed rows, this executable standing in for harness (no network, no daemon, no Harness work)",
    }))
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "ps") {
        return scripted::main();
    }
    let samples = match args.as_slice() {
        [] => 200,
        [flag, n] if flag == "--samples" => n
            .parse::<usize>()
            .ok()
            .filter(|n| (2..=10_000).contains(n))
            .ok_or_else(|| io::Error::other("--samples is 2..10000"))?,
        _ => return Err(io::Error::other("usage: progress_cost [--samples N]")),
    };
    println!("{}", serde_json::to_string_pretty(&measure(samples)?)?);
    Ok(())
}
