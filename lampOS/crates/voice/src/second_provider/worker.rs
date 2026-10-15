//! The second route as a provider worker, on the ordinary provider IPC:
//! the fixture's startup readiness, authority guard and credited output
//! (`crate::fixture_provider`), around [`Engine`] instead of a recording.
//!
//! [`serve`] is the process (`lamp-voice worker second`): the character of
//! the Live route's configuration, told the same time, memory note and
//! conversation thread as a Live session is, with the same tools, and each
//! turn kept in that thread and the memory's journal as the Live route
//! keeps its own.
use super::{ANNOUNCEMENT, Engine, Net, Told, ToolRunner, net::Message, output_request};
use crate::{
    provider_flow::OutputSender,
    provider_link::{LinkContext, ProviderLink, ProviderLinkState},
    provider_worker::{ProviderInput, ProviderOutput},
    transport::{Channel, WorkerChannels},
    wire::{Control, WorkerEvent},
};
use lamp_ipc::monotonic_us;
use lamp_turn_owner::{BootId, BoundaryGuard, MonoTime, Snapshot};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

/// The coordinator renews authority at least this often.
const CONTROL_LEASE_US: u64 = 250_000;
/// Control messages taken in one tick before input and output wait.
const CONTROL_SLICE: usize = 16;
/// Input commands taken in one tick, as the Live provider worker does.
const INPUT_SLICE: usize = 8;

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

/// The `worker second` flags: the Live provider worker's, as
/// `coordinator::provider_arguments` passes them, parsed by the same code,
/// but for `--keep-link`: this route has no link to keep.
pub fn parse_flags(dir: &Path, flags: &[String]) -> io::Result<crate::provider_worker::Parsed> {
    let parsed = crate::provider_worker::parse_flags(dir, flags)?;
    if parsed.2.keep_link {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the second voice route has no link to keep",
        ));
    }
    Ok(parsed)
}

/// The second route's worker process until Stop. `files` are the Live
/// provider worker's flags; this route uses the thread's file and the
/// tools (the memory among them), and has no use yet for the rest.
pub fn serve(
    channels: WorkerChannels,
    boot: BootId,
    provider_config: &Path,
    second_config: &Path,
    files: crate::provider_worker::Files,
) -> io::Result<()> {
    let character = crate::config::ProviderConfig::load(provider_config)?.instruction(false)?;
    let config = crate::second_config::load_from_path(second_config)?;
    let tools = files.tools;
    if let Some(socket) = &tools.memory {
        // As the Live route starts it: each known person's memory apart.
        crate::remember::start(
            socket.clone(),
            tools.who.as_deref().map(crate::known_faces::known_file_of),
        );
    }
    let net = Net::new(config, &crate::tools::declared(&tools)).map_err(io::Error::other)?;
    let thread = files.thread;
    let file = thread.clone();
    let clock = tools.clock;
    // Each turn is one person's, as a Live session is (#764): whose it is,
    // counted from the eyes' known file as the Live worker counts it, opens
    // the thread for them and asks for their kept things. A Days note is
    // frozen for a session; here a session is the run of turns for one
    // person, so it is asked again only when the person changes.
    let socket = tools.memory.clone();
    let who = tools
        .who
        .as_deref()
        .map(crate::known_faces::known_file_of)
        .filter(|_| socket.is_some());
    let days = crate::remember::days_from_environment();
    // Told after the character with `LAMP_TASKS_ASKED=on`, as the Live route is (#854).
    let tasks = tools.tasks.clone();
    let sessions = std::sync::Mutex::new(Sessions::default());
    let system = move || {
        let now = Instant::now();
        let Ok(mut sessions) = sessions.lock() else {
            return Told::default();
        };
        let (changed, person) = sessions.open(who.as_deref(), now);
        let memory = match &socket {
            Some(socket) if days => sessions.frozen(changed, |session| {
                crate::remember::told_for_session(socket, session, person.as_deref())
            }),
            Some(socket) => crate::remember::told_for_session(socket, 0, person.as_deref()),
            None => crate::remember::told(),
        };
        system_message(
            &character,
            crate::agent_tasks::noted(crate::local_time::told_here(clock), tasks.as_ref())
                .as_deref(),
            &memory,
            file.as_deref(),
        )
    };
    let runner: ToolRunner = Arc::new(move |request, heard, name, args| {
        let settings = tools.clone();
        Box::pin(async move {
            let call = lamp_gemini::ToolCall {
                id: String::new(),
                name,
                args,
            };
            tokio::task::spawn_blocking(move || {
                crate::tools::answer(&settings, &call, request, heard)
            })
            .await
            .unwrap_or_else(|_| crate::tools::unavailable())
        })
    });
    let engine = Engine::new(Arc::new(net), system, runner);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(channels, boot, engine, thread))
}

/// Whose the second route's turns are, and the Days note of that person's
/// run of turns.
#[derive(Default)]
struct Sessions {
    presence: crate::known_faces::Presence,
    /// Runs of turns opened so far: the Days session number.
    opened: u64,
    note: Option<String>,
}
impl Sessions {
    /// Open the thread for whoever is at the lamp now ([`Thread::opening`]):
    /// whether that is someone other than the last turn's, and whom.
    ///
    /// [`Thread::opening`]: crate::thread::Thread::opening
    fn open(&mut self, who: Option<&Path>, now: Instant) -> (bool, Option<String>) {
        let known = who
            .and_then(crate::known_faces::known_now)
            .map(|name| crate::remember::person_id(&name));
        let whose = who.and(self.presence.seen(known, now).map(str::to_owned));
        let Ok(mut thread) = crate::thread::THREAD.lock() else {
            return (false, None);
        };
        if who.is_some() {
            thread.at_lamp(whose);
        }
        let changed = thread.opening(now);
        (changed, thread.opened_for().map(str::to_owned))
    }

    /// The Days note for this run of turns, asked anew when the person changed.
    fn frozen(&mut self, changed: bool, ask: impl FnOnce(u64) -> String) -> String {
        if changed || self.note.is_none() {
            self.opened += 1;
            self.note = Some(ask(self.opened));
        }
        self.note.clone().unwrap_or_default()
    }
}

/// The system message for a turn begun now, as a Live session opened now
/// is told its instruction (`crate::thread::Recalling`): the character,
/// the time and place, the memory note, and the thread in the room the
/// rest leave it, taken first from `file` if the Live route wrote it since.
///
/// With it, the person's last exchanges as messages: up to
/// [`HISTORY_EXCHANGES`] of the last [`HISTORY_WITHIN`]. A Live session keeps
/// its own, and "and how many live there?" needs the one before. Taken
/// from the thread under the same lock as the recall, after the thread's
/// forgetting and reload, so a forgotten exchange is never given back.
pub(crate) fn system_message(
    character: &str,
    time: Option<&str>,
    memory: &str,
    file: Option<&Path>,
) -> Told {
    let told = format!("{character}{}{memory}", time.unwrap_or_default());
    let room = lamp_gemini::MAX_INSTRUCTION_BYTES.saturating_sub(told.len());
    let now = Instant::now();
    let (thread, recent) = crate::thread::current(file, |thread| {
        (
            thread.recall(now, room),
            thread.recent(now, HISTORY_WITHIN, HISTORY_EXCHANGES),
        )
    })
    .unwrap_or_default();
    Told {
        text: told + &thread.unwrap_or_default(),
        history: recent.into_iter().flat_map(messages).collect(),
    }
}

/// One exchange as chat messages. A request the lamp was given in writing
/// goes as a system note, never as words a person said; nothing is sent
/// for a person who said nothing.
fn messages(exchange: crate::thread::Recent) -> Vec<Message> {
    let mut messages = Vec::with_capacity(3);
    if let Some(written) = exchange.written {
        messages.push(Message::system(format!(
            "(A note you were given in writing, not words anyone said: {written})"
        )));
    }
    if !exchange.person.is_empty() {
        messages.push(Message::user(exchange.person));
    }
    messages.push(Message::assistant(exchange.lamp));
    messages
}

/// Exchanges given to the chat as messages, and for how long since each.
pub const HISTORY_EXCHANGES: usize = 6;
pub const HISTORY_WITHIN: Duration = Duration::from_secs(300);

/// Keep what a turn's output says in the thread, as the Live route keeps
/// its transcripts: the person's words, the lamp's (not the announcement),
/// and at the turn's end the exchange to the memory's journal and the
/// thread to its file.
pub(crate) fn keep(event: &ProviderOutput, file: Option<&Path>) {
    let now = Instant::now();
    match event {
        ProviderOutput::Transcript {
            request: None,
            text,
            finished,
        } => {
            if let Ok(mut thread) = crate::thread::THREAD.lock() {
                thread.person(text, *finished, now);
            }
        }
        ProviderOutput::Transcript {
            request: Some(request),
            text,
            ..
        } if text != ANNOUNCEMENT => {
            // Sentences come trimmed; the thread joins its pieces as given.
            if let Ok(mut thread) = crate::thread::THREAD.lock() {
                thread.lamp(*request, &format!("{text} "), now);
            }
        }
        ProviderOutput::TurnComplete { request, .. } => {
            crate::remember::finished(*request);
            save(file);
        }
        ProviderOutput::TurnFailed { .. } => save(file),
        _ => {}
    }
}

/// The thread to its file if it changed; one not written loses the thread
/// at the next restart and nothing else.
fn save(file: Option<&Path>) {
    let Some(path) = file else { return };
    let text = crate::thread::THREAD
        .lock()
        .ok()
        .and_then(|mut thread| thread.save(Instant::now(), SystemTime::now()));
    if let Some(text) = text
        && crate::thread::write_file(path, &text).is_ok()
        && let Ok(mut thread) = crate::thread::THREAD.lock()
    {
        thread.wrote(path);
    }
}

/// Run the engine as a provider worker until Stop. Readiness is the
/// fixture's: `Ready`, then one `ProviderLink` Ready on the control plane.
/// ponytail: the services are not probed first; the link says Ready and a
/// service that cannot be reached fails that turn. A probe at startup if a
/// dead key should show as an unavailable link.
pub async fn run(
    mut channels: WorkerChannels,
    boot: BootId,
    mut engine: Engine,
    thread: Option<PathBuf>,
) -> io::Result<()> {
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
    'tick: loop {
        // The turn tasks run while this waits.
        ticker.tick().await;
        gate.fresh(monotonic_us())?;
        match gate.drain(&mut channels.control, &mut engine)? {
            Drain::Stopped => return Ok(()),
            Drain::Deferred => continue,
            Drain::Empty => {}
        }
        // As many inputs a tick as the Live worker takes: a line replayed
        // to this route arrives faster than it was spoken, and a packet
        // left waiting past its age bound is refused.
        for _ in 0..INPUT_SLICE {
            if retained.is_none() {
                retained = channels.data.receive()?;
            }
            // The parent sends authority before its dependent input, possibly
            // after the first empty read: drain again before judging the input.
            match gate.drain(&mut channels.control, &mut engine)? {
                Drain::Stopped => return Ok(()),
                Drain::Deferred => continue 'tick,
                Drain::Empty => {}
            }
            let Some(command) = retained.take() else {
                break;
            };
            if gate.admits(monotonic_us(), &command)? {
                // A written request is kept for the thread as the note it
                // is, with the answer to it (`Thread::written`).
                if let ProviderInput::Written { request, text, .. } = &command
                    && let Ok(mut thread) = crate::thread::THREAD.lock()
                {
                    thread.written(*request, text);
                }
                engine.submit(command);
            }
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
            if let Some(event) = &pending {
                keep(event, thread.as_deref());
            }
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
            Told::default,
            Arc::new(|_, _, _, _| Box::pin(async { serde_json::Value::Null })),
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
            continues_cut_off: false,
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

    fn strings(flags: &[&str]) -> Vec<String> {
        flags.iter().map(|flag| (*flag).to_owned()).collect()
    }

    #[test]
    fn a_written_request_goes_to_the_chat_as_a_note_and_silence_as_nothing() {
        let written = messages(crate::thread::Recent {
            written: Some("Say ORCHID.".into()),
            person: String::new(),
            lamp: "ORCHID.".into(),
        });
        assert_eq!(
            written,
            [
                Message::system(
                    "(A note you were given in writing, not words anyone said: Say ORCHID.)"
                ),
                Message::assistant("ORCHID."),
            ]
        );
        let spoken = messages(crate::thread::Recent {
            written: None,
            person: "Hi.".into(),
            lamp: "Hello.".into(),
        });
        assert_eq!(spoken, [Message::user("Hi."), Message::assistant("Hello.")]);
    }

    #[test]
    fn the_second_worker_takes_the_live_workers_flags_but_keep_link() {
        let dir = Path::new("/run/lamp-voice");
        let flags = strings(&[
            "--provider-continuation",
            "kept-start",
            "--thread-file",
            "/run/lamp-voice/thread.json",
            "--session-fact",
            "--room-facts",
            "--air-facts",
            "--tools",
            "--actions-dir",
            "/run/lamp-exec",
            "--memory-socket",
            "/run/lamp-memory/memory.sock",
            "--clock",
            "--attention",
            "--attention-busy",
            "--name-signal",
            "--name-note",
            "--bus-root",
            "/run/lampos-bus",
        ]);
        let (_, _, files, visual) = parse_flags(dir, &flags).unwrap();
        assert_eq!(
            files.thread.as_deref(),
            Some(Path::new("/run/lamp-voice/thread.json"))
        );
        assert!(files.tools.decline && files.tools.clock && files.attention);
        assert_eq!(
            files.tools.memory.as_deref(),
            Some(Path::new("/run/lamp-memory/memory.sock"))
        );
        assert!(visual.is_none());
        assert!(parse_flags(dir, &[]).is_ok());
        for refused in [
            strings(&["--keep-link"]),
            strings(&["--tools", "--unknown"]),
            strings(&["--clock", "--tools"]),
        ] {
            assert!(parse_flags(dir, &refused).is_err(), "{refused:?}");
        }
    }

    /// A finished turn goes to the thread, its file and the journal, and
    /// the next turn's system message carries the character, the memory's
    /// note and that thread. In a child process: the thread and the
    /// memory are this process's globals.
    #[test]
    fn a_turn_is_kept_and_told_to_the_next() {
        use std::io::{BufRead, BufReader, Write};
        const CHILD: &str = "LAMP_SECOND_KEEP_TEST_CHILD";
        if std::env::var(CHILD).ok().as_deref() != Some("1") {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "second_provider::worker::tests::a_turn_is_kept_and_told_to_the_next",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env_remove(crate::remember::DAYS_ENVIRONMENT)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let dir = std::env::temp_dir().join(format!("lv-second-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let socket = dir.join("memory.sock");
        let file = dir.join("thread.json");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        // The memory: the kept list, then the journal's first append.
        let memory = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                let answer = match request["op"].as_str() {
                    Some("kept") => {
                        serde_json::json!({"ok":true,"entries":[{"text":"coffee black"}]})
                    }
                    _ => serde_json::json!({"ok":true}),
                };
                writeln!(stream, "{answer}").unwrap();
                if request["op"] == "append" {
                    return request;
                }
            }
            unreachable!()
        });
        crate::remember::start(socket.clone(), None);
        let request = 7;
        for event in [
            ProviderOutput::Transcript {
                request: None,
                text: "What should I drink?".into(),
                finished: true,
            },
            ProviderOutput::Transcript {
                request: Some(request),
                text: super::ANNOUNCEMENT.into(),
                finished: false,
            },
            ProviderOutput::Transcript {
                request: Some(request),
                text: "Try some tea.".into(),
                finished: false,
            },
            ProviderOutput::Transcript {
                request: Some(request),
                text: "It is calm.".into(),
                finished: false,
            },
            ProviderOutput::TurnComplete {
                request,
                idle: true,
            },
        ] {
            keep(&event, Some(&file));
        }
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(
            written.contains("What should I drink?")
                && written.contains("Try some tea. It is calm.")
        );
        assert!(!written.contains(super::ANNOUNCEMENT));
        let journalled = memory.join().unwrap();
        assert_eq!(journalled["heard"], "What should I drink?");
        assert_eq!(journalled["said"], "Try some tea. It is calm.");

        let told = system_message(
            "You are a lamp.",
            Some(" It is noon."),
            &crate::remember::told(),
            Some(&file),
        );
        assert!(
            told.text
                .starts_with("You are a lamp. It is noon.\n\nYour memory\n")
        );
        assert!(told.text.contains("- coffee black\n"));
        assert!(
            told.text
                .contains("Person: What should I drink?\nYou: Try some tea. It is calm.\n")
        );
        assert_eq!(
            told.history,
            [
                Message::user("What should I drink?"),
                Message::assistant("Try some tea. It is calm.")
            ]
        );
        // Forgotten, then another person's turn, then back: nothing of it
        // is told or given again (first builder's reviewer on #778).
        crate::thread::THREAD.lock().unwrap().forget();
        for whose in [Some("sam"), None] {
            let mut thread = crate::thread::THREAD.lock().unwrap();
            thread.at_lamp(whose.map(str::to_owned));
            thread.opening(Instant::now());
        }
        let told = system_message("You are a lamp.", None, "", Some(&file));
        assert!(told.history.is_empty(), "{:?}", told.history);
        assert!(!told.text.contains("Try some tea"), "{}", told.text);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
