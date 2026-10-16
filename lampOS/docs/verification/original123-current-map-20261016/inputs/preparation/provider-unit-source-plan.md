# Next provider-unit clock migration

Prepared **2026-10-16 06:28:30 +0700**, from shell `date`. Read-only source audit against frozen `9b28324e2232d62ff905e705d16f53c7836599f9`, compared with `e7d0f673d0c4bde4a770b19783d6084a8d935ec3`. No code, Cargo execution, Git mutation, network operation or device access was performed. The companion `tests-823-provider-unit-clock-next-proof.json` records source hashes, the exact proposed inverse, and existing native observations.

**Migrate the single `provider_worker::tests::resumed_question_survives_a_silent_service_and_the_assumption_is_reported` case next.** It can use the existing paused current-thread Tokio pattern without changing shared fixtures or introducing the early-end controller. The source audit supports a two-line Rust change; execution is still required to qualify it.

## Exact change boundary

1. In `lampOS/crates/voice/src/provider_worker.rs:8606`, change this case's attribute to `#[tokio::test(flavor = "current_thread", start_paused = true)]`.
2. In that same case, qualify only `let resumed_at = Instant::now();` at line 8618 as `let resumed_at = tokio::time::Instant::now();`.
3. Add a short English scope/evidence note to `lampOS/docs/realtime-tests.md` once measurements exist. Describe the two original two-second barrier waits using fixture time, unchanged host acquisition/authority clocks and unchanged real transport progress. Do not change the realtime selection or remove this identity from the full-run inventory.

No import, shared helper, production function, feature, dependency, lockfile or test-name change is necessary. Voice already has dev-only Tokio `test-util` at the pinned `1.53.1`; this proposal does not depend on either PR #868 or PR #924 merging first. Keep both reporting iterations, every original frame and all five direct assertion macros exactly as written. The existing `waited >= 1_900 ms` assertion reads the same clock as the unchanged two-second protocol barrier. Reversing just the attribute and one clock-reader qualification restores the entire original case byte for byte; this inverse was checked in memory without writing Rust.

## Why this case does not need a capture-clock adapter

The chosen test constructs the ordinary `LiveRig::start_with_barrier` with `BarrierPolicy::AssumeAfterQuiet`. That calls `start_all(..., Continuation::Off)` at `provider_worker.rs:5231`; `Continuation::Off` leaves the session configuration unchanged at line 281. It does not select `SuccessorStart::AfterFirstOutput` or retain the next question's opening PCM.

`LiveRig::speak` admits a real owner, sends a real Start and a real 160-sample audio packet stamped by `monotonic_us()`, waits for the worker's actual Started output, and then reads/asserts the actual WebSocket fields from the scripted service (`5486–5544`). The first call sees Start, Audio and End. The resumed call sees Start and Audio before returning. Thus both capture blocks have passed intake, the real Gemini InputSender/Actor and the service's frame reader before the long quiet wait. Only the resumed End is then sent and withheld. The later `provider_audio(8, 240)` is service output; it introduces no delayed capture packet.

Leave all acquisition clocks unchanged: `read_completed_at_us: monotonic_us()` at 5512, worker source-age subtraction and `Instant::now().checked_sub(age)` at 2897–2904, both Gemini age checks, sequence/capture ordering, permissions and authority leases. Nothing is restamped, clamped to fresh, or given a larger age bound. No aged capture is carried across the accelerated quiet interval in this scenario. This is a protocol-timer migration, not evidence that all capture-sensitive provider cases can pause Tokio alone.

The two modes are Legacy and Typed. `LinkClock::sample` is only called for Coordinated reporting (`provider_worker.rs:904–906`), so this case does not introduce the mixed protocol/IPC timestamp problem that the early-end controller must solve. Do not broaden the loop to Coordinated or apply the change globally.

## Actor and service progress remain real

Keep the worker's original 2 ms Tokio interval and Skip behavior (`912–913`), the Actor's original timer processing, every channel send/receive and `FakeService::try_receive` timeout/stream read. Keep the helper's original 2 ms or 5 ms awaits and host 3 s/5 s failure bounds. Its actual `OwnershipAssumed`, actual `activityEnd`, actual audio output and live-worker assertions still decide success.

The pinned Tokio `1.53.1` source says paused current-thread time advances to the next timer only when the runtime has no work; std Instant continues normally (`src/time/clock.rs:94–125`). Its same source warns that a bulk `advance` does not ensure timers have been processed, recommending timer-backed awaits for ordered progress (`220–245`). Therefore retain the current awaits and ordinary scheduler progress. Do not add a bulk two-second jump, fixed yield count, synthetic receipt, suppressed service reader, fabricated Actor completion, or clock reader that advances time. No new blocking thread or no-auto-advance bridge is needed for this single-runtime, in-process scenario.

This source-level argument is narrower than an execution result. If the unchanged assertions fail under the proposed opt-in clock, preserve the first failure and diagnose it. Do not enlarge this PR into an early-end framework or weaken an assertion to force a pass.

## Why the other three cases are not the smallest change

| Original case | Existing full-run case time | Required additional coordination |
|---|---:|---|
| `resumed_question_survives_a_silent_service_and_the_assumption_is_reported` | 4.053 s | Selected. Two original quiet barriers; PCM already received; one elapsed-time origin. |
| `kept_start::with_no_first_answer_the_patience_ends_in_the_strict_timeout_and_no_end` | 4.541 s | Retains 20 × 160-sample blocks sent every 10 ms across 2,500 ms patience, then the strict 2 s barrier. `pump`, `heard` and elapsed assertions currently use std time. The retained samples' source ages must advance consistently. |
| `review_2026_10_14::review_a_name_heard_after_a_requests_sound_ended_does_not_let_out_its_answer` | 5.230 s | Uses the worker's actual monotonic name/authority domain and a deliberately four-second-old bus hearing. |
| `review_2026_10_14::review_sound_a_name_heard_as_a_request_began_lets_out_the_answer_the_rule_would_hold` | 4.424 s | Same bus/acquisition domain, including the original +200 ms hearing and request-open ordering. |

These observations are from completed native full run `38820115320`, exact head `8af53fd459795029f6277679ded49bf7826def73`, not new isolated measurements at `9b`. The candidate source bytes are identical at those relevant current `e7d`/`9b` inputs; a controlled before/after pair still must be taken. The approximate four seconds of real waiting are an opportunity, not a measured saving from an unimplemented change.

Kept-start specifically checks real queued capture ages in `gemini/session.rs:1684–1718`, against `MAX_INPUT_AGE + patience`, with the remaining write deadline reduced by that actual age. Changing its `say`/`heard` clocks while leaving those captures effectively newborn would hide time spent retained. PR #924's explicit fixture CaptureClock and connector are a useful dependency there, but the provider acquisition timestamps, worker age translation and shared Gemini clock must inhabit one coherent domain. Do not supply a frozen clock, overwrite captured timestamps at drain, or merely increase freshness bounds. That is a separate change, not required for the selected case.

The two name tests deliberately wait until `began_us + 4_000_000` using `monotonic_us`, publish `ago_ms` calculated from that same time, and allow a further 200 ms for the worker's real 20 ms polling (`review_2026_10_14.rs:75–97`). The age also limits interference through the binary-wide bus root. Pausing Tokio alone leaves the wall-clock wait in place; substituting virtual time in only the producer would misdate the hearing relative to the worker and other tests. Do not backdate it further, shrink `AGO_US`, bypass the serial turn or fabricate successful bus delivery.

## Existing pattern and API references

PR #868's inspected local head is `1e38b543c9e0f58688ddd089e508b100f4e8d70c` (`tests/823-provider-capacity`). Its capacity scenario uses explicit paused current-thread Tokio time for modeled playback/service timing while IPC and authority remain on the host. The selected case reuses that narrow separation, without copying its shared Speaker/helper edits.

PR #924's inspected local head is `a22664ac9cb4ca7efbe28b501ec9810c52a00c5d` (`tests/823-gemini-capture-fixture`). `CaptureClock::now()` is a prompt, non-driving read of an admitted acquisition-domain Instant; the API does not authorize treating stale capture as fresh. The ordinary pair and connector retain System time. The selected case should continue using those ordinary constructors. These are locally inspected source references supplied for the audit, not fresh network claims about either PR's current status.

## Qualification before calling the migration ready

Freeze a fresh current base and run the untouched exact case first, recording shell-date boundaries, raw output, return code and host load. Apply only the two substitutions, then run the same case once with the same toolchain/profile/serial setting and capture the same evidence. Both Legacy and Typed iterations must run and the full original 1,900 ms assertion must hold in their protocol time. Measure host duration separately; the objective is to remove the two real two-second waits, not to shorten their protocol durations. No timing improvement is established by this plan.

Use compiled before/after lists to require the same Voice identities and ignored flags, with the target once. Compare the full affected Rust bytes by the two-substitution inverse and verify all original assertion bytes. Finish formatting, strict affected Clippy, the complete affected Voice tests and the required workspace build in the repository's gate order. A fresh native normal check does not exercise this realtime identity; the actual full run must show it exactly once. Preserve any first failure, do not retry it as a qualification, and keep the original-123 standalone serial target explicitly open.

## Frozen source identity

All source rows below are byte-identical between `e7d0f673` and `9b28324e`. None was edited by this audit.

| Path under `lampOS` | SHA-256 |
|---|---|
| `crates/voice/src/provider_worker.rs` | `9e2a59f6cfbe5e39a4baf96ccda72ea1ec90e95b1e0c9e9d999bd6916df18219` |
| `crates/voice/src/provider_worker/tests/review_2026_10_14.rs` | `42ff3b7094e8d934e4fa5423e7d0f077eabd2faa9443950951d4ad3811ca3386` |
| `crates/voice/src/provider_link.rs` | `b44639b8cddeacd8432f0870dd69d4ff29028b4855ef711cd64a6002584b499b` |
| `crates/voice/Cargo.toml` | `5da3e2d258a46b797bb25ec48ea538eba53f9b9d38dac1ff06abd8deb6597e72` |
| `crates/gemini/src/session.rs` | `2856a8d1910fb2b1d46930cc7db17e695543ddc9524f55c8f36a9250ffc0438b` |
| `crates/gemini/src/testing.rs` | `0a13daf42afc0024e8ed7473b1f323b4409ef0ef8ccd35bb8be90c9b8fa5cb6f` |
| `crates/gemini/src/config.rs` | `c1f727b99cf61b81d6ad3ecde27d584c84178c390bc676c5ec2e05e76f6145e2` |
| `Cargo.lock` | `40d62fead16a6f138c4d7dbe400a2e4a9855ceece96d19b1e642136431aed4c5` |

The exact original selected case including its attribute hashes to `312511b3b49890cea3533b539b2d7ea322cf3ef57a073fdbaff7d30276d779d2`. The companion proof hashes to `62eddfc08040c4e34a78751bd536d1f477c9155f4241dc139b3e68f484d6beac` and includes complete frozen refs, remaining source hashes, native row locations and the in-memory inverse proof.
