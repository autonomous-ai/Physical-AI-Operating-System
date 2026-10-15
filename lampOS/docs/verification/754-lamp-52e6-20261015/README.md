# #754 second voice on lamp-52e6, 2026-10-15

What ran, and how the readings were taken. Nothing in this folder is edited.

## The lamp

- lamp-52e6, voice lent by fifth to sixth (lamp-fifth `718a4a4ab`, ended
  `bb1fddc2e`). Body, eyes, memory and the rest were fifth's builds, untouched.
- Build: `lamp-voice` from `sixth/second-voice` at the commit before
  `2cac0c322..cc935a7e9` was rebased onto `lamp-voice` (the same code,
  `2d7f1d2eb` before the rebase). It was cross-built on a Mac with
  `cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.36 -p lamp-voice`,
  SHA-256 `9d10737d6cfcc82c1b6e7b111474701fe98df0d8ce173227d542385d1e988af9`, and installed under
  `/opt/lampOS/voice/<sha256>/`. It ran like the Docker build before it:
  all six workers up, `ldd` found every library.
- A drop-in `lamp-voice.service.d/sixth-754.conf` started that build with
  the recorded flags, a copy of `provider.json` whose `endpoint` was a closed
  local port (`wss://127.0.0.1:9/...`), so Gemini could not be reached, and
  `LAMP_VOICE_SECOND_CONFIG=/etc/lampOS/voice/second.json`:
  the company relay for speech to text, `gpt-4.1-mini` for the text model,
  and `elevenlabs` speech (the relay's OpenAI speech does not stream; see
  `docs/second-voice.md`). The coordinator recorded
  `voice_route_switched to=second why=live_link_not_ready` at startup.
- Afterwards the drop-in and files were removed, the recorded unit restarted,
  and `/etc/lampOS/voice` compared identical to its backup.

## How the turns were played

A Mac beside the lamp rendered each question with `say -v Daniel`, played it
with `afplay`, and followed the voice's event files over ssh: a copy of the
runner `lamp_v2_convo.py` (kept outside the repository, lampOS is Rust only)
with a group of four questions, run twice:

1. "Hey Lamp, what is the capital of France?"
2. "And how many people live there?"
3. "Hey Lamp, dim your light a little, please."
4. "Hey Lamp, name one yellow fruit."

The office around the lamp was busy with people talking throughout.

## Files

- `events.jsonl`: the session's event records (all `events-*.jsonl` of the
  voice session, as written).
- `journal.txt`: `journalctl -u lamp-voice` for the run, with the
  "lamp-voice second:" line of each turn.

## What came out

All eight questions were answered through the second route, in the
character's voice and words. The second answered from the first ("About 2.1
million people live in Paris"). Both light questions reached the executive
(`light_dimmer` in 42 ms; `light_on` in 32 ms, speech to text having heard
"Give me your light a little"). The announcement was said once, before the
first answer.

From the end of the person's line to the lamp's first write to the speaker
(the runner's `play_end_to_first_write_ms`, Mac playback end to the lamp's
`speaker_first_write`, clocks matched over ssh): 2988, 3557, 12552, 3454,
9889, 5009, 6493, 3047 ms. Median 4.3 s; without a tool call or a slow chat,
3.0 to 3.6 s. From the journal, per turn: the last words came 0.58 to 0.91 s
after the line; the chat's first event 1.2 to 1.5 s later on six turns and
2.0, 2.6, 5.6 and 7.7 s on the others; the first answer audio 0.36 to 0.61 s
after that. A tool call adds a second chat round (2.2 and 4.7 s).

## What was not shown

- The agreed latency targets are **not met**: steady p50 ≤ 1.5 s and the
  sentence within 1.2 s. The chat through the relay (1.2 s at best, spikes to
  7.7 s) and the last words from speech to text (0.6 to 0.9 s) are most of it.
- A switch in the middle of a line (the replay) was not played on the lamp;
  host tests cover it (`coordinator/tests/second_route.rs`).
- Gemini coming back mid-run was not played on the lamp (host tests cover the
  return).
- Earlier runs the same afternoon, which found the faults since mended
  (the lamp's name misheard, no history, the OpenAI speech not streaming, the
  level 6 dB low), are described in the pull request, not kept here.
