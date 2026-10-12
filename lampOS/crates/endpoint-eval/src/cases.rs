//! The synthetic `say` stress cohort (`endpoint-cases.json`): spliced macOS
//! voices with chosen pauses. Its final intonation is artificial, so it is
//! kept apart from human clips; it remains a regression stress test.
use crate::{
    Result,
    audio::{RATE, finish, gain, read_wav, say, silence},
    replay::{self, Event},
};
use lamp_endpoint::{SMART_TURN_V3_2, SmartTurn};
use lamp_live::activity::TurnDetector;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

#[derive(Deserialize)]
struct Cases {
    version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    /// `one_turn`, `fragment` (never finished) or `either`.
    expect: String,
    clips: Vec<Clip>,
}

/// Either `say` text (`text`, `voice`, `wpm`) or `file`, a 16 kHz mono PCM16
/// WAV relative to the cache.
#[derive(Deserialize)]
struct Clip {
    #[serde(default)]
    text: String,
    #[serde(default)]
    voice: String,
    #[serde(default)]
    wpm: u32,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    gain_db: Option<f64>,
    #[serde(default)]
    gap_ms: usize,
}

const LEAD_MS: usize = 600;
const TAIL_MS: usize = 3_000;

/// First and last sample above 1.5% of a clip's peak, per clip.
type Speech = Vec<(usize, usize)>;

/// The composed audio and its speech spans.
fn compose(case: &Case, cache: &Path) -> Result<(Vec<i16>, Speech)> {
    let mut pcm = silence(LEAD_MS);
    let mut speech = Vec::new();
    for clip in &case.clips {
        pcm.extend(silence(clip.gap_ms));
        let mut samples = match &clip.file {
            Some(file) => read_wav(&cache.join(file))?,
            None => say(&clip.voice, clip.wpm, &clip.text, cache)?,
        };
        gain(&mut samples, clip.gain_db.unwrap_or(-6.0));
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
    pcm.extend(silence(TAIL_MS));
    Ok((finish(pcm, &case.id), speech))
}

fn score(case: &Case, speech: &[(usize, usize)], events: &[Event]) -> Value {
    let ms = |sample: usize| (sample * 1_000 / RATE) as u64;
    let final_end = ms(speech.last().expect("one clip").1);
    let ends: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::End { at_block, decision } => Some(((*at_block as u64 + 1) * 10, decision)),
            Event::Start { .. } => None,
        })
        .collect();
    let early = ends.iter().filter(|(at, _)| *at < final_end).count();
    let last = ends.iter().find(|(at, _)| *at >= final_end);
    json!({
        "id": case.id, "expect": case.expect,
        "early_endpoints": early,
        "false_split": case.expect == "one_turn" && early > 0,
        "fragment_cut": case.expect == "fragment" && ends.iter().any(|(_, d)| d.basis == "model_complete"),
        "delay_ms": last.map(|(at, _)| at - final_end),
        "basis": last.map(|(_, d)| d.basis),
        "probability": last.and_then(|(_, d)| d.probability),
    })
}

fn summary(results: &[Value]) -> Value {
    let one_turn: Vec<_> = results
        .iter()
        .filter(|r| r["expect"] == "one_turn")
        .collect();
    let mut delays: Vec<u64> = one_turn
        .iter()
        .filter(|r| r["false_split"] == false)
        .filter_map(|r| r["delay_ms"].as_u64())
        .collect();
    delays.sort_unstable();
    let mut bases = BTreeMap::<String, u64>::new();
    for result in results {
        if let Some(basis) = result["basis"].as_str() {
            *bases.entry(basis.into()).or_default() += 1;
        }
    }
    json!({
        "one_turn_cases": one_turn.len(),
        "false_splits": one_turn.iter().filter(|r| r["false_split"] == true).map(|r| r["id"].clone()).collect::<Vec<_>>(),
        "missed_endpoints": results.iter().filter(|r| r["delay_ms"].is_null()).map(|r| r["id"].clone()).collect::<Vec<_>>(),
        "fragments_cut_by_model": results.iter().filter(|r| r["fragment_cut"] == true).count(),
        "either_split": results.iter().filter(|r| r["expect"] == "either" && r["early_endpoints"].as_u64() > Some(0)).map(|r| r["id"].clone()).collect::<Vec<_>>(),
        "delay_ms": {"p50": delays.get(delays.len() / 2), "p90": delays.get(delays.len() * 9 / 10), "max": delays.last()},
        "bases": bases,
    })
}

/// The cohort as end-to-end turn fixtures: a fragment is unfinished; single
/// turns and the ambiguous cases end finished.
pub fn turn_items(cases: &Path, cache: &Path) -> Result<Vec<(crate::turns::Item, Vec<i16>)>> {
    let cases: Cases = serde_json::from_slice(&fs::read(cases)?)?;
    if cases.version != 1 {
        return Err("unsupported case file".into());
    }
    cases
        .cases
        .iter()
        .map(|case| {
            let (pcm, _) = compose(case, cache)?;
            let item = crate::turns::Item {
                id: case.id.clone(),
                label: case.expect != "fragment",
            };
            Ok((item, pcm))
        })
        .collect()
}

pub fn run(
    cases: &Path,
    cache: &Path,
    model: &Path,
    latency_blocks: u16,
    out: &Path,
) -> Result<()> {
    let cases: Cases = serde_json::from_slice(&fs::read(cases)?)?;
    if cases.version != 1 {
        return Err("unsupported case file".into());
    }
    let composed = cases
        .cases
        .iter()
        .map(|case| compose(case, cache))
        .collect::<Result<Vec<_>>>()?;
    let model = Arc::new(SmartTurn::load(model, &SMART_TURN_V3_2)?);
    let mut report = serde_json::Map::new();
    for (name, policy) in replay::profiles() {
        let mut results = Vec::new();
        for (case, (pcm, speech)) in cases.cases.iter().zip(&composed) {
            let mut detector = match policy {
                Some(policy) => TurnDetector::v1_baseline().with_confirmation(
                    replay::virtual_classifier(&model, latency_blocks),
                    policy,
                )?,
                None => TurnDetector::v1_baseline(),
            };
            let events = replay::replay(pcm, &mut detector, false)?;
            results.push(score(case, speech, &events));
        }
        let summary = summary(&results);
        println!("{name}: {summary}");
        report.insert(name.into(), json!({"summary": summary, "cases": results}));
    }
    let report = json!({
        "cohort": "synthetic macOS say speech, spliced; not human, room or device audio",
        "virtual_latency_ms": latency_blocks * 10,
        "model": {"file": SMART_TURN_V3_2.file_name, "sha256": SMART_TURN_V3_2.sha256},
        "profiles": report,
    });
    fs::write(out, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
