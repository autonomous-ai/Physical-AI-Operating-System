//! The newest frame, kept in one private file for the voice service.
//!
//! A person asks the lamp what it sees. The voice service gives the model one
//! picture with what was said (`lamp-voice`, `--eyes-sight`), and this is
//! where it finds it: the frame the eyes last looked at, replaced whole about
//! six times a second while the camera is open and removed the moment it
//! closes. One file, never a history, readable by its owner only. The file's
//! modification time is how old the picture is.
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};

/// A frame larger than this is not kept: the voice service would refuse it.
pub const MOST_BYTES: usize = 64 * 1024;

pub struct Kept {
    path: PathBuf,
    writing: PathBuf,
    present: bool,
}

/// A whole JPEG with its own Huffman tables: what a reader other than this
/// process can decode. Camera frames sometimes come without the tables.
pub fn whole_jpeg(frame: &[u8]) -> bool {
    if frame.len() < 4 || !frame.starts_with(&[0xff, 0xd8]) || !frame.ends_with(&[0xff, 0xd9]) {
        return false;
    }
    let (mut at, mut tables) = (2usize, false);
    // Segments up to the start of the scan: marker, length, contents.
    while let Some(&[0xff, marker, high, low]) = frame.get(at..at + 4) {
        tables |= marker == 0xc4;
        if marker == 0xda {
            return tables;
        }
        at += 2 + usize::from(u16::from_be_bytes([high, low]));
    }
    false
}

impl Kept {
    pub fn new(path: PathBuf) -> Self {
        let mut writing = path.clone().into_os_string();
        writing.push(".new");
        Self {
            path,
            writing: writing.into(),
            present: false,
        }
    }

    /// Put this frame in the file's place. A reader sees the frame before or
    /// this one, never part of either.
    pub fn keep(&mut self, frame: &[u8]) -> io::Result<()> {
        if frame.len() > MOST_BYTES || !whole_jpeg(frame) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a whole JPEG of at most 64 KiB",
            ));
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&self.writing)?;
        file.write_all(frame)?;
        drop(file);
        fs::rename(&self.writing, &self.path)?;
        self.present = true;
        Ok(())
    }

    /// The camera closed: there is nothing to see.
    pub fn forget(&mut self) {
        if self.present {
            let _ = fs::remove_file(&self.path);
            self.present = false;
        }
    }
}

/// The camera closed, for whatever reason: the working frame and a full
/// look's still are both gone, at once (#842).
pub fn forget_all(working: &mut Option<Kept>, still: &mut Option<Kept>) {
    for kept in [working, still].into_iter().flatten() {
        kept.forget();
    }
}

/// One line saying how large the near face is, kept fresh while there is
/// one: how the voice service knows someone has come to sit in front of the
/// lamp (`lamp-voice`, `LAMP_EYES_PRESENCE_FILE`). No picture, no place, no
/// identity: a number between 0 and 1, the face's height as a share of the
/// picture's. A file not written for a second and a half means nobody.
pub struct Presence {
    path: PathBuf,
    writing: PathBuf,
    present: bool,
}

impl Presence {
    pub fn new(path: PathBuf) -> Self {
        let mut writing = path.clone().into_os_string();
        writing.push(".new");
        Self {
            path,
            writing: writing.into(),
            present: false,
        }
    }

    /// There is a face this large in the look just made.
    pub fn there(&mut self, size: f32) -> io::Result<()> {
        fs::write(&self.writing, format!("{:.3}\n", size.clamp(0.0, 1.0)))?;
        fs::rename(&self.writing, &self.path)?;
        self.present = true;
        Ok(())
    }

    /// The camera closed: nobody can be seen.
    pub fn gone(&mut self) {
        if self.present {
            let _ = fs::remove_file(&self.path);
            self.present = false;
        }
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(&self.writing);
    }
}

impl Drop for Kept {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(&self.writing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Reviewer's case on #842: a still finished just before the eyes close
    /// the camera to try the full size again, and the reopen fails. Every
    /// close goes through `forget_all`, so the still is gone with the
    /// working frame at once, not left for a 3 s expiry that a failed
    /// reopen's early return would skip.
    #[test]
    fn a_camera_close_removes_the_working_frame_and_the_still_at_once() {
        let directory = std::env::temp_dir().join(format!(
            "lamp-eyes-forget-all-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let (working_path, still_path) = (directory.join("sight.jpg"), directory.join("full.jpg"));
        let mut working = Some(Kept::new(working_path.clone()));
        let mut still = Some(Kept::new(still_path.clone()));
        working.as_mut().unwrap().keep(&frame(true, 10)).unwrap();
        still.as_mut().unwrap().keep(&frame(true, 10)).unwrap();
        assert!(working_path.exists() && still_path.exists());
        forget_all(&mut working, &mut still);
        assert!(!working_path.exists() && !still_path.exists());
        // Without the files, nothing to do.
        forget_all(&mut None, &mut None);
        fs::remove_dir_all(&directory).unwrap();
    }

    /// A frame shaped like the lamp's camera gives: two quantisation tables,
    /// the frame header, Huffman tables, a restart interval, the scan.
    fn frame(tables: bool, filler: usize) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8];
        let mut segment = |marker: u8, length: u16| {
            bytes.extend([0xff, marker]);
            bytes.extend(length.to_be_bytes());
            bytes.extend(vec![0u8; usize::from(length) - 2]);
        };
        segment(0xdb, 67);
        segment(0xdb, 67);
        segment(0xc0, 17);
        if tables {
            segment(0xc4, 31);
            segment(0xc4, 181);
        }
        segment(0xdd, 4);
        segment(0xda, 12);
        bytes.extend(vec![0x55; filler]);
        bytes.extend([0xff, 0xd9]);
        bytes
    }

    #[test]
    fn only_a_whole_jpeg_with_its_tables_is_one() {
        assert!(whole_jpeg(&frame(true, 100)));
        assert!(!whole_jpeg(&frame(false, 100)));
        let whole = frame(true, 100);
        assert!(!whole_jpeg(&whole[..whole.len() - 1]));
        assert!(!whole_jpeg(&whole[1..]));
        assert!(!whole_jpeg(&whole[..40]));
        assert!(!whole_jpeg(&[0xff, 0xd8, 0xff, 0xd9]));
        assert!(!whole_jpeg(b""));
        // A length that runs off the end.
        let mut cut = frame(true, 0);
        cut[4] = 0xff;
        cut[5] = 0xff;
        assert!(!whole_jpeg(&cut));
    }

    #[test]
    fn the_file_is_the_newest_frame_private_and_gone_when_forgotten() {
        let directory = std::env::temp_dir().join(format!(
            "lamp-eyes-kept-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sight.jpg");
        let mut kept = Kept::new(path.clone());
        kept.forget();
        assert!(!path.exists());
        let (first, second) = (frame(true, 10), frame(true, 2_000));
        kept.keep(&first).unwrap();
        assert_eq!(fs::read(&path).unwrap(), first);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        kept.keep(&second).unwrap();
        assert_eq!(fs::read(&path).unwrap(), second);
        // Refused frames leave the last good one where it was.
        assert!(kept.keep(&frame(false, 10)).is_err());
        assert!(kept.keep(&frame(true, MOST_BYTES)).is_err());
        assert_eq!(fs::read(&path).unwrap(), second);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        kept.forget();
        assert!(!path.exists());
        kept.keep(&first).unwrap();
        drop(kept);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        // The presence line beside it: a size, replaced whole, gone with
        // the camera.
        let line = directory.join("present");
        let mut presence = Presence::new(line.clone());
        presence.gone();
        assert!(!line.exists());
        presence.there(0.2134).unwrap();
        assert_eq!(fs::read_to_string(&line).unwrap(), "0.213\n");
        presence.there(7.0).unwrap();
        assert_eq!(fs::read_to_string(&line).unwrap(), "1.000\n");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        presence.gone();
        assert!(!line.exists());
        presence.there(0.5).unwrap();
        drop(presence);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(&directory).unwrap();
    }
}
