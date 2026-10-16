//! Explicit boundary-clock checks over real calls to the scripted capture device.
//! Time changes belong to the fixture or a completed scripted device operation;
//! the injected getter only reads the currently admitted value.
use super::*;
use crate::testing::{CaptureControl, ScriptedCapture};
use lamp_turn_owner::Controller;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Open,
    Configure,
    Start,
    Read,
    Status,
    Discard,
}

struct Plan {
    at: Arc<AtomicU64>,
    after: RefCell<[Option<u64>; 6]>,
    calls: RefCell<Vec<Call>>,
    driver_time: Option<u64>,
}

impl Plan {
    fn completed(&self, call: Call) {
        self.calls.borrow_mut().push(call);
        if let Some(at) = self.after.borrow_mut()[call as usize].take() {
            self.at.store(at, Ordering::SeqCst);
        }
    }
}

thread_local! {
    static PLANS: RefCell<BTreeMap<String, Rc<Plan>>> = const { RefCell::new(BTreeMap::new()) };
}
static NEXT_NAME: AtomicU64 = AtomicU64::new(1);

struct Device {
    inner: ScriptedCapture,
    plan: Rc<Plan>,
}

impl CaptureDevice for Device {
    fn open(name: &str) -> std::result::Result<Self, DeviceFault> {
        let plan = PLANS.with(|plans| plans.borrow().get(name).unwrap().clone());
        let inner = ScriptedCapture::open(name)?;
        plan.completed(Call::Open);
        Ok(Self { inner, plan })
    }

    fn configure(
        &mut self,
        config: CaptureConfig,
    ) -> std::result::Result<CaptureFormat, ConfigureError> {
        let result = self.inner.configure(config);
        self.plan.completed(Call::Configure);
        result
    }

    fn start(&mut self) -> std::result::Result<(), DeviceFault> {
        let result = self.inner.start();
        self.plan.completed(Call::Start);
        result
    }

    fn read(&mut self, into: &mut [i16]) -> std::result::Result<usize, DeviceFault> {
        let result = self.inner.read(into);
        self.plan.completed(Call::Read);
        result
    }

    fn status(&mut self) -> std::result::Result<crate::device::CaptureStatus, DeviceFault> {
        let mut result = self.inner.status();
        self.plan.completed(Call::Status);
        if let Ok(status) = result.as_mut() {
            status.monotonic_us = self.plan.driver_time;
        }
        result
    }

    fn discard(&mut self) -> std::result::Result<(), DeviceFault> {
        let result = self.inner.discard();
        self.plan.completed(Call::Discard);
        result
    }
}

type Clock = Box<dyn Fn() -> MonoTime>;
type Clocked = MicrophoneOn<Device, Clock>;

struct Fixture {
    name: String,
    control: CaptureControl,
    plan: Rc<Plan>,
}

impl Fixture {
    fn new(at: u64, driver_time: Option<u64>) -> Self {
        let name = format!(
            "capture-clock:{}",
            NEXT_NAME.fetch_add(1, Ordering::Relaxed)
        );
        let control = CaptureControl::register(&name);
        control.set_format(CaptureFormat {
            rate: 16_000,
            channels: 1,
            period_frames: 160,
            buffer_frames: 640,
            monotonic_status_timestamps: driver_time.is_some(),
        });
        let plan = Rc::new(Plan {
            at: Arc::new(AtomicU64::new(at)),
            after: RefCell::new([None; 6]),
            calls: RefCell::new(Vec::new()),
            driver_time,
        });
        PLANS.with(|plans| {
            assert!(
                plans
                    .borrow_mut()
                    .insert(name.clone(), plan.clone())
                    .is_none()
            );
        });
        Self {
            name,
            control,
            plan,
        }
    }

    fn now(&self) -> MonoTime {
        MonoTime::from_micros(self.plan.at.load(Ordering::SeqCst))
    }

    fn set(&self, at: u64) {
        self.plan.at.store(at, Ordering::SeqCst);
    }

    fn after(&self, call: Call, at: u64) {
        self.plan.after.borrow_mut()[call as usize] = Some(at);
    }

    fn clock(&self) -> Clock {
        let at = self.plan.at.clone();
        Box::new(move || MonoTime::from_micros(at.load(Ordering::SeqCst)))
    }

    fn permitted(&self) -> (Clocked, Controller) {
        let boot = BootId::new([106; 16]).unwrap();
        let mut controller = Controller::new(boot, self.now());
        controller
            .set_microphone_permission(self.now(), Permission::Allowed)
            .unwrap();
        let mut microphone =
            MicrophoneOn::with_clock(&self.name, boot, CaptureConfig::default(), self.clock())
                .unwrap();
        microphone
            .install(controller.snapshot(self.now()).unwrap())
            .unwrap();
        (microphone, controller)
    }

    fn calls(&self) -> Vec<Call> {
        self.plan.calls.borrow().clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        PLANS.with(|plans| {
            plans.borrow_mut().remove(&self.name);
        });
    }
}

#[test]
fn ordinary_constructor_and_explicit_host_clock_preserve_host_times_and_lifecycle() {
    for explicit in [false, true] {
        let fixture = Fixture::new(17, None);
        let boot = BootId::new([107; 16]).unwrap();
        let before = now().as_micros();
        let mut controller = Controller::new(boot, now());
        controller
            .set_microphone_permission(now(), Permission::Allowed)
            .unwrap();
        let mut microphone: MicrophoneOn<Device> = if explicit {
            MicrophoneOn::with_clock(
                &fixture.name,
                boot,
                CaptureConfig::default(),
                now as fn() -> MonoTime,
            )
        } else {
            MicrophoneOn::new(&fixture.name, boot, CaptureConfig::default())
        }
        .unwrap();
        assert!(fixture.calls().is_empty(), "construction opens nothing");
        microphone
            .install(controller.snapshot(now()).unwrap())
            .unwrap();
        assert!(fixture.calls().is_empty(), "permission opens nothing");
        let started = microphone.start().unwrap();
        fixture.control.push(&[31; CAPTURE_SAMPLES]);
        let frame = microphone.try_read().unwrap().unwrap();
        microphone.revoke().unwrap();
        let after = now().as_micros();
        let stamps = [
            started.open_started_at_us,
            started.prepared_at_us,
            started.capture_started_at_us,
            started.start_completed_at_us,
            frame.timing.first_read_started_at_us,
            frame.timing.last_read_started_at_us,
            frame.timing.read_completed_at_us,
            frame.timing.status_observed_at_us,
        ];
        assert!(stamps.iter().all(|at| before <= *at && *at <= after));
        assert!(stamps.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(frame.samples, [31; CAPTURE_SAMPLES]);
        assert_eq!((frame.epoch, frame.sequence), (1, 1));
        assert_eq!(frame.timing.alsa_status_monotonic_us, None);
        assert_eq!(fixture.control.counts(), (1, 1, 1, 1));
        assert_eq!(
            fixture.calls(),
            [
                Call::Open,
                Call::Configure,
                Call::Start,
                Call::Read,
                Call::Status,
                Call::Discard
            ]
        );
        assert!(!microphone.is_open());
    }
}

#[test]
fn explicit_clock_is_instance_local_and_does_not_relabel_the_driver_clock() {
    let first = Fixture::new(1_000_000, Some(77));
    let second = Fixture::new(9_000_000, Some(88));
    let (mut a, _) = first.permitted();
    let (mut b, _) = second.permitted();
    let started_a = a.start().unwrap();
    let started_b = b.start().unwrap();
    assert_eq!(started_a.open_started_at_us, 1_000_000);
    assert_eq!(started_a.prepared_at_us, 1_000_000);
    assert_eq!(started_a.capture_started_at_us, 1_000_000);
    assert_eq!(started_a.start_completed_at_us, 1_000_000);
    assert_eq!(started_b.start_completed_at_us, 9_000_000);
    first.set(1_002_000);
    first.control.push(&[11; CAPTURE_SAMPLES]);
    second.control.push(&[22; CAPTURE_SAMPLES]);
    let frame_a = a.try_read().unwrap().unwrap();
    let frame_b = b.try_read().unwrap().unwrap();
    assert_eq!(frame_a.timing.first_read_started_at_us, 1_002_000);
    assert_eq!(frame_a.timing.last_read_started_at_us, 1_002_000);
    assert_eq!(frame_a.timing.read_completed_at_us, 1_002_000);
    assert_eq!(frame_a.timing.status_observed_at_us, 1_002_000);
    assert_eq!(frame_b.timing.read_completed_at_us, 9_000_000);
    assert_eq!(frame_a.timing.alsa_status_monotonic_us, Some(77));
    assert_eq!(frame_b.timing.alsa_status_monotonic_us, Some(88));
    assert_eq!(frame_a.samples, [11; CAPTURE_SAMPLES]);
    assert_eq!(frame_b.samples, [22; CAPTURE_SAMPLES]);
    a.revoke().unwrap();
    b.revoke().unwrap();
}

#[test]
fn invalid_device_name_is_rejected_before_clock_or_device_access() {
    let boot = BootId::new([108; 16]).unwrap();
    for name in ["", "bad\nname"] {
        let error =
            MicrophoneOn::<Device, _>::with_clock(name, boot, CaptureConfig::default(), || {
                panic!("invalid construction must not sample the clock")
            })
            .err()
            .expect("invalid alias");
        assert!(matches!(error.kind, ErrorKind::InvalidConfig(_)));
    }
}

#[test]
fn read_budget_accepts_exactly_ten_ms_and_refuses_one_more_without_late_audio() {
    for (elapsed, accepted) in [(MAX_READ_US, true), (MAX_READ_US + 1, false)] {
        let fixture = Fixture::new(1_000_000, None);
        let (mut microphone, _) = fixture.permitted();
        microphone.start().unwrap();
        fixture.control.push(&[43; CAPTURE_SAMPLES]);
        fixture.after(Call::Read, 1_000_000 + elapsed);
        let result = microphone.try_read();
        if accepted {
            let frame = result.unwrap().unwrap();
            assert_eq!(frame.samples, [43; CAPTURE_SAMPLES]);
            assert_eq!(frame.timing.read_completed_at_us, 1_000_000 + MAX_READ_US);
            assert_eq!(frame.timing.successful_reads, 1);
            microphone.revoke().unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().kind,
                ErrorKind::Discontinuity(Discontinuity::ReadBudgetExceeded)
            );
            assert!(!microphone.is_open());
            assert_eq!(microphone.partial.samples, [0; CAPTURE_SAMPLES]);
            assert!(!fixture.calls().contains(&Call::Status));
        }
        assert_eq!(fixture.control.counts(), (1, 1, 1, 1));
    }
}

#[test]
fn authority_expiring_during_read_wins_over_budget_and_never_reaches_status() {
    let fixture = Fixture::new(1_000_000, None);
    let (mut microphone, mut controller) = fixture.permitted();
    let expires = controller
        .snapshot(fixture.now())
        .unwrap()
        .expires_at()
        .as_micros();
    microphone.start().unwrap();
    fixture.control.push(&[57; CAPTURE_SAMPLES]);
    fixture.after(Call::Read, expires);
    assert_eq!(
        microphone.try_read().unwrap_err().kind,
        ErrorKind::Discontinuity(Discontinuity::AuthorityExpired)
    );
    assert_eq!(fixture.control.counts(), (1, 1, 1, 1));
    assert!(!fixture.calls().contains(&Call::Status));
    assert_eq!(microphone.partial.samples, [0; CAPTURE_SAMPLES]);
    assert!(!microphone.is_open());
}

#[test]
fn authority_expiry_after_prepare_start_or_status_cannot_publish_completion() {
    for call in [Call::Configure, Call::Start, Call::Status] {
        let fixture = Fixture::new(1_000_000, None);
        let (mut microphone, mut controller) = fixture.permitted();
        let expires = controller
            .snapshot(fixture.now())
            .unwrap()
            .expires_at()
            .as_micros();
        fixture.after(call, expires);
        let error = match call {
            Call::Configure => microphone.prepare().unwrap_err(),
            Call::Start => {
                microphone.prepare().unwrap();
                microphone.start_prepared().unwrap_err()
            }
            Call::Status => {
                microphone.start().unwrap();
                fixture.control.push(&[61; CAPTURE_SAMPLES]);
                microphone.try_read().unwrap_err()
            }
            _ => unreachable!(),
        };
        assert_eq!(
            error.kind,
            ErrorKind::Discontinuity(Discontinuity::AuthorityExpired)
        );
        assert!(!microphone.is_open());
        assert!(!microphone.is_running());
        assert_eq!(microphone.partial.samples, [0; CAPTURE_SAMPLES]);
        assert_eq!(fixture.calls().last(), Some(&Call::Discard));
        assert_eq!(fixture.control.counts().2, 1);
    }
}

#[test]
fn partial_deadline_keeps_its_first_read_across_would_block() {
    for (elapsed, accepted) in [(39_999, true), (40_000, false)] {
        let fixture = Fixture::new(1_000_000, None);
        let (mut microphone, _) = fixture.permitted();
        microphone.start().unwrap();
        fixture.control.push(&[7; 10]);
        assert!(microphone.try_read().unwrap().is_none());
        fixture.set(1_020_000);
        assert!(microphone.try_read().unwrap().is_none());
        assert_eq!(microphone.partial.first_read_started_at_us, 1_000_000);
        fixture.set(1_000_000 + elapsed);
        fixture.control.push(&[8; 150]);
        let result = microphone.try_read();
        if accepted {
            let frame = result.unwrap().unwrap();
            assert_eq!(&frame.samples[..10], &[7; 10]);
            assert_eq!(&frame.samples[10..], &[8; 150]);
            assert_eq!(frame.timing.first_read_started_at_us, 1_000_000);
            assert_eq!(frame.timing.read_completed_at_us, 1_039_999);
            assert_eq!(frame.timing.successful_reads, 2);
            assert_eq!(fixture.control.counts().3, 3);
            microphone.revoke().unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().kind,
                ErrorKind::Discontinuity(Discontinuity::PartialTimeout)
            );
            assert_eq!(
                fixture.control.counts().3,
                2,
                "expiry checked before the next read"
            );
            assert!(!microphone.is_open());
            assert_eq!(microphone.partial.samples, [0; CAPTURE_SAMPLES]);
        }
    }
}

#[test]
fn clock_regression_latches_closed_even_after_the_clock_moves_forward() {
    let fixture = Fixture::new(1_000_000, None);
    let (mut microphone, _) = fixture.permitted();
    microphone.start().unwrap();
    fixture.control.push(&[71; 10]);
    assert!(microphone.try_read().unwrap().is_none());
    fixture.set(999_999);
    assert_eq!(
        microphone.maintain().unwrap_err().kind,
        ErrorKind::Discontinuity(Discontinuity::ClockRegression)
    );
    assert!(!microphone.is_open());
    assert_eq!(microphone.partial.samples, [0; CAPTURE_SAMPLES]);
    fixture.set(1_000_001);
    assert_eq!(
        microphone.reset().unwrap_err().kind,
        ErrorKind::Authority(lamp_turn_owner::Error::Faulted)
    );
    assert_eq!(fixture.control.counts(), (1, 1, 1, 1));
}

#[test]
fn device_error_precedes_read_budget_but_never_precedes_expired_authority() {
    for expired in [false, true] {
        let fixture = Fixture::new(1_000_000, None);
        let (mut microphone, mut controller) = fixture.permitted();
        let expires = controller
            .snapshot(fixture.now())
            .unwrap()
            .expires_at()
            .as_micros();
        microphone.start().unwrap();
        fixture.control.fail_next("read", errno::NODEV);
        fixture.after(
            Call::Read,
            if expired {
                expires
            } else {
                1_000_000 + MAX_READ_US + 1
            },
        );
        let expected = if expired {
            Discontinuity::AuthorityExpired
        } else {
            Discontinuity::Disconnected
        };
        assert_eq!(
            microphone.try_read().unwrap_err().kind,
            ErrorKind::Discontinuity(expected)
        );
        assert!(!microphone.is_open());
        assert!(!fixture.calls().contains(&Call::Status));
        assert_eq!(fixture.control.counts().2, 1);
    }
}
