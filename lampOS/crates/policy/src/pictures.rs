//! The lamp's picture with the attention question (#871, `lamp-policy
//! --images`), in shadow: for each utterance the policy asks about, the
//! images model is asked twice at once, on two connections, the same
//! questions without the picture and with it and the picture's own five
//! ([`attention::questions_with_picture`]), and one line is written with
//! both answers, both times and what the eyes saw. Nothing acts on it, and
//! the policy's own decision does not wait for it.
//!
//! The picture is the eyes' newest frame, and it leaves the lamp by the
//! voice's rule (`docs/eyes.md`): the eyes keep the file only while the voice
//! permits the camera, which is exactly while the microphone may listen, so
//! a muted lamp has none; and a frame older than [`FRESH`] or larger than
//! [`MOST_BYTES`] is not taken, as the voice's `sight` takes none. The
//! picture is never written anywhere: the line says whether one was sent,
//! its size and its age, or why none was.
use crate::{
    attention::{self, Context},
    decide::{Asked, Failed, Jev, Picture},
    policy::Seen,
    record::Record,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

/// Where the eyes keep their newest frame (`lamp-eyes --sight-file`).
pub const SIGHT_FILE: &str = "/run/lampos-eyes/sight.jpg";
/// The images model asked unless another is named: the first that takes a
/// box, and the one the issue asks about.
pub const DEFAULT_MODEL: &str = "typesafe/jev-1.14";
/// A frame older than this is not taken, as the voice's `sight::FRESH`.
pub const FRESH: Duration = Duration::from_secs(1);
/// Nor a larger one, as the voice's and the eyes' own bound.
pub const MOST_BYTES: u64 = 64 * 1024;
/// The key the state holds where the picture goes: a string nothing else
/// in a question holds.
pub const KEY: &str = "lamp-sight-picture-871";
/// What TypeSafe takes: each side from 256 to 1,568 pixels, and at least
/// 78,400 in all.
const LEAST_SIDE: usize = 256;
const MOST_SIDE: usize = 1_568;
const LEAST_PIXELS: usize = 78_400;

/// The newest frame, if it may be sent now, and how old it is; or why not.
pub fn fresh_picture(path: &Path, now: SystemTime) -> Result<(Vec<u8>, Duration), &'static str> {
    let age = keeping(path, now)?;
    let mut jpeg = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MOST_BYTES + 1).read_to_end(&mut jpeg))
        .map_err(|_| "unreadable")?;
    if jpeg.len() as u64 > MOST_BYTES || !jpeg.starts_with(&[0xff, 0xd8]) {
        return Err("not_a_jpeg");
    }
    Ok((jpeg, age))
}

/// Whether the eyes keep a fresh frame now, and how old it is: they keep
/// one only while the camera is permitted (the microphone is), and remove it
/// at once when it is not. Asked again at the moment a picture is to be
/// sent ([`permitted_now`]).
fn keeping(path: &Path, now: SystemTime) -> Result<Duration, &'static str> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        // The eyes keep none: the camera is not permitted (the microphone
        // is not), the eyes keep no frames, or they are not running.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err("no_frame"),
        Err(_) => return Err("unreadable"),
    };
    let age = metadata
        .modified()
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .unwrap_or(Duration::ZERO);
    if age > FRESH {
        return Err("stale");
    }
    if metadata.len() > MOST_BYTES {
        return Err("too_large");
    }
    Ok(age)
}

/// Whether a picture taken from `path` may still leave the lamp now: the
/// eyes still keep a fresh frame there. Asked at the last moment before the
/// request with the picture is written, after any slow looking up or
/// connecting (the reviewer on #873): removing the frame, as the eyes do
/// when the camera is closed, stops a picture not yet sent.
pub fn permitted_now(path: &Path) -> bool {
    keeping(path, SystemTime::now()).is_ok()
}

/// `jpeg` as TypeSafe takes it, with its size: as it is if it fits, or
/// made larger by a whole factor, each pixel repeated, and encoded again.
/// The eyes' 320 by 240 frame is under the least side and becomes 640 by
/// 480.
pub fn for_jev(jpeg: &[u8]) -> Result<(Vec<u8>, (usize, usize)), &'static str> {
    use zune_jpeg::{
        JpegDecoder,
        zune_core::{colorspace::ColorSpace, options::DecoderOptions},
    };
    let options = DecoderOptions::default()
        .set_max_width(MOST_SIDE)
        .set_max_height(MOST_SIDE)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(jpeg, options);
    let rgb = decoder.decode().map_err(|_| "not_a_jpeg")?;
    let (width, height) = decoder.dimensions().ok_or("not_a_jpeg")?;
    if width == 0 || height == 0 || rgb.len() != width * height * 3 {
        return Err("not_a_jpeg");
    }
    let factor = (1..=8)
        .find(|factor| {
            width.min(height) * factor >= LEAST_SIDE
                && width * height * factor * factor >= LEAST_PIXELS
        })
        .ok_or("too_small")?;
    if width.max(height) * factor > MOST_SIDE {
        return Err("too_large");
    }
    if factor == 1 {
        return Ok((jpeg.to_vec(), (width, height)));
    }
    let (wide, high) = (width * factor, height * factor);
    let mut larger = Vec::with_capacity(wide * high * 3);
    for y in 0..high {
        let row = &rgb[(y / factor) * width * 3..(y / factor + 1) * width * 3];
        for x in 0..wide {
            larger.extend_from_slice(&row[(x / factor) * 3..(x / factor) * 3 + 3]);
        }
    }
    let mut encoded = Vec::new();
    jpeg_encoder::Encoder::new(&mut encoded, 85)
        .encode(
            &larger,
            u16::try_from(wide).map_err(|_| "too_large")?,
            u16::try_from(high).map_err(|_| "too_large")?,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|_| "not_encoded")?;
    Ok((encoded, (wide, high)))
}

/// One utterance the policy is asking about.
#[derive(Clone, Debug)]
pub struct Job {
    pub context: Context,
    pub seen: Option<Seen>,
    /// When the policy sent its own question, in milliseconds since 1970.
    pub sent_at_unix_ms: u64,
}

/// What the images model said, or why it said nothing.
fn told(asked: Result<Asked, Failed>, context: &Context, extra: bool) -> Value {
    let asked = match asked {
        Ok(asked) => asked,
        Err(failed) => return json!({"failed": failed.kind()}),
    };
    let answers: serde_json::Map<String, Value> = asked
        .answers
        .iter()
        .filter_map(|(name, answer)| match answer {
            crate::decide::Answer::Noul(yes) => Some((name.clone(), json!(round(f64::from(*yes))))),
            crate::decide::Answer::Choice { .. } => None,
        })
        .collect();
    let judgement =
        attention::Answers::of(&asked.answers).map(|answers| attention::combine(&answers, context));
    let mut told = json!({
        "by": asked.model,
        "took_ms": u64::try_from(asked.took.as_millis()).unwrap_or(u64::MAX),
        "answers": answers,
    });
    match judgement {
        Ok(judgement) => {
            told["choice"] = json!(judgement.choice.as_str());
            told["probability"] = json!(round(f64::from(judgement.probability)));
        }
        Err(_) => told["failed"] = json!("missing"),
    }
    if extra {
        for name in attention::picture_questions() {
            if !asked.answers.contains_key(name) {
                told["failed"] = json!("missing");
            }
        }
    }
    told
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// The line for one utterance: what was known, what the eyes counted, the
/// picture's size and age or why none was sent, and the two answers.
pub fn line(job: &Job, picture: Value, without: Value, with: Value) -> Value {
    json!({
        "at_unix_ms": job.sent_at_unix_ms,
        "about": "attention_picture",
        "shadow": true,
        "context": job.context,
        "in_conversation": job.context.in_conversation(),
        "seen": job.seen.as_ref().map(|seen| json!({"faces": seen.faces, "near": seen.near,
                                           "facing": seen.facing, "age_ms": seen.age_ms})),
        "picture": picture,
        "without": without,
        "with": with,
    })
}

/// The thread that asks, and the way to give it work.
pub struct Pictures {
    jobs: mpsc::SyncSender<Job>,
    thread: thread::JoinHandle<()>,
}

/// How to ask.
#[derive(Clone, Debug)]
pub struct Options {
    pub sight_file: PathBuf,
    pub deadline: Duration,
    pub log_dir: PathBuf,
}

impl Pictures {
    /// Start asking `without` and `with`, two connections to the images
    /// model, writing to `pictures-YYYY-MM-DD.jsonl` in the log folder.
    pub fn start(mut without: Jev, mut with: Jev, options: Options) -> io::Result<Self> {
        let (jobs, taken) = mpsc::sync_channel::<Job>(0);
        let thread = thread::Builder::new()
            .name("pictures".into())
            .spawn(move || {
                let mut record = Record::of("pictures", &options.log_dir);
                for job in taken {
                    let line = ask_both(&mut without, &mut with, &job, &options);
                    // A line that cannot be written is lost: the policy goes on.
                    let _ = record.write(job.sent_at_unix_ms, &line);
                }
            })?;
        Ok(Self { jobs, thread })
    }

    /// Hand over `job` if the thread is free; an utterance that comes while
    /// it is still asking about the last is passed over, and false is said.
    pub fn offer(&self, job: Job) -> bool {
        self.jobs.try_send(job).is_ok()
    }

    /// Whether the thread is still there.
    pub fn alive(&self) -> bool {
        !self.thread.is_finished()
    }
}

/// Both askings at once, for one utterance, if there is a picture to send.
pub fn ask_both(without: &mut Jev, with: &mut Jev, job: &Job, options: &Options) -> Value {
    let frame = fresh_picture(&options.sight_file, SystemTime::now())
        .and_then(|(jpeg, age)| for_jev(&jpeg).map(|(sent, size)| (sent, size, age, jpeg.len())));
    let (jpeg, size, age, kept_bytes) = match frame {
        Ok(frame) => frame,
        Err(why) => {
            return line(
                job,
                json!({"sent": false, "why_not": why}),
                Value::Null,
                Value::Null,
            );
        }
    };
    let state = attention::state_text(&job.context);
    let pictured = attention::state_text_with_picture(&job.context, KEY);
    let picture = Picture {
        key: KEY.into(),
        jpeg,
    };
    let sent_bytes = picture.jpeg.len();
    let (plain, seen) = thread::scope(|scope| {
        let plain = scope.spawn(|| {
            crate::decide::Decider::ask(without, &state, &attention::questions(), options.deadline)
        });
        let seen = with.ask_with_pictures(
            &pictured,
            &attention::questions_with_picture(),
            std::slice::from_ref(&picture),
            options.deadline,
            &|| permitted_now(&options.sight_file),
        );
        (plain.join().unwrap_or(Err(Failed::TimedOut)), seen)
    });
    let picture = if matches!(seen, Err(Failed::Withdrawn)) {
        // Taken while permitted, and no longer by the time it was to go.
        json!({"sent": false, "why_not": "withdrawn"})
    } else {
        json!({"sent": true, "bytes": sent_bytes, "eyes_bytes": kept_bytes,
               "width": size.0, "height": size.1, "age_ms": u64::try_from(age.as_millis()).unwrap_or(u64::MAX)})
    };
    line(
        job,
        picture,
        told(plain, &job.context, false),
        told(seen, &job.context, true),
    )
}

#[cfg(test)]
mod tests;
