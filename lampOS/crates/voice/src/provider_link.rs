//! Provider availability is control state, never PCM credit or output authority.
//! A retained unsent observation is replaced only by a newer validated revision.
use crate::provider_worker::Fault;
// A stepped fixture's provider correlates its paused runtime with the
// scenario's logical clock, as its worker reads it (#823).
#[cfg(test)]
use crate::scenario_clock::monotonic_us;
use lamp_gemini::{Context, FailureReason, LinkUpdate, LinkUpdateKind};
#[cfg(not(test))]
use lamp_ipc::monotonic_us;
use serde::{Deserialize, Serialize};
use std::io;
use tokio::time::Instant;

const MAX_CLOCK_SAMPLE_US: u64 = 1_000;
const CLOCK_SAMPLE_ATTEMPTS: usize = 3;
const MAX_SETUP_US: u64 = 120_000_000;
const MAX_ATTEMPTS: u32 = 16;
const CLOCK_HISTORY: usize = 1_024;
const CLOCK_SPACING_US: u64 = 1_000;
/// The existing authority watchdog is an upper bound, never an added wait.
const MAX_CORRELATION_US: u64 = 250_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceClockBounds {
    pub observed_upper_us: u64,
    pub outage_started_upper_us: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderLink {
    pub revision: u64,
    pub session: u64,
    /// Converted source observation; never the time a queued status was sent.
    pub observed_at_us: u64,
    /// None for host-native observations (including the fixed fixture). The
    /// Gemini bridge supplies inclusive bounds, not an exact latency sample.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_clock: Option<SourceClockBounds>,
    pub state: ProviderLinkState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderLinkState {
    Ready {
        /// This attempt's setup interval, not the entire recovered outage.
        setup_us: u64,
        attempts: u32,
        context: LinkContext,
        outage_started_at_us: Option<u64>,
        after: Option<Fault>,
    },
    Unavailable {
        reason: Fault,
    },
    Terminal {
        reason: LinkFailure,
        fault: Fault,
        attempts: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkContext {
    Initial,
    Resumed,
    ResumedBeforeLatest,
    Fresh,
}
impl From<Context> for LinkContext {
    fn from(context: Context) -> Self {
        match context {
            Context::Initial => Self::Initial,
            Context::Resumed => Self::Resumed,
            Context::ResumedBeforeLatest => Self::ResumedBeforeLatest,
            Context::Fresh => Self::Fresh,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkFailure {
    Shutdown,
    InitialConnect,
    NotRecoverable,
    NoResumableContext,
    AttemptsExhausted,
    BudgetExhausted,
    Unstable,
    Lifecycle,
}
impl From<FailureReason> for LinkFailure {
    fn from(reason: FailureReason) -> Self {
        match reason {
            FailureReason::Shutdown => Self::Shutdown,
            FailureReason::InitialConnect => Self::InitialConnect,
            FailureReason::NotRecoverable => Self::NotRecoverable,
            FailureReason::NoResumableContext => Self::NoResumableContext,
            FailureReason::AttemptsExhausted => Self::AttemptsExhausted,
            FailureReason::BudgetExhausted => Self::BudgetExhausted,
            FailureReason::Unstable => Self::Unstable,
            FailureReason::Lifecycle => Self::Lifecycle,
        }
    }
}

impl ProviderLink {
    /// Basic wire/time bounds. Receiver boot and revision history remain the
    /// control channel and coordinator's responsibility. Age is not freshness:
    /// a correctly retained source observation can be older than its envelope.
    pub fn validate(&self, now_us: u64) -> io::Result<()> {
        if self.revision == 0
            || self.session == 0
            || self.observed_at_us == 0
            || self.observed_at_us > now_us
        {
            return Err(invalid("invalid provider link identity or time"));
        }
        if let Some(bounds) = self.source_clock {
            HostInterval::checked(self.observed_at_us, bounds.observed_upper_us)?;
            if bounds.observed_upper_us > now_us {
                return Err(invalid("provider observation upper bound is future"));
            }
            match (self.state, bounds.outage_started_upper_us) {
                (
                    ProviderLinkState::Ready {
                        outage_started_at_us: Some(lower),
                        ..
                    },
                    Some(upper),
                ) => {
                    HostInterval::checked(lower, upper)?;
                    if upper > bounds.observed_upper_us {
                        return Err(invalid("provider outage upper bound follows readiness"));
                    }
                }
                (
                    ProviderLinkState::Ready {
                        outage_started_at_us: None,
                        ..
                    },
                    None,
                )
                | (
                    ProviderLinkState::Unavailable { .. } | ProviderLinkState::Terminal { .. },
                    None,
                ) => {}
                _ => return Err(invalid("provider outage correlation is incomplete")),
            }
        }
        match self.state {
            ProviderLinkState::Ready {
                setup_us,
                attempts,
                context,
                outage_started_at_us,
                after,
            } => {
                if attempts == 0
                    || attempts > MAX_ATTEMPTS
                    || setup_us > MAX_SETUP_US
                    || setup_us > self.observed_at_us
                {
                    return Err(invalid("invalid provider setup bounds"));
                }
                match context {
                    LinkContext::Initial
                        if self.session == 1
                            && attempts == 1
                            && outage_started_at_us.is_none()
                            && after.is_none() => {}
                    LinkContext::Initial => {
                        return Err(invalid("inconsistent initial provider link"));
                    }
                    _ if self.session > 1
                        && after.is_some()
                        && outage_started_at_us
                            .is_some_and(|at| at > 0 && at <= self.observed_at_us) => {}
                    _ => return Err(invalid("inconsistent recovered provider link")),
                }
            }
            ProviderLinkState::Terminal { attempts, .. } if attempts > MAX_ATTEMPTS => {
                return Err(invalid("invalid terminal attempt count"));
            }
            _ => {}
        }
        Ok(())
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Actual host bounds around an original runtime observation. No rate equality
/// between the two clocks is assumed, and no later delivery can freshen it.
#[derive(Clone, Copy, Debug)]
struct HostInterval {
    lower: u64,
    upper: u64,
}
impl HostInterval {
    fn checked(lower: u64, upper: u64) -> io::Result<Self> {
        if lower == 0
            || upper
                .checked_sub(lower)
                .is_none_or(|width| width > MAX_CORRELATION_US)
        {
            return Err(invalid(
                "provider source correlation interval is invalid or too wide",
            ));
        }
        Ok(Self { lower, upper })
    }
}

#[derive(Clone, Copy, Debug)]
struct ClockSample {
    runtime: Instant,
    before: u64,
    after: u64,
}
impl ClockSample {
    fn sample() -> io::Result<Self> {
        Self::sample_with(|| {
            let before = monotonic_us();
            let runtime = Instant::now();
            let after = monotonic_us();
            (runtime, before, after)
        })
    }
    fn sample_with(mut read: impl FnMut() -> (Instant, u64, u64)) -> io::Result<Self> {
        for _ in 0..CLOCK_SAMPLE_ATTEMPTS {
            let (runtime, before, after) = read();
            let width = after
                .checked_sub(before)
                .ok_or_else(|| invalid("provider host clock regressed"))?;
            if width <= MAX_CLOCK_SAMPLE_US {
                return Self::new(runtime, before, after);
            }
        }
        Err(invalid(
            "provider clock sampling repeatedly exceeded its uncertainty bound",
        ))
    }
    fn new(runtime: Instant, before: u64, after: u64) -> io::Result<Self> {
        if before == 0
            || after
                .checked_sub(before)
                .is_none_or(|width| width > MAX_CLOCK_SAMPLE_US)
        {
            return Err(invalid("provider clock sampling was delayed or regressed"));
        }
        Ok(Self {
            runtime,
            before,
            after,
        })
    }
}

/// A bounded ordering correlation, sampled on existing control ticks. A source
/// Instant is enclosed by actual host samples, not projected using an assumed
/// equal clock rate. History holds at least about one second at the 1 ms sample
/// spacing; normal ticks are 2 ms. A cached outage point survives a long recovery.
pub(crate) struct LinkClock {
    history: std::collections::VecDeque<ClockSample>,
    latest: ClockSample,
    observation: Option<(Instant, HostInterval)>,
    outage: Option<(Instant, HostInterval)>,
}
impl LinkClock {
    pub(crate) fn sample() -> io::Result<Self> {
        Ok(Self::from_sample(ClockSample::sample()?))
    }
    fn from_sample(sample: ClockSample) -> Self {
        let mut history = std::collections::VecDeque::with_capacity(CLOCK_HISTORY);
        history.push_back(sample);
        Self {
            history,
            latest: sample,
            observation: None,
            outage: None,
        }
    }
    pub(crate) fn tick(&mut self) -> io::Result<()> {
        self.retain(ClockSample::sample()?)
    }
    fn retain(&mut self, sample: ClockSample) -> io::Result<()> {
        if sample.runtime == self.latest.runtime
            && sample.before == self.latest.before
            && sample.after == self.latest.after
        {
            return Ok(());
        }
        if sample.runtime < self.latest.runtime
            || sample.before < self.latest.after
            || (sample.runtime == self.latest.runtime && sample.before > self.latest.after)
        {
            return Err(invalid(
                "provider correlation clocks regressed or stopped independently",
            ));
        }
        self.latest = sample;
        if sample
            .runtime
            .duration_since(self.history.back().expect("initial sample").runtime)
            >= std::time::Duration::from_micros(CLOCK_SPACING_US)
        {
            if self.history.len() == CLOCK_HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(sample);
        }
        Ok(())
    }
    fn correlate(&self, source: Instant) -> io::Result<HostInterval> {
        if source > self.latest.runtime {
            return Err(invalid("provider source time is future"));
        }
        for (at, interval) in [self.observation, self.outage].into_iter().flatten() {
            if at == source {
                return Ok(interval);
            }
        }
        let mut earlier = None;
        for sample in self.history.iter().chain(std::iter::once(&self.latest)) {
            if sample.runtime == source {
                return HostInterval::checked(sample.before, sample.after);
            }
            if sample.runtime < source {
                earlier = Some(sample.before);
            } else {
                let earlier = earlier
                    .ok_or_else(|| invalid("provider source predates correlation history"))?;
                // The worker was held up between these two samples (a hot,
                // throttled board: lamp-4ace on 2026-10-16 at 16:38, 302 ms,
                // and the worker exited). When in the gap is not known: the
                // source is taken at the first sample after it, never
                // earlier than it was, and the link goes on, as lapsed
                // authority snapshots are skipped.
                if sample.after.saturating_sub(earlier) > MAX_CORRELATION_US {
                    eprintln!(
                        "lamp-voice provider: held up {} ms between clock samples; a link change in the gap is dated after it",
                        sample.before.saturating_sub(earlier) / 1_000
                    );
                    return HostInterval::checked(sample.before, sample.after);
                }
                return HostInterval::checked(earlier, sample.after);
            }
        }
        Err(invalid("provider source lacks enclosing clock samples"))
    }
    pub(crate) fn convert(&mut self, update: LinkUpdate) -> io::Result<ProviderLink> {
        self.convert_sampled(update, ClockSample::sample()?)
    }
    fn convert_sampled(
        &mut self,
        update: LinkUpdate,
        current: ClockSample,
    ) -> io::Result<ProviderLink> {
        self.retain(current)?;
        if self
            .observation
            .is_some_and(|(source, _)| update.observed_at < source)
        {
            return Err(invalid("provider source observation time regressed"));
        }
        let observed = self.correlate(update.observed_at)?;
        let mut outage_interval = None;
        let state = match update.kind {
            LinkUpdateKind::Ready {
                context,
                attempts,
                started_at,
                completed_at,
                outage_started_at,
                after,
            } => {
                if update.observed_at != completed_at
                    || outage_started_at.is_some_and(|at| at > started_at)
                {
                    return Err(invalid("inconsistent provider setup timestamps"));
                }
                let setup = completed_at
                    .checked_duration_since(started_at)
                    .ok_or_else(|| invalid("provider setup timestamps regressed"))?;
                if let Some(source) = outage_started_at {
                    outage_interval = Some((source, self.correlate(source)?));
                }
                ProviderLinkState::Ready {
                    setup_us: u64::try_from(setup.as_micros())
                        .map_err(|_| invalid("provider setup overflow"))?,
                    attempts,
                    context: context.into(),
                    outage_started_at_us: outage_interval.map(|(_, interval)| interval.lower),
                    after: after.map(Fault::from),
                }
            }
            LinkUpdateKind::Unavailable { error } => ProviderLinkState::Unavailable {
                reason: error.into(),
            },
            LinkUpdateKind::Terminal { failure } => ProviderLinkState::Terminal {
                reason: failure.reason.into(),
                fault: failure.error.into(),
                attempts: failure.attempts,
            },
        };
        let link = ProviderLink {
            revision: update.revision,
            session: update.session.get(),
            observed_at_us: observed.lower,
            source_clock: Some(SourceClockBounds {
                observed_upper_us: observed.upper,
                outage_started_upper_us: outage_interval.map(|(_, interval)| interval.upper),
            }),
            state,
        };
        link.validate(current.after)?;
        self.observation = Some((update.observed_at, observed));
        if matches!(update.kind, LinkUpdateKind::Unavailable { .. }) {
            self.outage = self.observation;
        } else if let Some(interval) = outage_interval {
            self.outage = Some(interval);
        }
        Ok(link)
    }
}

#[derive(Default)]
pub(crate) struct PendingLink {
    pending: Option<ProviderLink>,
    revision: u64,
    session: u64,
    observed_at_us: u64,
    unavailable_session: u64,
    terminal: bool,
}
impl PendingLink {
    pub(crate) fn observe(&mut self, link: ProviderLink, now_us: u64) -> io::Result<()> {
        link.validate(now_us)?;
        if link.revision <= self.revision
            || link.session < self.session
            || link.observed_at_us < self.observed_at_us
            || self.terminal
        {
            return Err(invalid("provider link revision, session or time regressed"));
        }
        match link.state {
            ProviderLinkState::Ready { .. } if link.session <= self.unavailable_session => {
                return Err(invalid("ready resurrected an unavailable session"));
            }
            ProviderLinkState::Unavailable { .. } => self.unavailable_session = link.session,
            ProviderLinkState::Terminal { .. } => self.terminal = true,
            _ => {}
        }
        self.revision = link.revision;
        self.session = link.session;
        self.observed_at_us = link.observed_at_us;
        self.pending = Some(link);
        Ok(())
    }
    pub(crate) fn try_send(
        &mut self,
        mut send: impl FnMut(ProviderLink) -> io::Result<()>,
    ) -> io::Result<()> {
        let Some(link) = self.pending else {
            return Ok(());
        };
        match send(link) {
            Ok(()) => {
                self.pending = None;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_gemini::{Error, Failure, SessionId};
    use std::time::Duration;

    fn unavailable(revision: u64, session: u64, at: u64) -> ProviderLink {
        ProviderLink {
            revision,
            session,
            observed_at_us: at,
            source_clock: None,
            state: ProviderLinkState::Unavailable {
                reason: Fault::Transport,
            },
        }
    }
    fn ready(revision: u64, session: u64, at: u64) -> ProviderLink {
        ProviderLink {
            revision,
            session,
            observed_at_us: at,
            source_clock: None,
            state: ProviderLinkState::Ready {
                setup_us: 10,
                attempts: 1,
                context: if session == 1 {
                    LinkContext::Initial
                } else {
                    LinkContext::Resumed
                },
                outage_started_at_us: (session != 1).then_some(at - 10),
                after: (session != 1).then_some(Fault::Transport),
            },
        }
    }
    fn update(revision: u64, session: u64, at: Instant, kind: LinkUpdateKind) -> LinkUpdate {
        LinkUpdate {
            revision,
            session: SessionId::new(session).unwrap(),
            observed_at: at,
            kind,
        }
    }

    fn sample(at: Instant, before: u64, after: u64) -> ClockSample {
        ClockSample::new(at, before, after).unwrap()
    }

    #[test]
    fn actual_brackets_accept_rate_drift_without_restamping_or_guessing_exact_time() {
        let at = Instant::now();
        let mut clock = LinkClock::from_sample(sample(at, 10_000_000, 10_000_001));
        for step in 1..=30 {
            let runtime_us = step * 20_000 + step / 3;
            clock
                .retain(sample(
                    at + Duration::from_micros(runtime_us),
                    10_000_000 + step * 20_000,
                    10_000_002 + step * 20_000,
                ))
                .unwrap();
        }
        // The real Mac repro accumulated 10 us in about 600 ms despite
        // 1--2 us brackets. The old fixed-rate projection rejects this.
        let old_projection = 10_000_000 + 600_010;
        assert!(old_projection > clock.latest.after + 1);
        let source = at + Duration::from_micros(250_004);
        let notice = update(
            1,
            1,
            source,
            LinkUpdateKind::Unavailable {
                error: Error::Transport,
            },
        );
        let first = clock.convert_sampled(notice, clock.latest).unwrap();
        assert_eq!(first.observed_at_us, 10_240_000);
        assert_eq!(first.source_clock.unwrap().observed_upper_us, 10_260_002);
        assert!(first.observed_at_us <= 10_250_000);
        assert!(first.source_clock.unwrap().observed_upper_us >= 10_250_000);
        // A late retry retains the already-correlated source interval exactly.
        let later = clock
            .convert_sampled(
                notice,
                sample(at + Duration::from_secs(30), 39_999_500, 39_999_502),
            )
            .unwrap();
        assert_eq!(first, later);
    }

    /// A worker held up 300 ms between two clock samples (lamp-4ace on
    /// 2026-10-16, 302 ms): a link change in the gap is dated at the first
    /// sample after it, and its recovery goes on, never a worker exit.
    #[test]
    fn a_link_change_inside_a_300_ms_hold_up_is_dated_after_it_and_goes_on() {
        let at = Instant::now();
        let mut clock = LinkClock::from_sample(sample(at, 10_000_000, 10_000_001));
        let held = sample(at + Duration::from_millis(300), 10_300_000, 10_300_002);
        let lost = clock
            .convert_sampled(
                update(
                    1,
                    1,
                    at + Duration::from_millis(150),
                    LinkUpdateKind::Unavailable {
                        error: Error::Transport,
                    },
                ),
                held,
            )
            .unwrap();
        assert_eq!(lost.observed_at_us, 10_300_000);
        assert_eq!(lost.source_clock.unwrap().observed_upper_us, 10_300_002);
        let mut pending = PendingLink::default();
        pending.observe(lost, 10_300_002).unwrap();
        // Another hold-up, and a Ready inside it: its outage start is the
        // cached one, and it goes on too.
        let held_again = sample(at + Duration::from_millis(700), 10_700_000, 10_700_002);
        let completed = at + Duration::from_millis(600);
        let ready = clock
            .convert_sampled(
                update(
                    2,
                    2,
                    completed,
                    LinkUpdateKind::Ready {
                        context: Context::Resumed,
                        attempts: 2,
                        started_at: completed - Duration::from_millis(20),
                        completed_at: completed,
                        outage_started_at: Some(at + Duration::from_millis(150)),
                        after: Some(Error::Transport),
                    },
                ),
                held_again,
            )
            .unwrap();
        assert_eq!(ready.observed_at_us, 10_700_000);
        assert!(matches!(
            ready.state,
            ProviderLinkState::Ready {
                outage_started_at_us: Some(10_300_000),
                ..
            }
        ));
        pending.observe(ready, 10_700_002).unwrap();
    }

    #[test]
    fn cached_outage_survives_long_recovery_and_history_rollover() {
        let at = Instant::now();
        let mut clock = LinkClock::from_sample(sample(at, 10_000_000, 10_000_001));
        let lost_at = at + Duration::from_millis(5);
        let lost = clock
            .convert_sampled(
                update(
                    1,
                    1,
                    lost_at,
                    LinkUpdateKind::Unavailable {
                        error: Error::Transport,
                    },
                ),
                sample(at + Duration::from_millis(10), 10_010_000, 10_010_002),
            )
            .unwrap();
        for step in 1..=1_200 {
            clock
                .retain(sample(
                    at + Duration::from_millis(10 + step * 100),
                    10_010_000 + step * 100_000,
                    10_010_002 + step * 100_000,
                ))
                .unwrap();
        }
        assert_eq!(clock.history.len(), CLOCK_HISTORY);
        assert!(clock.history.front().unwrap().runtime > lost_at);
        let completed = clock.latest.runtime;
        let ready = clock
            .convert_sampled(
                update(
                    2,
                    2,
                    completed,
                    LinkUpdateKind::Ready {
                        context: Context::Resumed,
                        attempts: 2,
                        started_at: completed - Duration::from_millis(20),
                        completed_at: completed,
                        outage_started_at: Some(lost_at),
                        after: Some(Error::Transport),
                    },
                ),
                clock.latest,
            )
            .unwrap();
        assert!(matches!(ready.state, ProviderLinkState::Ready {
            setup_us: 20_000, outage_started_at_us: Some(old), ..
        } if old == lost.observed_at_us));
        assert_eq!(
            ready.source_clock.unwrap().outage_started_upper_us,
            Some(lost.source_clock.unwrap().observed_upper_us)
        );
        assert!(
            clock.correlate(at + Duration::from_millis(1)).is_err(),
            "unknown evicted source must not be stamped with receipt time"
        );
    }

    #[test]
    fn future_regressed_and_uncovered_sources_fail_closed_and_a_wide_one_is_dated_late() {
        let at = Instant::now();
        let mut clock = LinkClock::from_sample(sample(at, 10_000_000, 10_000_001));
        assert!(clock.correlate(at + Duration::from_nanos(1)).is_err());
        assert!(clock.correlate(at - Duration::from_nanos(1)).is_err());
        clock
            .retain(sample(
                at + Duration::from_millis(300),
                10_300_000,
                10_300_001,
            ))
            .unwrap();
        // Wide: a source inside a 300 ms hold-up is dated at the sample
        // after it, never earlier (lamp-4ace, 2026-10-16 16:38).
        let wide = clock.correlate(at + Duration::from_millis(1)).unwrap();
        assert_eq!((wide.lower, wide.upper), (10_300_000, 10_300_001));
        let good_at = at + Duration::from_millis(300);
        clock
            .convert_sampled(
                update(
                    1,
                    1,
                    good_at,
                    LinkUpdateKind::Unavailable {
                        error: Error::Transport,
                    },
                ),
                clock.latest,
            )
            .unwrap();
        assert!(
            clock
                .convert_sampled(
                    update(
                        2,
                        1,
                        at,
                        LinkUpdateKind::Unavailable {
                            error: Error::Transport
                        }
                    ),
                    clock.latest
                )
                .is_err()
        );
        assert!(
            clock
                .retain(sample(
                    good_at - Duration::from_micros(1),
                    10_300_002,
                    10_300_003
                ))
                .is_err()
        );
        assert!(
            clock
                .retain(sample(
                    good_at + Duration::from_micros(1),
                    10_299_000,
                    10_299_001
                ))
                .is_err()
        );
        let wrong_setup = update(
            2,
            2,
            good_at,
            LinkUpdateKind::Ready {
                context: Context::Resumed,
                attempts: 1,
                started_at: good_at + Duration::from_micros(1),
                completed_at: good_at,
                outage_started_at: Some(good_at),
                after: Some(Error::Transport),
            },
        );
        assert!(clock.convert_sampled(wrong_setup, clock.latest).is_err());
    }

    #[test]
    fn clock_history_is_fixed_and_fast_sampling_cannot_evict_its_time_window() {
        let at = Instant::now();
        let mut clock = LinkClock::from_sample(sample(at, 10_000_000, 10_000_000));
        for ns in 1..=2_000 {
            clock
                .retain(sample(
                    at + Duration::from_nanos(ns),
                    10_000_000 + ns / 1_000,
                    10_000_000 + ns / 1_000,
                ))
                .unwrap();
        }
        assert_eq!(clock.history.len(), 1);
        for ms in 1..=2_000 {
            clock
                .retain(sample(
                    at + Duration::from_millis(ms),
                    10_000_000 + ms * 1_000,
                    10_000_000 + ms * 1_000,
                ))
                .unwrap();
        }
        assert_eq!(clock.history.len(), CLOCK_HISTORY);
        assert_eq!(
            clock
                .latest
                .runtime
                .duration_since(clock.history.front().unwrap().runtime),
            Duration::from_millis(1_023)
        );
        assert!(clock.correlate(at).is_err());
    }

    #[test]
    fn a_paused_runtime_clock_cannot_fabricate_host_elapsed_time() {
        let at = Instant::now();
        let mut clock = LinkClock::from_sample(sample(at, 10_000_000, 10_000_001));
        assert!(clock.retain(sample(at, 10_100_000, 10_100_001)).is_err());
    }

    #[test]
    fn a_preempted_clock_sample_retries_finitely_without_restamping_source() {
        let at = Instant::now();
        let mut attempts = 0;
        let good = ClockSample::sample_with(|| {
            attempts += 1;
            if attempts == 1 {
                (at, 10_000_000, 10_005_000)
            } else {
                (at + Duration::from_millis(5), 10_005_000, 10_005_005)
            }
        })
        .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(good.before, 10_005_000);
        attempts = 0;
        assert!(
            ClockSample::sample_with(|| {
                attempts += 1;
                (at, 10, 10_000)
            })
            .is_err()
        );
        assert_eq!(attempts, CLOCK_SAMPLE_ATTEMPTS);
        attempts = 0;
        assert!(
            ClockSample::sample_with(|| {
                attempts += 1;
                (at, 20, 10)
            })
            .is_err()
        );
        assert_eq!(attempts, 1);
        assert!(ClockSample::new(at, 1, 1_002).is_err());
        assert!(HostInterval::checked(u64::MAX, 1).is_err());
        assert!(HostInterval::checked(0, 1).is_err());
    }

    #[test]
    fn pending_control_is_one_latest_slot_and_would_block_never_restamps_it() {
        let mut slot = PendingLink::default();
        slot.observe(ready(1, 1, 100), 100).unwrap();
        let mut blocked = 0;
        slot.try_send(|_| {
            blocked += 1;
            Err(io::ErrorKind::WouldBlock.into())
        })
        .unwrap();
        slot.observe(unavailable(2, 1, 110), 110).unwrap();
        let latest = ready(3, 2, 200);
        slot.observe(latest, 200).unwrap();
        for _ in 0..1_000 {
            slot.try_send(|link| {
                assert_eq!(link, latest);
                blocked += 1;
                Err(io::ErrorKind::WouldBlock.into())
            })
            .unwrap();
        }
        let mut delivered = Vec::new();
        slot.try_send(|link| {
            delivered.push(link);
            Ok(())
        })
        .unwrap();
        slot.try_send(|link| {
            delivered.push(link);
            Ok(())
        })
        .unwrap();
        assert_eq!(blocked, 1_001);
        assert_eq!(delivered, vec![latest]);
        assert!(slot.pending.is_none());
        // Revisions make skipped states visible; outage remains explicit even
        // when Unavailable could not be transmitted before this recovered state.
        assert_eq!(latest.revision, 3);
    }

    #[test]
    fn stale_ready_or_terminal_resurrection_cannot_replace_pending_failure() {
        let mut slot = PendingLink::default();
        slot.observe(ready(1, 1, 100), 100).unwrap();
        let unavailable = unavailable(2, 1, 110);
        slot.observe(unavailable, 110).unwrap();
        assert!(slot.observe(ready(3, 1, 120), 120).is_err());
        assert!(slot.observe(ready(1, 2, 120), 120).is_err());
        assert!(slot.observe(ready(3, 2, 109), 120).is_err());
        assert_eq!(slot.pending, Some(unavailable));
        let terminal = ProviderLink {
            revision: 3,
            session: 2,
            observed_at_us: 130,
            source_clock: None,
            state: ProviderLinkState::Terminal {
                reason: LinkFailure::AttemptsExhausted,
                fault: Fault::Transport,
                attempts: 1,
            },
        };
        slot.observe(terminal, 130).unwrap();
        assert!(slot.observe(ready(4, 3, 140), 140).is_err());
        assert_eq!(slot.pending, Some(terminal));
        assert!(
            slot.try_send(|_| Err(io::ErrorKind::ConnectionRefused.into()))
                .is_err()
        );
        assert_eq!(slot.pending, Some(terminal));
    }

    #[test]
    fn terminal_source_observation_and_fixed_failure_fields_survive_conversion() {
        let at = Instant::now();
        let mut clock = LinkClock::from_sample(sample(at, 10_000_000, 10_000_000));
        let notice = update(
            4,
            2,
            at,
            LinkUpdateKind::Terminal {
                failure: Failure {
                    reason: FailureReason::BudgetExhausted,
                    error: Error::SetupTimeout,
                    attempts: 2,
                },
            },
        );
        let converted = clock.convert_sampled(notice, clock.latest).unwrap();
        assert_eq!(converted.observed_at_us, 10_000_000);
        assert_eq!(
            converted.state,
            ProviderLinkState::Terminal {
                reason: LinkFailure::BudgetExhausted,
                fault: Fault::SetupTimeout,
                attempts: 2,
            }
        );
    }

    #[test]
    fn optional_source_bounds_are_checked_and_old_wire_stays_host_native() {
        let mut link = ready(1, 1, 100);
        let old_wire = serde_json::to_value(link).unwrap();
        assert!(old_wire.get("source_clock").is_none());
        assert_eq!(
            serde_json::from_value::<ProviderLink>(old_wire)
                .unwrap()
                .source_clock,
            None
        );
        link.source_clock = Some(SourceClockBounds {
            observed_upper_us: 110,
            outage_started_upper_us: None,
        });
        link.validate(1_000_000).unwrap();
        assert!(link.validate(109).is_err());
        for (upper, outage) in [
            (99, None),
            (100 + MAX_CORRELATION_US + 1, None),
            (110, Some(100)),
        ] {
            let mut bad = link;
            bad.source_clock = Some(SourceClockBounds {
                observed_upper_us: upper,
                outage_started_upper_us: outage,
            });
            assert!(bad.validate(1_000_000).is_err());
        }
        let mut encoded = serde_json::to_value(link).unwrap();
        encoded["source_clock"]["unknown"] = serde_json::json!(1);
        assert!(serde_json::from_value::<ProviderLink>(encoded).is_err());
        let mut recovered = ready(3, 2, 200);
        recovered.source_clock = Some(SourceClockBounds {
            observed_upper_us: 205,
            outage_started_upper_us: Some(192),
        });
        recovered.validate(1_000_000).unwrap();
        recovered
            .source_clock
            .as_mut()
            .unwrap()
            .outage_started_upper_us = None;
        assert!(recovered.validate(1_000_000).is_err());
    }

    #[test]
    fn wire_rejects_extra_fields_and_invalid_bounds_without_rejecting_old_source_age() {
        let valid = ready(1, 1, 100);
        valid.validate(100_000_000).unwrap();
        let encoded = serde_json::to_value(valid).unwrap();
        assert_eq!(
            serde_json::from_value::<ProviderLink>(encoded.clone()).unwrap(),
            valid
        );
        let mut extra = encoded.clone();
        extra["secret"] = serde_json::json!("not accepted");
        assert!(serde_json::from_value::<ProviderLink>(extra).is_err());
        let mut extra = encoded;
        extra["state"]["secret"] = serde_json::json!(1);
        assert!(serde_json::from_value::<ProviderLink>(extra).is_err());
        assert!(
            ProviderLink {
                revision: 0,
                ..valid
            }
            .validate(100)
            .is_err()
        );
        assert!(
            ProviderLink {
                session: 0,
                ..valid
            }
            .validate(100)
            .is_err()
        );
        assert!(valid.validate(99).is_err());
        let mut invalid = valid;
        if let ProviderLinkState::Ready { attempts, .. } = &mut invalid.state {
            *attempts = 17;
        }
        assert!(invalid.validate(100).is_err());
    }
}
