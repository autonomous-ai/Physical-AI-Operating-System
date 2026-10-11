//! Offline lab: recorded microphone and accepted render through the real
//! canceller, detector, echo observer and duplex decision.
//!
//! Local files only. It opens no device and no connection, and it is not a
//! device result: the canceller here starts cold and runs on this host.
//!
//! ```text
//! duplex_lab LAB_DIR OUT_DIR [--near NEAR.pcm16le]
//! duplex_lab LAB_DIR OUT_DIR --duplex --room BLOCK [--segment SPEECH.pcm16le AT_BLOCK]
//! ```
//!
//! `LAB_DIR` holds `pre.pcm16le` (16 kHz microphone before cancellation),
//! `render.pcm16le` (24 kHz PCM the speaker accepted) and `schedule.tsv`, one
//! line per 10 ms capture block: the render cursor the canceller had analysed
//! before that block, and the delay hint it was given.
//!
//! The first form replays the recording as it happened and writes the
//! features of every block. `--near` adds a second 16 kHz signal to the
//! microphone before cancellation, saturating.
//!
//! The second form closes the loop. The decision runs on every block, and
//! when it yields the lab does what a paused lamp would do: the render the
//! canceller is given goes silent once the audio already queued has played,
//! and the microphone goes back to room noise once the echo of that audio
//! has arrived. `--room` names a stretch of the same recording with nobody
//! speaking, to take that noise from. `--segment` adds recorded speech to the
//! microphone from a given block: a synthetic overlap, whatever the speech
//! was recorded from. How the echo dies away is a model, not a measurement.
use lamp_audio::{
    CAPTURE_SAMPLES, EchoProcessor, RENDER_SAMPLES,
    duplex::{Confirmation, Heard, Hearing, Interruption, Margin, Step},
};
use lamp_live::activity::SpeechProbability;
use std::{
    fs,
    io::{self, BufWriter, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Blocks of already accepted audio that still play after a yield: the chunk
/// in flight and the device queue.
const QUEUED_BLOCKS: usize = 3;
/// Blocks from a render block to its echo in the microphone, shortest path.
const ECHO_BLOCKS: usize = 12;
/// Render blocks of silence that end a reply in the recording.
const REPLY_GAP_BLOCKS: usize = 60;

fn pcm(path: &Path) -> Result<Vec<i16>> {
    let bytes = fs::read(path)?;
    if bytes.len() % 2 != 0 {
        return Err(io::Error::other("odd PCM length").into());
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect())
}

fn usage() -> io::Error {
    io::Error::other(
        "usage: duplex_lab LAB_DIR OUT_DIR [--near NEAR] | --duplex --room BLOCK [--segment SPEECH AT_BLOCK]",
    )
}

struct Lab {
    pre: Vec<i16>,
    render: Vec<i16>,
    schedule: Vec<(usize, u16)>,
}

impl Lab {
    fn load(directory: &Path) -> Result<Self> {
        let pre = pcm(&directory.join("pre.pcm16le"))?;
        let render = pcm(&directory.join("render.pcm16le"))?;
        let schedule: Vec<(usize, u16)> = fs::read_to_string(directory.join("schedule.tsv"))?
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| {
                let (through, hint) = line.split_once('\t').ok_or_else(usage)?;
                Ok((through.parse()?, hint.parse()?))
            })
            .collect::<Result<_>>()?;
        if pre.len() < schedule.len() * CAPTURE_SAMPLES {
            return Err(io::Error::other("microphone shorter than the schedule").into());
        }
        let mut before = 0;
        for &(through, _) in &schedule {
            if through % RENDER_SAMPLES != 0 || through < before || through > render.len() {
                return Err(io::Error::other("invalid render cursor in the schedule").into());
            }
            before = through;
        }
        Ok(Self {
            pre,
            render,
            schedule,
        })
    }

    fn microphone(&self, block: usize) -> [i16; CAPTURE_SAMPLES] {
        let at = block * CAPTURE_SAMPLES;
        self.pre[at..at + CAPTURE_SAMPLES].try_into().unwrap()
    }

    /// Whether the render the canceller is given before `block` has any sound.
    fn sounding(&self, block: usize) -> bool {
        let through = self.schedule[block].0;
        let from = if block == 0 {
            0
        } else {
            self.schedule[block - 1].0
        };
        self.render[from..through]
            .iter()
            .any(|&s| s.unsigned_abs() > 33)
    }

    /// For each block, whether it lies inside a reply: from its first sound
    /// to the last before a long silence.
    fn replies(&self) -> Vec<bool> {
        let sounding: Vec<bool> = (0..self.schedule.len()).map(|b| self.sounding(b)).collect();
        let mut inside = vec![false; sounding.len()];
        let mut last = None;
        for block in 0..sounding.len() {
            if sounding[block] {
                let start = match last {
                    Some(last) if block - last <= REPLY_GAP_BLOCKS => last,
                    _ => block,
                };
                inside[start..=block].fill(true);
                last = Some(block);
            }
        }
        inside
    }
}

fn add(block: &mut [i16; CAPTURE_SAMPLES], other: &[i16]) {
    for (sample, add) in block.iter_mut().zip(other) {
        *sample = sample.saturating_add(*add);
    }
}

struct Features {
    post: BufWriter<fs::File>,
    table: BufWriter<fs::File>,
}

impl Features {
    fn create(out: &Path) -> Result<Self> {
        let mut table = BufWriter::new(fs::File::create(out.join("features.tsv"))?);
        writeln!(
            table,
            "block\tvad\tcapture_tenths\tresidual_tenths\trender_active\tmatched\trender_tenths\tcapture_explained\tresidual_explained\tresidual_floor\talignment_age\taec_alignment_ms\trender_quiet\theard"
        )?;
        Ok(Self {
            post: BufWriter::new(fs::File::create(out.join("post.pcm16le"))?),
            table,
        })
    }

    fn block(
        &mut self,
        index: usize,
        probability: f32,
        heard: Heard,
        echo: &EchoProcessor,
        clean: &[i16; CAPTURE_SAMPLES],
    ) -> Result<()> {
        let evidence = echo
            .evidence()
            .ok_or_else(|| io::Error::other("no evidence"))?;
        for sample in clean {
            self.post.write_all(&sample.to_le_bytes())?;
        }
        let matched = evidence.echo;
        writeln!(
            self.table,
            "{index}\t{probability:.4}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            evidence.capture_level.tenths_of_db(),
            evidence.residual_level.tenths_of_db(),
            evidence.render_active_blocks,
            u8::from(matched.is_some()),
            matched.map_or(-960, |m| m.render_level.tenths_of_db()),
            matched.map_or(0, |m| m.capture_explained),
            matched.map_or(0, |m| m.residual_explained),
            matched.map_or(0, |m| m.residual_playback_floor),
            matched.map_or(0, |m| m.alignment_age_blocks),
            echo.internal_alignment_ms().unwrap_or(-1),
            evidence.render_quiet_blocks,
            match heard {
                Heard::Still => 0,
                Heard::Clear => 1,
                Heard::Under => 2,
                Heard::Own => 3,
                Heard::Over => 4,
                Heard::Unknown => 5,
            },
        )?;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.post.flush()?;
        self.table.flush()?;
        Ok(())
    }
}

/// The recording as it happened.
fn replay(lab: &Lab, out: &Path, near: Option<Vec<i16>>) -> Result<()> {
    if near
        .as_ref()
        .is_some_and(|near| near.len() < lab.schedule.len() * CAPTURE_SAMPLES)
    {
        return Err(io::Error::other("near-end shorter than the schedule").into());
    }
    let mut echo = EchoProcessor::new(false);
    let mut speech = SpeechProbability::default();
    let mut features = Features::create(out)?;
    let mut analysed = 0;
    for (index, &(through, hint)) in lab.schedule.iter().enumerate() {
        while analysed < through {
            let block: [i16; RENDER_SAMPLES] =
                lab.render[analysed..analysed + RENDER_SAMPLES].try_into()?;
            echo.render(&block)?;
            analysed += RENDER_SAMPLES;
        }
        let mut block = lab.microphone(index);
        if let Some(near) = &near {
            let at = index * CAPTURE_SAMPLES;
            add(&mut block, &near[at..at + CAPTURE_SAMPLES]);
        }
        let clean = echo.capture(&block, hint)?;
        let probability = speech.analyze(&clean);
        let evidence = echo
            .evidence()
            .ok_or_else(|| io::Error::other("no evidence"))?;
        let heard = Hearing::EVALUATED.heard(probability, &evidence);
        features.block(index, probability, heard, &echo, &clean)?;
    }
    features.finish()?;
    println!(
        "{} capture blocks, {} render frames",
        lab.schedule.len(),
        analysed
    );
    Ok(())
}

/// Where the lamp is in its own reply.
enum Output {
    /// Handing the recorded render on as it was recorded.
    Playing,
    /// Yielded: `queued` more recorded blocks play, then silence. `echo`
    /// counts the blocks for which the recorded microphone is still true.
    Paused { queued: usize, echo: usize },
    /// Resumed: the recorded microphone holds echo that has not been played
    /// again yet for this many more blocks.
    Returning { blank: usize },
}

/// The loop closed: a yield stops the lamp.
fn duplex(lab: &Lab, out: &Path, room: usize, segment: Option<(Vec<i16>, usize)>) -> Result<()> {
    if (room + 100) * CAPTURE_SAMPLES > lab.pre.len() {
        return Err(io::Error::other("room noise stretch out of range").into());
    }
    let inside = lab.replies();
    let hearing = Hearing::EVALUATED;
    let mut interruption = Interruption::new(Confirmation::EVALUATED);
    let mut margin = Margin::default();
    let mut echo = EchoProcessor::new(false);
    let mut speech = SpeechProbability::default();
    let mut features = Features::create(out)?;
    let mut decisions = BufWriter::new(fs::File::create(out.join("decisions.tsv"))?);
    writeln!(decisions, "wall_block\tsource_block\tstep")?;
    let mut analysed = 0;
    let (mut source, mut wall) = (0_usize, 0_usize);
    let mut output = Output::Playing;
    let blocks = lab.schedule.len();
    // Wall time may run past the recording by what the yields added.
    while source < blocks && wall < blocks + 2_000 {
        let room_block = lab.microphone(room + wall % 100);
        // What the canceller is given and what the microphone holds.
        let hint = lab.schedule[source].1;
        let (mut microphone, advance);
        output = match output {
            Output::Playing => {
                (microphone, advance) = (lab.microphone(source), true);
                Output::Playing
            }
            Output::Paused { queued, echo } if queued > 0 => {
                (microphone, advance) = (lab.microphone(source), true);
                Output::Paused {
                    queued: queued - 1,
                    echo,
                }
            }
            Output::Paused { queued, echo } if echo > 0 => {
                // The render has stopped; its echo is still arriving.
                let block = (source + ECHO_BLOCKS - echo).min(blocks - 1);
                (microphone, advance) = (lab.microphone(block), false);
                Output::Paused {
                    queued,
                    echo: echo - 1,
                }
            }
            Output::Paused { queued, echo } => {
                (microphone, advance) = (room_block, false);
                Output::Paused { queued, echo }
            }
            Output::Returning { blank } => {
                (microphone, advance) = (room_block, true);
                if blank > 1 {
                    Output::Returning { blank: blank - 1 }
                } else {
                    Output::Playing
                }
            }
        };
        if advance {
            while analysed < lab.schedule[source].0 {
                let block: [i16; RENDER_SAMPLES] =
                    lab.render[analysed..analysed + RENDER_SAMPLES].try_into()?;
                echo.render(&block)?;
                analysed += RENDER_SAMPLES;
            }
        } else {
            echo.render(&[0; RENDER_SAMPLES])?;
        }
        if let Some((speech, at)) = &segment
            && wall >= *at
            && (wall - at + 1) * CAPTURE_SAMPLES <= speech.len()
        {
            let from = (wall - at) * CAPTURE_SAMPLES;
            add(&mut microphone, &speech[from..from + CAPTURE_SAMPLES]);
        }
        let clean = echo.capture(&microphone, hint)?;
        let probability = speech.analyze(&clean);
        let evidence = echo
            .evidence()
            .ok_or_else(|| io::Error::other("no evidence"))?;
        let heard = hearing.heard(probability, &evidence);
        margin.observe(heard, &evidence);
        features.block(wall, probability, heard, &echo, &clean)?;
        let speaking = inside[source] || !matches!(output, Output::Playing);
        let step = interruption.block(heard, speaking);
        match step {
            Step::Carry => {}
            Step::Yield => {
                writeln!(decisions, "{wall}\t{source}\tyield")?;
                output = Output::Paused {
                    queued: QUEUED_BLOCKS,
                    echo: ECHO_BLOCKS,
                };
            }
            Step::Resume => {
                writeln!(decisions, "{wall}\t{source}\tresume")?;
                output = Output::Returning { blank: ECHO_BLOCKS };
            }
            Step::Commit => {
                writeln!(decisions, "{wall}\t{source}\tcommit")?;
                // The reply is cancelled: skip to where the recording has
                // the lamp silent again.
                while source < blocks - 1 && inside[source] {
                    source += 1;
                }
                analysed = lab.schedule[source].0;
                output = Output::Playing;
            }
        }
        if advance && step != Step::Commit {
            source += 1;
        }
        wall += 1;
    }
    features.finish()?;
    decisions.flush()?;
    let (yields, commits, resumes) = interruption.counts();
    println!(
        "{wall} wall blocks for {blocks} recorded; yields {yields}, commits {commits}, resumes {resumes}; margin {:?} dB (person {:?}, own {:?})",
        margin.db(),
        margin.person_db(),
        margin.own_db()
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let lab = args.next().ok_or_else(usage)?;
    let out = args.next().ok_or_else(usage)?;
    let (mut near, mut closed, mut room, mut segment) = (None, false, None, None);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--near" if near.is_none() => {
                near = Some(pcm(Path::new(&args.next().ok_or_else(usage)?))?)
            }
            "--duplex" if !closed => closed = true,
            "--room" if room.is_none() => room = Some(args.next().ok_or_else(usage)?.parse()?),
            "--segment" if segment.is_none() => {
                let speech = pcm(Path::new(&args.next().ok_or_else(usage)?))?;
                segment = Some((speech, args.next().ok_or_else(usage)?.parse()?));
            }
            _ => return Err(usage().into()),
        }
    }
    let lab = Lab::load(Path::new(&lab))?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    match (closed, room) {
        (false, None) if segment.is_none() => replay(&lab, out, near),
        (true, Some(room)) if near.is_none() => duplex(&lab, out, room, segment),
        _ => Err(usage().into()),
    }
}
