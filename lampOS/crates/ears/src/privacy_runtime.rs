//! Linux capture with a Voice-owned lease. Bus/filesystem work stays on one
//! bounded pump; it cannot stall revocation in the microphone owner.
use crate::{Arguments, NAME, log};
use alsa::{
    Direction, ValueOr,
    pcm::{Access, Format, HwParams, PCM},
};
use lamp_bus::{Bus, Message, Notice, Subscriber};
use lamp_ears::{
    BLOCK, RATE, Settings,
    privacy::{Capture, CaptureOwner, Emission, LeaseGate, PublicationGate},
};
use lamp_msgs::{Diagnostic, Level, MicrophonePrivacy, RoomSound, SuddenSound, Voice, VoiceState};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TICK: Duration = Duration::from_millis(2);
const BATCH: usize = 16;
const CAPACITY: usize = 16;
const RETRY_US: u64 = 2_000_000;

struct Device(PCM);

impl Device {
    fn open(name: &str) -> io::Result<Self> {
        // Unlike the compatibility path, no read waits for a complete block.
        let pcm = PCM::new(name, Direction::Capture, true).map_err(io::Error::other)?;
        {
            let hardware = HwParams::any(&pcm).map_err(io::Error::other)?;
            hardware
                .set_access(Access::RWInterleaved)
                .map_err(io::Error::other)?;
            hardware
                .set_format(Format::s16())
                .map_err(io::Error::other)?;
            hardware.set_channels(1).map_err(io::Error::other)?;
            hardware
                .set_rate(RATE, ValueOr::Nearest)
                .map_err(io::Error::other)?;
            hardware
                .set_period_size_near(BLOCK as i64, ValueOr::Nearest)
                .map_err(io::Error::other)?;
            hardware
                .set_buffer_size_near(BLOCK as i64 * 10)
                .map_err(io::Error::other)?;
            pcm.hw_params(&hardware).map_err(io::Error::other)?;
        }
        if pcm
            .hw_params_current()
            .map_err(io::Error::other)?
            .get_rate()
            .map_err(io::Error::other)?
            != RATE
        {
            return Err(io::Error::other("room microphone rate is not 16 kHz"));
        }
        pcm.start().map_err(io::Error::other)?;
        Ok(Self(pcm))
    }
}

impl Capture for Device {
    fn read(&mut self, samples: &mut [i16]) -> io::Result<usize> {
        match self.0.io_i16().and_then(|reader| reader.readi(samples)) {
            Ok(count) => Ok(count),
            Err(error)
                if io::Error::from_raw_os_error(error.errno()).kind()
                    == io::ErrorKind::WouldBlock =>
            {
                Ok(0)
            }
            Err(error) => Err(io::Error::other(error)),
        }
    }

    fn discard(&mut self) -> io::Result<()> {
        self.0.drop().map_err(io::Error::other)
    }
}

struct Input {
    message: Message,
    fence: u64,
}

enum Output {
    Sound {
        emission: Emission,
        fence: u64,
        at_us: u64,
    },
    Health {
        open: bool,
        failures: u64,
        observed_us: u64,
    },
}

#[derive(Default)]
struct Shared {
    publication: PublicationGate,
    speaking: AtomicBool,
    stop: AtomicBool,
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(u128::from(u64::MAX)) as u64
        })
}

fn input(
    subscriber: &mut Subscriber,
    gate: &mut LeaseGate,
    sender: &SyncSender<Input>,
    shared: &Shared,
) -> io::Result<bool> {
    let before = (
        subscriber.malformed(),
        subscriber.expired(),
        subscriber.wrong_type(),
        subscriber.repeated(),
    );
    let message = subscriber.recv(Duration::ZERO)?;
    let after = (
        subscriber.malformed(),
        subscriber.expired(),
        subscriber.wrong_type(),
        subscriber.repeated(),
    );
    if before != after {
        gate.close();
        shared.publication.fence();
    }
    if subscriber
        .notices()
        .iter()
        .any(|notice| matches!(notice, Notice::SourceGone | Notice::Late))
    {
        gate.close();
        shared.publication.fence();
    }
    let Some(message) = message else {
        return Ok(false);
    };
    let previous = gate.epoch();
    let valid = gate
        .observe(&message, lamp_ipc::monotonic_us(), unix_ms())
        .is_ok();
    if !valid || previous != gate.epoch() || message.publisher_restarted {
        shared.publication.fence();
    }
    if !valid {
        return Ok(true);
    }
    let fence = shared.publication.generation();
    match sender.try_send(Input { message, fence }) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            gate.close();
            shared.publication.fence();
        }
        Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("capture owner ended")),
    }
    Ok(true)
}

fn connected(
    bus: &Bus,
    clock_id: &str,
    gate: &mut LeaseGate,
    sender: &SyncSender<Input>,
    output: &Receiver<Output>,
    shared: &Shared,
) -> io::Result<()> {
    let mut privacy = bus.subscribe_typed(
        MicrophonePrivacy::TOPIC,
        "ears_privacy",
        MicrophonePrivacy::TYPE,
    )?;
    let mut levels = bus.publisher_declared(RoomSound::TOPIC, &RoomSound::declaration(), NAME)?;
    let mut sudden =
        bus.publisher_declared(SuddenSound::TOPIC, &SuddenSound::declaration(), NAME)?;
    let mut health = bus.publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), NAME)?;
    let mut looked = 0;
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut drained = false;
        for _ in 0..BATCH {
            if !input(&mut privacy, gate, sender, shared)? {
                drained = true;
                break;
            }
        }
        if !drained {
            gate.close();
            shared.publication.fence();
        }
        // Only bounded metadata is queued, never PCM. A blocked publication
        // stops lease delivery; the independent owner then closes on expiry.
        for _ in 0..BATCH {
            match output.try_recv() {
                Ok(Output::Sound {
                    emission,
                    fence,
                    at_us,
                }) => {
                    if let Some(level) = emission.heard.level
                        && shared.publication.allows(
                            emission.permit,
                            fence,
                            lamp_ipc::monotonic_us(),
                        )
                    {
                        levels.publish_json(at_us, &level)?;
                    }
                    if let Some(sound) = emission.heard.sudden
                        && shared.publication.allows(
                            emission.permit,
                            fence,
                            lamp_ipc::monotonic_us(),
                        )
                    {
                        sudden.publish_json(at_us, &sound)?;
                    }
                }
                Ok(Output::Health {
                    open,
                    failures,
                    observed_us,
                }) => {
                    let message = if open {
                        "microphone observed open"
                    } else {
                        "microphone observed closed"
                    };
                    let report = Diagnostic::new(Level::Ok, NAME, message)
                        .with("privacy_controls", "on")
                        .with("capture_open_observed", open)
                        .with("capture_observed_us", observed_us)
                        .with("capture_clock_id", clock_id)
                        .with("capture_failures", failures);
                    health.publish_json(observed_us, &report)?;
                    sudden.alive()?;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        let now = lamp_ipc::monotonic_us();
        if now.saturating_sub(looked) >= 100_000 {
            looked = now;
            let speaking = bus.latest(VoiceState::TOPIC)?.is_some_and(|message| {
                unix_ms()
                    .checked_sub(message.header.at_unix_ms)
                    .is_some_and(|age| age < 3_000)
                    && message
                        .parse::<VoiceState>()
                        .is_ok_and(|state| state.state == Voice::Speaking)
            });
            shared.speaking.store(speaking, Ordering::Release);
        }
        thread::park_timeout(TICK);
    }
}

fn pump(
    bus: Bus,
    clock_id: String,
    sender: SyncSender<Input>,
    output: Receiver<Output>,
    shared: Arc<Shared>,
) {
    let mut gate = LeaseGate::new(clock_id.clone());
    while !shared.stop.load(Ordering::Acquire) {
        gate.close();
        shared.publication.fence();
        let _ = connected(&bus, &clock_id, &mut gate, &sender, &output, &shared);
        shared.publication.fence();
        shared.speaking.store(false, Ordering::Release);
        if !shared.stop.load(Ordering::Acquire) {
            thread::park_timeout(Duration::from_millis(250));
        }
    }
}

pub(super) fn run(arguments: &Arguments) -> io::Result<()> {
    let bus = match &arguments.bus {
        Some(root) => Bus::new(root)?,
        None => Bus::from_environment()?,
    }
    // The ordinary Bus discovery interval is 200 ms, longer than a privacy
    // lease. This selected reader must find new publishers before 50 ms.
    .rescan_every(Duration::from_millis(5));
    let clock_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .to_owned();
    let mut owner = CaptureOwner::<Device>::new(
        clock_id.clone(),
        Settings {
            rise_db: arguments.rise_db,
            ..Settings::default()
        },
    );
    let shared = Arc::new(Shared::default());
    let (send_input, input) = mpsc::sync_channel(CAPACITY);
    let (output, receive_output) = mpsc::sync_channel(CAPACITY);
    let pump_shared = shared.clone();
    let worker = thread::Builder::new()
        .name("ears-privacy-bus".into())
        .spawn(move || pump(bus, clock_id, send_input, receive_output, pump_shared))?;
    log("privacy controls on; microphone waits for Voice's fresh permission");
    let result = (|| {
        let mut seen_fence = shared.publication.generation();
        let mut failures = 0u64;
        let mut retry_at = 0u64;
        let mut reported = 0u64;
        loop {
            let fence = shared.publication.generation();
            if fence != seen_fence || fence == u64::MAX {
                owner.invalidate()?;
                seen_fence = fence;
            }
            let mut drained = false;
            for _ in 0..BATCH {
                match input.try_recv() {
                    Ok(input) if input.fence == seen_fence => {
                        // Malformed input is optional loss, but capture is
                        // mandatory closed until a later fresh valid lease.
                        if owner
                            .observe(&input.message, lamp_ipc::monotonic_us(), unix_ms())
                            .is_err()
                        {
                            shared.publication.update(None, seen_fence);
                            owner.invalidate()?;
                        }
                    }
                    Ok(_) => {
                        owner.invalidate()?;
                    }
                    Err(TryRecvError::Empty) => {
                        drained = true;
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        return Err(io::Error::other("privacy pump ended"));
                    }
                }
            }
            if !drained || seen_fence != shared.publication.generation() {
                owner.invalidate()?;
            }
            let permit = owner.maintain(lamp_ipc::monotonic_us())?;
            shared.publication.update(permit, seen_fence);
            if lamp_ipc::monotonic_us() >= retry_at {
                match owner.tick_while(
                    lamp_ipc::monotonic_us,
                    || Device::open(&arguments.device),
                    shared.speaking.load(Ordering::Acquire),
                    || seen_fence == shared.publication.generation(),
                ) {
                    Ok(Some(emission)) => {
                        let permit = owner.maintain(lamp_ipc::monotonic_us())?;
                        shared.publication.update(permit, seen_fence);
                        if shared.publication.allows(
                            emission.permit,
                            seen_fence,
                            lamp_ipc::monotonic_us(),
                        ) {
                            let _ = output.try_send(Output::Sound {
                                emission,
                                fence: seen_fence,
                                at_us: lamp_ipc::monotonic_us(),
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        failures = failures.saturating_add(1);
                        retry_at = lamp_ipc::monotonic_us().saturating_add(RETRY_US);
                        owner.invalidate()?;
                        shared.publication.update(None, seen_fence);
                    }
                }
            }
            if lamp_ipc::monotonic_us().saturating_sub(reported) >= 950_000 {
                reported = lamp_ipc::monotonic_us();
                let _ = output.try_send(Output::Health {
                    open: owner.is_open(),
                    failures,
                    observed_us: reported,
                });
            }
            thread::park_timeout(TICK);
        }
    })();
    shared.publication.fence();
    let close = owner.invalidate();
    shared.stop.store(true, Ordering::Release);
    worker.thread().unpark();
    // Bus filesystem work may be blocked. The handle is already gone; process
    // exit, rather than an unbounded join, ends that one remaining thread.
    if worker.is_finished() {
        let _ = worker.join();
    }
    close.and(result)
}
