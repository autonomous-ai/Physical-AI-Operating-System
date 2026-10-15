# #803 marking a bad moment, on lamp-52e6, 2026-10-15

- Voice lent by fifth (lamp-fifth `9de2b39ce`). Build: `lamp-voice` from
  `sixth/mark-weird` at `1dc895981`, cross-built with `cargo zigbuild
  --release --target aarch64-unknown-linux-gnu.2.36`, SHA-256
  `6466d1702b0154103625fda676f3c11a2128c31643ca815f77236d2bebb1a8b3`. It ran under
  `/opt/lampOS/voice/<sha256>/` from a drop-in with the recorded flags and
  `LAMP_MARK_WEIRD=on`. The provider worker was started with
  `--mark-weird /var/lib/lampOS/voice/sessions/e8589cb687b54f7ca22c7b41ce958d1b`.
- Played from a Mac beside the lamp (`say -v Daniel`), in a busy office:
  "Hey Lamp, what is the capital of Italy?" ("It's Rome."), then a mark.
  - First try, "Lamp, that was weird.": no mark. The turn was split by the
    turn-taking into two requests; the model's call came for the superseded
    one and was dropped ("dropped a late provider event (UnownedToolCall)").
  - Second try, "Hey Lamp, that answer was weird.": the lamp said "Thanks,
    I'll learn from that." 3.9 s after the line. Journal:
    `lamp-voice: marked for review: that answer was weird, session e8589cb687b54f7ca22c7b41ce958d1b, last 60 s`
    and `request 12 asked for mark_for_review; the marks answered in 24 ms;
    the answer was sent`.
- The mark: `/var/lib/lampOS/voice/marked/1792071701185-e8589cb687b54f7ca22c7b41ce958d1b/`.
  The folders were mode 700 and the files mode 600. `mark.json` had a window of
  16023110038 to 16083110038 µs (60 s) and 196 events. `events.jsonl` held 196
  lines from 16023424763 to 16082275046, all inside the window. The kinds were
  `reference_clock` 59, `hand_controls_health` 59, `ring_feedback` 27,
  `ring_requested` 21, then three each of the turn records.
- `lamp-voice marked` printed:
  `2026-10-15 20:41 · that answer was weird · session e8589cb687b54f7ca22c7b41ce958d1b · last 60 s · 196 records · /var/lib/lampOS/voice/marked/1792071701185-e8589cb687b54f7ca22c7b41ce958d1b`.
- The marked records are not kept here: they hold transcripts of an office
  where other people were talking. Afterwards the drop-in and `marked/`
  were removed, and the voice was restarted on its recorded unit
  (`releases/e4f1d32b…`).
- Not shown on a lamp: the 7-day sweep (host test only).
