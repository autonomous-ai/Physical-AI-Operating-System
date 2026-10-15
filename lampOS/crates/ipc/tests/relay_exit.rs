//! #837: a part that says its error and usage and then ends, as every part
//! does when its arguments are wrong, gets every line to stderr, every time.
//! `harness = false`: the test harness's own output and threads stay out of
//! it, so this program is its own `main`, run again as the part.
use std::process::Command;

const CHILD: &str = "LAMP_IPC_RELAY_EXIT_CHILD";
const SAID: &str = "lamp-test: unknown option --x\nusage: lamp-test --dir DIR [--y N]\n";
const RUNS: usize = 200;

fn main() {
    if std::env::var_os(CHILD).is_some() {
        {
            let _log = lamp_ipc::log_relay::guard();
            eprint!("{SAID}");
        }
        std::process::exit(2);
    }
    for run in 0..RUNS {
        let out = Command::new(std::env::current_exe().unwrap())
            .env(CHILD, "1")
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.code() == Some(2) && said == SAID && out.stdout.is_empty(),
            "run {run}: {:?}, stderr {said:?}, stdout {:?}",
            out.status,
            String::from_utf8_lossy(&out.stdout)
        );
    }
    println!("relay_exit: {RUNS} of {RUNS} said every line");
}
