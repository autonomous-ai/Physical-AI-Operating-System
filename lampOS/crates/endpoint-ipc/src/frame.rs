//! Length-prefixed little-endian frames on the worker's stdin and stdout.
//!
//! A frame is a `u32` body length followed by the body, whose first byte is
//! its kind. Each kind has an exact size (or an explicit bound), checked before
//! any allocation, so a corrupt peer cannot request unbounded memory.
use crate::{Token, WINDOW_SAMPLES};
use std::io::{self, Read};

/// Bumped on any incompatible frame change; the worker reports it in `Ready`.
pub const PROTOCOL: u16 = 1;
pub const MAX_REASON_BYTES: usize = 512;

const CLASSIFY: u8 = 1;
const READY: u8 = 2;
const VERDICT: u8 = 3;
const ERROR: u8 = 4;

const CLASSIFY_HEADER: usize = 1 + 8 + 8 + 8 + 1 + 8 + 4;
pub const MAX_REQUEST_BYTES: usize = CLASSIFY_HEADER + 2 * WINDOW_SAMPLES;
const READY_BYTES: usize = 1 + 2 + 8 + 8 + 32;
const VERDICT_BYTES: usize = 1 + 8 + 4 + 8 * 4;
const ERROR_HEADER: usize = 1 + 8 + 2;
const MAX_RESPONSE_BYTES: usize = ERROR_HEADER + MAX_REASON_BYTES;

/// A classification request without its PCM, which is read into a caller buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub request: u64,
    pub token: Token,
    pub submitted_at_us: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    /// The pinned model loaded and completed one cold inference.
    Ready {
        protocol: u16,
        load_us: u64,
        cold_us: u64,
        model_sha256: [u8; 32],
    },
    Verdict {
        request: u64,
        probability: f32,
        received_at_us: u64,
        features_us: u64,
        inference_us: u64,
        sent_at_us: u64,
    },
    /// Request 0 is a startup failure. The worker exits after any error.
    Error { request: u64, reason: String },
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Encodes a request, copying at most the last [`WINDOW_SAMPLES`] samples.
pub fn encode_request(request: &Request, pcm: &[i16]) -> Vec<u8> {
    let pcm = &pcm[pcm.len().saturating_sub(WINDOW_SAMPLES)..];
    let body = CLASSIFY_HEADER + 2 * pcm.len();
    let mut out = Vec::with_capacity(4 + body);
    out.extend_from_slice(&(body as u32).to_le_bytes());
    out.push(CLASSIFY);
    out.extend_from_slice(&request.request.to_le_bytes());
    out.extend_from_slice(&request.token.utterance.to_le_bytes());
    out.extend_from_slice(&request.token.generation.to_le_bytes());
    out.push(request.token.check);
    out.extend_from_slice(&request.submitted_at_us.to_le_bytes());
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

impl Response {
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(VERDICT_BYTES);
        match self {
            Self::Ready {
                protocol,
                load_us,
                cold_us,
                model_sha256,
            } => {
                body.push(READY);
                body.extend_from_slice(&protocol.to_le_bytes());
                body.extend_from_slice(&load_us.to_le_bytes());
                body.extend_from_slice(&cold_us.to_le_bytes());
                body.extend_from_slice(model_sha256);
            }
            Self::Verdict {
                request,
                probability,
                received_at_us,
                features_us,
                inference_us,
                sent_at_us,
            } => {
                body.push(VERDICT);
                body.extend_from_slice(&request.to_le_bytes());
                body.extend_from_slice(&probability.to_le_bytes());
                for value in [received_at_us, features_us, inference_us, sent_at_us] {
                    body.extend_from_slice(&value.to_le_bytes());
                }
            }
            Self::Error { request, reason } => {
                let mut end = reason.len().min(MAX_REASON_BYTES);
                while !reason.is_char_boundary(end) {
                    end -= 1;
                }
                body.push(ERROR);
                body.extend_from_slice(&request.to_le_bytes());
                body.extend_from_slice(&(end as u16).to_le_bytes());
                body.extend_from_slice(&reason.as_bytes()[..end]);
            }
        }
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }
}

/// Reads a body of at most `max` bytes. `None` is a clean end of stream at a
/// frame boundary; a stream ending inside a frame is an error.
fn read_body(input: &mut impl Read, max: usize, body: &mut Vec<u8>) -> io::Result<Option<()>> {
    let mut length = [0u8; 4];
    let mut filled = 0;
    while filled < length.len() {
        match input.read(&mut length[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > max {
        return Err(invalid("frame length out of bounds"));
    }
    body.clear();
    body.resize(length, 0);
    input.read_exact(body)?;
    Ok(Some(()))
}

fn u16_at(body: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([body[at], body[at + 1]])
}

fn u32_at(body: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(body[at..at + 4].try_into().expect("four bytes"))
}

fn u64_at(body: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(body[at..at + 8].try_into().expect("eight bytes"))
}

/// Worker side: reads one request and replaces `pcm` with its samples.
pub fn read_request(
    input: &mut impl Read,
    body: &mut Vec<u8>,
    pcm: &mut Vec<i16>,
) -> io::Result<Option<Request>> {
    if read_body(input, MAX_REQUEST_BYTES, body)?.is_none() {
        return Ok(None);
    }
    if body.len() < CLASSIFY_HEADER || body[0] != CLASSIFY {
        return Err(invalid("unknown request"));
    }
    let count = u32_at(body, 34) as usize;
    if count > WINDOW_SAMPLES || body.len() != CLASSIFY_HEADER + 2 * count {
        return Err(invalid("request size does not match its samples"));
    }
    pcm.clear();
    pcm.extend(
        body[CLASSIFY_HEADER..]
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]])),
    );
    Ok(Some(Request {
        request: u64_at(body, 1),
        token: Token {
            utterance: u64_at(body, 9),
            generation: u64_at(body, 17),
            check: body[25],
        },
        submitted_at_us: u64_at(body, 26),
    }))
}

/// Core side: reads one bounded, validated response.
pub fn read_response(input: &mut impl Read) -> io::Result<Option<Response>> {
    let mut body = Vec::with_capacity(MAX_RESPONSE_BYTES);
    if read_body(input, MAX_RESPONSE_BYTES, &mut body)?.is_none() {
        return Ok(None);
    }
    let response = match (body[0], body.len()) {
        (READY, READY_BYTES) => Response::Ready {
            protocol: u16_at(&body, 1),
            load_us: u64_at(&body, 3),
            cold_us: u64_at(&body, 11),
            model_sha256: body[19..51].try_into().expect("32 bytes"),
        },
        (VERDICT, VERDICT_BYTES) => {
            let probability = f32::from_le_bytes(body[9..13].try_into().expect("four bytes"));
            if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
                return Err(invalid("probability out of range"));
            }
            Response::Verdict {
                request: u64_at(&body, 1),
                probability,
                received_at_us: u64_at(&body, 13),
                features_us: u64_at(&body, 21),
                inference_us: u64_at(&body, 29),
                sent_at_us: u64_at(&body, 37),
            }
        }
        (ERROR, length) if length >= ERROR_HEADER => {
            let size = usize::from(u16_at(&body, 9));
            if length != ERROR_HEADER + size || size > MAX_REASON_BYTES {
                return Err(invalid("error size does not match its text"));
            }
            Response::Error {
                request: u64_at(&body, 1),
                reason: String::from_utf8_lossy(&body[ERROR_HEADER..]).into_owned(),
            }
        }
        _ => return Err(invalid("unknown response")),
    };
    Ok(Some(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: Token = Token {
        utterance: 7,
        generation: 3,
        check: 1,
    };

    #[test]
    fn request_round_trips_and_keeps_only_the_latest_window() {
        let pcm: Vec<i16> = (0..WINDOW_SAMPLES + 5).map(|i| i as i16).collect();
        let request = Request {
            request: 42,
            token: TOKEN,
            submitted_at_us: 99,
        };
        let bytes = encode_request(&request, &pcm);
        assert_eq!(bytes.len(), 4 + MAX_REQUEST_BYTES);
        let (mut body, mut out) = (Vec::new(), Vec::new());
        let decoded = read_request(&mut bytes.as_slice(), &mut body, &mut out)
            .unwrap()
            .unwrap();
        assert_eq!(decoded, request);
        assert_eq!(out, pcm[5..]);
        assert!(
            read_request(&mut [].as_slice(), &mut body, &mut out)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn responses_round_trip() {
        for response in [
            Response::Ready {
                protocol: PROTOCOL,
                load_us: 1,
                cold_us: 2,
                model_sha256: [9; 32],
            },
            Response::Verdict {
                request: 3,
                probability: 0.25,
                received_at_us: 4,
                features_us: 5,
                inference_us: 6,
                sent_at_us: 7,
            },
            Response::Error {
                request: 0,
                reason: "missing model".into(),
            },
        ] {
            let bytes = response.encode();
            assert_eq!(
                read_response(&mut bytes.as_slice()).unwrap(),
                Some(response)
            );
        }
    }

    #[test]
    fn long_reasons_are_truncated_on_a_character_boundary() {
        let reason = "é".repeat(MAX_REASON_BYTES);
        let bytes = Response::Error { request: 1, reason }.encode();
        let Some(Response::Error { reason, .. }) = read_response(&mut bytes.as_slice()).unwrap()
        else {
            panic!("not an error");
        };
        assert_eq!(reason.len(), MAX_REASON_BYTES);
    }

    #[test]
    fn malformed_frames_are_rejected_before_allocation() {
        let cases: [&[u8]; 6] = [
            &[0, 0, 0, 0],
            &[0xff, 0xff, 0xff, 0x7f, VERDICT],
            &[1, 0, 0, 0, 0x55],
            &[2, 0, 0, 0, VERDICT, 0],
            &[1, 0],
            &[45, 0, 0, 0, VERDICT],
        ];
        for bytes in cases {
            assert!(read_response(&mut &bytes[..]).is_err(), "{bytes:?}");
        }
        let mut verdict = Response::Verdict {
            request: 1,
            probability: 0.5,
            received_at_us: 0,
            features_us: 0,
            inference_us: 0,
            sent_at_us: 0,
        }
        .encode();
        verdict[13..17].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(read_response(&mut verdict.as_slice()).is_err());
        let mut request = encode_request(
            &Request {
                request: 1,
                token: TOKEN,
                submitted_at_us: 0,
            },
            &[1, 2, 3],
        );
        request[38] = 4;
        let (mut body, mut pcm) = (Vec::new(), Vec::new());
        assert!(read_request(&mut request.as_slice(), &mut body, &mut pcm).is_err());
    }
}
