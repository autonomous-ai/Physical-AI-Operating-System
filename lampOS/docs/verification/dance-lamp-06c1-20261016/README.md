# Dance on lamp-06c1, 2026-10-16 (#999, #1012)

**What ran.**
- **The lamp:** lamp-06c1, shared by jarvis and el, with holds on #705.
- **The build:** `lamp-skills` `9b17a81a` and `lamp-voice` `8320db12`, both from ARM64 run 38853330680 (`0d568f60f`: `lamp-voice` + #984 + this pull request). The skill folder is `skills/dance` at that commit. Every other part is el's base install, build 1009 (`6d2bea2e1`). The voice it replaced, `6d5fbb6d`, is kept on the lamp.
- **The asks:** spoken by the Mac beside the lamp (`say -v Samantha -r 165`, output volume 36, about 1 m away). There was no person at the lamp.
- **How they were read:**
  - each ask's bus was recorded on the lamp with `lamp-bus record` for 40–50 s;
  - four camera stills were taken from the Mac during each dance, looked at, and deleted;
  - the numbers were worked out by a script kept outside the repository (below).

**Kept here.** One folder per ask, with `voice/state`, `voice/said`, `body/joint_states`, `body/hand`, `skills/events` and `light/state`, as recorded and not edited, plus `result.json`. `voice/heard` is left out: it carries room noise transcribed as words, which could be fragments of people talking.

**How `result.json` is worked out.**
- **Line end:** the first time `voice/state` leaves `speaking` after the skill's `triggered` event.
- **Command:** the first `body/joint_states` message after that whose `commanded` differs from the one before the line end by more than 0.005 rad on any joint.
- **Motion:** the same, on `position`, by more than 0.02 rad.
- **Swing:** the largest `position` change from that moment on, on any joint.
- The clocks are each message's `at_unix_ms`, so all of them come from the lamp's own clock.

| Ask | Said by the Mac | The model's call | Clip | Line end → command | → motion | Swing (rad) |
|---|---|---|---|---|---|---|
| 1 | "Lamp, dance for me." | none: taken as not for it | – | – | – | – |
| 2 | "Lamp, can you show me a dance?" | none: "I don't have a physical body to move around." | – | – | – | – |
| 3 | "Lamp, can you show me a dance?" | `dance({})` | jazz | 34 ms | 514 ms | 0.155 |
| 4 | "Lamp, dance for me, please." | `dance({})` | waltz | 213 ms | 533 ms | 0.212 |
| 5 | "Lamp, can you do a slow, calm dance?" | `dance({style: calm})`, and **again 28 s later** | waltz | 11 ms (see below) | 171 ms | 0.213 |
| 6 | "Lamp, give me a long, upbeat dance!" | none | – | – | – | – |
| 7 | "Lamp, dance to this song." | `dance({style: calm})` | waltz | 228 ms | 468 ms | 0.201 |
| 8 | "Lamp, I want a long dance. Make it upbeat!" | `dance({style: upbeat, long: true})` | groove, twice | 227 ms | 467 ms | 0.477 |

**What came out.**
- **When the tool was called:** five asks of eight. In every one, the arguments were the ones meant: none, calm, calm for "dance to this song", and upbeat and long. The second clip of ask 8's long dance was told the body but not played: it came at the first clip's length, while the first still played, and the body dropped it (`commanded` is flat from 9 s on; below, and the reviewer's 22:11 on #1012). Ask 18 is the fixed case.
- **Ask 2** came before a restart of the voice. Its session had opened at 13:13, before `lamp-skills` could write `tools.json` at 13:14, so the model was never told of `dance` and said it had no body. The voice reads the skills' tools only when a session opens.
- **Latency:** the body was told the clip within 300 ms of the line end in all five, and a joint passed 0.02 rad within 533 ms.
- **The look**, from the stills, of the lamp only:
  - jazz and waltz are a small sway of the head (about 0.15–0.21 rad), which reads as moving to music more than as a dance;
  - the long groove is clearly a dance: the head is blurred in its stills, and the swing is 0.477 rad.
- **The light:** each time it was "given back to on warm at 1: it already was". The lamp's own light was already warm at the lowest level, so the glow could not be seen.

**Failures.**
1. **The dance can begin while the line is still being said** (ask 5).
   - `voice/state` went `speaking` at +922 ms and `thinking` at +943 ms, and stayed `thinking` for 15 s.
   - The line, "Let me sway to a calmer rhythm.", was published on `voice/said` at +768 and +892 ms, and takes about 2 s to say.
   - So `after = "say"` saw the line end after 20 ms of `speaking`.
   - `voice/state` is not, by itself, the end of the lamp's sound.
2. **A second dance nobody asked for** (ask 5).
   - At +27 s a request heard only noise, which the transcript shows as "¿Qué?".
   - The model called `dance` again, said the same line, and the lamp danced again.
   - Ask 8 also said its line twice, the second time for a noise request ("¿Quién?").
3. **Three asks of eight called no tool:**
   - ask 1: taken as not for it;
   - ask 2: no tool told;
   - ask 6: no call, at the same distance and volume.

**The measuring script** (scratch, not committed). In Python, it reads each `.jsonl` file of an ask, decodes each line's `payload_b64`, and applies the rules above. That's all it does.

**Not shown.**
- No person at the lamp, and no judgement by a person of dancing against machine. The judgement above is jarvis's, from the stills and the numbers.
- The clip actually seen in a still was not matched frame by frame to the commanded one.
- Hands on the lamp mid-dance (a push stopping the second clip) were tested only against a stand-in body.
- The glow with the lamp's own light at a level where it can be seen.

## Evening rerun, 18:1x–18:3x

- **Ask 13–14:** with the person sitting at the desk, facing the lamp.
  - **The build:** `lamp-skills` `2cde32b7` (`55617e0ac`, thinking after speaking is still the line).
  - **Both got past the busy-room gate** that held back the 14:3x and 16:0x retries (voice requests 214, 217 and 11).
- **Ask 15–17:** with nobody at the lamp.
  - **The build:** `lamp-skills` `9afcecf8` (`771c0c187`, `[again]`; ARM64 run 38879731791), together with the skill folder at that commit. The voice was restarted after it.

| Ask | Said by the Mac | Call | What happened | Line end → command | → motion | Swing (rad) |
|---|---|---|---|---|---|---|
| 13 | "Lamp, can you do a slow, calm dance for me?" | `dance({})` | groove; "the turn over 2141 ms" after the speaking began | 239 ms | 479 ms | 0.242 |
| 14 | "Lamp, that was great, dance again!" | `dance({long: true})` | groove, twice | 138 ms | 218 ms | 0.244 |
| 15 | "Lamp, can you show me a dance?" | `dance({})` | chill | 198 ms | 358 ms | 0.218 |
| 16 | "Lamp, do it." (within 45 s of 15) | `dance({})` | **refused by `[again]`**: "You just danced. Dance again only if they ask for it; otherwise answer what they said." Nothing moved | – | – | – |
| 17 | "Lamp, one more dance, please!" (within 45 s of 15) | `dance({})` | jazz: "one more" is in `if_heard` | 196 ms | 437 ms | 0.342 |

**What changed since the afternoon:**
- The after-say wait now ends when the turn is over. Neither ask 13 nor ask 14 began mid-line.
- An unasked repeat is refused: ask 16, where the model called the tool on a line with no dance word.
- An asked repeat runs: asks 14 and 17.
- The model still left the style out of "a slow, calm dance" (ask 13), so it picked at random: groove.

**Not shown:** a noise request (as voice request 42 was) meeting `[again]` on the lamp. Ask 16 stands in for it, a call on words that don't ask for a dance.

## The review's long dance, 23:5x

After the reviewer's P2 on #1012 (a long dance played only its first clip: ask 8's `commanded` was flat after about 9 s), the fix is installed on lamp-06c1:
- **The builds:** `lamp-skills` `b3d24ecf` and voice `15ca4bad`, both from ARM64 run 38933439842 on `24df21d44`.
- **The fix:** the second clip is sent one clip length plus 100 ms after the first.

**Ask 18,** "Lamp, I want a long dance. Make it upbeat!", spoken by the Mac with nobody at the lamp:
- The model called `dance({style: upbeat, long: true})`.
- The body was told `dance_groove` twice.
- The clip was commanded 226 ms after the turn ended, and moved by 546 ms.

The largest `commanded` change from the pose at the line's end, per second after it (rad):

| Seconds | 0–8 | 8–16 | 16–24 |
|---|---|---|---|
| Largest change | 0.17–0.24 | 0.19–0.24 | 0.05–0.08 (the body's idle sway) |

**Both clips played:** the dance runs without a gap to about 16 s, two grooves of about 8 s each on this body. In ask 8 it was flat (≤ 0.08) from 9 s.

## #1012 merged; its second clip dropped, and #1199's timing, 02:58–03:33

**Ask 19, 02:58.** With `lamp-skills` `a69648ca` and voice `9360c5d2` (#1012's merged head `63d46d363`, ARM64 38953839401), "Lamp, I want a long dance. Make it upbeat!" called `dance({style: upbeat, long: true})`, and `skills/events` says `dance_hype` was told twice. The body played one:
- `lamp-body`'s journal: `02:58:07.621 gesture dance_hype`, `02:58:14.640 gesture over`, `02:58:18.660 gesture idle`, and no second.
- `commanded` moved 0.30–0.37 rad for 0–7 s after the line, then no more than 0.08.

#1012 timed the second clip from the first `commanded` change after the send, never sooner than from the send. That change can be the lean as the lamp speaks rather than the clip, so it is no sure sign of when the body began.

**Asks 22–23, 03:31–03:33.** Same words, with `lamp-skills` `ac50ab14` and voice `a338f9b1` (#1199's first fix, `b74b1168e`, ARM64 38960693512, which matched the direction of the clip's first move). Both played two clips:

| Ask | `lamp-body`: `gesture dance_groove` began | The first clip seen begun, after it was sent |
|---|---|---|
| 22 | 03:31:27.501, then 03:31:35.681 | 386 ms |
| 23 | 03:33:04.141, then 03:33:14.321 | 2,072 ms |

- **Ask 22:** the start was seen 386 ms after the send, and the second waited from then. The body began the first about 280–350 ms after the send (the reviewer's replay, 07:37 on #1199), so the send + the clip's length + 100 ms would have sent the second about 200 ms before the first ended, and it would have been dropped. Ask 22 played both only because this fix waited longer.
- **Ask 23:** the body began on time. The line ended at 03:33:03.991, `lamp-body` began `dance_groove` at 04.141, and `commanded` shoulder and elbow move from 04.2 on. The start was *seen* 2 s late, though: the send's pose was taken mid-lean (head pitch -0.463 to -0.495 across the line end), and the clip's first real move looked only about 0.62 like groove's first frame. So the second clip came 2 s late. The two starts are 10.18 s apart for an 8.0 s groove (ask 22: 8.18 s), and the lamp stood still for 2 s in the middle of the dance (the reviewer, 04:04 on #1199).

**So the start can be timed neither from the send nor from `body/joint_states`.** Asks 19, 22 and 23 show it: the body begins a clip some time after it is sent (later still when it rises), and the lean as the lamp speaks moves `commanded` before the clip does. #1199 therefore sends the second clip when the body says on `body/gesture` (#1230) that the first is `over`, sends nothing more after `dropped`, and nothing if no `over` comes within the clip's length and 3 s. That has not yet run on a lamp.

## `body/gesture` on lamp-06c1, 17 Oct 09:40–09:43 (#1230, #1199)

- **The build:** body `a2745bf2` and `lamp-skills` `a2be4894`, from ARM64 run 38994438345 on #1199's `eda137451`, which is stacked on #1230's `3fa2c76b7`. The voice was el's `35aa38b9`, untouched.
- **The calls:** `dance` was called straight on `/run/lampos-actions/skills/call.sock`, as the voice calls it, because the voice's busy-room gate had held back the spoken asks that morning. With no line spoken, the after-say wait ended at its 5 s ("the voice did not begin within 5 s"), and the clips followed.

| Run | Call | `body/gesture` (the body journal gives the same times) |
|---|---|---|
| `sock1` | `{long: true, style: upbeat}` | `dance_hype` began 09:40:23.284, over 30.303; began **30.324** (21 ms later), over 37.343 |
| `sock2` | `{long: true, style: calm}` | `dance_chill` began 09:42:32.663, over 41.663; began **41.704** (41 ms later), over 50.703 |

**What came out:** both long dances played both clips, the second starting within a body step or two of the first's `over`, with no still between them. That's the case asks 19, 22 and 23 got wrong.

**Not shown:**
- a spoken ask, held back by the busy room;
- `dropped` on the lamp (a pause or a push mid-dance; tested in `crates/body` and `crates/skills`);
- a folded lamp standing for the clip (#1080).
