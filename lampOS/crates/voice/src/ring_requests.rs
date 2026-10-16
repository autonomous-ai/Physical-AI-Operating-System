//! Lights asked for that lapse (`light/request`, #940, `docs/skills.md`):
//! a skill's glow, or anything else that must not leave the person's own
//! light changed. Each asker (`claim.source`) has one request at a time,
//! its newest; the ring shows the highest priority that holds, the newest
//! of equal priority, and the person's lasting light once none does.
//!
//! An asker gives its light back by asking again with a time already
//! passed, or by not asking again before its time. A claim holds only on
//! the ring's own clock, and never for more than 30 s from now
//! ([`lamp_msgs::Claim::holds_at`]), so a skill renews what it keeps.
//!
//! The person comes first. When they ask for their own light
//! (`light/command`), every request below a person's that holds gives way,
//! and stays given way while its asker renews it; its next claim, after
//! that one has lapsed or been given back, is heard again.
use lamp_msgs::{Light, LightRequest, Priority};

/// At most this many askers are kept at once; another is passed over
/// until one's request lapses.
pub const MOST_SOURCES: usize = 16;

struct Held {
    request: LightRequest,
    /// The order it was taken in, so the newest of equal priority wins.
    order: u64,
}

pub struct Requests {
    clock_id: String,
    held: Vec<Held>,
    /// Askers the person's own light overruled, and when their claim
    /// would lapse: a renewal before then is the same claim, still
    /// overruled.
    overruled: Vec<(String, u64)>,
    taken: u64,
}

impl Requests {
    /// Requests on the monotonic clock `clock_id` (the host's boot).
    pub fn new(clock_id: impl Into<String>) -> Self {
        Self {
            clock_id: clock_id.into(),
            held: Vec::new(),
            overruled: Vec::new(),
            taken: 0,
        }
    }

    /// Take a request read at `now_us` on the ring's clock. With
    /// `before_person`, the person asked for their own light after it was
    /// sent, so it is overruled as it would have been had it come first.
    /// Whether it is now held: a request that is not a light there is, from
    /// another clock, overruled, or past [`MOST_SOURCES`] is not, nor is
    /// one that gives the light back.
    pub fn take(&mut self, request: LightRequest, now_us: u64, before_person: bool) -> bool {
        if request.check().is_err() || request.claim.clock_id != self.clock_id {
            return false;
        }
        let source = request.claim.source.clone();
        let holds = request.claim.holds_at(&self.clock_id, now_us);
        self.held.retain(|held| held.request.claim.source != source);
        if let Some(at) = self
            .overruled
            .iter()
            .position(|(overruled, _)| *overruled == source)
        {
            let until = self.overruled[at].1;
            if holds && now_us < until {
                // The same claim, renewed: still the person's turn.
                self.overruled[at].1 = until.max(request.claim.until_us);
                return false;
            }
            self.overruled.swap_remove(at);
        }
        if !holds {
            return false;
        }
        if before_person && request.claim.priority < Priority::Person {
            self.overrule(source, request.claim.until_us);
            return false;
        }
        self.held
            .retain(|held| held.request.claim.holds_at(&self.clock_id, now_us));
        if self.held.len() >= MOST_SOURCES {
            return false;
        }
        self.taken += 1;
        self.held.push(Held {
            request,
            order: self.taken,
        });
        true
    }

    /// The person asked for their own light at `now_us`: every request
    /// below a person's that holds gives way.
    pub fn person_asked(&mut self, now_us: u64) {
        let clock_id = &self.clock_id;
        self.overruled.retain(|(_, until)| now_us < *until);
        let mut given_way = Vec::new();
        self.held.retain(|held| {
            let claim = &held.request.claim;
            if !claim.holds_at(clock_id, now_us) {
                return false;
            }
            if claim.priority < Priority::Person {
                given_way.push((claim.source.clone(), claim.until_us));
                return false;
            }
            true
        });
        for (source, until) in given_way {
            self.overrule(source, until);
        }
    }

    fn overrule(&mut self, source: String, until_us: u64) {
        if let Some(kept) = self.overruled.iter_mut().find(|(at, _)| *at == source) {
            kept.1 = kept.1.max(until_us);
        } else if self.overruled.len() < MOST_SOURCES {
            self.overruled.push((source, until_us));
        }
        // Past that, too many to remember: its renewals are heard as new
        // claims.
    }

    /// The request shown at `now_us`, if one holds.
    pub fn shown(&self, now_us: u64) -> Option<&LightRequest> {
        self.held
            .iter()
            .filter(|held| held.request.claim.holds_at(&self.clock_id, now_us))
            .max_by_key(|held| (held.request.claim.priority, held.order))
            .map(|held| &held.request)
    }

    /// The light shown at `now_us`, if a request holds.
    pub fn light(&self, now_us: u64) -> Option<&Light> {
        self.shown(now_us).map(|request| &request.light)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_msgs::Claim;

    const BOOT: &str = "boot-one";
    const S: u64 = 1_000_000;

    fn request(source: &str, priority: Priority, until_us: u64, colour: &str) -> LightRequest {
        LightRequest {
            claim: Claim {
                source: source.into(),
                priority,
                clock_id: BOOT.into(),
                until_us,
            },
            light: Light::new(true, colour, 3).unwrap(),
        }
    }

    fn colour(requests: &Requests, now_us: u64) -> Option<&str> {
        requests.light(now_us).map(|light| light.colour.as_str())
    }

    #[test]
    fn the_highest_priority_that_holds_is_shown_the_newest_of_equal_and_none_once_lapsed() {
        let mut requests = Requests::new(BOOT);
        assert_eq!(colour(&requests, 0), None);
        assert!(requests.take(
            request("skill:a", Priority::Skill, 10 * S, "blue"),
            0,
            false
        ));
        assert!(requests.take(request("idle", Priority::Idle, 20 * S, "red"), 0, false));
        assert_eq!(colour(&requests, S), Some("blue"));
        assert!(requests.take(
            request("skill:b", Priority::Skill, 5 * S, "green"),
            S,
            false
        ));
        assert_eq!(colour(&requests, 2 * S), Some("green"), "the newest skill");
        assert_eq!(colour(&requests, 5 * S), Some("blue"), "b has lapsed");
        assert_eq!(colour(&requests, 10 * S), Some("red"), "a has lapsed");
        assert_eq!(colour(&requests, 20 * S), None, "the person's light again");
    }

    #[test]
    fn an_asker_has_one_request_and_gives_its_light_back_by_a_time_passed() {
        let mut requests = Requests::new(BOOT);
        requests.take(
            request("skill:a", Priority::Skill, 10 * S, "blue"),
            0,
            false,
        );
        requests.take(
            request("skill:a", Priority::Skill, 10 * S, "pink"),
            S,
            false,
        );
        assert_eq!(colour(&requests, 2 * S), Some("pink"));
        assert!(!requests.take(
            request("skill:a", Priority::Skill, 2 * S, "pink"),
            2 * S,
            false
        ));
        assert_eq!(colour(&requests, 2 * S), None);
    }

    #[test]
    fn another_clock_too_far_ahead_or_too_many_askers_are_passed_over() {
        let mut requests = Requests::new(BOOT);
        let mut other = request("skill:a", Priority::Skill, 10 * S, "blue");
        other.claim.clock_id = "boot-two".into();
        assert!(!requests.take(other, 0, false));
        assert!(!requests.take(
            request("skill:a", Priority::Skill, 31 * S, "blue"),
            0,
            false
        ));
        for n in 0..MOST_SOURCES {
            assert!(requests.take(
                request(&format!("skill:{n}"), Priority::Skill, 10 * S, "blue"),
                0,
                false
            ));
        }
        assert!(!requests.take(
            request("one-more", Priority::Safety, 10 * S, "red"),
            0,
            false
        ));
        // Once some have lapsed, there is room again.
        requests.take(request("skill:0", Priority::Skill, S, "blue"), 0, false);
        assert!(requests.take(
            request("one-more", Priority::Safety, 10 * S, "red"),
            2 * S,
            false
        ));
        assert_eq!(colour(&requests, 2 * S), Some("red"));
    }

    #[test]
    fn the_persons_light_overrules_lower_requests_while_they_are_renewed() {
        let mut requests = Requests::new(BOOT);
        requests.take(
            request("skill:a", Priority::Skill, 10 * S, "blue"),
            0,
            false,
        );
        requests.take(request("hand", Priority::Person, 3 * S, "white"), 0, false);
        requests.person_asked(S);
        // A person's own request is not overruled by their light.
        assert_eq!(colour(&requests, S), Some("white"));
        assert_eq!(colour(&requests, 3 * S), None, "the skill gave way");
        // Renewed before its claim lapsed: the same claim, still overruled.
        assert!(!requests.take(
            request("skill:a", Priority::Skill, 14 * S, "blue"),
            5 * S,
            false
        ));
        assert!(!requests.take(
            request("skill:a", Priority::Skill, 18 * S, "blue"),
            13 * S,
            false
        ));
        assert_eq!(colour(&requests, 13 * S), None);
        // A new claim after that one lapsed is heard again.
        assert!(requests.take(
            request("skill:a", Priority::Skill, 25 * S, "blue"),
            19 * S,
            false
        ));
        assert_eq!(colour(&requests, 19 * S), Some("blue"));
        // So is one after the asker gave its light back.
        requests.person_asked(20 * S);
        requests.take(
            request("skill:a", Priority::Skill, 0, "blue"),
            21 * S,
            false,
        );
        assert!(requests.take(
            request("skill:a", Priority::Skill, 30 * S, "blue"),
            21 * S,
            false
        ));
        assert_eq!(colour(&requests, 22 * S), Some("blue"));
    }

    #[test]
    fn a_request_sent_before_the_persons_light_is_overruled_as_if_it_had_come_first() {
        let mut requests = Requests::new(BOOT);
        assert!(!requests.take(request("skill:a", Priority::Skill, 10 * S, "blue"), 0, true));
        assert_eq!(colour(&requests, 0), None);
        assert!(!requests.take(
            request("skill:a", Priority::Skill, 12 * S, "blue"),
            S,
            false
        ));
        assert!(requests.take(request("hand", Priority::Person, 3 * S, "white"), 0, true));
        assert_eq!(colour(&requests, S), Some("white"));
    }
}
