# Final combined pilot build

Code through `a9cae6d17` includes the continuous conversation runner, finite
explicit head caller and stored-motor-failure reporting correction. The source
export was frozen before the commits; its 468-file manifest is
`049a86c27ae4595af7d52821f8b024586f1ec6bcc773c1de699431edc594b10d`.
Documentation added afterward does not change the tested code.

One ARM64 emulation job ran 2026-10-12 01:56:34–01:59:30 UTC, with the source
mounted read-only, two build jobs, and no overlapping container. Exact file sets
and hashes matched before and after. Workspace-crate outputs were rebuilt;
third-party dependencies were reused. Formatting, strict live/motor/evaluator
all-target Clippy, release binaries and the head operator example passed.
Focused release test executions: live motor 28, head qualification 3, camera 7,
options 20, startup cues 6, service cues 3, and motor all-targets 169; zero failed
or ignored. Selectors may overlap. This is not a full workspace or device gate.

The original failing motor crash test now passes. Its separate deterministic
host regression first reproduced the missing stored-failure adoption, then
passed with the correction; all 17 affected host tests and strict Clippy passed.
The original failed ARM gate is retained and not relabelled successful.

Exported voice artifact: 9,506,184 bytes, SHA-256
`b3186e7e40d838898d8575ba63732037e2082cb3463ca30adb24447550e673f4`.
Exported relay: 4,321,984 bytes, SHA-256
`e1dc042a0aa55f769f6c3c5f15f1d9477cffdc310bd04d25b38b0bfc1c13e54f`.
Both are AArch64 ELF and their build/export/host/final-volume hashes agree.
The separately exported motor binary and operator are not deployed for the audio
trial. The motor binary's final target-volume hash differs from its earlier
export; its receipt's broad agreement field requires correction. No physical
motor qualification follows from this build.

Host runner validation and its retained administrative timeout are recorded in
[the runner integration](../serve-runner-integration-20261012/README.md).
The first device stage of this combined build stopped before creating its state
directory or stopping legacy services: `/etc/asound.conf` had a different hash
from the old pin. Both new binaries had already been published into immutable
digest directories, without starting either. Current non-secret routing was
read and reviewed: Jieli `device_micro2` capture and C-Media playback through the
existing soft volume. Microphone and speaker mixer contents match the prior
settings after removing only their final newline; the new staging helper had
mixed trimmed command output with untrimmed expected strings. Those
administrative corrections are separate from Rust behavior. No audio played and
no new conversation acceptance is claimed by that failed preparation.

Current head provisioning remains incomplete: zero raw profiles were observed
on the device, but a nonzero ID5 profile's physical settling time and calibration
have not been established. The finite caller preserves the original 100 ms
motion authority per action. No guessed profile or fixture configuration is
approved for hardware. The first human pilot keeps motor movement disabled.
