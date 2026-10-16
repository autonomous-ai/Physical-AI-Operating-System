//! In-memory scripted service for deterministic tests of this transport and
//! of the callers built on it. There is no TLS, socket, credential or cloud
//! request here, and nothing in this module is used by the runtime.
use crate::{
    Attempt, Connect, Connection, Result, ResumptionHandle, SessionConfig, SessionId,
    session::{CaptureClockSource, setup_and_spawn_with_sources, socket_config},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
#[cfg(any(test, feature = "fixture-clock"))]
use std::sync::Arc;
use std::time::Duration;
use tokio::{io::DuplexStream, sync::mpsc, time::timeout};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        protocol::{CloseFrame, Role},
    },
};

const RECEIVE_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(any(test, feature = "fixture-clock"))]
pub(crate) mod task_joins;
#[cfg(any(test, feature = "fixture-clock"))]
pub use task_joins::{
    JoinFault, JoinOutcome, JoinReport, JoinedTask, MAX_JOINED_TASKS, TaskCompletion, TaskJoins,
    TaskKind,
};

/// Production and existing fixtures keep the direct ownership path.
#[derive(Clone, Default)]
pub(crate) enum JoinSource {
    #[default]
    Plain,
    #[cfg(any(test, feature = "fixture-clock"))]
    Retained(task_joins::Registrar),
}

#[cfg(any(test, feature = "fixture-clock"))]
pub(crate) mod actor_work;
#[cfg(any(test, feature = "fixture-clock"))]
pub use actor_work::{
    ActorWork, WorkBuffers, WorkCommand, WorkCommandKind, WorkFault, WorkMaintenance, WorkReceipt,
    WorkSnapshot,
};

#[derive(Clone, Default)]
pub(crate) enum ActorWorkSource {
    #[default]
    Plain,
    #[cfg(any(test, feature = "fixture-clock"))]
    Recorded(actor_work::Recorder),
}
#[cfg(any(test, feature = "fixture-clock"))]
impl ActorWorkSource {
    pub(crate) fn install(self, session: SessionId) -> Option<actor_work::Recorder> {
        match self {
            Self::Plain => None,
            Self::Recorded(recorder) => {
                recorder.install(session);
                Some(recorder)
            }
        }
    }
}

/// Capture time supplied explicitly to an in-memory fixture connection.
///
/// Reads must return the current admitted acquisition-domain Instant promptly,
/// without advancing time, pumping tasks or sampling the Tokio clock. Acquisition
/// timestamps must use that same domain. The controller owns monotonic updates,
/// epoch validation and cancellation: invalidation must never switch to host time.
#[cfg(any(test, feature = "fixture-clock"))]
pub trait CaptureClock: Send + Sync + 'static {
    fn now(&self) -> std::time::Instant;
}

/// The server end of one scripted connection.
pub struct FakeService {
    socket: WebSocketStream<DuplexStream>,
    closed: bool,
}
impl FakeService {
    /// Next client JSON message. Panics when none arrives within two seconds.
    pub async fn receive(&mut self) -> Value {
        self.try_receive(RECEIVE_TIMEOUT)
            .await
            .expect("client message")
    }
    /// Next client JSON message, or `None` on silence, close or stream end.
    pub async fn try_receive(&mut self, wait: Duration) -> Option<Value> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            match tokio::time::timeout_at(deadline, self.socket.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    return serde_json::from_slice(text.as_bytes()).ok();
                }
                Ok(Some(Ok(Message::Binary(bytes)))) => return serde_json::from_slice(&bytes).ok(),
                // Reading a ping queues its pong; send it, as a service would.
                Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => {
                    let _ = self.socket.flush().await;
                }
                // Silence within the wait.
                Err(_) => return None,
                _ => {
                    self.closed = true;
                    return None;
                }
            }
        }
    }
    /// Let time pass while answering keepalive pings, as a live service does
    /// throughout a long answer. Returns early with a client message if one
    /// arrives.
    pub async fn rest(&mut self, wait: Duration) -> Option<Value> {
        let until = tokio::time::Instant::now() + wait;
        if !self.closed
            && let Some(message) = self.try_receive(wait).await
        {
            return Some(message);
        }
        // A closed connection has nothing to answer; time still passes.
        tokio::time::sleep_until(until).await;
        None
    }
    /// Receive the setup message and acknowledge it.
    pub async fn accept(&mut self) -> Value {
        let setup = self.receive().await;
        self.send(json!({"setupComplete": {}})).await;
        setup
    }
    pub async fn send(&mut self, value: Value) {
        self.socket
            .send(Message::text(value.to_string()))
            .await
            .expect("scripted server send");
    }
    /// Write every message before the client can read any of them: the shape
    /// of a burst released by the network after a stall.
    pub async fn burst(&mut self, values: impl IntoIterator<Item = Value>) {
        for value in values {
            self.socket
                .feed(Message::text(value.to_string()))
                .await
                .expect("scripted server feed");
        }
        self.socket.flush().await.expect("scripted server flush");
    }
    pub async fn close(&mut self, code: u16, reason: &str) {
        let _ = self
            .socket
            .close(Some(CloseFrame {
                code: code.into(),
                reason: reason.to_owned().into(),
            }))
            .await;
    }
}

/// The samples of one client `realtimeInput.audio` message, if it is one.
pub fn sent_audio(message: &Value) -> Option<Vec<i16>> {
    let data = message["realtimeInput"]["audio"]["data"].as_str()?;
    let bytes = STANDARD.decode(data).ok()?;
    Some(
        bytes
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect(),
    )
}

/// One server message carrying 24 kHz PCM, every sample equal to `value`.
pub fn audio(value: i16, samples: usize) -> Value {
    let bytes: Vec<u8> = std::iter::repeat_n(value.to_le_bytes(), samples)
        .flatten()
        .collect();
    json!({"serverContent":{"modelTurn":{"parts":[{"inlineData":{
        "mimeType":"audio/pcm;rate=24000","data":STANDARD.encode(bytes)}}]}}})
}

/// `bytes` of filler between JPEG start and end markers: enough for the
/// transport, which checks the markers and the size and decodes nothing. It
/// is not a picture.
pub fn jpeg(bytes: usize, fill: u8) -> Vec<u8> {
    assert!(bytes >= 4);
    let mut image = vec![fill; bytes];
    image[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
    image[bytes - 2..].copy_from_slice(&[0xFF, 0xD9]);
    image
}

/// A client connection attempt and the scripted service at its other end. The
/// attempt completes only after the service acknowledges setup.
pub async fn pair(
    config: SessionConfig,
    session: SessionId,
    capacity: usize,
) -> (
    impl Future<Output = Result<Connection>> + Send + 'static,
    FakeService,
) {
    pair_with_clock_source(config, session, capacity, CaptureClockSource::System).await
}

/// The same actual setup and session as [`pair`], with an explicit capture clock
/// shared by its InputSender and Actor. Protocol deadlines still use Tokio time.
#[cfg(any(test, feature = "fixture-clock"))]
pub async fn pair_with_capture_clock(
    config: SessionConfig,
    session: SessionId,
    capacity: usize,
    clock: Arc<dyn CaptureClock>,
) -> (
    impl Future<Output = Result<Connection>> + Send + 'static,
    FakeService,
) {
    pair_with_clock_source(
        config,
        session,
        capacity,
        CaptureClockSource::Fixture(clock),
    )
    .await
}

/// One real fixture connection with capture time and historical Actor work
/// observations. Installation precedes InputSender publication and Actor spawn.
/// These receipts do not grant clock advancement or prove transport, service,
/// output-consumer or task completion. Existing factories remain unobserved.
#[cfg(any(test, feature = "fixture-clock"))]
pub async fn pair_with_capture_clock_and_work(
    config: SessionConfig,
    session: SessionId,
    capacity: usize,
    clock: Arc<dyn CaptureClock>,
) -> (
    impl Future<Output = Result<Connection>> + Send + 'static,
    FakeService,
    ActorWork,
) {
    let (work, recorder) = ActorWork::new(session);
    let (client, server) = tokio::io::duplex(capacity);
    let client =
        WebSocketStream::from_raw_socket(client, Role::Client, Some(socket_config())).await;
    let server =
        WebSocketStream::from_raw_socket(server, Role::Server, Some(socket_config())).await;
    (
        crate::session::setup_and_spawn_with_work_source(
            client,
            config,
            session,
            CaptureClockSource::Fixture(clock),
            JoinSource::Plain,
            ActorWorkSource::Recorded(recorder),
        ),
        FakeService {
            socket: server,
            closed: false,
        },
        work,
    )
}

async fn pair_with_clock_source(
    config: SessionConfig,
    session: SessionId,
    capacity: usize,
    capture_clock: CaptureClockSource,
) -> (
    impl Future<Output = Result<Connection>> + Send + 'static,
    FakeService,
) {
    pair_with_sources(config, session, capacity, capture_clock, JoinSource::Plain).await
}

async fn pair_with_sources(
    config: SessionConfig,
    session: SessionId,
    capacity: usize,
    capture_clock: CaptureClockSource,
    joins: JoinSource,
) -> (
    impl Future<Output = Result<Connection>> + Send + 'static,
    FakeService,
) {
    let (client, server) = tokio::io::duplex(capacity);
    let client =
        WebSocketStream::from_raw_socket(client, Role::Client, Some(socket_config())).await;
    let server =
        WebSocketStream::from_raw_socket(server, Role::Server, Some(socket_config())).await;
    (
        setup_and_spawn_with_sources(client, config, session, capture_clock, joins),
        FakeService {
            socket: server,
            closed: false,
        },
    )
}

/// A dial observed by a test: which session, whether a resumption handle was
/// presented, and the service that will answer it.
pub struct Dial {
    pub session: SessionId,
    pub resumed: bool,
    pub service: FakeService,
}

/// Connector whose every attempt is handed to the test as a [`Dial`]. Dropping
/// the service without accepting fails that attempt.
pub struct FakeConnector {
    config: SessionConfig,
    dials: mpsc::UnboundedSender<Dial>,
    capacity: usize,
    capture_clock: CaptureClockSource,
    joins: JoinSource,
}
pub fn connector(config: SessionConfig) -> (FakeConnector, mpsc::UnboundedReceiver<Dial>) {
    connector_with_capacity(config, crate::MAX_WIRE_BYTES * 2)
}
/// The same, with this many bytes between client and service: a service
/// that stops reading then blocks the client's writes almost at once.
pub fn connector_with_capacity(
    config: SessionConfig,
    capacity: usize,
) -> (FakeConnector, mpsc::UnboundedReceiver<Dial>) {
    connector_with_clock_source(config, capacity, CaptureClockSource::System)
}

/// Scripted connection attempts sharing one explicit capture clock, including
/// reconnects. Existing connector factories always retain the system clock.
#[cfg(any(test, feature = "fixture-clock"))]
pub fn connector_with_capture_clock(
    config: SessionConfig,
    capacity: usize,
    clock: Arc<dyn CaptureClock>,
) -> (FakeConnector, mpsc::UnboundedReceiver<Dial>) {
    connector_with_clock_source(config, capacity, CaptureClockSource::Fixture(clock))
}

fn connector_with_clock_source(
    config: SessionConfig,
    capacity: usize,
    capture_clock: CaptureClockSource,
) -> (FakeConnector, mpsc::UnboundedReceiver<Dial>) {
    connector_with_sources(config, capacity, capture_clock, JoinSource::Plain)
}

fn connector_with_sources(
    config: SessionConfig,
    capacity: usize,
    capture_clock: CaptureClockSource,
    joins: JoinSource,
) -> (FakeConnector, mpsc::UnboundedReceiver<Dial>) {
    let (dials, observed) = mpsc::unbounded_channel();
    (
        FakeConnector {
            config,
            dials,
            capacity,
            capture_clock,
            joins,
        },
        observed,
    )
}
/// In-memory supervisor with explicit capture time and actual task joins.
///
/// Install before the first attempt spawns. `max_tasks` bounds all Actor and
/// Dialing registrations for this fixture's lifetime (1..=MAX_JOINED_TASKS).
/// Drop the supervisor through its normal owner, then await `TaskJoins::finish`
/// within the controller's immutable host deadline. Finish never drives the
/// supervisor, drains output, requests shutdown or advances a clock.
#[cfg(any(test, feature = "fixture-clock"))]
pub fn supervisor_with_capture_clock_and_joins(
    config: SessionConfig,
    policy: crate::RecoveryPolicy,
    capacity: usize,
    clock: Arc<dyn CaptureClock>,
    max_tasks: std::num::NonZeroUsize,
) -> Result<(
    crate::Supervisor<FakeConnector>,
    mpsc::UnboundedReceiver<Dial>,
    TaskJoins,
)> {
    let (registrar, tasks) = TaskJoins::new(max_tasks)?;
    let joins = JoinSource::Retained(registrar);
    let (connector, dials) = connector_with_sources(
        config,
        capacity,
        CaptureClockSource::Fixture(clock),
        joins.clone(),
    );
    let supervisor = crate::Supervisor::new_with_join_source(connector, policy, joins)?;
    Ok((supervisor, dials, tasks))
}

impl Connect for FakeConnector {
    fn connect(&mut self, session: SessionId, resume: Option<ResumptionHandle>) -> Attempt {
        let resumed = resume.is_some();
        let config = self.config.clone().resume(resume);
        let dials = self.dials.clone();
        let capacity = self.capacity;
        let capture_clock = self.capture_clock.clone();
        let joins = self.joins.clone();
        Box::pin(async move {
            let (attempt, service) =
                pair_with_sources(config, session, capacity, capture_clock, joins).await;
            let _ = dials.send(Dial {
                session,
                resumed,
                service,
            });
            attempt.await
        })
    }
}

/// Next dial, or a panic when the supervisor does not attempt one in time.
pub async fn next_dial(observed: &mut mpsc::UnboundedReceiver<Dial>) -> Dial {
    dial_within(observed, RECEIVE_TIMEOUT).await
}
pub async fn dial_within(observed: &mut mpsc::UnboundedReceiver<Dial>, wait: Duration) -> Dial {
    timeout(wait, observed.recv())
        .await
        .expect("connection attempt")
        .expect("connector alive")
}
