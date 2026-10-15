# Privacy switch (#755): host evidence, 2026-10-15

Taken by Third on its development Mac, not on a lamp: Intel Core i7-10700K @
3.80 GHz, macOS 26.6.2 (Darwin 25.6.0), x86_64, Rust/Cargo 1.96.0. Nothing here
is a lamp result. No lamp, GPIO line, microphone or ring was touched. The raw
logs are kept outside the repository; this page keeps the counts and the names
of every failing test.

## The gate

From `lampOS/`, with the build folder outside the repository (see "Where the
build sits"):

```sh
cargo fmt --all --check
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo build --offline --workspace
CARGO_TARGET_DIR=<scratch>/new-target cargo test --offline --workspace --no-fail-fast -- --test-threads=1
```

| Base | clippy | Tests passed | Failed | Ignored | Harnesses |
|---|---|---|---|---|---|
| `5798f73d3` | clean | 4,467 | 0 | 34 | 293 |
| `a28048886` | clean | 4,506 | 3 | 34 | 294 |

On `a28048886` the three failures are `lamp-health`'s
`a_part_that_is_expected_and_silent_or_has_gone_quiet_ends_show_with_3` (this
Mac's disk is 91% full, so health's first line is a disk warning) and
`wrong_arguments_print_the_usage_and_end_with_2_for_serve_and_64_for_show`,
both of which fail on `a28048886` itself 3 times of 3, and `lamp-memory`'s
`a_relative_path_or_an_unknown_option_gets_the_usage_line_and_exit_code_two`,
which then passed 5 times of 5 alone. `cargo fmt --all --check` showed one diff,
in First's `crates/voice/src/agent_tasks.rs`, outside this change.

Native ARM64, run 38695339133 attempt 2 on `504244b64`: 4,558 passed, 0
failed, 36 ignored, 294 harnesses. Attempt 1 failed only in `lamp-policy`'s
`the_program_judges_what_the_voice_hears_and_prints_nothing_of_it`
(`policy/tests/program.rs:363`), as did Fourth's run 38697844896.

## Turned off, nothing changes

The switch is `LAMP_PRIVACY_CONTROLS=on` in Controls, Voice and Ears. Unset,
`off` and every other value are off. The new `controls/state.privacy` field is
left out of the JSON when empty, and the type, version and declaration of
every existing message are unchanged.

Controls built at `5798f73d3` and at this change ran the same replays with the
variable unset, and each output file, stdout, stderr and exit status was
compared byte for byte:

| Case | Output file | stderr | Exit |
|---|---|---|---|
| written hand fixture, `--enable` | same | same | same |
| written hand fixture, shadow | same | same | same |
| #711 strip fixture, `--touch`, `--enable` | same | same | same |
| #711 strip fixture, `--touch`, shadow | same | same | same |
| #711 working profile, `--touch`, `--enable` | same | one run lost its summary line | same |
| `replay --pins` without `--touch` | none | usage has one more line | same |
| privacy pins file, variable unset | none | usage has one more line | same |
| missing replay arguments | none | usage has one more line | same |

The missing summary line was not seen again in 20 more runs of that case on
each binary. It is the log relay's (see "Found, not mine").

A review of the off path, made by a subagent reading the code, found two
changes, both fixed before the gate: a pins file with a `privacy` section was
accepted with the variable unset (it is refused again, naming the variable),
and Ears stopped on any value but `on` or `off`, so `LAMP_PRIVACY_CONTROLS=`
in a unit would have stopped the room microphone (it now reads the variable
as Controls and Voice do, and names an odd value in one journal line).

## Turned on, both microphones close

A second review traced every path for a microphone left open on a closed,
missing, stale or faulty switch and found none. It suggested one more guard,
added: Ears refuses any further permit from a Voice worker session that has
reported the switch closed.

`controls_stopping_with_the_slide_open_closes_both_microphones_within_50_ms`
in `crates/voice/tests/privacy_pipeline.rs` is First's case: Controls stops
while the slide is open. Voice's microphone gate and Ears' handle are both
open 1 µs before the last acquisition is 50 ms old and both closed at 50 ms,
stay closed for the next 240 ms with Voice's permission still allowed, and
reopen only with a new worker after fresh HIGH has been steady for 60 ms. With
Ears made to keep a permit 10 ms longer, the test fails. Stale but repeated
`controls/state` is
`repeated_publication_preserves_original_acquisition_and_cannot_extend_capture`;
a fault, a restarted publisher or a changed reader is
`missing_controls_or_a_publisher_restart_cannot_renew_an_open_capture`; the
real worker process losing Controls is
`losing_controls_ends_the_real_worker_and_expires_its_lease_despite_current_allowed_authority`
in `privacy_worker.rs` (Linux only, so the ARM64 run).

## Latency on this Mac

`cargo test --offline --release -p lamp-controls --test privacy
synthetic_line_to_bus_latency_with_stalled_strip -- --ignored --nocapture`,
twice. From the injected GPIO read to the retained `controls/state` read back,
parsed and checked from the Bus, with the strip stalled, 100 repeats each:

| Run | First | p50 | p95 | Max |
|---|---|---|---|---|
| 1 | 588 µs | 293 µs | 550 µs | 583 µs |
| 2 | 577 µs | 304 µs | 590 µs | 603 µs |

The design target is 50 ms. This leaves out the real GPIO read, the 5 ms poll
interval, Voice and Ears reacting, ALSA closing the handles, the ring and any
sound or light.

## Failures kept

- A first full run stopped at `lamp-body`'s
  `abrupt_parent_channel_loss_expires_authority_in_real_child` ("motor startup
  timed out"). Alone it then failed once and passed twice; on `5798f73d3` it
  passed three times. It passed in the gate.
- A run with the build in the repository's own `target/`: 4,435 passed, 19
  failed, 34 ignored, 1 h 14 min. 17 were the Voice motor fixtures at
  `motor/tests.rs:441` ("owned child timed out", #739); the others were
  `lamp-health`'s and `lamp-memory`'s usage tests and `lamp-turn-end-wire`'s
  `startup_failures_are_final`.
- A focused Controls run failed `invalid_arguments_fail_before_any_bus_or_record_is_created`:
  an invalid argument exited with nothing on stderr. It passed in the gate.

### Where the build sits

`lamp-turn-end-wire`'s `startup_failures_are_final` failed every time with the
build under the repository (3 of 3) and passed every time on `5798f73d3` in a
scratch folder (3 of 3). The crate depends on nothing this change touches, and
both builds have the same Cargo hash. The test binary, copied byte for byte
(SHA-256 `3405987b…e0ff`) to the scratch folder, passed there and failed in
place, and the whole binary took 37–41 s in place against 8 s copied. Something
on this Mac makes processes started from under the repository much slower, so
timed process tests fail there. This is a fact about this Mac only.

### Found, not mine

`lamp_ipc::log_relay` (`895177f31`) sometimes loses a process's last stderr
lines when it exits quickly: `lamp-health`'s usage test fails on `5798f73d3`
and `a28048886` too, and `lamp-memory`'s, Controls' `program.rs:194` and
`touch_program.rs`, `lamp-policy`'s `program.rs:363` and one replay summary
line were lost the same way at random. It is reported in Third's log.

## Not shown

No lamp. Not the electrical level, the wiring or polarity of chip 1 line 9; not
that a microphone stops capturing sound or loses power; not the ring's colour or
timing; not the time from the switch to silence.
