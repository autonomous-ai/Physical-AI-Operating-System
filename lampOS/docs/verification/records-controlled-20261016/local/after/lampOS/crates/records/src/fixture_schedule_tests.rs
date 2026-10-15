use super::*;
use crate::{
    fixture_protocol::{Bootstrap, Channel, Checked, HOST_WAIT, Outcome, PERIOD_NS, Step, VERSION},
    fixture_schedule::Schedule,
};
use lamp_msgs::Diagnostic;
use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

const NONCE: &str = "recordsBoundary";
type Pipes = Channel<File, File>;

// Standard-library pipes coordinate close-on-exec setup with process spawning.
fn private_pipe() -> io::Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd)> {
    let (reader, writer) = std::io::pipe()?;
    Ok((reader.into(), writer.into()))
}

fn channels() -> (Pipes, Pipes, File) {
    let (requests, input) = private_pipe().unwrap();
    let (output, replies) = private_pipe().unwrap();
    let output = File::from(output);
    let probe = output.try_clone().unwrap();
    (
        Channel::new(output, File::from(input)).unwrap(),
        Channel::new(File::from(requests), File::from(replies)).unwrap(),
        probe,
    )
}

fn deadline() -> Instant {
    Instant::now() + HOST_WAIT
}

fn begin(parent: &mut Pipes) {
    parent
        .write_until(
            &Bootstrap {
                version: VERSION,
                nonce: NONCE.into(),
            },
            deadline(),
        )
        .unwrap();
}

fn request(parent: &mut Pipes, round: u64, at_ns: u64) {
    parent
        .write_until(
            &Step {
                nonce: NONCE.into(),
                round,
                at_ns,
            },
            deadline(),
        )
        .unwrap();
}

fn checked(parent: &mut Pipes) -> Checked {
    parent.read_until(deadline()).unwrap()
}

fn no_reply(probe: &mut File) {
    let error = probe.read(&mut [0]).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

fn cycle(schedule: &Schedule<File, File>, began: Instant) -> ReportCycle {
    ReportCycle {
        started: schedule.now(),
        host_started: began,
        host_seconds: Some(4),
        construction_or_publish: Ok(()),
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = fs::canonicalize("/tmp").unwrap().join(format!(
            "rcf-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }

    fn config(&self) -> (PathBuf, Config) {
        let mut config = Config::customer_default();
        config.bus_root = self.0.join("bus");
        let path = self.0.join("config.json");
        json_file::write(&path, &config).unwrap();
        (path, config)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn reads_and_pre_due_steps_are_pure_and_due_reply_waits_for_the_next_cycle() {
    let (mut parent, child, mut probe) = channels();
    begin(&mut parent);
    let mut schedule = Schedule::bootstrap(child, deadline()).unwrap();
    let first = schedule.now();
    for _ in 0..32 {
        assert_eq!(schedule.now(), first);
    }
    for (round, at) in [
        (1, 0),
        (2, 0),
        (3, PERIOD_NS / 2),
        (4, PERIOD_NS - 1),
        (5, PERIOD_NS),
    ] {
        request(&mut parent, round, at);
    }
    let began = Instant::now();
    schedule
        .wait(cycle(&schedule, began), Duration::from_secs(1))
        .unwrap();
    for (round, at) in [(1, 0), (2, 0), (3, PERIOD_NS / 2), (4, PERIOD_NS - 1)] {
        let reply = checked(&mut parent);
        assert_eq!(
            (reply.round, reply.at_ns, reply.completed_cycles),
            (round, at, 1)
        );
        assert_eq!((reply.cycle_started_ns, reply.next_due_ns), (0, PERIOD_NS));
    }
    no_reply(&mut probe);
    assert_eq!(schedule.now() - first, Duration::from_secs(1));
    request(&mut parent, 6, 2 * PERIOD_NS);
    schedule
        .wait(cycle(&schedule, began), Duration::from_secs(1))
        .unwrap();
    let reply = checked(&mut parent);
    assert_eq!((reply.round, reply.completed_cycles), (5, 2));
    assert_eq!(
        (reply.at_ns, reply.cycle_started_ns, reply.next_due_ns),
        (PERIOD_NS, PERIOD_NS, 2 * PERIOD_NS)
    );
    no_reply(&mut probe);
}

#[test]
fn invalid_initial_identity_round_or_time_never_advances_and_latches_failure() {
    for (nonce, round, at_ns) in [
        ("other", 1, 0),
        (NONCE, 0, 0),
        (NONCE, 2, 0),
        (NONCE, 1, 1),
        (NONCE, 1, PERIOD_NS + 1),
        (NONCE, 1, u64::MAX),
    ] {
        let (mut parent, child, _) = channels();
        begin(&mut parent);
        let mut schedule = Schedule::bootstrap(child, deadline()).unwrap();
        let original = schedule.now();
        parent
            .write_until(
                &Step {
                    nonce: nonce.into(),
                    round,
                    at_ns,
                },
                deadline(),
            )
            .unwrap();
        let began = Instant::now();
        assert!(
            schedule
                .wait(cycle(&schedule, began), Duration::from_secs(1))
                .is_err()
        );
        assert_eq!(schedule.now(), original);
        assert!(
            schedule
                .wait(cycle(&schedule, began), Duration::from_secs(1))
                .unwrap_err()
                .to_string()
                .contains("already failed")
        );
        assert_eq!(
            parent.read_until::<Checked>(deadline()).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}

#[test]
fn a_stale_round_backwards_time_or_future_jump_after_settling_is_terminal() {
    for (round, at_ns) in [
        (2, PERIOD_NS / 2),
        (3, PERIOD_NS / 2 - 1),
        (3, PERIOD_NS + 1),
    ] {
        let (mut parent, child, _) = channels();
        begin(&mut parent);
        let mut schedule = Schedule::bootstrap(child, deadline()).unwrap();
        let original = schedule.now();
        request(&mut parent, 1, 0);
        request(&mut parent, 2, PERIOD_NS / 2);
        request(&mut parent, round, at_ns);
        let began = Instant::now();
        assert!(
            schedule
                .wait(cycle(&schedule, began), Duration::from_secs(1))
                .is_err()
        );
        assert_eq!(checked(&mut parent).at_ns, 0);
        assert_eq!(checked(&mut parent).at_ns, PERIOD_NS / 2);
        assert_eq!(
            schedule.now() - original,
            Duration::from_nanos(PERIOD_NS / 2)
        );
        assert_eq!(
            parent.read_until::<Checked>(deadline()).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}

#[test]
fn zero_wait_and_changed_host_lifetime_cannot_acknowledge_a_cycle() {
    let (mut parent, child, _) = channels();
    begin(&mut parent);
    let mut schedule = Schedule::bootstrap(child, deadline()).unwrap();
    let began = Instant::now();
    assert!(
        schedule
            .wait(cycle(&schedule, began), Duration::ZERO)
            .is_err()
    );
    assert_eq!(
        parent.read_until::<Checked>(deadline()).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );

    let (mut parent, child, _) = channels();
    begin(&mut parent);
    let mut schedule = Schedule::bootstrap(child, deadline()).unwrap();
    request(&mut parent, 1, 0);
    request(&mut parent, 2, PERIOD_NS);
    schedule
        .wait(cycle(&schedule, began), Duration::from_secs(1))
        .unwrap();
    assert_eq!(checked(&mut parent).completed_cycles, 1);
    assert!(
        schedule
            .wait(
                cycle(&schedule, began + Duration::from_nanos(1)),
                Duration::from_secs(1)
            )
            .is_err()
    );
    assert_eq!(
        parent.read_until::<Checked>(deadline()).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn actual_first_and_due_factories_must_finish_before_checked_or_event() {
    let scratch = Scratch::new();
    let (path, mut config) = scratch.config();
    let bus = lamp_bus::Bus::new(&config.bus_root).unwrap();
    let mut first = bus
        .subscribe_typed(Diagnostic::TOPIC, "first", Diagnostic::TYPE)
        .unwrap();
    let second_root = scratch.0.join("other");
    let second_bus = lamp_bus::Bus::new(&second_root).unwrap();
    let mut second = second_bus
        .subscribe_typed(Diagnostic::TOPIC, "next", Diagnostic::TYPE)
        .unwrap();
    thread::scope(|threads| {
        let (mut parent, child, mut probe) = channels();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        begin(&mut parent);
        request(&mut parent, 1, 0);
        let child_path = path.clone();
        let result = scratch.0.join("last.json");
        let worker = threads.spawn(move || {
            let mut schedule = Schedule::bootstrap(child, deadline()).unwrap();
            let clock = schedule.clock();
            reporting_with(child_path, result, Some(4), &mut schedule, |root| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(HOST_WAIT).unwrap();
                let clock = clock.clone();
                Reporter::new_with_cadence_clock(root, move || clock.now())
            })
        });
        entered_rx.recv_timeout(HOST_WAIT).unwrap();
        no_reply(&mut probe);
        assert!(first.recv(Duration::ZERO).unwrap().is_none());
        release_tx.send(()).unwrap();
        assert_eq!(checked(&mut parent).completed_cycles, 1);
        assert!(first.recv(Duration::ZERO).unwrap().is_some());
        config.bus_root = second_root.clone();
        json_file::write(&path, &config).unwrap();
        request(&mut parent, 2, PERIOD_NS - 1);
        assert_eq!(checked(&mut parent).completed_cycles, 1);
        assert_eq!(
            entered_rx.try_recv().unwrap_err(),
            mpsc::TryRecvError::Empty
        );
        assert!(second.recv(Duration::ZERO).unwrap().is_none());
        request(&mut parent, 3, PERIOD_NS);
        entered_rx.recv_timeout(HOST_WAIT).unwrap();
        no_reply(&mut probe);
        assert!(second.recv(Duration::ZERO).unwrap().is_none());
        release_tx.send(()).unwrap();
        let reply = checked(&mut parent);
        assert_eq!(
            (reply.round, reply.completed_cycles, reply.at_ns),
            (3, 2, PERIOD_NS)
        );
        assert_eq!(reply.construction_or_publish, Outcome::ReturnedOk);
        assert!(second.recv(Duration::ZERO).unwrap().is_some());
        parent.close();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    });
}

#[test]
fn real_construction_failure_can_only_recover_on_the_original_next_due() {
    let scratch = Scratch::new();
    let (path, config) = scratch.config();
    let bus = lamp_bus::Bus::new(&config.bus_root).unwrap();
    let mut listener = bus
        .subscribe_typed(Diagnostic::TOPIC, "recover", Diagnostic::TYPE)
        .unwrap();
    thread::scope(|threads| {
        let (mut parent, child, _) = channels();
        begin(&mut parent);
        let result = scratch.0.join("last.json");
        let worker = threads.spawn(move || {
            let mut schedule = Schedule::bootstrap(child, deadline()).unwrap();
            let clock = schedule.clock();
            let mut first = true;
            reporting_with(path, result, Some(4), &mut schedule, |root| {
                if std::mem::take(&mut first) {
                    return Err(io::Error::other("held construction failure"));
                }
                let clock = clock.clone();
                Reporter::new_with_cadence_clock(root, move || clock.now())
            })
        });
        request(&mut parent, 1, 0);
        assert_eq!(
            checked(&mut parent).construction_or_publish,
            Outcome::ReturnedErr
        );
        assert!(listener.recv(Duration::ZERO).unwrap().is_none());
        request(&mut parent, 2, PERIOD_NS - 1);
        assert_eq!(
            checked(&mut parent).construction_or_publish,
            Outcome::ReturnedErr
        );
        assert!(listener.recv(Duration::ZERO).unwrap().is_none());
        request(&mut parent, 3, PERIOD_NS);
        let reply = checked(&mut parent);
        assert_eq!(reply.completed_cycles, 2);
        assert_eq!(reply.construction_or_publish, Outcome::ReturnedOk);
        assert!(listener.recv(Duration::ZERO).unwrap().is_some());
        parent.close();
        assert!(worker.join().unwrap().is_err());
    });
}
