//! Session duration is independent of turn, provider and hardware authority.
use std::{
    io,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Lifetime {
    Finite(u64),
    Serve,
}

impl Lifetime {
    pub(crate) fn validate(self) -> io::Result<()> {
        match self {
            Self::Finite(1..=600) | Self::Serve => Ok(()),
            _ => Err(io::Error::other("duration must be 1..600 seconds")),
        }
    }

    pub(crate) fn seconds(self) -> Option<u64> {
        match self {
            Self::Finite(seconds) => Some(seconds),
            Self::Serve => None,
        }
    }

    pub(crate) fn continuous(self) -> bool {
        self == Self::Serve
    }
}

pub(crate) struct SessionClock {
    lifetime: Lifetime,
    startup_deadline: Instant,
    ready: bool,
    deadline: Option<Instant>,
}

impl SessionClock {
    pub(crate) fn new(
        lifetime: Lifetime,
        started: Instant,
        startup_budget: Duration,
    ) -> io::Result<Self> {
        lifetime.validate()?;
        Ok(Self {
            lifetime,
            startup_deadline: started + startup_budget,
            ready: false,
            deadline: None,
        })
    }

    pub(crate) fn ready(&self) -> bool {
        self.ready
    }

    pub(crate) fn activate(&mut self, at: Instant) -> io::Result<()> {
        if self.ready || at >= self.startup_deadline {
            return Err(io::Error::other(
                "duplicate or late initial input readiness",
            ));
        }
        self.deadline = self
            .lifetime
            .seconds()
            .map(|seconds| at + Duration::from_secs(seconds));
        self.ready = true;
        Ok(())
    }

    pub(crate) fn startup_expired(&self, at: Instant) -> bool {
        !self.ready && at >= self.startup_deadline
    }

    pub(crate) fn expired(&self, at: Instant) -> bool {
        self.deadline.is_some_and(|deadline| at >= deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_passes_six_hundred_seconds_without_expiring_or_rearming_readiness() {
        let started = Instant::now();
        let mut clock =
            SessionClock::new(Lifetime::Serve, started, Duration::from_secs(25)).unwrap();
        clock.activate(started + Duration::from_secs(1)).unwrap();
        for seconds in [25, 599, 600, 601, 3_600, 86_400, 604_800] {
            let at = started + Duration::from_secs(seconds);
            assert!(!clock.startup_expired(at));
            assert!(!clock.expired(at));
            assert!(clock.ready());
            assert!(clock.activate(at).is_err());
        }
    }

    #[test]
    fn finite_qualification_keeps_its_original_deadline_from_readiness() {
        let started = Instant::now();
        let mut clock =
            SessionClock::new(Lifetime::Finite(600), started, Duration::from_secs(25)).unwrap();
        clock.activate(started + Duration::from_secs(2)).unwrap();
        assert!(!clock.expired(started + Duration::from_secs(601)));
        assert!(clock.expired(started + Duration::from_secs(602)));
        assert!(Lifetime::Finite(601).validate().is_err());
    }

    #[test]
    fn serve_does_not_invent_initial_readiness_or_remove_startup_failure() {
        let started = Instant::now();
        let mut clock =
            SessionClock::new(Lifetime::Serve, started, Duration::from_secs(25)).unwrap();
        assert!(!clock.ready());
        assert!(clock.startup_expired(started + Duration::from_secs(25)));
        assert!(clock.activate(started + Duration::from_secs(25)).is_err());
    }
}
