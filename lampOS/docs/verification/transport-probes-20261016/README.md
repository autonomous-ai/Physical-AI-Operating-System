# Explicit Voice transport success probes

Prepared 2026-10-16 05:55:29 +0700 from shell `date`, on base `7ae0eb647914e286817c29bff2abc2934fb19546`. This is preparation for #823, with no original scenario migrated and no speedup claimed. **Local affected gate: RED, 1,946 passed / five failed / 13 existing ignores. Native ARM64 is pending; this change remains a draft.**

The four qualified source paths and their SHA-256 are in [provenance.json](provenance.json). All raw evidence was copied without alteration; [SHA256SUMS](SHA256SUMS) covers this folder except itself. No measuring script or binary artifact is included. Dates and exact command arguments are in [local/root-gates.json](local/root-gates.json).

## Observation contract

The `cfg(test)`-only `Channel::install_probe` attaches once before any local send/receive attempt, including failed encoding, unsuccessful send, or empty/failed receive. Connection alone is permitted before installation. Fixture owners must install the required probes before releasing participants; installation does not prove peer inactivity. Ordinary constructors create no shared probe, and non-test builds contain no probe module, field or hooks.

Only successful actual socket send, followed by the original sequence commit, increments send evidence. Receive evidence follows actual datagram receipt, decoding, every original boot/order/age/grace/stall acceptance check and the existing receive commit. An observer failure cannot convert transport success into failure. Counts, last sequence, the prefix proven from sequence 1, and first accepted gap remain separate observations.

Snapshots make one bounded nonblocking attempt, with no I/O, clock sample, wait or pump. `Busy` supplies no receipt. Checked publication/counter overflow and invalid state latch a permanent observation failure. The retained `Arc` identity prevents allocation reuse from aliasing an old channel. A snapshot is historical transport acceptance, not proof of payload handling, worker polls, Actor/service progress, task return, child exit or reaping.

## Source and identity preservation

[Root independent review](local/root-independent-review.md) removes exactly the seven additive `cfg(test)` insertion forms (one occurs twice) and reconstructs the original `transport.rs` byte-for-byte. All original clock samples, send/receive commit locations and return paths remain. Both original transport tests and their nine assertion macro sites are exact; 164 other existing Voice files are unchanged. The documentation insertion reverses to the original document exactly. Ordinary test construction has additional unattached-slot bookkeeping; no generated-code identity or measured zero overhead is claimed.

[Compiled-list proof](local/root-identity-proof.json) retains all **1,950 original Voice/Memory/Eval identities and ignored flags**, adding exactly **14** probe cases for a total of **1,964**. [Before](local/root-baseline-list.log) and [after](local/root-list.log) lists are unedited. The clean baseline list was reused from the same base only after checking 637 conservative dependency inputs and unchanged manifests/features; [the reuse proof](local/baseline-reuse-proof.json) records this boundary. It contains none of the separate Moment changes.

All 14 added cases pass once in the focused execution and once in the full affected execution. They cover actual IPC success/refusal, accepted gaps, original grace/stall behavior, identity lifetime and install timing, plus clearly labeled private-state fault/overflow checks. The held-publication case uses an owned scoped thread and causal entered/release channels; it is not an IPC-completion claim. Queue saturation stops at the first real refusal within bounded attempts and a separate host guard. No flaky retry is added.

## Complete local qualification

Executed on the shared Intel Mac, with two Cargo build jobs and incremental compilation disabled. These are local debug results; they do not demonstrate Linux ARM64, a lamp, isolated latency, or the complete original-123 timing target.

| Stage | Exact command | Result |
| --- | --- | --- |
| Format | `cargo fmt --all --check` | Pass |
| Strict lint | `cargo clippy --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval --all-targets -- -D warnings` | Pass |
| Compiled list | `cargo nextest list --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval --message-format json` | All 1,950 original identities/ignored flags retained, exactly 14 added |
| Focused tests | `cargo test --locked --offline -p lamp-voice --lib transport:: -- --test-threads=1` | **36 pass / 0 fail / 1 existing ignore** |
| Complete affected tests | `cargo test --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval --no-fail-fast -- --test-threads=1` | **1,946 pass / 5 fail / 13 existing ignores**, 64 Cargo summaries |
| Workspace build | `cargo build --locked --offline --workspace` | Pass, completed after the failed test stage |

The complete test stage ran from `2026-10-16 05:19:56 +0700` to `2026-10-16 05:40:25 +0700`; all gate stages ended at `2026-10-16 05:41:42 +0700`. These wall timestamps describe qualification boundaries, not a performance comparison. The source hashes stayed unchanged throughout. [Raw complete log](local/root-tests.log), [focused log](local/root-focused-tests.log), and [terminal result](local/root-result.json) are retained. Author handoff and source-review documents under `local/` describe their earlier source-only stage; the terminal gate metadata above supersedes their then-pending execution status.

## Five preserved failures

All names below have prefix `coordinator::motor::tests::`.

| Identity | Actual result | Existing tracking |
| --- | --- | --- |
| `explicit_current_segment_reaches_inert_counts_without_completing_voice` | `tests.rs:1135`, `motor.is_ready()` false after the offer | Named in #739's Motor family; historical owned-child timeout differs from this exact symptom |
| `fixture_preflight_rejects_missing_wrong_and_failed_markers_and_reaps` | `tests.rs:581`, PID file missing; exact reap assertion unreached | Exact name and missing-PID symptom in #739 |
| `frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted` | `tests.rs:453`, 3-second preflight did not exit | Name in #739; shared preflight timeout also recorded there |
| `frozen_fixture_preflight_is_not_motor_startup` | Same `tests.rs:453` shared preflight timeout | Exact identity newly reported; identical shared boundary is already in #739 |
| `idle_ready_never_inspects_moves_or_claims_listening` | Same `tests.rs:453` shared preflight timeout | Name in #739; shared preflight timeout also recorded there |

[Read-only triage](motor-triage/README.md) retains exact issue links, fetched history, original failure output, unchanged source snapshots and the assertions not reached. The ready-state failure does not identify the failed adapter/supervisor condition. The missing-PID case does not retain its original preflight error variant or active loop iteration. Their precise causes remain unproved. The three timeout failures occur before Motor startup. The unchanged helper owns and waits for its preflight child before returning the original timeout; this is not evidence of successful Motor behavior.

These paths use Body's separate channels and `SystemClock`, not the changed Voice transport probe. The independent audit compares 221 tracked Voice/Body/IPC files and finds only Voice `transport.rs` changed; every install call is in a new probe test. This supports the #739 family classification but is not proof against indirect binary-layout or host-scheduling effects. No baseline rerun was made.

#906 names three different gated-cleanup tests, all of which pass here. Do not attribute these five failures to #906's precise symptoms. The joining rule permits the known timed-Mac exception only after this preparation's own ARM64 run is green and all five outcomes are disclosed. No native run, merge readiness, scenario migration or timing improvement is claimed in this package.
