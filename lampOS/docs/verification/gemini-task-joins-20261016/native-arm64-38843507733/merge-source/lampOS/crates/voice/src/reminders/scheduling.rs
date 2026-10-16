//! The optional child must not inherit the audio owner's realtime scheduling.
//! Call once on the child's initial thread, before claim, storage, or threads.
use std::io;

#[cfg(target_os = "linux")]
pub(super) fn background() -> io::Result<()> {
    use thread_priority::{
        NormalThreadSchedulePolicy, ThreadPriority, ThreadSchedulePolicy,
        set_thread_priority_and_policy, thread_native_id, thread_schedule_policy_param,
    };

    let thread = thread_native_id();
    let policy = ThreadSchedulePolicy::Normal(NormalThreadSchedulePolicy::Other);
    // In thread-priority 3.1.1, Other + Min sets static priority 0 and nice 19.
    // This deliberately gives optional persistence less CPU than ordinary work.
    // Either setter can fail, including after policy changed but before nice did.
    set_thread_priority_and_policy(thread, ThreadPriority::Min, policy)
        .map_err(io::Error::other)?;
    let (actual, parameters) = thread_schedule_policy_param(thread).map_err(io::Error::other)?;
    let nice = rustix::process::getpriority_process(None)?;
    if actual != policy || parameters.sched_priority != 0 || nice != 19 {
        return Err(io::Error::other(
            "Schedule consumer background scheduling was not established",
        ));
    }
    Ok(())
}

// Host fixtures keep their existing policy. This is not evidence of Linux
// demotion, which is checked using the real Linux setter and getters below.
#[cfg(not(target_os = "linux"))]
pub(super) fn background() -> io::Result<()> {
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::background;
    use thread_priority::{
        NormalThreadSchedulePolicy, ThreadSchedulePolicy, thread_native_id,
        thread_schedule_policy_param,
    };

    fn scheduling() -> (ThreadSchedulePolicy, i32, i32) {
        let (policy, parameters) = thread_schedule_policy_param(thread_native_id()).unwrap();
        (
            policy,
            parameters.sched_priority,
            rustix::process::getpriority_process(None).unwrap(),
        )
    }

    #[test]
    fn only_the_optional_thread_becomes_other_zero_with_nice_nineteen() {
        let parent_before = scheduling();
        let child = std::thread::spawn(|| {
            background().unwrap();
            scheduling()
        })
        .join()
        .unwrap();
        assert_eq!(
            child,
            (
                ThreadSchedulePolicy::Normal(NormalThreadSchedulePolicy::Other),
                0,
                19,
            )
        );
        assert_eq!(scheduling(), parent_before);
    }
}
