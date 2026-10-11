//! Worker-side serving loop, shared by the real worker and its test doubles.
//!
//! One request is read, classified and answered at a time. The request
//! buffers are zeroed after each answer; buffers inside `classify` are the
//! caller's to clear, and freed allocations are not scrubbed. Any failure is
//! reported once and ends the worker; the core supervisor decides whether a
//! fresh process may replace it.
use crate::frame::{self, PROTOCOL, Response};
use lamp_ipc::monotonic_us;
use std::{
    io::{self, Read, Write},
    panic::{AssertUnwindSafe, catch_unwind},
};

pub struct Classified {
    pub probability: f32,
    pub features_us: u64,
    pub inference_us: u64,
}

#[derive(Debug)]
pub enum Stop {
    /// The core closed the request pipe: an ordinary shutdown.
    Closed,
    Malformed(io::Error),
    Failed(String),
    Output(io::Error),
}

impl Stop {
    /// Process exit code for the worker binary.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Closed => 0,
            Self::Malformed(_) => 3,
            Self::Failed(_) => 4,
            Self::Output(_) => 5,
        }
    }
}

fn send(output: &mut impl Write, response: &Response) -> io::Result<()> {
    output.write_all(&response.encode())?;
    output.flush()
}

pub fn ready(
    output: &mut impl Write,
    load_us: u64,
    cold_us: u64,
    model_sha256: [u8; 32],
) -> io::Result<()> {
    send(
        output,
        &Response::Ready {
            protocol: PROTOCOL,
            load_us,
            cold_us,
            model_sha256,
        },
    )
}

/// Reports a startup failure; the caller exits nonzero afterwards.
pub fn startup_failed(output: &mut impl Write, reason: &str) -> io::Result<()> {
    send(
        output,
        &Response::Error {
            request: 0,
            reason: reason.into(),
        },
    )
}

pub fn serve(
    input: &mut impl Read,
    output: &mut impl Write,
    mut classify: impl FnMut(&[i16]) -> Result<Classified, String>,
) -> Stop {
    let (mut body, mut pcm) = (Vec::new(), Vec::new());
    loop {
        let request = match frame::read_request(input, &mut body, &mut pcm) {
            Ok(Some(request)) => request,
            Ok(None) => return Stop::Closed,
            Err(error) => {
                let reason = format!("malformed request: {error}");
                let _ = send(output, &Response::Error { request: 0, reason });
                return Stop::Malformed(error);
            }
        };
        let received_at_us = monotonic_us();
        let result = catch_unwind(AssertUnwindSafe(|| classify(&pcm)))
            .unwrap_or_else(|_| Err("classifier panicked".into()));
        pcm.fill(0);
        pcm.clear();
        body.fill(0);
        body.clear();
        let response = match result {
            Ok(classified) => Response::Verdict {
                request: request.request,
                probability: classified.probability,
                received_at_us,
                features_us: classified.features_us,
                inference_us: classified.inference_us,
                sent_at_us: monotonic_us(),
            },
            Err(reason) => Response::Error {
                request: request.request,
                reason,
            },
        };
        if let Err(error) = send(output, &response) {
            return Stop::Output(error);
        }
        if let Response::Error { reason, .. } = response {
            return Stop::Failed(reason);
        }
    }
}
