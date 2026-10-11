//! Perception worker and model provisioning.
//!
//! ```text
//! lamp-perception worker perception DIR CONTROLLER_BOOT WORKER_BOOT --model-dir ABS_DIR
//!     [--interval-ms 100..5000] [--frame-size WxH]
//! lamp-perception models verify ABS_DIR
//! lamp-perception models install SOURCE_FILE ABS_DIR
//! lamp-perception analyze --model-dir ABS_DIR FRAME.jpg
//! ```
//!
//! The worker role is `lamp_perception::process::run_worker`.
use lamp_live::process::parse_boot;
use lamp_perception::{
    analyze::{Analyzer, INPUT_WIDTH},
    model::{self, YUNET},
    process::run_worker,
};
use std::{
    io,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const USAGE: &str = "usage: lamp-perception worker perception DIR CONTROLLER_BOOT WORKER_BOOT --model-dir ABS_DIR [--interval-ms N] [--frame-size WxH] | models verify ABS_DIR | models install SOURCE ABS_DIR | analyze --model-dir ABS_DIR FRAME.jpg";

fn main() {
    if let Err(error) = run() {
        eprintln!("lamp-perception: {error}");
        std::process::exit(1);
    }
}

fn absolute(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "paths must be absolute").into());
    }
    Ok(path)
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["worker", "perception", dir, parent, worker, rest @ ..] => run_worker(
            Path::new(dir),
            parse_boot(parent)?,
            parse_boot(worker)?,
            rest,
        ),
        ["models", "verify", dir] => {
            let bytes = model::read_verified(&absolute(dir)?, &YUNET)?;
            println!(
                "{}",
                serde_json::json!({"model": YUNET.id, "bytes": bytes.len(), "sha256": YUNET.sha256, "verified": true})
            );
            Ok(())
        }
        ["models", "install", source, dir] => {
            let path = model::install(Path::new(source), &absolute(dir)?, &YUNET)?;
            println!(
                "{}",
                serde_json::json!({"model": YUNET.id, "installed": path, "sha256": YUNET.sha256})
            );
            Ok(())
        }
        ["analyze", "--model-dir", dir, frame] => {
            let bytes = model::read_verified(&absolute(dir)?, &YUNET)?;
            let mut analyzer = Analyzer::new(bytes, &YUNET, INPUT_WIDTH);
            let jpeg = std::fs::read(frame)?;
            let analysis = analyzer.analyze(&jpeg)?;
            println!(
                "{}",
                serde_json::json!({"exposure": analysis.exposure, "presence": analysis.presence,
                    "faces": analysis.faces, "attention": analysis.attention,
                    "decode_us": analysis.decode_us, "inference_us": analysis.inference_us,
                    "model": analysis.model, "scope": "one frame; no camera lineage or freshness"})
            );
            Ok(())
        }
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE).into()),
    }
}
