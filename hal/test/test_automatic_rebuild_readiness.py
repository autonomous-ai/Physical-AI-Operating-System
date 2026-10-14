"""Authorized follow-up capture joins a running rebuild within one bounded budget."""

import threading

import pytest

from hal.drivers.voice import voice_service as voice
from hal.drivers.voice._internal.realtime_turn import RealtimeTurnResult
from hal.test.test_turn_endpoint_capture import capture


@pytest.mark.parametrize('finish', ['endpoint', 'mid_capture', 'timeout', 'prepare_error', 'cancel'])
def test_authorized_rebuild_replays_complete_pcm_once_or_preserves_fallback(monkeypatch, finish):
    monkeypatch.setattr(voice.hal_config, 'REALTIME_PROVIDER', 'gemini')
    monkeypatch.setattr(voice, 'pending_main_question', lambda: None)
    monkeypatch.setattr(voice, 'harness_followup_active', lambda: False)
    prepared = threading.Event()
    ref = []

    def setup(service):
        ref.append(service)
        service._realtime.rebuilding = True
        def prepare():
            prepared.set()
            if finish == 'prepare_error':
                raise RuntimeError('provider preparation failed')
        service._realtime.prepare_turn.side_effect = prepare
        def ready():
            if finish == 'timeout':
                return False
            service._realtime.available = True
            service._realtime.rebuilding = False
            if finish == 'cancel':
                service._running = False
            return True
        service._realtime.wait_until_available.side_effect = ready

    def read(elapsed):
        assert prepared.wait(1)
        if finish == 'mid_capture':
            service = ref[0]
            service._realtime.available = True
            service._realtime.rebuilding = False

    def reply(*args, **kwargs):
        return RealtimeTurnResult(handled=True, transcript='Thirty-six.')

    with capture(monkeypatch, [(1, True, 'What is twelve times three?'), (4, False, None)],
                 realtime=True, wake_enabled=True, focus=lambda: True,
                 realtime_available=False, configure_service=setup,
                 on_read=read, on_realtime=reply) as result:
        rt = result.service._realtime
        if finish in {'endpoint', 'mid_capture'}:
            result.realtime.assert_called_once()
            rt.bind_audio_turn.assert_called_once()
            assert rt.append_audio.call_count == 2
            assert [float(call.args[0].max()) for call in rt.append_audio.call_args_list] == pytest.approx([10000 / 32767, 0])
        else:
            rt.bind_audio_turn.assert_not_called()
            rt.append_audio.assert_not_called()
            result.realtime.assert_not_called()
        if finish in {'mid_capture', 'prepare_error'}:
            rt.wait_until_available.assert_not_called()
        else:
            rt.wait_until_available.assert_called_once_with()
        if result.service._stt_drain_worker is not None:
            result.service._stt_drain_worker.shutdown(wait=True)


@pytest.mark.parametrize('gate', ['closed_wake', 'other_provider'])
def test_unavailable_unqualified_turn_does_not_wait_for_rebuild(monkeypatch, gate):
    monkeypatch.setattr(voice.hal_config, 'REALTIME_PROVIDER', 'openai' if gate == 'other_provider' else 'gemini')
    monkeypatch.setattr(voice, 'pending_main_question', lambda: None)
    monkeypatch.setattr(voice, 'harness_followup_active', lambda: False)
    def setup(service):
        service._realtime.rebuilding = True
    with capture(monkeypatch, [(1, True, 'What is twelve times three?'), (4, False, None)],
                 realtime=True, wake_enabled=True, focus=lambda: gate != 'closed_wake',
                 realtime_available=False, configure_service=setup) as result:
        result.service._realtime.wait_until_available.assert_not_called()
        result.service._realtime.bind_audio_turn.assert_not_called()
        if result.service._stt_drain_worker is not None:
            result.service._stt_drain_worker.shutdown(wait=True)
