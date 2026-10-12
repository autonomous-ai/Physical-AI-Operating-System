# Camera conversation and shutdown revalidation

The native candidate completed two owned replies in one live session on Lamp-4ace,
with two fresh local camera-frame offers, 955 matched ring receipts, no extra
admission and no reported playback gap. The prompts asked what Lamp could see
and then asked twelve divided by three. Provider transcripts were:

> I see a desk with an all-in-one computer, a secondary monitor, a specialized keyboard, a mouse, and a framed picture.

> Twelve divided by three is four.

These are provider transcripts, not an independently listened transcript or a
verified scene description. Image submission was queued locally; no claim is made
that a particular image was consumed by the model. The 180-second local room
recording has 8,640,000 frames and no dropped frames. Acoustic timing is unscored.

The original private test controller exited 2 and kept its result as
`incomplete_or_failed`: its shutdown verifier incorrectly required a `worker`
field on the camera's `Stopped` message. That message has no such field. The
actual channel binds the worker identity, ordering and freshness; revocation
receipts and the subsequent stopped receipt belong to that verified channel.

Independent revalidation replayed all 2,243 metadata events and 955 ring pairs,
checked stop order and the original deadline, and exercised the corrected
verifier with five controls covering twenty negative variants. It passed.
The original failed result remains unchanged; the correction and revalidation
have separate hashes in [results.json](results.json). Raw IPC envelopes were
not archived, so channel binding also relies on the pinned production source.

Observed camera revocation to driver cleanup was 36.453 ms, with 212.319 ms
left before its original deadline. The full Stop reached inactive after
273.159 ms. All eight observed child identities were gone, hardware owners
were absent, and the stationary muted rollback and actual mixer values were
verified. Motor motion was disabled.

Local endpoint to first ALSA write was 1.653628 s and 1.584504 s. This excludes
the 1.2 s silence wait and is not acoustic latency or a matched benchmark.
No recordings or image payloads are committed.
