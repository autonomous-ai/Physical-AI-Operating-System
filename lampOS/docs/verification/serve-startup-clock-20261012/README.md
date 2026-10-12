# Serve runner startup and expiry checks, 12 October 2026

Main composed Agent 2's exact six-file proposal through `32ca80f61` with one
narrow correction: every held cue samples the clock at its own scheduling
decision, after parsing. A batch cannot reuse a pong-handler entry time after
preemption. The regression accepts a still-fresh cue, then refuses the next
expired cue without publishing readiness. Original event times are preserved.

The underlying proposal excludes pre-session transport probes from clock mapping,
keeps relay reads active while the player prepares, and holds at most 64 unmapped
cues until their original freshness can be established. It never re-stamps a cue
or extends the 100 ms delivery bound. Held cues expire even if no map arrives.

From a standalone source export: formatting and strict all-target Clippy for
`lamp-voice-eval` pass. There are 39 passing focused tests: 24 physical-backend
boundary tests, slow-start and ordinary eight-turn loopback controls, and 13
final-dispatch controls. The release executable built successfully; its digest
and exact integrated file digests are in [results.json](results.json).

Two verification failures remain in the record. An earlier sandbox invocation
could not start its local child process; the same test passed outside the
sandbox. Main's first 45-second wrapper deadline was too short for the eight-turn
simulation (69.252 seconds on the successful run). That run was terminated, its
one orphan fake child stopped explicitly, and a fresh run passed with a
180-second external test deadline. No product deadline or test assertion changed.

This checkpoint has not yet driven a real Lamp conversation. The simulations
use cooperative processes and synthetic room datagrams; they are not proof of
natural interaction or acoustic timing. Device results remain separate.
