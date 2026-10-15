# Independent Gemini capture-clock review

Recorded 2026-10-16 03:38:33+0700 from `date`. Reviewed the author's stable five-file source in `/private/tmp/lampos-tests-gemini-capture-clock`, base `48b3737d5e83efa00278172cb602d3525d368a43`, after the explicit stable-source/hash handshake. No repository edits, Cargo/build/test execution, Git mutation, network or devices.

**No concrete source blocker found.** The seam preserves the actual InputSender/Actor path, original age checks, default constructors and protocol clock. The review is source evidence; root's default/feature gates and compiled inventory comparison remain necessary.

## Independent preservation evidence

`/private/tmp/tests-823-gemini-capture-clock-independent-evidence/audit_source.py` independently reverses the reviewed additions and source plumbing. Complete reconstructed `session.rs`, `testing.rs`, `Cargo.toml`, and reliability documentation match current HEAD byte-for-byte; the three code/manifest originals also match the saved design snapshots. The proof does not normalize whitespace or apply a generated inverse diff. Reconstructed files, exact diffs and `proof.json` are saved in that directory.

All 19 planning snapshots match current HEAD. All eleven inventoried original test files retain their complete bytes: 174 statically inventoried test functions and 789 assertion macro sites. All 30 other original Gemini Rust files match HEAD, including examples, recovery code/tests and `lib.rs`; these were hashed for preservation, not behavior-reviewed outside this task's scope. Cargo.lock, dependency versions/features, Tokio dependency configuration, workspace/dependent manifests and every out-of-scope path remain unchanged. The only manifest addition is the empty, default-off `fixture-clock` feature table.

The twelve new `#[tokio::test(start_paused = true)]` cases have 80 assertion macro sites. They add no sleeps, yields-as-proof, retries, ignored tests or modifications to old assertions. These are static counts, not a compiled inventory.

## Source and sampling contract

One cloneable `CaptureClockSource` lives in the single Shared initializer (`session.rs:44–62,719–745`). A normal non-test, feature-off library contains only System; its sole source read is `Instant::now()`. Under `cfg(any(test, feature = "fixture-clock"))`, Fixture delegates once to the supplied Arc and never samples a host fallback. The trait and explicit factories use that same cfg; the new unit module uses ordinary `cfg(test)`. This deliberately includes the new tests in normal native unit execution while preserving a default-off public fixture API for ordinary library builds.

The six semantic sample sites retain their exact placement:

| Site | Preserved ordering and bound |
| --- | --- |
| `try_audio:214–220` | Read after ready/retired checks, before sample-size validation/enqueue. |
| `InputState::accept:824–837` | First operation, before open/sequence/capture-order checks and remembered-state updates. |
| Actor Audio `:1805–1806` | A second fresh helper call after successful accept; it does not reuse accept's age. |
| `try_image:341–370` | Read after ready/size/JPEG checks and before current-request/grant/slot decisions. |
| `send_image:1189–1210` | Under the original image lock after taking the frame/currentness calculation; still read before revoked/current/fresh branches. |
| `drain_held:1728–1757` | Per popped block, after decrementing retained samples, before expiry/write-budget/discarded-request decisions. |

The helper still uses checked subtraction and strict `< MAX_INPUT_AGE` (`:2438–2443`); future and exact-upper-bound captures fail. `lib.rs` keeps audio 2,500 ms and images 2 s. Held input keeps `MAX_INPUT_AGE + patience`; normal/held write budgets remain `min(write, bound - age)`. There is no shorter fixture bound, saturating replacement, cached second read or extra validating sample.

All 23 original `Clock::now()` source lines are identical and ordered. Both original 10 ms interval/Skip blocks are exact. Full-file reversal also preserves Tokio setup/read/response/barrier/write/keepalive deadlines, image offered/write timing, IMAGE_WRITE 400 ms, select order and one-server-message-per-turn yield. This seam introduces no Actor poll acknowledgement or coordinated Tokio advancement.

## Factory and receipt review

The setup wrapper (`session.rs:645–792`) selects System explicitly and delegates to one original setup/spawn body. Neither wrapper samples capture time. Ready still follows actual `setupComplete`; channel/Actor initialization and spawning retain their order. Pair factories share one raw-duplex body with unchanged lazy setup future (`testing.rs:151–205`). Default pair, connector and connector-with-capacity explicitly select System, including under feature unification. Custom connectors clone the same source into each attempt while retaining resume/config/capacity and Dial-before-await-setup ordering (`:223–278`). Real connect remains on the System setup wrapper.

The new tests' positive claims use actual setup acknowledgements and framed PCM/JPEG receipts. Read counts supplement those receipts: two sent blocks account for six actual age samples; sequence/capture-order failure stops after the first Actor sample, and queued-age expiry is arranged synchronously before the current-thread Actor can handle the command. The one-hour future fixture domain makes a silent System fallback fail the custom path and proves the three old fake factories remain isolated.

For pending-image expiry (`capture_clock_tests.rs:412–444`), two distinct service messages are sent and actually received as events in sequence. The second cannot be handled without another Actor loop top, which runs `send_image` before the next socket message (`session.rs:1104–1165`). With request/grant unchanged and commands empty, the resulting dropped ledger plus later real PCM receipt proves age rejection while audio remains usable.

For retained audio (`capture_clock_tests.rs:447–524`), the real unmatched ToolResponse command follows Audio in the same FIFO; its actual StaleToolResponse event cannot precede Audio command completion (`session.rs:1807–1819,1931–1933`). Only then does the controller advance capture time. Real server audio releases the held start. Bound-minus-one-nanosecond requires actual retained PCM; the exact full bound requires StaleInput before held PCM write. The success setup uses paused Tokio and ample duplex capacity so the original tiny write budget can succeed without added slack. This is a meaningful boundary case, but its execution remains a required gate, not a result of this review.

Terminal helper `ended:104–123` awaits the actual disconnected state, event EOF and Actor JoinHandle before looking for remaining decoded peer messages. It does not infer Actor completion from state or empty live-socket reads. Its final decoded-message absence is not a raw-byte/partial-frame assertion: the explicit 64-byte stalled-write test may leave partial framing by design and expects the unchanged WriteTimeout. The stale-audio paths separately reject before writing by source ordering. No test claims OS exit.

The remaining cases exercise capture/Tokio independence through an actual ReadTimeout, two actual connector attempts with resumption metadata and a shared source, and original capacity/write-timeout behavior using a real 64-byte duplex buffer. They do not exercise a live remote service.

## Limits and pinned hashes

The explicit clock contract requires prompt pure nondecreasing acquisition-domain reads. Gemini intentionally cannot validate epoch identity or controller invalidation from `std::Instant`; the later checked Moment adapter owns that proof and must hold the last admitted reading until cancellation. Arbitrary callbacks that block, recurse through the image lock, or advance time on reads violate this fixture contract. No new validation/fallback policy was inserted here.

Root still owns default and feature-on compilation/execution, complete old identity/ignored-list equality, strict Clippy, and ordinary Voice/Surround dependent builds under feature unification. Source review establishes neither test success nor a provider completed-poll barrier, latency improvement, ARM64 qualification or original-123 completion.

- `crates/gemini/Cargo.toml`: `5b94eadddf2bdc8677d8e584a6cc3ffbdf38f79eba775a9dd141268b1c9a0816`
- `crates/gemini/src/session.rs`: `27f0d1b329d5407e2b85941a60fd747d12eed3fc7aa07accbf0ceda373ae6073`
- `crates/gemini/src/testing.rs`: `ac34ac1c456b9e87557bec5aa9c6c4f1a5bc86fcbd09d9518bc13c98859081d3`
- `crates/gemini/src/session/capture_clock_tests.rs`: `5c2bbcae2d621c982213fb49d23a2a841d21f089e692e97aa465d4d6d2aeef05`
- `docs/gemini-session-reliability.md`: `753906cd40b84c767b04769e136b065b0ac17a6c8e970f3181f116ffcd50275e`
