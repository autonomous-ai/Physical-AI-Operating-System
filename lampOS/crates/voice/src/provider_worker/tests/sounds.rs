//! A short sound let through in a quiet room (`LAMP_SHORT_SOUNDS=quiet-room`)
//! and checked (`quiet-room-checked`, #800), through the worker's own loop
//! against the scripted service: the request's sound is ended marked as a
//! sound, the service writes down what it heard, and the model answers.
use super::*;

fn quiet() -> (crate::room_told::Room, bool) {
    (crate::room_told::Room::Quiet, false)
}

fn checked(on: bool) -> Options {
    Options {
        reporting: Reporting::Typed,
        tools: crate::tools::Settings::default(),
        attention: Some(quiet),
        name_signal: false,
        sounds_checked: on,
    }
}

fn heard(text: &str) -> serde_json::Value {
    json!({"serverContent":{"inputTranscription":{"text":text}}})
}

/// A request whose sound the coordinator ended as a sound let through,
/// heard as `text`, and answered by the model: whether the answer's sound
/// came out of the worker.
async fn answered(options: Options, text: &str) -> bool {
    let mut rig = LiveRig::start_with(quick_policy(), options);
    let mut dial = rig.ready().await;
    let request = rig.speak(&mut dial, false, false).await;
    let owner = rig.snapshot.owner().unwrap();
    rig.owner.input_ended(real_time(), owner).unwrap();
    rig.publish();
    rig.parent
        .data
        .send(ProviderInput::End {
            request,
            privacy_generation: rig.snapshot.microphone_generation(),
            sound: true,
        })
        .unwrap();
    let ended = rig.service_message(&mut dial).await;
    assert!(
        ended["realtimeInput"].get("activityEnd").is_some(),
        "{ended}"
    );
    dial.service.send(heard(text)).await;
    assert!(matches!(
        rig.output().await,
        ProviderOutput::Transcript { request: None, .. }
    ));
    dial.service.send(provider_audio(9, 480)).await;
    dial.service.send(idle_complete()).await;
    let first = rig.output().await;
    let played = matches!(first, ProviderOutput::Audio { request: got, .. } if got == request);
    if !played {
        assert!(
            matches!(first, ProviderOutput::TurnComplete { request: done, .. } if done == request),
            "{first:?}"
        );
    }
    rig.stop().await;
    played
}

/// On lamp-52e6 on 2026-10-15 at 13:03 a room sound of 0.7 s, let through
/// in a quiet room, was heard as "¿Qué?" and answered. Checked, it is not;
/// a short word said to the lamp still is; and without the switch both are
/// answered, as before.
#[tokio::test]
async fn a_sound_heard_as_another_language_is_held_and_a_short_word_is_answered() {
    assert!(!answered(checked(true), "¿Qué?").await);
    assert!(answered(checked(true), "Yes.").await);
    assert!(answered(checked(false), "¿Qué?").await);
}

/// The worker reads `--sounds-checked` where the coordinator puts it, after
/// the first-output file and before the camera; and the second voice's
/// worker, which takes the same flags, takes it too, rather than refusing
/// it as unknown and leaving the lamp without its second voice, and checks
/// sounds with it (`second_provider`'s own test holds the answer).
#[test]
fn both_workers_take_the_flag_that_checks_sounds() {
    let dir = std::path::Path::new("/run/lamp-voice");
    let flags = |list: &[&str]| {
        list.iter()
            .map(|flag| (*flag).to_owned())
            .collect::<Vec<_>>()
    };
    let checked = flags(&[
        "--attention",
        crate::first_output::FLAG,
        "/run/lamp-voice/first-output",
        crate::not_speech::CHECKED_FLAG,
    ]);
    let (_, _, files, visual) = parse_flags(dir, &checked).unwrap();
    assert!(files.attention && files.sounds_checked && visual.is_none());
    let (_, _, files, _) = parse_flags(dir, &flags(&["--attention"])).unwrap();
    assert!(!files.sounds_checked);
    let (_, _, files, _) = crate::second_provider::worker::parse_flags(dir, &checked).unwrap();
    assert!(crate::second_provider::worker::checks_sounds(&files));
    // As on the Live route, only with the attention rule on.
    let (_, _, files, _) = crate::second_provider::worker::parse_flags(
        dir,
        &flags(&[crate::not_speech::CHECKED_FLAG]),
    )
    .unwrap();
    assert!(files.sounds_checked && !crate::second_provider::worker::checks_sounds(&files));
}
