# Voice owner's first checks — 2026-10-12

Agent 1 took over voice from Main on this day
([handoff](../../../coordination/voice-handoff-20261012.md)). This is what was
checked and run before anything on the branch was changed. Host, emulated
ARM, lab replay and lamp results are kept apart. No acoustic score is given:
nobody listened to a recording.

## The candidate on this branch

`lamp-voice` is canonical `4b0787792` plus two commits: Main's 45-path
composition (`6ff31ca1d`: the `mic1` capture hold and the opt-in Smart Turn
endpoint worker) and its seven-file documentation overlay (`ecd6251ac`).

- Both patches were applied to a clean export of `7e28dc7ac` and of
  `4b0787792`; the result is byte-identical to the source Main handed over,
  apart from the two handoff files canonical added in between.
- The five `mic1` Rust files are byte-identical to `lamp-a1-mic1-reference`.

## Host gate

macOS x86_64, rustc 1.96.0, two jobs, from a fresh export and a fresh target
directory, 09:56:10Z to 10:23:19Z. Every step exited 0.

| Step | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | clean |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 1,817 passed, 0 failed, 13 ignored |

The 1,817 are 1,805 in 107 result lines and 12 in the endpoint process test,
which has its own harness. Of the 13 ignored, six need the pinned model, one
needs a built endpoint worker and the rest are long or timed runs.
Linux-only code is not compiled on this host: the capture worker and the
ALSA device code are covered by Main's ARM gate, not by this one.

## The one failed test in the ARM gate is the emulator

`missing_executable_fails_at_once` failed under QEMU with
`worker closed its output` and passed on the lamp
([Main's record](../endpoint-native-20261012/README.md)). Cause: under
`qemu-aarch64-static` 7.2.22, starting a program that does not exist reports
success.

| | Native x86_64 Linux | aarch64 under QEMU |
|---|---|---|
| `Command::spawn` of a missing program | `Err(No such file or directory)`, 0.17 to 0.22 ms | `Ok(pid)`, 3.3 to 3.4 ms |
| Child's output | none | end of file at once |
| Child's exit | none | status 127 |
| Runs | 3 of 3 | 3 of 3 |

The probe built its command as `start()` in
`crates/endpoint-ipc/src/client.rs` does: absolute path, no pre-exec hook,
cleared environment, piped input and output. The emulator's own trace shows
`clone(CLONE_VM|CLONE_VFORK|...)`, the child's `execve` failing with
`ENOENT`, `exit_group(127)`, and the parent told nothing. So the client sees
a worker that started and then closed its output, which is what it reported.
The second failure Main recorded, the 20 ms assertion in a syscall-traced
rerun, is the trace's own cost: 26.1 ms pass between the two clock reads
around the classifier's thread start in that log, and the untraced test took
10 ms in all. That part was read from Main's log and not measured again.

The test cannot pass under this emulator as written. Nothing was changed.

## On the lamp, from the voice owner's hands

Lamp `4ace`, boot `76a7af25`. Runtime `lamp-live` SHA-256 `33f2eb21...9800`,
the `mic1` capture-hold build: not the composed `7e5a...` binary, which has
still not run live. Flags as in Main's Gemini runs: `directed`, duplex,
`--microphone mic1 --reference-clock capture`, kept-start continuation,
diagnostics on. Main's handover and rollback were used unchanged under fresh
names; every run rolled back verified, with the legacy services restored, the
motor port masked and the mixers as they were.

| Run (UTC) | What | Outcome |
|---|---|---|
| 10:54:10 `mic1-hello-noz78tgr` | one cached question | refused at stage: the legacy stack had been unmuted by a tap on the touch sensor 26 s after it was muted. Nothing was stopped |
| 10:55:26 to 10:57:14 `mic1-hello-odt3pcrf` | the same | answered. Ready at 1.89 s, input admitted 4.43 s, end decided 8.94 s after 1,200 ms of quiet, first ALSA write 10.14 s, reply retired 13.80 s, no playback gap. The input transcript lacks the question's opening clause. A second input at 63.48 s ended the run by the kit's one-prompt rule, which is why its status reads incomplete |
| 11:00:25 to 11:04:06 `mic1-talk-4ws69fg7` | 180 s with a person in the room, no played prompt | collected. Nine inputs admitted, six replies spoken, three turns replaced before any output because the speaker went on, one cut by the session's end. First ALSA write 1.30 to 1.48 s after each end-of-speech decision. One playback gap, in the first reply. No yield while the lamp spoke |

In the 180-second run the lamp's microphone was at -51 to -56 dBFS from 13 s
to 95 s with at most two blocks in ten seconds scored as speech, and the lamp
said nothing. From 95 s on there was continuous speech in the room, in
Vietnamese, and the lamp answered it in Vietnamese. Whether that speech was
addressed to the lamp is not known; no transcript text is kept here. A VM
build of Main's ran on the Mac during this run; the room recording passed its
integrity checks.

Timing here is software timing: from the runtime's decision that speech had
ended, which itself waits 1,200 ms, to the first write the speaker accepted.

## Found, not yet fixed

- **The start of a reply after a pause is barely cancelled on `mic1`.** In
  Main's lower-speaker session the second reply, 16.4 s after the first, was
  cancelled by 0.7 to 12.5 dB for 1.75 s and by 28 to 41 dB after that. The
  reference clock controller reads the render 133 ppm fast at the start of a
  session and still 57 ppm fast after 40 s, although speaker and microphone
  agree to about 5 ppm: it steers what it observes to 1,200 frames while the
  ring starts at 1,200 and the observation includes about 130 frames more.
  The render slid 32 frames (1.33 ms) across that pause. In a lab replay of
  the same recording through the same canceller, the reply is cancelled by
  10.6 to 11.1 dB with the slide and by 25.1 to 25.7 dB without it; a single
  step of 18 frames either way is enough to cause it, 9 frames one way
  already halves the cancellation, and 6 does nothing. One lamp
  observation, reproduced in the lab; `mic2` not separated. A first draft of
  a change still leaves 16 to 23 frames of slide per session and breaks one
  existing test: not proposed yet.
- **The first reply of a session** lets the lamp's voice through 5 to 13 dB
  down for half a second, about half a second in. That is not the slide: it
  happens with eleven of twelve constant offsets. Cause not found.
- **Session resumption through the relay is unverified.** The runtime asks
  for it; V1 turned it off with a comment that the relay does not forward it.
  The trace has no session-wide count of resumption updates.
- Open as before: the opening words of a request, speech that is not for the
  lamp, a person heard over the lamp, and the 2.5 to 2.7 s from the last word
  to the reply.

## Not done

No ARM gate was run by Agent 1 on this branch: Main's gate was on the same
code. Nothing ran live on the composed binary or with the endpoint worker.
