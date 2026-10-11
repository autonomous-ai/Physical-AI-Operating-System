# Audio device seam

Scope: `crates/audio/src/{device,alsa_device,testing,capture,pcm}.rs` and the
capture and speaker loops in `crates/live/src/audio_workers.rs`. Both
boundaries are behind the trait. **The two worker loops themselves still have
no test.**

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

The speaker side has the same shape:

```rust
pub enum PlaybackOpenError { Fault(DeviceFault), Unsupported(PcmFormat) }

pub trait PlaybackDevice: Sized {
    fn open(name: &str, clock_mode: bool) -> Result<(Self, PcmFormat), PlaybackOpenError>;
    fn avail_delay(&mut self) -> Result<(i64, i64), DeviceFault>;   // free space, queued delay
    fn start(&mut self) -> Result<(), DeviceFault>;
    fn write(&mut self, samples: &[i16]) -> Result<usize, DeviceFault>;   // one nonblocking write
    fn drain(&mut self) -> Result<(), DeviceFault>;
    fn prepare(&mut self) -> Result<(), DeviceFault>;
    fn discard(&mut self) -> Result<(), DeviceFault>;
    fn state(&self) -> DeviceState;
}
```

- `SpeakerOn<D: PlaybackDevice>` is the boundary: the guard, the ownership
  ledger, the clock mode, the finish deadline and every discard decision are
  untouched. `Speaker` is still the ALSA one (`SpeakerOn<AlsaPlayback>`).
- `AlsaPlayback` makes the same calls in the same order as before, including
  the negotiation and the start threshold that keeps a clock-mode device from
  starting on a write.
- The boundary checks the reported format against its own bounds as well.
- `speaker_on::<D>` is the speaker worker over any device; `speaker` calls it
  with `AlsaPlayback`.

## The scripted device

`lamp_audio::testing::{CaptureControl, ScriptedCapture}`. A test registers a
name, opens a `MicrophoneOn<ScriptedCapture>` on it through the same `open`
the boundary always uses, and then decides what the device holds and when it
fails: push samples, fail the next call of a given name with a given `errno`,
change the reported stream state, stall the next read, change the format it
configures to. It counts opens, starts, discards and reads.

`lamp_audio::testing::{PlaybackControl, ScriptedPlayback}` is the speaker
counterpart. It keeps everything the device **accepted**, in order, which is
what an echo reference may be built from, and how many frames are still
queued. Nothing plays by itself: the test says how many frames the device has
played out. It can limit how much one write takes, fail any call, change the
reported state and report an out-of-bounds format.

Neither has a clock or a rate. A room or a real-time model belongs on top of
them: accepted speaker PCM fed back into the scripted microphone at a chosen
gain and delay is exactly the scene the [echo evidence](echo-evidence.md)
was tested on, and Agent 2's simulation harness is the intended user. Nothing
in this module is used by the runtime.

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

The speaker boundary over the scripted device,
`crates/audio/tests/speaker_scripted.rs`, 6 tests, also on the host:

| Test | What it pins |
|---|---|
| `only_what_the_device_accepted_is_written_and_a_full_device_is_not_a_fault` | A write the device takes 100 of 240 frames of reports 100 and continues from there; a full queue accepts nothing and is not an error; room returns when the device plays a block out |
| `cancellation_discards_what_the_device_still_holds` | A cancelled turn's queued audio is dropped at the device, not left to play |
| `an_underrun_or_a_vanished_speaker_is_a_fault_and_nothing_is_replayed` | A failing write or queue observation discards the queue and latches the boundary closed; the next block never reaches the device |
| `a_discard_that_fails_latches_the_boundary_closed` | A failed discard is not treated as silence |
| `the_clock_is_primed_with_accepted_zeros_and_started_only_when_told` | Speech cannot start the clock; two accepted zero blocks prime it; the device is not running until `start_clock`; speech then joins the same cursor; privacy closes it |
| `a_device_outside_the_accepted_format_is_refused_at_open` | Four out-of-bounds formats, a buffer too small for the clock, a missing device and a failing open |

The existing Linux tests against ALSA's `null` plugin
(`crates/audio/tests/capture_linux.rs`, `pcm_linux.rs` and the unit tests in
`pcm.rs`) still run, through `AlsaCapture` and `AlsaPlayback`.

Results are in [Agent 1's status](../coordination/agent-1.md).

## Not done

- **The capture and speaker loops themselves have no test yet.**
  `capture_on` and `speaker_on` exist and compile for the board; nothing
  drives them. `audio_workers` is compiled only on Linux
  (`crates/live/src/lib.rs`, the integrator's file), so a test of it runs in a
  Linux container or needs that gate opened. This is the next piece: both
  loops over the two scripted devices with the reference link between them,
  which is also the first place accepted speaker PCM, the canceller and the
  echo evidence run together off the board.
- **Nothing here is evidence about a device.** A scripted stall is a sleep; a
  scripted xrun is a number. They prove what the boundary does when the
  device says so, not when or how a real codec says it.
- Restart remains a new worker incarnation, which is the integrator's. The
  boundary's own `reset` then `start` is tested above and gives a new epoch.
