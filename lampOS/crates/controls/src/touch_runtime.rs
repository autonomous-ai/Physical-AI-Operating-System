//! Opt-in dual-source owner. GPIO is drained before the nonblocking strip
//! socket; no I2C operation, worker wait, recording write or hand log runs here.
use crate::{
    engine::{Config, Engine},
    input::{InputError, Observation, PinReader},
    runtime::{Counts, owner_lock},
    strip_worker::StripReader,
    touch::{TouchEngine, TouchOutput},
    touch_pins::TouchPinsConfig,
    touch_replay::{Input, Record},
};
use lamp_bus::{Bus, Message, Published, Publisher};
use lamp_msgs::{
    ControlFault, ControlGesture, ControlsState, Diagnostic, Hand, Level, TouchEvent, TouchState,
};
use std::{fs::File, io, path::Path};

pub const DRAIN_LIMIT: usize = 32;
pub const STATE_EVERY_US: u64 = 20_000;
/// How often the body's `body/hand` is looked at. The body says it with
/// every step it passes on, every 80 ms.
pub const HAND_EVERY_US: u64 = 20_000;

/// Whether the body feels the head pushed by hand now (#711): a fresh
/// `body/hand` from the body that says so. Missing, expired or from anyone
/// else is not pushed, so a stopped body never silences the head.
pub fn pushed_now(message: Option<&Message>, now_unix_ms: u64) -> bool {
    message.is_some_and(|message| {
        !message.expired
            && !message.header.expired_at(now_unix_ms)
            && message.header.source == "body"
            && message.parse::<Hand>().is_ok_and(|hand| hand.pushed)
    })
}

/// The body's hand, read from the bus at most every [`HAND_EVERY_US`].
struct HandIn {
    bus: Bus,
    looked_us: Option<u64>,
    pushed: bool,
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

pub struct Service<G: PinReader, S: StripReader> {
    gpio: G,
    pub strip: S,
    wiring: TouchPinsConfig,
    engine: TouchEngine,
    primary: Option<Primary>,
    state: Publisher,
    events: Publisher,
    diagnostic: Publisher,
    last_state_us: Option<u64>,
    last_report_us: Option<u64>,
    pub counts: Counts,
    pub gpio_accepted: u64,
    pub strip_accepted: u64,
    last_strip_error: Option<String>,
    last_gpio_error: Option<String>,
    recorder: Option<crate::record::Recorder>,
    hand: HandIn,
    _lock: File,
}

struct Primary {
    engine: Engine,
    state: Publisher,
    gestures: Publisher,
    name: String,
}

impl<G: PinReader, S: StripReader> Service<G, S> {
    pub fn new(
        bus: &Bus,
        config: Config,
        wiring: TouchPinsConfig,
        gpio: G,
        strip: S,
        clock: &str,
    ) -> io::Result<Self> {
        let touch = wiring.check(&config).map_err(io::Error::other)?;
        let lock = owner_lock(bus.root())?;
        let state =
            bus.publisher_declared(TouchState::TOPIC, &TouchState::declaration(), "controls")?;
        let engine = TouchEngine::new(touch, clock, state.boot()).map_err(io::Error::other)?;
        // Preserve the installed Voice receiver's primary-button behavior in
        // touch mode. Head/base/reset are never sent to this legacy engine.
        let primary = if let Some(pin) = wiring.buttons.first() {
            let state = bus.publisher_declared(
                ControlsState::TOPIC,
                &ControlsState::declaration(),
                "controls",
            )?;
            Some(Primary {
                engine: Engine::new(
                    Config {
                        enabled: config.enabled,
                        buttons: vec![pin.name.clone()],
                        head_pads: vec![],
                        base_selected_mask: None,
                    },
                    clock,
                    state.boot(),
                )
                .map_err(io::Error::other)?,
                state,
                gestures: bus.publisher_declared(
                    ControlGesture::TOPIC,
                    &ControlGesture::declaration(),
                    "controls",
                )?,
                name: pin.name.clone(),
            })
        } else {
            None
        };
        Ok(Self {
            gpio,
            strip,
            wiring,
            engine,
            primary,
            state,
            events: bus.publisher_declared(
                TouchEvent::TOPIC,
                &TouchEvent::declaration(),
                "controls",
            )?,
            diagnostic: bus.publisher_declared(
                Diagnostic::TOPIC,
                &Diagnostic::declaration(),
                "controls",
            )?,
            last_state_us: None,
            last_report_us: None,
            counts: Counts::default(),
            gpio_accepted: 0,
            strip_accepted: 0,
            last_strip_error: None,
            last_gpio_error: None,
            recorder: None,
            hand: HandIn {
                bus: bus.clone(),
                looked_us: None,
                pushed: false,
            },
            _lock: lock,
        })
    }

    /// Before the head's samples: a touch that begins while the head is
    /// pushed asks nothing from its first sample.
    fn look_at_hand(&mut self, now_us: u64) {
        if self
            .hand
            .looked_us
            .is_some_and(|at| now_us.saturating_sub(at) < HAND_EVERY_US)
        {
            return;
        }
        self.hand.looked_us = Some(now_us);
        let latest = self.hand.bus.latest(Hand::TOPIC).ok().flatten();
        let pushed = pushed_now(latest.as_ref(), unix_ms());
        if pushed != self.hand.pushed {
            self.hand.pushed = pushed;
            self.record(now_us, Input::HeadPushed { pushed });
            self.engine.set_head_pushed(pushed);
        }
    }

    pub fn record_to(&mut self, path: &Path) -> io::Result<()> {
        if self.recorder.is_some() {
            return Err(io::Error::other("touch recorder is already attached"));
        }
        self.recorder = Some(crate::record::Recorder::create_touch(path)?);
        Ok(())
    }

    pub fn finish_recording(&mut self) -> Option<crate::record::RecordStats> {
        self.recorder.take().map(crate::record::Recorder::finish)
    }

    fn record(&mut self, now_us: u64, input: Input) {
        if let Some(recorder) = &mut self.recorder {
            recorder.submit_touch(&Record { now_us, input });
        }
    }

    fn gpio_fault(&mut self, now_us: u64, fault: ControlFault) {
        self.record(now_us, Input::GpioFault { fault });
        self.engine.gpio_fault(now_us, fault);
        if let Some(primary) = &mut self.primary {
            primary.engine.input_fault(now_us, fault);
        }
    }

    fn gpio_sample(&mut self, now_us: u64, sample: Observation) -> Result<(), InputError> {
        let gpio = match self.wiring.gpio_sample(sample.clone()) {
            Ok(gpio) => gpio,
            Err(error) => {
                self.gpio_fault(now_us, error.fault);
                return Err(error);
            }
        };
        self.record(
            now_us,
            Input::Gpio {
                sample: gpio.clone(),
            },
        );
        // Validate the complete acquisition before allowing the legacy primary
        // owner to retain a Stop or mute from any part of this observation.
        if let Err(error) = self.engine.ingest_gpio(now_us, gpio) {
            if let Some(primary) = &mut self.primary {
                primary.engine.input_fault(now_us, error.fault);
            }
            return Err(error);
        }
        if let Some(primary) = &mut self.primary {
            let mut button = sample.clone();
            button.buttons.retain(|level| level.name == primary.name);
            button.head_pads.clear();
            button.base_mask = None;
            if let Err(error) = primary.engine.ingest(now_us, button) {
                self.record(now_us, Input::GpioFault { fault: error.fault });
                self.engine.gpio_fault(now_us, error.fault);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Failure to spawn the optional I2C child leaves healthy GPIO running.
    /// No automatic replacement worker is started after any strip failure.
    pub fn strip_unavailable(&mut self, now_us: u64, message: &str) {
        self.strip_error(now_us, InputError::new(ControlFault::Missing, message));
    }

    fn strip_error(&mut self, now_us: u64, error: InputError) {
        self.counts.reader_errors += 1;
        self.last_strip_error = Some(error_detail(&error));
        self.record(now_us, Input::StripFault { fault: error.fault });
        self.engine.strip_fault(now_us, error.fault);
    }

    pub fn step(&mut self, now: impl Fn() -> u64) -> TouchOutput {
        let began = now();
        self.look_at_hand(began);
        let mut drained = 0;
        for _ in 0..DRAIN_LIMIT {
            match self.gpio.read(now()) {
                Ok(Some(sample)) => {
                    drained += 1;
                    match self.gpio_sample(now(), sample) {
                        Ok(()) => {
                            self.counts.accepted += 1;
                            self.gpio_accepted += 1;
                        }
                        Err(error) => {
                            self.counts.rejected += 1;
                            self.last_gpio_error = Some(error_detail(&error));
                        }
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    self.counts.reader_errors += 1;
                    self.last_gpio_error = Some(error_detail(&error));
                    self.gpio_fault(now(), error.fault);
                    break;
                }
            }
        }
        if drained == DRAIN_LIMIT {
            self.counts.drain_overflows += 1;
            self.gpio_fault(now(), ControlFault::Overflow);
        }
        let mut drained = 0;
        for _ in 0..DRAIN_LIMIT {
            match self.strip.read(now()) {
                Ok(Some(sample)) => {
                    drained += 1;
                    let received_us = now();
                    self.record(
                        received_us,
                        Input::Strip {
                            sample: sample.clone(),
                        },
                    );
                    match self.engine.ingest_strip(received_us, sample) {
                        Ok(()) => {
                            self.counts.accepted += 1;
                            self.strip_accepted += 1;
                        }
                        Err(_) => self.counts.rejected += 1,
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let received_us = now();
                    self.strip_error(received_us, error);
                    break;
                }
            }
        }
        if drained == DRAIN_LIMIT {
            self.counts.drain_overflows += 1;
            let received_us = now();
            self.record(
                received_us,
                Input::StripFault {
                    fault: ControlFault::Overflow,
                },
            );
            self.engine.strip_fault(received_us, ControlFault::Overflow);
        }
        let at_us = now();
        let output = self.engine.tick(at_us);
        let publish_state = !output.events.is_empty()
            || self
                .last_state_us
                .is_none_or(|last| at_us.saturating_sub(last) >= STATE_EVERY_US);
        if publish_state {
            account(
                &mut self.counts,
                self.state.publish_json(at_us, &output.state),
            );
            self.last_state_us = Some(at_us);
        }
        for event in &output.events {
            account(&mut self.counts, self.events.publish_json(at_us, event));
        }
        if let Some(primary) = &mut self.primary {
            let output = primary.engine.tick(at_us);
            if publish_state || !output.gestures.is_empty() {
                account(
                    &mut self.counts,
                    primary.state.publish_json(at_us, &output.state),
                );
            }
            for gesture in &output.gestures {
                account(
                    &mut self.counts,
                    primary.gestures.publish_json(at_us, gesture),
                );
            }
        }
        if self
            .last_report_us
            .is_none_or(|last| at_us.saturating_sub(last) >= 1_000_000)
        {
            if self.events.alive().is_err() {
                self.counts.publish_errors += 1;
            }
            if let Some(primary) = &mut self.primary
                && primary.gestures.alive().is_err()
            {
                self.counts.publish_errors += 1;
            }
            let ready = output.state.sources.iter().all(|source| source.ready);
            let recording = self
                .recorder
                .as_ref()
                .map(crate::record::Recorder::stats)
                .unwrap_or_default();
            let report = Diagnostic::new(
                if ready && self.counts.publish_errors == 0 && !recording.failed {
                    Level::Ok
                } else {
                    Level::Error
                },
                "controls",
                if ready {
                    "touch evidence published; owner application is not verified"
                } else {
                    "a configured touch source is unavailable; other sources continue"
                },
            )
            .with("touch_enabled", output.state.enabled)
            .with("gpio_accepted", self.gpio_accepted)
            .with("strip_accepted", self.strip_accepted)
            .with(
                "last_strip_error",
                self.last_strip_error.as_deref().unwrap_or("none"),
            )
            .with(
                "last_gpio_error",
                self.last_gpio_error.as_deref().unwrap_or("none"),
            )
            .with("rejected", self.counts.rejected)
            .with("reader_errors", self.counts.reader_errors)
            .with("publish_errors", self.counts.publish_errors)
            .with("dropped", self.counts.dropped)
            .with("longest_step_us", self.counts.longest_step_us)
            .with("record_dropped", recording.dropped)
            .with("record_failed", recording.failed)
            .with("physical_stop_verified", false)
            .with("capture_permission", "not_owned");
            account(
                &mut self.counts,
                self.diagnostic.publish_json(at_us, &report),
            );
            self.last_report_us = Some(at_us);
        }
        self.counts.steps += 1;
        self.counts.longest_step_us = self.counts.longest_step_us.max(now().saturating_sub(began));
        output
    }
}

fn account(counts: &mut Counts, result: io::Result<Published>) {
    match result {
        Ok(published) => {
            counts.published += 1;
            counts.dropped += published.dropped as u64;
        }
        Err(_) => counts.publish_errors += 1,
    }
}

fn error_detail(error: &InputError) -> String {
    error
        .to_string()
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect()
}
