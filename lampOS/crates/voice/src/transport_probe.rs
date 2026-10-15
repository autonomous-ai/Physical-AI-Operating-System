//! Explicit observations of successful fixture transport operations only.
//! Publication and reads are bounded and nonblocking; neither performs I/O or
//! samples time. A snapshot describes transport acceptance, never handled work.
use lamp_turn_owner::BootId;
use std::{
    fmt, io,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};

#[derive(Debug)]
struct Identity {
    name: String,
    sender: BootId,
    peer: BootId,
}

/// Keeps the identity allocation alive even after its channel and probe drop.
#[derive(Clone, Debug)]
pub(crate) struct ChannelId(Arc<Identity>);

impl ChannelId {
    pub(crate) fn name(&self) -> &str {
        &self.0.name
    }
}

impl PartialEq for ChannelId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ChannelId {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    Send,
    Receive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Gap {
    pub(crate) expected: u64,
    pub(crate) observed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Progress {
    pub(crate) direction: Direction,
    /// The configured boot; every counted operation actually matched it.
    pub(crate) boot: BootId,
    pub(crate) count: u64,
    pub(crate) last_sequence: Option<u64>,
    /// Proven consecutive successes starting at sequence 1, not a high-water mark.
    pub(crate) contiguous_through: u64,
    pub(crate) first_gap: Option<Gap>,
}

impl Progress {
    fn empty(direction: Direction, boot: BootId) -> Self {
        Self {
            direction,
            boot,
            count: 0,
            last_sequence: None,
            contiguous_through: 0,
            first_gap: None,
        }
    }

    fn validate(&self) -> Result<(), Fault> {
        let Some(last) = self.last_sequence else {
            return if self.count == 0 && self.contiguous_through == 0 && self.first_gap.is_none() {
                Ok(())
            } else {
                Err(Fault::InvalidState)
            };
        };
        if self.count == 0 || self.count > last || self.contiguous_through > self.count {
            return Err(Fault::InvalidState);
        }
        match self.first_gap {
            None if self.count == last && self.contiguous_through == last => Ok(()),
            Some(gap)
                if self.contiguous_through.checked_add(1) == Some(gap.expected)
                    && gap.expected < gap.observed
                    && gap.observed <= last
                    && self.contiguous_through < self.count =>
            {
                Ok(())
            }
            _ => Err(Fault::InvalidState),
        }
    }

    fn next(self, boot: BootId, sequence: u64) -> Result<Self, Fault> {
        self.validate()?;
        if boot != self.boot {
            return Err(Fault::WrongBoot);
        }
        let count = self.count.checked_add(1).ok_or(Fault::CountOverflow)?;
        let previous = self.last_sequence.unwrap_or(0);
        if sequence <= previous {
            return Err(Fault::NonIncreasingSequence);
        }
        let mut next = Self {
            count,
            last_sequence: Some(sequence),
            ..self
        };
        if self.first_gap.is_none() {
            let expected = previous.checked_add(1).ok_or(Fault::InvalidState)?;
            if sequence == expected {
                next.contiguous_through = sequence;
            } else {
                next.first_gap = Some(Gap {
                    expected,
                    observed: sequence,
                });
            }
        }
        Ok(next)
    }
}

/// A historical value, not a lease on the probe's later health or progress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Snapshot {
    pub(crate) channel: ChannelId,
    pub(crate) revision: u64,
    pub(crate) send: Progress,
    pub(crate) receive: Progress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Fault {
    AlreadyInstalled = 1,
    InstalledAfterActivity = 2,
    WrongBoot = 3,
    NonIncreasingSequence = 4,
    CountOverflow = 5,
    RevisionOverflow = 6,
    InvalidState = 7,
}

impl Fault {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => None,
            1 => Some(Self::AlreadyInstalled),
            2 => Some(Self::InstalledAfterActivity),
            3 => Some(Self::WrongBoot),
            4 => Some(Self::NonIncreasingSequence),
            5 => Some(Self::CountOverflow),
            6 => Some(Self::RevisionOverflow),
            _ => Some(Self::InvalidState),
        }
    }
}

/// Busy supplies no receipt. It does not wait or assume publication completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotError {
    Busy,
    Failed(Fault),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => write!(f, "transport probe publication is in progress"),
            Self::Failed(fault) => write!(f, "transport probe failed: {fault:?}"),
        }
    }
}
impl std::error::Error for SnapshotError {}

#[derive(Default)]
struct Counters {
    count: AtomicU64,
    last_sequence: AtomicU64,
    contiguous_through: AtomicU64,
    gap_expected: AtomicU64,
    gap_observed: AtomicU64,
}

impl Counters {
    fn read(&self, direction: Direction, boot: BootId) -> Result<Progress, Fault> {
        let last = self.last_sequence.load(Ordering::SeqCst);
        let expected = self.gap_expected.load(Ordering::SeqCst);
        let observed = self.gap_observed.load(Ordering::SeqCst);
        if (expected == 0) != (observed == 0) {
            return Err(Fault::InvalidState);
        }
        Ok(Progress {
            direction,
            boot,
            count: self.count.load(Ordering::SeqCst),
            last_sequence: (last != 0).then_some(last),
            contiguous_through: self.contiguous_through.load(Ordering::SeqCst),
            first_gap: (expected != 0).then_some(Gap { expected, observed }),
        })
    }

    fn write(&self, progress: Progress) {
        self.count.store(progress.count, Ordering::SeqCst);
        self.last_sequence
            .store(progress.last_sequence.unwrap_or(0), Ordering::SeqCst);
        self.contiguous_through
            .store(progress.contiguous_through, Ordering::SeqCst);
        self.gap_expected.store(
            progress.first_gap.map_or(0, |gap| gap.expected),
            Ordering::SeqCst,
        );
        self.gap_observed.store(
            progress.first_gap.map_or(0, |gap| gap.observed),
            Ordering::SeqCst,
        );
    }
}

struct Shared {
    channel: ChannelId,
    // One Channel's &mut operations are the sole publisher. Even revisions
    // enclose complete snapshots, odd revisions enclose writes; checked growth
    // prevents ABA. SeqCst words let a reader reject any overlapping revision
    // in one attempt, without locks or retrying a publication in progress.
    revision: AtomicU64,
    failure: AtomicU8,
    send: Counters,
    receive: Counters,
}

impl Shared {
    fn failure(&self) -> Option<Fault> {
        Fault::from_code(self.failure.load(Ordering::SeqCst))
    }

    fn fail(&self, fault: Fault) {
        let _ = self
            .failure
            .compare_exchange(0, fault as u8, Ordering::SeqCst, Ordering::SeqCst);
    }

    fn begin_publication(&self, revision: u64) -> Result<u64, Fault> {
        if !revision.is_multiple_of(2) || self.revision.load(Ordering::SeqCst) != revision {
            return Err(Fault::InvalidState);
        }
        let next = revision.checked_add(2).ok_or(Fault::RevisionOverflow)?;
        self.revision.store(revision + 1, Ordering::SeqCst);
        Ok(next)
    }

    fn finish_publication(&self, revision: u64, send: Progress, receive: Progress) {
        self.send.write(send);
        self.receive.write(receive);
        self.revision.store(revision, Ordering::SeqCst);
    }
}

#[derive(Clone)]
pub(crate) struct Probe {
    shared: Arc<Shared>,
}

impl Probe {
    /// One bounded consistency attempt, with no retry, lock, I/O or clock read.
    pub(crate) fn snapshot(&self) -> Result<Snapshot, SnapshotError> {
        if let Some(fault) = self.shared.failure() {
            return Err(SnapshotError::Failed(fault));
        }
        let revision = self.shared.revision.load(Ordering::SeqCst);
        if !revision.is_multiple_of(2) {
            return Err(SnapshotError::Busy);
        }
        let identity = &self.shared.channel.0;
        let send = self.shared.send.read(Direction::Send, identity.sender);
        let receive = self.shared.receive.read(Direction::Receive, identity.peer);
        if let Some(fault) = self.shared.failure() {
            return Err(SnapshotError::Failed(fault));
        }
        if self.shared.revision.load(Ordering::SeqCst) != revision {
            return Err(SnapshotError::Busy);
        }
        // A partial read may look inconsistent while publication is active.
        // Only a stable revision permits treating that inconsistency as a fault.
        let (Ok(send), Ok(receive)) = (send, receive) else {
            self.shared.fail(Fault::InvalidState);
            return Err(SnapshotError::Failed(Fault::InvalidState));
        };
        let counted_revision = send
            .count
            .checked_add(receive.count)
            .and_then(|count| count.checked_mul(2));
        if send.validate().is_err()
            || receive.validate().is_err()
            || counted_revision != Some(revision)
        {
            self.shared.fail(Fault::InvalidState);
            return Err(SnapshotError::Failed(Fault::InvalidState));
        }
        Ok(Snapshot {
            channel: self.shared.channel.clone(),
            revision,
            send,
            receive,
        })
    }
}

struct Writer {
    shared: Arc<Shared>,
    revision: u64,
    send: Progress,
    receive: Progress,
}

impl Writer {
    fn record(&mut self, direction: Direction, boot: BootId, sequence: u64) {
        if self.shared.failure().is_some() {
            return;
        }
        if self.send.direction != Direction::Send
            || self.receive.direction != Direction::Receive
            || self.send.boot != self.shared.channel.0.sender
            || self.receive.boot != self.shared.channel.0.peer
            || self.send.validate().is_err()
            || self.receive.validate().is_err()
        {
            self.shared.fail(Fault::InvalidState);
            return;
        }
        let current = match direction {
            Direction::Send => self.send,
            Direction::Receive => self.receive,
        };
        let next = match current.next(boot, sequence) {
            Ok(next) => next,
            Err(fault) => {
                self.shared.fail(fault);
                return;
            }
        };
        let revision = match self.shared.begin_publication(self.revision) {
            Ok(revision) => revision,
            Err(fault) => {
                self.shared.fail(fault);
                return;
            }
        };
        match direction {
            Direction::Send => self.send = next,
            Direction::Receive => self.receive = next,
        }
        self.shared
            .finish_publication(revision, self.send, self.receive);
        self.revision = revision;
    }
}

/// Ordinary test channels own only this unattached slot, never a shared probe.
pub(super) struct Slot {
    sender: BootId,
    peer: BootId,
    attempted: bool,
    writer: Option<Writer>,
}

impl Slot {
    pub(super) fn new(sender: BootId, peer: BootId) -> Self {
        Self {
            sender,
            peer,
            attempted: false,
            writer: None,
        }
    }

    pub(super) fn attempt(&mut self) {
        self.attempted = true;
    }

    pub(super) fn install(&mut self, name: &str) -> io::Result<Probe> {
        let fault = if self.attempted {
            Some(Fault::InstalledAfterActivity)
        } else if self.writer.is_some() {
            Some(Fault::AlreadyInstalled)
        } else {
            None
        };
        if let Some(fault) = fault {
            if let Some(writer) = &self.writer {
                writer.shared.fail(fault);
            }
            return Err(io::Error::other(format!(
                "cannot install transport probe: {fault:?}"
            )));
        }
        let shared = Arc::new(Shared {
            channel: ChannelId(Arc::new(Identity {
                name: name.to_owned(),
                sender: self.sender,
                peer: self.peer,
            })),
            revision: AtomicU64::new(0),
            failure: AtomicU8::new(0),
            send: Counters::default(),
            receive: Counters::default(),
        });
        self.writer = Some(Writer {
            shared: shared.clone(),
            revision: 0,
            send: Progress::empty(Direction::Send, self.sender),
            receive: Progress::empty(Direction::Receive, self.peer),
        });
        Ok(Probe { shared })
    }

    pub(super) fn record(&mut self, direction: Direction, boot: BootId, sequence: u64) {
        if let Some(writer) = &mut self.writer {
            writer.record(direction, boot, sequence);
        }
    }
}

#[path = "transport_probe_tests.rs"]
mod tests;
