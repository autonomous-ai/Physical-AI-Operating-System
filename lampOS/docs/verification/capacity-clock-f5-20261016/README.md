# Capacity fixture clock — first f5 qualification

The original 20-second answer / five-second reply-buffer test keeps its complete
scenario, packet/capacity assertions, four-second playback hold and real IPC.
Only its modeled speaker/service timing uses paused Tokio time. Ordinary worker
code, host IPC/authority clocks, the unpaused interruption test and existing
ignored long measurements remain unchanged.

This is the **first local qualification on f5a20ba28f3227ab06979b6e662883aede96b046**,
with current upstream Memory #961 and build #976 repairs included. All times
come from `date`. Host: x86_64 macOS, Rust 1.96.0, locked/offline, jobs 2,
incremental off, debug/dev profile, one reused exclusive target. Native ARM64
and device qualification are pending. No commit, push or PR was made here.

## Results

| Stage | First result |
| --- | --- |
| Clean four-package compiled list | 2,504 identities, 13 ignored, 84 binaries |
| Original selected baseline | PASS, 24.108 s |
| Formatting / strict all-target Clippy | Both exit 0 |
| Candidate compiled comparison | All 2,504 identities and 13 ignore flags exact; zero added/removed |
| Same selected original after | PASS, 2.074 s |
| Full serial affected suite | 2,491 PASS / 0 FAIL / 13 ignored; every nonignored identity exactly once |
| Workspace build | Exit 0 |
| Four-package documentation checks | Exit 0; four doc targets, zero doctest cases |
| Source audit | 914 scoped inputs and all 4,343 tracked lampOS inputs exact through completion |

The full suite ran 11:26:30–11:48:06; build ended 11:49:58; documentation ended
11:50:04; the runner completed 11:50:05. The independent audit was recorded at
11:50:30. All are +0700 on 2026-10-16. Baseline/candidate selected commands ran
once each; the separately planned complete suite ran once. Retries were zero.
The selected case also passed in that full suite at 2.063 s; this is not used
to replace its first selected after measurement.

The **24.108 → 2.074 s** pair measures local nextest process execution with the
same four-package command/toolchain/target. Shared host load was uncontrolled.
The after output is fixture time, not physical playback or hardware latency.
Both complete printed result lines, distinct binary hashes and exact commands
are retained. The historical **24.049 → 0.516 s / 1,105-library-identity** pair
belongs to initial base `37b8be9e`, not f5; the source doc explicitly says so.

## Preservation and previous failures

`f5-gate/terminal-audit/independent-result.json` independently reconciles the
compiled set, 2,491 unique full-runtime outcomes, ordinal sequence, zero retry
markers and hashes. Source inverses retain all 109 existing worker test bodies,
562 assertion macros, 24 capacity assertions, numeric tokens, ignore attributes
and the upstream `cut_in: false` addition. There are no new source tests.

The old dirty e5aa worktree, its HEAD/status and 997 input hashes are untouched.
`retained-e5aa-failed-gate/` preserves the earlier **2,385 PASS / 3 FAIL / 13
ignored** gate and independent successful build. Its Memory voice_people,
Motor #906 and second-route failures remain failures. Each exact identity passes
once in the current f5 full gate; that is neither a cause diagnosis nor a
blanket known-flake exception. #935's reminder identity is distinct and also
passes once here. #893 was closed administratively, not fixed by that closure.
See `historical-failure-followthrough.json` and `f5-gate/owner-status-note.json`.
No #739 exception is invoked for this green local gate.

`reviewed-source-proposal/` preserves the source-only review exactly, including
its then-pending qualification wording and initial doc append. The separately
approved correction in `historical-doc-clarification/` labels old measurements
as historical. `frozen-current-source/` and `approved-source-files.json` name
the actual two qualified source/doc bytes. Old raw evidence is never rewritten
to reflect newer outcomes. The 14 initial historical files already copied into
`../capacity-clock-20261015/` are individually pinned by the source/history
manifest and `SOURCE-HISTORY-SHA256SUMS`.

## Exact gates and scope

The Cargo metadata reverse dependency closure is Memory, Voice and Voice-eval;
Surround remains the broader historical gate selection. Commands, environments,
start/end times, exits and raw logs are under `f5-gate/baseline/` and
`f5-gate/candidate/`. In command summaries below, `PACKAGES` means
`-p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-surround`:

```text
cargo +1.96.0 fmt --all --check
cargo +1.96.0 clippy --locked --offline PACKAGES --all-targets -- -D warnings
cargo +1.96.0 nextest list --locked --offline PACKAGES --message-format json
cargo +1.96.0 nextest run --locked --offline PACKAGES --profile default --test-threads 1 --retries 0 --no-fail-fast
cargo +1.96.0 build --locked --offline --workspace
cargo +1.96.0 test --locked --offline PACKAGES --doc -- --test-threads=1
```

The selected command uses exact `binary_id(lamp-voice)` and the complete
original test name, with success output immediate. Full execution uses the
unchanged default profile. Full commands are serialized as argument arrays,
not abbreviated, in both `gates.json` files. No native run or lamp action was
started by this gate.

## Evidence integrity

`SHA256SUMS` covers every package file except itself; the external exact
publication manifest additionally hashes it and maps all destinations.
`raw-copy-provenance.json` pins each untouched copied source. Excluded scripts
are named and hashed in `excluded-scripts.json`; no scripts, compiled output,
binaries or symlinks are included. The first external Python runner syntax
preflight is retained externally and documented, with no Rust/test execution
or source error hidden. It was corrected before the candidate gate started.

The target is released after terminal audit; this package does not require
retained binary files. No rerun is necessary to verify these recorded results.
