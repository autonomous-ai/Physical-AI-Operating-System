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
input.try_image(request, frame, grant, captured_at, jpeg) -> Result<ImageOffer, ImageRefusal>
input.revoke_images(through_grant) -> ImageRevocation
input.image_ledger() -> ImageLedger
```

| Bound | Value |
|---|---|
| Image size (`MAX_IMAGE_BYTES`) | 64 KiB of JPEG, about 87 KiB on the wire |
| A look's picture in a written turn (`MAX_LOOK_IMAGE_BYTES`, #941) | 184 KiB of JPEG, about 245 KiB on the wire; the turn with its note must be one message of at most `MAX_WIRE_BYTES` (256 KiB), or it is refused as `MessageTooLarge` |
| Images per request (`MAX_IMAGES_PER_REQUEST`) | 2 |
| Age from capture, at offer and again at sending (`MAX_IMAGE_AGE`) | 2 s |
| Waiting images | 1 slot per connection; a newer image of the same request replaces an unsent one and does not count twice |
| One image write | 400 ms, then the connection fails as `WriteTimeout` |
| Commands held while an image is written | 64, beyond the 32 of the command queue |

`frame` is the caller's own token. It comes back unchanged in the receipt, so
the caller can tie what reached the socket to the camera frame it checked.

`grant` is the caller's number for the camera permission the frame was
captured under, increasing with each new permission. Checking the grant
before offering is not enough: an accepted image may still be waiting, or
half written, when the camera closes. `revoke_images(through)` is the final
boundary. It is synchronous, takes the same lock an offer takes, and:

| `ImageRevocation` | What was found and done |
|---|---|
| `Nothing` | No image of grant `through` or earlier was waiting or being written |
| `DroppedUnsent` | One was waiting. It is gone. The request and its audio are untouched |
| `AbandonedInFlight` | One was being written. The write is abandoned and the connection with it, because part of the image may already be on the wire |

After it, every offer under grant `through` or earlier is refused as
`Revoked`, on this connection and, through the supervisor, on every later
one. A new permission is a new, higher grant.

A revocation and the completion of the write are decided under that one
lock, and nowhere else. Whichever takes it first wins: either the image is no
longer in flight and the revocation reports `Nothing`, or the abandonment is
latched and the write does not count, even if its last byte went out in the
same instant. `AbandonedInFlight` therefore always means the connection is
abandoned and nothing held behind the image is sent. The image itself may be
complete on the wire by then; that cannot be undone, and it is counted as
revoked, not sent. Before `bb882e024` a write that completed in the same poll
as the revocation won, and the held audio and `End` followed it. The
integrator's audit found that with a transport that revokes from inside the
final flush; its regression is in the suite.

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
| `Revoked` | Offered under a camera grant that has been revoked |

A refusal changes nothing: not the connection, not the request, not its
audio.

## After acceptance

- **Dropped unsent** if, before it can be written, the request's input ends,
  the request is cancelled or superseded, or the image passes 2 s. Counted in
  `ImageLedger::dropped`. An image never outlives its request and is never
  attached to the next one.
- **Cancellation of the request** is the same synchronous retirement as for
  audio. It does not interrupt a write already in progress; that write is
  bounded at 400 ms.
- **Shutdown stops the write at once.** Nothing held behind the image is
  sent: no audio, no `End`. The connection is abandoned with the image
  incomplete. Before `ee5387da7` the held audio and `activityEnd` were sent
  after `shutdown()` once the blocked write drained. The integrator's review
  found that and supplied the regression; it was a defect in a path with no
  caller, not a leak on a device.
- **Revoking the camera grant stops the write at once** in the same way. The
  connection ends as `InputCancelled`, which the supervisor reconnects from,
  and the request that was speaking is reported lost once at `InputOpen`. A
  camera that closes mid-write therefore costs that question. The window is
  the write itself: 2.5 ms in the in-memory test, bounded at 400 ms.
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

`ImageLedger` has five counters (`accepted`, `sent`, `replaced`, `dropped`,
`revoked`) and the last `ImageReceipt`: request, frame token, bytes, how long the image
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
| `shutdown_during_a_blocked_image_write_sends_nothing_that_was_held_behind_it` | Audio and `End` held behind a blocked image, then `shutdown()`: the peer receives neither, and no complete image |
| `a_revoked_camera_grant_withdraws_a_waiting_image_and_leaves_the_request_alone` | `DroppedUnsent`; later offers under that grant or an older one are `Revoked`; audio and the request unaffected; a newer grant is accepted |
| `revoking_the_grant_of_an_image_being_written_abandons_its_connection` | `AbandonedInFlight`; the connection ends as `InputCancelled`; nothing held is sent; an unrelated grant changes nothing |
| `a_revoked_camera_grant_is_remembered_across_a_reconnect_and_an_outage` | Revoked with and without a connection; the replacement refuses the same grants |
| `a_revocation_that_lands_as_the_image_write_completes_still_abandons_the_connection` | A revocation issued from inside the last flush of the image write: `AbandonedInFlight`, the connection ends as `InputCancelled`, nothing but the image itself reaches the peer, and the ledger shows it revoked, not sent |

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
`image_waited_ms`, event timings from the moment the `End` was queued locally
(it reaches the wire after the image if the image is still being written),
and `answer`, the model's output transcript. The picture is opened once, must
be a regular file not reached through a link, and at most 64 KiB plus one
byte is read from it before any connection is made. The open itself is given
one second, so a name swapped for a pipe cannot hang the probe. This is the only probe that prints the model's words,
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
3. Call `try_image` with the request, its own token for the frame, the grant
   number, and the capture time translated into this process's `Instant`
   domain, exactly as it already does for audio.
4. Call `revoke_images` the moment a camera grant ends, before anything else
   that tick, and treat `AbandonedInFlight` as the loss of that connection.
5. Read `image_ledger()` to trace what was sent, replaced, dropped or
   revoked.

None of that is implemented. Until it is, `try_image` has no caller.
