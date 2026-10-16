"""Provider PCM must sound the same regardless of network packet boundaries."""

import math
import threading
from types import SimpleNamespace

import numpy as np
import pytest

from hal.drivers.voice.tts.resampler import PCMResampler
from hal.drivers.voice.tts.service import TTSService


def _service(packets, src_rate=24000, boost=1.0):
    svc = object.__new__(TTSService)
    svc._np = np
    svc._stop_event = threading.Event()
    svc._voice = "test"
    svc._model = "test"
    svc._speed = 1.1
    svc._instructions = None
    svc._backend = SimpleNamespace(
        sample_rate=src_rate, volume_boost=boost,
        stream_pcm=lambda **kwargs: iter(packets),
    )
    return svc


def _collect(frames):
    frames = list(frames)
    assert all(f.dtype == np.float32 and f.ndim == 2 and f.shape[1] == 1 for f in frames)
    return np.concatenate(frames).ravel() if frames else np.empty(0, dtype=np.float32)


@pytest.mark.parametrize("src,dst", [(24000, 44100), (24000, 48000), (22050, 44100),
                                      (48000, 16000), (24000, 24000)])
@pytest.mark.parametrize("packet_bytes", [1, 7, 4096, 100000])
def test_pcm_matches_continuous_sample_clock_for_any_packet_boundaries(src, dst, packet_bytes):
    pcm = (np.sin(2 * np.pi * 3100 * np.arange(2401) / src) * 24000).astype(np.int16)
    raw = pcm.tobytes()
    packets = [raw[i:i + packet_bytes] for i in range(0, len(raw), packet_bytes)]
    actual = _collect(_service(packets, src)._iter_tts_samples("hello", dst))
    count = math.ceil(len(pcm) * dst / src)
    expected = PCMResampler(src, dst).process(pcm / 32768., final=True)
    assert len(actual) == count
    np.testing.assert_allclose(actual, expected, atol=1e-6, rtol=0)


@pytest.mark.parametrize("raw", [b"", b"\x01", b"\x00\x40", b"\x00\x40\xff"])
def test_empty_short_and_incomplete_pcm_at_eof(raw):
    actual = _collect(_service([raw])._iter_tts_samples("hello", 44100))
    if len(raw) < 2:
        assert len(actual) == 0
    else:
        # One 0.5 sample: two output samples, peak at the sample itself.
        assert len(actual) == 2
        assert actual[0] == pytest.approx(0.5, abs=0.05)


def test_cancel_does_not_flush_held_samples():
    pcm = np.full(2000, 0x2000, dtype=np.int16).tobytes()
    svc = _service([pcm, pcm])
    frames = svc._iter_tts_samples("hello", 44100)
    first = next(frames)
    svc._stop_event.set()
    assert list(frames) == []
    assert len(first) < math.ceil(2000 * 44100 / 24000)


def test_parallel_syntheses_do_not_share_resampling_state():
    raw = np.arange(103, dtype=np.int16).tobytes()
    svc = _service([raw[:77], raw[77:]])
    head = svc._iter_tts_samples("head", 44100)
    tail = svc._iter_tts_samples("tail", 44100)
    first_head = next(head)
    actual_tail = _collect(tail)
    actual_head = _collect([first_head, *head])
    np.testing.assert_array_equal(actual_head, actual_tail)
    np.testing.assert_array_equal(actual_head, _collect(svc._iter_tts_samples("next", 44100)))


def test_gain_also_applies_to_flushed_tail():
    raw = np.full(300, 0x0800, dtype=np.int16).tobytes()
    plain = _collect(_service([raw])._iter_tts_samples("hello", 44100))
    boosted = _collect(_service([raw], boost=2.5)._iter_tts_samples("hello", 44100))
    np.testing.assert_allclose(boosted, plain * 2.5, atol=1e-6)
