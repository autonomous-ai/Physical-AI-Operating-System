//! A part that blocks SIGTERM in `main` after its log relay is installed
//! still receives it there: the relay's thread never takes a signal.
//! `harness = false`: the test harness's own threads would take the signal
//! otherwise, so this program is its own `main`, run again as the part.
use nix::sys::signal::{SigSet, Signal, kill};
use std::process::Command;

const CHILD: &str = "LAMP_IPC_RELAY_SIGNALS_CHILD";

fn main() {
    if std::env::var_os(CHILD).is_some() {
        let _log = lamp_ipc::log_relay::guard();
        let mut mask = SigSet::empty();
        mask.add(Signal::SIGTERM);
        mask.thread_block().unwrap();
        kill(nix::unistd::Pid::this(), Signal::SIGTERM).unwrap();
        assert_eq!(mask.wait().unwrap(), Signal::SIGTERM);
        println!("took SIGTERM in main");
        return;
    }
    let out = Command::new(std::env::current_exe().unwrap())
        .env(CHILD, "1")
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && said.contains("took SIGTERM in main"),
        "SIGTERM blocked in main after the relay ended the process: {:?} {said}",
        out.status
    );
    println!("relay_signals: ok");
}
