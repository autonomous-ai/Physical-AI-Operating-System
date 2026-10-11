use super::*;
use std::sync::atomic::AtomicUsize;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lamp-output-test-{}-{}-{}",
            std::process::id(),
            lamp_ipc::monotonic_ns(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(36 + samples.len() as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn sine(rate: u32, hz: f64, count: usize) -> Source {
    decode_source(&wav(
        rate,
        1,
        &(0..count)
            .map(|n| {
                (8192.0 * (2.0 * std::f64::consts::PI * hz * n as f64 / f64::from(rate)).sin())
                    as i16
            })
            .collect::<Vec<_>>(),
    ))
    .unwrap()
}

#[test]
fn exact_unique_output_is_required() {
    let names = vec!["Headphones".into(), OUTPUT_NAME.into()];
    assert_eq!(unique_output_index(&names, OUTPUT_NAME).unwrap(), 1);
    for name in [
        "",
        "default",
        "imac speakers",
        "iMac Speakers ",
        "Headphones",
    ] {
        assert!(unique_output_index(&names, name).is_err());
    }
    assert!(unique_output_index(&[], OUTPUT_NAME).is_err());
    assert!(unique_output_index(&[OUTPUT_NAME.into(), OUTPUT_NAME.into()], OUTPUT_NAME).is_err());
}

#[test]
fn bounded_pcm_rejects_incomplete_unsupported_and_silent_files() {
    let good = wav(16000, 1, &[1, 2, -3, 4]);
    for length in 0..good.len() {
        assert!(decode_source(&good[..length]).is_err());
    }
    let mut extra = good.clone();
    extra.push(0);
    assert!(decode_source(&extra).is_err());
    for (offset, value) in [(20, 3), (22, 3), (32, 9), (34, 24)] {
        let mut bad = good.clone();
        bad[offset] = value;
        assert!(decode_source(&bad).is_err(), "offset {offset}");
    }
    assert!(decode_source(&wav(7999, 1, &[1])).is_err());
    assert!(decode_source(&wav(192001, 1, &[1])).is_err());
    assert!(decode_source(&wav(16000, 2, &[1])).is_err());
    assert!(decode_source(&wav(16000, 1, &[])).is_err());
    assert!(decode_source(&wav(16000, 1, &[0, 0])).is_err());
    for rail in [i16::MIN, i16::MAX] {
        assert!(decode_source(&wav(16000, 1, &[1, rail])).is_err());
    }
    assert!(decode_source(&wav(8000, 1, &vec![1; 8000 * 120 + 1])).is_err());
    let mut huge = good;
    huge[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_source(&huge).is_err());
}

#[test]
fn accepts_padded_metadata_and_preserves_exact_source_hash() {
    let mut bytes = wav(16000, 1, &[200, -200]);
    bytes.extend_from_slice(b"JUNK\x01\0\0\0x\0");
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    let source = decode_source(&bytes).unwrap();
    assert_eq!(source.info.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(source.info.frames, 2);
    for complete_chunk in [&bytes[12..36], &bytes[36..48]] {
        let mut duplicate = bytes.clone();
        duplicate.extend_from_slice(complete_chunk);
        let size = duplicate.len() as u32 - 8;
        duplicate[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(decode_source(&duplicate).is_err());
    }
}

#[test]
fn opens_only_bounded_regular_non_symlink_files() {
    let temp = Temp::new();
    let path = temp.0.join("source.wav");
    let bytes = wav(16000, 1, &[100, -200]);
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(
        load_source(&path).unwrap().info.file_bytes,
        bytes.len() as u64
    );
    let link = temp.0.join("link.wav");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(load_source(&link).is_err());
    let fifo = temp.0.join("fifo.wav");
    // rustix exposes mkfifoat on Linux but not macOS; this POSIX test helper
    // creates only a private fixture and never invokes an audio/device command.
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(load_source(&fifo).is_err());
    assert!(load_source(&temp.0).is_err());
    let oversized = temp.0.join("oversized.wav");
    File::create(&oversized)
        .unwrap()
        .set_len(MAX_WAV_BYTES + 1)
        .unwrap();
    assert!(load_source(&oversized).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn native_rate_preserves_samples_and_stereo_order() {
    let source = decode_source(&wav(48000, 2, &[1000, -2000, 3000, -4000])).unwrap();
    let prepared = prepare(&source, 48000, 2, || Ok(false)).unwrap().unwrap();
    assert_eq!(prepared.samples, source.samples);
    assert_eq!(prepared.info.rate_conversion, "none");
    assert!(prepare(&source, 48000, 1, || Ok(false)).is_err());
}

#[test]
fn mono_16k_to_stereo_48k_preserves_duration_pitch_and_identity() {
    let source = sine(16000, 1000.0, 1600);
    let prepared = prepare(&source, 48000, 2, || Ok(false)).unwrap().unwrap();
    assert_eq!(prepared.info.frames, 4800);
    assert_eq!(prepared.info.duration_s, source.info.duration_s);
    let mut error = 0.0_f64;
    for (frame, pair) in prepared
        .samples
        .chunks_exact(2)
        .enumerate()
        .skip(48)
        .take(4704)
    {
        assert_eq!(pair[0], pair[1]);
        let expected = 0.25 * (2.0 * std::f64::consts::PI * frame as f64 / 48.0).sin();
        error += (f64::from(pair[0]) - expected).powi(2);
    }
    assert!((error / 4704.0).sqrt() < 0.001);
    let again = prepare(&source, 48000, 2, || Ok(false)).unwrap().unwrap();
    assert_eq!(
        prepared.info.sha256_float32_le,
        again.info.sha256_float32_le
    );
    assert_ne!(prepared.info.sha256_float32_le, source.info.sha256);
}

#[test]
fn downsampling_rejects_alias_energy() {
    let source = sine(16000, 6000.0, 1600);
    let prepared = prepare(&source, 8000, 1, || Ok(false)).unwrap().unwrap();
    let rms = (prepared.samples[32..768]
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        / 736.0)
        .sqrt();
    assert!(rms < 0.01, "aliased RMS {rms}");
}

#[test]
fn conversion_budget_cancel_and_peak_are_enforced() {
    let source = sine(16000, 1000.0, 1600);
    assert!(prepare(&source, 8001, 2, || Ok(false)).is_err());
    assert!(prepare(&source, 48000, 3, || Ok(false)).is_err());
    assert!(prepare(&source, 48000, 2, || Ok(true)).unwrap().is_none());
    let checks = std::cell::Cell::new(0);
    let longer = sine(16000, 1000.0, 3200);
    assert!(
        prepare(&longer, 48000, 2, || {
            checks.set(checks.get() + 1);
            Ok(checks.get() >= 2)
        })
        .unwrap()
        .is_none()
    );
    assert_eq!(
        checks.get(),
        2,
        "mid-prepare cancellation is checked before completion"
    );
    let mut oversized = sine(8000, 500.0, 8);
    oversized.info.frames = 8000 * 120;
    assert!(prepare(&oversized, 192000, 2, || Ok(false)).is_err());
    let mut invalid = sine(16000, 1000.0, 16);
    invalid.samples[1] = f32::NAN;
    assert!(prepare(&invalid, 16000, 1, || Ok(false)).is_err());
    invalid.samples[1] = 1.0;
    assert!(prepare(&invalid, 16000, 1, || Ok(false)).is_err());
}

fn renderer() -> Renderer {
    let source = decode_source(&wav(8000, 1, &[100, 200, 300, 400, 500, 600])).unwrap();
    Renderer {
        prepared: prepare(&source, 8000, 1, || Ok(false)).unwrap().unwrap(),
        shared: Arc::default(),
        cursor: 0,
        previous: None,
    }
}
fn time(index: u64) -> Times {
    Times {
        host_ns: 1_000_000_000 + index * 500_000,
        callback_ns: 2_000_000_000 + index * 500_000,
        device_ns: 2_001_000_000 + index * 500_000,
    }
}

#[test]
fn last_buffer_trim_tail_checks_and_estimated_drain_are_distinct() {
    let mut renderer = renderer();
    let mut output = [0.0; 4];
    renderer.render(&mut output, time(0), false);
    assert!(output.iter().all(|v| *v != 0.0));
    renderer.render(&mut output, time(1), false);
    assert_ne!(output[1], 0.0);
    assert_eq!(&output[2..], &[0.0, 0.0]);
    assert!(!renderer.shared.snapshot().complete(6));
    for i in 2..=3 {
        renderer.render(&mut output, time(i), false);
    }
    assert_eq!(renderer.shared.snapshot().tail_callbacks, 2);
    assert!(
        !renderer.shared.snapshot().complete(6),
        "estimated hardware drain still ahead"
    );
    renderer.render(&mut output, time(4), false);
    let snapshot = renderer.shared.snapshot();
    assert!(snapshot.complete(6));
    assert_eq!(snapshot.submitted_frames, 6);
    assert_eq!(snapshot.intentional_zero_frames, 14);
    assert_eq!(snapshot.rejected_frames, 0);
    renderer.render(&mut output, time(5), true);
    assert!(
        !renderer.shared.snapshot().complete(6),
        "late xrun invalidates complete frames"
    );
}

#[test]
fn faults_cancel_and_malformed_callbacks_submit_no_new_source() {
    for fault in 0..5 {
        let mut renderer = renderer();
        let mut output = [1.0; 4];
        match fault {
            0 => renderer.shared.cancelled.store(true, Ordering::Release),
            1 => renderer.shared.note_error(4),
            2 => {
                renderer
                    .shared
                    .callbacks
                    .store(MAX_CALLBACKS, Ordering::Release);
            }
            _ => {}
        }
        let mut timestamp = time(0);
        if fault == 3 {
            timestamp.callback_ns = 0;
        }
        renderer.render(&mut output, timestamp, fault == 4);
        assert_eq!(output, [0.0; 4]);
        assert_eq!(renderer.shared.snapshot().submitted_frames, 0);
    }
    let mut renderer = renderer();
    renderer.render(&mut vec![1.0; MAX_CALLBACK_FRAMES + 1], time(0), false);
    assert_ne!(renderer.shared.snapshot().faults & 2, 0);
    let source = decode_source(&wav(8000, 2, &[100; 8])).unwrap();
    renderer.prepared = prepare(&source, 8000, 2, || Ok(false)).unwrap().unwrap();
    renderer.render(&mut [1.0; 3], time(1), false);
    assert_eq!(renderer.shared.snapshot().submitted_frames, 0);
}

#[test]
fn cancel_after_partial_delivery_is_not_completion() {
    let mut renderer = renderer();
    renderer.render(&mut [0.0; 4], time(0), false);
    renderer.shared.cancelled.store(true, Ordering::Release);
    let mut output = [1.0; 4];
    renderer.render(&mut output, time(1), false);
    assert_eq!(output, [0.0; 4]);
    assert_eq!(renderer.shared.snapshot().submitted_frames, 4);
    assert!(!renderer.shared.snapshot().complete(6));
}

#[test]
fn stream_clock_loss_fails_but_host_arrival_jitter_is_diagnostic() {
    let mut renderer = renderer();
    renderer.render(&mut [0.0; 4], time(0), false);
    let mut delayed = time(1);
    delayed.host_ns += 15_000_000;
    renderer.render(&mut [0.0; 4], delayed, false);
    assert_eq!(renderer.shared.snapshot().faults, 0);
    let mut lost = time(3);
    lost.host_ns = delayed.host_ns + 1000;
    renderer.render(&mut [0.0; 4], lost, false);
    assert_eq!(renderer.shared.snapshot().timestamp_gaps, 1);
}

#[test]
fn controller_cancels_and_times_out_without_any_callback() {
    let state = Shared::default();
    assert_eq!(
        stop_decision(&state.snapshot(), 6, 1_000_000, 0, 1, true),
        Some("cancelled")
    );
    assert_eq!(
        stop_decision(&state.snapshot(), 6, 1_000_000, 5_000_000_000, 1, false),
        Some("first_callback_timeout")
    );
    assert_eq!(
        stop_decision(&state.snapshot(), 6, 1_000_000, 8_001_000_000, 1, false),
        Some("wall_deadline")
    );
    state.callbacks.store(1, Ordering::Release);
    state.last_host_ns.store(10, Ordering::Release);
    assert_eq!(
        stop_decision(&state.snapshot(), 6, 1_000_000, 1, 2_000_000_010, false),
        Some("callback_stall")
    );
}

#[test]
fn pre_cancel_and_invalid_wav_leave_final_failure_without_device_access() {
    let temp = Temp::new();
    let cancel = temp.0.join("cancel");
    std::fs::write(&cancel, b"").unwrap();
    let options = PlayOptions {
        output_device: OUTPUT_NAME.into(),
        wav: temp.0.join("missing.wav"),
        output: temp.0.join("cancelled"),
        cancel_file: Some(cancel),
        prepared_start_ms: None,
    };
    let report = play(&options).unwrap();
    assert_eq!(report["status"], "cancelled_before_prepare");
    assert_eq!(report["audio_stream_opened"], false);
    assert_eq!(report["valid"], false);
    assert!(options.output.join("metadata.json").is_file());
    assert!(play(&options).is_err(), "never overwrite an attempt");
    let invalid = PlayOptions {
        output: temp.0.join("invalid"),
        cancel_file: None,
        prepared_start_ms: None,
        ..options
    };
    let report = play(&invalid).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["audio_stream_opened"], false);
    assert_eq!(report["valid"], false);
}

// Prepared two-phase start, through the production callback (`render`) and
// controller (`wait_for_grant`, `StartGate`, grant reader); CPAL's callback
// delivery is the only simulated part.

const ATTEMPT: &str = "attempt-7";

fn armed() -> Renderer {
    let renderer = renderer();
    renderer.shared.armed.store(true, Ordering::Release);
    renderer
}

fn grant(expires: u64) -> GrantInput {
    GrantInput::Line(format!(
        r#"{{"schema":1,"kind":"start","attempt_id":"{ATTEMPT}","expires_monotonic_ns":{expires}}}"#
    ))
}

/// Renders one callback into a buffer that starts nonzero, so any source
/// copy or missed zero-fill is visible.
fn callback(renderer: &mut Renderer, index: u64) -> [f32; 4] {
    let mut output = [9.0; 4];
    renderer.render(&mut output, time(index), false);
    output
}

/// Waits through the production controller on a fake clock that advances
/// 1 ms per call, delivering `inputs` one per wait step.
fn wait(
    renderer: &Renderer,
    gate: &mut StartGate,
    start_ns: u64,
    max_wait_ns: u64,
    inputs: Vec<GrantInput>,
    cancel: bool,
) -> GrantWait {
    let clock = std::cell::Cell::new(start_ns);
    let mut inputs = inputs.into_iter();
    wait_for_grant(
        &renderer.shared,
        gate,
        max_wait_ns,
        || {
            clock.set(clock.get() + 1_000_000);
            clock.get()
        },
        || inputs.next(),
        || Ok(cancel),
    )
    .unwrap()
}

/// A never-started attempt: silent buffers, no source copied, cursor at 0.
fn assert_never_played(renderer: &mut Renderer, from: u64) {
    for index in from..from + 3 {
        assert_eq!(callback(renderer, index), [0.0; 4]);
    }
    let state = renderer.shared.snapshot();
    assert_eq!(state.submitted_frames, 0);
    assert_eq!(state.source_started_host_ns, 0);
    assert_eq!(renderer.cursor, 0);
}

#[test]
fn grant_reader_bounds_lines_and_reports_malformed_and_closed_input() {
    let read = |bytes: Vec<u8>| {
        let (sender, receiver) = sync_channel(MAX_GRANT_LINES + 1);
        read_grant_lines(std::io::Cursor::new(bytes), &sender);
        drop(sender);
        receiver.iter().collect::<Vec<_>>()
    };
    let mut oversized = b"first\n".to_vec();
    oversized.extend(vec![b'x'; MAX_GRANT_LINE_BYTES + 10]);
    oversized.push(b'\n');
    assert_eq!(
        read(oversized),
        [GrantInput::Line("first".into()), GrantInput::Malformed]
    );
    assert_eq!(read(b"no newline".to_vec()), [GrantInput::Malformed]);
    assert_eq!(read(Vec::new()), [GrantInput::Closed]);
    assert_eq!(read(vec![0xff, b'\n']), [GrantInput::Malformed]);
    let many = read("line\n".repeat(20).into_bytes());
    assert_eq!(many.len(), MAX_GRANT_LINES, "bounded queue and reads");
}

#[test]
fn the_gate_accepts_one_original_grant_and_refuses_everything_else() {
    let now = 1_000_000_000;
    let decide = |input: GrantInput| StartGate::new(ATTEMPT).input(input, now).unwrap();
    assert_eq!(
        decide(grant(now + 50_000_000)),
        GateDecision::Start {
            expires_ns: now + 50_000_000
        }
    );
    let line = |text: &str| GrantInput::Line(text.into());
    for (input, reason) in [
        (grant(now), "grant_expired_on_receipt"),
        (grant(now - 1), "grant_expired_on_receipt"),
        (
            grant(now + MAX_GRANT_HORIZON_NS + 1),
            "grant_horizon_exceeds_bound",
        ),
        (
            line(
                r#"{"schema":1,"kind":"start","attempt_id":"other","expires_monotonic_ns":2000000000}"#,
            ),
            "grant_for_another_attempt",
        ),
        (
            line(
                r#"{"schema":2,"kind":"start","attempt_id":"attempt-7","expires_monotonic_ns":1050000000}"#,
            ),
            "grant_malformed",
        ),
        (
            line(
                r#"{"schema":1,"kind":"stop","attempt_id":"attempt-7","expires_monotonic_ns":1050000000}"#,
            ),
            "grant_malformed",
        ),
        (
            line(
                r#"{"schema":1,"kind":"start","attempt_id":"attempt-7","expires_monotonic_ns":1050000000,"x":1}"#,
            ),
            "grant_malformed",
        ),
        (line("not json"), "grant_malformed"),
        (GrantInput::Malformed, "grant_malformed"),
        (GrantInput::Closed, "stopped_before_grant"),
    ] {
        assert_eq!(decide(input), GateDecision::Refuse(reason), "{reason}");
    }
    // One original line decides. Nothing after it can start or restart.
    let mut gate = StartGate::new(ATTEMPT);
    assert!(gate.input(line("bad"), now).is_some());
    assert_eq!(gate.input(grant(now + 50_000_000), now), None);
    assert_eq!(gate.duplicates, 1);
}

#[test]
fn positive_control_stays_silent_until_granted_then_plays_from_frame_zero() {
    let mut renderer = armed();
    // The stream runs before the grant: silent, no source.
    for index in 0..3 {
        assert_eq!(callback(&mut renderer, index), [0.0; 4]);
    }
    assert_eq!(renderer.shared.snapshot().armed_zero_frames, 12);
    let mut gate = StartGate::new(ATTEMPT);
    let received = time(3).host_ns - 2_000_000;
    let outcome = wait(
        &renderer,
        &mut gate,
        received - 5_000_000,
        1_000_000_000,
        vec![grant(received + 100_000_000)],
        false,
    );
    let GrantWait::Granted {
        expires_ns,
        received_ns,
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(expires_ns, received + 100_000_000);
    assert!(received_ns < expires_ns && received_ns < time(3).host_ns);
    assert_eq!(renderer.shared.snapshot().grant_expires_ns, expires_ns);
    let first = callback(&mut renderer, 3);
    assert_eq!(first, renderer.prepared.samples[..4]);
    for index in 4..=7 {
        callback(&mut renderer, index);
    }
    let state = renderer.shared.snapshot();
    assert_eq!(state.source_started_host_ns, time(3).host_ns);
    assert_eq!(state.submitted_frames, 6);
    assert!(state.complete(6));
    assert_eq!(
        stop_decision(&state, 6, 750_000, 0, time(7).host_ns, false),
        Some("frames_submitted_and_estimated_drain_checked")
    );
    // A later grant line is a counted duplicate and changes nothing.
    assert_eq!(gate.input(grant(u64::MAX), time(8).host_ns), None);
    assert_eq!(gate.duplicates, 1);
    assert_eq!(callback(&mut renderer, 8), [0.0; 4]);
    assert_eq!(renderer.shared.snapshot().submitted_frames, 6);
}

#[test]
fn a_grant_that_expires_before_the_first_submission_never_copies_source() {
    let mut renderer = armed();
    callback(&mut renderer, 0);
    let mut gate = StartGate::new(ATTEMPT);
    // Accepted while still valid, but the next callback comes after expiry:
    // a late process or device start cannot play the stimulus late.
    let expires = time(1).host_ns - 1;
    let outcome = wait(
        &renderer,
        &mut gate,
        expires - 10_000_000,
        1_000_000_000,
        vec![grant(expires)],
        false,
    );
    assert!(matches!(outcome, GrantWait::Granted { .. }), "{outcome:?}");
    assert_never_played(&mut renderer, 1);
    let state = renderer.shared.snapshot();
    assert!(state.start_expired);
    assert_eq!(
        stop_decision(&state, 6, 750_000, 0, time(4).host_ns, false),
        Some("start_expired_at_first_submission")
    );
}

#[test]
fn a_grant_already_expired_on_receipt_is_refused_and_never_installed() {
    let mut renderer = armed();
    callback(&mut renderer, 0);
    let mut gate = StartGate::new(ATTEMPT);
    let outcome = wait(
        &renderer,
        &mut gate,
        5_000,
        1_000_000_000,
        vec![grant(5_000)],
        false,
    );
    assert_eq!(
        outcome,
        GrantWait::NotStarted {
            status: "grant_refused",
            reason: Some("grant_expired_on_receipt")
        }
    );
    assert_eq!(renderer.shared.snapshot().grant_expires_ns, 0);
    assert_never_played(&mut renderer, 1);
}

#[test]
fn stop_cancel_malformed_and_timeout_before_a_grant_never_play() {
    let start = time(0).host_ns;
    let cases: [(Vec<GrantInput>, bool, &str); 4] = [
        (vec![GrantInput::Closed], false, "stopped_before_grant"),
        (
            vec![grant(start + 50_000_000)],
            true,
            "cancelled_before_grant",
        ),
        (vec![GrantInput::Malformed], false, "grant_refused"),
        (Vec::new(), false, "grant_timeout"),
    ];
    for (inputs, cancel, expected) in cases {
        let mut renderer = armed();
        callback(&mut renderer, 0);
        let mut gate = StartGate::new(ATTEMPT);
        let outcome = wait(&renderer, &mut gate, start, 20_000_000, inputs, cancel);
        assert!(
            matches!(outcome, GrantWait::NotStarted { status, .. } if status == expected),
            "{expected}: {outcome:?}"
        );
        // The controller then latches cancellation for the callback.
        renderer.shared.cancelled.store(true, Ordering::Release);
        assert_never_played(&mut renderer, 1);
    }
}

#[test]
fn stream_faults_or_a_stall_before_the_grant_end_the_wait_without_playing() {
    let mut renderer = armed();
    callback(&mut renderer, 0);
    renderer.shared.note_error(4);
    let mut gate = StartGate::new(ATTEMPT);
    let outcome = wait(
        &renderer,
        &mut gate,
        time(0).host_ns,
        1_000_000_000,
        Vec::new(),
        false,
    );
    assert_eq!(
        outcome,
        GrantWait::NotStarted {
            status: "failed_before_grant",
            reason: Some("callback_or_stream_fault")
        }
    );
    assert_never_played(&mut renderer, 1);
    let renderer = armed();
    let mut stalled = renderer;
    callback(&mut stalled, 0);
    let outcome = wait(
        &stalled,
        &mut StartGate::new(ATTEMPT),
        time(0).host_ns + 2_000_000_000,
        10_000_000_000,
        Vec::new(),
        false,
    );
    assert_eq!(
        outcome,
        GrantWait::NotStarted {
            status: "failed_before_grant",
            reason: Some("callback_stall")
        }
    );
}

#[test]
fn cancel_after_the_grant_but_before_the_first_frame_never_plays() {
    let mut renderer = armed();
    callback(&mut renderer, 0);
    let mut gate = StartGate::new(ATTEMPT);
    let outcome = wait(
        &renderer,
        &mut gate,
        time(0).host_ns,
        1_000_000_000,
        vec![grant(time(0).host_ns + 500_000_000)],
        false,
    );
    assert!(matches!(outcome, GrantWait::Granted { .. }));
    renderer.shared.cancelled.store(true, Ordering::Release);
    assert_never_played(&mut renderer, 1);
}

#[test]
fn prepared_and_default_entry_points_refuse_each_others_options() {
    let temp = Temp::new();
    let options = PlayOptions {
        output_device: OUTPUT_NAME.into(),
        wav: temp.0.join("missing.wav"),
        output: temp.0.join("default"),
        cancel_file: None,
        prepared_start_ms: Some(1000),
    };
    assert!(play(&options).is_err());
    let (_sender, inputs) = sync_channel(1);
    let mut ready = |_: &Value| Ok(());
    let default = PlayOptions {
        prepared_start_ms: None,
        ..options.clone()
    };
    assert!(
        play_prepared(
            &default,
            PreparedIo {
                ready: &mut ready,
                inputs
            }
        )
        .is_err()
    );
    for bad in [0, MAX_PREPARED_WAIT_MS + 1] {
        let invalid = PlayOptions {
            prepared_start_ms: Some(bad),
            ..options.clone()
        };
        assert!(invalid.validate().is_err());
    }
}
