//! Same-host attended operator, not an authority or camera-evidence producer.
//! Every requested action must be present in argv. No retry or implicit return.
use lamp_ipc::{Endpoint, MAX_DATAGRAM_BYTES, monotonic_us};
use lamp_live::head_qualification::{Action, CONTROL_AGE_US, Reply, Request, Stage};
use std::{
    fs::OpenOptions,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};

fn exchange(endpoint: &Endpoint, request: &Request, log: &mut std::fs::File) -> io::Result<Reply> {
    let bytes = serde_json::to_vec(request).map_err(io::Error::other)?;
    // Evidence precedes the attempted send. There is no retry after uncertainty.
    writeln!(
        log,
        "{}",
        serde_json::json!({"at_us":monotonic_us(),"attempt":request})
    )?;
    endpoint.try_send(&bytes)?;
    let start = Instant::now();
    let mut bytes = [0; MAX_DATAGRAM_BYTES];
    loop {
        match endpoint.try_recv(&mut bytes) {
            Ok(n) => {
                let reply: Reply = serde_json::from_slice(&bytes[..n]).map_err(io::Error::other)?;
                writeln!(
                    log,
                    "{}",
                    serde_json::json!({"received_at_us":monotonic_us(),"reply":reply})
                )?;
                let sequence = match request {
                    Request::Status { sequence, .. }
                    | Request::Act { sequence, .. }
                    | Request::Stop { sequence, .. } => *sequence,
                };
                if reply.sequence != sequence
                    || monotonic_us()
                        .checked_sub(reply.at_us)
                        .is_none_or(|age| age >= CONTROL_AGE_US)
                {
                    return Err(io::Error::other(
                        "foreign, duplicated or stale operator reply",
                    ));
                }
                if let Some(error) = &reply.error {
                    return Err(io::Error::other(error.clone()));
                }
                return Ok(reply);
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    && start.elapsed() < Duration::from_millis(100) =>
            {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(e) => return Err(e),
        }
    }
}
fn await_servicing(
    endpoint: &Endpoint,
    log: &mut std::fs::File,
) -> io::Result<lamp_interaction::BootId> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut bytes = [0; MAX_DATAGRAM_BYTES];
    loop {
        match endpoint.try_recv(&mut bytes) {
            Ok(n) => {
                let hello: Reply = serde_json::from_slice(&bytes[..n]).map_err(io::Error::other)?;
                writeln!(
                    log,
                    "{}",
                    serde_json::json!({"received_at_us":monotonic_us(),"control_servicing":hello,"listening_readiness":false})
                )?;
                if hello.sequence != 0
                    || hello.ticket.is_some()
                    || hello.error.is_some()
                    || monotonic_us()
                        .checked_sub(hello.at_us)
                        .is_none_or(|age| age >= CONTROL_AGE_US)
                {
                    return Err(io::Error::other("invalid control startup announcement"));
                }
                return Ok(hello.session);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(e) => return Err(e),
        }
    }
}
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let plain = ["inspect", "outbound", "reinspect", "return"];
    let setup = [
        "inspect_profile",
        "prepare_profile",
        "inspect",
        "outbound",
        "reinspect",
        "return",
    ];
    if args.len() < 2 || (args[2..] != plain && args[2..] != setup) {
        return Err(io::Error::other(
            "usage: head_qualification_operator ABS_PRIVATE_CHANNEL_DIR NEW_RECEIPTS.jsonl inspect outbound reinspect return; no default actions",
        ));
    }
    let directory = PathBuf::from(&args[0]);
    let path = PathBuf::from(&args[1]);
    if !directory.is_absolute() || !path.is_absolute() {
        return Err(io::Error::other("paths must be absolute"));
    }
    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    let mut endpoint = Endpoint::bind(directory.join("operator.sock"))?;
    // Startup is the only reconnect loop: no action has been submitted yet.
    let starting = Instant::now();
    loop {
        match endpoint.connect(directory.join("runtime.sock")) {
            Ok(()) => break,
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    && starting.elapsed() < Duration::from_secs(30) =>
            {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(e) => return Err(e),
        }
    }
    let initial_session = await_servicing(&endpoint, &mut log)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let actions = if args.len() == 8 {
        vec![
            Action::InspectProfile,
            Action::PrepareProfile,
            Action::Inspect,
            Action::Outbound,
            Action::Reinspect,
            Action::Return,
        ]
    } else {
        vec![
            Action::Inspect,
            Action::Outbound,
            Action::Reinspect,
            Action::Return,
        ]
    };
    let mut next = 0;
    let mut sequence = 0;
    let mut owner = None;
    let mut session = Some(initial_session);
    let result = (|| {
        while Instant::now() < deadline && sequence < 4094 {
            sequence += 1;
            let reply = exchange(
                &endpoint,
                &Request::Status {
                    sequence,
                    sent_at_us: monotonic_us(),
                },
                &mut log,
            )?;
            if session.is_some_and(|s| s != reply.session) {
                return Err(io::Error::other("runtime session changed"));
            }
            session = Some(reply.session);
            if reply.stage == Stage::Inhibited {
                return Err(io::Error::other("qualification inhibited; no return"));
            }
            if next == actions.len()
                && reply.stage == Stage::Returned
                && reply.motor_reaped
                && reply.cleanup_verified
            {
                log.sync_all()?;
                return Ok(());
            }
            if next < actions.len()
                && let Some(ticket) = reply.ticket
                && ticket.stage == actions[next].stage()
            {
                if owner.is_some_and(|o| o != ticket.owner)
                    || monotonic_us() >= ticket.expires_at_us
                {
                    return Err(io::Error::other("original turn/receipt changed"));
                }
                owner = Some(ticket.owner);
                sequence += 1;
                let action_reply = exchange(
                    &endpoint,
                    &Request::Act {
                        sequence,
                        sent_at_us: monotonic_us(),
                        ticket: ticket.id,
                        owner: ticket.owner,
                        action: actions[next],
                    },
                    &mut log,
                )?;
                if Some(action_reply.session) != session {
                    return Err(io::Error::other("action session changed"));
                }
                next += 1;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "qualification deadline; no automatic return",
        ))
    })();
    if result.is_err() {
        // Exactly one Stop attempt; a lost ACK never replays a movement.
        sequence += 1;
        let _ = exchange(
            &endpoint,
            &Request::Stop {
                sequence,
                sent_at_us: monotonic_us(),
            },
            &mut log,
        );
        let _ = log.sync_all();
    }
    result
}
