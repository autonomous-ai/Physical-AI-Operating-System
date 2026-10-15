//! The lamp speaks when spoken to (`LAMP_SPOKEN_TO=on`, #829, step 2),
//! through the worker's own loop against the scripted service: in a room
//! not known to be busy and outside a conversation with the lamp, an
//! answer nobody asked the lamp for is held, and so is any lamp action it
//! calls for.
use super::*;

fn quiet() -> (crate::room_told::Room, bool) {
    (crate::room_told::Room::Quiet, false)
}

fn quiet_going_on() -> (crate::room_told::Room, bool) {
    (crate::room_told::Room::Quiet, true)
}

fn options(
    spoken_to: bool,
    room: fn() -> (crate::room_told::Room, bool),
    tools: crate::tools::Settings,
) -> Options {
    Options {
        reporting: Reporting::Typed,
        tools,
        attention: Some(room),
        name_signal: false,
        sounds_checked: false,
        name_in_passing_ignored: false,
        policy_holds: None,
        wait_for_room: false,
        spoken_to,
    }
}

fn heard(text: &str) -> serde_json::Value {
    json!({"serverContent":{"inputTranscription":{"text":text}}})
}

/// The first builder's two-person scene (#829, 02:16): the opening line,
/// said by one person to another, was answered 11 times in 12. And a line
/// heard from across the office on lamp-4ace at 20:51 on 15 October.
const NOT_TO_THE_LAMP: [&str; 2] = [
    "Did you get my message about the budget?",
    "Voglio mangiare sushi.",
];
const TO_THE_LAMP: &str = "Hey Lamp, what is the weather like in Hanoi today?";

/// A request heard as `text` and answered: whether the answer's sound came
/// out of the worker.
async fn answered(options: Options, text: &str) -> bool {
    let mut rig = LiveRig::start_with(quick_policy(), options);
    let mut dial = rig.ready().await;
    let request = rig.speak(&mut dial, false, true).await;
    dial.service.send(heard(text)).await;
    dial.service.send(provider_audio(7, 240)).await;
    dial.service.send(idle_complete()).await;
    let mut played = false;
    loop {
        match rig.output().await {
            ProviderOutput::Audio { request: of, .. } if of == request => played = true,
            ProviderOutput::TurnComplete { request: of, .. } if of == request => break,
            _ => {}
        }
    }
    rig.stop().await;
    played
}

#[tokio::test]
async fn what_nobody_said_to_the_lamp_is_held_with_the_switch_and_played_without() {
    let tools = crate::tools::Settings::default;
    for text in NOT_TO_THE_LAMP {
        assert!(
            !answered(options(true, quiet, tools()), text).await,
            "{text}"
        );
        assert!(
            answered(options(false, quiet, tools()), text).await,
            "{text}"
        );
    }
}

#[tokio::test]
async fn the_lamp_called_by_name_or_in_a_conversation_is_answered_with_the_switch() {
    let tools = crate::tools::Settings::default;
    assert!(answered(options(true, quiet, tools()), TO_THE_LAMP).await);
    // B's follow-up in the first builder's scene, "And what about
    // tomorrow?", while the lamp is in a conversation.
    assert!(
        answered(
            options(true, quiet_going_on, tools()),
            "And what about tomorrow?"
        )
        .await
    );
}

/// A request heard as `text` whose answer calls for the lamp action
/// `action`: what the service was answered, and what the executive was
/// asked, if it was asked at all.
async fn acted(
    spoken_to: bool,
    text: &str,
    action: &str,
) -> (serde_json::Value, Option<serde_json::Value>) {
    use std::io::{BufRead, Write};
    let home = crate::process::SessionDirectory::create().unwrap();
    let listener =
        std::os::unix::net::UnixListener::bind(home.path.join(crate::tools::CALL_SOCKET)).unwrap();
    listener.set_nonblocking(true).unwrap();
    // The executive, which changes the lamp if it is asked, until the
    // service has its answer.
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let executive = std::thread::spawn({
        let done = done.clone();
        move || {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let mut line = String::new();
                        std::io::BufReader::new(stream.try_clone().unwrap())
                            .read_line(&mut line)
                            .unwrap();
                        stream
                            .write_all(b"{\"ok\":true,\"result\":{\"changed\":true}}\n")
                            .unwrap();
                        return Some(serde_json::from_str::<serde_json::Value>(&line).unwrap());
                    }
                    Err(_) if done.load(std::sync::atomic::Ordering::Acquire) => return None,
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(2)),
                }
            }
        }
    });
    let mut rig = LiveRig::start_with(
        quick_policy(),
        options(
            spoken_to,
            quiet,
            crate::tools::Settings {
                decline: false,
                exec: Some(home.path.clone()),
                memory: None,
                clock: false,
                ..Default::default()
            },
        ),
    );
    let mut dial = rig.ready().await;
    rig.speak(&mut dial, false, true).await;
    dial.service.send(heard(text)).await;
    assert!(matches!(
        rig.output().await,
        ProviderOutput::Transcript { request: None, .. }
    ));
    dial.service.send(function_call("c9", action)).await;
    // A call that is run is answered only after the executive has done it.
    let answer = rig.service_message(&mut dial).await;
    done.store(true, std::sync::atomic::Ordering::Release);
    let asked = executive.join().unwrap();
    rig.stop().await;
    (answer, asked)
}

/// The first builder's first test for step 2 (#829, 02:00): on 15 October
/// at 14:49 a word overheard in the office's talk turned the lamp's volume
/// up for 11 hours. A turn nobody said to the lamp never runs a lamp
/// action; without the switch, as before, it does.
#[tokio::test]
async fn a_turn_nobody_said_to_the_lamp_never_changes_the_lamp() {
    for action in ["volume_up", "volume_mute", "light_on", "light_colour"] {
        for text in NOT_TO_THE_LAMP.into_iter().chain(["Thailand."]) {
            let (answer, asked) = acted(true, text, action).await;
            assert_eq!(
                answer,
                json!({"toolResponse":{"functionResponses":[
                    {"id":"c9","name":action,"response":{"ok":true}}
                ]}}),
                "{action} {text}"
            );
            assert_eq!(asked, None, "{action} {text}");
        }
        let (_, asked) = acted(false, NOT_TO_THE_LAMP[0], action).await;
        assert_eq!(
            asked,
            Some(json!({"op":"call","name":action,"args":{"colour":"blue"}})),
            "{action}"
        );
    }
}

/// Its second: the same turn with the lamp's name changes the lamp.
#[tokio::test]
async fn the_same_turn_with_the_lamps_name_changes_the_lamp() {
    for action in ["volume_up", "volume_mute", "light_on", "light_colour"] {
        let (answer, asked) = acted(true, "Hey Lamp, turn yourself up a bit.", action).await;
        assert_eq!(
            answer,
            json!({"toolResponse":{"functionResponses":[
                {"id":"c9","name":action,"response":{"changed":true}}
            ]}}),
            "{action}"
        );
        assert_eq!(
            asked,
            Some(json!({"op":"call","name":action,"args":{"colour":"blue"}})),
            "{action}"
        );
    }
}

/// Both workers take `--spoken-to` where the coordinator puts it, right
/// after `--name-in-passing-ignored`: the second voice's worker, which
/// takes the same flags, would otherwise refuse it and leave the lamp
/// without its second voice. That route has no attention rule, and does
/// not apply it.
#[test]
fn both_workers_take_the_spoken_to_flag_in_its_place() {
    let dir = std::path::Path::new("/run/lamp-voice");
    let flags = [
        crate::attention::FLAG,
        crate::attention::BUSY_FLAG,
        crate::attention::NEAR_FLAG,
        crate::attention::PASSING_FLAG,
        crate::attention::SPOKEN_TO_FLAG,
        crate::first_output::FLAG,
        "/run/lamp-voice/first-output",
    ]
    .map(str::to_owned);
    let (_, _, files, visual) = parse_flags(dir, &flags).unwrap();
    assert!(files.attention && files.name_in_passing_ignored && files.spoken_to);
    assert!(visual.is_none());
    let (_, _, files, _) = crate::second_provider::worker::parse_flags(dir, &flags).unwrap();
    assert!(files.spoken_to);
    let (_, _, files, _) = parse_flags(dir, &flags[..3]).unwrap();
    assert!(!files.spoken_to);
}

/// Puts the process-wide mark of the lamp's own turn back when dropped,
/// even when a test fails: the worker marks its own turn for the whole
/// process, and each rig numbers its requests from one.
struct OwnMark(u64);

impl OwnMark {
    fn kept() -> Self {
        Self(crate::attention::OWN_REQUEST.load(std::sync::atomic::Ordering::Acquire))
    }
}

impl Drop for OwnMark {
    fn drop(&mut self) {
        crate::attention::OWN_REQUEST.store(self.0, std::sync::atomic::Ordering::Release);
    }
}

/// How the lamp's own turn before the room's line ended.
#[derive(Clone, Copy, Debug)]
enum OwnTurn {
    /// Its sound went out.
    Spoke,
    /// The service completed it with nothing to say.
    Silent,
    /// Its owner was cancelled before any sound.
    Cancelled,
}

/// The reviewer on #918 at 05:11, through the worker's own loop: the lamp's
/// own written turn, ended as `own`, then the two-person scene's unnamed
/// opening line in a room not known to be busy. Whether that line's answer
/// is played: only after news the lamp spoke is it a reply.
async fn after_the_lamps_own_turn(own: OwnTurn) -> bool {
    let _mark = OwnMark::kept();
    let mut rig = LiveRig::start_with(
        quick_policy(),
        options(true, quiet, crate::tools::Settings::default()),
    );
    let mut dial = rig.ready().await;
    let owner = rig
        .owner
        .admit(real_time(), lamp_turn_owner::AdmittedInput::NewTurn)
        .unwrap();
    let news = owner.turn();
    rig.publish();
    rig.parent
        .data
        .send(ProviderInput::Written {
            request: news,
            session: dial.session.get(),
            privacy_generation: rig.snapshot.microphone_generation(),
            text: "Tell them the task they gave you is done.".to_owned(),
            look: false,
        })
        .unwrap();
    assert!(matches!(
        rig.output().await,
        ProviderOutput::Started { request, .. } if request == news
    ));
    let written = rig.service_message(&mut dial).await;
    assert!(written.get("clientContent").is_some(), "{written}");
    match own {
        OwnTurn::Spoke => {
            dial.service.send(provider_audio(5, 240)).await;
            dial.service.send(idle_complete()).await;
        }
        OwnTurn::Silent => dial.service.send(idle_complete()).await,
        OwnTurn::Cancelled => {
            rig.owner.cancel_turn(real_time(), owner).unwrap();
            rig.publish();
            dial.service.send(idle_complete()).await;
        }
    }
    // A cancelled turn's own output is never delivered; the others end.
    if !matches!(own, OwnTurn::Cancelled) {
        loop {
            match rig.output().await {
                ProviderOutput::TurnComplete { request, .. } if request == news => break,
                _ => {}
            }
        }
    }
    let request = rig.speak(&mut dial, false, true).await;
    dial.service.send(heard(NOT_TO_THE_LAMP[0])).await;
    dial.service.send(provider_audio(7, 240)).await;
    dial.service.send(idle_complete()).await;
    let mut played = false;
    loop {
        match rig.output().await {
            ProviderOutput::Audio { request: of, .. } if of == request => played = true,
            ProviderOutput::TurnComplete { request: of, .. } if of == request => break,
            _ => {}
        }
    }
    rig.stop().await;
    played
}

#[tokio::test]
async fn news_the_lamp_spoke_is_replied_to() {
    assert!(after_the_lamps_own_turn(OwnTurn::Spoke).await);
}

#[tokio::test]
async fn an_own_turn_completed_silent_is_no_news() {
    assert!(!after_the_lamps_own_turn(OwnTurn::Silent).await);
}

#[tokio::test]
async fn an_own_turn_cancelled_before_it_spoke_is_no_news() {
    assert!(!after_the_lamps_own_turn(OwnTurn::Cancelled).await);
}

/// The reviewer on #918 at 06:17, through the worker's own loop: the
/// lamp's own written turn says two pieces of words, which use up the output
/// credit, and then its sound, which waits for credit. Proven taken from the
/// service by a later call for the time, which is answered at once. Then
/// either its owner is cancelled before the credit comes back, and that
/// sound is never handed on; or the credit comes back, and it is. Whether
/// the room's unnamed line after it is played: only after news the lamp
/// actually handed on is it a reply.
async fn after_queued_news(cancelled: bool) -> bool {
    let _mark = OwnMark::kept();
    let mut rig = LiveRig::start_with(
        quick_policy(),
        options(
            true,
            quiet,
            crate::tools::Settings {
                clock: true,
                ..Default::default()
            },
        ),
    );
    let mut dial = rig.ready().await;
    let owner = rig
        .owner
        .admit(real_time(), lamp_turn_owner::AdmittedInput::NewTurn)
        .unwrap();
    let news = owner.turn();
    rig.publish();
    rig.parent
        .data
        .send(ProviderInput::Written {
            request: news,
            session: dial.session.get(),
            privacy_generation: rig.snapshot.microphone_generation(),
            text: "Tell them the task they gave you is done.".to_owned(),
            look: false,
        })
        .unwrap();
    assert!(matches!(
        rig.output().await,
        ProviderOutput::Started { request, .. } if request == news
    ));
    rig.service_message(&mut dial).await;
    // Two pieces of words, read and their credit kept back.
    for text in ["Your task", " is done."] {
        dial.service
            .send(json!({"serverContent":{"outputTranscription":{"text":text,"finished":false}}}))
            .await;
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut taken = 0;
    while taken < 2 {
        rig.publish();
        if let Some(wire) = rig.parent.data.receive::<Wire>().unwrap() {
            assert!(
                matches!(wire, Wire::Output(ProviderOutput::Transcript { .. })),
                "{wire:?}"
            );
            rig.output_flow.received().unwrap();
            taken += 1;
        }
        assert!(Instant::now() < deadline, "the words did not come");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    // The sound waits for credit; the call after it shows it was taken.
    dial.service.send(provider_audio(5, 240)).await;
    dial.service
        .send(function_call("t1", crate::tools::THE_TIME))
        .await;
    let answered = rig.service_message(&mut dial).await;
    assert!(answered.get("toolResponse").is_some(), "{answered}");
    assert!(
        rig.parent.data.receive::<Wire>().unwrap().is_none(),
        "no credit, no output"
    );
    if cancelled {
        rig.owner.cancel_turn(real_time(), owner).unwrap();
        for _ in 0..25 {
            rig.publish();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    rig.output_flow
        .replenish(0, &mut rig.parent.control)
        .unwrap();
    // The call's turn, then the answer after it.
    dial.service.send(idle_complete()).await;
    dial.service.send(idle_complete()).await;
    if !cancelled {
        loop {
            match rig.output().await {
                ProviderOutput::TurnComplete { request, .. } if request == news => break,
                _ => {}
            }
        }
    } else {
        for _ in 0..25 {
            rig.publish();
            assert!(
                !matches!(
                    rig.take(),
                    Some(Wire::Output(ProviderOutput::Audio { request, .. })) if request == news
                ),
                "a cancelled turn's sound was handed on"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    let request = rig.speak(&mut dial, false, true).await;
    dial.service.send(heard(NOT_TO_THE_LAMP[0])).await;
    dial.service.send(provider_audio(7, 240)).await;
    dial.service.send(idle_complete()).await;
    let mut played = false;
    loop {
        match rig.output().await {
            ProviderOutput::Audio { request: of, .. } if of == request => played = true,
            ProviderOutput::TurnComplete { request: of, .. } if of == request => break,
            _ => {}
        }
    }
    rig.stop().await;
    played
}

#[tokio::test]
async fn news_whose_sound_waited_for_credit_and_then_went_is_replied_to() {
    assert!(after_queued_news(false).await);
}

#[tokio::test]
async fn news_whose_sound_a_cancellation_dropped_before_it_went_is_no_news() {
    assert!(!after_queued_news(true).await);
}
