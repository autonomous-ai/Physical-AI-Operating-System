"""Continuous band-limited PCM resampling with state owned by one synthesis request."""

from functools import lru_cache
from math import gcd

import numpy as np

# Kaiser-windowed sinc. Linear interpolation mirrored 24 kHz speech into
# 12-22 kHz at ~10 dB under the real top end, which bright voices (Phonon) made
# audible as a squeak on the lamp speaker. 24 zero crossings per side with a
# cutoff at 90% of the lower Nyquist put those images below -60 dB for ~1 ms
# of added latency.
_ZERO_CROSSINGS = 24
_CUTOFF = 0.45  # of the lower sample rate
_KAISER_BETA = 8.0
_BLOCK = 8192  # output samples per gather, bounds temporary memory


@lru_cache(maxsize=16)
def _filter(src_rate: int, dst_rate: int):
    """Return (half-width in input samples, per-phase taps) for src -> dst."""
    g = gcd(src_rate, dst_rate)
    phases = dst_rate // g
    cutoff = _CUTOFF * min(src_rate, dst_rate) / src_rate  # cycles per input sample
    half = int(np.ceil(_ZERO_CROSSINGS / (2.0 * cutoff)))
    offsets = np.arange(-half + 1, half + 1, dtype=np.float64)
    frac = np.arange(phases, dtype=np.float64) * g / dst_rate
    d = offsets[None, :] - frac[:, None]
    window = np.i0(_KAISER_BETA * np.sqrt(np.clip(1.0 - (d / half) ** 2, 0.0, None))) / np.i0(_KAISER_BETA)
    taps = 2.0 * cutoff * np.sinc(2.0 * cutoff * d) * window
    taps /= taps.sum(axis=1, keepdims=True)  # unity gain at DC for every phase
    return half, taps.astype(np.float32)


class PCMResampler:
    """Keep the sample clock and filter history across network chunks.

    Output sample n sits at input position n * src / dst, like before; the
    stream is held back by the filter's half-width until `final` flushes it.
    """

    def __init__(self, src_rate: int, dst_rate: int):
        self.src_rate = src_rate
        self.dst_rate = dst_rate
        self._received = 0
        self._emitted = 0
        if src_rate == dst_rate:
            return
        self._half, self._taps = _filter(src_rate, dst_rate)
        self._g = gcd(src_rate, dst_rate)
        self._offsets = np.arange(-self._half + 1, self._half + 1, dtype=np.int64)
        # Silence before the first sample; _start is the input index of _buf[0].
        self._buf = np.zeros(self._half, dtype=np.float32)
        self._start = -self._half

    def process(self, samples, *, final: bool = False):
        samples = np.asarray(samples, dtype=np.float32)
        if self.src_rate == self.dst_rate:
            return samples
        src, dst, half = self.src_rate, self.dst_rate, self._half
        self._received += len(samples)
        buf = np.concatenate((self._buf, samples))
        if final:
            end = (self._received * dst + src - 1) // src
            buf = np.concatenate((buf, np.zeros(half, dtype=np.float32)))
        else:
            # Emit n only once input floor(n * src / dst) + half has arrived.
            ready = self._received - half
            end = (ready * dst + src - 1) // src if ready > 0 else 0
        end = max(end, self._emitted)

        out = np.empty(end - self._emitted, dtype=np.float32)
        for lo in range(self._emitted, end, _BLOCK):
            n = np.arange(lo, min(lo + _BLOCK, end), dtype=np.int64)
            base = (n * src) // dst
            phase = ((n * src) % dst) // self._g
            window = buf[base[:, None] + self._offsets[None, :] - self._start]
            out[lo - self._emitted:lo - self._emitted + len(n)] = np.einsum(
                "ij,ij->i", window, self._taps[phase])
        self._emitted = end

        keep = (end * src) // dst - half + 1 - self._start
        keep = min(max(keep, 0), len(buf))
        self._buf = buf[keep:].copy()
        self._start += keep
        # Sinc ringing can overshoot full scale a hair; the speaker path expects [-1, 1].
        return np.clip(out, -1.0, 1.0)
