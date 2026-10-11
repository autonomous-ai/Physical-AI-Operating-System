//! `lamp-endpoint-eval`: offline end-of-turn evaluation on fixtures.
//!
//! - `probe`: model scores at every pause of labelled human clips.
//! - `replay`: the production detector over the same clips for each profile.
//! - `cases`: the synthetic `say` stress cohort, kept apart from human clips.
//! - `profile`: startup, stage latency and resource use of the real worker.
//!
//! Results are classification and replay evidence on a development host. They
//! are not room recordings, device timing or direct-human acceptance.
use std::{error::Error, path::PathBuf};

mod audio;
mod cases;
mod corpus;
mod probe;
mod profile;
mod replay;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

struct Arguments {
    named: Vec<(String, String)>,
    flags: Vec<String>,
    positional: Vec<String>,
}

impl Arguments {
    fn parse(arguments: impl Iterator<Item = String>) -> Self {
        let mut parsed = Self {
            named: Vec::new(),
            flags: Vec::new(),
            positional: Vec::new(),
        };
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            if let Some(name) = argument.strip_prefix("--") {
                match arguments.peek() {
                    Some(value) if !value.starts_with("--") => {
                        parsed.named.push((name.into(), arguments.next().unwrap()));
                    }
                    _ => parsed.flags.push(name.into()),
                }
            } else {
                parsed.positional.push(argument);
            }
        }
        parsed
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.named
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.get(name)
            .ok_or_else(|| format!("missing --{name}").into())
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|flag| flag == name)
    }
}

fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_default();
    let arguments = Arguments::parse(arguments);
    let threads = arguments
        .get("threads")
        .map_or(Ok(std::thread::available_parallelism()?.get()), str::parse)?;
    match command.as_str() {
        "probe" => {
            let directories: Vec<PathBuf> =
                arguments.positional.iter().map(PathBuf::from).collect();
            let clips = corpus::load(&directories, arguments.flag("synthetic"))?;
            let gain_db = arguments.get("gain-db").map_or(Ok(0.0), str::parse)?;
            eprintln!("probe: {} clips, {threads} threads", clips.len());
            probe::run(
                &clips,
                arguments.required("model")?.as_ref(),
                arguments.required("out")?.as_ref(),
                gain_db,
                threads,
            )
        }
        "vad" => {
            let directories: Vec<PathBuf> =
                arguments.positional.iter().map(PathBuf::from).collect();
            let clips = corpus::load(&directories, arguments.flag("synthetic"))?;
            let gain_db = arguments.get("gain-db").map_or(Ok(0.0), str::parse)?;
            probe::vad(&clips, arguments.required("out")?.as_ref(), gain_db)
        }
        "replay" => {
            let directories: Vec<PathBuf> =
                arguments.positional.iter().map(PathBuf::from).collect();
            let every = arguments.get("every").map_or(Ok(1), str::parse::<usize>)?;
            let clips: Vec<_> = corpus::load(&directories, arguments.flag("synthetic"))?
                .into_iter()
                .step_by(every.max(1))
                .collect();
            let mode = match arguments.get("mode").unwrap_or("virtual") {
                "virtual" => replay::Mode::Virtual {
                    latency_blocks: arguments
                        .get("latency-ms")
                        .map_or(Ok(130), str::parse::<u16>)?
                        .div_ceil(10)
                        .max(1),
                },
                "paced" => replay::Mode::Paced {
                    worker: arguments.required("worker")?.into(),
                },
                other => return Err(format!("unknown mode {other}").into()),
            };
            let profiles: Vec<String> = arguments
                .get("profiles")
                .map(|list| list.split(',').map(str::to_owned).collect())
                .unwrap_or_default();
            let gain_db = arguments.get("gain-db").map_or(Ok(0.0), str::parse)?;
            eprintln!("replay: {} clips, {threads} threads", clips.len());
            replay::run_human(
                &clips,
                arguments.required("model")?.as_ref(),
                &mode,
                &profiles,
                gain_db,
                threads,
                arguments.required("out")?.as_ref(),
            )
        }
        "profile" => profile::run(
            arguments.required("worker")?.as_ref(),
            arguments.required("model")?.as_ref(),
            arguments.required("speech")?.as_ref(),
            arguments.get("requests").map_or(Ok(100), str::parse)?,
            arguments.get("starts").map_or(Ok(5), str::parse)?,
            arguments.required("out")?.as_ref(),
        ),
        "cases" => cases::run(
            arguments.required("cases")?.as_ref(),
            arguments.required("cache")?.as_ref(),
            arguments.required("model")?.as_ref(),
            arguments
                .get("latency-ms")
                .map_or(Ok(130), str::parse::<u16>)?
                .div_ceil(10)
                .max(1),
            arguments.required("out")?.as_ref(),
        ),
        _ => Err(
            "usage: lamp-endpoint-eval probe|replay|cases|profile --model M --out O [options] DIR..."
                .into(),
        ),
    }
}
