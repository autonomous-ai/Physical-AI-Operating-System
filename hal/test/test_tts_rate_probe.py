"""Output rate probe must not walk down to a low rate while the speaker is busy (#688)."""

import logging
from contextlib import nullcontext

from hal.drivers.voice.tts import service as module


class PortAudioError(Exception):
    """Mirrors sounddevice.PortAudioError: args = (errormsg, errcode[, host_info])."""


def busy():
    return PortAudioError(
        "Error opening OutputStream: Device unavailable [PaErrorCode -9985]", -9985
    )


def unsupported():
    return PortAudioError(
        "Error opening OutputStream: Invalid sample rate [PaErrorCode -9997]", -9997
    )


def ebusy_host():
    return PortAudioError(
        "Error opening OutputStream: Unanticipated host error [PaErrorCode -9999]",
        -9999,
        (8, -16, "Device or resource busy"),
    )


class _Stream:
    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def write(self, data):
        return False


class FakeSD:
    PortAudioError = PortAudioError

    def __init__(self, fail=lambda rate, n: None):
        # fail(rate, open_number) -> exception to raise, or None to open fine
        self.fail = fail
        self.opens = []

    def query_devices(self, idx, kind=None):
        return {"name": "speaker"}

    def OutputStream(self, samplerate, **kwargs):
        self.opens.append(samplerate)
        err = self.fail(samplerate, len(self.opens))
        if err is not None:
            raise err
        return _Stream()


def _svc(monkeypatch, sd, rate=None, cache=None):
    import hal.drivers.audio_route as route

    monkeypatch.setattr(route, "stream_open_guard", nullcontext)
    svc = object.__new__(module.TTSService)
    svc._sd = sd
    svc._output_device = 14
    svc._device_rate = rate
    svc._rate_cache = dict(cache or {})
    return svc


def test_free_device_verifies_44100(monkeypatch):
    sd = FakeSD()
    svc = _svc(monkeypatch, sd)
    svc._probe_device_rate()
    assert svc._device_rate == 44100
    assert svc._rate_cache == {"speaker": 44100}
    assert sd.opens == [44100]


def test_busy_recovery_probe_keeps_known_rate(monkeypatch):
    # Music holds the speaker; the playback-setup recovery path re-probes.
    sd = FakeSD(lambda rate, n: busy())
    svc = _svc(monkeypatch, sd, rate=44100, cache={"speaker": 44100})
    svc._probe_device_rate(force=True, use_cache=False)
    assert svc._device_rate == 44100
    assert svc._rate_cache == {"speaker": 44100}
    assert sd.opens == [44100]  # stopped after the first busy open


def test_busy_then_free_does_not_lock_lower_rate(monkeypatch):
    # The #688 trace: busy for the first 6 opens, free on the 7th (8000).
    sd = FakeSD(lambda rate, n: busy() if n <= 6 else None)
    svc = _svc(monkeypatch, sd, rate=44100, cache={"speaker": 44100})
    svc._probe_device_rate(force=True, use_cache=False)
    assert svc._device_rate == 44100
    assert svc._rate_cache == {"speaker": 44100}


def test_busy_with_no_known_rate_leaves_rate_unset(monkeypatch):
    sd = FakeSD(lambda rate, n: busy())
    svc = _svc(monkeypatch, sd)
    svc._probe_device_rate()
    assert svc._device_rate is None
    assert svc._rate_cache == {}
    assert sd.opens == [44100]


def test_alsa_ebusy_host_error_counts_as_busy(monkeypatch):
    sd = FakeSD(lambda rate, n: ebusy_host())
    svc = _svc(monkeypatch, sd, rate=48000, cache={"speaker": 48000})
    svc._probe_device_rate(force=True, use_cache=False)
    assert svc._device_rate == 48000
    assert sd.opens == [44100]


def test_portaudio_reinit_guard_counts_as_busy(monkeypatch):
    sd = FakeSD(
        lambda rate, n: RuntimeError("PortAudio re-init in progress — stream open rejected")
    )
    svc = _svc(monkeypatch, sd, rate=44100, cache={"speaker": 44100})
    svc._probe_device_rate(force=True, use_cache=False)
    assert svc._device_rate == 44100
    assert sd.opens == [44100]


def test_unsupported_rates_still_fall_back_and_warn(monkeypatch, caplog):
    sd = FakeSD(lambda rate, n: unsupported() if rate in (44100, 48000) else None)
    svc = _svc(monkeypatch, sd)
    with caplog.at_level(logging.WARNING, logger=module.logger.name):
        svc._probe_device_rate()
    assert svc._device_rate == 16000
    assert svc._rate_cache == {"speaker": 16000}
    assert sd.opens == [44100, 48000, 16000]
    assert any("16000" in r.getMessage() for r in caplog.records if r.levelno == logging.WARNING)


def test_full_rate_does_not_warn(monkeypatch, caplog):
    svc = _svc(monkeypatch, FakeSD())
    with caplog.at_level(logging.WARNING, logger=module.logger.name):
        svc._probe_device_rate()
    assert not [r for r in caplog.records if r.levelno >= logging.WARNING]


def test_recovery_probe_on_free_device_replaces_stale_low_rate(monkeypatch):
    sd = FakeSD()
    svc = _svc(monkeypatch, sd, rate=8000, cache={"speaker": 8000})
    svc._probe_device_rate(force=True, use_cache=False)
    assert svc._device_rate == 44100
    assert svc._rate_cache == {"speaker": 44100}


def test_all_rates_failing_keeps_known_rate(monkeypatch):
    sd = FakeSD(lambda rate, n: unsupported())
    svc = _svc(monkeypatch, sd, rate=44100, cache={"speaker": 44100})
    svc._probe_device_rate(force=True, use_cache=False)
    assert svc._device_rate == 44100
    assert svc._rate_cache == {"speaker": 44100}
    assert len(sd.opens) == 7


def test_cache_hit_skips_open(monkeypatch):
    sd = FakeSD()
    svc = _svc(monkeypatch, sd, cache={"speaker": 48000})
    svc._probe_device_rate(force=True)
    assert svc._device_rate == 48000
    assert sd.opens == []


def test_is_device_busy_classification():
    assert module._is_device_busy(busy())
    assert module._is_device_busy(ebusy_host())
    assert module._is_device_busy(RuntimeError("PortAudio re-init in progress — stream open rejected"))
    assert not module._is_device_busy(unsupported())
    assert not module._is_device_busy(
        PortAudioError("Unanticipated host error", -9999, (8, -22, "Invalid argument"))
    )
    assert not module._is_device_busy(ValueError("boom"))


def test_output_rate_reprobes_after_busy_boot(monkeypatch):
    # Busy at boot leaves no rate; the next speech re-probes once the speaker is free.
    sd = FakeSD(lambda rate, n: busy() if n == 1 else None)
    svc = _svc(monkeypatch, sd)
    svc._probe_device_rate()
    assert svc._device_rate is None
    assert svc._output_rate() == 44100
    assert svc._rate_cache == {"speaker": 44100}
    assert sd.opens == [44100, 44100]


def test_output_rate_falls_back_while_still_busy(monkeypatch):
    sd = FakeSD(lambda rate, n: busy())
    svc = _svc(monkeypatch, sd)
    assert svc._output_rate() == module.TTS_SAMPLE_RATE
    assert sd.opens == [44100]


def test_output_rate_known_rate_does_not_open(monkeypatch):
    sd = FakeSD()
    svc = _svc(monkeypatch, sd, rate=48000)
    assert svc._output_rate() == 48000
    assert sd.opens == []


def test_output_rate_skips_probe_while_own_stream_is_open(monkeypatch):
    sd = FakeSD()
    svc = _svc(monkeypatch, sd)
    svc._stream = object()
    assert svc._output_rate() == module.TTS_SAMPLE_RATE
    assert sd.opens == []
