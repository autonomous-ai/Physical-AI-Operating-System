use lamp_controls::mute::MuteRecord;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = PathBuf::from(format!(
            "/tmp/ct-mute-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn file(&self) -> PathBuf {
        self.0.join("mute.json")
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn owner_restart_restores_mute_before_any_permission_is_considered() {
    let root = Root::new();
    let mut record = MuteRecord::load(&root.file(), "boot", 1_000).unwrap();
    assert!(!record.muted);
    assert!(record.request_mute(100));
    record.save(&root.file()).unwrap();
    assert_eq!(
        MuteRecord::load(&root.file(), "boot", 1_000).unwrap(),
        record
    );
    assert_eq!(
        fs::metadata(root.file()).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn explicit_clear_survives_owner_restart_and_ignores_retained_mute() {
    let root = Root::new();
    let mut record = MuteRecord::new("boot").unwrap();
    record.request_mute(100);
    record.clear_after_explicit_user_request(200).unwrap();
    record.save(&root.file()).unwrap();
    let mut restored = MuteRecord::load(&root.file(), "boot", 300).unwrap();
    assert!(!restored.request_mute(100));
    assert!(!restored.request_mute(200));
    assert!(!restored.muted);
    assert!(restored.request_mute(201));
    assert!(restored.muted);
}

#[test]
fn a_new_machine_boot_resets_only_the_software_preference() {
    let root = Root::new();
    let mut record = MuteRecord::new("old-boot").unwrap();
    record.request_mute(1_000_000);
    record.save(&root.file()).unwrap();
    let restored = MuteRecord::load(&root.file(), "new-boot", 1).unwrap();
    assert_eq!(restored, MuteRecord::new("new-boot").unwrap());
}

#[test]
fn malformed_future_or_oversized_state_is_not_unmuted() {
    let root = Root::new();
    for bytes in [
        b"{broken".to_vec(),
        vec![b'x'; 4097],
        br#"{"version":1,"clock_id":"boot","muted":false,"through_us":999}"#.to_vec(),
    ] {
        fs::write(root.file(), bytes).unwrap();
        fs::set_permissions(root.file(), fs::Permissions::from_mode(0o600)).unwrap();
        assert!(MuteRecord::load(&root.file(), "boot", 100).is_err());
    }
}

#[test]
fn stale_clear_and_unknown_record_versions_are_refused() {
    let mut record = MuteRecord::new("boot").unwrap();
    record.request_mute(100);
    assert!(record.clear_after_explicit_user_request(99).is_err());
    assert!(record.muted);
    record.version = 2;
    assert!(record.check().is_err());
}

#[test]
fn exposed_or_symlink_state_and_nonprivate_directory_are_refused() {
    let root = Root::new();
    let record = MuteRecord::new("boot").unwrap();
    record.save(&root.file()).unwrap();
    fs::set_permissions(root.file(), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(MuteRecord::load(&root.file(), "boot", 100).is_err());
    fs::remove_file(root.file()).unwrap();
    let link = root.0.join("other");
    fs::write(&link, "untouched").unwrap();
    std::os::unix::fs::symlink(&link, root.file()).unwrap();
    assert!(record.save(&root.file()).is_err());
    assert_eq!(fs::read_to_string(link).unwrap(), "untouched");
    fs::remove_file(root.file()).unwrap();
    fs::set_permissions(&root.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(record.save(&root.file()).is_err());
}

#[test]
fn an_incomplete_other_write_is_not_overwritten() {
    let root = Root::new();
    MuteRecord::new("boot").unwrap().save(&root.file()).unwrap();
    fs::write(root.0.join(".mute.json.pending"), "incomplete").unwrap();
    let mut record = MuteRecord::new("boot").unwrap();
    record.request_mute(100);
    assert!(record.save(&root.file()).is_err());
    assert!(MuteRecord::load(&root.file(), "boot", 100).is_err());
    assert!(record.muted);
    assert_eq!(
        fs::read_to_string(root.0.join(".mute.json.pending")).unwrap(),
        "incomplete"
    );
}
