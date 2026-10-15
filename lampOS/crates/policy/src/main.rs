//! The policy as a process:
//! `lamp-policy --key-file ABSOLUTE_PATH --log-dir ABSOLUTE_DIR ...`.
//!
//! It runs until it is killed. The standard library cannot catch a signal
//! and this crate takes in nothing that can, so SIGTERM ends it where it
//! stands. A question that was out is then not decided. A line of the log
//! is written with one call, so it is there whole or not at all. Its
//! sockets on the bus are left for the voice's next message to remove, and
//! it stops being listed as a publisher when the bus's lease runs out.
//!
//! Everything this prints is about itself: where it listens, how many,
//! how long, which choice, which kind of failure. What was heard and what
//! the lamp said are never printed (`lamp_policy::policy::Note`).

use lamp_bus::Bus;
use lamp_policy::{
    decide::{DEFAULT_DEADLINE, DEFAULT_ENDPOINT, DEFAULT_MODEL, Endpoint, Jev},
    gather::PAUSE,
    hello::{self, Hello},
    pictures::{self, Pictures},
    policy::{self, Options},
    progress::{self, Progress},
    record::private_dir,
};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    sync::atomic::AtomicBool,
    time::Duration,
};

const USAGE: &str = "usage: lamp-policy --key-file ABSOLUTE_PATH --log-dir ABSOLUTE_DIR [--endpoint URL] [--model NAME] [--deadline-ms N] [--bus-root ABSOLUTE_DIR] [--progress] [--hello] [--utc-offset-minutes N] [--act progress] [--act hello] [--images MODEL [--sight-file ABSOLUTE_PATH]]";
const HELP: &str = "The lamp's policy, in shadow mode: judges whether each thing the voice hears was said to\n\
the lamp, publishes the decision on the bus (policy/decisions) and writes it down. Nothing\n\
acts on it. See docs/policy.md.\n\
  --key-file     a file whose one line is the key for the decision model's service; never printed\n\
  --log-dir      where the decisions log is kept, one file a day. It holds what was heard, in\n\
                 words, so it is a directory only its owner can enter: made with mode 0700 if it\n\
                 is not there, refused if its group or everyone can enter it\n\
  --endpoint     where the decision model is asked (default https://openrouter.ai/api/alpha/decisions);\n\
                 plain http only to this machine, for a test\n\
  --model        the model asked for (default typesafe/jev-1.13)\n\
  --deadline-ms  how long one asking may take, from 1 to 60000 (default 3000); after it a rule decides\n\
  --bus-root     the bus's directory (default LAMP_BUS_ROOT, or /run/lampos-bus)\n\
  --progress     also judge, in shadow, whether progress of the work handed over (tasks/status)\n\
                 is worth telling now, with a second connection to the same model. Off unless set\n\
  --utc-offset-minutes  the lamp's local time east of UTC, for the hour of the day the progress\n\
                 question is shown (-720 to 840); without it the hour is not known\n\
  --act progress the progress decisions act: published with shadow false, so that the voice\n\
                 tells a say_now. Needs --progress. \"Is this said to the lamp?\" stays shadow\n\
  --hello        also judge whether a hello the voice holds (voice/arrival) is said now, with\n\
                 a connection of its own to the same model. Off unless set\n\
  --act hello    the hello decisions act: say_now says the hello, wait keeps it. Needs --hello\n\
  --images       also ask MODEL (typesafe/jev-1.14 takes pictures) each attention question twice at once,\n\
                 without the eyes' newest frame and with it and six questions about it, and write both\n\
                 to pictures-YYYY-MM-DD.jsonl. Shadow; the decision never waits for it. A frame is sent\n\
                 only while the eyes keep one (the camera is permitted while the microphone is) and it\n\
                 is at most 1 s old; it is never written. Off unless set\n\
  --sight-file   the eyes' frame (default /run/lampos-eyes/sight.jpg). Needs --images\n\
What is heard is sent to the decision model's service. Do not run this on real speech in the\n\
office until the owner has said so: it is for played tests.";

#[derive(Debug, Eq, PartialEq)]
struct Arguments {
    key_file: PathBuf,
    log_dir: PathBuf,
    endpoint: String,
    model: String,
    deadline: Duration,
    bus: Option<PathBuf>,
    /// The progress question beside the attention one: off unless set.
    progress: bool,
    utc_offset_minutes: Option<i32>,
    /// Its decisions act (`--act progress`): off unless set.
    act_progress: bool,
    /// The hello question (`--hello`), and whether it acts (`--act hello`):
    /// off unless set.
    hello: bool,
    act_hello: bool,
    /// The images model asked with the eyes' frame (`--images`, #871): off
    /// unless set.
    images: Option<String>,
    sight_file: Option<PathBuf>,
}

fn parse(arguments: impl Iterator<Item = String>) -> Result<Option<Arguments>, String> {
    let (mut key_file, mut log_dir, mut endpoint, mut model, mut deadline, mut bus) =
        (None, None, None, None, None, None);
    let (mut progress, mut utc_offset_minutes) = (false, None);
    let (mut hello, mut act_progress, mut act_hello) = (false, false, false);
    let (mut images, mut sight_file) = (None, None);
    let mut arguments = arguments;
    while let Some(option) = arguments.next() {
        if option == "--help" || option == "-h" {
            return Ok(None);
        }
        // The options without a value.
        if option == "--progress" || option == "--hello" {
            let flag = if option == "--progress" {
                &mut progress
            } else {
                &mut hello
            };
            if std::mem::replace(flag, true) {
                return Err(format!("{option} was given twice"));
            }
            continue;
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("{option} needs a value"))?;
        let absolute = |value: &str| -> Result<PathBuf, String> {
            let path = PathBuf::from(value);
            path.is_absolute()
                .then_some(path)
                .ok_or_else(|| format!("{option} must be an absolute path"))
        };
        let twice = match option.as_str() {
            "--key-file" => key_file.replace(absolute(&value)?).is_some(),
            "--log-dir" => log_dir.replace(absolute(&value)?).is_some(),
            "--bus-root" => bus.replace(absolute(&value)?).is_some(),
            "--sight-file" => sight_file.replace(absolute(&value)?).is_some(),
            "--images" => {
                if value.is_empty() || value.chars().any(char::is_control) || value.starts_with('-')
                {
                    return Err("--images is the model asked with the picture".into());
                }
                images.replace(value).is_some()
            }
            "--endpoint" => {
                Endpoint::parse(&value)?;
                endpoint.replace(value).is_some()
            }
            "--model" => {
                if value.is_empty() || value.chars().any(char::is_control) {
                    return Err("--model is a model's name".into());
                }
                model.replace(value).is_some()
            }
            // The questions that may act: each named once.
            "--act" => match value.as_str() {
                "progress" => std::mem::replace(&mut act_progress, true),
                "hello" => std::mem::replace(&mut act_hello, true),
                _ => {
                    return Err("--act takes progress or hello, the questions that may act".into());
                }
            },
            "--utc-offset-minutes" => {
                let minutes = value
                    .parse::<i32>()
                    .ok()
                    .filter(|minutes| (-720..=840).contains(minutes))
                    .ok_or("--utc-offset-minutes is a whole number from -720 to 840")?;
                utc_offset_minutes.replace(minutes).is_some()
            }
            "--deadline-ms" => {
                let milliseconds = value
                    .parse::<u64>()
                    .ok()
                    .filter(|milliseconds| (1..=60_000).contains(milliseconds))
                    .ok_or("--deadline-ms is a whole number from 1 to 60000")?;
                deadline
                    .replace(Duration::from_millis(milliseconds))
                    .is_some()
            }
            other => return Err(format!("unknown option {other}")),
        };
        if twice {
            return Err(format!("{option} was given twice"));
        }
    }
    if act_progress && !progress {
        return Err("--act progress needs --progress".into());
    }
    if sight_file.is_some() && images.is_none() {
        return Err("--sight-file needs --images".into());
    }
    if act_hello && !hello {
        return Err("--act hello needs --hello".into());
    }
    Ok(Some(Arguments {
        key_file: key_file.ok_or("--key-file is required")?,
        log_dir: log_dir.ok_or("--log-dir is required")?,
        endpoint: endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned()),
        model: model.unwrap_or_else(|| DEFAULT_MODEL.to_owned()),
        deadline: deadline.unwrap_or(DEFAULT_DEADLINE),
        bus,
        progress,
        utc_offset_minutes,
        act_progress,
        hello,
        act_hello,
        images,
        sight_file,
    }))
}

/// One line to the journal. A journal that has gone away is no reason for
/// the policy to stop.
fn log(line: &str) {
    let _ = writeln!(io::stderr(), "lamp-policy: {line}");
}

/// What the start line says of the questions beside attention: as it was
/// with `--progress` alone, and with `--hello` after it.
fn questions(arguments: &Arguments) -> String {
    let mut said = String::new();
    if arguments.progress {
        said.push_str(" progress=true");
    }
    if arguments.hello {
        said.push_str(" hello=true");
    }
    let acting: Vec<&str> = [
        (arguments.act_progress, "progress"),
        (arguments.act_hello, "hello"),
    ]
    .into_iter()
    .filter_map(|(acts, name)| acts.then_some(name))
    .collect();
    if !acting.is_empty() {
        said.push_str(&format!(" act={}", acting.join(",")));
    }
    said
}

fn run(arguments: &Arguments) -> io::Result<()> {
    // The key first: without it no model can be asked, and that is to be
    // found out before anything is subscribed to.
    let jev = Jev::new(&arguments.endpoint, &arguments.model, &arguments.key_file)?;
    let bus = match &arguments.bus {
        Some(root) => Bus::new(root)?,
        None => Bus::from_environment()?,
    };
    log(&format!(
        "started endpoint={} model={} deadline_ms={} pause_ms={} log_dir={} bus_root={} shadow={}{}{}",
        arguments.endpoint,
        arguments.model,
        arguments.deadline.as_millis(),
        PAUSE.as_millis(),
        arguments.log_dir.display(),
        bus.root().display(),
        policy::SHADOW,
        questions(arguments),
        arguments
            .images
            .as_ref()
            .map_or_else(String::new, |model| format!(" images={model}")),
    ));
    let options = Options {
        log_dir: arguments.log_dir.clone(),
        deadline: arguments.deadline,
        pause: PAUSE,
    };
    // Nothing sets the flag: the process runs until it is killed.
    let stop = AtomicBool::new(false);
    // The picture's two askings, each on its own connection to the images
    // model, on a thread of their own (#871).
    let pictures = match &arguments.images {
        Some(model) => Some(Pictures::start(
            Jev::new(&arguments.endpoint, model, &arguments.key_file)?,
            Jev::new(&arguments.endpoint, model, &arguments.key_file)?,
            pictures::Options {
                sight_file: arguments
                    .sight_file
                    .clone()
                    .unwrap_or_else(|| pictures::SIGHT_FILE.into()),
                deadline: arguments.deadline.max(Duration::from_secs(10)),
                log_dir: arguments.log_dir.clone(),
                presence: pictures::Presence::default(),
            },
        )?),
        None => None,
    };
    // Each question a judge of its own, on its own connection: no question
    // waits for another.
    let judge = || Jev::new(&arguments.endpoint, &arguments.model, &arguments.key_file);
    let progress = if arguments.progress {
        let options = progress::Options {
            act: arguments.act_progress,
            ..progress::Options::new(arguments.deadline, arguments.utc_offset_minutes)
        };
        Some(Progress::start(judge()?, options)?)
    } else {
        None
    };
    let hello = if arguments.hello {
        let options = hello::Options {
            act: arguments.act_hello,
            ..hello::Options::new(arguments.deadline, arguments.utc_offset_minutes)
        };
        Some(Hello::start(judge()?, options)?)
    } else {
        None
    };
    policy::run_beside(
        &bus,
        jev,
        &options,
        &stop,
        &mut |note| log(&note.to_string()),
        policy::Beside {
            progress,
            hello,
            pictures,
        },
    )
    .map(|_| ())
}

fn main() -> ExitCode {
    // Its log lines never block it or end it, and its last ones reach the
    // journal (`lamp_ipc::log_relay`).
    let _log = lamp_ipc::log_relay::guard();
    let arguments = match parse(std::env::args().skip(1)) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(why) => {
            eprintln!("lamp-policy: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    // Before anything else is made: a directory others can enter is the
    // caller's mistake, like a wrong option.
    if let Err(error) = private_dir(&arguments.log_dir) {
        eprintln!("lamp-policy: {error}");
        return ExitCode::from(2);
    }
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lamp-policy: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(text: &str) -> Result<Option<Arguments>, String> {
        parse(text.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn the_options_are_read_and_the_rest_have_their_defaults() {
        let all = of("--key-file /etc/lampos/decisions.key --log-dir /var/lib/lampos/decisions --endpoint http://127.0.0.1:8080/decisions --model typesafe/jev-2 --deadline-ms 1500 --bus-root /run/lampos-bus")
            .unwrap()
            .unwrap();
        assert_eq!(
            all,
            Arguments {
                key_file: "/etc/lampos/decisions.key".into(),
                log_dir: "/var/lib/lampos/decisions".into(),
                endpoint: "http://127.0.0.1:8080/decisions".into(),
                model: "typesafe/jev-2".into(),
                deadline: Duration::from_millis(1_500),
                bus: Some("/run/lampos-bus".into()),
                progress: false,
                utc_offset_minutes: None,
                act_progress: false,
                hello: false,
                act_hello: false,
                images: None,
                sight_file: None,
            }
        );
        let least = of("--log-dir /d --key-file /k").unwrap().unwrap();
        assert_eq!(
            least,
            Arguments {
                key_file: "/k".into(),
                log_dir: "/d".into(),
                endpoint: "https://openrouter.ai/api/alpha/decisions".into(),
                model: "typesafe/jev-1.13".into(),
                deadline: Duration::from_millis(3_000),
                bus: None,
                progress: false,
                utc_offset_minutes: None,
                act_progress: false,
                hello: false,
                act_hello: false,
                images: None,
                sight_file: None,
            }
        );
        // The progress question, and the lamp's local time for it.
        let progress = of("--log-dir /d --progress --key-file /k --utc-offset-minutes 420")
            .unwrap()
            .unwrap();
        assert!(progress.progress && !progress.act_progress);
        assert_eq!(progress.utc_offset_minutes, Some(420));
        // The picture's askings (#871), with the eyes' frame where it is
        // unless another is named.
        let pictured = of("--log-dir /d --key-file /k --images typesafe/jev-1.14")
            .unwrap()
            .unwrap();
        assert_eq!(
            (pictured.images.as_deref(), pictured.sight_file.as_deref()),
            (Some("typesafe/jev-1.14"), None)
        );
        let elsewhere = of("--log-dir /d --key-file /k --images m --sight-file /tmp/sight.jpg")
            .unwrap()
            .unwrap();
        assert_eq!(
            elsewhere.sight_file.as_deref(),
            Some(std::path::Path::new("/tmp/sight.jpg"))
        );
        // And acting on it.
        let acting = of("--log-dir /d --progress --act progress --key-file /k")
            .unwrap()
            .unwrap();
        assert!(acting.progress && acting.act_progress);
        // The hello question, and both acting.
        let both = of("--log-dir /d --hello --progress --act hello --act progress --key-file /k")
            .unwrap()
            .unwrap();
        assert!(both.hello && both.act_hello && both.progress && both.act_progress);
        assert_eq!(
            questions(&both),
            " progress=true hello=true act=progress,hello"
        );
        assert_eq!(questions(&acting), " progress=true act=progress");
        let only = of("--log-dir /d --key-file /k --progress")
            .unwrap()
            .unwrap();
        assert_eq!(questions(&only), " progress=true");
        assert_eq!(
            questions(&of("--log-dir /d --key-file /k").unwrap().unwrap()),
            ""
        );
        assert_eq!(
            of("--log-dir /d --key-file /k --utc-offset-minutes -300")
                .unwrap()
                .unwrap()
                .utc_offset_minutes,
            Some(-300)
        );
        assert_eq!(of("--help").unwrap(), None);
        assert_eq!(of("-h").unwrap(), None);
    }

    #[test]
    fn anything_else_is_refused_with_the_reason() {
        let paths = "--key-file /k --log-dir /d";
        assert!(of(paths).is_ok());
        for refused in [
            "",
            "--key-file /k",
            "--log-dir /d",
            "--key-file key --log-dir /d",
            "--key-file /k --log-dir decisions",
            "--key-file /k --log-dir /d --bus-root relative",
            "--key-file /k --log-dir /d --endpoint openrouter.ai/api",
            "--key-file /k --log-dir /d --endpoint ftp://openrouter.ai/api",
            "--key-file /k --log-dir /d --endpoint https://",
            "--key-file /k --log-dir /d --endpoint http://openrouter.ai/api/alpha/decisions",
            "--key-file /k --log-dir /d --endpoint https://host:0/",
            "--key-file /k --log-dir /d --deadline-ms 0",
            "--key-file /k --log-dir /d --deadline-ms 60001",
            "--key-file /k --log-dir /d --deadline-ms soon",
            "--key-file /k --log-dir /d --deadline-ms 1.5",
            "--key-file /k --log-dir /d --key-file /other",
            "--key-file /k --log-dir /d --deadline-ms 5 --deadline-ms 5",
            "--key-file /k --log-dir /d --shadow off",
            "--key-file /k --log-dir /d extra",
            "--key-file /k --log-dir",
            "--key-file /k --log-dir /d --progress --progress",
            "--key-file /k --log-dir /d --act progress",
            "--key-file /k --log-dir /d --progress --act attention",
            "--key-file /k --log-dir /d --act hello",
            "--key-file /k --log-dir /d --progress --act hello",
            "--key-file /k --log-dir /d --hello --hello",
            "--key-file /k --log-dir /d --hello --act hello --act hello",
            "--key-file /k --log-dir /d --progress --act progress --act progress",
            "--key-file /k --log-dir /d --progress --act",
            "--key-file /k --log-dir /d --utc-offset-minutes 841",
            "--key-file /k --log-dir /d --utc-offset-minutes -721",
            "--key-file /k --log-dir /d --utc-offset-minutes 7h",
            "--key-file /k --log-dir /d --utc-offset-minutes 1 --utc-offset-minutes 1",
            "--key-file /k --log-dir /d --sight-file /s",
            "--key-file /k --log-dir /d --images m --sight-file s",
            "--key-file /k --log-dir /d --images --progress",
            "--key-file /k --log-dir /d --images m --images m",
        ] {
            assert!(of(refused).is_err(), "{refused:?}");
        }
        assert_eq!(
            of("--key-file /k --log-dir /d --shadow off").unwrap_err(),
            "unknown option --shadow"
        );
        assert!(of("--key-file /k --log-dir /d --deadline-ms 1").is_ok());
        assert!(of("--key-file /k --log-dir /d --deadline-ms 60000").is_ok());
    }
}
