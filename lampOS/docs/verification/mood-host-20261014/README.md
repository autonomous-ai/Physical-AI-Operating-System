# Mood authored-day replay, 2026-10-14

These files contain **synthetic, authored metadata**, not a lamp recording.
They exercise the pure state engine over a fixed 24-hour local day. No person,
microphone, camera, bus, actuator, account or device supplied the observations.

| Artifact | Contents |
|---|---|
| [authored-day.jsonl](authored-day.jsonl) | Exact stdout from the existing host replay example: metadata, phases, minute samples and summary. |
| [authored-day.svg](authored-day.svg) | Standalone Matplotlib plot of awake, energy and warmth; sampled sleep intervals are shaded. |
| [replay-stderr.txt](replay-stderr.txt) | Exact stderr from the replay capture; empty, with process exit status 0. |

## Capture provenance

The capture ran at approximately **2026-10-14 12:00:58 UTC / 19:00:58 ICT** on
`macOS-26.6.2-x86_64-i386-64bit-Mach-O`, with Python 3.14.2 and Matplotlib 3.11.0.
The working checkout was based on
`1733307d7672ad302154984952a77772f8ebd6e5`, with the Mood change still uncommitted.

This capture used the already built `target/debug/examples/replay_day`.
No Cargo build was performed for this capture. The executable hash identifies
what ran; the separate working-source hashes do not claim a clean rebuild.
No binary or Python file is retained under `lampOS` for this evidence.

| Captured input or source | SHA-256 |
|---|---|
| Existing replay executable | `29e119259fdba780c4813c3649ee98ba38f9ba2105f75009e85252695d5021ff` |
| `crates/mood/src/replay.rs` | `adc1a1dc660594ddd25577b98c69a6a522892c6bf031a2e4bbe19a136c2d505c` |
| `crates/mood/examples/replay_day.rs` | `0524271630d62415cbe38f74763995389861cf106b5b766f4f4aadd0342bccd0` |
| `crates/mood/src/engine.rs` | `ce45291ea8cca7210e75729392a2061bf992c2c8ecbfef4ab71649ddca27c235` |
| `crates/msgs/src/mood.rs` | `b3cc291cda353627c68767df07d8be37c476ab828bc9d90079e841894e19048d` |
| `authored-day.jsonl` | `77ba44452538e22ccb59e61fbd51703ba8a4d2b8970d3e0db1ba4c023271809a` |
| `authored-day.svg` | `fbcf4d713915f77b7a27509ed7f69af2a3732c5dc5115dacdd0adc8a2107f4af` |

The retained JSONL has 454,183 bytes. Its `scenario` is
`task35-authored-day-v1` and its `provenance` is `synthetic_authored`.
[The generator](../../../crates/mood/src/replay.rs) and the first JSONL record
provide the complete authored point events, company/air intervals, refresh
cadence, lease lengths and simultaneous-event order.

The example advances the synthetic monotonic clock every second. Its origin
is 1,000,000 microseconds; local minute is `(elapsed_s / 60) modulo 1440`.
It uses no wall-clock or timezone lookup. Company arrives every second with
a two-second lease in four half-open intervals, with explicit disappearance
at each interval end. Air arrives every five seconds with a fifteen-second
lease. One synthetic face token is reused; it is not a person's identity.
Thanks and laughter are explicit replay-only events, not live classifications.

## Exact commands

The capture ran from `/private/tmp/lamp-fourth-mood/lampOS`:

```sh
mkdir -p docs/verification/mood-host-20261014
target/debug/examples/replay_day > docs/verification/mood-host-20261014/authored-day.jsonl 2> docs/verification/mood-host-20261014/replay-stderr.txt
```

To build and reproduce the example from the corresponding source:

```sh
cargo run --offline --locked -p lamp-mood --example replay_day
```

Plotting used a scratch Python script outside the repository:

```sh
MPLBACKEND=Agg MPLCONFIGDIR=/private/tmp/mood-matplotlib-20261014 python3 /private/tmp/plot_mood_authored_day_20261014.py docs/verification/mood-host-20261014/authored-day.jsonl docs/verification/mood-host-20261014/authored-day.svg /private/tmp/mood-authored-day-20261014.png
```

The script reads only the retained JSONL, selects `record: sample`, and plots
`elapsed_s / 3600` against the three scalars. It connects samples without
smoothing, uses the same 0–1 vertical domain for each scalar, and shades
contiguous sampled `asleep` intervals. All axes, headings and annotations
state the authored scope. SVG fonts are embedded as paths; there are no
external images, scripts or network resources. The inspection PNG and scratch
script remain outside the repository.

## Records, units and observed curve

| Record kind | Count |
|---|---:|
| Metadata | 1 |
| Phase | 14 |
| Sample | 1,441 |
| Summary | 1 |
| Total JSONL records | 1,457 |

Samples cover elapsed seconds 0 through 86,400 inclusive at 60-second
intervals. `awake`, `energy` and `warmth` are dimensionless authored values
between 0 and 1. CO2 values in the authored input metadata are in ppm; input
times and leases use seconds, and published `stamp.at_us` uses microseconds.
All samples keep familiarity `unknown`; a sleeping sample has awake 0 and
no attention target.

The summary counts **29,364 valid applied inputs**: 28,804 face observations,
541 air observations, eight sudden sounds, two ordinary touches, one kindly
touch, one name, one reminder, two thanks, two laughter, one speech-activity
lease and one active-Voice lease. These include sleep/cooldown no-ops. The
nine `meaningful_event_updates` count only changes reported directly by event
application; they exclude changes from clock advancement and are not bus
publication or latency counts.

- Dawn, company, generic speech activity, active Voice, noise and replay-only
  feedback leave the initial sleep latch closed. Name wakes at 08:00.
- Quiet periods first appear asleep in minute samples at 11:00, 13:00, 15:15
  and 20:30. Ordinary touch wakes at 11:15 and 16:00; the reminder wakes at
  13:30 without selecting attention. These sampled times do not claim exact
  sub-minute automatic transition times.
- Kindly touch at 13:31, thanks at 13:32 and laughter at 13:33 raise warmth
  from 0.60 to a sampled peak of 0.7152891. It returns toward 0.60 without a
  negative warmth penalty.
- Fresh authored CO2 at 1,400 ppm begins at 14:00. Energy is still 0.30 at
  14:10, is 0.26 at 14:15, and reaches 0.225 at 14:30. Clear air at 600 ppm
  starts then; energy recovers to 0.290625 at 14:45.
- Awake bangs at 17:00 and 17:02 briefly raise energy to approximately 0.55.
  The 17:00:30 repeat is inside cooldown and adds no pulse. At 17:01, the
  first pulse has decayed to energy 0.315625.
- Evening company settles awake to about 0.5503 at 19:20. Long quiet then
  leads back to sleep, which incidental night noise does not clear.

## Validation and limits

The capture exited 0 and stderr is zero bytes. The plotting validation checks
record counts, consecutive one-minute samples, phase coverage, event-count
sum/bounds, monotonic timestamps/sequences, finite bounded scalar values,
sleep invariants, unknown familiarity, and the sole synthetic face token.
The 146,515-byte standalone SVG parses as XML and has no image, script,
external reference or external DTD. Its rendered PNG was visually inspected
outside the repository: labels are legible and unclipped, annotations match
the curve, and the synthetic-data label is prominent. A second direct replay
execution produced byte-identical JSONL and empty stderr.
The parent then rebuilt the example from the current working source with
`cargo run --offline --locked -p lamp-mood --example replay_day`. The rebuilt
executable SHA-256 is
`28fb64e68057891b7072b7e2dcd70fb12ced0e6965c319f4dd204c8abe7336b8`;
its stdout was byte-identical to the retained JSONL (`cmp`, exit 0).
`replay-rebuild.txt` retains that successful Cargo output. Host timing,
scoped test results and outstanding full gates are recorded separately in
[host-checks.md](host-checks.md).

No real recorded day, device execution, sensor detection timing,
accepted-event-to-publication timing, actuator response, acoustic feedback,
user acceptance or biological calibration is shown. Minute sampling cannot
resolve one-to-five-second attention leases or every transient. This authored
day does not exercise publisher restarts, wall-clock jumps, malformed inputs
or stalled I/O; those require separate engine/adapter/runtime checks. It is
not accelerated evidence of a day spent beside a person.
