//! What a topic, a subscriber, a publisher and an encoding may be called.
//!
//! A topic's name becomes a path under the root and a subscriber's name part
//! of a socket's file name, so both are checked here, as text, before
//! anything is asked of the filesystem.
use std::io;

/// The most a whole topic name may be.
pub const MOST_TOPIC_BYTES: usize = 96;
const MOST_SEGMENT_BYTES: usize = 32;
const MOST_ENCODING_BYTES: usize = 64;

/// The two names a topic's own directory uses. A topic `a/subs` would be the
/// directory where topic `a` keeps its subscribers.
const KEPT: [&str; 2] = ["subs", "latest"];

fn refused(what: &str, name: &str, rule: &str) -> io::Error {
    // A name can be any length: say enough of it to find it.
    let shown: String = name.chars().take(48).collect();
    let more = if shown.len() < name.len() { "..." } else { "" };
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{what} {shown:?}{more} is not {rule}"),
    )
}

fn small(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

/// `[a-z0-9][a-z0-9_-]{0,31}`
fn segment(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=MOST_SEGMENT_BYTES).contains(&bytes.len())
        && small(bytes[0])
        && bytes
            .iter()
            .all(|&byte| small(byte) || matches!(byte, b'_' | b'-'))
}

/// A topic: segments joined by `/`, which nests directories, or by `.`,
/// which stays in the directory's name. Starting every segment with a letter
/// or digit is what keeps `..`, an empty segment and an absolute path out.
pub fn topic(name: &str) -> io::Result<()> {
    const RULE: &str = "a topic: segments of [a-z0-9][a-z0-9_-]{0,31} joined by / or ., \
                        96 bytes at most, no directory called subs or latest";
    let fits = name.len() <= MOST_TOPIC_BYTES;
    let shaped = name
        .split('/')
        .all(|directory| !KEPT.contains(&directory) && directory.split('.').all(segment));
    if fits && shaped {
        Ok(())
    } else {
        Err(refused("topic", name, RULE))
    }
}

/// `[a-z0-9_-]{1,32}`: a subscriber's name, or a publisher's.
fn plain(what: &str, name: &str) -> io::Result<()> {
    let bytes = name.as_bytes();
    if (1..=MOST_SEGMENT_BYTES).contains(&bytes.len())
        && bytes
            .iter()
            .all(|&byte| small(byte) || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(refused(what, name, "[a-z0-9_-]{1,32}"))
    }
}

pub fn subscriber(name: &str) -> io::Result<()> {
    plain("subscriber name", name)
}

pub fn source(name: &str) -> io::Result<()> {
    plain("publisher name", name)
}

/// How to read a payload: `json`, `none`, or a label for media such as
/// `pcm_s16le_16000_mono`. It travels inside the header's JSON and is
/// printed by tools, so it is kept to characters that need no escaping.
pub fn encoding(label: &str) -> io::Result<()> {
    let bytes = label.as_bytes();
    if (1..=MOST_ENCODING_BYTES).contains(&bytes.len())
        && small(bytes[0])
        && bytes
            .iter()
            .all(|&byte| small(byte) || matches!(byte, b'_' | b'-' | b'.' | b'+' | b'/'))
    {
        Ok(())
    } else {
        Err(refused("encoding", label, "[a-z0-9][a-z0-9_.+/-]{0,63}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_topic_is_segments_joined_by_a_slash_or_a_dot() {
        for name in [
            "mic2",
            "servos",
            "gemini/transcript",
            "room.state",
            "a",
            "0",
            "a-b_c/d.e/f",
            "subs.x",
            "latest-value",
            "x/subs.y",
        ] {
            assert!(topic(name).is_ok(), "{name}");
        }
        let longest = [["a"; 32].concat(), ["b"; 32].concat(), ["c"; 30].concat()].join("/");
        assert_eq!(longest.len(), 96);
        assert!(topic(&longest).is_ok());
    }

    #[test]
    fn a_topic_that_could_leave_the_root_or_name_nothing_is_refused() {
        for name in [
            "",
            "/",
            "/mic2",
            "mic2/",
            "../x",
            "..",
            ".",
            "a/../b",
            "a//b",
            "a..b",
            ".a",
            "a.",
            "A",
            "mic 2",
            "mic2\n",
            "-a",
            "_a",
            "a/-b",
            "caf\u{e9}",
            "a\0b",
            "a\\b",
        ] {
            let error = topic(name).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{name:?}");
        }
        assert!(topic(&["a"; 33].concat()).is_err());
        let long = [["a"; 32].concat(), ["b"; 32].concat(), ["c"; 31].concat()].join("/");
        assert_eq!(long.len(), 97);
        assert!(topic(&long).is_err());
        // The error names the topic, and only the start of a long one.
        let text = topic(&"x".repeat(5_000)).unwrap_err().to_string();
        assert!(text.contains("xxxx") && text.len() < 300, "{text}");
    }

    #[test]
    fn a_topic_cannot_be_another_topics_own_directory_or_file() {
        for name in ["subs", "latest", "a/subs", "a/latest", "subs/a", "a/subs/b"] {
            assert!(topic(name).is_err(), "{name}");
        }
    }

    #[test]
    fn a_subscriber_and_a_publisher_have_one_plain_word_for_a_name() {
        for name in ["follow", "lamp-body", "a", "_", "-", "0_o", &"n".repeat(32)] {
            assert!(subscriber(name).is_ok(), "{name}");
            assert!(source(name).is_ok(), "{name}");
        }
        for name in ["", "a.b", "a/b", "A", "a b", "..", &"n".repeat(33)] {
            assert!(subscriber(name).is_err(), "{name:?}");
            assert!(source(name).is_err(), "{name:?}");
        }
        assert!(
            subscriber("a.b")
                .unwrap_err()
                .to_string()
                .starts_with("subscriber name \"a.b\"")
        );
        assert!(
            source("A")
                .unwrap_err()
                .to_string()
                .starts_with("publisher name \"A\"")
        );
    }

    #[test]
    fn an_encoding_is_a_label_that_needs_no_escaping() {
        for label in [
            "json",
            "none",
            "pcm_s16le_16000_mono",
            "jpeg",
            "image/jpeg",
            "x.y+z-1",
        ] {
            assert!(encoding(label).is_ok(), "{label}");
        }
        for label in ["", "JSON", "a b", "a\"b", "_a", &"e".repeat(65)] {
            assert!(encoding(label).is_err(), "{label:?}");
        }
    }
}
