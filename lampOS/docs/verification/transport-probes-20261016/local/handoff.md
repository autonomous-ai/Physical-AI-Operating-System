# Explicit Voice transport success probes: source handoff

Prepared 2026-10-16 05:10:55+0700, from shell `date`. Worktree `/private/tmp/lampos-tests-transport-probes`, branch `tests/823-transport-probes`, base `7ae0eb647914e286817c29bff2abc2934fb19546`. Source is held for root review and gates.

## API and observation contract

`Channel::install_probe(&mut self) -> io::Result<transport::probe::Probe>` is available only under `cfg(test)`. Attach once before the first local send/receive attempt. A failed encoding/send, failed receive or empty receive also prevents later installation. Connecting is allowed before installation; the API does not inspect peer queues or prove that a peer has not started. A fixture owner must attach its required probes before releasing startup participants. Ordinary `Channel`, `WorkerChannels` and `ReferenceChannels` constructors create no shared probe.

`Probe` is cloneable and shared across threads. `Probe::snapshot() -> Result<Snapshot, SnapshotError>` makes one bounded nonblocking attempt. `SnapshotError::Busy` means no receipt is available from that attempt; it never means an operation completed. `SnapshotError::Failed(Fault)` is permanently latched. Neither observation path waits, performs I/O, samples a clock, pumps, advances or awaits. Installation allocates only one bounded identity and shared counter state, and there is no trace/queue/registry.

`Snapshot` contains `channel: ChannelId`, `revision: u64`, and `send`/`receive: Progress`. Each progress has `direction`, configured `boot`, `count`, actual `last_sequence: Option<u64>`, `contiguous_through`, and `first_gap: Option<Gap { expected, observed }>`. Every counted boot has actually matched that configuration. Both directions start at count zero with no accepted sequence. Consecutive-prefix proof begins at sequence 1; an accepted gap permanently leaves it at the last complete prefix and retains the first gap. A count or last/highest sequence cannot substitute for that proof. Production gap acceptance is unchanged.

Channel identity uses an owned Arc allocation with pointer equality. Probe handles and every retained Snapshot keep that allocation alive, so dropping the original channel/handles cannot permit address reuse to alias a later channel. Names are only diagnostics; equal names/boots/counters do not mean equal channel identity.

The send hook runs after actual `Endpoint::try_send` success and the original send-sequence assignment. The receive hook runs after decoding, both original clock samples, all existing order/boot/age/grace/stall checks and the original `received` assignment. Actual successful transport status is never converted into an error because observation failed: the live probe fails instead. Failed sends and refused/empty receives have no success observation.

## Publication and failure reasoning

One Channel’s exclusive mutable send/receive operations are the sole publisher. A checked even revision surrounds complete snapshots; publication temporarily makes it odd and writes atomic counter words before the next even revision. All these atomics are sequentially consistent. A reader accepts only an unchanged even revision around its data reads, so overlapping publication gives Busy; revision overflow fails before any counter publication and cannot wrap/ABA. Stable snapshots additionally validate representation, count/prefix/gap consistency and revision equal to twice the total accepted count. Invalid representations latch failure only after the reader establishes a stable revision.

Wrong boot, nonincreasing sequence, counter overflow, publication overflow, invalid state and duplicate/late installation latch the first failure. Subsequent writer calls do not publish additional successful evidence. Historical Snapshots remain historical values: a later failure or later successful operation does not mutate them. A successful read can linearize immediately before a concurrent later failure; it is not a lease on future health or a global completion barrier. A controller must check the live probe and its own announced causal target under an independent host watchdog.

This observes only channel acceptance. It does not observe payload handling, worker polls, downstream credit consumption, Actor/service progress, task return, process exit or reaping. There is no provider/controller/Moment/Gemini feature/dependency change and no scenario migration or speedup claim.

## Source preservation

- `before.json` and the two `before-*` files snapshot the clean base before editing. `verify-source.py` removes exactly the enumerated cfg-test additions and recovers `transport.rs` byte-for-byte; `reversal-additions.json` retains those exact insertions.
- All original receive sample locations, send/receive commit locations, boot/order/age/gap/grace/stall behavior and return/error paths are unchanged. The whole file has the same seven original clock calls; there are no clock calls in the new probe module.
- Both old test bodies, attributes and nine assertion macros are byte-identical. Whole old module SHA-256: `9b5164732d82e500c315e954b540cdf00c47c710aa4d4ae183269b22c28372c6`. Individual body/attribute hashes are in `source-proof.json`.
- All 164 other existing Voice files are unchanged. Only `transport.rs` and the narrow documentation section are tracked changes; only the two new adjacent modules are untracked additions. Manifests, lockfile, workers, clocks, original scenarios and Git index remain unchanged.
- Removing the documentation insertion restores the entire new-base document exactly, including endpoint cleanup text. New declarations, fields, installation and hooks are cfg-test only; ordinary non-test source reverses to the original. The no-probe unit path performs only the unattached-slot bookkeeping in addition to the original behavior. This is a source default-path proof, not generated-code equivalence or measured zero overhead.

## New cases and verification status

There are fourteen new cases and 115 assertion macro sites. Real Unix datagram coverage checks separate send enqueue/accepted receive; encoding and unconnected failure followed by real success without skipped sequence; bounded queue saturation up to the first actual WouldBlock; decode/boot/age/future/order refusal; accepted initial/later gaps; startup grace and the original single sender-stall exception; retained identity after channel/probe drop; late installation after failed/empty/successful I/O; faulted observation while actual delivery still succeeds; and ordinary constructors without probes.

Counter/publication limits, invalid state, wrong-boot/order observer faults and corrupt stable snapshots use explicitly labeled private-state instrumentation. Their assigned states are not evidence of real IPC work. The contention case holds the exact shared begin/finish publication operations on an owned scoped publisher thread using causal entered/release channels, checks Busy while held, and joins after release; it is a publication-state case, not an IPC completion claim. Its receives are bounded and sender disconnection releases the held publisher during unwind. There are no sleeps, flaky retries or detached threads. Saturation is bounded by 4,096 queued attempts and an independent host guard, and stops on the first refusal.

Pinned Rust 1.96 direct rustfmt and `--check` passed for only the three owned Rust files with `skip_children=true`; see `rustfmt-final.json`. No Cargo, build or runtime test was run. Root still owns fresh compilation, strict lint, real-socket execution, compiled identities and affected/default suite qualification. Source review cannot establish runtime success or ARM64 behavior.

The first scratch source-audit regex matched the `for` in `impl Trait for Type`; it was corrected to detect loop statements. The successful byte reversal and scope audit then completed. This was a verifier false positive, not a source or runtime test failure.

Recommended focused commands from the worktree’s `lampOS/`, using the root-allocated target and an environment permitting real local Unix sockets:

```sh
cargo test --locked --offline -p lamp-voice --lib transport::probe::tests:: -- --test-threads=1
cargo test --locked --offline -p lamp-voice --lib transport::tests:: -- --test-threads=1
```

## Held source hashes

| Path under worktree | SHA-256 |
| --- | --- |
| `lampOS/crates/voice/src/transport.rs` | `ea318a0e1be7523411b8c4d8ca878d3c743965bf3a5b69b6b3026b4cb720c29b` |
| `lampOS/crates/voice/src/transport_probe.rs` | `99f86fe4b888c9e61b4ddef1d1ad70c9d1795830667ed013f21d2a086de777e8` |
| `lampOS/crates/voice/src/transport_probe_tests.rs` | `d2a1e5cca930741f727d7c9c4647a84ed532490adbb3663cc8f20edff571fa2b` |
| `lampOS/docs/voice-test-fixtures.md` | `04bf0a07ddbd0e9de139988b65c51aa23a11bb1b542176a8185a473d923a19d4` |
