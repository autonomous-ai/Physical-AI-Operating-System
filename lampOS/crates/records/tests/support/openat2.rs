// On Linux the store opens every directory with `openat2`, and refuses to run
// without it. User-mode emulation (`qemu-aarch64` 7.2, as in a local podman
// ARM64 gate) has no `openat2`, so a test that opens a store cannot run there:
// it says so and stops (#1210). A real kernel has had `openat2` since 5.6, so
// CI never skips, and fails if it would.

#[cfg(target_os = "linux")]
pub fn store_cannot_run_here() -> bool {
    use rustix::fs::{CWD, Mode, OFlags, ResolveFlags, openat2};
    use std::io::Write;
    static MISSING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let missing = *MISSING.get_or_init(|| {
        let probe = openat2(
            CWD,
            "/",
            OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::empty(),
        );
        matches!(probe, Err(rustix::io::Errno::NOSYS))
    });
    if missing {
        assert!(
            std::env::var_os("CI").is_none(),
            "openat2 is not implemented (ENOSYS) on CI: the store's tests must run there"
        );
        // Straight to stderr, beside the test's name: libtest captures
        // `eprintln!` and shows it only for a failing test (the reviewer on
        // #1211).
        let _ = writeln!(
            std::io::stderr(),
            "skipped: openat2 is not implemented here (ENOSYS, as under user-mode \
             emulation), and the store refuses to run without it"
        );
    }
    missing
}

#[cfg(not(target_os = "linux"))]
pub fn store_cannot_run_here() -> bool {
    false
}
