use lamp_bus::Bus;
use lamp_controls::{
    engine::Config,
    input::{InputError, Level as PinLevel, Observation, PinReader},
    mpr121,
    mute::MuteRecord,
    pins::{Pin, Pull},
    receiver::Receiver,
    strip_worker::StripReader,
    touch::{StripSample, TouchOutput},
    touch_pins::TouchPinsConfig,
    touch_receiver::TouchReceiver,
    touch_runtime::{DRAIN_LIMIT, Service},
};
use lamp_msgs::{
    ClockStamp, ControlFault, ControlsState, Diagnostic, Level, TouchGesture, TouchIntent,
    TouchSource, TouchSourceState, TouchState,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const CLOCK: &str = "touch-runtime-test";

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = if Path::new("/private/tmp").is_dir() {
            PathBuf::from("/private/tmp")
        } else {
            fs::canonicalize(std::env::temp_dir()).unwrap()
        };
        let path = parent.join(format!(
            "ctr-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Default)]
struct Gpio {
    values: Rc<RefCell<VecDeque<Result<Observation, InputError>>>>,
    reads: Rc<Cell<usize>>,
}
impl PinReader for Gpio {
    fn read(&mut self, _: u64) -> Result<Option<Observation>, InputError> {
        self.reads.set(self.reads.get() + 1);
        self.values.borrow_mut().pop_front().transpose()
    }
}

#[derive(Clone, Default)]
struct Strip {
    values: Rc<RefCell<VecDeque<Result<StripSample, InputError>>>>,
    reads: Rc<Cell<usize>>,
}
impl StripReader for Strip {
    fn read(&mut self, _: u64) -> Result<Option<StripSample>, InputError> {
        self.reads.set(self.reads.get() + 1);
        self.values.borrow_mut().pop_front().transpose()
    }
}

// Explicit authored host profile. No GPIO/I2C is opened by these tests, and
// these values are not a recommended electrical calibration.
fn base() -> mpr121::Config {
    serde_json::from_value(serde_json::json!({
        "bus":0,"address":90,"every_ms":10,"pads":[11,10,9,8,7,6,5,4,3,2,1,0],
        "profile":{
            "touch_thresholds":([12;12]),"release_thresholds":([6;12]),
            "baseline":{
                "rising":{"mhd":1,"nhd":1,"ncl":14,"fdl":0},
                "falling":{"mhd":1,"nhd":1,"ncl":255,"fdl":2},
                "touched":{"nhd":0,"ncl":0,"fdl":0}
            },
            "ffi":34,"sfi":10,"esi_ms":1,"global_cdc":16,"global_cdt":1,
            "cdc":([0;12]),"cdt":([0;12]),"debounce_touch":2,"debounce_release":2,
            "ecr":143,"autoconfig":{"mode":"disabled"}
        }
    }))
    .unwrap()
}

fn wiring() -> TouchPinsConfig {
    let pin = |name: &str, line| Pin {
        name: name.into(),
        line,
        active_low: true,
        pull: Pull::Up,
    };
    TouchPinsConfig {
        chip: "/dev/gpiochip0".into(),
        buttons: vec![pin("primary", 99)],
        head_pads: vec![pin("head", 37)],
        reset: Some(pin("reset", 100)),
        every_ms: 10,
        base: Some(base()),
        head_qualify_ms: None,
        head_tap_min_ms: None,
        head_gap_ms: None,
    }
}

fn config(enabled: bool) -> Config {
    Config {
        enabled,
        buttons: vec!["primary".into()],
        head_pads: vec!["head".into()],
        base_selected_mask: Some(4095),
    }
}

struct Rig {
    service: Service<Gpio, Strip>,
    bus: Bus,
    gpio: Gpio,
    strip: Strip,
    gpio_sequence: u64,
    strip_sequence: u64,
    _root: Root,
}

impl Rig {
    fn new(enabled: bool) -> Self {
        let root = Root::new();
        let bus = Bus::new(&root.0).unwrap().rescan_every(Duration::ZERO);
        let gpio = Gpio::default();
        let strip = Strip::default();
        let service = Service::new(
            &bus,
            config(enabled),
            wiring(),
            gpio.clone(),
            strip.clone(),
            CLOCK,
        )
        .unwrap();
        Self {
            service,
            bus,
            gpio,
            strip,
            gpio_sequence: 0,
            strip_sequence: 0,
            _root: root,
        }
    }

    fn gpio_sample(&mut self, at: u64, primary: bool, reset: bool, head: bool) -> Observation {
        self.gpio_sequence += 1;
        Observation {
            stamp: ClockStamp {
                clock_id: CLOCK.into(),
                at_us: at,
                sequence: self.gpio_sequence,
            },
            session_id: "gpio-reader".into(),
            buttons: vec![
                PinLevel {
                    name: "primary".into(),
                    active: primary,
                },
                PinLevel {
                    name: "reset".into(),
                    active: reset,
                },
            ],
            head_pads: vec![PinLevel {
                name: "head".into(),
                active: head,
            }],
            base_mask: None,
        }
    }

    fn gpio(&mut self, at: u64, primary: bool, reset: bool, head: bool) {
        let sample = self.gpio_sample(at, primary, reset, head);
        self.gpio.values.borrow_mut().push_back(Ok(sample));
    }

    fn strip(&mut self, at: u64, mask: u16) {
        self.strip_sequence += 1;
        self.strip.values.borrow_mut().push_back(Ok(StripSample {
            stamp: ClockStamp {
                clock_id: CLOCK.into(),
                at_us: at,
                sequence: self.strip_sequence,
            },
            session_id: "strip-reader".into(),
            mask,
        }));
    }

    fn step(&mut self, at: u64) -> TouchOutput {
        let output = self.service.step(|| at);
        output.state.check().unwrap();
        for event in &output.events {
            event.check().unwrap();
        }
        output
    }

    fn legacy(&self) -> ControlsState {
        let state: ControlsState = self
            .bus
            .latest(ControlsState::TOPIC)
            .unwrap()
            .unwrap()
            .parse()
            .unwrap();
        state.check().unwrap();
        state
    }
}

fn source(output: &TouchOutput, source: TouchSource) -> &TouchSourceState {
    output
        .state
        .sources
        .iter()
        .find(|state| state.source == source)
        .unwrap()
}

fn diagnostic_value<'a>(report: &'a Diagnostic, key: &str) -> &'a str {
    &report
        .values
        .iter()
        .find(|value| value.key == key)
        .unwrap()
        .value
}

#[test]
fn rejected_complete_gpio_cannot_retain_a_primary_stop_or_long_hold_mute() {
    for release_at in [60_000, 2_040_000] {
        let mut rig = Rig::new(true);
        rig.gpio(0, false, false, false);
        rig.step(0);
        for at in (20_000..release_at).step_by(20_000) {
            rig.gpio(at, true, false, false);
            assert!(rig.step(at).events.is_empty());
        }
        let mut bad = rig.gpio_sample(release_at, false, false, false);
        bad.head_pads[0].name = "unexpected-pad".into();
        // Structurally valid, but inconsistent with the complete configured input.
        bad.check().unwrap();
        rig.gpio.values.borrow_mut().push_back(Ok(bad));
        let output = rig.step(release_at);
        assert!(output.events.is_empty());
        assert_eq!(output.state.event_sequence, 0);
        assert_eq!(output.state.rejected_inputs, 1);
        assert_eq!(
            source(&output, TouchSource::Primary).fault,
            Some(ControlFault::Invalid)
        );
        let state = rig.legacy();
        assert!(!state.ready);
        assert!(state.latest_stop.is_none() && state.latest_mute.is_none());
        assert_eq!(state.event_sequence, 0);
        assert_eq!(state.rejected_inputs, 1);
        let mut owner = Receiver::new(MuteRecord::new(CLOCK).unwrap()).unwrap();
        let effect = owner.observe(release_at, &state).unwrap();
        assert!(!effect.stop && !effect.software_muted);
        assert_eq!(rig.service.counts.rejected, 1);
        assert_eq!(rig.service.counts.publish_errors, 0);
    }
}

#[test]
fn strip_startup_failure_leaves_primary_gpio_and_legacy_stop_working() {
    let mut rig = Rig::new(true);
    let mut reports = rig
        .bus
        .subscribe_typed(Diagnostic::TOPIC, "test", Diagnostic::TYPE)
        .unwrap();
    rig.service
        .strip_unavailable(0, &format!("spawn failed\n{}", "x".repeat(400)));
    for (at, primary) in [(0, false), (20_000, true), (40_000, false)] {
        rig.gpio(at, primary, false, false);
        let output = rig.step(at);
        assert!(source(&output, TouchSource::Primary).ready);
        assert_eq!(
            source(&output, TouchSource::Strip).fault,
            Some(ControlFault::Missing)
        );
        if at == 40_000 {
            assert_eq!(output.events.len(), 1);
            assert_eq!(output.events[0].when_speaking, TouchIntent::Stop);
            assert_eq!(output.events[0].source, TouchSource::Primary);
        }
    }
    let state = rig.legacy();
    assert!(state.ready && state.latest_stop.is_some());
    let mut owner = Receiver::new(MuteRecord::new(CLOCK).unwrap()).unwrap();
    assert!(owner.observe(40_000, &state).unwrap().stop);
    assert_eq!(rig.service.counts.reader_errors, 1);
    assert_eq!(rig.service.gpio_accepted, 3);
    assert_eq!(rig.service.strip_accepted, 0);
    let report: Diagnostic = reports
        .recv(Duration::from_millis(100))
        .unwrap()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(report.level, Level::Error);
    assert_eq!(diagnostic_value(&report, "physical_stop_verified"), "false");
    assert_eq!(diagnostic_value(&report, "capture_permission"), "not_owned");
    assert!(diagnostic_value(&report, "last_strip_error").contains("spawn failed"));
    assert!(!diagnostic_value(&report, "last_strip_error").contains('\n'));
}

#[test]
fn strip_reader_failure_withdraws_strip_contact_without_losing_primary() {
    let mut rig = Rig::new(true);
    for at in (0..=80_000).step_by(20_000) {
        rig.gpio(at, false, false, false);
        rig.strip(at, if at < 40_000 { 0 } else { 7 });
        rig.step(at);
    }
    rig.strip
        .values
        .borrow_mut()
        .push_back(Err(InputError::new(ControlFault::Missing, "I2C lost")));
    rig.gpio(100_000, true, false, false);
    let output = rig.step(100_000);
    let strip = source(&output, TouchSource::Strip);
    assert!(!strip.ready && strip.active.is_none() && strip.contact_id.is_none());
    assert!(source(&output, TouchSource::Primary).ready);
    assert!(
        output
            .state
            .latest
            .iter()
            .all(|event| event.source != TouchSource::Strip)
    );
    rig.gpio(120_000, false, false, false);
    let output = rig.step(120_000);
    assert!(
        output
            .events
            .iter()
            .any(|event| event.source == TouchSource::Primary
                && event.when_speaking == TouchIntent::Stop)
    );
    assert!(rig.legacy().latest_stop.is_some());
    assert_eq!(rig.service.counts.reader_errors, 1);
}

#[test]
fn gpio_reader_failure_does_not_cancel_fresh_strip_touch() {
    let mut rig = Rig::new(true);
    for at in [0, 20_000] {
        rig.gpio(at, false, false, false);
        rig.strip(at, 0);
        rig.step(at);
    }
    rig.gpio
        .values
        .borrow_mut()
        .push_back(Err(InputError::new(ControlFault::Missing, "GPIO lost")));
    let mut strip_touches = 0;
    for at in [40_000, 60_000, 80_000] {
        rig.strip(at, 7);
        let output = rig.step(at);
        assert_eq!(
            source(&output, TouchSource::Primary).fault,
            Some(ControlFault::Missing)
        );
        assert!(source(&output, TouchSource::Strip).ready);
        strip_touches += output
            .events
            .iter()
            .filter(|event| {
                event.source == TouchSource::Strip && event.gesture == TouchGesture::Touch
            })
            .count();
    }
    assert_eq!(strip_touches, 1);
    assert!(!rig.legacy().ready);
    assert!(rig.legacy().latest_stop.is_none());
}

#[test]
fn one_source_never_renews_the_other_sources_acquisition_time() {
    for live_strip in [false, true] {
        let mut rig = Rig::new(true);
        rig.gpio(0, false, false, false);
        rig.strip(0, 0);
        rig.step(0);
        let mut last = None;
        for at in (20_000..=100_000).step_by(20_000) {
            if live_strip {
                rig.strip(at, 0);
            } else {
                rig.gpio(at, false, false, false);
            }
            last = Some(rig.step(at));
        }
        let output = last.unwrap();
        let (live, stale) = if live_strip {
            (TouchSource::Strip, TouchSource::Primary)
        } else {
            (TouchSource::Primary, TouchSource::Strip)
        };
        assert!(source(&output, live).ready);
        let stale = source(&output, stale);
        assert!(!stale.ready && stale.active.is_none());
        assert_eq!(stale.observed_us, Some(0));
        assert_eq!(stale.fault, Some(ControlFault::Stale));
    }
}

#[test]
fn stale_arrival_never_makes_a_fresh_source_fail_or_restamps_old_data() {
    let mut rig = Rig::new(true);
    rig.gpio(100_000, false, false, false);
    rig.strip(0, 7);
    let output = rig.step(100_000);
    assert!(source(&output, TouchSource::Primary).ready);
    assert_eq!(
        source(&output, TouchSource::Strip).fault,
        Some(ControlFault::Stale)
    );
    assert_eq!(source(&output, TouchSource::Strip).observed_us, None);
    assert!(output.events.is_empty());
    assert_eq!(rig.service.counts.rejected, 1);
    assert_eq!(rig.service.gpio_accepted, 1);
    assert_eq!(rig.service.strip_accepted, 0);
}

#[test]
fn draining_a_source_is_bounded_and_overflow_faults_only_that_source() {
    for overflow_strip in [false, true] {
        let mut rig = Rig::new(true);
        let now = 1_000_000;
        for offset in 0..DRAIN_LIMIT {
            let at = now - (DRAIN_LIMIT - offset - 1) as u64;
            if overflow_strip {
                rig.strip(at, 0);
            } else {
                rig.gpio(at, false, false, false);
            }
        }
        if overflow_strip {
            rig.gpio(now, false, false, false);
        } else {
            rig.strip(now, 0);
        }
        let output = rig.step(now);
        let (failed, healthy, reads) = if overflow_strip {
            (
                TouchSource::Strip,
                TouchSource::Primary,
                rig.strip.reads.get(),
            )
        } else {
            (
                TouchSource::Primary,
                TouchSource::Strip,
                rig.gpio.reads.get(),
            )
        };
        assert_eq!(reads, DRAIN_LIMIT);
        assert_eq!(source(&output, failed).fault, Some(ControlFault::Overflow));
        assert!(source(&output, healthy).ready);
        assert!(output.events.is_empty());
        assert_eq!(rig.service.counts.drain_overflows, 1);
        assert_eq!(rig.service.counts.accepted, DRAIN_LIMIT as u64 + 1);
    }
}

#[test]
fn shadow_state_and_events_are_observable_but_both_receivers_refuse_authority() {
    let mut rig = Rig::new(false);
    for (at, primary) in [(0, false), (20_000, true), (40_000, false)] {
        rig.gpio(at, primary, false, false);
        rig.strip(at, 0);
        let output = rig.step(at);
        assert!(!output.state.enabled);
        if at == 40_000 {
            assert_eq!(output.events.len(), 1);
            assert!(!output.events[0].enabled);
            assert!(
                TouchReceiver::new(CLOCK)
                    .unwrap()
                    .observe(at, &output.state)
                    .is_err()
            );
        }
    }
    let state = rig.legacy();
    assert!(state.ready && !state.enabled && state.latest_stop.is_some());
    let mut owner = Receiver::new(MuteRecord::new(CLOCK).unwrap()).unwrap();
    assert!(owner.observe(40_000, &state).is_err());
    assert!(!owner.mute_record().muted);
    let advertised: TouchState = rig
        .bus
        .latest(TouchState::TOPIC)
        .unwrap()
        .unwrap()
        .parse()
        .unwrap();
    assert!(!advertised.enabled);
    assert_eq!(rig.service.counts.publish_errors, 0);
}

#[test]
fn head_and_reset_roles_never_enter_legacy_primary_retention() {
    let mut rig = Rig::new(true);
    for at in (0..=500_000).step_by(50_000) {
        rig.gpio(at, false, false, false);
        rig.step(at);
    }
    let mut head_events = Vec::new();
    for at in [520_000, 540_000] {
        rig.gpio(at, false, false, true);
        head_events.extend(rig.step(at).events);
    }
    assert!(
        head_events
            .iter()
            .any(|event| event.source == TouchSource::Head && event.gesture == TouchGesture::Touch)
    );
    for at in (560_000..=5_560_000).step_by(50_000) {
        rig.gpio(at, false, true, false);
        rig.step(at);
    }
    rig.gpio(5_580_000, false, false, false);
    let output = rig.step(5_580_000);
    assert_eq!(output.events.len(), 1);
    assert_eq!(output.events[0].source, TouchSource::Reset);
    assert_eq!(output.events[0].when_speaking, TouchIntent::FactoryReset);
    let state = rig.legacy();
    assert!(state.latest_stop.is_none() && state.latest_mute.is_none());
    assert_eq!(state.event_sequence, 0);
}

#[test]
fn missing_all_inputs_never_advertises_a_released_or_ready_hand() {
    let mut rig = Rig::new(true);
    let output = rig.step(1_000_000);
    assert!(output.events.is_empty());
    for source in &output.state.sources {
        assert!(!source.ready && !source.armed);
        assert_eq!(source.fault, Some(ControlFault::Missing));
        assert!(source.active.is_none() && source.observed_us.is_none());
    }
    assert!(!rig.legacy().ready);
    assert_eq!(rig.service.counts.accepted, 0);
}

#[test]
fn a_second_touch_owner_cannot_publish_into_the_same_bus_root() {
    let rig = Rig::new(true);
    let result = Service::new(
        &rig.bus,
        config(true),
        wiring(),
        Gpio::default(),
        Strip::default(),
        CLOCK,
    );
    assert!(result.is_err());
}
