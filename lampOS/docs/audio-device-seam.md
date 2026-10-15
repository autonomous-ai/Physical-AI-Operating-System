# Audio device seam

Scope: `crates/audio/src/{device,alsa_device,testing,capture}.rs` and the
capture loop in `crates/voice/src/audio_workers.rs`. **The capture half is
done. The speaker half is not started.**

## Why

The microphone and speaker boundaries decide whether audio may be read or
written: permission, privacy generations, epochs, sequence numbers, deadlines,
what counts as a discontinuity. Until now they called ALSA directly, so every
one of those decisions could only be exercised on Linux against ALSA's `null`
plugin, which never stalls, never underruns and never disappears. The capture
and speaker loops built on them had no tests at all off the board.

A trait between the boundary and the device changes that. The boundary and
the worker loop are unchanged above it. Below it is ALSA on the board and
anything a test scripts elsewhere.

## Shape

```rust
// crates/audio/src/device.rs, every unix target
pub struct DeviceFault { pub errno: i32, pub call: &'static str }
pub enum DeviceState { Setup, Prepared, Running, XRun, Draining, Suspended, Disconnected, Other }
pub struct CaptureStatus { state, monotonic_us: Option<u64>, available: i64, delayed: i64 }
pub enum ConfigureError { Fault(DeviceFault), Unsupported(CaptureFormat) }

pub trait CaptureDevice: Sized {
    fn open(name: &str) -> Result<Self, DeviceFault>;
    fn configure(&mut self, config: CaptureConfig) -> Result<CaptureFormat, ConfigureError>;
    fn start(&mut self) -> Result<(), DeviceFault>;
    fn read(&mut self, into: &mut [i16]) -> Result<usize, DeviceFault>;   // one nonblocking read
    fn status(&mut self) -> Result<CaptureStatus, DeviceFault>;
    fn discard(&mut self) -> Result<(), DeviceFault>;
}
```

- `MicrophoneOn<D: CaptureDevice>` is the boundary. `Microphone` is still the
  name of the ALSA one (`MicrophoneOn<AlsaCapture>`, Linux only), so no call
  site changed.
- `AlsaCapture` makes exactly the calls the boundary made before, in the same
  order, including the same negotiation and the same acceptance of a format.
  It is the only code that mentions ALSA on the capture side.
- The boundary checks the reported format against its own bounds as well. A
  device cannot talk it into a rate, channel count or buffer it would not
  have accepted from ALSA.
- `CaptureError` carries a `DeviceFault` (call name and `errno`) where it
  carried an `alsa::Error`. The fields are `device_error` and `close_error`.
  Nothing outside the audio crate read the old field.
- `errno` values are named in `device::errno`. Two that the boundary acts on,
  the suspended-stream and bad-state codes, exist only on Linux; elsewhere
  they keep their Linux numbers so a scripted device can still produce them.
- `capture_on::<D>` in `audio_workers.rs` is the capture worker over any
  device. `capture` and `capture_with_noise_suppression` call it with
  `AlsaCapture` and are otherwise as they were.

## The scripted device

`lamp_audio::testing::{CaptureControl, ScriptedCapture}`. A test registers a
name, opens a `MicrophoneOn<ScriptedCapture>` on it through the same `open`
the boundary always uses, and then decides what the device holds and when it
fails: push samples, fail the next call of a given name with a given `errno`,
change the reported stream state, stall the next read, change the format it
configures to. It counts opens, starts, discards and reads.

It has no clock and no rate. A room or a real-time model belongs on top of
it; Agent 2's simulation harness is the intended user. Nothing in it is used
by the runtime.

## Tested

On the macOS host, `crates/audio/src/capture.rs` unit tests, 12 in all. The
last eight are new behavior coverage that did not exist off Linux:

| Test | What it pins |
|---|---|
| `blocks_are_assembled_across_short_reads_in_order_from_sequence_one` | 100 + 60 samples make block 1 exactly; the next 160 make block 2; an empty device is neither an error nor a frame; no status clock is invented |
| `an_xrun_closes_the_device_destroys_the_partial_block_and_restarts_as_a_new_epoch` | The partial block is erased; nothing reopens before `reset`; the restart is epoch 2, sequence 1, and carries nothing the old handle held |
| `a_device_that_vanishes_or_stops_running_is_a_discontinuity_never_silence` | Four stream states and four `errno` values, on read and on status, each mapped to its discontinuity; a whole block read just before is not handed over |
| `a_read_that_blocks_past_its_budget_is_a_discontinuity_not_late_audio` | A read that returns after 15 ms: `ReadBudgetExceeded`, device closed, the late block never delivered |
| `a_block_left_incomplete_for_a_buffer_length_is_dropped_not_finished_late` | Ten samples, then 42 ms: `PartialTimeout` instead of a block assembled across the gap |
| `open_and_configuration_failures_leave_nothing_open` | A missing device; a failing configure, with the opened handle discarded; four out-of-bounds formats refused |
| `privacy_lineage_change_discards_an_incomplete_block` | Denied and allowed again inside one snapshot: closed, 40 samples erased |
| `a_failed_discard_is_reported_and_the_handle_is_still_gone` | The close error is returned and the device is closed regardless |
| `after_a_stall_a_muted_authority_keeps_the_microphone_closed` | After an overrun, a fresh snapshot that mutes the microphone: `prepare` refuses with `NotPermitted` and the device is not opened again |

The existing Linux tests against ALSA's `null` plugin
(`crates/audio/tests/capture_linux.rs`) still run through `AlsaCapture`.

Results are in [Agent 1's status](../coordination/agent-1.md).

## After a stall (#784)

A capture worker held up for a few hundred milliseconds (on lamp-4ace, the
kernel re-tuning its SD/MMC hosts) comes back to a lapsed authority or an
overrun buffer: `AuthorityExpired`, `XRun`, `PartialTimeout` or
`ReadBudgetExceeded` closes the device as before. Once the microphone has
started, the worker (`audio_workers.rs`) no longer ends on these: it says
`CaptureGap` with the closed epoch, keeps the microphone closed, and opens it
again only through `reset` and `prepare` under the coordinator's next fresh
snapshot, which checks the privacy switch and the mute before the device is
touched (the test above). It then starts it without waiting for the speaker
clock, which is already running, and reports `CaptureStarted` with the new
epoch. A changed privacy generation in the gap, any other discontinuity, or
no reopen within 1.5 s still ends the worker. The coordinator drops the
closed epoch's frames still in flight, and an utterance the gap cut into is
abandoned as a hand Stop abandons it; what is still said of it after the gap
is dropped until its speaker pauses, so it is not a new question.

## Not done

- **The speaker half.** `pcm.rs` still calls ALSA directly and is Linux
  only. The trait for it was published in `A1-007`. Until it exists there is
  no scripted speaker, no accepted-PCM echo path through a scripted device,
  and no off-board test of the speaker loop.
- **The capture loop itself has no test yet.** `capture_on` exists and
  compiles for the board; nothing drives it. `audio_workers` is compiled only
  on Linux (`crates/voice/src/lib.rs`, the integrator's file), so a test of it
  runs in a Linux container or needs that gate opened.
- **Nothing here is evidence about a device.** A scripted stall is a sleep; a
  scripted xrun is a number. They prove what the boundary does when the
  device says so, not when or how a real codec says it.
- Restart remains a new worker incarnation, which is the integrator's. The
  boundary's own `reset` then `start` is tested above and gives a new epoch.
