//! The lamp's light: asked of the ring's owner, and read back from it.
//!
//! The executive does not write the ring. It publishes the light it wants
//! on the bus (`light/command`, the newest value kept) and reads what the
//! ring's owner says it is showing (`light/state`). A lamp whose ring
//! owner is not publishing has no light to ask for, and the light's
//! functions are not offered. V1's rules: on is warm white, a colour is
//! one of ten, brighter is a quarter more with the colour kept (a dark
//! lamp starts gently), dimmer is half.

use crate::{ON, Outcome};
use std::time::{Duration, Instant};

/// A light that can be asked for and read back.
pub trait Light {
    /// Whether its owner is there to carry a request out.
    fn there(&mut self) -> bool;
    /// What it shows now, as its owner reports it.
    fn shown(&mut self) -> Option<[u8; 3]>;
    /// Ask for a light. False when the request could not even be made.
    fn ask(&mut self, rgb: [u8; 3]) -> bool;
}

/// How long the owner has to show what was asked for.
pub const CONFIRM_WAIT: Duration = Duration::from_millis(600);
const UNCONFIRMED: &str = "I asked for it but could not confirm my light changed.";
const UNREAD: &str = "I could not tell what my light is showing.";

/// Ask, then read back until the owner shows it or the wait is over.
fn set<L: Light>(light: &mut L, rgb: [u8; 3], said: String) -> Outcome {
    if !light.ask(rgb) {
        return Outcome::Failed("I could not ask for my light to change.".into());
    }
    let started = Instant::now();
    loop {
        if light.shown() == Some(rgb) {
            return Outcome::Done(said);
        }
        if started.elapsed() >= CONFIRM_WAIT {
            return Outcome::Failed(UNCONFIRMED.into());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

pub fn on<L: Light>(light: &mut L) -> Outcome {
    match light.shown() {
        None => Outcome::Failed(UNREAD.into()),
        Some(now) if now != [0, 0, 0] => Outcome::Done("My light is already on.".into()),
        Some(_) => set(light, ON, "My light is on, warm white.".into()),
    }
}

pub fn off<L: Light>(light: &mut L) -> Outcome {
    match light.shown() {
        None => Outcome::Failed(UNREAD.into()),
        Some([0, 0, 0]) => Outcome::Done("My light is already off.".into()),
        Some(_) => set(light, [0, 0, 0], "My light is off.".into()),
    }
}

pub fn colour<L: Light>(light: &mut L, name: &str, rgb: [u8; 3]) -> Outcome {
    match light.shown() {
        None => Outcome::Failed(UNREAD.into()),
        Some(now) if now == rgb => Outcome::Done(format!("My light is already {name}.")),
        Some(_) => set(light, rgb, format!("My light is {name} now.")),
    }
}

/// A quarter more at its brightest channel, the others in proportion.
pub fn brighter<L: Light>(light: &mut L) -> Outcome {
    let Some(now) = light.shown() else {
        return Outcome::Failed(UNREAD.into());
    };
    let peak = u32::from(*now.iter().max().unwrap_or(&0));
    if peak == 255 {
        return Outcome::Done("My light is already at its brightest.".into());
    }
    // A dark lamp starts gently.
    let mut next = [32, 27, 20];
    if peak > 0 {
        let next_peak = 255.min(peak + (peak / 4).max(1));
        for (to, from) in next.iter_mut().zip(now) {
            *to = (u32::from(from) * next_peak / peak) as u8;
        }
    }
    set(light, next, "My light is a step brighter.".into())
}

/// Half on every channel. Rounding can reach dark from a dim light.
pub fn dimmer<L: Light>(light: &mut L) -> Outcome {
    let Some(now) = light.shown() else {
        return Outcome::Failed(UNREAD.into());
    };
    if now == [0, 0, 0] {
        return Outcome::Done("My light is already off.".into());
    }
    let next = now.map(|channel| channel / 2);
    let said = if next == [0, 0, 0] {
        "My light was so dim that dimming it again switched it off."
    } else {
        "My light is half as bright."
    };
    set(light, next, said.into())
}

/// The light through the bus.
pub struct BusLight {
    bus: lamp_bus::Bus,
    wanted: lamp_bus::Publisher,
    started: Instant,
}

/// Where the executive asks, and where the ring's owner answers.
pub const WANTED: &str = "light/command";
pub const STATE: &str = "light/state";
/// A state older than this, by the wall clock, is nobody's: its owner has
/// stopped saying it.
const STATE_LASTS_MS: u64 = 3_000;

#[derive(serde::Deserialize, serde::Serialize)]
struct Rgb {
    rgb: [u8; 3],
}

impl BusLight {
    pub fn new(bus: lamp_bus::Bus) -> std::io::Result<Self> {
        let wanted = bus.publisher(WANTED, lamp_bus::Kind::Latest, "actions")?;
        Ok(Self {
            bus,
            wanted,
            started: Instant::now(),
        })
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

impl Light for BusLight {
    fn there(&mut self) -> bool {
        self.shown().is_some()
    }
    fn shown(&mut self) -> Option<[u8; 3]> {
        let state = self.bus.latest(STATE).ok()??;
        let lasts = state.header.expires_ms.unwrap_or(STATE_LASTS_MS);
        if unix_ms() > state.header.at_unix_ms.saturating_add(lasts) {
            return None;
        }
        state.parse::<Rgb>().ok().map(|state| state.rgb)
    }
    fn ask(&mut self, rgb: [u8; 3]) -> bool {
        let at_us = self.started.elapsed().as_micros() as u64;
        self.wanted.publish_json(at_us, &Rgb { rgb }).is_ok()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A ring owner that shows what it is asked, or does not.
    pub(crate) struct Fake {
        pub shown: Option<[u8; 3]>,
        pub obeys: bool,
        pub asked: Vec<[u8; 3]>,
    }
    impl Fake {
        pub(crate) fn showing(rgb: [u8; 3]) -> Self {
            Self {
                shown: Some(rgb),
                obeys: true,
                asked: Vec::new(),
            }
        }
    }
    impl Light for Fake {
        fn there(&mut self) -> bool {
            self.shown.is_some()
        }
        fn shown(&mut self) -> Option<[u8; 3]> {
            self.shown
        }
        fn ask(&mut self, rgb: [u8; 3]) -> bool {
            self.asked.push(rgb);
            if self.obeys {
                self.shown = Some(rgb);
            }
            true
        }
    }
    fn said(outcome: Outcome) -> String {
        match outcome {
            Outcome::Done(said) => said,
            Outcome::Failed(said) => panic!("expected it done, was told: {said}"),
        }
    }

    #[test]
    fn on_is_warm_white_off_is_dark_and_neither_is_asked_twice() {
        let mut light = Fake::showing([0, 0, 0]);
        assert_eq!(said(on(&mut light)), "My light is on, warm white.");
        assert_eq!(light.shown, Some(ON));
        assert_eq!(said(on(&mut light)), "My light is already on.");
        assert_eq!(said(off(&mut light)), "My light is off.");
        assert_eq!(said(off(&mut light)), "My light is already off.");
        assert_eq!(light.asked, [ON, [0, 0, 0]]);
    }

    #[test]
    fn a_colour_is_set_by_name_and_said_by_name() {
        let mut light = Fake::showing(ON);
        let blue = crate::colour("blue").unwrap();
        assert_eq!(
            said(colour(&mut light, "blue", blue)),
            "My light is blue now."
        );
        assert_eq!(light.shown, Some([0, 100, 255]));
        assert_eq!(
            said(colour(&mut light, "blue", blue)),
            "My light is already blue."
        );
        assert_eq!(light.asked.len(), 1);
    }

    /// V1's arithmetic, on V1's own examples: a quarter more at the peak
    /// with the hue kept, a gentle start from dark, a stop at full; half on
    /// every channel, down to dark.
    #[test]
    fn brighter_and_dimmer_keep_the_colour_and_stop_at_the_ends() {
        let mut light = Fake::showing([100, 50, 200]);
        assert_eq!(said(brighter(&mut light)), "My light is a step brighter.");
        assert_eq!(light.shown, Some([125, 62, 250]));
        assert_eq!(said(brighter(&mut light)), "My light is a step brighter.");
        assert_eq!(light.shown, Some([127, 63, 255]));
        assert_eq!(
            said(brighter(&mut light)),
            "My light is already at its brightest."
        );
        let mut dark = Fake::showing([0, 0, 0]);
        assert_eq!(said(brighter(&mut dark)), "My light is a step brighter.");
        assert_eq!(dark.shown, Some([32, 27, 20]));

        let mut light = Fake::showing([255, 180, 100]);
        assert_eq!(said(dimmer(&mut light)), "My light is half as bright.");
        assert_eq!(light.shown, Some([127, 90, 50]));
        let mut faint = Fake::showing([1, 1, 0]);
        assert_eq!(
            said(dimmer(&mut faint)),
            "My light was so dim that dimming it again switched it off."
        );
        assert_eq!(said(dimmer(&mut faint)), "My light is already off.");
    }

    /// A ring owner that is not saying what it shows, and one that takes a
    /// request and does not show it: each is said as what it is, the second
    /// after the wait and no longer.
    #[test]
    fn a_light_that_cannot_be_read_or_does_not_change_is_said_so() {
        let failed = |outcome: Outcome| match outcome {
            Outcome::Failed(said) => said,
            Outcome::Done(said) => panic!("expected a failure, was told: {said}"),
        };
        let mut gone = Fake {
            shown: None,
            obeys: true,
            asked: Vec::new(),
        };
        assert_eq!(failed(on(&mut gone)), UNREAD);
        assert_eq!(failed(dimmer(&mut gone)), UNREAD);
        assert!(gone.asked.is_empty());
        let mut deaf = Fake {
            shown: Some([0, 0, 0]),
            obeys: false,
            asked: Vec::new(),
        };
        let started = Instant::now();
        assert_eq!(failed(on(&mut deaf)), UNCONFIRMED);
        let waited = started.elapsed();
        assert!(waited >= CONFIRM_WAIT && waited < CONFIRM_WAIT + Duration::from_millis(300));
        assert_eq!(deaf.asked, [ON]);
    }

    /// Through a real bus in a directory of its own: the request is kept as
    /// the topic's newest value, the state is read back from the owner's
    /// topic, and a state its owner has stopped saying is nobody's.
    #[test]
    fn through_the_bus_a_request_is_kept_and_a_stale_state_is_no_light() {
        let root = std::env::temp_dir().join(format!(
            "lamp-actions-light-{}-{:x}",
            std::process::id(),
            unix_ms()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let bus = lamp_bus::Bus::new(&root).unwrap();
        let mut light = BusLight::new(lamp_bus::Bus::new(&root).unwrap()).unwrap();
        // Nobody says what the ring shows: there is no light to ask for.
        assert!(!light.there());
        let mut ring = bus
            .publisher(STATE, lamp_bus::Kind::Latest, "ring")
            .unwrap();
        ring.publish_json(1, &Rgb { rgb: [0, 0, 0] }).unwrap();
        assert!(light.there());
        assert_eq!(light.shown(), Some([0, 0, 0]));
        assert!(light.ask(ON));
        let wanted: Rgb = bus.latest(WANTED).unwrap().unwrap().parse().unwrap();
        assert_eq!(wanted.rgb, ON);
        // The owner said so with a life of 20 ms, and then stopped.
        ring.expires_after(Some(Duration::from_millis(20)));
        ring.publish_json(2, &Rgb { rgb: ON }).unwrap();
        assert_eq!(light.shown(), Some(ON));
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(light.shown(), None);
        assert!(!light.there());
        std::fs::remove_dir_all(root).unwrap();
    }
}
