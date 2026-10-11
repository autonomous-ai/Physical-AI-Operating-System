//! Offline lab: recorded microphone and accepted render through the real
//! canceller, detector and echo observer, in the recorded interleave.
//!
//! Local files only. It opens no device and no connection, and it is not a
//! device result: the canceller here starts cold and runs on this host.
//!
//! ```text
//! duplex_lab LAB_DIR OUT_DIR [--near NEAR.pcm16le]
//! ```
//!
//! `LAB_DIR` holds `pre.pcm16le` (16 kHz microphone before cancellation),
//! `render.pcm16le` (24 kHz PCM the speaker accepted) and `schedule.tsv`, one
//! line per 10 ms capture block: the render cursor the canceller had analysed
//! before that block, and the delay hint it was given. `--near` adds a second
//! 16 kHz signal to the microphone before cancellation, saturating. Mixed-in
//! speech is a synthetic overlap, whatever it was recorded from.
use lamp_audio::{CAPTURE_SAMPLES, EchoProcessor, RENDER_SAMPLES};
use lamp_live::activity::SpeechProbability;
use std::{
    fs,
    io::{self, BufWriter, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

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

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let usage = || io::Error::other("usage: duplex_lab LAB_DIR OUT_DIR [--near NEAR.pcm16le]");
    let lab = args.next().ok_or_else(usage)?;
    let out = args.next().ok_or_else(usage)?;
    let mut near = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--near" if near.is_none() => {
                near = Some(pcm(Path::new(&args.next().ok_or_else(usage)?))?)
            }
            _ => return Err(usage().into()),
        }
    }
    let lab = Path::new(&lab);
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    let pre = pcm(&lab.join("pre.pcm16le"))?;
    let render = pcm(&lab.join("render.pcm16le"))?;
    let schedule: Vec<(usize, u16)> = fs::read_to_string(lab.join("schedule.tsv"))?
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let (through, hint) = line.split_once('\t').ok_or_else(usage)?;
            Ok((through.parse()?, hint.parse()?))
        })
        .collect::<Result<_>>()?;
    if pre.len() < schedule.len() * CAPTURE_SAMPLES
        || near
            .as_ref()
            .is_some_and(|near| near.len() < schedule.len() * CAPTURE_SAMPLES)
    {
        return Err(io::Error::other("microphone shorter than the schedule").into());
    }
    let mut echo = EchoProcessor::new(false);
    let mut speech = SpeechProbability::default();
    let mut post = BufWriter::new(fs::File::create(out.join("post.pcm16le"))?);
    let mut features = BufWriter::new(fs::File::create(out.join("features.tsv"))?);
    writeln!(
        features,
        "block\tvad\tcapture_tenths\tresidual_tenths\trender_active\tmatched\trender_tenths\tcapture_explained\tresidual_explained\tresidual_floor\talignment_age\taec_alignment_ms"
    )?;
    let mut analysed = 0;
    for (index, &(through, hint)) in schedule.iter().enumerate() {
        if through % RENDER_SAMPLES != 0 || through < analysed || through > render.len() {
            return Err(io::Error::other(format!("invalid render cursor at block {index}")).into());
        }
        while analysed < through {
            let block: [i16; RENDER_SAMPLES] =
                render[analysed..analysed + RENDER_SAMPLES].try_into()?;
            echo.render(&block)?;
            analysed += RENDER_SAMPLES;
        }
        let at = index * CAPTURE_SAMPLES;
        let mut block: [i16; CAPTURE_SAMPLES] = pre[at..at + CAPTURE_SAMPLES].try_into()?;
        if let Some(near) = &near {
            for (sample, add) in block.iter_mut().zip(&near[at..at + CAPTURE_SAMPLES]) {
                *sample = sample.saturating_add(*add);
            }
        }
        let clean = echo.capture(&block, hint)?;
        let probability = speech.analyze(&clean);
        let evidence = echo
            .evidence()
            .ok_or_else(|| io::Error::other("no evidence"))?;
        for sample in clean {
            post.write_all(&sample.to_le_bytes())?;
        }
        let matched = evidence.echo;
        writeln!(
            features,
            "{index}\t{probability:.4}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
        )?;
    }
    post.flush()?;
    features.flush()?;
    println!(
        "{} capture blocks, {} render frames",
        schedule.len(),
        analysed
    );
    Ok(())
}
