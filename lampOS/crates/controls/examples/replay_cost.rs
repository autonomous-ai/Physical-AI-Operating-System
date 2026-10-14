//! Local replay cost, not input-to-silence latency or a device benchmark.
use lamp_controls::{engine::Config, replay};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repetitions: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "100".into())
        .parse()?;
    if !(1..=10_000).contains(&repetitions) {
        return Err("repetitions must be 1..10000".into());
    }
    let mut config: Config = serde_json::from_str(include_str!("../fixtures/controls.json"))?;
    config.enabled = true;
    let mut elapsed = Vec::with_capacity(repetitions);
    let mut last = None;
    for _ in 0..repetitions {
        let began = Instant::now();
        let records = replay::read(include_bytes!("../fixtures/hand-controls.jsonl").as_slice())?;
        let summary = replay::run(
            config.clone(),
            "written-controls",
            &records,
            None,
            std::io::sink(),
        )?;
        elapsed.push(began.elapsed().as_nanos());
        last = Some(summary);
    }
    let first_ns = elapsed[0];
    let mut ordered = elapsed.clone();
    ordered.sort_unstable();
    println!(
        "{}",
        serde_json::json!({
            "boundary":"input parse, engine construction, injected replay and JSON serialization to sink; configuration parsing precedes timing",
            "repetitions":repetitions,"first_ns":first_ns,"median_ns":ordered[ordered.len()/2],
            "worst_ns":ordered.last(),"runs_ns":elapsed,"last_replay":last,
            "physical_latency_measured":false
        })
    );
    Ok(())
}
