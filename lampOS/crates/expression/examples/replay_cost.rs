//! Host-only computational cost of a fixed two-second written scenario.
//! This includes engine construction, input JSON parsing and output JSON
//! serialization to a sink. It does not measure transport or physical latency.
use lamp_expression::{Catalogue, Engine, replay};
use std::{io, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runs = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "100".into())
        .parse::<usize>()?;
    if !(1..=10_000).contains(&runs) {
        return Err("runs must be 1..10000".into());
    }
    let fixture = include_bytes!("../fixtures/five-behaviours.jsonl");
    let catalogue = Catalogue::builtin();
    let mut elapsed = Vec::with_capacity(runs);
    let mut expected = None;
    for _ in 0..runs {
        let began = Instant::now();
        let mut engine = Engine::new(catalogue.clone(), true, "written-scenario")?;
        let result = replay::run(fixture.as_slice(), io::sink(), &mut engine, Some(2_000_000))?;
        elapsed.push(began.elapsed().as_nanos() as u64);
        if result.rejected != 0 {
            return Err("written fixture rejected an input".into());
        }
        let shape = (result.input_records, result.ticks);
        if expected.is_some_and(|expected| expected != shape) {
            return Err("replay shape changed".into());
        }
        expected = Some(shape);
    }
    let first_ns = elapsed[0];
    elapsed.sort_unstable();
    let (input_records, ticks) = expected.unwrap();
    println!(
        "{}",
        serde_json::json!({
            "kind":"host_computation_only", "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
            "boundary":"construct engine, parse fixed JSONL, run two simulated seconds, serialize output to sink",
            "runs":runs, "input_records_per_run":input_records, "ticks_per_run":ticks,
            "first_ns":first_ns, "median_ns":elapsed[runs/2], "worst_ns":elapsed[runs-1],
            "rejected_inputs":0, "physical_measurement":false
        })
    );
    Ok(())
}
