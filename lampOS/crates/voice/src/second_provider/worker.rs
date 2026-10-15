//! The second route as a provider worker, on the ordinary provider IPC:
//! the fixture's startup readiness, authority guard and credited output
//! (`crate::fixture_provider`), around [`Engine`] instead of a recording.
use super::{Engine, output_request};
use crate::{
    provider_flow::OutputSender,
    provider_link::{LinkContext, ProviderLink, ProviderLinkState},
    provider_worker::{ProviderInput, ProviderOutput},
    transport::{Channel, WorkerChannels},
    wire::{Control, WorkerEvent},
};
use lamp_ipc::monotonic_us;
use lamp_turn_owner::{BootId, BoundaryGuard, MonoTime, Snapshot};
use std::{io, time::Duration};

/// The coordinator renews authority at least this often.
const CONTROL_LEASE_US: u64 = 250_000;
/// Control messages taken in one tick before input and output wait.
const CONTROL_SLICE: usize = 16;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// The fixture's authority bookkeeping: who owns the turn, which requests
/// are over, and whether the controller is still there.
struct Gate {
    guard: BoundaryGuard,
    flow: OutputSender,
    last_control_us: u64,
    highest_seen: u64,
    retired_through: u64,
}

enum Drain {
    Empty,
    Deferred,
    Stopped,
}

impl Gate {
    fn new(boot: BootId, now_us: u64) -> Self {
        Self {
            guard: BoundaryGuard::new(boot, MonoTime::from_micros(now_us)),
            flow: OutputSender::default(),
            last_control_us: now_us,
            highest_seen: 0,
            retired_through: 0,
        }
    }

    /// As the fixture's `install_inner`: a newer owner retires every
    /// earlier request, no owner retires all seen, and an old owner coming
    /// back is a fault. The engine drops the retired requests' work.
    fn install(&mut self, now_us: u64, state: Snapshot, engine: &mut Engine) -> io::Result<()> {
        self.fresh(now_us)?;
        self.guard
            .install(MonoTime::from_micros(now_us), state)
            .map_err(io::Error::other)?;
        self.last_control_us = now_us;
        if let Some(owner) = state.owner() {
            if owner.turn() <= self.retired_through || owner.turn() < self.highest_seen {
                return Err(invalid(
                    "second route authority resurrected a retired owner",
                ));
            }
            self.highest_seen = owner.turn();
            self.retired_through = self.retired_through.max(owner.turn() - 1);
        } else {
            self.retired_through = self.retired_through.max(self.highest_seen);
        }
        engine.retire_through(self.retired_through);
        Ok(())
    }

    /// As the fixture's `fresh`, less its finite-run clock rules.
    fn fresh(&self, now_us: u64) -> io::Result<()> {
        if now_us.saturating_sub(self.last_control_us) >= CONTROL_LEASE_US {
            return Err(invalid("second route controller heartbeat expired"));
        }
        if let Some(state) = self.guard.state()
            && (now_us >= state.expires_at().as_micros()
                || (state.owner().is_some()
                    && !state.listening_ready(MonoTime::from_micros(now_us))))
        {
            return Err(invalid("second route privacy or input lease expired"));
        }
        Ok(())
    }

    /// Input is taken only for the admitted owner, under the current
    /// microphone generation. Input for a retired request is dropped.
    fn admits(&self, now_us: u64, command: &ProviderInput) -> io::Result<bool> {
        let (request, privacy) = match command {
            ProviderInput::Start {
                request,
                privacy_generation,
                ..
            }
            | ProviderInput::Audio {
                request,
                privacy_generation,
                ..
            }
            | ProviderInput::End {
                request,
                privacy_generation,
                ..
            }
            | ProviderInput::Written {
                request,
                privacy_generation,
                ..
            }
            | ProviderInput::Reminder {
                request,
                privacy_generation,
                ..
            } => (*request, *privacy_generation),
        };
        let state = self
            .guard
            .state()
            .ok_or_else(|| invalid("second route has no authority"))?;
        if request == 0
            || !state.listening_ready(MonoTime::from_micros(now_us))
            || privacy != state.microphone_generation()
        {
            return Err(invalid(
                "second route input lost privacy or retention authority",
            ));
        }
        if request <= self.retired_through {
            return Ok(false);
        }
        if state.owner().map(|owner| owner.turn()) != Some(request) {
            return Err(invalid("second route input has no matching admitted owner"));
        }
        Ok(true)
    }

    /// Output of a request goes only to its owner while input is retained.
    fn may_send(&self, now_us: u64, event: &ProviderOutput) -> io::Result<()> {
        let Some(request) = output_request(event) else {
            return Ok(());
        };
        let state = self
            .guard
            .state()
            .ok_or_else(|| invalid("second route output has no authority"))?;
        if !state.listening_ready(MonoTime::from_micros(now_us))
            || state.owner().map(|owner| owner.turn()) != Some(request)
        {
            return Err(invalid("second route output lost original owner"));
        }
        Ok(())
    }

    fn drain(&mut self, control: &mut Channel, engine: &mut Engine) -> io::Result<Drain> {
        for _ in 0..CONTROL_SLICE {
            match control.receive()? {
                Some(Control::Stop) => return Ok(Drain::Stopped),
                Some(Control::Authority { snapshot }) => {
                    self.install(monotonic_us(), snapshot, engine)?
                }
                Some(Control::ProviderOutputCapacity { through }) => self.flow.grant(through)?,
                Some(Control::StartCapture | Control::ConnectReference { .. }) => {
                    return Err(invalid("invalid second route provider control"));
                }
                None => return Ok(Drain::Empty),
            }
        }
        Ok(Drain::Deferred)
    }
}

/// Run the engine as a provider worker until Stop. Readiness is the
/// fixture's: `Ready`, then one `ProviderLink` Ready on the control plane.
/// ponytail: the services are not probed first; the link says Ready and a
/// service that cannot be reached fails that turn. A probe at startup if a
/// dead key should show as an unavailable link.
pub async fn run(mut channels: WorkerChannels, boot: BootId, mut engine: Engine) -> io::Result<()> {
    let started_us = monotonic_us();
    channels.control.send(WorkerEvent::Ready)?;
    let ready = ProviderLink {
        revision: 1,
        session: 1,
        observed_at_us: started_us,
        source_clock: None,
        state: ProviderLinkState::Ready {
            setup_us: monotonic_us() - started_us,
            attempts: 1,
            context: LinkContext::Initial,
            outage_started_at_us: None,
            after: None,
        },
    };
    ready.validate(monotonic_us())?;
    channels
        .control
        .send(WorkerEvent::ProviderLink { link: ready })?;
    let mut gate = Gate::new(boot, monotonic_us());
    let mut ticker = tokio::time::interval(Duration::from_millis(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // At most one input read ahead of its authority, and one output waiting
    // for credit or a full socket.
    let mut retained: Option<ProviderInput> = None;
    let mut pending: Option<ProviderOutput> = None;
    loop {
        // The turn tasks run while this waits.
        ticker.tick().await;
        gate.fresh(monotonic_us())?;
        match gate.drain(&mut channels.control, &mut engine)? {
            Drain::Stopped => return Ok(()),
            Drain::Deferred => continue,
            Drain::Empty => {}
        }
        if retained.is_none() {
            retained = channels.data.receive()?;
        }
        // The parent sends authority before its dependent input, possibly
        // after the first empty read: drain again before judging the input.
        match gate.drain(&mut channels.control, &mut engine)? {
            Drain::Stopped => return Ok(()),
            Drain::Deferred => continue,
            Drain::Empty => {}
        }
        if let Some(command) = retained.take()
            && gate.admits(monotonic_us(), &command)?
        {
            engine.submit(command);
        }
        // Output kept waiting for a request since retired is dropped.
        if pending
            .as_ref()
            .and_then(output_request)
            .is_some_and(|request| request <= gate.retired_through)
        {
            pending = None;
        }
        if pending.is_none() {
            pending = engine.try_output();
        }
        if let Some(event) = pending.take() {
            gate.may_send(monotonic_us(), &event)?;
            if !gate.flow.try_send(|| channels.data.send(&event))? {
                pending = Some(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::second_provider::{Listener, Services, net};
    use futures_util::{
        future::BoxFuture,
        stream::{self, BoxStream},
    };
    use lamp_turn_owner::{AdmissionState, AdmittedInput, CaptureState, Controller, Permission};
    use std::sync::Arc;

    /// Services that never answer: only authority is under test.
    struct Silent;
    impl Services for Silent {
        fn listen(&self) -> BoxFuture<'static, net::Result<Box<dyn Listener>>> {
            Box::pin(std::future::pending())
        }
        fn chat(&self, _: Vec<net::Message>) -> BoxStream<'static, net::Result<net::ChatEvent>> {
            Box::pin(stream::pending())
        }
        fn speak(&self, _: String) -> BoxStream<'static, net::Result<Vec<i16>>> {
            Box::pin(stream::pending())
        }
    }

    #[tokio::test]
    async fn a_new_owner_retires_the_old_turn_and_its_late_input() {
        let now_us = 10_000;
        let at = MonoTime::from_micros(now_us);
        let boot = BootId::new([7; 16]).unwrap();
        let mut owner = Controller::new(boot, at);
        owner
            .set_microphone_permission(at, Permission::Allowed)
            .unwrap();
        let until = MonoTime::from_micros(now_us + 100_000);
        owner
            .set_capture(at, CaptureState::RetainingUntil(until))
            .unwrap();
        owner
            .set_admission(at, AdmissionState::OpenUntil(until))
            .unwrap();
        let mut engine = Engine::new(
            Arc::new(Silent),
            String::new,
            Arc::new(|_, _| Box::pin(async { serde_json::Value::Null })),
        );
        let mut gate = Gate::new(boot, now_us);
        let first = owner.admit(at, AdmittedInput::NewTurn).unwrap();
        let state = owner.snapshot(at).unwrap();
        gate.install(now_us, state, &mut engine).unwrap();
        let start = |request| ProviderInput::Start {
            request,
            session: 1,
            privacy_generation: state.microphone_generation(),
            continues: false,
        };
        assert!(gate.admits(now_us, &start(first.turn())).unwrap());
        engine.submit(start(first.turn()));
        assert!(gate.may_send(now_us, &engine.try_output().unwrap()).is_ok());
        assert!(engine.turn.is_some());

        let second = owner.admit(at, AdmittedInput::Interruption).unwrap();
        gate.install(now_us, owner.snapshot(at).unwrap(), &mut engine)
            .unwrap();
        assert!(engine.turn.is_none());
        assert!(!gate.admits(now_us, &start(first.turn())).unwrap());
        assert!(gate.admits(now_us, &start(second.turn())).unwrap());
        // Output of the old owner would be refused at the boundary.
        let late = ProviderOutput::GenerationComplete {
            request: first.turn(),
        };
        assert!(gate.may_send(now_us, &late).is_err());
    }
}
