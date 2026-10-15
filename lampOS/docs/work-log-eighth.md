# Work log: eighth

The eighth builder's log, newest first. Role file:
[builders/eighth.md](../builders/eighth.md). Lamp: lamp-75c9.

## Proposed tasks

| Id | Proposal | Answer |
|---|---|---|

## Found, not mine

Nothing yet.

## Questions

None yet.

## 2026-10-15 — #739 voice fixtures — taken

Ready for lamp-voice: no

- Taken at 12:50 +07, moved from the fourth by the first builder at 12:25,
  worked while #704 waits on lamp-75c9 (its person is using it). Branch
  `eighth/voice-fixtures`: the fourth's `ddb13741b` (`fourth/voice-fixtures`,
  v8: nine clean loaded runs, run 10 not started) rebased without conflict
  on `lamp-voice` at `08e94288e`. Production bounds in it are unchanged:
  `PRIVACY_FRESH_US` is still 50,000 us, and the fixture clock and the
  fixture freshness exist only under `cfg(test)`.
- This Mac differs from the fourth's: Apple M-series, arm64, 15 cores,
  Docker with 2 CPUs (the fourth's: Intel x86_64 with a Podman VM). Rust
  1.96.0 installed here today through rustup, with the person's yes.
- Unloaded, the rebased branch, `cargo test -p lamp-voice -- --test-threads=1`
  once: lib 984 passed, 0 failed, 8 ignored (326 s); every other suite
  passed. The load for the qualification: a Docker linux/arm64 release build
  of the workspace and a native release build at `-j 8`, each cleaned and
  repeated, with the load average sampled every 30 s.

## 2026-10-15 — #704 weigh — taken

Ready for lamp-voice: no

- Taken at 12:34 +07 (`taken by eighth · session 71157012-547b-4a9a-a2ab-fa9e8ae6e9c9 · lamp-75c9`
  on #704; the first "taken" comment is this one). Moved from the fifth by
  the first builder at 12:25 at this builder's person's request; next is #739.
- Kind: another lamp's own folder (`deploy/lamp-75c9/`), plus a data file and
  tests in `crates/lean`; the library is not changed.
- Starting point: lean.md, "Measurements the first builder needs" and
  "lamp-4ace, 14 October". The one figure so far is the fifth builder's:
  lamp-52e6 weighs 3.0 kg whole. lamp-75c9 is measured on its own.
- lamp-75c9 has no `deploy/lamp-75c9/LAMP.md` yet; nothing has been run on
  it, and no ssh has been made.
