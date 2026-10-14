# Reuse decisions: V1, ROS 2 and existing Rust systems

Review date: 2026-10-10. This is a source-based decision record, not a performance
comparison. V1 means main commit `d5efe9d7b73cc529b34cd4abe97624682a82ca94`.
The current Rust checkpoint is `64529dee4d1228dd9373298a807d506f94421dca`.
External links below were inspected on the review date; rolling documentation
can change. Implementation status is deliberately separate from the design.

The rule is to preserve useful behavior and proven interfaces, reuse maintained
implementations where they fit, and write Rust for the remaining Lamp-owned
responsibilities. A new language alone does not fix timing, echo, ownership or
interaction quality. Do not label an entire system obsolete before examining
its current mechanisms and measuring the relevant alternative.

## What V1 already gets right

Source links in this section are pinned to the baseline commit, so this document
remains useful if lampOS becomes a separate repository. Detailed protocol and
unit evidence lives in the local [hardware contract](hardware-contract.md).

| Preserve | V1 evidence | Rust decision and present status |
|---|---|---|
| Per-field sensor freshness and partial readiness | Environment [service](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/environment/service.py#L109) and [aggregation](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/environment/group.py#L58) exclude stale/faulted samples and retain individual metric timestamps. | Preserve original acquisition times, missing fields and explicit sources. `ObservationTracker` already supplies incarnation, ordering and age checks. The new [environmental snapshot contract](environment-snapshots.md) implements typed values, per-field freshness and fault/restart behavior without I/O. Sensor acquisition and device qualification remain missing. A fresh temperature must not refresh CO2. |
| Privacy closure before slow teardown | [Privacy gates](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/privacy.py#L24) close before service shutdown; camera checks privacy around acquisition. | Keep local authoritative denial and recheck at delivery. Rust capture/camera authority and final output guards implement this principle. Privacy does not wait for a cloud response. |
| Latest camera observation instead of a growing backlog | [Capture](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/camera/video_capture_device.py#L166) replaces the latest response; perception shares frames. | Rust camera has bounded scratch/pending/in-flight buffers and retains the original capture grant. Real camera and conversational vision qualification are still pending. |
| Discard obsolete camera buffers before using a frame | The idle capture loop [grabs twice before reading](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/camera/video_capture_device.py#L555). | Rust discards otherwise valid old startup frames one per poll, preserving sequence/timestamp continuity, the original two-second first-frame deadline and 250 ms age limit. No old frame is exported. After one accepted frame, stale timestamps remain terminal. V1 callback timestamps and old-frame fallback are not reused. [Native qualification](camera-runtime.md#bounded-startup-discard-and-next-native-check) remains pending. |
| Fast touch acquisition independent of slow actions | [MPR121](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/mpr121.py#L642) separates polling/action execution, uses a capacity-two queue, invalidation generations and event age checks. | Port the recognition and cancellation cases. Route qualified events to the interaction owner; avoid copying direct voice/agent side effects. Ordinary touch/buttons are not yet implemented in Rust. |
| One serialized motor bus, with an atomic overload lockout | [Motor service](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/motors/animation_service.py#L1153) blocks new goals before disabling torque on overload. | Preserve one bus writer and distinguish cancellation from torque release. Rust now has the finite MotorControl boundary plus the opt-in [supervised motor worker](motor-worker.md). It retains local inhibition, exact profile/readback checks and position-only writes. Automatic choreography and physical motion qualification remain absent. Limits, units and adapter lifecycle need physical qualification. |

These are meaningful examples of good V1 engineering. V1 runs many hardware
owners as threads inside one HAL process; it is not correct to say that it has
no separation. The change is independent failure boundaries, explicit output
authority and bounded communication, while preserving useful subsystem logic.

## What must change

For the first connected ring policy, retain V1's
[replaceable pending event](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/base.py#L37)
and [stale-restore regression](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/test/test_led_restore_generation.py#L38)
semantics through one pending frame and existing turn/presentation generations.
V1's [speaking wave](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/rgb/effects.py#L284)
is a random simulated equalizer, not actual playback evidence. Its timed effect
thread stop also cannot prove the old writer terminated. The Rust
[ring integration](ring-choreography.md) instead uses fixed prototype phase cues,
matched speaker receipts, one supervised writer and explicit shutdown evidence.
It adds no animation thread, delayed bare `off()` callback, privacy color or
motion. Visual quality and crash recovery still require physical qualification.

| Observed problem | Replacement rule | Verification required |
|---|---|---|
| V1 motor events append to an unbounded list with no expiry or interaction owner ([source](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/motors/animation_service.py#L364)). | Bounded admission, explicit replacement semantics and owner/expiry checks before each physical write. | Flood the queue, cancel/restart owners and verify obsolete gestures cannot execute. |
| V1 camera shutdown clears its thread handle after a timed join without confirming exit ([source](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/d5efe9d7b73cc529b34cd4abe97624682a82ca94/hal/drivers/camera/video_capture_device.py#L692)). | A stop request is not completed shutdown. Retain ownership until actual exit/reaping; report failure. Rust process shutdown already checks exit status. | Stuck worker, late completion and restart tests; no two device owners. |
| Legacy startup can configure torque and queue motion; normalized position values are not degrees. | Read-only inspection first; explicit per-device units, calibrated bounds and current placement evidence before motion. | Fresh whole-device view, limits, current/load protection and stop behavior on Lamp. A successful serial write is insufficient. |
| V1's ordinary TTS capture suppression avoids some self-interruption but discards user overlap. | Preserve continuous capture and a stable render reference; qualify interruption admission separately. | Matched reply-only and quiet/normal overlapping-speech trials, with lost opening words and acoustic stop measured. |
| Current Rust treats six high-VAD blocks as permission to cancel the old reply. Playback residue has triggered that path. | Separate candidate detection from qualified admission; retain the beginning of speech while deciding. | A gate must reject echo without rejecting genuine quiet overlap. No threshold or classifier is selected merely because it passes one recording. |

The opt-in [V1-style turn-taking baseline](turn-taking-baseline.md) now ports
V1's ordinary own-playback suppression and fresh-prefix behavior before further
interruption experiments. Rust retains continuous capture/AEC and uses original
accepted-write/cursor and host-read evidence. Its declared 100 ms tail is not a
measured echo bound, and overlapping user speech is intentionally withheld.
`vad-only` remains the default until physical parity is qualified. This is a
policy reuse decision, not a new DSP engine or a claim of natural full duplex.

The last two rows are documented in the [microphone audit](microphone-v1-v2-audit.md).
They show why the new runtime also needs criticism: process separation and Rust
ownership do not themselves solve conversational perception.

The [pinned AEC source audit](aec-alignment.md) found no sample-rate or float
normalization mismatch. Keep Sonora's existing processing while gathering its
internal alignment separately from the supplied queue hint. V1's delay default,
idle bypass and adaptation-preserving buffer reset are different mechanisms;
copying one constant does not reproduce them or establish better duplex audio.

The [provider output capacity contract](provider-flow-control.md) also preserves
V1's useful bounded frame queues and cancellation-aware synthesis producers.
Capacity must propagate back to generation without pausing local microphone or
control handling. The Rust implementation reserves at most two provider packets
in flight, retains strict IPC age checks and keeps the 30-second coordinator
backlog bound. The later [provider integration](provider-recovery-integration.md)
adds playback-paced worker regressions and preserves the distinction between
modeled playback and real acoustic delivery.

For [conversation lifecycle cues](live-session-cues.md), preserve V1 main
`d5efe9d7b`'s `hal/telemetry/voice_metrics.py` distinction between monotonic
observations, interaction ownership and unknown endpoints. A software callback
does not prove acoustic onset. Reuse Rust's existing bounded `CueSink` and Unix
datagrams for both providers; no new bus, callback authority, timer thread or
network dependency belongs on this observation path.


## Borrow from current ROS 2

**Lifecycle:** ROS 2 distinguishes configuration, inactivity, activation and
terminal states. Adopt that separation in worker supervision: opening a device
does not prove useful input or output readiness. A worker restart must establish
new identity and fresh evidence. The current finite Rust runtime fails visibly
and reaps workers; a general automatic restart supervisor is not implemented.
[Managed nodes](https://design.ros2.org/articles/node_lifecycle.html)

**Actions:** ROS 2 separates an accepted goal, execution, cancellation in progress
and terminal outcomes. Use those distinctions for speech and motion. A request
to stop, device acceptance and observed physical completion are different
events. Keep a bounded result with the original action identity; a delayed
callback cannot complete a newer action. This is a contract to reuse, without
requiring the entire ROS action transport in the first release.
[Actions](https://design.ros2.org/articles/actions.html)

**Communication policy:** ROS 2 makes history depth, reliability, durability,
deadline, lifespan and liveliness explicit. Use per-channel policies and expose
incompatibility or loss. Current Rust Unix IPC already bounds messages and
separates control from audio. Every new channel needs its own overflow and
freshness rules; one generic queue setting is insufficient.
[QoS policies](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/interfaces/topics/About-Quality-of-Service-Settings.rst)

**Scheduling:** ROS 2 supports callback groups and assigning them to different
executors. Its current event executor reduces polling work, but the documented
event queue can grow without bound under overload. Borrow explicit scheduling
and ready-event handling, while keeping bounded queues and independent audio
workers. Do not assume that using an executor, async Rust or more threads proves
a deadline. Measure scheduling delays under concurrent load.
[Executors](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/client-libraries/About-Executors/About-Executors.rst)

**Real-time discipline:** avoid unbounded blocking and cold allocation on timing
paths, report overruns and distinguish mean throughput from worst-tail timing.
The OS, drivers and device buffers remain part of the path. Rust's memory
safety is useful but does not supply a real-time scheduling guarantee.
[Real-time programming](https://github.com/ros2/ros2_documentation/blob/rolling/source/Capabilities/Motion-planning/Real-Time-Programming.rst)

**Deployment:** a ROS node is a logical component, not necessarily an OS process.
Lamp's separate physical workers are a deliberate isolation choice. The five
servos still need one owner because they share a bus. Do not create competing
drivers simply to give each joint a process.
[Launch design](https://design.ros2.org/articles/roslaunch.html)

### Channel contracts for Lamp

These are design requirements. Existing audio/control implementations are
described in [live runtime](live-runtime.md); the remaining channels must be
implemented and qualified before claiming the whole graph runs.

| Channel | Required policy |
|---|---|
| Microphone and accepted render PCM | Ordered, bounded, timestamped blocks; sequence gaps and reference loss are explicit faults. Do not arbitrarily drop old blocks as if PCM were independent camera frames. |
| Camera | Latest useful frame, bounded in-flight ownership, acquisition age and original privacy grant. Drop/replace obsolete pending frames deliberately. |
| Environmental values | Latest per field, explicit units/source, unavailable and fault states, per-field age. A new value for one field does not refresh another. |
| Touch/button events | Bounded ordered recognized events, acquisition times, no invented multi-taps after overflow, generation/expiry checks before an action. |
| Speech/ring/motion commands | Original turn/action identity through the final writer; reject expired or superseded output. Never replay old movement commands after restart. |
| Privacy/revocation | Independent of bulk-data congestion; denied by default after lost authority. A middleware delivery receipt is not proof of physical silence. |
| Diagnostics | Bounded and nonblocking; distinguish dropped diagnostics from a clean measurement. Preserve source/configuration/fixture identities for replay. |

## Reuse existing Rust implementations before inventing infrastructure

Keep the already selected Rust libraries and reviewed DSP interfaces where they
fit. Do not rewrite working resampling, cryptography, audio backends or model
inference just to call the project a rewrite. Lamp owns the runtime policy,
hardware contracts and end-to-end experience; dependencies remain explicit.

| Candidate | Useful existing work | Decision now |
|---|---|---|
| [iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2) | Rust shared-memory IPC, bounded-resource communication mechanisms and large-payload transfer without serialization copies. | Evaluate before implementing a custom cross-process camera frame pool. No replacement of working control IPC is selected. ARM64 support and crash recovery need qualification; upstream desktop results are not Lamp results. |
| [dora](https://github.com/dora-rs/dora) | Rust dataflow runtime, process lifecycle, declared graphs, record/replay and observability. | Evaluate reuse for launch/supervision/tooling before building a general robot framework. Verify cancellation, bounded overload, device exclusivity and our local timing targets. No runtime dependency has been added. |
| [Zenoh / ROS 2 integration](https://github.com/ros2/rmw_zenoh/blob/rolling/docs/design.md) | Current ROS 2 can use Zenoh; its design separates discovery from direct same-host peer communication. | Consider when remote components or ROS interoperation are actually required. Network discovery does not belong in the immediate local interruption path. No network middleware is required for the desk-chat slice. |

These are inspected candidates, not benchmark winners. Any dependency decision
must include license, maintenance, platform, failure behavior and memory review.
A replacement must pass the same workload and failure cases before it displaces
current code. Retain failed trials as well as successful ones.

## Next implementation and proof

1. Finish physical validation of the already-built terminal audio shutdown fix.
   Then use paired cached-reply tests to distinguish false interruption from
   real overlap. Preserve the quiet-speaker case; do not solve echo by deafening
   Lamp during its own answer.
2. The small environmental snapshot contract is now implemented and host-tested
   with explicit units, missing values, per-field freshness and restart/fault
   regressions. Keep it I/O-free until configured sensor workers are
   implemented; qualify the installed hardware before integrating it.
   Do not auto-probe the bus.
3. Reuse V1 touch recognition and safety regression cases as those workers are
   added. Require real device exit before restart and preserve acquisition time.
4. The optional ring policy now shares actual playback events and interaction
   ownership. Qualify its physical timing/visual quality and complete crash
   recovery, then add safe motion and fresh vision. Stillness is valid. No
   automatic startup motion.
5. Compare reusable infrastructure candidates only on the workload they would
   replace: control latency under camera/audio load, bounded memory, dropped
   observations, cancellation, worker death and restart. Keep cloud response
   time separate from local runtime costs.

The general-purpose opportunity is reusable ownership, lifecycle, device
contracts and diagnostics, with robot-specific profiles and drivers. Lamp is
the first qualification target. It does not yet establish support for arbitrary
robots or a tenfold speedup. There is still no accepted matched V1-main/V2
end-to-end p50/p95 comparison.

## Candidate admission before interruption

The [input admission contract](input-admission.md) borrows V1's distinction
between local speech suspicion and destructive cancellation. It does not copy
V1's unqualified energy thresholds, three-second hardware warmup assumption,
or transcript/provider confirmation. In the current explicit-activity Gemini
configuration, Start itself requests interruption; its response is circular
evidence. The bounded local gate preserves original audio and candidate identity
while leaving pending/rejected output ownership intact. Directed qualification
still chooses VAD-only acceptance and has no new classifier accuracy result.

## Reply startup buffering experiment

Before changing playback timing, the native V1 producer and persistent TTS
stream were inspected at the pinned V1 commit. V1 requests at least 120 ms
driver latency and immediately writes available native audio; its pending
audio handles speaker contention rather than a fixed startup delay. Retain
continuous speaker ownership and bounded cancellation, while reusing the Rust
`7f24ea81d` startup-gate experiment behind explicit `--playout-start-ms 0..200`,
default zero. The [contract and held-out enqueue replay](playout-start.md)
distinguish queue-horizon modeling from driver and acoustic measurements.
A language rewrite or a passing replay does not establish V1 playback parity.

## Continuous chat core (private proposal, 2026-10-11)

The finite Rust coordinator already owns provider sessions, accepted audio,
privacy, ring phases and cancellation. The `serve` slice reuses that path and the
existing `rtrb` SPSC dependency instead of introducing another conversation engine
or a logging service dependency. It removes only the finite session/ring ceilings
and replaces qualification trace retention with bounded best-effort metadata.
Typed receipt lineage is extracted from trace lookups so dropping observations
cannot change authority. See [continuous chat core](serve-runtime.md) for limits,
including the separate mute/reopen lifecycle integration and native qualification.

## Continuous-service physical mute supervision

Keep the existing physical GPIO worker, 50 ms observation freshness, 60 ms
unmute stability and strict per-runtime privacy checks. A service-level GPIO-only
wait precedes opening any audio/provider worker; fresh physical mute returns to
that wait only after verified shutdown. Reuse the same bounded metadata writer
and termination registration through cycles. Do not revive permits, replay PCM
or claim provider-memory continuity. See [service privacy](service-privacy.md).


## Optional ring startup isolation

Inspected V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94`:
`hal/runtime.py::_init_led` catches optional driver startup errors; RGB open failure
in `hal/drivers/rgb/rgb_service.py::start` leaves the light unavailable while audio
initializes separately. Keep that optional-component boundary. Reuse the Rust
private socket contract and existing optional ring stop/kill/status supervisor;
retain an unready child instead of dropping ownership with its startup error.
The Rust implementation adds a per-attempt socket namespace and inode-checked
cleanup, plus strict separation of mandatory heartbeat errors. It does not copy
V1 shared callback/thread ownership or add a recovery worker. No new dependency,
optical measurement, V1 speed claim or physical parity claim is implied. See
[worker startup](worker-startup.md).

## Expression: authored timing and bounded requests (2026-10-14)

This addition reviews the current checkout, separately from the pinned audit
above. It is a source review for [Expression](expression.md), with no new
hardware or performance measurement. The earlier lamp observations below are
the first builder's records in [Body](body.md), not results from Third's machine.

**Keep from V1:** the named owners in `hal/drivers/motors/hold.py`, and the
generation checks demonstrated by `hal/test/test_led_restore_generation.py`,
`test_emotion_status_expiry.py` and `test_rgb_shutdown.py`: an old restore or
late frame cannot overwrite its successor or relight a stopped output. Keep
gesture data and explicit joint units. Replace the growing motor event list
(`animation_service.py::dispatch`) with bounded, expiring requests. Do not copy
the random values in `rgb/effects.py::speaking_wave` as evidence of speech, or
the privacy and sleep changes inside `routes/emotion.py` as expression policy.

**Keep the body's measured protections:** `crates/body/src/gaze.rs` waits
400 ms of stillness, takes the median of three sightings, widens its deadband
while speaking and never searches. `body.rs` starts a thinking glance at 36
encoder counts and leaves an arrived joint alone until the new target is at
least 20 counts away. The first builder recorded three played exchanges each
way on lamp-4ace: the base was 8.3–11.7 degrees from its listening direction at
voice onset before the glance, and 1.4–3.2 degrees after it. This showed a
smaller late return, not return completed before speech, and nobody had judged
its appearance. Expression must preserve calibrated gaze and local motor
limits. A tiny requested nod can be swallowed by that real servo deadband.
The existing `clips/data.rs` nod lasts about 3.1 seconds, with over half a
second before its offsets start changing; it is not a listening-pause nod.

| What exists; primary source inspected on 2026-10-14 | What Expression takes |
|---|---|
| [BML 1.0](https://projects.cs.ru.is/projects/behavior-markup-language/wiki/Wiki) describes gesture sync points, composition, progress and warning feedback, and dropping a behavior whose timing constraints cannot be met. | Preparation, stroke and relaxation in authored data; a late gesture shrinks or is omitted; requested timing is separate from observed completion. No XML realizer dependency or BML compliance claim. |
| Maintained [ROS `twist_mux`](https://raw.githubusercontent.com/ros-teleop/twist_mux/rolling/include/twist_mux/topic_handle.hpp) selects by priority and expiry. Expired motion is masked; an expired safety lock remains locked. | Short leases and explicit precedence at the output owner. Expression expiry restores fallback; it must never clear a privacy or safety denial. |
| The current [Reachy Mini API](https://huggingface.co/docs/reachy_mini/API/reachymini) evaluates recorded moves at a chosen frequency, cancels at the next playback iteration, and composes audio-derived motion with the target pose. | Pure sampling on an injected clock, bounded cancellation and additive offsets. Reuse the pattern, without a Python service or SDK dependency. |
| [ELEGNT](https://arxiv.org/html/2501.12493v1) reports 21 valid video-study participants: expressive motion helped especially in social tasks, while motion and motor noise could interfere with lighting tasks. [Murray et al.](https://proceedings.mlr.press/v164/murray22a.html) found learned nod timing preferable to random and rule-based baselines in their robot study. | Stillness while working and sparse, event-linked listening cues. These published results motivate tests; they do not validate this lamp's pause heuristic or comfort over weeks. |

**Change the timing interface:** reuse `lamp_ipc::monotonic_us()` and identify
its shared host clock separately from publisher instances and turns. The bus
header's `at_us` has no cross-publisher clock promise. A real playback deadline
and stamped level, user speech/pause edges, interruption/cancellation and an
explicit concentration state are missing from the existing bus types. The
current `voice/state` also omits the early return and duplex-yield overrides
sent privately by `coordinator.rs` to `BodyLink`. `room/state` describes room
speech, not concentration. First wires these inputs and the body/ring owners;
Expression cannot infer the missing facts from transcript arrival.

## What this Expression review has not shown

No V1 or Rust tests were run for this source review, and no lamp was accessed.
The reviewed implementations were not benchmarked against each other. The
100 ms light response and roughly 350 ms body transition remain design targets;
the inherited Disney attribution, doubled-stumble figure and 0.6/0.3-second
gaze claim were not verified here. Playback prediction error, motor lag, gaze
calibration, glare, acoustic interference and whether the result looks natural
still require the first builder's measurements and observation on the lamp.

## Controls: keep the recognition cases, change the actions (2026-10-14)

This is a source review of the current V1 checkout for [Controls](controls.md),
not another hardware measurement. The control surface on the base is MPR121;
the TTP223 pads on the head are pet input only. GPIO and the base must stop
speech; a pet must not interrupt it. The detailed V1 behavior is in
[`physical-controls.md`](../../robots/lamp/docs/physical-controls.md).

**Keep the button's edge rule:** `hal/drivers/gpio_button.py::_on_edge`
suppresses repeated presses for 200 ms and repeated releases independently for
200 ms. A press and release less than 200 ms apart still count. Ignore a release
without a matching press, commit a hold only on release, and never delay the
first Stop for the 400 ms multi-click window. V1 measures GPIO hold duration at
callback arrival; the new injected-clock recognizer instead preserves the
adapter's acquisition times so delayed delivery cannot lengthen a hold.

**Keep the base's qualification:** `hal/drivers/mpr121.py` separates the
5 ms footprint filter from the 10 ms contact/chord qualification in the Lamp
configuration. Three selected pads must be active together; separate short
intervals do not accumulate. Qualification survives staggered lift, and the
falling sample closes the qualifying interval before it is cleared. A contact
held at startup is ignored until release. Keep the tests for these cases,
stationary footprint growth versus travel, and the measured mask sequences in
`hal/test/test_mpr121_swipe.py`. The chosen full-release grace for a qualified
stationary contact is 30 ms, shorter than V1 Automatic's 120 ms handoff grace
and equal to its fast stationary tap-to-talk floor. Moving/unqualified contacts
retain 120 ms and produce no tap or mute. This is an explicit change for the
new Stop-only tap mapping; sampled mask replay cannot establish a physical
minimum touch duration.

**Keep the head's grouping:** `hal/drivers/ttp223.py` waits 500 ms after
initialization; either edge restarts its 200 ms contact timer. The first-session
chime is at that session's end, not the first edge, and does not mean readiness.
Each completed session restarts a 1.2 s decision window; early pet recognition
can resolve sooner in V1. Controls keeps generic pet grouping and omits V1's
directional labels and early spatial-reversal path. Each response attempt starts
a 1.5 s cooldown, extended by contacts during it. The 20 ms LOW rule qualifies
only an undetected candidate pad, not every touch. Controls uses explicitly
configured known pads; discovery, pin polarity and any probe belong to First's
adapter. Do not guess wiring or
reuse the legacy fallback pins: V1's button fallback conflicts with the privacy
switch, and its touch fallback conflicts with the reset button.

**Change the action policy:** short, double and repeated taps request Stop;
a released hold of at least 2 s requests one-way `SoftwareMute`. Five seconds
and longer still mean mute, with no shutdown/reset escalation. Neither Stop nor
a missing Controls process clears a mute. Reboot, reset, sleep, Harness routing
and voice-mode toggles are not copied. Stop must reach the existing owners of
following, current and pending speech, and music; it must not cancel agent work
or directly release motor torque. Pet feedback stays small and non-interrupting;
V1's brain notification is limited to once per minute.

| Primary source inspected on 2026-10-14 | What is taken |
|---|---|
| [gpiocdev 0.8.0](https://docs.rs/gpiocdev/latest/gpiocdev/) and its [upstream repository](https://github.com/warthog618/gpiocdev-rs) provide a pure Rust Linux character-device interface. Voice already pins this version. | Reuse it for First's Linux adapter instead of another GPIO implementation; keep recognition independent of the library. No new hardware adapter is supplied by Third. |
| [Official libgpiod Rust bindings](https://libgpiod.readthedocs.io/en/master/rust_api.html) wrap the C library. | A maintained alternative where that library is already packaged; no second GPIO dependency is selected here. |
| [Linux line requests](https://www.kernel.org/doc/html/latest/userspace-api/gpio/gpio-v2-get-line-ioctl.html) are exclusive. Kernel debounce waits for a stable level and timestamps the end of that interval. [Edge events](https://www.kernel.org/doc/html/latest/userspace-api/gpio/gpio-v2-line-event-read.html) carry sequence numbers that reveal queue overflow. | Do not substitute 200 ms kernel debounce for V1's per-polarity suppression. Preserve capture time and report lost transitions as a fault; never manufacture a release from missing input. |
| [NXP AN3892](https://www.nxp.com/docs/en/application-note/AN3892.pdf) defines MPR121 touch/release hysteresis, chip debounce and overcurrent status. | Preserve the adapter's distinction between a clean electrode mask and a fault. Chip debounce `0x22` means three consecutive detections, independently of the software clocks. OVCF must not become an all-released sample. |

The physical slide remains exclusively with Voice's existing GPIO worker and
[supervised privacy lifecycle](service-privacy.md): 50 ms observation freshness,
60 ms stable unmute, and verified cleanup before reopening. A Controls message
is a request, never a microphone/camera permit. The receiving owner must retain
an accepted software inhibit through Controls loss/restart, restore it before
opening workers, and expose a separate explicit clear procedure. Bus delivery
alone is not proof that capture or playback stopped. The exact persistence and
consumer integration boundaries belong in [Controls](controls.md).

## What this Controls review has not shown

No tests or benchmarks were run for this review and no lamp was accessed. V1's
recorded traces and configured timing constants are inherited evidence, not
new results from Third. Native GPIO/I2C behavior, installed pad sensitivity,
touch-to-silence latency, physical privacy, output-owner integration and the
user's interpretation of the feedback still require First's qualification.
