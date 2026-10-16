# Scenario Moments on e5aa: failed local gate, unready

Packaged from `date`: **2026-10-16 07:50:16 +0700**. Current base **`e5aa43747ec1540893b4da4509d14288e7e70806`**; the three Rust files remain byte-exact to reviewed PR #925 head **`dee1cdd455072a92491bc800fd3993a124ee411a`**. The existing document prefix and all **101 historical evidence files** are preserved.

The fresh four-package serial gate remains **2,410 passed / 3 failed / 13 existing ignored**, exit **101**, completed **2026-10-16 07:42:47 +0700** after the subsequent workspace build passed. Formatting, strict all-target Clippy, compiled listing and workspace build passed. The actual clean-base and changed compiled lists retain all **2,401** existing identities/ignore flags and add exactly **25** tests for **2,426** total. The independent root result audit accounts for all identities exactly once across **83 binary paths**, with zero filtered or measured tests. All **974 frozen inputs** remained exact. All **25 new Moment tests and seven original clock tests passed**.

This preparation adds immutable origin-bound Moment snapshots, explicitly admitted thread-local observers and inclusive `Moment::between` projection. Reads neither pump peers nor advance time. It migrates zero original realtime scenarios and provides no standalone original-123 timing result.

The three failures, with exact raw blocks and unchanged source in [triage](triage/README.md), are:

| Exact suffix under `coordinator::motor::tests` | Observed boundary |
| --- | --- |
| `late_started_callback_cannot_restore_an_offer_after_owner_replacement` | Shared owned-child wait at `tests.rs:601`; progress timed out, ready=false, reaped=true; child output through `ready_send`. The specific wait call is unproved. |
| `pending_segment_privacy_revoke_wins_before_any_position_write` | `tests.rs:1056`, cleanup unconfirmed / inhibit acknowledgement timeout; `child_finish=Ok("ControlLost")`. The following zero-write assertion did not run. |
| `same_owner_input_end_cancels_queued_gesture_before_inert_bus_write` | `tests.rs:1193`, required `child-position-writes` file absent. |

The first and third are exact identities already in #906; the first now fails at the shared wait rather than the historical later file read. The middle identity is newly reported relative to that snapshot, and owner acceptance is not established. All use original host/SystemClock motor fixtures, without the new Moment API. Unchanged source does not prove cause; reaped=true does not prove clean terminal receipts. No retry, longer deadline or missing-file acceptance was introduced.

Exact commands and date/exit records are in `gate/gates.json`, with complete unedited logs. From `lampOS/`, pinned Rust 1.96.0, jobs 2, incremental 0, target `/private/tmp/tests-823-slow-progress-target`:

```sh
cargo fmt --all --check
cargo clippy --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-surround --all-targets -- -D warnings
cargo nextest list --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-surround --message-format json
cargo test --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-surround --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

The earlier **1,960 / 2 / 13** failed local gate remains unchanged in [`../fresh/`](../fresh/). The original normal ARM64 run **38819671092** is copied separately into `historical-native-38819671092/`: head `dee1cdd455072a92491bc800fd3993a124ee411a`, tested merge `67c3d4b00551eb8c981923d69272aa255cf31eb7`, **5,000 unique passes** including all 25 new tests. That normal run observed none of the original 123 and is historical evidence for the old head, not qualification of this integration. The still earlier **1,741 / 4 / 8** failed gate remains linked from the preserved parent README.

**Unready: the new head still needs native qualification and resolution of the current local failure disposition.** No known-failure exception or motor-owner acceptance is inferred. No device, speedup or combined serial bound is claimed. Source-only reviews retain their original in-progress wording; the terminal records here establish the actual failed local result. `provenance.json`, `preservation-audit.json` and `SHA256SUMS` identify every raw copy and preserved input. Packaging ran no tests and changed no source.
