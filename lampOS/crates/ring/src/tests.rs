use super::*;
use lamp_turn_owner::{
    AdmissionState, AdmittedInput, CaptureState, Controller, Permission, TurnOwner,
};
use std::cell::Cell;

thread_local! {
    static TIME: Cell<u64> = const { Cell::new(10) };
}

fn at(us: u64) -> MonoTime {
    MonoTime::from_micros(us)
}
fn clock() -> MonoTime {
    at(TIME.get())
}
fn advance_to(us: u64) {
    TIME.set(us);
}
fn boot(id: u8) -> BootId {
    BootId::new([id; 16]).unwrap()
}
fn ceiling(value: u16) -> ChannelCeiling {
    ChannelCeiling::new(value).unwrap()
}

#[derive(Default)]
struct Sink {
    frames: Vec<EncodedFrame>,
    failures: usize,
    delay_us: u64,
}

impl FrameSink for Sink {
    fn write_frame(&mut self, frame: &EncodedFrame) -> io::Result<()> {
        self.frames.push(frame.clone());
        TIME.set(TIME.get() + self.delay_us);
        if self.failures > 0 {
            self.failures -= 1;
            return Err(io::Error::other("injected output fault"));
        }
        Ok(())
    }
}

fn ready() -> Controller {
    let mut controller = Controller::new(boot(1), at(0));
    controller
        .set_microphone_permission(at(0), Permission::Allowed)
        .unwrap();
    controller
        .set_capture(at(0), CaptureState::RetainingUntil(at(1_000_000)))
        .unwrap();
    controller
        .set_admission(at(0), AdmissionState::OpenUntil(at(1_000_000)))
        .unwrap();
    controller
}

fn permit(
    controller: &mut Controller,
    owner: TurnOwner,
    kind: OutputKind,
    lease_us: u64,
) -> OutputPermit {
    let plan = controller.plan_output(clock(), owner, kind, None).unwrap();
    controller.issue_output(clock(), plan, lease_us).unwrap()
}

fn setup() -> (GuardedRing<Sink>, Controller, TurnOwner, OutputPermit) {
    advance_to(10);
    let mut controller = ready();
    let owner = controller.admit(clock(), AdmittedInput::NewTurn).unwrap();
    controller.input_ended(clock(), owner).unwrap();
    let permit = permit(&mut controller, owner, OutputKind::Light, 250_000);
    let snapshot = controller.snapshot(clock()).unwrap();
    let mut ring = GuardedRing::with_clock(Sink::default(), boot(1), clock).unwrap();
    ring.install_authority(snapshot).unwrap();
    (ring, controller, owner, permit)
}

fn paint(ring: &mut GuardedRing<Sink>, permit: OutputPermit) -> Result<WriteReceipt, Error> {
    ring.show(&[Rgb::new(9, 25, 60); PIXEL_COUNT], ceiling(120), permit)
}

fn is_black(frame: &EncodedFrame) -> bool {
    *frame == EncodedFrame::black()
}

fn decode_channel(bits: &[u8]) -> u8 {
    bits.iter().fold(0, |byte, bit| {
        assert!(matches!(bit, 0xC0 | 0xFC));
        (byte << 1) | u8::from(*bit == 0xFC)
    })
}

#[test]
fn all_channel_values_encode_each_bit_in_wire_order() {
    for value in 0..=u8::MAX {
        let encoded = encode_byte(value);
        assert_eq!(decode_channel(&encoded), value);
        for (index, symbol) in encoded.iter().enumerate() {
            let expected = if value & (1 << (7 - index)) != 0 {
                0xFC
            } else {
                0xC0
            };
            assert_eq!(*symbol, expected);
        }
    }
}

#[test]
fn every_pixel_has_grb_order_and_low_primer_reset() {
    assert_eq!(FRAME_BYTES, 1028);
    for value in 0..=u8::MAX {
        for channel in 0..3 {
            let mut color = Rgb::BLACK;
            match channel {
                0 => color.red = value,
                1 => color.green = value,
                _ => color.blue = value,
            }
            let frame = encode(&[color; PIXEL_COUNT], ceiling(120));
            assert!(frame.bytes()[..PRIMER_BYTES].iter().all(|byte| *byte == 0));
            assert!(
                frame.bytes()[FRAME_BYTES - RESET_BYTES..]
                    .iter()
                    .all(|byte| *byte == 0)
            );
            for pixel in frame.bytes()[PRIMER_BYTES..FRAME_BYTES - RESET_BYTES].chunks_exact(24) {
                let actual = [
                    decode_channel(&pixel[8..16]),
                    decode_channel(&pixel[..8]),
                    decode_channel(&pixel[16..]),
                ];
                let mut expected = [0; 3];
                expected[channel] = value.min(120);
                assert_eq!(actual, expected);
            }
        }
    }
}

#[test]
fn ceiling_preserves_ratios_to_integer_quantization_and_never_increases_channels() {
    for limit in 0..=120 {
        for peak in 1..=u8::MAX {
            for lower in 0..=peak {
                let before = Rgb::new(peak, lower, 0);
                let after = before.capped(ceiling(limit));
                assert_eq!(after.red, peak.min(limit as u8));
                assert_eq!(after.blue, 0);
                assert!(after.green <= lower && after.green <= limit as u8);
                if peak <= limit as u8 {
                    assert_eq!(before, after);
                } else {
                    let quantization_error = (i32::from(after.green) * i32::from(peak)
                        - i32::from(lower) * i32::from(limit))
                    .abs();
                    assert!(quantization_error * 2 <= i32::from(peak));
                }
            }
        }
    }
    assert_eq!(Rgb::BLACK.capped(ceiling(0)), Rgb::BLACK);
    assert_eq!(
        Rgb::new(255, 127, 0).capped(ceiling(40)),
        Rgb::new(40, 20, 0)
    );
}

#[test]
fn every_out_of_range_ceiling_is_rejected() {
    for invalid in 121..=u16::MAX {
        assert_eq!(ChannelCeiling::new(invalid), Err(InvalidCeiling(invalid)));
    }
    assert_eq!(ceiling(0).get(), 0);
    assert_eq!(ceiling(120).get(), 120);
}

#[test]
fn startup_black_does_not_create_authority_or_readiness() {
    let (_, _, _, permit) = setup();
    let mut ring = GuardedRing::with_clock(Sink::default(), boot(1), clock).unwrap();
    assert_eq!(ring.sink.frames.len(), 1);
    assert!(is_black(&ring.sink.frames[0]));
    assert!(matches!(
        paint(&mut ring, permit).unwrap_err().cause,
        Failure::Authority(lamp_turn_owner::Error::NoAuthority)
    ));
    assert_eq!(ring.sink.frames.len(), 1);
}

#[test]
fn only_light_permits_can_paint_and_wrong_kind_cannot_erase_current_cue() {
    let (mut ring, mut controller, owner, light) = setup();
    let speech = permit(&mut controller, owner, OutputKind::Speech, 250_000);
    paint(&mut ring, light).unwrap();
    let count = ring.sink.frames.len();
    assert!(matches!(
        paint(&mut ring, speech).unwrap_err().cause,
        Failure::Authority(lamp_turn_owner::Error::WrongOutputKind)
    ));
    assert_eq!(ring.sink.frames.len(), count);
    assert!(!is_black(ring.sink.frames.last().unwrap()));
}

#[test]
fn turn_revocation_blanks_without_another_paint_and_old_packets_cannot_erase_new_cue() {
    let (mut ring, mut controller, _, old) = setup();
    let old_state = controller.snapshot(clock()).unwrap();
    ring.install_authority(old_state).unwrap();
    paint(&mut ring, old).unwrap();
    advance_to(20);
    let owner = controller
        .admit(clock(), AdmittedInput::Interruption)
        .unwrap();
    let fresh = permit(&mut controller, owner, OutputKind::Light, 250_000);
    let revoked = ring
        .install_authority(controller.snapshot(clock()).unwrap())
        .unwrap();
    assert!(matches!(
        revoked.unwrap().reason,
        BlankReason::AuthorityLost(_)
    ));
    assert!(is_black(ring.sink.frames.last().unwrap()));
    paint(&mut ring, fresh).unwrap();
    let count = ring.sink.frames.len();
    assert!(paint(&mut ring, old).is_err());
    assert!(ring.install_authority(old_state).is_err());
    assert_eq!(ring.sink.frames.len(), count);
    assert!(!is_black(ring.sink.frames.last().unwrap()));
}

#[test]
fn privacy_revocation_blanks_and_reopening_cannot_reuse_old_permit() {
    let (mut ring, mut controller, _, light) = setup();
    paint(&mut ring, light).unwrap();
    advance_to(20);
    controller
        .set_microphone_permission(clock(), Permission::Denied)
        .unwrap();
    assert!(
        ring.install_authority(controller.snapshot(clock()).unwrap())
            .unwrap()
            .is_some()
    );
    assert!(is_black(ring.sink.frames.last().unwrap()));
    advance_to(30);
    controller
        .set_microphone_permission(clock(), Permission::Allowed)
        .unwrap();
    ring.install_authority(controller.snapshot(clock()).unwrap())
        .unwrap();
    assert!(paint(&mut ring, light).is_err());
    assert!(is_black(ring.sink.frames.last().unwrap()));
}

#[test]
fn expiry_is_checked_on_idle_tick_and_blanks_only_once() {
    let (mut ring, _, _, light) = setup();
    paint(&mut ring, light).unwrap();
    advance_to(250_010);
    let blank = ring.tick().unwrap().unwrap();
    assert_eq!(
        blank.reason,
        BlankReason::AuthorityLost(lamp_turn_owner::Error::ExpiredState)
    );
    assert!(is_black(ring.sink.frames.last().unwrap()));
    let count = ring.sink.frames.len();
    assert!(ring.tick().unwrap().is_none());
    assert_eq!(ring.sink.frames.len(), count);
}

#[test]
fn expiry_during_a_write_is_detected_and_blanked_on_return() {
    let (mut ring, mut controller, owner, _) = setup();
    let short = permit(&mut controller, owner, OutputKind::Light, 5);
    ring.sink.delay_us = 6;
    assert!(matches!(
        paint(&mut ring, short).unwrap_err().cause,
        Failure::Authority(lamp_turn_owner::Error::ExpiredPermit)
    ));
    assert_eq!(ring.sink.frames.len(), 3);
    assert!(is_black(ring.sink.frames.last().unwrap()));
}

#[test]
fn transport_loss_latches_fault_and_only_attempts_one_black_frame() {
    let (mut ring, _, _, light) = setup();
    paint(&mut ring, light).unwrap();
    assert_eq!(
        ring.control_lost().unwrap().reason,
        BlankReason::ControlLost
    );
    assert!(ring.is_faulted());
    let count = ring.sink.frames.len();
    assert!(matches!(
        ring.control_lost().unwrap_err().cause,
        Failure::Faulted
    ));
    assert!(matches!(
        paint(&mut ring, light).unwrap_err().cause,
        Failure::Faulted
    ));
    assert_eq!(ring.sink.frames.len(), count);
}

#[test]
fn primary_and_cleanup_errors_are_both_reported_and_not_retried() {
    let (mut ring, _, _, light) = setup();
    ring.sink.failures = 2;
    let error = paint(&mut ring, light).unwrap_err();
    assert!(matches!(error.cause, Failure::Write(WriteFailure::Io(_))));
    assert!(matches!(error.cleanup_error, Some(WriteFailure::Io(_))));
    assert_eq!(ring.sink.frames.len(), 3);
    assert!(ring.is_faulted());
    assert!(paint(&mut ring, light).is_err());
    assert_eq!(ring.sink.frames.len(), 3);
}

#[test]
fn one_late_write_is_a_late_frame_not_a_fault() {
    // As lamp-4ace's ring at 03:26:20 on 16 Oct: one write returned after
    // 15.2 ms while the SD/MMC host re-tuned (#911). The frame went out.
    let (mut ring, _, _, light) = setup();
    ring.sink.delay_us = 15_197;
    paint(&mut ring, light).unwrap();
    assert!(!ring.is_faulted());
    assert_eq!(ring.late_writes(), 1);
    ring.sink.delay_us = 0;
    paint(&mut ring, light).unwrap();
    assert!(!ring.is_faulted());
    assert_eq!(ring.late_writes(), 1);
}

#[test]
fn late_writes_lasting_the_late_run_are_an_observed_fault_not_a_claimed_timeout() {
    let (mut ring, _, _, _) = setup();
    ring.sink.delay_us = 15_000;
    let frame = EncodedFrame::black();
    let mut kept = 0u64;
    let failure = loop {
        match ring.transfer(&frame, clock()) {
            Ok(_) => kept += 1,
            Err(failure) => break failure,
        }
    };
    assert!(matches!(failure, WriteFailure::OverBudget { .. }));
    // About two seconds of late writes were kept as frames first.
    assert_eq!(kept, LATE_RUN_US / 15_000);
    assert_eq!(ring.late_writes(), kept);
}

#[test]
fn a_dark_gap_between_late_writes_is_not_a_late_run() {
    // A late write, then five seconds of closed privacy with no writes, then
    // one late write on reopening: two isolated late frames, not 5 s of them.
    let (mut ring, mut controller) = idle();
    ring.sink.delay_us = 15_000;
    rest(&mut ring).unwrap();
    controller
        .set_microphone_permission(clock(), Permission::Denied)
        .unwrap();
    ring.install_authority(controller.snapshot(clock()).unwrap())
        .unwrap()
        .unwrap();
    let count = ring.sink.frames.len();
    for step in 1..=50u64 {
        advance_to(100_000 * step);
        assert!(
            ring.install_authority(controller.snapshot(clock()).unwrap())
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(ring.sink.frames.len(), count);
    let now = clock();
    controller
        .set_microphone_permission(now, Permission::Allowed)
        .unwrap();
    controller
        .set_capture(
            now,
            CaptureState::RetainingUntil(at(now.as_micros() + 1_000_000)),
        )
        .unwrap();
    controller
        .set_admission(
            now,
            AdmissionState::OpenUntil(at(now.as_micros() + 1_000_000)),
        )
        .unwrap();
    ring.install_authority(controller.snapshot(clock()).unwrap())
        .unwrap();
    rest(&mut ring).unwrap();
    assert!(!ring.is_faulted());
    assert_eq!(ring.late_writes(), 3);
}

#[test]
fn monotonic_regression_faults_and_blanks_an_active_cue() {
    let (mut ring, _, _, light) = setup();
    paint(&mut ring, light).unwrap();
    advance_to(9);
    assert!(ring.tick().unwrap().is_some());
    assert!(ring.is_faulted());
    assert!(is_black(ring.sink.frames.last().unwrap()));
    let count = ring.sink.frames.len();
    assert!(ring.tick().is_err());
    assert_eq!(ring.sink.frames.len(), count);
}

#[test]
fn shutdown_is_explicit_idempotent_and_permanently_rejects_paints() {
    let (mut ring, _, _, light) = setup();
    paint(&mut ring, light).unwrap();
    assert_eq!(
        ring.shutdown().unwrap().unwrap().reason,
        BlankReason::Shutdown
    );
    assert!(ring.is_stopped());
    assert!(!ring.is_faulted());
    let count = ring.sink.frames.len();
    assert!(ring.shutdown().unwrap().is_none());
    assert!(matches!(
        paint(&mut ring, light).unwrap_err().cause,
        Failure::Stopped
    ));
    assert_eq!(ring.sink.frames.len(), count);
}

#[test]
fn partial_frame_write_is_reported_without_sending_the_remainder() {
    struct ShortWriter {
        calls: usize,
    }
    impl io::Write for ShortWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            assert_eq!(bytes.len(), FRAME_BYTES);
            Ok(FRAME_BYTES - 1)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("unexpected flush")
        }
    }
    let mut writer = ShortWriter { calls: 0 };
    assert_eq!(
        write_once(&mut writer, &EncodedFrame::black())
            .unwrap_err()
            .kind(),
        io::ErrorKind::WriteZero
    );
    assert_eq!(writer.calls, 1);
}

// ---- The resting frame: light that belongs to no turn ----------------------

fn rest(ring: &mut GuardedRing<Sink>) -> Option<WriteReceipt> {
    ring.show_resting(&[Rgb::new(6, 4, 2); PIXEL_COUNT], ceiling(40))
        .unwrap()
}

/// A ring whose authority says: microphone permitted, input ready, no owner.
fn idle() -> (GuardedRing<Sink>, Controller) {
    advance_to(10);
    let mut controller = ready();
    let mut ring = GuardedRing::with_clock(Sink::default(), boot(1), clock).unwrap();
    ring.install_authority(controller.snapshot(clock()).unwrap())
        .unwrap();
    (ring, controller)
}

#[test]
fn a_resting_frame_needs_fresh_authority_a_permitted_microphone_and_no_owner() {
    advance_to(10);
    let mut ring = GuardedRing::with_clock(Sink::default(), boot(1), clock).unwrap();
    // Nothing installed yet: only the startup black frame is ever written.
    assert!(rest(&mut ring).is_none());
    assert_eq!(ring.sink.frames.len(), 1);

    let (mut ring, _) = idle();
    assert!(rest(&mut ring).is_some());
    assert!(!is_black(ring.sink.frames.last().unwrap()));

    // A turn owns the lamp: no new resting frame is painted.
    let (mut ring, _, _, _) = setup();
    let count = ring.sink.frames.len();
    assert!(rest(&mut ring).is_none());
    assert_eq!(ring.sink.frames.len(), count);

    // The microphone is not permitted: none either.
    advance_to(10);
    let mut controller = ready();
    controller
        .set_microphone_permission(clock(), Permission::Denied)
        .unwrap();
    let mut ring = GuardedRing::with_clock(Sink::default(), boot(1), clock).unwrap();
    ring.install_authority(controller.snapshot(clock()).unwrap())
        .unwrap();
    assert!(rest(&mut ring).is_none());
    assert_eq!(ring.sink.frames.len(), 1);
}

#[test]
fn closing_privacy_blanks_a_resting_frame_at_once() {
    let (mut ring, mut controller) = idle();
    rest(&mut ring).unwrap();
    advance_to(20);
    controller
        .set_microphone_permission(clock(), Permission::Denied)
        .unwrap();
    let blank = ring
        .install_authority(controller.snapshot(clock()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        blank.reason,
        BlankReason::AuthorityLost(lamp_turn_owner::Error::PrivacyClosed)
    );
    assert!(is_black(ring.sink.frames.last().unwrap()));
    // And it stays dark: no resting frame while privacy is closed.
    let count = ring.sink.frames.len();
    assert!(rest(&mut ring).is_none());
    assert!(ring.tick().unwrap().is_none());
    assert_eq!(ring.sink.frames.len(), count);
}

#[test]
fn a_resting_frame_does_not_outlive_its_authority() {
    let (mut ring, _) = idle();
    rest(&mut ring).unwrap();
    advance_to(100_000);
    assert!(ring.tick().unwrap().is_none());
    advance_to(250_010);
    let blank = ring.tick().unwrap().unwrap();
    assert_eq!(
        blank.reason,
        BlankReason::AuthorityLost(lamp_turn_owner::Error::ExpiredState)
    );
    assert!(is_black(ring.sink.frames.last().unwrap()));
    assert!(rest(&mut ring).is_none());
}

#[test]
fn a_cue_is_painted_over_a_resting_frame_with_no_dark_frame_between() {
    let (mut ring, mut controller) = idle();
    rest(&mut ring).unwrap();
    advance_to(20);
    let owner = controller.admit(clock(), AdmittedInput::NewTurn).unwrap();
    let light = permit(&mut controller, owner, OutputKind::Light, 100_000);
    // The owner appears: the resting frame stays where it is.
    assert!(
        ring.install_authority(controller.snapshot(clock()).unwrap())
            .unwrap()
            .is_none()
    );
    let before = ring.sink.frames.len();
    // No new resting frame under an owner, and the cue goes straight over it.
    assert!(rest(&mut ring).is_none());
    paint(&mut ring, light).unwrap();
    assert_eq!(ring.sink.frames.len(), before + 1);
    assert!(
        ring.sink
            .frames
            .iter()
            .skip(1)
            .all(|frame| !is_black(frame))
    );
    // From here it is a cue like any other: its own permit ends it.
    assert!(rest(&mut ring).is_none());
    advance_to(100_030);
    let blank = ring.tick().unwrap().unwrap();
    assert!(matches!(blank.reason, BlankReason::AuthorityLost(_)));
}

#[test]
fn a_resting_frame_is_not_a_cue_and_a_stop_still_ends_everything() {
    let (mut ring, _) = idle();
    rest(&mut ring).unwrap();
    assert!(ring.shutdown().unwrap().is_some());
    assert!(is_black(ring.sink.frames.last().unwrap()));
    assert!(
        ring.show_resting(&[Rgb::new(6, 4, 2); PIXEL_COUNT], ceiling(40))
            .is_err()
    );
}
