//! Pinned model assets. A model is used only if its exact bytes match the
//! built-in specification; there is no download, fallback or "close enough".
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub license: &'static str,
}

/// OpenCV Zoo YuNet 2023mar face detector with five landmarks (MIT). See
/// `assets/models/README.md` for provenance; identical to V1's copy.
pub const YUNET: ModelSpec = ModelSpec {
    id: "yunet-2023mar",
    file: "face_detection_yunet_2023mar.onnx",
    bytes: 232_589,
    sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
    license: "MIT",
};

#[derive(Debug)]
pub enum ModelError {
    Missing(PathBuf),
    NotRegularFile,
    Symlink,
    HardLinked(u64),
    Size {
        expected: u64,
        actual: u64,
    },
    Digest {
        actual: String,
    },
    /// Group- or world-writable file or directory.
    Writable(u32),
    /// Owned by neither this user nor root.
    Owner(u32),
    Changed,
    Io(io::Error),
    Load(String),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(path) => write!(f, "model file {} is missing", path.display()),
            Self::NotRegularFile => f.write_str("model path is not a regular file"),
            Self::Symlink => f.write_str("model path is a symbolic link"),
            Self::HardLinked(links) => write!(f, "model file has {links} hard links"),
            Self::Size { expected, actual } => {
                write!(f, "model is {actual} bytes; expected {expected}")
            }
            Self::Digest { actual } => write!(f, "model SHA-256 {actual} does not match"),
            Self::Writable(mode) => write!(f, "model path is writable by others (mode {mode:o})"),
            Self::Owner(uid) => write!(f, "model path is owned by uid {uid}"),
            Self::Changed => f.write_str("model file changed while it was read"),
            Self::Io(error) => write!(f, "model I/O: {error}"),
            Self::Load(error) => write!(f, "model load: {error}"),
        }
    }
}
impl std::error::Error for ModelError {}
impl From<io::Error> for ModelError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn check_owner_and_mode(metadata: &fs::Metadata) -> Result<(), ModelError> {
    let euid = rustix::process::geteuid().as_raw();
    if metadata.uid() != euid && metadata.uid() != 0 {
        return Err(ModelError::Owner(metadata.uid()));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(ModelError::Writable(metadata.mode() & 0o7777));
    }
    Ok(())
}

/// Read `dir/spec.file` and verify its type, ownership, permissions, size and
/// digest. The opened descriptor is compared with the checked path, so a
/// swapped file is reported rather than read.
pub fn read_verified(dir: &Path, spec: &ModelSpec) -> Result<Vec<u8>, ModelError> {
    let directory = fs::metadata(dir)?;
    if !directory.is_dir() {
        return Err(ModelError::NotRegularFile);
    }
    check_owner_and_mode(&directory)?;
    let path = dir.join(spec.file);
    let before = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ModelError::Missing(path));
        }
        Err(error) => return Err(error.into()),
    };
    if before.file_type().is_symlink() {
        return Err(ModelError::Symlink);
    }
    if !before.is_file() {
        return Err(ModelError::NotRegularFile);
    }
    if before.nlink() != 1 {
        return Err(ModelError::HardLinked(before.nlink()));
    }
    check_owner_and_mode(&before)?;
    if before.len() != spec.bytes {
        return Err(ModelError::Size {
            expected: spec.bytes,
            actual: before.len(),
        });
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(&path)?;
    let opened = file.metadata()?;
    if (opened.dev(), opened.ino()) != (before.dev(), before.ino()) {
        return Err(ModelError::Changed);
    }
    let mut bytes = Vec::with_capacity(spec.bytes as usize);
    Read::by_ref(&mut file)
        .take(spec.bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != spec.bytes {
        return Err(ModelError::Changed);
    }
    let actual = sha256_hex(&bytes);
    if actual != spec.sha256 {
        return Err(ModelError::Digest { actual });
    }
    Ok(bytes)
}

/// Copy a verified model into `dir` (which must already exist and not be
/// writable by others): write a new private temporary file, sync it, rename
/// it into place, sync the directory, then verify the installed path.
/// An existing different file is never overwritten.
pub fn install(source: &Path, dir: &Path, spec: &ModelSpec) -> Result<PathBuf, ModelError> {
    let source_metadata = fs::symlink_metadata(source)?;
    if source_metadata.file_type().is_symlink() {
        return Err(ModelError::Symlink);
    }
    if !source_metadata.is_file() {
        return Err(ModelError::NotRegularFile);
    }
    if source_metadata.len() != spec.bytes {
        return Err(ModelError::Size {
            expected: spec.bytes,
            actual: source_metadata.len(),
        });
    }
    let mut bytes = Vec::with_capacity(spec.bytes as usize);
    fs::File::open(source)?
        .take(spec.bytes + 1)
        .read_to_end(&mut bytes)?;
    let actual = sha256_hex(&bytes);
    if bytes.len() as u64 != spec.bytes || actual != spec.sha256 {
        return Err(ModelError::Digest { actual });
    }
    let directory = fs::metadata(dir)?;
    check_owner_and_mode(&directory)?;
    let target = dir.join(spec.file);
    if fs::symlink_metadata(&target).is_ok() {
        // Identical bytes are already installed; anything else is refused.
        read_verified(dir, spec)?;
        return Ok(target);
    }
    let temporary = dir.join(format!(".{}.{}.partial", spec.file, std::process::id()));
    let result = (|| -> Result<(), ModelError> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
        file.sync_all()?;
        fs::rename(&temporary, &target)?;
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    read_verified(dir, spec)?;
    Ok(target)
}

/// The vendored copy inside this source tree, for tests and provisioning.
pub fn vendored_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models")
}
