//! Cloud I/O stays in a separate process. Input is private, bounded and tied to
//! its capture privacy generation; response lineage never follows a new turn.
//!
//! Production Legacy mode ends on a lost unfinished request. Explicit Typed
//! and Coordinated modes retain its terminal outcome while recovering without
//! replay. Coordinated availability bypasses PCM credit, not ownership or the
//! ordered answer stream. See `docs/provider-link-channel.md`.
use crate::{
    config::ProviderConfig,
    provider_flow::{OutputSender, PACKET_SAMPLES},
    provider_link::{LinkClock, LinkFailure, PendingLink, ProviderLink},
    transport::{Channel, WorkerChannels},
    visual_context::{ContextDelivery, Source as VisualSource, Turn as VisualTurn},
    wire::{Control, WorkerEvent},
};
use lamp_gemini::{
    BarrierPolicy, BarrierWait, Connect, Context, Dialer, Discard, Error, Event, Failure,
    FailureReason, Notice, RecoveryPolicy, RequestId, Resumption, SessionConfig, SessionId, Stage,
    StartRefusal, State, SuccessorStart, Supervisor,
};
use lamp_interaction::{BootId, BoundaryGuard, MonoTime, Permission, Snapshot};
use lamp_ipc::monotonic_us;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io,
    path::Path,
    str::FromStr,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_QUEUED_OUTPUT: usize = 512;
/// Above this backlog no further provider event is taken. The transport then
/// stops reading its socket, so a fast or bursty service is held back by flow
/// control instead of ending the worker. One event adds at most 50 packets.
const OUTPUT_HIGH_WATER: usize = 384;
/// Initial setup is unchanged. A handle the service volunteers is kept in this
/// process's memory only, so a reconnect can restore the conversation.
const RESUMPTION: Resumption = Resumption::Retain;
/// Until live service probes qualify cancellation ordering, an old response
/// must settle before a successor is committed. Quiet alone cannot attribute
/// a late answer. The assumption policy remains explicit in transport tests.
const BARRIER: BarrierPolicy = BarrierPolicy::Require;
/// How long `Continuation::KeptStart` keeps a next request's start back. In
/// the one real-service run that shows it, an answer's first audio came
/// 1.43 s after its question ended. A proposed value, not a measured need.
pub const KEPT_START_PATIENCE: Duration = Duration::from_millis(2_500);

/// What the transport does when a person goes on speaking before the first
/// answer has begun. Chosen once, by an explicit option, for the worker's
/// lifetime; the policy stays `Require` either way.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Continuation {
    /// As it has always been: the next request's start is written at once
    /// and the barrier's quiet bound counts from that interruption.
    #[default]
    Off,
    /// The next request's start and opening audio are kept in the transport
    /// until the earlier answer begins, and is then interrupted, or
    /// completes, for at most [`KEPT_START_PATIENCE`]; the barrier's quiet
    /// bound counts only while a request's End is withheld; and how each
    /// barrier ended is written to the worker's standard error.
    KeptStart,
}
impl Continuation {
    pub const FLAG: &'static str = "--provider-continuation";
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::KeptStart => "kept-start",
        }
    }
    /// The provider worker's own arguments for this choice: none when off,
    /// so that a worker started without the option is started as before.
    pub fn arguments(self) -> Vec<String> {
        match self {
            Self::Off => Vec::new(),
            Self::KeptStart => vec![Self::FLAG.to_owned(), self.as_str().to_owned()],
        }
    }
    /// Take a leading `--provider-continuation VALUE` off a worker's flags.
    pub fn take(flags: &[String]) -> io::Result<(Self, &[String])> {
        match flags {
            [flag, value, rest @ ..] if flag == Self::FLAG => Ok((value.parse()?, rest)),
            [flag] if flag == Self::FLAG => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "provider continuation needs a value",
            )),
            _ => Ok((Self::Off, flags)),
        }
    }
    pub(crate) fn apply(self, config: SessionConfig) -> io::Result<SessionConfig> {
        match self {
            Self::Off => Ok(config),
            Self::KeptStart => config
                .barrier_wait(BarrierWait::AtCommit)
                .barrier_reports(true)
                .successor_start(SuccessorStart::AfterFirstOutput {
                    patience: KEPT_START_PATIENCE,
                })
                .map_err(|error| io::Error::other(format!("provider continuation: {error}"))),
        }
    }
}
impl FromStr for Continuation {
    type Err = io::Error;
    fn from_str(value: &str) -> io::Result<Self> {
        match value {
            "off" => Ok(Self::Off),
            "kept-start" => Ok(Self::KeptStart),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "provider continuation must be off or kept-start",
            )),
        }
    }
}
const CONTROL_SLICE: usize = 16;
const INPUT_SLICE: usize = 8;
const RETAINED_INPUT_US: u64 = 100_000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderInput {
    Start {
        request: u64,
        /// The connection selected at admission, never replaced at dequeue.
        session: u64,
        privacy_generation: u64,
    },
    Audio {
        request: u64,
        privacy_generation: u64,
        sequence: u64,
        read_completed_at_us: u64,
        samples: Vec<i16>,
    },
    End {
        request: u64,
        privacy_generation: u64,
    },
}
impl ProviderInput {
    fn request(&self) -> u64 {
        match self {
            Self::Start { request, .. }
            | Self::Audio { request, .. }
            | Self::End { request, .. } => *request,
        }
    }
    fn privacy_generation(&self) -> u64 {
        match self {
            Self::Start {
                privacy_generation, ..
            }
            | Self::Audio {
                privacy_generation, ..
            }
            | Self::End {
                privacy_generation, ..
            } => *privacy_generation,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderOutput {
    Ready {
        setup_us: u64,
    },
    Started {
        request: u64,
        /// The connection that actually accepted Start, never current link state.
        session: u64,
        waiting_for_barrier: bool,
    },
    Audio {
        request: u64,
        sequence: u64,
        /// Wrapper event observation, not a socket-read or acoustic timestamp.
        provider_event_at_us: u64,
        source_frames: usize,
        source_offset: usize,
        samples: Vec<i16>,
    },
    Transcript {
        request: Option<u64>,
        text: String,
        finished: bool,
    },
    GenerationComplete {
        request: u64,
    },
    Interrupted {
        request: u64,
    },
    TurnComplete {
        request: u64,
        idle: bool,
    },
    TurnFailed {
        request: u64,
        stage: FailedStage,
        reason: Fault,
    },
    /// Input up to `request` was abandoned, not continued, while the
    /// transport still kept some of it or had written some of it without an
    /// End. Only under `Continuation::KeptStart`. `dropped_samples` were
    /// kept and never sent. `sent_samples` had already been handed to the
    /// connection: they are not discarded and cannot be recalled, and the
    /// next request is heard after them.
    InputDiscarded {
        request: u64,
        dropped_samples: u64,
        sent_samples: u64,
    },
    OwnershipAssumed {
        request: Option<u64>,
        superseded: Option<u64>,
    },
    /// The upstream output stream has ended, after its retained PCM and turn
    /// outcomes. This is not process EOF: a later local Start refusal can
    /// still produce TurnFailed(NotDelivered), until explicit Stop.
    Ended {
        reason: LinkFailure,
        fault: Fault,
        attempts: u32,
    },
}

/// Compatibility encoding for explicit Typed mode. Turn failures and ownership
/// assumptions share the exact flat wire shape with [`ProviderOutput`].
/// Coordinated link availability instead uses the separate control channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderStatus {
    /// No provider connection. Input admitted from now until `Recovered`
    /// cannot be delivered and will be reported with `TurnFailed`. Output that
    /// arrived before the failure may still follow.
    Unavailable { reason: Fault },
    /// A connection is ready again. `context` says whether the conversation
    /// so far is still known to the provider.
    Recovered {
        outage_us: u64,
        attempts: u32,
        context: RecoveredContext,
        reason: Fault,
    },
    /// The provider will produce no further output for this accepted request.
    /// Its input was not replayed and its answer was not requested again. All
    /// output that did arrive for it precedes this event.
    TurnFailed {
        request: u64,
        stage: FailedStage,
        reason: Fault,
    },
    /// The provider never confirmed that `superseded` stopped. `request`
    /// proceeds on the assumption that it did, so the ownership of its answer
    /// is not qualified evidence.
    OwnershipAssumed {
        request: Option<u64>,
        superseded: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailedStage {
    /// Admitted while no connection existed; nothing of it reached the provider.
    NotDelivered,
    InputOpen,
    AwaitingResponse,
    /// Part of an answer was delivered before the connection was lost.
    Responding,
}
impl From<Stage> for FailedStage {
    fn from(stage: Stage) -> Self {
        match stage {
            Stage::InputOpen => Self::InputOpen,
            Stage::AwaitingResponse => Self::AwaitingResponse,
            // A generated answer is settled, never failed.
            Stage::Responding | Stage::Generated => Self::Responding,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveredContext {
    Resumed,
    /// Resumed from a point before the latest exchange.
    ResumedBeforeLatest,
    /// The provider no longer knows the earlier conversation.
    Fresh,
}

/// Fixed failure categories. No server text, URL or credential is carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case", deny_unknown_fields)]
pub enum Fault {
    ConnectTimeout,
    SetupTimeout,
    ReadTimeout,
    WriteTimeout,
    BarrierTimeout,
    ResponseTimeout,
    StalledResponse,
    CompletionTimeout,
    Authentication,
    RateLimited,
    QuotaExceeded,
    ServerUnavailable,
    ServerRejected,
    ServerGoAway,
    Transport,
    PeerClosed {
        code: Option<u16>,
    },
    /// The provider sent something this client does not accept.
    Protocol,
    /// A fault inside this process, including a closed or overloaded queue.
    Local,
}
impl From<Error> for Fault {
    fn from(error: Error) -> Self {
        match error {
            Error::ConnectTimeout => Self::ConnectTimeout,
            Error::SetupTimeout => Self::SetupTimeout,
            Error::ReadTimeout => Self::ReadTimeout,
            Error::WriteTimeout => Self::WriteTimeout,
            Error::BarrierTimeout => Self::BarrierTimeout,
            Error::ResponseTimeout => Self::ResponseTimeout,
            Error::StalledResponse => Self::StalledResponse,
            Error::CompletionTimeout => Self::CompletionTimeout,
            Error::Authentication => Self::Authentication,
            Error::RateLimited => Self::RateLimited,
            Error::QuotaExceeded => Self::QuotaExceeded,
            Error::ServerUnavailable => Self::ServerUnavailable,
            Error::ServerRejected => Self::ServerRejected,
            Error::ServerGoAway => Self::ServerGoAway,
            Error::Transport => Self::Transport,
            Error::PeerClosed { code } => Self::PeerClosed { code },
            Error::MalformedMessage
            | Error::UnsupportedMessage
            | Error::MessageTooLarge
            | Error::InvalidAudio
            | Error::UnexpectedResponse => Self::Protocol,
            Error::InvalidConfiguration
            | Error::Backpressure
            | Error::StaleInput
            | Error::InputSequence
            | Error::StaleRequest
            | Error::OverlappingInput
            | Error::InputCancelled
            | Error::NotConnected
            | Error::Closed => Self::Local,
        }
    }
}

/// How a provider outage reaches the coordinator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reporting {
    /// The contract the coordinator implements today. A recovery appears as a
    /// repeated `Ready`; a request that cannot be finished ends the worker.
    Legacy,
    /// Send [`ProviderStatus`] and keep running through a lost request.
    Typed,
    /// Availability uses priority control; ordered PCM and exact outcomes use
    /// the credited data stream. This mode requires a coordinated consumer.
    Coordinated,
}

pub(crate) struct Options {
    pub(crate) reporting: Reporting,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Outbound {
    Output(ProviderOutput),
    Status(ProviderStatus),
}

/// Unsent output of retired requests is dropped, as the capacity contract
/// requires: at playback pace it would delay the next answer by seconds.
/// Survivors keep their order. Link and failure statuses are never dropped.
fn prune_retired_output(output: &mut VecDeque<Outbound>, retired_through: u64) {
    output.retain(|item| {
        let Outbound::Output(event) = item else {
            return true;
        };
        let request = match event {
            ProviderOutput::Ready { .. }
            | ProviderOutput::TurnFailed { .. }
            // About a retired request by definition, and still owed.
            | ProviderOutput::InputDiscarded { .. }
            | ProviderOutput::OwnershipAssumed { .. }
            | ProviderOutput::Ended { .. } => None,
            ProviderOutput::Transcript { request, .. } => *request,
            ProviderOutput::Started { request, .. }
            | ProviderOutput::Audio { request, .. }
            | ProviderOutput::GenerationComplete { request }
            | ProviderOutput::Interrupted { request }
            | ProviderOutput::TurnComplete { request, .. } => Some(*request),
        };
        request.is_none_or(|request| request > retired_through)
    });
}

/// The single refused Start stays in intake until its outcome has reserved
/// space. Input consumption pauses meanwhile; neither a later request nor a
/// full PCM queue can overwrite the report or grow the bounded output queue.
fn queue_undelivered(handoff: &mut InputHandoff, output: &mut VecDeque<Outbound>) {
    if output.len() < MAX_QUEUED_OUTPUT
        && let Some(request) = handoff.undelivered.take()
    {
        output.push_back(Outbound::Status(ProviderStatus::TurnFailed {
            request,
            stage: FailedStage::NotDelivered,
            reason: Fault::Local,
        }));
    }
}

pub async fn run(channels: WorkerChannels, boot: BootId, config_path: &Path) -> Result<()> {
    run_with_reporting(channels, boot, config_path, Reporting::Legacy).await
}

/// Explicit consumer-contract selection. The default entry point stays Legacy
/// until coordinated admission and terminal handling qualify together.
pub async fn run_with_reporting(
    channels: WorkerChannels,
    boot: BootId,
    config_path: &Path,
    reporting: Reporting,
) -> Result<()> {
    run_with_visual_context(channels, boot, config_path, reporting, None).await
}

/// Optional worker-local camera stream. No JPEG travels through audio IPC.
pub async fn run_with_visual_context(
    channels: WorkerChannels,
    boot: BootId,
    config_path: &Path,
    reporting: Reporting,
    visual: Option<(VisualSource, BootId)>,
) -> Result<()> {
    run_with_continuation(
        channels,
        boot,
        config_path,
        reporting,
        visual,
        Continuation::Off,
    )
    .await
}

/// The same, with an explicit choice of what happens when a person goes on
/// before the first answer has begun.
pub async fn run_with_continuation(
    channels: WorkerChannels,
    boot: BootId,
    config_path: &Path,
    reporting: Reporting,
    visual: Option<(VisualSource, BootId)>,
    continuation: Continuation,
) -> Result<()> {
    let config = continuation.apply(
        ProviderConfig::load(config_path)?
            .into_session_with_camera(visual.is_some())?
            .resumption(RESUMPTION)
            .barrier_policy(BARRIER),
    )?;
    let provider = Supervisor::new(Dialer::new(config), RecoveryPolicy::default())?;
    let options = Options { reporting };
    let visual = visual
        .map(|(source, worker)| ContextDelivery::new(source, boot, worker))
        .transpose()?;
    serve_with_visual(channels, boot, provider, options, visual).await
}

/// Paired producer/consumer qualification uses the real loop without loading
/// credentials or selecting a cloud endpoint. This seam is absent from builds.
#[cfg(test)]
pub(crate) async fn serve_coordinated_test<C: Connect>(
    channels: WorkerChannels,
    boot: BootId,
    provider: Supervisor<C>,
) -> Result<()> {
    serve(
        channels,
        boot,
        provider,
        Options {
            reporting: Reporting::Coordinated,
        },
    )
    .await
}

#[cfg(test)]
async fn serve<C: Connect>(
    channels: WorkerChannels,
    boot: BootId,
    provider: Supervisor<C>,
    options: Options,
) -> Result<()> {
    serve_with_visual(channels, boot, provider, options, None).await
}

async fn serve_with_visual<C: Connect>(
    mut channels: WorkerChannels,
    boot: BootId,
    mut provider: Supervisor<C>,
    options: Options,
    mut visual: Option<ContextDelivery>,
) -> Result<()> {
    channels.control.send(WorkerEvent::Ready)?;
    let coordinated = options.reporting == Reporting::Coordinated;
    let typed = options.reporting != Reporting::Legacy;
    let mut link_clock = coordinated.then(LinkClock::sample).transpose()?;
    let mut link_status = PendingLink::default();
    let mut upstream_ended = false;
    let mut intake = ProviderIntake::new(boot, monotonic_us());
    intake.handoff.tolerate_outage = typed;
    let mut ticker = tokio::time::interval(Duration::from_millis(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Link status is small and must not wait behind playback-paced audio.
    let mut urgent: VecDeque<Outbound> = VecDeque::with_capacity(4);
    let mut output = VecDeque::with_capacity(MAX_QUEUED_OUTPUT);
    let mut output_sequence = 0u64;
    let mut discards = DiscardLog::default();
    loop {
        // Control is polled first and never waits for the provider: stop,
        // privacy and retirement are serviced every tick whether a connection
        // is ready, reconnecting, draining or delivering a burst.
        tokio::select! {
            biased;
            _=ticker.tick()=>{
                let mut control_budget = CONTROL_SLICE;
                let control = intake.drain_control(&mut monotonic_us, &mut || channels.control.receive(), &mut control_budget)?;
                if control == ControlDrain::Stopped { provider.shutdown(); return Ok(()); }
                // Notice a dead connection and start its replacement now, even
                // while no provider event is being taken.
                service_link(&mut provider, link_clock.as_mut(), &mut link_status, |link|
                    channels.control.send(WorkerEvent::ProviderLink { link }))?;
                // Camera revocation also bypasses PCM credit and a full control
                // slice. A partial cloud image write may abandon that connection;
                // ordinary lost-turn handling remains the sole terminal outcome.
                if let Some(visual) = visual.as_mut() {
                    visual.authority(&mut intake.guard, &provider, monotonic_us());
                }
                // Local retirement cannot wait behind a full control slice.
                intake.handoff.synchronize(&provider)?;
                prune_retired_output(&mut output, intake.handoff.retired_through);
                if control == ControlDrain::Deferred { continue; }
                queue_undelivered(&mut intake.handoff, &mut output);
                if intake.handoff.undelivered.is_none() {
                    let received = intake.receive_inputs(&mut monotonic_us, &mut || channels.control.receive(), &mut channels.data, &provider, &mut control_budget);
                    match received {
                        Ok(ControlDrain::Stopped) => { provider.shutdown(); return Ok(()); }
                        Ok(ControlDrain::Deferred) => continue,
                        Ok(ControlDrain::Empty) => {}
                        // Admitted input cannot be held until a connection exists:
                        // it would be answered late or not at all. Say so and end.
                        Err(error) if !provider.is_connected() => return Err(io::Error::other(format!("provider unavailable while input was admitted: {error}")).into()),
                        Err(error) => return Err(error),
                    }
                }
                if let Some(visual) = visual.as_mut() {
                    visual.authority(&mut intake.guard, &provider, monotonic_us());
                    visual.poll(intake.handoff.visual_turn(), &provider, monotonic_us());
                    // Frame reads and privacy use separate transports. Read
                    // priority again immediately before exporting the JPEG.
                    let control = intake.drain_control(&mut monotonic_us, &mut || channels.control.receive(), &mut control_budget)?;
                    if control == ControlDrain::Stopped { provider.shutdown(); return Ok(()); }
                    visual.authority(&mut intake.guard, &provider, monotonic_us());
                    intake.handoff.synchronize(&provider)?;
                    if control == ControlDrain::Empty {
                        visual.export(intake.handoff.visual_turn(), &mut intake.guard, &provider);
                    }
                    visual.send_receipt(|receipt| channels.control.send(WorkerEvent::VisualContext { receipt: receipt.clone() }))?;
                }
                queue_undelivered(&mut intake.handoff, &mut output);
                // receive_inputs can observe a newer authority too.
                prune_retired_output(&mut output, intake.handoff.retired_through);
                for _ in 0..8 {
                    // Link status goes first; it still needs capacity like
                    // every other packet.
                    let queue = if urgent.is_empty() { &mut output } else { &mut urgent };
                    let Some(front)=queue.front() else {break;};
                    if intake.output_flow.try_send(|| channels.data.send(front))? {
                        queue.pop_front();
                    } else { break; }
                }
            },
            notice=next_notice(&mut provider, coordinated), if !upstream_ended && output.len() < OUTPUT_HIGH_WATER=>{
                match notice {
                    Ok(Notice::Ready {..} | Notice::Unavailable {..}) if coordinated => {
                        return Err(io::Error::other("availability escaped the provider control plane").into());
                    },
                    Ok(Notice::Ready {session,context,attempts,elapsed,after})=>{
                        let elapsed_us = elapsed.as_micros().try_into().unwrap_or(u64::MAX);
                        match (after, typed) {
                            (Some(error), true)=>urgent.push_back(Outbound::Status(ProviderStatus::Recovered {
                                outage_us: elapsed_us,
                                attempts,
                                context: match context {
                                    Context::Resumed | Context::Initial => RecoveredContext::Resumed,
                                    Context::ResumedBeforeLatest => RecoveredContext::ResumedBeforeLatest,
                                    Context::Fresh => RecoveredContext::Fresh,
                                },
                                reason: error.into(),
                            })),
                            (after, _)=>{
                                if let Some(error)=after {
                                    // The current contract carries a recovery only as
                                    // a repeated Ready. Fixed categories, no server text.
                                    eprintln!("lamp-live provider: recovered after {error}; session={} context={context:?} attempts={attempts} outage_ms={}", session.get(), elapsed.as_millis());
                                }
                                output.push_back(Outbound::Output(ProviderOutput::Ready { setup_us: elapsed_us }));
                            },
                        }
                    },
                    Ok(Notice::Event(Event::BarrierAssumed {superseded,successor,..})) if typed=>output.push_back(Outbound::Status(ProviderStatus::OwnershipAssumed {
                        request: successor.map(RequestId::get),
                        superseded: superseded.map(RequestId::get),
                    })),
                    Ok(Notice::Event(event))=>forward(event, &mut output, &mut output_sequence, &mut discards)?,
                    Ok(Notice::Unavailable {error})=>{
                        if typed {
                            urgent.push_back(Outbound::Status(ProviderStatus::Unavailable { reason: error.into() }));
                        } else {
                            eprintln!("lamp-live provider: connection lost ({error}); output already received is still delivered");
                        }
                    },
                    Ok(Notice::Settled {lineage,error})=>{
                        // Generation completed and every block was forwarded.
                        // The closed connection is this request's idle barrier.
                        eprintln!("lamp-live provider: request {} settled by {error} after generation completed", lineage.request.get());
                        output.push_back(Outbound::Output(ProviderOutput::TurnComplete {request:lineage.request.get(),idle:true}));
                    },
                    Ok(Notice::TurnLost {lineage,stage,error})=>{
                        let request = lineage.request.get();
                        if !typed {
                            return Err(io::Error::other(format!("{}; request {request} was lost at {stage:?} and is not retried", connection_ended(State::Disconnected(error)))).into());
                        }
                        // Later input for it is swallowed, not sent to a new session.
                        intake.handoff.fail(request);
                        // A newer refused Start must not suppress this older
                        // accepted request's ordered terminal outcome. Refused
                        // Starts are never tracked by the supervisor, so these
                        // two outcome sources are disjoint.
                        output.push_back(Outbound::Status(ProviderStatus::TurnFailed { request, stage: stage.into(), reason: error.into() }));
                    },
                    Err(failure) if coordinated => {
                        // Stop polling the terminal future. The process remains
                        // alive to drain retained output, refuse later input
                        // exactly once, and service privacy and explicit Stop.
                        output.push_back(Outbound::Output(ProviderOutput::Ended {
                            reason: failure.reason.into(), fault: failure.error.into(), attempts: failure.attempts,
                        }));
                        upstream_ended = true;
                    },
                    Err(failure)=>return Err(ended(failure).into()),
                }
                if output.len()>MAX_QUEUED_OUTPUT { return Err(io::Error::other("provider IPC output backlog exceeded bound").into()); }
            }
        }
    }
}

/// Control upkeep has no PCM queue or credit argument. The callback must be a
/// nonblocking transport send; WouldBlock preserves one original observation.
fn service_link<C: Connect>(
    provider: &mut Supervisor<C>,
    clock: Option<&mut LinkClock>,
    pending: &mut PendingLink,
    send: impl FnMut(ProviderLink) -> io::Result<()>,
) -> io::Result<()> {
    if let Some(clock) = clock {
        clock.tick()?;
        if let Some(update) = provider.poll_link() {
            pending.observe(clock.convert(update)?, monotonic_us())?;
        }
        pending.try_send(send)
    } else {
        provider.maintain();
        Ok(())
    }
}

/// Select one API for the entire worker lifetime. Both futures are cancellation
/// safe, so the control tick never waits for data, backoff or a hung connection.
async fn next_notice<C: Connect>(
    provider: &mut Supervisor<C>,
    coordinated: bool,
) -> std::result::Result<Notice, Failure> {
    if coordinated {
        provider.next_output().await.map(Notice::from)
    } else {
        provider.next().await
    }
}

fn forward(
    event: Event,
    output: &mut VecDeque<Outbound>,
    output_sequence: &mut u64,
    discards: &mut DiscardLog,
) -> Result<()> {
    let mut push = |item: ProviderOutput| output.push_back(Outbound::Output(item));
    match event {
        Event::InputStarted {
            lineage,
            waiting_for_barrier,
        } => push(ProviderOutput::Started {
            request: lineage.request.get(),
            session: lineage.session.get(),
            waiting_for_barrier,
        }),
        Event::Audio { lineage, pcm, .. } => {
            let provider_event_at_us = monotonic_us();
            let source_frames = pcm.len();
            for (index, part) in pcm.chunks(PACKET_SAMPLES).enumerate() {
                *output_sequence = output_sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("provider output counter exhausted"))?;
                push(ProviderOutput::Audio {
                    request: lineage.request.get(),
                    sequence: *output_sequence,
                    provider_event_at_us,
                    source_frames,
                    source_offset: index * PACKET_SAMPLES,
                    samples: part.to_vec(),
                });
            }
        }
        Event::OutputTranscript {
            lineage,
            text,
            finished,
        } => push_text(&mut push, Some(lineage.request.get()), text, finished),
        Event::UncorrelatedInputTranscript { text, finished, .. } => {
            push_text(&mut push, None, text, finished);
        }
        Event::ModelText { .. } | Event::VoiceActivity { .. } => {}
        Event::GenerationComplete { lineage } => push(ProviderOutput::GenerationComplete {
            request: lineage.request.get(),
        }),
        Event::Interrupted { lineage } => push(ProviderOutput::Interrupted {
            request: lineage.request.get(),
        }),
        Event::TurnComplete { lineage, idle } => push(ProviderOutput::TurnComplete {
            request: lineage.request.get(),
            idle,
        }),
        Event::GoAway { time_left, .. } => eprintln!(
            "lamp-live provider: service announced a close; time_left_ms={:?}",
            time_left.map(|left| left.as_millis())
        ),
        Event::Discarded { reason, .. } => discards.note(reason),
        Event::InputDiscarded {
            through,
            dropped_samples,
            sent_samples,
            ..
        } => {
            eprintln!(
                "lamp-live provider: input through request {} abandoned: dropped_unsent_samples={dropped_samples} already_sent_samples={sent_samples}",
                through.get()
            );
            push(ProviderOutput::InputDiscarded {
                request: through.get(),
                dropped_samples,
                sent_samples,
            });
        }
        Event::BarrierAssumed {
            superseded,
            successor,
            ..
        } => eprintln!(
            "lamp-live provider: no confirmation that request {:?} stopped; request {:?} proceeds on that assumption (unqualified ownership)",
            superseded.map(RequestId::get),
            successor.map(RequestId::get)
        ),
        // Only a session configured to report its barriers sends these:
        // `Continuation::KeptStart` configures one. Counts and times, no
        // audio and no text.
        Event::Barrier {
            outcome, report, ..
        } => eprintln!(
            "lamp-live provider: barrier {outcome:?}: superseded={:?} successor={:?} waited_ms={} quiet_ms={} successor_input_open={} end_withheld_ms={:?} withheld audio_frames={} text_frames={} generation_complete={} interrupted={} completions_not_idle={} start_held_ms={:?} start_released_by={:?} retained_samples={} waiting_for_input={} resumable_updates={} unresumable_updates={}",
            report.superseded.map(RequestId::get),
            report.successor.map(RequestId::get),
            report.waited.as_millis(),
            report.quiet.as_millis(),
            report.successor_input_open,
            report.end_withheld.map(|held| held.as_millis()),
            report.audio_frames,
            report.text_frames,
            report.generation_complete,
            report.interrupted,
            report.completions_not_idle,
            report.start_held.map(|held| held.as_millis()),
            report.start_released_by,
            report.retained_samples,
            report.waiting_for_input,
            report.resumable_updates,
            report.unresumable_updates
        ),
    }
    Ok(())
}

/// Late provider events never reach the coordinator. Each kind is reported
/// once per worker so a recurring pattern is visible without unbounded output.
#[derive(Default)]
struct DiscardLog {
    seen: [bool; 4],
}
impl DiscardLog {
    fn note(&mut self, reason: Discard) {
        let index = match reason {
            Discard::LateInterruption => 0,
            Discard::LateTerminal => 1,
            Discard::LateOutput => 2,
            Discard::UnownedOutput => 3,
        };
        if !std::mem::replace(&mut self.seen[index], true) {
            eprintln!(
                "lamp-live provider: dropped a late provider event ({reason:?}); it changed no request"
            );
        }
    }
}

/// Terminal provider failure as one line of fixed categories.
fn ended(failure: Failure) -> io::Error {
    match failure.reason {
        FailureReason::Shutdown => connection_ended(State::Closed),
        FailureReason::Lifecycle => connection_ended(State::Ready),
        reason => io::Error::other(format!(
            "{}; {reason:?} after {} recovery attempt(s)",
            connection_ended(State::Disconnected(failure.error)),
            failure.attempts
        )),
    }
}

/// Gemini state contains only fixed error categories and an optional numeric
/// close code. Never substitute a raw close reason, server body, URL or header.
fn connection_ended(state: State) -> io::Error {
    match state {
        State::Disconnected(error) => {
            io::Error::other(format!("provider connection ended: {error}"))
        }
        State::Closed => io::Error::other("provider connection closed locally"),
        State::Ready => io::Error::other(
            "provider event stream closed without a terminal state: client lifecycle failure",
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlDrain {
    Empty,
    Deferred,
    Stopped,
}

struct RetainedInput {
    command: ProviderInput,
    received_at_us: u64,
}

/// Local process intake; this never grants authority or contacts the provider.
/// Keep at most one original command when the shared control budget is exhausted.
struct ProviderIntake {
    guard: BoundaryGuard,
    handoff: InputHandoff,
    output_flow: OutputSender,
    last_control_us: u64,
    saw_allowed: bool,
    retained: Option<RetainedInput>,
    stopped: bool,
}
impl ProviderIntake {
    fn new(boot: BootId, now_us: u64) -> Self {
        Self {
            guard: BoundaryGuard::new(boot, MonoTime::from_micros(now_us)),
            handoff: InputHandoff::default(),
            output_flow: OutputSender::default(),
            last_control_us: now_us,
            saw_allowed: false,
            retained: None,
            stopped: false,
        }
    }

    fn stop(&mut self) {
        self.stopped = true;
        self.retained = None;
        self.guard.invalidate();
    }

    fn check_retained(&self, now_us: u64) -> Result<()> {
        if self.stopped {
            return Err(io::Error::other("provider intake stopped").into());
        }
        if self.retained.as_ref().is_some_and(|retained| {
            now_us
                .checked_sub(retained.received_at_us)
                .is_none_or(|age| age >= RETAINED_INPUT_US)
        }) {
            return Err(
                io::Error::other("provider retained input expired or clock regressed").into(),
            );
        }
        Ok(())
    }

    fn drain_control(
        &mut self,
        now: &mut impl FnMut() -> u64,
        receive: &mut impl FnMut() -> io::Result<Option<Control>>,
        remaining: &mut usize,
    ) -> Result<ControlDrain> {
        let result = self.drain_control_inner(now, receive, remaining);
        if result.is_err() {
            self.stop();
        }
        result
    }

    fn drain_control_inner(
        &mut self,
        now: &mut impl FnMut() -> u64,
        receive: &mut impl FnMut() -> io::Result<Option<Control>>,
        remaining: &mut usize,
    ) -> Result<ControlDrain> {
        self.check_retained(now())?;
        let mut empty = false;
        while *remaining > 0 {
            let Some(command) = receive()? else {
                empty = true;
                break;
            };
            *remaining -= 1;
            match command {
                Control::Stop => {
                    self.stop();
                    return Ok(ControlDrain::Stopped);
                }
                Control::Authority { snapshot } => {
                    let at = now();
                    let privacy_changed = self.guard.state().is_some_and(|old| {
                        old.microphone_generation() != snapshot.microphone_generation()
                    });
                    self.guard.install(MonoTime::from_micros(at), snapshot)?;
                    self.handoff.observe_authority(snapshot)?;
                    self.last_control_us = at;
                    if self.saw_allowed
                        && (privacy_changed
                            || snapshot.microphone_permission() != Permission::Allowed)
                    {
                        self.stop();
                        return Ok(ControlDrain::Stopped);
                    }
                    self.saw_allowed |= snapshot.microphone_permission() == Permission::Allowed;
                }
                Control::ProviderOutputCapacity { through } => {
                    self.output_flow.grant(through)?;
                }
                Control::StartCapture | Control::ConnectReference { .. } => {
                    return Err(io::Error::other("invalid provider command").into());
                }
            }
        }
        if now().saturating_sub(self.last_control_us) > 250_000 {
            return Err(io::Error::other("provider controller heartbeat expired").into());
        }
        Ok(if empty {
            ControlDrain::Empty
        } else {
            ControlDrain::Deferred
        })
    }

    fn receive_inputs(
        &mut self,
        now: &mut impl FnMut() -> u64,
        receive_control: &mut impl FnMut() -> io::Result<Option<Control>>,
        data: &mut Channel,
        input: &impl InputSink,
        remaining: &mut usize,
    ) -> Result<ControlDrain> {
        let result = self.receive_inputs_inner(now, receive_control, data, input, remaining);
        if result.is_err() {
            self.stop();
        }
        result
    }

    fn receive_inputs_inner(
        &mut self,
        now: &mut impl FnMut() -> u64,
        receive_control: &mut impl FnMut() -> io::Result<Option<Control>>,
        data: &mut Channel,
        input: &impl InputSink,
        remaining: &mut usize,
    ) -> Result<ControlDrain> {
        self.check_retained(now())?;
        self.handoff.synchronize(input)?;
        for _ in 0..INPUT_SLICE {
            if self.handoff.undelivered.is_some() {
                break;
            }
            if self.retained.is_none()
                && let Some(command) = data.receive::<ProviderInput>()?
            {
                self.retained = Some(RetainedInput {
                    command,
                    received_at_us: now(),
                });
            }
            // The parent sends Authority before its dependent input. Both can
            // arrive after the first empty control read, so reread control after
            // data receipt. Never submit while more control may still be queued.
            let control = self.drain_control(now, receive_control, remaining)?;
            if control == ControlDrain::Stopped {
                return Ok(control);
            }
            // Apply observed cancellation even when the bounded slice filled.
            // This preserves Gemini's existing old-response barrier and FIFO End.
            self.handoff.synchronize(input)?;
            if control == ControlDrain::Deferred {
                return Ok(control);
            }
            self.check_retained(now())?;
            let Some(retained) = self.retained.take() else {
                break;
            };
            // Empty control plus unknown owner is a protocol error, not a reason
            // to invent an authority grace period or relabel this command.
            self.handoff.submit(retained.command, now(), input)?;
        }
        Ok(ControlDrain::Empty)
    }
}

/// Only successful submissions change the input's open/closed state. Authority
/// and audio use separate sockets, so root ownership is not proof that a Start
/// or End has reached Gemini yet. This tracker retains no PCM or request history.
#[derive(Default)]
struct InputHandoff {
    authority: Option<Snapshot>,
    highest_seen: u64,
    retired_through: u64,
    retired_sent_through: u64,
    /// Through which request retirement came with no owner at all: the
    /// request was given up, where a newer owner means it was continued.
    /// Kept apart from retirement and passed on by itself: several
    /// authorities can be read in one turn, and an abandonment followed at
    /// once by a newer owner would otherwise be read as a continuation.
    abandoned_through: u64,
    abandoned_sent_through: u64,
    submitted: Option<SubmittedInput>,
    /// Requests the provider will not serve. Their remaining input is dropped
    /// here, exactly like retired input, until the coordinator retires them.
    failed_through: u64,
    /// Report an undeliverable request instead of ending the worker.
    tolerate_outage: bool,
    /// The sole unreported request whose selected connection refused its Start.
    /// Drain it before receiving another command; it is never overwritten.
    undelivered: Option<u64>,
}

#[derive(Clone, Copy)]
struct SubmittedInput {
    request: RequestId,
    session: SessionId,
    open: bool,
}

impl InputHandoff {
    fn visual_turn(&self) -> Option<VisualTurn> {
        let submitted = self.submitted?;
        (submitted.open && submitted.request.get() > self.retired_through.max(self.failed_through))
            .then_some(VisualTurn {
                request: submitted.request,
                session: submitted.session,
            })
    }

    /// Called for every validated control packet, not just the final snapshot in
    /// a tick. An owner followed by cancellation must retire its later queued
    /// Start even if that Start has never been submitted to the provider.
    fn observe_authority(&mut self, state: Snapshot) -> Result<()> {
        if let Some(owner) = state.owner() {
            let request = owner.turn();
            if request <= self.retired_through || request < self.highest_seen {
                return Err(
                    io::Error::other("provider authority resurrected a retired request").into(),
                );
            }
            self.highest_seen = request;
            self.retired_through = self.retired_through.max(request - 1);
        } else {
            self.retired_through = self.retired_through.max(self.highest_seen);
            self.abandoned_through = self.abandoned_through.max(self.highest_seen);
        }
        self.authority = Some(state);
        Ok(())
    }

    fn synchronize(&mut self, input: &impl InputSink) -> Result<()> {
        if self.abandoned_through > self.abandoned_sent_through {
            // No owner followed these: abandoned. A transport that kept any
            // of their input drops what it has not yet sent. First, so that
            // a later request that was continued keeps its words.
            input.discard(RequestId::new(self.abandoned_through)?);
            self.abandoned_sent_through = self.abandoned_through;
            // An abandonment is a retirement too.
            self.retired_sent_through = self.retired_sent_through.max(self.abandoned_through);
        }
        if self.retired_through > self.retired_sent_through {
            // This is synchronous local revocation, independent of queue space
            // or the network. It must happen even if closing the input fails.
            // A newer owner followed these: continued. Their words stay.
            input.retire(RequestId::new(self.retired_through)?);
            self.retired_sent_through = self.retired_through;
        }
        if let Some(submitted) = self.submitted
            && submitted.request.get() <= self.retired_through
        {
            if submitted.open {
                // Enqueued before any successor Start on the same FIFO. Merely
                // dropping a root End that is still on the IPC socket would
                // leave the Gemini actor's old input open and reject the Start.
                match input.end(submitted.request) {
                    // Its connection is gone; there is no open input to close.
                    Err(error)
                        if self.tolerate_outage
                            && (error == Error::NotConnected
                                || (error == Error::StaleRequest
                                    && input
                                        .belongs_to_replaced_connection(submitted.request))) => {}
                    other => other?,
                }
            }
            self.submitted = None;
        }
        Ok(())
    }

    /// The provider will not serve this request. Nothing further of it is
    /// submitted, and it is never treated as open again.
    fn fail(&mut self, request: u64) {
        self.failed_through = self.failed_through.max(request);
        if self
            .submitted
            .is_some_and(|submitted| submitted.request.get() <= request)
        {
            self.submitted = None;
        }
    }

    /// A missing connection or proven old-session input is survivable only in
    /// typed recovery. An arbitrary stale request remains a protocol fault.
    fn refused(&mut self, request: RequestId, error: Error, input: &impl InputSink) -> Result<()> {
        let was_submitted = self
            .submitted
            .is_some_and(|submitted| submitted.request == request);
        let replaced = was_submitted
            && error == Error::StaleRequest
            && input.belongs_to_replaced_connection(request);
        if self.tolerate_outage && (error == Error::NotConnected || replaced) {
            if !was_submitted && self.undelivered.is_some() {
                return Err(io::Error::other("unreported refused Start must be drained").into());
            }
            self.fail(request.get());
            // Audio/End refusal cannot undo an accepted Start. The supervisor
            // reports that request's actual stage after draining its prior PCM.
            if !was_submitted {
                self.undelivered = Some(request.get());
            }
            return Ok(());
        }
        Err(error.into())
    }

    /// These two checked refusals happen before enqueue or supersession. They
    /// cannot create a supervisor terminal receipt, so the worker owes one.
    /// Other errors keep their existing fatal semantics; in particular an
    /// arbitrary StaleRequest is not proof of a replaced connection.
    fn refused_start(&mut self, request: RequestId, refusal: StartRefusal) -> Result<()> {
        let error = match refusal {
            StartRefusal::NotConnected => Error::NotConnected,
            StartRefusal::SessionMismatch { .. } => Error::StaleRequest,
            StartRefusal::Refused(error) => return Err(error.into()),
        };
        if !self.tolerate_outage {
            return Err(error.into());
        }
        if self.undelivered.is_some() {
            return Err(io::Error::other("unreported refused Start must be drained").into());
        }
        self.fail(request.get());
        self.undelivered = Some(request.get());
        Ok(())
    }

    fn submit(
        &mut self,
        command: ProviderInput,
        now_us: u64,
        input: &impl InputSink,
    ) -> Result<()> {
        self.synchronize(input)?;
        let state = self
            .authority
            .ok_or_else(|| io::Error::other("provider has no authority"))?;
        let now = MonoTime::from_micros(now_us);
        if !state.listening_ready(now)
            || command.privacy_generation() != state.microphone_generation()
        {
            return Err(io::Error::other("provider input lost privacy authority").into());
        }
        let request = RequestId::new(command.request())?;
        if request.get() <= self.retired_through.max(self.failed_through) {
            // Old PCM retains its old identity and is discarded without reaching
            // the provider. Its sequence/timestamp cannot poison the new input.
            return Ok(());
        }
        if state.owner().map(|owner| owner.turn()) != Some(request.get()) {
            return Err(io::Error::other("provider input has no matching current owner").into());
        }
        match command {
            ProviderInput::Start { session, .. } => {
                if self.submitted.is_some() {
                    return Err(io::Error::other("provider input already started").into());
                }
                let expected = SessionId::new(session)?;
                if let Err(refusal) = input.start(expected, request) {
                    return self.refused_start(request, refusal);
                }
                self.submitted = Some(SubmittedInput {
                    request,
                    session: expected,
                    open: true,
                });
            }
            ProviderInput::Audio {
                sequence,
                read_completed_at_us,
                samples,
                ..
            } => {
                self.require_open(request)?;
                let age = now_us
                    .checked_sub(read_completed_at_us)
                    .map(Duration::from_micros)
                    .filter(|age| *age < lamp_gemini::MAX_INPUT_AGE)
                    .ok_or_else(|| io::Error::other("invalid or stale capture time"))?;
                let captured = Instant::now()
                    .checked_sub(age)
                    .ok_or_else(|| io::Error::other("invalid capture age"))?;
                if let Err(error) = input.audio(request, sequence, captured, &samples) {
                    return self.refused(request, error, input);
                }
            }
            ProviderInput::End { .. } => {
                self.require_open(request)?;
                if let Err(error) = input.end(request) {
                    return self.refused(request, error, input);
                }
                if let Some(submitted) = self.submitted.as_mut() {
                    submitted.open = false;
                }
            }
        }
        Ok(())
    }

    fn require_open(&self, request: RequestId) -> Result<()> {
        if !self
            .submitted
            .is_some_and(|submitted| submitted.request == request && submitted.open)
        {
            return Err(io::Error::other("provider input is not open for this request").into());
        }
        Ok(())
    }
}

/// The production sink is the supervised connection's bounded command queue.
/// The small seam lets regressions exercise cross-channel ordering without
/// credentials or a socket.
trait InputSink {
    fn start(
        &self,
        session: SessionId,
        request: RequestId,
    ) -> std::result::Result<(), StartRefusal>;
    fn audio(
        &self,
        request: RequestId,
        sequence: u64,
        captured: Instant,
        samples: &[i16],
    ) -> lamp_gemini::Result<()>;
    fn end(&self, request: RequestId) -> lamp_gemini::Result<()>;
    fn retire(&self, request: RequestId);
    /// Retirement of a request that was given up and not continued. A sink
    /// that keeps no input treats it as any retirement.
    fn discard(&self, request: RequestId) {
        self.retire(request);
    }
    /// Exact successful submission lineage belongs to an earlier connection.
    /// Absence of this proof never turns StaleRequest into a recoverable error.
    fn belongs_to_replaced_connection(&self, _request: RequestId) -> bool {
        false
    }
}

impl<C: Connect> InputSink for Supervisor<C> {
    fn start(
        &self,
        session: SessionId,
        request: RequestId,
    ) -> std::result::Result<(), StartRefusal> {
        self.try_start_for_session(session, request)
    }
    fn audio(
        &self,
        request: RequestId,
        sequence: u64,
        captured: Instant,
        samples: &[i16],
    ) -> lamp_gemini::Result<()> {
        self.try_audio(request, sequence, captured, samples)
    }
    fn end(&self, request: RequestId) -> lamp_gemini::Result<()> {
        self.try_end(request)
    }
    fn retire(&self, request: RequestId) {
        Supervisor::retire(self, request);
    }
    fn discard(&self, request: RequestId) {
        Supervisor::discard(self, request);
    }
    fn belongs_to_replaced_connection(&self, request: RequestId) -> bool {
        self.input_belongs_to_replaced_connection(request)
    }
}

fn push_text(
    push: &mut impl FnMut(ProviderOutput),
    request: Option<u64>,
    mut text: String,
    finished: bool,
) {
    while text.len() > 3500 {
        let mut split = 3500;
        while !text.is_char_boundary(split) {
            split -= 1;
        }
        let tail = text.split_off(split);
        push(ProviderOutput::Transcript {
            request,
            text,
            finished: false,
        });
        text = tail;
    }
    push(ProviderOutput::Transcript {
        request,
        text,
        finished,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_interaction::{AdmissionState, AdmittedInput, CaptureState, Controller};
    use std::cell::{Cell, RefCell};

    const NOW: u64 = 10_000_000;

    #[test]
    fn started_wire_retains_actual_source_session_and_requires_it_on_decode() {
        let mut output = VecDeque::new();
        forward(
            Event::InputStarted {
                lineage: lamp_gemini::Lineage {
                    session: lamp_gemini::SessionId::new(7).unwrap(),
                    request: RequestId::new(23).unwrap(),
                },
                waiting_for_barrier: true,
            },
            &mut output,
            &mut 0,
            &mut DiscardLog::default(),
        )
        .unwrap();
        let Some(Outbound::Output(started)) = output.pop_front() else {
            panic!("missing Started");
        };
        let value = serde_json::to_value(&started).unwrap();
        assert_eq!(value["session"], 7);
        assert!(matches!(
            serde_json::from_value::<ProviderOutput>(value).unwrap(),
            ProviderOutput::Started {
                request: 23,
                session: 7,
                waiting_for_barrier: true
            }
        ));
        assert!(
            serde_json::from_value::<ProviderOutput>(serde_json::json!({
                "kind":"started", "request":23, "waiting_for_barrier":true
            }))
            .is_err(),
            "a missing source session cannot borrow the current connection"
        );
    }

    #[test]
    fn output_capacity_cannot_renew_authority_or_controller_heartbeat() {
        let mut intake = ProviderIntake::new(BootId::new([9; 16]).unwrap(), NOW);
        let mut budget = CONTROL_SLICE;
        let mut command = Some(Control::ProviderOutputCapacity { through: 2 });
        let error = intake
            .drain_control(
                &mut || NOW + 250_001,
                &mut || Ok(command.take()),
                &mut budget,
            )
            .unwrap_err();
        assert!(error.to_string().contains("heartbeat expired"));
        assert_eq!(intake.last_control_us, NOW);
        assert!(intake.guard.state().is_none());
        assert!(intake.stopped);
    }

    #[test]
    fn retired_output_is_removed_before_new_output_without_changing_survivor_order() {
        let mut queue = VecDeque::new();
        for sequence in 0..MAX_QUEUED_OUTPUT - 4 {
            queue.push_back(Outbound::Output(ProviderOutput::Audio {
                request: 1,
                sequence: sequence as u64 + 1,
                provider_event_at_us: NOW,
                source_frames: PACKET_SAMPLES,
                source_offset: 0,
                samples: vec![1; PACKET_SAMPLES],
            }));
        }
        queue.push_back(Outbound::Output(ProviderOutput::TurnComplete {
            request: 1,
            idle: true,
        }));
        // A failure report is never dropped, even for a request since retired.
        queue.push_back(Outbound::Status(ProviderStatus::TurnFailed {
            request: 1,
            stage: FailedStage::Responding,
            reason: Fault::Transport,
        }));
        queue.push_back(Outbound::Output(ProviderOutput::Started {
            request: 2,
            session: 1,
            waiting_for_barrier: true,
        }));
        queue.push_back(Outbound::Output(ProviderOutput::Transcript {
            request: None,
            text: "unattributed".into(),
            finished: true,
        }));
        prune_retired_output(&mut queue, 1);
        assert_eq!(queue.len(), 3);
        assert!(matches!(
            queue.pop_front(),
            Some(Outbound::Status(ProviderStatus::TurnFailed {
                request: 1,
                ..
            }))
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Outbound::Output(ProviderOutput::Started { request: 2, .. }))
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Outbound::Output(ProviderOutput::Transcript {
                request: None,
                ..
            }))
        ));
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Start(u64),
        Audio(u64, u64, Vec<i16>),
        End(u64),
        EndBlocked(u64),
        Retire(u64),
    }

    /// Models FIFO submission and synchronous retirement, not cloud timing.
    /// Gemini's own transport tests cover the old-response barrier separately.
    #[derive(Default)]
    struct RecordingSink {
        calls: RefCell<Vec<Call>>,
        open: Cell<Option<u64>>,
        retired: Cell<u64>,
        reject_end: Cell<bool>,
        reject_start: Cell<bool>,
        disconnected: Cell<bool>,
        /// Zero means the initial session (1); tests can install a replacement.
        session: Cell<u64>,
    }
    impl InputSink for RecordingSink {
        fn start(
            &self,
            session: SessionId,
            request: RequestId,
        ) -> std::result::Result<(), StartRefusal> {
            if self.disconnected.get() {
                return Err(StartRefusal::NotConnected);
            }
            let live = SessionId::new(self.session.get().max(1)).unwrap();
            if live != session {
                return Err(StartRefusal::SessionMismatch { live });
            }
            if self.reject_start.get() {
                return Err(StartRefusal::Refused(lamp_gemini::Error::Backpressure));
            }
            if self.open.get().is_some() || request.get() <= self.retired.get() {
                return Err(StartRefusal::Refused(lamp_gemini::Error::OverlappingInput));
            }
            self.open.set(Some(request.get()));
            self.calls.borrow_mut().push(Call::Start(request.get()));
            Ok(())
        }
        fn audio(
            &self,
            request: RequestId,
            sequence: u64,
            _captured: Instant,
            samples: &[i16],
        ) -> lamp_gemini::Result<()> {
            if self.disconnected.get() {
                return Err(lamp_gemini::Error::NotConnected);
            }
            if self.open.get() != Some(request.get()) || request.get() <= self.retired.get() {
                return Err(lamp_gemini::Error::StaleRequest);
            }
            self.calls
                .borrow_mut()
                .push(Call::Audio(request.get(), sequence, samples.to_vec()));
            Ok(())
        }
        fn end(&self, request: RequestId) -> lamp_gemini::Result<()> {
            if self.disconnected.get() {
                return Err(lamp_gemini::Error::NotConnected);
            }
            if self.reject_end.get() {
                self.calls
                    .borrow_mut()
                    .push(Call::EndBlocked(request.get()));
                return Err(lamp_gemini::Error::Backpressure);
            }
            if self.open.get() != Some(request.get()) {
                return Err(lamp_gemini::Error::StaleRequest);
            }
            self.calls.borrow_mut().push(Call::End(request.get()));
            self.open.set(None);
            Ok(())
        }
        fn retire(&self, request: RequestId) {
            self.retired.set(self.retired.get().max(request.get()));
            self.calls.borrow_mut().push(Call::Retire(request.get()));
        }
    }

    fn controller() -> Controller {
        let mut owner = Controller::new(BootId::new([9; 16]).unwrap(), time());
        owner
            .set_microphone_permission(time(), Permission::Allowed)
            .unwrap();
        owner
            .set_capture(
                time(),
                CaptureState::RetainingUntil(MonoTime::from_micros(NOW + 1_000_000)),
            )
            .unwrap();
        owner
            .set_admission(
                time(),
                AdmissionState::OpenUntil(MonoTime::from_micros(NOW + 1_000_000)),
            )
            .unwrap();
        owner
    }
    fn time() -> MonoTime {
        MonoTime::from_micros(NOW)
    }
    fn authorize(owner: &mut Controller, handoff: &mut InputHandoff) -> u64 {
        let turn = owner.admit(time(), AdmittedInput::NewTurn).unwrap();
        handoff
            .observe_authority(owner.snapshot(time()).unwrap())
            .unwrap();
        turn.turn()
    }
    fn generation(handoff: &InputHandoff) -> u64 {
        handoff.authority.unwrap().microphone_generation()
    }
    fn start(request: u64, handoff: &InputHandoff) -> ProviderInput {
        ProviderInput::Start {
            session: 1,
            request,
            privacy_generation: generation(handoff),
        }
    }

    #[test]
    fn start_wire_requires_an_explicit_selected_session() {
        let command = serde_json::json!({
            "kind":"start", "request":1, "session":7, "privacy_generation":2
        });
        assert!(matches!(
            serde_json::from_value::<ProviderInput>(command.clone()).unwrap(),
            ProviderInput::Start {
                request: 1,
                session: 7,
                privacy_generation: 2
            }
        ));
        for field in ["request", "session", "privacy_generation"] {
            let mut missing = command.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ProviderInput>(missing).is_err());
        }
        let mut unknown = command;
        unknown["current_session"] = serde_json::json!(8);
        assert!(serde_json::from_value::<ProviderInput>(unknown).is_err());
    }

    #[test]
    fn zero_session_fails_without_a_submission_or_not_delivered_receipt() {
        let mut owner = controller();
        let mut handoff = InputHandoff {
            tolerate_outage: true,
            ..InputHandoff::default()
        };
        let request = authorize(&mut owner, &mut handoff);
        let sink = RecordingSink::default();
        assert!(
            handoff
                .submit(
                    ProviderInput::Start {
                        request,
                        session: 0,
                        privacy_generation: generation(&handoff),
                    },
                    NOW,
                    &sink
                )
                .is_err()
        );
        assert!(sink.calls.borrow().is_empty());
        assert!(handoff.submitted.is_none());
        assert!(handoff.undelivered.is_none());
        assert_eq!(handoff.failed_through, 0);
    }

    #[test]
    fn checked_session_refusal_is_typed_only_and_never_spends_the_live_input() {
        for tolerate_outage in [false, true] {
            let mut owner = controller();
            let mut handoff = InputHandoff {
                tolerate_outage,
                ..InputHandoff::default()
            };
            let request = authorize(&mut owner, &mut handoff);
            let sink = RecordingSink::default();
            sink.session.set(2);
            // The checked comparison must precede every mutating input call.
            let result = handoff.submit(start(request, &handoff), NOW, &sink);
            assert_eq!(result.is_ok(), tolerate_outage);
            assert_eq!(handoff.undelivered, tolerate_outage.then_some(request));
            assert_eq!(
                handoff.failed_through,
                if tolerate_outage { request } else { 0 }
            );
            assert!(handoff.submitted.is_none());
            assert!(sink.calls.borrow().is_empty());
            assert_eq!(sink.retired.get(), 0);
            assert!(sink.open.get().is_none());
        }
    }
    fn audio(request: u64, sequence: u64, sample: i16, handoff: &InputHandoff) -> ProviderInput {
        ProviderInput::Audio {
            request,
            privacy_generation: generation(handoff),
            sequence,
            read_completed_at_us: NOW - 200_000,
            samples: vec![sample; 160],
        }
    }
    fn end(request: u64, handoff: &InputHandoff) -> ProviderInput {
        ProviderInput::End {
            request,
            privacy_generation: generation(handoff),
        }
    }

    fn intake_channels() -> (
        crate::process::SessionDirectory,
        WorkerChannels,
        WorkerChannels,
    ) {
        let directory = crate::process::SessionDirectory::create().unwrap();
        let parent_boot = BootId::new([9; 16]).unwrap();
        let worker_boot = BootId::new([8; 16]).unwrap();
        let mut parent =
            WorkerChannels::bind(&directory.path, "provider", parent_boot, worker_boot, false)
                .unwrap();
        let mut worker =
            WorkerChannels::bind(&directory.path, "provider", parent_boot, worker_boot, true)
                .unwrap();
        parent.connect(&directory.path, "provider", false).unwrap();
        worker.connect(&directory.path, "provider", true).unwrap();
        (directory, parent, worker)
    }

    #[test]
    fn separate_socket_authority_in_empty_drain_gap_precedes_new_start() {
        let (_directory, mut parent, mut worker) = intake_channels();
        let mut owner = controller();
        let mut intake = ProviderIntake::new(BootId::new([9; 16]).unwrap(), NOW);
        let sink = RecordingSink::default();
        let initial = owner.snapshot(time()).unwrap();
        parent
            .control
            .send(Control::Authority { snapshot: initial })
            .unwrap();
        let mut initial_budget = CONTROL_SLICE;
        intake
            .drain_control(
                &mut || NOW,
                &mut || worker.control.receive(),
                &mut initial_budget,
            )
            .unwrap();
        let turn = owner.admit(time(), AdmittedInput::NewTurn).unwrap();
        let state = owner.snapshot(time()).unwrap();
        let mut injected = false;
        let mut control_budget = CONTROL_SLICE;
        assert_eq!(
            intake
                .drain_control(
                    &mut || NOW,
                    &mut || {
                        let control = worker.control.receive()?;
                        if !injected {
                            assert!(control.is_none());
                            injected = true;
                            // Force the legal interleaving: parent sends control before data,
                            // but both sends occur after the worker observed an empty control socket.
                            parent
                                .control
                                .send(Control::Authority { snapshot: state })?;
                            parent.data.send(ProviderInput::Start {
                                session: 1,
                                request: turn.turn(),
                                privacy_generation: state.microphone_generation(),
                            })?;
                        }
                        Ok(control)
                    },
                    &mut control_budget
                )
                .unwrap(),
            ControlDrain::Empty
        );
        assert_eq!(
            intake
                .receive_inputs(
                    &mut || NOW,
                    &mut || worker.control.receive(),
                    &mut worker.data,
                    &sink,
                    &mut control_budget
                )
                .unwrap(),
            ControlDrain::Empty
        );
        assert!(injected);
        assert_eq!(*sink.calls.borrow(), vec![Call::Start(turn.turn())]);
    }

    struct SocketRig {
        _directory: crate::process::SessionDirectory,
        parent: WorkerChannels,
        worker: WorkerChannels,
        owner: Controller,
        intake: ProviderIntake,
        sink: RecordingSink,
    }
    impl SocketRig {
        fn new() -> Self {
            let (directory, parent, worker) = intake_channels();
            let mut rig = Self {
                _directory: directory,
                parent,
                worker,
                owner: controller(),
                intake: ProviderIntake::new(BootId::new([9; 16]).unwrap(), NOW),
                sink: RecordingSink::default(),
            };
            rig.owner.admit(time(), AdmittedInput::NewTurn).unwrap();
            let state = rig.owner.snapshot(time()).unwrap();
            rig.parent
                .control
                .send(Control::Authority { snapshot: state })
                .unwrap();
            rig.parent
                .data
                .send(ProviderInput::Start {
                    session: 1,
                    request: 1,
                    privacy_generation: state.microphone_generation(),
                })
                .unwrap();
            assert_eq!(rig.tick(NOW).unwrap(), ControlDrain::Empty);
            assert_eq!(*rig.sink.calls.borrow(), vec![Call::Start(1)]);
            rig
        }
        fn tick(&mut self, at: u64) -> Result<ControlDrain> {
            let mut budget = CONTROL_SLICE;
            let control = self.intake.drain_control(
                &mut || at,
                &mut || self.worker.control.receive(),
                &mut budget,
            )?;
            if control == ControlDrain::Stopped {
                return Ok(control);
            }
            self.intake.handoff.synchronize(&self.sink)?;
            if control == ControlDrain::Deferred {
                return Ok(control);
            }
            self.intake.receive_inputs(
                &mut || at,
                &mut || self.worker.control.receive(),
                &mut self.worker.data,
                &self.sink,
                &mut budget,
            )
        }
    }

    #[test]
    fn separate_socket_interruption_retires_old_data_and_keeps_new_prefix() {
        let mut rig = SocketRig::new();
        let new = rig
            .owner
            .admit(time(), AdmittedInput::Interruption)
            .unwrap();
        let state = rig.owner.snapshot(time()).unwrap();
        // Already queued old audio is intentionally malformed; once retired it
        // must not poison the next question's sequence, samples or capture age.
        rig.parent
            .data
            .send(ProviderInput::Audio {
                request: 1,
                privacy_generation: state.microphone_generation(),
                sequence: u64::MAX,
                read_completed_at_us: u64::MAX,
                samples: vec![],
            })
            .unwrap();
        let mut injected = false;
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            rig.intake
                .drain_control(
                    &mut || NOW,
                    &mut || {
                        let control = rig.worker.control.receive()?;
                        if !injected {
                            assert!(control.is_none());
                            injected = true;
                            rig.parent
                                .control
                                .send(Control::Authority { snapshot: state })?;
                            for command in [
                                ProviderInput::Start {
                                    session: 1,
                                    request: new.turn(),
                                    privacy_generation: state.microphone_generation(),
                                },
                                ProviderInput::Audio {
                                    request: new.turn(),
                                    privacy_generation: state.microphone_generation(),
                                    sequence: 1,
                                    read_completed_at_us: NOW - 200_000,
                                    samples: vec![21; 160],
                                },
                                ProviderInput::End {
                                    request: new.turn(),
                                    privacy_generation: state.microphone_generation(),
                                },
                            ] {
                                rig.parent.data.send(command)?;
                            }
                        }
                        Ok(control)
                    },
                    &mut budget
                )
                .unwrap(),
            ControlDrain::Empty
        );
        assert_eq!(
            rig.intake
                .receive_inputs(
                    &mut || NOW,
                    &mut || rig.worker.control.receive(),
                    &mut rig.worker.data,
                    &rig.sink,
                    &mut budget
                )
                .unwrap(),
            ControlDrain::Empty
        );
        assert_eq!(
            *rig.sink.calls.borrow(),
            vec![
                Call::Start(1),
                Call::Retire(1),
                Call::End(1),
                Call::Start(2),
                Call::Audio(2, 1, vec![21; 160]),
                Call::End(2)
            ]
        );
    }

    #[test]
    fn separate_socket_stop_privacy_and_coalesced_reopen_precede_input() {
        for mode in 0..3 {
            let mut rig = SocketRig::new();
            let audio = audio(1, 1, 41, &rig.intake.handoff);
            let priority = if mode == 0 {
                Control::Stop
            } else {
                rig.owner
                    .set_microphone_permission(time(), Permission::Denied)
                    .unwrap();
                if mode == 2 {
                    rig.owner
                        .set_microphone_permission(time(), Permission::Allowed)
                        .unwrap();
                }
                Control::Authority {
                    snapshot: rig.owner.snapshot(time()).unwrap(),
                }
            };
            let mut pending = Some((priority, audio));
            let mut budget = CONTROL_SLICE;
            assert_eq!(
                rig.intake
                    .drain_control(
                        &mut || NOW,
                        &mut || {
                            let control = rig.worker.control.receive()?;
                            if let Some((priority, command)) = pending.take() {
                                assert!(control.is_none());
                                rig.parent.control.send(priority)?;
                                rig.parent.data.send(command)?;
                            }
                            Ok(control)
                        },
                        &mut budget
                    )
                    .unwrap(),
                ControlDrain::Empty
            );
            assert_eq!(
                rig.intake
                    .receive_inputs(
                        &mut || NOW,
                        &mut || rig.worker.control.receive(),
                        &mut rig.worker.data,
                        &rig.sink,
                        &mut budget
                    )
                    .unwrap(),
                ControlDrain::Stopped
            );
            assert_eq!(*rig.sink.calls.borrow(), vec![Call::Start(1)]);
            assert!(rig.intake.retained.is_none());
            assert!(rig.tick(NOW).is_err());
        }
    }

    #[test]
    fn full_control_budget_defers_one_original_input_and_expires_at_original_receipt() {
        for expire in [false, true] {
            let mut rig = SocketRig::new();
            rig.owner
                .admit(time(), AdmittedInput::Interruption)
                .unwrap();
            let mut states: VecDeque<_> = (0..17)
                .map(|_| rig.owner.snapshot(time()).unwrap())
                .collect();
            let privacy = states[0].microphone_generation();
            let mut budget = CONTROL_SLICE;
            assert_eq!(
                rig.intake
                    .drain_control(
                        &mut || NOW,
                        &mut || rig.worker.control.receive(),
                        &mut budget
                    )
                    .unwrap(),
                ControlDrain::Empty
            );
            // Queue new input and authority after the initial drain. Further
            // authority packets are fed one per receive, independent of the OS
            // Unix datagram queue limit.
            rig.parent
                .data
                .send(ProviderInput::Start {
                    session: 1,
                    request: 2,
                    privacy_generation: privacy,
                })
                .unwrap();
            rig.parent
                .data
                .send(ProviderInput::Audio {
                    request: 2,
                    privacy_generation: privacy,
                    sequence: 1,
                    read_completed_at_us: NOW - 200_000,
                    samples: vec![51; 160],
                })
                .unwrap();
            rig.parent
                .control
                .send(Control::Authority {
                    snapshot: states.pop_front().unwrap(),
                })
                .unwrap();
            assert_eq!(
                rig.intake
                    .receive_inputs(
                        &mut || NOW,
                        &mut || {
                            let control = rig.worker.control.receive()?;
                            if control.is_some()
                                && let Some(snapshot) = states.pop_front()
                            {
                                rig.parent.control.send(Control::Authority { snapshot })?;
                            }
                            Ok(control)
                        },
                        &mut rig.worker.data,
                        &rig.sink,
                        &mut budget
                    )
                    .unwrap(),
                ControlDrain::Deferred
            );
            assert_eq!(budget, 0);
            assert!(states.is_empty()); // Last authority remains on the control socket.
            let retained = rig.intake.retained.as_ref().unwrap();
            assert_eq!(retained.received_at_us, NOW);
            assert_eq!(retained.command.request(), 2);
            assert_eq!(
                *rig.sink.calls.borrow(),
                vec![Call::Start(1), Call::Retire(1), Call::End(1)]
            );
            if expire {
                let error = rig.tick(NOW + RETAINED_INPUT_US).unwrap_err();
                assert!(error.to_string().contains("retained input expired"));
                assert!(rig.intake.retained.is_none());
                assert_eq!(
                    *rig.sink.calls.borrow(),
                    vec![Call::Start(1), Call::Retire(1), Call::End(1)]
                );
            } else {
                assert_eq!(rig.tick(NOW + 1).unwrap(), ControlDrain::Empty);
                assert!(rig.intake.retained.is_none());
                assert_eq!(
                    *rig.sink.calls.borrow(),
                    vec![
                        Call::Start(1),
                        Call::Retire(1),
                        Call::End(1),
                        Call::Start(2),
                        Call::Audio(2, 1, vec![51; 160])
                    ]
                );
            }
        }
    }

    #[test]
    fn intake_keeps_eight_command_slice_and_does_not_wait_for_unknown_authority() {
        let mut rig = SocketRig::new();
        for sequence in 1..=9 {
            rig.parent
                .data
                .send(audio(1, sequence, sequence as i16, &rig.intake.handoff))
                .unwrap();
        }
        rig.tick(NOW).unwrap();
        assert_eq!(
            rig.sink
                .calls
                .borrow()
                .iter()
                .filter(|call| matches!(call, Call::Audio(..)))
                .count(),
            INPUT_SLICE
        );
        rig.tick(NOW).unwrap();
        assert_eq!(rig.sink.calls.borrow().len(), 10);
        rig.parent
            .data
            .send(ProviderInput::Start {
                session: 1,
                request: 2,
                privacy_generation: generation(&rig.intake.handoff),
            })
            .unwrap();
        let error = rig.tick(NOW).unwrap_err();
        assert!(error.to_string().contains("no matching current owner"));
        assert!(rig.intake.retained.is_none());
        assert!(rig.intake.stopped);
    }

    #[test]
    fn selected_session_survives_mixed_control_deferral_and_refused_tail_drain() {
        let (_directory, mut parent, mut worker) = intake_channels();
        let mut owner = controller();
        let mut intake = ProviderIntake::new(BootId::new([9; 16]).unwrap(), NOW);
        intake.handoff.tolerate_outage = true;
        let sink = RecordingSink::default();
        sink.session.set(2);
        let request = authorize(&mut owner, &mut intake.handoff);
        let privacy_generation = generation(&intake.handoff);
        let mut controls: VecDeque<_> = (0..17)
            .map(|i| {
                if i % 2 == 0 {
                    Control::Authority {
                        snapshot: owner.snapshot(time()).unwrap(),
                    }
                } else {
                    Control::ProviderOutputCapacity { through: 2 }
                }
            })
            .collect();
        parent.control.send(controls.pop_front().unwrap()).unwrap();
        for command in [
            start(request, &intake.handoff),
            audio(request, 1, 61, &intake.handoff),
            end(request, &intake.handoff),
        ] {
            parent.data.send(command).unwrap();
        }
        // Receive the original data, then interleave authority/capacity packets
        // on the separate urgent socket until the shared control slice is full.
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake
                .receive_inputs(
                    &mut || NOW,
                    &mut || {
                        let packet = worker.control.receive()?;
                        if packet.is_some()
                            && let Some(next) = controls.pop_front()
                        {
                            parent.control.send(next)?;
                        }
                        Ok(packet)
                    },
                    &mut worker.data,
                    &sink,
                    &mut budget
                )
                .unwrap(),
            ControlDrain::Deferred
        );
        assert_eq!(budget, 0);
        assert!(controls.is_empty());
        let retained = intake.retained.as_ref().unwrap();
        assert_eq!(retained.received_at_us, NOW);
        assert!(matches!(
            retained.command,
            ProviderInput::Start { session: 1, .. }
        ));
        assert!(sink.calls.borrow().is_empty());

        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake
                .receive_inputs(
                    &mut || NOW + 1,
                    &mut || worker.control.receive(),
                    &mut worker.data,
                    &sink,
                    &mut budget
                )
                .unwrap(),
            ControlDrain::Empty
        );
        assert_eq!(intake.handoff.undelivered, Some(request));
        let mut output = VecDeque::new();
        queue_undelivered(&mut intake.handoff, &mut output);
        let mut budget = CONTROL_SLICE;
        intake
            .receive_inputs(
                &mut || NOW + 2,
                &mut || worker.control.receive(),
                &mut worker.data,
                &sink,
                &mut budget,
            )
            .unwrap();
        queue_undelivered(&mut intake.handoff, &mut output);
        assert_eq!(output.len(), 1);
        assert!(
            matches!(output[0], Outbound::Status(ProviderStatus::TurnFailed {
            request: got, stage: FailedStage::NotDelivered, ..
        }) if got == request)
        );
        assert!(
            sink.calls.borrow().is_empty(),
            "A's prefix/tail must not reach B"
        );
        assert!(intake.handoff.submitted.is_none());

        let next = owner.admit(time(), AdmittedInput::NewTurn).unwrap();
        parent
            .control
            .send(Control::Authority {
                snapshot: owner.snapshot(time()).unwrap(),
            })
            .unwrap();
        for command in [
            ProviderInput::Start {
                request: next.turn(),
                session: 2,
                privacy_generation,
            },
            ProviderInput::Audio {
                request: next.turn(),
                privacy_generation,
                sequence: 1,
                read_completed_at_us: NOW - 200_000,
                samples: vec![71; 160],
            },
            ProviderInput::End {
                request: next.turn(),
                privacy_generation,
            },
        ] {
            parent.data.send(command).unwrap();
        }
        let mut budget = CONTROL_SLICE;
        intake
            .receive_inputs(
                &mut || NOW + 3,
                &mut || worker.control.receive(),
                &mut worker.data,
                &sink,
                &mut budget,
            )
            .unwrap();
        assert_eq!(
            *sink.calls.borrow(),
            vec![
                Call::Retire(request),
                Call::Start(next.turn()),
                Call::Audio(next.turn(), 1, vec![71; 160]),
                Call::End(next.turn())
            ]
        );
        queue_undelivered(&mut intake.handoff, &mut output);
        assert_eq!(
            output.len(),
            1,
            "refused input has exactly one terminal report"
        );
    }

    #[test]
    fn intake_does_not_extend_snapshot_capture_or_controller_leases() {
        for capture_expiry in [false, true] {
            let mut rig = SocketRig::new();
            if capture_expiry {
                rig.owner
                    .set_capture(
                        time(),
                        CaptureState::RetainingUntil(MonoTime::from_micros(NOW + 10)),
                    )
                    .unwrap();
                rig.parent
                    .control
                    .send(Control::Authority {
                        snapshot: rig.owner.snapshot(time()).unwrap(),
                    })
                    .unwrap();
                rig.tick(NOW).unwrap();
            }
            let expires = rig
                .intake
                .handoff
                .authority
                .unwrap()
                .expires_at()
                .as_micros();
            rig.parent
                .data
                .send(audio(1, 1, 71, &rig.intake.handoff))
                .unwrap();
            assert!(rig.tick(expires).is_err());
            assert_eq!(*rig.sink.calls.borrow(), vec![Call::Start(1)]);
        }
        let mut rig = SocketRig::new();
        let error = rig.tick(NOW + 250_001).unwrap_err();
        assert!(error.to_string().contains("controller heartbeat expired"));
        assert!(rig.intake.stopped);
    }

    #[test]
    fn priority_transfer_closes_old_input_before_successor_and_discards_queued_old_data() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let old = authorize(&mut owner, &mut handoff);
        handoff.submit(start(old, &handoff), NOW, &sink).unwrap();
        handoff
            .submit(audio(old, 1, 11, &handoff), NOW, &sink)
            .unwrap();

        let new = authorize(&mut owner, &mut handoff);
        handoff.synchronize(&sink).unwrap();
        // These messages were already in the other IPC socket before transfer.
        for command in [
            audio(old, 2, 99, &handoff),
            end(old, &handoff),
            start(old, &handoff),
            end(old, &handoff),
        ] {
            handoff.submit(command, NOW, &sink).unwrap();
        }
        handoff.submit(start(new, &handoff), NOW, &sink).unwrap();
        handoff
            .submit(audio(new, 1, 21, &handoff), NOW, &sink)
            .unwrap();
        handoff
            .submit(audio(new, 2, 22, &handoff), NOW, &sink)
            .unwrap();
        handoff.submit(end(new, &handoff), NOW, &sink).unwrap();
        assert_eq!(
            *sink.calls.borrow(),
            vec![
                Call::Start(old),
                Call::Audio(old, 1, vec![11; 160]),
                Call::Retire(old),
                Call::End(old),
                Call::Start(new),
                Call::Audio(new, 1, vec![21; 160]),
                Call::Audio(new, 2, vec![22; 160]),
                Call::End(new),
            ]
        );
    }

    #[test]
    fn an_already_submitted_end_is_not_repeated_on_retirement() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let old = authorize(&mut owner, &mut handoff);
        handoff.submit(start(old, &handoff), NOW, &sink).unwrap();
        handoff.submit(end(old, &handoff), NOW, &sink).unwrap();
        let new = authorize(&mut owner, &mut handoff);
        handoff.synchronize(&sink).unwrap();
        handoff.synchronize(&sink).unwrap();
        handoff.submit(end(old, &handoff), NOW, &sink).unwrap();
        handoff.submit(start(new, &handoff), NOW, &sink).unwrap();
        assert_eq!(
            *sink.calls.borrow(),
            vec![
                Call::Start(old),
                Call::End(old),
                Call::Retire(old),
                Call::Start(new)
            ]
        );
    }

    #[test]
    fn overtaken_start_never_creates_a_phantom_old_input_or_end() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let old = authorize(&mut owner, &mut handoff);
        let new = authorize(&mut owner, &mut handoff);
        for command in [
            start(old, &handoff),
            audio(old, 1, 11, &handoff),
            end(old, &handoff),
        ] {
            handoff.submit(command, NOW, &sink).unwrap();
        }
        handoff.submit(start(new, &handoff), NOW, &sink).unwrap();
        assert_eq!(
            *sink.calls.borrow(),
            vec![Call::Retire(old), Call::Start(new)]
        );
    }

    #[test]
    fn coalesced_owner_then_cancellation_retires_a_start_not_yet_received() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let old = authorize(&mut owner, &mut handoff);
        let snapshot = owner.snapshot(time()).unwrap();
        owner
            .cancel_turn(time(), snapshot.owner().unwrap())
            .unwrap();
        handoff
            .observe_authority(owner.snapshot(time()).unwrap())
            .unwrap();
        handoff.submit(start(old, &handoff), NOW, &sink).unwrap();
        handoff.submit(end(old, &handoff), NOW, &sink).unwrap();
        assert_eq!(*sink.calls.borrow(), vec![Call::Retire(old)]);
        assert!(handoff.observe_authority(snapshot).is_err());
    }

    #[test]
    fn a_future_or_unknown_request_cannot_borrow_current_authority() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let current = authorize(&mut owner, &mut handoff);
        for command in [
            start(0, &handoff),
            start(current + 1, &handoff),
            audio(current + 1, 1, 90, &handoff),
            end(current + 1, &handoff),
        ] {
            assert!(handoff.submit(command, NOW, &sink).is_err());
        }
        assert!(sink.calls.borrow().is_empty());
        handoff
            .submit(start(current, &handoff), NOW, &sink)
            .unwrap();
    }

    #[test]
    fn input_requires_a_successful_start_and_cannot_reopen_after_end() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let current = authorize(&mut owner, &mut handoff);
        assert!(
            handoff
                .submit(audio(current, 1, 10, &handoff), NOW, &sink)
                .is_err()
        );
        assert!(handoff.submit(end(current, &handoff), NOW, &sink).is_err());
        handoff
            .submit(start(current, &handoff), NOW, &sink)
            .unwrap();
        assert!(
            handoff
                .submit(start(current, &handoff), NOW, &sink)
                .is_err()
        );
        handoff.submit(end(current, &handoff), NOW, &sink).unwrap();
        assert!(handoff.submit(end(current, &handoff), NOW, &sink).is_err());
        assert!(
            handoff
                .submit(audio(current, 2, 20, &handoff), NOW, &sink)
                .is_err()
        );
        assert_eq!(
            *sink.calls.borrow(),
            vec![Call::Start(current), Call::End(current)]
        );
    }

    #[test]
    fn close_backpressure_revokes_output_but_never_starts_a_successor() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let old = authorize(&mut owner, &mut handoff);
        handoff.submit(start(old, &handoff), NOW, &sink).unwrap();
        let new = authorize(&mut owner, &mut handoff);
        sink.reject_end.set(true);
        assert!(handoff.submit(start(new, &handoff), NOW, &sink).is_err());
        assert_eq!(
            *sink.calls.borrow(),
            vec![Call::Start(old), Call::Retire(old), Call::EndBlocked(old)]
        );
        assert_eq!(handoff.submitted.unwrap().request.get(), old);
        assert!(handoff.submitted.unwrap().open);
    }

    #[test]
    fn rejected_start_is_not_later_closed_as_if_it_had_been_submitted() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let old = authorize(&mut owner, &mut handoff);
        sink.reject_start.set(true);
        assert!(handoff.submit(start(old, &handoff), NOW, &sink).is_err());
        sink.reject_start.set(false);
        let new = authorize(&mut owner, &mut handoff);
        handoff.submit(start(new, &handoff), NOW, &sink).unwrap();
        assert_eq!(
            *sink.calls.borrow(),
            vec![Call::Retire(old), Call::Start(new)]
        );
    }

    #[test]
    fn outage_after_accepted_start_never_reports_that_request_as_not_delivered() {
        for refuse_end in [false, true] {
            let mut owner = controller();
            let mut handoff = InputHandoff {
                tolerate_outage: true,
                ..InputHandoff::default()
            };
            let sink = RecordingSink::default();
            let request = authorize(&mut owner, &mut handoff);
            handoff
                .submit(start(request, &handoff), NOW, &sink)
                .unwrap();
            handoff
                .submit(audio(request, 1, 7, &handoff), NOW, &sink)
                .unwrap();
            sink.disconnected.set(true);
            let refused = if refuse_end {
                end(request, &handoff)
            } else {
                audio(request, 2, 8, &handoff)
            };
            handoff.submit(refused, NOW, &sink).unwrap();
            assert_eq!(
                handoff.undelivered, None,
                "the supervisor owns the accepted request's actual terminal stage"
            );
            assert_eq!(handoff.failed_through, request);
            assert!(handoff.submitted.is_none());
            // The remaining input is discarded even after connectivity returns.
            sink.disconnected.set(false);
            handoff
                .submit(audio(request, 3, 9, &handoff), NOW, &sink)
                .unwrap();
            handoff.submit(end(request, &handoff), NOW, &sink).unwrap();
            assert_eq!(
                *sink.calls.borrow(),
                vec![Call::Start(request), Call::Audio(request, 1, vec![7; 160])]
            );
        }
    }

    #[test]
    fn outage_before_start_reports_not_delivered_once_and_never_replays_input() {
        let mut owner = controller();
        let mut handoff = InputHandoff {
            tolerate_outage: true,
            ..InputHandoff::default()
        };
        let sink = RecordingSink::default();
        sink.disconnected.set(true);
        let request = authorize(&mut owner, &mut handoff);
        handoff
            .submit(start(request, &handoff), NOW, &sink)
            .unwrap();
        assert_eq!(handoff.undelivered.take(), Some(request));
        sink.disconnected.set(false);
        for command in [
            start(request, &handoff),
            audio(request, 1, 9, &handoff),
            end(request, &handoff),
        ] {
            handoff.submit(command, NOW, &sink).unwrap();
        }
        assert_eq!(handoff.undelivered, None);
        assert!(sink.calls.borrow().is_empty());
    }

    #[test]
    fn typed_recovery_does_not_swallow_an_unproven_stale_request() {
        let mut owner = controller();
        let mut handoff = InputHandoff {
            tolerate_outage: true,
            ..InputHandoff::default()
        };
        let sink = RecordingSink::default();
        let request = authorize(&mut owner, &mut handoff);
        handoff
            .submit(start(request, &handoff), NOW, &sink)
            .unwrap();
        assert!(
            handoff
                .refused(RequestId::new(request).unwrap(), Error::StaleRequest, &sink)
                .is_err()
        );
        assert_eq!(handoff.failed_through, 0);
        assert_eq!(handoff.undelivered, None);
        assert_eq!(handoff.submitted.unwrap().request.get(), request);
    }

    #[test]
    fn pending_not_delivered_report_cannot_be_overwritten_by_a_new_refused_start() {
        let mut owner = controller();
        let mut handoff = InputHandoff {
            tolerate_outage: true,
            ..InputHandoff::default()
        };
        let sink = RecordingSink::default();
        sink.disconnected.set(true);
        let old = authorize(&mut owner, &mut handoff);
        handoff.submit(start(old, &handoff), NOW, &sink).unwrap();
        let next = authorize(&mut owner, &mut handoff);
        assert!(handoff.submit(start(next, &handoff), NOW, &sink).is_err());
        assert_eq!(handoff.undelivered, Some(old));
        assert_eq!(handoff.failed_through, old);
        let mut output = VecDeque::new();
        queue_undelivered(&mut handoff, &mut output);
        handoff.submit(start(next, &handoff), NOW, &sink).unwrap();
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(output.len(), 2);
        for request in [old, next] {
            assert!(
                matches!(output.pop_front(), Some(Outbound::Status(ProviderStatus::TurnFailed {
                request: reported, stage: FailedStage::NotDelivered, ..
            })) if reported == request)
            );
        }
    }

    #[test]
    fn refused_start_outcome_waits_for_capacity_without_growing_output() {
        let mut handoff = InputHandoff {
            undelivered: Some(7),
            ..InputHandoff::default()
        };
        let mut output = VecDeque::new();
        for sequence in 1..=MAX_QUEUED_OUTPUT {
            output.push_back(Outbound::Output(ProviderOutput::Audio {
                request: 6,
                sequence: sequence as u64,
                provider_event_at_us: NOW,
                source_frames: PACKET_SAMPLES,
                source_offset: 0,
                samples: vec![2; PACKET_SAMPLES],
            }));
        }
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(handoff.undelivered, Some(7));
        assert_eq!(output.len(), MAX_QUEUED_OUTPUT);
        output.pop_front();
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(handoff.undelivered, None);
        assert_eq!(output.len(), MAX_QUEUED_OUTPUT);
        assert!(matches!(
            output.back(),
            Some(Outbound::Status(ProviderStatus::TurnFailed {
                request: 7,
                stage: FailedStage::NotDelivered,
                ..
            }))
        ));
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(output.len(), MAX_QUEUED_OUTPUT);
    }

    #[test]
    fn refused_start_pauses_intake_before_more_pcm_but_does_not_block_stop() {
        let (_directory, mut parent, mut worker) = intake_channels();
        let mut owner = controller();
        let mut intake = ProviderIntake::new(BootId::new([9; 16]).unwrap(), NOW);
        intake.handoff.tolerate_outage = true;
        let request = authorize(&mut owner, &mut intake.handoff);
        let sink = RecordingSink::default();
        sink.disconnected.set(true);
        for command in [
            start(request, &intake.handoff),
            audio(request, 1, 8, &intake.handoff),
            end(request, &intake.handoff),
        ] {
            parent.data.send(command).unwrap();
        }
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake
                .receive_inputs(
                    &mut || NOW,
                    &mut || worker.control.receive(),
                    &mut worker.data,
                    &sink,
                    &mut budget
                )
                .unwrap(),
            ControlDrain::Empty
        );
        assert_eq!(intake.handoff.undelivered, Some(request));
        assert!(
            matches!(worker.data.receive::<ProviderInput>().unwrap(), Some(ProviderInput::Audio { request: received, .. }) if received == request)
        );
        parent.control.send(Control::Stop).unwrap();
        assert_eq!(
            intake
                .drain_control(&mut || NOW, &mut || worker.control.receive(), &mut budget)
                .unwrap(),
            ControlDrain::Stopped
        );
        assert!(intake.stopped);
        assert!(sink.calls.borrow().is_empty());
    }

    #[test]
    fn privacy_generation_and_expired_authority_still_prevent_audio() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let current = authorize(&mut owner, &mut handoff);
        handoff
            .submit(start(current, &handoff), NOW, &sink)
            .unwrap();
        let mut wrong_generation = audio(current, 1, 10, &handoff);
        if let ProviderInput::Audio {
            privacy_generation, ..
        } = &mut wrong_generation
        {
            *privacy_generation += 1;
        }
        assert!(handoff.submit(wrong_generation, NOW, &sink).is_err());
        let expires = handoff.authority.unwrap().expires_at().as_micros();
        assert!(
            handoff
                .submit(audio(current, 1, 10, &handoff), expires, &sink)
                .is_err()
        );
        owner
            .set_microphone_permission(time(), Permission::Denied)
            .unwrap();
        handoff
            .observe_authority(owner.snapshot(time()).unwrap())
            .unwrap();
        assert!(
            handoff
                .submit(audio(current, 1, 10, &handoff), NOW, &sink)
                .is_err()
        );
        assert!(
            sink.calls
                .borrow()
                .iter()
                .all(|call| !matches!(call, Call::Audio(..)))
        );
    }

    #[test]
    fn retained_prefix_keeps_capture_age_and_rejects_stale_or_future_samples() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let current = authorize(&mut owner, &mut handoff);
        handoff
            .submit(start(current, &handoff), NOW, &sink)
            .unwrap();
        handoff
            .submit(audio(current, 1, 10, &handoff), NOW, &sink)
            .unwrap();
        for captured_at in [NOW - 1_000_000, NOW + 1] {
            let mut invalid = audio(current, 2, 90, &handoff);
            if let ProviderInput::Audio {
                read_completed_at_us,
                ..
            } = &mut invalid
            {
                *read_completed_at_us = captured_at;
            }
            assert!(handoff.submit(invalid, NOW, &sink).is_err());
        }
        assert_eq!(
            *sink.calls.borrow(),
            vec![Call::Start(current), Call::Audio(current, 1, vec![10; 160])]
        );
    }

    #[test]
    fn retired_capture_metadata_does_not_poison_successor_sequence_or_prefix() {
        let mut owner = controller();
        let mut handoff = InputHandoff::default();
        let sink = RecordingSink::default();
        let old = authorize(&mut owner, &mut handoff);
        let new = authorize(&mut owner, &mut handoff);
        let mut stale = audio(old, u64::MAX, 99, &handoff);
        if let ProviderInput::Audio {
            read_completed_at_us,
            samples,
            ..
        } = &mut stale
        {
            *read_completed_at_us = u64::MAX;
            samples.clear();
        }
        handoff.submit(stale, NOW, &sink).unwrap();
        handoff.submit(start(new, &handoff), NOW, &sink).unwrap();
        handoff
            .submit(audio(new, 1, 20, &handoff), NOW, &sink)
            .unwrap();
        assert_eq!(
            *sink.calls.borrow(),
            vec![
                Call::Retire(old),
                Call::Start(new),
                Call::Audio(new, 1, vec![20; 160])
            ]
        );
    }

    #[test]
    fn event_eof_preserves_sanitized_peer_close_code() {
        let error = connection_ended(State::Disconnected(lamp_gemini::Error::PeerClosed {
            code: Some(4029),
        }));
        assert_eq!(
            error.to_string(),
            "provider connection ended: Gemini Live PeerClosed { code: Some(4029) }"
        );
        assert_eq!(
            connection_ended(State::Disconnected(lamp_gemini::Error::ServerRejected)).to_string(),
            "provider connection ended: Gemini Live ServerRejected"
        );
    }

    #[test]
    fn event_eof_distinguishes_client_lifecycle_failure_from_local_shutdown() {
        assert!(
            connection_ended(State::Ready)
                .to_string()
                .contains("client lifecycle failure")
        );
        assert_eq!(
            connection_ended(State::Closed).to_string(),
            "provider connection closed locally"
        );
    }

    // ---- Supervised provider connection over real local IPC -------------
    //
    // The service is scripted in memory; no credential, TLS or cloud request.
    // Authority uses the real monotonic clock because `serve` does.
    use lamp_gemini::{
        Credential, GOOGLE_ENDPOINT, SessionConfig,
        testing::{Dial, audio as provider_audio, connector},
    };
    use serde_json::json;
    use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};

    /// Everything the worker can put on the data channel once the proposed
    /// statuses are accepted: the shape the coordinator would decode.
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum Wire {
        Status(ProviderStatus),
        Output(ProviderOutput),
    }

    struct LiveRig {
        _directory: crate::process::SessionDirectory,
        parent: WorkerChannels,
        output_flow: crate::provider_flow::OutputReceiver,
        owner: Controller,
        snapshot: Snapshot,
        dials: UnboundedReceiver<Dial>,
        worker: JoinHandle<Result<()>>,
    }
    fn real_time() -> MonoTime {
        MonoTime::from_micros(monotonic_us())
    }
    impl LiveRig {
        fn start(policy: RecoveryPolicy) -> Self {
            Self::start_with(
                policy,
                Options {
                    reporting: Reporting::Legacy,
                },
            )
        }
        fn start_with(policy: RecoveryPolicy, options: Options) -> Self {
            Self::start_with_barrier(policy, options, BARRIER)
        }
        fn start_with_barrier(
            policy: RecoveryPolicy,
            options: Options,
            barrier: BarrierPolicy,
        ) -> Self {
            Self::start_all(policy, options, barrier, false, Continuation::Off)
        }
        fn start_camera() -> Self {
            Self::start_all(
                quick_policy(),
                Options {
                    reporting: Reporting::Coordinated,
                },
                BARRIER,
                true,
                Continuation::Off,
            )
        }
        fn start_all(
            policy: RecoveryPolicy,
            options: Options,
            barrier: BarrierPolicy,
            camera: bool,
            continuation: Continuation,
        ) -> Self {
            let (directory, parent, worker) = intake_channels();
            let boot = BootId::new([9; 16]).unwrap();
            let mut owner = Controller::new(boot, real_time());
            owner
                .set_microphone_permission(real_time(), Permission::Allowed)
                .unwrap();
            // The configuration the worker itself builds, with its own choice
            // of continuation applied by the same function.
            let config = continuation
                .apply(
                    SessionConfig::new(GOOGLE_ENDPOINT, Credential::api_key("test-only").unwrap())
                        .unwrap()
                        .resumption(RESUMPTION)
                        .barrier_policy(barrier),
                )
                .unwrap();
            let (dialer, dials) = connector(config);
            let provider = Supervisor::new(dialer, policy).unwrap();
            let visual = if camera {
                owner
                    .set_camera_permission(real_time(), Permission::Allowed)
                    .unwrap();
                Some(
                    ContextDelivery::new(
                        camera_test_source(&directory.path),
                        boot,
                        BootId::new([8; 16]).unwrap(),
                    )
                    .unwrap(),
                )
            } else {
                None
            };
            let snapshot = owner.snapshot(real_time()).unwrap();
            let mut rig = Self {
                _directory: directory,
                parent,
                output_flow: crate::provider_flow::OutputReceiver::default(),
                owner,
                snapshot,
                dials,
                worker: tokio::spawn(serve_with_visual(worker, boot, provider, options, visual)),
            };
            rig.publish();
            rig
        }
        /// One coordinator heartbeat: fresh input leases and authority.
        fn publish(&mut self) {
            self.heartbeat().unwrap();
        }
        fn heartbeat(&mut self) -> io::Result<()> {
            let at = monotonic_us();
            let now = MonoTime::from_micros(at);
            let lease = MonoTime::from_micros(at + 100_000);
            self.owner
                .set_capture(now, CaptureState::RetainingUntil(lease))
                .unwrap();
            self.owner
                .set_admission(now, AdmissionState::OpenUntil(lease))
                .unwrap();
            self.snapshot = self.owner.snapshot(now).unwrap();
            self.parent.control.send(Control::Authority {
                snapshot: self.snapshot,
            })
        }
        async fn pump(&mut self, duration: Duration) {
            let until = Instant::now() + duration;
            while Instant::now() < until {
                self.publish();
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
        async fn output(&mut self) -> ProviderOutput {
            match self.wire().await {
                Wire::Output(output) => output,
                Wire::Status(status) => panic!("unexpected provider status: {status:?}"),
            }
        }
        /// One packet, if one has arrived, counted against output capacity
        /// and its capacity returned as an empty reply buffer would.
        fn take(&mut self) -> Option<Wire> {
            let wire = self.parent.data.receive::<Wire>().unwrap()?;
            self.output_flow.received().unwrap();
            self.output_flow
                .replenish(0, &mut self.parent.control)
                .unwrap();
            Some(wire)
        }
        async fn wire(&mut self) -> Wire {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.publish();
                if let Some(wire) = self.take() {
                    return wire;
                }
                assert!(!self.worker.is_finished(), "worker ended before output");
                assert!(Instant::now() < deadline, "no provider output in time");
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
        /// Heartbeat until the worker attempts a connection.
        async fn dial(&mut self) -> Dial {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                self.publish();
                if let Ok(dial) = self.dials.try_recv() {
                    return dial;
                }
                assert!(Instant::now() < deadline, "no connection attempt in time");
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
        /// Heartbeat until the worker ends; its result and the wait.
        async fn exit(&mut self) -> (Result<()>, Duration) {
            let started = Instant::now();
            while !self.worker.is_finished() {
                assert!(started.elapsed() < Duration::from_secs(3), "worker hung");
                // The worker may close its socket at any point from here on.
                let _ = self.heartbeat();
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            ((&mut self.worker).await.unwrap(), started.elapsed())
        }
        /// Heartbeat until the scripted service has the worker's next message.
        async fn service_message(&mut self, dial: &mut Dial) -> serde_json::Value {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                self.publish();
                if let Some(message) = dial.service.try_receive(Duration::from_millis(5)).await {
                    return message;
                }
                assert!(Instant::now() < deadline, "service saw no client message");
            }
        }
        async fn ready(&mut self) -> Dial {
            let mut dial = self.dial().await;
            let setup = self.service_message(&mut dial).await;
            assert_eq!(
                setup["setup"].get("sessionResumption").is_some(),
                dial.resumed
            );
            dial.service.send(json!({"setupComplete": {}})).await;
            assert!(matches!(self.output().await, ProviderOutput::Ready { .. }));
            dial
        }
        async fn link(&mut self) -> crate::provider_link::ProviderLink {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                self.publish();
                match self.parent.control.receive::<WorkerEvent>().unwrap() {
                    Some(WorkerEvent::Ready) | None => {}
                    Some(WorkerEvent::ProviderLink { link }) => {
                        link.validate(monotonic_us()).unwrap();
                        return link;
                    }
                    Some(other) => panic!("unexpected provider control event: {other:?}"),
                }
                assert!(
                    !self.worker.is_finished(),
                    "worker ended before link observation"
                );
                assert!(Instant::now() < deadline, "no link observation in time");
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
        async fn coordinated_ready(&mut self) -> Dial {
            let mut dial = self.dial().await;
            assert!(self.service_message(&mut dial).await.get("setup").is_some());
            dial.service.send(json!({"setupComplete": {}})).await;
            let link = self.link().await;
            assert!(matches!(
                link.state,
                crate::provider_link::ProviderLinkState::Ready {
                    context: crate::provider_link::LinkContext::Initial,
                    ..
                }
            ));
            assert!(
                self.take().is_none(),
                "availability must not consume PCM credit"
            );
            dial
        }
        async fn stop(&mut self) {
            self.parent.control.send(Control::Stop).unwrap();
            self.exit().await.0.unwrap();
        }
        /// Admit a request and submit its start, one block and optional end.
        async fn speak(&mut self, dial: &mut Dial, interruption: bool, end: bool) -> u64 {
            let input = if interruption {
                AdmittedInput::Interruption
            } else {
                AdmittedInput::NewTurn
            };
            let owner = self.owner.admit(real_time(), input).unwrap();
            let request = owner.turn();
            self.publish();
            let privacy_generation = self.snapshot.microphone_generation();
            self.parent
                .data
                .send(ProviderInput::Start {
                    session: dial.session.get(),
                    request,
                    privacy_generation,
                })
                .unwrap();
            self.parent
                .data
                .send(ProviderInput::Audio {
                    request,
                    privacy_generation,
                    sequence: 1,
                    read_completed_at_us: monotonic_us(),
                    samples: vec![3; 160],
                })
                .unwrap();
            assert!(matches!(
                self.output().await,
                ProviderOutput::Started { request: started, .. } if started == request
            ));
            if end {
                self.owner.input_ended(real_time(), owner).unwrap();
                self.publish();
                self.parent
                    .data
                    .send(ProviderInput::End {
                        request,
                        privacy_generation,
                    })
                    .unwrap();
            }
            // The service sees exactly this request's boundaries and audio.
            let mut expected = vec!["activityStart", "audio"];
            if end {
                expected.push("activityEnd");
            }
            for field in expected {
                let message = self.service_message(dial).await;
                assert!(
                    message["realtimeInput"].get(field).is_some(),
                    "expected {field}, service received {message}"
                );
            }
            request
        }
    }
    fn camera_test_source(directory: &Path) -> VisualSource {
        VisualSource {
            socket: directory.join(crate::camera_worker::FRAME_SOCKET),
            camera: BootId::new([7; 16]).unwrap(),
            source: "desk".into(),
            width: 320,
            height: 240,
            max_frame_bytes: 128 * 1024,
        }
    }

    #[tokio::test]
    async fn visual_camera_missing_or_closed_before_first_frame_preserves_complete_audio() {
        for close_permission in [false, true] {
            let mut rig = LiveRig::start_camera();
            let mut dial = rig.coordinated_ready().await;
            let request = rig.speak(&mut dial, false, false).await;
            if close_permission {
                rig.owner
                    .set_camera_permission(real_time(), Permission::Denied)
                    .unwrap();
            }
            // Open input continues normally while an absent camera times out.
            let until = Instant::now() + Duration::from_millis(280);
            let mut receipt = None;
            while Instant::now() < until {
                rig.publish();
                while let Some(event) = rig.parent.control.receive::<WorkerEvent>().unwrap() {
                    match event {
                        WorkerEvent::VisualContext { receipt: got } => receipt = Some(got),
                        other => panic!("unexpected control: {other:?}"),
                    }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            let receipt = receipt.expect("camera failure must be visible");
            assert_eq!(
                (receipt.request, receipt.session),
                (request, dial.session.get())
            );
            assert_eq!(
                receipt.outcome,
                if close_permission {
                    crate::visual_context::Outcome::PrivacyClosed
                } else {
                    crate::visual_context::Outcome::Expired
                }
            );
            rig.parent
                .data
                .send(ProviderInput::End {
                    request,
                    privacy_generation: rig.snapshot.microphone_generation(),
                })
                .unwrap();
            assert!(
                rig.service_message(&mut dial).await["realtimeInput"]
                    .get("activityEnd")
                    .is_some()
            );
            dial.service.send(provider_audio(14, 480)).await;
            dial.service
                .send(json!({"serverContent":{"generationComplete":true}}))
                .await;
            dial.service.send(idle_complete()).await;
            assert!(
                matches!(rig.output().await,ProviderOutput::Audio { request: got, samples, .. }
                if got==request && samples==vec![14;480])
            );
            assert!(
                matches!(rig.output().await,ProviderOutput::GenerationComplete { request: got } if got==request)
            );
            assert!(
                matches!(rig.output().await,ProviderOutput::TurnComplete { request: got, idle:true } if got==request)
            );
            assert!(rig.take().is_none());
            rig.stop().await;
        }
    }

    #[tokio::test]
    async fn visual_real_camera_link_reaches_same_input_without_blocking_its_audio_or_end() {
        use lamp_camera::frames::{FrameMeta, FrameServer, Role};
        let mut rig = LiveRig::start_camera();
        let source = camera_test_source(&rig._directory.path);
        let mut camera = FrameServer::bind(
            &source.socket,
            BootId::new([9; 16]).unwrap(),
            &[(Role::Context, BootId::new([8; 16]).unwrap())],
            source.max_frame_bytes,
        )
        .unwrap();
        let mut dial = rig.coordinated_ready().await;
        let request = rig.speak(&mut dial, false, false).await;
        let deadline = Instant::now() + Duration::from_secs(1);
        while !camera.wants_frame() {
            rig.publish();
            camera.service(monotonic_us());
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let at = monotonic_us();
        let meta = FrameMeta {
            request: 1,
            camera_worker: source.camera,
            capture_epoch: 1,
            frame_sequence: 11,
            grant: rig.owner.camera_grant(real_time()).unwrap(),
            source: source.source,
            format: *b"MJPG",
            width: 320,
            height: 240,
            driver_sequence: 1,
            driver_timestamp: None,
            dequeue_started_at_us: at - 100,
            dequeue_completed_at_us: at - 50,
            copied_at_us: at,
            derived: None,
        };
        let jpeg = [255, 216, 255, 224, 6, 7, 255, 217];
        assert_eq!(camera.offer_meta(&meta, &jpeg, monotonic_us()).unwrap(), 1);
        // This is a real private stream feeding the production provider loop.
        let mut receipt = None;
        while receipt.is_none() {
            rig.publish();
            camera.service(monotonic_us());
            if let Some(event) = rig.parent.control.receive::<WorkerEvent>().unwrap() {
                let WorkerEvent::VisualContext { receipt: got } = event else {
                    panic!("unexpected {event:?}")
                };
                receipt = Some(got);
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let receipt = receipt.unwrap();
        assert_eq!(receipt.outcome, crate::visual_context::Outcome::Queued);
        assert_eq!(
            (receipt.request, receipt.session, receipt.frame_sequence),
            (request, dial.session.get(), Some(11))
        );
        let image = rig.service_message(&mut dial).await;
        assert_eq!(image["realtimeInput"]["video"]["mimeType"], "image/jpeg");
        rig.parent
            .data
            .send(ProviderInput::Audio {
                request,
                privacy_generation: rig.snapshot.microphone_generation(),
                sequence: 2,
                read_completed_at_us: monotonic_us(),
                samples: vec![19; 160],
            })
            .unwrap();
        rig.parent
            .data
            .send(ProviderInput::End {
                request,
                privacy_generation: rig.snapshot.microphone_generation(),
            })
            .unwrap();
        assert!(
            rig.service_message(&mut dial).await["realtimeInput"]
                .get("audio")
                .is_some()
        );
        assert!(
            rig.service_message(&mut dial).await["realtimeInput"]
                .get("activityEnd")
                .is_some()
        );
        assert!(
            dial.service
                .try_receive(Duration::from_millis(10))
                .await
                .is_none(),
            "no repeated image"
        );
        rig.stop().await;
    }

    fn quick_policy() -> RecoveryPolicy {
        RecoveryPolicy {
            first_backoff: Duration::from_millis(20),
            max_backoff: Duration::from_millis(40),
            ..RecoveryPolicy::default()
        }
    }
    fn resumable() -> serde_json::Value {
        json!({"sessionResumptionUpdate":{"newHandle":"HANDLE-ONE","resumable":true}})
    }
    fn idle_complete() -> serde_json::Value {
        json!({"serverContent":{"turnComplete":true,"interactionStatus":"IDLE"}})
    }

    async fn memory_link<C: Connect>(
        provider: &mut Supervisor<C>,
        clock: &mut LinkClock,
        pending: &mut PendingLink,
    ) -> ProviderLink {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let mut received = None;
                service_link(provider, Some(&mut *clock), pending, |link| {
                    received = Some(link);
                    Ok(())
                })
                .unwrap();
                if let Some(link) = received {
                    return link;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("in-memory provider link")
    }

    fn memory_provider(
        policy: RecoveryPolicy,
    ) -> (
        Supervisor<lamp_gemini::testing::FakeConnector>,
        UnboundedReceiver<Dial>,
        LinkClock,
        PendingLink,
    ) {
        let config = SessionConfig::new(GOOGLE_ENDPOINT, Credential::api_key("test-only").unwrap())
            .unwrap()
            .resumption(RESUMPTION)
            .barrier_policy(BARRIER);
        let (connector, dials) = connector(config);
        (
            Supervisor::new(connector, policy).unwrap(),
            dials,
            LinkClock::sample().unwrap(),
            PendingLink::default(),
        )
    }

    /// Audit regression: root selected A before the worker received this Start.
    /// Advance the real in-memory supervisor to B before consuming that command.
    /// The selected session travels unchanged through the handoff. Refusal
    /// produces one local outcome; neither its opening PCM nor tail reaches B.
    #[tokio::test]
    async fn audit_delayed_start_selected_on_a_must_not_reach_recovered_b() {
        use crate::provider_link::ProviderLinkState;
        use lamp_gemini::testing::next_dial;
        let (mut provider, mut dials, mut clock, mut pending) = memory_provider(quick_policy());
        let mut first = next_dial(&mut dials).await;
        first.service.accept().await;
        let selected = memory_link(&mut provider, &mut clock, &mut pending).await;
        assert!(matches!(selected.state, ProviderLinkState::Ready { .. }));
        let mut owner = controller();
        let mut handoff = InputHandoff {
            tolerate_outage: true,
            ..InputHandoff::default()
        };
        let request = authorize(&mut owner, &mut handoff);
        let queued_before_outage = ProviderInput::Start {
            request,
            session: selected.session,
            privacy_generation: generation(&handoff),
        };

        first.service.send(resumable()).await;
        first
            .service
            .close(1012, "controlled audit transition")
            .await;
        assert!(matches!(
            memory_link(&mut provider, &mut clock, &mut pending)
                .await
                .state,
            ProviderLinkState::Unavailable { .. }
        ));
        let mut second = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                service_link(&mut provider, Some(&mut clock), &mut pending, |_| {
                    panic!("replacement cannot be ready before setup")
                })
                .unwrap();
                if let Ok(dial) = dials.try_recv() {
                    break dial;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        second.service.accept().await;
        let replacement = memory_link(&mut provider, &mut clock, &mut pending).await;
        assert!(matches!(replacement.state, ProviderLinkState::Ready { .. }));
        assert!(replacement.session > selected.session);

        handoff
            .submit(queued_before_outage, NOW, &provider)
            .unwrap();
        assert_eq!(handoff.undelivered, Some(request));
        assert!(handoff.submitted.is_none());
        let mut output = VecDeque::new();
        queue_undelivered(&mut handoff, &mut output);
        queue_undelivered(&mut handoff, &mut output);
        for command in [
            start(request, &handoff),
            audio(request, 1, 7, &handoff),
            end(request, &handoff),
        ] {
            handoff.submit(command, NOW, &provider).unwrap();
        }
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(output.len(), 1);
        assert!(matches!(output.pop_front(), Some(Outbound::Status(
            ProviderStatus::TurnFailed { request: got, stage: FailedStage::NotDelivered,
                reason: Fault::Local }
        )) if got == request));
        assert!(
            second
                .service
                .try_receive(Duration::from_millis(10))
                .await
                .is_none()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(10), next_notice(&mut provider, true))
                .await
                .is_err(),
            "refused Start must not create a supervisor outcome or Started"
        );

        // A fresh, explicitly B-selected request works; the old refusal cannot
        // borrow B, poison B's input queue, or replay the previous prefix.
        let fresh = authorize(&mut owner, &mut handoff);
        handoff
            .submit(
                ProviderInput::Start {
                    request: fresh,
                    session: replacement.session,
                    privacy_generation: generation(&handoff),
                },
                NOW,
                &provider,
            )
            .unwrap();
        handoff
            .submit(audio(fresh, 1, 23, &handoff), NOW, &provider)
            .unwrap();
        handoff
            .submit(end(fresh, &handoff), NOW, &provider)
            .unwrap();
        assert!(matches!(next_notice(&mut provider, true).await.unwrap(),
            Notice::Event(Event::InputStarted { lineage, .. })
                if lineage.request.get() == fresh && lineage.session.get() == replacement.session));
        assert!(
            second.service.receive().await["realtimeInput"]
                .get("activityStart")
                .is_some()
        );
        assert!(
            second.service.receive().await["realtimeInput"]
                .get("audio")
                .is_some()
        );
        assert!(
            second.service.receive().await["realtimeInput"]
                .get("activityEnd")
                .is_some()
        );
        assert!(
            second
                .service
                .try_receive(Duration::from_millis(10))
                .await
                .is_none()
        );
        // At the actual worker sink boundary, a late A-selected Start cannot
        // retire B's already accepted response. Authority-driven cancellation
        // is separate and is not invoked by this refusal.
        assert_eq!(
            InputSink::start(
                &provider,
                SessionId::new(selected.session).unwrap(),
                RequestId::new(fresh + 1).unwrap()
            ),
            Err(StartRefusal::SessionMismatch {
                live: second.session
            })
        );
        second.service.send(provider_audio(29, 960)).await;
        assert!(matches!(next_notice(&mut provider, true).await.unwrap(),
            Notice::Event(Event::Audio { lineage, pcm, .. })
                if lineage.request.get() == fresh && lineage.session == second.session
                    && pcm == vec![29; 960]));
        assert!(
            second
                .service
                .try_receive(Duration::from_millis(10))
                .await
                .is_none()
        );
        provider.shutdown();
    }

    /// A successful local enqueue is not proof that activityStart was written.
    #[tokio::test]
    async fn audit_accepted_start_can_fail_before_started_acknowledgement() {
        use crate::provider_link::ProviderLinkState;
        use lamp_gemini::testing::next_dial;
        for ended in [false, true] {
            let (mut provider, mut dials, mut clock, mut pending) =
                memory_provider(RecoveryPolicy {
                    max_attempts: 0,
                    ..RecoveryPolicy::default()
                });
            let mut first = next_dial(&mut dials).await;
            first.service.accept().await;
            assert!(matches!(
                memory_link(&mut provider, &mut clock, &mut pending)
                    .await
                    .state,
                ProviderLinkState::Ready { .. }
            ));
            let request = RequestId::new(1).unwrap();
            provider.try_start(request).unwrap();
            if ended {
                provider.try_end(request).unwrap();
            }
            // No await between enqueue and peer drop: the current-thread actor
            // has not consumed Start, so it cannot have emitted InputStarted.
            drop(first);
            assert!(matches!(
                memory_link(&mut provider, &mut clock, &mut pending)
                    .await
                    .state,
                ProviderLinkState::Unavailable { .. }
            ));
            assert!(matches!(
                memory_link(&mut provider, &mut clock, &mut pending)
                    .await
                    .state,
                ProviderLinkState::Terminal { .. }
            ));
            let expected = if ended {
                Stage::AwaitingResponse
            } else {
                Stage::InputOpen
            };
            let actual = next_notice(&mut provider, true).await.unwrap();
            eprintln!("audit: accepted_end={ended} first_ordered_notice={actual:?}");
            assert!(matches!(actual, Notice::TurnLost { stage, .. } if stage == expected));
            provider.shutdown();
        }
    }

    #[tokio::test]
    async fn coordinated_memory_control_reconnects_while_prior_answer_is_unconsumed() {
        use crate::provider_link::{LinkContext, ProviderLinkState};
        use lamp_gemini::testing::next_dial;
        let (mut provider, mut dials, mut clock, mut pending) = memory_provider(quick_policy());
        let mut first = next_dial(&mut dials).await;
        first.service.accept().await;
        assert!(matches!(
            memory_link(&mut provider, &mut clock, &mut pending)
                .await
                .state,
            ProviderLinkState::Ready { .. }
        ));
        let request = RequestId::new(1).unwrap();
        provider.try_start(request).unwrap();
        assert!(matches!(
            next_notice(&mut provider, true).await.unwrap(),
            Notice::Event(Event::InputStarted { .. })
        ));
        provider.try_end(request).unwrap();
        first.service.receive().await;
        first.service.receive().await;
        first.service.send(resumable()).await;
        first.service.send(provider_audio(7, 1_920)).await;
        first.service.close(1012, "unconsumed old answer").await;
        let unavailable = memory_link(&mut provider, &mut clock, &mut pending).await;
        assert!(matches!(
            unavailable.state,
            ProviderLinkState::Unavailable { .. }
        ));
        // This is the same control upkeep called by serve at zero output credit.
        // Do not call next_output until B's setup has completed and been reported.
        let mut second = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                service_link(&mut provider, Some(&mut clock), &mut pending, |_| {
                    panic!("no setup yet")
                })
                .unwrap();
                if let Ok(dial) = dials.try_recv() {
                    break dial;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(second.resumed);
        second.service.accept().await;
        let ready = memory_link(&mut provider, &mut clock, &mut pending).await;
        assert!(ready.revision > unavailable.revision);
        assert_eq!(ready.session, second.session.get());
        assert!(matches!(ready.state, ProviderLinkState::Ready {
            context: LinkContext::Resumed, outage_started_at_us: Some(at), ..
        } if at == unavailable.observed_at_us));
        assert!(
            matches!(next_notice(&mut provider, true).await.unwrap(), Notice::Event(Event::Audio { pcm, .. }) if pcm == vec![7; 1_920])
        );
        assert!(
            matches!(next_notice(&mut provider, true).await.unwrap(), Notice::TurnLost {
            lineage, stage: Stage::Responding, ..
        } if lineage.request == request)
        );
        provider.shutdown();
    }

    #[tokio::test]
    async fn coordinated_memory_blocked_link_send_keeps_latest_source_stamp_and_shutdown() {
        use crate::provider_link::ProviderLinkState;
        use lamp_gemini::testing::next_dial;
        let (mut provider, mut dials, mut clock, mut pending) = memory_provider(quick_policy());
        let mut first = next_dial(&mut dials).await;
        first.service.accept().await;
        memory_link(&mut provider, &mut clock, &mut pending).await;
        first.service.send(resumable()).await;
        first.service.close(1012, "coalesced outage").await;
        let mut latest = None;
        let mut second = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                service_link(&mut provider, Some(&mut clock), &mut pending, |link| {
                    latest = Some(link);
                    Err(io::ErrorKind::WouldBlock.into())
                })
                .unwrap();
                if let Ok(dial) = dials.try_recv() {
                    break dial;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            latest.unwrap().state,
            ProviderLinkState::Unavailable { .. }
        ));
        second.service.accept().await;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                service_link(&mut provider, Some(&mut clock), &mut pending, |link| {
                    latest = Some(link);
                    Err(io::ErrorKind::WouldBlock.into())
                })
                .unwrap();
                if matches!(latest.unwrap().state, ProviderLinkState::Ready { .. }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        let original = latest.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(
            memory_link(&mut provider, &mut clock, &mut pending).await,
            original
        );
        // Another unanswered setup must not defer local shutdown. A blocked
        // control callback cannot keep the supervisor alive or replay input.
        second.service.close(1012, "hung setup").await;
        memory_link(&mut provider, &mut clock, &mut pending).await;
        let _hanging = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                service_link(&mut provider, Some(&mut clock), &mut pending, |_| {
                    Err(io::ErrorKind::WouldBlock.into())
                })
                .unwrap();
                if let Ok(dial) = dials.try_recv() {
                    break dial;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        provider.shutdown();
        let terminal = memory_link(&mut provider, &mut clock, &mut pending).await;
        assert!(matches!(
            terminal.state,
            ProviderLinkState::Terminal {
                reason: LinkFailure::Shutdown,
                ..
            }
        ));
        assert_eq!(
            next_notice(&mut provider, true).await.unwrap_err().reason,
            FailureReason::Shutdown
        );
    }

    #[tokio::test]
    async fn coordinated_memory_terminal_control_does_not_erase_the_answer_or_turn_failure() {
        use crate::provider_link::ProviderLinkState;
        use lamp_gemini::testing::next_dial;
        let (mut provider, mut dials, mut clock, mut pending) =
            memory_provider(RecoveryPolicy::DISABLED);
        let mut first = next_dial(&mut dials).await;
        first.service.accept().await;
        memory_link(&mut provider, &mut clock, &mut pending).await;
        let request = RequestId::new(1).unwrap();
        provider.try_start(request).unwrap();
        next_notice(&mut provider, true).await.unwrap();
        provider.try_end(request).unwrap();
        first.service.receive().await;
        first.service.receive().await;
        first.service.send(provider_audio(9, 2_000)).await;
        first
            .service
            .close(1012, "terminal with retained audio")
            .await;
        assert!(matches!(
            memory_link(&mut provider, &mut clock, &mut pending)
                .await
                .state,
            ProviderLinkState::Unavailable { .. }
        ));
        assert!(matches!(
            memory_link(&mut provider, &mut clock, &mut pending)
                .await
                .state,
            ProviderLinkState::Terminal { .. }
        ));
        assert!(
            matches!(next_notice(&mut provider, true).await.unwrap(), Notice::Event(Event::Audio { pcm, .. }) if pcm == vec![9; 2_000])
        );
        assert!(
            matches!(next_notice(&mut provider, true).await.unwrap(), Notice::TurnLost { lineage, stage: Stage::Responding, .. } if lineage.request == request)
        );
        assert_eq!(
            next_notice(&mut provider, true).await.unwrap_err().reason,
            FailureReason::NotRecoverable
        );
        assert_eq!(
            provider.try_start(RequestId::new(2).unwrap()),
            Err(Error::NotConnected)
        );
        assert!(dials.try_recv().is_err());
        provider.shutdown();
    }

    #[tokio::test]
    async fn coordinated_reconnect_and_link_status_progress_without_any_pcm_credit() {
        use crate::provider_link::{LinkContext, ProviderLinkState};
        let mut rig = LiveRig::start_with(
            quick_policy(),
            Options {
                reporting: Reporting::Coordinated,
            },
        );
        let mut dial = rig.coordinated_ready().await;
        dial.service.send(resumable()).await;
        let request = rig.speak(&mut dial, false, true).await;
        // Consume the two granted packets but return no credit. Read them
        // promptly, so this is output backpressure rather than stale IPC.
        dial.service.send(provider_audio(7, 1_920)).await;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut consumed = 0;
        while consumed < 2 {
            rig.publish();
            if let Some(ProviderOutput::Audio {
                request: got,
                samples,
                ..
            }) = rig.parent.data.receive().unwrap()
            {
                assert_eq!(got, request);
                assert_eq!(samples, vec![7; 960]);
                rig.output_flow.received().unwrap();
                consumed += 1;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        dial.service.send(provider_audio(8, 1_920)).await;
        dial.service.close(1012, "scripted restart").await;
        let unavailable = rig.link().await;
        assert!(matches!(
            unavailable.state,
            ProviderLinkState::Unavailable { .. }
        ));
        let mut next = rig.dial().await;
        let setup = rig.service_message(&mut next).await;
        assert!(setup["setup"].get("sessionResumption").is_some());
        next.service.send(json!({"setupComplete":{}})).await;
        let recovered = rig.link().await;
        // This is the first resume from A's advertised point. Resumed does
        // not acknowledge that the point includes the latest client messages;
        // later-connection input with only an inherited A handle is a separate
        // ResumedBeforeLatest case covered by the supervisor tests.
        assert!(
            matches!(recovered.state, ProviderLinkState::Ready {
            context: LinkContext::Resumed, outage_started_at_us: Some(at), ..
        } if at == unavailable.observed_at_us),
            "recovered={recovered:?}; unavailable={unavailable:?}"
        );
        assert!(recovered.session > unavailable.session);
        assert!(recovered.revision > unavailable.revision);
        assert!(
            rig.parent
                .data
                .receive::<ProviderOutput>()
                .unwrap()
                .is_none()
        );
        // The old answer and its exact terminal stage still follow in order.
        rig.output_flow
            .replenish(0, &mut rig.parent.control)
            .unwrap();
        for _ in 0..2 {
            assert!(matches!(rig.output().await, ProviderOutput::Audio {
                request: got, samples, ..
            } if got == request && samples == vec![8; 960]));
        }
        assert!(
            matches!(rig.wire().await, Wire::Status(ProviderStatus::TurnFailed {
            request: got, stage: FailedStage::Responding, ..
        }) if got == request)
        );
        rig.stop().await;
    }

    #[tokio::test]
    async fn coordinated_terminal_link_precedes_ordered_pcm_outcome_and_single_ended() {
        use crate::provider_link::ProviderLinkState;
        let mut rig = LiveRig::start_with(
            RecoveryPolicy::DISABLED,
            Options {
                reporting: Reporting::Coordinated,
            },
        );
        let mut dial = rig.coordinated_ready().await;
        let request = rig.speak(&mut dial, false, true).await;
        dial.service.send(provider_audio(9, 3_840)).await;
        // Drain only the two already granted packets, withholding more credit.
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut consumed = 0;
        while consumed < 2 {
            rig.publish();
            if let Some(ProviderOutput::Audio { request: got, .. }) =
                rig.parent.data.receive().unwrap()
            {
                assert_eq!(got, request);
                rig.output_flow.received().unwrap();
                consumed += 1;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        dial.service.close(1012, "terminal").await;
        let mut revisions = Vec::new();
        loop {
            let link = rig.link().await;
            revisions.push(link.revision);
            if matches!(link.state, ProviderLinkState::Terminal { .. }) {
                break;
            }
            assert!(matches!(link.state, ProviderLinkState::Unavailable { .. }));
        }
        assert!(revisions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(!rig.worker.is_finished());
        assert!(
            rig.parent
                .data
                .receive::<ProviderOutput>()
                .unwrap()
                .is_none()
        );
        rig.output_flow
            .replenish(0, &mut rig.parent.control)
            .unwrap();
        for _ in 0..2 {
            assert!(matches!(rig.output().await, ProviderOutput::Audio {
                request: got, samples, ..
            } if got == request && samples == vec![9; 960]));
        }
        assert!(
            matches!(rig.wire().await, Wire::Status(ProviderStatus::TurnFailed {
            request: got, stage: FailedStage::Responding, ..
        }) if got == request)
        );
        assert!(matches!(rig.output().await, ProviderOutput::Ended { .. }));
        rig.pump(Duration::from_millis(25)).await;
        assert!(rig.take().is_none(), "Ended must be emitted once");
        assert!(
            rig.dials.try_recv().is_err(),
            "disabled recovery must not replay input"
        );
        // An in-flight Start can arrive after Ended. It needs its own terminal
        // receipt; upstream termination is not a silent local intake cutoff.
        let old = rig.snapshot.owner().unwrap();
        rig.owner.cancel_turn(real_time(), old).unwrap();
        let newer = rig
            .owner
            .admit(real_time(), AdmittedInput::NewTurn)
            .unwrap();
        rig.publish();
        let privacy_generation = rig.snapshot.microphone_generation();
        rig.parent
            .data
            .send(ProviderInput::Start {
                session: 1,
                request: newer.turn(),
                privacy_generation,
            })
            .unwrap();
        assert!(
            matches!(rig.wire().await, Wire::Status(ProviderStatus::TurnFailed {
            request: got, stage: FailedStage::NotDelivered, ..
        }) if got == newer.turn())
        );
        rig.parent
            .data
            .send(ProviderInput::Audio {
                request: newer.turn(),
                privacy_generation,
                sequence: 1,
                read_completed_at_us: monotonic_us(),
                samples: vec![3; 160],
            })
            .unwrap();
        rig.parent
            .data
            .send(ProviderInput::End {
                request: newer.turn(),
                privacy_generation,
            })
            .unwrap();
        rig.pump(Duration::from_millis(25)).await;
        assert!(
            rig.take().is_none(),
            "refused Start has exactly one outcome"
        );
        assert!(
            rig.dials.try_recv().is_err(),
            "terminal upstream must never reconnect"
        );
        assert!(
            dial.service
                .try_receive(Duration::from_millis(5))
                .await
                .is_none()
        );
        rig.stop().await;
    }

    #[tokio::test]
    async fn coordinated_stop_does_not_wait_for_hung_reconnect_or_data_credit() {
        use crate::provider_link::ProviderLinkState;
        let mut rig = LiveRig::start_with(
            quick_policy(),
            Options {
                reporting: Reporting::Coordinated,
            },
        );
        let mut dial = rig.coordinated_ready().await;
        dial.service.send(resumable()).await;
        dial.service.close(1012, "outage").await;
        assert!(matches!(
            rig.link().await.state,
            ProviderLinkState::Unavailable { .. }
        ));
        let mut hanging = rig.dial().await;
        assert!(
            rig.service_message(&mut hanging)
                .await
                .get("setup")
                .is_some()
        );
        // Setup is deliberately left unanswered. Local control owns shutdown.
        rig.parent.control.send(Control::Stop).unwrap();
        let (result, elapsed) = rig.exit().await;
        result.unwrap();
        assert!(
            elapsed < Duration::from_millis(250),
            "Stop took {elapsed:?}"
        );
        assert!(
            hanging
                .service
                .try_receive(Duration::from_millis(10))
                .await
                .is_none()
        );
    }

    #[test]
    fn coordinated_full_control_send_is_bounded_and_does_not_consume_stop() {
        use crate::provider_link::{ProviderLink, ProviderLinkState};
        let (_directory, mut parent, mut worker) = intake_channels();
        let mut blocked = false;
        for _ in 0..4_096 {
            match worker.control.send(WorkerEvent::Ready) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    blocked = true;
                    break;
                }
                Err(error) => panic!("unexpected control send error: {error}"),
            }
        }
        assert!(blocked, "control socket must have finite capacity");
        let mut pending = PendingLink::default();
        pending
            .observe(
                ProviderLink {
                    revision: 1,
                    session: 1,
                    observed_at_us: monotonic_us(),
                    source_clock: None,
                    state: ProviderLinkState::Unavailable {
                        reason: Fault::Transport,
                    },
                },
                monotonic_us(),
            )
            .unwrap();
        pending
            .try_send(|link| worker.control.send(WorkerEvent::ProviderLink { link }))
            .unwrap();
        parent.control.send(Control::Stop).unwrap();
        let mut intake = ProviderIntake::new(BootId::new([9; 16]).unwrap(), monotonic_us());
        let mut budget = CONTROL_SLICE;
        assert_eq!(
            intake
                .drain_control(
                    &mut monotonic_us,
                    &mut || worker.control.receive(),
                    &mut budget
                )
                .unwrap(),
            ControlDrain::Stopped
        );
        let mut recovered = 0;
        pending
            .try_send(|_| {
                recovered += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(recovered, 1, "WouldBlock must retain exactly one status");
    }

    #[tokio::test]
    async fn coordinated_privacy_revocation_stops_a_pending_connection() {
        let mut rig = LiveRig::start_with(
            quick_policy(),
            Options {
                reporting: Reporting::Coordinated,
            },
        );
        let mut dial = rig.coordinated_ready().await;
        dial.service.send(resumable()).await;
        dial.service.close(1012, "outage").await;
        let _unavailable = rig.link().await;
        let mut hanging = rig.dial().await;
        assert!(
            rig.service_message(&mut hanging)
                .await
                .get("setup")
                .is_some()
        );
        rig.owner
            .set_microphone_permission(real_time(), Permission::Denied)
            .unwrap();
        rig.snapshot = rig.owner.snapshot(real_time()).unwrap();
        rig.parent
            .control
            .send(Control::Authority {
                snapshot: rig.snapshot,
            })
            .unwrap();
        let result = tokio::time::timeout(Duration::from_millis(250), &mut rig.worker)
            .await
            .unwrap()
            .unwrap();
        result.unwrap();
        assert!(
            hanging
                .service
                .try_receive(Duration::from_millis(10))
                .await
                .is_none()
        );
    }

    #[test]
    fn coordinated_terminal_and_failure_receipts_keep_the_existing_full_output_bound() {
        let mut output = VecDeque::from([Outbound::Output(ProviderOutput::Ended {
            reason: LinkFailure::NotRecoverable,
            fault: Fault::Transport,
            attempts: 0,
        })]);
        for request in 1..MAX_QUEUED_OUTPUT as u64 {
            output.push_back(Outbound::Output(ProviderOutput::TurnFailed {
                request,
                stage: FailedStage::Responding,
                reason: Fault::Transport,
            }));
        }
        // These are terminal receipts, not permission to play: cancellation
        // cannot erase them just because their immutable owner was retired.
        prune_retired_output(&mut output, u64::MAX);
        assert_eq!(output.len(), MAX_QUEUED_OUTPUT);
        let mut handoff = InputHandoff {
            undelivered: Some(999),
            ..InputHandoff::default()
        };
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(output.len(), MAX_QUEUED_OUTPUT);
        assert_eq!(handoff.undelivered, Some(999));
        assert!(matches!(
            output.pop_front(),
            Some(Outbound::Output(ProviderOutput::Ended { .. }))
        ));
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(output.len(), MAX_QUEUED_OUTPUT);
        assert_eq!(handoff.undelivered, None);
        assert!(matches!(
            output.back(),
            Some(Outbound::Status(ProviderStatus::TurnFailed {
                request: 999,
                stage: FailedStage::NotDelivered,
                ..
            }))
        ));
        queue_undelivered(&mut handoff, &mut output);
        assert_eq!(output.len(), MAX_QUEUED_OUTPUT);
    }

    #[tokio::test]
    async fn typed_replaced_session_drops_old_input_without_failing_the_new_connection() {
        use lamp_gemini::{LinkUpdate, LinkUpdateKind, OutputNotice, testing::next_dial};

        async fn link<C: Connect>(provider: &mut Supervisor<C>) -> LinkUpdate {
            tokio::time::timeout(Duration::from_secs(2), async {
                let mut tick = tokio::time::interval(Duration::from_millis(1));
                loop {
                    tick.tick().await;
                    if let Some(update) = provider.poll_link() {
                        return update;
                    }
                }
            })
            .await
            .expect("link observation")
        }

        // Exercise Audio, End and the implicit End before successor Start in
        // both modes. The production Legacy contract must still fail closed.
        for case in 0..6 {
            let mode = case % 3;
            let typed = case >= 3;
            let config =
                SessionConfig::new(GOOGLE_ENDPOINT, Credential::api_key("test-only").unwrap())
                    .unwrap()
                    .resumption(RESUMPTION)
                    .barrier_policy(BARRIER);
            let (connector, mut dials) = connector(config);
            let mut provider = Supervisor::new(connector, quick_policy()).unwrap();
            let mut first = next_dial(&mut dials).await;
            first.service.accept().await;
            assert!(matches!(
                link(&mut provider).await.kind,
                LinkUpdateKind::Ready { .. }
            ));
            let mut owner = controller();
            let mut handoff = InputHandoff {
                tolerate_outage: typed,
                ..InputHandoff::default()
            };
            let old = authorize(&mut owner, &mut handoff);
            handoff
                .submit(start(old, &handoff), NOW, &provider)
                .unwrap();
            assert!(matches!(
                provider.next_output().await.unwrap(),
                OutputNotice::Event(Event::InputStarted { .. })
            ));
            assert!(
                first.service.receive().await["realtimeInput"]
                    .get("activityStart")
                    .is_some()
            );
            handoff
                .submit(audio(old, 1, 7, &handoff), NOW, &provider)
                .unwrap();
            assert!(
                first.service.receive().await["realtimeInput"]
                    .get("audio")
                    .is_some()
            );
            first.service.send(resumable()).await;
            first.service.close(1011, "test disconnect").await;
            assert!(matches!(
                link(&mut provider).await.kind,
                LinkUpdateKind::Unavailable { .. }
            ));
            let mut replacement = next_dial(&mut dials).await;
            assert!(replacement.resumed);
            replacement.service.accept().await;
            assert!(matches!(
                link(&mut provider).await.kind,
                LinkUpdateKind::Ready { .. }
            ));
            assert!(provider.is_connected());

            if mode < 2 {
                let command = if mode == 0 {
                    audio(old, 2, 8, &handoff)
                } else {
                    end(old, &handoff)
                };
                let result = handoff.submit(command, NOW, &provider);
                if !typed {
                    assert!(result.unwrap_err().to_string().contains("StaleRequest"));
                    provider.shutdown();
                    continue;
                }
                result.expect("known old-session input is discarded");
                assert_eq!(handoff.undelivered, None);
                assert!(
                    matches!(provider.next_output().await.unwrap(), OutputNotice::TurnLost { lineage, stage: Stage::InputOpen, .. } if lineage.request.get() == old)
                );
                assert!(
                    replacement
                        .service
                        .try_receive(Duration::from_millis(10))
                        .await
                        .is_none()
                );
            }
            let next = authorize(&mut owner, &mut handoff);
            let result = handoff.submit(
                ProviderInput::Start {
                    request: next,
                    session: replacement.session.get(),
                    privacy_generation: generation(&handoff),
                },
                NOW,
                &provider,
            );
            if !typed {
                assert!(result.unwrap_err().to_string().contains("StaleRequest"));
                provider.shutdown();
                continue;
            }
            result.expect("new connection remains usable");
            assert!(
                matches!(provider.next_output().await.unwrap(), OutputNotice::Event(Event::InputStarted { lineage, .. }) if lineage.request.get() == next)
            );
            assert!(
                replacement.service.receive().await["realtimeInput"]
                    .get("activityStart")
                    .is_some()
            );
            assert_eq!(handoff.undelivered, None);
            provider.shutdown();
        }
    }

    #[tokio::test]
    async fn exhausted_output_credit_preserves_input_and_priority_stop() {
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let request = rig.speak(&mut dial, false, true).await;
        dial.service
            .send(provider_audio(5, lamp_gemini::MAX_OUTPUT_SAMPLES))
            .await;
        // Consume the two already-reserved packets without returning capacity.
        // Nothing else may be timestamped/sent while playback has no space.
        for _ in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                rig.publish();
                if let Some(output) = rig.parent.data.receive::<ProviderOutput>().unwrap() {
                    rig.output_flow.received().unwrap();
                    assert!(
                        matches!(output, ProviderOutput::Audio { request: current, .. } if current == request)
                    );
                    break;
                }
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
        rig.pump(Duration::from_millis(150)).await;
        assert!(
            rig.parent
                .data
                .receive::<ProviderOutput>()
                .unwrap()
                .is_none()
        );
        assert!(!rig.worker.is_finished());

        let next = rig
            .owner
            .admit(real_time(), AdmittedInput::Interruption)
            .unwrap();
        rig.publish();
        let privacy_generation = rig.snapshot.microphone_generation();
        rig.parent
            .data
            .send(ProviderInput::Start {
                session: 1,
                request: next.turn(),
                privacy_generation,
            })
            .unwrap();
        rig.parent
            .data
            .send(ProviderInput::Audio {
                request: next.turn(),
                privacy_generation,
                sequence: 1,
                read_completed_at_us: monotonic_us(),
                samples: vec![9; 160],
            })
            .unwrap();
        // The new microphone activity reaches the scripted service even while
        // all provider-output credits are exhausted. No old audio is relabelled.
        let start = rig.service_message(&mut dial).await;
        assert!(start["realtimeInput"].get("activityStart").is_some());
        let audio = rig.service_message(&mut dial).await;
        assert!(audio["realtimeInput"].get("audio").is_some());
        assert!(
            rig.parent
                .data
                .receive::<ProviderOutput>()
                .unwrap()
                .is_none()
        );
        rig.parent.control.send(Control::Stop).unwrap();
        let (result, elapsed) = rig.exit().await;
        assert!(result.is_ok());
        println!(
            "MEASURED stop_during_zero_output_credit_us={} (host control send to worker task end)",
            elapsed.as_micros()
        );
        assert!(elapsed < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn complete_answer_then_idle_provider_loss_recovers_with_context_and_a_second_ready() {
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        let request = rig.speak(&mut dial, false, true).await;
        // 2,000 frames arrive as one provider message; the service then closes
        // the idle session, as measured on V1 after 86-198 s without a turn.
        dial.service.send(provider_audio(5, 2_000)).await;
        dial.service
            .send(json!({"serverContent":{"generationComplete":true}}))
            .await;
        dial.service.send(idle_complete()).await;
        dial.service.close(1008, "The operation was aborted").await;
        let mut frames = 0;
        for offset in [0, 960, 1_920] {
            let ProviderOutput::Audio {
                request: owner,
                source_frames,
                source_offset,
                samples,
                ..
            } = rig.output().await
            else {
                panic!("expected provider audio");
            };
            assert_eq!(
                (owner, source_frames, source_offset),
                (request, 2_000, offset)
            );
            assert!(samples.iter().all(|sample| *sample == 5));
            frames += samples.len();
        }
        assert_eq!(frames, 2_000);
        assert!(matches!(
            rig.output().await,
            ProviderOutput::GenerationComplete { request: done } if done == request
        ));
        assert!(matches!(
            rig.output().await,
            ProviderOutput::TurnComplete { request: done, idle: true } if done == request
        ));
        // The coordinator finishes the turn; the worker reconnects on its own.
        let owner = rig.snapshot.owner().unwrap();
        rig.owner.complete_turn(real_time(), owner).unwrap();
        let mut redial = rig.ready().await;
        assert!(redial.resumed);
        assert_eq!(redial.session.get(), 2);
        // The next question is served by the resumed session.
        let next = rig.speak(&mut redial, false, true).await;
        redial.service.send(provider_audio(6, 240)).await;
        assert!(matches!(
            rig.output().await,
            ProviderOutput::Audio { request: owner, .. } if owner == next
        ));
        rig.parent.control.send(Control::Stop).unwrap();
        assert!(rig.exit().await.0.is_ok());
    }

    #[tokio::test]
    async fn answer_generated_before_the_service_closed_is_settled_not_failed() {
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        let request = rig.speak(&mut dial, false, true).await;
        dial.service.send(provider_audio(5, 480)).await;
        dial.service
            .send(json!({"serverContent":{"generationComplete":true}}))
            .await;
        dial.service.close(1008, "idle").await;
        assert!(matches!(rig.output().await, ProviderOutput::Audio { .. }));
        assert!(matches!(
            rig.output().await,
            ProviderOutput::GenerationComplete { .. }
        ));
        // No idle completion ever came from the service. The closed connection
        // is the barrier, so the fully delivered answer can finish normally.
        assert!(matches!(
            rig.output().await,
            ProviderOutput::TurnComplete { request: done, idle: true } if done == request
        ));
        rig.ready().await;
    }

    #[tokio::test]
    async fn provider_loss_before_an_answer_completes_ends_the_worker_naming_request_and_stage() {
        for (stage, expected) in [(0, "InputOpen"), (1, "AwaitingResponse"), (2, "Responding")] {
            let mut rig = LiveRig::start(quick_policy());
            let mut dial = rig.ready().await;
            dial.service.send(resumable()).await;
            let request = rig.speak(&mut dial, false, stage > 0).await;
            if stage == 2 {
                dial.service.send(provider_audio(5, 480)).await;
                assert!(matches!(rig.output().await, ProviderOutput::Audio { .. }));
            }
            dial.service.close(1011, "PRIVATE BACKEND DETAIL").await;
            let (result, _) = rig.exit().await;
            let message = result.unwrap_err().to_string();
            assert!(
                message.contains("PeerClosed { code: Some(1011) }"),
                "{message}"
            );
            assert!(
                message.contains(&format!("request {request} was lost at {expected}")),
                "{message}"
            );
            assert!(!message.contains("PRIVATE"));
        }
    }

    #[tokio::test]
    async fn stop_and_heartbeat_are_serviced_while_a_reconnect_hangs() {
        let mut rig = LiveRig::start(RecoveryPolicy {
            outage_budget: Duration::from_secs(5),
            ..quick_policy()
        });
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        dial.service.close(1008, "idle").await;
        // The service never answers the reconnect. For longer than the 250 ms
        // controller heartbeat bound the worker must keep draining control.
        let _hung = rig.dial().await;
        rig.pump(Duration::from_millis(400)).await;
        assert!(!rig.worker.is_finished());
        assert!(
            rig.parent
                .data
                .receive::<ProviderOutput>()
                .unwrap()
                .is_none()
        );
        rig.parent.control.send(Control::Stop).unwrap();
        let (result, waited) = rig.exit().await;
        assert!(result.is_ok());
        println!(
            "MEASURED stop_during_hung_reconnect_us={} (local socket send to worker task end, host)",
            waited.as_micros()
        );
        assert!(waited < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn input_admitted_during_an_outage_ends_the_worker_instead_of_being_held() {
        let mut rig = LiveRig::start(RecoveryPolicy {
            outage_budget: Duration::from_secs(5),
            ..quick_policy()
        });
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        dial.service.close(1008, "idle").await;
        let _hung = rig.dial().await;
        let owner = rig
            .owner
            .admit(real_time(), AdmittedInput::NewTurn)
            .unwrap();
        rig.publish();
        rig.parent
            .data
            .send(ProviderInput::Start {
                session: 1,
                request: owner.turn(),
                privacy_generation: rig.snapshot.microphone_generation(),
            })
            .unwrap();
        let message = rig.exit().await.0.unwrap_err().to_string();
        assert!(
            message.contains("provider unavailable while input was admitted"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn exhausted_recovery_and_quota_end_with_fixed_categories() {
        let mut rig = LiveRig::start(RecoveryPolicy {
            max_attempts: 2,
            ..quick_policy()
        });
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        dial.service.close(1008, "idle").await;
        drop(rig.dial().await);
        drop(rig.dial().await);
        let message = rig.exit().await.0.unwrap_err().to_string();
        assert!(
            message.contains("AttemptsExhausted after 2 recovery attempt(s)"),
            "{message}"
        );

        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        dial.service
            .close(1011, "You exceeded your current quota PRIVATE")
            .await;
        let message = rig.exit().await.0.unwrap_err().to_string();
        assert_eq!(
            message,
            "provider connection ended: Gemini Live QuotaExceeded; NotRecoverable after 0 recovery attempt(s)"
        );

        // No handle was ever issued: the conversation could not be restored,
        // so the loss is reported rather than hidden behind a fresh session.
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        dial.service.close(1008, "idle").await;
        let message = rig.exit().await.0.unwrap_err().to_string();
        assert!(message.contains("NoResumableContext"), "{message}");
    }

    #[tokio::test]
    async fn fast_provider_burst_is_held_by_flow_control_and_arrives_complete_in_order() {
        // Built before anything is on a real-time lease: encoding megabytes of
        // scripted audio must not be what makes this harness miss a heartbeat.
        const MESSAGES: usize = 40;
        const FRAMES: usize = lamp_gemini::MAX_OUTPUT_SAMPLES;
        let burst: Vec<_> = (0..MESSAGES)
            .map(|index| provider_audio(index as i16, FRAMES))
            .collect();
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let request = rig.speak(&mut dial, false, true).await;
        // 80 s of speech offered at once: 40 provider messages of 2 s each,
        // 2,000 IPC packets. The coordinator now grants at most two packets
        // in flight, so its backlog would pass the 512-packet bound that used to
        // end it. The IPC freshness rule means the coordinator side must keep
        // reading; it does, as fast as packets arrive.
        let mut service = dial.service;
        let script = tokio::spawn(async move {
            service.burst(burst).await;
            service.send(idle_complete()).await;
            service
        });
        let started = Instant::now();
        let (mut packets, mut last_sequence, mut frames) = (0, 0, 0usize);
        loop {
            match rig.output().await {
                ProviderOutput::Audio {
                    request: owner,
                    sequence,
                    source_offset,
                    samples,
                    ..
                } => {
                    if packets == 0 {
                        // Backpressure reached the service: with most of the
                        // burst unread it is still blocked on its socket.
                        assert!(!script.is_finished());
                    }
                    assert_eq!(owner, request);
                    assert!(sequence > last_sequence);
                    last_sequence = sequence;
                    // Every block carries its message index: none is lost,
                    // duplicated or reordered.
                    let expected = (frames / FRAMES) as i16;
                    assert_eq!(source_offset, frames % FRAMES);
                    assert!(samples.iter().all(|sample| *sample == expected));
                    frames += samples.len();
                    packets += 1;
                }
                ProviderOutput::TurnComplete { idle: true, .. } => break,
                other => panic!("unexpected provider output: {other:?}"),
            }
        }
        assert_eq!((packets, frames), (MESSAGES * 50, MESSAGES * FRAMES));
        println!(
            "MEASURED burst_80s_audio_2000_packets_delivered_ms={} (scripted service to local IPC reader, host)",
            started.elapsed().as_millis()
        );
        let _service = script.await.unwrap();
        rig.parent.control.send(Control::Stop).unwrap();
        assert!(rig.exit().await.0.is_ok());
    }

    #[tokio::test]
    async fn late_provider_events_never_reach_the_coordinator_for_a_newer_request() {
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let first = rig.speak(&mut dial, false, true).await;
        dial.service.send(provider_audio(5, 240)).await;
        dial.service.send(idle_complete()).await;
        assert!(matches!(rig.output().await, ProviderOutput::Audio { .. }));
        assert!(matches!(
            rig.output().await,
            ProviderOutput::TurnComplete { request, idle: true } if request == first
        ));
        let owner = rig.snapshot.owner().unwrap();
        rig.owner.complete_turn(real_time(), owner).unwrap();
        // The person starts a follow-up. The interruption that its explicit
        // activityStart requested, and a duplicate completion, arrive late.
        let second = rig.speak(&mut dial, false, false).await;
        dial.service.send(provider_audio(66, 240)).await;
        dial.service
            .send(json!({"serverContent":{"interrupted":true}}))
            .await;
        dial.service.send(idle_complete()).await;
        rig.pump(Duration::from_millis(120)).await;
        assert!(
            rig.parent
                .data
                .receive::<ProviderOutput>()
                .unwrap()
                .is_none()
        );
        // The follow-up ends and is answered under its own identity only.
        let owner = rig.snapshot.owner().unwrap();
        rig.owner.input_ended(real_time(), owner).unwrap();
        rig.publish();
        rig.parent
            .data
            .send(ProviderInput::End {
                request: second,
                privacy_generation: rig.snapshot.microphone_generation(),
            })
            .unwrap();
        rig.pump(Duration::from_millis(40)).await;
        dial.service.send(provider_audio(7, 240)).await;
        let ProviderOutput::Audio {
            request, samples, ..
        } = rig.output().await
        else {
            panic!("expected the follow-up answer");
        };
        assert_eq!(request, second);
        assert!(samples.iter().all(|sample| *sample == 7));
    }

    // ---- Speaking speed through the output capacity contract ------------
    //
    // Real time: the IPC and authority clocks cannot be paused. The harness
    // plays the coordinator exactly as `provider-flow-control.md` specifies:
    // every received packet is counted, and capacity is granted from the free
    // space of a reply buffer that drains at 24,000 samples per second.

    const LARGEST: usize = lamp_gemini::MAX_OUTPUT_SAMPLES;
    const REPLY_BUFFER: usize = crate::provider_flow::MAX_REPLY_SAMPLES;

    /// The coordinator's reply buffer and speaker, reduced to their arithmetic.
    /// Playback advances by measured time only: time spent held or with
    /// nothing to play is never made up afterwards.
    struct Speaker {
        /// Samples the buffer may hold. The real coordinator has 30 s; a test
        /// may give it less so a short answer still exhausts capacity.
        room: usize,
        received: u64,
        played: u64,
        /// When playback was last advanced; set by the first audio.
        at: Option<Instant>,
        /// Microsecond-samples not yet amounting to a whole sample.
        fraction: u64,
        held: Duration,
        /// Samples of playing time lost with an empty buffer.
        starved: u64,
        /// The largest backlog ever held.
        peak: usize,
    }
    impl Speaker {
        fn new(room: usize) -> Self {
            Self {
                room,
                received: 0,
                played: 0,
                at: None,
                fraction: 0,
                held: Duration::ZERO,
                starved: 0,
                peak: 0,
            }
        }
        fn hear(&mut self, samples: usize) {
            self.at.get_or_insert_with(Instant::now);
            self.received += samples as u64;
        }
        /// Advance playback to now, unless it is on `hold`, and return the
        /// backlog.
        fn backlog(&mut self, hold: bool) -> usize {
            if let Some(last) = self.at {
                let now = Instant::now();
                self.at = Some(now);
                let elapsed = now - last;
                if hold {
                    self.held += elapsed;
                } else {
                    self.fraction += elapsed.as_micros() as u64 * 24_000;
                    let due = self.fraction / 1_000_000;
                    self.fraction %= 1_000_000;
                    let waiting = self.received - self.played;
                    self.played += due.min(waiting);
                    self.starved += due.saturating_sub(waiting);
                }
            }
            let backlog = (self.received - self.played) as usize;
            self.peak = self.peak.max(backlog);
            backlog
        }
        /// Cancel what is queued, as the coordinator does on a new admission.
        fn flush(&mut self) {
            self.played = self.received;
        }
    }
    impl LiveRig {
        /// One coordinator tick: heartbeat, then capacity from the speaker's
        /// free space. A smaller `room` is expressed as space already taken.
        fn grant(&mut self, speaker: &mut Speaker, hold: bool) {
            self.publish();
            let backlog = speaker.backlog(hold);
            assert!(backlog <= speaker.room, "reply buffer overran: {backlog}");
            self.output_flow
                .replenish(
                    REPLY_BUFFER - speaker.room + backlog,
                    &mut self.parent.control,
                )
                .unwrap();
        }
        /// Every packet that has arrived, each counted against capacity. An
        /// excess over what was granted fails here.
        fn arrived(&mut self) -> Vec<Wire> {
            let mut all = Vec::new();
            while let Some(wire) = self.parent.data.receive::<Wire>().unwrap() {
                self.output_flow.received().unwrap();
                all.push(wire);
            }
            all
        }
    }
    struct Playback {
        packets: u64,
        frames: usize,
        /// First tick to the idle completion arriving.
        completed: Duration,
        /// First tick to the last sample leaving the speaker.
        finished: Duration,
        held: Duration,
        starved: Duration,
        peak: usize,
    }
    impl LiveRig {
        /// Take one answer of `total` samples at speaking speed, to the end of
        /// its playback. Playback stops once, for `pause`, after `pause_after`
        /// samples have been played.
        async fn play_at_speaking_speed(
            &mut self,
            request: u64,
            room: usize,
            total: u64,
            pause_after: u64,
            pause: Duration,
        ) -> Playback {
            let mut speaker = Speaker::new(room);
            let started = Instant::now();
            let (mut packets, mut frames) = (0u64, 0usize);
            let mut completed = None;
            // When the last sample left the speaker, and the playing time
            // lost to an empty buffer up to then.
            let mut finished = None;
            loop {
                let hold = speaker.played >= pause_after && speaker.held < pause;
                self.grant(&mut speaker, hold);
                for wire in self.arrived() {
                    assert!(completed.is_none(), "output after completion: {wire:?}");
                    match wire {
                        Wire::Output(ProviderOutput::Audio {
                            request: owner,
                            source_offset,
                            samples,
                            ..
                        }) => {
                            assert_eq!(owner, request);
                            // Every block carries its message index: nothing
                            // lost, repeated or reordered.
                            let expected = (frames / LARGEST) as i16;
                            assert_eq!(source_offset, frames % LARGEST);
                            assert!(samples.iter().all(|sample| *sample == expected));
                            frames += samples.len();
                            packets += 1;
                            speaker.hear(samples.len());
                        }
                        Wire::Output(ProviderOutput::GenerationComplete { .. }) => {}
                        Wire::Output(ProviderOutput::TurnComplete { idle: true, .. }) => {
                            completed = Some(started.elapsed());
                        }
                        other => panic!("unexpected at speaking speed: {other:?}"),
                    }
                }
                if finished.is_none() && speaker.played == total {
                    finished = Some((started.elapsed(), speaker.starved));
                }
                if let (Some(completed), Some((finished, starved))) = (completed, finished) {
                    return Playback {
                        packets,
                        frames,
                        completed,
                        finished,
                        held: speaker.held,
                        starved: Duration::from_micros(starved * 1_000_000 / 24_000),
                        peak: speaker.peak,
                    };
                }
                assert!(!self.worker.is_finished(), "worker ended mid-answer");
                assert!(
                    started.elapsed() < Duration::from_secs(total / 24_000 + 120),
                    "answer never finished"
                );
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    }
    /// How long the scripted network stalls inside a long answer, and how many
    /// largest messages then arrive with no gap between them.
    const STALL: Duration = Duration::from_secs(3);
    const BURST: u64 = 5;

    /// The service side of one long answer: largest messages at ten times
    /// speaking speed, a network stall a third of the way in followed by a
    /// ten-second burst, pings answered throughout, and idle completion
    /// withheld for the answer's own duration, as the service assumes
    /// playback. Returns how long the writing took, and the service: dropping
    /// it would be a connection loss.
    fn long_answer(
        mut service: lamp_gemini::testing::FakeService,
        seconds: u64,
    ) -> JoinHandle<(Duration, lamp_gemini::testing::FakeService)> {
        tokio::spawn(async move {
            let started = Instant::now();
            let messages = seconds / 2;
            let stall_at = messages / 3;
            for index in 0..messages {
                if index == stall_at {
                    service.rest(STALL).await;
                }
                service.send(provider_audio(index as i16, LARGEST)).await;
                if !(stall_at..stall_at + BURST - 1).contains(&index) {
                    service.rest(Duration::from_millis(200)).await;
                }
            }
            let written = started.elapsed();
            service
                .send(json!({"serverContent":{"generationComplete":true}}))
                .await;
            service
                .rest(Duration::from_secs(seconds).saturating_sub(started.elapsed()))
                .await;
            service.send(idle_complete()).await;
            (written, service)
        })
    }
    /// A `seconds` long answer through a reply buffer of `room` samples, with
    /// playback held for `pause` once five seconds have been played.
    async fn speaking_speed(seconds: u64, room: usize, pause: Duration) {
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let request = rig.speak(&mut dial, false, true).await;
        let script = long_answer(dial.service, seconds);
        let total = seconds * 24_000;
        let played = rig
            .play_at_speaking_speed(request, room, total, 5 * 24_000, pause)
            .await;
        let (written, _service) = script.await.unwrap();
        assert_eq!(played.frames as u64, total);
        assert_eq!(played.packets, seconds * 25);
        println!(
            "MEASURED speaking_speed seconds={seconds} reply_buffer_s={:.1} hold_ms={} playback_finished_ms={} completion_received_ms={} starved_ms={} service_wrote_in_ms={} packets={} peak_backlog_s={:.2} (scripted service through output capacity to a local IPC reader and a modeled speaker, host)",
            room as f64 / 24_000.0,
            played.held.as_millis(),
            played.finished.as_millis(),
            played.completed.as_millis(),
            played.starved.as_millis(),
            written.as_millis(),
            played.packets,
            played.peak as f64 / 24_000.0
        );
        // Playback was held for the whole pause and never ran dry: the stall
        // and the pause were both absorbed by what was already buffered.
        assert!(played.held >= pause, "{:?}", played.held);
        assert!(
            played.starved < Duration::from_millis(100),
            "{:?}",
            played.starved
        );
        // So the answer took its own length plus the pause, and no longer.
        let expected = Duration::from_secs(seconds) + played.held + played.starved;
        assert!(
            played.finished >= expected && played.finished <= expected + Duration::from_secs(1),
            "{:?} against {expected:?}",
            played.finished
        );
        // Its completion needs capacity like any packet and still arrives.
        assert!(played.completed <= played.finished + Duration::from_secs(1));
        // Capacity really was exhausted: the buffer filled and stayed bounded.
        assert!(played.peak <= room);
        if total as usize > room {
            assert!(played.peak + 2 * PACKET_SAMPLES >= room, "{}", played.peak);
        }
        // The service was not slowed to playback speed.
        assert!(
            written <= STALL + Duration::from_secs(seconds / 4 + 3),
            "{written:?}"
        );
        rig.parent.control.send(Control::Stop).unwrap();
        assert!(rig.exit().await.0.is_ok());
    }

    #[tokio::test]
    async fn twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity() {
        // A small buffer so a short answer exhausts capacity for most of its
        // length. Playback also stops for four seconds with the buffer full.
        speaking_speed(20, 5 * 24_000, Duration::from_secs(4)).await;
    }

    #[tokio::test]
    #[ignore = "one minute of real time; run explicitly"]
    async fn sixty_second_answer_follows_output_capacity() {
        speaking_speed(60, REPLY_BUFFER, Duration::from_secs(10)).await;
    }

    #[tokio::test]
    #[ignore = "three minutes of real time; run explicitly"]
    async fn three_minute_answer_follows_output_capacity() {
        speaking_speed(180, REPLY_BUFFER, Duration::from_secs(20)).await;
    }

    #[tokio::test]
    async fn interrupting_a_capacity_paced_answer_is_not_delayed_by_its_queued_audio() {
        let answer: Vec<_> = (0..20)
            .map(|index| provider_audio(index, LARGEST))
            .collect();
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let old = rig.speak(&mut dial, false, true).await;
        // Forty seconds generated at once into a two-second reply buffer: the
        // worker holds its full backlog and the transport holds the rest.
        let mut speaker = Speaker::new(2 * 24_000);
        // One coordinator tick while only the old answer may arrive.
        let hear = |rig: &mut LiveRig, speaker: &mut Speaker| {
            rig.grant(speaker, false);
            for wire in rig.arrived() {
                let Wire::Output(ProviderOutput::Audio {
                    request, samples, ..
                }) = wire
                else {
                    panic!("audio only");
                };
                assert_eq!(request, old);
                speaker.hear(samples.len());
            }
        };
        for message in answer {
            dial.service.send(message).await;
            hear(&mut rig, &mut speaker);
        }
        let started = Instant::now();
        while speaker.played < 2 * 24_000 {
            hear(&mut rig, &mut speaker);
            assert!(
                started.elapsed() < Duration::from_secs(8),
                "playback stalled"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        // The person interrupts. The coordinator drops what it had queued.
        let owner = rig
            .owner
            .admit(real_time(), AdmittedInput::Interruption)
            .unwrap();
        let new = owner.turn();
        speaker.flush();
        rig.grant(&mut speaker, false);
        let interrupted_at = Instant::now();
        let privacy_generation = rig.snapshot.microphone_generation();
        rig.parent
            .data
            .send(ProviderInput::Start {
                session: 1,
                request: new,
                privacy_generation,
            })
            .unwrap();
        rig.parent
            .data
            .send(ProviderInput::Audio {
                request: new,
                privacy_generation,
                sequence: 1,
                read_completed_at_us: monotonic_us(),
                samples: vec![3; 160],
            })
            .unwrap();
        rig.owner.input_ended(real_time(), owner).unwrap();
        rig.publish();
        rig.parent
            .data
            .send(ProviderInput::End {
                request: new,
                privacy_generation: rig.snapshot.microphone_generation(),
            })
            .unwrap();
        // The scripted service confirms the interruption and answers. Packets
        // keep being counted and capacity granted while it does.
        let mut stale = 0u64;
        let mut first_new = None;
        let mut step = 0;
        while first_new.is_none() {
            rig.grant(&mut speaker, false);
            for wire in rig.arrived() {
                match wire {
                    Wire::Output(ProviderOutput::Audio {
                        request, samples, ..
                    }) if request == new => {
                        assert!(samples.iter().all(|sample| *sample == 99));
                        first_new.get_or_insert(interrupted_at.elapsed());
                    }
                    // Already sent before the interruption: at most the two
                    // packets of outstanding capacity. Discarded on arrival.
                    Wire::Output(ProviderOutput::Audio { .. }) => stale += 1,
                    Wire::Output(_) => {}
                    Wire::Status(status) => panic!("unexpected status: {status:?}"),
                }
            }
            if let Some(message) = dial.service.try_receive(Duration::from_millis(2)).await {
                let input = &message["realtimeInput"];
                let expected = ["activityStart", "audio", "activityEnd"][step];
                assert!(input.get(expected).is_some(), "{message}");
                step += 1;
                if step == 2 {
                    dial.service
                        .send(json!({"serverContent":{"interrupted":true}}))
                        .await;
                    dial.service.send(idle_complete()).await;
                } else if step == 3 {
                    dial.service.send(provider_audio(99, 960)).await;
                }
            }
            assert!(
                interrupted_at.elapsed() < Duration::from_secs(5),
                "new answer delayed"
            );
        }
        let first_new = first_new.unwrap();
        println!(
            "MEASURED interruption_under_capacity new_answer_first_packet_ms={} stale_packets_after_interruption={stale} (admission to first packet of the new answer at a local IPC reader, scripted service, host)",
            first_new.as_millis()
        );
        // The cancelled answer still had about 36 s queued, 15 s of it in this
        // worker. At playback pace that would have come first.
        assert!(first_new < Duration::from_secs(1), "{first_new:?}");
        assert!(stale <= 2, "{stale} stale packets");
    }

    // ---- Proposed typed outage events ------------------------------------

    #[test]
    fn proposed_status_events_have_a_fixed_wire_shape_beside_provider_output() {
        for (status, expected) in [
            (
                ProviderStatus::Unavailable {
                    reason: Fault::PeerClosed { code: Some(1008) },
                },
                json!({"kind":"unavailable","reason":{"category":"peer_closed","code":1008}}),
            ),
            (
                ProviderStatus::Recovered {
                    outage_us: 1_500_000,
                    attempts: 2,
                    context: RecoveredContext::ResumedBeforeLatest,
                    reason: Fault::ServerGoAway,
                },
                json!({"kind":"recovered","outage_us":1_500_000,"attempts":2,
                    "context":"resumed_before_latest","reason":{"category":"server_go_away"}}),
            ),
            (
                ProviderStatus::TurnFailed {
                    request: 7,
                    stage: FailedStage::AwaitingResponse,
                    reason: Fault::StalledResponse,
                },
                json!({"kind":"turn_failed","request":7,"stage":"awaiting_response",
                    "reason":{"category":"stalled_response"}}),
            ),
            (
                ProviderStatus::OwnershipAssumed {
                    request: Some(8),
                    superseded: None,
                },
                json!({"kind":"ownership_assumed","request":8,"superseded":null}),
            ),
        ] {
            assert_eq!(
                serde_json::to_value(Outbound::Status(status.clone())).unwrap(),
                expected
            );
            assert_eq!(
                serde_json::from_value::<ProviderStatus>(expected).unwrap(),
                status
            );
        }
        // Existing outputs keep their exact shape inside the same queue.
        assert_eq!(
            serde_json::to_value(Outbound::Output(ProviderOutput::TurnComplete {
                request: 3,
                idle: true
            }))
            .unwrap(),
            json!({"kind":"turn_complete","request":3,"idle":true})
        );
        // Every transport error has a category; none carries text.
        assert_eq!(Fault::from(Error::QuotaExceeded), Fault::QuotaExceeded);
        assert_eq!(Fault::from(Error::MalformedMessage), Fault::Protocol);
        assert_eq!(Fault::from(Error::Backpressure), Fault::Local);
        assert!(
            serde_json::from_value::<ProviderStatus>(json!({"kind":"unavailable",
            "reason":{"category":"transport"},"detail":"x"}))
            .is_err()
        );
    }

    #[tokio::test]
    async fn typed_reporting_fails_one_turn_and_keeps_serving_after_a_lost_answer() {
        let mut rig = LiveRig::start_with(
            quick_policy(),
            Options {
                reporting: Reporting::Typed,
            },
        );
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        let lost = rig.speak(&mut dial, false, true).await;
        dial.service.send(provider_audio(5, 1_920)).await;
        dial.service.close(1011, "PRIVATE BACKEND DETAIL").await;
        // In order on the wire: what arrived, then the explicit failure. The
        // link status is not held behind the audio.
        let mut seen = Vec::new();
        let mut frames = 0;
        let mut redial = None;
        while !seen.contains(&"recovered") || !seen.contains(&"turn_failed") {
            if redial.is_none()
                && let Ok(mut attempt) = rig.dials.try_recv()
            {
                let setup = rig.service_message(&mut attempt).await;
                assert_eq!(setup["setup"]["sessionResumption"]["handle"], "HANDLE-ONE");
                attempt.service.send(json!({"setupComplete": {}})).await;
                redial = Some(attempt);
            }
            rig.publish();
            // Statuses are packets too: each is counted and capacity returned.
            match rig.take() {
                Some(Wire::Output(ProviderOutput::Audio {
                    request, samples, ..
                })) => {
                    assert_eq!(request, lost);
                    assert!(!seen.contains(&"turn_failed"), "audio after its failure");
                    frames += samples.len();
                }
                Some(Wire::Status(ProviderStatus::Unavailable { reason })) => {
                    assert_eq!(reason, Fault::PeerClosed { code: Some(1011) });
                    seen.push("unavailable");
                }
                Some(Wire::Status(ProviderStatus::TurnFailed {
                    request,
                    stage,
                    reason,
                })) => {
                    assert_eq!((request, stage), (lost, FailedStage::Responding));
                    assert_eq!(reason, Fault::PeerClosed { code: Some(1011) });
                    seen.push("turn_failed");
                }
                Some(Wire::Status(ProviderStatus::Recovered {
                    context,
                    attempts,
                    reason,
                    ..
                })) => {
                    assert_eq!((context, attempts), (RecoveredContext::Resumed, 1));
                    assert_eq!(reason, Fault::PeerClosed { code: Some(1011) });
                    seen.push("recovered");
                }
                Some(other) => panic!("unexpected: {other:?}"),
                None => tokio::time::sleep(Duration::from_millis(2)).await,
            }
            assert!(
                !rig.worker.is_finished(),
                "typed reporting keeps the worker alive"
            );
        }
        assert_eq!(frames, 1_920);
        assert_eq!(
            seen.iter().filter(|kind| **kind == "turn_failed").count(),
            1
        );
        assert!(
            seen.iter().position(|kind| *kind == "unavailable")
                < seen.iter().position(|kind| *kind == "recovered")
        );
        // The coordinator fails that turn and the next question is served.
        let owner = rig.snapshot.owner().unwrap();
        rig.owner.cancel_turn(real_time(), owner).unwrap();
        let mut redial = redial.unwrap();
        let next = rig.speak(&mut redial, false, true).await;
        redial.service.send(provider_audio(6, 240)).await;
        assert!(matches!(
            rig.output().await,
            ProviderOutput::Audio { request, .. } if request == next
        ));
    }

    #[tokio::test]
    async fn typed_reporting_fails_a_question_admitted_during_an_outage_once_and_never_sends_it() {
        let mut rig = LiveRig::start_with(
            RecoveryPolicy {
                outage_budget: Duration::from_secs(5),
                ..quick_policy()
            },
            Options {
                reporting: Reporting::Typed,
            },
        );
        let mut dial = rig.ready().await;
        dial.service.send(resumable()).await;
        dial.service.close(1008, "idle").await;
        assert!(matches!(
            rig.wire().await,
            Wire::Status(ProviderStatus::Unavailable { .. })
        ));
        // The reconnect is unanswered when the person speaks.
        let mut redial = rig.dial().await;
        let owner = rig
            .owner
            .admit(real_time(), AdmittedInput::NewTurn)
            .unwrap();
        let request = owner.turn();
        rig.publish();
        let privacy_generation = rig.snapshot.microphone_generation();
        rig.parent
            .data
            .send(ProviderInput::Start {
                session: 1,
                request,
                privacy_generation,
            })
            .unwrap();
        assert!(matches!(
            rig.wire().await,
            Wire::Status(ProviderStatus::TurnFailed { request: failed, stage: FailedStage::NotDelivered, .. })
                if failed == request
        ));
        // Its remaining input keeps arriving until the coordinator reacts.
        for sequence in 1..=5 {
            rig.parent
                .data
                .send(ProviderInput::Audio {
                    request,
                    privacy_generation,
                    sequence,
                    read_completed_at_us: monotonic_us(),
                    samples: vec![3; 160],
                })
                .unwrap();
        }
        rig.parent
            .data
            .send(ProviderInput::End {
                request,
                privacy_generation,
            })
            .unwrap();
        // The connection comes back while that question is still the owner.
        let setup = rig.service_message(&mut redial).await;
        assert!(setup["setup"].get("sessionResumption").is_some());
        redial.service.send(json!({"setupComplete": {}})).await;
        assert!(matches!(
            rig.wire().await,
            Wire::Status(ProviderStatus::Recovered { .. })
        ));
        rig.pump(Duration::from_millis(100)).await;
        // Reported once; nothing of it reached the new session, late or at all.
        assert!(rig.parent.data.receive::<Wire>().unwrap().is_none());
        assert!(
            redial
                .service
                .try_receive(Duration::from_millis(20))
                .await
                .is_none()
        );
        rig.owner.cancel_turn(real_time(), owner).unwrap();
        let next = rig.speak(&mut redial, false, true).await;
        assert!(next > request);
    }

    #[tokio::test]
    async fn production_policy_never_commits_a_successor_without_a_terminal_barrier() {
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let cut = rig.speak(&mut dial, false, true).await;
        let resumed = rig.speak(&mut dial, true, false).await;
        assert!(resumed > cut);
        let owner = rig.snapshot.owner().unwrap();
        rig.owner.input_ended(real_time(), owner).unwrap();
        rig.publish();
        rig.parent
            .data
            .send(ProviderInput::End {
                request: resumed,
                privacy_generation: rig.snapshot.microphone_generation(),
            })
            .unwrap();
        // The opening audio was already delivered. Only End must wait. A
        // silent service cannot prove that a later answer belongs to this turn.
        let (result, _) = rig.exit().await;
        let error = result.unwrap_err().to_string();
        assert!(error.contains("BarrierTimeout"), "{error}");
        assert!(
            dial.service
                .try_receive(Duration::from_millis(20))
                .await
                .is_none(),
            "successor was committed without a provider terminal event"
        );
    }

    #[tokio::test]
    async fn resumed_question_survives_a_silent_service_and_the_assumption_is_reported() {
        // The person continues after the endpointer cut the question. The
        // service says nothing about the cut-off request for the whole bound.
        for reporting in [Reporting::Legacy, Reporting::Typed] {
            let mut rig = LiveRig::start_with_barrier(
                quick_policy(),
                Options { reporting },
                BarrierPolicy::AssumeAfterQuiet,
            );
            let mut dial = rig.ready().await;
            let cut = rig.speak(&mut dial, false, true).await;
            let resumed_at = Instant::now();
            let resumed = rig.speak(&mut dial, true, false).await;
            // Its opening is already with the service, in order and whole.
            let owner = rig.snapshot.owner().unwrap();
            rig.owner.input_ended(real_time(), owner).unwrap();
            rig.publish();
            rig.parent
                .data
                .send(ProviderInput::End {
                    request: resumed,
                    privacy_generation: rig.snapshot.microphone_generation(),
                })
                .unwrap();
            if reporting == Reporting::Typed {
                assert!(matches!(
                    rig.wire().await,
                    Wire::Status(ProviderStatus::OwnershipAssumed { request: Some(new), superseded: Some(old) })
                        if new == resumed && old == cut
                ));
            }
            let message = rig.service_message(&mut dial).await;
            assert!(message["realtimeInput"].get("activityEnd").is_some());
            let waited = resumed_at.elapsed();
            assert!(waited >= Duration::from_millis(1_900), "{waited:?}");
            dial.service.send(provider_audio(8, 240)).await;
            assert!(matches!(
                rig.output().await,
                ProviderOutput::Audio { request, .. } if request == resumed
            ));
            assert!(!rig.worker.is_finished());
        }
    }

    /// `Continuation::KeptStart` through the worker's own loop: the real
    /// intake, hand-off and session, against the scripted service, with
    /// input at the rate it is spoken and its own capture times.
    mod kept_start {
        use super::*;
        use lamp_gemini::testing::sent_audio;

        impl LiveRig {
            fn start_kept() -> Self {
                Self::start_kept_reporting(Reporting::Legacy)
            }
            /// `Coordinated` is what the command line starts the worker with.
            fn start_kept_reporting(reporting: Reporting) -> Self {
                Self::start_all(
                    quick_policy(),
                    Options { reporting },
                    BARRIER,
                    false,
                    Continuation::KeptStart,
                )
            }
            async fn kept_ready(&mut self, reporting: Reporting) -> Dial {
                if reporting == Reporting::Coordinated {
                    self.coordinated_ready().await
                } else {
                    self.ready().await
                }
            }
            /// A person speaks again: a newly admitted request and its
            /// Start. With the start kept the service sees nothing of it.
            async fn begin(&mut self, dial: &Dial, input: AdmittedInput) -> u64 {
                let owner = self.owner.admit(real_time(), input).unwrap();
                let request = owner.turn();
                self.publish();
                self.parent
                    .data
                    .send(ProviderInput::Start {
                        session: dial.session.get(),
                        request,
                        privacy_generation: self.snapshot.microphone_generation(),
                    })
                    .unwrap();
                loop {
                    match self.output().await {
                        ProviderOutput::Started {
                            request: started, ..
                        } if started == request => return request,
                        // The earlier request's own lifecycle may come first.
                        ProviderOutput::Interrupted { .. }
                        | ProviderOutput::TurnComplete { .. }
                        | ProviderOutput::InputDiscarded { .. } => {}
                        other => panic!("unexpected before the start: {other:?}"),
                    }
                }
            }
            /// Blocks of 10 ms, one every 10 ms, each read now. Every
            /// sample of a block is `base` plus its sequence.
            async fn say(&mut self, request: u64, blocks: std::ops::Range<u64>, base: i16) {
                for sequence in blocks {
                    self.publish();
                    self.parent
                        .data
                        .send(ProviderInput::Audio {
                            request,
                            privacy_generation: self.snapshot.microphone_generation(),
                            sequence,
                            read_completed_at_us: monotonic_us(),
                            samples: vec![base + sequence as i16; 160],
                        })
                        .unwrap();
                    self.pump(Duration::from_millis(10)).await;
                }
            }
            /// The current request's input ends.
            fn finish(&mut self, request: u64) {
                let owner = self.snapshot.owner().unwrap();
                assert_eq!(owner.turn(), request);
                self.owner.input_ended(real_time(), owner).unwrap();
                self.publish();
                self.parent
                    .data
                    .send(ProviderInput::End {
                        request,
                        privacy_generation: self.snapshot.microphone_generation(),
                    })
                    .unwrap();
            }
            /// What the scripted service receives until it has received
            /// nothing for `quiet`: "start", "end", or the first sample of
            /// an audio block.
            async fn heard(&mut self, dial: &mut Dial, quiet: Duration) -> Vec<String> {
                let mut all = Vec::new();
                let mut last = Instant::now();
                while last.elapsed() < quiet {
                    self.publish();
                    let Some(message) = dial.service.try_receive(Duration::from_millis(5)).await
                    else {
                        continue;
                    };
                    last = Instant::now();
                    let input = &message["realtimeInput"];
                    all.push(if input.get("activityStart").is_some() {
                        "start".to_owned()
                    } else if input.get("activityEnd").is_some() {
                        "end".to_owned()
                    } else {
                        let samples = sent_audio(&message)
                            .unwrap_or_else(|| panic!("unexpected client message {message}"));
                        assert_eq!(samples.len(), 160);
                        assert!(samples.iter().all(|sample| *sample == samples[0]));
                        samples[0].to_string()
                    });
                }
                all
            }
            /// The next output that is not the earlier request's lifecycle.
            async fn answer(&mut self) -> ProviderOutput {
                loop {
                    match self.output().await {
                        ProviderOutput::Interrupted { .. }
                        | ProviderOutput::TurnComplete { .. } => {}
                        other => return other,
                    }
                }
            }
        }
        const QUIET: Duration = Duration::from_millis(60);
        fn blocks(range: std::ops::Range<u64>, base: i16) -> Vec<String> {
            range
                .map(|sequence| (base + sequence as i16).to_string())
                .collect()
        }
        fn interrupted() -> serde_json::Value {
            json!({"serverContent":{"interrupted":true}})
        }

        #[tokio::test]
        async fn a_resumed_question_is_kept_until_the_first_answer_begins_and_exactly_it_is_delivered()
         {
            resumed_question_kept_and_delivered(Reporting::Legacy).await;
        }

        /// The same through the contract the command line selects.
        #[tokio::test]
        async fn a_resumed_question_is_kept_and_delivered_through_the_coordinated_contract() {
            resumed_question_kept_and_delivered(Reporting::Coordinated).await;
        }

        async fn resumed_question_kept_and_delivered(reporting: Reporting) {
            let mut rig = LiveRig::start_kept_reporting(reporting);
            let mut dial = rig.kept_ready(reporting).await;
            let cut = rig.speak(&mut dial, false, true).await;
            let resumed = rig.begin(&dial, AdmittedInput::Interruption).await;
            assert!(resumed > cut);
            // A third of a second of speech. None of it reaches the service.
            rig.say(resumed, 1..31, 0).await;
            assert!(rig.heard(&mut dial, QUIET).await.is_empty());
            // The first answer begins, and goes on a little.
            dial.service.send(provider_audio(7, 240)).await;
            dial.service.send(provider_audio(7, 240)).await;
            let mut expected = vec!["start".to_owned()];
            expected.extend(blocks(1..31, 0));
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
            // Interrupted while producing, the service confirms.
            dial.service.send(interrupted()).await;
            dial.service.send(idle_complete()).await;
            // The person is still speaking: straight through now, then End.
            rig.say(resumed, 31..41, 0).await;
            rig.finish(resumed);
            let mut expected = blocks(31..41, 0);
            expected.push("end".to_owned());
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
            // Nothing of the first answer was ever the coordinator's, and
            // the next audio is the resumed request's.
            dial.service.send(provider_audio(8, 240)).await;
            assert!(matches!(
                rig.answer().await,
                ProviderOutput::Audio { request, samples, .. } if request == resumed && samples == [8; 240]
            ));
            assert!(!rig.worker.is_finished());
            rig.stop().await;
        }

        #[tokio::test]
        async fn two_resumed_utterances_are_delivered_in_order_as_one_question_with_one_end() {
            let mut rig = LiveRig::start_kept();
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let second = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(second, 1..11, 0).await;
            rig.finish(second);
            // They go on again before anything has been answered.
            let third = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(third, 1..11, 100).await;
            assert!(rig.heard(&mut dial, QUIET).await.is_empty());
            dial.service.send(provider_audio(7, 240)).await;
            let mut expected = vec!["start".to_owned()];
            expected.extend(blocks(1..11, 0));
            expected.extend(blocks(1..11, 100));
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
            dial.service.send(interrupted()).await;
            dial.service.send(idle_complete()).await;
            rig.finish(third);
            // One End: the second request was never committed by itself.
            assert_eq!(rig.heard(&mut dial, QUIET).await, ["end"]);
            dial.service.send(provider_audio(9, 240)).await;
            assert!(matches!(
                rig.answer().await,
                ProviderOutput::Audio { request, .. } if request == third
            ));
            rig.stop().await;
        }

        #[tokio::test]
        async fn a_first_answer_that_completes_with_nothing_lets_the_resumed_question_start() {
            let mut rig = LiveRig::start_kept();
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let resumed = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(resumed, 1..11, 0).await;
            rig.finish(resumed);
            assert!(rig.heard(&mut dial, QUIET).await.is_empty());
            dial.service.send(idle_complete()).await;
            let mut expected = vec!["start".to_owned()];
            expected.extend(blocks(1..11, 0));
            expected.push("end".to_owned());
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
            dial.service.send(provider_audio(9, 240)).await;
            assert!(matches!(
                rig.answer().await,
                ProviderOutput::Audio { request, .. } if request == resumed
            ));
            rig.stop().await;
        }

        #[tokio::test]
        async fn a_stop_or_a_mute_while_the_question_is_kept_writes_none_of_it() {
            // Under both contracts: the second is the command line's.
            for (reporting, mute) in [
                (Reporting::Legacy, false),
                (Reporting::Legacy, true),
                (Reporting::Coordinated, false),
                (Reporting::Coordinated, true),
            ] {
                let mut rig = LiveRig::start_kept_reporting(reporting);
                let mut dial = rig.kept_ready(reporting).await;
                rig.speak(&mut dial, false, true).await;
                let resumed = rig.begin(&dial, AdmittedInput::Interruption).await;
                rig.say(resumed, 1..21, 0).await;
                assert!(rig.heard(&mut dial, QUIET).await.is_empty());
                if mute {
                    rig.owner
                        .set_microphone_permission(real_time(), Permission::Denied)
                        .unwrap();
                    rig.snapshot = rig.owner.snapshot(real_time()).unwrap();
                    rig.parent
                        .control
                        .send(Control::Authority {
                            snapshot: rig.snapshot,
                        })
                        .unwrap();
                } else {
                    rig.parent.control.send(Control::Stop).unwrap();
                }
                // The worker ends by itself. No further heartbeat: a muted
                // owner has no lease to renew.
                let started = Instant::now();
                while !rig.worker.is_finished() {
                    assert!(started.elapsed() < Duration::from_secs(3), "worker hung");
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                (&mut rig.worker).await.unwrap().unwrap();
                // Kept, then dropped: not a start, not a block.
                assert!(
                    dial.service
                        .try_receive(Duration::from_millis(50))
                        .await
                        .is_none(),
                    "{reporting:?} mute={mute}"
                );
            }
        }

        #[tokio::test]
        async fn a_stop_while_the_kept_question_is_being_written_ends_the_writing() {
            let mut rig = LiveRig::start_kept();
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let resumed = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(resumed, 1..151, 0).await;
            assert!(rig.heard(&mut dial, QUIET).await.is_empty());
            dial.service.send(provider_audio(7, 240)).await;
            let first = rig.service_message(&mut dial).await;
            assert!(first["realtimeInput"].get("activityStart").is_some());
            rig.parent.control.send(Control::Stop).unwrap();
            rig.exit().await.0.unwrap();
            let mut written = 0;
            while let Some(message) = dial.service.try_receive(Duration::from_millis(50)).await {
                assert!(sent_audio(&message).is_some(), "{message}");
                written += 1;
            }
            assert!(written < 150, "all of it was written after the stop");
        }

        #[tokio::test]
        async fn the_service_closing_while_the_question_is_kept_writes_none_of_it_and_replays_none()
        {
            let mut rig = LiveRig::start_kept();
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let resumed = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(resumed, 1..11, 0).await;
            assert!(rig.heard(&mut dial, QUIET).await.is_empty());
            dial.service.close(1008, "quota exceeded").await;
            // Whatever the worker does next about the lost connection, the
            // kept question was never written to it, and nothing of it is
            // written to any connection that replaces it.
            let deadline = Instant::now() + Duration::from_millis(600);
            while Instant::now() < deadline && !rig.worker.is_finished() {
                let _ = rig.heartbeat();
                if let Ok(mut redial) = rig.dials.try_recv() {
                    while let Some(message) =
                        redial.service.try_receive(Duration::from_millis(100)).await
                    {
                        assert!(
                            message.get("realtimeInput").is_none(),
                            "replayed to a new connection: {message}"
                        );
                    }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }

        #[tokio::test]
        async fn with_no_first_answer_the_patience_ends_in_the_strict_timeout_and_no_end() {
            let mut rig = LiveRig::start_kept();
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let resumed = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(resumed, 1..21, 0).await;
            rig.finish(resumed);
            assert!(rig.heard(&mut dial, QUIET).await.is_empty());
            // Nothing from the service at all. After the patience the start
            // and the kept question are written anyway.
            let kept_since = Instant::now();
            let mut expected = vec!["start".to_owned()];
            expected.extend(blocks(1..21, 0));
            let mut wire = Vec::new();
            while wire.len() < expected.len() {
                assert!(kept_since.elapsed() < Duration::from_secs(4), "{wire:?}");
                wire.extend(rig.heard(&mut dial, QUIET).await);
            }
            assert_eq!(wire, expected);
            assert!(kept_since.elapsed() >= KEPT_START_PATIENCE - Duration::from_millis(400));
            // And then the strict wait, which nothing satisfies.
            let (result, _) = {
                let started = Instant::now();
                while !rig.worker.is_finished() {
                    assert!(started.elapsed() < Duration::from_secs(4), "worker hung");
                    let _ = rig.heartbeat();
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                ((&mut rig.worker).await.unwrap(), started.elapsed())
            };
            let error = result.unwrap_err().to_string();
            assert!(error.contains("BarrierTimeout"), "{error}");
            assert!(
                dial.service
                    .try_receive(Duration::from_millis(20))
                    .await
                    .is_none(),
                "the question was committed without a confirmation"
            );
        }

        /// A request given up while its opening is kept, with no newer
        /// owner: abandoned, not continued. The worker sees that in the
        /// authority it already receives and tells the transport, which
        /// drops what it kept. None of it reaches the service, and the next
        /// question is heard alone.
        #[tokio::test]
        async fn an_abandoned_kept_question_is_dropped_unsent_and_the_next_one_is_heard_alone() {
            let mut rig = LiveRig::start_kept();
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let abandoned = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(abandoned, 1..11, 0).await;
            // The coordinator gives the turn up: no owner follows it.
            let owner = rig.snapshot.owner().unwrap();
            rig.owner.cancel_turn(real_time(), owner).unwrap();
            // The typed outcome: ten blocks dropped unsent, none sent.
            assert!(matches!(
                rig.answer().await,
                ProviderOutput::InputDiscarded { request, dropped_samples: 1_600, sent_samples: 0 }
                    if request == abandoned
            ));
            assert!(rig.heard(&mut dial, QUIET).await.is_empty());
            // The first answer begins and is interrupted: the start alone.
            dial.service.send(provider_audio(7, 240)).await;
            assert_eq!(rig.heard(&mut dial, QUIET).await, ["start"]);
            dial.service.send(interrupted()).await;
            dial.service.send(idle_complete()).await;
            assert!(
                rig.heard(&mut dial, Duration::from_millis(150))
                    .await
                    .is_empty()
            );
            // The next question continues an activity that holds nothing.
            let next = rig.begin(&dial, AdmittedInput::NewTurn).await;
            rig.say(next, 1..6, 200).await;
            rig.finish(next);
            let mut expected = blocks(1..6, 200);
            expected.push("end".to_owned());
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
            dial.service.send(provider_audio(9, 240)).await;
            assert!(matches!(
                rig.answer().await,
                ProviderOutput::Audio { request, .. } if request == next
            ));
            rig.stop().await;
        }

        /// The boundary. Abandoned after the kept words were written, there
        /// is nothing left to drop: they are in an activity with no End,
        /// and the outcome says sent, not discarded. The next question is
        /// heard after them, under its own End.
        #[tokio::test]
        async fn an_abandonment_after_the_words_were_written_is_reported_as_sent_not_discarded() {
            let mut rig = LiveRig::start_kept();
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let abandoned = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(abandoned, 1..11, 0).await;
            dial.service.send(provider_audio(7, 240)).await;
            let mut expected = vec!["start".to_owned()];
            expected.extend(blocks(1..11, 0));
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
            let owner = rig.snapshot.owner().unwrap();
            rig.owner.cancel_turn(real_time(), owner).unwrap();
            assert!(matches!(
                rig.answer().await,
                ProviderOutput::InputDiscarded { request, dropped_samples: 0, sent_samples: 1_600 }
                    if request == abandoned
            ));
            dial.service.send(interrupted()).await;
            dial.service.send(idle_complete()).await;
            // Never committed by itself.
            assert!(
                rig.heard(&mut dial, Duration::from_millis(150))
                    .await
                    .is_empty()
            );
            let next = rig.begin(&dial, AdmittedInput::NewTurn).await;
            rig.say(next, 1..6, 200).await;
            rig.finish(next);
            let mut expected = blocks(1..6, 200);
            expected.push("end".to_owned());
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
            rig.stop().await;
        }

        #[tokio::test]
        async fn without_the_option_a_resumed_question_is_written_at_once_as_before() {
            let mut rig = LiveRig::start(quick_policy());
            let mut dial = rig.ready().await;
            rig.speak(&mut dial, false, true).await;
            let resumed = rig.begin(&dial, AdmittedInput::Interruption).await;
            rig.say(resumed, 1..4, 0).await;
            let mut expected = vec!["start".to_owned()];
            expected.extend(blocks(1..4, 0));
            assert_eq!(rig.heard(&mut dial, QUIET).await, expected);
        }
    }
}
