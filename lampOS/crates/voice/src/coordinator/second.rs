//! The second voice route beside the Live one (lampOS#754).
//!
//! With `LAMP_VOICE_SECOND_CONFIG` set, the service starts the `second`
//! worker as a standby next to Gemini's. Both speak the same provider wire,
//! so a switch is a swap of the worker, its link register and its output
//! credit with the active ones: every path written for `rig.provider` and
//! `cloud` then runs unchanged against the other route. The standby's
//! control goes to its own register; its output is drained, credited and
//! dropped, which is how a late answer of the route switched away from is
//! never heard.
//!
//! Switching:
//! - At rest, to the second route while Gemini's link is not Ready (and has
//!   not ended for good: Gemini's end still ends the service), and back to
//!   Gemini, silently, on the first idle tick its link is Ready again.
//! - Mid-turn, when Gemini fails the turn before any of its answer was
//!   heard, or its link changes during the line: the line is replayed to
//!   the second route from [`Line`], a copy of what was sent for it.
//!
//! [`Line`] is held in memory only, never on disk or the bus, bounded at
//! 30 s of 16 kHz mono, and cleared when the line is answered or abandoned
//! and at once when an authority without its owner or with the microphone
//! closed is published (mute, privacy, cancellation). Without a configured
//! route `Rig::second` is `None` and every function here is the old call.
use super::*;
use std::ffi::OsString;

/// 30 s of 16 kHz mono microphone audio.
const MAX_LINE_SAMPLES: usize = 30 * 16_000;
/// Standby control and output packets taken per coordinator tick.
const STANDBY_SLICE: usize = 16;

/// What was sent to the active Live route for the line in hand.
#[derive(Default)]
pub(super) struct Line {
    request: u64,
    commands: Vec<ProviderInput>,
    samples: usize,
    /// Every command of `request` since its start is in `commands`.
    complete: bool,
}

impl Line {
    fn keep(&mut self, command: &ProviderInput) {
        let request = request_of(command);
        if matches!(
            command,
            ProviderInput::Start { .. }
                | ProviderInput::Written { .. }
                | ProviderInput::Reminder { .. }
        ) {
            self.clear();
            self.request = request;
            self.complete = true;
        } else if request != self.request || !self.complete {
            return;
        }
        if let ProviderInput::Audio { samples, .. } = command {
            self.samples += samples.len();
            if self.samples > MAX_LINE_SAMPLES {
                // Too long to replay: kept no more, and not replayed.
                self.clear();
                return;
            }
        }
        self.commands.push(command.clone());
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn covers(&self, request: u64) -> bool {
        self.complete && self.request == request
    }
}

fn request_of(command: &ProviderInput) -> u64 {
    match command {
        ProviderInput::Start { request, .. }
        | ProviderInput::Audio { request, .. }
        | ProviderInput::End { request, .. }
        | ProviderInput::Written { request, .. }
        | ProviderInput::Reminder { request, .. } => *request,
    }
}

pub(super) struct Route {
    /// The standby: Gemini's worker while the second route speaks, the
    /// second route's otherwise.
    worker: Worker,
    cloud: CloudAvailability,
    flow: OutputReceiver,
    /// `rig.provider` is the second route's worker.
    second_active: bool,
    line: Line,
    /// A line being replayed to the second route, sent before anything
    /// queued after it.
    replay: VecDeque<ProviderInput>,
}

/// The configuration's path when the variable names one.
pub(super) fn configured(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|path| !path.is_empty()).map(PathBuf::from)
}

/// The `worker second` arguments: the Live worker's, its configuration
/// first, then the second route's, then its flags but `--keep-link`.
pub(super) fn arguments(provider: &[String], second_config: &Path) -> io::Result<Vec<String>> {
    let (config, flags) = provider
        .split_first()
        .ok_or_else(|| io::Error::other("provider arguments lack a configuration"))?;
    let second_config = second_config
        .to_str()
        .ok_or_else(|| io::Error::other("second route configuration path must be UTF-8"))?;
    let mut arguments = vec![config.clone(), second_config.to_owned()];
    arguments.extend(
        flags
            .iter()
            .filter(|flag| *flag != crate::provider_worker::KEEP_LINK_FLAG)
            .cloned(),
    );
    Ok(arguments)
}

impl Route {
    pub(super) fn new(worker: Worker) -> Self {
        Self {
            worker,
            cloud: CloudAvailability::default(),
            flow: OutputReceiver::default(),
            second_active: false,
            line: Line::default(),
            replay: VecDeque::new(),
        }
    }

    /// A standby already past its startup, for the coordinator's tests.
    #[cfg(test)]
    pub(super) fn standing_by(worker: Worker, link: ProviderLink) -> Self {
        let mut route = Self::new(worker);
        route
            .cloud
            .observe(link, monotonic_us())
            .expect("valid standby link");
        route
    }

    /// Start the standby when the Live service is configured for one.
    pub(super) fn spawn(
        executable: &Path,
        directory: &Path,
        boot: BootId,
        arguments: Option<&[String]>,
        on_wait: impl FnMut() -> io::Result<()>,
    ) -> io::Result<Option<Self>> {
        let Some(arguments) = arguments else {
            return Ok(None);
        };
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        Worker::spawn_with_tick(executable, directory, "second", boot, &arguments, on_wait)
            .map(|worker| Some(Self::new(worker)))
    }

    pub(super) fn worker_mut(&mut self) -> &mut Worker {
        &mut self.worker
    }

    /// During the remaining startup: keep the standby's authority fresh and
    /// take its readiness before its packets age.
    pub(super) fn startup_tick(
        &mut self,
        snapshot: Option<Snapshot>,
        startup: &mut StartupEvents,
    ) -> io::Result<()> {
        if let Some(snapshot) = snapshot {
            self.publish(snapshot)?;
        }
        for _ in 0..STANDBY_SLICE {
            match self.worker.channels.control.receive::<WorkerEvent>()? {
                Some(WorkerEvent::ProviderLink { link }) => {
                    let at = monotonic_us();
                    self.cloud.observe(link, at)?;
                    let mut event = link_trace(link, at);
                    event["route"] = json!("second");
                    startup.push(event)?;
                }
                Some(WorkerEvent::Fault { code }) => return Err(io::Error::other(code)),
                Some(_) => {
                    return Err(io::Error::other(
                        "unexpected second route event before activation",
                    ));
                }
                None => break,
            }
        }
        if self
            .worker
            .channels
            .data
            .receive::<ProviderOutput>()?
            .is_some()
        {
            return Err(io::Error::other("second route output before input"));
        }
        self.flow.replenish(0, &mut self.worker.channels.control)
    }

    /// Authority to the standby. A line is the current owner's alone: it
    /// goes the moment an authority without that owner, or with the
    /// microphone closed, is published.
    pub(super) fn publish(&mut self, snapshot: Snapshot) -> io::Result<()> {
        if snapshot.microphone_permission() != Permission::Allowed
            || snapshot.owner().map(|owner| owner.turn()) != Some(self.line.request)
        {
            self.line.clear();
        }
        self.worker
            .channels
            .control
            .send(Control::Authority { snapshot })
    }

    /// The input in hand can be taken over: by the second route from
    /// Gemini, before any answer was heard, with every sent command kept.
    fn can_take(&self, current: &Reply) -> bool {
        !self.second_active
            && self.cloud.available()
            && !current.had_audio
            && current.pcm.is_empty()
            && current.reminder.is_none()
            && current.cloud.outcome.is_none()
            && (!current.cloud.start_submitted || self.line.covers(current.owner.turn()))
    }

    /// A line goes once it is answered or abandoned, and a replay with
    /// the request it was for.
    fn upkeep(&mut self, reply: Option<&Reply>) {
        let turn = reply.map(|current| current.owner.turn());
        if reply.is_none_or(|current| {
            current.had_audio || current.cloud.outcome.is_some() || current.cloud.input_abandoned
        }) || turn != Some(self.line.request)
        {
            self.line.clear();
        }
        if self
            .replay
            .front()
            .is_some_and(|command| Some(request_of(command)) != turn)
        {
            self.replay.clear();
        }
    }

    /// The standby's priority link reports and output: kept and dropped.
    fn drain(&mut self, trace: &mut SessionState) -> Result<()> {
        let mut budget = STANDBY_SLICE;
        service_provider_control(
            &mut self.worker.channels.control,
            &mut self.cloud,
            &mut budget,
            trace,
        )?;
        for _ in 0..STANDBY_SLICE {
            let Some(event) = self.worker.channels.data.receive::<ProviderOutput>()? else {
                break;
            };
            self.flow.received()?;
            if let ProviderOutput::Ended {
                reason,
                fault,
                attempts,
            } = event
            {
                self.cloud.end_output()?;
                record(
                    trace,
                    json!({"kind":"provider_output_ended","at_us":monotonic_us(),
                    "reason":reason,"fault":fault,"attempts":attempts,"standby":true}),
                )?;
            }
        }
        self.flow
            .replenish(0, &mut self.worker.channels.control)
            .map_err(Into::into)
    }
}

fn swap(
    provider: &mut Worker,
    cloud: &mut CloudAvailability,
    flow: &mut OutputReceiver,
    route: &mut Route,
) {
    std::mem::swap(provider, &mut route.worker);
    std::mem::swap(cloud, &mut route.cloud);
    std::mem::swap(flow, &mut route.flow);
    route.second_active = !route.second_active;
}

/// Swap to the second route and give it the line in hand: its start (with
/// the second route's session), everything sent and everything queued.
fn take_over(
    provider: &mut Worker,
    cloud: &mut CloudAvailability,
    flow: &mut OutputReceiver,
    route: &mut Route,
    current: &mut Reply,
    queue: &mut VecDeque<ProviderInput>,
) -> io::Result<usize> {
    swap(provider, cloud, flow, route);
    let session = cloud
        .ready_session()
        .ok_or_else(|| io::Error::other("second route lost its link before a replay"))?;
    let turn = current.owner.turn();
    let sent = if route.line.covers(turn) {
        std::mem::take(&mut route.line.commands)
    } else {
        Vec::new()
    };
    route.line.clear();
    route.replay = sent.into();
    route.replay.extend(queue.drain(..));
    for command in &mut route.replay {
        if let ProviderInput::Start { session: s, .. }
        | ProviderInput::Written { session: s, .. }
        | ProviderInput::Reminder { session: s, .. } = command
        {
            *s = session;
        }
    }
    current.cloud = CloudRequest::new(session)?;
    Ok(route.replay.len())
}

/// Once per tick, after the first priority link service: the standby's
/// reports, the line's upkeep, a line whose link changed, and the switches
/// at rest.
pub(super) fn service(
    rig: &mut Rig,
    cloud: &mut CloudAvailability,
    flow: &mut OutputReceiver,
    reply: &mut Option<Reply>,
    queue: &mut VecDeque<ProviderInput>,
    draining: bool,
    trace: &mut SessionState,
) -> Result<()> {
    let Rig {
        provider, second, ..
    } = rig;
    let Some(route) = second.as_mut() else {
        return Ok(());
    };
    route.drain(trace)?;
    route.upkeep(reply.as_ref());
    if let Some(current) = reply.as_mut() {
        // The link changed during the line, which the gate left for this.
        let changed = current.cloud.movable
            && !current.cloud.input_end_submitted
            && !current.cloud.input_abandoned
            && current.cloud.outcome.is_none()
            && cloud.ready_session() != Some(current.cloud.session);
        if changed && route.can_take(current) {
            let commands = take_over(provider, cloud, flow, route, current, queue)?;
            record(
                trace,
                json!({"kind":"voice_route_switched","at_us":monotonic_us(),"to":"second",
                "why":"live_link_changed","turn":current.owner.turn(),"replayed_commands":commands}),
            )?;
        }
        current.cloud.movable = route.can_take(current);
    }
    let at_rest = queue.is_empty() && route.replay.is_empty() && !draining;
    if !route.second_active
        && at_rest
        && reply
            .as_ref()
            .is_none_or(|current| current.cloud.outcome.is_some())
        && !cloud.available()
        && !cloud.terminal()
        && !cloud.output_ended()
        && route.cloud.available()
    {
        swap(provider, cloud, flow, route);
        record(
            trace,
            json!({"kind":"voice_route_switched","at_us":monotonic_us(),"to":"second",
            "why":"live_link_not_ready","turn":null,"replayed_commands":0}),
        )?;
    } else if route.second_active
        && at_rest
        && reply.is_none()
        && (route.cloud.available() || route.cloud.terminal() || route.cloud.output_ended())
    {
        swap(provider, cloud, flow, route);
        record(
            trace,
            json!({"kind":"voice_route_switched","at_us":monotonic_us(),"to":"live",
            "why":if cloud.available() {"live_link_ready"} else {"live_link_ended"},
            "turn":null,"replayed_commands":0}),
        )?;
    }
    Ok(())
}

/// Before an output of the active route is taken: a turn Gemini failed
/// before any of its answer was heard goes to the second route instead.
/// True when the failure was taken over and is not to be observed.
pub(super) fn takes_failed_turn(
    rig: &mut Rig,
    cloud: &mut CloudAvailability,
    flow: &mut OutputReceiver,
    reply: &mut Option<Reply>,
    queue: &mut VecDeque<ProviderInput>,
    event: &ProviderOutput,
    trace: &mut SessionState,
) -> Result<bool> {
    let ProviderOutput::TurnFailed {
        request,
        stage,
        reason,
    } = *event
    else {
        return Ok(false);
    };
    let Rig {
        provider, second, ..
    } = rig;
    let Some(route) = second.as_mut() else {
        return Ok(false);
    };
    let Some(current) = reply
        .as_mut()
        .filter(|current| current.owner.turn() == request && current.cloud.start_submitted)
    else {
        return Ok(false);
    };
    if !route.can_take(current) {
        return Ok(false);
    }
    let commands = take_over(provider, cloud, flow, route, current, queue)?;
    record(
        trace,
        json!({"kind":"voice_route_switched","at_us":monotonic_us(),"to":"second",
        "why":"live_turn_failed","turn":request,"stage":stage,"reason":reason,
        "replayed_commands":commands}),
    )?;
    Ok(true)
}

/// The one send of queued input to the active route. While Gemini speaks,
/// what it is sent is kept as the line; a line being replayed goes first,
/// and what was queued since joins it in order.
pub(super) fn flush(
    rig: &mut Rig,
    cloud: &CloudAvailability,
    reply: &mut Option<Reply>,
    queue: &mut VecDeque<ProviderInput>,
) -> io::Result<()> {
    let Rig {
        provider, second, ..
    } = rig;
    let channel = &mut provider.channels.data;
    let Some(route) = second.as_mut() else {
        return flush_provider_input(cloud, reply, queue, channel);
    };
    // A replay goes with the request it was for (a cancellation in this
    // tick may have ended it).
    route.upkeep(reply.as_ref());
    if !route.replay.is_empty() {
        route.replay.extend(queue.drain(..));
        return flush_provider_input_with(cloud, reply, &mut route.replay, |command| {
            channel.send(command)
        });
    }
    let keep = !route.second_active;
    let line = &mut route.line;
    flush_provider_input_with(cloud, reply, queue, |command| {
        channel.send(command)?;
        if keep {
            line.keep(command);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(request: u64, sequence: u64, samples: usize) -> ProviderInput {
        ProviderInput::Audio {
            request,
            privacy_generation: 1,
            sequence,
            read_completed_at_us: 1,
            samples: vec![7; samples],
        }
    }

    fn start(request: u64) -> ProviderInput {
        ProviderInput::Start {
            request,
            session: 1,
            privacy_generation: 1,
            continues: false,
        }
    }

    #[test]
    fn a_line_is_bounded_at_thirty_seconds_and_then_not_replayed() {
        let mut line = Line::default();
        line.keep(&start(4));
        for sequence in 1..=3_000 {
            line.keep(&audio(4, sequence, 160));
        }
        assert!(line.covers(4));
        assert_eq!(line.samples, MAX_LINE_SAMPLES);
        assert_eq!(line.commands.len(), 3_001);
        line.keep(&audio(4, 3_001, 160));
        assert!(!line.covers(4));
        assert!(line.commands.is_empty() && line.commands.capacity() == 0);
        // Nothing of that request is kept again; a new line starts afresh.
        line.keep(&audio(4, 3_002, 160));
        assert!(line.commands.is_empty());
        line.keep(&start(5));
        assert!(line.covers(5));
    }

    #[test]
    fn a_line_keeps_only_its_own_request_from_its_start() {
        let mut line = Line::default();
        line.keep(&audio(3, 1, 160));
        assert!(!line.covers(3));
        line.keep(&start(3));
        line.keep(&audio(3, 1, 160));
        line.keep(&audio(2, 9, 160));
        line.keep(&ProviderInput::End {
            request: 3,
            privacy_generation: 1,
            sound: false,
        });
        assert_eq!(line.commands.len(), 3);
    }

    #[test]
    fn the_arguments_are_the_live_workers_with_both_configurations_and_no_kept_link() {
        let provider: Vec<String> = [
            "/etc/provider.json",
            "--thread-file",
            "/t",
            "--keep-link",
            "--tools",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            arguments(&provider, Path::new("/etc/second.json")).unwrap(),
            [
                "/etc/provider.json",
                "/etc/second.json",
                "--thread-file",
                "/t",
                "--tools"
            ]
        );
        assert!(configured(None).is_none());
        assert!(configured(Some(OsString::new())).is_none());
        assert_eq!(
            configured(Some("/etc/second.json".into())),
            Some(PathBuf::from("/etc/second.json"))
        );
    }

    /// Mute, privacy and cancellation all publish an authority without
    /// the line's owner or with the microphone closed: the line goes then.
    #[test]
    fn a_line_goes_at_once_when_its_owner_or_the_microphone_does() {
        let directory = SessionDirectory::create().unwrap();
        let (worker, mut peer) = Worker::sleeping_fixture(&directory.path, "second");
        let mut route = Route::new(worker);
        let (mut owner, current, _) = super::super::tests::active_reply();
        let turn = current.owner.turn();
        route.line.keep(&start(turn));
        route.publish(owner.snapshot(now()).unwrap()).unwrap();
        assert!(route.line.covers(turn));
        owner
            .set_microphone_permission(now(), Permission::Denied)
            .unwrap();
        let closed = owner.snapshot(now()).unwrap();
        route.publish(closed).unwrap();
        assert!(!route.line.covers(turn) && route.line.commands.is_empty());
        assert!(matches!(
            peer.control.receive::<Control>().unwrap(),
            Some(Control::Authority { .. })
        ));
    }

    #[test]
    fn a_line_goes_once_it_is_answered_or_abandoned() {
        let directory = SessionDirectory::create().unwrap();
        let (worker, _peer) = Worker::sleeping_fixture(&directory.path, "second");
        let mut route = Route::new(worker);
        let (_owner, mut current, _) = super::super::tests::active_reply();
        let turn = current.owner.turn();
        current.had_audio = false;
        route.line.keep(&start(turn));
        route.upkeep(Some(&current));
        assert!(route.line.covers(turn));
        // Its first answer audio.
        current.had_audio = true;
        route.upkeep(Some(&current));
        assert!(!route.line.covers(turn));
        current.had_audio = false;
        route.line.keep(&start(turn));
        current.cloud.input_abandoned = true;
        route.upkeep(Some(&current));
        assert!(!route.line.covers(turn));
        route.line.keep(&start(turn));
        route.replay.push_back(start(turn));
        route.upkeep(None);
        assert!(!route.line.covers(turn));
        // A replay cancelled with its turn is not sent to the next owner.
        assert!(route.replay.is_empty());
    }
}
