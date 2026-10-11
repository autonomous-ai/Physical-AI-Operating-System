use super::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::time::{Duration, Instant};

fn select(name: &str) -> Result<cpal::Device> {
    let host = cpal::host_from_id(cpal::HostId::CoreAudio)?;
    let devices: Vec<_> = host.output_devices()?.take(257).collect();
    if devices.len() > 256 {
        return Err(io::Error::other("output device inventory exceeds 256").into());
    }
    let index = unique_output_index(
        &devices.iter().map(ToString::to_string).collect::<Vec<_>>(),
        name,
    )?;
    Ok(devices
        .into_iter()
        .nth(index)
        .expect("validated device index"))
}

fn identity(device: &cpal::Device) -> Result<Value> {
    Ok(json!({"name": device.to_string(), "id": device.id()?.to_string()}))
}

fn configuration(supported: &cpal::SupportedStreamConfig) -> Value {
    json!({"sample_rate_hz": supported.sample_rate(), "channels": supported.channels(),
        "sample_format": supported.sample_format().to_string(),
        "supported_buffer_size": format!("{:?}", supported.buffer_size()),
        "requested_buffer_size": "Default"})
}

pub(super) fn inspect(name: &str) -> Result<Value> {
    let device = select(name)?;
    let supported = device.default_output_config()?;
    Ok(
        json!({"host": "CoreAudio", "selected_output": identity(&device)?,
        "selected_default_config": configuration(&supported), "audio_stream_opened": false,
        "scope": "output_inventory_only_not_physical_route_or_audibility_proof",
        "play_format_supported": supported.sample_format() == cpal::SampleFormat::F32
            && (1..=2).contains(&supported.channels())
            && (8000..=192000).contains(&supported.sample_rate())}),
    )
}

pub(super) fn play(
    options: &PlayOptions,
    source: Source,
    report: &mut Value,
    mut prepared_io: Option<PreparedIo<'_>>,
    attempt_id: &str,
) -> Result<()> {
    let device = select(&options.output_device)?;
    let selected = identity(&device)?;
    let supported = device.default_output_config()?;
    let config = supported.config();
    report["host"] = json!("CoreAudio");
    report["selected_output"] = selected.clone();
    report["selected_default_config"] = configuration(&supported);
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "selected output default must deliver float32; no alternate config is selected",
        )
        .into());
    }
    let Some(prepared) = prepare(&source, config.sample_rate, config.channels, || {
        cancellation_requested(options.cancel_file.as_deref())
    })?
    else {
        report["status"] = json!("cancelled_during_prepare");
        return Ok(());
    };
    report["prepared"] = serde_json::to_value(&prepared.info)?;
    let target = prepared.info.frames;
    let duration_ns = target * 1_000_000_000 / u64::from(config.sample_rate);
    // Release the decoded source before any native stream is opened.
    drop(source);
    if cancellation_requested(options.cancel_file.as_deref())? {
        report["status"] = json!("cancelled_before_stream");
        return Ok(());
    }
    let state = Arc::new(Shared::default());
    // Armed before the stream exists: no callback can submit source first.
    state.armed.store(prepared_io.is_some(), Ordering::Release);
    let mut renderer = Renderer {
        prepared,
        shared: state.clone(),
        cursor: 0,
        previous: None,
        clock: lamp_ipc::monotonic_ns,
    };
    let error_state = state.clone();
    report["build_started_host_monotonic_ns"] = json!(lamp_ipc::monotonic_ns());
    let stream = device.build_output_stream(
        config,
        move |output: &mut [f32], info: &cpal::CallbackInfo| {
            let timestamp = info.timestamp();
            renderer.render(
                output,
                Times {
                    host_ns: lamp_ipc::monotonic_ns(),
                    callback_ns: u64::try_from(timestamp.callback.as_nanos()).unwrap_or(0),
                    device_ns: u64::try_from(timestamp.device.as_nanos()).unwrap_or(0),
                },
                info.xrun(),
            );
        },
        move |error| {
            let kind = match error.kind() {
                cpal::ErrorKind::DeviceNotAvailable => 1,
                cpal::ErrorKind::PermissionDenied => 2,
                cpal::ErrorKind::StreamInvalidated => 4,
                cpal::ErrorKind::DeviceChanged => 8,
                _ => 16,
            };
            error_state.note_error(kind);
        },
        Some(Duration::from_secs(3)),
    )?;
    report["build_finished_host_monotonic_ns"] = json!(lamp_ipc::monotonic_ns());
    report["audio_stream_opened"] = json!(true);
    report["initial_stream_buffer_frames"] = json!(stream.buffer_size().ok());
    let mut errors = Vec::new();
    let mut stop_reason = "stream_start_failed";
    let run = (|| -> Result<()> {
        if cancellation_requested(options.cancel_file.as_deref())? {
            state.cancelled.store(true, Ordering::Release);
            stop_reason = "cancelled_before_start";
            return Ok(());
        }
        report["start_requested_host_monotonic_ns"] = json!(lamp_ipc::monotonic_ns());
        stream.start()?;
        if let Some(io) = prepared_io.as_mut() {
            match prepared_start(options, &state, report, io, attempt_id)? {
                None => {}
                Some(status) => {
                    // Nothing was granted: the armed callback never submitted
                    // a source frame, and cancellation keeps it that way.
                    state.cancelled.store(true, Ordering::Release);
                    stop_reason = status;
                    return Ok(());
                }
            }
        }
        let began = Instant::now();
        loop {
            let cancelled = cancellation_requested(options.cancel_file.as_deref())?;
            if cancelled {
                state.cancelled.store(true, Ordering::Release);
            }
            let snapshot = state.snapshot();
            if let Some(reason) = stop_decision(
                &snapshot,
                target,
                duration_ns,
                began.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
                lamp_ipc::monotonic_ns(),
                cancelled,
            ) {
                stop_reason = reason;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    if let Err(error) = run {
        state.faults.fetch_or(8, Ordering::Release);
        errors.push(error.to_string());
        stop_reason = "control_or_start_error";
    }
    if stop_reason != "frames_submitted_and_estimated_drain_checked"
        && !state.cancelled.load(Ordering::Acquire)
    {
        state.faults.fetch_or(8, Ordering::Release);
    }
    if let Some(io) = prepared_io.as_ref() {
        // Control lines after the decision never start anything; count them.
        let late = std::iter::from_fn(|| io.inputs.try_recv().ok())
            .filter(|input| matches!(input, GrantInput::Line(_) | GrantInput::Malformed))
            .count();
        report["prepared_start"]["control_lines_after_decision"] = json!(late);
    }
    // Control never waits for a callback to notice cancellation or a stalled stream.
    report["stop_requested_host_monotonic_ns"] = json!(lamp_ipc::monotonic_ns());
    if let Err(error) = stream.pause() {
        errors.push(format!("pause: {error}"));
    }
    report["final_stream_buffer_frames"] = json!(stream.buffer_size().ok());
    drop(stream);
    report["stream_dropped_host_monotonic_ns"] = json!(lamp_ipc::monotonic_ns());
    let snapshot = state.snapshot();
    let route_end = identity(&device);
    let config_end = device
        .default_output_config()
        .map(|value| configuration(&value));
    match route_end {
        Ok(value) => {
            if value != selected {
                errors.push("selected output identity changed".into());
            }
            report["selected_output_at_end"] = value;
        }
        Err(error) => errors.push(format!("output identity after stop: {error}")),
    }
    match config_end {
        Ok(value) => {
            if value != report["selected_default_config"] {
                errors.push("selected default output configuration changed".into());
            }
            report["selected_default_config_at_end"] = value;
        }
        Err(error) => errors.push(format!("output configuration after stop: {error}")),
    }
    let valid = stop_reason == "frames_submitted_and_estimated_drain_checked"
        && snapshot.complete(target)
        && errors.is_empty();
    report["valid"] = json!(valid);
    report["status"] = json!(if valid {
        "completed_delivery_checks"
    } else if snapshot.start_expired {
        "start_expired"
    } else if let Some(status) = PREPARED_STATUSES.iter().find(|s| **s == stop_reason) {
        status
    } else if snapshot.cancelled {
        "cancelled"
    } else {
        "failed_delivery_checks"
    });
    report["stop_reason"] = json!(stop_reason);
    report["counters"] = serde_json::to_value(snapshot)?;
    report["errors"] = json!(errors);
    Ok(())
}

/// Statuses of a prepared attempt that never started its source.
const PREPARED_STATUSES: [&str; 6] = [
    "cancelled_before_grant",
    "stopped_before_grant",
    "grant_refused",
    "grant_timeout",
    "failed_before_grant",
    "ready_not_published",
];

/// From a running silent stream to an installed grant. Returns a status when
/// nothing may be played.
fn prepared_start(
    options: &PlayOptions,
    state: &Shared,
    report: &mut Value,
    io: &mut PreparedIo<'_>,
    attempt_id: &str,
) -> Result<Option<&'static str>> {
    let cancelled = || cancellation_requested(options.cancel_file.as_deref());
    // The output is live (a callback has run) before readiness is claimed.
    let opened = Instant::now();
    while state.callbacks.load(Ordering::Acquire) == 0 {
        if cancelled()? {
            return Ok(Some("cancelled_before_grant"));
        }
        if opened.elapsed() >= Duration::from_secs(5) {
            report["prepared_start"]["failure"] = json!("first_callback_timeout");
            return Ok(Some("failed_before_grant"));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let ready = json!({
        "schema": 1, "kind": "prepared_ready", "attempt_id": attempt_id,
        "ready_monotonic_ns": lamp_ipc::monotonic_ns(), "clock": GRANT_CLOCK,
        "source_sha256": report["source"]["sha256"],
        "prepared_sha256_float32_le": report["prepared"]["sha256_float32_le"],
        "output": report["selected_output"],
        "output_config": report["selected_default_config"],
        "max_wait_ms": options.prepared_start_ms,
        "max_grant_horizon_ns": MAX_GRANT_HORIZON_NS,
        "grant": {"schema": 1, "kind": "start", "attempt_id": attempt_id,
            "expires_monotonic_ns": "absolute CLOCK_MONOTONIC ns"}
    });
    report["prepared_start"]["ready"] = ready.clone();
    if let Err(error) = (io.ready)(&ready) {
        report["prepared_start"]["failure"] = json!(format!("ready receipt: {error}"));
        return Ok(Some("ready_not_published"));
    }
    let mut gate = StartGate::new(attempt_id);
    let max_wait_ns = u64::from(options.prepared_start_ms.unwrap_or(0)) * 1_000_000;
    let inputs = &io.inputs;
    let outcome = wait_for_grant(
        state,
        &mut gate,
        max_wait_ns,
        lamp_ipc::monotonic_ns,
        || inputs.recv_timeout(Duration::from_millis(2)).ok(),
        cancelled,
    )?;
    report["prepared_start"]["grant"] = match &outcome {
        GrantWait::Granted {
            expires_ns,
            received_ns,
        } => json!({"decision": "start", "expires_monotonic_ns": expires_ns,
            "received_monotonic_ns": received_ns}),
        GrantWait::NotStarted { status, reason } => {
            json!({"decision": status, "reason": reason})
        }
    };
    report["prepared_start"]["duplicate_grant_lines"] = json!(gate.duplicates);
    Ok(match outcome {
        GrantWait::Granted { .. } => None,
        GrantWait::NotStarted { status, .. } => Some(status),
    })
}
