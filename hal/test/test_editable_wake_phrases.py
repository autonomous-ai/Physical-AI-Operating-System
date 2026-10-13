"""Owner-edited wake phrases and the bare-wake acknowledgement."""

import json
import threading
import unicodedata

import pytest

from hal import config as hal_config
from hal.drivers.voice._internal.speaker_decorate import SpeakerDecorator


def _decorator(words: list[str]) -> SpeakerDecorator:
    decorator = object.__new__(SpeakerDecorator)
    decorator._wake_words = words
    decorator._wake_words_lock = threading.Lock()
    return decorator


def test_bare_wake_phrase_is_only_the_phrase():
    decorator = _decorator(["hey sam", "ê bé na ơi"])

    assert decorator.is_bare_wake_phrase("Hey Sam.")
    assert decorator.is_bare_wake_phrase("hey sam! Hey sam")
    assert decorator.is_bare_wake_phrase("Hey Sem")  # one typo in the name, as for the gate
    assert decorator.is_bare_wake_phrase("Ê, bé Na ơi!")
    assert not decorator.is_bare_wake_phrase("Hey Sam, what time is it?")
    assert not decorator.is_bare_wake_phrase("Hey Sam. What time is it?")
    assert not decorator.is_bare_wake_phrase("")
    assert not _decorator([]).is_bare_wake_phrase("hey sam")


def test_vietnamese_phrase_matches_across_unicode_forms():
    typed = unicodedata.normalize("NFC", "ê bé na ơi")
    heard = unicodedata.normalize("NFD", "Ê bé Na ơi, mấy giờ rồi?")
    decorator = _decorator([typed])

    assert decorator.starts_with_wake_word(heard)


def _write_config(tmp_path, monkeypatch, data):
    path = tmp_path / "config.json"
    path.write_text(json.dumps(data))
    monkeypatch.setattr(hal_config, "OS_CONFIG_PATH", str(path))


def test_custom_wake_phrases_reads_config(tmp_path, monkeypatch):
    _write_config(tmp_path, monkeypatch, {"wake_phrases": ["  Hey Sam ", "", 3, "Ê Bé Na ơi"]})
    assert hal_config.custom_wake_phrases() == ["hey sam", "ê bé na ơi"]

    _write_config(tmp_path, monkeypatch, {"wake_phrases": "hey sam"})
    assert hal_config.custom_wake_phrases() == []

    _write_config(tmp_path, monkeypatch, {})
    assert hal_config.custom_wake_phrases() == []


def _voice_service():
    from hal.drivers.voice.voice_service import VoiceService

    service = object.__new__(VoiceService)
    service._device_wake_words = ["hey autonomous", "hey intern-v2"]
    return service


def test_owner_phrases_replace_the_built_in_list(tmp_path, monkeypatch):
    service = _voice_service()

    _write_config(tmp_path, monkeypatch, {})
    assert service._effective_wake_words(["hey sam"]) == ["hey autonomous", "hey intern-v2", "hey sam"]

    _write_config(tmp_path, monkeypatch, {"wake_phrases": ["Ê bé na ơi", "hey sam"]})
    assert service._effective_wake_words(["hey sam"]) == ["ê bé na ơi", "hey sam"]


def test_set_wake_words_reapplies_owner_phrases(tmp_path, monkeypatch):
    service = _voice_service()
    service._decorator = _decorator([])
    _write_config(tmp_path, monkeypatch, {"wake_phrases": ["hey lisa"]})

    service.set_wake_words(["hey sam"])
    assert service._decorator._wake_words == ["hey lisa"]


def test_wake_chime_toggle_defaults_on(tmp_path, monkeypatch):
    _write_config(tmp_path, monkeypatch, {})
    assert hal_config.wake_chime_enabled()
    _write_config(tmp_path, monkeypatch, {"wake_chime": True})
    assert hal_config.wake_chime_enabled()
    _write_config(tmp_path, monkeypatch, {"wake_chime": False})
    assert not hal_config.wake_chime_enabled()


def test_name_slot_rewrite_confirms_only_with_full_prefix():
    decorator = _decorator(["hello sam", "hey sam", "ê bé na ơi"])

    assert decorator.matches_wake_prefix_slot("Hello, sir.")  # device-observed 07/10/2026
    assert decorator.matches_wake_prefix_slot("herself. Hello, sir?")
    assert decorator.matches_wake_prefix_slot("Hey, son, what time is it?")
    assert not decorator.matches_wake_prefix_slot("I sent.")
    assert not decorator.matches_wake_prefix_slot("she said hello there to me")


def test_bare_wake_accepts_a_rewritten_name_only_when_allowed():
    decorator = _decorator(["hello sam"])

    assert not decorator.is_bare_wake_phrase("Hello, sir.")
    assert decorator.is_bare_wake_phrase("Hello, sir.", allow_name_slot=True)
    assert not decorator.is_bare_wake_phrase("Hello, sir. What time is it?", allow_name_slot=True)


def _chime_tts(monkeypatch):
    from unittest.mock import MagicMock

    from hal.drivers.voice.tts.service import TTSService

    tts = object.__new__(TTSService)
    tts._stream_lock = threading.Lock()
    tts._np = MagicMock()
    tts._sd = MagicMock()
    tts._backend = None
    tts._device_rate = 48000
    tts._ensure_stream = MagicMock()
    monkeypatch.setattr(TTSService, "available", property(lambda self: True))
    tts._speaker_muted = lambda: False
    tts._ack_chime_samples = lambda rate: MagicMock()
    return tts


def test_button_style_chime_skips_instead_of_waiting_for_a_busy_speaker(monkeypatch):
    tts = _chime_tts(monkeypatch)

    with tts._stream_lock:  # another writer owns the speaker
        assert tts.try_play_ack_chime() is False
    tts._ensure_stream.assert_not_called()
    assert tts.try_play_ack_chime() is True
    tts._ensure_stream.return_value.write.assert_called_once()


def test_wake_followup_window_reads_the_owner_setting(tmp_path, monkeypatch):
    monkeypatch.setattr(hal_config, "WAKEWORD_FOLLOWUP_TIMEOUT_S", 10.0)
    _write_config(tmp_path, monkeypatch, {})
    assert hal_config.wake_followup_timeout_s() == 10.0
    _write_config(tmp_path, monkeypatch, {"wake_followup_s": 15})
    assert hal_config.wake_followup_timeout_s() == 15.0
    _write_config(tmp_path, monkeypatch, {"wake_followup_s": 1})
    assert hal_config.wake_followup_timeout_s() == hal_config.WAKE_FOLLOWUP_MIN_S
    _write_config(tmp_path, monkeypatch, {"wake_followup_s": 600})
    assert hal_config.wake_followup_timeout_s() == hal_config.WAKE_FOLLOWUP_MAX_S
    _write_config(tmp_path, monkeypatch, {"wake_followup_s": "15"})
    assert hal_config.wake_followup_timeout_s() == 10.0


def test_wake_chime_interrupts_a_reply_instead_of_skipping(monkeypatch):
    tts = _chime_tts(monkeypatch)
    tts._interruptible = True
    tts._stream_lock.acquire()  # a reply owns the speaker
    stopped = []

    def _stop(preserve_main_queue=False):
        stopped.append(preserve_main_queue)
        tts._stream_lock.release()  # the reply's writer yields the stream

    tts.stop = _stop
    assert tts.play_wake_chime() is True
    assert stopped == [True]
    tts._ensure_stream.return_value.write.assert_called_once()


def test_wake_chime_cuts_even_uninterruptible_speech(monkeypatch):
    tts = _chime_tts(monkeypatch)
    tts._interruptible = False
    tts._stream_lock.acquire()
    tts.stop = lambda preserve_main_queue=False: tts._stream_lock.release()

    assert tts.play_wake_chime() is True
    tts._ensure_stream.return_value.write.assert_called_once()

def test_wake_light_defaults_to_the_board_listening_look(tmp_path, monkeypatch):
    from hal import wake_light
    from hal.presets import EMO_LISTENING, EMOTION_PRESETS, FX_BLINK, FX_PULSE

    _write_config(tmp_path, monkeypatch, {})
    assert wake_light.enabled()
    assert wake_light.listening_preset() == EMOTION_PRESETS[EMO_LISTENING]

    _write_config(tmp_path, monkeypatch, {"wake_light": {"color": [0, 30, 60], "effect": "spin", "speed": 9}})
    preset = wake_light.listening_preset()
    assert preset["color"] == [0, 30, 60]
    assert preset["effect"] == FX_PULSE
    assert preset["speed"] == wake_light.SPEED_MAX

    _write_config(tmp_path, monkeypatch, {"wake_light": {"effect": "solid", "color": [0, 0, 0]}})
    preset = wake_light.listening_preset()
    assert "effect" not in preset
    assert preset["color"] == EMOTION_PRESETS[EMO_LISTENING]["color"]  # black is ignored
    assert wake_light.look_preset(effect="blink")["effect"] == FX_BLINK


def test_wake_light_cannot_be_off_together_with_the_chime(tmp_path, monkeypatch):
    from hal import wake_light

    _write_config(tmp_path, monkeypatch, {"wake_light": {"enabled": False}})
    assert not wake_light.enabled()
    _write_config(tmp_path, monkeypatch, {"wake_light": {"enabled": False}, "wake_chime": False})
    assert wake_light.enabled()  # a hand-edited config still gives the wake a cue


def test_wake_followup_route_reports_the_window_in_effect(tmp_path, monkeypatch):
    from hal.routes.voice import get_wake_followup

    monkeypatch.setattr(hal_config, "WAKEWORD_FOLLOWUP_TIMEOUT_S", 5.0)  # lamp image .env
    _write_config(tmp_path, monkeypatch, {})
    assert get_wake_followup() == {"seconds": 5.0, "image_default_s": 5.0}
    _write_config(tmp_path, monkeypatch, {"wake_followup_s": 12})
    assert get_wake_followup()["seconds"] == 12.0


def _intern(words: list[str]) -> SpeakerDecorator:
    decorator = _decorator(words)
    decorator._name_anywhere = True
    return decorator


def test_intern_wakes_on_a_name_anywhere_in_a_sentence():
    decorator = _intern(["autonomous", "intern"])

    assert decorator.starts_with_wake_word("Hey intern, turn on the light")
    assert decorator.starts_with_wake_word("What time is it, Autonomous?")
    assert decorator.starts_with_wake_word("Can you, intern, play some music")
    assert decorator.starts_with_wake_word("Intern ơi, mấy giờ rồi")
    # Whole words only: "internet" is not the name.
    assert not decorator.starts_with_wake_word("the internet is down")
    assert not decorator.starts_with_wake_word("turn on the light")


def test_intern_bare_wake_is_a_greeting_around_a_name():
    decorator = _intern(["autonomous", "intern"])

    assert decorator.is_bare_wake_phrase("Intern.")
    assert decorator.is_bare_wake_phrase("Hey intern!")
    assert decorator.is_bare_wake_phrase("Wake up, Autonomous")
    assert decorator.is_bare_wake_phrase("Intern ơi")
    assert decorator.is_bare_wake_phrase("Hello Intrn")  # one typo, as for the gate
    assert not decorator.is_bare_wake_phrase("Hey intern, what time is it?")
    assert not decorator.is_bare_wake_phrase("Hey")
    assert decorator.is_bare_wake_phrase("Hello sir", allow_name_slot=True)
    assert not decorator.is_bare_wake_phrase("Hello sir")


def test_intern_loose_and_name_slot_confirmation():
    decorator = _intern(["intern"])

    assert decorator.matches_wake_word_loosely("could you, intrn, read this")
    assert not decorator.matches_wake_word_loosely("read this please")
    assert decorator.matches_wake_prefix_slot("Hello sir. What time is it?")
    assert not decorator.matches_wake_prefix_slot("What time is it")


def test_lamp_keeps_phrases_at_a_sentence_edge():
    decorator = _decorator(["hey lamp"])

    assert decorator.starts_with_wake_word("hey lamp turn on")
    assert not decorator.starts_with_wake_word("can you hey lamp turn on")


def test_wake_name_of_drops_the_greeting():
    from hal.drivers.voice._internal.speaker_decorate import wake_name_of

    assert wake_name_of("Hey Sam") == "sam"
    assert wake_name_of("wake up sam") == "sam"
    assert wake_name_of("sam") == "sam"
    assert wake_name_of("ê bé na ơi") == "bé na"
