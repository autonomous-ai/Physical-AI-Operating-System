//! #1178: a worker's stderr is its parent's relay pipe. The worker's relay
//! must leave that pipe as the parent made it, blocking, or the parent's
//! next `eprintln!` into a full pipe fails and panics. It runs its own
//! binary again, as the worker, with only this test selected.
use rustix::fd::OwnedFd;
use std::process::{Command, Stdio};
use std::time::Duration;

const CHILD: &str = "LAMP_IPC_RELAY_SHARED_CHILD";
const NAME: &str = "a_worker_relay_leaves_its_parents_pipe_blocking";

#[test]
fn a_worker_relay_leaves_its_parents_pipe_blocking() {
    if std::env::var_os(CHILD).is_some() {
        {
            let _log = lamp_ipc::log_relay::guard();
            eprintln!("a worker's line");
        }
        std::process::exit(0);
    }
    let (read, write) = rustix::pipe::pipe().unwrap();
    let parent = rustix::io::dup(&write).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([NAME, "--exact", "--nocapture", "--test-threads", "1"])
        .env(CHILD, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::from(write))
        .status()
        .unwrap();
    assert!(status.success());
    // The parent fills its pipe: a blocking one makes it wait, a
    // non-blocking one fails at once with `AGAIN`.
    let filling = std::thread::spawn(move || -> rustix::io::Errno {
        loop {
            if let Err(error) = rustix::io::write(&parent, &[b'z'; 4096]) {
                return error;
            }
        }
    });
    std::thread::sleep(Duration::from_millis(300));
    let waited = !filling.is_finished();
    // No reader: a waiting write ends with `PIPE`.
    drop::<OwnedFd>(read);
    let ended = filling.join().unwrap();
    assert!(
        waited,
        "the worker's relay made the parent's pipe non-blocking: {ended}"
    );
}
