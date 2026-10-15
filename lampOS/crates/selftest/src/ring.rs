//! The ring: each colour asked for on `light/command`, as the actions
//! program asks, and seen on `light/state`, where the ring's owner
//! (`lamp-voice`) says what it shows. The light asked for before the test
//! is asked for again after it, so the person keeps their own setting.
//!
//! Only at the factory and when asked. At the first boot the ring is only
//! listened to: a new lamp running through ten colours in its new home is
//! a machine testing itself, not the creature its person is meeting.
use crate::Check;
use lamp_bus::Bus;
use lamp_msgs::{Light, PartResult, Verdict};
use std::time::{Duration, Instant};

/// The name the self-test asks under.
pub const SOURCE: &str = "selftest";
/// The ring looks at what is asked twenty times a second; a colour not
/// shown within this is not coming.
pub const SHOWN_WITHIN: Duration = Duration::from_millis(1_000);
/// How long each colour is held once shown, so a person at the factory
/// sees it.
pub const HOLD: Duration = Duration::from_millis(400);
/// The ring says what it shows once a second; a state older than this is
/// from a ring that stopped.
pub const STATE_FRESH: Duration = Duration::from_secs(3);
/// The level the colours are shown at: bright enough to tell apart.
pub const LEVEL: u8 = 3;

/// The light the ring says it shows, and how long ago it said it.
fn shown(bus: &Bus) -> Result<Option<(Light, Duration)>, String> {
    match crate::listen::newest::<Light>(bus, Light::TOPICS[1])? {
        Some(seen) => Ok(Some((seen.value, seen.age))),
        None => Ok(None),
    }
}

/// Waits until the ring says it shows `light`, or `within` has passed.
fn wait_shown(bus: &Bus, light: &Light, within: Duration) -> bool {
    let until = Instant::now() + within;
    loop {
        if let Ok(Some((now, age))) = shown(bus)
            && now == *light
            && age < within
        {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub struct Ring<'a> {
    pub bus: &'a Bus,
    /// Whether to ask for the colours, or only listen.
    pub active: bool,
}

impl Check for Ring<'_> {
    fn part(&self) -> &str {
        "ring"
    }

    fn run(&mut self, deadline: Instant) -> PartResult {
        let part = "ring";
        let before = match shown(self.bus) {
            Err(why) => {
                return PartResult::new(
                    part,
                    Verdict::Fail,
                    &format!("{} cannot be read: {why}", Light::TOPICS[1]),
                );
            }
            Ok(None) => {
                return PartResult::new(
                    part,
                    Verdict::NotChecked,
                    &format!(
                        "nothing was ever said on {}: is lamp-voice running?",
                        Light::TOPICS[1]
                    ),
                );
            }
            Ok(Some((_, age))) if age > STATE_FRESH => {
                return PartResult::new(
                    part,
                    Verdict::Fail,
                    &format!(
                        "the newest {} is {:.1} s old: the ring stopped saying what it shows",
                        Light::TOPICS[1],
                        age.as_secs_f64()
                    ),
                );
            }
            Ok(Some((light, _))) => light,
        };
        if !self.active {
            return PartResult::new(part, Verdict::Pass, "listened only")
                .with("colour", &before.colour);
        }
        // The person's own light: the newest asked for, which the bus
        // keeps, or what is shown if nobody ever asked.
        let own = crate::listen::newest::<Light>(self.bus, Light::TOPICS[0])
            .ok()
            .flatten()
            .map(|seen| seen.value)
            .filter(|light| light.check().is_ok())
            .unwrap_or(before);
        let mut asks =
            match self
                .bus
                .publisher_declared(Light::TOPICS[0], &Light::declaration(), SOURCE)
            {
                Ok(asks) => asks,
                Err(error) => {
                    return PartResult::new(
                        part,
                        Verdict::NotChecked,
                        &format!("{} cannot be asked: {error}", Light::TOPICS[0]),
                    );
                }
            };
        let mut clock = 0;
        let mut ask = |light: &Light| {
            clock += 1;
            asks.publish_json(clock, light).is_ok()
        };
        let mut missed = Vec::new();
        let mut reached = 0;
        for colour in Light::COLOURS {
            if Instant::now() + SHOWN_WITHIN > deadline {
                break;
            }
            let light = Light {
                on: true,
                colour: colour.into(),
                level: LEVEL,
            };
            if !ask(&light) || !wait_shown(self.bus, &light, SHOWN_WITHIN) {
                missed.push(colour);
            } else {
                std::thread::sleep(HOLD);
            }
            reached += 1;
        }
        let restored = ask(&own) && wait_shown(self.bus, &own, SHOWN_WITHIN);
        let result = if !missed.is_empty() {
            PartResult::new(
                part,
                Verdict::Fail,
                &format!("not shown within 1 s: {}", missed.join(", ")),
            )
        } else if reached < Light::COLOURS.len() {
            PartResult::new(
                part,
                Verdict::NotChecked,
                &format!(
                    "ran out of time after {reached} of {} colours",
                    Light::COLOURS.len()
                ),
            )
        } else if !restored {
            PartResult::new(
                part,
                Verdict::Fail,
                "every colour shown, but the light before was not shown again",
            )
        } else {
            PartResult::new(part, Verdict::Pass, "")
        };
        result
            .with("colours", reached)
            .with("restored", &own.colour)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct Root(std::path::PathBuf);
    impl Root {
        fn new(name: &str) -> Self {
            let root = std::path::PathBuf::from(format!("/tmp/ls-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn light(colour: &str, level: u8) -> Light {
        Light {
            on: true,
            colour: colour.into(),
            level,
        }
    }

    /// A ring that shows what is asked, except `refuses`, and says so as
    /// lamp-voice's does.
    fn ring(
        root: &std::path::Path,
        refuses: Option<&'static str>,
        stop: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        let root = root.to_owned();
        let bus = Bus::new(&root).unwrap();
        let mut asked = bus
            .subscribe_typed(Light::TOPICS[0], "ring", Light::TYPE)
            .unwrap();
        let mut says = bus
            .publisher_declared(Light::TOPICS[1], &Light::declaration(), "ring")
            .unwrap();
        says.publish_json(0, &light("warm", 1)).unwrap();
        std::thread::spawn(move || {
            let mut now = light("warm", 1);
            let mut clock = 1;
            while !stop.load(Ordering::Relaxed) {
                if let Ok(Some(message)) = asked.recv(Duration::from_millis(20))
                    && let Ok(wanted) = message.parse::<Light>()
                    && Some(wanted.colour.as_str()) != refuses
                {
                    now = wanted;
                }
                clock += 1;
                says.publish_json(clock, &now).unwrap();
            }
        })
    }

    #[test]
    fn every_colour_is_shown_and_the_persons_own_light_comes_back() {
        let root = Root::new("ring-ok");
        let bus = Bus::new(&root.0).unwrap();
        let mut person = bus
            .publisher_declared(Light::TOPICS[0], &Light::declaration(), "actions")
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let owner = ring(&root.0, None, stop.clone());
        person.publish_json(1, &light("blue", 2)).unwrap();
        assert!(wait_shown(&bus, &light("blue", 2), SHOWN_WITHIN));
        let result = Ring {
            bus: &bus,
            active: true,
        }
        .run(Instant::now() + Duration::from_secs(30));
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert!(
            result
                .values
                .iter()
                .any(|v| v.key == "restored" && v.value == "blue")
        );
        assert_eq!(shown(&bus).unwrap().unwrap().0, light("blue", 2));
        stop.store(true, Ordering::Relaxed);
        owner.join().unwrap();
    }

    #[test]
    fn a_colour_never_shown_fails_and_the_light_is_still_put_back() {
        let root = Root::new("ring-red");
        let bus = Bus::new(&root.0).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let owner = ring(&root.0, Some("red"), stop.clone());
        let result = Ring {
            bus: &bus,
            active: true,
        }
        .run(Instant::now() + Duration::from_secs(30));
        assert_eq!(result.verdict, Verdict::Fail, "{result:?}");
        assert!(result.reason.contains("red"), "{}", result.reason);
        assert_eq!(shown(&bus).unwrap().unwrap().0, light("warm", 1));
        stop.store(true, Ordering::Relaxed);
        owner.join().unwrap();
    }

    #[test]
    fn at_first_boot_the_ring_is_only_listened_to() {
        let root = Root::new("ring-listen");
        let bus = Bus::new(&root.0).unwrap();
        let quiet = Ring {
            bus: &bus,
            active: false,
        }
        .run(Instant::now() + Duration::from_secs(5));
        assert_eq!(quiet.verdict, Verdict::NotChecked);
        let stop = Arc::new(AtomicBool::new(false));
        let owner = ring(&root.0, None, stop.clone());
        let heard = Ring {
            bus: &bus,
            active: false,
        }
        .run(Instant::now() + Duration::from_secs(5));
        assert_eq!(heard.verdict, Verdict::Pass, "{heard:?}");
        assert!(
            bus.latest(Light::TOPICS[0]).unwrap().is_none(),
            "nothing was asked"
        );
        stop.store(true, Ordering::Relaxed);
        owner.join().unwrap();
    }
}
