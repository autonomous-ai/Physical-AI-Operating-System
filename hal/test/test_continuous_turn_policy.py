"""An incomplete continuous-mic turn must never become an agent request."""

import threading
from unittest.mock import Mock

import numpy as np
import pytest

from hal.drivers.voice import voice_service as voice
from hal.drivers.voice._internal.continuous_capture import ContinuousCapture
from hal.drivers.voice._internal.realtime_turn import RealtimeTurnResult
from hal.drivers.voice._internal.wakeword_focus import WakeWordFocus


class ScriptedCapture(ContinuousCapture):
    """Supply the capture contract without starting a hardware reader."""

    def __init__(self, read, gap):
        self._read = read
        self._gap = gap

    def read(self, frames):
        self._read()
        return np.zeros((frames, 1), dtype=np.int16), False

    def consume_discontinuity(self):
        return self._gap


@pytest.mark.parametrize('phase', ['connect', 'stream'])
@pytest.mark.parametrize('gap', [False, True])
def test_gap_never_commits_or_dispatches_partial_request(monkeypatch, phase, gap):
    request = 'What is twelve times three?'
    service = Mock()
    service._automatic_reply_lock = threading.Lock()
    service._automatic_reply_stop = None
    service._automatic_reply_cancelled_at = None
    service._stt_drain_worker = None
    service._stt_drain_future = None
    service._running = True
    service._np = np
    service._tts = None
    service._silence_vad = None
    service._tts_is_speaking.return_value = False
    service._music_is_playing.return_value = False
    service._wakeword_focus = WakeWordFocus(5)
    service._realtime.available = True
    service._realtime.rebuilding = False
    service._realtime.sample_rate = 16000
    prepared = threading.Event()
    service._realtime.prepare_turn.side_effect = prepared.set
    service.bare_wake_phrase.return_value = False
    service._decorator.identify_and_decorate.return_value = (request, None, None)
    service._decorator.classify_wake_word.return_value = (request, 'voice')
    stt = Mock()
    stt.is_closed.return_value = False
    service._stt.create_session.return_value = stt
    connect_release = threading.Event()
    connect_finished = threading.Event()
    connect_entered = threading.Event()
    callback = []

    def start(on_transcript):
        callback.append(on_transcript)
        connect_entered.set()
        on_transcript(request, True)
        try:
            assert connect_release.wait(2)
            return True
        finally:
            connect_finished.set()

    stt.start.side_effect = start
    stt.close.side_effect = connect_release.set

    def read():
        if phase == 'stream':
            stt._on_transcript_cb(request, True)
        else:
            assert connect_entered.wait(2)
            connect_release.set()
            assert connect_finished.wait(2)
        if not gap:
            service._running = False

    monkeypatch.setattr(voice.hal_config, 'REALTIME_ENABLED', True)
    monkeypatch.setattr(voice.hal_config, 'REALTIME_PROVIDER', 'gemini')
    monkeypatch.setattr(voice.hal_config, 'WAKEWORD_ENABLED', False)
    monkeypatch.setattr(voice.voice_cfg, 'LIVE_MODE', False)
    monkeypatch.setattr(voice.voice_cfg, 'TURN_END_ENABLED', False)
    monkeypatch.setattr(voice.voice_cfg, 'SILENCE_VAD_ENABLED', False)
    monkeypatch.setattr(voice, 'finalize_session', lambda *args: (request, [], 2.0))
    monkeypatch.setattr(voice, 'is_noise_turn', lambda *args, **kwargs: False)
    monkeypatch.setattr(voice, 'voice_metrics', Mock())
    def listening_event(*args, **kwargs):
        # The stream-loop check must see a prepared provider before reading.
        assert prepared.wait(2)
        return Mock()

    monkeypatch.setattr(voice.requests, 'post', listening_event)
    realtime = Mock(return_value=RealtimeTurnResult())
    dispatch = Mock()
    monkeypatch.setattr(voice, 'run_realtime_turn', realtime)
    monkeypatch.setattr(voice, 'dispatch_turn', dispatch)
    monkeypatch.setattr(voice, '_WaitFiller', Mock())

    try:
        voice.VoiceService._stream_session(
            service, ScriptedCapture(read, gap), 320, 16000,
            preconnected_session=stt if phase == 'stream' else None,
            harness_voice={'enabled': False, 'generation': 1},
        )
    finally:
        connect_release.set()
        if phase == 'connect':
            assert connect_finished.wait(2)
            # The upload owner closes a connection that finishes after capture
            # cancellation; capture itself must not wait for network cleanup.
            assert service._automatic_stt_upload.done.wait(2)

    stt.close.assert_called_once()
    service._realtime.finish_capture.assert_called_once()
    if gap:
        dispatch.assert_not_called()
        realtime.assert_not_called()
        service._realtime.commit_audio.assert_not_called()
        service._silero_reset_state.assert_called_once()
        if phase == 'stream':
            service._realtime.discard_open_activity.assert_called_once_with('capture-gap')
    elif phase == 'connect':
        # Stopping while the independent connect is in flight cancels this turn.
        dispatch.assert_not_called()
        realtime.assert_not_called()
    else:
        dispatch.assert_called_once()
        realtime.assert_called_once()
