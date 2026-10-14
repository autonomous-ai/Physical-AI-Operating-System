"""Step 4 of make.sh: the name's training inputs, as `lamp-name` would give them to the model.

    python make_data.py words|talk|check --part I/K

As in training/stop (its make_data.py): each example is 2.59 s, nine inputs
80 ms apart; an input is a call when the name ended 0.02 to 0.74 s before
the end of what it sees; left out while the name is still being said or
ended 0.74 to 1.2 s before; 0 otherwise. "The name ended" is the end of
"Lamp" in "Hey Lamp" and a bare "Lamp", and the end of "ơi" in "Lamp ơi".

Every short clip, call or not, is placed the same way: its name or word
ending 0.02 to 0.10 s before the end of the first input's sound, with
quiet or other sound before it, as a person calls after a silence. What
follows is the rest of the same line (generated), or, for the cut MSWC
words, nothing in half and in half another clip of talk after a pause.

What is done to the person's voice, at random: the first tenth of a second
after each silence lost (half), a loudspeaker's muffling (two in five), a
room (three in ten); a level from -40 to -12 dBFS. Around it: nothing but
the microphone's floor (two in five), other talk 10 to 30 dB under (one in
three), other talk 0 to 10 dB under (one in ten), noise 10 to 30 dB under
(the rest).

"words": the calls and every other short clip. "talk": 13 h of long stretches of
LibriSpeech readers, generated voices, the lamp-like replies, generated
talk read with a Vietnamese accent, and FLEURS Vietnamese, alone or two at
once, all 0. "check": calls by the generated
voices held back from training (the number 3 more than a multiple of 8),
for validation; the talk to check against is training/stop's check-talk.

Two things tried in the second round (docs/bare-name.md, "What was tried")
and not in the model, each off unless its switch is set:

- NAME_KOKORO=on: Kokoro's calls, near misses and replies among the
  words, its talk among the talk around them, "talk-kokoro" (an hour of
  its talk and replies), and no reply that starts with the name ("Lamp
  here, ...") taken as a negative.
- NAME_VOICE_CHANGE=on: half the clips played 15 % slower to 15 % faster,
  and half the voices given one to three bumps or dips (microphone.py).

With neither set, the inputs are task 5's, made the same way.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path

import numpy as np
from scipy.signal import resample_poly

import shared
from features import STEP, WINDOW, FrontEnd, stacks
from microphone import clip_onsets, colour, loudspeaker
from mix import RATE, active_level, gain_for, mixture, place, room
from store import Store

INPUTS = 9
LENGTH = WINDOW + STEP * (INPUTS - 1)
SEEN_UNTIL = (STEP * np.arange(INPUTS) + 31_712) / RATE
ALONE = 99.0
# The way each call was made, kept with every input: 1 "Hey Lamp", 2 "Lamp ơi", 3 a bare "Lamp".
WAYS = {"hey": 1, "oi": 2, "bare": 3}
# Where the inputs go: lampOS/.cache/name/inputs, or NAME_INPUTS.
OUT = Path(os.environ.get("NAME_INPUTS", str(shared.INPUTS)))
# The two switches above.
KOKORO = os.environ.get("NAME_KOKORO", "off") == "on"
VOICE_CHANGE = os.environ.get("NAME_VOICE_CHANGE", "off") == "on"


def kokoro_sets(name: str, keep=lambda m: True) -> list:
    """Kokoro's set of that name, with NAME_KOKORO=on, as a (store, keep) pair."""
    return [(Store(name, shared.CLIPS), keep)] if KOKORO else []


def labels(ends: list[tuple[float, float]]) -> np.ndarray:
    out = np.zeros(INPUTS, np.int8)
    for j, seen in enumerate(SEEN_UNTIL):
        for start, end in ends:
            since = seen - end
            if 0.02 <= since <= 0.74:
                out[j] = 1
                break
            if (since < 0.02 and seen > start) or 0.74 < since <= 1.2:
                out[j] = -1
    return out


def validation_voice(meta: dict) -> bool:
    """Generated voices held back from training for validation."""
    s = meta.get("speaker", "")
    return s.startswith("piper-") and int(s.split("-")[1]) % 8 == 3


def held_out_reader(meta: dict) -> bool:
    """LibriSpeech dev readers training/stop keeps back for validation."""
    return meta.get("speaker", "").startswith("libri-") and meta["speaker"].endswith("3")


class Voices:
    """Stretches of talk, taken from sets of clips one after another."""

    def __init__(self, sets: list[tuple[Store, object]]):
        self.items = [(store, i) for store, keep in sets for i, m in enumerate(store.meta) if keep(m)]

    def stretch(self, rng: np.random.Generator, length: int) -> np.ndarray:
        parts, have = [], 0
        while have < length + RATE:
            store, i = self.items[rng.integers(len(self.items))]
            clip = store.audio(i)
            gap = np.zeros(int(rng.uniform(0.05, 0.6) * RATE), np.float32)
            parts += [clip, gap]
            have += len(clip) + len(gap)
        whole = np.concatenate(parts)
        at = rng.integers(0, len(whole) - length)
        return whole[at : at + length]


def scaled(x: np.ndarray, level: float, wanted: float) -> np.ndarray:
    return x * gain_for(level, wanted)


def faster(clip: np.ndarray, anchor: float | None, rng: np.random.Generator):
    """The clip played 15 % slower to 15 % faster (pitch and formants move with it, as a
    smaller or larger speaker's would), and where its name ends then."""
    down = int(rng.integers(85, 116))
    y = resample_poly(clip, 100, down).astype(np.float32)
    return y, (anchor * 100 / down if anchor is not None else None)


def lamp_says_its_name(meta: dict) -> bool:
    """A reply that starts with the name, as a call does ("Lamp here, ..."): not a negative."""
    return meta.get("what") == "reply" and meta.get("text", "").startswith("Lamp")


def person_voice(x: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    """What the lamp's microphone and the room may do to the person's voice."""
    if rng.random() < 0.3:
        x = room(x, rng)
    if rng.random() < 0.4:
        x = loudspeaker(x, rng)
    if rng.random() < 0.5:
        x = clip_onsets(x, rng)
    if VOICE_CHANGE and rng.random() < 0.5:
        x = colour(x, rng)
    return x


def example(clip, anchor, call, after, talk, noise, rng):
    """One example around a short clip whose name or word ends at `anchor` seconds into it."""
    if anchor is None:
        at = int(rng.integers(-len(clip) // 2, LENGTH - len(clip) // 2))
        ends = []
    else:
        end_at = SEEN_UNTIL[0] - rng.uniform(0.02, 0.10)
        at = int(round((end_at - anchor) * RATE))
        ends = [(end_at - 0.3, end_at)]
    person = place(clip, at, LENGTH)
    if after is not None:
        # Another clip of talk after a pause, as the rest of what was said.
        start = at + len(clip) + int(rng.uniform(0.05, 0.35) * RATE)
        person = person + place(scaled(after, active_level(after), active_level(clip)), start, LENGTH)
    person = person_voice(person, rng)
    level = rng.uniform(-40, -12)
    person = scaled(person, active_level(person), level)
    choice = rng.random()
    other, relative = None, ALONE
    if choice < 0.4:
        pass
    elif choice < 0.73:
        relative = -rng.uniform(10, 30)
    elif choice < 0.83:
        relative = -rng.uniform(0, 10)
    else:
        n = noise.stretch(rng, LENGTH)
        other = scaled(n, active_level(n), level - rng.uniform(10, 30))
    if other is None and relative != ALONE:
        o = talk.stretch(rng, LENGTH)
        other = scaled(o, active_level(o), level + relative)
    sound = mixture(person, other, rng)
    y = labels(ends) if call else np.zeros(INPUTS, np.int8)
    return sound, y, relative


def words(part: int, parts: int, check: bool) -> None:
    front = FrontEnd(shared.STOP_CACHE / "oww", threads=int(os.environ.get("OMP_NUM_THREADS", "2")))
    root = shared.CLIPS
    talk = Voices([
        (Store("libri-talk-train"), lambda m: not held_out_reader(m)),
        (Store("piper-talk-train"), lambda m: True),
        (Store("fleurs-vi-train", root), lambda m: True),
        (Store("piper-accented-talk-train", root), lambda m: True),
        *kokoro_sets("kokoro-talk-train"),
    ])
    noise = Voices([(Store("sc-noise"), lambda m: True)])
    after = Voices([
        (Store("libri-talk-train"), lambda m: not held_out_reader(m)),
        (Store("fleurs-vi-train", root), lambda m: True),
        (Store("piper-accented-talk-train", root), lambda m: True),
    ])
    train_voice = lambda m: not validation_voice(m)  # noqa: E731
    if check:
        jobs_spec = [("piper-calls-train", root, True, validation_voice, 1)]
    else:
        jobs_spec = [
            ("piper-calls-train", root, True, train_voice, 1),
            ("mswc-lamp-train", root, True, lambda m: True, 8),
            ("piper-near-train", root, False, train_voice, 1),
            ("mswc-near-train", root, False, lambda m: True, 1),
            ("mswc-word-train", None, False, lambda m: True, 1),
            ("sc-word-train", None, False, lambda m: True, 1),
            ("libri-lamp-train", root, False, lambda m: True, 20),
            ("piper-replies-train", root, False, lambda m: not (KOKORO and lamp_says_its_name(m)), 1),
        ]
        if KOKORO:
            jobs_spec += [
                ("kokoro-calls-train", root, True, lambda m: True, 1),
                ("kokoro-near-train", root, False, lambda m: True, 1),
                ("kokoro-replies-train", root, False, lambda m: not lamp_says_its_name(m), 1),
            ]
    stores, jobs = {}, []
    for name, where, call, keep, repeat in jobs_spec:
        store = Store(name, where) if where else Store(name)
        stores[name] = store
        for i, m in enumerate(store.meta):
            if keep(m):
                jobs += [(name, i, r, call) for r in range(repeat)]
    names = [s[0] for s in jobs_spec]
    xs, ys, srcs, levels, ways = [], [], [], [], []
    for n, (name, i, r, call) in enumerate(jobs):
        if n % parts != part:
            continue
        rng = np.random.default_rng([n, r, 11 if check else 13])
        store = stores[name]
        meta = store.meta[i]
        clip = store.audio(i)
        if "name_end" in meta:
            anchor = meta["name_end"]
        elif "span" in meta:
            anchor = meta["span"][1]
        else:
            anchor = None
        if VOICE_CHANGE and not check and rng.random() < 0.5:
            clip, anchor = faster(clip, anchor, rng)
        follow = None
        if name.startswith("mswc") and rng.random() < 0.5:
            follow = after.stretch(rng, int(rng.uniform(0.6, 1.5) * RATE))
        sound, y, relative = example(clip, anchor, call, follow, talk, noise, rng)
        x = stacks(front.of(sound))
        assert len(x) == INPUTS
        keep_rows = y >= 0
        k = int(keep_rows.sum())
        xs.append(x[keep_rows].astype(np.float16))
        ys.append(y[keep_rows])
        srcs.append(np.full(k, names.index(name), np.int16))
        levels.append(np.full(k, relative, np.float16))
        ways.append(np.full(k, WAYS.get(meta.get("what"), 0) if call else 0, np.int8))
        if n % 5000 < parts:
            print(f"part {part}: {n}/{len(jobs)}", flush=True)
    OUT.mkdir(parents=True, exist_ok=True)
    tag = "check" if check else "words"
    np.savez(
        OUT / f"{tag}-{part}.npz",
        x=np.concatenate(xs), y=np.concatenate(ys), source=np.concatenate(srcs),
        level=np.concatenate(levels), suppressed=np.zeros(sum(len(v) for v in ys), np.int8), names=np.array(names),
        way=np.concatenate(ways),
    )
    print(f"{tag} part {part}: {sum(len(v) for v in ys)} inputs, {int(sum(v.sum() for v in ys))} calls")


def talk(part: int, parts: int, kokoro: bool = False) -> None:
    """Long stretches of talk, every input 0: 13 h, or Kokoro's hour."""
    front = FrontEnd(shared.STOP_CACHE / "oww", threads=int(os.environ.get("OMP_NUM_THREADS", "2")))
    root = shared.CLIPS
    readers = Voices([(Store("libri-talk-train"), lambda m: not held_out_reader(m)), (Store("libri-more-talk-train"), lambda m: True)])
    generated = Voices([(Store("piper-talk-train"), lambda m: True), (Store("piper-replies-train", root), lambda m: True)])
    vietnamese = Voices([(Store("fleurs-vi-train", root), lambda m: True)])
    accented = Voices([(Store("piper-accented-talk-train", root), lambda m: True)])
    if kokoro:
        # An hour more, in Kokoro's voices: its talk (three in four read with the
        # Vietnamese accent) and its lamp-like replies, but not one that starts
        # with the name.
        plan = [("one", Voices([
            *kokoro_sets("kokoro-talk-train"),
            *kokoro_sets("kokoro-replies-train", lambda m: not lamp_says_its_name(m)),
        ]))] * 60
    else:
        plan = (
            [("one", readers)] * 300 + [("one", generated)] * 120 + [("one", accented)] * 180
            + [("one", vietnamese)] * 120 + [("two", readers)] * 60
        )
    minute = 60 * RATE
    xs = []
    for n, (how, voices) in enumerate(plan):
        if n % parts != part:
            continue
        rng = np.random.default_rng([n, 19 if kokoro else 17])
        a = voices.stretch(rng, minute)
        a = scaled(a, active_level(a), rng.uniform(-40, -12))
        if rng.random() < 0.3:
            a = clip_onsets(a, rng)
        b = None
        if how == "two":
            b = vietnamese.stretch(rng, minute) if rng.random() < 0.5 else generated.stretch(rng, minute)
            b = scaled(b, active_level(b), active_level(a) + rng.uniform(-20, 0))
        sound = mixture(a, b, rng) if b is not None else mixture(None, a, rng)
        xs.append(stacks(front.of(sound)).astype(np.float16))
        if n % 60 < parts:
            print(f"talk part {part}: {n}/{len(plan)}", flush=True)
    OUT.mkdir(parents=True, exist_ok=True)
    x = np.concatenate(xs)
    np.savez(OUT / f"talk-{'kokoro-' if kokoro else ''}{part}.npz", x=x, y=np.zeros(len(x), np.int8), source=np.full(len(x), -1, np.int16), names=np.array([]))
    print(f"talk part {part}: {len(x)} inputs")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("what", choices=["words", "talk", "talk-kokoro", "check"])
    p.add_argument("--part", default="0/1")
    a = p.parse_args()
    part, parts = (int(v) for v in a.part.split("/"))
    if a.what in ("talk", "talk-kokoro"):
        talk(part, parts, kokoro=a.what == "talk-kokoro")
    else:
        words(part, parts, a.what == "check")
