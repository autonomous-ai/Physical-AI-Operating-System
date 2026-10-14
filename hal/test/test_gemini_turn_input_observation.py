"""Turn-mode input transcription is observable without enabling live lifecycle."""

import asyncio
import queue
import threading
from types import SimpleNamespace

import pytest

from hal.realtime.models import OutputEvent, TextOutput, UserSpeechOutput
from hal.realtime.voice_agent import gemini_live
from hal.realtime.voice_agent.gemini_live import GeminiLiveAgent


class Session:
    def __init__(self, messages):
        self.messages = messages

    async def receive(self):
        for message in self.messages:
            yield message


def message(text, *, finish=False):
    return SimpleNamespace(
        usage_metadata=None, tool_call=None, session_resumption_update=None,
        go_away=None,
        server_content=SimpleNamespace(
            grounding_metadata=None, model_turn=None, output_transcription=None,
            input_transcription=SimpleNamespace(text=text, finished=finish),
            interrupted=False, turn_complete=finish, generation_complete=False,
        ),
    )


@pytest.mark.parametrize('live', [False, True])
def test_actual_provider_input_observation_does_not_become_assistant_output(monkeypatch, live):
    monkeypatch.setattr(gemini_live.app_config, 'LIVE_MODE', live)
    agent = object.__new__(GeminiLiveAgent)
    agent._session = Session([message('What is twelve '), message('times three?', finish=True)])
    agent._first_audio_received = True
    agent._recv_queue = queue.Queue()
    agent._turn_done = threading.Event()
    agent._turn_gen = 8
    agent._user_transcript = ''
    agent._live_user_turn_id = ''
    agent._live_speech_emitted = False

    asyncio.run(asyncio.wait_for(agent._async_receive_turn(), timeout=1))
    events = []
    while not agent._recv_queue.empty():
        events.append(agent._recv_queue.get_nowait())
    observations = [event for event in events if isinstance(event, OutputEvent)
                    and isinstance(event.output, UserSpeechOutput)]
    assert ''.join(event.output.transcript for event in observations) == 'What is twelve times three?'
    assert len(observations) == 2
    assert all(event.gen == agent._turn_gen for event in observations)
    assert all(event.output.endpoint_at is None for event in observations)
    assert not any(isinstance(event, OutputEvent) and isinstance(event.output, TextOutput)
                   for event in events)
    if not live:
        assert not agent._live_user_turn_id
        assert not agent._live_speech_emitted
        assert all(event.output.turn_id == f'gemini-turn-{agent._turn_gen}' for event in observations)
    else:
        assert all(event.output.turn_id.startswith('gemini-') for event in observations)
        assert all(not event.output.turn_id.startswith('gemini-turn-') for event in observations)
