# A hello asking the policy, on the development Mac (#752, 2026-10-15)

The second builder's evidence for the hello asking the policy
([speaking-first.md](../../speaking-first.md#asking-the-policy-752),
[policy.md](../../policy.md#hello-on-arrival-say-hello-now-752)). **A
laptop's, not a lamp's.** No lamp, no camera, no model: the policy's judge
is a stand-in that answers at once.

Apple M2 Max, macOS 26.6.2, aarch64, Rust 1.96.0 pinned, release build;
shared with other sessions (load average about 3 to 5).

    cargo run --release -p lamp-voice --example hello_wait -- --samples 30

Thirty arrivals, each a new person after six minutes away, there three
seconds: the voice's `Arrival` says the arrival on `voice/arrival` on its
first ready call and is called every 2 ms, as its loop calls it, until it
has the answer; the policy's own loop (`run_with_questions`, `--hello`,
acting) decides with the stand-in and writes its log line. Then the same
arrival without asking.

| Run | Held for the policy: median | first | largest | By the rule alone |
|---|---|---|---|---|
| [1](hello-wait-run1.json) | 31.6 ms | 33.2 ms | 38.0 ms | 0.0 ms |
| [2](hello-wait-run2.json) | 22.4 ms | 34.8 ms | 36.3 ms | 0.0 ms |

Most of it is the two loops' own waits (the policy's 5 to 20 ms, the
voice's 2 ms) and the bus. What this does not say: Jev's own time from the
lamp (0.5 to 1 s for the attention question; the hello question's asking is
cut at 1.5 s), the coordinator's own loop around `Arrival::hello`, and
anything of a real person sitting down.
