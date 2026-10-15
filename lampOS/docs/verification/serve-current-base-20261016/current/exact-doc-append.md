
## Coordinated Serve-session fixtures (#823)

`ClockedServeFixture` coordinates the real relay process, fake Serve runtime,
prepared-player threads, optional fake recorder and delayed local transport.
All fourteen original `serve_session` identities use this explicit clock,
including their twenty-two attempts. Their complete test bodies, 114 assertion
macros and original fault inputs remain unchanged. Ordinary fake commands keep
their host clocks; control requires the explicit `fake-clock-serve-session`,
`fake-clock-serve-runtime`, `fake-clock-room-recorder` and
`fake-clock-slow-transport` commands.

Clock reads remain pure. Advances retain the original 2 ms poll boundaries and
require actual completed work and counted pipe, cue and room-input progress.
Prepared readiness and expiring grants still use the original channels, real
room datagrams and 100 ms lateness bound. A continuing relay's successful
SIGTERM send requires actual runtime signal observation at the next original
poll. A completed terminal relay stops further runtime steps, including after
forced kill. Runtime completion holds logical time during final metadata,
relay completion and child exit.
A terminal progress receipt does not prove an OS process or prepared worker has
exited. Bounded host guards, real kill/reap and thread completion remain separate.

The recorder adapter uses the existing parser and clock/wait entry point. Its
first progress receipt follows the actual `ready.json` write; the backend still
reads that file through its bounded parser under the original 15 s readiness
guard. The three recorder controls retain their full 6 s, 6 s and 23 s waits.
The optional declared 20 s capture end is not an exit receipt. A terminal
recorder acknowledgement freezes time until the backend observes its owned
`Child` exit; recording coverage and validity still use the original checks.
After relay completion, recorder and prepared-player work can drain at the
original 2 ms cadence, with recorder exit confirmed before player completion.

The delayed-transport control retains the entire 400 ms delay and 2300 ms first
preparation. Its wrapper neither consumes queued stdin nor fabricates stdout.
After acknowledging the completed delay it closes its control participant and
performs the original exec with the original arguments and inherited streams.
The controller separately requires a fresh ServeRelay registration from that
real relay before runtime bootstrap. The delay acknowledgement alone cannot
establish exec, relay readiness or child exit. Startup uses one cumulative host
deadline; later logical rounds cannot reset it, and a shorter preparation
deadline still wins. Recorder due times also bound each logical advance.

This migration covers the original Serve-session family. The six separate
direct `serve_relay` tests retain their existing path. Clock-controlled results
check ordering and the full configured windows; they do not measure hardware
latency or establish a result for the complete original 123-test inventory.
