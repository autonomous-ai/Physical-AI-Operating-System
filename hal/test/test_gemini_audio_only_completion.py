"""Server IDLE can complete native PCM without requiring a text mirror."""

from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from hal.realtime.models import AudioOutput, OutputEvent, TurnDoneEvent
from hal.realtime.voice_agent import gemini_live
from hal.drivers.voice._internal import realtime_turn
from hal.test.test_gemini_nonblocking_grace import _agent, _receive, _status, _terminal


def audio_message(*, thought=False, idle=False, samples=160):
    message = _status('IDLE') if idle else _terminal()
    message.server_content.turn_complete = idle
    message.server_content.model_turn = SimpleNamespace(parts=[SimpleNamespace(
        thought=thought, text=None, inline_data=SimpleNamespace(data=b'\x01\x00' * samples),
    )])
    return message


@pytest.fixture(autouse=True)
def config(monkeypatch):
    monkeypatch.setattr(gemini_live.app_config, 'LIVE_MODE', False)
    monkeypatch.setattr(gemini_live.app_config, 'REALTIME_NONBLOCKING_TOOL_GRACE_S', .02)
    monkeypatch.setattr(gemini_live.app_config, 'REALTIME_OUTCOME_TIMEOUT_S', .02)
    monkeypatch.setattr(gemini_live.app_config, 'REALTIME_RECV_QUEUE_TIMEOUT_S', .04)


@pytest.mark.parametrize('same_packet', [False, True])
def test_idle_with_nonthought_pcm_completes_without_output_transcription(same_packet):
    messages = [audio_message(idle=True)] if same_packet else [audio_message(), _status('IDLE')]
    events = _receive(_agent(messages))
    assert any(isinstance(event, OutputEvent) and isinstance(event.output, AudioOutput) for event in events)
    done = [event for event in events if isinstance(event, TurnDoneEvent)]
    assert len(done) == 1
    assert done[0].execution_completed
    assert not done[0].fallback_to_main


@pytest.mark.parametrize('messages', [[_status('IDLE')], [audio_message(thought=True), _status('IDLE')]])
def test_empty_or_thought_only_idle_does_not_claim_answer(messages):
    events = _receive(_agent(messages))
    assert events[-1].fallback_to_main
    assert not events[-1].execution_completed
    assert not any(isinstance(event, OutputEvent) and isinstance(event.output, AudioOutput) for event in events)


def test_pending_tool_still_requires_fallback_despite_pcm_and_idle():
    agent = _agent([audio_message(), _status('IDLE')])
    agent._pending_tool_calls = {'unfinished-action'}
    events = _receive(agent)
    assert events[-1].fallback_to_main
    assert not events[-1].execution_completed


def test_pcm_without_idle_cannot_bypass_extended_outcome_watchdog():
    events = _receive(_agent([audio_message(), _terminal(generation=True)]))
    assert events[-1].fallback_to_main
    assert not events[-1].execution_completed


def test_overflowed_unvalidated_continuation_still_falls_back_at_idle():
    events = _receive(_agent([
        audio_message(), _terminal(generation=True), audio_message(samples=1_000_001), _status('IDLE'),
    ]))
    assert events[-1].fallback_to_main
    assert not events[-1].execution_completed


def test_pcm_evidence_is_not_inherited_by_next_response():
    agent = _agent([audio_message(), _status('IDLE')])
    assert _receive(agent)[-1].execution_completed
    agent._session.messages.put_nowait(_status('IDLE'))
    events = _receive(agent)
    assert events[-1].fallback_to_main
    assert not events[-1].execution_completed


def test_interruption_does_not_turn_pcm_into_success():
    interrupted = _status('IDLE')
    interrupted.server_content.interrupted = True
    events = _receive(_agent([audio_message(), interrupted]))
    assert not events[-1].execution_completed


@pytest.mark.parametrize('native', [True, False])
def test_renderer_only_claims_audio_only_response_when_native_playback_occurs(monkeypatch, native):
    events = _receive(_agent([audio_message(), _status('IDLE')]))
    outputs = [event.output for event in events if isinstance(event, OutputEvent)]
    done = events[-1]
    assert done.execution_completed
    for name, value in [('REALTIME_ENABLED', True), ('REALTIME_PROVIDER', 'gemini'),
                        ('REALTIME_NATIVE_AUDIO', native)]:
        monkeypatch.setattr(realtime_turn.hal_config, name, value)
    monkeypatch.setattr(realtime_turn, 'native_voice', lambda tts: None)
    monkeypatch.setattr(realtime_turn, 'gemini_needs_idle_workaround', lambda: False)
    monkeypatch.setattr(realtime_turn, 'pending_main_question', lambda: '')
    monkeypatch.setattr(realtime_turn, '_reply_language_name', lambda: 'English')
    monkeypatch.setattr(realtime_turn, '_thinking_cue_start', Mock())
    monkeypatch.setattr(realtime_turn, '_thinking_cue_clear', Mock())
    realtime = Mock(available=True, execution_completed=done.execution_completed,
                    intentional_silence=False, output_sample_rate=24000)
    realtime.stream_output.return_value = iter(outputs)
    tts = Mock()
    tts.native_play_begin.return_value = True
    result = realtime_turn.run_realtime_turn(
        realtime, tts, lambda text: text, 'Name one fruit', [b'audio'], 1.0,
        harness_followup=False, wait_filler=Mock(),
    )
    assert result.handled is native
    assert result.route == (realtime_turn.ROUTE_HANDLED if native else realtime_turn.ROUTE_NO_OUTPUT)
    assert tts.native_play_frame.called is native
    tts.speak.assert_not_called()
