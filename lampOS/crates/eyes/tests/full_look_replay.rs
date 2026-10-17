//! A full look's head settle replayed on lamp-52e6's recorded
//! `body/joint_states` (`docs/verification/look-at-name-lamp-52e6-20261014`),
//! as the reviewer replayed it on #1251: one ask every 10 ms from its start to 2.8 s
//! before its end (4,113 asks),
//! the joints read at the ask and then every 50 ms as the eyes read them, a
//! frame every 33 ms. Counts the waits that reach the 2 s cap.
//! `FULL_LOOK_REPLAY=1 cargo test -p lamp-eyes --test full_look_replay -- --nocapture`
//! also prints every ask's wait. lamp-06c1's V1 roll
//! (`v1-moves-lamp-06c1-20261016`) is replayed the same way.
use base64::{Engine, engine::general_purpose::STANDARD};
use lamp_eyes::full_look::{Head, SETTLE_AT_MOST_US, Wanted, head_of};
use lamp_msgs::JointState;

const LAMP_52E6: &str = "look-at-name-lamp-52e6-20261014/body_joint_states.jsonl";
const LAMP_06C1: &str = "v1-moves-lamp-06c1-20261016/roll-body_joint_states.jsonl";

fn recording(name: &str) -> Vec<(u64, u64, JointState)> {
    let path = format!(
        "{}/../../docs/verification/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let header = row.get("header")?;
            let payload = STANDARD.decode(row["payload_b64"].as_str()?).unwrap();
            Some((
                header["at_unix_ms"].as_u64()?,
                header["expires_ms"].as_u64()?,
                serde_json::from_slice(&payload).unwrap(),
            ))
        })
        .collect()
}

/// Every ask's wait in microseconds and when its still was taken, in ms
/// from the recording's start, or `None` for a look lost.
/// Asks begin with the recording and end 2.8 s before it does.
fn replay(name: &str, movement_only: bool) -> Vec<Option<(u64, u64)>> {
    let messages = recording(name);
    let first = messages[0].0;
    let asks = (messages[messages.len() - 1].0 - first - 2_800).div_ceil(10);
    let latest = |now: u64| {
        messages
            .iter()
            .rev()
            .find(|(at, _, _)| *at <= now)
            .filter(|(at, expires, _)| now - at < *expires)
            .map_or(Head::Absent, |(at, _, joints)| {
                head_of(joints, *at, now, movement_only)
            })
    };
    (0..asks)
        .map(|ask| {
            let asked = first + ask * 10;
            let mut wanted = Wanted::new(asked * 1_000);
            let mut head = Head::Absent;
            let (mut read, mut frame) = (asked, asked.div_ceil(33) * 33);
            loop {
                let now = read.min(frame);
                if now == read {
                    head = latest(now);
                    read += 50;
                }
                let frames = frame / 33 - u64::from(now < frame);
                if now == frame {
                    frame += 33;
                }
                wanted.body(head, now * 1_000, frames);
                if wanted.take(frames) {
                    return wanted.waited_us().map(|waited| (waited, now - first));
                }
                if wanted.given_up(now * 1_000) {
                    return None;
                }
            }
        })
        .collect()
}

fn capped(waits: &[Option<(u64, u64)>]) -> usize {
    waits
        .iter()
        .filter(|wait| wait.is_none_or(|(wait, _)| wait >= SETTLE_AT_MOST_US))
        .count()
}

/// Asks, and how many waited the whole 2 s, without and with movement only.
fn caps(name: &str) -> (usize, usize, usize) {
    let (before, after) = (replay(name, false), replay(name, true));
    if std::env::var_os("FULL_LOOK_REPLAY").is_some() {
        for (ask, (off, on)) in before.iter().zip(&after).enumerate() {
            println!("{} {:?} {:?}", ask * 10, off, on);
        }
    }
    assert!(
        before.iter().chain(&after).all(Option::is_some),
        "a look lost"
    );
    let (off, on) = (capped(&before), capped(&after));
    eprintln!(
        "{name}: capped at 2 s of {}: off {off}, on {on}",
        before.len()
    );
    (before.len(), off, on)
}

#[test]
fn an_arm_standing_up_with_the_camera_s_aim_held_does_not_hold_a_still_to_the_cap() {
    // lamp-52e6 stands up from 3.1 to 8.1 s, shoulder and elbow both rising
    // at about 150 counts a second. Judged joint by joint (#1251) 329 of the
    // asks waited 2 s; judged by the camera's aim, 36.
    let (asks, off, on) = caps(LAMP_52E6);
    assert_eq!((asks, off), (4_113, 14));
    assert!(on <= 40, "on {on}");
    // lamp-06c1 playing V1's roll: the aim itself turns at 140 to 260
    // counts a second for most of it, and that is waited for. Joint by
    // joint 451 waited 2 s.
    let (asks, off, on) = caps(LAMP_06C1);
    assert_eq!((asks, off), (3_712, 188));
    assert!(on <= 400, "on {on}");
}
