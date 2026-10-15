//! `lamp-same-voice pairs MODEL.onnx LIST.tsv`: each clip of LIST (lines of
//! `id<TAB>path`, 16 kHz mono 16-bit WAV) embedded, then the cosine of
//! every pair, one `a<TAB>b<TAB>cosine` line each, and on standard error
//! each clip's length and how long its embedding took. Local files only.
use lamp_same_voice::{RATE, Voice, cosine, read_wav};
use std::{path::Path, process::ExitCode, time::Instant};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, model, list] = args.as_slice() else {
        eprintln!("usage: lamp-same-voice pairs MODEL.onnx LIST.tsv");
        return ExitCode::FAILURE;
    };
    if command != "pairs" {
        eprintln!("usage: lamp-same-voice pairs MODEL.onnx LIST.tsv");
        return ExitCode::FAILURE;
    }
    match pairs(Path::new(model), Path::new(list)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lamp-same-voice: {error}");
            ExitCode::FAILURE
        }
    }
}

fn pairs(model: &Path, list: &Path) -> Result<(), String> {
    let loaded = Instant::now();
    let voice = Voice::load(model)?;
    eprintln!("loaded in {} ms", loaded.elapsed().as_millis());
    let text =
        std::fs::read_to_string(list).map_err(|error| format!("{}: {error}", list.display()))?;
    let mut clips = Vec::new();
    for line in text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (id, path) = line
            .split_once('\t')
            .ok_or_else(|| format!("not id<TAB>path: {line}"))?;
        let samples = read_wav(Path::new(path))?;
        let started = Instant::now();
        let embedding = voice.embedding(&samples)?;
        eprintln!(
            "{id}\t{:.2} s\t{} ms",
            samples.len() as f32 / RATE as f32,
            started.elapsed().as_millis()
        );
        clips.push((id.to_owned(), embedding));
    }
    for (i, (a, first)) in clips.iter().enumerate() {
        for (b, second) in &clips[i + 1..] {
            println!("{a}\t{b}\t{:.4}", cosine(first, second));
        }
    }
    Ok(())
}
