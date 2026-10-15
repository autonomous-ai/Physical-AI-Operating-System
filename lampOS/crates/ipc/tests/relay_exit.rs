//! #837: a part that says its error and usage and then ends, as every part
//! does when its arguments are wrong, gets every line to stderr, every time.
//! An ordinary test, which the ARM64 run picks up: it runs its own binary
//! again, as the part, with only this test selected.
use std::process::Command;

const CHILD: &str = "LAMP_IPC_RELAY_EXIT_CHILD";
const SAID: &str = "lamp-test: unknown option --x\nusage: lamp-test --dir DIR [--y N]\n";
const NAME: &str = "a_part_that_ends_after_its_error_says_every_line";
const RUNS: usize = 200;

#[test]
fn a_part_that_ends_after_its_error_says_every_line() {
    if std::env::var_os(CHILD).is_some() {
        {
            let _log = lamp_ipc::log_relay::guard();
            eprint!("{SAID}");
        }
        std::process::exit(2);
    }
    for run in 0..RUNS {
        // The test harness's own lines go to stdout; stderr is the part's.
        let out = Command::new(std::env::current_exe().unwrap())
            .args([NAME, "--exact", "--nocapture", "--test-threads", "1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.code() == Some(2) && said == SAID,
            "run {run}: {:?}, stderr {said:?}",
            out.status
        );
    }
}
