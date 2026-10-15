//! One bounded JSON document, with a lifetime owner and durable replacement.
//!
//! Records owns the atomic write sequence. Its error can occur after rename,
//! so any failed save poisons this adapter until it is dropped and reopened.
//! Recovery validates and synchronizes the current file and directory before
//! returning its bytes. The engine still owns schema and value validation.
//!
//! The private directory and advisory lock exclude cooperating owners. They
//! do not establish a security boundary against a hostile process of the same
//! user. Descriptor and pathname checks detect accidental replacement; they
//! cannot make an uncooperative same-user writer participate in the lock.

use lamp_records::{config::absolute_path, json_file};
use rustix::fs::{self as unix, AtFlags, FileType, Mode, OFlags};
use serde_json::value::RawValue;
use std::{
    ffi::OsStr,
    fs::{File, Metadata, TryLockError},
    io,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub const MAX_STATE_BYTES: usize = 65_536;
const STATE: &str = "settings.json";
const LOCK: &str = ".settings.lock";

/// A save error is indeterminate: the new document may already be installed.
/// The caller must stop mutating and reopen/load before reporting a value as
/// durable. Implementations must bound input before parsing or allocating it.
pub trait Storage: Send + 'static {
    fn load(&mut self) -> io::Result<Option<Vec<u8>>>;
    fn save(&mut self, bytes: &[u8]) -> io::Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    bytes: u64,
    modified_s: i64,
    modified_ns: i64,
    changed_s: i64,
    changed_ns: i64,
}

impl From<&Metadata> for Stamp {
    fn from(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: metadata.len(),
            modified_s: metadata.mtime(),
            modified_ns: metadata.mtime_nsec(),
            changed_s: metadata.ctime(),
            changed_ns: metadata.ctime_nsec(),
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn private(metadata: &Metadata, directory: bool) -> io::Result<()> {
    let expected_mode = if directory { 0o700 } else { 0o600 };
    if (directory && !metadata.is_dir())
        || (!directory && (!metadata.is_file() || metadata.nlink() != 1))
        || metadata.mode() & 0o7777 != expected_mode
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "settings storage requires caller ownership, mode 0700 directories and singly linked mode 0600 files",
        ));
    }
    Ok(())
}

fn trusted_ancestor(metadata: &Metadata) -> io::Result<()> {
    let trusted_owner =
        metadata.uid() == 0 || metadata.uid() == rustix::process::geteuid().as_raw();
    let root_sticky = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
    if !metadata.is_dir() || !trusted_owner || (metadata.mode() & 0o022 != 0 && !root_sticky) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "settings directory has an untrusted ancestor",
        ));
    }
    Ok(())
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

/// These checks strengthen Records' root-or-caller and private-parent policy:
/// the settings directory itself must already be canonical, caller-owned and
/// exactly mode 0700. No directory is provisioned or repaired by this adapter.
fn directory(path: &Path) -> io::Result<File> {
    absolute_path(path)?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut held: File = unix::open("/", flags, Mode::empty())?.into();
    trusted_ancestor(&held.metadata()?)?;
    for component in path.components() {
        if let Component::Normal(name) = component {
            held = unix::openat(&held, name, flags, Mode::empty())?.into();
            trusted_ancestor(&held.metadata()?)?;
        }
    }
    private(&held.metadata()?, true)?;
    if path.canonicalize()? != path {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings directory must use its canonical absolute spelling",
        ));
    }
    Ok(held)
}

/// Check a named entry before opening it, then check the descriptor again.
/// NONBLOCK also prevents a concurrently substituted FIFO from stalling open.
fn existing(directory: &File, name: &OsStr, limit: usize) -> io::Result<Option<File>> {
    let before = match unix::statat(directory, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
        || before.st_uid != rustix::process::geteuid().as_raw()
        || before.st_mode & 0o7777 != 0o600
        || before.st_nlink != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "settings entry must be a caller-owned, singly linked mode 0600 regular file",
        ));
    }
    if before.st_size < 0 || before.st_size > limit as _ {
        return Err(invalid("settings file exceeds its byte bound"));
    }
    let held: File = unix::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into();
    private(&held.metadata()?, false)?;
    let after = unix::fstat(&held)?;
    if before.st_dev != after.st_dev || before.st_ino != after.st_ino {
        return Err(invalid("settings entry changed during open"));
    }
    if held.metadata()?.len() > limit as u64 {
        return Err(invalid("settings file exceeds its byte bound"));
    }
    Ok(Some(held))
}

fn owner_lock(directory: &File) -> io::Result<File> {
    let held = match existing(directory, OsStr::new(LOCK), 0)? {
        Some(held) => held,
        None => match unix::openat(
            directory,
            LOCK,
            OFlags::RDWR
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(fd) => File::from(fd),
            Err(rustix::io::Errno::EXIST) => existing(directory, OsStr::new(LOCK), 0)?
                .ok_or_else(|| invalid("settings owner lock disappeared"))?,
            Err(error) => return Err(error.into()),
        },
    };
    private(&held.metadata()?, false)?;
    held.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => io::Error::new(
            io::ErrorKind::WouldBlock,
            "settings directory already has an owner",
        ),
        TryLockError::Error(error) => error,
    })?;
    let named = existing(directory, OsStr::new(LOCK), 0)?
        .ok_or_else(|| invalid("settings owner lock disappeared"))?;
    if !same_file(&held.metadata()?, &named.metadata()?) {
        return Err(invalid("settings owner lock changed during acquisition"));
    }
    held.sync_all()?;
    directory.sync_all()?;
    Ok(held)
}

#[derive(Debug)]
pub struct FileStore {
    directory_path: PathBuf,
    path: PathBuf,
    temporary: PathBuf,
    directory: File,
    lock: File,
    expected: Option<Stamp>,
    poisoned: bool,
}

impl FileStore {
    /// Open a preexisting, canonical, caller-owned mode 0700 directory.
    /// The empty private lock file remains in place after this owner exits.
    pub fn open(dir: &Path) -> io::Result<Self> {
        let directory = directory(dir)?;
        let path = dir.join(STATE);
        let temporary = json_file::temporary_path(&path)?;
        existing(
            &directory,
            temporary
                .file_name()
                .ok_or_else(|| invalid("missing temporary name"))?,
            MAX_STATE_BYTES,
        )?;
        let lock = owner_lock(&directory)?;
        let state = existing(&directory, OsStr::new(STATE), MAX_STATE_BYTES)?;
        let expected = state
            .as_ref()
            .map(|file| file.metadata().map(|metadata| Stamp::from(&metadata)))
            .transpose()?;
        let store = Self {
            directory_path: dir.to_owned(),
            path,
            temporary,
            directory,
            lock,
            expected,
            poisoned: false,
        };
        store.verify_owner()?;
        Ok(store)
    }

    fn healthy(&self) -> io::Result<()> {
        if self.poisoned {
            return Err(io::Error::other(
                "settings storage requires reopen and recovery after a failed operation",
            ));
        }
        Ok(())
    }

    fn verify_owner(&self) -> io::Result<()> {
        let named_directory = directory(&self.directory_path)?;
        private(&self.directory.metadata()?, true)?;
        if !same_file(&self.directory.metadata()?, &named_directory.metadata()?) {
            return Err(invalid("settings directory identity changed"));
        }
        private(&self.lock.metadata()?, false)?;
        let named_lock = existing(&self.directory, OsStr::new(LOCK), 0)?
            .ok_or_else(|| invalid("settings owner lock disappeared"))?;
        if !same_file(&self.lock.metadata()?, &named_lock.metadata()?) {
            return Err(invalid("settings owner lock identity changed"));
        }
        Ok(())
    }

    fn temporary_present(&self) -> io::Result<bool> {
        Ok(existing(
            &self.directory,
            self.temporary
                .file_name()
                .ok_or_else(|| invalid("missing temporary name"))?,
            MAX_STATE_BYTES,
        )?
        .is_some())
    }

    fn current(&self) -> io::Result<Option<File>> {
        let file = existing(&self.directory, OsStr::new(STATE), MAX_STATE_BYTES)?;
        let stamp = file
            .as_ref()
            .map(|file| file.metadata().map(|metadata| Stamp::from(&metadata)))
            .transpose()?;
        if stamp != self.expected {
            return Err(invalid("settings state changed outside this owner"));
        }
        Ok(file)
    }

    fn load_inner(&self) -> io::Result<Option<Vec<u8>>> {
        self.verify_owner()?;
        self.temporary_present()?;
        let Some(held) = self.current()? else {
            self.directory.sync_all()?;
            self.verify_owner()?;
            self.current()?;
            return Ok(None);
        };
        // RawValue preserves duplicate fields. Deserializing through Value
        // would erase evidence needed by the engine's strict schema parser.
        let raw: Box<RawValue> = json_file::read(&self.path, MAX_STATE_BYTES)?;
        self.current()?;
        held.sync_all()?;
        self.directory.sync_all()?;
        self.verify_owner()?;
        self.current()?;
        Ok(Some(raw.get().as_bytes().to_vec()))
    }

    fn save_inner(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > MAX_STATE_BYTES {
            return Err(invalid("settings document exceeds its byte bound"));
        }
        let raw: &RawValue = serde_json::from_slice(bytes)
            .map_err(|_| invalid("settings document is not valid JSON"))?;
        self.verify_owner()?;
        self.temporary_present()?;
        self.current()?;
        // Raw JSON has already been bounded, so Records' serializer cannot
        // allocate an unbounded document before applying its larger report cap.
        json_file::write(&self.path, &raw)?;
        self.verify_owner()?;
        if self.temporary_present()? {
            return Err(invalid("settings temporary remained after replacement"));
        }
        let held = existing(&self.directory, OsStr::new(STATE), MAX_STATE_BYTES)?
            .ok_or_else(|| invalid("settings state disappeared after replacement"))?;
        let installed = Stamp::from(&held.metadata()?);
        let confirmed: Box<RawValue> = json_file::read(&self.path, MAX_STATE_BYTES)?;
        if confirmed.get() != raw.get() {
            return Err(invalid("settings contents changed after replacement"));
        }
        self.expected = Some(installed);
        self.verify_owner()?;
        self.current()?;
        Ok(())
    }
}

impl Storage for FileStore {
    fn load(&mut self) -> io::Result<Option<Vec<u8>>> {
        self.healthy()?;
        let result = self.load_inner();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn save(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.healthy()?;
        let result = self.save_inner(bytes);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// The tests below that start a process and those that save run one at
    /// a time: a child inherits Records' locked directory descriptor until it
    /// execs, and a parallel save into that directory would then be refused.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "lamp-settings-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, bytes: &[u8]) {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(self.0.join(name))
                .unwrap();
            file.write_all(bytes).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn state_round_trips_without_erasing_duplicate_fields() {
        let _serial = serial();
        let dir = Scratch::new();
        let bytes = br#"{"revision":1,"revision":2,"nested":{"level":1,"level":2}}"#;
        let mut store = FileStore::open(&dir.0).unwrap();
        assert_eq!(store.load().unwrap(), None);
        store.save(bytes).unwrap();
        assert_eq!(store.load().unwrap().unwrap(), bytes);
        assert_eq!(
            fs::metadata(dir.0.join(STATE)).unwrap().mode() & 0o7777,
            0o600
        );
        drop(store);
        let mut recovered = FileStore::open(&dir.0).unwrap();
        assert_eq!(recovered.load().unwrap().unwrap(), bytes);
    }

    #[test]
    fn owner_lock_is_lifetime_exclusive_and_persists_for_reopen() {
        let _serial = serial();
        let dir = Scratch::new();
        let owner = FileStore::open(&dir.0).unwrap();
        let error = FileStore::open(&dir.0).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        let lock = fs::metadata(dir.0.join(LOCK)).unwrap();
        assert_eq!(lock.len(), 0);
        assert_eq!(lock.mode() & 0o7777, 0o600);
        drop(owner);
        let _next = FileStore::open(&dir.0).unwrap();
        assert_eq!(fs::metadata(dir.0.join(LOCK)).unwrap().ino(), lock.ino());
    }

    #[test]
    fn owner_lock_excludes_another_process() {
        let _serial = serial();
        const CHILD_DIRECTORY: &str = "LAMP_SETTINGS_LOCK_PROBE_DIRECTORY";
        if let Some(path) = std::env::var_os(CHILD_DIRECTORY) {
            let error = FileStore::open(Path::new(&path)).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
            return;
        }
        let dir = Scratch::new();
        let _owner = FileStore::open(&dir.0).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store::tests::owner_lock_excludes_another_process",
            ])
            .env(CHILD_DIRECTORY, &dir.0)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }

    #[test]
    fn noncanonical_missing_and_untrusted_directories_are_refused() {
        let _serial = serial();
        let dir = Scratch::new();
        assert!(FileStore::open(Path::new("relative")).is_err());
        assert!(FileStore::open(&dir.0.join("missing")).is_err());
        assert!(FileStore::open(&dir.0.join(".")).is_err());
        assert!(FileStore::open(&dir.0.join("child/..")).is_err());
        assert!(FileStore::open(Path::new(&format!("{}/", dir.0.display()))).is_err());
        fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o750)).unwrap();
        assert!(FileStore::open(&dir.0).is_err());
        fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o700)).unwrap();
        let linked = dir.0.join("linked");
        symlink(&dir.0, &linked).unwrap();
        assert!(FileStore::open(&linked).is_err());
        assert!(FileStore::open(&linked.join("descendant")).is_err());
    }

    #[test]
    fn writable_ancestor_is_refused_even_when_final_directory_is_private() {
        let _serial = serial();
        let dir = Scratch::new();
        let parent = dir.0.join("parent");
        let private_child = parent.join("settings");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&private_child).unwrap();
        fs::set_permissions(&private_child, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(FileStore::open(&private_child).is_err());
    }

    #[test]
    fn data_temp_and_lock_refuse_links_special_files_and_public_permissions() {
        let _serial = serial();
        let state = Path::new("/private/settings.json");
        let temporary = json_file::temporary_path(state).unwrap();
        for name in [
            STATE,
            LOCK,
            temporary.file_name().unwrap().to_str().unwrap(),
        ] {
            for kind in ["symlink", "hardlink", "fifo", "directory", "public"] {
                let dir = Scratch::new();
                dir.write("outside", b"{}");
                let path = dir.0.join(name);
                match kind {
                    "symlink" => symlink(dir.0.join("outside"), &path).unwrap(),
                    "hardlink" => fs::hard_link(dir.0.join("outside"), &path).unwrap(),
                    "fifo" => {
                        // rustix exposes mkfifoat on Linux but not macOS.
                        // This POSIX helper creates only the private fixture.
                        assert!(
                            Command::new("mkfifo")
                                .arg(&path)
                                .status()
                                .unwrap()
                                .success()
                        );
                    }
                    "directory" => fs::create_dir(&path).unwrap(),
                    "public" => {
                        dir.write(name, b"");
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
                    }
                    _ => unreachable!(),
                }
                assert!(
                    FileStore::open(&dir.0).is_err(),
                    "accepted {name} as {kind}"
                );
                assert_eq!(fs::read(dir.0.join("outside")).unwrap(), b"{}");
            }
        }
    }

    #[test]
    fn oversized_files_and_invalid_json_never_become_loaded_state() {
        let _serial = serial();
        let dir = Scratch::new();
        dir.write(STATE, &vec![b' '; MAX_STATE_BYTES + 1]);
        assert!(FileStore::open(&dir.0).is_err());
        fs::remove_file(dir.0.join(STATE)).unwrap();
        for bytes in [b"".as_slice(), b"{", b"{} {}", b"\xff"] {
            dir.write(STATE, bytes);
            let mut store = FileStore::open(&dir.0).unwrap();
            assert!(store.load().is_err());
            assert!(store.save(b"{}").is_err());
            assert_eq!(fs::read(dir.0.join(STATE)).unwrap(), bytes);
            drop(store);
            fs::remove_file(dir.0.join(STATE)).unwrap();
        }
    }

    #[test]
    fn oversized_temporary_is_refused_without_deleting_it() {
        let _serial = serial();
        let dir = Scratch::new();
        let temporary = json_file::temporary_path(&dir.0.join(STATE)).unwrap();
        dir.write(
            temporary.file_name().unwrap().to_str().unwrap(),
            &vec![b' '; MAX_STATE_BYTES + 1],
        );
        assert!(FileStore::open(&dir.0).is_err());
        assert_eq!(
            fs::metadata(temporary).unwrap().len(),
            (MAX_STATE_BYTES + 1) as u64
        );
    }

    #[test]
    fn document_bound_is_checked_before_parse_or_write() {
        let _serial = serial();
        let dir = Scratch::new();
        let mut store = FileStore::open(&dir.0).unwrap();
        let mut at_limit = vec![b'a'; MAX_STATE_BYTES];
        at_limit[0] = b'"';
        at_limit[MAX_STATE_BYTES - 1] = b'"';
        store.save(&at_limit).unwrap();
        assert_eq!(store.load().unwrap().unwrap(), at_limit);
        let oversized = vec![0xff; MAX_STATE_BYTES + 1];
        let error = store.save(&oversized).unwrap_err();
        assert!(error.to_string().contains("byte bound"));
        assert_eq!(fs::read(dir.0.join(STATE)).unwrap(), at_limit);
        assert!(store.load().is_err());
    }

    #[test]
    fn known_temporary_is_recovered_without_accumulating_residue() {
        let _serial = serial();
        let dir = Scratch::new();
        let temporary = json_file::temporary_path(&dir.0.join(STATE)).unwrap();
        dir.write(temporary.file_name().unwrap().to_str().unwrap(), b"partial");
        let mut store = FileStore::open(&dir.0).unwrap();
        assert_eq!(store.load().unwrap(), None);
        for value in 0..8 {
            store
                .save(format!("{{\"revision\":{value}}}").as_bytes())
                .unwrap();
            assert!(!temporary.exists());
            assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
        }
    }

    #[test]
    fn failed_records_write_poison_requires_reopen() {
        let _serial = serial();
        let dir = Scratch::new();
        let mut store = FileStore::open(&dir.0).unwrap();
        store.save(b"{\"revision\":1}").unwrap();
        // Records takes a separate, nonblocking directory lock per write.
        // Contention exercises a real pre-rename write failure without hooks.
        let contender = File::open(&dir.0).unwrap();
        contender.try_lock().unwrap();
        assert!(store.save(b"{\"revision\":2}").is_err());
        drop(contender);
        assert!(store.save(b"{\"revision\":3}").is_err());
        assert!(store.load().is_err());
        drop(store);
        let mut recovered = FileStore::open(&dir.0).unwrap();
        assert_eq!(recovered.load().unwrap().unwrap(), b"{\"revision\":1}");
    }

    #[test]
    fn directory_and_lock_replacements_are_detected_before_save() {
        let _serial = serial();
        let dir = Scratch::new();
        let store_dir = dir.0.join("store");
        fs::create_dir(&store_dir).unwrap();
        fs::set_permissions(&store_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut store = FileStore::open(&store_dir).unwrap();
        store.save(b"{\"revision\":1}").unwrap();
        fs::rename(&store_dir, dir.0.join("moved")).unwrap();
        fs::create_dir(&store_dir).unwrap();
        fs::set_permissions(&store_dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(store.save(b"{\"revision\":2}").is_err());
        assert!(!store_dir.join(STATE).exists());
        assert_eq!(
            fs::read(dir.0.join("moved").join(STATE)).unwrap(),
            b"{\"revision\":1}"
        );
        drop(store);

        let mut store = FileStore::open(&store_dir).unwrap();
        fs::rename(store_dir.join(LOCK), store_dir.join("old-lock")).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(b"{}").is_err());
    }

    #[test]
    fn state_replacement_outside_owner_is_detected() {
        let _serial = serial();
        let dir = Scratch::new();
        let mut store = FileStore::open(&dir.0).unwrap();
        store.save(b"{\"revision\":1}").unwrap();
        dir.write("replacement", b"{\"revision\":2}");
        fs::rename(dir.0.join("replacement"), dir.0.join(STATE)).unwrap();
        assert!(store.save(b"{\"revision\":3}").is_err());
        assert_eq!(fs::read(dir.0.join(STATE)).unwrap(), b"{\"revision\":2}");
    }
}
