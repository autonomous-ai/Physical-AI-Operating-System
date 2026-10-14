"""Verified audio admission must preserve routing and separate speech roles."""

from unittest.mock import Mock

import pytest

from hal.drivers.voice._internal import realtime_turn as turn
from hal.realtime.models import TextOutput, UserSpeechOutput
from hal.realtime.models.signal import DelegateSignal, RejectSignal


@pytest.fixture
def run_turn(monkeypatch):
    monkeypatch.setattr(turn.hal_config, 'REALTIME_ENABLED', True)
    monkeypatch.setattr(turn.hal_config, 'REALTIME_NATIVE_AUDIO', False)
    monkeypatch.setattr(turn.hal_config, 'REALTIME_PROVIDER', 'gemini')
    monkeypatch.setattr(turn.hal_config, 'REALTIME_REQUIRE_TRANSCRIPT', True)
    monkeypatch.setattr(turn.hal_config, 'REALTIME_MIN_COMMIT_DURATION_S', 0.4)
    monkeypatch.setattr(turn, 'gemini_needs_idle_workaround', lambda: False)
    monkeypatch.setattr(turn, 'pending_main_question', lambda: '')
    monkeypatch.setattr(turn, '_reply_language_name', lambda: 'English')
    monkeypatch.setattr(turn, '_thinking_cue_start', Mock())
    monkeypatch.setattr(turn, '_thinking_cue_clear', Mock())
    realtime = Mock(available=True)
    tts = Mock()

    def run(outputs=(), **kwargs):
        realtime.stream_output.return_value = iter(outputs)
        options = dict(combined='', buf_duration=1.0, audio_is_speech=True,
                       audio_admitted=True, harness_followup=False)
        options.update(kwargs)
        result = turn.run_realtime_turn(
            realtime, tts, lambda text: text, rt_audio_buffer=[b'audio'],
            wait_filler=Mock(), **options,
        )
        return result, realtime, tts

    return run


def test_verified_gemini_audio_commits_without_stt(run_turn):
    result, realtime, tts = run_turn([TextOutput(text='The answer is thirty-six.')])
    realtime.commit_audio.assert_called_once()
    assert result.handled
    assert result.transcript == 'The answer is thirty-six.'
    tts.speak.assert_called_once()


@pytest.mark.parametrize('provider,speech,duration,admitted', [
    ('openai', True, 1.0, True),
    ('gemini', False, 1.0, True),
    ('gemini', None, 1.0, True),
    ('gemini', True, 0.399, True),
    ('gemini', True, 1.0, False),
])
def test_admission_cannot_bypass_provider_speech_duration_or_opt_in(
    monkeypatch, run_turn, provider, speech, duration, admitted,
):
    monkeypatch.setattr(turn.hal_config, 'REALTIME_PROVIDER', provider)
    result, realtime, tts = run_turn(
        audio_is_speech=speech, buf_duration=duration, audio_admitted=admitted,
    )
    realtime.commit_audio.assert_not_called()
    tts.speak.assert_not_called()
    assert not result.handled
    assert result.route == turn.ROUTE_NOISE_DROPPED


def test_default_caller_still_requires_transcript(monkeypatch):
    monkeypatch.setattr(turn.hal_config, 'REALTIME_ENABLED', True)
    monkeypatch.setattr(turn.hal_config, 'REALTIME_PROVIDER', 'gemini')
    monkeypatch.setattr(turn.hal_config, 'REALTIME_REQUIRE_TRANSCRIPT', True)
    realtime = Mock(available=True)
    result = turn.run_realtime_turn(
        realtime, None, lambda text: text, '', [b'audio'], 1.0,
        audio_is_speech=True, harness_followup=False,
    )
    realtime.commit_audio.assert_not_called()
    assert result.route == turn.ROUTE_NOISE_DROPPED


def test_user_transcription_is_carried_and_saved_without_becoming_reply(run_turn):
    result, realtime, tts = run_turn([
        UserSpeechOutput(turn_id='one', transcript='What is twelve '),
        UserSpeechOutput(turn_id='one', transcript='times three?'),
        UserSpeechOutput(turn_id='one', transcript_finished=True),
        TextOutput(text='Thirty-six.'),
    ])
    assert result.user_transcript == 'What is twelve times three?'
    assert result.transcript == 'Thirty-six.'
    realtime.save_turn.assert_called_once_with(
        user_text='What is twelve times three?', agent_text='Thirty-six.',
    )
    assert tts.speak.call_args.args == ('Thirty-six.',)


def test_user_transcription_alone_is_not_a_handled_answer(run_turn):
    result, realtime, tts = run_turn([
        UserSpeechOutput(turn_id='one', transcript='What is twelve times three?'),
    ])
    assert result.user_transcript == 'What is twelve times three?'
    assert result.transcript == ''
    assert not result.handled
    realtime.save_turn.assert_not_called()
    tts.speak.assert_not_called()


@pytest.mark.parametrize('signal', [RejectSignal(), DelegateSignal(message='Check my calendar')])
def test_admitted_audio_keeps_rejection_and_delegation_authoritative(run_turn, signal):
    result, realtime, tts = run_turn([
        UserSpeechOutput(turn_id='one', transcript='Check my calendar'), signal,
    ])
    realtime.commit_audio.assert_called_once()
    assert not result.handled
    assert result.user_transcript == 'Check my calendar'
    assert result.rejected is isinstance(signal, RejectSignal)
    assert result.delegated is isinstance(signal, DelegateSignal)
    realtime.save_turn.assert_not_called()
    tts.speak.assert_not_called()


def test_explicit_tap_keeps_its_existing_provider_and_duration_policy(monkeypatch, run_turn):
    monkeypatch.setattr(turn.hal_config, 'REALTIME_PROVIDER', 'openai')
    _, realtime, _ = run_turn(explicit_capture=True, buf_duration=0.1)
    realtime.commit_audio.assert_called_once()
