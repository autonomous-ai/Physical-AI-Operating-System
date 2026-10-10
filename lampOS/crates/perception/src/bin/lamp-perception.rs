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
//! The worker uses the runtime's worker socket names (`perception.c.w`,
//! `perception.d.w`, connecting to the parent's `.p` ends) and envelopes, so
//! the supervisor's ordinary spawn/handshake applies. `Ready` is sent only
//! after the model has been verified and an inference plan built.
use lamp_live::{
    process::parse_boot,
    transport::Channel,
    wire::{Control, WorkerEvent},
};
use lamp_perception::{
    analyze::{Analyzer, INPUT_WIDTH},
    model::{self, YUNET},
    observation::PerceptionEvent,
    worker::{self, PerceptionControl, PerceptionLink, WorkerOptions},
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

/// The runtime's two private datagram channels under the worker role name.
struct Link {
    control: Channel,
    data: Channel,
}

impl PerceptionLink for Link {
    fn receive_control(&mut self) -> io::Result<Option<PerceptionControl>> {
        Ok(match self.control.receive::<Control>()? {
            None => None,
            Some(Control::Authority { snapshot }) => Some(PerceptionControl::Authority(snapshot)),
            Some(Control::Stop) => Some(PerceptionControl::Stop),
            Some(_) => {
                return Err(io::Error::other(
                    "perception worker rejects capture, reference and provider control",
                ));
            }
        })
    }
    fn publish(&mut self, event: PerceptionEvent) -> io::Result<()> {
        self.data.send(event)
    }
}

fn run_worker(
    dir: &Path,
    parent: lamp_interaction::BootId,
    worker: lamp_interaction::BootId,
    flags: &[&str],
) -> Result<()> {
    let mut model_dir = None;
    let mut interval_ms: u64 = 250;
    let mut frame_size = (1280, 720);
    let mut iter = flags.iter();
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))?;
        match *flag {
            "--model-dir" => model_dir = Some(absolute(value)?),
            "--interval-ms" => interval_ms = value.parse()?,
            "--frame-size" => {
                let (w, h) = value
                    .split_once('x')
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))?;
                frame_size = (w.parse()?, h.parse()?);
            }
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE).into()),
        }
    }
    if !(100..=5_000).contains(&interval_ms) {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "interval must be 100..5000 ms").into(),
        );
    }
    let model_dir = model_dir.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, USAGE))?;
    let mut control = Channel::bind(&dir.join("perception.c.w"), worker, parent, 100_000)?;
    let mut data = Channel::bind(&dir.join("perception.d.w"), worker, parent, 100_000)?;
    control.connect(&dir.join("perception.c.p"))?;
    data.connect(&dir.join("perception.d.p"))?;
    let mut link = Link { control, data };
    // Inference must never compete with audio for the CPU.
    let _ = rustix::process::nice(10);
    let prepared = model::read_verified(&model_dir, &YUNET)
        .map_err(Box::<dyn std::error::Error + Send + Sync>::from)
        .and_then(|bytes| {
            let mut analyzer = Analyzer::new(bytes, &YUNET, INPUT_WIDTH);
            analyzer.prepare(frame_size.0, frame_size.1)?;
            Ok(analyzer)
        });
    let analyzer = match prepared {
        Ok(analyzer) => analyzer,
        Err(error) => {
            // Report a typed fault before exiting; never claim readiness.
            let _ = link.control.send(WorkerEvent::Fault {
                code: format!("perception_model_unavailable: {error}")
                    .chars()
                    .take(200)
                    .collect(),
            });
            return Err(error);
        }
    };
    link.control.send(WorkerEvent::Ready)?;
    let mut options = WorkerOptions::new(
        dir.join(lamp_live::camera_worker::FRAME_SOCKET),
        parent,
        worker,
    );
    options.interval_us = interval_ms * 1_000;
    worker::run(&mut link, analyzer, options)?;
    Ok(())
}
