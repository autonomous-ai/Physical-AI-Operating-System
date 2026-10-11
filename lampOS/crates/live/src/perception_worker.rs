//! The perception worker role (`worker perception ...`), hosted by this
//! runtime. The perception library (`lamp-perception`) does not depend on the
//! runtime; this adapter connects its loop to the ordinary worker channels,
//! so supervision, handshake and envelopes are the same as for every other
//! worker. `Ready` is sent only after the model has been verified and an
//! inference plan built; a model failure is reported as a typed fault first.
use crate::{
    camera_worker::FRAME_SOCKET,
    process::parse_boot,
    transport::WorkerChannels,
    wire::{Control, WorkerEvent},
};
use lamp_interaction::BootId;
use lamp_perception::{
    analyze::{Analyzer, INPUT_WIDTH},
    model::{self, YUNET},
    observation::PerceptionEvent,
    worker::{self, CONTEXT_SOCKET, PerceptionControl, PerceptionLink, WorkerOptions},
};
use std::{
    io,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const USAGE: &str = "worker perception DIR CONTROLLER_BOOT WORKER_BOOT --model-dir ABS_DIR [--interval-ms 100..5000] [--frame-size WxH] [--frame-consumer context=PROVIDER_BOOT]";

fn invalid(message: &str) -> Box<dyn std::error::Error + Send + Sync> {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_owned()).into()
}

fn absolute(path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(invalid("paths must be absolute"));
    }
    Ok(path)
}

impl PerceptionLink for WorkerChannels {
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

/// Run the perception worker role until stop, fault or heartbeat loss.
/// `channels` are the worker side of the ordinary connected worker channels.
pub fn run(
    mut channels: WorkerChannels,
    parent: BootId,
    worker: BootId,
    dir: &Path,
    flags: &[String],
) -> Result<()> {
    let mut model_dir = None;
    let mut context = None;
    let mut interval_ms: u64 = 250;
    let mut frame_size = (1280, 720);
    let mut iter = flags.iter();
    while let Some(flag) = iter.next() {
        let value = iter.next().ok_or_else(|| invalid(USAGE))?;
        match flag.as_str() {
            "--model-dir" => model_dir = Some(absolute(value)?),
            "--interval-ms" => interval_ms = value.parse()?,
            "--frame-consumer" => {
                // Only the provider's visual-context role is served here.
                let boot = value.strip_prefix("context=").ok_or_else(|| {
                    invalid("perception serves only --frame-consumer context=BOOT")
                })?;
                if context.replace(parse_boot(boot)?).is_some() {
                    return Err(invalid("duplicate context consumer"));
                }
            }
            "--frame-size" => {
                let (w, h) = value.split_once('x').ok_or_else(|| invalid(USAGE))?;
                frame_size = (w.parse()?, h.parse()?);
            }
            _ => return Err(invalid(USAGE)),
        }
    }
    if !(100..=5_000).contains(&interval_ms) {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "interval must be 100..5000 ms").into(),
        );
    }
    let model_dir = model_dir.ok_or_else(|| invalid(USAGE))?;
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
            let _ = channels.control.send(WorkerEvent::Fault {
                code: format!("perception_model_unavailable: {error}")
                    .chars()
                    .take(200)
                    .collect(),
            });
            return Err(error);
        }
    };
    channels.control.send(WorkerEvent::Ready)?;
    let mut options = WorkerOptions::new(dir.join(FRAME_SOCKET), parent, worker);
    options.interval_us = interval_ms * 1_000;
    options.context = context.map(|boot| (boot, dir.join(CONTEXT_SOCKET)));
    worker::run(&mut channels, analyzer, options)?;
    Ok(())
}
