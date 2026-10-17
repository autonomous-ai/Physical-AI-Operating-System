//! Whether the internet answers while the voice's link is down
//! (`lamp-health serve --reach-probe on`, #830).
//!
//! `lamp/online` follows the voice's link to its speech service. On a
//! product lamp that link goes through the company's relay, so a relay
//! outage (17 Oct, 02:29) would read as no internet, and the lamp would say
//! so with the internet fine. While the link is down, and only then, this
//! asks the internet itself: a TCP connection to any of [`TARGETS`], at
//! most every [`PROBE_EVERY`], each attempt bounded
//! by [`PROBE_WITHIN`]. Nothing is sent on a connection made; it is closed
//! at once. The probe runs on its own thread, so the loop never waits.
use std::{
    net::{SocketAddr, TcpStream},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

/// What the probe connects to: two public name servers, on 443 only. Never
/// port 53: a router that forces its own resolver redirects 53 to itself,
/// and that resolver answers with the WAN down, so a dead network would
/// read as online (the reviewer on #1247). Any answering is enough (the
/// first builder on #830, 17 Oct). IPv4 only.
pub const TARGETS: [&str; 2] = ["1.1.1.1:443", "8.8.8.8:443"];
/// How often the internet is asked while the link is down.
pub const PROBE_EVERY: Duration = Duration::from_secs(30);
/// How long one probe may take, for all targets at once.
pub const PROBE_WITHIN: Duration = Duration::from_secs(2);

/// The default targets, parsed.
pub fn targets() -> Vec<SocketAddr> {
    TARGETS
        .iter()
        .map(|target| target.parse().expect("a fixed socket address"))
        .collect()
}

/// The probe's state between report times.
pub struct Reach {
    targets: Vec<SocketAddr>,
    every: Duration,
    within: Duration,
    /// When the last probe ended, and whether the internet answered.
    last: Option<(Instant, bool)>,
    /// A probe under way, and when it began.
    running: Option<(Instant, Receiver<bool>)>,
}

impl Reach {
    pub fn new(targets: Vec<SocketAddr>) -> Self {
        Self::with_times(targets, PROBE_EVERY, PROBE_WITHIN)
    }

    pub fn with_times(targets: Vec<SocketAddr>, every: Duration, within: Duration) -> Self {
        Self {
            targets,
            every,
            within,
            last: None,
            running: None,
        }
    }

    /// Each report time: takes a probe that has ended, and while the link is
    /// down starts one when the last is [`PROBE_EVERY`] old. With the link
    /// up, what was found is forgotten, so the next outage asks at once.
    pub fn tend(&mut self, link_down: bool, now: Instant) {
        if let Some((began, result)) = &self.running {
            match result.try_recv() {
                Ok(answered) => {
                    self.last = Some((now, answered));
                    self.running = None;
                }
                // A thread that ended without a word, or one past its bound
                // and a margin, answered nothing.
                Err(TryRecvError::Disconnected) => {
                    self.last = Some((now, false));
                    self.running = None;
                }
                Err(TryRecvError::Empty)
                    if now.saturating_duration_since(*began) > self.within * 2 =>
                {
                    self.last = Some((now, false));
                    self.running = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if !link_down {
            self.last = None;
            return;
        }
        let due = self
            .last
            .is_none_or(|(at, _)| now.saturating_duration_since(at) >= self.every);
        if due && self.running.is_none() {
            self.running = Some((now, probe(self.targets.clone(), self.within)));
        }
    }

    /// What the last probe in this outage found: the internet answering or
    /// not; none while no probe has ended in it.
    pub fn verdict(&self) -> Option<bool> {
        self.last.map(|(_, answered)| answered)
    }

    /// Whether the last probe, in this outage, found the internet answering.
    pub fn answers(&self) -> bool {
        self.verdict() == Some(true)
    }
}

/// Connects to every target at once, each on its own thread, bounded by
/// `within`; says whether any connected.
fn probe(targets: Vec<SocketAddr>, within: Duration) -> Receiver<bool> {
    let (tell, told) = mpsc::sync_channel(1);
    let _ = thread::Builder::new()
        .name("lamp-health-reach".into())
        .spawn(move || {
            let (each, all) = mpsc::channel();
            for target in &targets {
                let each = each.clone();
                let target = *target;
                let _ = thread::Builder::new()
                    .name("lamp-health-reach-one".into())
                    .spawn(move || {
                        let _ = each.send(TcpStream::connect_timeout(&target, within).is_ok());
                    });
            }
            drop(each);
            let until = Instant::now() + within;
            let mut answered = false;
            while let Some(left) = until.checked_duration_since(Instant::now()) {
                match all.recv_timeout(left) {
                    Ok(true) => {
                        answered = true;
                        break;
                    }
                    Ok(false) => {}
                    Err(_) => break,
                }
            }
            let _ = tell.send(answered);
        });
    told
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn settle(reach: &mut Reach, link_down: bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while reach.running.is_some() {
            assert!(Instant::now() < until, "the probe never ended");
            thread::sleep(Duration::from_millis(2));
            reach.tend(link_down, Instant::now());
        }
    }

    /// A port nothing listens on: refused at once.
    fn closed() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        address
    }

    #[test]
    fn the_internet_answering_while_the_link_is_down_is_found_and_forgotten_when_it_is_up() {
        let open = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut reach = Reach::new(vec![closed(), open.local_addr().unwrap()]);
        // The link up: nothing is asked.
        reach.tend(false, Instant::now());
        assert!(reach.running.is_none() && reach.verdict().is_none());
        // Down: one probe, and either target answering is enough.
        reach.tend(true, Instant::now());
        assert!(reach.running.is_some());
        settle(&mut reach, true);
        assert!(reach.answers());
        // Not asked again before its time.
        reach.tend(true, Instant::now());
        assert!(reach.running.is_none());
        // Up again: forgotten, so the next outage asks at once.
        reach.tend(false, Instant::now());
        assert!(!reach.answers());
    }

    #[test]
    fn with_no_target_answering_the_probe_says_no_and_asks_again_only_after_its_time() {
        let every = Duration::from_millis(200);
        let mut reach = Reach::with_times(vec![closed(), closed()], every, PROBE_WITHIN);
        let start = Instant::now();
        reach.tend(true, start);
        settle(&mut reach, true);
        assert!(!reach.answers());
        assert_eq!(reach.verdict(), Some(false), "a verdict: no internet");
        let ended = reach.last.unwrap().0;
        reach.tend(true, ended + every / 2);
        assert!(reach.running.is_none(), "not before its time");
        reach.tend(true, ended + every);
        assert!(reach.running.is_some(), "again after it");
    }

    #[test]
    fn the_default_targets_are_never_on_port_53() {
        // A router forcing its own resolver answers on 53 with the WAN down.
        let targets = targets();
        assert!(targets.len() >= 2);
        assert!(targets.iter().all(|target| target.port() == 443));
    }

    #[test]
    fn a_target_that_never_answers_is_given_up_within_the_bound() {
        // A reserved, unroutable address: the connect hangs until its bound.
        let within = Duration::from_millis(300);
        let mut reach = Reach::with_times(
            vec!["10.255.255.1:53".parse().unwrap()],
            PROBE_EVERY,
            within,
        );
        let began = Instant::now();
        reach.tend(true, began);
        settle(&mut reach, true);
        assert!(!reach.answers());
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "{:?}",
            began.elapsed()
        );
    }
}
