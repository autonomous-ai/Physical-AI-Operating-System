//! Rust features and tract inference versus Pipecat's pinned reference, and
//! the real `lamp-endpoint-worker` process behind the core's client.
//! Model tests are ignored by default: run `crates/endpoint/fetch-model.sh`
//! first, then `cargo test -p lamp-endpoint -- --include-ignored`.
use lamp_endpoint::{FeatureExtractor, SMART_TURN_V3_2, SmartTurn, WINDOW_SAMPLES};
use lamp_endpoint_ipc::{
    Availability, Classifier, Limits, ProcessClassifier, Submit, Teardown, Token, WorkerCommand,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const REFERENCE: &str = include_str!("fixtures/pipecat-reference.json");

fn xorshift(seed: u32) -> impl FnMut() -> u32 {
    let mut state = seed;
    move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    }
}

/// The same integer signals as `fixtures/make_reference.py`.
fn signal(name: &str) -> Vec<i16> {
    let noise = |seed, count, modulus: u32, offset: i32| {
        let mut next = xorshift(seed);
        (0..count)
            .map(|_| (next() % modulus) as i32 - offset)
            .map(|value| value as i16)
            .collect::<Vec<_>>()
    };
    match name {
        "noise_2s" => noise(1, 32_000, 12_001, 6_000),
        "short_0p3s" => noise(7, 4_800, 6_001, 3_000),
        "voiced_9s" => {
            let mut next = xorshift(2);
            (0..144_000)
                .map(|i| {
                    let pulse = if i % 100 < 3 { 9_000 } else { 0 };
                    let value = pulse * ((i / 1_600) % 10 + 1) / 10 + (next() % 801) as i32 - 400;
                    value as i16
                })
                .collect()
        }
        "silence" => Vec::new(),
        other => panic!("unknown reference signal {other}"),
    }
}

fn reference() -> Value {
    serde_json::from_str(REFERENCE).unwrap()
}

fn model_path() -> PathBuf {
    std::env::var_os("LAMP_SMART_TURN_MODEL").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../.cache/endpoint")
                .join(SMART_TURN_V3_2.file_name)
        },
        PathBuf::from,
    )
}

#[test]
fn features_match_pipecat_reference() {
    let reference = reference();
    assert_eq!(reference["model_sha256"], SMART_TURN_V3_2.sha256);
    let mut extractor = FeatureExtractor::default();
    for (name, case) in reference["cases"].as_object().unwrap() {
        let pcm = signal(name);
        assert_eq!(pcm.len() as u64, case["samples"].as_u64().unwrap());
        let features = extractor.compute(&pcm);
        let stride = case["stride"].as_u64().unwrap() as usize;
        let mut worst = 0f64;
        for (index, expected) in case["sampled"].as_array().unwrap().iter().enumerate() {
            let actual = f64::from(features[index * stride]);
            worst = worst.max((actual - expected.as_f64().unwrap()).abs());
        }
        let sum = features.iter().map(|&v| f64::from(v)).sum::<f64>();
        let abs_sum = features.iter().map(|&v| f64::from(v).abs()).sum::<f64>();
        eprintln!("{name}: max sampled difference {worst:e}");
        assert!(worst < 1e-4, "{name}: sampled feature difference {worst}");
        // 64,000 values: a mean absolute difference below 1e-6.
        assert!(
            (sum - case["sum"].as_f64().unwrap()).abs() < 0.064,
            "{name} sum"
        );
        assert!(
            (abs_sum - case["abs_sum"].as_f64().unwrap()).abs() < 0.064,
            "{name} abs sum"
        );
    }
}

#[test]
#[ignore = "requires the pinned model; run crates/endpoint/fetch-model.sh"]
fn tract_probability_matches_onnx_runtime_reference() {
    let model = SmartTurn::load(&model_path(), &SMART_TURN_V3_2).unwrap();
    let mut extractor = FeatureExtractor::default();
    for (name, case) in reference()["cases"].as_object().unwrap() {
        let probability = model.probability(extractor.compute(&signal(name))).unwrap();
        let expected = case["probability"].as_f64().unwrap();
        eprintln!("{name}: tract {probability:.7} onnxruntime {expected:.7}");
        assert!((f64::from(probability) - expected).abs() < 1e-4, "{name}");
    }
}

#[test]
fn model_load_refuses_wrong_bytes_before_parsing() {
    let dir = std::env::temp_dir().join(format!("lamp-endpoint-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("model.onnx");
    std::fs::write(&path, b"not a model").unwrap();
    let error = SmartTurn::load(&path, &SMART_TURN_V3_2).err().unwrap();
    assert!(error.to_string().contains("unexpected size"), "{error}");
    let mut same_size = SMART_TURN_V3_2;
    same_size.bytes = 11;
    let error = SmartTurn::load(&path, &same_size).err().unwrap();
    assert!(error.to_string().contains("pinned SHA256"), "{error}");
    std::fs::remove_dir_all(dir).unwrap();
}

fn worker(model: &Path) -> ProcessClassifier {
    ProcessClassifier::spawn(
        WorkerCommand::smart_turn(
            env!("CARGO_BIN_EXE_lamp-endpoint-worker").into(),
            model.into(),
        ),
        Limits::default(),
    )
}

fn settle(classifier: &ProcessClassifier, limit: Duration) -> Availability {
    let started = Instant::now();
    loop {
        let state = classifier.availability();
        if state != Availability::Loading || started.elapsed() > limit {
            return state;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

const TOKEN: Token = Token {
    utterance: 4,
    generation: 2,
    check: 0,
};

#[test]
fn worker_process_refuses_missing_or_wrong_models_without_blocking() {
    let dir = std::env::temp_dir().join(format!("lamp-endpoint-worker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wrong = dir.join("model.onnx");
    std::fs::write(&wrong, b"not a model").unwrap();
    for (model, expected) in [
        (dir.join("missing.onnx"), "No such file"),
        (wrong, "unexpected size"),
    ] {
        let started = Instant::now();
        let mut classifier = worker(&model);
        assert_eq!(classifier.submit(TOKEN, &[1; 160]), Submit::Unavailable);
        assert!(started.elapsed() < Duration::from_millis(100));
        let Availability::Failed(reason) = settle(&classifier, Duration::from_secs(10)) else {
            panic!("{} did not fail", model.display());
        };
        assert!(reason.contains(expected), "{reason}");
        assert_eq!(
            classifier.wait_finished(Duration::from_secs(2)),
            Teardown::Reaped
        );
        let report = classifier.report();
        assert_eq!((report.spawned, report.ready, report.kills), (1, 0, 0));
        assert_eq!(classifier.submit(TOKEN, &[1; 160]), Submit::Unavailable);
        assert!(classifier.poll().is_none());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires the pinned model; run crates/endpoint/fetch-model.sh"]
fn worker_process_matches_reference_and_reports_stage_costs() {
    let mut classifier = worker(&model_path());
    assert_eq!(
        settle(&classifier, Duration::from_secs(30)),
        Availability::Ready
    );
    let report = classifier.report();
    assert_eq!(report.model_sha256.as_deref(), Some(SMART_TURN_V3_2.sha256));
    eprintln!(
        "worker load {} ms, cold inference {} ms, spawn to ready {} ms",
        report.load_us.unwrap() / 1_000,
        report.cold_us.unwrap() / 1_000,
        report.ready_after_us.unwrap() / 1_000
    );
    let reference = reference();
    for (name, check) in [("voiced_9s", 0u8), ("noise_2s", 1), ("short_0p3s", 2)] {
        let token = Token { check, ..TOKEN };
        assert_eq!(classifier.submit(token, &signal(name)), Submit::Accepted);
        assert_eq!(
            classifier.submit(Token { check: 9, ..token }, &[0; 160]),
            Submit::Busy
        );
        let started = Instant::now();
        let outcome = loop {
            if let Some(outcome) = classifier.poll() {
                break outcome;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "no result");
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!((outcome.token, outcome.worker), (token, 1));
        let verdict = outcome.result.unwrap();
        let expected = reference["cases"][name]["probability"].as_f64().unwrap();
        assert!(
            (f64::from(verdict.probability) - expected).abs() < 1e-4,
            "{name}"
        );
        eprintln!("{name}: {verdict:?}");
    }
    // A long request is truncated to the model window before it is copied.
    let long = vec![0i16; 3 * WINDOW_SAMPLES];
    assert_eq!(classifier.submit(TOKEN, &long), Submit::Accepted);
    let pid = classifier.report().pid.unwrap();
    classifier.close();
    assert_eq!(
        classifier.wait_finished(Duration::from_secs(5)),
        Teardown::Reaped
    );
    assert!(classifier.report().last_exit.is_some(), "{pid} not reaped");
}
