"""Silence-clock evidence must retain real gaps and the last loud frame's time."""

from unittest.mock import Mock

import numpy as np
import pytest

from hal.drivers.voice import voice_service as voice
from hal.test.test_turn_endpoint_capture import capture


def frames(loud, count=100):
    return [((index + 1) * .064, index in loud, None) for index in range(count)]


def test_spaced_noise_bursts_never_become_one_continuous_speech_window(monkeypatch):
    windows = []
    def classify(window, rate):
        values = window.reshape(3, 1024)[:, 0].tolist()
        windows.append(values)
        assert rate == 16000
        return bool(np.all(window))
    with capture(monkeypatch, frames({0, 11, 22}), enabled=False,
                 silence_vad=True, speech_windows=classify) as result:
        assert windows[:4] == [[10000, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 10000]]
        assert [10000, 10000, 10000] not in windows
        assert result.consumed[-1] == pytest.approx(2.56)
        assert len(windows) == len(result.consumed) // 3


@pytest.mark.parametrize('loud', [{0, 1, 2}, {0, 2}, {0}])
def test_accepted_speech_clock_stops_at_last_loud_frame_not_window_tail(monkeypatch, loud):
    windows = []
    def classify(window, _):
        windows.append(window.copy())
        # Fail-open deliberately includes pure silence; it must not extend time.
        return True
    with capture(monkeypatch, frames(loud), enabled=False,
                 silence_vad=True, speech_windows=classify) as result:
        last_loud_at = (max(loud) + 1) * .064
        assert 2.5 < result.consumed[-1] - last_loud_at <= 2.564
        assert any(np.count_nonzero(window) == 0 for window in windows)
        final_audio = result.dispatch.call_args.args[3]
        # finalize_session retains the configured tail AFTER the last loud frame.
        assert len(final_audio) == max(loud) + 5


def test_all_quiet_fail_open_windows_do_not_refresh_speech_clock(monkeypatch):
    with capture(monkeypatch, frames(set()), enabled=False,
                 silence_vad=True, speech_windows=lambda *_: True) as result:
        assert result.consumed[-1] == pytest.approx(2.56)
        assert result.service._silence_window_is_speech.call_count == 13


@pytest.mark.parametrize('accepted', [False, True])
def test_pending_loud_frame_must_be_classified_before_quiet_can_close(monkeypatch, accepted):
    classifications = []
    def classify(window, _):
        contains_loud = bool(np.any(window))
        classifications.append(contains_loud)
        return contains_loud and accepted
    with capture(monkeypatch, frames({39}), enabled=False,
                 silence_vad=True, speech_windows=classify) as result:
        # Loud frame 40 starts an unfinished window after the old silence deadline.
        # Frame 41 is quiet but must wait for frame 42's VAD result.
        assert classifications[13]
        assert result.consumed[-1] == pytest.approx(5.12 if accepted else 2.688)


@pytest.mark.parametrize('failure', ['unavailable', 'inference'])
def test_model_fail_open_preserves_real_speech_and_never_refreshes_on_quiet(monkeypatch, failure):
    def configure(service):
        service._silence_vad = Mock(available=failure != 'unavailable')
        service._silence_vad.is_speech.side_effect = RuntimeError('model failed')
        service._silence_window_is_speech.side_effect = lambda data, rate: voice.VoiceService._silence_window_is_speech(service, data, rate)
    with capture(monkeypatch, frames({0}), enabled=False, silence_vad=True,
                 configure_service=configure) as result:
        assert result.consumed[-1] == pytest.approx(2.624)
        result.service._silence_vad.reset_state.assert_called_once()


def test_disabled_silence_vad_keeps_existing_rms_only_path(monkeypatch):
    with capture(monkeypatch, [(1, True, None), (4, False, None)], enabled=False,
                 silence_vad=False) as result:
        result.service._silence_window_is_speech.assert_not_called()
        assert result.consumed == [1, 4]


def test_manual_capture_retains_loud_only_windows_and_existing_trim(monkeypatch):
    import threading
    from types import SimpleNamespace

    snapshot = {'enabled': True, 'focusAvailable': True, 'generation': 1,
                'machineId': 'mac', 'agentId': 'agent', 'focusRevision': 1}
    manual = SimpleNamespace(cancelled=threading.Event(), finished=threading.Event())
    original = voice.VoiceService._stream_session
    def explicit(service, *args, **kwargs):
        kwargs['manual_capture'] = manual
        kwargs['harness_voice'] = snapshot
        return original(service, *args, **kwargs)
    monkeypatch.setattr(voice.VoiceService, '_stream_session', explicit)
    monkeypatch.setattr(voice, 'read_voice_mode', lambda: snapshot)
    monkeypatch.setattr('hal.drivers.harness.led.set_capturing', Mock())
    windows = []
    def classify(window, _):
        windows.append(window.copy())
        return bool(np.all(window))
    def read(elapsed):
        if elapsed >= 19 * .064:
            manual.finished.set()
    def configure(service):
        service._tts = None
    with capture(monkeypatch, frames({0, 3, 6}, count=19), enabled=False,
                 silence_vad=True, speech_windows=classify, on_read=read,
                 configure_service=configure) as result:
        assert len(windows) == 1
        assert np.count_nonzero(windows[0]) == 3072
        assert len(result.consumed) == 19
        assert len(result.dispatch.call_args.args[3]) == 11
        assert result.metrics.speech_end.call_args.args[0] == 'manual_tap'
