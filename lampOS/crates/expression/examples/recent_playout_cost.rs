//! Compare host computation for the same authored trace in strict/default
//! mode and the optional 100 ms recent-level mode. This is not a transport,
//! acoustic or motor benchmark. Run on each machine being compared:
//! cargo run --offline --release -p lamp-expression --example recent_playout_cost
use lamp_expression::{Catalogue, Engine, replay};
use std::{io, time::Instant};

const FIXTURE: &[u8] = include_bytes!("../fixtures/recent-playout.jsonl");
const CLOCK: &str = "recent-playout-fixture";
const WARM_RUNS: usize = 100;
const EXPIRED: &str = "playout is expired, too far ahead, overlapping or out of order";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("usage: recent_playout_cost (one first run and 100 warm runs per mode)".into());
    }
    let recent = Catalogue::from_json(include_str!("../playout-behaviours.json"))?;
    let modes = [
        ("strict_default", Catalogue::builtin(), 2),
        ("recent_100ms", recent, 0),
    ];
    let mut results = Vec::new();
    for (name, catalogue, refused) in modes {
        let mut durations = Vec::with_capacity(WARM_RUNS + 1);
        for _ in 0..=WARM_RUNS {
            let began = Instant::now();
            let mut engine = Engine::new(catalogue.clone(), true, CLOCK)?;
            let summary = replay::run(FIXTURE, io::sink(), &mut engine, Some(400_000))?;
            durations.push(u64::try_from(began.elapsed().as_nanos())?);
            if (
                summary.input_records,
                summary.accepted,
                summary.rejected,
                summary.ticks,
            ) != (15, 15 - refused, refused, 21)
                || summary.warnings.len() != refused
            {
                return Err(
                    format!("{name}: counts differ from the authored regression trace").into(),
                );
            }
            for (warning, line) in summary.warnings.iter().zip([6, 7]) {
                if warning.kind != replay::WarningKind::RejectedInput
                    || warning.line != Some(line)
                    || warning.detail != EXPIRED
                {
                    return Err(format!("{name}: unexpected refusal reason").into());
                }
            }
        }
        let mut warm = durations[1..].to_vec();
        warm.sort_unstable();
        let median = (warm[WARM_RUNS / 2 - 1] as f64 + warm[WARM_RUNS / 2] as f64) / 2.0;
        results.push(serde_json::json!({
            "mode":name,
            "recent_playout_us":catalogue.timing.recent_playout_us,
            "input_records_per_run":15,
            "accepted_per_run":15-refused,
            "expected_refusals_per_run":refused,
            "ticks_per_run":21,
            "first_ns":durations[0],
            "warm_runs":WARM_RUNS,
            "warm_median_ns":median,
            "warm_worst_ns":warm[WARM_RUNS-1],
            "all_runs_ns_in_order":durations,
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "kind":"host_computation_only",
            "os":std::env::consts::OS,
            "arch":std::env::consts::ARCH,
            "debug_assertions":cfg!(debug_assertions),
            "fixture":"crates/expression/fixtures/recent-playout.jsonl",
            "fixture_provenance":"authored synthetic data, not a lamp recording",
            "authored_until_us":400000,
            "boundary":"clone catalogue, construct engine, parse JSONL, plan on injected clock, serialize outputs to sink",
            "catalogue_parsing_timed":false,
            "physical_or_transport_measurement":false,
            "body_lag_known":false,
            "comparison":"two modes of this build, strict first then optional; not a historical binary comparison",
            "authored_timing":{
                "first_three_frame_durations_us":20000,
                "first_three_publication_offsets_from_start_us":1700,
                "first_three_transport_delays_us":[0,20000,20000],
                "first_three_arrival_to_next_tick_us":[18300,18300,18300],
                "physical_timing_measured":false
            },
            "results":results,
        }))?
    );
    Ok(())
}
