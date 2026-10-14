# Schedule consumer scheduler audit

Read-only audit, retrieved 2026-10-14. No Cargo, dependency download, source/Git
edit, service call, scheduler change, or device access was performed.

## Existing path and inheritance

`lampOS/deploy/lamp-4ace/lamp-voice.service:30` selects RR, priority 20. It
does not select reset-on-fork. `reminders/transport.rs:60–78` launches the same
executable with `schedule-consumer`, redirects stdout/stderr, and sets only a
new process group. `main.rs:14` reaches `reminders::run_consumer` directly.

Linux preserves scheduling policy/parameters across fork and exec. Reset-on-
fork is an explicit exception; lowering RR to OTHER is allowed even with a
zero realtime priority limit, subject to other security restrictions.
[Linux scheduling manual](https://man7.org/linux/man-pages/man7/sched.7.html).
`CommandExt::process_group(0)` changes PGID via setpgid, not scheduler policy.
[Rust CommandExt](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.process_group).
Therefore the existing child inherits RR20 if its spawning thread has RR20
and no reset flag. This is a source/kernel-contract conclusion, not a live
lamp observation; deployment overrides and thread-specific changes were not
measured.

## Library finding

Pinned rustix1.1.5 has no safe policy setter: local
`src/not_implemented.rs:81` lists sched_setscheduler; thread/sched.rs provides
affinity/getcpu only. Enabling its thread feature does not solve this. Its
process getpriority/setpriority operate on niceness, not RR policy.

The published thread-priority manifest identifies release3.1.1, MIT,
edition2024, MSRV1.85. Linux dependencies are libc>=0.2.123, bitflags2,
cfg-if1, log0.4, rustversion1. No feature section or build script is declared.
Use a Linux-target dependency pinned `=3.1.1`; it is not currently locked.
[Published manifest](https://docs.rs/crate/thread-priority/latest/source/Cargo.toml),
[maintainer repository](https://github.com/iddm/thread-priority).

Verified safe API:

```rust
let id = thread_priority::thread_native_id();
let policy = thread_priority::ThreadSchedulePolicy::Normal(
    thread_priority::NormalThreadSchedulePolicy::Other,
);
thread_priority::set_thread_priority_and_policy(
    id,
    thread_priority::ThreadPriority::Os(
        thread_priority::ThreadPriorityOsValue::default(),
    ),
    policy,
)?;
let (actual, params) = thread_priority::thread_schedule_policy_param(id)?;
// Require actual == policy && params.sched_priority == 0.
```

The setter returns Result<(), Error>; getter returns
Result<(ThreadSchedulePolicy, ScheduleParams), Error>. Linux normal-policy
implementation sets pthread policy with static priority0, then sets current
thread niceness. Thus Os(default()) also requests nice0. Errors are explicit,
including Error::OS(i32); a nice failure can follow successful demotion.
`set_current_thread_priority` preserves policy; Min with Other requests nice19.
[Setter API](https://docs.rs/thread-priority/latest/thread_priority/unix/fn.set_thread_priority_and_policy.html),
[implementation](https://docs.rs/thread-priority/latest/src/thread_priority/unix.rs.html#577-724).
OsValue is a private u32 wrapper with derived Default; Unix has no From<u32>
conversion, so `0u32.into()` is not the supported construction.
[OS value](https://docs.rs/thread-priority/latest/thread_priority/struct.ThreadPriorityOsValue.html),
[error types](https://docs.rs/thread-priority/latest/src/thread_priority/lib.rs.html#184-215).

## Minimal recommendation and failure boundary

Call a private Linux-only reset-and-readback helper as the first enabled
child operation in run_consumer, before reading paths, acquiring the claim,
opening storage, or creating any threads. Current consumer::run obtains its
claim at consumer.rs:455 and opens the ledger at458. Keep the non-Linux
helper a documented no-op for host fixtures; that proves no Linux demotion.
Leave the parent, audio workers, unit configuration, and default-off path
alone. No pre_exec, inline unsafe, shell, or installed chrt dependency.

Fail the optional child on setter/getter failure or mismatched readback,
including partial success. It must send no claim or offer, open no ledger,
and never report ready. Parent's existing unavailable-child handling should
leave ordinary Voice running. No retry or fallback that continues under RR.
If inherited nice>0 cannot be raised to0, the conservative result is optional
unavailability; do not silently add privileges. Nice0 is a deliberate scope
choice, separate from static scheduler priority0.

## Meaningful no-lamp verification, not performed

1. Linux ordinary subprocess: run the actual child bootstrap, read Other/0
   and nice0 before any claim/storage hook; compare parent policy unchanged.
2. Isolated Linux helper with CAP_SYS_NICE or suitable RLIMIT_RTPRIO: set only
   that helper to RR20, execute the same Command/process_group path, verify
   child begins RR20 then becomes Other/0 before claim, while helper stays
   RR20. Use bounded blocking IPC and an external test timeout, never put the
   Cargo runner or main Voice process into realtime. If unavailable, report
   missing RR-transition coverage explicitly rather than a passing test.
3. Inject reset, readback, and mismatch failures before the continuation;
   assert no claim/files/provider setup and optional unavailable status, while
   the parent remains usable. Verify the real process exits on bootstrap error.
4. macOS compile/fixtures establish portability only. No API/source inspection
   demonstrates lamp scheduling behavior or an audio latency improvement.

The code snippet is reviewed API design only; it has not been compiled here.

## Authorized implementation addendum

After this read-only recommendation, Main explicitly chose Other + Min,
deliberately requesting nice19 instead of nice0, and authorized the narrow
implementation. This removes the positive-inherited-nice problem described
above: it never asks for higher normal-policy priority. Optional work may
receive less CPU under load; no response-time improvement is claimed.

Changed only Voice Cargo.toml (Linux-only pinned dependency), reminders.rs
(private module and reset immediately after exact on gate), and new
reminders/scheduling.rs. The helper propagates setter/getter failures and
checks Other/static0/nice19 before any configured paths, claim, or storage.
Non-Linux is explicitly a no-op. One Linux-only real setter/getter test runs
on a separate std::thread and checks that the calling test thread's policy,
static priority, and nice value remain unchanged. It does not claim to test
an inherited RR20 transition without a permitted realtime test environment.

Scoped rustfmt1.96.0 and its --check passed; empty successful raw output is
`/private/tmp/lamp-third-schedule-child-scheduler-format.txt`. Whitespace
diff check passed. No Cargo, lock edit, dependency download, test execution,
or device access was performed; compilation and the Linux test remain pending
Main's serialized gate. No injection-only abstraction was introduced merely
to test direct error propagation before the remaining child entry code.
