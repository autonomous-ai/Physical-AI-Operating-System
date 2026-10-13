//! The receiving side: a datagram socket in the topic's `subs/`, and the
//! count of what did not reach it.
//!
//! Nothing promises a subscriber every message. What it is promised is that
//! it can tell: each message says how many of its publisher's came before it
//! and were not received here, and for a topic whose values replace each
//! other the newest is always in the `latest` file, which a subscriber with
//! nothing waiting looks at.
use crate::{
    Bus, FILE_MODE, MOST_SOCKET_PATH_BYTES, name,
    wire::{self, Message},
};
use std::{
    fs, io,
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// Publishers whose count a subscriber keeps. With more on one topic, the
/// one first heard is forgotten, and its next message counts as its first.
const MOST_PUBLISHERS: usize = 32;

/// A path taken by another socket is passed over this many times.
const TRIES: usize = 8;

/// The last message had from one publisher.
struct Heard {
    source: String,
    boot: String,
    seq: u64,
    /// The instance heard under this name before the present one.
    before: Option<String>,
}

pub struct Subscriber {
    topic: String,
    path: PathBuf,
    latest: PathBuf,
    socket: UnixDatagram,
    buffer: Vec<u8>,
    /// What the socket has been told to wait: nothing yet, or this long.
    waits: Option<Duration>,
    heard: Vec<Heard>,
    looked: Option<Instant>,
    look_every: Duration,
    malformed: u64,
    repeated: u64,
}

impl Subscriber {
    pub(crate) fn new(bus: &Bus, topic: &str, subscriber: &str) -> io::Result<Self> {
        static BOUND: AtomicU64 = AtomicU64::new(0);
        name::topic(topic)?;
        name::subscriber(subscriber)?;
        let directory = bus.root.join(topic);
        let subs = directory.join("subs");
        let mut made = false;
        for _ in 0..TRIES {
            let stem = format!(
                "{subscriber}.{}.{}",
                process::id(),
                BOUND.fetch_add(1, Ordering::Relaxed)
            );
            let path = subs.join(format!("{stem}.sock"));
            let length = path.as_os_str().len();
            if length > MOST_SOCKET_PATH_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "socket path {} is {length} bytes and a Unix socket's address holds \
                         {MOST_SOCKET_PATH_BYTES}: use a shorter root, topic or subscriber name",
                        path.display()
                    ),
                ));
            }
            if !made {
                crate::make_directory(&subs, bus.directory_mode)?;
                made = true;
            }
            // A socket's file can be seen a moment before the socket can be
            // sent to, and a publisher that looks in that moment finds it
            // refused and removes it as a dead subscriber's: 61 of 400
            // subscribers lost theirs that way to a publisher that looked
            // with every message (macOS, 2026-10-13). So the socket is bound
            // under a name publishers pass over, and is given its own only
            // once it is whole. By a link and not a rename, which would
            // replace whatever had that name.
            let birth = subs.join(format!("{stem}.new"));
            let socket = match UnixDatagram::bind(&birth) {
                Ok(socket) => socket,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
                Err(error) => {
                    return Err(io::Error::new(
                        error.kind(),
                        format!("bind {}: {error}", birth.display()),
                    ));
                }
            };
            let named = fs::set_permissions(&birth, fs::Permissions::from_mode(FILE_MODE))
                .and_then(|()| fs::hard_link(&birth, &path));
            let _ = fs::remove_file(&birth);
            match named {
                Ok(()) => {}
                // Left by a process that had this number before. A
                // publisher will remove it; this one takes the next name.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(io::Error::new(
                        error.kind(),
                        format!("name {}: {error}", path.display()),
                    ));
                }
            }
            return Ok(Self {
                topic: topic.into(),
                path,
                latest: directory.join("latest"),
                socket,
                buffer: vec![0; wire::MOST_BYTES + 1],
                waits: None,
                heard: Vec::new(),
                looked: None,
                look_every: bus.rescan_every,
                malformed: 0,
                repeated: 0,
            });
        }
        Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("no free socket name for {subscriber} in {}", subs.display()),
        ))
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Datagrams that arrived and were not a message of this topic.
    pub fn malformed(&self) -> u64 {
        self.malformed
    }

    /// Messages that arrived after a newer one from the same publisher had
    /// already been returned, and were passed over.
    pub fn repeated(&self) -> u64 {
        self.repeated
    }

    /// The next message, waiting at most `timeout` for one. What is not a
    /// message of this topic is counted and passed over.
    ///
    /// With nothing waiting, a topic's `latest` file is looked at, at most
    /// once in the time a publisher takes to notice a new subscriber, and
    /// returned if it is newer than what this subscriber has had. That is
    /// what makes the last value of a topic safe: the message that fell on a
    /// full queue, or was sent in the moment before the publisher knew of
    /// this subscriber, has no later one behind it to show the gap.
    pub fn recv(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        if let Some(message) = self.queued(timeout)? {
            return Ok(Some(message));
        }
        match self.kept_news() {
            Some(kept) => self.queued_or(kept).map(Some),
            None => Ok(None),
        }
    }

    /// A publisher sends a value to the sockets before it keeps it, so by
    /// the time the file holds one, whatever was delivered of it and of
    /// those before it is in the queue. Those come first, in their order:
    /// the file is ahead only of a queue that is empty, and then nothing
    /// that was delivered is passed over for it.
    fn queued_or(&mut self, (header, payload): (wire::Header, Vec<u8>)) -> io::Result<Message> {
        Ok(match self.queued(Duration::ZERO)? {
            Some(message) => message,
            None => self.account(header, payload),
        })
    }

    /// The next message from the socket, waiting at most `timeout` for it.
    fn queued(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        let started = Instant::now();
        let mut left = timeout;
        loop {
            self.wait(left)?;
            match self.socket.recv(&mut self.buffer) {
                Ok(length) => {
                    let arrived = wire::split(&self.topic, &self.buffer[..length])
                        .map(|(header, payload)| (header, payload.to_vec()));
                    match arrived {
                        Ok((header, payload)) if self.news(&header) => {
                            return Ok(Some(self.account(header, payload)));
                        }
                        Ok(_) => self.repeated += 1,
                        Err(_) => self.malformed += 1,
                    }
                    // What was passed over does not make the wait longer
                    // than was asked for: once the time is up, only what is
                    // already here is read.
                    left = timeout.saturating_sub(started.elapsed());
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    left = timeout.saturating_sub(started.elapsed());
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// The topic's current value, if it keeps one. It counts as received:
    /// `recv` does not return the same message again.
    pub fn latest(&mut self) -> io::Result<Option<Message>> {
        self.looked = Some(Instant::now());
        Ok(
            wire::kept(&self.latest, &self.topic)?.map(|(header, payload)| {
                if self.kept_is_news(&header) {
                    self.account(header, payload)
                } else {
                    Message {
                        header,
                        payload,
                        gap: 0,
                        publisher_restarted: false,
                    }
                }
            }),
        )
    }

    /// A read timeout of zero means "for ever" to a socket, and a short one
    /// is rounded up to the kernel's tick, so not waiting at all is asked
    /// for differently. Nothing is asked again while the wait stays the same.
    fn wait(&mut self, left: Duration) -> io::Result<()> {
        if self.waits == Some(left) {
            return Ok(());
        }
        if left.is_zero() {
            self.socket.set_nonblocking(true)?;
        } else {
            if self.waits.is_some_and(|waited| waited.is_zero()) {
                self.socket.set_nonblocking(false)?;
            }
            self.socket.set_read_timeout(Some(left))?;
        }
        self.waits = Some(left);
        Ok(())
    }

    /// What the topic keeps, if this subscriber has not had it and has not
    /// looked a moment ago.
    fn kept_news(&mut self) -> Option<(wire::Header, Vec<u8>)> {
        let now = Instant::now();
        if self
            .looked
            .is_some_and(|at| now.saturating_duration_since(at) < self.look_every)
        {
            return None;
        }
        self.looked = Some(now);
        // A file that cannot be read is no reason to fail a wait: `latest`
        // says what is wrong with it when asked.
        wire::kept(&self.latest, &self.topic)
            .ok()?
            .filter(|(header, _)| self.kept_is_news(header))
    }

    /// Whether the kept value is news. The file is replaced a moment after
    /// the datagrams are sent, so just after a publisher began again it can
    /// still hold the last value of the instance before, which this
    /// subscriber has already heard the successor of.
    fn kept_is_news(&self, header: &wire::Header) -> bool {
        self.news(header)
            && !self.heard.iter().any(|heard| {
                heard.source == header.source && heard.before.as_deref() == Some(&header.boot)
            })
    }

    /// Whether this is later than everything had from its publisher.
    fn news(&self, header: &wire::Header) -> bool {
        self.heard
            .iter()
            .find(|heard| heard.source == header.source)
            .is_none_or(|heard| heard.boot != header.boot || header.seq > heard.seq)
    }

    /// Note a message that is news, and say how it stands to the last one
    /// from its publisher.
    fn account(&mut self, header: wire::Header, payload: Vec<u8>) -> Message {
        let known = self
            .heard
            .iter_mut()
            .find(|heard| heard.source == header.source);
        let (gap, publisher_restarted) = match known {
            Some(heard) if heard.boot == header.boot => {
                let gap = header.seq.saturating_sub(heard.seq + 1);
                heard.seq = header.seq;
                (gap, false)
            }
            // Another instance under the same name: its count began again,
            // so nothing can be said of what was missed.
            Some(heard) => {
                let before = std::mem::replace(&mut heard.boot, header.boot.clone());
                heard.before = Some(before);
                heard.seq = header.seq;
                (0, true)
            }
            None => {
                if self.heard.len() == MOST_PUBLISHERS {
                    self.heard.remove(0);
                }
                self.heard.push(Heard {
                    source: header.source.clone(),
                    boot: header.boot.clone(),
                    seq: header.seq,
                    before: None,
                });
                (0, false)
            }
        };
        Message {
            header,
            payload,
            gap,
            publisher_restarted,
        }
    }
}

impl Drop for Subscriber {
    /// Publishers stop sending when the file has gone. A subscriber that
    /// dies without getting here leaves it, and the next publish that finds
    /// nobody behind it removes it.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kind;

    #[test]
    fn the_kept_value_is_not_taken_ahead_of_messages_still_in_the_queue() {
        let root = std::env::temp_dir().join(format!("lbs{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let bus = Bus::new(&root).unwrap().rescan_every(Duration::ZERO);
        let mut subscriber = bus.subscribe("servos", "follow").unwrap();
        let mut publisher = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
        for yaw in 1..=3u64 {
            assert_eq!(publisher.publish_json(yaw, &yaw).unwrap().delivered, 1);
        }
        // A wait that ended a moment before the three arrived finds the
        // third in the file, and all three in the queue behind it.
        let kept = subscriber.kept_news().expect("the third, in the file");
        assert_eq!(kept.0.seq, 3);
        let message = subscriber.queued_or(kept).unwrap();
        assert_eq!((message.header.seq, message.gap), (1, 0));
        for seq in 2..=3 {
            let message = subscriber.recv(Duration::from_secs(5)).unwrap().unwrap();
            assert_eq!((message.header.seq, message.gap), (seq, 0));
        }
        assert!(subscriber.recv(Duration::ZERO).unwrap().is_none());
        assert_eq!(subscriber.repeated(), 0);

        // With the queue empty the file is all there is. This publisher
        // looks once, before the other subscriber is there, and its second
        // value is sent to the first subscriber only.
        drop(publisher);
        let seldom = bus.clone().rescan_every(Duration::from_secs(3_600));
        let mut publisher = seldom.publisher("servos", Kind::Latest, "eyes").unwrap();
        assert_eq!(publisher.publish_json(4, &4).unwrap().delivered, 1);
        let mut other = bus.subscribe("servos", "other").unwrap();
        assert_eq!(publisher.publish_json(5, &5).unwrap().delivered, 1);
        let kept = other.kept_news().expect("the value, in the file");
        let message = other.queued_or(kept).unwrap();
        assert_eq!((message.header.seq, message.gap), (2, 0));
        assert_eq!(message.parse::<u64>().unwrap(), 5);
        assert!(other.recv(Duration::ZERO).unwrap().is_none());
        drop((subscriber, other));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_last_value_of_an_instance_that_began_again_is_not_news() {
        let root = std::env::temp_dir().join(format!("lbr{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let bus = Bus::new(&root).unwrap().rescan_every(Duration::ZERO);
        let wait = Duration::from_secs(5);
        let mut subscriber = bus.subscribe("servos", "follow").unwrap();
        let mut first = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
        for yaw in 1..=3u64 {
            first.publish_json(yaw, &yaw).unwrap();
            assert_eq!(subscriber.recv(wait).unwrap().unwrap().header.seq, yaw);
        }
        let kept = root.join("servos/latest");
        let old = fs::read(&kept).unwrap();
        drop(first);
        let mut second = bus.publisher("servos", Kind::Latest, "lamp-body").unwrap();
        second.publish_json(9, &9).unwrap();
        let message = subscriber.recv(wait).unwrap().unwrap();
        assert_eq!((message.header.seq, message.publisher_restarted), (1, true));
        // As the file is in the moment between the new instance's first
        // datagram and its first value kept: still the old instance's.
        fs::write(&kept, &old).unwrap();
        assert!(subscriber.recv(Duration::ZERO).unwrap().is_none());
        // Asked for, the file is what it is, and it changes nothing of what
        // has been heard.
        let asked = subscriber.latest().unwrap().unwrap();
        assert_eq!((asked.header.seq, asked.publisher_restarted), (3, false));
        assert_eq!(asked.parse::<u64>().unwrap(), 3);
        second.publish_json(10, &10).unwrap();
        let message = subscriber.recv(wait).unwrap().unwrap();
        assert_eq!((message.header.seq, message.gap), (2, 0));
        assert!(!message.publisher_restarted);
        assert!(subscriber.recv(Duration::ZERO).unwrap().is_none());
        assert_eq!(subscriber.repeated(), 0);
        drop(subscriber);
        fs::remove_dir_all(&root).unwrap();
    }
}
