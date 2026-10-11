//! The room's audio loop on the ALSA loopback card (Linux only).
//!
//! Each iteration blocks reading one 10 ms block of the runtime's accepted
//! speaker PCM (the loopback clock paces the room), applies queued control
//! commands, mixes, and writes one block into the microphone loopback. Both
//! streams are recorded to WAV for the whole room session, with a bounded
//! duration. Only the loopback PCMs named in the configuration are opened.
use super::room::{BLOCK, Command, Echo, RATE, RoomCore, WorkerSignal, load_stimulus};
use alsa::{
    Direction, ValueOr,
    pcm::{Access, Format, HwParams, PCM},
};
use lamp_ipc::monotonic_us;
use serde_json::json;
use std::{
    fs,
    io::Write,
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct RoomOptions {
    /// Loopback capture carrying the runtime's speaker output.
    pub speaker_in: String,
    /// Loopback playback feeding the runtime's microphone.
    pub mic_out: String,
    /// Private control socket path (created; parent must be 0700).
    pub control: PathBuf,
    /// New directory for recordings and the room event log.
    pub out: PathBuf,
    pub echo: Echo,
    /// gpio-sim `pull` attribute for the privacy line (pull-up = open).
    pub privacy_pull: Option<PathBuf>,
    pub max_seconds: u32,
}

fn open(device: &str, direction: Direction) -> Result<PCM> {
    let pcm = PCM::new(device, direction, false)?;
    {
        let params = HwParams::any(&pcm)?;
        params.set_access(Access::RWInterleaved)?;
        params.set_format(Format::s16())?;
        params.set_channels(1)?;
        params.set_rate(RATE, ValueOr::Nearest)?;
        params.set_period_size_near(BLOCK as i64, ValueOr::Nearest)?;
        params.set_buffer_size_near((BLOCK * 4) as i64)?;
        pcm.hw_params(&params)?;
    }
    let accepted = {
        let current = pcm.hw_params_current()?;
        current.get_rate()? == RATE && current.get_channels()? == 1
    };
    if !accepted {
        return Err(format!("{device} did not accept {RATE} Hz mono").into());
    }
    Ok(pcm)
}

/// Find runtime worker processes by their exact `worker ROLE` arguments.
fn worker_pids(role: &str) -> Vec<i32> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid: i32 = entry.file_name().to_str()?.parse().ok()?;
            let cmdline = fs::read(entry.path().join("cmdline")).ok()?;
            let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
            args.windows(2)
                .any(|pair| pair[0] == b"worker" && pair[1] == role.as_bytes())
                .then_some(pid)
        })
        .collect()
}

fn signal_workers(role: &str, signal: WorkerSignal) -> usize {
    use rustix::process::{Pid, Signal, kill_process};
    let signal = match signal {
        WorkerSignal::Kill => Signal::KILL,
        WorkerSignal::Stop => Signal::STOP,
        WorkerSignal::Continue => Signal::CONT,
    };
    worker_pids(role)
        .into_iter()
        .filter_map(Pid::from_raw)
        .filter(|pid| kill_process(*pid, signal).is_ok())
        .count()
}

fn wav(path: &Path) -> Result<hound::WavWriter<std::io::BufWriter<fs::File>>> {
    Ok(hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?)
}

/// Run the room until `Stop`, `max_seconds`, or an audio error.
pub fn run(options: &RoomOptions) -> Result<()> {
    fs::create_dir(&options.out)?;
    fs::set_permissions(&options.out, fs::Permissions::from_mode(0o700))?;
    let mut log = fs::OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(options.out.join("room-events.jsonl"))?;
    let mut record = |value: serde_json::Value| -> Result<()> {
        writeln!(log, "{value}")?;
        Ok(())
    };
    let speaker = open(&options.speaker_in, Direction::Capture)?;
    let mic = open(&options.mic_out, Direction::Playback)?;
    let control = UnixDatagram::bind(&options.control)?;
    fs::set_permissions(&options.control, fs::Permissions::from_mode(0o600))?;
    control.set_nonblocking(true)?;
    let mut speaker_wav = wav(&options.out.join("speaker.wav"))?;
    let mut mic_wav = wav(&options.out.join("microphone.wav"))?;
    let mut room = RoomCore::new(options.echo);
    let speaker_io = speaker.io_i16()?;
    let mic_io = mic.io_i16()?;
    // Two blocks of lead so the first write never underruns.
    let silence = [0_i16; BLOCK];
    mic_io.writei(&silence)?;
    mic_io.writei(&silence)?;
    speaker.start()?;
    let started_us = monotonic_us();
    record(json!({"kind":"room_start","at_us":started_us,"rate":RATE,"block":BLOCK,
        "echo":options.echo,"speaker_in":options.speaker_in,"mic_out":options.mic_out,
        "scope":"digital loopback room; not acoustic"}))?;
    let reply = UnixDatagram::unbound()?;
    let mut input = [0_i16; BLOCK];
    let mut output = [0_i16; BLOCK];
    let mut datagram = [0_u8; 4096];
    let max_us = u64::from(options.max_seconds) * 1_000_000;
    let mut xruns = 0_u64;
    loop {
        match speaker_io.readi(&mut input) {
            Ok(frames) if frames == BLOCK => {}
            Ok(frames) => input[frames..].fill(0),
            Err(error) => {
                xruns += 1;
                record(json!({"kind":"speaker_xrun","at_us":monotonic_us(),"error":error.to_string()}))?;
                speaker.prepare()?;
                speaker.start()?;
                input.fill(0);
            }
        }
        for _ in 0..16 {
            let count = match control.recv(&mut datagram) {
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
            };
            let now = monotonic_us();
            let command: Command = match serde_json::from_slice(&datagram[..count]) {
                Ok(command) => command,
                Err(error) => {
                    record(json!({"kind":"command_rejected","at_us":now,"error":error.to_string()}))?;
                    continue;
                }
            };
            record(json!({"kind":"command","at_us":now,"command":command}))?;
            match command {
                Command::Play { id, wav, reply } => match load_stimulus(&wav) {
                    Ok(pcm) => room.play(id, pcm, reply),
                    Err(reason) => {
                        room.play(id, Vec::new(), reply);
                        record(json!({"kind":"stimulus_unreadable","id":id,"error":reason}))?;
                    }
                },
                Command::Cancel { id } => room.cancel(id, now),
                Command::Echo { echo } => match echo.validate() {
                    Ok(echo) => room.set_echo(echo),
                    Err(error) => record(json!({"kind":"command_rejected","at_us":now,"error":error}))?,
                },
                Command::Privacy { muted } => {
                    let Some(pull) = &options.privacy_pull else {
                        record(json!({"kind":"command_rejected","at_us":now,"error":"no privacy line configured"}))?;
                        continue;
                    };
                    fs::write(pull, if muted { "pull-down" } else { "pull-up" })?;
                    record(json!({"kind":"privacy_set","at_us":monotonic_us(),"muted":muted}))?;
                }
                Command::Signal { role, signal } => {
                    let count = signal_workers(&role, signal);
                    record(json!({"kind":"worker_signalled","at_us":monotonic_us(),
                        "role":role,"signal":signal,"processes":count}))?;
                }
                Command::Stop {} => {
                    record(json!({"kind":"room_stop","at_us":now,"xruns":xruns}))?;
                    speaker_wav.finalize()?;
                    mic_wav.finalize()?;
                    return Ok(());
                }
            }
        }
        let handed_us = monotonic_us();
        room.mix(&input, &mut output, handed_us);
        if let Err(error) = mic_io.writei(&output) {
            xruns += 1;
            record(json!({"kind":"microphone_xrun","at_us":monotonic_us(),"error":error.to_string()}))?;
            mic.prepare()?;
            mic_io.writei(&silence)?;
            mic_io.writei(&output)?;
        }
        for sample in input {
            speaker_wav.write_sample(sample)?;
        }
        for sample in output {
            mic_wav.write_sample(sample)?;
        }
        for (target, notice) in room.take_notices() {
            record(json!({"kind":"notice","notice":notice}))?;
            if let Some(target) = target {
                let _ = reply.send_to(&serde_json::to_vec(&notice)?, target);
            }
        }
        if monotonic_us().saturating_sub(started_us) >= max_us {
            record(json!({"kind":"room_stop","at_us":monotonic_us(),"reason":"max_seconds","xruns":xruns}))?;
            speaker_wav.finalize()?;
            mic_wav.finalize()?;
            return Ok(());
        }
    }
}
