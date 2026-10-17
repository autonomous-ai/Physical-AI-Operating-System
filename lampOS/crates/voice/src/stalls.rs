//! What holds a loop that has a deadline (#1178), with `LAMP_STALLS=on`.
//!
//! On lamp-4ace on 2026-10-17 the voice ended at 00:00:06, five seconds after
//! the lamp's daily log jobs began, on an "authority snapshot has expired".
//! Every loop of the voice that holds an authority renews it against a lease
//! of 250 ms, 100 ms during a turn, so some loop was held past that. Log lines
//! cannot be it: they go through [`lamp_ipc::log_relay`] and never wait. This
//! says which loop it was, and what held it.
//!
//! Each timed loop wraps a pass in [`Stalls::begin`]. A pass's own work of
//! [`OVER`] or more, or a gap of that much from the end of one pass's work to
//! the start of the next (its wait overran), is one line in
//! `stalls-<role>.log` under [`DEFAULT_DIR`] (or [`DIR_ENVIRONMENT`]), never
//! on stderr: which loop, how long, this thread's page faults and context
//! switches over exactly that span (`getrusage(RUSAGE_THREAD)`), and, read
//! just after, the lamp's major faults, dirty and writeback pages and
//! runnable and blocked processes (`/proc/vmstat`, `/proc/stat`: the lamps'
//! kernel has no `/proc/pressure`). A work line says how its pass ended:
//! `done` at its normal end, `skipped` when it left early and the loop went
//! on (a `continue` not preceded by [`Pass::done`]), or `loop` when the loop
//! itself ended in it, on an error or a stop: the pass that ends a worker.
//!
//! The loop's own cost: two `getrusage` calls and two clock reads a pass, and
//! at a stall one `try_send` of a few numbers. A thread of its own reads
//! `/proc`, builds the line and writes it. Off unless set: then a pass is one
//! branch, and nothing is read or written.
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const ENVIRONMENT: &str = "LAMP_STALLS";
pub const DIR_ENVIRONMENT: &str = "LAMP_STALLS_DIR";
pub const DEFAULT_DIR: &str = "/run/lampos-voice";
/// A pass or a gap this long is recorded: half the lease during a turn.
pub const OVER: Duration = Duration::from_millis(50);
/// A role's file starts over past this, the one before kept as
/// `stalls-<role>.old.log`, so the newest lines are always there.
pub const MOST_BYTES: u64 = 512 * 1024;
/// Records waiting for the recorder; one that finds the queue full is
/// dropped, never waited for, and the next line says how many were.
const QUEUE: usize = 64;
/// How often the recorder reads the lamp's counters, so a stall's line can
/// say what the lamp did from just before it began; and how many readings
/// it keeps (10 s).
const SAMPLE: Duration = Duration::from_millis(100);
const SAMPLES: usize = 100;
/// Recorders whose loop has ended, still writing: waited for once, at the
/// end of `main` ([`finish_all`]), after every device is closed, never in a
/// loop's own way out.
static ENDED: std::sync::Mutex<Vec<JoinHandle<()>>> = std::sync::Mutex::new(Vec::new());

/// Wait, at most `longest`, for every ended loop's recorder to write what it
/// was given. For the end of `main`, beside `log_relay::finish`.
pub fn finish_all(longest: Duration) {
    let ended = ENDED
        .lock()
        .map(|mut ended| std::mem::take(&mut *ended))
        .unwrap_or_default();
    wait_for(ended, longest);
}

/// [`finish_all`] when it goes, so a panic that unwinds out of `main` still
/// gives the recorders their moment: the `"loop"` line of a pass that
/// panicked is written while the stack unwinds (the reviewer on #1179).
/// `let stalls_end = Finishing(..)` before the work, dropped after it.
pub struct Finishing(pub Duration);

impl Drop for Finishing {
    fn drop(&mut self) {
        finish_all(self.0);
    }
}

fn wait_for(recorders: Vec<JoinHandle<()>>, longest: Duration) {
    let until = Instant::now() + longest;
    while recorders.iter().any(|recorder| !recorder.is_finished()) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The switch and its folder, for a worker started with a cleared
/// environment, so each worker records its own loop too.
pub fn passed_on() -> Vec<(&'static str, std::ffi::OsString)> {
    [ENVIRONMENT, DIR_ENVIRONMENT]
        .into_iter()
        .filter_map(|name| Some((name, std::env::var_os(name)?)))
        .collect()
}

/// One thread's counters over a span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Thread {
    pub minor_faults: u64,
    pub major_faults: u64,
    pub voluntary: u64,
    pub involuntary: u64,
}

impl Thread {
    fn since(self, before: Thread) -> Thread {
        Thread {
            minor_faults: self.minor_faults.saturating_sub(before.minor_faults),
            major_faults: self.major_faults.saturating_sub(before.major_faults),
            voluntary: self.voluntary.saturating_sub(before.voluntary),
            involuntary: self.involuntary.saturating_sub(before.involuntary),
        }
    }
}

/// The whole lamp's, as the recorder reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct System {
    pub major_faults: u64,
    pub dirty_pages: u64,
    pub writeback_pages: u64,
    pub running: u64,
    pub blocked: u64,
}

/// Where the time and the counters come from: the kernel, or a test's own.
pub trait Counters: Send + 'static {
    fn now(&mut self) -> Instant {
        Instant::now()
    }
    /// The calling thread's, read on the loop.
    fn thread(&mut self) -> Thread;
    /// The lamp's, read on the recorder.
    fn system(&mut self) -> Option<System>;
}

pub struct Kernel;
impl Counters for Kernel {
    /// `RUSAGE_THREAD` is Linux's; elsewhere (a builder's Mac) nothing is
    /// counted, and the lines still say when and for how long.
    #[cfg(not(target_os = "linux"))]
    fn thread(&mut self) -> Thread {
        Thread::default()
    }
    #[cfg(target_os = "linux")]
    fn thread(&mut self) -> Thread {
        use nix::sys::resource::{UsageWho, getrusage};
        let Ok(usage) = getrusage(UsageWho::RUSAGE_THREAD) else {
            return Thread::default();
        };
        let count = |value: std::os::raw::c_long| u64::try_from(value).unwrap_or(0);
        Thread {
            minor_faults: count(usage.minor_page_faults()),
            major_faults: count(usage.major_page_faults()),
            voluntary: count(usage.voluntary_context_switches()),
            involuntary: count(usage.involuntary_context_switches()),
        }
    }
    fn system(&mut self) -> Option<System> {
        let vmstat = std::fs::read_to_string("/proc/vmstat").ok()?;
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        Some(parse_system(&vmstat, &stat))
    }
}

pub fn parse_system(vmstat: &str, stat: &str) -> System {
    let find = |text: &str, name: &str| {
        text.lines()
            .find_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some(name)).then(|| words.next()?.parse().ok())?
            })
            .unwrap_or(0)
    };
    System {
        major_faults: find(vmstat, "pgmajfault"),
        dirty_pages: find(vmstat, "nr_dirty"),
        writeback_pages: find(vmstat, "nr_writeback"),
        running: find(stat, "procs_running"),
        blocked: find(stat, "procs_blocked"),
    }
}

/// How a pass ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    /// [`Pass::done`]: its normal end, before its wait.
    Done,
    /// Left early, and the loop went on.
    Skipped,
    /// The loop ended in it: an error or a stop.
    Loop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// A pass's own work.
    Work(End),
    /// From the end of one pass's work to the start of the next.
    Gap,
}

/// What the loop hands the recorder: a few numbers, no allocation.
#[derive(Clone, Copy, Debug)]
struct Record {
    kind: Kind,
    began_us: u64,
    took: Duration,
    thread: Thread,
    /// Records the queue had no room for since the last one sent.
    lost_before: u64,
}

/// One loop's recorder. Dropped when the loop ends, it waits a moment for
/// the lines still queued.
pub struct Stalls<C: Counters = Kernel> {
    on: Option<On<C>>,
}

struct On<C: Counters> {
    counters: C,
    sender: Option<SyncSender<Record>>,
    recorder: Option<JoinHandle<()>>,
    /// The end of the last pass's work, and the thread's counters then.
    last_end: Option<(Instant, Thread)>,
    /// A long pass that left early, held until it is known whether the loop
    /// went on after it.
    left: Option<Record>,
    lost: u64,
}

impl Stalls {
    /// On with `LAMP_STALLS=on`, in `LAMP_STALLS_DIR` or [`DEFAULT_DIR`].
    pub fn from_environment(role: &'static str) -> Self {
        let dir = std::env::var_os(DIR_ENVIRONMENT)
            .map_or_else(|| PathBuf::from(DEFAULT_DIR), PathBuf::from);
        Self::from_value(
            role,
            std::env::var(ENVIRONMENT).ok().as_deref(),
            &dir,
            Kernel,
            Kernel,
        )
    }
}

impl<C: Counters> Stalls<C> {
    /// On only for `Some("on")`; `system` is read on the recorder's thread.
    pub fn from_value<S: Counters>(
        role: &'static str,
        value: Option<&str>,
        dir: &Path,
        counters: C,
        system: S,
    ) -> Self {
        if value != Some("on") {
            return Self { on: None };
        }
        let (sender, receiver) = mpsc::sync_channel(QUEUE);
        let path = dir.join(format!("stalls-{role}.log"));
        let recorder = std::thread::Builder::new()
            .name("lamp-stalls".into())
            .spawn(move || record(role, &path, receiver, system));
        match recorder {
            Ok(recorder) => Self {
                on: Some(On {
                    counters,
                    sender: Some(sender),
                    recorder: Some(recorder),
                    last_end: None,
                    left: None,
                    lost: 0,
                }),
            },
            Err(error) => {
                eprintln!("lamp-voice: stalls of {role} not recorded: {error}");
                Self { on: None }
            }
        }
    }

    pub fn on(&self) -> bool {
        self.on.is_some()
    }

    /// The start of a pass; a gap since the last one's work is recorded. The
    /// pass ends when the guard goes: [`Pass::done`] before its wait, or
    /// dropped when it leaves early.
    pub fn begin(&mut self) -> Pass<'_, C> {
        let Some(on) = self.on.as_mut() else {
            return Pass {
                stalls: self,
                began: None,
            };
        };
        if let Some(mut left) = on.left.take() {
            left.kind = Kind::Work(End::Skipped);
            on.send(left);
        }
        let now = on.counters.now();
        let thread = on.counters.thread();
        if let Some((end, before)) = on.last_end.take() {
            let gap = now.saturating_duration_since(end);
            if gap >= OVER {
                on.send(Record {
                    kind: Kind::Gap,
                    began_us: lamp_ipc::monotonic_us().saturating_sub(gap.as_micros() as u64),
                    took: gap,
                    thread: thread.since(before),
                    lost_before: 0,
                });
            }
        }
        Pass {
            stalls: self,
            began: Some((now, thread, lamp_ipc::monotonic_us())),
        }
    }
}

impl<C: Counters> On<C> {
    fn send(&mut self, mut record: Record) {
        if let Some(sender) = &self.sender {
            record.lost_before = self.lost;
            // Never waited for: a full queue loses this one, and says so next.
            match sender.try_send(record) {
                Ok(()) => self.lost = 0,
                Err(_) => self.lost += 1,
            }
        }
    }
}

impl<C: Counters> Drop for Stalls<C> {
    fn drop(&mut self) {
        let Some(on) = self.on.as_mut() else {
            return;
        };
        // The loop ended in the pass that left last.
        if let Some(mut left) = on.left.take() {
            left.kind = Kind::Work(End::Loop);
            on.send(left);
        }
        // Never waited for here: the loop's own cleanup (a microphone to
        // close, a ring to blank) comes first. `finish_all` waits at the end.
        on.sender = None;
        if let Some(recorder) = on.recorder.take()
            && let Ok(mut ended) = ENDED.lock()
        {
            ended.push(recorder);
        }
    }
}

impl<C: Counters> Stalls<C> {
    /// Ends this loop's recording and waits, at most `longest`, for its
    /// lines: for a test, which reads them next.
    pub fn finish(mut self, longest: Duration) {
        let recorder = self.on.as_mut().and_then(|on| on.recorder.take());
        drop(self);
        wait_for(recorder.into_iter().collect(), longest);
    }
}

/// One pass of a timed loop.
pub struct Pass<'a, C: Counters = Kernel> {
    stalls: &'a mut Stalls<C>,
    began: Option<(Instant, Thread, u64)>,
}

impl<C: Counters> Pass<'_, C> {
    /// The pass's work is over (before its wait).
    pub fn done(mut self) {
        self.end(true);
    }

    fn end(&mut self, done: bool) {
        let Some((began, before, began_us)) = self.began.take() else {
            return;
        };
        let Some(on) = self.stalls.on.as_mut() else {
            return;
        };
        let now = on.counters.now();
        let thread = on.counters.thread();
        let took = now.saturating_duration_since(began);
        if took >= OVER {
            let record = Record {
                kind: Kind::Work(End::Done),
                began_us,
                took,
                thread: thread.since(before),
                lost_before: 0,
            };
            if done {
                on.send(record);
            } else {
                on.left = Some(record);
            }
        }
        on.last_end = Some((now, thread));
    }
}

impl<C: Counters> Drop for Pass<'_, C> {
    fn drop(&mut self) {
        self.end(false);
    }
}

/// The recorder's thread: a line for each record, in a file that starts
/// over at [`MOST_BYTES`]. Between records it reads the lamp's counters
/// every [`SAMPLE`], so a line says what the lamp did from just before the
/// stall began.
fn record(role: &'static str, path: &Path, records: Receiver<Record>, mut system: impl Counters) {
    let mut file: Option<(File, u64)> = None;
    let mut samples: std::collections::VecDeque<(u64, System)> =
        std::collections::VecDeque::with_capacity(SAMPLES);
    let mut line = String::with_capacity(512);
    let pid = std::process::id();
    let mut said_unopened = false;
    loop {
        let record = match records.recv_timeout(SAMPLE) {
            Ok(record) => record,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(now) = system.system() {
                    if samples.len() == SAMPLES {
                        samples.pop_front();
                    }
                    samples.push_back((lamp_ipc::monotonic_us(), now));
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let now = system.system();
        // The last reading from before the stall began, if any is that old.
        let before = samples
            .iter()
            .rev()
            .find(|(at, _)| *at <= record.began_us)
            .copied();
        line.clear();
        write_line(&mut line, role, pid, &record, before, now);
        if file.is_none() {
            file = open(path);
        }
        // Past the bound, as found or as grown: the one before is kept.
        if file
            .as_ref()
            .is_some_and(|(_, size)| size + line.len() as u64 > MOST_BYTES)
        {
            let old = path.with_file_name(format!("stalls-{role}.old.log"));
            let _ = std::fs::rename(path, old);
            file = open(path);
        }
        match file.as_mut() {
            Some((file, size)) => {
                if file.write_all(line.as_bytes()).is_ok() {
                    *size += line.len() as u64;
                }
            }
            // Said once, so a wrong folder is not silent.
            None if !said_unopened => {
                said_unopened = true;
                eprintln!(
                    "lamp-voice: stalls of {role} cannot be written to {}",
                    path.display()
                );
            }
            None => {}
        }
    }
}

fn open(path: &Path) -> Option<(File, u64)> {
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)
        .ok()?;
    let size = file.metadata().map_or(0, |meta| meta.len());
    Some((file, size))
}

fn write_line(
    line: &mut String,
    role: &str,
    pid: u32,
    record: &Record,
    before: Option<(u64, System)>,
    now: Option<System>,
) {
    use std::fmt::Write as _;
    let (kind, end) = match record.kind {
        Kind::Work(End::Done) => ("work", Some("done")),
        Kind::Work(End::Skipped) => ("work", Some("skipped")),
        Kind::Work(End::Loop) => ("work", Some("loop")),
        Kind::Gap => ("gap", None),
    };
    let t = record.thread;
    let _ = write!(
        line,
        "{{\"role\":\"{role}\",\"pid\":{pid},\"kind\":\"{kind}\",\"ms\":{},\"at_us\":{}",
        record.took.as_micros() as f64 / 1000.0,
        record.began_us
    );
    if let Some(end) = end {
        let _ = write!(line, ",\"end\":\"{end}\"");
    }
    if record.lost_before > 0 {
        let _ = write!(line, ",\"lost_before\":{}", record.lost_before);
    }
    let _ = write!(
        line,
        ",\"thread\":{{\"major_faults\":{},\"minor_faults\":{},\"voluntary_switches\":{},\"involuntary_switches\":{}}}",
        t.major_faults, t.minor_faults, t.voluntary, t.involuntary
    );
    match now {
        Some(now) => {
            // From the reading just before the stall began (at most
            // `SAMPLE` before it) to just after it ended.
            let _ = write!(line, ",\"system\":{{\"major_faults_during\":");
            match before {
                Some((at, before)) => {
                    let _ = write!(
                        line,
                        "{},\"read_ms_before\":{}",
                        now.major_faults.saturating_sub(before.major_faults),
                        record.began_us.saturating_sub(at) / 1000
                    );
                }
                None => line.push_str("null"),
            }
            let _ = write!(
                line,
                ",\"dirty_pages\":{},\"writeback_pages\":{},\"running\":{},\"blocked\":{}}}",
                now.dirty_pages, now.writeback_pages, now.running, now.blocked
            );
        }
        None => line.push_str(",\"system\":null"),
    }
    line.push_str("}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    /// The time and the thread's counters, both moved by the test.
    #[derive(Clone)]
    struct Set(Arc<[AtomicU64; 4]>, Instant, Arc<AtomicU64>);
    impl Default for Set {
        fn default() -> Self {
            Self(Arc::default(), Instant::now(), Arc::default())
        }
    }
    impl Set {
        /// Time passes only when the test says so.
        fn hold(&self, ms: u64) {
            self.2.fetch_add(ms, Ordering::SeqCst);
        }
        fn add(&self, minor: u64, major: u64, voluntary: u64, involuntary: u64) {
            for (slot, by) in self.0.iter().zip([minor, major, voluntary, involuntary]) {
                slot.fetch_add(by, Ordering::SeqCst);
            }
        }
    }
    impl Counters for Set {
        fn now(&mut self) -> Instant {
            self.1 + Duration::from_millis(self.2.load(Ordering::SeqCst))
        }
        fn thread(&mut self) -> Thread {
            let read = |n: usize| self.0[n].load(Ordering::SeqCst);
            Thread {
                minor_faults: read(0),
                major_faults: read(1),
                voluntary: read(2),
                involuntary: read(3),
            }
        }
        fn system(&mut self) -> Option<System> {
            Some(System {
                major_faults: 100,
                dirty_pages: 7,
                writeback_pages: 5,
                running: 4,
                blocked: 2,
            })
        }
    }

    /// Counters that must never be read.
    struct Never;
    impl Counters for Never {
        fn thread(&mut self) -> Thread {
            panic!("a thread counter was read with stalls off")
        }
        fn system(&mut self) -> Option<System> {
            panic!("a system counter was read with stalls off")
        }
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stalls-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn a_long_pass_is_one_line_with_the_counters_of_exactly_that_pass() {
        let dir = folder("pass");
        let set = Set::default();
        let mut stalls = Stalls::from_value("ring", Some("on"), &dir, set.clone(), set.clone());
        // Short passes: nothing.
        for _ in 0..5 {
            let pass = stalls.begin();
            set.add(1, 0, 1, 0);
            pass.done();
        }
        // Counters moved before the long pass are not its.
        set.add(1000, 50, 1000, 1000);
        let pass = stalls.begin();
        set.add(3, 2, 1, 4);
        set.hold(60);
        pass.done();
        // The next pass begins at once: no gap line beside the work line.
        stalls.begin().done();
        stalls.finish(Duration::from_secs(5));
        let written = lines(&dir.join("stalls-ring.log"));
        assert_eq!(written.len(), 1, "{written:?}");
        let line = &written[0];
        assert_eq!(line["role"], "ring");
        assert_eq!(line["kind"], "work");
        assert_eq!(line["end"], "done");
        assert_eq!(line["ms"], 60.0);
        assert_eq!(line["thread"]["minor_faults"], 3);
        assert_eq!(line["thread"]["major_faults"], 2);
        assert_eq!(line["thread"]["voluntary_switches"], 1);
        assert_eq!(line["thread"]["involuntary_switches"], 4);
        assert_eq!(line["system"]["writeback_pages"], 5);
        assert_eq!(line["system"]["blocked"], 2);
    }

    /// A pass that leaves early: the guard goes without `done`.
    fn ends_early(stalls: &mut Stalls<Set>, set: &Set) -> Result<(), ()> {
        let _pass = stalls.begin();
        set.hold(70);
        Err(())
    }

    #[test]
    fn a_wait_that_overran_is_a_gap_and_a_pass_that_left_early_says_whether_the_loop_went_on() {
        let dir = folder("gap");
        let set = Set::default();
        let mut stalls = Stalls::from_value("speaker", Some("on"), &dir, set.clone(), set.clone());
        stalls.begin().done();
        // The wait between two passes, held: counted from the work's end.
        set.add(0, 7, 0, 0);
        set.hold(60);
        stalls.begin().done();
        // Left early, and the loop goes on: skipped.
        assert!(ends_early(&mut stalls, &set).is_err());
        stalls.begin().done();
        // Left early, and the loop ends in it: the pass that ended it.
        assert!(ends_early(&mut stalls, &set).is_err());
        stalls.finish(Duration::from_secs(5));
        let written = lines(&dir.join("stalls-speaker.log"));
        assert_eq!(written.len(), 3, "{written:?}");
        assert_eq!(written[0]["kind"], "gap");
        assert_eq!(written[0]["thread"]["major_faults"], 7);
        assert_eq!(written[0]["ms"], 60.0);
        assert!(written[0].get("end").is_none());
        assert_eq!(written[1]["kind"], "work");
        assert_eq!(written[1]["end"], "skipped");
        assert_eq!(written[1]["ms"], 70.0);
        assert_eq!(written[2]["end"], "loop");
    }

    #[test]
    fn only_on_turns_it_on_and_off_reads_and_writes_nothing() {
        let dir = folder("off");
        for value in [None, Some("1"), Some("off"), Some("ON"), Some("")] {
            let mut stalls = Stalls::from_value("coordinator", value, &dir, Never, Never);
            assert!(!stalls.on(), "{value:?}");
            stalls.begin().done();
            let _ = stalls.begin();
            stalls.begin().done();
        }
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
        assert!(
            Stalls::from_value(
                "coordinator",
                Some("on"),
                &dir,
                Set::default(),
                Set::default()
            )
            .on()
        );
    }

    /// The speaker's and the capture's loops end a pass with
    /// `finish_tick(tick)`, its wait; every one of them, the speaker's six
    /// lapse exits of #1214 among them (the midnight path, #1178), ends the
    /// pass's work with `done()` first, so a long lapse tick is one `work`
    /// line and its wait a `gap`. Those loops need a sound device, so no test
    /// runs them; this holds their shape.
    #[test]
    fn every_audio_loop_wait_ends_its_pass_with_done_first() {
        let source = include_str!("audio_workers.rs");
        let lines: Vec<&str> = source.lines().collect();
        let waits: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.trim() == "finish_tick(tick);")
            .map(|(at, _)| at)
            .collect();
        assert!(waits.len() >= 10, "{} waits found", waits.len());
        for at in waits {
            assert_eq!(
                lines[at - 1].trim(),
                "stall_pass.done();",
                "audio_workers.rs:{} waits without ending its pass",
                at + 1
            );
        }
    }

    #[test]
    fn a_pass_that_panics_is_written_as_the_loops_end_while_the_stack_unwinds() {
        let dir = folder("panic");
        let set = Set::default();
        let (counters, system) = (set.clone(), set.clone());
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _end = Finishing(Duration::from_secs(5));
            let mut stalls = Stalls::from_value("speaker", Some("on"), &dir, counters, system);
            let _pass = stalls.begin();
            set.hold(80);
            panic!("a pass that panics");
        }));
        assert!(unwound.is_err());
        let written = lines(&dir.join("stalls-speaker.log"));
        assert_eq!(written.len(), 1, "{written:?}");
        assert_eq!(written[0]["end"], "loop");
        assert_eq!(written[0]["ms"], 80.0);
    }

    #[test]
    fn a_full_file_starts_over_and_keeps_the_one_before() {
        let dir = folder("full");
        let path = dir.join("stalls-capture.log");
        std::fs::write(&path, vec![b'x'; (MOST_BYTES - 10) as usize]).unwrap();
        let set = Set::default();
        let mut stalls = Stalls::from_value("capture", Some("on"), &dir, set.clone(), set.clone());
        let pass = stalls.begin();
        set.hold(60);
        pass.done();
        stalls.finish(Duration::from_secs(5));
        assert_eq!(lines(&path).len(), 1);
        let old = std::fs::metadata(dir.join("stalls-capture.old.log")).unwrap();
        assert_eq!(old.len(), MOST_BYTES - 10);
    }

    #[test]
    fn the_kernels_counters_move_on_linux_and_proc_lines_are_parsed() {
        // Read, and moved by touching fresh memory on Linux (2050 faults for
        // these 8 MiB on lamp-75c9); never backwards anywhere.
        let mut kernel = Kernel;
        let before = kernel.thread();
        let mut touched = vec![0u8; 8 << 20];
        for at in (0..touched.len()).step_by(4096) {
            touched[at] = 1;
        }
        // Kept, so an optimised build touches it (CI tests with --release).
        std::hint::black_box(&mut touched);
        let after = kernel.thread();
        if cfg!(target_os = "linux") {
            assert!(
                after.minor_faults > before.minor_faults,
                "{before:?} {after:?}"
            );
        }
        assert!(after.minor_faults >= before.minor_faults);
        assert!(after.major_faults >= before.major_faults);
        assert!(after.voluntary >= before.voluntary);
        assert!(after.involuntary >= before.involuntary);
        assert_eq!(touched.iter().filter(|byte| **byte == 1).count(), 2048);
        let vmstat = "nr_dirty 120\nnr_writeback 8\npgmajfault 4321\n";
        let stat = "cpu  1 2 3\nprocs_running 3\nprocs_blocked 1\n";
        assert_eq!(
            parse_system(vmstat, stat),
            System {
                major_faults: 4321,
                dirty_pages: 120,
                writeback_pages: 8,
                running: 3,
                blocked: 1
            }
        );
    }
}
