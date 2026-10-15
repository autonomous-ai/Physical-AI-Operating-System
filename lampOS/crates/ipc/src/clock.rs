//! Whether the wall clock is set (#830, `docs/offline.md`). The lamp has no
//! battery clock: after power it starts at its last saved time until a time
//! client sets it, and with no internet it stays wrong.

/// Whether the kernel's clock is kept by a network time client, read as
/// systemd's `ntp_synced()` reads it for `timedatectl`: `adjtimex` with
/// nothing to change, unsynchronised status off and a maximum error under
/// 16 s. Reading needs no privilege. The only unsafe call here: the
/// standard library and rustix do not wrap `adjtimex`, and with `modes`
/// zero it only reads into the struct it is given.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub fn clock_synced() -> Option<bool> {
    // SAFETY: an all-zero `timex` is valid; with `modes` zero the call only
    // fills it in and changes nothing.
    let mut timex: libc::timex = unsafe { std::mem::zeroed() };
    // SAFETY: `timex` is a valid, exclusively borrowed `struct timex`.
    let state = unsafe { libc::adjtimex(&mut timex) };
    if state < 0 {
        return None;
    }
    Some(timex.status & libc::STA_UNSYNC == 0 && timex.maxerror < 16_000_000)
}

/// Elsewhere the kernel cannot be asked.
#[cfg(not(target_os = "linux"))]
pub fn clock_synced() -> Option<bool> {
    None
}
