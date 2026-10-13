//! What a topic carries, said once by whoever publishes it.
//!
//! ROS checks, when a subscriber connects, that both ends mean the same
//! message: they "must agree on the layout". Here nothing connects, so the
//! agreement is a file. A publisher that declares its topic writes
//! `<topic>/declaration`, one line of JSON: the type's name and version, the
//! topic's kind, how often a message is promised and how long one lasts. A
//! second publisher that says another type or kind is refused, and so is a
//! subscriber that expects another type. Every message then carries the type
//! name in its header, for the subscriber that was there before any
//! declaration was.
//!
//! The file is the topic's and not a publisher's: it stays when its writer
//! goes, like the kept value, until the lamp restarts or somebody removes it.
use crate::{FILE_MODE, name, wire::Kind};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    time::Duration,
};

/// More than any declaration is: a file that is not one could be any size.
const MOST_BYTES: u64 = 4_096;

/// Another publisher can write the first declaration between this one's
/// look and its own write. It then looks again, this many times at most.
const TRIES: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    /// The type's name: its package, then its own name
    /// (`lamp_msgs/JointState`).
    #[serde(rename = "type")]
    pub type_name: String,
    /// Which version of that type the publisher writes. A subscriber is not
    /// refused over it: a newer version adds fields an older reader ignores.
    pub version: u32,
    pub kind: Kind,
    /// A message is promised at least this often. A subscriber calls the
    /// topic late when none has come for more than twice as long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_ms: Option<u64>,
    /// How long a message means anything. The publisher puts it in every
    /// header as `expires_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifespan_ms: Option<u64>,
}

impl Declaration {
    /// A topic of `kind` carrying `type_name`, with no period and no
    /// lifespan promised.
    pub fn new(type_name: &str, version: u32, kind: Kind) -> Self {
        Self {
            type_name: type_name.into(),
            version,
            kind,
            period_ms: None,
            lifespan_ms: None,
        }
    }

    /// Promise a message at least every `period`.
    pub fn every(mut self, period: Duration) -> Self {
        self.period_ms = Some(period.as_millis() as u64);
        self
    }

    /// Say that a message means nothing `lifespan` after it was sent.
    pub fn lasting(mut self, lifespan: Duration) -> Self {
        self.lifespan_ms = Some(lifespan.as_millis() as u64);
        self
    }

    pub fn period(&self) -> Option<Duration> {
        self.period_ms.map(Duration::from_millis)
    }

    pub fn lifespan(&self) -> Option<Duration> {
        self.lifespan_ms.map(Duration::from_millis)
    }

    /// Whether this can be a topic's declaration at all.
    pub(crate) fn check(&self) -> io::Result<()> {
        name::type_name(&self.type_name)?;
        if self.period_ms == Some(0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a declared period of 0 ms promises nothing: leave it out",
            ));
        }
        Ok(())
    }

    fn kind_word(&self) -> &'static str {
        match self.kind {
            Kind::Latest => "latest",
            Kind::Event => "event",
        }
    }
}

fn invalid(why: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

/// What the file at `path` declares, or `None` if there is no file.
pub(crate) fn read(path: &Path) -> io::Result<Option<Declaration>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MOST_BYTES + 1).read_to_end(&mut bytes)?;
    let declaration: Declaration = serde_json::from_slice(&bytes)
        .map_err(|error| invalid(format!("{}: not a declaration: {error}", path.display())))?;
    declaration
        .check()
        .map_err(|error| invalid(format!("{}: {error}", path.display())))?;
    Ok(Some(declaration))
}

/// Why a subscriber that expects `expected` cannot have `topic`, if what is
/// declared for it is another type.
pub(crate) fn expected(topic: &str, declared: &Declaration, expected: &str) -> io::Result<()> {
    if declared.type_name == expected {
        return Ok(());
    }
    Err(invalid(format!(
        "topic {topic} is declared to carry {} (version {}) and this subscriber expects \
         {expected}: the two programs disagree about the topic",
        declared.type_name, declared.version
    )))
}

/// Make `declaration` the topic's, in `directory`, unless the topic already
/// has one of another type or kind. One with the same type and kind is
/// replaced: the version, the period and the lifespan are the newest
/// publisher's to say. A reader sees the one before or this one, whole.
pub(crate) fn write(
    directory: &Path,
    topic: &str,
    declaration: &Declaration,
    boot: &str,
    directory_mode: u32,
) -> io::Result<()> {
    declaration.check()?;
    let path = directory.join("declaration");
    // No topic's directory begins with a dot, and no two publishers have
    // one `boot`.
    let writing = directory.join(format!(".declaration.{boot}.new"));
    let mut line = serde_json::to_vec(declaration)?;
    line.push(b'\n');
    let put = || -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(&writing)?;
        file.write_all(&line)?;
        // The mode asked for, whatever the process's umask took from it.
        file.set_permissions(fs::Permissions::from_mode(FILE_MODE))
    };
    for _ in 0..TRIES {
        // Whether this would be the topic's first declaration.
        let first = match read(&path) {
            Ok(None) => true,
            Ok(Some(there)) if there == *declaration => return Ok(()),
            Ok(Some(there))
                if there.type_name != declaration.type_name || there.kind != declaration.kind =>
            {
                return Err(invalid(format!(
                    "topic {topic} is already declared as {} ({}) and this publisher says {} \
                     ({}): two programs disagree about the topic. If the first no longer \
                     runs, remove {}",
                    there.type_name,
                    there.kind_word(),
                    declaration.type_name,
                    declaration.kind_word(),
                    path.display()
                )));
            }
            // The same type and kind: the rest is the newest publisher's.
            Ok(Some(_)) => false,
            // A file that cannot be read as a declaration says nothing, and
            // nothing can disagree with it: it is replaced.
            Err(error) if error.kind() == io::ErrorKind::InvalidData => false,
            Err(error) => return Err(error),
        };
        match put() {
            // The first of a topic nobody has subscribed to.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                crate::make_directory(directory, directory_mode)?;
                put()?;
            }
            done => done?,
        }
        if !first {
            return fs::rename(&writing, &path);
        }
        // The first declaration of a topic is put there by a link, which
        // fails where a rename would replace: of two publishers that start
        // at once and disagree, one is told.
        let linked = fs::hard_link(&writing, &path);
        let _ = fs::remove_file(&writing);
        match linked {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(format!(
        "{} kept changing while it was being declared",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaration_is_one_line_of_json_and_says_only_what_is_promised() {
        let bare = Declaration::new("lamp_msgs/Speech", 1, Kind::Event);
        assert_eq!(
            serde_json::to_string(&bare).unwrap(),
            "{\"type\":\"lamp_msgs/Speech\",\"version\":1,\"kind\":\"event\"}"
        );
        let full = Declaration::new("lamp_msgs/JointState", 2, Kind::Latest)
            .every(Duration::from_millis(50))
            .lasting(Duration::from_millis(500));
        let text = serde_json::to_string(&full).unwrap();
        assert_eq!(
            text,
            "{\"type\":\"lamp_msgs/JointState\",\"version\":2,\"kind\":\"latest\",\
             \"period_ms\":50,\"lifespan_ms\":500}"
        );
        assert_eq!(serde_json::from_str::<Declaration>(&text).unwrap(), full);
        assert_eq!(full.period(), Some(Duration::from_millis(50)));
        assert_eq!(full.lifespan(), Some(Duration::from_millis(500)));
        assert_eq!((bare.period(), bare.lifespan()), (None, None));
        // A field a later version adds is passed over.
        let later = "{\"type\":\"lamp_msgs/Speech\",\"version\":3,\"kind\":\"event\",\"depth\":5}";
        assert_eq!(
            serde_json::from_str::<Declaration>(later).unwrap().version,
            3
        );
    }

    #[test]
    fn a_declaration_needs_a_type_name_and_a_period_that_promises_something() {
        assert!(
            Declaration::new("lamp_msgs/Speech", 1, Kind::Event)
                .check()
                .is_ok()
        );
        for name in ["", "Speech", "lamp_msgs/speech", "lamp msgs/Speech"] {
            let error = Declaration::new(name, 1, Kind::Event).check().unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{name:?}");
        }
        let never = Declaration::new("lamp_msgs/Speech", 1, Kind::Event).every(Duration::ZERO);
        assert!(never.check().unwrap_err().to_string().contains("period"));
    }
}
