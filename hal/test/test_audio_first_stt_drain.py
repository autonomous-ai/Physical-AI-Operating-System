"""Gemini audio admission must not wait for STT, without losing fallback text."""
import threading

import pytest

from hal.drivers.voice._internal.realtime_turn import RealtimeTurnResult
from hal.test.test_turn_endpoint_capture import capture


def configure_audio_first(monkeypatch, speech=True):
    from hal.drivers.voice import voice_service
    monkeypatch.setattr(voice_service.hal_config, "REALTIME_PROVIDER", "gemini")
    monkeypatch.setattr(voice_service.hal_config, "REALTIME_MIN_COMMIT_DURATION_S", 0.1)
    monkeypatch.setattr(voice_service, "pending_main_question", lambda: None)

    def configure(service):
        service.bare_wake_phrase.return_value = False
        service._rt_noise_is_speech.return_value = speech
    return configure


def test_audio_reply_and_capture_return_before_stt_final(monkeypatch):
    closing, release = threading.Event(), threading.Event()
    service_ref = []
    configure = configure_audio_first(monkeypatch)

    def setup(service):
        configure(service)
        service_ref.append(service)

    def drain(stt, service):
        closing.set()
        assert release.wait(3), "capture still waited for STT after Gemini replied"
        service._decorator.starts_with_wake_word.return_value = True
        stt._on_transcript_cb("late STT words", True)
        service._play_wake_chime.assert_not_called()

    def reply(*args, **kwargs):
        assert closing.wait(2)
        assert args[3] == ""
        assert kwargs["audio_admitted"] is True
        return RealtimeTurnResult(handled=True, transcript="Four.", user_transcript="What is two plus two?")

    try:
        with capture(monkeypatch, [(1, True, None), (4, False, None)],
                     realtime=True, wake_enabled=True, focus=lambda: True,
                     configure_service=setup, on_drain=drain, on_realtime=reply) as result:
            assert not release.is_set()
            result.realtime.assert_called_once()
            result.service._realtime.save_turn.assert_called_once_with(
                user_text="What is two plus two?", agent_text="Four.")
    finally:
        release.set()
        if service_ref and service_ref[0]._stt_drain_worker:
            service_ref[0]._stt_drain_worker.shutdown(wait=True)


def test_audio_first_failure_waits_for_complete_fallback_transcript(monkeypatch):
    replied = threading.Event()

    def drain(stt, service):
        assert replied.wait(2), "audio was held behind STT"
        stt._on_transcript_cb("Please check my memory", True)

    def reply(*args, **kwargs):
        assert kwargs["audio_admitted"] is True
        replied.set()
        return RealtimeTurnResult()

    with capture(monkeypatch, [(1, True, None), (4, False, None)],
                 realtime=True, configure_service=configure_audio_first(monkeypatch),
                 on_drain=drain, on_realtime=reply) as result:
        assert result.dispatch.call_args.args[2] == "Please check my memory"
        result.service._stt_drain_worker.shutdown(wait=True)


@pytest.mark.parametrize("speech,focus", [(False, True), (None, True), (True, False)])
def test_audio_alone_cannot_bypass_speech_or_wake_gate(monkeypatch, speech, focus):
    with capture(monkeypatch, [(1, True, None), (4, False, None)],
                 realtime=True, wake_enabled=True, focus=lambda: focus,
                 configure_service=configure_audio_first(monkeypatch, speech)) as result:
        assert not any(c.kwargs.get("audio_admitted") for c in result.realtime.call_args_list)
        if result.service._stt_drain_worker:
            result.service._stt_drain_worker.shutdown(wait=True)
