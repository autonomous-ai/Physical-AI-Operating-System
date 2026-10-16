//! Moves a skill brings, as files (#940, `docs/skills.md`): the same as the
//! body's own gestures ([`crate::clips`]), read at start from a folder
//! (`lamp-body --moves DIR`) instead of built in.
//!
//! One move a file, `NAME.json`:
//!
//! ```json
//! {"name": "wiggle", "step_ms": 750, "frames": [[0, 0, 0, 0, 0], [4, 0, 2, 6, 6], [0, 0, 0, 0, 0]]}
//! ```
//!
//! Each frame is offsets in encoder counts from the pose a standing lamp is
//! in, for head pitch, head roll, base yaw, shoulder and elbow, `step_ms`
//! apart. A move is held to what the built-in clips promise: it begins and
//! ends at nothing, lasts 1.5 to 10 s, takes no joint farther than
//! [`FARTHEST`] and asks none to go faster than [`FASTEST`]. One that does
//! not is refused with the reason, and the others are still taken. It is
//! asked for by name as a gesture is (`{"gesture":"wiggle"}` on the mood
//! socket), and played as one, only while the lamp stands.
use crate::clips::{self, Clip};
use serde::Deserialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// The farthest a move may take each joint from the pose, in counts: head
/// pitch, head roll, base yaw, shoulder, elbow. As the built-in clips.
pub const FARTHEST: [i16; 5] = [200, 200, 150, 350, 350];
/// The fastest a move may ask each joint to go, counts a second, with a
/// count of rounding either way at each end of a step. As the built-in
/// clips.
pub const FASTEST: [f32; 5] = [900.0, 1_300.0, 1_000.0, 800.0, 800.0];
/// At most this many moves are read from a folder; a folder with more is
/// refused whole.
pub const MOST_MOVES: usize = 64;
/// At most this many entries of the folder are looked at, moves or not; a
/// folder with more is refused whole.
pub const MOST_ENTRIES: usize = 1_024;
/// A move's file is at most this long.
const MOST_BYTES: u64 = 256 * 1024;
const SHORTEST_US: u64 = 1_500_000;
const LONGEST_US: u64 = 10_000_000;

/// What a folder of moves gave: the moves taken, in the order of their
/// file names, and each file refused with why.
#[derive(Debug, Default)]
pub struct Loaded {
    pub moves: Vec<&'static Clip>,
    pub refused: Vec<(PathBuf, String)>,
}

impl Loaded {
    /// The move of this name, if one was taken.
    pub fn named(&self, name: &str) -> Option<&'static Clip> {
        self.moves.iter().copied().find(|clip| clip.name == name)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: String,
    step_ms: u16,
    frames: Vec<[i16; 5]>,
}

/// Read every `*.json` in `folder`. A folder that cannot be read is an
/// error; a file that is not a move this body can play is refused and the
/// rest are taken. A folder of more than [`MOST_MOVES`] moves or
/// [`MOST_ENTRIES`] entries is refused whole, in one line and without
/// reading on, so the start's work stays bounded and which moves play never
/// depends on how many were left there. The moves live as long as the body:
/// they are read once, at start.
pub fn load(folder: &Path) -> io::Result<Loaded> {
    let mut files = Vec::new();
    for (looked, entry) in fs::read_dir(folder)?.enumerate() {
        let too_many = if looked >= MOST_ENTRIES {
            Some(format!("more than {MOST_ENTRIES} entries"))
        } else if files.len() > MOST_MOVES {
            Some(format!("more than {MOST_MOVES} moves"))
        } else {
            None
        };
        if let Some(why) = too_many {
            return Ok(Loaded {
                moves: Vec::new(),
                refused: vec![(
                    folder.to_owned(),
                    format!("{why} in the folder: none taken"),
                )],
            });
        }
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        // A Mac's `._` files and anything hidden are not moves.
        if name.starts_with('.') || path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        files.push(path);
    }
    if files.len() > MOST_MOVES {
        return Ok(Loaded {
            moves: Vec::new(),
            refused: vec![(
                folder.to_owned(),
                format!("more than {MOST_MOVES} moves in the folder: none taken"),
            )],
        });
    }
    files.sort();
    let mut loaded = Loaded::default();
    for path in files {
        match read(&path).and_then(|file| check(file, &loaded)) {
            Ok(clip) => loaded.moves.push(clip),
            Err(why) => loaded.refused.push((path, why)),
        }
    }
    Ok(loaded)
}

/// One open of the file, never blocking (a FIFO answers at once), its kind
/// and size checked on that same handle and at most [`MOST_BYTES`] read from
/// it: a file swapped after the check can neither be read whole nor hold
/// the start (the fifth on #994).
fn read(path: &Path) -> Result<File, String> {
    use rustix::fs::{Mode, OFlags};
    use std::io::Read;
    let opened = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| io::Error::from(error).to_string())?;
    let opened = std::fs::File::from(opened);
    let metadata = opened.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("not a file".into());
    }
    let mut text = Vec::new();
    opened
        .take(MOST_BYTES + 1)
        .read_to_end(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() as u64 > MOST_BYTES {
        return Err(format!("longer than {MOST_BYTES} bytes"));
    }
    serde_json::from_slice(&text).map_err(|error| format!("not a move: {error}"))
}

/// A move this body can play, or why not.
fn check(file: File, loaded: &Loaded) -> Result<&'static Clip, String> {
    let name = file.name;
    if name.is_empty()
        || name.len() > 32
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte))
    {
        return Err(format!(
            "{name:?} is not a name: 1 to 32 of a-z, 0-9, - and _"
        ));
    }
    if clips::named(&name).is_some() || name == clips::CALLED.name {
        return Err(format!("{name:?} is one of the body's own gestures"));
    }
    // The first file in name order keeps the name.
    if loaded.named(&name).is_some() {
        return Err(format!("{name:?} is already another file's move"));
    }
    if !(40..=1_000).contains(&file.step_ms) {
        return Err(format!("step_ms {} is not from 40 to 1000", file.step_ms));
    }
    let frames = file.frames;
    let duration_us = (frames.len().saturating_sub(1) as u64) * u64::from(file.step_ms) * 1_000;
    if !(SHORTEST_US..=LONGEST_US).contains(&duration_us) {
        return Err(format!(
            "it lasts {} ms, not 1500 to 10000",
            duration_us / 1_000
        ));
    }
    if frames.first() != Some(&[0; 5]) || frames.last() != Some(&[0; 5]) {
        return Err("it does not begin and end at nothing".into());
    }
    let step = f32::from(file.step_ms) / 1_000.0;
    for (at, pair) in frames.windows(2).enumerate() {
        for joint in 0..5 {
            if pair[1][joint].unsigned_abs() > FARTHEST[joint].unsigned_abs() {
                return Err(format!(
                    "frame {} takes joint {joint} {} counts, farther than {}",
                    at + 1,
                    pair[1][joint],
                    FARTHEST[joint]
                ));
            }
            let speed = (f32::from(pair[1][joint]) - f32::from(pair[0][joint])).abs() / step;
            if speed > FASTEST[joint] + 2.0 / step {
                return Err(format!(
                    "frame {} asks joint {joint} for {speed:.0} counts a second, faster than {}",
                    at + 1,
                    FASTEST[joint]
                ));
            }
        }
    }
    Ok(Box::leak(Box::new(Clip {
        name: Box::leak(name.into_boxed_str()),
        step_ms: file.step_ms,
        frames: Box::leak(frames.into_boxed_slice()),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Folder(PathBuf);
    impl Folder {
        fn new(name: &str) -> Self {
            let folder =
                std::env::temp_dir().join(format!("lamp-moves-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&folder);
            fs::create_dir_all(&folder).unwrap();
            Self(folder)
        }
        fn write(&self, file: &str, text: &str) {
            fs::write(self.0.join(file), text).unwrap();
        }
    }
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A 2 s move: up to 40 counts of head pitch and back, 50 ms a step.
    fn good(name: &str) -> String {
        let mut frames = vec!["[0,0,0,0,0]".to_owned()];
        for step in 1..40 {
            let pitch = if step <= 20 {
                step * 2
            } else {
                (40 - step) * 2
            };
            frames.push(format!("[{pitch},0,0,0,0]"));
        }
        frames.push("[0,0,0,0,0]".into());
        format!(
            "{{\"name\":\"{name}\",\"step_ms\":50,\"frames\":[{}]}}",
            frames.join(",")
        )
    }

    #[test]
    fn a_folder_of_moves_is_read_in_file_order_and_each_plays_as_a_gesture() {
        let folder = Folder::new("good");
        folder.write("b-wiggle.json", &good("wiggle"));
        folder.write("a-peek.json", &good("peek-left"));
        folder.write("._a-peek.json", "\0\0mac");
        folder.write("notes.txt", "not a move");
        let loaded = load(&folder.0).unwrap();
        assert!(loaded.refused.is_empty(), "{:?}", loaded.refused);
        let names: Vec<_> = loaded.moves.iter().map(|clip| clip.name).collect();
        assert_eq!(names, ["peek-left", "wiggle"]);
        let wiggle = loaded.named("wiggle").unwrap();
        assert_eq!(wiggle.duration_us(), 2_000_000);
        assert_eq!(wiggle.at(1_000_000), Some([40.0, 0.0, 0.0, 0.0, 0.0]));
        assert_eq!(wiggle.at(2_000_000), None);
        assert_eq!(loaded.named("nod"), None);
    }

    #[test]
    fn a_move_the_body_cannot_play_is_refused_with_why_and_the_rest_are_taken() {
        let folder = Folder::new("refused");
        folder.write("ok.json", &good("ok"));
        let cases = [
            ("bad-json.json", "{".to_owned(), "not a move"),
            (
                "extra.json",
                good("extra").replace("\"step_ms\"", "\"speed\":1,\"step_ms\""),
                "not a move",
            ),
            ("nod.json", good("nod"), "body's own gestures"),
            ("called.json", good("called"), "body's own gestures"),
            ("z-again.json", good("ok"), "already another file's move"),
            ("upper.json", good("Wiggle"), "is not a name"),
            (
                "slow-step.json",
                good("slow").replace("\"step_ms\":50", "\"step_ms\":20"),
                "not from 40 to 1000",
            ),
            (
                "short.json",
                "{\"name\":\"short\",\"step_ms\":50,\"frames\":[[0,0,0,0,0],[0,0,0,0,0]]}".into(),
                "not 1500 to 10000",
            ),
            (
                "long.json",
                good("long").replace("\"step_ms\":50", "\"step_ms\":300"),
                "not 1500 to 10000",
            ),
            (
                "ends.json",
                good("ends").replace("[2,0,0,0,0],[0,0,0,0,0]]", "[2,0,0,0,0],[2,0,0,0,0]]"),
                "begin and end at nothing",
            ),
            (
                "far.json",
                good("far").replace("[40,0,0,0,0]", "[40,0,151,0,0]"),
                "farther than 150",
            ),
            (
                "fast.json",
                good("fast").replace("[40,0,0,0,0]", "[40,0,0,0,90]"),
                "faster than 800",
            ),
        ];
        for (file, text, _) in &cases {
            folder.write(file, text);
        }
        let loaded = load(&folder.0).unwrap();
        let names: Vec<_> = loaded.moves.iter().map(|clip| clip.name).collect();
        assert_eq!(names, ["ok"]);
        assert_eq!(loaded.refused.len(), cases.len());
        for (file, _, why) in cases {
            let (_, said) = loaded
                .refused
                .iter()
                .find(|(path, _)| path.ends_with(file))
                .unwrap_or_else(|| panic!("{file} not refused"));
            assert!(said.contains(why), "{file}: {said}");
        }
    }

    #[test]
    fn a_folder_that_cannot_be_read_is_an_error_and_an_oversized_one_is_refused_whole() {
        assert!(load(Path::new("/nonexistent/lamp-moves")).is_err());
        let folder = Folder::new("many");
        for n in 0..MOST_MOVES {
            folder.write(&format!("{n:03}.json"), &good(&format!("m{n}")));
        }
        assert_eq!(load(&folder.0).unwrap().moves.len(), MOST_MOVES);
        // One more: none taken, in one line naming the folder.
        folder.write("064.json", &good("m64"));
        let loaded = load(&folder.0).unwrap();
        assert!(loaded.moves.is_empty());
        assert_eq!(loaded.refused.len(), 1);
        assert_eq!(loaded.refused[0].0, folder.0);
        assert!(
            loaded.refused[0].1.contains("more than 64 moves"),
            "{:?}",
            loaded.refused
        );
        // A folder of many other files is not read through either.
        let crowded = Folder::new("crowded");
        for n in 0..=MOST_ENTRIES {
            crowded.write(&format!("{n:04}.txt"), "");
        }
        let loaded = load(&crowded.0).unwrap();
        assert!(loaded.moves.is_empty());
        assert_eq!(loaded.refused.len(), 1);
        assert!(
            loaded.refused[0].1.contains("more than 1024 entries"),
            "{:?}",
            loaded.refused
        );
    }

    #[test]
    fn the_documented_example_is_a_move_this_body_takes() {
        let folder = Folder::new("example");
        let module = include_str!("moves.rs");
        let example = module
            .lines()
            .find_map(|line| line.strip_prefix("//! {\"name\""))
            .map(|rest| format!("{{\"name\"{rest}"))
            .unwrap();
        folder.write("wiggle.json", &example);
        let loaded = load(&folder.0).unwrap();
        assert!(loaded.refused.is_empty(), "{:?}", loaded.refused);
        assert_eq!(loaded.named("wiggle").unwrap().duration_us(), 1_500_000);
    }

    /// The fifth on #994: a FIFO among the moves is refused at once, never
    /// waited on, and a file longer than the cap is refused from what is
    /// read, not from a size looked at before.
    #[test]
    fn a_fifo_is_refused_at_once_and_a_long_file_by_what_is_read() {
        let folder = Folder::new("fifo");
        let fifo = folder.0.join("a-fifo.json");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        folder.write("b-long.json", &" ".repeat(MOST_BYTES as usize + 1));
        folder.write("c-ok.json", &good("ok"));
        let started = std::time::Instant::now();
        let loaded = load(&folder.0).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        let names: Vec<_> = loaded.moves.iter().map(|clip| clip.name).collect();
        assert_eq!(names, ["ok"]);
        let why = |file: &str| {
            loaded
                .refused
                .iter()
                .find(|(path, _)| path.ends_with(file))
                .map(|(_, why)| why.clone())
                .unwrap()
        };
        assert_eq!(why("a-fifo.json"), "not a file");
        assert!(why("b-long.json").starts_with("longer than"));
    }

    #[test]
    fn the_promise_is_the_built_in_clips_own() {
        // Every built-in clip would be taken as a move file.
        for clip in clips::ALL.into_iter().chain([&clips::CALLED]) {
            if clip.duration_us() < SHORTEST_US {
                continue;
            }
            let file = File {
                name: format!("{}-again", clip.name),
                step_ms: clip.step_ms,
                frames: clip.frames.to_vec(),
            };
            assert!(check(file, &Loaded::default()).is_ok(), "{}", clip.name);
        }
    }
}
