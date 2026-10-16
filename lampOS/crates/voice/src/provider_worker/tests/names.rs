//! A name told is a name kept (`LAMP_NAMES=checked`, the first builder at
//! 13:43 on 16 October), through the worker's own loop against the
//! scripted service: once what the person said shows them giving their
//! name, the model is told beside that request to keep it.
use super::*;

fn options(checked: bool) -> Options {
    Options {
        tools: crate::tools::Settings {
            who: Some("/run/lampos-eyes/who.sock".into()),
            names_checked: checked,
            ..Default::default()
        },
        ..Options::reporting(Reporting::Typed)
    }
}

fn heard(text: &str) -> serde_json::Value {
    json!({"serverContent":{"inputTranscription":{"text":text}}})
}

/// A request in which the person said `text`: whether the service was told
/// to keep a name with it. A note is written as the request is committed,
/// just before its end.
async fn told_to_keep(checked: bool, text: &str) -> bool {
    let mut rig = LiveRig::start_with(quick_policy(), options(checked));
    let mut dial = rig.ready().await;
    let request = rig.speak(&mut dial, false, false).await;
    dial.service.send(heard(text)).await;
    // The loop takes the words, and adds what it adds beside the request.
    let settled = Instant::now() + Duration::from_millis(200);
    while Instant::now() < settled {
        rig.publish();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let owner = rig.snapshot.owner().unwrap();
    rig.owner.input_ended(real_time(), owner).unwrap();
    rig.publish();
    rig.parent
        .data
        .send(ProviderInput::End {
            request,
            privacy_generation: rig.snapshot.microphone_generation(),
            sound: false,
            cut_in: false,
        })
        .unwrap();
    let mut told = false;
    loop {
        let message = rig.service_message(&mut dial).await;
        if message["realtimeInput"]["text"]
            .as_str()
            .is_some_and(|text| text.contains(crate::known_faces::NAME_TOLD_NOTE))
        {
            told = true;
        }
        if message["realtimeInput"].get("activityEnd").is_some() {
            break;
        }
    }
    rig.stop().await;
    told
}

#[tokio::test]
async fn with_names_checked_a_name_given_is_told_to_be_kept_and_plain_talk_is_not() {
    assert!(
        told_to_keep(true, "Hi Lamp, my name is Jim.").await,
        "a name given was not told"
    );
    assert!(
        !told_to_keep(true, "Hi Lamp, what time is it?").await,
        "plain talk was told"
    );
    assert!(
        !told_to_keep(false, "Hi Lamp, my name is Jim.").await,
        "told without the switch"
    );
}
