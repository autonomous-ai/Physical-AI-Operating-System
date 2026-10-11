This branch carries four intentionally failing review regressions for MAIN-023/025. It is not a deployable runtime checkpoint. Agent 2 should copy or cherry-pick the tests and fix their owned evaluator code.

# Agent 2 voice-eval checkpoint review

**Recommendation: do not integrate unchanged. Two P2 dispatch defects remain, reproduced by four intentionally failing offline audit regressions. The original trigger-delay failure is fixed.**

Reviewed published `b7fbe352e62b01909bc22ab85eb25eabf6ec285e` over canonical `162585a28260b1bc44d51cbd196744dc443c5fc9`. Exactly 15 evaluator source/test/manifest/doc files are overlaid. No Agent 2 camera, perception, live, interaction or root workspace source was imported. Production hashes match `manifest.json` after all checks; the three extra `tests/audit_*` files are review scaffolding, excluded from `voice-eval-only.patch`.

## 1. P2 — final backend dispatch allows a terminated/invalid session

`crates/voice-eval/src/physical/mac.rs:970–984` drains relayed lines and queues the returned events. `handle()` sets `session.end` for SessionEnd at 541–556, or `session.cue_failed` for CueInvalid at 496–500 / cue_failure at 350–363. The final refusal at 985–991 checks only `provider_failed`; Player::start still occurs at 995. Therefore a SessionEnd or malformed/unknown cue consumed during the final drain can be followed by stimulus playback. The zero-wait receive at 602–603 also collapses disconnected and empty channels, so transport EOF is not independently fail-closed there.

`audit_final_dispatch.rs` drives the actual backend with a real local pipe transport and a recording-only Player. It establishes valid session/readiness and proves a known SessionEnd or CueInvalid/RunEnd each still starts the Player once, returns success and `withheld=None`. Both assertions fail in `dispatch.log`. This is a deterministic backend-boundary test with consumed termination, not a timed whole-run race or acoustic test. The production final drain consumes the same state immediately before that refusal decision.

**Correction:** latch all session/protocol/transport terminal states and check them immediately before Player::start. Distinguish an ended/invalid session from a provider failure in the returned withheld reason/status. An unknown terminal must not become a readiness or successful completion trigger. Add cases where the event is consumed in the final drain, plus EOF-without-SessionEnd; keep the provider failure latch.

## 2. P2 — final runner drain bypasses timing/overlap conditions

`crates/voice-eval/src/runner.rs:289–304` checks the 100 ms lateness limit and `require_lamp_speaking`. The new drain at 309 can consume more events or take time. Lines 310–317 then recheck only session/provider termination, before inject at 320. They do not recheck either condition that made the planned stimulus valid.

`audit_runner_dispatch.rs` runs the actual production `run_attempt` with a virtual clock:

- A speech retirement consumed in the final drain still permits the second topic-change stimulus at 3,200,000 us; it should be PreconditionLost. Injections are `[501000, 3200000]` rather than only the first.
- Finite 200 ms final-drain work still injects at 701,000 us against target 501,000 us, exceeding the 100 ms bound; it should be TriggerStale. No real delay threshold or acoustic performance is inferred.

Both assertions fail in `runner-dispatch.log`.

**Correction:** bound final-drain work, then re-evaluate current time and required speaking occurrence immediately before dispatch, as well as session/provider state. Preserve pending evidence and explicit withholding if the bound is exhausted. The physical backend should not silently invalidate a phase-dependent stimulus while doing additional final-drain work either; its final dispatch contract must carry enough eligibility information or return updated events for revalidation.

## Accepted improvements

- The archived original regression receives listening_ready at 1,000 us, admission at 100,000 us and provider_failed at 200,000 us before the planned 501,000 us dispatch. It now observes zero injections and ProviderUnavailable. Its sole API adaptation was `Injection.withheld=None`; timing/assertions are unchanged.
- The failure latch is attempt-wide, so advancing anchor/known_turn cannot revive scheduling.
- Provider failure is classified before lateness, while retaining a separate late finding. Partial playback remains evidence and must retire before completion. A failure stays in the answer denominator and outside the success numerator.
- Normalized provider session/outcome detail is bounded to 2,048 serialized bytes; oversized detail is marked truncated. Up to 32 original failure/drain records are retained in physical evidence.
- An actual unowned input_discarded_endpoint stays unowned; it is not a local endpoint or answer-latency boundary. The parser has no requirement for a synthetic owner. Existing published drain fixtures still include a turn; they do not alone establish the real unowned schema, which was inspected directly.
- Unknown completion cues fail schema validation. Final evaluation does not turn unknown outcomes into success; the remaining problem is dispatch after the resulting invalid-session event.
- Relay framing bounds and process-group cleanup tests pass. No camera/perception activation is needed for this evaluator scope.

## Integration/dependencies

The only evaluator manifest change enables rustix's already-locked `process` feature. Canonical Cargo.lock remained byte-identical: SHA256 `84aee5e8ba3627d2316c0f712c1e4eeebbb5e8d9f80f44abfc5d0f236d294f48`. The complete evaluator crate and its all-target Clippy compile against canonical sibling crates with --locked --offline. No Agent 2 root Cargo/lock changes, perception crate, image dependencies or shared actor changes are required. Do not import Agent 2's whole lockfile or remove canonical real-device-audio-pilot.md.

The patch is exactly the 14 changed evaluator crate files plus docs/voice-eval.md. Its reverse apply-check passed against this source-only copy. It is a review/owner-fix base, not an approval to merge the two defects. Root's newer provider work is outside the patch.

## Checks and limits

- Rust/Cargo 1.96.0, host x86_64 macOS, two build jobs, fresh private target.
- Ordinary package invocation: 99 passing executions and 9 local Unix-socket EPERM failures. Cargo stopped at physical_loopback. This initial invocation is retained, not relabeled a source failure or a full pass.
- Socket-enabled focused rerun: **11 physical_loopback + 7 provider_failed + 7 relay_hardening = 25 passed, 0 failed**, 223.24 seconds. It uses only fake runtime/socket/recording players.
- Across these runs: **121 distinct published tests and the 1 original audit regression passed** (two earlier physical passes are duplicates, not extra tests).
- **Four additional audit regressions fail intentionally**, proving the two defects above.
- `cargo +stable clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings`: pass, including private audit tests.
- `cargo +stable fmt -p lamp-voice-eval -- --check`: pass.

No full workspace, full remaining evaluator-test execution, Linux/ARM or physical qualification is claimed. No canonical/external-agent source, commits, devices, cloud, camera, microphone, speaker or model inference were touched. All owned test jobs have ended. Exact commands, exits, log hashes and source/patch identity are in review-receipt.json and per-gate receipts.
