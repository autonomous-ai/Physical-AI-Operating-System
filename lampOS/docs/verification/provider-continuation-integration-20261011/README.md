# Optional provider continuation integration

The combined Rust runtime can now retain a resumed question until the earlier
response produces output or settles, instead of immediately requesting an
interruption before any answer exists. `--provider-continuation kept-start`
selects a 2,500 ms wait, the existing strict `Require` settlement policy, and
barrier timing from the successor's commit. The default remains `off`.

This composes Agent 1's `b19cadd09` code and `8ab02d2c6` documentation onto Main's
current source, preserving the F1 acknowledgment deadline and prepared player.
The final retirement check closes the reproduced awaited-write cancellation
race. Fixture mode rejects the option before worker startup; worker tests now
cover successful continuation and Stop/mute under the actual coordinated
contract. The private shell probe wrapper is excluded; the documentation gives
direct commands for the Rust example.

Main verified a standalone source export: format, strict all-target Gemini/live
Clippy, and **697 library tests passed**, zero failed, six existing ignored.
Linux/ARM64 emulation passed format, the same strict package lint, **42 selected
tests**, zero failed/ignored, and the release link. Exact commands, source and
artifact hashes are in [results.json](results.json). This is not a fresh full
workspace run, cloud qualification, or native timing measurement.

Kept audio has a deliberate bounded age extension inside the session. Stop and
privacy closure discard unsent buffered work. A request retired after its End
was handed to the WebSocket cannot withdraw those bytes. More subtly, a
cancelled retained request's audio already accepted into an open activity may
still be heard when a later request commits it; the explicit distinction between
continuation and genuine discard remains separate work. The option is therefore
experimental and is not enabled in the installed default.

The earlier [three service probes](../continuation-service-20261011/README.md)
provide one timeout and two strict passes on an earlier proposal. They do not
qualify this combined microphone-to-speaker path. The next native check is an
ordinary-conversation control with the option explicitly selected; it cannot
claim interruption success without actually exercising that event.
