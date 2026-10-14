"""Keep provider input and assistant output distinct across captured turn history."""

from unittest.mock import Mock

import pytest

from hal.drivers.voice._internal import realtime_turn, turn_dispatch
from hal.realtime.models import TextOutput, UserSpeechOutput
from hal.test.test_turn_endpoint_capture import capture
from hal.test.test_device_voice_pipeline import setup_pipeline  # noqa: F401
from hal.test.test_device_pipeline_realtime_admission import completed_turn

STT = "Hello maam"
QUERY = "What is the weather in Paris today?"
ANSWER = "It is sunny in Paris today."


@pytest.fixture
def history_environment(monkeypatch):
    monkeypatch.setattr(realtime_turn.hal_config, "REALTIME_ENABLED", True)
    monkeypatch.setattr(realtime_turn.hal_config, "REALTIME_NATIVE_AUDIO", False)
    monkeypatch.setattr(realtime_turn.hal_config, "REALTIME_PROVIDER", "gemini")
    monkeypatch.setattr(realtime_turn, "gemini_needs_idle_workaround", lambda: False)
    monkeypatch.setattr(realtime_turn, "_reply_language_name", lambda: "English")
    monkeypatch.setattr(realtime_turn, "_thinking_cue_start", lambda: None)
    monkeypatch.setattr(realtime_turn, "_thinking_cue_clear", lambda: None)
    monkeypatch.setattr(turn_dispatch, "voice_metrics", Mock())
    monkeypatch.setattr(turn_dispatch, "_take_vision_handoff", lambda **kw: ("", ""))
    monkeypatch.setattr(turn_dispatch, "_take_look_snapshot_marker", lambda: "")


def decorator():
    result = Mock(spec=["classify_wake_word", "identify_and_decorate", "submit_speech_emotion_from_session"])
    result.classify_wake_word.side_effect = lambda text: (text, "voice")
    result.identify_and_decorate.side_effect = lambda text, audio: (text, "known-user", "User")
    return result


@pytest.mark.parametrize("provider_input, expected", [(QUERY, QUERY), ("", STT), ("  ", STT)])
def test_capture_saves_and_syncs_provider_input_separately_from_reply(
        monkeypatch, history_environment, provider_input, expected):
    def configure(service):
        service._realtime.stream_output.return_value = iter([
            UserSpeechOutput(turn_id="input-1", transcript=provider_input), TextOutput(text=ANSWER),
        ])
        service.strip_rt_markers.side_effect = lambda text: text

    with capture(monkeypatch, [(1, True, STT), (4, False, None)], realtime=True,
                 configure_service=configure,
                 on_realtime=realtime_turn.run_realtime_turn) as result:
        result.service._realtime.save_turn.assert_called_once_with(
            user_text=expected, agent_text=ANSWER,
        )
        result.dispatch.assert_called_once()
        call = result.dispatch.call_args
        turn_dispatch.dispatch_turn(*call.args, **call.kwargs)
        sent = result.service._sensing_sender.send.call_args
        assert sent.args[0] == f"[skills: input-branching]\n[HANDLED] {expected}\n[REPLY] {ANSWER}"
        assert sent.kwargs["event_type"] == "voice_agent_handled"
        assert sent.kwargs["voice_turn_type"] == "voice"


@pytest.mark.parametrize("answered_for_main", [False, True])
def test_delegation_uses_provider_input_only_for_explicit_answered_policy(
        history_environment, answered_for_main):
    result = realtime_turn.RealtimeTurnResult(
        delegated=True, answered_for_main=answered_for_main, route="realtime_delegated",
        user_transcript=QUERY, transcript=ANSWER,
    )
    sender, identity = Mock(), decorator()
    turn_dispatch.dispatch_turn(identity, sender, STT, [], [], result,
                                event_type_override="voice_command")
    sent = sender.send.call_args
    expected = QUERY if answered_for_main else STT
    assert sent.args[0].split("\n", 1)[0] == expected
    assert ANSWER not in sent.args[0].split("\n", 1)[0]
    assert sent.kwargs["event_type"] == "voice_command"
    identity.classify_wake_word.assert_called_once_with(STT)


def test_pure_stt_fallback_keeps_local_transcript(history_environment):
    sender, identity = Mock(), decorator()
    result = realtime_turn.RealtimeTurnResult(route="realtime_no_output", user_transcript=QUERY)
    turn_dispatch.dispatch_turn(identity, sender, STT, [], [], result,
                                event_type_override="voice_followup")
    assert sender.send.call_args.args[0] == STT
    assert sender.send.call_args.kwargs["event_type"] == "voice_followup"


def test_manual_device_finalizer_syncs_provider_input_with_original_command_policy(
        setup_pipeline, monkeypatch, history_environment):
    from hal.drivers.voice._internal import device_voice_pipeline

    pipeline = setup_pipeline.pipeline
    pipeline.decorator = decorator()
    pipeline.sensing_sender = Mock()
    pipeline.realtime_turn = Mock(return_value=realtime_turn.RealtimeTurnResult(
        route="realtime_handled", handled=True, user_transcript=QUERY, transcript=ANSWER,
    ))
    monkeypatch.setattr(device_voice_pipeline, "dispatch_turn", turn_dispatch.dispatch_turn)
    turn = completed_turn(STT)
    try:
        pipeline._finalize(turn)
    finally:
        pipeline._cleanup(turn)
    sent = pipeline.sensing_sender.send.call_args
    assert sent.args[0] == f"[skills: input-branching]\n[HANDLED] {QUERY}\n[REPLY] {ANSWER}"
    assert sent.kwargs["event_type"] == "voice_agent_handled"
    assert sent.kwargs["voice_turn_type"] == "voice_command"


def test_answered_sync_preserves_speaker_metadata_and_original_wake_classification(history_environment):
    sender, identity = Mock(), decorator()
    identity.classify_wake_word.side_effect = lambda text: (STT, "voice_command")
    result = realtime_turn.RealtimeTurnResult(
        handled=True, user_transcript=QUERY, transcript=ANSWER, route="realtime_handled",
    )
    turn_dispatch.dispatch_turn(
        identity, sender, "Hey Sam, " + STT, [], [], result,
        identity=(f"[user: Leo]\n{STT}\n[enrollment: pending]", "leo-id", "Leo"),
    )
    assert sender.send.call_args.args[0] == (
        f"[skills: input-branching]\n[HANDLED] [user: Leo]\n{QUERY}\n"
        f"[enrollment: pending]\n[REPLY] {ANSWER}"
    )
    assert sender.send.call_args.kwargs["voice_turn_type"] == "voice_command"
    identity.classify_wake_word.assert_called_once_with("Hey Sam, " + STT)
    identity.identify_and_decorate.assert_not_called()
    identity.submit_speech_emotion_from_session.assert_called_once_with([], user="leo-id")


@pytest.mark.parametrize("provider_input", [QUERY, ""])
def test_early_reply_history_uses_provider_input_after_late_stt_drain(
        monkeypatch, history_environment, provider_input):
    import threading

    closing, replied = threading.Event(), threading.Event()
    local_text = "Please check my memory"
    late_text = "and include yesterday"
    expected = provider_input or f"{local_text} {late_text}"

    def close():
        closing.set()
        assert replied.wait(2), "realtime was blocked by STT draining"

    def reply(*args, **kwargs):
        assert closing.wait(2)
        assert kwargs["save_history"] is False
        replied.set()
        return realtime_turn.RealtimeTurnResult(
            handled=True, route="realtime_handled", user_transcript=provider_input,
            transcript=ANSWER,
        )

    with capture(monkeypatch, [(1, True, local_text), (4, False, None)], realtime=True,
                 on_close=close, on_realtime=reply, close_transcript=late_text) as result:
        result.service._realtime.save_turn.assert_called_once_with(
            user_text=expected, agent_text=ANSWER,
        )
        call = result.dispatch.call_args
        turn_dispatch.dispatch_turn(*call.args, **call.kwargs)
        assert result.service._sensing_sender.send.call_args.args[0] == (
            f"[skills: input-branching]\n[HANDLED] {expected}\n[REPLY] {ANSWER}"
        )


def test_provider_input_replacement_cannot_replace_speaker_name(history_environment):
    sender, identity = Mock(), decorator()
    result = realtime_turn.RealtimeTurnResult(
        handled=True, user_transcript=QUERY, transcript=ANSWER, route="realtime_handled",
    )
    turn_dispatch.dispatch_turn(
        identity, sender, "Leo", [], [], result,
        identity=("Speaker - Leo: Leo", "leo-id", "Leo"),
    )
    assert sender.send.call_args.args[0] == (
        f"[skills: input-branching]\n[HANDLED] Speaker - Leo: {QUERY}\n[REPLY] {ANSWER}"
    )


def test_provider_input_replacement_preserves_unknown_speaker_prefix(history_environment):
    sender, identity = Mock(), decorator()
    result = realtime_turn.RealtimeTurnResult(
        handled=True, user_transcript=QUERY, transcript=ANSWER, route="realtime_handled",
    )
    suffix = " (audio saved at /tmp/unknown.wav)"
    turn_dispatch.dispatch_turn(
        identity, sender, "Speaker", [], [], result,
        identity=("Unknown Speaker: [voice:abc] Speaker" + suffix, "unknown", None),
    )
    assert sender.send.call_args.args[0] == (
        f"[skills: input-branching]\n[HANDLED] Unknown Speaker: [voice:abc] {QUERY}"
        f"{suffix}\n[REPLY] {ANSWER}"
    )
