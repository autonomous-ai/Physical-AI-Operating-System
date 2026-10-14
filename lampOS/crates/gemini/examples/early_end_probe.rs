//! Can the live service be made to forget a request ended too early
//! (second-P1, #723; docs/early-end-taken-back.md), and does the note the
//! voice sends after one get the rest of it answered as one? It runs five
//! trials of three requests each, from recorded speech, and prints one JSON
//! line a trial of timings and the service's own words.
//!
//! ```sh
//! cargo run --locked --release -p lamp-gemini --example early_end_probe -- \
//!     /etc/lampOS/voice/provider.json SPEECH_DIR [--dry-run]
//! ```
//!
//! The provider file is the voice's own, read as it is (fields a probe does
//! not use are ignored), passed as a path; it is never printed and nothing
//! is written anywhere. `SPEECH_DIR` holds the six 16-bit WAV files named in
//! [`trials::FILES`]. Nothing is played or recorded: no microphone or
//! speaker is opened. At most twenty requests, ended within two minutes.
//! `--dry-run` reads the files and the provider file, connects to nothing,
//! and prints what a run would send.
#[path = "shared/provider.rs"]
mod provider;
#[path = "early_end_probe/trials.rs"]
mod trials;

use lamp_gemini::{BarrierPolicy, Dialer, Resumption, Timeouts};
use provider::{Outcome, session, speech, trim};
use std::{path::Path, time::Duration};
use trials::{FILES, Probe, Speech, plan};

fn load(directory: &Path) -> Outcome<Speech> {
    let mut each = Vec::with_capacity(FILES.len());
    for name in FILES {
        let decoded = speech(&directory.join(name))?;
        let spoken = trim(&decoded);
        if !(4_800..=160_000).contains(&spoken.len()) {
            return Err(format!("{name} must hold 0.3 to 10 seconds of speech").into());
        }
        each.push(spoken.to_vec());
    }
    let [seed, piece, check, half, whole, rest]: [Vec<i16>; 6] =
        each.try_into().map_err(|_| "six speech files are needed")?;
    Ok(Speech {
        seed,
        piece,
        check,
        half,
        whole,
        rest,
    })
}

fn main() -> Outcome<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Outcome<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (provider, directory, dry) = match arguments.as_slice() {
        [provider, directory] => (provider, directory, false),
        [provider, directory, flag] if flag == "--dry-run" => (provider, directory, true),
        _ => return Err("usage: early_end_probe PROVIDER.json SPEECH_DIR [--dry-run]".into()),
    };
    let speech = load(Path::new(directory))?;
    // Read and checked in a dry run too, so that it fails as a run would.
    let config = session(Path::new(provider))?
        .resumption(Resumption::Request)
        .timeouts(Timeouts {
            barrier: Duration::from_secs(5),
            ..Timeouts::default()
        })?
        .barrier_policy(BarrierPolicy::AssumeAfterQuiet);
    if dry {
        println!("{}", plan(&speech));
        return Ok(());
    }
    let mut dialer = Dialer::new(config);
    let mut probe = Probe::new(&mut dialer, &speech);
    let started = std::time::Instant::now();
    probe.run(|row| println!("{row}")).await;
    println!(
        "{}",
        serde_json::json!({"requests": probe.requests,
            "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0})
    );
    Ok(())
}
