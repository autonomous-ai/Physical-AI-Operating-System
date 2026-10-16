//! Optional Schedule consumer. All path access, storage and Schedule calls are
//! isolated in one child. The coordinator polls only bounded local datagrams.
//! A prepared delivery is not permission to speak; a final Voice owner decides.
use lamp_schedule::delivery::{PreparedToken, QualifiedCompletion, QualifiedRetirement};
use lamp_turn_owner::TurnOwner;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, io, path::PathBuf, rc::Rc, time::Instant};

mod consumer;
mod scheduling;
#[cfg(test)]
mod tests;
mod transport;

#[cfg(test)]
pub(crate) use consumer::roundtrip_tests::RoundtripFixture;

pub const SPEAK_VARIABLE: &str = "LAMP_SCHEDULE_SPEAK";

/// Whether Schedule speech is on: `on`, or `skill`, where a due reminder
/// also waits for a skill's `from: schedule` ask (#939, (c);
/// [`crate::say::SKILL_WAITS_US`]).
fn speaking(value: Result<&str, &std::env::VarError>) -> bool {
    matches!(value, Ok("on" | "skill"))
}
pub const LEDGER_VARIABLE: &str = "LAMP_SCHEDULE_DELIVERY_DIR";
pub const LEASE_US: u64 = 750_000;
/// Due this long ago or more, a reminder is said only when someone is at the
/// lamp, once and gently, as something asked for earlier, and is then
/// acknowledged (the first builder, 2026-10-16: lamp-4ace's "stretch",
/// twelve hours late, was never said and stayed pending).
pub const OVERDUE_MS: u64 = 60 * 60_000;
const POLL_TARGET_US: u64 = 2_000;
const POLL_PACKETS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Offer {
    pub token: PreparedToken,
    pub text: String,
    pub late: bool,
    pub late_by_ms: Option<u64>,
    pub observed_at_us: u64,
    pub expires_at_us: u64,
}
impl Offer {
    /// Due an hour ago or more ([`OVERDUE_MS`]).
    pub fn overdue(&self) -> bool {
        self.late && self.late_by_ms.is_some_and(|ms| ms >= OVERDUE_MS)
    }
    /// Whether it may begin now: an overdue one only with someone at the
    /// lamp, never into an empty room.
    pub fn may_begin(&self, someone_here: bool) -> bool {
        !self.overdue() || someone_here
    }

    fn fresh(&self, now_us: u64) -> bool {
        self.token.key.validate().is_ok()
            && self.token.attempt > 0
            && !self.text.trim().is_empty()
            && self.text.len() <= 300
            && !self.text.chars().any(char::is_control)
            && self.observed_at_us <= now_us
            && self.observed_at_us < self.expires_at_us
            && self
                .observed_at_us
                .checked_add(LEASE_US)
                .is_some_and(|limit| self.expires_at_us <= limit)
            && now_us < self.expires_at_us
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ToWorker {
    Complete {
        token: PreparedToken,
        proof: QualifiedCompletion,
    },
    Retire {
        token: PreparedToken,
        proof: QualifiedRetirement,
    },
    Abandon {
        token: PreparedToken,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum FromWorker {
    Offer { offer: Offer },
    Withdraw { token: PreparedToken },
    Settled { token: PreparedToken },
    Idle {},
}

struct Progress {
    token: PreparedToken,
    owner: TurnOwner,
    privacy: u64,
    session: u64,
    had_audio: bool,
    qualified: bool,
    retired: u64,
    final_cursor: Option<(u64, u64, u64)>,
    live: bool,
    settlement: Option<ToWorker>,
}

/// Lives on the actual coordinator Reply. The owning consumer keeps only the
/// shared, bounded settlement slot, so a dropped Reply cannot silently become
/// completed speech. No destructor performs I/O or asserts cancellation.
pub(crate) struct Receipt(Rc<RefCell<Progress>>);
impl Receipt {
    pub fn invalidate(&mut self) {
        self.0.borrow_mut().qualified = false;
    }
    pub fn audio(&mut self) {
        self.0.borrow_mut().had_audio = true;
    }
    pub fn retired(&mut self, sequence: u64, epoch: u64, frame: u64) {
        let mut state = self.0.borrow_mut();
        state.retired = state.retired.saturating_add(1);
        state.final_cursor = Some((sequence, epoch, frame));
    }
    /// Called only after the real successful whole-reply join, never directly
    /// from SpeechRetired (which retires only one PCM generation).
    pub fn completed(&mut self) {
        let mut state = self.0.borrow_mut();
        let Some((playback_sequence, final_epoch, final_frame)) = state.final_cursor else {
            return;
        };
        if !state.qualified || !state.had_audio || state.retired == 0 || state.settlement.is_some()
        {
            return;
        }
        let proof = QualifiedCompletion {
            correlation: state.token.correlation.clone(),
            owner_boot: state.owner.boot().bytes(),
            turn: state.owner.turn(),
            microphone_generation: state.privacy,
            provider_session: state.session,
            playback_sequence,
            final_epoch,
            final_frame,
            retired_generations: state.retired,
        };
        state.settlement = Some(ToWorker::Complete {
            token: state.token.clone(),
            proof,
        });
    }
    /// A provider deferred before any PCM was received and the coordinator
    /// successfully revoked/published the exact owner. Such an attempt can be
    /// retried later. Partially spoken/otherwise uncertain attempts stay pending.
    pub fn deferred_before_audio(&mut self) {
        let mut state = self.0.borrow_mut();
        if state.had_audio || state.settlement.is_some() {
            return;
        }
        let proof = QualifiedRetirement {
            correlation: state.token.correlation.clone(),
            retired_by_session: state.token.correlation.consumer_session.clone(),
            basis: lamp_schedule::delivery::RetirementBasis::OwnerFenced {
                owner_boot: state.owner.boot().bytes(),
                turn: state.owner.turn(),
                microphone_generation: state.privacy,
            },
        };
        state.settlement = Some(ToWorker::Retire {
            token: state.token.clone(),
            proof,
        });
    }
}
impl Drop for Receipt {
    fn drop(&mut self) {
        let mut state = self.0.borrow_mut();
        state.live = false;
        if state.settlement.is_none() {
            state.settlement = Some(ToWorker::Abandon {
                token: state.token.clone(),
            });
        }
    }
}

#[cfg(test)]
pub(crate) struct Probe(Rc<RefCell<Progress>>);
#[cfg(test)]
impl Probe {
    pub fn settlement(&self) -> Option<ToWorker> {
        self.0.borrow().settlement.clone()
    }
}
#[cfg(test)]
pub(crate) fn receipt_for_test(owner: TurnOwner, privacy: u64, session: u64) -> (Receipt, Probe) {
    let state = Rc::new(RefCell::new(Progress {
        token: PreparedToken {
            key: lamp_schedule::delivery::DeliveryKey {
                store_id: "0123456789abcdef0123456789abcdef".into(),
                id: 1,
                delivery: 1,
            },
            attempt: 1,
            correlation: lamp_schedule::delivery::Correlation {
                consumer_session: "test-consumer".into(),
                sequence: 1,
            },
        },
        owner,
        privacy,
        session,
        had_audio: false,
        qualified: true,
        retired: 0,
        final_cursor: None,
        live: true,
        settlement: None,
    }));
    (Receipt(Rc::clone(&state)), Probe(state))
}

/// One instance belongs to run_source, outside every mute/unmute audio cycle.
pub(crate) struct Reminders {
    transport: transport::Transport,
    offer: Option<Offer>,
    active: Option<Rc<RefCell<Progress>>>,
    unadmitted: Option<ToWorker>,
    sent: bool,
    failed: bool,
    /// `LAMP_SCHEDULE_SPEAK=skill`: a due reminder waits for a skill to ask.
    by_skill: bool,
}
impl Reminders {
    pub fn from_environment() -> io::Result<Option<Self>> {
        let value = std::env::var(SPEAK_VARIABLE);
        if !speaking(value.as_deref()) {
            return Ok(None);
        }
        let by_skill = value.as_deref() == Ok("skill");
        // Config is validated in the child. A slow/missing directory cannot
        // delay audio startup; parent startup only creates/spawns its IPC peer.
        let transport = transport::Transport::spawn(&std::env::current_exe()?)?;
        Ok(Some(Self {
            transport,
            offer: None,
            active: None,
            unadmitted: None,
            sent: false,
            failed: false,
            by_skill,
        }))
    }

    /// Whether a due reminder waits for a skill's ask (`LAMP_SCHEDULE_SPEAK=skill`).
    pub fn by_skill(&self) -> bool {
        self.by_skill
    }

    pub fn poll(&mut self, now_us: u64) {
        if self.failed {
            // Retain and reap the one failed child; never manufacture a new
            // consumer merely because its termination signal was sent.
            let _ = self.transport.try_exit();
            return;
        }
        if let Err(error) = self.poll_inner(now_us) {
            // Due reminders stay pending until the voice restarts: say so.
            eprintln!(
                "lamp-voice: Schedule consumer failed ({error}); due reminders wait for a restart"
            );
            self.failed = true;
            self.offer = None;
            self.transport.stop();
        }
    }
    fn poll_inner(&mut self, now_us: u64) -> io::Result<()> {
        let started = Instant::now();
        self.transport.poll()?;
        if self.transport.try_exit()? {
            return Err(io::Error::other("schedule consumer exited"));
        }
        for _ in 0..POLL_PACKETS {
            if started.elapsed().as_micros() >= u128::from(POLL_TARGET_US) {
                break;
            }
            let Some(message) = self.transport.receive::<FromWorker>()? else {
                break;
            };
            match message {
                FromWorker::Offer { offer }
                    if self.transport.claim_held() && offer.fresh(now_us) =>
                {
                    if self.unadmitted.is_some() {
                        continue;
                    } else if let Some(active) = &self.active {
                        if active.borrow().token == offer.token {
                            self.offer = Some(offer);
                        }
                    } else {
                        self.offer = Some(offer);
                    }
                }
                FromWorker::Offer { .. } => self.offer = None,
                FromWorker::Withdraw { token } => {
                    if self
                        .offer
                        .as_ref()
                        .is_some_and(|offer| offer.token == token)
                    {
                        self.offer = None;
                    }
                    if self.active.is_none() && self.unadmitted.is_none() {
                        self.unadmitted = Some(ToWorker::Retire {
                            proof: QualifiedRetirement {
                                correlation: token.correlation.clone(),
                                retired_by_session: token.correlation.consumer_session.clone(),
                                basis: lamp_schedule::delivery::RetirementBasis::NeverAdmitted {},
                            },
                            token,
                        });
                        self.sent = false;
                    }
                }
                FromWorker::Settled { token } => {
                    if self
                        .unadmitted
                        .as_ref()
                        .is_some_and(|command| match command {
                            ToWorker::Retire { token: pending, .. } => pending == &token,
                            _ => false,
                        })
                    {
                        self.unadmitted = None;
                        self.offer = None;
                        self.sent = false;
                    }
                    if self
                        .active
                        .as_ref()
                        .is_some_and(|state| state.borrow().token == token && !state.borrow().live)
                    {
                        self.active = None;
                        self.offer = None;
                        self.sent = false;
                    }
                }
                FromWorker::Idle {} => {}
            }
        }
        if !self.sent
            && let Some(active) = &self.active
        {
            let state = active.borrow();
            if !state.live
                && let Some(settlement) = &state.settlement
            {
                self.sent = self.transport.send(settlement)?;
            }
        }
        if !self.sent
            && let Some(settlement) = &self.unadmitted
        {
            self.sent = self.transport.send(settlement)?;
        }
        Ok(())
    }
    pub fn offer(&self, now_us: u64) -> Option<Offer> {
        if self.failed
            || self.active.is_some()
            || self.unadmitted.is_some()
            || !self.transport.claim_held()
        {
            return None;
        }
        self.offer
            .as_ref()
            .filter(|offer| offer.fresh(now_us))
            .cloned()
    }
    pub fn active_valid(&self, owner: TurnOwner, now_us: u64) -> bool {
        let Some(active) = &self.active else {
            return true;
        };
        let state = active.borrow();
        state.owner != owner
            || (!self.failed
                && self
                    .offer
                    .as_ref()
                    .is_some_and(|offer| offer.token == state.token && offer.fresh(now_us)))
    }
    pub fn admitted(
        &mut self,
        offer: Offer,
        owner: TurnOwner,
        privacy: u64,
        session: u64,
    ) -> Receipt {
        let state = Rc::new(RefCell::new(Progress {
            token: offer.token,
            owner,
            privacy,
            session,
            had_audio: false,
            qualified: true,
            retired: 0,
            final_cursor: None,
            live: true,
            settlement: None,
        }));
        self.active = Some(Rc::clone(&state));
        self.sent = false;
        Receipt(state)
    }
}

/// Internal child entry. It opens no provider, microphone, speaker or actuator.
pub fn run_consumer() -> io::Result<()> {
    if !speaking(std::env::var(SPEAK_VARIABLE).as_deref()) {
        return Err(io::Error::other("Schedule speech is disabled"));
    }
    scheduling::background()?;
    let ledger = std::env::var_os(LEDGER_VARIABLE)
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::other("Schedule speech needs an explicit persistent delivery directory")
        })?;
    let socket = std::env::var_os("LAMP_SCHEDULE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/lampos-schedule/call.sock"));
    let root = std::env::var_os(lamp_bus::ROOT_VARIABLE)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(lamp_bus::DEFAULT_ROOT));
    consumer::run(&ledger, &socket, &root)
}
