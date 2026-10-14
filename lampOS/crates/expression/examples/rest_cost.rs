//! Host computation for authored hold/resume and finishing-nod scenarios.
//! The timed boundary includes engine construction, JSONL parsing, injected
//! planning and output serialization to a sink. No transport, sleeping, sound
//! or physical movement is timed. Intentional stale-frame refusals are checked.
use lamp_expression::{Catalogue, Engine, replay};
use std::{io, time::Instant};

struct Scenario {
    name: &'static str,
    fixture: &'static [u8],
    until_us: u64,
    records: usize,
    refused: usize,
    ticks: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() > 1 {
        return Err("usage: rest_cost [WARM_RUNS]".into());
    }
    let warm_runs = args
        .first()
        .map_or(Ok(100), |value| value.parse::<usize>())?;
    if !(1..=10_000).contains(&warm_runs) {
        return Err("warm runs must be 1..10000".into());
    }
    let catalogue = Catalogue::from_json(include_str!("../rest-behaviours.json"))?;
    let scenarios = [
        Scenario {
            name: "held-resumed",
            fixture: include_bytes!("../fixtures/held-resumed.jsonl"),
            until_us: 300_000,
            records: 18,
            refused: 4,
            ticks: 16,
        },
        Scenario {
            name: "finish-nod",
            fixture: include_bytes!("../fixtures/finish-nod.jsonl"),
            until_us: 1_800_000,
            records: 37,
            refused: 0,
            ticks: 91,
        },
    ];
    let mut results = Vec::new();
    for scenario in scenarios {
        let mut durations = Vec::with_capacity(warm_runs + 1);
        for _ in 0..=warm_runs {
            let began = Instant::now();
            let mut engine = Engine::new(catalogue.clone(), true, "expression-rest-fixture")?;
            let summary = replay::run(
                scenario.fixture,
                io::sink(),
                &mut engine,
                Some(scenario.until_us),
            )?;
            durations.push(u64::try_from(began.elapsed().as_nanos())?);
            if (summary.input_records, summary.rejected, summary.ticks)
                != (scenario.records, scenario.refused, scenario.ticks)
            {
                return Err(format!(
                    "{}: replay shape differs from its regression fixture",
                    scenario.name
                )
                .into());
            }
        }
        let mut warm = durations[1..].to_vec();
        warm.sort_unstable();
        results.push(serde_json::json!({
            "scenario":scenario.name,
            "authored_until_us":scenario.until_us,
            "input_records_per_run":scenario.records,
            "intentionally_refused_inputs_per_run":scenario.refused,
            "ticks_per_run":scenario.ticks,
            "first_ns":durations[0],
            "warm_runs":warm_runs,
            "warm_median_ns":warm[warm_runs/2],
            "warm_worst_ns":warm[warm_runs-1],
            "all_runs_ns_in_order":durations,
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "kind":"host_computation_only",
            "os":std::env::consts::OS,
            "arch":std::env::consts::ARCH,
            "boundary":"construct engine, parse authored JSONL, plan on injected clock, serialize outputs to a sink",
            "catalogue_parsing_timed":false,
            "physical_or_transport_measurement":false,
            "body_lag_known":false,
            "results":results,
        }))?
    );
    Ok(())
}
