# Strict continuation service probe — 2026-10-11

This is one real service functional test from the Mac through Lamp's configured
`campaign-api.autonomous.ai` route and `gemini-3.8-live`, using cached synthetic
speech. It is not a Lamp, microphone, speaker, acoustic latency or human-speech
measurement. No office recording was sent. The service probe uses its documented
generic short-answer instruction, not the production chat instruction.

## Result

The strict `Require` policy reached `BarrierTimeout`. The first input was accepted
in full (69,337 samples). A continuation started 300 ms after its local End was
queued, while no reply audio had been observed. Only 27,680 of the continuation's
58,671 samples were accepted before disconnection; its End was never queued.
Neither request delivered audio. The connected observation lasted 7.331 seconds;
the outer command exited 1. No timeout killed the test process.

This fails the service prerequisite for enabling speculative early endpointing.
It also identifies a local timer-placement problem: the two-second settlement
bound expires while the successor is still speaking, before its End. Separating
bounded input upload from waiting to commit is a candidate correction, not proof
that the service will eventually settle the previous request. Strict stale-output
quarantine and an absolute bound must remain.

The public API provides no End-written acknowledgement or raw dropped-event
ledger. Therefore this result does not prove exact wire ordering, that no terminal
was sent by the server, or whether a later answer would preserve the whole thought.
The requested eight-second aftermath window was not completed because the session
ended. Semantic correctness remains unscored. These limits are explicit in
[result.json](result.json).

## Inputs and reproduction

The existing macOS Daniel 185-wpm fixtures were used unchanged, PCM16 mono 16 kHz:

- `hesitation-a.wav`: “Lamp, I've had a difficult morning, and I think the part
  that bothered me was”; 69,337 samples, SHA-256
  `a6dc8bc85b0b74434d9dd0e137eb2a5c72c277c95ca59514a8e3bb39d9520af7`.
- `hesitation-b.wav`: “that nobody listened. I just want to talk about it for a
  minute.”; 58,671 samples, SHA-256
  `103b410386d0943fdce7279cad21b9a6e4ef7d1e39c70c6463ca570a4eca1de6`.

With an approved local provider configuration and these local cached fixtures:

```sh
cargo run --locked --release -p lamp-gemini --example service_probe --   /path/to/provider.json /path/to/hesitation-a.wav   continue-before-output /path/to/hesitation-b.wav 300
```

The probe sends 10 ms blocks with a host-paced wait. On this run, 4.334 seconds of
first-input PCM took 5.027 seconds to queue. Do not use its wall time to estimate
natural speaking pace, network delay or device latency. The fixture hashes were
checked by the execution wrapper; the probe itself does not verify SHA-256.

## Provenance and validation

The sanitized result retains the request/session identities and all exposed
observations; only absolute local fixture paths were changed to basenames and an
execution receipt was added. Original result SHA-256:
`5d0653abfb723a8061e2dd9c7673cc3ce1471f5ba57245a8e1e3ee0d8b5e497e`.
Tested executable SHA-256:
`b9740b13d778f3e6defa672bdb2db54ab407224903765809f8029102a3051d30`.
Frozen source manifest SHA-256:
`721f0d4f3c148e9c1bb9ea828bbaec3d37c9c9582098fe979740eb2bed4671c8`.
The four-file probe patch is based on `34cddbafbe440375a1ef516c81f7a319d8cad85b`
and changes the example and documentation only, not the Gemini library.

Host validation on that frozen source passed formatting, strict all-target
`lamp-gemini` Clippy, all 10 service-probe tests using the production transport
against an in-memory service, and the release example build. This is no
Linux/ARM64 qualification. Credentials, PCM and office recordings are not included.
See [Gemini reliability](../../gemini-session-reliability.md) for the probe contract.
