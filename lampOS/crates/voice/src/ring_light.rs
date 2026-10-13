//! The lamp's own light on the bus (`docs/bus.md`): the ring's owner takes
//! what is asked for on `light/command` and says what it shows on
//! `light/state`, both `lamp_msgs::Light`.
//!
//! Whoever wants the light changed (the actions program, when someone
//! says "make your light blue") publishes the light wanted and reads the
//! state back; it never writes the ring. The newest wanted light is kept
//! by the bus, so a ring that starts later, or again, takes up the light
//! that was last asked for.
//!
//! Nothing here can stop the ring. A bus that cannot be used leaves the
//! lamp with the light it has, and a request that is not a light there is
//! (a colour that does not exist, a level outside 1 to 5) is passed over.
use crate::ring_face::Lit;
use lamp_bus::{Bus, Publisher, Subscriber};
use lamp_msgs::Light;
use std::{io, path::Path, time::Duration};

/// The name the ring's owner publishes under.
pub const SOURCE: &str = "ring";
/// What is asked for is looked at this often. A person who asks for a
/// light hears the answer more than a second later; the ring has changed
/// long before.
const LOOK_EVERY_US: u64 = 50_000;
/// What is shown is said this often when nothing changes, as the topic
/// promises, and a state means nothing this long after it was said: a
/// ring that has stopped is not a light that can be asked for.
const SAY_EVERY_US: u64 = 1_000_000;
const STATE_LASTS: Duration = Duration::from_secs(3);
/// At most this many requests are taken in one look.
const MOST_AT_ONCE: usize = 8;

/// The light before anyone has asked for one, as the message.
pub fn first() -> Light {
    Light {
        on: true,
        colour: "warm".into(),
        level: 1,
    }
}

/// A message as the face's own light, if it is one there is.
pub fn lit(light: &Light) -> Option<Lit> {
    light.check().ok()?;
    Lit::of(light.on, &light.colour, light.level)
}

pub struct RingLight {
    asked: Subscriber,
    shown: Publisher,
    now: Light,
    looked_at_us: u64,
    said_at_us: Option<u64>,
}

impl RingLight {
    /// The ring's light on the bus under `root`, and the light to begin
    /// with: the one last asked for, if the bus keeps one that there is,
    /// or else the first.
    pub fn on(root: &Path) -> io::Result<(Self, Lit)> {
        let bus = Bus::new(root)?;
        let [wanted, state] = Light::TOPICS;
        let asked = bus.subscribe_typed(wanted, SOURCE, Light::TYPE)?;
        let mut shown = bus.publisher_declared(state, &Light::declaration(), SOURCE)?;
        shown.expires_after(Some(STATE_LASTS));
        let kept = bus
            .latest(wanted)
            .ok()
            .flatten()
            .and_then(|message| message.parse::<Light>().ok())
            .filter(|light| lit(light).is_some());
        let now = kept.unwrap_or_else(first);
        let begins = lit(&now).unwrap_or(Lit::first());
        Ok((
            Self {
                asked,
                shown,
                now,
                looked_at_us: 0,
                said_at_us: None,
            },
            begins,
        ))
    }

    /// Call from the ring's loop with its clock. A light to show from now
    /// on, if another than the one shown was asked for since the last
    /// call. What is shown is said when it changes and once a second.
    pub fn tick(&mut self, now_us: u64) -> Option<Lit> {
        let mut changed = None;
        if now_us.saturating_sub(self.looked_at_us) >= LOOK_EVERY_US {
            self.looked_at_us = now_us;
            for _ in 0..MOST_AT_ONCE {
                let Ok(Some(message)) = self.asked.recv(Duration::ZERO) else {
                    break;
                };
                let Ok(asked) = message.parse::<Light>() else {
                    continue;
                };
                // The newest that is a light there is, is the one taken.
                if let Some(face) = lit(&asked)
                    && asked != self.now
                {
                    self.now = asked;
                    changed = Some(face);
                }
            }
        }
        let due = self
            .said_at_us
            .is_none_or(|at| now_us.saturating_sub(at) >= SAY_EVERY_US);
        if changed.is_some() || due {
            self.said_at_us = Some(now_us);
            let _ = self.shown.publish_json(now_us, &self.now);
        }
        changed
    }

    /// The light the ring is showing, as it says it.
    pub fn now(&self) -> &Light {
        &self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_bus::Kind;

    struct Root(std::path::PathBuf);
    impl Root {
        fn new(name: &str) -> Self {
            let root = std::path::PathBuf::from(format!("/tmp/lv-{name}-{}", std::process::id()));
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

    fn light(on: bool, colour: &str, level: u8) -> Light {
        Light {
            on,
            colour: colour.into(),
            level,
        }
    }

    fn state(bus: &Bus) -> Option<Light> {
        bus.latest(Light::TOPICS[1])
            .unwrap()
            .map(|message| message.parse().unwrap())
    }

    /// Tick until the ring has taken a new light, for a second at most.
    fn taken(ring: &mut RingLight, from_us: &mut u64) -> Option<Lit> {
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            *from_us += LOOK_EVERY_US;
            if let Some(lit) = ring.tick(*from_us) {
                return Some(lit);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        None
    }

    #[test]
    fn the_ring_begins_with_the_first_light_and_says_what_it_shows() {
        let root = Root::new("light-first");
        let (mut ring, begins) = RingLight::on(&root.0).unwrap();
        assert_eq!(begins, Lit::first());
        assert_eq!(ring.now(), &first());
        let bus = Bus::new(&root.0).unwrap();
        // Nothing is said until its loop runs.
        assert_eq!(state(&bus), None);
        assert_eq!(ring.tick(10_000), None);
        assert_eq!(state(&bus), Some(light(true, "warm", 1)));
        // Declared as the type has it, and good for three seconds.
        let said = bus.latest(Light::TOPICS[1]).unwrap().unwrap();
        assert_eq!(said.header.source, "ring");
        assert_eq!(said.header.expires_ms, Some(3_000));
        assert_eq!(
            bus.declaration(Light::TOPICS[1]).unwrap(),
            Some(Light::declaration())
        );
    }

    #[test]
    fn a_light_asked_for_is_taken_shown_and_said_at_once() {
        let root = Root::new("light-asked");
        let (mut ring, _) = RingLight::on(&root.0).unwrap();
        let bus = Bus::new(&root.0).unwrap();
        let mut asks = bus
            .publisher_declared(Light::TOPICS[0], &Light::declaration(), "actions")
            .unwrap();
        let mut at = 0u64;
        ring.tick(at);
        asks.publish_json(1, &light(true, "blue", 3)).unwrap();
        assert_eq!(taken(&mut ring, &mut at), Lit::of(true, "blue", 3));
        assert_eq!(state(&bus), Some(light(true, "blue", 3)));
        // The same again changes nothing.
        asks.publish_json(2, &light(true, "blue", 3)).unwrap();
        for _ in 0..5 {
            at += LOOK_EVERY_US;
            assert_eq!(ring.tick(at), None);
            std::thread::sleep(Duration::from_millis(2));
        }
        // Off keeps the colour it would be.
        asks.publish_json(3, &light(false, "blue", 3)).unwrap();
        let off = taken(&mut ring, &mut at).unwrap();
        assert!(!off.on);
        assert_eq!(state(&bus), Some(light(false, "blue", 3)));
        // Several at once: the newest is the one shown.
        for (n, colour) in ["red", "green", "pink"].into_iter().enumerate() {
            asks.publish_json(4 + n as u64, &light(true, colour, 5))
                .unwrap();
        }
        let mut last = None;
        while let Some(lit) = taken(&mut ring, &mut at) {
            last = Some(lit);
            if ring.now().colour == "pink" {
                break;
            }
        }
        assert_eq!(last, Lit::of(true, "pink", 5));
        assert_eq!(state(&bus), Some(light(true, "pink", 5)));
    }

    #[test]
    fn what_is_not_a_light_there_is_is_passed_over() {
        let root = Root::new("light-odd");
        let (mut ring, _) = RingLight::on(&root.0).unwrap();
        let bus = Bus::new(&root.0).unwrap();
        let mut asks = bus
            .publisher_declared(Light::TOPICS[0], &Light::declaration(), "actions")
            .unwrap();
        let mut at = 0u64;
        ring.tick(at);
        asks.publish_json(1, &light(true, "mauve", 2)).unwrap();
        asks.publish_json(2, &light(true, "blue", 9)).unwrap();
        asks.publish_json(3, &serde_json::json!({"rgb": [0, 0, 255]}))
            .unwrap();
        for _ in 0..10 {
            at += LOOK_EVERY_US;
            assert_eq!(ring.tick(at), None);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(ring.now(), &first());
        // And a light there is, after them, is taken.
        asks.publish_json(4, &light(true, "cyan", 2)).unwrap();
        assert_eq!(taken(&mut ring, &mut at), Lit::of(true, "cyan", 2));
    }

    #[test]
    fn a_ring_that_starts_later_takes_up_the_light_last_asked_for() {
        let root = Root::new("light-kept");
        let bus = Bus::new(&root.0).unwrap();
        let mut asks = bus
            .publisher_declared(Light::TOPICS[0], &Light::declaration(), "actions")
            .unwrap();
        asks.publish_json(1, &light(true, "purple", 4)).unwrap();
        let (mut ring, begins) = RingLight::on(&root.0).unwrap();
        assert_eq!(Some(begins), Lit::of(true, "purple", 4));
        ring.tick(1_000);
        assert_eq!(state(&bus), Some(light(true, "purple", 4)));
        drop(ring);
        // A kept request that is not a light there is: the first light.
        let other = Root::new("light-kept-odd");
        let bus = Bus::new(&other.0).unwrap();
        let mut asks = bus
            .publisher(Light::TOPICS[0], Kind::Latest, "old")
            .unwrap();
        asks.publish_json(1, &serde_json::json!({"rgb": [1, 2, 3]}))
            .unwrap();
        // A topic declared by an older program as no type at all cannot be
        // subscribed to as a light: the ring goes on without the bus.
        match RingLight::on(&other.0) {
            Ok((_, begins)) => assert_eq!(begins, Lit::first()),
            Err(error) => assert!(!error.to_string().is_empty()),
        }
    }

    #[test]
    fn what_it_shows_is_said_again_every_second_and_not_more_often() {
        let root = Root::new("light-said");
        let (mut ring, _) = RingLight::on(&root.0).unwrap();
        let bus = Bus::new(&root.0).unwrap();
        let mut hears = bus
            .subscribe_typed(Light::TOPICS[1], "test", Light::TYPE)
            .unwrap();
        // Three seconds of the ring's loop, a tick every 2 ms.
        for tick in 0..1_500u64 {
            ring.tick(tick * 2_000);
        }
        let mut said = 0;
        while hears.recv(Duration::from_millis(50)).unwrap().is_some() {
            said += 1;
        }
        assert_eq!(said, 3);
    }
}
