//! The checks that only listen: each part's newest reading on the bus, and
//! how old it is. Nothing here asks a part to do anything.
//!
//! Each check is a judge, a plain function from what was heard to a
//! [`PartResult`], and [`Listen`], which reads the bus for it.
use crate::Check;
use lamp_bus::Bus;
use lamp_msgs::{AirState, BoardState, JointState, PartResult, RoomSound, TouchState, Verdict};
use serde::de::DeserializeOwned;
use std::{
    marker::PhantomData,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// A reading and how long ago it was published.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen<T> {
    pub age: Duration,
    pub value: T,
}

/// What the bus gave for a topic: its newest reading, nothing ever, or why
/// it could not be read.
pub type Heard<T> = Result<Option<Seen<T>>, String>;

/// The joints `lamp-body` reports, base to head.
pub const JOINTS: [&str; 5] = ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"];

/// Air is read every 5 s; three periods missed is a sensor that stopped.
pub const AIR_FRESH: Duration = Duration::from_secs(15);
/// The board is read every 5 s by `lamp-health`.
pub const BOARD_FRESH: Duration = Duration::from_secs(15);
/// The room level, the joints and the pads come ten times a second.
pub const FAST_FRESH: Duration = Duration::from_secs(1);

/// A room microphone at or under this level gives nothing: even a quiet
/// room is well above it.
pub const SILENT_DB: f32 = -90.0;
/// A pad held this long while nobody is asked to touch it is stuck.
pub const STUCK: Duration = Duration::from_secs(10);
/// The hottest a board may be during the test. Under the SoC's own
/// throttling point; to be confirmed against lamp-4ace's trip points.
pub const HOT_C: f32 = 85.0;
/// Less of the disk free than this fails, as `lamp-health` calls it an error.
pub const DISK_FREE_LEAST: f32 = 0.03;

/// What a missing, stale or unreadable reading means, or the reading.
fn fresh<'a, T>(
    part: &str,
    heard: &'a Heard<T>,
    topic: &str,
    owner: &str,
    within: Duration,
) -> Result<&'a T, PartResult> {
    match heard {
        Err(why) => Err(PartResult::new(
            part,
            Verdict::Fail,
            &format!("{topic} cannot be read: {why}"),
        )),
        Ok(None) => Err(PartResult::new(
            part,
            Verdict::NotChecked,
            &format!("nothing was ever said on {topic}: is {owner} running?"),
        )),
        Ok(Some(seen)) if seen.age > within => Err(PartResult::new(
            part,
            Verdict::Fail,
            &format!(
                "the newest {topic} is {:.1} s old: {owner} stopped giving it",
                seen.age.as_secs_f64()
            ),
        )
        .with("age_s", format!("{:.1}", seen.age.as_secs_f64()))),
        Ok(Some(seen)) => Ok(&seen.value),
    }
}

/// The air sensor gives readings, in range.
pub fn air(heard: &Heard<AirState>) -> PartResult {
    let air = match fresh("air", heard, AirState::TOPIC, "lamp-air", AIR_FRESH) {
        Ok(air) => air,
        Err(result) => return result,
    };
    if let Err(why) = air.check() {
        return PartResult::new("air", Verdict::Fail, &format!("out of range: {why}"));
    }
    let values = [
        air.pm2_5.is_some(),
        air.humidity.is_some(),
        air.temperature.is_some(),
        air.co2_ppm.is_some(),
    ];
    if !values.contains(&true) {
        return PartResult::new("air", Verdict::Fail, "the sensor gives no values");
    }
    let mut result = PartResult::new("air", Verdict::Pass, "");
    if let Some(co2) = air.co2_ppm {
        result = result.with("co2_ppm", co2);
    }
    if let Some(t) = air.temperature {
        result = result.with("temperature_c", format!("{t:.1}"));
    }
    result
}

/// The room microphone hears something, however quiet.
pub fn room_mic(heard: &Heard<RoomSound>) -> PartResult {
    let part = "room_mic";
    let sound = match fresh(part, heard, RoomSound::TOPIC, "lamp-ears", FAST_FRESH) {
        Ok(sound) => sound,
        Err(result) => return result,
    };
    if let Err(why) = sound.check() {
        return PartResult::new(part, Verdict::Fail, &format!("out of range: {why}"));
    }
    if sound.level_db <= SILENT_DB {
        return PartResult::new(
            part,
            Verdict::Fail,
            &format!(
                "silent at {:.0} dBFS: the microphone gives nothing",
                sound.level_db
            ),
        );
    }
    PartResult::new(part, Verdict::Pass, "").with("level_db", format!("{:.1}", sound.level_db))
}

/// One joint answers: `lamp-body` reports where it is.
pub fn joint(name: &str, heard: &Heard<JointState>) -> PartResult {
    let joints = match fresh(name, heard, JointState::TOPIC, "lamp-body", FAST_FRESH) {
        Ok(joints) => joints,
        Err(result) => return result,
    };
    let Some(at) = joints.name.iter().position(|n| n == name) else {
        return PartResult::new(name, Verdict::Fail, "lamp-body does not report this joint");
    };
    match joints.position.get(at) {
        Some(p) if p.is_finite() => {
            PartResult::new(name, Verdict::Pass, "").with("position_rad", format!("{p:.3}"))
        }
        Some(p) => PartResult::new(name, Verdict::Fail, &format!("its position reads {p}")),
        None => PartResult::new(name, Verdict::Fail, "lamp-body gives no position for it"),
    }
}

/// Every touch source is ready, without a fault, and not held down.
pub fn touch(heard: &Heard<TouchState>) -> PartResult {
    let part = "touch";
    let state = match fresh(part, heard, TouchState::TOPIC, "lamp-controls", FAST_FRESH) {
        Ok(state) => state,
        Err(result) => return result,
    };
    if !state.enabled {
        return PartResult::new(part, Verdict::NotChecked, "touch reading is switched off");
    }
    if state.sources.is_empty() {
        return PartResult::new(part, Verdict::Fail, "no touch source reports");
    }
    let mut touched = Vec::new();
    for source in &state.sources {
        if let Some(fault) = &source.fault {
            return PartResult::new(part, Verdict::Fail, &format!("{}: {fault:?}", source.name));
        }
        if !source.ready {
            return PartResult::new(
                part,
                Verdict::Fail,
                &format!("{} is not ready", source.name),
            );
        }
        if source.active == Some(true) {
            let held = Duration::from_micros(source.held_us.unwrap_or(0));
            if held >= STUCK {
                return PartResult::new(
                    part,
                    Verdict::Fail,
                    &format!(
                        "{} stuck: held {:.0} s ({})",
                        source.name,
                        held.as_secs_f64(),
                        source.active_names.join(", ")
                    ),
                );
            }
            touched.push(source.name.as_str());
        }
    }
    if !touched.is_empty() {
        return PartResult::new(
            part,
            Verdict::NotChecked,
            &format!("touched during the check: {}", touched.join(", ")),
        );
    }
    PartResult::new(part, Verdict::Pass, "idle").with("sources", state.sources.len())
}

/// The board is cool enough and has room on its disk.
pub fn board(heard: &Heard<BoardState>) -> PartResult {
    let part = "board";
    let state = match fresh(part, heard, BoardState::TOPIC, "lamp-health", BOARD_FRESH) {
        Ok(state) => state,
        Err(result) => return result,
    };
    let hottest = state.temperature_c.iter().max_by(|a, b| a.1.total_cmp(b.1));
    if let Some((sensor, &c)) = hottest
        && c >= HOT_C
    {
        return PartResult::new(part, Verdict::Fail, &format!("{sensor} is at {c:.0} °C"));
    }
    if state.disk_free < DISK_FREE_LEAST {
        return PartResult::new(
            part,
            Verdict::Fail,
            &format!(
                "the disk is all but full: {:.1}% free",
                state.disk_free * 100.0
            ),
        );
    }
    let mut result = PartResult::new(part, Verdict::Pass, "")
        .with("disk_free", format!("{:.3}", state.disk_free));
    if let Some((sensor, c)) = hottest {
        result = result.with("hottest", format!("{sensor} {c:.0} °C"));
    }
    result
}

/// The newest reading on `topic`, and its age by the publisher's wall clock.
pub fn newest<T: DeserializeOwned>(bus: &Bus, topic: &str) -> Heard<T> {
    let Some(message) = bus.latest(topic).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let age = Duration::from_millis(now_ms.saturating_sub(message.header.at_unix_ms));
    let value = message.parse::<T>().map_err(|e| e.to_string())?;
    Ok(Some(Seen { age, value }))
}

/// How a reading on one topic becomes a part's result.
type Judge<'a, T> = Box<dyn Fn(&Heard<T>) -> PartResult + 'a>;

/// A check that reads one topic and judges it.
pub struct Listen<'a, T> {
    part: String,
    bus: &'a Bus,
    topic: &'static str,
    judge: Judge<'a, T>,
    kind: PhantomData<T>,
}

impl<'a, T: DeserializeOwned> Listen<'a, T> {
    pub fn new(
        part: &str,
        bus: &'a Bus,
        topic: &'static str,
        judge: impl Fn(&Heard<T>) -> PartResult + 'a,
    ) -> Self {
        Self {
            part: part.to_owned(),
            bus,
            topic,
            judge: Box::new(judge),
            kind: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> Check for Listen<'_, T> {
    fn part(&self) -> &str {
        &self.part
    }

    fn run(&mut self, _deadline: Instant) -> PartResult {
        (self.judge)(&newest(self.bus, self.topic))
    }
}

/// Every check that only listens, in the order they run.
pub fn checks(bus: &Bus) -> Vec<Box<dyn Check + '_>> {
    let mut checks: Vec<Box<dyn Check + '_>> = vec![
        Box::new(Listen::new("room_mic", bus, RoomSound::TOPIC, room_mic)),
        Box::new(Listen::new("air", bus, AirState::TOPIC, air)),
        Box::new(Listen::new("touch", bus, TouchState::TOPIC, touch)),
        Box::new(Listen::new("board", bus, BoardState::TOPIC, board)),
    ];
    for name in JOINTS {
        checks.push(Box::new(Listen::new(
            name,
            bus,
            JointState::TOPIC,
            move |heard: &Heard<JointState>| joint(name, heard),
        )));
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_msgs::AirFeel;
    use std::collections::BTreeMap;

    fn seen<T>(age_ms: u64, value: T) -> Heard<T> {
        Ok(Some(Seen {
            age: Duration::from_millis(age_ms),
            value,
        }))
    }

    fn good_air() -> AirState {
        AirState {
            pm1_0: Some(3.0),
            pm2_5: Some(4.0),
            pm4_0: Some(4.0),
            pm10: Some(4.0),
            humidity: Some(55.0),
            temperature: Some(31.0),
            co2_ppm: Some(650),
            feel: AirFeel::Fresh,
        }
    }

    #[test]
    fn air_in_range_and_fresh_passes() {
        let result = air(&seen(2_000, good_air()));
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert!(
            result
                .values
                .iter()
                .any(|v| v.key == "co2_ppm" && v.value == "650")
        );
    }

    #[test]
    fn a_reading_never_given_is_not_checked_and_an_old_one_fails() {
        let never = air(&Ok(None));
        assert_eq!(never.verdict, Verdict::NotChecked);
        assert!(never.reason.contains("lamp-air"), "{}", never.reason);
        let old = air(&seen(40_000, good_air()));
        assert_eq!(old.verdict, Verdict::Fail);
        assert!(old.reason.contains("40.0 s old"), "{}", old.reason);
        let unreadable = air(&Err("not JSON".into()));
        assert_eq!(unreadable.verdict, Verdict::Fail);
    }

    #[test]
    fn air_with_no_values_or_out_of_range_fails() {
        let empty = AirState {
            pm1_0: None,
            pm2_5: None,
            pm4_0: None,
            pm10: None,
            humidity: None,
            temperature: None,
            co2_ppm: None,
            feel: AirFeel::Unknown,
        };
        assert!(air(&seen(1, empty)).reason.contains("no values"));
        let wet = AirState {
            humidity: Some(140.0),
            ..good_air()
        };
        assert_eq!(air(&seen(1, wet)).verdict, Verdict::Fail);
    }

    fn sound(level_db: f32) -> RoomSound {
        RoomSound {
            level_db,
            floor_db: None,
            own_voice: false,
        }
    }

    #[test]
    fn a_quiet_room_passes_and_digital_silence_fails() {
        assert_eq!(room_mic(&seen(50, sound(-62.0))).verdict, Verdict::Pass);
        let dead = room_mic(&seen(50, sound(-120.0)));
        assert_eq!(dead.verdict, Verdict::Fail);
        assert!(dead.reason.contains("silent"), "{}", dead.reason);
        assert_eq!(room_mic(&seen(3_000, sound(-62.0))).verdict, Verdict::Fail);
    }

    fn joints() -> JointState {
        JointState {
            name: JOINTS.iter().map(|n| n.to_string()).collect(),
            position: vec![0.0, 0.4, -1.2, 0.01, f64::NAN],
            velocity: vec![],
            effort: vec![],
            commanded: vec![],
        }
    }

    #[test]
    fn each_joint_answers_for_itself() {
        let heard = seen(80, joints());
        assert_eq!(joint("shoulder", &heard).verdict, Verdict::Pass);
        assert_eq!(joint("head_pitch", &heard).verdict, Verdict::Fail);
        let gone = joint("wrist", &heard);
        assert!(gone.reason.contains("does not report"), "{}", gone.reason);
    }

    fn board_state(hottest: f32, disk_free: f32) -> BoardState {
        BoardState {
            temperature_c: BTreeMap::from([("cpu".into(), hottest), ("gpu".into(), 40.0)]),
            cpu_load: 0.3,
            memory_used: 0.4,
            disk_free,
            uptime_s: 600,
            wifi_dbm: None,
            clock_synced: None,
        }
    }

    #[test]
    fn a_hot_board_or_a_full_disk_fails() {
        let fine = board(&seen(1_000, board_state(55.0, 0.5)));
        assert_eq!(fine.verdict, Verdict::Pass, "{fine:?}");
        assert!(fine.values.iter().any(|v| v.value == "cpu 55 °C"));
        let hot = board(&seen(1_000, board_state(91.0, 0.5)));
        assert!(hot.reason.contains("cpu is at 91"), "{}", hot.reason);
        let full = board(&seen(1_000, board_state(55.0, 0.01)));
        assert!(full.reason.contains("all but full"), "{}", full.reason);
    }

    #[test]
    fn every_part_that_listens_is_checked_once() {
        let root = std::env::temp_dir().join(format!("lamp-selftest-bus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut checks = checks(&bus);
        let names: Vec<_> = checks.iter().map(|c| c.part().to_owned()).collect();
        assert_eq!(
            names,
            [
                "room_mic",
                "air",
                "touch",
                "board",
                "base_yaw",
                "shoulder",
                "elbow",
                "head_roll",
                "head_pitch"
            ]
        );
        let test = crate::run(lamp_msgs::Occasion::OnDemand, &mut checks, crate::BUDGET);
        assert!(
            test.parts.iter().all(|p| p.verdict == Verdict::NotChecked),
            "{test:?}"
        );
        assert_eq!(test.check(), Ok(()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_reading_on_the_bus_is_heard_with_its_age() {
        let root = std::env::temp_dir().join(format!("lamp-selftest-air-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut sensor = bus
            .publisher_declared(AirState::TOPIC, &AirState::declaration(), "air")
            .unwrap();
        sensor.publish_json(1, &good_air()).unwrap();
        let heard = newest::<AirState>(&bus, AirState::TOPIC).unwrap().unwrap();
        assert_eq!(heard.value, good_air());
        assert!(heard.age < Duration::from_secs(5), "{:?}", heard.age);
        let mut check = Listen::new("air", &bus, AirState::TOPIC, air);
        assert_eq!(check.run(Instant::now()).verdict, Verdict::Pass);
        drop(sensor);
        std::fs::remove_dir_all(root).unwrap();
    }
}
