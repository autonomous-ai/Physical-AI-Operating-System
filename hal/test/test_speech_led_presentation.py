"""Speech must progress independently of delayed and superseded LED work."""
import threading
import time
from unittest.mock import Mock

import pytest

from hal import app_state as state
from hal.drivers.rgb.presentation import GuardedFrame, LatestPresentation
from hal.drivers.rgb.rgb_service import RGBService


@pytest.fixture
def lamp(monkeypatch):
    for key, value in {
        'rgb_service': Mock(led_count=32), '_sleeping': False,
        '_tts_speaking': False, '_effect_thread': None,
        '_effect_stop': threading.Event(), '_effect_name': None,
        '_restore_timer': None, '_restore_generation': 0,
        '_restore_lock': threading.RLock(), '_speech_led_epoch': 0,
        '_speech_led_context': threading.local(),
        '_current_emotion': 'idle', '_emotion_generation': 0,
        'tts_service': Mock(realtime_feedback=False),
    }.items():
        monkeypatch.setattr(state, key, value)
    monkeypatch.setattr(state, '_run_effect', Mock())
    monkeypatch.setattr(state, '_restore_user_led_display', Mock())
    worker = LatestPresentation(state._present_tts_led, name='test-speech-led')
    monkeypatch.setattr(state, '_speech_led', worker)
    yield worker
    worker.close()
    state._effect_stop.set()
    if state._effect_thread:
        state._effect_thread.join(1)


def block_stop(monkeypatch):
    entered, release = threading.Event(), threading.Event()
    def stop():
        entered.set()
        assert release.wait(3)
    monkeypatch.setattr(state, '_stop_current_effect', stop)
    return entered, release


def test_callbacks_return_while_led_stop_is_blocked(lamp, monkeypatch):
    entered, release = block_stop(monkeypatch)
    try:
        state._on_tts_speak_start()
        assert entered.wait(1)
        returned = threading.Event()
        def audio():
            state._on_tts_speak_end()
            state._on_tts_speak_start()
            returned.set()  # The next PCM write can proceed without releasing LED.
        audio_thread = threading.Thread(target=audio)
        audio_thread.start()
        assert returned.wait(0.5)
        assert state._tts_speaking
    finally:
        release.set()
    audio_thread.join(1)
    assert lamp.wait_idle(1)
    state._effect_thread.join(1)
    assert state._run_effect.call_count == 1
    state._restore_user_led_display.assert_not_called()


def test_end_cancels_start_waiting_in_led_stop(lamp, monkeypatch):
    entered, release = block_stop(monkeypatch)
    try:
        state._on_tts_speak_start()
        assert entered.wait(1)
        state._on_tts_speak_end()
    finally:
        release.set()
    assert lamp.wait_idle(1)
    state._run_effect.assert_not_called()
    state._restore_user_led_display.assert_called_once()


def test_explicit_override_revokes_start_before_slow_stop_finishes(lamp, monkeypatch):
    entered, release = block_stop(monkeypatch)
    revoked = threading.Event()
    original_epoch = state._speech_led_epoch
    def override():
        state._cancel_pending_restore()
        revoked.set()
    try:
        state._on_tts_speak_start()
        assert entered.wait(1)
        thread = threading.Thread(target=override)
        thread.start()
        # The override publishes revocation before blocking on rendering.
        deadline = time.monotonic() + 1
        while state._speech_led_epoch == original_epoch and time.monotonic() < deadline:
            revoked.wait(0.001)
        assert state._speech_led_epoch != original_epoch
    finally:
        release.set()
    thread.join(1)
    assert revoked.is_set()
    assert lamp.wait_idle(1)
    state._run_effect.assert_not_called()


def test_sleep_during_stop_does_not_start_speech_wave(lamp, monkeypatch):
    entered, release = block_stop(monkeypatch)
    try:
        state._on_tts_speak_start()
        assert entered.wait(1)
        state._sleeping = True
    finally:
        release.set()
    assert lamp.wait_idle(1)
    state._run_effect.assert_not_called()


def test_semantic_end_cleanup_is_not_coalesced_or_replayed(lamp, monkeypatch):
    from hal.routes import emotion
    apply = Mock()
    monkeypatch.setattr(emotion, 'express_emotion', apply)
    entered, release = block_stop(monkeypatch)
    state._current_emotion = 'thinking'
    state.tts_service.realtime_feedback = True
    try:
        state._on_tts_speak_start()
        assert entered.wait(1)
        state._on_tts_speak_end()
        apply.assert_called_once()
        state.tts_service.realtime_feedback = False
        state._on_tts_speak_start()
    finally:
        release.set()
    assert lamp.wait_idle(1)
    apply.assert_called_once()
    state._restore_user_led_display.assert_not_called()


def test_slow_semantic_end_cannot_publish_after_new_start(lamp, monkeypatch):
    entered, release = threading.Event(), threading.Event()
    def cleanup():
        entered.set()
        assert release.wait(2)
    monkeypatch.setattr(state, '_clear_thinking_after_reply', cleanup)
    state._tts_speaking = True
    thread = threading.Thread(target=state._on_tts_speak_end)
    try:
        thread.start()
        assert entered.wait(1)
        state._on_tts_speak_start()
        assert lamp.wait_idle(1)
    finally:
        release.set()
    thread.join(1)
    assert not thread.is_alive()
    assert lamp.wait_idle(1)
    assert state._tts_speaking
    state._restore_user_led_display.assert_not_called()


def test_resting_restore_does_not_revoke_current_wave(lamp):
    state._on_tts_speak_start()
    assert lamp.wait_idle(1)
    state._effect_thread.join(1)
    proxy = state._run_effect.call_args.args[-1]
    state._restore_user_led()
    proxy.dispatch('solid', (1, 2, 3))
    frame = state.rgb_service.dispatch.call_args.args[1]
    assert frame.valid()
    state._on_tts_speak_end()
    assert not frame.valid()


def test_bounded_pending_state_and_exception_recovery():
    entered, release = threading.Event(), threading.Event()
    seen = []
    def render(value, current):
        if value == 0:
            entered.set()
            assert release.wait(2)
            raise RuntimeError('injected failed LED transition')
        seen.append(value)
    worker = LatestPresentation(render, name='test-bounded-led')
    try:
        worker.submit(0)
        assert entered.wait(1)
        first_thread = worker._thread
        for value in range(1, 1001):
            worker.submit(value)
        assert worker._thread is first_thread
        assert worker._pending[1] == 1000
        release.set()
        assert worker.wait_idle(1)
        assert seen == [1000]
    finally:
        release.set()
        worker.close()
    worker.submit(1001)
    assert worker._pending is None
    assert not first_thread.is_alive()


@pytest.mark.parametrize('reason', ['revoked', 'expired', 'current'])
def test_rgb_rechecks_guard_at_driver_boundary(reason):
    svc = RGBService.__new__(RGBService)
    svc._driver_lock = threading.RLock()
    svc._handle_solid = Mock()
    frame = GuardedFrame((1, 2, 3), lambda: reason != 'revoked',
                         time.monotonic() - (1 if reason == 'expired' else 0))
    svc.handle_event('solid', frame)
    assert svc._handle_solid.call_count == (1 if reason == 'current' else 0)


def test_delayed_speech_enqueue_cannot_evict_new_user_color():
    from hal.drivers.base import ServiceBase
    from hal.drivers.rgb.presentation import SpeechRGB
    service = RGBService.__new__(RGBService)
    ServiceBase.__init__(service, 'rgb-test')
    service._running.set()
    service.led_count = 32
    service._driver_lock = threading.RLock()
    service._handle_solid = Mock()
    current = True
    entered, release = threading.Event(), threading.Event()
    dispatch = service.dispatch
    def delayed(command, payload):
        entered.set()
        assert release.wait(2)
        dispatch(command, payload)
    service.dispatch = delayed
    speech = SpeechRGB(service, lambda: current, threading.Event())
    thread = threading.Thread(target=lambda: speech.dispatch('solid', (1, 2, 3)))
    try:
        thread.start()
        assert entered.wait(1)
        current = False
        dispatch('solid', (9, 8, 7))
    finally:
        release.set()
    thread.join(1)
    assert not thread.is_alive()
    event = service._current_event
    assert event.payload == (9, 8, 7)
    service.handle_event(event.event_type, event.payload)
    service._handle_solid.assert_called_once_with((9, 8, 7))
