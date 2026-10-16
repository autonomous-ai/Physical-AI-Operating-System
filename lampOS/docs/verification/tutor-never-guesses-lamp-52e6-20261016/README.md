# Desk tutor never guesses a page it cannot read: lamp-52e6, 2026-10-16

For #941, item 1 of the first builder's 13:29: "If the still doesn't show legible writing, it says what it can't read and asks for the page closer."

**How.** `lamp-gemini`'s `service_probe` was run in `image` mode on lamp-52e6 with the lamp's own `provider.json` (Gemini Live). It opens its own session, and the voice, eyes and body services were not touched.
- **Question:** the spoken WAV "Lamp, can you check my work?" (`say -v Daniel`, 16 kHz mono).
- **Instruction** (`LAMP_PROBE_INSTRUCTION_FILE`, new in this change): `deploy/character/lamp.txt`, then Desk tutor's `SKILL.md` body (#1002), then its say note, as lamp-skills hands it to the voice.
- **Picture:** one JPEG, as the full look sends it. `page.html` (in this folder) is a handwritten-style maths page with one line wrong (`48 ÷ 6 + 2 × 4 = 40`), rendered at 1280×960 and scaled to 1024 wide:
  - **sharp:** 59,597 B;
  - **blurred** (`ffmpeg boxblur=12:2`): 18,183 B, with no digit legible.

```sh
sudo LAMP_PROBE_INSTRUCTION_FILE=instruction.txt ./service_probe /etc/lampOS/voice/provider.json q.wav image blur.jpg
```

**What the model said** (one image sent each time):

| Picture | Answer |
|---|---|
| blurred | "Let me look." (and nothing more in the 40 s window) |
| blurred | "Let me look. I can't quite make that out, it's a bit blurry. Could you hold the page a little closer or make sure it's in focus?" |
| sharp | "Let me look. Your first equation is perfectly correct, and so is the third. For the second one, just watch your order of operations—you'll want to do your division and multiplication before adding." |

**Reading**
- On the blurred page it never guessed. Once it said it could not read the page and asked for it closer; once it said only "Let me look." In the voice that line starts a full look, but the probe sends none, so the turn ends there.
- The sharp control shows the same instruction does read and tutor: lines 1 and 3 right, line 2 found wrong, a hint and not the answer.
- In #987's screen test, the 3264 still did invent content. That was without Desk tutor's note; with the note, the blurred still is refused. Two runs are not a rate.

**Not shown:** a real paper page under the lean (#1011, #1051) at 1280×960. That is the owner's 15:00 run on lamp-4ace.

No picture of a person was taken. The pictures are of a rendered page and are not kept here; `page.html` makes them again.
