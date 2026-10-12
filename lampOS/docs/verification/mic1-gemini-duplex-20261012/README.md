# MIC1 Gemini duplex diagnostic — 12 October 2026

The finite run completed collection with runtime/transport exit **0/0**, valid
100-second room capture, certified audio diagnostics, and verified rollback.
Its functional result remains limited: **sunset was admitted and answered;
coffee was fully played but was never admitted or answered.** There was one
admission and one completed provider reply. This does not pass an interruption
or natural-duplex qualification.

The provider's output transcript was:

> At sunset, sunlight travels through more of the atmosphere, which scatters away the blue light and lets the longer red and orange wavelengths pass through to your eyes.

This is provider metadata, not a listening or ASR result. The input transcript
was session scoped; it was not used as fresh admission authority.

## Exact treatment and observed behavior

The immutable `14fca820…8941e` runtime used MIC1/C-Media, a finite 70-second
Gemini run, `--turn-taking duplex`, `--provider-continuation kept-start`,
`--reference-clock capture`, `--reference-recovery off`, noise suppression off,
Sonora AEC3, a 200 ms startup buffer, ring ceiling 12, and audio diagnostics.
Camera context and motors were off. No mixer writes occurred; final raw mixer
readbacks matched the saved baseline.

The same cached sunset and coffee clips were physically replayed once through
the iMac speaker. Coffee followed the observed original owner's first ALSA
write and completion of the first player, without the fixed-pair experiment's
five-second warm-up. The recorded host first-write-receipt-to-grant interval was
310.948 ms. The prepared player's 100 ms deadline bounds its first source copy;
it is neither a new Lamp authority grant nor a measured acoustic boundary.

Two provisional duplex yields resumed the same reply, with no new admission or
duplex commit. Each resulting accepted-zero interval contains **11,760 frames
at 24 kHz: 490 ms**, or 980 ms total. The trace labels these as actual zero PCM
accepted while the reply was unfinished. Acoustic gap duration was not measured.
The reply contains 216,240 accepted speech frames and ended as
`audio_written_with_playback_gaps_unscored`.

## Preservation and independent checks

The reused receipt verifier checked the archive against its manifest, diagnostic
completion/ACKs and byte hashes, contiguous record offsets, both full source
player deliveries, room WAV/ledger continuity, and cleanup/rollback receipts.
It verified the two zero intervals against retained render PCM without running
AEC, ASR, or listening. The room WAV contains 4,800,000 mono float32 frames at
48 kHz, 9,375 callbacks, and no reported capture fault or clipping. Its SHA-256 is
`e17f7ed428e7dd978f86a62777ce8b4085fa6dfedd0cc3aa96759775e29d25ec`.

The runtime was reaped without a forced kill. Rollback records same-boot
stationary legacy restoration, both software mutes set, no remaining trial
hardware owners, empty temporary drop-ins, and unchanged final mixers. Its
receipt SHA-256 is
`fd25f208b3d30586c71949218f04fc264d86468cca9d1f38e5bfae08be87fd0b`.

All 119 original/provenance files were copied without overwrite or hardlinks to
`.cache/physical-audio/20261012/mic1-gemini-o7lfc33c/`, relative to `lampOS`.
Every destination hash matches the unchanged source. The preservation manifest
SHA-256 is `1063fdfab66517fc6a7f897d78ba278a06e55abd737f4e1abcd1be9080e48946`.
[results.json](results.json) retains exact source/build/controller pins, the
historical command, individual events, media hashes, and restoration evidence.

## A1's separate fixed-pair analysis

A1's original 15-file analysis of the **earlier fixed-source MIC1/MIC2 pair** is
preserved under `_provenance/a1-fixed-pair-analysis/`. It was not independently
rerun here. A1 reports that MIC1's loud Lamp signal appears two capture blocks
before its corresponding render reaches the canceller, and reports only
5.2 dB / 1.6 dB Lamp-only reduction in the two selected reply copies. These are
attributed analysis findings, not a measurement from this Gemini run or a new
calibrated acoustic timing claim. Root assigned A1 the timing correction.

The current failure does not establish that MIC1 is incapable. No gain/AGC
cause, microphone default promotion, human listening, ASR, acoustic score, or
accepted human-interruption performance is claimed. Host, Linux, CoreAudio and
WAV clocks remain separate; existing restoration receipts do not inspect the
current device state.
