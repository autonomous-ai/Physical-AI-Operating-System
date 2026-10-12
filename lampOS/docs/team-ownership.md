# Proposed subsystem ownership from V1 history

Reviewed 2026-10-11 against V1 `origin/main` at
`d5efe9d7b73cc529b34cd4abe97624682a82ca94`. These are proposed assignments based on
relevant Git changes, not confirmed availability, Rust proficiency or a ranking
by commit count. Source histories overlap; one person should not own every area
they have touched. The owner assigns people. Existing agents retain current
file ownership until a handoff is agreed.

| Suggested person | Primary V2 responsibility | Concrete V1 evidence |
|---|---|---|
| Leo (`leo-super-dev`) | Small core, worker integration and ownership/cancellation contracts; review cross-component changes. | [Protect capture from passive speech](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/0a21a0817cef86ba09ea58d461e2be98673c30ad); [stale LED/camera ownership](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/d5efe9d7b73cc529b34cd4abe97624682a82ca94). |
| `ndtran11-png` | Audio capture/playback, echo cancellation and natural interruption, paired with core review. | [AEC and barge-in](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/259b9ad61ce6e96d5a3f2561ff1b63d6535788ed). |
| `loclt1011` | Camera, perception and fresh visual observations for desk interaction. | [Gaze repoint preserves desk presence](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/b12c155c858247954dd00d336be3b5d631ebc497). |
| Kyle (`kyle-aus`) | Motor bus, hardware safety, calibration and physical input drivers. | [Gate raw movement during overload and retry torque-off](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/f349443e7f89d515c3a59aa7ca8cfb2380a44beb). Physical safety behavior still needs per-device review. |
| Darren (`darren101004`) | Interaction behavior and choreography: presence, sleep/wake, purposeful ring/head/body cues. | [Suspend presence timing while asleep and restart on wake](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/6dccfb2e0154e226d1b5f98923f7c072c6bddb4c). |
| Elvis / `69tc` | Installation, service lifecycle, board configuration and deployment. | [Orange Pi image clock initialization](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/8a90855c85b61e3c59a4bfd44c0b37f3d5680562); [device proxy integration](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/6339e59903929f00d0f3f740fdb3429f6215e31f). |
| Tony (`jolhada`) | Desk setup/calibration experience and eventual setup UI, when needed for the release. | [Setup flow](https://github.com/autonomous-ai/Physical-AI-Operating-System/commit/bfbd25b44cf5621e0c6a350425ced40ca23e84bd). This evidence is older; confirm current availability. |

Names are the commit author names with verified GitHub account associations;
`Leo`/`leo` and `elvis`/`69tc` are grouped by those associations. No personal
email address is needed for this assignment.

Keep one integration owner and one person accountable for whole-interaction
acceptance. The history inspected does not establish a dedicated QA owner, so
that assignment remains open. Everyone must provide component tests and
failure/latency evidence; the integration owner runs shared real-Lamp scenarios.

The current Claude Code agents remain focused on speech continuity and endpoint
detection; Main owns integration and device scheduling. Environment sensing is
lower priority than completing voice. Do not start a Harness or memory team for
this first release: those remain future boundaries in the architecture.

For every subsystem handoff, define its owned process/files, bounded messages,
readiness, crash behavior, privacy/stop response, measurable latency boundary and
device test. Use independent processes where they isolate work or failure, with
one writer per shared hardware bus. A Rust crate alone is not crash isolation.

## Named V2 architecture map

The [ASCII architecture](architecture.md#runtime-responsibilities) now puts these
proposed owners directly beside each core responsibility and worker boundary.
Darren owns proposed expressive behavior and ring presentation; Leo reviews ring
driver contracts, whose V1 history is predominantly his. `ndtran11-png` owns the
proposed audio/provider/end-of-turn workstream, and `loclt1011` owns camera and
perception; neither account has been expanded into an unverified personal name.
Kyle owns the proposed physical controls and enabled sensor drivers as well as
the single motor-bus writer. A dedicated whole-interaction QA owner and future
Harness/memory owners remain explicitly unassigned. This is a staffing proposal,
not a change to current agent file ownership or an implementation-complete claim.
