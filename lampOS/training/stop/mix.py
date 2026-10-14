"""Putting a person's word under another voice, at a level said in decibels.

Levels. A signal's level is its active level: the mean power of its 10 ms
frames that are within 30 dB of its loudest 5 %, in dB on the scale of
16-bit sound (0 dBFS is a full-scale square wave's power, 32768^2). A word's
level is taken over the word alone (start to end); the other voice's over
the whole stretch it plays. "-10 dB" means the word's level is 10 dB under
the other voice's.

The lamp's numbers it is set to (docs/duplex-listening.md, 2026-10-14): its
own voice at -9.6 to -16.4 dBFS in its second microphone, a person from -12
to -27 dBFS; so a person level with it at best and 15 dB under it at worst.

What else is done to a mixture, at random when training:
- a suppressor like a canceller's: in each time-frequency cell, a gain that
  takes off what the other voice (the echo) puts there, overdone or underdone
  by a random factor, with a floor. It takes some of the person too.
- the microphone's band: a high-pass at 80 to 400 Hz and a low-pass at 3.5
  to 7.5 kHz.
- a room, on the person: a synthetic decaying tail, 0.15 to 0.5 s.
- level, a noise floor, and now and then clipping.
"""

from __future__ import annotations

import numpy as np
from scipy.signal import butter, fftconvolve, istft, sosfilt, stft

RATE = 16_000
FULL = 32768.0


def active_level(x: np.ndarray) -> float:
    """Active level in dBFS of `x` (on the 16-bit scale); -120 for silence."""
    x = np.asarray(x, np.float64)
    hop = RATE // 100
    n = len(x) // hop
    if n == 0:
        return -120.0
    power = np.mean(x[: n * hop].reshape(n, hop) ** 2, axis=1) / FULL**2
    loud = np.percentile(power, 95)
    if loud <= 1e-12:
        return -120.0
    active = power[power >= loud * 10 ** (-30 / 10)]
    return float(10 * np.log10(np.mean(active) + 1e-20))


def gain_for(level_now: float, level_wanted: float) -> float:
    return float(10 ** ((level_wanted - level_now) / 20))


def high_low(x: np.ndarray, high: float | None, low: float | None) -> np.ndarray:
    y = x
    if high:
        y = sosfilt(butter(2, high, "highpass", fs=RATE, output="sos"), y)
    if low:
        y = sosfilt(butter(4, low, "lowpass", fs=RATE, output="sos"), y)
    return y.astype(np.float32)


def room(x: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    rt60 = rng.uniform(0.15, 0.5)
    n = int(rt60 * RATE)
    t = np.arange(n) / RATE
    tail = rng.standard_normal(n) * np.exp(-6.9 * t / rt60)
    tail[: int(0.002 * RATE)] = 0.0
    h = tail / np.sqrt(np.sum(tail**2)) * rng.uniform(0.15, 0.6)
    h[0] = 1.0
    return fftconvolve(x, h)[: len(x)].astype(np.float32)


def suppress(person: np.ndarray, echo: np.ndarray, strength: float, floor: float) -> np.ndarray:
    """A suppressor's output for `person + echo`, given the echo: the gain each cell would get."""
    mixture = person + echo
    _, _, m = stft(mixture, fs=RATE, nperseg=512, noverlap=384)
    _, _, e = stft(echo, fs=RATE, nperseg=512, noverlap=384)
    pm = np.abs(m) ** 2 + 1e-6
    gain = np.clip(1.0 - strength * np.abs(e) ** 2 / pm, floor, 1.0)
    _, y = istft(m * gain, fs=RATE, nperseg=512, noverlap=384)
    return y[: len(mixture)].astype(np.float32)


def place(clip: np.ndarray, at: int, length: int) -> np.ndarray:
    """`clip` put into silence of `length` samples with its first sample at `at` (which may be negative)."""
    out = np.zeros(length, np.float32)
    a, b = max(0, at), min(length, at + len(clip))
    if b > a:
        out[a:b] = clip[a - at : b - at]
    return out


def mixture(
    person: np.ndarray | None,
    other: np.ndarray | None,
    rng: np.random.Generator | None = None,
    noise: np.ndarray | None = None,
    done: dict | None = None,
) -> np.ndarray:
    """The person and the other voice, each already placed and scaled to the full length, heard
    together: with the training augmentations when `rng` is given, plainly otherwise. What was
    done at random is noted in `done`, if given."""
    length = len(person) if person is not None else len(other)
    p = np.zeros(length, np.float32) if person is None else person
    o = np.zeros(length, np.float32) if other is None else other
    if rng is not None:
        if person is not None and rng.random() < 0.3:
            p = room(p, rng)
        if rng.random() < 0.4 and other is not None:
            if done is not None:
                done["suppressed"] = True
            y = suppress(p, o, strength=rng.uniform(0.5, 2.0), floor=rng.uniform(0.03, 0.3))
        else:
            y = p + o
        if noise is not None:
            y = y + noise
        if rng.random() < 0.5:
            y = high_low(y, rng.uniform(80, 400), rng.uniform(3500, 7500))
        floor_db = rng.uniform(-80, -55)
        y = y + rng.standard_normal(length).astype(np.float32) * FULL * 10 ** (floor_db / 20)
        if rng.random() < 0.05:
            y = np.clip(y * rng.uniform(1.5, 4.0), -FULL, FULL - 1)
    else:
        y = p + o
        # The microphone's own floor, fixed when testing: -70 dBFS of white noise.
        y = y + np.random.default_rng(len(y)).standard_normal(length).astype(np.float32) * FULL * 10 ** (-70 / 20)
    return np.clip(y, -FULL, FULL - 1).astype(np.float32)
