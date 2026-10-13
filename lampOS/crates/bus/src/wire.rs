//! One message as it travels: a line of JSON saying what it is, a line end,
//! then the payload's bytes as they are.
//!
//! The header is text so that anything can read it: a shell, a script, a
//! person with `lamp-bus tap`. The payload is left alone so that sound costs
//! nothing to carry. The whole of it is one datagram, and the same bytes are
//! what a topic's `latest` file holds.
use crate::name;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::{self, Read},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// The most one message may be, header and payload together. A datagram is
/// taken whole or not at all, and this is under what a Linux datagram
/// socket takes with the sizes it is given by default.
pub const MOST_BYTES: usize = 60_000;

/// The payload is one JSON value.
pub const JSON: &str = "json";
/// There is no payload: the header is the message.
pub const NONE: &str = "none";

/// How far past its time a message is still handed over: two processes read
/// the same clock a moment apart, and a value that lasts 20 ms is not late
/// because the reader was scheduled 21 ms after the writer.
pub const EXPIRY_GRACE_MS: u64 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A value that replaces the one before it: where the servos are, who is
    /// in the room. The newest is also kept in the topic's `latest` file.
    Latest,
    /// Something that happened and is over: a piece of sound, a word heard.
    /// Nothing is kept: whoever was not listening does not get it.
    Event,
}

/// What every message says of itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub topic: String,
    pub kind: Kind,
    /// How to read the payload: [`JSON`], [`NONE`] or a media label.
    pub enc: String,
    /// What the payload is, as the publisher declared it for the topic
    /// (`lamp_msgs/JointState`). A publisher that declared nothing, and a
    /// recording made before there were declarations, say nothing here.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    /// The publisher's name.
    pub source: String,
    /// Sixteen hex characters picked when the publisher was made. `seq`
    /// counts within one of these.
    pub boot: String,
    /// From 1, one more with every message this publisher sent on the topic.
    pub seq: u64,
    /// When, by the publisher's monotonic clock, in microseconds.
    pub at_us: u64,
    /// When, by the publisher's wall clock, in milliseconds since 1970.
    pub at_unix_ms: u64,
    /// How long after `at_unix_ms` the value still means something. A
    /// subscriber is not handed a message older than that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_ms: Option<u64>,
}

impl Header {
    /// Whether the value had outlived `expires_ms` at `now_unix_ms`, by more
    /// than [`EXPIRY_GRACE_MS`]. A message that names no limit never has.
    pub fn expired_at(&self, now_unix_ms: u64) -> bool {
        self.expires_ms.is_some_and(|lasting| {
            now_unix_ms
                > self
                    .at_unix_ms
                    .saturating_add(lasting)
                    .saturating_add(EXPIRY_GRACE_MS)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub header: Header,
    pub payload: Vec<u8>,
    /// How many messages from this same publisher instance were sent
    /// between the last one this subscriber got and this one.
    pub gap: u64,
    /// The publisher of this name is not the instance heard before: it
    /// began again, and its count with it.
    pub publisher_restarted: bool,
    /// The value had outlived its `expires_ms` when it was read. Only a
    /// topic's kept value is ever returned so (`Bus::latest`,
    /// `Subscriber::latest`): it is the last thing known, and the caller is
    /// told that it is old. `Subscriber::recv` passes an expired message
    /// over and counts it.
    pub expired: bool,
}

impl Message {
    /// The payload as JSON.
    pub fn json(&self) -> io::Result<serde_json::Value> {
        self.parse()
    }

    /// The payload as a `T`, when it is JSON.
    pub fn parse<T: DeserializeOwned>(&self) -> io::Result<T> {
        if self.header.enc != JSON {
            return Err(invalid(format!(
                "the payload is {}, not json",
                self.header.enc
            )));
        }
        serde_json::from_slice(&self.payload).map_err(io::Error::from)
    }
}

fn invalid(why: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

/// The wall clock, in milliseconds since 1970.
pub(crate) fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// The header and the payload of what arrived on `topic`, or why it is not
/// a message of that topic.
pub(crate) fn split<'a>(topic: &str, datagram: &'a [u8]) -> io::Result<(Header, &'a [u8])> {
    if datagram.len() > MOST_BYTES {
        return Err(invalid(format!("more than {MOST_BYTES} bytes")));
    }
    let Some(end) = datagram.iter().position(|&byte| byte == b'\n') else {
        return Err(invalid("no line end after the header".into()));
    };
    let header: Header = serde_json::from_slice(&datagram[..end])
        .map_err(|error| invalid(format!("header: {error}")))?;
    if header.topic != topic {
        return Err(invalid(format!("a message of topic {:?}", header.topic)));
    }
    let boot = header.boot.as_bytes();
    if boot.len() != 16
        || !boot
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(invalid("boot is not 16 hex characters".into()));
    }
    if header.seq == 0 {
        return Err(invalid("seq is 0".into()));
    }
    name::source(&header.source)
        .and_then(|()| name::encoding(&header.enc))
        .and_then(|()| header.type_name.as_deref().map_or(Ok(()), name::type_name))
        .map_err(|error| invalid(error.to_string()))?;
    Ok((header, &datagram[end + 1..]))
}

/// What a topic's `latest` file holds, if there is one.
pub(crate) fn kept(path: &Path, topic: &str) -> io::Result<Option<(Header, Vec<u8>)>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    // A file that is not one of ours could be any size.
    let mut bytes = Vec::new();
    file.take(MOST_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    let (header, payload) =
        split(topic, &bytes).map_err(|error| invalid(format!("{}: {error}", path.display())))?;
    Ok(Some((header, payload.to_vec())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            topic: "servos".into(),
            kind: Kind::Latest,
            enc: JSON.into(),
            type_name: None,
            source: "body".into(),
            boot: "9f3a6c1e04b7d25a".into(),
            seq: 412,
            at_us: 98_123_456,
            at_unix_ms: 1_791_878_400_123,
            expires_ms: Some(500),
        }
    }

    fn datagram(header: &Header, payload: &[u8]) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(header).unwrap();
        bytes.push(b'\n');
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn the_header_is_one_line_of_json_with_its_fields_in_a_fixed_order() {
        let mut header = header();
        assert_eq!(
            serde_json::to_string(&header).unwrap(),
            "{\"topic\":\"servos\",\"kind\":\"latest\",\"enc\":\"json\",\"source\":\"body\",\
             \"boot\":\"9f3a6c1e04b7d25a\",\"seq\":412,\"at_us\":98123456,\
             \"at_unix_ms\":1791878400123,\"expires_ms\":500}"
        );
        // A value that does not expire says nothing about it.
        header.expires_ms = None;
        header.kind = Kind::Event;
        let text = serde_json::to_string(&header).unwrap();
        assert!(text.contains("\"kind\":\"event\"") && !text.contains("expires_ms"));
        assert_eq!(serde_json::from_str::<Header>(&text).unwrap(), header);
        // A publisher that declared the topic says what the payload is,
        // after how to read it. One that did not says nothing: a header
        // written before there were declarations reads as it always did.
        assert!(!text.contains("type"));
        header.type_name = Some("lamp_msgs/JointState".into());
        let text = serde_json::to_string(&header).unwrap();
        assert!(
            text.contains("\"enc\":\"json\",\"type\":\"lamp_msgs/JointState\",\"source\""),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<Header>(&text).unwrap(), header);
    }

    #[test]
    fn a_message_has_expired_fifty_milliseconds_after_its_time_and_never_without_one() {
        let mut header = header();
        // Sent at ...123, lasting 500: good to ...623, handed over to ...673.
        let sent = header.at_unix_ms;
        assert!(!header.expired_at(0));
        assert!(!header.expired_at(sent));
        assert!(!header.expired_at(sent + 500));
        assert!(!header.expired_at(sent + 500 + EXPIRY_GRACE_MS));
        assert!(header.expired_at(sent + 500 + EXPIRY_GRACE_MS + 1));
        assert!(header.expired_at(u64::MAX));
        header.expires_ms = Some(0);
        assert!(!header.expired_at(sent + EXPIRY_GRACE_MS));
        assert!(header.expired_at(sent + EXPIRY_GRACE_MS + 1));
        // A limit too large to add is no limit.
        header.expires_ms = Some(u64::MAX);
        assert!(!header.expired_at(u64::MAX));
        header.expires_ms = None;
        assert!(!header.expired_at(u64::MAX));
    }

    #[test]
    fn a_datagram_splits_at_the_first_line_end_and_the_payload_is_untouched() {
        let header = header();
        // The payload has line ends, a NUL and bytes that are not text.
        let payload = [b'\n', 0, 0xff, b'\n', 7];
        let bytes = datagram(&header, &payload);
        let (got, rest) = split("servos", &bytes).unwrap();
        assert_eq!(got, header);
        assert_eq!(rest, payload);
        // No payload at all is a message too.
        let bare = datagram(&header, b"");
        assert!(split("servos", &bare).unwrap().1.is_empty());
        // A field this version does not know is passed over.
        let extended =
            b"{\"topic\":\"servos\",\"kind\":\"event\",\"enc\":\"none\",\"source\":\"x\",\
            \"boot\":\"0123456789abcdef\",\"seq\":1,\"at_us\":0,\"at_unix_ms\":0,\"later\":[1]}\n";
        assert_eq!(split("servos", extended).unwrap().0.seq, 1);
    }

    #[test]
    fn what_is_not_a_message_of_the_topic_is_refused_with_the_reason() {
        let good = header();
        let reason = |bytes: &[u8]| split("servos", bytes).unwrap_err().to_string();
        assert!(reason(b"").contains("no line end"));
        assert!(reason(b"hello").contains("no line end"));
        assert!(reason(&serde_json::to_vec(&good).unwrap()).contains("no line end"));
        assert!(reason(b"hello\n").contains("header"));
        assert!(reason(b"{}\n").contains("header"));
        assert!(reason(b"\xff\xfe\n").contains("header"));
        let changed = |change: fn(&mut Header)| {
            let mut header = header();
            change(&mut header);
            reason(&datagram(&header, b"{}"))
        };
        assert!(changed(|header| header.topic = "mic2".into()).contains("topic \"mic2\""));
        assert!(changed(|header| header.seq = 0).contains("seq is 0"));
        assert!(changed(|header| header.boot = "9F3A6C1E04B7D25A".into()).contains("boot"));
        assert!(changed(|header| header.boot = "9f3a".into()).contains("boot"));
        assert!(changed(|header| header.source = "Lamp Body".into()).contains("publisher name"));
        assert!(changed(|header| header.enc = String::new()).contains("encoding"));
        assert!(changed(|header| header.type_name = Some("joint state".into())).contains("type "));
        let mut large = datagram(&good, b"");
        large.resize(MOST_BYTES + 1, b' ');
        assert!(reason(&large).contains("more than 60000 bytes"));
        large.truncate(MOST_BYTES);
        assert!(split("servos", &large).is_ok());
    }

    #[test]
    fn a_json_payload_reads_as_a_value_or_as_a_type_and_other_payloads_do_not() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Pose {
            pitch: u16,
            yaw: u16,
        }
        let mut message = Message {
            header: header(),
            payload: b"{\"pitch\":2048,\"yaw\":1990}".to_vec(),
            gap: 0,
            publisher_restarted: false,
            expired: false,
        };
        assert_eq!(message.json().unwrap()["yaw"], 1990);
        assert_eq!(
            message.parse::<Pose>().unwrap(),
            Pose {
                pitch: 2048,
                yaw: 1990
            }
        );
        message.payload = b"{\"pitch\":".to_vec();
        assert!(message.json().is_err());
        message.header.enc = "pcm_s16le_16000_mono".into();
        message.payload = b"{}".to_vec();
        let error = message.json().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("pcm_s16le_16000_mono, not json"));
    }
}
