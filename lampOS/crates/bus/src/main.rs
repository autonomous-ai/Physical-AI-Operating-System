//! `lamp-bus`: the bus from a shell. See `docs/bus.md`.
//!
//! Everything here goes through the library the lamp's processes use, so a
//! `tap` is an ordinary subscriber and can be as slow as it likes without a
//! publisher noticing.
use lamp_bus::{Bus, Header, Kind, Message, Publisher, Subscriber};
use serde::Deserialize;
use std::{
    collections::HashMap,
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

const USAGE: &str = "\
usage: lamp-bus [--root PATH] COMMAND ...

  topics
      The topics under the root, how many listen and whether a value is kept.
  tap TOPIC [--name NAME] [--count N] [--seconds S]
      Subscribe and print each message: its header, then its payload if that
      is JSON, else <N bytes>. A value the topic keeps is printed too, once
      nothing else is waiting. Ends after N messages or S seconds, and with
      status 1 if N was asked for and fewer came.
  latest TOPIC
      Print the topic's current value. Status 1 if it keeps none.
  pub TOPIC latest|event JSON [--at-us N]
      Publish one JSON message as lamp_bus_cli.
  record DIR TOPIC... [--seconds S] [--max-mb N]
      Append every message of the topics to DIR/<topic with / as _>.jsonl
      until killed or for S seconds. A file stops growing at N MiB (64).
  replay FILE [--speed X]
      Publish a recording again as replay, X times as fast (0: at once).

The root is --root, else $LAMP_BUS_ROOT, else /run/lampos-bus.
";

/// Every option takes one value.
const OPTIONS: [&str; 7] = [
    "root", "name", "count", "seconds", "max-mb", "speed", "at-us",
];

/// A wait is cut into pieces this long: an empty one is when a subscriber
/// looks at the topic's kept value.
const TICK: Duration = Duration::from_millis(200);

enum Failure {
    /// The command line was wrong: status 2, with the usage.
    Usage(String),
    /// It was right and did not work: status 1.
    Failed(String),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        // What the library refuses as input is a name or a size the caller
        // typed.
        if error.kind() == io::ErrorKind::InvalidInput {
            Self::Usage(error.to_string())
        } else {
            Self::Failed(error.to_string())
        }
    }
}

struct Arguments {
    words: Vec<String>,
    options: Vec<(String, String)>,
    help: bool,
}

impl Arguments {
    fn read(given: impl Iterator<Item = String>) -> Result<Self, Failure> {
        let mut arguments = Self {
            words: Vec::new(),
            options: Vec::new(),
            help: false,
        };
        let mut given = given;
        while let Some(word) = given.next() {
            if matches!(word.as_str(), "--help" | "-h") {
                arguments.help = true;
            } else if let Some(option) = word.strip_prefix("--") {
                if !OPTIONS.contains(&option) {
                    return Err(Failure::Usage(format!("unknown option {word}")));
                }
                if arguments.options.iter().any(|(name, _)| name == option) {
                    return Err(Failure::Usage(format!("{word} is given twice")));
                }
                let value = given
                    .next()
                    .ok_or_else(|| Failure::Usage(format!("{word} needs a value")))?;
                arguments.options.push((option.into(), value));
            } else {
                arguments.words.push(word);
            }
        }
        Ok(arguments)
    }

    /// Take an option out, read as a `T`.
    fn option<T: FromStr>(&mut self, name: &str, what: &str) -> Result<Option<T>, Failure> {
        let Some(at) = self.options.iter().position(|(option, _)| option == name) else {
            return Ok(None);
        };
        let (_, value) = self.options.remove(at);
        match value.parse() {
            Ok(value) => Ok(Some(value)),
            Err(_) => Err(Failure::Usage(format!("--{name} {value:?} is not {what}"))),
        }
    }

    /// A number of seconds, or of anything else that cannot be negative.
    fn amount(&mut self, name: &str) -> Result<Option<f64>, Failure> {
        match self.option::<f64>(name, "a number")? {
            Some(amount) if !(0.0..=1e9).contains(&amount) => Err(Failure::Usage(format!(
                "--{name} {amount} is not between 0 and 1e9"
            ))),
            amount => Ok(amount),
        }
    }

    /// The words after the command, once every option it knows has been
    /// taken out: what is left over does not belong to it.
    fn words(self, least: usize, most: usize) -> Result<Vec<String>, Failure> {
        if let Some((option, _)) = self.options.first() {
            return Err(Failure::Usage(format!(
                "--{option} does not go with {}",
                self.words[0]
            )));
        }
        let given = self.words.len() - 1;
        if given < least || given > most {
            return Err(Failure::Usage(format!(
                "{} takes {} and was given {given}",
                self.words[0],
                match (least, most) {
                    (0, 0) => "no arguments".to_string(),
                    _ if least == most => format!("{least} arguments"),
                    _ => format!("{least} arguments or more"),
                }
            )));
        }
        Ok(self.words.into_iter().skip(1).collect())
    }
}

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(status) => status,
        Err(Failure::Usage(why)) => {
            eprintln!("lamp-bus: {why}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(why)) => {
            eprintln!("lamp-bus: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run(given: impl Iterator<Item = String>) -> Result<ExitCode, Failure> {
    let mut arguments = Arguments::read(given)?;
    if arguments.help {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    let bus = match arguments.option::<PathBuf>("root", "a path")? {
        Some(root) => Bus::new(root)?,
        None => Bus::from_environment()?,
    };
    let Some(command) = arguments.words.first().cloned() else {
        return Err(Failure::Usage("no command".into()));
    };
    match command.as_str() {
        "topics" => {
            arguments.words(0, 0)?;
            topics(&bus)
        }
        "tap" => {
            let name = arguments.option::<String>("name", "a name")?;
            let count = arguments.option::<u64>("count", "a whole number")?;
            let seconds = arguments.amount("seconds")?;
            let words = arguments.words(1, 1)?;
            let subscriber = bus.subscribe(&words[0], name.as_deref().unwrap_or("tap"))?;
            tap(subscriber, count, seconds.map(Duration::from_secs_f64))
        }
        "latest" => {
            let words = arguments.words(1, 1)?;
            match bus.latest(&words[0])? {
                Some(message) => {
                    say(&line(&message));
                    Ok(ExitCode::SUCCESS)
                }
                None => Err(Failure::Failed(format!("{} keeps no value", words[0]))),
            }
        }
        "pub" => {
            let at_us = arguments.option::<u64>("at-us", "a whole number")?;
            let words = arguments.words(3, 3)?;
            let kind = match words[1].as_str() {
                "latest" => Kind::Latest,
                "event" => Kind::Event,
                other => {
                    return Err(Failure::Usage(format!(
                        "a topic's kind is latest or event, not {other}"
                    )));
                }
            };
            let value: serde_json::Value = serde_json::from_str(&words[2])
                .map_err(|error| Failure::Usage(format!("{:?} is not JSON: {error}", words[2])))?;
            let mut publisher = bus.publisher(&words[0], kind, "lamp_bus_cli")?;
            // This process has no clock it shares with anyone: 0 says so.
            let outcome = publisher.publish_json(at_us.unwrap_or(0), &value)?;
            say(&format!(
                "delivered {} dropped {} removed {}",
                outcome.delivered, outcome.dropped, outcome.removed
            ));
            Ok(ExitCode::SUCCESS)
        }
        "record" => {
            let seconds = arguments.amount("seconds")?;
            let megabytes = arguments.amount("max-mb")?.unwrap_or(64.0);
            let words = arguments.words(2, usize::MAX)?;
            record(
                &bus,
                Path::new(&words[0]),
                &words[1..],
                seconds.map(Duration::from_secs_f64),
                megabytes,
            )
        }
        "replay" => {
            let speed = arguments.amount("speed")?.unwrap_or(1.0);
            let words = arguments.words(1, 1)?;
            replay(&bus, Path::new(&words[0]), speed)
        }
        other => Err(Failure::Usage(format!("unknown command {other}"))),
    }
}

/// One line on standard output, at once. False when nobody reads it any
/// more, which is how a `tap` piped into `head` ends.
fn say(line: &str) -> bool {
    let mut out = io::stdout().lock();
    writeln!(out, "{line}").and_then(|()| out.flush()).is_ok()
}

/// A message on one line: what it says of itself, then what it carries.
fn line(message: &Message) -> String {
    let header = serde_json::to_string(&message.header).unwrap_or_default();
    match message.json() {
        // Printed from the value, so that a payload with line ends in it
        // is still one line.
        Ok(value) => format!("{header} {value}"),
        Err(_) => format!("{header} <{} bytes>", message.payload.len()),
    }
}

/// How long to wait in one go, or `None` once the time is up.
fn tick(until: Option<Instant>) -> Option<Duration> {
    match until {
        None => Some(TICK),
        Some(until) => {
            let left = until.saturating_duration_since(Instant::now());
            (!left.is_zero()).then(|| left.min(TICK))
        }
    }
}

fn topics(bus: &Bus) -> Result<ExitCode, Failure> {
    for topic in bus.topics()? {
        let stale = match topic.stale {
            0 => String::new(),
            stale => format!(" stale={stale}"),
        };
        say(&format!(
            "{} subscribers={}{stale} latest={}",
            topic.name,
            topic.subscribers,
            if topic.latest { "yes" } else { "no" }
        ));
    }
    Ok(ExitCode::SUCCESS)
}

fn tap(
    mut subscriber: Subscriber,
    count: Option<u64>,
    lasting: Option<Duration>,
) -> Result<ExitCode, Failure> {
    let until = lasting.map(|lasting| Instant::now() + lasting);
    let mut printed = 0;
    while count.is_none_or(|count| printed < count) {
        let Some(wait) = tick(until) else {
            break;
        };
        let Some(message) = subscriber.recv(wait)? else {
            continue;
        };
        if message.publisher_restarted {
            eprintln!("lamp-bus: {} began again", message.header.source);
        }
        if message.gap > 0 {
            eprintln!(
                "lamp-bus: missed {} from {} before seq {}",
                message.gap, message.header.source, message.header.seq
            );
        }
        if !say(&line(&message)) {
            break;
        }
        printed += 1;
    }
    Ok(if count.is_some_and(|count| printed < count) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 with padding, added to `text`.
fn base64(bytes: &[u8], text: &mut Vec<u8>) {
    for chunk in bytes.chunks(3) {
        let group = chunk
            .iter()
            .fold(0u32, |group, &byte| group << 8 | u32::from(byte))
            << (8 * (3 - chunk.len()));
        for place in 0..4 {
            text.push(if place <= chunk.len() {
                ALPHABET[(group >> (18 - 6 * place) & 63) as usize]
            } else {
                b'='
            });
        }
    }
}

/// The bytes of padded base64, or `None` if it is not that.
fn unbase64(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    for (index, chunk) in text.chunks(4).enumerate() {
        let padding = chunk.iter().rev().take_while(|&&byte| byte == b'=').count();
        // Padding ends the text and never makes a whole group.
        if padding > 2 || (padding > 0 && (index + 1) * 4 != text.len()) {
            return None;
        }
        let mut group = 0u32;
        for &byte in &chunk[..4 - padding] {
            group = group << 6 | ALPHABET.iter().position(|&letter| letter == byte)? as u32;
        }
        group <<= 6 * padding;
        bytes.extend_from_slice(&group.to_be_bytes()[1..4 - padding]);
    }
    Some(bytes)
}

/// One message as a line of a recording.
fn recorded(message: &Message, line: &mut Vec<u8>) -> io::Result<()> {
    line.clear();
    line.extend_from_slice(b"{\"header\":");
    serde_json::to_writer(&mut *line, &message.header)?;
    line.extend_from_slice(b",\"payload_b64\":\"");
    base64(&message.payload, line);
    line.extend_from_slice(b"\"}\n");
    Ok(())
}

/// One topic being recorded.
struct Recording {
    subscriber: Subscriber,
    path: PathBuf,
    file: fs::File,
    bytes: u64,
}

/// How a topic's recording ended.
struct Recorded {
    topic: String,
    path: PathBuf,
    messages: u64,
    missed: u64,
    full: bool,
    failed: Option<String>,
}

impl Recording {
    /// Append each message as it comes. A line goes to the file in one
    /// write and nothing is held back, so the file is whole lines whenever
    /// this process is killed.
    fn run(mut self, until: Option<Instant>, most: u64, megabytes: f64) -> Recorded {
        let mut ended = Recorded {
            topic: self.subscriber.topic().into(),
            path: self.path,
            messages: 0,
            missed: 0,
            full: false,
            failed: None,
        };
        let mut line = Vec::new();
        while let Some(wait) = tick(until) {
            let message = match self.subscriber.recv(wait) {
                Ok(Some(message)) => message,
                Ok(None) => continue,
                Err(error) => {
                    ended.failed = Some(error.to_string());
                    break;
                }
            };
            ended.missed += message.gap;
            let written = recorded(&message, &mut line).and_then(|()| {
                if self.bytes + line.len() as u64 > most {
                    return Ok(false);
                }
                self.file.write_all(&line).map(|()| true)
            });
            match written {
                Ok(true) => {
                    self.bytes += line.len() as u64;
                    ended.messages += 1;
                }
                Ok(false) => {
                    eprintln!(
                        "lamp-bus: record: {} has reached {megabytes} MiB: no more of {} is recorded",
                        ended.path.display(),
                        ended.topic
                    );
                    ended.full = true;
                    break;
                }
                Err(error) => {
                    ended.failed = Some(error.to_string());
                    break;
                }
            }
        }
        ended
    }
}

fn record(
    bus: &Bus,
    directory: &Path,
    topics: &[String],
    lasting: Option<Duration>,
    megabytes: f64,
) -> Result<ExitCode, Failure> {
    let most = (megabytes * 1024.0 * 1024.0) as u64;
    let mut paths: Vec<PathBuf> = Vec::new();
    for topic in topics {
        let path = directory.join(format!("{}.jsonl", topic.replace('/', "_")));
        if paths.contains(&path) {
            return Err(Failure::Usage(format!(
                "{topic} would be recorded to {}, as another topic is",
                path.display()
            )));
        }
        paths.push(path);
    }
    // Every subscriber is bound and every file open before any of them
    // runs: a name that is wrong stops the whole recording, not a part.
    let mut subscribers = Vec::new();
    for topic in topics {
        subscribers.push(bus.subscribe(topic, "record")?);
    }
    fs::create_dir_all(directory)?;
    let mut recordings = Vec::new();
    for (subscriber, path) in subscribers.into_iter().zip(paths) {
        let file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)?;
        let bytes = file.metadata()?.len();
        recordings.push(Recording {
            subscriber,
            path,
            file,
            bytes,
        });
    }
    let until = lasting.map(|lasting| Instant::now() + lasting);
    let running: Vec<_> = recordings
        .into_iter()
        .map(|recording| thread::spawn(move || recording.run(until, most, megabytes)))
        .collect();
    let mut failed = false;
    for thread in running {
        let Ok(ended) = thread.join() else {
            failed = true;
            continue;
        };
        eprintln!(
            "lamp-bus: record: {} of {} in {}, {} missed{}",
            ended.messages,
            ended.topic,
            ended.path.display(),
            ended.missed,
            if ended.full { ", file full" } else { "" }
        );
        if let Some(why) = ended.failed {
            eprintln!("lamp-bus: record: {}: {why}", ended.topic);
            failed = true;
        }
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// A line of a recording.
#[derive(Deserialize)]
struct Line {
    header: Header,
    payload_b64: String,
}

fn replay(bus: &Bus, file: &Path, speed: f64) -> Result<ExitCode, Failure> {
    let lines = io::BufReader::new(fs::File::open(file)?).lines();
    let mut publishers: Vec<Publisher> = Vec::new();
    // When each recorded publisher instance was first replayed, and what
    // its clock said then. The spacing kept is the one within an instance:
    // two publishers need not have shared a clock.
    let mut instances: HashMap<String, (Instant, u64)> = HashMap::new();
    let (mut sent, mut refused) = (0u64, 0u64);
    for (index, text) in lines.enumerate() {
        let wrong =
            |why: String| Failure::Failed(format!("{} line {}: {why}", file.display(), index + 1));
        let text = text?;
        if text.trim().is_empty() {
            continue;
        }
        let Line {
            header,
            payload_b64,
        } = serde_json::from_str(&text).map_err(|error| wrong(error.to_string()))?;
        let payload =
            unbase64(&payload_b64).ok_or_else(|| wrong("payload_b64 is not base64".into()))?;
        if speed > 0.0 {
            let now = Instant::now();
            let &mut (began, first_us) = instances
                .entry(header.boot.clone())
                .or_insert((now, header.at_us));
            let after = header.at_us.saturating_sub(first_us) as f64 / 1e6 / speed;
            let due = Duration::try_from_secs_f64(after)
                .ok()
                .and_then(|after| began.checked_add(after))
                .ok_or_else(|| wrong(format!("due {after} seconds on")))?;
            thread::sleep(due.saturating_duration_since(now));
        }
        let at = match publishers
            .iter()
            .position(|publisher| publisher.topic() == header.topic)
        {
            Some(at) => at,
            None => {
                let publisher = bus
                    .publisher(&header.topic, header.kind, "replay")
                    .map_err(|error| wrong(error.to_string()))?;
                publishers.push(publisher);
                publishers.len() - 1
            }
        };
        let publisher = &mut publishers[at];
        publisher.expires_after(header.expires_ms.map(Duration::from_millis));
        match publisher.publish_bytes(header.at_us, &header.enc, &payload) {
            Ok(_) => sent += 1,
            Err(error) => {
                eprintln!("lamp-bus: replay: line {}: {error}", index + 1);
                refused += 1;
            }
        }
    }
    let totals = publishers
        .iter()
        .map(Publisher::totals)
        .fold((0, 0), |(delivered, dropped), totals| {
            (delivered + totals.delivered, dropped + totals.dropped)
        });
    eprintln!(
        "lamp-bus: replay: {sent} messages, {} delivered, {} dropped",
        totals.0, totals.1
    );
    Ok(if refused > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_the_standard_alphabet_with_padding_and_reads_back() {
        let text = |bytes: &[u8]| {
            let mut text = Vec::new();
            base64(bytes, &mut text);
            String::from_utf8(text).unwrap()
        };
        // RFC 4648, section 10.
        for (bytes, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(text(bytes.as_bytes()), expected);
            assert_eq!(unbase64(expected).unwrap(), bytes.as_bytes());
        }
        assert_eq!(text(&[0xfb, 0xff, 0xfe]), "+//+");
        // Every byte value, at every length up to a few groups.
        let all: Vec<u8> = (0..=255).collect();
        for length in 0..all.len() {
            assert_eq!(unbase64(&text(&all[..length])).unwrap(), &all[..length]);
        }
        for wrong in [
            "Zg", "Zg=", "Z===", "====", "Zg==Zg==", "Zm9v\n", "Zm9-", "=Zm9",
        ] {
            assert!(unbase64(wrong).is_none(), "{wrong}");
        }
    }

    #[test]
    fn options_are_taken_out_and_what_is_left_over_is_refused() {
        let read = |words: &[&str]| Arguments::read(words.iter().map(|word| word.to_string()));
        let Ok(mut arguments) = read(&["tap", "--count", "3", "servos", "--root", "/x"]) else {
            panic!("refused");
        };
        assert!(matches!(arguments.option::<u64>("count", ""), Ok(Some(3))));
        assert!(matches!(arguments.option::<u64>("count", ""), Ok(None)));
        // `--root` has not been taken: it does not go with the command.
        assert!(matches!(arguments.words(1, 1), Err(Failure::Usage(_))));
        let Ok(arguments) = read(&["tap", "servos"]) else {
            panic!("refused");
        };
        assert!(matches!(arguments.words(1, 1), Ok(words) if words == ["servos"]));
        for wrong in [
            &["tap", "--count"][..],
            &["tap", "--colour", "red"],
            &["tap", "--count", "1", "--count", "2"],
        ] {
            assert!(matches!(read(wrong), Err(Failure::Usage(_))), "{wrong:?}");
        }
        let Ok(mut arguments) = read(&["tap", "--seconds", "-1", "--count", "x"]) else {
            panic!("refused");
        };
        assert!(matches!(
            arguments.amount("seconds"),
            Err(Failure::Usage(_))
        ));
        assert!(matches!(
            arguments.option::<u64>("count", "a whole number"),
            Err(Failure::Usage(why)) if why.contains("--count \"x\" is not a whole number")
        ));
    }
}
