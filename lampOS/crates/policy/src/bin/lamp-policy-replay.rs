//! The Policy's logged attention decisions asked again of other decision
//! models (#871): `lamp_policy::replay`. Run where the logs are; it prints
//! no words. Nothing on the bus, nothing acts.
use lamp_policy::{
    decide::{DEFAULT_ENDPOINT, Decider, Jev},
    replay::{self, Should},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    thread,
    time::Duration,
};

const USAGE: &str = "usage: lamp-policy-replay --key-file ABSOLUTE_PATH --log-dir ABSOLUTE_DIR --days YYYY-MM-DD[,YYYY-MM-DD...] --models NAME[,NAME...] [--labels ABSOLUTE_FILE] [--only-labelled] [--endpoint URL] [--deadline-ms 10000] [--pace-ms 500] [--most N]\n       lamp-policy-replay --again ABSOLUTE_FILE --labels ABSOLUTE_FILE\n       lamp-policy-replay --pictures --log-dir ABSOLUTE_DIR --days YYYY-MM-DD[,...] [--labels ABSOLUTE_FILE]";
const HELP: &str = "Asks each attention decision in the decisions logs of the days given again, of each\n\
model given in turn (at most four), as it was asked on the day, and prints one line per\n\
decision and a summary: each model's choice, probability and time, and the kinds of what\n\
was heard (words, the lamp named, another language), never the words. --labels gives what\n\
should have happened, one JSON line each: {\"at_unix_ms\": N, \"should\": \"answer\"|\"stay_quiet\"}.\n\
--again counts an earlier run's printed lines again with --labels, and asks nothing.\n\
--pictures counts the lines lamp-policy --images wrote (pictures-YYYY-MM-DD.jsonl), a label\n\
taken for the line nearest it within 3 s, and asks nothing.\n\
What was heard is sent to the decision model's service again, as on the day.";

#[derive(Debug, PartialEq)]
enum Run {
    Ask(Arguments),
    /// An earlier run's lines, and the labels to count them with.
    Again {
        lines: PathBuf,
        labels: PathBuf,
    },
    /// The picture lines of these days, counted (#871).
    Pictures {
        log: PathBuf,
        days: Vec<String>,
        labels: Option<PathBuf>,
    },
}

#[derive(Debug, PartialEq)]
struct Arguments {
    key: PathBuf,
    log: PathBuf,
    days: Vec<String>,
    models: Vec<String>,
    labels: Option<PathBuf>,
    only_labelled: bool,
    endpoint: String,
    deadline: Duration,
    pace: Duration,
    most: Option<usize>,
}

fn absolute(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("file and directory paths must be absolute".into())
    }
}

fn number(value: &str, least: u64, most: u64) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| (least..=most).contains(n))
        .ok_or_else(|| format!("expected a whole number from {least} to {most}"))
}

fn day(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

fn parse(args: impl Iterator<Item = String>) -> Result<Option<Run>, String> {
    let mut args = args;
    let (mut key, mut log, mut days, mut models, mut labels) = (None, None, None, None, None);
    let mut again = None;
    let mut pictures = false;
    let mut only_labelled = false;
    let mut endpoint = DEFAULT_ENDPOINT.to_owned();
    let (mut deadline, mut pace, mut most) = (10_000, 500, None);
    let mut seen = BTreeSet::new();
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            return Ok(None);
        }
        if !seen.insert(option.clone()) {
            return Err(format!("{option} is given twice"));
        }
        if option == "--only-labelled" {
            only_labelled = true;
            continue;
        }
        if option == "--pictures" {
            pictures = true;
            continue;
        }
        let value = args.next().ok_or(format!("{option} needs a value"))?;
        match option.as_str() {
            "--key-file" => key = Some(absolute(&value)?),
            "--log-dir" => log = Some(absolute(&value)?),
            "--labels" => labels = Some(absolute(&value)?),
            "--again" => again = Some(absolute(&value)?),
            "--days" => {
                let listed: Vec<String> = value.split(',').map(str::to_owned).collect();
                if listed.is_empty() || !listed.iter().all(|listed| day(listed)) {
                    return Err("--days is YYYY-MM-DD, separated by commas".into());
                }
                days = Some(listed);
            }
            "--models" => {
                let listed: Vec<String> = value.split(',').map(str::to_owned).collect();
                if !(1..=4).contains(&listed.len())
                    || listed.iter().any(|model| {
                        model.is_empty() || model.chars().any(|c| c.is_control() || c == ' ')
                    })
                {
                    return Err("--models is one to four model names, separated by commas".into());
                }
                models = Some(listed);
            }
            "--endpoint" => endpoint = value,
            "--deadline-ms" => deadline = number(&value, 1, 60_000)?,
            "--pace-ms" => pace = number(&value, 0, 10_000)?,
            "--most" => most = Some(number(&value, 1, 100_000)? as usize),
            _ => return Err(format!("{option} is not an option")),
        }
    }
    if pictures {
        if key.is_some() || models.is_some() || again.is_some() || only_labelled {
            return Err("--pictures takes --log-dir, --days and --labels only".into());
        }
        return Ok(Some(Run::Pictures {
            log: log.ok_or("--log-dir is required")?,
            days: days.ok_or("--days is required")?,
            labels,
        }));
    }
    if let Some(lines) = again {
        if seen.len() != 2 {
            return Err("--again takes --labels and nothing else".into());
        }
        let labels = labels.ok_or("--again needs --labels")?;
        return Ok(Some(Run::Again { lines, labels }));
    }
    if only_labelled && labels.is_none() {
        return Err("--only-labelled needs --labels".into());
    }
    Ok(Some(Run::Ask(Arguments {
        key: key.ok_or("--key-file is required")?,
        log: log.ok_or("--log-dir is required")?,
        days: days.ok_or("--days is required")?,
        models: models.ok_or("--models is required")?,
        labels,
        only_labelled,
        endpoint,
        deadline: Duration::from_millis(deadline),
        pace: Duration::from_millis(pace),
        most,
    })))
}

fn pictures(log: &Path, days: &[String], labels: Option<&PathBuf>) -> io::Result<()> {
    let labels = match labels {
        Some(path) => replay::labels(&std::fs::read_to_string(path)?).map_err(io::Error::other)?,
        None => BTreeMap::new(),
    };
    let lines = replay::read_pictures(log, days)?;
    println!("{}", replay::pictures_summary(&lines, &labels));
    Ok(())
}

fn again(lines: &PathBuf, labels: &PathBuf) -> io::Result<()> {
    let labels = replay::labels(&std::fs::read_to_string(labels)?).map_err(io::Error::other)?;
    let (lines, models) =
        replay::relabelled(&std::fs::read_to_string(lines)?, &labels).map_err(io::Error::other)?;
    println!("{}", replay::summary(&lines, &models));
    Ok(())
}

fn run(arguments: &Arguments) -> io::Result<()> {
    let labels: BTreeMap<u64, Should> = match &arguments.labels {
        Some(path) => replay::labels(&std::fs::read_to_string(path)?).map_err(io::Error::other)?,
        None => BTreeMap::new(),
    };
    let mut judges: Vec<Jev> = arguments
        .models
        .iter()
        .map(|model| Jev::new(&arguments.endpoint, model, &arguments.key))
        .collect::<io::Result<_>>()?;
    let (logged, unread) = replay::read(&arguments.log, &arguments.days)?;
    let chosen: Vec<_> = logged
        .into_iter()
        .filter(|logged| !arguments.only_labelled || labels.contains_key(&logged.at_unix_ms))
        .take(arguments.most.unwrap_or(usize::MAX))
        .collect();
    eprintln!(
        "lamp-policy-replay: {} decisions to ask of {} ({} lines not read)",
        chosen.len(),
        arguments.models.join(", "),
        unread
    );
    let mut lines = Vec::new();
    let mut stdout = io::stdout().lock();
    let mut first = true;
    for (index, logged) in chosen.iter().enumerate() {
        let mut replayed = Vec::new();
        // The same question to each model, one after the other.
        for (model, judge) in arguments.models.iter().zip(judges.iter_mut()) {
            if !first {
                thread::sleep(arguments.pace);
            }
            first = false;
            let judge: &mut dyn Decider = judge;
            replayed.push((
                model.clone(),
                replay::ask(judge, logged, arguments.deadline),
            ));
        }
        let line = replay::line(logged, labels.get(&logged.at_unix_ms).copied(), &replayed);
        writeln!(stdout, "{line}")?;
        lines.push(line);
        if (index + 1) % 20 == 0 {
            eprintln!("lamp-policy-replay: {} of {}", index + 1, chosen.len());
        }
    }
    writeln!(stdout, "{}", replay::summary(&lines, &arguments.models))?;
    stdout.flush()
}

fn main() -> ExitCode {
    match parse(std::env::args().skip(1)) {
        Ok(None) => {
            println!("{USAGE}\n{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Some(chosen)) => match match &chosen {
            Run::Ask(arguments) => run(arguments),
            Run::Again { lines, labels } => again(lines, labels),
            Run::Pictures { log, days, labels } => pictures(log, days, labels.as_ref()),
        } {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("lamp-policy-replay: {error}");
                ExitCode::FAILURE
            }
        },
        Err(why) => {
            eprintln!("lamp-policy-replay: {why}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(line: &str) -> Result<Option<Arguments>, String> {
        match parse(line.split_whitespace().map(str::to_owned))? {
            Some(Run::Ask(arguments)) => Ok(Some(arguments)),
            Some(Run::Again { .. } | Run::Pictures { .. }) => Err("another run".into()),
            None => Ok(None),
        }
    }

    #[test]
    fn the_arguments_name_the_logs_the_days_and_the_models() {
        let arguments = of("--key-file /k --log-dir /var/lib/lampos-policy --days 2026-10-14,2026-10-15 --models typesafe/jev-1.13,typesafe/jev-1.14")
            .unwrap()
            .unwrap();
        assert_eq!(arguments.days, ["2026-10-14", "2026-10-15"]);
        assert_eq!(arguments.models, ["typesafe/jev-1.13", "typesafe/jev-1.14"]);
        assert_eq!(arguments.endpoint, DEFAULT_ENDPOINT);
        assert_eq!(arguments.deadline, Duration::from_secs(10));
        assert_eq!(arguments.pace, Duration::from_millis(500));
        assert_eq!((arguments.most, arguments.only_labelled), (None, false));
        let labelled = of("--key-file /k --log-dir /d --days 2026-10-15 --models m --labels /l --only-labelled --pace-ms 0 --most 40")
            .unwrap()
            .unwrap();
        assert!(labelled.only_labelled);
        assert_eq!(labelled.most, Some(40));
        assert_eq!(labelled.pace, Duration::ZERO);
        assert_eq!(of("--help").unwrap(), None);
        assert_eq!(
            parse(
                "--again /r --labels /l"
                    .split_whitespace()
                    .map(str::to_owned)
            )
            .unwrap(),
            Some(Run::Again {
                lines: "/r".into(),
                labels: "/l".into()
            })
        );
        assert_eq!(
            parse(
                "--pictures --log-dir /d --days 2026-10-15"
                    .split_whitespace()
                    .map(str::to_owned)
            )
            .unwrap(),
            Some(Run::Pictures {
                log: "/d".into(),
                days: vec!["2026-10-15".into()],
                labels: None
            })
        );
        for refused in [
            "--again /r",
            "--again /r --labels /l --days 2026-10-15",
            "--pictures --log-dir /d --days 2026-10-15 --models m",
            "--pictures --days 2026-10-15",
        ] {
            assert!(
                parse(refused.split_whitespace().map(str::to_owned)).is_err(),
                "{refused}"
            );
        }
        for refused in [
            "--log-dir /d --days 2026-10-15 --models m",
            "--key-file k --log-dir /d --days 2026-10-15 --models m",
            "--key-file /k --log-dir /d --days 15-10-2026 --models m",
            "--key-file /k --log-dir /d --days 2026-10-15 --models a,b,c,d,e",
            "--key-file /k --log-dir /d --days 2026-10-15 --models a,,b",
            "--key-file /k --log-dir /d --days 2026-10-15 --models m --only-labelled",
            "--key-file /k --log-dir /d --days 2026-10-15 --models m --deadline-ms 0",
            "--key-file /k --log-dir /d --days 2026-10-15 --models m --most 1 --most 2",
            "--key-file /k --log-dir /d --days 2026-10-15 --models m --other x",
        ] {
            assert!(of(refused).is_err(), "{refused}");
        }
    }
}
