//! Startup, per-stage latency and resource use of the real worker process,
//! measured through the core's client exactly as a session uses it.
use crate::{Result, audio::read_wav};
use lamp_endpoint_ipc::{
    Availability, Classifier, Limits, ProcessClassifier, Submit, Token, WINDOW_SAMPLES,
    WorkerCommand,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

/// Peak and current resident memory (kB) and CPU time (ms) of a process.
fn usage(pid: u32) -> Value {
    if let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) {
        let field = |name: &str| {
            status
                .lines()
                .find(|line| line.starts_with(name))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse::<u64>().ok())
        };
        let ticks = fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                let fields: Vec<&str> = stat.rsplit(") ").next()?.split_whitespace().collect();
                Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
            });
        // Linux USER_HZ is 100 on every supported target.
        return json!({"source": "proc", "vm_hwm_kb": field("VmHWM:"), "vm_rss_kb": field("VmRSS:"),
            "threads": field("Threads:"), "cpu_ms": ticks.map(|t| t * 10)});
    }
    let output = Command::new("ps")
        .args(["-o", "rss=,time=", "-p", &pid.to_string()])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    json!({"source": "ps", "rss_kb_and_cpu_time": output})
}

fn percentiles(mut values: Vec<u64>) -> Value {
    values.sort_unstable();
    let at = |q: f64| values.get(((values.len() - 1) as f64 * q).round() as usize);
    json!({"n": values.len(), "p50": at(0.5), "p90": at(0.9), "p99": at(0.99), "max": values.last()})
}

/// `windows` are request lengths in samples; audio comes from `speech`.
pub fn run(
    worker: &Path,
    model: &Path,
    speech: &Path,
    requests: usize,
    starts: usize,
    out: &Path,
) -> Result<()> {
    let audio = read_wav(speech)?;
    if audio.len() < WINDOW_SAMPLES {
        return Err("speech fixture shorter than 8 s".into());
    }
    let command = WorkerCommand::smart_turn(worker.to_path_buf(), model.to_path_buf());
    let mut startups = Vec::new();
    for _ in 0..starts {
        let mut client = ProcessClassifier::spawn(command.clone(), Limits::default());
        let started = Instant::now();
        while client.availability() == Availability::Loading {
            if started.elapsed() > Duration::from_secs(60) {
                return Err("worker never became ready".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        let report = client.report();
        startups.push(
            json!({"availability": format!("{:?}", client.availability()),
            "observed_ready_ms": started.elapsed().as_millis() as u64,
            "ready_after_us": report.ready_after_us, "load_us": report.load_us,
            "cold_us": report.cold_us, "after_ready": report.pid.map(usage)}),
        );
        client.close();
        client.wait_finished(Duration::from_secs(5));
    }
    let mut client = ProcessClassifier::spawn(command, Limits::default());
    while client.availability() == Availability::Loading {
        thread::sleep(Duration::from_millis(1));
    }
    let pid = client.report().pid.ok_or("no worker pid")?;
    let idle = usage(pid);
    let mut by_window = serde_json::Map::new();
    let windows = [16_000usize, 64_000, WINDOW_SAMPLES];
    let mut sustained = Vec::new();
    for &window in &windows {
        let mut stages: [Vec<u64>; 7] = Default::default();
        for index in 0..requests {
            let offset = (index * 1_601) % (audio.len() - window);
            let token = Token {
                utterance: index as u64,
                generation: 0,
                check: 0,
            };
            let started = Instant::now();
            if client.submit(token, &audio[offset..offset + window]) != Submit::Accepted {
                return Err(format!("request {index} not accepted").into());
            }
            let submit_us = started.elapsed().as_micros() as u64;
            let outcome = loop {
                if let Some(outcome) = client.poll() {
                    break outcome;
                }
                thread::sleep(Duration::from_micros(200));
            };
            let verdict = outcome.result?;
            for (slot, value) in stages.iter_mut().zip([
                submit_us,
                verdict.snapshot_us,
                verdict.delivery_us,
                verdict.features_us,
                verdict.inference_us,
                verdict.return_us,
                verdict.round_trip_us,
            ]) {
                slot.push(value);
            }
            if window == WINDOW_SAMPLES && index % 50 == 49 {
                sustained.push(json!({"requests": index + 1, "usage": usage(pid)}));
            }
        }
        let names = [
            "submit_call_us",
            "snapshot_us",
            "delivery_us",
            "features_us",
            "inference_us",
            "return_us",
            "round_trip_us",
        ];
        let summary: serde_json::Map<String, Value> = names
            .iter()
            .zip(stages)
            .map(|(name, values)| ((*name).to_owned(), percentiles(values)))
            .collect();
        by_window.insert(format!("{}_ms", window / 16), Value::Object(summary));
    }
    let report = client.report();
    let after = usage(pid);
    client.close();
    let reaped = client.wait_finished(Duration::from_secs(5));
    let result = json!({
        "host": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "boundary": "development host or VM; not Lamp hardware",
        "starts": startups, "idle_after_ready": idle, "after_requests": after,
        "sustained_8s_windows": sustained, "stages": by_window,
        "report": {"requests": report.requests, "verdicts": report.verdicts, "kills": report.kills,
                   "spawned": report.spawned}, "reaped": reaped,
    });
    fs::write(out, serde_json::to_vec_pretty(&result)?)?;
    println!("{}", serde_json::to_string_pretty(&result["stages"])?);
    Ok(())
}
