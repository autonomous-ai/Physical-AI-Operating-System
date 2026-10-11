# Visual context for the Gemini session

Scope: `crates/gemini` only. This is the transport's side of giving Gemini a
picture: `InputSender::try_image`, `Supervisor::try_image` and what the
connection does with an accepted image. Nothing in the provider worker,
coordinator or camera path calls it yet, and no live service has seen it.

## The constraint

One WebSocket carries the person's audio and any image, in order. An image
therefore delays the audio behind it by its own transmission time, and audio
older than one second is refused by this transport. So the image is kept
small, kept out of the audio queue, and never allowed to wait in front of
audio that is already queued.

## Contract

```rust
input.try_image(request, frame, captured_at, jpeg) -> Result<ImageOffer, ImageRefusal>
input.image_ledger() -> ImageLedger
```

| Bound | Value |
|---|---|
| Image size (`MAX_IMAGE_BYTES`) | 64 KiB of JPEG, about 87 KiB on the wire |
| Images per request (`MAX_IMAGES_PER_REQUEST`) | 2 |
| Age from capture, at offer and again at sending (`MAX_IMAGE_AGE`) | 2 s |
| Waiting images | 1 slot per connection; a newer image of the same request replaces an unsent one and does not count twice |
| One image write | 400 ms, then the connection fails as `WriteTimeout` |
| Commands held while an image is written | 64, beyond the 32 of the command queue |

`frame` is the caller's own token. It comes back unchanged in the receipt, so
the caller can tie what reached the socket to the camera frame, grant and
privacy generation it checked. The transport knows none of those.

An image is accepted only for the request that is speaking now: the last one
started, not cancelled, its input not ended. It is sent as
`realtimeInput.video` with `image/jpeg`, inside that request's own activity,
after any audio already queued.

| `ImageRefusal` | When |
|---|---|
| `NotConnected` | No ready connection |
| `NotCurrent` | Unknown, superseded or cancelled request, or its input has ended |
| `TooLarge` | Over 64 KiB |
| `NotJpeg` | Does not start and end with the JPEG markers. Nothing else is parsed |
| `TooOld` | 2 s or more since capture, or a capture time in the future |
| `LimitReached` | The request already carries two |

A refusal changes nothing: not the connection, not the request, not its
audio.

## After acceptance

- **Dropped unsent** if, before it can be written, the request's input ends,
  the request is cancelled or superseded, or the image passes 2 s. Counted in
  `ImageLedger::dropped`. An image never outlives its request and is never
  attached to the next one.
- **Cancellation** is the same synchronous retirement as for audio. It cannot
  interrupt a write already in progress; that write is bounded at 400 ms.
- **During the write** the connection reads nothing from the service. Output
  already received keeps being delivered from the event queue. Audio and
  `End` offered meanwhile are held in order and sent when the write
  completes, so a slow write cannot overflow the command queue.
- **A reconnect** takes the image with the connection. Nothing is carried to
  the replacement and nothing is sent again. On the replacement the old
  request is `NotCurrent`.
- **If the link cannot take the image in 400 ms** the connection fails with
  `WriteTimeout`, which the supervisor reconnects from. The request that was
  speaking is then reported lost at `InputOpen`, like any connection lost
  mid-question. This is the cost of a picture on a bad link; it is why the
  coordinator, not the transport, decides when a picture is worth sending.

`ImageLedger` has four counters (`accepted`, `sent`, `replaced`, `dropped`)
and the last `ImageReceipt`: request, frame token, bytes, how long the image
waited from offer to completed write, and how long the write itself took.
There is no new `Event` and no new `Error` variant: the provider worker
matches both exhaustively and is not mine to change.

## Tested

Host, scripted in-memory peer, `crates/gemini/src/session/tests/images.rs`
and one supervisor test in `recovery/tests/pressure.rs`:

| Test | What it pins |
|---|---|
| `image_is_sent_once_inside_its_request_after_audio_already_queued` | A 64 KiB image offered between two audio blocks leaves after both, once, before `activityEnd`; receipt carries the frame token |
| `a_refused_image_changes_nothing_else` | Every refusal; the limit; audio and the connection untouched |
| `a_newer_image_replaces_one_not_yet_sent_and_counts_once` | Newest wins; the replaced one does not use the allowance |
| `an_image_whose_request_ended_or_was_cancelled_first_is_dropped_unsent` | Never sent after `End` or a local cancellation |
| `audio_offered_during_a_slow_image_write_is_held_and_sent_in_order` | 60 audio blocks offered while a 64 KiB image is stuck behind a peer with 4 KiB of room: none refused, all delivered in order after the image |
| `an_image_the_link_cannot_take_fails_the_connection_within_its_write_bound` | `WriteTimeout` at 400 ms on a virtual clock |
| `image_is_refused_during_an_outage_and_never_carried_to_the_replacement` | `NotConnected` during the outage, `NotCurrent` after it, nothing re-sent |

## Not established

- **Whether the service uses the picture.** With automatic activity detection
  off, it is an assumption that a `realtimeInput.video` frame sent between
  `activityStart` and `activityEnd` becomes part of that turn's context.
- **What it costs on a real link.** The 400 ms bound and the 64 KiB size were
  chosen from arithmetic, not measurement: 87 KiB needs about 1.8 Mbit/s of
  uplink to pass in 400 ms.
- **Whether two images help** more than one, and what the model does with a
  second.

The probe for the first two:

```sh
service_probe /absolute/private/provider.json QUESTION.wav image PICTURE.jpg
```

It speaks the question, offers the picture 200 ms in, and prints one JSON
line: whether the image was sent or dropped, `image_written_in_ms`,
`image_waited_ms`, event timings from `activityEnd`, and `answer`, the model's
output transcript. This is the only probe that prints the model's words,
because nothing else shows whether the picture was used. Choose a picture and
a question fit to appear in a log, for example a single coloured object and
"What colour is the thing I am holding?".

## For the integrator

Agreed with Agent 2 (`A1-006`): the provider receives **reduced** frames, at
most 64 KiB and 640 pixels wide, from the perception worker's frame link,
with the original camera frame identity, grant and dequeue time. The provider
worker then has to:

1. Take a frame only when the coordinator asks for visual context for the
   current owner.
2. Check the camera grant and the frame's age against its own current state.
3. Call `try_image` with the request, its own token for the frame and the
   capture time translated into this process's `Instant` domain, exactly as it
   already does for audio.
4. Read `image_ledger()` to trace what was sent, replaced or dropped.

None of that is implemented. Until it is, `try_image` has no caller.
