# Gemini capture-clock preparation: source handoff

Date: 2026-10-16 03:35:22 +0700, from `date`. Worktree
`/private/tmp/lampos-tests-gemini-capture-clock/lampOS`; parent-provided base
`48b3737d5`. All 19 files from the earlier design snapshot matched this base
before edits. No owner work was erased.

The five source/document paths below are the complete edit scope. Existing
`lib.rs` constants, all existing test files, and the other 30 existing Gemini
Rust files are byte-identical. Recovery and other owners' implementation files
were hashed only for preservation, not edited or behavior-audited. No Voice,
Supervisor, capture-hold, native-head, Cargo.lock, workflow or device changes.

| Edited/new path, relative to lampOS | SHA-256 |
|---|---|
| `crates/gemini/Cargo.toml` | `5b94eadddf2bdc8677d8e584a6cc3ffbdf38f79eba775a9dd141268b1c9a0816` |
| `crates/gemini/src/session.rs` | `27f0d1b329d5407e2b85941a60fd747d12eed3fc7aa07accbf0ceda373ae6073` |
| `crates/gemini/src/testing.rs` | `ac34ac1c456b9e87557bec5aa9c6c4f1a5bc86fcbd09d9518bc13c98859081d3` |
| `crates/gemini/src/session/capture_clock_tests.rs` | `5c2bbcae2d621c982213fb49d23a2a841d21f089e692e97aa465d4d6d2aeef05` |
| `docs/gemini-session-reliability.md` | `753906cd40b84c767b04769e136b065b0ac17a6c8e970f3181f116ffcd50275e` |

## Contract

- One `CaptureClockSource` in Shared reaches the actual InputSender and Actor.
  The private System branch samples std::Instant once at each old site; the
  explicit Fixture branch only delegates to its supplied pure clock.
- `testing::CaptureClock`, `pair_with_capture_clock(config, session, capacity,
  Arc<dyn CaptureClock>)`, and `connector_with_capture_clock(config, capacity,
  Arc<dyn CaptureClock>)` are available under `cfg(any(test, feature =
  "fixture-clock"))`. The feature is default-off. Ordinary non-test library
  builds have only System; every old constructor explicitly chooses System even
  when fixture support is compiled. The old constructors retain their signatures.
- Raw pair setup, connector construction, and the setup/spawn body are each
  shared once, with no copied Actor/serving logic. The same Arc follows each
  attempt through a custom FakeConnector; original Dial/setup ordering remains.
- The three helper calls and three direct image/held sites preserve all six
  semantic samples. InputState::accept still samples before sequence validation;
  the Audio branch still calls the helper again after accept. Strict 2500 ms
  audio / 2 s image / MAX_INPUT_AGE + patience held-audio limits are unchanged.
- All 23 Tokio now expressions and both original 10 ms Skip intervals are
  unchanged. There are no new Actor progress hooks, input mutations, retries,
  timeout slack, or migrations of old scenarios. Capture reads do not pump time.

## New test identities

These 12 unit tests are under ordinary `cfg(test)` so the existing native
workspace command includes them without changing the workflow or enabling a
feature. Feature-on library/dependent compilation remains a separate gate.

- `session::capture_clock_tests::custom_setup_and_sender_clones_share_one_pure_capture_clock`
- `session::capture_clock_tests::every_existing_factory_keeps_system_capture_time_with_fixture_support`
- `session::capture_clock_tests::audio_offer_keeps_the_exact_full_bound_and_refusal_order`
- `session::capture_clock_tests::queued_audio_is_checked_again_by_the_actor_at_the_exact_bound`
- `session::capture_clock_tests::sequence_and_capture_order_fail_after_one_actor_read_not_two`
- `session::capture_clock_tests::image_offer_and_actual_send_keep_the_full_bound_and_refusal_order`
- `session::capture_clock_tests::pending_image_expires_on_an_actual_actor_turn_and_leaves_audio_live`
- `session::capture_clock_tests::held_audio_is_written_one_nanosecond_inside_its_full_extended_bound`
- `session::capture_clock_tests::held_audio_at_its_full_extended_bound_fails_before_any_pcm_write`
- `session::capture_clock_tests::capture_updates_do_not_move_tokio_or_prevent_its_read_deadline`
- `session::capture_clock_tests::connector_attempts_share_the_explicit_clock_and_keep_setup_metadata`
- `session::capture_clock_tests::connector_keeps_the_requested_duplex_capacity_and_original_write_timeout`

The tests exercise actual setup acknowledgements, InputSender, Actor and framed
in-memory WebSocket traffic. Accepted audio read counts supplement actual PCM
receipts; counts are not completion acknowledgements. The held-audio fixture
uses an actual FIFO stale-tool-response discard after its Audio command. The
image fixture observes two successive actual service events with an intervening
loop top, then checks the image ledger and the next real audio frame. Terminal
tests await actual state, event EOF and the Actor JoinHandle before inspecting
remaining peer input. The small-buffer test intentionally allows only 64 bytes
and expects the original WriteTimeout for the stalled real frame.

The fixture's capture clock is advanced only by an explicit setter, never by
reading it. Exact accepted upper-bound-minus-one-nanosecond cases use paused
Tokio and an ample duplex buffer so a ready write can finish without consuming
its original remaining deadline. Default factory coverage necessarily exercises
the unchanged host capture clock; no host timing measurement is claimed.

## Verification completed and pending

`rustfmt --edition 2024 --config skip_children=true --check` passed for the three
owned Rust files. Exact argv/cwd/date/exit and raw output are in
`rustfmt-check.json` / `rustfmt-check.log`. Formatting never traversed old test
modules.

`reverse-source.py` applies an explicit list of exact single-occurrence
transformations (no whitespace normalization or generated inverse diff). It
recovers the original session.rs, testing.rs, Cargo.toml and all old document
text byte for byte; `reversal.json` records the result and `reversed/` the exact
bytes. `source-audit.json` records all hashes and the 30 unchanged existing Rust
files. The previously inventoried eleven session/integration test files retain
all 174 source test functions and 789 assert-macro sites. These are static
source counts, not compiled full-crate results.

**No Cargo build, clippy, test, Git, network or device operation ran in this
implementation task.** Root owns compile/execution gates and the independent
review. No green, latency, native ARM64, real-service or original-123 completion
claim is made. Required remaining checks include default and feature-on Gemini
compilation/tests, old compiled identity/ignored-list preservation, normal
Voice/Surround dependency compilation with feature unification, strict clippy
and the appropriate workspace gates. No retry is proposed for a test failure.

Epoch identity and monotonic admission belong to the later checked Moment adapter;
this trait does not manufacture that proof. A provider completed-poll barrier and
coordinated Tokio advancement remain later work. Root invalidation must retain
the last admitted capture reading until controller cancellation, never select
host time as a fallback.
