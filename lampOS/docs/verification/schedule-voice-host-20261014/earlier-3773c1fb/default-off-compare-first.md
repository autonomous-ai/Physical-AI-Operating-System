# Refreshed pre-change baseline at 3773c1fb

Commit: `3773c1fb88190fb5289bf1378eaa1f5fe5d134cb`.

Report: `/private/tmp/lamp-third-schedule-rebased-baseline.json`.
Source-only workspace:
`/private/tmp/lamp-third-schedule-rebased-baseline-bihzuwg7/lampOS`.

The report contains SHA-256 hashes of all 1,354 archived files before adding
the two scratch helpers, helper hashes, resolved local manifests, binaries,
all 72 cases with exact environments and output paths, and the raw commands.
All 33 local package manifests resolve inside this copy. The 34 Cargo.toml
files found directly in the tree also include the workspace manifest.

The earlier baseline JSON, capture script and `baseline-4vokhav1` tree were
not modified. This is a new baseline against the rebased upstream source;
its expanded helper and fixed fixture paths intentionally differ from the old
baseline, so compare final output to this report's bytes, not the old hashes.

## Observed result

Offline locked build passed. Two profiles each passed all 36 independent
tools/speak switch combinations: absent, `off`, `ON`, `true`, `1`, and ` on `.
All stdout bytes were exactly equal to that profile's absent/absent case;
all stderr was empty. The deliberately invalid relative Schedule socket and
ledger paths did not become files/directories. Every original source file
and both helpers retained their initial hash after the capture.

| Profile | Cases | Actions bytes | Voice bytes | Declared Voice tools |
|---|---:|---:|---:|---:|
| Existing tools + memory + clock | 36 | 1,194 | 5,857 | 14 |
| Same, with existing Tasks enabled | 36 | 1,194 | 7,035 | 16 |

Actions SHA-256 for both:
`86e2d2bd148df7f1521f2123b59e4566d72948350d2a3cb2f7e3996c5872a2c3`.
Ordinary Voice SHA-256:
`7a2c314940ec89528081ae6607b8feeb5338284781d788f583593e92c16dfc40`.
Tasks-enabled Voice SHA-256:
`1ccc5e2fc72e869defabee27a994d57567c827190219d4f4942a139ccb3086a8`.

The Tasks profile genuinely adds `start_agent_task` and `confirm_agent_task`
and changes their pure routes. No task is submitted and no Harness/provider,
socket or hardware is contacted. The helper reads only its fixture tools.json.
Ordinary Written serialization is captured verbatim and is equal in all cases.

## Exact final-copy comparison

1. Make a separate source-only copy of the final implementation. Copy these
   two helper files byte-for-byte from the baseline workspace to the same
   relative locations in the final copy:
   - `crates/actions/examples/schedule_off_actions.rs`
     (`8fc55e05703d1aa5ca58348132e3f4b0d7eed202e443f608f0d5af562c8f35e1`)
   - `crates/voice/examples/schedule_off_voice.rs`
     (`12d36f2d201b1117df21a55574855c8b44bcd2ec06f7b6fa980603e809b41695`)
2. In the final copy, use its own `CARGO_TARGET_DIR` and run:
   `cargo build --offline --locked -p lamp-actions -p lamp-voice --example schedule_off_actions --example schedule_off_voice`.
3. Reuse each case's exact `environment` object from the report. Do not
   regenerate fixture paths or normalize stdout; paths are part of the actual
   captured arguments and Debug route bytes. The fixed catalog directory is
   `/private/tmp/lamp-third-schedule-rebased-baseline-bihzuwg7/fixed-comparison-context/executive`.
4. Run the final Actions helper with no arguments, checking its stdout against
   the case's Actions file and requiring empty stderr. Only continue on exact
   equality. The baseline fixture tools.json then already has these identical
   bytes and can remain untouched.
5. Run the final Voice helper with `[catalog_directory, profile]` (profile is
   `ordinary` or `tasks`) and compare its stdout byte-for-byte with that case's
   Voice file, requiring empty stderr. The report records these complete
   baseline commands; change only the executable location to the final copy.
6. Record final source/helper/binary hashes and verify all resolved local
   manifests stay inside that final copy. Preserve this baseline evidence.

These are metadata/API byte-equivalence checks, not a test of a running
Actions server's optional adapter startup, the reminder consumer lifecycle,
provider readiness, speech or hardware. Those remain covered by their own
focused integration tests and eventual lamp qualification.

## Commands and retained logs

Parent orchestration cwd: `/Users/d/code/autonomous-os`.

`python3 /private/tmp/lamp-third-schedule-rebased-baseline-prepare.py`
prepared the checked archive and helpers. First attempt failed on the host's
older Python tarfile API before extraction/examples/build; raw transcript
`/private/tmp/lamp-third-schedule-rebased-baseline-prepare-first.txt`.
The checked regular-file/directory extraction then passed; output
`/private/tmp/lamp-third-schedule-rebased-baseline-prepare-second.txt`.

`python3 /private/tmp/lamp-third-schedule-rebased-baseline-capture.py`
ran the offline build and captures. Raw orchestration output:
`/private/tmp/lamp-third-schedule-rebased-baseline-capture-first.txt`.
Build raw output:
`/private/tmp/lamp-third-schedule-rebased-baseline-build-first.txt`.

The child Cargo command above ran from the baseline workspace and passed in
31.54 s measured command wall time (Cargo reports 31.43 s). This is build
duration on the development host, not lamp or interactive latency.
