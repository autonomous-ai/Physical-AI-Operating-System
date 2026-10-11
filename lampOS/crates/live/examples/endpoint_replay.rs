//! Offline replay of the endpoint candidate on synthetic speech fixtures.
//!
//! Renders each clip once with macOS `say` (same arguments as the acoustic
//! stimulus cache), adds a deterministic noise floor, computes the Rust VAD
//! score per 10 ms block and replays the actual `TurnDetector` variants:
//! `vad-600`, `v1-1200` and `candidate` (V1 baseline plus Smart Turn).
//!
//! `--classify DIR` instead scores every `DIR/labels.json` WAV once with the
//! Rust features and model (for labelled upstream test clips).
//!
//! `--mode virtual` runs inference synchronously and releases each verdict
//! `--latency-ms` later in capture time (reproducible; emulates a slower CPU).
//! `--mode paced` feeds blocks in real time to the real background worker.
//! This is synthetic input on a development host, not device evidence.
//!
//! cargo run --release -p lamp-live --example endpoint_replay -- \
//!   --cases crates/live/examples/endpoint-cases.json --cache .cache/endpoint/speech \
//!   --model .cache/endpoint/smart-turn-v3.2-gpu.onnx --out REPORT.json
use lamp_endpoint::{
    Availability, Classifier, FeatureExtractor, Outcome, SMART_TURN_V3_2, SmartTurn,
    SmartTurnWorker, Submit, Token, Verdict,
};
use lamp_live::activity::{
    Activity, ConfirmPolicy, EndpointDecision, ObservedAudio, SpeechProbability, TurnDetector,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const RATE: usize = 16_000;
const BLOCK: usize = 160;
const LEAD_MS: usize = 600;
const TAIL_MS: usize = 3_000;
/// Uniform noise of +/-40 LSB: about -63 dBFS RMS.
const NOISE: u32 = 40;

#[derive(Deserialize)]
struct Cases {
    version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    expect: String,
    clips: Vec<Clip>,
}

/// Either `say` text (`text`, `voice`, `wpm`) or `file`, a 16 kHz mono PCM16
/// WAV relative to `--cache`.
#[derive(Deserialize)]
struct Clip {
    #[serde(default)]
    text: String,
    #[serde(default)]
    voice: String,
    #[serde(default)]
    wpm: u32,
    #[serde(default)]
    file: Option<PathBuf>,
    #[serde(default)]
    gain_db: Option<f64>,
    #[serde(default)]
    gap_ms: usize,
}

fn render(clip: &Clip, cache: &Path) -> Result<Vec<i16>> {
    if let Some(file) = &clip.file {
        return read_wav(&cache.join(file));
    }
    let key = Sha256::digest(format!("{}\n{}\n{}", clip.voice, clip.wpm, clip.text));
    let name = key.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let path = cache.join(format!("{name}.wav"));
    if !path.exists() {
        if !cfg!(target_os = "macos") {
            return Err(format!("missing rendered clip {}", path.display()).into());
        }
        fs::create_dir_all(cache)?;
        let partial = cache.join(format!(".{name}.wav"));
        let status = Command::new("/usr/bin/say")
            .args(["-v", &clip.voice, "-r", &clip.wpm.to_string(), "-o"])
            .arg(&partial)
            .args([
                "--file-format=WAVE",
                "--data-format=LEI16@16000",
                &clip.text,
            ])
            .status()?;
        if !status.success() {
            return Err(format!("say failed for {:?}", clip.text).into());
        }
        fs::rename(&partial, &path)?;
    }
    read_wav(&path)
}

fn read_wav(path: &Path) -> Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate != RATE as u32 || spec.channels != 1 || spec.bits_per_sample != 16 {
        return Err(format!("{} is not 16 kHz mono PCM16", path.display()).into());
    }
    Ok(reader
        .samples::<i16>()
        .collect::<std::result::Result<_, _>>()?)
}

struct Composed {
    pcm: Vec<i16>,
    /// Per clip: first and last sample above 1.5% of the clip's peak.
    speech: Vec<(usize, usize)>,
}

fn compose(case: &Case, cache: &Path) -> Result<Composed> {
    let mut pcm = vec![0i16; LEAD_MS * RATE / 1_000];
    let mut speech = Vec::new();
    for clip in &case.clips {
        pcm.resize(pcm.len() + clip.gap_ms * RATE / 1_000, 0);
        let gain = 10f64.powf(clip.gain_db.unwrap_or(-6.0) / 20.0);
        let samples = render(clip, cache)?
            .into_iter()
            .map(|s| (f64::from(s) * gain).round().clamp(-32_768.0, 32_767.0) as i16)
            .collect::<Vec<_>>();
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        let threshold = (u32::from(peak) * 15 / 1_000).max(1) as u16;
        let first = samples.iter().position(|s| s.unsigned_abs() >= threshold);
        let last = samples.iter().rposition(|s| s.unsigned_abs() >= threshold);
        let (Some(first), Some(last)) = (first, last) else {
            return Err(format!("{}: silent clip", case.id).into());
        };
        speech.push((pcm.len() + first, pcm.len() + last));
        pcm.extend(samples);
    }
    pcm.resize(pcm.len() + TAIL_MS * RATE / 1_000, 0);
    pcm.resize(pcm.len().div_ceil(BLOCK) * BLOCK, 0);
    let mut state = case.id.bytes().fold(2_166_136_261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    }) | 1;
    for sample in &mut pcm {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = (state % (2 * NOISE + 1)) as i32 - NOISE as i32;
        *sample = (i32::from(*sample) + noise).clamp(-32_768, 32_767) as i16;
    }
    Ok(Composed { pcm, speech })
}

/// Synchronous inference whose verdict becomes visible `latency` blocks later.
struct Virtual {
    model: Arc<SmartTurn>,
    extractor: FeatureExtractor,
    latency: u16,
    pending: Option<(u16, Outcome)>,
}

impl Classifier for Virtual {
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit {
        if self.pending.is_some() {
            return Submit::Busy;
        }
        let started = Instant::now();
        let features = self.extractor.compute(pcm);
        let features_us = started.elapsed().as_micros() as u64;
        let started = Instant::now();
        let result = self
            .model
            .probability(features)
            .map(|probability| Verdict {
                probability,
                features_us,
                inference_us: started.elapsed().as_micros() as u64,
            })
            .map_err(|error| error.to_string());
        self.pending = Some((self.latency, Outcome { token, result }));
        Submit::Accepted
    }
    fn poll(&mut self) -> Option<Outcome> {
        match self.pending.as_mut() {
            Some((remaining, _)) if *remaining > 1 => {
                *remaining -= 1;
                None
            }
            Some(_) => self.pending.take().map(|(_, outcome)| outcome),
            None => None,
        }
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
}

/// One background worker shared by every case, as in one session.
#[derive(Clone)]
struct Shared(Arc<Mutex<SmartTurnWorker>>);

impl Classifier for Shared {
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit {
        self.0.lock().unwrap().submit(token, pcm)
    }
    fn poll(&mut self) -> Option<Outcome> {
        self.0.lock().unwrap().poll()
    }
    fn availability(&self) -> Availability {
        self.0.lock().unwrap().availability()
    }
}

#[derive(Debug)]
enum Event {
    Start {
        at_ms: u64,
        prefix_ms: u64,
    },
    End {
        at_ms: u64,
        decision: EndpointDecision,
    },
}

fn replay(pcm: &[i16], detector: &mut TurnDetector, paced: bool) -> Result<Vec<Event>> {
    let mut vad = SpeechProbability::default();
    let mut events = Vec::new();
    let started = Instant::now();
    for (index, chunk) in pcm.chunks_exact(BLOCK).enumerate() {
        if paced {
            let due = Duration::from_millis(index as u64 * 10);
            if let Some(wait) = due.checked_sub(started.elapsed()) {
                thread::sleep(wait);
            }
        }
        let samples: [i16; BLOCK] = chunk.try_into()?;
        let probability = vad.analyze(&samples);
        let at_us = (index as u64 + 1) * 10_000;
        let audio = ObservedAudio {
            sequence: index as u64 + 1,
            captured_at_us: at_us,
            samples,
        };
        match detector.push(audio, probability) {
            Activity::Start(prefix) => events.push(Event::Start {
                at_ms: at_us / 1_000,
                prefix_ms: prefix[0].captured_at_us / 1_000 - 10,
            }),
            Activity::End(_) => events.push(Event::End {
                at_ms: at_us / 1_000,
                decision: detector
                    .last_endpoint()
                    .cloned()
                    .ok_or("End without decision")?,
            }),
            Activity::Fault(reason) => return Err(reason.into()),
            Activity::Quiet | Activity::Continue(_) => {}
        }
    }
    Ok(events)
}

fn score(case: &Case, composed: &Composed, events: &[Event]) -> Value {
    let ms = |sample: usize| (sample * 1_000 / RATE) as u64;
    let first_speech = ms(composed.speech[0].0);
    let (final_start, final_end) = *composed.speech.last().unwrap();
    let (final_start, final_end) = (ms(final_start), ms(final_end));
    let ends = events
        .iter()
        .filter_map(|event| match event {
            Event::End { at_ms, decision } => Some((*at_ms, decision)),
            Event::Start { .. } => None,
        })
        .collect::<Vec<_>>();
    let starts = events
        .iter()
        .filter_map(|event| match event {
            Event::Start { at_ms, .. } => Some(*at_ms),
            Event::End { .. } => None,
        })
        .collect::<Vec<_>>();
    let early = ends.iter().filter(|(at, _)| *at < final_end).count();
    let last = ends.iter().find(|(at, _)| *at >= final_end);
    let prefix_ms = events.iter().find_map(|event| match event {
        Event::Start { prefix_ms, .. } => Some(*prefix_ms),
        Event::End { .. } => None,
    });
    json!({
        "id": case.id,
        "expect": case.expect,
        "speech_ms": composed.speech.iter().map(|(a, b)| [ms(*a), ms(*b)]).collect::<Vec<_>>(),
        "starts": starts,
        "ends": ends.iter().map(|(at, d)| json!({"at_ms": at, "decision": d})).collect::<Vec<_>>(),
        "early_endpoints": early,
        "false_split": case.expect == "one_turn" && early > 0,
        "split_before_final_start": ends.iter().any(|(at, _)| *at < final_start),
        "delay_ms": last.map(|(at, _)| at - final_end),
        "basis": last.map(|(_, d)| d.basis),
        "probability": last.and_then(|(_, d)| d.probability),
        "onset_margin_ms": prefix_ms.map(|prefix| first_speech as i64 - prefix as i64),
    })
}

fn percentile(values: &mut [u64], q: f64) -> Option<u64> {
    values.sort_unstable();
    (!values.is_empty()).then(|| values[((values.len() - 1) as f64 * q).round() as usize])
}

fn summary(results: &[Value]) -> Value {
    let one_turn = results.iter().filter(|r| r["expect"] == "one_turn");
    let mut delays = one_turn
        .clone()
        .filter(|r| r["false_split"] == false)
        .filter_map(|r| r["delay_ms"].as_u64())
        .collect::<Vec<_>>();
    let mut bases = BTreeMap::<String, u64>::new();
    for result in results {
        if let Some(basis) = result["basis"].as_str() {
            *bases.entry(basis.into()).or_default() += 1;
        }
    }
    json!({
        "one_turn_cases": one_turn.clone().count(),
        "false_splits": one_turn.clone().filter(|r| r["false_split"] == true).map(|r| r["id"].clone()).collect::<Vec<_>>(),
        "missed_endpoints": results.iter().filter(|r| r["delay_ms"].is_null()).map(|r| r["id"].clone()).collect::<Vec<_>>(),
        "fragments_cut_by_model": results.iter().filter(|r| r["expect"] == "fragment" && r["ends"].as_array().is_some_and(|ends| ends.iter().any(|e| e["decision"]["basis"] == "model_complete"))).count(),
        "fragments": results.iter().filter(|r| r["expect"] == "fragment").count(),
        "either_split": results.iter().filter(|r| r["expect"] == "either" && r["early_endpoints"].as_u64() > Some(0)).map(|r| r["id"].clone()).collect::<Vec<_>>(),
        "delay_ms": {"min": delays.iter().min(), "p50": percentile(&mut delays, 0.5), "p90": percentile(&mut delays, 0.9), "max": delays.iter().max()},
        "bases": bases,
    })
}

fn argument(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1).cloned())
}

/// Probability for each labelled WAV in `dir/labels.json`, in file order.
fn classify(dir: &Path, model: &Path, out: &Path) -> Result<()> {
    let labels: Vec<Value> = serde_json::from_slice(&fs::read(dir.join("labels.json"))?)?;
    let model = SmartTurn::load(model, &SMART_TURN_V3_2)?;
    let mut extractor = FeatureExtractor::default();
    let mut scored = Vec::with_capacity(labels.len());
    for label in labels {
        let file = label["file"].as_str().ok_or("label without file")?;
        let pcm = read_wav(&dir.join(file))?;
        let started = Instant::now();
        let probability = model.probability(extractor.compute(&pcm))?;
        scored.push(json!({"file": file, "probability": probability,
            "elapsed_us": started.elapsed().as_micros() as u64}));
    }
    fs::write(out, serde_json::to_vec(&scored)?)?;
    Ok(())
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let required = |name| argument(&args, name).ok_or(format!("missing {name}"));
    if let Some(dir) = argument(&args, "--classify") {
        let model = PathBuf::from(required("--model")?);
        return classify(Path::new(&dir), &model, Path::new(&required("--out")?));
    }
    let cases: Cases = serde_json::from_slice(&fs::read(required("--cases")?)?)?;
    if cases.version != 1 {
        return Err("unsupported case file".into());
    }
    let cache = PathBuf::from(required("--cache")?);
    let out = PathBuf::from(required("--out")?);
    let model_path = argument(&args, "--model").map(PathBuf::from);
    let mode = argument(&args, "--mode").unwrap_or_else(|| "virtual".into());
    let latency_ms = argument(&args, "--latency-ms").map_or(Ok(100), |v| v.parse::<u16>())?;
    let mut policy = ConfirmPolicy::default();
    if let Some(value) = argument(&args, "--threshold") {
        policy.complete_probability = value.parse()?;
    }
    if let Some(value) = argument(&args, "--check-ms") {
        policy.check_blocks = value.parse::<u16>()? / 10;
    }
    if let Some(value) = argument(&args, "--commit-ms") {
        policy.commit_blocks = value.parse::<u16>()? / 10;
    }
    if let Some(value) = argument(&args, "--incomplete-ms") {
        policy.incomplete_blocks = value.parse::<u16>()? / 10;
    }
    let composed = cases
        .cases
        .iter()
        .map(|case| compose(case, &cache))
        .collect::<Result<Vec<_>>>()?;
    let mut variants = BTreeMap::new();
    for (name, make) in [
        ("vad-600", TurnDetector::default as fn() -> TurnDetector),
        ("v1-1200", TurnDetector::v1_baseline),
    ] {
        let results = cases
            .cases
            .iter()
            .zip(&composed)
            .map(|(case, c)| Ok(score(case, c, &replay(&c.pcm, &mut make(), false)?)))
            .collect::<Result<Vec<_>>>()?;
        variants.insert(
            name,
            json!({"summary": summary(&results), "cases": results}),
        );
    }
    if let Some(model_path) = model_path {
        let started = Instant::now();
        let startup;
        let classifier: Box<dyn Fn() -> Box<dyn Classifier>> = if mode == "paced" {
            let worker = SmartTurnWorker::spawn(model_path, SMART_TURN_V3_2)?;
            while worker.availability() == Availability::Loading {
                thread::sleep(Duration::from_millis(5));
            }
            if worker.availability() != Availability::Ready {
                return Err(format!("model unavailable: {:?}", worker.availability()).into());
            }
            let cold = worker.startup().ok_or("no startup timing")?;
            startup = json!({"load_us": cold.load_us, "cold_inference_us": cold.cold_inference_us});
            let shared = Shared(Arc::new(Mutex::new(worker)));
            Box::new(move || Box::new(shared.clone()))
        } else {
            let model = Arc::new(SmartTurn::load(&model_path, &SMART_TURN_V3_2)?);
            startup = json!({"load_us": started.elapsed().as_micros() as u64});
            let latency = latency_ms.div_ceil(10).max(1);
            Box::new(move || {
                Box::new(Virtual {
                    model: model.clone(),
                    extractor: FeatureExtractor::default(),
                    latency,
                    pending: None,
                })
            })
        };
        let mut timings = Vec::new();
        let mut results = Vec::new();
        for (case, c) in cases.cases.iter().zip(&composed) {
            let mut detector =
                TurnDetector::v1_baseline().with_confirmation(classifier(), policy)?;
            let events = replay(&c.pcm, &mut detector, mode == "paced")?;
            for event in &events {
                if let Event::End { decision, .. } = event
                    && let (Some(f), Some(i)) = (decision.features_us, decision.inference_us)
                {
                    timings.push(f + i);
                }
            }
            let result = score(case, c, &events);
            eprintln!(
                "{:<20} delay {:>5} basis {:<17} p {:<10} early {}",
                case.id,
                result["delay_ms"],
                result["basis"].as_str().unwrap_or("-"),
                result["probability"],
                result["early_endpoints"]
            );
            results.push(result);
        }
        let mut warm = timings.clone();
        variants.insert(
            "candidate",
            json!({
                "mode": mode, "latency_ms": (mode != "paced").then_some(latency_ms),
                "policy": policy, "startup": startup,
                "decisive_inference_us": {"count": warm.len(), "p50": percentile(&mut warm, 0.5), "max": warm.iter().max()},
                "summary": summary(&results), "cases": results,
            }),
        );
    }
    let report = json!({
        "source": "synthetic macOS say speech; not room or device audio",
        "model": model_path_label(&args),
        "variants": variants,
    });
    fs::write(&out, serde_json::to_vec_pretty(&report)?)?;
    for (name, variant) in report["variants"].as_object().unwrap() {
        println!("{name}: {}", variant["summary"]);
    }
    Ok(())
}

fn model_path_label(args: &[String]) -> Value {
    argument(args, "--model").map_or(
        Value::Null,
        |_| json!({"file": SMART_TURN_V3_2.file_name, "sha256": SMART_TURN_V3_2.sha256}),
    )
}
