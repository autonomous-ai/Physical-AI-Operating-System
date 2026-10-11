//! MAIN-021: automatic boundary proposals end to end. A real lamp-live
//! `Recorder` writes the certified render of a 24 kHz reply; a synthetic room
//! WAV at 16 kHz holds a distinct stimulus and that reply at known samples in
//! noise; the trial is imported through the recorder-receipt path; proposals
//! must find both boundaries and stay separate from annotations.
mod common;

use lamp_acoustic::hash;
use lamp_interaction::{BootId, TurnOwner};
use lamp_live::{
    diagnostic_control::{DiagnosticFinished, DiagnosticStarted, DiagnosticStream},
    diagnostics::{
        Config, DEFAULT_FINISH_WAIT, EndMeta, EndReason, PrivacyMeta, Recorder, RenderMeta,
        SilenceKind, StreamKind, SubmitResult,
    },
};
use lamp_voice_eval::{
    align,
    import::{ImportOptions, import_trace},
    plan::LoadedPlan,
    proposals,
    record::{AttemptRecord, StimulusSource},
    stimulus::StimulusCatalog,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

const BOOT: [u8; 16] = [7; 16];
const ROOM: usize = 160_000;
const STIMULUS_AT: usize = 24_000;
const REPLY_AT: usize = 100_321;

/// Deterministic speech-like signal: shaped noise with a syllable envelope.
fn speechlike(seed: u64, samples: usize, rate: f64) -> Vec<f32> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut low = 0.0_f32;
    (0..samples)
        .map(|n| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let noise = (state >> 40) as f32 / (1u64 << 24) as f32 - 0.5;
            low = 0.7 * low + 0.3 * noise;
            let t = n as f64 / rate;
            let syllable = (std::f64::consts::PI * 4.0 * t + seed as f64).sin().abs();
            (low as f64 * syllable * 8000.0) as f32
        })
        .collect()
}

fn write_wav(path: &Path, samples: &[f32]) -> (String, String) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    let mut pcm = Vec::new();
    for v in samples {
        let sample = v.round().clamp(-32768.0, 32767.0) as i16;
        writer.write_sample(sample).unwrap();
        pcm.extend_from_slice(&sample.to_le_bytes());
    }
    writer.finalize().unwrap();
    (
        hash(&fs::read(path).unwrap()),
        format!("{:x}", Sha256::digest(&pcm)),
    )
}

/// The certified render of `reply` (24 kHz), owned by turn 1.
fn render(runtime: &Path, reply: &[i16]) -> Vec<Value> {
    let boot = BootId::new(BOOT).unwrap();
    let mut recorder = Recorder::start(Config::new(
        runtime.join("render-audio"),
        boot,
        StreamKind::Render,
    ))
    .unwrap();
    let started = DiagnosticStarted {
        boot,
        stream: DiagnosticStream::Render,
        started_at_us: recorder.started_at_us(),
    };
    let at = recorder.started_at_us();
    recorder.try_privacy(PrivacyMeta {
        at_us: at,
        generation: 1,
        open: true,
    });
    let owner: TurnOwner =
        serde_json::from_value(json!({"boot": BOOT, "turn": 1, "generation": 1})).unwrap();
    let meta = |first: u64, count: u64, owner: Option<TurnOwner>| RenderMeta {
        playback_epoch: 1,
        privacy_generation: 1,
        first_sample: first,
        end_sample: first + count,
        accepted_at_us: at + 1 + first,
        queue_observed_at_us: at + 1 + first,
        queued_frames: count as i64,
        owner,
        chunk_sequence: None,
    };
    recorder.try_silence(meta(0, 240, None), SilenceKind::Prime, 240);
    let mut first = 240;
    for chunk in reply.chunks(240) {
        // Paced like a real producer, so the 64-record queue never fills.
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_eq!(
            recorder.try_render(meta(first, chunk.len() as u64, Some(owner)), chunk),
            SubmitResult::Queued
        );
        first += chunk.len() as u64;
    }
    let report = recorder.finish(
        EndMeta {
            at_us: lamp_ipc::monotonic_us().max(at + 10_000_000),
            reason: EndReason::Stopped,
        },
        DEFAULT_FINISH_WAIT,
    );
    assert!(report.valid, "{report:?}");
    let receipt = DiagnosticFinished::from_report(started, report);
    vec![
        json!({"kind":"run_start","at_us":1,"provider_kind":"one_cached_reply"}),
        json!({"kind":"audio_diagnostics_started","worker":"speaker","worker_boot":BOOT,"details":started}),
        json!({"kind":"audio_diagnostics_certified","worker":"speaker","worker_boot":BOOT,
            "valid":true,"receipt":receipt,"error":null}),
        json!({"kind":"run_end","at_us":2,"status":"completed_unscored","error":null}),
    ]
}

struct Trial {
    _dir: common::Private,
    record: AttemptRecord,
    stimulus: Vec<f32>,
    reply_16k: Vec<f32>,
}

fn trial(label: &str) -> Trial {
    let dir = common::Private::new(label);
    let runtime = dir.join("runtime");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let reply_24k = speechlike(11, 36_000, 24_000.0);
    let reply_pcm: Vec<i16> = reply_24k.iter().map(|v| *v as i16).collect();
    let trace = render(&runtime, &reply_pcm);
    let events = runtime.join("events.jsonl");
    fs::write(
        &events,
        trace.iter().map(|v| format!("{v}\n")).collect::<String>(),
    )
    .unwrap();
    // The room hears a distinct stimulus, then the reply, in noise.
    let stimulus = speechlike(12, 32_000, 16_000.0);
    let reply_float: Vec<f32> = reply_pcm.iter().map(|v| f32::from(*v)).collect();
    let reply_16k = align::resample(&reply_float, 24_000, 16_000);
    let mut room: Vec<f32> = speechlike(13, ROOM, 16_000.0)
        .iter()
        .map(|v| v * 0.02)
        .collect();
    for (i, v) in stimulus.iter().enumerate() {
        room[STIMULUS_AT + i] += v;
    }
    for (i, v) in reply_16k.iter().enumerate() {
        room[REPLY_AT + i] += v * 0.3;
    }
    let (wav_sha, pcm_sha) = write_wav(&dir.join("lamp-52e6-voice.wav"), &room);
    let mut receipt: Value =
        serde_json::from_str(include_str!("fixtures/recording-import-examples.json")).unwrap();
    let mut receipt = receipt["source_capture_receipt"].take();
    let frames = ROOM as u64;
    let inner = &mut receipt["receipt"]["captures"]["voice"];
    inner["frames"] = json!(frames);
    inner["pcm_bytes"] = json!(frames * 2);
    inner["pcm_sha256"] = json!(pcm_sha);
    let outer = &mut receipt["captures"]["voice"];
    outer["frames_received"] = json!(frames);
    outer["pcm_bytes_received"] = json!(frames * 2);
    outer["pcm_sha256_received"] = json!(pcm_sha);
    outer["wav_sha256"] = json!(wav_sha);
    let receipt_path = dir.join("lamp-52e6-capture-receipt.json");
    fs::write(&receipt_path, receipt.to_string()).unwrap();
    let catalog = StimulusCatalog::load().unwrap();
    let plan = LoadedPlan::default_plan(&catalog).unwrap();
    let out = dir.join("run");
    lamp_voice_eval::ledger::create_run_directory(&out).unwrap();
    let record = import_trace(
        &plan,
        &catalog,
        &ImportOptions {
            run_id: "proposals".into(),
            events_path: &events,
            scenario: "fixed-reply-echo-only".into(),
            turn_map: Vec::new(),
            room_metadata: None,
            room_receipt: Some((&receipt_path, "voice")),
            room_independent: true,
            source: StimulusSource::LoudspeakerSynthetic,
        },
        &out,
    )
    .unwrap();
    assert_eq!(record.evidence["room_audio"]["valid"], true);
    assert_eq!(record.evidence["target_audio"]["render"]["verified"], true);
    Trial {
        _dir: dir,
        record,
        stimulus,
        reply_16k,
    }
}

/// The active-sample rule proposals use: |v| > max(64, 2% of the peak).
fn active(pcm: &[f32]) -> (usize, usize) {
    let peak = pcm.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    align::active_span(pcm, 64_f32.max(0.02 * peak)).unwrap()
}

fn near(value: &Value, expected_sample: usize) {
    let seconds = value.as_f64().unwrap_or(f64::NAN);
    let expected = expected_sample as f64 / 16_000.0;
    assert!(
        (seconds - expected).abs() <= 0.002,
        "{seconds} vs {expected}"
    );
}

#[test]
fn reply_and_stimulus_boundaries_are_proposed_but_never_annotated() {
    let t = trial("prop-ok");
    let stimulus = proposals::Stimulus {
        sha256: "synthetic".into(),
        pcm: t.stimulus.clone(),
    };
    let out = proposals::propose(
        "proposals",
        std::slice::from_ref(&t.record),
        Some(&stimulus),
    );
    assert_eq!(out["kind"], "boundary_proposals");
    let entry = &out["attempts"][&t.record.attempt_id];
    assert_eq!(entry["listened"], false);
    assert_eq!(entry["room"]["verified"], true);
    assert_eq!(entry["render"]["verified"], true);
    let reply = &entry["replies"][0];
    assert_eq!(reply["status"], "located", "{reply}");
    assert_eq!(reply["turn"], 1);
    let (onset, last) = active(&t.reply_16k);
    near(&reply["first_audible_reply_audio_s"], REPLY_AT + onset);
    near(&reply["reply_audio_end_s"], REPLY_AT + last);
    let (start, end) = active(&t.stimulus);
    near(
        &entry["stimulus"]["user_speech_start_s"],
        STIMULUS_AT + start,
    );
    near(&entry["stimulus"]["user_speech_end_s"], STIMULUS_AT + end);
    assert_eq!(entry["stimulus"]["status"], "located");
    // Proposals never enter the scored annotation schema.
    assert!(lamp_voice_eval::annotations::Annotations::parse(out.to_string().as_bytes()).is_err());
}

#[test]
fn without_exact_stimulus_pcm_the_speech_end_is_unavailable_not_invented() {
    let t = trial("prop-nostim");
    let out = proposals::propose("proposals", std::slice::from_ref(&t.record), None);
    let entry = &out["attempts"][&t.record.attempt_id];
    assert_eq!(entry["stimulus"]["status"], "unavailable");
    assert!(
        entry["stimulus"]["reason"]
            .as_str()
            .unwrap()
            .contains("transcript")
    );
    assert_eq!(entry["replies"][0]["status"], "located");
}

#[test]
fn a_changed_room_recording_yields_no_proposal() {
    let t = trial("prop-changed");
    let wav = t.record.evidence["room_audio"]["wav_path"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut bytes = fs::read(&wav).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&wav, bytes).unwrap();
    let out = proposals::propose("proposals", std::slice::from_ref(&t.record), None);
    let entry = &out["attempts"][&t.record.attempt_id];
    assert_eq!(entry["room"]["verified"], false);
    assert_eq!(entry["replies"], json!([]));
}
