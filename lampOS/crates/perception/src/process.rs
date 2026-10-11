//! The perception worker process entry, shared by the `lamp-perception`
//! binary and any harness that hosts worker roles. It uses the runtime's
//! worker socket names (`perception.c.w`, `perception.d.w`, connecting to the
//! parent's `.p` ends) and envelopes, so ordinary supervision applies. `Ready`
//! is sent only after the model has been verified and an inference plan built.
use crate::{
    analyze::{Analyzer, INPUT_WIDTH},
    model::{self, YUNET},
    observation::PerceptionEvent,
    worker::{self, PerceptionControl, PerceptionLink, WorkerOptions},
};
use lamp_live::{
    transport::Channel,
    wire::{Control, WorkerEvent},
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

/// Run the perception worker role until stop, fault or heartbeat loss.
pub fn run_worker(
    dir: &Path,
    parent: lamp_interaction::BootId,
    worker: lamp_interaction::BootId,
    flags: &[&str],
) -> Result<()> {
    let mut model_dir = None;
    let mut context = None;
    let mut interval_ms: u64 = 250;
    let mut frame_size = (1280, 720);
    let mut iter = flags.iter();
    while let Some(flag) = iter.next() {
        let value = iter.next().ok_or_else(|| invalid(USAGE))?;
        match *flag {
            "--model-dir" => model_dir = Some(absolute(value)?),
            "--interval-ms" => interval_ms = value.parse()?,
            "--frame-consumer" => {
                // Only the provider's visual-context role is served here.
                let boot = value.strip_prefix("context=").ok_or_else(|| {
                    invalid("perception serves only --frame-consumer context=BOOT")
                })?;
                if context
                    .replace(lamp_live::process::parse_boot(boot)?)
                    .is_some()
                {
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
    options.context = context.map(|boot| (boot, dir.join(worker::CONTEXT_SOCKET)));
    worker::run(&mut link, analyzer, options)?;
    Ok(())
}
