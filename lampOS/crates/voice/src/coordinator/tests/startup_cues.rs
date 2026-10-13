//! Startup state history versus live cues, through the production startup
//! retention (`StartupEvents`), conversation-entry recording and the real
//! `CueSink`. Local sockets only; no hardware or provider connection.
use super::*;
use std::os::unix::{fs::PermissionsExt, net::UnixDatagram};

struct Entered {
    _directory: SessionDirectory,
    socket: UnixDatagram,
    cues: Option<CueSink>,
    trace: SessionState,
}

impl Entered {
    fn received(&self) -> Vec<Value> {
        let mut cues = Vec::new();
        let mut bytes = [0; 512];
        while let Ok(count) = self.socket.recv(&mut bytes) {
            cues.push(serde_json::from_slice(&bytes[..count]).unwrap());
        }
        cues
    }
    fn sink(&self) -> &CueSink {
        self.cues.as_ref().unwrap()
    }
}

/// A provider Ready is received during startup; later children take
/// `startup` to start. `conversation()` then records the retained events
/// at entry, and listening readiness follows on a later tick.
fn enter(startup: Duration) -> Entered {
    let directory = SessionDirectory::create().unwrap();
    let (mut parent, mut provider) = pair(&directory, "provider");
    provider
        .control
        .send(WorkerEvent::ProviderLink {
            link: ready_link(1, 1),
        })
        .unwrap();
    let mut retained = StartupEvents::default();
    retained.poll(&mut parent, "provider").unwrap();
    std::thread::sleep(startup);
    let path = directory.path.join("cue.sock");
    let socket = UnixDatagram::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut sink = CueSink::connect(&path).unwrap();
    sink.set_boot(new_boot().unwrap());
    let mut entered = Entered {
        _directory: directory,
        socket,
        cues: Some(sink),
        trace: SessionState::finite(),
    };
    for event in retained.events {
        record(&mut entered.trace, event).unwrap();
    }
    flush_cues(&mut entered.trace, &mut entered.cues);
    record(
        &mut entered.trace,
        json!({"kind":"listening_ready","at_us":monotonic_us(),"scope":"test"}),
    )
    .unwrap();
    flush_cues(&mut entered.trace, &mut entered.cues);
    entered
}

fn kinds(cues: &[Value]) -> Vec<&str> {
    cues.iter()
        .map(|cue| cue["kind"].as_str().unwrap())
        .collect()
}

#[test]
fn a_ready_retained_through_a_slow_startup_is_history_not_a_cue_fault() {
    let entered = enter(Duration::from_millis(120));
    // Before the fix this latched cue_invalid_or_unavailable and published
    // nothing, not even listening readiness.
    assert_eq!(entered.sink().fault(), None);
    let cues = entered.received();
    assert_eq!(kinds(&cues), ["listening_ready"]);
    // The old Ready is neither sent nor re-stamped; it is only counted.
    assert_eq!(entered.sink().historical_state_skipped(), 1);
    assert_eq!(entered.trace[0]["kind"], "provider_ready");
}

#[test]
fn a_fresh_startup_ready_is_still_published_before_readiness() {
    let entered = enter(Duration::from_millis(20));
    assert_eq!(entered.sink().fault(), None);
    let cues = entered.received();
    assert_eq!(kinds(&cues), ["provider_link", "listening_ready"]);
    assert_eq!(cues[0]["link"], "ready");
    assert_eq!(
        cues[1]["sequence"],
        cues[0]["sequence"].as_u64().unwrap() + 1
    );
    assert_eq!(entered.sink().historical_state_skipped(), 0);
}

#[test]
fn after_readiness_a_stale_state_record_still_latches_the_fault() {
    let mut entered = enter(Duration::from_millis(20));
    let old = monotonic_us() - 150_000;
    record(
        &mut entered.trace,
        json!({"kind":"provider_unavailable","at_us":old,"coordinator_observed_at_us":old,"link":{}}),
    )
    .unwrap();
    flush_cues(&mut entered.trace, &mut entered.cues);
    assert_eq!(
        entered.sink().fault(),
        Some("cue_invalid_or_unavailable"),
        "a live link change too old to send must not be dropped silently"
    );
    assert_eq!(entered.sink().historical_state_skipped(), 0);
}

#[test]
fn a_future_stamped_startup_record_is_still_rejected() {
    let directory = SessionDirectory::create().unwrap();
    let path = directory.path.join("cue.sock");
    let socket = UnixDatagram::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut cues = Some(CueSink::connect(&path).unwrap());
    cues.as_mut().unwrap().set_boot(new_boot().unwrap());
    let mut trace = SessionState::finite();
    let future = monotonic_us() + 1_000_000;
    record(
        &mut trace,
        json!({"kind":"provider_ready","at_us":future,"coordinator_observed_at_us":future,"link":{}}),
    )
    .unwrap();
    flush_cues(&mut trace, &mut cues);
    assert_eq!(cues.unwrap().fault(), Some("cue_invalid_or_unavailable"));
}

/// What `conversation()` records at readiness: the link state it was just
/// validated against, then listening readiness, at the same tick.
fn readiness_records(at: u64) -> [Value; 2] {
    [
        json!({"kind":"provider_link_current","at_us":at,"state":"ready","session":1,
            "basis":"coordinator link register at listening readiness"}),
        json!({"kind":"listening_ready","at_us":at,"scope":"test"}),
    ]
}

#[test]
fn after_a_slow_startup_the_current_link_state_is_published_fresh() {
    let directory = SessionDirectory::create().unwrap();
    let (mut parent, mut provider) = pair(&directory, "provider");
    provider
        .control
        .send(WorkerEvent::ProviderLink {
            link: ready_link(1, 1),
        })
        .unwrap();
    let mut retained = StartupEvents::default();
    retained.poll(&mut parent, "provider").unwrap();
    std::thread::sleep(Duration::from_millis(120));
    let path = directory.path.join("cue.sock");
    let socket = UnixDatagram::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut cues = Some(CueSink::connect(&path).unwrap());
    cues.as_mut().unwrap().set_boot(new_boot().unwrap());
    let mut trace = SessionState::finite();
    for event in retained.events {
        record(&mut trace, event).unwrap();
    }
    for event in readiness_records(monotonic_us()) {
        record(&mut trace, event).unwrap();
    }
    flush_cues(&mut trace, &mut cues);
    let entered = Entered {
        _directory: directory,
        socket,
        cues,
        trace,
    };
    assert_eq!(entered.sink().fault(), None);
    let received = entered.received();
    assert_eq!(kinds(&received), ["provider_link", "listening_ready"]);
    assert_eq!(received[0]["link"], "ready");
    // Its own fresh time, not the retained Ready's.
    assert_eq!(received[0]["event_us"], received[1]["event_us"]);
    assert_eq!(entered.sink().historical_state_skipped(), 1);
}

#[test]
fn a_current_link_record_keeps_the_strict_bound_and_shape() {
    let directory = SessionDirectory::create().unwrap();
    let path = directory.path.join("cue.sock");
    let socket = UnixDatagram::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    socket.set_nonblocking(true).unwrap();
    for record_value in [
        // Current state is never history: too old to send is a fault.
        json!({"kind":"provider_link_current","at_us":monotonic_us() - 150_000,"state":"ready"}),
        json!({"kind":"provider_link_current","at_us":monotonic_us(),"state":"maybe"}),
    ] {
        let mut cues = Some(CueSink::connect(&path).unwrap());
        cues.as_mut().unwrap().set_boot(new_boot().unwrap());
        let mut trace = SessionState::finite();
        record(&mut trace, record_value).unwrap();
        flush_cues(&mut trace, &mut cues);
        assert_eq!(cues.unwrap().fault(), Some("cue_invalid_or_unavailable"));
    }
    let mut bytes = [0; 512];
    assert!(socket.recv(&mut bytes).is_err(), "nothing was published");
}
