//! Private, bounded records. Callers serialize writers and persist an intent
//! before issuing its idempotent side effect. A failed write is never an ack.
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read, Write},
    path::{Component, Path},
};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn normal_name(name: &OsStr) -> io::Result<()> {
    let text = name
        .to_str()
        .ok_or_else(|| invalid("record name must be ASCII"))?;
    if text.is_empty()
        || text.len() > 128
        || text == "."
        || text == ".."
        || !text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(invalid("invalid record name"));
    }
    Ok(())
}

fn open_directory(path: &Path) -> io::Result<File> {
    if !path.is_absolute() {
        return Err(invalid("an absolute, canonical directory path is required"));
    }
    let mut directory = File::open("/")?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = fs::openat(
                    &directory,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?
                .into();
            }
            _ => return Err(invalid("directory path must not contain dot components")),
        }
    }
    Ok(directory)
}

fn private(file: &File, kind: FileType) -> io::Result<()> {
    let stat = fs::fstat(file)?;
    if FileType::from_raw_mode(stat.st_mode) != kind
        || stat.st_mode & 0o077 != 0
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || (kind == FileType::RegularFile && stat.st_nlink != 1)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "record must be private, owned by this user, and not linked",
        ));
    }
    Ok(())
}

/// Open a private directory by walking every ancestor without following links.
/// `/tmp` and `/var` can be symlinks on macOS; supply their canonical paths.
pub fn directory(path: &Path) -> io::Result<File> {
    let file = open_directory(path)?;
    private(&file, FileType::Directory)?;
    Ok(file)
}

/// Create only the last component beneath existing, link-free ancestors.
pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    if directory(path).is_ok() {
        return Ok(());
    }
    let name = path
        .file_name()
        .ok_or_else(|| invalid("directory needs a final component"))?;
    normal_name(name)?;
    let parent = open_directory(
        path.parent()
            .ok_or_else(|| invalid("directory needs a parent"))?,
    )?;
    match fs::mkdirat(&parent, name, Mode::from_bits_retain(0o700)) {
        Ok(()) => parent.sync_all()?,
        Err(error) if error == rustix::io::Errno::EXIST => {}
        Err(error) => return Err(error.into()),
    }
    directory(path)?;
    Ok(())
}

fn parent_and_name(path: &Path) -> io::Result<(File, &OsStr)> {
    let name = path
        .file_name()
        .ok_or_else(|| invalid("record needs a filename"))?;
    normal_name(name)?;
    let parent = directory(
        path.parent()
            .ok_or_else(|| invalid("record needs a parent"))?,
    )?;
    Ok((parent, name))
}

fn open_record(parent: &File, name: &OsStr) -> io::Result<Option<File>> {
    let file: File = match fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(file) => file.into(),
        Err(error) if error == rustix::io::Errno::NOENT => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    private(&file, FileType::RegularFile)?;
    Ok(Some(file))
}

pub fn read(path: &Path, max_bytes: usize) -> io::Result<Option<Vec<u8>>> {
    let (parent, name) = parent_and_name(path)?;
    let Some(file) = open_record(&parent, name)? else {
        return Ok(None);
    };
    let limit = u64::try_from(max_bytes)
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("record bound is too large"))?;
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(invalid("record exceeds its byte bound"));
    }
    Ok(Some(bytes))
}

/// Write, sync, atomically replace, then sync the parent. A leftover temporary
/// file is uncommitted; the last complete record remains authoritative. All
/// side effects using these records must tolerate replay of their stable ID.
pub fn write(path: &Path, bytes: &[u8], max_bytes: usize) -> io::Result<()> {
    if bytes.len() > max_bytes {
        return Err(invalid("record exceeds its byte bound"));
    }
    let (parent, name) = parent_and_name(path)?;
    open_record(&parent, name)?;
    let temp = format!(".{}.pending", name.to_str().unwrap());
    let mut file: File = fs::openat(
        &parent,
        temp.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::from_bits_retain(0o600),
    )?
    .into();
    private(&file, FileType::RegularFile)?;
    file.set_len(0)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::renameat(&parent, temp.as_str(), &parent, name)?;
    parent.sync_all()
}

/// Remove one private record, never a directory or a link, and sync the parent.
pub fn remove(path: &Path) -> io::Result<()> {
    let (parent, name) = parent_and_name(path)?;
    if open_record(&parent, name)?.is_some() {
        fs::unlinkat(&parent, name, AtFlags::empty())?;
        parent.sync_all()?;
    }
    Ok(())
}

pub struct DirectoryLock {
    _file: File,
}

impl DirectoryLock {
    pub fn acquire(path: &Path) -> io::Result<Self> {
        ensure_private_dir(path)?;
        let parent = directory(path)?;
        let file: File = fs::openat(
            &parent,
            ".surround.lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_bits_retain(0o600),
        )?
        .into();
        private(&file, FileType::RegularFile)?;
        file.try_lock()
            .map_err(|error| io::Error::other(format!("surround directory is locked: {error}")))?;
        Ok(Self { _file: file })
    }
}
