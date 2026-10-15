//! The ring, listened to: `light/state`, where the ring's owner
//! (`lamp-voice`) says what light it is to show. The self-test asks for
//! nothing on the ring.
//!
//! Asking for each colour on `light/command` and putting the person's light
//! back cannot be made safe from here: a request the person makes between
//! the self-test's read and its write would be overwritten, and no read
//! closes that window. The colours come back when the ring's owner takes a
//! test request of its own that never touches the person's light (#812).
//!
//! `light/state` says what the owner took, not what it drew: the ring
//! refuses to draw under the privacy switch or while a turn owns the lamp.
//! So the ring is at best `not_checked` here.
use crate::Check;
use lamp_bus::Bus;
use lamp_msgs::{Light, PartResult, Verdict};
use std::time::{Duration, Instant};

/// The ring says what it shows once a second; a state older than this is
/// from a ring that stopped.
pub const STATE_FRESH: Duration = Duration::from_secs(3);

/// The light the ring says it shows, and how long ago it said it.
fn shown(bus: &Bus) -> Result<Option<(Light, Duration)>, String> {
    match crate::listen::newest::<Light>(bus, Light::TOPICS[1])? {
        Some(seen) => Ok(Some((seen.value, seen.age))),
        None => Ok(None),
    }
}

/// What the ring's owner says, judged.
pub fn ring(heard: &Result<Option<(Light, Duration)>, String>) -> PartResult {
    let part = "ring";
    match heard {
        Err(why) => PartResult::new(
            part,
            Verdict::Fail,
            &format!("{} cannot be read: {why}", Light::TOPICS[1]),
        ),
        Ok(None) => PartResult::new(
            part,
            Verdict::NotChecked,
            &format!(
                "nothing was ever said on {}: is lamp-voice running?",
                Light::TOPICS[1]
            ),
        ),
        Ok(Some((_, age))) if *age > STATE_FRESH => PartResult::new(
            part,
            Verdict::Fail,
            &format!(
                "the newest {} is {:.1} s old: the ring stopped saying what it shows",
                Light::TOPICS[1],
                age.as_secs_f64()
            ),
        ),
        Ok(Some((light, _))) => PartResult::new(
            part,
            Verdict::NotChecked,
            "listened only: the ring's owner runs, and what it draws cannot be read back",
        )
        .with("colour", &light.colour),
    }
}

pub struct Ring<'a> {
    pub bus: &'a Bus,
}

impl Check for Ring<'_> {
    fn part(&self) -> &str {
        "ring"
    }

    fn run(&mut self, _deadline: Instant) -> PartResult {
        ring(&shown(self.bus))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(colour: &str) -> Light {
        Light {
            on: true,
            colour: colour.into(),
            level: 2,
        }
    }

    #[test]
    fn a_ring_that_says_what_it_shows_is_listened_to_never_passed() {
        let heard = ring(&Ok(Some((light("blue"), Duration::from_millis(400)))));
        assert_eq!(heard.verdict, Verdict::NotChecked);
        assert!(
            heard.reason.starts_with("listened only"),
            "{}",
            heard.reason
        );
        assert_eq!(ring(&Ok(None)).verdict, Verdict::NotChecked);
        let stopped = ring(&Ok(Some((light("blue"), Duration::from_secs(9)))));
        assert_eq!(stopped.verdict, Verdict::Fail);
        assert_eq!(ring(&Err("gone".into())).verdict, Verdict::Fail);
    }

    #[test]
    fn the_check_asks_nothing_of_the_ring() {
        let root = std::path::PathBuf::from(format!("/tmp/ls-ring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut owner = bus
            .publisher_declared(Light::TOPICS[1], &Light::declaration(), "ring")
            .unwrap();
        owner.publish_json(1, &light("warm")).unwrap();
        let result = Ring { bus: &bus }.run(Instant::now() + Duration::from_secs(5));
        assert_eq!(result.verdict, Verdict::NotChecked, "{result:?}");
        assert!(
            bus.latest(Light::TOPICS[0]).unwrap().is_none(),
            "nothing was asked on light/command"
        );
        drop(owner);
        std::fs::remove_dir_all(root).unwrap();
    }
}
