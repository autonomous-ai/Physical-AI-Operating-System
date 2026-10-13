//! Publish and subscribe between the lamp's processes, with no process in
//! the middle. See `docs/bus.md`.
//!
//! The lamp is several processes, each somebody's own, and the rule is that
//! one of them failing takes none of the others down. A broker would be the
//! one process all of them need. So a topic here is a directory, a
//! subscriber is a datagram socket in it, and a publisher sends each message
//! to every socket it finds there without waiting for any: a subscriber that
//! is slow, stopped or dead loses messages and nobody else notices.
//!
//! What is lost is not hidden. Every message carries its publisher's count,
//! and a subscriber is told with each one how many it missed before it.
//!
//! ```no_run
//! use lamp_bus::{Bus, Kind};
//! use std::time::Duration;
//!
//! let bus = Bus::from_environment()?;
//! let mut servos = bus.publisher("servos", Kind::Latest, "body")?;
//! servos.publish_json(98_123_456, &serde_json::json!({"yaw": 1990}))?;
//!
//! let mut follow = bus.subscribe("servos", "follow")?;
//! if let Some(message) = follow.recv(Duration::from_millis(100))? {
//!     println!("{} missed, yaw {}", message.gap, message.json()?["yaw"]);
//! }
//! # Ok::<(), std::io::Error>(())
//! ```
mod name;
mod publish;
mod subscribe;
mod wire;

pub use name::MOST_TOPIC_BYTES;
pub use publish::{MOST_SUBSCRIBERS, Published, Publisher, SubscriberTotals, Totals};
pub use subscribe::Subscriber;
pub use wire::{Header, JSON, Kind, MOST_BYTES, Message, NONE};

use std::{
    env, fs, io,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, PermissionsExt},
        net::UnixDatagram,
    },
    path::{Path, PathBuf},
    time::Duration,
};

/// Where the topics are when nothing else is said: memory, not a disk, and
/// empty after a restart of the lamp.
pub const DEFAULT_ROOT: &str = "/run/lampos-bus";
/// The environment variable that says otherwise.
pub const ROOT_VARIABLE: &str = "LAMP_BUS_ROOT";

/// The longest path a subscriber's socket may have. A Unix socket's address
/// holds 104 bytes on macOS and 108 on Linux, the last of them a NUL, and
/// what is tried on one must hold on the other.
pub const MOST_SOCKET_PATH_BYTES: usize = 103;

/// Sockets and the `latest` file: the owner and the group, nobody else.
const FILE_MODE: u32 = 0o660;

#[derive(Clone, Debug)]
pub struct Bus {
    root: PathBuf,
    directory_mode: u32,
    rescan_every: Duration,
}

/// One topic as the directories show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topic {
    pub name: String,
    /// Sockets in `subs/` with a process behind them.
    pub subscribers: usize,
    /// Sockets left by a subscriber that died. The topic's next publish
    /// removes them.
    pub stale: usize,
    /// There is a `latest` file.
    pub latest: bool,
}

impl Bus {
    /// The bus under `root`. Nothing is created until somebody publishes a
    /// value that is kept, or subscribes.
    pub fn new(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        // A socket is addressed by its path, and a relative one would mean
        // another place to every process with another working directory.
        if !root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("bus root {} is not an absolute path", root.display()),
            ));
        }
        Ok(Self {
            root,
            directory_mode: 0o770,
            rescan_every: Duration::from_millis(200),
        })
    }

    /// The bus under `LAMP_BUS_ROOT`, or under `/run/lampos-bus`.
    pub fn from_environment() -> io::Result<Self> {
        match env::var_os(ROOT_VARIABLE) {
            Some(root) if !root.is_empty() => Self::new(root),
            _ => Self::new(DEFAULT_ROOT),
        }
    }

    /// The mode of every directory this bus creates, whatever the umask.
    pub fn directory_mode(mut self, mode: u32) -> Self {
        self.directory_mode = mode;
        self
    }

    /// How long a publisher goes on with the subscribers it knows before it
    /// reads the directory again: the longest a new subscriber waits for
    /// its first message, and how often a subscriber with nothing waiting
    /// looks at `latest`.
    pub fn rescan_every(mut self, every: Duration) -> Self {
        self.rescan_every = every;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A publisher of `topic` called `source`. Touches nothing: a root that
    /// is not there yet is found, or not, with each publish.
    pub fn publisher(&self, topic: &str, kind: Kind, source: &str) -> io::Result<Publisher> {
        Publisher::new(self, topic, kind, source)
    }

    /// Bind a socket in the topic's `subs/`, creating the directories that
    /// are missing. Publishers already running find it at their next look.
    pub fn subscribe(&self, topic: &str, name: &str) -> io::Result<Subscriber> {
        Subscriber::new(self, topic, name)
    }

    /// The topic's current value, if it keeps one.
    pub fn latest(&self, topic: &str) -> io::Result<Option<Message>> {
        name::topic(topic)?;
        let kept = wire::kept(&self.root.join(topic).join("latest"), topic)?;
        Ok(kept.map(|(header, payload)| Message {
            header,
            payload,
            gap: 0,
            publisher_restarted: false,
        }))
    }

    /// Every topic that has a `subs/` or a `latest`, by name. A topic of
    /// events that nobody has subscribed to has neither and is not listed.
    pub fn topics(&self) -> io::Result<Vec<Topic>> {
        let mut found = Vec::new();
        match self.look(&self.root, "", &mut found) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    fn look(&self, directory: &Path, prefix: &str, found: &mut Vec<Topic>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let Ok(part) = entry.file_name().into_string() else {
                continue;
            };
            let name = format!("{prefix}{part}");
            if part.contains('/') || name::topic(&name).is_err() || !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            let subs = path.join("subs");
            let latest = path.join("latest").is_file();
            if latest || subs.is_dir() {
                let (subscribers, stale) = held(&subs);
                found.push(Topic {
                    name: name.clone(),
                    subscribers,
                    stale,
                    latest,
                });
            }
            // A directory that vanishes while it is read is not an error.
            let _ = self.look(&path, &format!("{name}/"), found);
        }
        Ok(())
    }
}

/// How many sockets in `subs` have somebody behind them, and how many do
/// not. Connecting asks the kernel without sending anybody anything.
fn held(subs: &Path) -> (usize, usize) {
    let (mut live, mut stale) = (0, 0);
    let Ok(entries) = fs::read_dir(subs) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        // A subscriber's socket has its `.sock` name once it can be sent to.
        if !entry.file_name().to_string_lossy().ends_with(".sock")
            || !entry.file_type().is_ok_and(|kind| kind.is_socket())
        {
            continue;
        }
        let refused = UnixDatagram::unbound()
            .and_then(|probe| probe.connect(entry.path()))
            .is_err_and(|error| error.kind() == io::ErrorKind::ConnectionRefused);
        if refused {
            stale += 1;
        } else {
            live += 1;
        }
    }
    (live, stale)
}

/// Create `path` and whatever is missing above it, each with exactly
/// `mode`. A directory that is already there is left as it is.
fn make_directory(path: &Path, mode: u32) -> io::Result<()> {
    let create = || match fs::DirBuilder::new().mode(mode).create(path) {
        // The umask may have taken bits away.
        Ok(()) => fs::set_permissions(path, fs::Permissions::from_mode(mode)),
        // Another process made it first.
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        Err(error) => Err(error),
    };
    match create() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => match path.parent() {
            Some(parent) => {
                make_directory(parent, mode)?;
                create()
            }
            None => Err(error),
        },
        done => done,
    }
}
