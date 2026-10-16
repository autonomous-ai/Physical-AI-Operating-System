# Microphone boundary clock: source-only handoff

Frozen `2026-10-16 07:25:16 +0700`; handed off `2026-10-16 07:26:42 +0700`. Times came from shell `date`.

Tree `/private/tmp/lampos-tests-capture-clock`, branch `tests/823-capture-clock`, exact merged base `e5aa43747ec1540893b4da4509d14288e7e70806`. The worktree was created at that base as requested; its initial tracked status was clean. There are no commits or pushes.

Only three paths changed:

- `lampOS/crates/audio/src/capture.rs`: defaulted per-instance `Fn() -> MonoTime` clock and explicit `MicrophoneOn::with_clock`. The existing `new` passes the unchanged host `now` function. The Linux `Microphone` alias is unchanged.
- `lampOS/crates/audio/src/capture/clock_tests.rs`: nine new source tests using actual calls to the existing ScriptedCapture device. Fixture updates, including updates after a scripted device operation returns, move time; the injected getter only loads the current atomic value.
- `lampOS/docs/voice-test-fixtures.md`: the explicit constructor's contract and limitations, plus a narrow introductory wording correction to distinguish unit-test peers from a public explicit fixture API.

`complete-delta.diff` includes all three paths, including the new test module. `frozen-files.json` holds their exact hashes. `inverse.json` restores both existing files byte-for-byte and identifies the one new file to remove. Initial raw files remain in `starting-source/`.

## Interface choice and preservation

The clock-interface audit inspected Audio and adjacent named traits before editing. Audio has no clock trait. Gemini's pure clock returns Instant and is feature-gated; Camera uses a fallible mutable sample; Controls includes waiting; Schedule/Mood return compound samples. Body's MonoTime trait belongs to an unrelated domain dependency. The implementation reuses the standard callable convention already present in Ring and prepared fixture players rather than adding a trait, crate dependency, feature, global state or runtime option. Details and source hashes are in `clock-interface-audit.json`.

The default clock is a function pointer. It adds one clock field to MicrophoneOn but no allocation. A supplied closure is owned directly and retains its own Send/Sync/lifetime properties; this preparation neither creates threads nor grants task ownership. Reads must be prompt and pure by API contract. There is no invalidation protocol or clock controller in this change; a later worker controller must own admitted monotonic updates and cancellation. Existing regression checks still latch the capture boundary fault, never fallback to host time.

All nine original sample sites remain: constructor, authority advance, open start, prepare completion, start call, start completion, read start, read completion, and status completion. The seven former raw microsecond samples now call the owned source and extract MonoTime's microseconds. The two MonoTime sites call that same source. The default's unchanged helper samples `lamp_ipc::monotonic_us()` exactly once per site. No sample was inserted, removed or moved across a device call or check.

The 10 ms read budget comparison, buffer bounds/expiry, authority checks before and after calls, error precedence, privacy state, device calls, result branches and Drop cleanup remain unchanged. Driver status timestamps are still distinct and unchanged. There is no pacing or progress observation here.

The entire original capture test module is byte-exact: **13 tests and 86 assertion macros**. All **7224 unowned tracked files** are byte-exact, including audio_workers, the capture-hold Rig and its warm-up loop, every original capture_hold_worker case, transport, Provider prototype-independent source, and Cargo manifests/lockfile. The starting snapshot records 7,226 tracked files and all source test entries. No original assertion or identity was altered.

## New boundary tests

`new-case-identities.json` records nine source-only identities under `lamp-audio::capture::clock_tests`:

1. Ordinary constructor and explicit host source produce host-bracketed timestamps and the same real scripted device lifecycle.
2. Independent per-instance clocks retain acquisition-domain timestamps and do not relabel optional driver timestamps.
3. Invalid aliases fail before clock/device access.
4. Exactly 10,000 microseconds is accepted; 10,001 is rejected without returning late audio or reaching status.
5. Expiry during a completed read precedes the read-budget check and withholds status/audio.
6. Expiry after configure/start/status withholds the corresponding successful completion and closes the device.
7. WouldBlock does not renew the first partial-read deadline; 39,999 versus 40,000 microseconds retains the original boundary and sample order.
8. Clock regression clears partial input and remains faulted even after the supplied clock advances again.
9. Device failure precedes the read-budget error, while expired authority still precedes the device failure.

These are not compiled identities or passing tests yet. No sleeps, retries, worker emulation, task joins or original-case migrations were added.

## Review gate and native limits

**No Cargo command, build, executable test, network/GitHub call, device access, commit or push ran.** Pinned Rust 1.96.0 rustfmt parsed/formatted the two Rust files and its final check passed; `git diff --check` passed. Those source checks do not establish compilation or runtime correctness.

The full reverse-dependency scope was computed from all 38 workspace manifests, including normal, dev, build, target-specific and optional local edges, and each declared path was resolved back to the named package. `reverse-dependency-scope.json` retains that graph and hashes. The conservative all-target scope is:

```
-p lamp-audio -p lamp-memory -p lamp-voice -p lamp-voice-eval
```

Audio → Voice and Evaluator; Voice → Evaluator and Memory's dev dependency. No reverse dependency path to Surround exists in this source graph. After independent review, the required gate is full formatting, strict all-target Clippy for this scope, an actual-base compiled identity/ignore comparison showing exactly these nine additions, the focused new tests plus the full affected suite once, and workspace build. Exact Cargo commands/environment remain root's gate decision; none has started and no target is reserved by this worker.

MicrophoneOn and these scripted tests are Unix code. The ALSA Microphone alias and audio_workers/capture_hold_worker integration are Linux-only and cannot be qualified on macOS. The native gate must compile their unchanged callers, retain all existing test identities, and actually execute the existing capture-hold binary with default/realtime selection: profile pr currently excludes it. This adds no device qualification, original-four migration, worker-completion evidence, thread-ownership guarantee or speedup claim.
