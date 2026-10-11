//! Runner player for the simulated room: sends a stimulus to the room's
//! control socket and waits (on its own thread) for the room's `Started` and
//! `Finished` notices. The reported start is the host time the stimulus's
//! first block was handed to the microphone loopback (a digital onset, not an
//! acoustic one).
use super::room::{Command, Notice};
use crate::{Result, physical::mac::Player, stimulus::SceneTiming};
use lamp_ipc::monotonic_us;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
/// Extra time after the planned stimulus end before delivery is called failed.
const FINISH_GRACE_US: u64 = 3_000_000;
const START_BOUND_US: u64 = 2_000_000;

pub struct RoomPlayer {
    control: PathBuf,
    /// Short private directory for reply sockets (socket paths are length-bounded).
    replies: PathBuf,
    next_id: u64,
}

impl RoomPlayer {
    pub fn new(control: PathBuf) -> Result<Self> {
        let replies = PathBuf::from(format!(
            "/tmp/lve-room-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&replies)?;
        fs::set_permissions(&replies, fs::Permissions::from_mode(0o700))?;
        Ok(Self {
            control,
            replies,
            next_id: 1,
        })
    }
}

impl Drop for RoomPlayer {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.replies);
    }
}

fn wait_notice(socket: &UnixDatagram, id: u64, until_us: u64) -> Option<Notice> {
    let mut buffer = [0_u8; 1024];
    while monotonic_us() < until_us {
        let wait = (until_us - monotonic_us()).clamp(1_000, 50_000);
        socket
            .set_read_timeout(Some(Duration::from_micros(wait)))
            .ok()?;
        if let Ok(count) = socket.recv(&mut buffer)
            && let Ok(notice) = serde_json::from_slice::<Notice>(&buffer[..count])
            && notice.id() == id
        {
            return Some(notice);
        }
    }
    None
}

impl Player for RoomPlayer {
    fn start(
        &mut self,
        wav: &Path,
        timing: &SceneTiming,
        _report_directory: &Path,
        cancel_file: &Path,
    ) -> Result<JoinHandle<Value>> {
        let id = self.next_id;
        self.next_id += 1;
        let reply_path = self.replies.join(format!("{id}.sock"));
        let socket = UnixDatagram::bind(&reply_path)?;
        let command = Command::Play {
            id,
            wav: wav.to_path_buf(),
            reply: Some(reply_path.clone()),
        };
        let requested = monotonic_us();
        UnixDatagram::unbound()?.send_to(&serde_json::to_vec(&command)?, &self.control)?;
        let control = self.control.clone();
        let cancel_file = cancel_file.to_path_buf();
        let duration_us = u64::from(timing.duration_ms) * 1000;
        Ok(thread::spawn(move || {
            let report = (|| {
                let Some(started) = wait_notice(&socket, id, requested + START_BOUND_US) else {
                    return json!({"valid": false, "status": "room did not start the stimulus",
                        "runner_requested_host_monotonic_ns": requested * 1000});
                };
                let (first_sample, onset_us) = match started {
                    Notice::Started { mic_sample, host_us, .. } => (mic_sample, host_us),
                    Notice::Rejected { reason, .. } => {
                        return json!({"valid": false, "status": "rejected", "error": reason});
                    }
                    other => return json!({"valid": false, "status": "unexpected notice", "notice": other}),
                };
                let deadline = onset_us + duration_us + FINISH_GRACE_US;
                let mut finished = None;
                while monotonic_us() < deadline {
                    if cancel_file.exists() {
                        let _ = UnixDatagram::unbound().and_then(|s| {
                            s.send_to(
                                &serde_json::to_vec(&Command::Cancel { id }).unwrap_or_default(),
                                &control,
                            )
                        });
                    }
                    if let Some(notice) = wait_notice(&socket, id, (monotonic_us() + 100_000).min(deadline)) {
                        finished = Some(notice);
                        break;
                    }
                }
                match finished {
                    Some(Notice::Finished { mic_sample, host_us, samples, cancelled, .. }) => json!({
                        "valid": !cancelled,
                        "status": if cancelled { "cancelled" } else { "room_delivered" },
                        // Digital onset: first block handed to the microphone loopback.
                        "start_requested_host_monotonic_ns": onset_us * 1000,
                        "runner_requested_host_monotonic_ns": requested * 1000,
                        "room_first_sample": first_sample,
                        "room_end_sample": mic_sample,
                        "room_finished_host_monotonic_ns": host_us * 1000,
                        "samples": samples,
                        "boundary": "loopback hand-off of the first and last blocks; digital, not acoustic"}),
                    _ => json!({"valid": false, "status": "room did not report the end of the stimulus",
                        "start_requested_host_monotonic_ns": onset_us * 1000}),
                }
            })();
            let _ = fs::remove_file(&reply_path);
            report
        }))
    }
    fn describe(&self) -> Value {
        json!({"kind": "simulated digital room", "control": self.control,
            "note": "process simulation on kernel loopback hardware; not acoustic"})
    }
}
