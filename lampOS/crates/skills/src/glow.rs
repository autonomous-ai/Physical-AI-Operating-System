//! A skill's light asked of the ring on `light/request` (#940,
//! `docs/skills.md`), with `LAMP_LIGHT_REQUESTS=on`. The skill never touches
//! the person's light: it asks from `skill:<name>` at skill priority, asks
//! again while its run keeps the light, and gives it back by asking with a
//! time already passed. The ring shows it over the person's light until
//! then, or until the person asks for theirs, and then shows theirs again
//! by itself.
use crate::Call;
use lamp_bus::{Bus, Publisher};
use lamp_msgs::{Claim, Light, LightRequest, Priority};
use std::{
    io,
    sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc},
    thread::JoinHandle,
    time::Duration,
};

/// The switch, off unless `on`, as the ring's.
pub const REQUESTS_ENVIRONMENT: &str = "LAMP_LIGHT_REQUESTS";
/// A skill's light is asked again this often while its run keeps it...
pub const GLOW_EVERY: Duration = Duration::from_secs(10);
/// ...and each ask holds this long, so a late one still comes before the
/// last has lapsed: a gap would let the glow come back over a light the
/// person asked for meanwhile (#993).
pub const GLOW_HOLDS_US: u64 = 15_000_000;

/// The light a colour with no level asks for when the light's own is not
/// known: the middle, so a page lit by a skill can be read (the seventh on
/// #1132). A skill that needs a level names it.
pub const LEVEL_UNKNOWN: u8 = 3;

/// How a skill asks the ring for its light: the bus, to see that the ring
/// takes `light/request`, the publisher on it, the boot its claims are on,
/// and how often a light kept is asked again.
#[derive(Clone)]
pub struct Glows {
    pub bus: Bus,
    pub out: Arc<Mutex<Publisher>>,
    pub clock_id: String,
    pub every: Duration,
}

/// A lock that a panic elsewhere left poisoned is still used: what it holds
/// is a light or a publisher, whole either way (the seventh on #1132).
fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One run's light, asked of the ring while the run lasts.
pub struct Glow {
    glows: Glows,
    source: String,
    /// What the run's light steps have made of the light so far: the light
    /// shown before the run, then each step's.
    light: Option<Light>,
    /// Once asked: what is asked again, and by whom.
    renewing: Option<Renewing>,
}

/// The renewals of a skill's light: the light asked again, their stop, and
/// their thread.
struct Renewing {
    held: Arc<Mutex<Light>>,
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

impl Glow {
    /// A run of `skill`, with the light `shown` before it, if known.
    pub fn new(glows: &Glows, skill: &str, shown: Option<Light>) -> Self {
        // A claim names its source in at most 64 characters, and a skill's
        // name may have 64 of its own.
        let source: String = format!("skill:{skill}").chars().take(64).collect();
        Self {
            glows: glows.clone(),
            source,
            light: shown,
            renewing: None,
        }
    }

    /// Asks the ring for what `call`, a light or a dim, makes of the light,
    /// and says how it went.
    pub fn ask(&mut self, call: &Call) -> String {
        let what = match call {
            Call::Light { colour, .. } => format!("light {colour}"),
            _ => "dim".into(),
        };
        let Some(light) = asked(call, self.light.as_ref()) else {
            return format!("{what}: left alone, since no light is on to dim");
        };
        // The skill's steps build on each other as written, whether or not
        // the last one could be asked.
        self.light = Some(light.clone());
        // Looked at each time, so a ring that starts during the run is asked
        // from its next step on (the seventh on #1132).
        let taken = self
            .glows
            .bus
            .subscribers(LightRequest::TOPIC)
            .is_ok_and(|subscribers| !subscribers.is_empty());
        if !taken {
            return format!(
                "{what}: left alone, since nothing takes light/request (the ring needs {REQUESTS_ENVIRONMENT}=on)"
            );
        }
        let until_us = lamp_ipc::monotonic_us() + GLOW_HOLDS_US;
        if let Err(error) = ask_ring(&self.glows, &self.source, &light, until_us) {
            return format!("{what}: light/request not written ({error})");
        }
        let level = light.level;
        match &self.renewing {
            Some(renewing) => *locked(&renewing.held) = light,
            None => {
                let held = Arc::new(Mutex::new(light));
                let (stop, stopped) = mpsc::channel::<()>();
                let (glows, source, again) =
                    (self.glows.clone(), self.source.clone(), Arc::clone(&held));
                let thread = std::thread::spawn(move || {
                    while let Err(mpsc::RecvTimeoutError::Timeout) =
                        stopped.recv_timeout(glows.every)
                    {
                        let light = locked(&again).clone();
                        let until_us = lamp_ipc::monotonic_us() + GLOW_HOLDS_US;
                        if let Err(error) = ask_ring(&glows, &source, &light, until_us) {
                            eprintln!("lamp-skills: {source}'s light not asked again: {error}");
                        }
                    }
                });
                self.renewing = Some(Renewing { held, stop, thread });
            }
        }
        format!("{what}: asked of the ring at level {level}")
    }

    /// Stops asking and gives the light back, if it was asked for.
    pub fn give_back(self) -> Option<String> {
        let Self {
            glows,
            source,
            renewing,
            ..
        } = self;
        let Renewing { held, stop, thread } = renewing?;
        drop(stop);
        let _ = thread.join();
        let light = locked(&held).clone();
        // A time already passed: the ring lets it go at once, and shows the
        // person's light again.
        let until_us = lamp_ipc::monotonic_us().saturating_sub(1);
        Some(match ask_ring(&glows, &source, &light, until_us) {
            Ok(()) => "light given back: asked again with a time already passed".into(),
            Err(error) => format!(
                "light not given back at once: light/request not written ({error}); the ring lets it go within {} s",
                GLOW_HOLDS_US / 1_000_000
            ),
        })
    }
}

/// The light a step asks for, from what the light is so far (`from`): a
/// colour at its level, or else at the level the light has
/// ([`LEVEL_UNKNOWN`] when that is not known); a dim steps down from a
/// light that is on, never below 1. None for a dim with no light on.
pub fn asked(call: &Call, from: Option<&Light>) -> Option<Light> {
    match call {
        Call::Light { colour, level, .. } => Some(Light {
            on: true,
            colour: colour.clone(),
            level: level.unwrap_or_else(|| from.map_or(LEVEL_UNKNOWN, |from| from.level)),
        }),
        Call::Dim { steps, .. } => from.filter(|from| from.on).map(|from| Light {
            on: true,
            colour: from.colour.clone(),
            level: from.level.saturating_sub(*steps).max(1),
        }),
        _ => None,
    }
}

/// One `light/request` from `source` at skill priority, holding until
/// `until_us` on the host's monotonic clock.
fn ask_ring(glows: &Glows, source: &str, light: &Light, until_us: u64) -> io::Result<()> {
    let request = LightRequest {
        claim: Claim {
            source: source.to_owned(),
            priority: Priority::Skill,
            clock_id: glows.clock_id.clone(),
            until_us,
        },
        light: light.clone(),
    };
    locked(&glows.out)
        .publish_json(lamp_ipc::monotonic_us(), &request)
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_bus::{Bus, Subscriber};

    fn light(on: bool, colour: &str, level: u8) -> Light {
        Light {
            on,
            colour: colour.into(),
            level,
        }
    }

    fn colour(colour: &str, level: Option<u8>) -> Call {
        Call::Light {
            colour: colour.into(),
            level,
            lasting: None,
        }
    }

    fn dim(steps: u8) -> Call {
        Call::Dim {
            steps,
            until_ends: false,
        }
    }

    #[test]
    fn a_step_asks_for_its_colour_at_its_level_or_the_lights_and_a_dim_steps_down() {
        let blue = light(true, "blue", 3);
        assert_eq!(
            asked(&colour("warm", Some(5)), Some(&blue)),
            Some(light(true, "warm", 5))
        );
        assert_eq!(
            asked(&colour("warm", None), Some(&blue)),
            Some(light(true, "warm", 3))
        );
        // Not known: the middle, so a page can be read.
        assert_eq!(
            asked(&colour("warm", None), None),
            Some(light(true, "warm", LEVEL_UNKNOWN))
        );
        assert_eq!(asked(&dim(1), Some(&blue)), Some(light(true, "blue", 2)));
        assert_eq!(asked(&dim(4), Some(&blue)), Some(light(true, "blue", 1)));
        // Nothing on to dim, or not known: left alone.
        assert_eq!(asked(&dim(1), Some(&light(false, "blue", 3))), None);
        assert_eq!(asked(&dim(1), None), None);
    }

    /// A bus, the skills' asking on it, and nothing yet taking requests.
    fn a_bus(every: Duration) -> (tempfile::TempDir, Glows) {
        let dir = tempfile::Builder::new()
            .prefix("gl")
            .tempdir_in("/tmp")
            .unwrap();
        let bus = Bus::new(dir.path()).unwrap();
        let out = bus
            .publisher_declared(LightRequest::TOPIC, &LightRequest::declaration(), "skills")
            .unwrap();
        let glows = Glows {
            bus,
            out: Arc::new(Mutex::new(out)),
            clock_id: "boot-test".into(),
            every,
        };
        (dir, glows)
    }

    /// The same with a ring taking requests.
    fn on_a_bus(every: Duration) -> (tempfile::TempDir, Glows, Subscriber) {
        let (dir, glows) = a_bus(every);
        let ring = glows.bus.subscribe(LightRequest::TOPIC, "ring").unwrap();
        (dir, glows, ring)
    }

    fn taken(ring: &mut Subscriber) -> Vec<LightRequest> {
        let mut requests = Vec::new();
        while let Ok(Some(message)) = ring.recv(Duration::ZERO) {
            requests.push(message.parse().unwrap());
        }
        requests
    }

    #[test]
    fn a_light_is_asked_from_the_skill_asked_again_while_kept_and_given_back() {
        let (_dir, glows, mut ring) = on_a_bus(Duration::from_millis(50));
        let mut glow = Glow::new(&glows, "desk-tutor", Some(light(true, "blue", 3)));
        let said = glow.ask(&colour("white", Some(5)));
        assert_eq!(said, "light white: asked of the ring at level 5");
        let first = taken(&mut ring);
        assert_eq!(first.len(), 1);
        let now = lamp_ipc::monotonic_us();
        assert_eq!(first[0].light, light(true, "white", 5));
        assert_eq!(first[0].claim.source, "skill:desk-tutor");
        assert_eq!(first[0].claim.priority, Priority::Skill);
        assert_eq!(first[0].claim.clock_id, "boot-test");
        assert!(first[0].claim.holds_at("boot-test", now));
        // A dim in the same run steps down from the light asked.
        assert_eq!(glow.ask(&dim(2)), "dim: asked of the ring at level 3");
        std::thread::sleep(Duration::from_millis(180));
        let again = taken(&mut ring);
        assert!(again.len() >= 3, "asked again every 50 ms: {again:?}");
        assert!(
            again[1..]
                .iter()
                .all(|request| request.light == light(true, "white", 3)
                    && request
                        .claim
                        .holds_at("boot-test", lamp_ipc::monotonic_us())),
            "{again:?}"
        );
        assert_eq!(
            glow.give_back().as_deref(),
            Some("light given back: asked again with a time already passed")
        );
        let back = taken(&mut ring);
        assert_eq!(back.len(), 1, "no renewal after the give-back: {back:?}");
        assert!(
            !back[0]
                .claim
                .holds_at("boot-test", lamp_ipc::monotonic_us())
        );
        std::thread::sleep(Duration::from_millis(120));
        assert!(taken(&mut ring).is_empty(), "the renewals have stopped");
    }

    #[test]
    fn nothing_is_asked_without_a_taker_or_a_light_to_dim_and_nothing_given_back() {
        let (_dir, glows) = a_bus(Duration::from_millis(50));
        let mut glow = Glow::new(&glows, "hello", Some(light(true, "blue", 3)));
        assert!(
            glow.ask(&colour("warm", None))
                .contains("left alone, since nothing takes light/request"),
        );
        assert_eq!(glow.give_back(), None);
        let mut ring = glows.bus.subscribe(LightRequest::TOPIC, "ring").unwrap();
        let mut glow = Glow::new(&glows, "hello", Some(light(false, "blue", 3)));
        assert_eq!(
            glow.ask(&dim(1)),
            "dim: left alone, since no light is on to dim"
        );
        assert_eq!(glow.give_back(), None);
        assert!(taken(&mut ring).is_empty());
    }

    /// The seventh on #1132: a ring that starts during a run is asked from
    /// the run's next step on.
    #[test]
    fn a_ring_that_starts_during_a_run_is_asked_from_the_next_step() {
        let (_dir, glows) = a_bus(Duration::from_secs(10));
        let mut glow = Glow::new(&glows, "desk-tutor", Some(light(true, "blue", 3)));
        assert!(glow.ask(&colour("white", Some(5))).contains("left alone"));
        let mut ring = glows.bus.subscribe(LightRequest::TOPIC, "ring").unwrap();
        assert_eq!(
            glow.ask(&dim(1)),
            "dim: asked of the ring at level 4",
            "the first step's light is the one the second steps down from"
        );
        assert_eq!(taken(&mut ring).len(), 1);
        assert!(glow.give_back().is_some());
    }

    #[test]
    fn a_long_name_is_cut_to_a_source_the_ring_takes() {
        let (_dir, glows, mut ring) = on_a_bus(Duration::from_secs(10));
        let name = "a".repeat(64);
        let mut glow = Glow::new(&glows, &name, None);
        glow.ask(&colour("warm", Some(2)));
        let request = taken(&mut ring).remove(0);
        assert_eq!(request.claim.source.len(), 64);
        assert!(request.check().is_ok());
        glow.give_back();
    }
}
