#![cfg(unix)]
//! The speaker boundary over a scripted device. Nothing here is a codec, a
//! clock or a room: the test says what the device accepted and played.
use lamp_audio::{
    RENDER_SAMPLES,
    device::{DeviceState, errno},
    pcm::{PcmFormat, SpeakerOn},
    testing::{PlaybackControl, ScriptedPlayback},
};
use lamp_interaction::{
    AdmissionState, AdmittedInput, BootId, CaptureState, Controller, MonoTime, OutputKind,
    OutputPermit, Permission, TurnOwner,
};
use lamp_ipc::monotonic_us;

type Speaker = SpeakerOn<ScriptedPlayback>;

fn now() -> MonoTime {
    MonoTime::from_micros(monotonic_us())
}
fn block(value: i16) -> [i16; RENDER_SAMPLES] {
    [value; RENDER_SAMPLES]
}
/// A controller with one admitted turn whose input has ended, and a permit
/// to speak its answer.
fn answering(boot: BootId) -> (Controller, TurnOwner, OutputPermit) {
    let mut controller = Controller::new(boot, now());
    controller
        .set_microphone_permission(now(), Permission::Allowed)
        .unwrap();
    let until = now().checked_add(500_000).unwrap();
    controller
        .set_capture(now(), CaptureState::RetainingUntil(until))
        .unwrap();
    controller
        .set_admission(now(), AdmissionState::OpenUntil(until))
        .unwrap();
    let owner = controller.admit(now(), AdmittedInput::NewTurn).unwrap();
    controller.input_ended(now(), owner).unwrap();
    let output = controller
        .plan_output(now(), owner, OutputKind::Speech, None)
        .unwrap();
    let permit = controller.issue_output(now(), output, 100_000).unwrap();
    (controller, owner, permit)
}
fn speaking(
    name: &str,
    id: u8,
) -> (
    Speaker,
    PlaybackControl,
    Controller,
    TurnOwner,
    OutputPermit,
) {
    let device = PlaybackControl::register(name);
    let boot = BootId::new([id; 16]).unwrap();
    let mut speaker = Speaker::open(name, boot).unwrap();
    let (mut controller, owner, permit) = answering(boot);
    speaker
        .install(controller.snapshot(now()).unwrap())
        .unwrap();
    (speaker, device, controller, owner, permit)
}

#[test]
fn only_what_the_device_accepted_is_written_and_a_full_device_is_not_a_fault() {
    let (mut speaker, device, _controller, _owner, permit) = speaking("scripted:room", 61);
    // The device takes 100 of 240 frames. That is all that was written, and
    // all an echo reference may be built from.
    device.accept_at_most(Some(100));
    let samples: [i16; RENDER_SAMPLES] = std::array::from_fn(|index| index as i16);
    let first = speaker.try_write(permit, &samples).unwrap();
    assert_eq!(first.frames, 100);
    assert_eq!(device.accepted(), samples[..100]);
    assert_eq!((first.first_sample.frame, first.end_sample.frame), (0, 100));
    device.accept_at_most(None);
    let rest = speaker.try_write(permit, &samples[100..]).unwrap();
    assert_eq!(rest.frames, 140);
    assert_eq!(rest.first_sample, first.end_sample);
    assert_eq!(device.accepted(), samples);
    // Fill the 960-frame queue. Nothing is played, so nothing more fits.
    for _ in 0..3 {
        assert_eq!(
            speaker.try_write(permit, &block(7)).unwrap().frames,
            RENDER_SAMPLES
        );
    }
    assert_eq!(device.queued(), 960);
    assert_eq!(speaker.try_write(permit, &block(8)).unwrap().frames, 0);
    assert_eq!(device.accepted().len(), 960, "nothing was accepted twice");
    // The device plays one block out and has room for one again.
    device.play(RENDER_SAMPLES);
    let status = speaker.poll().unwrap();
    assert_eq!(status.queued_frames, 720);
    assert_eq!(
        speaker.try_write(permit, &block(8)).unwrap().frames,
        RENDER_SAMPLES
    );
}

#[test]
fn cancellation_discards_what_the_device_still_holds() {
    let (mut speaker, device, mut controller, owner, permit) = speaking("scripted:cancel", 62);
    assert_eq!(
        speaker.try_write(permit, &block(100)).unwrap().frames,
        RENDER_SAMPLES
    );
    assert_eq!(device.queued(), RENDER_SAMPLES);
    assert!(speaker.is_active());
    controller.cancel_turn(now(), owner).unwrap();
    speaker
        .install(controller.snapshot(now()).unwrap())
        .unwrap();
    // Dropped at the device, not left to play out.
    assert_eq!(device.queued(), 0);
    assert_eq!(device.counts().discards, 1);
    assert!(!speaker.is_active());
    assert!(speaker.try_write(permit, &block(100)).is_err());
    assert_eq!(device.accepted().len(), RENDER_SAMPLES);
}

#[test]
fn an_underrun_or_a_vanished_speaker_is_a_fault_and_nothing_is_replayed() {
    for (index, (call, code)) in [
        ("write", errno::PIPE),
        ("write", errno::NODEV),
        ("avail_delay", errno::NODEV),
    ]
    .into_iter()
    .enumerate()
    {
        let (mut speaker, device, mut controller, _owner, permit) =
            speaking(&format!("scripted:fault-{index}"), 63);
        assert_eq!(
            speaker.try_write(permit, &block(5)).unwrap().frames,
            RENDER_SAMPLES
        );
        device.fail_next(call, code);
        assert!(speaker.try_write(permit, &block(6)).is_err(), "{call}");
        // What was queued is discarded, and the boundary is latched closed:
        // the same permit and a fresh snapshot open nothing.
        assert_eq!(device.queued(), 0);
        assert_eq!(device.counts().discards, 1);
        assert!(
            speaker
                .install(controller.snapshot(now()).unwrap())
                .is_err()
        );
        assert!(speaker.try_write(permit, &block(6)).is_err());
        assert_eq!(device.accepted(), block(5), "block 6 never reached it");
    }
}

#[test]
fn a_discard_that_fails_latches_the_boundary_closed() {
    let (mut speaker, device, mut controller, _owner, permit) = speaking("scripted:discard", 64);
    speaker.try_write(permit, &block(5)).unwrap();
    device.fail_next("discard", errno::IO);
    // A failed discard is not confirmation that queued speech stopped.
    assert!(speaker.stop().is_err());
    assert!(
        speaker
            .install(controller.snapshot(now()).unwrap())
            .is_err()
    );
    assert!(speaker.try_write(permit, &block(6)).is_err());
}

#[test]
fn the_clock_is_primed_with_accepted_zeros_and_started_only_when_told() {
    let device = PlaybackControl::register("scripted:clock");
    let boot = BootId::new([65; 16]).unwrap();
    let mut speaker = Speaker::open_clock("scripted:clock", boot).unwrap();
    assert!(speaker.zero_authority().is_err(), "no authority installed");
    let (mut controller, _owner, permit) = answering(boot);
    speaker
        .install(controller.snapshot(now()).unwrap())
        .unwrap();
    let token = speaker.zero_authority().unwrap();
    // Speech cannot start the clock, and the clock cannot start unprimed.
    assert!(speaker.try_write(permit, &block(9)).is_err());
    assert!(speaker.start_clock(token).is_err());
    let first = speaker.try_write_silence(token, 240).unwrap();
    let second = speaker.try_write_silence(token, 240).unwrap();
    assert_eq!((first.frames, second.frames), (240, 240));
    assert_eq!(second.first_sample, first.end_sample);
    // Written to, and still not running: nothing starts by itself.
    assert_eq!(device.counts().starts, 0);
    assert_eq!(device.state(), DeviceState::Prepared);
    speaker.start_clock(token).unwrap();
    assert!(speaker.clock_started());
    assert_eq!(device.counts().starts, 1);
    assert_eq!(device.state(), DeviceState::Running);
    assert_eq!(device.accepted(), vec![0; 480]);
    // The primed zeros play out; speech then joins the same running clock.
    device.play(480);
    speaker.maintain().unwrap();
    let spoken = speaker.try_write(permit, &block(9)).unwrap();
    assert_eq!(spoken.frames, RENDER_SAMPLES);
    assert_eq!(spoken.first_sample, second.end_sample);
    assert_eq!(device.accepted()[480..], block(9));
    // Privacy closes it: discarded, and no more zeros either.
    controller
        .set_microphone_permission(now(), Permission::Denied)
        .unwrap();
    let _ = speaker.install(controller.snapshot(now()).unwrap());
    assert!(speaker.try_write_silence(token, 240).is_err());
    assert!(speaker.try_write(permit, &block(9)).is_err());
}
#[test]
fn a_device_outside_the_accepted_format_is_refused_at_open() {
    let boot = BootId::new([66; 16]).unwrap();
    let good = PcmFormat {
        rate: 24_000,
        period_frames: 240,
        buffer_frames: 960,
    };
    for (index, format) in [
        PcmFormat {
            rate: 48_000,
            ..good
        },
        PcmFormat {
            period_frames: 0,
            ..good
        },
        PcmFormat {
            period_frames: 480,
            ..good
        },
        PcmFormat {
            buffer_frames: 1_920,
            ..good
        },
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("scripted:format-{index}");
        PlaybackControl::register(&name).set_format(format);
        assert!(Speaker::open(&name, boot).is_err(), "{format:?}");
    }
    // One block of buffer is enough to speak into, and too little to prime
    // a two-block clock.
    let name = "scripted:format-small";
    PlaybackControl::register(name).set_format(PcmFormat {
        buffer_frames: 240,
        ..good
    });
    assert!(Speaker::open(name, boot).is_ok());
    assert!(Speaker::open_clock(name, boot).is_err());
    // A name no device answers to, and a device that fails to open.
    assert!(Speaker::open("scripted:absent", boot).is_err());
    let failing = PlaybackControl::register("scripted:open-fails");
    failing.fail_next("open", errno::NODEV);
    assert!(Speaker::open("scripted:open-fails", boot).is_err());
    assert_eq!(failing.counts().opens, 0);
}
