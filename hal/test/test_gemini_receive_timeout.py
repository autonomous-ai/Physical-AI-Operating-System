"""A missing Gemini terminal must not poison the next automatic turn."""

import threading
import time

import pytest

from hal import config
from hal.realtime.models import OutputEvent, TextOutput, TurnDoneEvent, UserSpeechOutput
from hal.realtime.orchestrator import RealtimeOrchestrator
from hal.realtime.voice_agent.base import VoiceAgentBase
from hal.realtime.voice_agent.gemini_live import GeminiLiveAgent


class OtherProvider(VoiceAgentBase):
    @property
    def sample_rate(self):
        return 16000

    def _do_connect(self): ...
    def _do_disconnect(self): ...
    def _send_loop(self): ...
    def _recv_loop(self): ...


@pytest.fixture
def agent(monkeypatch):
    monkeypatch.setattr(config, 'LIVE_MODE', False)
    monkeypatch.setattr(config, 'REALTIME_PROVIDER', 'gemini')
    monkeypatch.setattr(config, 'REALTIME_RECV_QUEUE_TIMEOUT_S', 0.001)
    instance = object.__new__(GeminiLiveAgent)
    VoiceAgentBase.__init__(instance)
    instance._connected.set()
    instance._committed_at = time.monotonic()
    instance._turn_done = threading.Event()
    instance._pending_tool_calls = set()
    instance._requires_fresh_session = False
    instance._turn_gen = 8
    return instance


@pytest.mark.parametrize('observation', [None, UserSpeechOutput(turn_id='u8', transcript='John, tea or coffee?'),
                                        TextOutput(text='Partial response')])
def test_timeout_retires_session_without_fake_completion_or_replay(agent, observation):
    if observation is not None:
        agent._recv_queue.put(OutputEvent(gen=8, output=observation))
    assert list(agent.receive()) == ([] if observation is None else [observation])
    assert agent.requires_fresh_session
    assert not agent._turn_done.is_set()
    assert not agent.execution_completed
    assert agent._turn_gen == 8
    assert agent._send_queue.empty()


def test_next_capture_replaces_timed_out_session_before_audio(agent):
    assert list(agent.receive()) == []
    orchestrator = object.__new__(RealtimeOrchestrator)
    orchestrator._agent = agent
    orchestrator._skip_post_idle_recycle = False
    replacement = OtherProvider()
    observed = []

    def rebuild(reason, **kwargs):
        observed.append((reason, kwargs))
        assert agent._send_queue.empty()
        orchestrator._agent = replacement
        return True

    orchestrator._rebuild_now = rebuild
    orchestrator.prepare_turn()
    assert orchestrator._agent is replacement
    assert observed == [('gemini-unresolved-tool-call', {'discard_old_on_failure': True})]
    assert orchestrator._skip_post_idle_recycle
    assert agent._send_queue.empty()


@pytest.mark.parametrize('case', ['live_idle', 'never_committed', 'terminal_seen', 'cancelled'])
def test_idle_completed_and_cancelled_consumers_do_not_trigger_timeout_replacement(monkeypatch, agent, case):
    stop = None
    if case == 'live_idle':
        monkeypatch.setattr(config, 'LIVE_MODE', True)
    elif case == 'never_committed':
        agent._committed_at = 0
    elif case == 'terminal_seen':
        agent._turn_done.set()
        agent._recv_queue.put(TurnDoneEvent(execution_completed=True))
    else:
        stop = threading.Event()
        stop.set()
    assert list(agent.receive(stop_event=stop)) == []
    assert not agent.requires_fresh_session
    assert agent._send_queue.empty()


def test_consumer_timeout_after_provider_terminal_does_not_rebuild(agent):
    agent._turn_done.set()
    assert list(agent.receive()) == []
    assert not agent.requires_fresh_session


def test_other_provider_timeout_keeps_existing_behavior(monkeypatch):
    monkeypatch.setattr(config, 'REALTIME_RECV_QUEUE_TIMEOUT_S', 0.001)
    agent = OtherProvider()
    agent._committed_at = time.monotonic()
    assert list(agent.receive()) == []
    assert not agent.requires_fresh_session
    assert not agent.execution_completed
    assert agent._send_queue.empty()


def test_timeout_quarantine_preserves_no_output_fallback_without_inventing_rejection(monkeypatch, agent):
    from unittest.mock import Mock
    from hal.drivers.voice._internal import realtime_turn as turn

    monkeypatch.setattr(config, 'REALTIME_ENABLED', True)
    monkeypatch.setattr(config, 'REALTIME_NATIVE_AUDIO', False)
    monkeypatch.setattr(config, 'REALTIME_MIN_COMMIT_DURATION_S', 0.4)
    monkeypatch.setattr(turn, 'gemini_needs_idle_workaround', lambda: False)
    monkeypatch.setattr(turn, 'pending_main_question', lambda: '')
    monkeypatch.setattr(turn, '_reply_language_name', lambda: 'English')
    monkeypatch.setattr(turn, '_thinking_cue_start', lambda: None)
    monkeypatch.setattr(turn, '_thinking_cue_clear', lambda: None)
    realtime = Mock(available=True)
    realtime.stream_output.side_effect = lambda **kwargs: agent.receive(**kwargs)
    result = turn.run_realtime_turn(
        realtime, None, lambda text: text, '', [b'audio'], 1.0,
        audio_is_speech=True, audio_admitted=True, harness_followup=False,
        wait_filler=Mock(),
    )
    assert agent.requires_fresh_session
    assert result.route == turn.ROUTE_NO_OUTPUT
    assert not result.rejected
    assert not result.handled
    assert not turn.should_drop_downstream_turn(result)
    realtime.recover_session.assert_not_called()
    realtime.append_audio.assert_not_called()
