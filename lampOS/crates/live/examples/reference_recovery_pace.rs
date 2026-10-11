//! Paced check of reference recovery between two processes.
//!
//! No audio device. A child process stands in for the speaker and sends one
//! accepted-render packet per block of a clock `SKEW_PPM` fast, over a
//! private datagram socket. This process stands in for the capture worker:
//! every 10 ms it drains the socket and runs the production calls,
//! `accept` and `capture_step`, and the real canceller. Both are paced by
//! the host's clock and scheduled by its kernel. A script makes either side
//! stop for a while, or lose or repeat a packet, at a given second; the
//! host adds stalls of its own.
//!
//! It is this software under real pacing. It is not time on a lamp: there
//! is no USB clock, no ALSA and no device buffer to overrun.
//!
//! ```text
//! reference_recovery_pace SECONDS SKEW_PPM fixed|capture off|bounded [SCRIPT...]
//!   sound                 the speaker carries speech, not zeros
//!   sender-pause=MS@S     the speaker process stops for MS at second S
//!   capture-pause=MS@S    the capture loop stops for MS at second S
//!   lose=N                the speaker's packet N never arrives
//!   twice=N               the speaker's packet N arrives twice
//!   privacy=S             the capture side's privacy generation moves on at second S
//! ```
use lamp_audio::{CAPTURE_SAMPLES, EchoProcessor, RENDER_SAMPLES};
use lamp_interaction::{
    AdmissionState, AdmittedInput, BootId, CaptureState, Controller, MonoTime, Permission,
    TurnOwner,
};
use lamp_ipc::{Endpoint, MAX_DATAGRAM_BYTES, monotonic_us};
use lamp_live::{
    process::SessionDirectory,
    reference::{PRIME_FRAMES, RecoveryPolicy, ReferenceAssembly},
    wire::{ClockPrimed, ReferenceFault, RenderPayload, RenderReference},
};
use serde_json::{Value, json};
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn usage() -> io::Error {
    io::Error::other(
        "usage: reference_recovery_pace SECONDS SKEW_PPM fixed|capture off|bounded [sound] [sender-pause=MS@S] [capture-pause=MS@S] [lose=N] [twice=N] [privacy=S]",
    )
}

#[derive(Default)]
struct Script {
    sound: bool,
    /// Milliseconds, at second.
    sender_pauses: Vec<(u64, u64)>,
    capture_pauses: Vec<(u64, u64)>,
    lose: Vec<u64>,
    twice: Vec<u64>,
    privacy_at: Option<u64>,
}

impl Script {
    fn parse(items: &[String]) -> Result<Self> {
        let mut script = Self::default();
        let pause = |value: &str| -> Result<(u64, u64)> {
            let (ms, at) = value.split_once('@').ok_or_else(usage)?;
            Ok((ms.parse()?, at.parse()?))
        };
        for item in items {
            match item.split_once('=') {
                None if item == "sound" => script.sound = true,
                Some(("sender-pause", value)) => script.sender_pauses.push(pause(value)?),
                Some(("capture-pause", value)) => script.capture_pauses.push(pause(value)?),
                Some(("lose", value)) => script.lose.push(value.parse()?),
                Some(("twice", value)) => script.twice.push(value.parse()?),
                Some(("privacy", value)) => script.privacy_at = Some(value.parse()?),
                _ => return Err(usage().into()),
            }
        }
        Ok(script)
    }

    /// What the speaker process is told, as its own argument.
    fn for_speaker(&self) -> String {
        let list = |values: &[u64]| {
            values
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        let pauses = self
            .sender_pauses
            .iter()
            .map(|(ms, at)| format!("{ms}@{at}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{};{};{};{}",
            u8::from(self.sound),
            pauses,
            list(&self.lose),
            list(&self.twice)
        )
    }

    fn from_speaker(text: &str) -> Result<Self> {
        let parts: Vec<&str> = text.split(';').collect();
        let [sound, pauses, lose, twice] = parts.as_slice() else {
            return Err(usage().into());
        };
        let numbers = |text: &str| -> Result<Vec<u64>> {
            text.split(',')
                .filter(|item| !item.is_empty())
                .map(|item| Ok(item.parse()?))
                .collect()
        };
        let mut script = Self {
            sound: *sound == "1",
            lose: numbers(lose)?,
            twice: numbers(twice)?,
            ..Self::default()
        };
        for item in pauses.split(',').filter(|item| !item.is_empty()) {
            let (ms, at) = item.split_once('@').ok_or_else(usage)?;
            script.sender_pauses.push((ms.parse()?, at.parse()?));
        }
        Ok(script)
    }
}

fn main() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [role, own, peer, seconds, skew, script] if role == "--speaker" => speaker(
            Path::new(own),
            Path::new(peer),
            seconds.parse()?,
            skew.parse()?,
            &Script::from_speaker(script)?,
        ),
        [seconds, skew, clock, recovery, script @ ..]
            if matches!(clock.as_str(), "fixed" | "capture")
                && matches!(recovery.as_str(), "off" | "bounded") =>
        {
            capture(
                seconds.parse()?,
                skew.parse()?,
                clock == "capture",
                recovery == "bounded",
                &Script::parse(script)?,
            )
        }
        _ => Err(usage().into()),
    }
}

fn speaker_owner() -> Result<TurnOwner> {
    let at = MonoTime::from_micros(monotonic_us());
    let mut controller = Controller::new(BootId::new([7; 16])?, at);
    controller.set_microphone_permission(at, Permission::Allowed)?;
    let until = at
        .checked_add(900_000)
        .ok_or_else(|| io::Error::other("lease"))?;
    controller.set_capture(at, CaptureState::RetainingUntil(until))?;
    controller.set_admission(at, AdmissionState::OpenUntil(until))?;
    Ok(controller.admit(at, AdmittedInput::NewTurn)?)
}

/// Sleep until a millisecond before `due`, then read the clock until it.
fn wait_until(due: Instant) {
    if let Some(wait) = due
        .checked_duration_since(Instant::now())
        .and_then(|wait| wait.checked_sub(Duration::from_millis(1)))
    {
        sleep(wait);
    }
    while Instant::now() < due {
        std::hint::spin_loop();
    }
}

/// One packet per 10 ms of a clock `skew_ppm` fast. A pause is the process
/// doing nothing; what it owes afterwards it sends at once, each accepted
/// when it is sent, as a speaker whose queue has run out would.
fn speaker(own: &Path, peer: &Path, seconds: u64, skew_ppm: f64, script: &Script) -> Result<()> {
    let mut link = Endpoint::bind(own)?;
    link.connect(peer)?;
    let owner = speaker_owner()?;
    let mut buffer = [0; MAX_DATAGRAM_BYTES];
    let started = loop {
        match link.try_recv(&mut buffer) {
            Ok(_) => break Instant::now(),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                sleep(Duration::from_micros(200))
            }
            Err(error) => return Err(error.into()),
        }
    };
    let period = Duration::from_secs_f64(0.01 / (1.0 + skew_ppm * 1e-6));
    let packets = (seconds as f64 * 100.0 * (1.0 + skew_ppm * 1e-6)) as u64 + 200;
    let mut paused = vec![false; script.sender_pauses.len()];
    for packet in 1..=packets {
        wait_until(started + period.mul_f64(packet as f64));
        for (index, (ms, at)) in script.sender_pauses.iter().enumerate() {
            if !paused[index] && started.elapsed() >= Duration::from_secs(*at) {
                paused[index] = true;
                sleep(Duration::from_millis(*ms));
            }
        }
        let at = monotonic_us();
        let first = PRIME_FRAMES as u64 + (packet - 1) * RENDER_SAMPLES as u64;
        let payload = if script.sound {
            RenderPayload::Speech {
                owner,
                samples: (0..RENDER_SAMPLES as u64)
                    .map(|i| (((first + i) % 97) as i16 - 48) * 120)
                    .collect(),
            }
        } else {
            RenderPayload::Silence {
                frames: RENDER_SAMPLES as u16,
            }
        };
        let bytes = serde_json::to_vec(&RenderReference {
            playback_epoch: 1,
            privacy_generation: 2,
            sequence: packet,
            accepted_at_us: at,
            queue_observed_at_us: at,
            first_sample: first,
            queued_frames: 456,
            payload,
        })?;
        let copies = if script.lose.contains(&packet) {
            0
        } else if script.twice.contains(&packet) {
            2
        } else {
            1
        };
        for _ in 0..copies {
            match link.try_send(&bytes) {
                Ok(()) => {}
                // The capture side has finished or faulted and gone.
                Err(_) if packet > 100 => return Ok(()),
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

fn capture(
    seconds: u64,
    skew_ppm: f64,
    clocked: bool,
    recover: bool,
    script: &Script,
) -> Result<()> {
    let directory = SessionDirectory::create()?;
    let (own, peer) = (
        directory.path.join("capture"),
        directory.path.join("speaker"),
    );
    let mut link = Endpoint::bind(&own)?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--speaker")
        .arg(&peer)
        .arg(&own)
        .arg(seconds.to_string())
        .arg(skew_ppm.to_string())
        .arg(script.for_speaker())
        .stdin(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while link.connect(&peer).is_err() {
        if Instant::now() > deadline {
            let _ = child.kill();
            return Err(io::Error::other("speaker process did not bind").into());
        }
        sleep(Duration::from_millis(2));
    }
    let mut state = match (clocked, recover) {
        (false, false) => ReferenceAssembly::default(),
        (true, false) => ReferenceAssembly::capture_clocked(),
        (false, true) => ReferenceAssembly::recentring(RecoveryPolicy::BOUNDED),
        (true, true) => ReferenceAssembly::recovering(RecoveryPolicy::BOUNDED),
    };
    let mut echo = EchoProcessor::new(false);
    let primed = monotonic_us();
    state.prime(
        ClockPrimed {
            playback_epoch: 1,
            privacy_generation: 2,
            accepted_zero_frames: PRIME_FRAMES,
            accepted_at_us: primed,
            queue_observed_at_us: primed,
            queued_frames: PRIME_FRAMES,
            discarded: None,
        },
        monotonic_us(),
        2,
        &mut echo,
    )?;
    let requested = monotonic_us();
    state.started(1, requested, monotonic_us(), monotonic_us())?;
    link.try_send(b"go")?;
    let begun = Instant::now();
    let mut buffer = [0; MAX_DATAGRAM_BYTES];
    let silence = [0; CAPTURE_SAMPLES];
    let (mut blocks, mut bypassed, mut reseeds) = (0_u64, 0_u64, 0_u32);
    let (mut worst_late, mut worst_gap, mut last_packet_at) = (Duration::ZERO, 0_u64, 0_u64);
    let mut statuses: Vec<Value> = Vec::new();
    let mut paused = vec![false; script.capture_pauses.len()];
    let mut fault = None;
    'run: for block in 1..=seconds * 100 {
        let due = begun + Duration::from_micros(5_000 + block * 10_000);
        wait_until(due);
        for (index, (ms, at)) in script.capture_pauses.iter().enumerate() {
            if !paused[index] && begun.elapsed() >= Duration::from_secs(*at) {
                paused[index] = true;
                sleep(Duration::from_millis(*ms));
            }
        }
        // How far behind its time this loop is: its own pauses and the host's.
        let late = Instant::now().saturating_duration_since(due);
        worst_late = worst_late.max(late);
        let privacy = if script
            .privacy_at
            .is_some_and(|at| begun.elapsed() >= Duration::from_secs(at))
        {
            3
        } else {
            2
        };
        let step = (|| -> Result<Option<u16>> {
            loop {
                let length = match link.try_recv(&mut buffer) {
                    Ok(length) => length,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.into()),
                };
                let packet: RenderReference = serde_json::from_slice(&buffer[..length])?;
                if last_packet_at > 0 {
                    worst_gap = worst_gap.max(packet.accepted_at_us.saturating_sub(last_packet_at));
                }
                last_packet_at = last_packet_at.max(packet.accepted_at_us);
                state.accept(packet, monotonic_us(), privacy, &mut echo)?;
            }
            // What a microphone would have waiting behind a loop this late.
            let waiting = 16 + late.as_micros() as i64 * 16 / 1_000;
            let read = monotonic_us();
            let step = state.capture_step(waiting, read - 1, read, &mut echo)?;
            if let Some(delay) = step {
                echo.capture(&silence, delay)?;
            }
            state.note_score(0.0);
            Ok(step)
        })();
        match step {
            Ok(Some(_)) => {}
            Ok(None) => bypassed += 1,
            Err(error) => {
                fault = Some(match error.downcast_ref::<ReferenceFault>() {
                    Some(fault) => format!("{:?}", fault.reason),
                    None => error.to_string(),
                });
                break 'run;
            }
        }
        blocks = block;
        reseeds += u32::from(state.take_reseed());
        while let Some(status) = state.take_status() {
            statuses.push(json!({"block":block,"second":block as f64 / 100.0,
                "trust":status.trust,"cause":status.cause,"untrusted_us":status.untrusted_us}));
        }
    }
    while let Some(status) = state.take_status() {
        statuses.push(
            json!({"block":blocks,"trust":status.trust,"cause":status.cause,
            "untrusted_us":status.untrusted_us}),
        );
    }
    let _ = child.kill();
    let _ = child.wait();
    let timing = state.timing();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "reference": if clocked { "capture" } else { "fixed" },
            "recovery": if recover { "bounded" } else { "off" },
            "skew_ppm": skew_ppm,
            "seconds_asked": seconds,
            "capture_blocks": blocks,
            "fault": fault,
            "statuses": statuses,
            "reseeds": reseeds,
            "bypassed_capture_blocks": bypassed,
            "counts": timing.recovery,
            "accepted_through": timing.accepted_through,
            "analysed_through": timing.analysed_through,
            "capture_blocks_processed": timing.capture_blocks_processed,
            "learned_ppm": state.drift().map(|drift| drift.steady_ppm),
            "worst_loop_late_us": worst_late.as_micros() as u64,
            "worst_gap_between_packets_us": worst_gap,
            "boundary": "two host processes, real clock and scheduler, no audio device",
        }))?
    );
    Ok(())
}
