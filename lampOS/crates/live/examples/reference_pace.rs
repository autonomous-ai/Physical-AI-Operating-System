//! Paced check of the echo reference between two processes.
//!
//! No audio device. A child process stands in for the speaker: it sends one
//! accepted-render packet per block of a clock that runs `SKEW_PPM` faster
//! than this host's, over a private datagram socket. This process stands in
//! for the capture worker: every 10 ms of the host clock it drains the
//! socket and runs the production reference calls and the real canceller on
//! a block of silence. Both are paced by the real clock and scheduled by the
//! real kernel, which the virtual-time tests are not.
//!
//! It is a check of this software under real pacing. It is not time on a
//! lamp: there is no USB clock here and no ALSA.
//!
//! ```text
//! reference_pace SECONDS SKEW_PPM fixed|clocked
//! ```
use lamp_audio::{CAPTURE_SAMPLES, EchoProcessor, RENDER_SAMPLES};
use lamp_ipc::{Endpoint, MAX_DATAGRAM_BYTES, monotonic_us};
use lamp_live::{
    process::SessionDirectory,
    reference::{PRIME_FRAMES, ReferenceAssembly},
    wire::{ClockPrimed, ReferenceFault, RenderPayload, RenderReference},
};
use serde_json::json;
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn usage() -> io::Error {
    io::Error::other("usage: reference_pace SECONDS SKEW_PPM fixed|clocked")
}

fn main() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [role, own, peer, seconds, skew] if role == "--speaker" => speaker(
            Path::new(own),
            Path::new(peer),
            seconds.parse()?,
            skew.parse()?,
        ),
        [seconds, skew, mode] if mode == "fixed" || mode == "clocked" => {
            capture(seconds.parse()?, skew.parse()?, mode == "clocked")
        }
        _ => Err(usage().into()),
    }
}

/// One block per 10 ms of a clock `skew_ppm` fast, from the moment the
/// capture side says it has primed.
fn speaker(own: &Path, peer: &Path, seconds: u64, skew_ppm: f64) -> Result<()> {
    let mut link = Endpoint::bind(own)?;
    link.connect(peer)?;
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
    let blocks = (seconds as f64 * 100.0 * (1.0 + skew_ppm * 1e-6)) as u64 + 200;
    for block in 1..=blocks {
        let due = started + period.mul_f64(block as f64);
        // Sleep most of the way and spin the last millisecond: a desktop
        // kernel may wake a sleeping background process tens of milliseconds
        // late, which a speaker's write loop on the lamp is not.
        if let Some(wait) = due
            .checked_duration_since(Instant::now())
            .and_then(|wait| wait.checked_sub(Duration::from_millis(1)))
        {
            sleep(wait);
        }
        while Instant::now() < due {
            std::hint::spin_loop();
        }
        let at = monotonic_us();
        let packet = RenderReference {
            playback_epoch: 1,
            privacy_generation: 2,
            sequence: block,
            accepted_at_us: at,
            queue_observed_at_us: at,
            first_sample: PRIME_FRAMES as u64 + (block - 1) * RENDER_SAMPLES as u64,
            queued_frames: 456,
            payload: RenderPayload::Silence {
                frames: RENDER_SAMPLES as u16,
            },
        };
        match link.try_send(&serde_json::to_vec(&packet)?) {
            Ok(()) => {}
            // The capture side has finished or faulted and gone.
            Err(_) if block > 100 => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn capture(seconds: u64, skew_ppm: f64, clocked: bool) -> Result<()> {
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
        .stdin(Stdio::null())
        .spawn()?;
    // The child binds before it can be connected to.
    let deadline = Instant::now() + Duration::from_secs(5);
    while link.connect(&peer).is_err() {
        if Instant::now() > deadline {
            let _ = child.kill();
            return Err(io::Error::other("speaker process did not bind").into());
        }
        sleep(Duration::from_millis(2));
    }
    let mut state = if clocked {
        ReferenceAssembly::capture_clocked()
    } else {
        ReferenceAssembly::default()
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
    let (mut blocks, mut busy, mut worst_late) = (0_u64, Duration::ZERO, Duration::ZERO);
    let (mut least, mut most) = (usize::MAX, 0);
    let (mut last_packet_at, mut worst_packet_gap) = (0_u64, 0_u64);
    let mut fault = None;
    'run: for block in 1..=seconds * 100 {
        // Half a block after the speaker's, as a device's phase might be.
        let due = begun + Duration::from_micros(5_000 + block * 10_000);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            sleep(wait);
        }
        // How late this process itself was woken: a stall of the host, which
        // on a lamp would be a stall of the audio devices too.
        worst_late = worst_late.max(Instant::now().saturating_duration_since(due));
        let work = Instant::now();
        let step = (|| -> Result<()> {
            loop {
                let length = match link.try_recv(&mut buffer) {
                    Ok(length) => length,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.into()),
                };
                let packet: RenderReference = serde_json::from_slice(&buffer[..length])?;
                if last_packet_at > 0 {
                    worst_packet_gap =
                        worst_packet_gap.max(packet.accepted_at_us.saturating_sub(last_packet_at));
                }
                last_packet_at = packet.accepted_at_us;
                state.accept(packet, monotonic_us(), 2, &mut echo)?;
            }
            let delay = state.before_capture(16, monotonic_us())?;
            state.render_for_capture(monotonic_us(), &mut echo)?;
            echo.capture(&silence, delay)?;
            state.captured(monotonic_us() - 1, monotonic_us())
        })();
        busy += work.elapsed();
        if let Err(error) = step {
            fault = Some(match error.downcast_ref::<ReferenceFault>() {
                Some(fault) => format!("{:?}", fault.reason),
                None => error.to_string(),
            });
            break 'run;
        }
        blocks = block;
        if let Some(drift) = state.drift() {
            least = least.min(drift.unread_frames);
            most = most.max(drift.unread_frames);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let resident = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse::<u64>().ok());
    let timing = state.timing();
    let drift = state.drift();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "mode": if clocked { "clocked" } else { "fixed" },
            "skew_ppm": skew_ppm,
            "seconds_asked": seconds,
            "capture_blocks": blocks,
            "fault": fault,
            "accepted_through": timing.accepted_through,
            "analysed_through": timing.analysed_through,
            "learned_ppm": drift.map(|drift| drift.steady_ppm),
            "rate_ppm": drift.map(|drift| drift.ppm),
            "ring_frames_least_most": drift.map(|_| [least, most]),
            "rendered_blocks": drift.map(|drift| drift.rendered_blocks),
            "busy_us_per_block": busy.as_micros() as f64 / blocks.max(1) as f64,
            "worst_wake_late_us": worst_late.as_micros() as u64,
            "worst_gap_between_packets_us": worst_packet_gap,
            "last_packet_age_us_at_end": monotonic_us().saturating_sub(last_packet_at),
            "resident_kib": resident,
            "boundary": "two host processes, real clock and scheduler, no audio device",
        }))?
    );
    Ok(())
}
