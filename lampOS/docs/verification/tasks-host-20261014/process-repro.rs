use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};
use std::{os::unix::process::CommandExt, process::{Command,Stdio}, time::{Duration,Instant}};
fn main() {
    if std::env::args().nth(1).as_deref() == Some("child") { std::thread::sleep(Duration::from_millis(100)); println!("scripted child"); return; }
    let mut child = Command::new(std::env::current_exe().unwrap()).arg("child").stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn().unwrap();
    let pid = Pid::from_child(&child);
    println!("child pid {pid:?}");
    println!("live group CONT {:?}",kill_process_group(pid,Signal::CONT));
    println!("live group probe {:?}",rustix::process::test_kill_process_group(pid));
    let until = Instant::now() + Duration::from_secs(1);
    loop {
        let result = waitid(WaitId::Pid(pid), WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT);
        if !matches!(result, Ok(None)) { println!("waitid {result:?}"); break; }
        if Instant::now() >= until { println!("waitid deadline"); break; }
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("kill group {:?}", kill_process_group(pid,Signal::KILL));
    println!("exited group probe {:?}",rustix::process::test_kill_process_group(pid));
    println!("child.kill {:?}",child.kill());
    println!("child.try_wait {:?}",child.try_wait());
    println!("child.try_wait again {:?}",child.try_wait());
}
