// On Linux the store opens every directory with `openat2`, and refuses to run
// without it. User-mode emulation (`qemu-aarch64` 7.2, as in a local podman
// ARM64 gate) has no `openat2`, so a test that opens a store cannot run there:
// it says so and stops (#1210). A real kernel has had `openat2` since 5.6, so
// CI never skips, and fails if it would.

#[cfg(target_os = "linux")]
pub fn store_cannot_run_here() -> bool {
    use rustix::fs::{CWD, Mode, OFlags, ResolveFlags, openat2};
    static MISSING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *MISSING.get_or_init(|| {
        let probe = openat2(
            CWD,
            "/",
            OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::empty(),
        );
        if !matches!(probe, Err(rustix::io::Errno::NOSYS)) {
            return false;
        }
        assert!(
            std::env::var_os("CI").is_none(),
            "openat2 is not implemented (ENOSYS) on CI: the store's tests must run there"
        );
        eprintln!(
            "skipped: openat2 is not implemented here (ENOSYS, as under user-mode \
             emulation), and the store refuses to run without it"
        );
        true
    })
}

#[cfg(not(target_os = "linux"))]
pub fn store_cannot_run_here() -> bool {
    false
}
