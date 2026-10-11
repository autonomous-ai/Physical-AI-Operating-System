//! Scripted audio devices for tests of the boundaries and of the worker
//! loops built on them. Nothing here is used by the runtime, and nothing here
//! behaves like a codec, a clock or a room: a test decides what the device
//! holds and when it fails.
use crate::{
    capture::{CaptureConfig, CaptureFormat},
    device::{
        CaptureDevice, CaptureStatus, ConfigureError, DeviceFault, DeviceState, PlaybackDevice,
        PlaybackOpenError, errno,
    },
    pcm::PcmFormat,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

struct CaptureScript {
    format: CaptureFormat,
    state: DeviceState,
    /// Samples the next reads will return, oldest first.
    queued: VecDeque<i16>,
    /// One-shot failures by call name: open, configure, start, read, status,
    /// discard.
    faults: Vec<(&'static str, i32)>,
    /// How long the next read blocks before it returns.
    stall: Option<Duration>,
    opens: u32,
    starts: u32,
    discards: u32,
    reads: u64,
}

/// A test's hold on one scripted microphone. The device itself is opened by
/// name through [`CaptureDevice::open`], as the boundary opens any device.
#[derive(Clone)]
pub struct CaptureControl(Arc<Mutex<CaptureScript>>);

static CAPTURES: Mutex<BTreeMap<String, CaptureControl>> = Mutex::new(BTreeMap::new());

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl CaptureControl {
    /// Make `name` openable. Names are process-wide; use one per test.
    pub fn register(name: &str) -> Self {
        let control = Self(Arc::new(Mutex::new(CaptureScript {
            format: CaptureFormat {
                rate: 16_000,
                channels: 1,
                period_frames: 160,
                buffer_frames: 640,
                monotonic_status_timestamps: false,
            },
            state: DeviceState::Setup,
            queued: VecDeque::new(),
            faults: Vec::new(),
            stall: None,
            opens: 0,
            starts: 0,
            discards: 0,
            reads: 0,
        })));
        locked(&CAPTURES).insert(name.to_owned(), control.clone());
        control
    }
    fn script(&self) -> MutexGuard<'_, CaptureScript> {
        locked(&self.0)
    }
    /// What `configure` will report as installed.
    pub fn set_format(&self, format: CaptureFormat) {
        self.script().format = format;
    }
    /// Samples the microphone has "captured" and not yet handed over.
    pub fn push(&self, samples: &[i16]) {
        self.script().queued.extend(samples);
    }
    /// The next call of this name fails once with `errno`, one of
    /// [`crate::device::errno`].
    pub fn fail_next(&self, call: &'static str, errno: i32) {
        self.script().faults.push((call, errno));
    }
    /// The stream state reported by `status` from now on.
    pub fn set_state(&self, state: DeviceState) {
        self.script().state = state;
    }
    /// The next read blocks for `duration`, as a stuck driver call does.
    pub fn stall_next_read(&self, duration: Duration) {
        self.script().stall = Some(duration);
    }
    pub fn queued(&self) -> usize {
        self.script().queued.len()
    }
    /// How often the device was opened, started and discarded, and how many
    /// reads it has served.
    pub fn counts(&self) -> (u32, u32, u32, u64) {
        let script = self.script();
        (script.opens, script.starts, script.discards, script.reads)
    }
}

pub struct ScriptedCapture(CaptureControl);

impl ScriptedCapture {
    fn failing(&self, call: &'static str) -> Result<(), DeviceFault> {
        let mut script = self.0.script();
        match script.faults.iter().position(|(name, _)| *name == call) {
            Some(index) => {
                let (_, errno) = script.faults.remove(index);
                Err(DeviceFault::new(call, errno))
            }
            None => Ok(()),
        }
    }
}

impl CaptureDevice for ScriptedCapture {
    fn open(name: &str) -> Result<Self, DeviceFault> {
        let control = locked(&CAPTURES)
            .get(name)
            .cloned()
            .ok_or(DeviceFault::new("open", errno::NOENT))?;
        let device = Self(control);
        device.failing("open")?;
        let mut script = device.0.script();
        script.opens += 1;
        script.state = DeviceState::Setup;
        drop(script);
        Ok(device)
    }

    fn configure(&mut self, _config: CaptureConfig) -> Result<CaptureFormat, ConfigureError> {
        self.failing("configure").map_err(ConfigureError::Fault)?;
        let mut script = self.0.script();
        script.state = DeviceState::Prepared;
        Ok(script.format)
    }

    fn start(&mut self) -> Result<(), DeviceFault> {
        self.failing("start")?;
        let mut script = self.0.script();
        script.starts += 1;
        script.state = DeviceState::Running;
        Ok(())
    }

    fn read(&mut self, into: &mut [i16]) -> Result<usize, DeviceFault> {
        let stall = self.0.script().stall.take();
        if let Some(stall) = stall {
            std::thread::sleep(stall);
        }
        self.failing("read")?;
        let mut script = self.0.script();
        script.reads += 1;
        if script.queued.is_empty() {
            return Err(DeviceFault::new("read", errno::AGAIN));
        }
        let frames = into.len().min(script.queued.len());
        for (target, sample) in into.iter_mut().zip(script.queued.drain(..frames)) {
            *target = sample;
        }
        Ok(frames)
    }

    fn status(&mut self) -> Result<CaptureStatus, DeviceFault> {
        self.failing("status")?;
        let script = self.0.script();
        Ok(CaptureStatus {
            state: script.state,
            monotonic_us: None,
            available: script.queued.len() as i64,
            delayed: script.queued.len() as i64,
        })
    }

    fn discard(&mut self) -> Result<(), DeviceFault> {
        let mut script = self.0.script();
        script.discards += 1;
        script.queued.clear();
        script.state = DeviceState::Setup;
        drop(script);
        self.failing("discard")
    }
}

struct PlaybackScript {
    format: PcmFormat,
    state: DeviceState,
    clock_mode: bool,
    /// Frames accepted and not yet played.
    queued: usize,
    /// Everything the device accepted, in order. This, and nothing that was
    /// only offered, is what an echo reference may be built from.
    accepted: Vec<i16>,
    /// The most a single write takes, however much room there is.
    accept_at_most: Option<usize>,
    /// One-shot failures by call name: open, avail_delay, start, write,
    /// drain, prepare, discard.
    faults: Vec<(&'static str, i32)>,
    opens: u32,
    starts: u32,
    discards: u32,
    prepares: u32,
}

/// A test's hold on one scripted speaker. Nothing plays by itself: a test
/// says how many frames the device has played out.
#[derive(Clone)]
pub struct PlaybackControl(Arc<Mutex<PlaybackScript>>);

static PLAYBACKS: Mutex<BTreeMap<String, PlaybackControl>> = Mutex::new(BTreeMap::new());

/// How often a scripted speaker was opened, started, discarded and prepared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackCounts {
    pub opens: u32,
    pub starts: u32,
    pub discards: u32,
    pub prepares: u32,
}

impl PlaybackControl {
    /// Make `name` openable. Names are process-wide; use one per test.
    pub fn register(name: &str) -> Self {
        let control = Self(Arc::new(Mutex::new(PlaybackScript {
            format: PcmFormat {
                rate: 24_000,
                period_frames: 240,
                buffer_frames: 960,
            },
            state: DeviceState::Setup,
            clock_mode: false,
            queued: 0,
            accepted: Vec::new(),
            accept_at_most: None,
            faults: Vec::new(),
            opens: 0,
            starts: 0,
            discards: 0,
            prepares: 0,
        })));
        locked(&PLAYBACKS).insert(name.to_owned(), control.clone());
        control
    }
    fn script(&self) -> MutexGuard<'_, PlaybackScript> {
        locked(&self.0)
    }
    /// What `open` will report as installed.
    pub fn set_format(&self, format: PcmFormat) {
        self.script().format = format;
    }
    /// The next call of this name fails once with `errno`, one of
    /// [`crate::device::errno`].
    pub fn fail_next(&self, call: &'static str, errno: i32) {
        self.script().faults.push((call, errno));
    }
    /// The device has played out this many of the frames it was holding.
    pub fn play(&self, frames: usize) {
        let mut script = self.script();
        script.queued = script.queued.saturating_sub(frames);
    }
    /// From now on one write takes at most this many frames.
    pub fn accept_at_most(&self, frames: Option<usize>) {
        self.script().accept_at_most = frames;
    }
    pub fn set_state(&self, state: DeviceState) {
        self.script().state = state;
    }
    pub fn state(&self) -> DeviceState {
        self.script().state
    }
    pub fn queued(&self) -> usize {
        self.script().queued
    }
    /// Every sample the device accepted so far, in order.
    pub fn accepted(&self) -> Vec<i16> {
        self.script().accepted.clone()
    }
    pub fn counts(&self) -> PlaybackCounts {
        let script = self.script();
        PlaybackCounts {
            opens: script.opens,
            starts: script.starts,
            discards: script.discards,
            prepares: script.prepares,
        }
    }
}

pub struct ScriptedPlayback(PlaybackControl);

impl ScriptedPlayback {
    fn failing(&self, call: &'static str) -> Result<(), DeviceFault> {
        let mut script = self.0.script();
        match script.faults.iter().position(|(name, _)| *name == call) {
            Some(index) => {
                let (_, errno) = script.faults.remove(index);
                Err(DeviceFault::new(call, errno))
            }
            None => Ok(()),
        }
    }
}

impl PlaybackDevice for ScriptedPlayback {
    fn open(name: &str, clock_mode: bool) -> Result<(Self, PcmFormat), PlaybackOpenError> {
        let control = locked(&PLAYBACKS)
            .get(name)
            .cloned()
            .ok_or(PlaybackOpenError::Fault(DeviceFault::new(
                "open",
                errno::NOENT,
            )))?;
        let device = Self(control);
        device.failing("open").map_err(PlaybackOpenError::Fault)?;
        let mut script = device.0.script();
        script.opens += 1;
        script.clock_mode = clock_mode;
        script.state = DeviceState::Prepared;
        let format = script.format;
        drop(script);
        Ok((device, format))
    }

    fn avail_delay(&mut self) -> Result<(i64, i64), DeviceFault> {
        self.failing("avail_delay")?;
        let script = self.0.script();
        let room = (script.format.buffer_frames as usize).saturating_sub(script.queued);
        Ok((room as i64, script.queued as i64))
    }

    fn start(&mut self) -> Result<(), DeviceFault> {
        self.failing("start")?;
        let mut script = self.0.script();
        script.starts += 1;
        script.state = DeviceState::Running;
        Ok(())
    }

    fn write(&mut self, samples: &[i16]) -> Result<usize, DeviceFault> {
        self.failing("write")?;
        let mut script = self.0.script();
        let room = (script.format.buffer_frames as usize).saturating_sub(script.queued);
        let frames = samples
            .len()
            .min(room)
            .min(script.accept_at_most.unwrap_or(usize::MAX));
        if frames == 0 {
            return Err(DeviceFault::new("write", errno::AGAIN));
        }
        script.accepted.extend_from_slice(&samples[..frames]);
        script.queued += frames;
        // Outside clock mode a device starts by itself once written to.
        if !script.clock_mode && script.state == DeviceState::Prepared {
            script.state = DeviceState::Running;
        }
        Ok(frames)
    }

    fn drain(&mut self) -> Result<(), DeviceFault> {
        self.failing("drain")?;
        let mut script = self.0.script();
        if script.queued > 0 {
            return Err(DeviceFault::new("drain", errno::AGAIN));
        }
        script.state = DeviceState::Setup;
        Ok(())
    }

    fn prepare(&mut self) -> Result<(), DeviceFault> {
        self.failing("prepare")?;
        let mut script = self.0.script();
        script.prepares += 1;
        script.state = DeviceState::Prepared;
        Ok(())
    }

    fn discard(&mut self) -> Result<(), DeviceFault> {
        let mut script = self.0.script();
        script.discards += 1;
        script.queued = 0;
        script.state = DeviceState::Setup;
        drop(script);
        self.failing("discard")
    }

    fn state(&self) -> DeviceState {
        self.0.script().state
    }
}
