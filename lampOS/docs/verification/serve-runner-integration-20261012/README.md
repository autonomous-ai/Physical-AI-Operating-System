# Continuous conversation runner integration

The thirty-path Agent 2 package through `c51da7102` is integrated on
`3c92d67d5`. It includes the reviewed `c7dc0978` transport-exit correction,
prepared playback, current readiness cues and a finite eight-scene serve plan.
Every staged source path matches the frozen combined source used below.
No device, acoustic or human acceptance is established by this integration.

## Source and focused gates

Frozen source: 463 files; manifest SHA-256
`160777d723aa6cd5e2703c5317d16e0d8e427b1162917f481992924003ff09f8`.
Package patch SHA-256:
`43c17a6e9a6ce60e84d13ef6d69a85e7769bb1a7d6ea756b7dc861573df3ec80`.

Host formatting and strict `lamp-voice-eval` / `lamp-observer` all-target Clippy
passed. The serial package-test invocation exceeded its 500-second administrative
budget. Its original failed receipt is retained. The already-running serve-session
test binary subsequently finished 11/11; the full retained log contains 296
passes, zero failures and zero ignored tests. A separate bounded completion ran
only the six missing `target_audio` tests and both packages' doctest stages,
then built the release runner. All supplemental commands exited zero. This is
302 observed passing tests across the retained binary results and supplemental
run, not a claim that the original timed-out Cargo invocation succeeded or that
a full workspace gate ran.

Supplemental receipt SHA-256:
`4275edc1214df1916f47cf4c2aefeae40c3b48f40f12c834b3bdf857ef5a992f`.
The 4,557,040-byte macOS runner SHA-256 is
`2976a03ea2b2669ae5b81fd96cf4661d8d44e0d571c66b29a4c51448ab0c187f`.
All source identities were checked before and after. The original timeout log
SHA-256 is `9b8d45dae3d92b2737444ed7a7c33b93299355c40da7b435bf0ffe484ae9d1d0`.

The same frozen source built both release ARM64 binaries and passed strict
Gemini/live/motor/evaluator Clippy under emulation. Readiness-focused tests passed:
startup cues 6/6, service cues 3/3. Kept-start 11/11, continuation/discard 12/12
and all 169 motor tests also passed. The optional live motor selector failed one
of sixteen tests: a child crash could leave no `motor_degraded` record although
voice ownership and next dispatch remained correct. That is a real separate
failure-propagation defect; the original gate remains failed. Its narrow fix and
the finite head caller have a subsequent combined gate. These earlier binaries
are not described as a wholly passing release.

## Evidence boundary

The runner waits for accepted current availability and provider-link cues. It
prepares playback before granting it, preserves the original exclusive 100 ms
start deadline, and reports runtime exit separately from outer transport exit.
A clean runtime with a failed SSH/relay process cannot certify an attempt.
The eight scenes are cached; no speech regeneration or V1 benchmark is implied.

The upcoming physical attempt uses one provider session with duplex admission,
capture-clock reference, 200 ms playout start and kept-start continuation. The
UART remains inaccessible and no head or motor option is selected. Camera and
motor qualification remain separate. Recordings remain local and acoustic scores
stay unmeasured until their independent evidence is reviewed.
