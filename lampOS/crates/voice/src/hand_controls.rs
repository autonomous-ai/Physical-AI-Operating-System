//! Optional hand controls. Only the coordinator owns the mute latch; a single
//! bounded pump does bus/filesystem I/O for all supervised runtime cycles.
//!
//! A mute request closes live authority before persistence. Before audio opens,
//! `arm_active` stores a conservative muted recovery record without changing the
//! live preference. A crash/fault therefore restores muted; only verified clean
//! cleanup, or a stop the service was asked for (SIGTERM, an update; #960),
//! may disarm that guard, writing the live latch as it was. An unacknowledged request before any durable
//! write cannot be promised to survive arbitrary storage/power failure.
use lamp_bus::{Bus, Kind, Message, Notice, Subscriber};
use lamp_controls::{mute::MuteRecord, receiver::Receiver};
use lamp_msgs::{ControlGesture, ControlIntent, ControlsState};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver as ChannelReceiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const QUEUE_CAPACITY: usize = 32;
const POLL_LIMIT: usize = 16;
const PUMP_TICK: Duration = Duration::from_millis(2);
const RECONNECT_AFTER: Duration = Duration::from_millis(250);
const FINAL_JOIN_WAIT: Duration = Duration::from_millis(20);
const SUBSCRIBER: &str = "voice_hand";

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlsOptions {
    pub stop: bool,
    /// Existing trusted private directory, absolute filename; enables mute.
    pub mute_record: Option<PathBuf>,
    /// Overrides `LAMP_BUS_ROOT`; neither setting alone enables consumption.
    pub bus_root: Option<PathBuf>,
}

impl ControlsOptions {
    pub fn enabled(&self) -> bool {
        self.stop || self.mute_record.is_some()
    }

    /// Shape checks only: disabled options never open a path or start a thread.
    pub fn validate(&self) -> io::Result<()> {
        if self
            .bus_root
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(invalid("hand controls bus root must be absolute"));
        }
        if self
            .mute_record
            .as_ref()
            .is_some_and(|path| !path.is_absolute() || path.file_name().is_none())
        {
            return Err(invalid(
                "hand controls mute record needs an absolute filename",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Observation {
    pub stop: bool,
    pub muted: bool,
    pub mute_changed: bool,
    pub pets: u64,
    /// Coordinator poll/acceptance time, on the shared monotonic clock. This
    /// excludes recognizer, publication, bus and pump queue delay.
    pub observed_at_us: Option<u64>,
    /// Earliest original release accepted for Stop/mute in this poll. Neither
    /// timestamp is speaker delivery or evidence of acoustic silence.
    pub acquired_at_us: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub received: u64,
    pub rejected: u64,
    pub lost: u64,
    pub reconnects: u64,
    pub discontinuities: u64,
    pub pump_running: bool,
    pub persistence_failed: bool,
}

#[derive(Default)]
struct Shared {
    received: AtomicU64,
    rejected: AtomicU64,
    lost: AtomicU64,
    reconnects: AtomicU64,
    discontinuities: AtomicU64,
    queued: AtomicUsize,
    running: AtomicBool,
    stop: AtomicBool,
}

fn add(counter: &AtomicU64, amount: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(amount))
    });
}

impl Shared {
    fn fence(&self) {
        // Saturation remains a permanently unsafe continuity observation.
        let _ = self
            .discontinuities
            .fetch_update(Ordering::Release, Ordering::Relaxed, |value| {
                Some(value.saturating_add(1))
            });
    }

    fn reject(&self) {
        add(&self.rejected, 1);
        self.fence();
    }
}

enum Input {
    State(Box<ControlsState>),
    Pet(ControlGesture),
}

struct Queued {
    input: Input,
    generation: u64,
}

struct Pump {
    thread: Option<JoinHandle<()>>,
    finished: ChannelReceiver<()>,
    shared: Arc<Shared>,
}

struct PumpFinished {
    shared: Arc<Shared>,
    finished: SyncSender<()>,
}

impl Drop for PumpFinished {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Release);
        self.shared.fence();
        let _ = self.finished.try_send(());
    }
}

impl Drop for Pump {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            worker.thread().unpark();
            // Never wait indefinitely for a filesystem/syscall inside Bus.
            // This owner is dropped only on final service exit, not per cycle.
            if self.finished.recv_timeout(FINAL_JOIN_WAIT).is_ok() || worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

pub struct HandControls {
    options: ControlsOptions,
    receiver: Receiver,
    input: ChannelReceiver<Queued>,
    shared: Arc<Shared>,
    seen_generation: u64,
    dirty: bool,
    armed: bool,
    persistence_failed: bool,
    // One handle for the entire service, not one per supervised audio cycle.
    _pump: Option<Pump>,
}

impl HandControls {
    /// Composed coordinator tests use the real receiver and bounded channel,
    /// without a GPIO, bus thread, persistence path or invented permissions.
    #[cfg(test)]
    pub(crate) fn with_test_state(
        clock_id: &str,
        started_us: u64,
        state: ControlsState,
    ) -> io::Result<Self> {
        let (sender, input) = mpsc::sync_channel(QUEUE_CAPACITY);
        let shared = Arc::new(Shared::default());
        let owner = Self::from_parts(
            ControlsOptions {
                stop: true,
                ..Default::default()
            },
            MuteRecord::new(clock_id)?,
            input,
            shared,
            started_us,
        )?;
        enqueue(&sender, &owner.shared, Input::State(Box::new(state)));
        Ok(owner)
    }

    /// Restore and validate durability before any Voice worker is opened.
    /// The pump alone may open bus paths; an unavailable bus is optional input
    /// loss, not permission to unmute and not a failure of healthy audio.
    pub fn restore(options: &ControlsOptions) -> io::Result<Option<Self>> {
        options.validate()?;
        if !options.enabled() {
            return Ok(None);
        }
        let clock_id = lamp_controls::runtime::clock_id(None)?;
        let now_us = lamp_ipc::monotonic_us();
        let record = match &options.mute_record {
            Some(path) => {
                let record = MuteRecord::load(path, &clock_id, now_us)?;
                // Checks the private parent and first-use/rebased persistence
                // now. A path that cannot store a mute may not open audio.
                record.save(path)?;
                record
            }
            None => MuteRecord::new(&clock_id)?,
        };
        let (sender, input) = mpsc::sync_channel(QUEUE_CAPACITY);
        let shared = Arc::new(Shared::default());
        let mut controls = Self::from_parts(options.clone(), record, input, shared, now_us)?;
        let root = options
            .bus_root
            .clone()
            .or_else(|| std::env::var_os(lamp_bus::ROOT_VARIABLE).map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(lamp_bus::DEFAULT_ROOT));
        let pump_shared = Arc::clone(&controls.shared);
        let (finished, completion) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("voice-hand-controls".into())
            .spawn(move || {
                pump_shared.running.store(true, Ordering::Release);
                let _finished = PumpFinished {
                    shared: Arc::clone(&pump_shared),
                    finished,
                };
                pump(root, sender, &pump_shared);
            })?;
        controls._pump = Some(Pump {
            thread: Some(worker),
            finished: completion,
            shared: Arc::clone(&controls.shared),
        });
        Ok(Some(controls))
    }

    fn from_parts(
        options: ControlsOptions,
        record: MuteRecord,
        input: ChannelReceiver<Queued>,
        shared: Arc<Shared>,
        now_us: u64,
    ) -> io::Result<Self> {
        let receiver = if options.mute_record.is_some() {
            Receiver::with_hold_clear(record, now_us)?
        } else {
            Receiver::new(record)?
        };
        Ok(Self {
            options,
            receiver,
            input,
            shared,
            seen_generation: 0,
            dirty: false,
            armed: false,
            persistence_failed: false,
            _pump: None,
        })
    }

    pub fn health(&self) -> Health {
        Health {
            received: self.shared.received.load(Ordering::Relaxed),
            rejected: self.shared.rejected.load(Ordering::Relaxed),
            lost: self.shared.lost.load(Ordering::Relaxed),
            reconnects: self.shared.reconnects.load(Ordering::Relaxed),
            discontinuities: self.shared.discontinuities.load(Ordering::Acquire),
            pump_running: self.shared.running.load(Ordering::Acquire),
            persistence_failed: self.persistence_failed,
        }
    }

    fn muted(&self) -> bool {
        self.options.mute_record.is_some() && self.receiver.mute_record().muted
    }

    fn fence_at(&mut self, now_us: u64) {
        if self.receiver.fence_hold_clear(now_us).is_err() {
            // An exhausted/invalid optional receiver cannot clear its latch,
            // but must not turn bus loss into failure of healthy unmuted Voice.
            add(&self.shared.rejected, 1);
        }
    }

    /// No bus/filesystem I/O, blocking receive, channel send or provider call.
    pub fn poll(&mut self) -> io::Result<Observation> {
        self.poll_with_clock(lamp_ipc::monotonic_us)
    }

    #[cfg(test)]
    fn poll_at(&mut self, now_us: u64) -> io::Result<Observation> {
        self.poll_with_clock(|| now_us)
    }

    fn poll_with_clock(&mut self, mut now: impl FnMut() -> u64) -> io::Result<Observation> {
        let now_us = now();
        let mut observation = Observation::default();
        let generation = self.shared.discontinuities.load(Ordering::Acquire);
        if generation != self.seen_generation || generation == u64::MAX {
            self.fence_at(now_us);
            self.seen_generation = generation;
        }
        let mut consumed = 0;
        for _ in 0..POLL_LIMIT {
            let queued = match self.input.try_recv() {
                Ok(queued) => queued,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.fence_at(now());
                    break;
                }
            };
            self.shared.queued.fetch_sub(1, Ordering::AcqRel);
            consumed += 1;
            let now_us = now();
            if queued.generation != generation {
                self.fence_at(now_us);
            }
            match queued.input {
                Input::State(state) => match self.receiver.observe(now_us, &state) {
                    Ok(effects) => {
                        if self.options.stop && effects.stop_requested {
                            observation.stop = true;
                            if let Some(stop) = state.latest_stop {
                                earlier(&mut observation.acquired_at_us, stop.released_us);
                            }
                        }
                        if self.options.mute_record.is_some() && effects.mute_changed {
                            self.dirty = true;
                            observation.mute_changed = true;
                            if let Some(mute) = state.latest_mute {
                                earlier(&mut observation.acquired_at_us, mute.released_us);
                            }
                        }
                    }
                    Err(_) => add(&self.shared.rejected, 1),
                },
                Input::Pet(event) => match self.receiver.pet(now_us, &event) {
                    Ok(true) => observation.pets += 1,
                    Ok(false) => {}
                    Err(_) => add(&self.shared.rejected, 1),
                },
            }
        }
        // A full batch may have newer requests still queued. It cannot clear.
        // Older one-way mutes and fresh Stops remain eligible in later batches.
        let now_us = now();
        if consumed == POLL_LIMIT
            || self.shared.discontinuities.load(Ordering::Acquire) != generation
            || self.persistence_failed
        {
            self.fence_at(now_us);
        }
        observation.muted = self.muted();
        if observation.stop || observation.mute_changed {
            observation.observed_at_us = Some(now_us);
        }
        Ok(observation)
    }

    /// Discard any queued/previous clear before an active runtime transition.
    /// It never clears software mute or grants a physical privacy permission.
    pub fn fence_clear(&mut self) -> io::Result<()> {
        self.fence_at(lamp_ipc::monotonic_us());
        Ok(())
    }

    /// Called only after local authority revocation, before a possibly failing
    /// teardown result is finalized. No acknowledgement precedes this save.
    pub fn persist_muted(&mut self) -> io::Result<()> {
        if self.muted() && self.dirty {
            let result = self
                .receiver
                .mute_record()
                .save(self.options.mute_record.as_ref().unwrap());
            if let Err(error) = result {
                self.persistence_failed = true;
                return Err(error);
            }
            self.dirty = false;
        }
        Ok(())
    }

    /// No audio workers may exist. Saves a mute before considering a fresh
    /// explicit clear, then saves the proposal before releasing the live latch.
    pub fn quiescent(&mut self) -> io::Result<bool> {
        self.quiescent_observation().map(|(allowed, _)| allowed)
    }

    /// The quiescent transaction with its accepted input report preserved.
    /// Muted supervisors use this to report advisory pets without any audio
    /// action. `muted` reflects the final latch after the persistence/clear
    /// transaction; the other fields describe this bounded input poll.
    pub fn quiescent_observation(&mut self) -> io::Result<(bool, Observation)> {
        self.quiescent_observation_with_clock(lamp_ipc::monotonic_us)
    }

    #[cfg(test)]
    fn quiescent_at(&mut self, now_us: u64) -> io::Result<bool> {
        self.quiescent_with_clock(|| now_us)
    }

    #[cfg(test)]
    fn quiescent_with_clock(&mut self, now: impl FnMut() -> u64) -> io::Result<bool> {
        self.quiescent_observation_with_clock(now)
            .map(|(allowed, _)| allowed)
    }

    fn quiescent_observation_with_clock(
        &mut self,
        mut now: impl FnMut() -> u64,
    ) -> io::Result<(bool, Observation)> {
        let mut observation = self.poll_with_clock(&mut now)?;
        self.persist_muted()?;
        let allowed = self.after_persistence(now)?;
        observation.muted = self.muted();
        Ok((allowed, observation))
    }

    fn after_persistence(&mut self, mut now: impl FnMut() -> u64) -> io::Result<bool> {
        if self.persistence_failed {
            return Err(io::Error::other(
                "hand mute persistence fault requires recovery",
            ));
        }
        // Disk sync may have blocked while the pump read a newer request or
        // lost continuity. Eligibility is checked after that work, not at the
        // earlier poll. Queued input is consumed on the next bounded tick.
        let generation = self.shared.discontinuities.load(Ordering::Acquire);
        if generation != self.seen_generation || generation == u64::MAX {
            self.fence_at(now());
            self.seen_generation = generation;
            return Ok(!self.muted());
        }
        if self.shared.queued.load(Ordering::Acquire) != 0 {
            return Ok(!self.muted());
        }
        if let Some(path) = &self.options.mute_record
            && self.receiver.clear_pending()
        {
            // Expiration is optional input loss, not a Voice service failure.
            if let Ok(prepared) = self.receiver.prepare_clear(now()) {
                let saved = prepared.record().save(path);
                if let Err(error) = saved {
                    self.persistence_failed = true;
                    return Err(error);
                }
                self.receiver.commit_clear(prepared)?;
                self.armed = false;
            }
        }
        Ok(!self.muted())
    }

    /// Before opening any audio worker, store the conservative restart guard.
    /// The live preference is unchanged. Do not call from the urgent audio loop.
    pub fn arm_active(&mut self) -> io::Result<()> {
        let Some(path) = &self.options.mute_record else {
            return Ok(());
        };
        if self.muted() || self.persistence_failed {
            return Err(io::Error::other("hand mute inhibits an active runtime"));
        }
        if !self.armed {
            let mut guard = self.receiver.mute_record().clone();
            guard.muted = true;
            if let Err(error) = guard.save(path) {
                self.persistence_failed = true;
                return Err(error);
            }
            self.armed = true;
        }
        Ok(())
    }

    /// Only after VERIFIED clean destruction of all audio workers. Fault,
    /// physical/software lifecycle exits and interrupted cleanup must keep the
    /// conservative disk guard until the supervisor chooses its safe next step.
    pub fn disarm_after_cleanup(&mut self) -> io::Result<()> {
        let Some(path) = &self.options.mute_record else {
            return Ok(());
        };
        if self.persistence_failed {
            return Err(io::Error::other(
                "hand mute persistence fault requires recovery",
            ));
        }
        if self.armed || self.dirty {
            if let Err(error) = self.receiver.mute_record().save(path) {
                self.persistence_failed = true;
                return Err(error);
            }
            self.armed = false;
            self.dirty = false;
        }
        Ok(())
    }
}

fn earlier(value: &mut Option<u64>, next: u64) {
    *value = Some(value.map_or(next, |previous| previous.min(next)));
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(u128::from(u64::MAX)) as u64
        })
}

fn parse(message: Message, state: bool, now_unix_ms: u64) -> io::Result<Input> {
    let header = &message.header;
    let (topic, type_name, kind, ttl) = if state {
        (ControlsState::TOPIC, ControlsState::TYPE, Kind::Latest, 300)
    } else {
        (
            ControlGesture::TOPIC,
            ControlGesture::TYPE,
            Kind::Event,
            500,
        )
    };
    if header.topic != topic
        || header.source != "controls"
        || header.type_name.as_deref() != Some(type_name)
        || header.kind != kind
        || header.enc != lamp_bus::JSON
        || header.seq == 0
        || header.boot.len() != 16
        || !header.boot.bytes().all(|byte| byte.is_ascii_hexdigit())
        || header.expires_ms != Some(ttl)
        || message.expired
        || header.expired_at(now_unix_ms)
        || header.at_unix_ms > now_unix_ms.saturating_add(lamp_bus::EXPIRY_GRACE_MS)
    {
        return Err(invalid("invalid or expired hand controls envelope"));
    }
    if state {
        let payload: ControlsState = message.parse()?;
        payload
            .check()
            .map_err(|_| invalid("invalid controls state"))?;
        if payload.session_id != header.boot {
            return Err(invalid(
                "controls state does not name its publisher instance",
            ));
        }
        Ok(Input::State(Box::new(payload)))
    } else {
        let payload: ControlGesture = message.parse()?;
        payload
            .check()
            .map_err(|_| invalid("invalid controls gesture"))?;
        // Commands are only consumed through the cumulative state. The event
        // publisher's boot differs from the state publisher/session identity.
        Ok(Input::Pet(payload))
    }
}

fn enqueue(sender: &SyncSender<Queued>, shared: &Shared, input: Input) -> bool {
    let queued = Queued {
        input,
        generation: shared.discontinuities.load(Ordering::Acquire),
    };
    // Publish the reservation before the item, so a quiescent clear never
    // mistakes a producer in try_send for an empty queue. One producer only.
    shared.queued.fetch_add(1, Ordering::AcqRel);
    match sender.try_send(queued) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            add(&shared.lost, 1);
            shared.fence();
            true
        }
        Err(TrySendError::Disconnected(_)) => {
            shared.queued.fetch_sub(1, Ordering::AcqRel);
            false
        }
    }
}

fn read_one(
    subscriber: &mut Subscriber,
    state: bool,
    sender: &SyncSender<Queued>,
    shared: &Shared,
) -> io::Result<bool> {
    let before = (
        subscriber.malformed(),
        subscriber.expired(),
        subscriber.wrong_type(),
    );
    let message = subscriber.recv(Duration::ZERO)?;
    let after = (
        subscriber.malformed(),
        subscriber.expired(),
        subscriber.wrong_type(),
    );
    if before != after {
        shared.reject();
    }
    if let Some(message) = message {
        if message.gap != 0 || message.publisher_restarted {
            add(&shared.lost, message.gap);
            shared.fence();
        }
        match parse(message, state, wall_ms()) {
            Ok(Input::Pet(event)) if event.intent != ControlIntent::Pet => {}
            Ok(input) => {
                add(&shared.received, 1);
                if !enqueue(sender, shared, input) {
                    return Ok(false);
                }
            }
            Err(_) => shared.reject(),
        }
    }
    for notice in subscriber.notices() {
        if matches!(notice, Notice::SourceGone | Notice::Late) {
            shared.fence();
        }
    }
    Ok(true)
}

fn subscriptions(root: &Path) -> io::Result<(Subscriber, Subscriber)> {
    let bus = Bus::new(root)?;
    let state = bus.subscribe_typed(ControlsState::TOPIC, SUBSCRIBER, ControlsState::TYPE)?;
    let pets = bus.subscribe_typed(ControlGesture::TOPIC, SUBSCRIBER, ControlGesture::TYPE)?;
    Ok((state, pets))
}

fn pump(root: PathBuf, sender: SyncSender<Queued>, shared: &Shared) {
    while !shared.stop.load(Ordering::Acquire) {
        shared.fence();
        if let Ok((mut state, mut pets)) = subscriptions(&root) {
            'connected: while !shared.stop.load(Ordering::Acquire) {
                for _ in 0..POLL_LIMIT / 2 {
                    for (subscriber, is_state) in [(&mut state, true), (&mut pets, false)] {
                        match read_one(subscriber, is_state, &sender, shared) {
                            Ok(true) => {}
                            Ok(false) => return,
                            Err(_) => break 'connected,
                        }
                    }
                }
                thread::park_timeout(PUMP_TICK);
            }
        }
        if !shared.stop.load(Ordering::Acquire) {
            add(&shared.reconnects, 1);
            shared.fence();
            thread::park_timeout(RECONNECT_AFTER);
        }
    }
}

#[cfg(test)]
mod tests;
