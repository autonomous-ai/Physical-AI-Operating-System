# Backend/player clock boundary audit

Source baseline: `740e1e892f974faa89eac47c4ac7f27e75aca038`.
All references in this table are **baseline** `lampOS/crates/voice-eval/src/physical/mac.rs` line numbers. `date` boundaries are recorded separately beside the gate logs. No scenario timing, existing assertion, fixture identity, workflow, profile or product policy changes.

## Every timing site

| Baseline site | Selected clock / reason |
|---|---|
| `await_ready` 110, 113; 2 ms receive poll | Host `Instant`; actual prepared-worker readiness watchdog remains 10 s. The idle callback still consumes transport using logical samples. |
| `HumanPromptPlayer::start` 235 | Unchanged host time; human prompting is not a clocked fixture. |
| `FakeRoomPlayer::start` 274 | Explicit pure callback after serialization/size validation, before the datagram. Default callback still samples `monotonic_us` once. Same send and report engine for both entry points. Checked µs→ns conversion now rejects overflow before any datagram. |
| `FakePreparedRoomPlayer` 346, 351–357, 373–374, 408–411 | Unchanged host timestamps, room-ready wait, deliberate fault delays, grant wait and fresh copy expiry check. This player is explicitly not migrated in this seam. |
| `ready_receipt` 807, 828, 831 | Host 1 s receipt-publication/read watchdog, unchanged. |
| `consume_transport` 1060, 1064, 1069 | Injected source for ping due check, immediate line drain deadline, held-cue expiry. Same fresh samples and order. |
| `room_ended` 1121; `lose_evidence` 1147 | Injected source for semantic coverage comparison and evidence-loss stamps. Actual `child.try_wait()` remains the observation of recorder exit. A controlled recorder must later share this domain; no recorder migration in this patch. |
| `grant` 1240, 1272 via `grant_clock_us` | Injected source for separate pre-send expiry and post-send receipt samples. Existing test-only override still takes precedence, preserving the independent grant review tests byte-for-byte. |
| `take` 1366 | Injected protocol-failure stamp; existing cue-failed early return still avoids sampling. |
| `send_ping` 1392 | Injected once immediately before original JSON serialization/write/flush; the original 250 ms next-ping offset remains. No new sample on missing session/stdin paths. |
| `handle` 1403; `release_held` 1444; `decide_cue` 1472 | Injected receipt sample at original entry, then a separate fresh sample for each successful freshness decision after parsing and map validation. Original receipt times are retained for held cues. No clock-map algorithm or cue lease change. |
| `wait_line` 1593 | Injected comparison decides the original `try_recv` versus host `recv_timeout(until-now)`. Shared receive engine retains framing/disconnection/end synthesis. This is not a scheduler and a controlled time source alone does not advance blocked fixtures. |
| `Drop` 1640; `wait_child` 1696 | Host process cleanup/exit watchdog, unchanged 2 s Drop fallback and 20 ms polls. Actual reap remains required. |
| `join_bounded` 1706, 1711–1713 | Host worker completion watchdog and unchanged 2 s cancellation grace/20 ms polls. |
| `begin` 1806 | Injected recorder-spawn diagnostic stamp only. |
| `begin` 1813–1815 | Host 15 s ready-file watchdog/50 ms polls, unchanged. |
| `Backend::now_us` 1914 | One pure injected read; no stepping or caching. |
| `next_event` 1924, 1930, 1937, 1941, 1947, 1959 | Injected held expiry, ping due, next ping/1 ms floor, 20 ms serve poll horizon, both deadline comparisons. Fresh sample order preserved. Existing channel wait implementation unchanged. |
| `runner_time` 1975 | Injected only when no original receipt time exists, preserving lazy fallback evaluation. |
| `arm` 2004 | Injected arming stamp before prepare; actual ready wait remains host time. |
| `inject` 2060–2062, 2070, 2078, 2086, 2110 | Injected target/remaining time, drain/held-cue checks, refusal and actual start-request samples. The existing ≤1 ms host sleep remains. Refusal samples moved to the argument of the same helper, after that call's other arguments and before receipt construction; no sample is shared with the earlier lateness check. |
| `finish` 2185, 2187 | Injected 10 s runtime self-end logical deadline and comparison. The runtime must actually report/end; no synthetic exit. |
| `finish` 2203, 2205 | Injected overall **pre-end** bound `(seconds + allowance_seconds + 30) s`. It still feeds `next_event` in the same domain. |
| `finish` 2212 | Explicit **host** post-end drain through `wait_line_on_host`. Same two host samples (deadline creation, receive comparison), same 300 ms per quiet receive and same receive/disconnection engine. A frozen fixture clock after the real runtime end cannot stall this drain. |
| `finish` 2227, 2231 | Host immediate cleanup of players after a failed start. |
| `finish` 2245 | Host 5 s transport exit bound; unchanged kill/reap behavior. |
| `finish` 2309 | Host 25 s aggregate playback join bound. |
| `finish` 2360 | Host 5 s unused-player join bound. |
| `finish` 2369 | Host recorder exit bound `seconds + 60` s. |
| Existing unit tests 2789 onward | All existing test modules and all 27 identities are byte-identical. The `grant_clock_us` helper fallback now invokes the backend source; its test override logic is unchanged. |

## New regressions

Five additional unit identities live in `physical/backend_clock_tests.rs`:

- Pure repeat reads stay at the same supplied instant; an explicit external set changes time; default backend sample stays between surrounding host samples.
- Startup receipt, ping, Pong, held-cue release and direct cue decision use the selected domain and distinct samples. Both cue receipt and fresh decision timestamps are asserted.
- A completed session with an open but silent channel consumes the unchanged 300 ms **host** drain and reaps its inert pipe consumer with the fixture clock frozen. The only fixture read is the original pre-end bound creation.
- Immediate fake delivery preserves every datagram byte and the report fields for repeated and advanced fixture instants; default construction still reports host time and the same payload.
- An unrepresentable nanosecond stamp fails before any room datagram is delivered.

The only launched process is the fixture's owned `/bin/cat`, with stdout/stderr discarded; closing its owned stdin ends it through existing backend cleanup. Room sockets live in private, short `/tmp/lve-clock-<pid>-<counter>` directories and are removed by fixture Drop. No device, network service or credentials are involved.

## Limitations

No stepping scheduler, control protocol, player/recorder migration, accelerated loopback test, or speedup claim. Pure clock reads cannot acknowledge another process's work. Root will run/review the remaining package and workspace gates and publish only its own branch. Native ARM64 remains unverified here.
