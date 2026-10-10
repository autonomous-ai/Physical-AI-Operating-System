//! Bounded incremental line framing for the physical relay.
//!
//! `BufRead::lines()` allocates a whole line before any limit can be checked,
//! and byte-index truncation can split a UTF-8 character. This reader keeps at
//! most `max` bytes of one line, never truncates, and reports oversized,
//! non-UTF-8 and unterminated lines as explicit frames. An oversized line is
//! consumed through its newline without being retained, so one bad line costs
//! no more memory than a valid one.
use sha2::{Digest, Sha256};
use std::io::{self, BufRead, BufReader, Read};

/// Read size for the underlying transport; also the minimum useful `max`.
const CHUNK_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// A complete UTF-8 line without its `\n` (and without a `\r` before it).
    Line(String),
    /// A line longer than the bound. `terminated` is false when the transport
    /// closed before its newline. `bytes` counts every byte of the line.
    Oversized { bytes: u64, terminated: bool },
    /// A complete line within the bound that is not valid UTF-8.
    InvalidUtf8 { bytes: usize },
    /// The transport closed inside a line within the bound.
    Truncated { bytes: usize },
}

impl Frame {
    /// The failure text for a non-line frame; `None` for a valid line.
    pub fn failure(&self, what: &str, max: usize) -> Option<String> {
        match self {
            Frame::Line(_) => None,
            Frame::Oversized {
                bytes,
                terminated: true,
            } => Some(format!(
                "{what} line of {bytes} bytes exceeds the {max}-byte limit"
            )),
            Frame::Oversized {
                bytes,
                terminated: false,
            } => Some(format!(
                "{what} closed inside a line already {bytes} bytes long, over the {max}-byte limit"
            )),
            Frame::InvalidUtf8 { bytes } => {
                Some(format!("{what} line of {bytes} bytes is not valid UTF-8"))
            }
            Frame::Truncated { bytes } => Some(format!(
                "{what} closed inside a line ({bytes} bytes without a newline)"
            )),
        }
    }
}

pub struct LineFramer<R> {
    reader: BufReader<R>,
    max: usize,
    line: Vec<u8>,
    /// Bytes of an oversized line consumed so far.
    discarding: Option<u64>,
}

impl<R: Read> LineFramer<R> {
    pub fn new(reader: R, max: usize) -> Self {
        Self {
            reader: BufReader::with_capacity(CHUNK_BYTES, reader),
            max,
            line: Vec::new(),
            discarding: None,
        }
    }

    /// The next frame, or `None` after a clean end of input.
    pub fn next_frame(&mut self) -> io::Result<Option<Frame>> {
        loop {
            let available = match self.reader.fill_buf() {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if available.is_empty() {
                if let Some(bytes) = self.discarding.take() {
                    return Ok(Some(Frame::Oversized {
                        bytes,
                        terminated: false,
                    }));
                }
                if self.line.is_empty() {
                    return Ok(None);
                }
                let bytes = self.line.len();
                self.line.clear();
                return Ok(Some(Frame::Truncated { bytes }));
            }
            let newline = available.iter().position(|&b| b == b'\n');
            let content = newline.unwrap_or(available.len());
            let consumed = newline.map_or(content, |i| i + 1);
            if let Some(count) = self.discarding.as_mut() {
                *count += content as u64;
                self.reader.consume(consumed);
                if newline.is_some() {
                    let bytes = self.discarding.take().unwrap_or_default();
                    return Ok(Some(Frame::Oversized {
                        bytes,
                        terminated: true,
                    }));
                }
                continue;
            }
            if self.line.len() + content > self.max {
                let bytes = (self.line.len() + content) as u64;
                self.line.clear();
                self.reader.consume(consumed);
                if newline.is_some() {
                    return Ok(Some(Frame::Oversized {
                        bytes,
                        terminated: true,
                    }));
                }
                self.discarding = Some(bytes);
                continue;
            }
            self.line.extend_from_slice(&available[..content]);
            self.reader.consume(consumed);
            if newline.is_none() {
                continue;
            }
            if self.line.last() == Some(&b'\r') {
                self.line.pop();
            }
            let bytes = std::mem::take(&mut self.line);
            return Ok(Some(match String::from_utf8(bytes) {
                Ok(line) => Frame::Line(line),
                Err(error) => Frame::InvalidUtf8 {
                    bytes: error.as_bytes().len(),
                },
            }));
        }
    }
}

/// A reader that hashes and counts exactly the bytes it returns.
pub struct HashingReader<R> {
    inner: R,
    hasher: Sha256,
    pub bytes: u64,
}

impl<R: Read> HashingReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            bytes: 0,
        }
    }
    /// Hash the rest of the input, then return the lowercase hex SHA-256.
    pub fn finish(mut self) -> io::Result<(String, u64)> {
        io::copy(&mut self, &mut io::sink())?;
        let digest = self.hasher.finalize();
        Ok((
            digest.iter().map(|b| format!("{b:02x}")).collect(),
            self.bytes,
        ))
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..count]);
        self.bytes += count as u64;
        Ok(count)
    }
}

impl<R: Read> LineFramer<HashingReader<R>> {
    /// Hash the unread remainder (including buffered bytes) and return the
    /// digest of everything the source produced.
    pub fn finish_hash(self) -> io::Result<(String, u64)> {
        // Discarded buffered bytes were already hashed when they were read.
        self.reader.into_inner().finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Delivers the input in fixed-size pieces to exercise chunk boundaries.
    struct Pieces<'a> {
        data: &'a [u8],
        step: usize,
    }
    impl Read for Pieces<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.step.min(buffer.len()).min(self.data.len());
            buffer[..count].copy_from_slice(&self.data[..count]);
            self.data = &self.data[count..];
            Ok(count)
        }
    }

    fn frames(data: &[u8], max: usize, step: usize) -> Vec<Frame> {
        let mut framer = LineFramer::new(Pieces { data, step }, max);
        let mut out = Vec::new();
        while let Some(frame) = framer.next_frame().unwrap() {
            out.push(frame);
        }
        out
    }

    #[test]
    fn lines_split_across_reads_are_reassembled_exactly() {
        for step in [1, 2, 7, 64, 9000] {
            assert_eq!(
                frames(b"alpha\r\n\nbeta\ngamma\n", 16, step),
                vec![
                    Frame::Line("alpha".into()),
                    Frame::Line(String::new()),
                    Frame::Line("beta".into()),
                    Frame::Line("gamma".into()),
                ],
                "step {step}"
            );
        }
    }

    #[test]
    fn a_line_at_the_bound_passes_and_one_byte_more_is_rejected_whole() {
        let mut data = vec![b'a'; 10];
        data.push(b'\n');
        data.extend(vec![b'b'; 11]);
        data.extend(b"\nok\n");
        for step in [1, 3, 10, 11, 12, 100] {
            assert_eq!(
                frames(&data, 10, step),
                vec![
                    Frame::Line("a".repeat(10)),
                    Frame::Oversized {
                        bytes: 11,
                        terminated: true
                    },
                    Frame::Line("ok".into()),
                ],
                "step {step}"
            );
        }
    }

    #[test]
    fn oversized_input_is_counted_but_never_retained() {
        let mut data = vec![b'x'; 5 * 1024 * 1024];
        data.push(b'\n');
        let mut framer = LineFramer::new(&data[..], 64);
        assert_eq!(
            framer.next_frame().unwrap(),
            Some(Frame::Oversized {
                bytes: 5 * 1024 * 1024,
                terminated: true
            })
        );
        assert!(framer.line.capacity() <= 64 + CHUNK_BYTES);
        assert_eq!(framer.next_frame().unwrap(), None);
    }

    #[test]
    fn multibyte_characters_are_never_split_or_lossily_replaced() {
        // "é" is two bytes; a 3-byte bound must reject "éé", not cut it.
        let data = "é\néé\n".as_bytes();
        for step in [1, 2, 3] {
            assert_eq!(
                frames(data, 3, step),
                vec![
                    Frame::Line("é".into()),
                    Frame::Oversized {
                        bytes: 4,
                        terminated: true
                    }
                ]
            );
        }
        assert_eq!(
            frames(b"ok\n\xff\xfe\nnext\n", 16, 1),
            vec![
                Frame::Line("ok".into()),
                Frame::InvalidUtf8 { bytes: 2 },
                Frame::Line("next".into())
            ]
        );
        // A character cut by the end of input is truncation, not a line.
        assert_eq!(
            frames(&"é".as_bytes()[..1], 16, 1),
            vec![Frame::Truncated { bytes: 1 }]
        );
    }

    #[test]
    fn end_of_input_inside_a_line_is_explicit() {
        assert_eq!(
            frames(b"done\npartial", 16, 4),
            vec![Frame::Line("done".into()), Frame::Truncated { bytes: 7 }]
        );
        assert_eq!(
            frames(&[b'z'; 40], 16, 4),
            vec![Frame::Oversized {
                bytes: 40,
                terminated: false
            }]
        );
        assert!(frames(b"", 16, 4).is_empty());
    }

    #[test]
    fn hashing_covers_every_byte_even_after_an_early_stop() {
        let data = b"one\ntwo\nthree\n";
        let mut framer = LineFramer::new(HashingReader::new(&data[..]), 16);
        assert_eq!(
            framer.next_frame().unwrap(),
            Some(Frame::Line("one".into()))
        );
        let (hash, bytes) = framer.finish_hash().unwrap();
        assert_eq!(bytes, data.len() as u64);
        assert_eq!(hash, lamp_acoustic::hash(data));
        let (whole, _) = HashingReader::new(&data[..]).finish().unwrap();
        assert_eq!(whole, hash);
    }

    #[test]
    fn failures_have_explicit_text() {
        let oversized = Frame::Oversized {
            bytes: 70_000,
            terminated: true,
        };
        assert!(
            oversized
                .failure("relay", 65_536)
                .unwrap()
                .contains("70000 bytes exceeds the 65536-byte limit")
        );
        assert!(Frame::Line("x".into()).failure("relay", 1).is_none());
        assert!(
            Frame::Truncated { bytes: 3 }
                .failure("trace", 9)
                .unwrap()
                .contains("closed inside a line")
        );
    }
}
