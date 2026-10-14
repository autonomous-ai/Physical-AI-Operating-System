"""What the lamp's microphone does to a voice, as the task describes it.

- **The first tenth of a second after a silence is lost.** Wherever speech
  begins after at least 0.25 s of quiet, the first 60 to 140 ms of it are
  taken down by 20 to 40 dB, rising back over 10 ms.
- **A voice from a loudspeaker is muffled.** A small speaker's band
  (high-pass 250 to 600 Hz, low-pass 3 to 6 kHz), a resonance of +3 to
  +8 dB somewhere from 0.8 to 2.5 kHz, and a little saturation.
- **Another microphone, another voice.** One to three bumps or dips of up
  to 8 dB anywhere from 100 Hz to 6 kHz (`colour`), as a different
  microphone, room or throat would put there.

Both are guesses at the lamp from its documents, not measurements of it.
"""

from __future__ import annotations

import numpy as np
from scipy.signal import butter, lfilter, sosfilt

RATE = 16_000


def onsets(x: np.ndarray, quiet_s: float = 0.25) -> list[int]:
    """Samples at which speech begins after at least `quiet_s` of quiet (40 dB under the loudest)."""
    hop = RATE // 100
    n = len(x) // hop
    if n == 0:
        return []
    power = np.mean(x[: n * hop].astype(np.float64).reshape(n, hop) ** 2, axis=1) + 1e-9
    loud = power >= power.max() * 1e-4
    out, quiet = [], int(quiet_s * 100)  # a start counts as after quiet
    run = quiet
    for i, is_loud in enumerate(loud):
        if is_loud:
            if run >= int(quiet_s * 100):
                out.append(i * hop)
            run = 0
        else:
            run += 1
    return out


def clip_onsets(x: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    y = np.array(x, np.float32)
    for at in onsets(y):
        length = int(rng.uniform(0.06, 0.14) * RATE)
        gain = 10 ** (-rng.uniform(20, 40) / 20)
        ramp = int(0.01 * RATE)
        shape = np.full(length + ramp, gain, np.float32)
        shape[length:] = np.linspace(gain, 1.0, ramp)
        end = min(len(y), at + len(shape))
        y[at:end] *= shape[: end - at]
    return y


def peaking(freq: float, gain_db: float, q: float = 1.0):
    """A peaking filter (RBJ cookbook) as (b, a)."""
    a_gain = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * freq / RATE
    alpha = np.sin(w0) / (2 * q)
    b = np.array([1 + alpha * a_gain, -2 * np.cos(w0), 1 - alpha * a_gain])
    a = np.array([1 + alpha / a_gain, -2 * np.cos(w0), 1 - alpha / a_gain])
    return b / a[0], a / a[0]


def loudspeaker(x: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    y = sosfilt(butter(2, rng.uniform(250, 600), "highpass", fs=RATE, output="sos"), x)
    y = sosfilt(butter(4, rng.uniform(3000, 6000), "lowpass", fs=RATE, output="sos"), y)
    b, a = peaking(rng.uniform(800, 2500), rng.uniform(3, 8))
    y = lfilter(b, a, y)
    peak = np.max(np.abs(y)) + 1e-9
    drive = rng.uniform(1.0, 2.5)
    y = np.tanh(drive * y / peak) / np.tanh(drive) * peak
    return y.astype(np.float32)


def colour(x: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    """One to three peaking filters, each -8 to +8 dB, 100 Hz to 6 kHz, one to three octaves wide."""
    y = np.asarray(x, np.float64)
    for _ in range(int(rng.integers(1, 4))):
        freq = float(np.exp(rng.uniform(np.log(100), np.log(6000))))
        b, a = peaking(freq, rng.uniform(-8, 8), q=rng.uniform(0.5, 2.0))
        y = lfilter(b, a, y)
    return y.astype(np.float32)
