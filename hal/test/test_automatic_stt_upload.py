"""Authorized Gemini capture proceeds independently of STT transport."""

import threading
from unittest.mock import Mock

import numpy as np
import pytest

from hal.drivers.voice import voice_service as voice
from hal.drivers.voice._internal.realtime_turn import RealtimeTurnResult
from hal.drivers.voice._internal.stt_upload import STTUpload
from hal.drivers.voice._internal.wakeword_focus import WakeWordFocus


@pytest.fixture
def setup(monkeypatch):
    service = Mock()
    service._running = True
    service._np = np
    service._tts = None
    service._tts_is_speaking.return_value = False
    service._music_is_playing.return_value = False
    service._automatic_reply_lock = threading.Lock()
    service._automatic_reply_stop = None
    service._automatic_reply_cancelled_at = None
    service._automatic_stt_upload = None
    service._stt_drain_worker = None
    service._stt_drain_future = None
    service._silence_vad = None
    service._turn_detector = None
    service._wakeword_focus = WakeWordFocus(5)
    service._wakeword_focus.refresh()
    service._realtime.available = True
    service._realtime.rebuilding = False
    service._realtime.sample_rate = 16000
    service.bare_wake_phrase.return_value = False
    service._rt_noise_is_speech.return_value = True
    service._decorator.starts_with_wake_word.return_value = False
    service._decorator.classify_wake_word.side_effect = lambda text: (text, 'voice')
    service._decorator.identify_and_decorate.side_effect = lambda text, *a, **kw: (text, None, None)
    stt = Mock()
    stt.is_closed.return_value = False
    service._stt.create_session.return_value = stt
    releases = []

    def start(callback):
        stt.callback = callback
        return True

    stt.start.side_effect = start
    stt.close.side_effect = lambda: stt.callback('Complete fallback request', True)
    for name, value in [('REALTIME_ENABLED', True), ('REALTIME_PROVIDER', 'gemini'),
                        ('REALTIME_MIN_COMMIT_DURATION_S', 0.01), ('WAKEWORD_ENABLED', True)]:
        monkeypatch.setattr(voice.hal_config, name, value)
    monkeypatch.setattr(voice.voice_cfg, 'LIVE_MODE', False)
    monkeypatch.setattr(voice.voice_cfg, 'TURN_END_ENABLED', False)
    monkeypatch.setattr(voice.voice_cfg, 'SILENCE_VAD_ENABLED', False)
    monkeypatch.setattr(voice, 'pending_main_question', lambda: None)
    monkeypatch.setattr(voice, 'harness_followup_active', lambda: False)
    monkeypatch.setattr(voice, 'turn_should_close', lambda *args: True)
    monkeypatch.setattr(voice, 'is_noise_turn', lambda *args, **kwargs: False)
    monkeypatch.setattr(voice, 'build_turn_context', lambda *args: 'test')
    monkeypatch.setattr(voice, 'voice_metrics', Mock())
    monkeypatch.setattr(voice.requests, 'post', Mock())
    monkeypatch.setattr(voice, '_WaitFiller', Mock())
    monkeypatch.setattr('hal.drivers.tracking.gaze.on_speech_end', lambda: None)
    dispatch = Mock()
    reply = Mock(return_value=RealtimeTurnResult(
        handled=True, transcript='Thirty-six.', user_transcript='What is twelve times three?',
    ))
    monkeypatch.setattr(voice, 'dispatch_turn', dispatch)
    monkeypatch.setattr(voice, 'run_realtime_turn', reply)
    prepared = threading.Event()
    service._realtime.prepare_turn.side_effect = prepared.set

    def run(on_read=None, preconnected=None):
        frames = iter([np.full((1024, 1), 10000, dtype=np.int16), np.zeros((1024, 1), dtype=np.int16)])
        def read(_):
            if (voice.hal_config.REALTIME_PROVIDER == 'gemini'
                    and (not voice.hal_config.WAKEWORD_ENABLED or service._wakeword_focus.is_active())):
                assert prepared.wait(1)
            if on_read:
                on_read()
            return next(frames), False
        mic = Mock()
        mic.read.side_effect = read
        voice.VoiceService._stream_session(
            service, mic, 1024, 16000, speech_pre_buffer=[b'\x01\x00' * 1024],
            preconnected_session=preconnected,
            harness_voice={'enabled': False, 'generation': 1},
        )

    yield service, stt, reply, dispatch, run, releases
    for event in releases:
        event.set()
    worker = service._automatic_stt_upload
    if isinstance(worker, STTUpload):
        worker.cancel()
        assert worker.done.wait(3)
        worker._thread.join(1)
    if service._stt_drain_worker is not None:
        service._stt_drain_worker.shutdown(wait=True)


@pytest.mark.parametrize('phase', ['connect', 'send'])
def test_gemini_commits_and_returns_while_stt_transport_is_blocked(setup, phase):
    service, stt, reply, _, run, releases = setup
    entered, release = threading.Event(), threading.Event()
    releases.append(release)
    original_start = stt.start.side_effect
    def block(*args):
        if phase == 'connect':
            original_start(*args)
        entered.set()
        assert release.wait(3)
        return True
    if phase == 'connect':
        stt.start.side_effect = block
    else:
        stt.send_audio.side_effect = block
    def response(*args, **kwargs):
        assert entered.is_set() and not release.is_set()
        assert kwargs['audio_admitted']
        return RealtimeTurnResult(handled=True, transcript='Thirty-six.', user_transcript='Twelve times three?')
    reply.side_effect = response
    run(lambda: entered.wait(1))
    reply.assert_called_once()
    assert not release.is_set()
    assert not service._automatic_stt_upload.done.is_set()
    assert service._realtime.append_audio.call_count >= 3
    release.set()
    assert service._automatic_stt_upload.done.wait(1)
    stt.close.assert_called_once()


def test_busy_slot_does_not_create_orphan_raw_session_and_gemini_still_answers(setup):
    service, stt, reply, _, run, releases = setup
    release = threading.Event()
    releases.append(release)
    raw = Mock()
    raw.start.side_effect = lambda cb: release.wait(3)
    old = STTUpload(raw, Mock(), max_bytes=4096).start()
    service._automatic_stt_upload = old
    run()
    service._stt.create_session.assert_not_called()
    stt.start.assert_not_called()
    stt.close.assert_not_called()
    reply.assert_called_once()
    assert service._automatic_stt_upload is old
    assert not old.done.is_set()


def test_failed_stt_partial_is_never_forwarded_as_complete_main_request(setup):
    service, stt, reply, dispatch, run, _ = setup
    def fail(callback):
        stt.callback = callback
        callback('Delete all', True)
        return False
    stt.start.side_effect = fail
    reply.return_value = RealtimeTurnResult()
    run()
    assert service._automatic_stt_upload.outcome.status == 'start_failed'
    assert not any(call.args[2] for call in dispatch.call_args_list)
    stt.close.assert_called_once()


def test_failed_realtime_waits_for_full_worker_fallback(setup):
    service, stt, reply, dispatch, run, releases = setup
    answered = threading.Event()
    def close():
        assert answered.wait(2)
        stt.callback('Please check my calendar', True)
    stt.close.side_effect = close
    def response(*args, **kwargs):
        answered.set()
        return RealtimeTurnResult()
    reply.side_effect = response
    run()
    assert service._automatic_stt_upload.outcome.complete
    assert any(call.args[2] == 'Please check my calendar' for call in dispatch.call_args_list)


@pytest.mark.parametrize('gate', ['other_provider', 'closed_focus', 'pending_main', 'keepalive'])
def test_other_routes_keep_existing_stt_ownership(monkeypatch, setup, gate):
    service, stt, _, _, run, _ = setup
    if gate == 'other_provider':
        monkeypatch.setattr(voice.hal_config, 'REALTIME_PROVIDER', 'openai')
    elif gate == 'closed_focus':
        service._wakeword_focus.clear()
        service._realtime.prepare_turn.side_effect = None
    elif gate == 'pending_main':
        monkeypatch.setattr(voice, 'pending_main_question', lambda: 'What is your name?')
    if gate == 'keepalive':
        stt.close.side_effect = lambda: stt._on_transcript_cb('Complete fallback request', True)
    # Excluded routes still execute the ordinary STT start or keepalive path.
    run(preconnected=stt if gate == 'keepalive' else None)
    assert service._automatic_stt_upload is None
    stt.close.assert_called_once()
    if gate == 'keepalive':
        stt.start.assert_not_called()
    else:
        stt.start.assert_called_once()


def test_stop_during_connect_returns_without_network_wait_and_retires_callbacks(setup):
    service, stt, reply, dispatch, run, releases = setup
    entered, release = threading.Event(), threading.Event()
    releases.append(release)
    def start(callback):
        stt.callback = callback
        entered.set()
        assert release.wait(3)
        return True
    stt.start.side_effect = start
    def stop_capture():
        assert entered.wait(1)
        service._running = False
    run(stop_capture)
    upload = service._automatic_stt_upload
    assert not upload.done.is_set()
    assert not release.is_set()
    stt.callback('Hello Lamp, stale command', True)
    assert not any(call.args[2] for call in dispatch.call_args_list)
    reply.assert_not_called()
    release.set()
    assert upload.done.wait(1)
    assert upload.outcome.status == 'cancelled'
    stt.close.assert_called_once()
