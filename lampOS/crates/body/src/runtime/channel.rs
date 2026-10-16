//! Two private, finite datagram queues. Data never renews control authority.
use super::config::invalid;
use crate::control::Clock;
use lamp_ipc::{Endpoint, MAX_DATAGRAM_BYTES};
use lamp_turn_owner::{BootId, MonoTime};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs, io,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
};

pub const MAX_AGE_US: u64 = 100_000;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Packet<T> {
    boot: BootId,
    sequence: u64,
    sent_at_us: u64,
    payload: T,
}
pub struct Channel {
    endpoint: Endpoint,
    own: BootId,
    peer: BootId,
    sent: u64,
    received: u64,
    last_time: u64,
}
impl Channel {
    fn new(endpoint: Endpoint, own: BootId, peer: BootId) -> Self {
        Self {
            endpoint,
            own,
            peer,
            sent: 0,
            received: 0,
            last_time: 0,
        }
    }
    pub fn send<T: Serialize>(&mut self, payload: &T, now: MonoTime) -> io::Result<()> {
        let sequence = self
            .sent
            .checked_add(1)
            .ok_or_else(|| invalid("motor send sequence exhausted"))?;
        let mut bytes = [0u8; MAX_DATAGRAM_BYTES];
        let mut cursor = io::Cursor::new(bytes.as_mut_slice());
        serde_json::to_writer(
            &mut cursor,
            &Packet {
                boot: self.own,
                sequence,
                sent_at_us: now.as_micros(),
                payload,
            },
        )
        .map_err(invalid)?;
        let length = cursor.position() as usize;
        self.endpoint.try_send(&bytes[..length])?;
        self.sent = sequence;
        Ok(())
    }
    /// The packet's age is measured on `clock` after it is dequeued. A time read
    /// before the read would refuse a packet sent in between as from the future.
    pub fn receive<T: DeserializeOwned>(&mut self, clock: &impl Clock) -> io::Result<Option<T>> {
        let mut bytes = [0; MAX_DATAGRAM_BYTES];
        let n = match self.endpoint.try_recv(&mut bytes) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
            Err(e) => return Err(e),
        };
        let now = clock.now();
        let p: Packet<T> = serde_json::from_slice(&bytes[..n]).map_err(invalid)?;
        if p.boot != self.peer
            || self.received.checked_add(1) != Some(p.sequence)
            || p.sent_at_us < self.last_time
            || now
                .as_micros()
                .checked_sub(p.sent_at_us)
                .is_none_or(|v| v > MAX_AGE_US)
        {
            return Err(invalid("motor packet boot/order/freshness violation"));
        }
        self.received = p.sequence;
        self.last_time = p.sent_at_us;
        Ok(Some(p.payload))
    }
}
pub struct Channels {
    pub control: Channel,
    pub data: Channel,
    peer_control: Option<PathBuf>,
    peer_data: Option<PathBuf>,
    control_connected: bool,
    connected: bool,
}
impl Channels {
    pub fn pair(parent: BootId, worker: BootId) -> io::Result<(Self, Self)> {
        let (pc, wc) = Endpoint::pair()?;
        let (pd, wd) = Endpoint::pair()?;
        let side = |c, d, own, peer| Self {
            control: Channel::new(c, own, peer),
            data: Channel::new(d, own, peer),
            peer_control: None,
            peer_data: None,
            control_connected: true,
            connected: true,
        };
        Ok((side(pc, pd, parent, worker), side(wc, wd, worker, parent)))
    }
    pub fn bind(directory: &Path, parent: BootId, worker: BootId, child: bool) -> io::Result<Self> {
        let (side, other, own, peer) = if child {
            ("w", "p", worker, parent)
        } else {
            ("p", "w", parent, worker)
        };
        Ok(Self {
            control: Channel::new(
                Endpoint::bind(directory.join(format!("motor.c.{side}")))?,
                own,
                peer,
            ),
            data: Channel::new(
                Endpoint::bind(directory.join(format!("motor.d.{side}")))?,
                own,
                peer,
            ),
            peer_control: Some(directory.join(format!("motor.c.{other}"))),
            peer_data: Some(directory.join(format!("motor.d.{other}"))),
            control_connected: false,
            connected: false,
        })
    }
    /// One attempt, no blocking or rebind. The supervisor owns the startup bound.
    pub fn connect_step(&mut self) -> io::Result<bool> {
        if self.connected {
            return Ok(true);
        }
        if !self.control_connected {
            for path in [&self.peer_control, &self.peer_data] {
                let path = path.as_ref().expect("named peer");
                let meta = match fs::symlink_metadata(path) {
                    Ok(m) => m,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
                    Err(e) => return Err(e),
                };
                if !meta.file_type().is_socket()
                    || meta.uid() != rustix::process::geteuid().as_raw()
                {
                    return Err(invalid("motor peer path is not this user's socket"));
                }
                // bind and chmod are separate syscalls. Private parent directory
                // prevents outsiders from accessing this short startup interval.
                // Endpoint still rechecks the exact socket at final connect.
                if meta.mode() & 0o077 != 0 {
                    return Ok(false);
                }
            }
            match self
                .control
                .endpoint
                .connect(self.peer_control.as_ref().expect("named peer"))
            {
                Ok(()) => self.control_connected = true,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e),
            }
        }
        match self
            .data
            .endpoint
            .connect(self.peer_data.as_ref().expect("named peer"))
        {
            Ok(()) => {
                self.connected = true;
                Ok(true)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}
