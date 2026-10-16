"""TTS resampling must not mirror speech into frequencies the voice never had."""

import numpy as np
import pytest

from hal.drivers.voice.tts.resampler import PCMResampler


def _tone(freq, sr, secs=1.0, amp=0.5):
    t = np.arange(int(sr * secs)) / sr
    return (amp * np.sin(2 * np.pi * freq * t)).astype(np.float32)


def _band_db(x, sr, lo, hi):
    x = x[len(x) // 10: -len(x) // 10] * np.hanning(len(x) - 2 * (len(x) // 10))
    p = np.abs(np.fft.rfft(x)) ** 2
    f = np.fft.rfftfreq(len(x), 1 / sr)
    return 10 * np.log10(p[(f >= lo) & (f < hi)].sum() / p.sum() + 1e-30)


def _stream(x, src, dst, packet=1920):
    rs = PCMResampler(src, dst)
    out = [rs.process(x[i:i + packet]) for i in range(0, len(x), packet)]
    return np.concatenate(out + [rs.process([], final=True)])


def test_upsampling_bright_speech_leaves_no_image_above_12k():
    # A 9 kHz sibilant at 24 kHz used to come back at 15 kHz (24 - 9) at about -20 dB.
    y = _stream(_tone(9000, 24000), 24000, 44100)
    assert _band_db(y, 44100, 14000, 16000) < -60


def test_upsampling_keeps_the_voice_band():
    for freq in (300, 3000, 9000):
        y = _stream(_tone(freq, 24000), 24000, 44100)
        core = y[4410:-4410]
        assert np.max(np.abs(core)) == pytest.approx(0.5, abs=0.01)


def test_downsampling_rejects_content_above_the_new_nyquist():
    # 10 kHz at 48 kHz would alias to 6 kHz at 16 kHz.
    y = _stream(_tone(10000, 48000), 48000, 16000)
    # Skip the click where the tone switches on and off.
    assert np.max(np.abs(y[1600:-1600])) < 0.001


@pytest.mark.parametrize("src,dst", [(24000, 44100), (24000, 48000), (22050, 44100), (48000, 16000)])
def test_output_length_follows_the_sample_clock(src, dst):
    x = _tone(1000, src, secs=0.37)
    assert len(_stream(x, src, dst, packet=333)) == -(-len(x) * dst // src)


def test_same_rate_is_a_passthrough():
    x = _tone(1000, 24000, secs=0.1)
    np.testing.assert_array_equal(_stream(x, 24000, 24000), x)


def test_cached_phrases_use_the_same_filter():
    from hal.drivers.voice.tts.service import TTSService

    svc = object.__new__(TTSService)
    x = _tone(9000, 24000)
    y = svc._resample(x, 24000, 44100)
    assert len(y) == -(-len(x) * 44100 // 24000)
    assert _band_db(y, 44100, 14000, 16000) < -60
