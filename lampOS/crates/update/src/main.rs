//! `lamp-update`: a lamp keeps its lampOS current by itself
//! (docs/updates.md). On the lamp: `check` says what the newest build of
//! its channel would change, `once` does one round, `serve` one round
//! every so often. Where builds are made: `manifest` and `sign`.
use lamp_update::{
    Config, Outcome, World,
    fetch::Web,
    install::{self, Install, System},
    manifest, publish, round,
    world::{BusEars, Deaf, Ears, Monotonic, Statvfs, Systemctl},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

const USAGE: &str = "\
lamp-update check|once|serve --channel-url URL [--channel NAME] [--key-file PATH] [--lamp NAME]
                             [--apply on|off] [--every-s N] [--quiet-wait-s N] [--watch-s N]
                             [--keep-free-mb N] [--root DIR] [--bus-root DIR]
lamp-update install --channel-url URL [--channel NAME] [--key-file PATH] [--setup on|off]
lamp-update manifest --binaries DIR --deploy DIR --channel NAME --sequence N --commit SHA --files-url URL
lamp-update sign --key-file PRIVATE --manifest FILE --out FILE
lamp-update public-key --key-file PRIVATE
lamp-update --help

check   says what the newest build of the channel would change on this lamp, and
        writes nothing (with any --apply).
once    one round: with --apply on, fetches what changed, installs it beside the old
        build, switches at a quiet moment (nobody heard or answered for 45 s), watches
        each part for --watch-s (60) and puts the previous build back if one is not
        well. Without --apply on (the default) it is check.
serve   a round every --every-s (900), until stopped.
install lampOS on a blank lamp (docs/install.md): the newest build's binary of every
        product part, in its folder with its current link; the product units, tmpfiles,
        users and zone; enables the lamp and starts nothing. Run again, it changes
        nothing. As root, on the lamp itself.

  --channel-url   the channel's folder, https://.../ or file:///.../, ending in /
  --channel       the channel followed (lamp-voice)
  --key-file      the lamp's public key, base64 (/etc/lampOS/update/key.pub)
  --setup         install: on also writes setup's two settings, so a lamp without
                  setup's marker starts setup at boot (off)
  --lamp          this lamp's name, which of the build's lamp folders is its own
                  (the host name in ROOT/etc/hostname)
  --quiet-wait-s  how long a round waits for a quiet moment (3600)
  --keep-free-mb  room kept free on the disk beyond the new build (256)
  --root          the filesystem the lamp's paths are under (/), for tests
  --bus-root      the bus (LAMP_BUS_ROOT, or /run/lampos-bus)

manifest  writes DIR/manifest.json for the binaries in --binaries and the units of
          each lamp folder (one with a LAMP.md) in --deploy, copied beside them.
sign      writes the signed envelope (lampos-manifest.json) for a manifest, with a
          private key: an unencrypted PKCS#8 Ed25519 key in base64.
public-key prints the public key of a private key, in base64: what a lamp is given.

check and once end with 0 when up to date, switched, or only saying what they would
do; 1 when refused, put back or failed; 3 when the channel cannot be reached (nothing
changed); 4 when no quiet moment came (nothing switched). A wrong option ends with 2.
";

fn usage() -> ExitCode {
    eprint!("{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = arguments.split_first() else {
        return usage();
    };
    let Some(options) = options(rest) else {
        return usage();
    };
    let result = match command.as_str() {
        "--help" => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        "check" | "once" | "serve" => return on_the_lamp(command, &options),
        "install" => install(&options),
        "manifest" => make_manifest(&options),
        "sign" => sign(&options),
        "public-key" => public_key(&options),
        _ => return usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("lamp-update: {why}");
            ExitCode::FAILURE
        }
    }
}

/// `--name value` pairs, each name once.
fn options(words: &[String]) -> Option<BTreeMap<String, String>> {
    let mut found = BTreeMap::new();
    let mut words = words.iter();
    while let Some(name) = words.next() {
        let name = name.strip_prefix("--")?;
        let value = words.next()?;
        if found.insert(name.to_owned(), value.clone()).is_some() {
            return None;
        }
    }
    Some(found)
}

fn wanted<'a>(options: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, String> {
    options
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("--{name} is needed"))
}

fn make_manifest(options: &BTreeMap<String, String>) -> Result<(), String> {
    known(
        options,
        &[
            "binaries",
            "deploy",
            "channel",
            "sequence",
            "commit",
            "files-url",
        ],
    )?;
    let sequence = wanted(options, "sequence")?
        .parse()
        .map_err(|_| "--sequence is a whole number".to_owned())?;
    let made = publish::make(
        Path::new(wanted(options, "binaries")?),
        Path::new(wanted(options, "deploy")?),
        wanted(options, "channel")?,
        sequence,
        wanted(options, "commit")?,
        wanted(options, "files-url")?,
    )
    .map_err(|error| error.to_string())?;
    println!(
        "manifest of build {} of {}: {} binaries, {} units",
        made.sequence,
        made.channel,
        made.files.len(),
        made.units.len()
    );
    Ok(())
}

fn install(options: &BTreeMap<String, String>) -> Result<(), String> {
    known(options, &["channel-url", "channel", "key-file", "setup"])?;
    let setup = match options.get("setup").map(String::as_str) {
        None | Some("off") => false,
        Some("on") => true,
        Some(_) => return Err("--setup is on or off".into()),
    };
    let channel_url = wanted(options, "channel-url")?.to_owned();
    if !(channel_url.starts_with("https://") || channel_url.starts_with("file:///"))
        || !channel_url.ends_with('/')
    {
        return Err("--channel-url is an https:// or file:/// folder, ending in /".into());
    }
    let channel = options.get("channel").map_or("lamp-voice", String::as_str);
    if !manifest::is_token(channel) {
        return Err("--channel is a plain name".into());
    }
    // The image's key: a trust root never comes from the channel (#806).
    let key_file = options
        .get("key-file")
        .map_or("/etc/lampOS/update/key.pub", String::as_str);
    let key = fs::read_to_string(key_file).map_err(|error| format!("{key_file}: {error}"))?;
    let key = manifest::read_public_key(&key).map_err(|why| format!("{key_file}: {why}"))?;
    let config = Install {
        root: PathBuf::from("/"),
        channel: channel.to_owned(),
        channel_url,
        key,
        records: lamp_update::install::RECORDS.into(),
        setup,
        models: install::models()?,
    };
    let mut channel = Web::new().map_err(|error| format!("TLS cannot be set up: {error}"))?;
    let mut say = |line: &str| eprintln!("lamp-update: {line}");
    let done = install::install(&config, &mut channel, &mut System, &mut say)?;
    if done.changed.is_empty() {
        println!("build {} is installed; nothing changed", done.sequence);
    } else {
        println!(
            "build {} installed: {} changes; nothing started",
            done.sequence,
            done.changed.len()
        );
    }
    Ok(())
}

fn sign(options: &BTreeMap<String, String>) -> Result<(), String> {
    known(options, &["key-file", "manifest", "out"])?;
    let key =
        fs::read_to_string(wanted(options, "key-file")?).map_err(|error| error.to_string())?;
    let key = manifest::read_private_key(&key).map_err(|why| why.0)?;
    let bytes = fs::read(wanted(options, "manifest")?).map_err(|error| error.to_string())?;
    let envelope = manifest::seal(&bytes, &key).map_err(|why| why.0)?;
    fs::write(wanted(options, "out")?, envelope).map_err(|error| error.to_string())?;
    let public =
        manifest::read_public_key(&manifest::public_key_text(&key)).map_err(|why| why.0)?;
    println!("signed with key {}", manifest::fingerprint(&public));
    Ok(())
}

fn public_key(options: &BTreeMap<String, String>) -> Result<(), String> {
    known(options, &["key-file"])?;
    let key =
        fs::read_to_string(wanted(options, "key-file")?).map_err(|error| error.to_string())?;
    let key = manifest::read_private_key(&key).map_err(|why| why.0)?;
    println!("{}", manifest::public_key_text(&key));
    Ok(())
}

fn known(options: &BTreeMap<String, String>, names: &[&str]) -> Result<(), String> {
    match options.keys().find(|name| !names.contains(&name.as_str())) {
        Some(name) => Err(format!("--{name} is not an option here")),
        None => Ok(()),
    }
}

fn seconds(
    options: &BTreeMap<String, String>,
    name: &str,
    default: u64,
) -> Result<Duration, String> {
    match options.get(name) {
        None => Ok(Duration::from_secs(default)),
        Some(value) => value
            .parse::<u64>()
            .ok()
            .filter(|seconds| (1..=86_400).contains(seconds))
            .map(Duration::from_secs)
            .ok_or_else(|| format!("--{name} is 1 to 86400 seconds")),
    }
}

/// The configuration of a round on the lamp, from its options.
fn config(
    command: &str,
    options: &BTreeMap<String, String>,
) -> Result<(Config, Duration, PathBuf), String> {
    known(
        options,
        &[
            "channel-url",
            "channel",
            "key-file",
            "lamp",
            "apply",
            "every-s",
            "quiet-wait-s",
            "watch-s",
            "keep-free-mb",
            "root",
            "bus-root",
        ],
    )?;
    let root = PathBuf::from(options.get("root").map_or("/", String::as_str));
    let on = |path: &str| root.join(path.trim_start_matches('/'));
    let channel_url = wanted(options, "channel-url")?.to_owned();
    if !(channel_url.starts_with("https://") || channel_url.starts_with("file:///"))
        || !channel_url.ends_with('/')
    {
        return Err("--channel-url is an https:// or file:/// folder, ending in /".into());
    }
    let channel = options.get("channel").map_or("lamp-voice", String::as_str);
    if !manifest::is_token(channel) {
        return Err("--channel is a plain name".into());
    }
    let key_file = options
        .get("key-file")
        .cloned()
        .unwrap_or_else(|| on("/etc/lampOS/update/key.pub").display().to_string());
    let key = fs::read_to_string(&key_file).map_err(|error| format!("{key_file}: {error}"))?;
    let key = manifest::read_public_key(&key).map_err(|why| format!("{key_file}: {why}"))?;
    let lamp = match options.get("lamp") {
        Some(lamp) => lamp.clone(),
        None => fs::read_to_string(on("/etc/hostname"))
            .map(|name| name.trim().to_owned())
            .map_err(|error| format!("the host name cannot be read ({error}); give --lamp"))?,
    };
    if !manifest::is_token(&lamp) {
        return Err(format!("{lamp:?} is not a lamp's name"));
    }
    let apply = match options.get("apply").map(String::as_str) {
        None | Some("off") => false,
        Some("on") => command != "check",
        Some(_) => return Err("--apply is on or off".into()),
    };
    let keep_free_mb: u64 = match options.get("keep-free-mb") {
        None => 256,
        Some(value) => value
            .parse()
            .ok()
            .filter(|mb| *mb <= 1_000_000)
            .ok_or("--keep-free-mb is a whole number of MB")?,
    };
    let bus_root = options
        .get("bus-root")
        .cloned()
        .or_else(|| std::env::var(lamp_bus::ROOT_VARIABLE).ok())
        .unwrap_or_else(|| lamp_bus::DEFAULT_ROOT.to_owned());
    let config = Config {
        root,
        units: "/etc/systemd/system/".into(),
        records: lamp_update::install::RECORDS.into(),
        channel: channel.to_owned(),
        channel_url,
        key,
        lamp,
        apply,
        quiet_wait: seconds(options, "quiet-wait-s", 3600)?,
        watch: seconds(options, "watch-s", 60)?,
        keep_free: keep_free_mb * 1_000_000,
    };
    Ok((
        config,
        seconds(options, "every-s", 900)?,
        PathBuf::from(bus_root),
    ))
}

fn status(outcome: &Outcome) -> u8 {
    match outcome {
        Outcome::UpToDate(_)
        | Outcome::Switched(_)
        | Outcome::Would(_)
        | Outcome::NothingPublished => 0,
        Outcome::Unreachable(_) => 3,
        Outcome::NotQuiet(_) => 4,
        Outcome::Refused(_) | Outcome::RolledBack { .. } | Outcome::Failed(_) => 1,
    }
}

fn on_the_lamp(command: &str, options: &BTreeMap<String, String>) -> ExitCode {
    let (config, every, bus_root) = match config(command, options) {
        Ok(found) => found,
        Err(why) => {
            eprintln!("lamp-update: {why}");
            return usage();
        }
    };
    let mut say = |line: &str| eprintln!("lamp-update: {line}");
    let mut channel = match Web::new() {
        Ok(web) => web,
        Err(error) => {
            eprintln!("lamp-update: TLS cannot be set up: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Without --apply on nothing is waited for or watched: the bus is not
    // even subscribed to.
    let mut ears: Box<dyn Ears> = if config.apply {
        Box::new(BusEars::new(&bus_root, &mut say))
    } else {
        Box::new(Deaf)
    };
    let mut systemd = Systemctl::default();
    let clock = Monotonic::default();
    eprintln!(
        "lamp-update: {command} channel={} url={} lamp={} apply={} key={}",
        config.channel,
        config.channel_url,
        config.lamp,
        if config.apply { "on" } else { "off" },
        manifest::fingerprint(&config.key)
    );
    let mut last = String::new();
    loop {
        let outcome = {
            let mut world = World {
                channel: &mut channel,
                systemd: &mut systemd,
                ears: ears.as_mut(),
                disk: &Statvfs,
                clock: &clock,
                say: &mut say,
            };
            round(&config, &mut world)
        };
        let text = outcome.to_string();
        if command != "serve" {
            println!("{text}");
            return ExitCode::from(status(&outcome));
        }
        // A round that says what the one before said is not said again.
        if text != last {
            eprintln!("lamp-update: {text}");
            last = text;
        }
        // Between rounds the bus is still read, so that what the voice said
        // meanwhile does not wait in the sockets.
        let _ = ears.listen(every);
    }
}
