//! Whether the lamp is paused (`lamp/pause`, `docs/keeper.md`, "Named
//! states and the pause"). A paused body goes to rest, folding if it
//! stands, takes no mood, gesture, look, call or request, and holds there
//! until resumed. Read from the topic's kept value at most once a second:
//! a pause is asked by a person or an update, and a second is soon enough.

use lamp_bus::Bus;
use lamp_msgs::Pause;
use std::time::{Duration, Instant};

const LOOK_EVERY: Duration = Duration::from_secs(1);

/// The body's end of `lamp/pause`.
pub struct PauseIn {
    bus: Bus,
    looked: Option<Instant>,
    paused: bool,
}

impl PauseIn {
    pub fn on(bus: Bus) -> Self {
        Self {
            bus,
            looked: None,
            paused: false,
        }
    }

    /// Whether it is paused now. Never waits longer than a file read.
    pub fn paused(&mut self) -> bool {
        let now = Instant::now();
        if self.looked.is_none_or(|at| now - at >= LOOK_EVERY) {
            self.looked = Some(now);
            self.paused = said_paused(&self.bus);
        }
        self.paused
    }
}

/// What the kept value says; nothing kept, or nothing readable, is not
/// paused.
fn said_paused(bus: &Bus) -> bool {
    bus.latest(Pause::TOPIC)
        .ok()
        .flatten()
        .and_then(|message| message.parse::<Pause>().ok())
        .is_some_and(|pause| pause.paused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_paused_only_while_the_kept_value_says_so() {
        let root = std::env::temp_dir().join(format!("lamp-body-pause-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut pause = PauseIn::on(Bus::new(&root).unwrap());
        assert!(!pause.paused(), "nothing said");
        let mut out = bus
            .publisher_declared(Pause::TOPIC, &Pause::declaration(), "keeper")
            .unwrap();
        out.publish_json(1, &Pause { paused: true }).unwrap();
        pause.looked = None;
        assert!(pause.paused());
        out.publish_json(2, &Pause { paused: false }).unwrap();
        pause.looked = None;
        assert!(!pause.paused());
        let _ = std::fs::remove_dir_all(&root);
    }
}
