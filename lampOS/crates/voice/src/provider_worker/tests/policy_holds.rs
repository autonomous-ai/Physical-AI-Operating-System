//! The Policy holds the lamp's actions (#800, `LAMP_POLICY_HOLDS_ACTIONS=on`),
//! through the worker's own loop against the scripted service, an executive
//! that answers every call and a bus of the test's own: an action in a turn
//! the Policy judged `stay_quiet` is answered, not run, and none of the
//! turn's sound comes out; one in a turn it took as the lamp's runs; with no
//! decision it runs after the wait; and without the switch it runs at once.
use super::*;
use std::io::{BufRead, Write};

/// An executive that answers every call done, telling the test each line
/// it was sent; and the Policy's place on the bus.
struct Desk {
    home: crate::process::SessionDirectory,
    bus: std::path::PathBuf,
    decisions: lamp_bus::Publisher,
    asked: std::sync::mpsc::Receiver<serde_json::Value>,
}

impl Desk {
    fn new(tag: &str) -> Self {
        let home = crate::process::SessionDirectory::create().unwrap();
        let listener =
            std::os::unix::net::UnixListener::bind(home.path.join(crate::tools::CALL_SOCKET))
                .unwrap();
        let (told, asked) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut line = String::new();
                if std::io::BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .is_err()
                {
                    continue;
                }
                // Told before it answers, so a test that has the answer
                // finds what was asked already there.
                if told
                    .send(serde_json::from_str::<serde_json::Value>(&line).unwrap())
                    .is_err()
                {
                    return;
                }
                let _ = stream.write_all(b"{\"ok\":true,\"result\":{\"volume\":60}}\n");
            }
        });
        // Short: a socket's whole path must fit in 103 bytes.
        let bus = std::path::PathBuf::from(format!("/tmp/lph-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bus);
        std::fs::create_dir_all(&bus).unwrap();
        let decisions = lamp_bus::Bus::new(&bus)
            .unwrap()
            .publisher_declared(
                lamp_msgs::Decision::TOPIC,
                &lamp_msgs::Decision::declaration(),
                "policy",
            )
            .unwrap();
        Self {
            home,
            bus,
            decisions,
            asked,
        }
    }

    fn options(&self, holds: bool) -> Options {
        Options {
            reporting: Reporting::Typed,
            tools: crate::tools::Settings {
                decline: false,
                exec: Some(self.home.path.clone()),
                memory: None,
                clock: false,
                ..Default::default()
            },
            attention: None,
            name_signal: false,
            sounds_checked: false,
            name_in_passing_ignored: false,
            policy_holds: holds.then(|| self.bus.clone()),
        }
    }

    /// The Policy decides about what was said, for real.
    fn decide(&mut self, choice: &str) {
        let decision = json!({"about": "attention", "choice": choice, "probability": 0.235,
                              "answers": {}, "by": "typesafe/jev-1.13-20260917",
                              "took_ms": 540, "shadow": false});
        self.decisions.publish_json(0, &decision).unwrap();
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.bus);
    }
}

fn volume_up(id: &str) -> serde_json::Value {
    json!({"toolCall":{"functionCalls":[{"id":id,"name":"volume_up","args":{}}]}})
}

/// The turn the lamp heard, ended; the Policy's decision about it, if any,
/// taken on a tick; then the model asks for `volume_up`. What the model is
/// told, and how long after the call.
async fn asked_for_volume(
    desk: &mut Desk,
    holds: bool,
    choice: Option<&str>,
) -> (LiveRig, Dial, u64, serde_json::Value, Duration) {
    let mut rig = LiveRig::start_with(quick_policy(), desk.options(holds));
    let mut dial = rig.ready().await;
    let request = rig.speak(&mut dial, false, true).await;
    if let Some(choice) = choice {
        desk.decide(choice);
    }
    let called = Instant::now();
    dial.service.send(volume_up("v1")).await;
    let told = rig.service_message(&mut dial).await;
    (rig, dial, request, told, called.elapsed())
}

/// The worker's outputs for `long`, its heartbeat kept. In this rig a turn
/// that made a call is not completed, so nothing out is waited for this way.
async fn outputs_for(rig: &mut LiveRig, long: Duration) -> Vec<ProviderOutput> {
    let until = Instant::now() + long;
    let mut out = Vec::new();
    while Instant::now() < until {
        rig.publish();
        while let Some(wire) = rig.take() {
            if let Wire::Output(output) = wire {
                out.push(output);
            }
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    out
}

fn response(told: &serde_json::Value) -> serde_json::Value {
    told["toolResponse"]["functionResponses"][0]["response"].clone()
}

/// lamp-4ace, 2026-10-15 14:49: "Thailand" heard in a busy office, the
/// Policy `stay_quiet`, and the lamp turned itself up for eleven hours.
/// Held: the model is told it was not done and to say nothing, the
/// executive is never asked, and what the model says next is not let out.
#[tokio::test]
async fn an_action_in_a_turn_the_policy_judged_not_for_the_lamp_is_not_done_and_nothing_is_said() {
    let mut desk = Desk::new("held");
    let (mut rig, mut dial, request, told, _) =
        asked_for_volume(&mut desk, true, Some("stay_quiet")).await;
    assert_eq!(
        response(&told),
        json!({"done": false, "next": crate::policy_holds::HELD})
    );
    // What the model says next is not let out. (Let out, as when the
    // Policy took the turn as the lamp's below, it comes at once.)
    dial.service.send(provider_audio(9, 480)).await;
    let out = outputs_for(&mut rig, Duration::from_millis(500)).await;
    assert!(
        !out.iter().any(
            |output| matches!(output, ProviderOutput::Audio { request: of, .. } if *of == request)
        ),
        "{out:?}"
    );
    // The executive was never asked. (Never a blocking wait here: the
    // worker shares this test's runtime.)
    assert!(desk.asked.try_recv().is_err());
    rig.stop().await;
}

/// A turn the Policy took as the lamp's (`answer`, or `look_up`): the
/// action runs, and its answer goes back as it would without the switch.
#[tokio::test]
async fn an_action_in_a_turn_the_policy_took_as_the_lamps_runs() {
    for choice in ["answer", "look_up"] {
        let mut desk = Desk::new(choice);
        let (mut rig, mut dial, request, told, _) =
            asked_for_volume(&mut desk, true, Some(choice)).await;
        assert_eq!(response(&told), json!({"volume": 60}), "{choice}");
        assert_eq!(desk.asked.try_recv().unwrap()["name"], "volume_up");
        // And what the model says next is let out.
        dial.service.send(provider_audio(9, 480)).await;
        let out = outputs_for(&mut rig, Duration::from_millis(500)).await;
        assert!(
            out.iter().any(
                |output| matches!(output, ProviderOutput::Audio { request: of, .. } if *of == request)
            ),
            "{choice}: {out:?}"
        );
        rig.stop().await;
    }
}

/// No decision, or only a shadow one: after the wait the action runs as it
/// did; without the switch it runs at once, whatever was decided.
#[tokio::test]
async fn with_no_decision_in_time_it_runs_after_the_wait_and_without_the_switch_at_once() {
    let mut desk = Desk::new("late");
    let (mut rig, _dial, _, told, took) = asked_for_volume(&mut desk, true, None).await;
    assert_eq!(response(&told), json!({"volume": 60}));
    assert!(
        took >= crate::policy_holds::WAIT - Duration::from_millis(100),
        "{took:?}"
    );
    rig.stop().await;

    let mut desk = Desk::new("off");
    let (mut rig, _dial, _, told, took) =
        asked_for_volume(&mut desk, false, Some("stay_quiet")).await;
    assert_eq!(response(&told), json!({"volume": 60}));
    assert!(took < Duration::from_secs(1), "{took:?}");
    rig.stop().await;
}

/// The worker reads `--policy-holds-actions` where the coordinator puts it,
/// after `--sounds-checked`.
#[test]
fn the_switch_is_read_after_sounds_checked() {
    let dir = std::path::Path::new("/run/test");
    let flags: Vec<String> = [crate::not_speech::CHECKED_FLAG, crate::policy_holds::FLAG]
        .map(String::from)
        .into();
    let (_, _, files, _) = parse_flags(dir, &flags).unwrap();
    assert!(files.sounds_checked && files.policy_holds);
    let (_, _, files, _) = parse_flags(dir, &[]).unwrap();
    assert!(!files.policy_holds);
    // Out of its place it is refused.
    let wrong: Vec<String> = [crate::policy_holds::FLAG, crate::not_speech::CHECKED_FLAG]
        .map(String::from)
        .into();
    assert!(parse_flags(dir, &wrong).is_err());
}

/// The turn heard, ended, and the model asking for `volume_up`, kept for
/// the Policy's decision, which never comes here.
async fn parked_volume(desk: &mut Desk) -> (LiveRig, Dial) {
    let mut rig = LiveRig::start_with(quick_policy(), desk.options(true));
    let mut dial = rig.ready().await;
    rig.speak(&mut dial, false, true).await;
    dial.service.send(volume_up("v1")).await;
    // Kept: nothing is answered at once.
    assert!(
        outputs_for(&mut rig, Duration::from_millis(100))
            .await
            .is_empty()
    );
    (rig, dial)
}

/// The reviewer on #908: a call kept for the Policy's decision whose turn
/// is cancelled meanwhile (its owner gone) is never run, also after the
/// wait in which an uncancelled one would have run.
#[tokio::test]
async fn a_kept_action_whose_turn_is_cancelled_is_never_run() {
    let mut desk = Desk::new("cancelled");
    let (mut rig, mut dial) = parked_volume(&mut desk).await;
    let owner = rig.snapshot.owner().unwrap();
    rig.owner.cancel_turn(real_time(), owner).unwrap();
    rig.publish();
    // The service settles the turn it was asked to stop, as Gemini does.
    dial.service.send(idle_complete()).await;
    // Never run, also after the wait in which an uncancelled one would have
    // run.
    outputs_for(
        &mut rig,
        crate::policy_holds::WAIT + Duration::from_millis(300),
    )
    .await;
    assert!(!rig.worker.is_finished(), "the worker ended");
    assert!(desk.asked.try_recv().is_err(), "volume_up was run");
    rig.stop().await;
}

/// The reviewer on #908: a call the service withdrew while it was kept is
/// never run.
#[tokio::test]
async fn a_kept_action_the_service_withdrew_is_never_run() {
    let mut desk = Desk::new("withdrawn");
    let (mut rig, mut dial) = parked_volume(&mut desk).await;
    dial.service
        .send(json!({"toolCallCancellation":{"ids":["v1"]}}))
        .await;
    outputs_for(
        &mut rig,
        crate::policy_holds::WAIT + Duration::from_millis(300),
    )
    .await;
    assert!(desk.asked.try_recv().is_err(), "volume_up was run");
    rig.stop().await;
}
