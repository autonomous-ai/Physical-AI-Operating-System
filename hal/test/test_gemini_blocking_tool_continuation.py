"""Blocking auxiliary tools keep their user's answer on the same consumer turn."""

import asyncio
import queue
import threading
from types import SimpleNamespace

import pytest
from google.genai.live import AsyncSession

from hal.realtime.models import FunctionCallOutput, FunctionCallResultInput, OutputEvent, TextOutput, TurnDoneEvent
from hal.realtime.voice_agent import gemini_live
from hal.realtime.voice_agent.gemini_live import GeminiLiveAgent


def message(*, tool=None, text=None, terminal=False, generation=False):
    return SimpleNamespace(
        usage_metadata=None, session_resumption_update=None, go_away=None,
        voice_activity=None,
        tool_call=SimpleNamespace(function_calls=[SimpleNamespace(name=tool, id='aux', args={})]) if tool else None,
        server_content=SimpleNamespace(
            grounding_metadata=None, model_turn=None, input_transcription=None,
            output_transcription=SimpleNamespace(text=text) if text else None,
            interrupted=False, turn_complete=terminal, generation_complete=generation,
        ),
    )


class Session:
    receive = AsyncSession.receive

    def __init__(self, messages):
        self.messages = asyncio.Queue()
        for item in messages:
            self.messages.put_nowait(item)
        self.waiting = asyncio.Event()
        self.responses = []

    async def _receive(self):
        if self.messages.empty():
            self.waiting.set()
        return await self.messages.get()

    async def send_tool_response(self, *, function_responses):
        self.responses.extend(function_responses)


def agent_for(messages):
    agent = object.__new__(GeminiLiveAgent)
    agent._session = Session(messages)
    agent._config = SimpleNamespace(model='gemini-3.8-live')
    agent._recv_queue = queue.Queue()
    agent._turn_done = threading.Event()
    agent._pending_tool_calls = set()
    agent._gated_audio_frames = 0
    agent._first_audio_received = False
    agent._last_audio_sent_at = None
    agent._activity_end_sent_at = None
    agent._turn_gen = 0
    agent._user_transcript = 'Why is the sky blue?'
    return agent


@pytest.mark.parametrize('pre_ack_text', [False, True])
@pytest.mark.parametrize('same_message', [False, True])
@pytest.mark.parametrize('generation', [False, True])
def test_auxiliary_tool_boundary_waits_for_ack_then_spoken_answer(monkeypatch, same_message, generation, pre_ack_text):
    monkeypatch.setattr(gemini_live.app_config, 'LIVE_MODE', False)
    boundary = dict(terminal=not generation, generation=generation)
    messages = ([message(tool='express_emotion', **boundary)] if same_message else
                [message(tool='express_emotion'), message(**boundary)])

    if pre_ack_text:
        messages.insert(1, message(text='Let me think.', terminal=True))

    async def run():
        agent = agent_for(messages)
        task = asyncio.create_task(agent._async_receive_turn())
        try:
            await asyncio.wait_for(agent._session.waiting.wait(), 1)
            assert not task.done()
            assert not agent._turn_done.is_set()
            events = list(agent._recv_queue.queue)
            assert not any(isinstance(event, TurnDoneEvent) for event in events)
            assert sum(isinstance(event, OutputEvent) and isinstance(event.output, FunctionCallOutput)
                       for event in events) == 1
            generation_before = agent._turn_gen
            await agent._async_send_input(FunctionCallResultInput(call_id='aux', output='{"result":"ok"}'))
            assert len(agent._session.responses) == 1
            assert not agent._pending_tool_calls
            agent._session.messages.put_nowait(message(text='Blue light scatters more.', terminal=True))
            await asyncio.wait_for(task, 1)
            assert agent._turn_gen == generation_before
            events = list(agent._recv_queue.queue)
            assert sum(isinstance(event, TurnDoneEvent) for event in events) == 1
            answers = [event.output.text for event in events if isinstance(event, OutputEvent)
                       and isinstance(event.output, TextOutput)]
            assert answers == (['Let me think.'] if pre_ack_text else []) + ['Blue light scatters more.']
            assert agent._turn_done.is_set()
        finally:
            if not task.done():
                task.cancel()
                await asyncio.gather(task, return_exceptions=True)

    asyncio.run(run())


@pytest.mark.parametrize('routing', ['reject_turn', 'delegate_to_main'])
def test_routing_tool_after_auxiliary_still_ends_turn(monkeypatch, routing):
    monkeypatch.setattr(gemini_live.app_config, 'LIVE_MODE', False)
    agent = agent_for([message(tool='express_emotion'), message(terminal=True),
                       message(tool=routing, terminal=True)])
    asyncio.run(asyncio.wait_for(agent._async_receive_turn(), 1))
    events = list(agent._recv_queue.queue)
    assert any(isinstance(event, OutputEvent) and isinstance(event.output, FunctionCallOutput)
               and event.output.name == routing for event in events)
    assert sum(isinstance(event, TurnDoneEvent) for event in events) == 1


def test_cancelled_auxiliary_wait_does_not_invent_completion(monkeypatch):
    monkeypatch.setattr(gemini_live.app_config, 'LIVE_MODE', False)

    async def run():
        agent = agent_for([message(tool='express_emotion'), message(terminal=True)])
        task = asyncio.create_task(agent._async_receive_turn())
        await asyncio.wait_for(agent._session.waiting.wait(), 1)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await asyncio.wait_for(task, 1)
        assert not agent._turn_done.is_set()
        assert not any(isinstance(event, TurnDoneEvent) for event in agent._recv_queue.queue)

    asyncio.run(run())
