# Real-time systems lessons for the Lamp V2 runtime

Design research, 12 October 2026. No runtime implementation or device qualification is implied.

## Recommendation and evidence boundary

Keep Linux on the current Lamp, with a small deterministic Rust control core, direct audio paths, and separately supervised workers. Put process creation, termination, reaping, provider reconnection and persistence outside the control loop. Keep remote agentic planning behind an engine-independent contract; neither an RTOS nor a deterministic executive replaces Hermes or another reasoning engine.

Start with ordinary Linux scheduling and measured resource isolation. Consider PREEMPT_RT only after an instrumented baseline identifies scheduler latency that it can improve. Add an independently operating controller only if a measured, hazard-derived actuator or watchdog requirement cannot be satisfied by the existing board and servo system. Rust memory safety, a 2 ms loop, and the PREEMPT_RT label do not establish hard real-time behavior.

Current V2 source inspected: **`e1eae8de5ad65c1ec6c6450948afbcb510982240`**, fetched `origin/lamp-v2-chat`; all V2 links below pin this commit. This supersedes `10607a43963ca1a1e7c1b0bfda024a9ecb0b99f9`, especially for motor clearance and measured-stop code. Existing design notes were background, not proof. Upstream primary documentation was checked on 12 October 2026; `latest`/`main` documentation is not evidence that the installed Lamp supports those features.

The repository image script targets Orange Pi 4 Pro v2/A733 and names a vendor Linux 5.15.147 image, but explicitly uses a hardware-team base image. The ALSA profile names USB audio devices. Those are source configuration facts, not a fresh inventory of the deployed board, kernel configuration, firmware or wiring. No device was accessed for this study. [Image script][board-image] [ALSA profile][board-audio]

## What V2 already provides

| Verified current behavior | Implication for the design |
| --- | --- |
| Nonblocking Unix datagrams separate control from bulk traffic; maximum datagram is 8 KiB. Local monotonic timestamps, boot identities and sequence checks reject stale or reordered messages. | Retain the transport discipline. Socket permissions and same-user peers are not a complete hostile-process security boundary. Successful send proves kernel enqueue, not consumption. [IPC][v2-ipc] [Ordering][v2-wire] |
| Capture and speaker are separate workers; capture-to-speaker reference traffic has its own channels. Capture is 16 kHz, rendering 24 kHz, both in 10 ms blocks. | Preserve direct media paths and the accepted-PCM echo-reference contract; do not route every sample through the brain or a general event broker. [Channels][v2-transport] [Audio blocks][v2-audio] |
| Coordinator and audio workers use 2 ms ticks with bounded message counts. Provider work permits two packets and a 2 ms cooperative slice; one decode may exceed the slice. | This is bounded cooperative work, not preemptive scheduling or a proved worst-case execution time. A full provider slice can already consume an entire coordinator tick. [Coordinator budget][v2-budget] [Audio loop][v2-audio-loop] |
| Output permission checks ownership, input readiness, privacy, generations and freshness at the boundary. Existing input/output leases are short, with several 100 ms bounds. | Reuse this authority model. Expiring authority is valuable only when the enforcing worker actually runs; it cannot stop autonomous hardware while the host is wedged. [Output gate][v2-gate] [Lease constants][v2-budget] |
| ALSA capture is nonblocking, but the 10 ms operation limit is checked after the call returns. Playback allows a 40 ms accepted queue and distinguishes its 100 ms tail watchdog from acoustic silence. | Neither setting bounds a kernel/USB stall or measures room sound. Keep operation overruns, accepted frames, hardware status and acoustic evidence separate. [Capture][v2-capture] [PCM][v2-pcm] [Ownership][v2-audio-ownership] |
| Physical microphone privacy is a separate userspace GPIO worker, polling every 5 ms and using a 250 ms controller lease. | Preserve its independence. This source does not demonstrate an electrically independent microphone power cutoff. [GPIO privacy][v2-privacy] |
| Trace writing uses a bounded 64-entry queue, bounded records and rotated files; filesystem work is on a separate thread, with loss counters. | Reuse bounded diagnostics. Metadata construction/serialization can still allocate; a nonblocking sink does not automatically make the producer allocation-free. [Trace writer][v2-trace] |
| Optional camera failure revokes the visual grant without making healthy voice fail. Motor replacement requires observing the old child exit. | Generalize these existing fault-containment and ownership rules, rather than claim that V2 needs them invented. [Camera lifecycle][v2-camera] [Motor supervisor][v2-motor-supervisor] |
| Startup and final cleanup can spawn, wait and reap children. Generic forced teardown calls `kill()` followed by blocking `wait()`; optional live teardown already has a signal-only path. | Complete the separation of lifecycle work from interaction deadlines before adding richer brain services. [Process lifecycle][v2-process] |
| Continuous service currently returns an error once terminal provider output and retained obligations settle, explicitly recording `automatic_retry: false`. The attended unit has `Restart=no`. | Provider exhaustion remains a product lifecycle gap. Raising thread priority will not repair it; do not silently change the attended pilot into an unattended deployment. [Terminal provider path][v2-provider-end] [Pilot unit][v2-unit] |

The current motor branch is more specific than “cancel stops motion.” It distinguishes `HostAccepted`, `ReadbackReached`, `Inhibited` and `Stopped`; the last concerns cleanup and can include a separate hold report. A separately provisioned measured-pose hold runs after ordinary motion is inhibited, with at most a 100 ms software deadline and bounded sample age. `ObservedAtRest` requires stationary post-command readback, but the code explicitly excludes collision clearance, force, safe torque release and independent reconstruction of servo acquisition time. Preserve all these distinctions. [Motor receipts][v2-motor-wire] [Measured hold][v2-stop]

## Transferable lessons from mature systems

**Linux PREEMPT_RT: make latency sources visible before promoting priorities.** Many kernel locks become priority-inheriting sleepable locks, and most interrupts execute in threads; some low-level regions remain nonpreemptible. A runnable high-priority FIFO thread can starve required work. Threaded IRQs normally start at FIFO priority 50, so arbitrary application priority 99 can make the relevant device interrupts wait. This is a scheduling mechanism, not an application deadline guarantee. [Kernel theory][linux-rt]

Firmware and hardware remain outside that argument: the kernel documentation specifically identifies stalls caused by firmware, including ARM secure-world execution. Establish the actual vendor kernel, driver behavior and firmware limits. PREEMPT_RT is a candidate kernel configuration requiring an audio/USB/camera regression campaign, not a switch that makes the existing application safe. [Hardware limits][linux-hardware]

Linux `SCHED_DEADLINE` combines earliest deadlines with runtime/period reservations and throttling. That can contain a misbehaving CPU consumer, but a depleted speaker budget can also delay useful work. It requires measured demand, blocking analysis and admission control; do not assign it to every service. Retain a non-real-time CPU reserve when experimenting with FIFO/RR; inspect the installed scheduler/cgroup behavior rather than disabling RT throttling globally. [Deadline scheduler][linux-deadline] [Scheduling controls][linux-sched]

**Zephyr: short interrupt work, bounded handoffs and separate blocking domains.** Its scheduler selects the highest-priority ready thread; optional deadline ordering operates among threads of equal static priority. ISRs should do little work and wake thread context. A workqueue handler that blocks delays everything behind it. Transfer these disciplines to Lamp's kernel-to-worker boundary and worker queues; a Linux userspace GPIO callback is not an ISR. [Scheduler][zephyr-sched] [Interrupts][zephyr-isr] [Workqueues][zephyr-work]

Zephyr memory slabs bound the number and size of buffers, avoiding variable-size heap fragmentation. Its task watchdog supports independent task channels with an optional hardware fallback if the software watchdog or scheduler stops. Lamp should borrow fixed-capacity media/control storage and progress-specific watchdogs; software channels sharing a stalled Linux kernel do not provide that independent fallback. [Slabs][zephyr-slabs] [Task watchdog][zephyr-watchdog]

**FreeRTOS: a small scheduler still needs disciplined application code.** Default task scheduling is fixed-priority preemptive scheduling with time slicing at equal priority; a high-priority task that never blocks can starve lower priorities. Tasks, queues and synchronization objects can use application-provided static storage. Its mutex priority inheritance is deliberately basic: a task holding multiple mutexes retains inherited priority until all are released. Transfer finite task populations, explicit buffer sizing and short lock ownership; do not assume a mutex removes inversion or a small kernel eliminates overload. [Scheduling][freertos-sched] [Static allocation][freertos-memory] [Mutexes][freertos-mutex]

**seL4: make both authority and CPU consumption explicit.** Mixed-criticality scheduling contexts express CPU budget and period. A passive RPC server can execute using the caller's donated scheduling context, making delegated CPU work accountable. Capability-mediated IPC and explicit fault handlers suggest narrow services with explicit authority and contained failure. Lamp can implement analogous bounded requests, ownership grants and independent supervisors on Linux; it does not thereby acquire seL4's enforcement or verification. [MCS][sel4-mcs] [IPC][sel4-ipc] [Fault handling][sel4-fault]

seL4 verification applies to stated architectures/configurations, with documented exclusions and work in progress. It does not certify arbitrary drivers, startup code, DMA devices or the entire robot. A Lamp port would need a qualified board/driver/virtualization design; no such qualified stack was established here. [Verification scope][sel4-proof]

**RTIC and Embassy: useful Rust execution references, not a required Lamp rewrite.** RTIC declares task-local and shared resources; its shared-resource locking raises dynamic priority according to a ceiling protocol. Embassy's executor uses statically allocated tasks, no executor heap allocation, and wake-driven polling. Borrow explicit ownership, bounded task populations and short execution segments. Inference: neither approach makes a blocking driver call or an unbounded future poll preemptible. Use them as MCU candidates only if a later hardware requirement justifies firmware work. [RTIC resources][rtic] [Embassy executor][embassy]

## Proposed process and execution structure

```text
systemd: outer lifecycle, startup readiness, service watchdog
  |
  +-- lamp-supervisor [ordinary scheduling; lifecycle and recovery only]
  |     spawn / readiness / cgroup placement / terminate / reap / backoff
  |     observes progress; never issues unchecked actuator commands
  |
  +-- control core [one deterministic owner; no disk/network/process waits]
  |     attention + policy -> short-lived output grants and action identities
  |     priority control channels -> final boundary checks in each writer
  |
  +-- privacy worker       +-- capture/AEC <--> speaker writer
  +-- motor writer         +-- ring/light writer
  |
  +-- cloud/agent adapter   +-- camera/perception/sensors   +-- trace writer
         bounded proposals/events; no direct device authority

Linux drivers / actual device firmware -> physical audio, light and motion
```

This extends the existing process separation. The supervisor owns all child handles and restart state; the control core owns interaction/output authority. Neither may impersonate the other's successful completion. Device workers are the only actuator writers. Give each worker only its required device access and IPC endpoints; same-UID socket isolation alone cannot contain a compromised peer. Credentials, inference and provider SDKs stay out of device writers.

Reserve three cancellation operations in the contracts: **speech supersession**, **physical action cancellation**, and **durable task cancellation**. A new conversation may invalidate an old utterance; it must not silently cancel an ongoing research task. Physical cancellation must revoke the particular action's authority and request the qualified stop policy. Do not report mechanical completion from a task cancellation acknowledgement or worker exit.

### Dispatch, priority and provisional budgets

The following are **initial profiling targets**, not measured V2 performance, shipping guarantees, or scheduler reservations already configured. A budget means CPU execution demand for one activation, excluding time blocked in a kernel call; instrument both CPU demand and elapsed time. Exceeding a target requires investigation and requalification, not dropping a valid safety command to make the metric pass.

| Execution owner | Release/service interval | Initial CPU budget per activation | Deadline policy |
| --- | --- | --- | --- |
| Control core | Event-ready, with 2 ms maximum intended timer interval | 250 us | Control before proposals; no lifecycle waits; record missed service intervals. |
| Privacy worker | Edge-ready if qualified; 5 ms fallback polling | 100 us | Candidate healthy-path goal: observed local edge to all relevant output revocations within 10 ms. |
| Speaker writer | 2 ms maintenance, 10 ms audio blocks | 250 us | Maintain ownership even when there is no new PCM; flush only according to the explicit policy. |
| Capture/AEC | 10 ms blocks; urgent control between bounded operations | 2 ms | Retain or explicitly mark discontinuity; no hidden backlog masquerading as live input. |
| Motor writer, when enabled | 2 ms service interval | 250 us | Check authority, transaction and stop deadlines locally; physical limit remains separately qualified. |
| Provider, vision, sensors, tracing | Event-driven, per-worker rate/capacity limits | Measured per worker; ordinary scheduling | May degrade or shed stale work; cannot consume unbounded control time or memory. |

The first five nominal budgets total 59.5% of one CPU before IRQs, kernel work and other services. That arithmetic is a planning check, not a utilization result: the real core type, thermal state, AEC cost and driver blocking remain unmeasured. Do not assume all cores have equal capacity or reserve a CPU without measuring the required IRQ paths.

Use readiness-driven waits and absolute monotonic deadlines; after a missed period, skip obsolete work and record the miss rather than executing an unlimited catch-up burst. Absolute `clock_nanosleep` deadlines avoid relative-timer drift but still permit scheduling delay after wakeup. An `epoll`/timer-based implementation should preserve that absolute schedule. Keep both operation-count and elapsed-time limits, remembering neither preempts an operation already inside a syscall or decoder. [Absolute sleeps][linux-sleep]

Initial qualification uses `SCHED_OTHER`. If measurements justify promotion, evaluate a bounded priority order: required device IRQs, privacy/control revocation, speaker/capture, motor maintenance, then ordinary application work. Actual IRQ dependencies can change that order. Do not copy numerical FIFO priorities across boards or treat Rust's standard mutex as a promised priority-inheritance primitive. Prefer single ownership and bounded mailboxes over locks shared across priority levels.

Retain the existing 100 ms authority leases and 20 ms refresh cadence initially, subject to the exact channel contract. The 10 ms healthy-path goal above is distinct from lease-expiry containment and from physical/acoustic stopping. A missed refresh must reduce authority; extending a lease to hide an overloaded loop changes the failure envelope and requires explicit review. [Refresh path][v2-refresh]

### Memory and overload

Preallocate fixed PCM blocks, control messages and bounded event pools on deadline-sensitive paths. Current wire encoding uses owned JSON allocation; replace or bound allocation on the critical control path only after measuring it, while retaining schema/version checks. Avoid migrating all IPC to shared memory without evidence: shared slots add generation, lifetime and privacy-erasure obligations. [Current encoding][v2-encode]

After initialization, optionally lock and pre-touch only the small critical working sets, including stack pages. Record lock failure as a qualification failure if the timing claim depends on residency. Do not fork from that process: page locking does not remove copy-on-write faults introduced by forking. Model weights and caches belong in constrained ordinary workers. [Memory locking][linux-mlock]

Cap worker count, RSS, scratch storage and queue occupancy. On cgroup v2, memory protection and limits have different semantics; protection depends on ancestors and available memory, and overcommit can still cause OOM. CPU controls for ordinary scheduling are not a universal RT budget mechanism. Verify the actual vendor kernel supports the planned controllers. [cgroup v2][linux-cgroup]

Define saturation per message type: coalesce replaceable sensor state; discard stale camera frames; report audio discontinuities; keep task results in the durable task domain. Revocation must dominate ordinary proposals. A full critical-control queue must lead to loss of authority and a visible fault, not unbounded retries or silent success. Keep existing boot/sequence/age checks when changing transports; remote clocks require a separate time model.

## Lifecycle, recovery and watchdog design

Use explicit worker states: `Absent -> Starting -> Ready -> Degraded -> Inhibiting -> Stopping -> Exited`; unresolved ownership or repeated failures enter `Quarantined`. State transitions carry worker boot identity, reason and evidence. `Ready` requires the actual capability to accept work, not just successful `exec`. Optional workers can remain degraded while local voice/privacy continue.

On a worker fault, the core revokes its grants immediately and the supervisor performs teardown independently. Replace an actuator writer only after observing the exact old child exit; unresolved exit must quarantine ownership. Restart with a new boot identity, fresh readiness and fresh authority. Never replay an old motion command merely because the new process is available. Retain the current motor supervisor's distinction between a clean exit and forced termination. [Motor replacement][v2-motor-replace]

The supervisor itself must use nonblocking per-worker progress/status checks. A camera stuck in teardown must not block supervision of the speaker or motor. Keep unavoidable blocking initialization in the owning worker or a bounded helper, with an explicit limit on pending recoveries. A software timeout can mark ownership unresolved; it cannot force an uninterruptible kernel operation to complete. Moving all blocking calls into one shared supervision thread would merely relocate the failure bottleneck.

The current motor supervisor specifies a 3 s startup deadline, 50 ms progress bound, 20 ms inhibit acknowledgement, and 250 ms teardown/reap bounds; missing a deadline is failure, never proof of released ownership. Preserve these as protocol limits during migration, not newly asserted physical guarantees. Host parent-death `SIGKILL` cannot itself retract a servo goal already accepted. [Supervisor clocks][v2-motor-clocks] [Native worker death handling][v2-motor-native]

Make terminal provider failures a **capability transition**: cloud unavailable, local OS still alive. Settle owned input/output obligations, revoke the failed provider's generation, retain physical privacy and local controls, then recover through the supervisor. Distinguish retryable connection failures, quota/time-window exhaustion and credentials requiring configuration. Use capped exponential backoff with jitter and a retry budget; never repeat an ambiguous side effect solely to recover a stream. This is proposed behavior; the current terminal path still exits. [Existing terminal behavior][v2-provider-end]

Use systemd `Type=notify` only after implementing truthful readiness, and `WatchdogSec` only with progress-sensitive keepalives. The documented service watchdog detects missed pings and applies the configured restart policy; it does not directly make outputs safe. Candidate first experiment: a 100 ms supervisor progress heartbeat and a 1 s service-watchdog timeout, adjusted after load tests. Neither substitutes for 10 ms local revocation or motor stop qualification. Check installed systemd compatibility before selecting directives. [Service readiness and watchdog][systemd-service]

A worker heartbeat must include meaningful progress: last audio maintenance, last authority check, latest accepted frame/reference accounting, or last completed bounded motor poll. A separate timer thread that keeps pinging while the useful thread is stuck creates false health. Disk-full logging or cloud quota exhaustion should not trigger whole-device reboots while local control is healthy.

A hardware watchdog is a distinct layer: Linux's watchdog interface can arrange reset when keepalives stop, but availability, timeout granularity and behavior depend on the driver/device. Enable only after qualifying reset, boot readiness and motor torque/pose behavior; reboot is not an emergency-stop strategy. An independent MCU/watchdog circuit is justified only by a requirement to retain a safe response despite Linux/kernel failure, and only with verified electrical authority over the relevant actuator. [Linux watchdog API][linux-watchdog]

## What must be measured before making stronger claims

| Evidence boundary | Required qualification |
| --- | --- |
| Platform inventory | Actual board revision, RAM, kernel/config including preemption mode, systemd/cgroups, firmware, CPU topology/governor, thermal behavior, IRQ affinity, USB topology and audio/camera drivers. Source defaults are insufficient. |
| CPU scheduling | Per-thread CPU demand, wakeup delay, dispatch gaps, page faults, queue high-water marks and deadline misses under declared load. Kernel `timerlat` separates IRQ and thread wakeup latency; it does not measure end-to-end audio or motor stopping. Verify tool/config availability on the vendor kernel. [timerlat][linux-timerlat] |
| Privacy and speech | Electrical input edge, userspace observation, authority revocation, final accepted PCM, driver queue status, actual microphone retention and external acoustic silence as separate timestamps. Test cold/warm use, ordinary supersession and hard privacy revocation independently. |
| Motion | Request received, inhibition, host-accepted serial bytes, servo readback and independently observed physical stop under qualified placement/load. Measure stopping distance/time, joint combinations, stale feedback, obstruction and hold/torque-release behavior with an authorized fixture. The 100 ms hold policy is not this result. |
| Recovery | Provider quota/auth/network failures; worker crash, hang and restart storms; disk full; logger failure; camera/inference overload; CPU/memory/USB contention; unplug and driver errors. Healthy mandatory capabilities must remain usable, with truthful degradation. |
| Isolation | Prove device access restrictions, bounded memory/queues, fresh ownership after restart, no concurrent bus writers, and no provider-controlled bypass of final output checks. Test a stuck child whose exit cannot yet be confirmed. |

Report p50/p95/p99/p99.9, maximum observed, sample count, test duration, failure/miss counts and exact clock boundaries. Compare normal Linux and any RT candidate on the same hardware/configuration and workload. A finite stress test maximum is not a proved worst-case execution time; successful host tests do not establish acoustic privacy, collision safety or bounded kernel response.

## Alternatives and migration implications

| Alternative | Decision for this design |
| --- | --- |
| Ordinary Linux + separated Rust workers | Recommended initial platform. It preserves the existing ALSA/USB/camera/network stack and makes supervision and resource ownership explicit. Requires measured qualification, not an assumption of adequate latency. |
| PREEMPT_RT Linux | Conditional experiment after baseline measurements identify relevant scheduling stalls. Requires a compatible board kernel/driver build and regression testing; cannot bound arbitrary firmware or mechanical behavior. |
| `SCHED_DEADLINE` for selected threads | Defer until measured CPU demand and blocking justify reservations. Budget exhaustion and admission failure need explicit failure handling. |
| Zephyr/FreeRTOS replacing the application processor OS | Not justified by present evidence. Would require a qualified board port plus audio/camera/network/USB integration or new hardware boundaries; no demonstrated benefit pays for that migration yet. |
| seL4-based system or Linux partition | Keep as a future isolation study if threat/safety requirements justify the platform work. Requires a complete driver/DMA/VM/board design and an explicit verification scope; kernel assurance alone does not transfer to Lamp. |
| RTIC/Embassy on an additional MCU | Conditional later option for independent timing/watchdog requirements. The MCU must be the sole owner of its actuator link, with a defined host lease and safe firmware response; no speculative firmware rewrite now. |

Recommended implementation sequence, for a later authorized build:

1. Preserve and replay existing authority, stale-message, privacy, receipt, optional-worker and motor ownership cases against the pinned baseline. Add lifecycle contracts before rearranging processes; retain speech/task/physical cancellation distinctions.
2. Separate the lifecycle supervisor from the interaction owner. Move spawn/open/preflight, blocking teardown/reap and persistent I/O out of its dispatch path. Generalize the already nonblocking optional-worker pattern; prove failures remain contained.
3. Make provider failure a recoverable capability state. Prove quota exhaustion leaves local control alive; prove retry cannot duplicate an ambiguous action and cannot resurrect stale audio or motion.
4. Instrument CPU/elapsed budgets and resource high-water marks, then remove measured hot-path allocation/blocking. Apply ordinary-worker resource caps; preallocate/lock only critical working sets with a documented memory budget.
5. Qualify current Linux on the actual device under authorized load. Try affinity, scheduling and PREEMPT_RT only as separately measured changes. Require a hazard-derived physical timing requirement before proposing an MCU or power-control circuit.
6. Keep lampOS standalone: no required legacy Python HAL/Go server bridge. Run Rust formatting, clippy and applicable tests from a source-only copy for implementation changes; retain the old deployment only as an explicit temporary qualification/rollback option.

Verification for this research: read current source and primary upstream documentation; checked source anchors and Markdown references. No runtime edits, installs, on-device access, scheduling changes, commits or deployment were performed. Runtime tests and physical measurements are outside this document-only workstream.

## Source links

V2 links pin the audited commit. Board configuration links pin the independent repository snapshot inspected; they do not identify the current device image.

[board-image]: https://github.com/autonomous-ai/autonomous-os/blob/b8c23cc9916de3f074c34b8689134437d2ebb4b6/scripts/imager/build-orangepi.sh#L2
[board-audio]: https://github.com/autonomous-ai/autonomous-os/blob/b8c23cc9916de3f074c34b8689134437d2ebb4b6/robots/lamp/rootfs/etc/asound.conf#L1
[v2-ipc]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/ipc/src/lib.rs#L1
[v2-wire]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/wire.rs#L615
[v2-transport]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/transport.rs#L66
[v2-audio]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/audio/src/lib.rs#L32
[v2-budget]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/coordinator.rs#L75
[v2-audio-loop]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/audio_workers.rs#L36
[v2-gate]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/interaction/src/controller.rs#L225
[v2-capture]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/audio/src/capture.rs#L5
[v2-pcm]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/audio/src/pcm.rs#L1
[v2-audio-ownership]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/audio/src/ownership.rs#L1
[v2-privacy]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/physical_privacy.rs#L15
[v2-trace]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/service_trace.rs#L1
[v2-camera]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/coordinator/camera.rs#L1
[v2-process]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/process.rs#L289
[v2-provider-end]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/coordinator.rs#L2825
[v2-unit]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/deploy/4ace-attended/lamp-live-pilot%40.service#L1
[v2-motor-supervisor]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/motor/src/runtime/supervisor.rs#L1
[v2-motor-wire]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/motor/src/runtime/wire.rs#L191
[v2-stop]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/motor/src/control/physical_stop.rs#L1
[v2-refresh]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/coordinator.rs#L2730
[v2-encode]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/live/src/wire.rs#L577
[v2-motor-replace]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/motor/src/runtime/supervisor.rs#L263
[v2-motor-clocks]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/motor/src/runtime/supervisor.rs#L18
[v2-motor-native]: https://github.com/autonomous-ai/autonomous-os/blob/e1eae8de5ad65c1ec6c6450948afbcb510982240/lampOS/crates/motor/src/runtime/native.rs#L152
[linux-rt]: https://docs.kernel.org/core-api/real-time/theory.html
[linux-hardware]: https://docs.kernel.org/core-api/real-time/hardware.html
[linux-deadline]: https://docs.kernel.org/scheduler/sched-deadline.html
[linux-sched]: https://man7.org/linux/man-pages/man7/sched.7.html
[linux-sleep]: https://man7.org/linux/man-pages/man2/clock_nanosleep.2.html
[linux-mlock]: https://man7.org/linux/man-pages/man2/mlock.2.html
[linux-cgroup]: https://docs.kernel.org/admin-guide/cgroup-v2.html
[linux-timerlat]: https://docs.kernel.org/tools/rtla/rtla-timerlat-top.html
[linux-watchdog]: https://docs.kernel.org/watchdog/watchdog-api.html
[zephyr-sched]: https://docs.zephyrproject.org/latest/kernel/services/scheduling/index.html
[zephyr-isr]: https://docs.zephyrproject.org/latest/kernel/services/interrupts.html
[zephyr-work]: https://docs.zephyrproject.org/latest/kernel/services/threads/workqueue.html
[zephyr-slabs]: https://docs.zephyrproject.org/latest/kernel/memory_management/slabs.html
[zephyr-watchdog]: https://docs.zephyrproject.org/latest/services/task_wdt/index.html
[freertos-sched]: https://www.freertos.org/Documentation/02-Kernel/02-Kernel-features/01-Tasks-and-co-routines/04-Task-scheduling
[freertos-memory]: https://www.freertos.org/Documentation/02-Kernel/02-Kernel-features/09-Memory-management/03-Static-vs-Dynamic-memory-allocation
[freertos-mutex]: https://freertos.org/Real-time-embedded-RTOS-mutexes.html
[sel4-mcs]: https://docs.sel4.systems/Tutorials/mcs.html
[sel4-ipc]: https://docs.sel4.systems/Tutorials/ipc.html
[sel4-fault]: https://docs.sel4.systems/Tutorials/fault-handlers.html
[sel4-proof]: https://docs.sel4.systems/projects/sel4/verified-configurations.html
[rtic]: https://rtic.rs/2/book/en/by-example/resources.html
[embassy]: https://docs.embassy.dev/embassy-executor/git/std/index.html
[systemd-service]: https://github.com/systemd/systemd/blob/main/man/systemd.service.xml#L738
