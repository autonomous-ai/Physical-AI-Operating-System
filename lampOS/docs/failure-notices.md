# Spoken failure notices

Scope: `crates/audio/src/notice.rs` and the `lamp-audio notices` check. This
is a loader and a provisioning contract. It plays nothing, and no notice is
packaged in this source.

## The rule

When the voice service cannot be reached there is nobody to ask for a voice.
What Lamp may say about an outage therefore has to be on the device already,
recorded once in the approved voice. The runtime reads those recordings at
startup and never makes one: no synthesis request, no download, no fallback
voice, no resampling. A notice that is missing or fails a check is reported
unavailable and Lamp stays silent.

This replaces V1's behavior only in part. V1 kept a cache keyed by a hash of
provider, voice, model, speed and text, stayed silent when the rate-limit
notice was not in it, and for any other cached phrase synthesized on a miss.
The silence on a miss is kept. The synthesis on a miss is not.

## Directory

One directory, outside the source tree, given to the runtime by configuration:

```
manifest.json
provider-unavailable.en-US.wav
turn-lost.en-US.wav
```

`manifest.json`:

```json
{
  "schema_version": 1,
  "notices": [
    {
      "kind": "provider_unavailable",
      "language": "en-US",
      "transcript": "I can't reach my voice service right now.",
      "voice": "Aoede",
      "voice_revision": "gemini-live-2026-09",
      "approval": "production",
      "file": "provider-unavailable.en-US.wav",
      "wav_sha256": "64 lowercase hexadecimal characters",
      "frames": 52800
    }
  ]
}
```

The transcript, voice and revision above are placeholders showing the shape.
No wording or voice has been approved.

| Field | Rule |
|---|---|
| `kind` | `provider_unavailable`, `turn_lost` or `service_refused` |
| `language` | A BCP 47 tag, matched exactly. `en` does not satisfy `en-US` |
| `transcript` | The words in the recording, 1 to 240 bytes, no control characters. Kept for records and review; never sent anywhere to be spoken |
| `voice`, `voice_revision` | What produced the recording, 1 to 64 bytes each |
| `approval` | `production` or `test` |
| `file` | A bare `.wav` name in this directory: letters, digits, `.`, `-`, `_`; not hidden; at most 64 bytes |
| `wav_sha256` | SHA-256 of the whole file |
| `frames` | Mono frames at 24 kHz, 7,200 to 240,000: 0.3 s to 10 s |

Unknown fields, an unknown kind or approval, a second entry for the same kind
and language, more than 8 entries, or a manifest over 16 KiB make the whole
manifest invalid.

Each recording must be a regular file opened without following a link, a
complete RIFF/WAVE container of mono 24 kHz 16-bit integer PCM with exactly
the stated number of frames, matching its hash, unchanged in size while read,
and not all silence. Worst case held in memory: 8 notices of 10 s, 3.84 MB.

## What the runtime gets

```rust
let notices = Notices::load(directory, Approval::Production);   // never fails
match notices.get(NoticeKind::ProviderUnavailable, "en-US") {
    Ok(notice) => notice.samples(),   // &[i16], 24 kHz mono, as recorded
    Err(missing) => /* stay silent; report `missing` */,
}
notices.capability();   // what can and cannot be said, for a status report
```

`Notices::none()` is the same with no directory configured.

| `Missing` | Meaning |
|---|---|
| `no_directory` | None configured, or it does not exist |
| `no_manifest` | The directory has no `manifest.json` |
| `invalid_manifest` | Not a bounded regular file of the expected shape |
| `not_provisioned` | No entry of this kind in this language |
| `not_approved` | Provisioned only as a test asset |
| `invalid_entry` | Name, transcript, voice, hash or length malformed |
| `asset_missing` | The named file is not there |
| `asset_unreadable` | Not a bounded regular file, reached through a link, or changed while read |
| `asset_hash_mismatch` | The file is not the one the manifest describes |
| `asset_format` | Not complete mono 24 kHz PCM16 of the stated length, or silent |

One bad entry refuses only itself. A bad manifest refuses everything. Reasons
are fixed categories; no path or file content is carried.

## Test assets

A recording marked `test` is refused by `Approval::Production` with
`not_approved` and loads only when the caller asks for `Approval::Test`. The
tests in this repository write tones and mark them `production` only to
exercise the checks; a tone in a temporary directory is not a voice asset and
nothing here ships one.

## Checking a directory

```sh
lamp-audio notices /path/to/notices              # as a production runtime would
lamp-audio notices /path/to/notices --accept-test
```

Prints the capability as JSON: what loaded, with transcript, voice, hash and
duration, and what was refused and why. Run it on the device after
provisioning and keep the output with the provisioning record.

## For the integrator

This crate cannot speak. The coordinator owns that:

- Samples go through the existing guarded speaker path, in 240-sample blocks,
  under a current output owner. Nothing in this module grants one.
- A notice is not an answer. It must not count as a completed reply, improve
  an answer-latency figure or be attributed to the lost request.
- Privacy, stop and a new admission still cut it, like any speech.
- Choosing whether to say anything is a product decision. Staying silent is
  valid. Saying the same notice at every reconnect is not: V1 debounced its
  rate-limit notice for 300 s.
- Publish `capability()` in a status report so a missing notice is known
  before the outage in which it would have been needed.

## Verified, and not

Host, `crates/audio/tests/notice.rs`, 7 tests: an exact load; every
manifest-level failure; test assets refused in production; one faulty asset
refusing only itself; format, length and silence checks at both bounds; a
linked asset; malformed names and text.

Not done: no approved wording, no approved voice, no recorded asset, no
provisioning on any device, and nothing that plays a notice.
