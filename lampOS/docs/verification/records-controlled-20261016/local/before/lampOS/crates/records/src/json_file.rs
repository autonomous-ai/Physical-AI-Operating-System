//! Bounded configuration/result files with descriptor-relative atomic writes.
use crate::config::{absolute_path, invalid};
use rustix::{
    fd::OwnedFd,
    fs::{self, AtFlags, FileType, Mode, OFlags},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{File, Metadata},
    io::{self, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub const CONFIG_BYTES: usize = 65_536;
pub const REPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    modified_s: i64,
    modified_ns: i64,
    changed_s: i64,
    changed_ns: i64,
}

fn no_atime() -> OFlags {
    #[cfg(target_os = "linux")]
    {
        OFlags::NOATIME
    }
    #[cfg(not(target_os = "linux"))]
    {
        OFlags::empty()
    }
}

/// Open read-only, without touching the access time where the kernel allows
/// it. Linux grants `O_NOATIME` only for a file the caller owns (or with
/// `CAP_FOWNER`) and refuses any other with `EPERM`, so a reader that is not
/// root opens `/` and root's directories without it. Found on 2026-10-14 when
/// the ARM64 tests first ran as an ordinary user.
fn open_read(dir: &OwnedFd, name: &std::ffi::OsStr, flags: OFlags) -> rustix::io::Result<OwnedFd> {
    match fs::openat(dir, name, flags | no_atime(), Mode::empty()) {
        Err(rustix::io::Errno::PERM) if !no_atime().is_empty() => {
            fs::openat(dir, name, flags, Mode::empty())
        }
        opened => opened,
    }
}

fn owned(stat: &fs::Stat) -> bool {
    stat.st_uid == rustix::process::geteuid().as_raw() || stat.st_uid == 0
}

fn trusted_directory(fd: &OwnedFd, last_private: bool) -> io::Result<()> {
    let stat = fs::fstat(fd)?;
    let writable = stat.st_mode & 0o022 != 0;
    let sticky_root = stat.st_uid == 0 && stat.st_mode & 0o1000 != 0;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
        || !owned(&stat)
        || (writable && !sticky_root)
        || (last_private && stat.st_mode & 0o077 != 0)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "untrusted JSON file directory",
        ));
    }
    Ok(())
}

fn parent(path: &Path, private: bool) -> io::Result<OwnedFd> {
    absolute_path(path)?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = match fs::open("/", flags | no_atime(), Mode::empty()) {
        Err(rustix::io::Errno::PERM) if !no_atime().is_empty() => {
            fs::open("/", flags, Mode::empty())?
        }
        opened => opened?,
    };
    trusted_directory(&fd, false)?;
    for component in path
        .parent()
        .ok_or_else(|| invalid("missing parent"))?
        .components()
    {
        if let Component::Normal(name) = component {
            fd = open_read(&fd, name, flags)?;
            trusted_directory(&fd, false)?;
        }
    }
    trusted_directory(&fd, private)?;
    Ok(fd)
}

fn regular(stat: &fs::Stat) -> io::Result<()> {
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || !owned(stat)
        || stat.st_mode & 0o022 != 0
        || stat.st_nlink != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "JSON file must be a trusted, singly linked regular file",
        ));
    }
    Ok(())
}

fn stamp(stat: &Metadata) -> Stamp {
    Stamp {
        device: stat.dev(),
        inode: stat.ino(),
        size: stat.size(),
        modified_s: stat.mtime(),
        modified_ns: stat.mtime_nsec(),
        changed_s: stat.ctime(),
        changed_ns: stat.ctime_nsec(),
    }
}

/// A matching stamp avoids repeatedly parsing the last result between runs.
pub fn read_changed<T: DeserializeOwned>(
    path: &Path,
    limit: usize,
    prior: Option<&Stamp>,
) -> io::Result<Option<(T, Stamp)>> {
    let dir = parent(path, false)?;
    let fd = open_read(
        &dir,
        path.file_name().unwrap(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
    )?;
    let stat = fs::fstat(&fd)?;
    regular(&stat)?;
    let mut file = File::from(fd);
    let before = stamp(&file.metadata()?);
    if before.size > limit as u64 {
        return Err(invalid("JSON file exceeds its size bound"));
    }
    if prior == Some(&before) {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(before.size as usize);
    (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit || stamp(&file.metadata()?) != before {
        return Err(invalid(
            "JSON file changed during read or exceeded its bound",
        ));
    }
    let value = serde_json::from_slice(&bytes).map_err(|error| invalid(error.to_string()))?;
    Ok(Some((value, before)))
}

pub fn read<T: DeserializeOwned>(path: &Path, limit: usize) -> io::Result<T> {
    read_changed(path, limit, None)?
        .map(|(value, _)| value)
        .ok_or_else(|| io::Error::other("missing JSON result"))
}

pub fn temporary_path(path: &Path) -> io::Result<PathBuf> {
    absolute_path(path)?;
    Ok(path.with_file_name(format!(
        ".{}.lamp-records-tmp",
        path.file_name().unwrap().to_str().unwrap()
    )))
}

/// Path equality alone misses case-folded names. Compare validated descriptors
/// before permitting an output or its reserved temporary to replace an input.
pub fn same_file(first: &Path, second: &Path) -> io::Result<bool> {
    fn identity(path: &Path) -> io::Result<(u64, u64)> {
        let dir = parent(path, false)?;
        let file = File::from(open_read(
            &dir,
            path.file_name().unwrap(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        )?);
        regular(&fs::fstat(&file)?)?;
        let metadata = file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    let first = identity(first)?;
    match identity(second) {
        Ok(second) => Ok(first == second),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// The report's parent must already exist and be private. No record root is
/// created as a side effect. Only this process's newly created temp is removed.
pub fn write<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > REPORT_BYTES {
        return Err(invalid("report exceeds its size bound"));
    }
    let dir = parent(path, true)?;
    fs::flock(&dir, fs::FlockOperation::NonBlockingLockExclusive)?;
    let name = path.file_name().unwrap();
    match fs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => regular(&stat)?,
        Err(rustix::io::Errno::NOENT) => {}
        Err(error) => return Err(error.into()),
    }
    // A single reserved temp per output, under the writer lock, bounds crash
    // residue. Never remove a linked/special/untrusted replacement there.
    let temporary = temporary_path(path)?;
    let temp = temporary.file_name().unwrap().to_str().unwrap();
    match fs::statat(&dir, temp, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => {
            regular(&stat)?;
            if stat.st_mode & 0o7777 != 0o600 {
                return Err(invalid("temporary report file is not private"));
            }
            fs::unlinkat(&dir, temp, AtFlags::empty())?;
        }
        Err(rustix::io::Errno::NOENT) => {}
        Err(error) => return Err(error.into()),
    }
    let fd = fs::openat(
        &dir,
        temp,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    let mut file = File::from(fd);
    let result = (|| -> io::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::renameat(&dir, temp, &dir, name)?;
        fs::fsync(&dir)?;
        Ok(())
    })();
    // After successful rename this name is absent. It was exclusively ours;
    // trusted private-parent ownership is the namespace contract here too.
    if result.is_err() {
        let _ = fs::unlinkat(&dir, temp, AtFlags::empty());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs as stdfs,
        os::unix::fs::{PermissionsExt, symlink},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn scratch() -> std::path::PathBuf {
        let dir = stdfs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "lamp-records-json-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        stdfs::create_dir(&dir).unwrap();
        stdfs::set_permissions(&dir, stdfs::Permissions::from_mode(0o700)).unwrap();
        dir
    }
    #[test]
    fn atomic_result_replaces_only_known_file_and_cache_detects_change() {
        let dir = scratch();
        let path = dir.join("last.json");
        write(&path, &vec![1_u64, 2]).unwrap();
        let (value, stamp) = read_changed::<Vec<u64>>(&path, 100, None).unwrap().unwrap();
        assert_eq!(value, vec![1, 2]);
        assert!(
            read_changed::<Vec<u64>>(&path, 100, Some(&stamp))
                .unwrap()
                .is_none()
        );
        write(&path, &vec![3_u64]).unwrap();
        assert_eq!(read::<Vec<u64>>(&path, 100).unwrap(), vec![3]);
        assert!(
            read_changed::<Vec<u64>>(&path, 100, Some(&stamp))
                .unwrap()
                .is_some()
        );
        assert_eq!(stdfs::read_dir(&dir).unwrap().count(), 1);
        assert!(read::<Vec<u64>>(&path, 1).is_err());
        stdfs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn result_symlink_and_hardlink_are_refused_without_touching_target() {
        let dir = scratch();
        let target = dir.join("outside.json");
        stdfs::write(&target, b"[42]").unwrap();
        let link = dir.join("last.json");
        symlink(&target, &link).unwrap();
        assert!(write(&link, &vec![0]).is_err());
        assert!(read::<Vec<u64>>(&link, 100).is_err());
        stdfs::remove_file(&link).unwrap();
        stdfs::hard_link(&target, &link).unwrap();
        assert!(write(&link, &vec![0]).is_err());
        assert!(read::<Vec<u64>>(&link, 100).is_err());
        assert_eq!(stdfs::read(&target).unwrap(), b"[42]");
        stdfs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn one_private_reserved_temporary_is_recovered_after_an_interrupted_write() {
        let dir = scratch();
        let path = dir.join("last.json");
        let temp = dir.join(".last.json.lamp-records-tmp");
        stdfs::write(&temp, b"partial JSON from an interrupted write").unwrap();
        stdfs::set_permissions(&temp, stdfs::Permissions::from_mode(0o600)).unwrap();
        write(&path, &vec![7_u64]).unwrap();
        assert_eq!(read::<Vec<u64>>(&path, 100).unwrap(), vec![7]);
        assert!(!temp.exists());
        symlink(&path, &temp).unwrap();
        assert!(write(&path, &vec![8_u64]).is_err());
        assert_eq!(read::<Vec<u64>>(&path, 100).unwrap(), vec![7]);
        stdfs::remove_dir_all(dir).unwrap();
    }
}
