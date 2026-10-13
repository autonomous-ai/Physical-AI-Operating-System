//! Look at one JPEG file with the pinned model and print what the body
//! would be told, and how long ten looks took. For checking the eyes by hand
//! without a camera: `cargo run --release -p lamp-eyes --example look MODEL JPEG`.
use lamp_eyes::{
    faces::Detector,
    picture::Picture,
    sight::{Seen, choose},
};
use std::{path::Path, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, model, jpeg] = args.as_slice() else {
        return Err("usage: look MODEL JPEG".into());
    };
    let started = Instant::now();
    let detector = Detector::load(Path::new(model))?;
    let loaded = started.elapsed();
    let bytes = std::fs::read(jpeg)?;
    let mut times = Vec::new();
    let mut seen = Seen { face: None };
    let mut count = 0;
    for _ in 0..10 {
        let started = Instant::now();
        let picture = Picture::from_jpeg(&bytes)?;
        let faces = detector.find(&picture)?;
        count = faces.len();
        seen = Seen {
            face: choose(&faces, picture.filled),
        };
        times.push(started.elapsed().as_secs_f32() * 1e3);
    }
    times.sort_by(f32::total_cmp);
    println!(
        "{} face(s); {}; load {:.0} ms; a look {:.1} ms at best, {:.1} at worst of ten",
        count,
        serde_json::to_string(&seen)?,
        loaded.as_secs_f32() * 1e3,
        times[0],
        times[9]
    );
    Ok(())
}
