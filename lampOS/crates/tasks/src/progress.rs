//! How each task handed over is going, while its agent works
//! (`serve --progress`, `docs/tasks.md`, "Progress on the bus").
//!
//! `harness show` is empty while an agent works (measured from lamp-4ace on
//! 2026-10-15, 00:05); `harness ps` lists, for every harness on a machine,
//! its state, when it was last active and an `about` line made of its last
//! few events, which changes as the agent reports. So each machine with
//! tasks being watched is read with one `ps`, on the watch's schedule, and
//! what changed is put on the bus as typed state (`tasks/status`): who has
//! the work, its state, how long, when last active, whether the line
//! changed and how much, and cheap signs read from it without any model.
//! The line itself stays here, in memory, and is handed over on the
//! request socket (`get_progress`) only when the lamp is about to say it.
use crate::{
    engine::{WATCH_EARLY_FOR_MS, WATCH_EARLY_MS, WATCH_LATER_MS, Watched},
    harness::Listed,
};
use lamp_msgs::{AgentState, Sign, TaskProgress, TaskStatus};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The signs, each with the phrases that show it: words in order, matched
/// whole and without regard to case, `#` standing for any number. A sign
/// is a hint that a step was taken, read from words an agent wrote about
/// its own work; it is never taken for proof that the step happened.
///
/// Chosen from how coding agents report (Claude Code's and Codex's own
/// progress summaries, as their makers show them), not measured on the
/// lamp's own tasks yet: how often each fires on real `about` lines is
/// what the first builder hands back (`docs/tasks.md`).
pub const SIGNS: &[(Sign, &[&str])] = &[
    (
        Sign::TestsPass,
        &[
            "tests pass",
            "tests passed",
            "tests passing",
            "tests are passing",
            "tests now pass",
            "tests all pass",
            "all tests pass",
            "all tests passed",
            "test suite passes",
            "suite passes",
            "tests are green",
            "tests green",
            "# passed",
            "# passing",
            "# tests pass",
            "# tests passed",
            "0 failed",
        ],
    ),
    (
        Sign::BuildWorks,
        &[
            "build succeeds",
            "build succeeded",
            "build passes",
            "build passed",
            "build works",
            "build is green",
            "builds cleanly",
            "builds successfully",
            "compiles",
            "compiles cleanly",
            "compiled successfully",
            "clippy is clean",
            "clippy clean",
        ],
    ),
    (
        Sign::Fixed,
        &["fixed", "resolved", "the fix works", "fix works"],
    ),
    (
        Sign::Found,
        &[
            "found",
            "root cause",
            "the cause",
            "the culprit",
            "identified",
            "tracked down",
            "figured out",
        ],
    ),
    (
        Sign::Done,
        &[
            "done",
            "finished",
            "completed",
            "implemented",
            "all set",
            "wrapped up",
        ],
    ),
    (
        Sign::PullRequest,
        &[
            "pull request",
            "opened a pr",
            "opened pr",
            "created a pr",
            "pr opened",
            "pr is up",
            "pr #",
        ],
    ),
];

/// A phrase with one of these among the three words before it is not taken:
/// it is denied, hoped for or still to come ("not fixed", "once the build
/// succeeds", "I'll open a pull request"). Words matched whole.
const NOT_YET: &[&str] = &[
    "not",
    "no",
    "never",
    "cannot",
    "can't",
    "don't",
    "doesn't",
    "didn't",
    "isn't",
    "aren't",
    "wasn't",
    "weren't",
    "won't",
    "hasn't",
    "haven't",
    "couldn't",
    "shouldn't",
    "without",
    "until",
    "unless",
    "if",
    "once",
    "when",
    "whether",
    "to",
    "will",
    "would",
    "should",
    "might",
    "may",
    "must",
    "need",
    "needs",
    "try",
    "trying",
    "next",
    "i'll",
    "we'll",
    "let's",
    "going",
    "before",
];

/// The words of `text`, lowercase, each with where it ends: runs of letters,
/// digits and apostrophes (a curly one read as straight).
fn words(text: &str) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    let mut word = String::new();
    for (at, c) in text.char_indices() {
        let c = if c == '\u{2019}' { '\'' } else { c };
        if c.is_alphanumeric() || c == '\'' {
            word.extend(c.to_lowercase());
        } else if !word.is_empty() {
            found.push((std::mem::take(&mut word), at));
        }
    }
    if !word.is_empty() {
        found.push((word, text.len()));
    }
    found
}

fn matches(pattern: &str, word: &str) -> bool {
    if pattern == "#" {
        !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit())
    } else {
        pattern == word
    }
}

/// The signs in `text` ([`SIGNS`]), each once.
pub fn signs_in(text: &str) -> BTreeSet<Sign> {
    let words = words(text);
    let mut found = BTreeSet::new();
    // A link to a pull request says one was opened, whatever is around it.
    if text.contains("/pull/") {
        found.insert(Sign::PullRequest);
    }
    for (sign, phrases) in SIGNS {
        'phrases: for phrase in *phrases {
            let pattern: Vec<&str> = phrase.split(' ').collect();
            for start in 0..words.len() {
                let Some(run) = words.get(start..start + pattern.len()) else {
                    break;
                };
                if !pattern.iter().zip(run).all(|(p, (w, _))| matches(p, w)) {
                    continue;
                }
                let before = &words[start.saturating_sub(3)..start];
                if before.iter().any(|(w, _)| NOT_YET.contains(&w.as_str())) {
                    continue;
                }
                // A question is not a step taken ("is it fixed?").
                let end = run[run.len() - 1].1;
                if text[end..].trim_start().starts_with('?') {
                    continue;
                }
                found.insert(*sign);
                break 'phrases;
            }
        }
    }
    found
}

/// The share of the distinct words of `after` that are not in `before`,
/// from 0 to 1: how much of a progress line is new.
pub fn new_share(before: Option<&str>, after: &str) -> f32 {
    let old: BTreeSet<String> = before
        .map(|text| words(text).into_iter().map(|(w, _)| w).collect())
        .unwrap_or_default();
    let new: BTreeSet<String> = words(after).into_iter().map(|(w, _)| w).collect();
    if new.is_empty() {
        return 0.0;
    }
    let fresh = new.iter().filter(|word| !old.contains(*word)).count();
    (fresh as f32 / new.len() as f32).clamp(0.0, 1.0)
}

/// A state as `harness ps` writes it, in the bus's words. One this version
/// does not know is `unknown`, never guessed.
pub fn agent_state(listed: &str) -> AgentState {
    match listed {
        "working" => AgentState::Working,
        "idle" => AgentState::Idle,
        "needs-input" => AgentState::NeedsInput,
        "starting" => AgentState::Starting,
        "installing" => AgentState::Installing,
        "stopped" => AgentState::Stopped,
        "offline" => AgentState::Offline,
        "failed" => AgentState::Failed,
        _ => AgentState::Unknown,
    }
}

#[derive(Clone, Debug)]
struct Tracked {
    engine: String,
    state: AgentState,
    working_s: Option<u64>,
    last_active_unix_ms: Option<u64>,
    read_unix_ms: u64,
    about: Option<String>,
    changes: u32,
    changed: bool,
    new_share: f32,
    signs: BTreeSet<Sign>,
    signs_seen: BTreeSet<Sign>,
}

/// What came of one `ps`, for the diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reads {
    pub reads: u64,
    pub failures: u64,
    /// How long the last read took, and the longest, in milliseconds, from
    /// handing it to the CLI worker to its outcome.
    pub last_ms: u64,
    pub most_ms: u64,
}

/// Every task being watched, as last read, and when each machine was.
#[derive(Debug, Default)]
pub struct Tracker {
    tasks: BTreeMap<u64, Tracked>,
    /// When each machine was last asked, by the boot clock, answered or
    /// not: a failed read waits its turn like any other.
    asked: BTreeMap<String, u64>,
    reads: Reads,
    /// A value worth publishing has not been taken yet.
    fresh: bool,
}

impl Tracker {
    pub fn reads(&self) -> Reads {
        self.reads
    }

    /// The machine whose `ps` is due, if any: the watch's schedule, every
    /// half minute while any of its tasks is in its first ten minutes and
    /// every two after, counted from the last time it was asked (from the
    /// youngest task's handing over before the first). Tasks no longer
    /// watched are let go here.
    pub fn due(&mut self, watched: &[Watched], now_ms: u64) -> Option<String> {
        let before = self.tasks.len();
        self.tasks
            .retain(|id, _| watched.iter().any(|w| w.task_id == *id));
        if self.tasks.len() != before {
            self.fresh = true;
        }
        let mut machines: BTreeMap<&str, (bool, Option<u64>)> = BTreeMap::new();
        for task in watched {
            let entry = machines
                .entry(task.machine_id.as_str())
                .or_insert((false, None));
            entry.0 |= task
                .accepted_ago_ms
                .is_some_and(|age| age < WATCH_EARLY_FOR_MS);
            // The youngest handing over in this boot; one from before it
            // counts as long ago.
            let since = task.accepted_ago_ms.unwrap_or(u64::MAX);
            entry.1 = Some(entry.1.map_or(since, |youngest: u64| youngest.min(since)));
        }
        machines
            .into_iter()
            .filter_map(|(machine, (early, youngest))| {
                let every = if early {
                    WATCH_EARLY_MS
                } else {
                    WATCH_LATER_MS
                };
                let ago = match self.asked.get(machine) {
                    Some(at) => now_ms.saturating_sub(*at),
                    None => youngest.unwrap_or(u64::MAX),
                };
                (ago >= every).then_some((ago, machine))
            })
            .max()
            .map(|(_, machine)| machine.to_owned())
    }

    /// The read of `machine` is handed to the CLI worker now.
    pub fn asked(&mut self, machine: &str, now_ms: u64) {
        self.asked.insert(machine.to_owned(), now_ms);
    }

    /// What one `ps` of `machine` came back with. `wall_ms` is the wall
    /// clock as it came; `took_ms` how long it took.
    pub fn read(
        &mut self,
        machine: &str,
        listed: Result<Vec<Listed>, String>,
        watched: &[Watched],
        wall_ms: u64,
        took_ms: u64,
    ) {
        self.reads.last_ms = took_ms;
        self.reads.most_ms = self.reads.most_ms.max(took_ms);
        let Ok(listed) = listed else {
            self.reads.failures = self.reads.failures.saturating_add(1);
            return;
        };
        self.reads.reads = self.reads.reads.saturating_add(1);
        // "Changed" says what this read found; another machine's tasks
        // keep their count and their latest change, not the flag.
        for tracked in self.tasks.values_mut() {
            tracked.changed = false;
        }
        for task in watched.iter().filter(|w| w.machine_id == machine) {
            let row = listed.iter().find(|row| row.agent_id == task.agent_id);
            let state = row.map_or(AgentState::Missing, |row| agent_state(&row.state));
            let about = row.and_then(|row| row.about.clone());
            let working_s = task.accepted_ago_ms.map(|ms| ms / 1_000);
            let last_active_unix_ms = row.and_then(|row| row.last_active_unix_ms);
            match self.tasks.get_mut(&task.task_id) {
                None => {
                    // The first line read is where counting begins.
                    let signs_seen = about.as_deref().map(signs_in).unwrap_or_default();
                    self.tasks.insert(
                        task.task_id,
                        Tracked {
                            engine: task.engine.clone(),
                            state,
                            working_s,
                            last_active_unix_ms,
                            read_unix_ms: wall_ms,
                            about,
                            changes: 0,
                            changed: false,
                            new_share: 0.0,
                            signs: BTreeSet::new(),
                            signs_seen,
                        },
                    );
                }
                Some(tracked) => {
                    tracked.state = state;
                    tracked.working_s = working_s;
                    tracked.read_unix_ms = wall_ms;
                    if last_active_unix_ms.is_some() {
                        tracked.last_active_unix_ms = last_active_unix_ms;
                    }
                    // A line that went away (the agent stopped and is no
                    // longer listed) is not a change of what it says.
                    if let Some(text) = about
                        && tracked.about.as_deref() != Some(text.as_str())
                    {
                        let now = signs_in(&text);
                        let before = tracked.about.as_deref().map(signs_in).unwrap_or_default();
                        tracked.new_share = new_share(tracked.about.as_deref(), &text);
                        tracked.signs = now.difference(&before).copied().collect();
                        tracked.signs_seen.extend(now);
                        tracked.changes = tracked.changes.saturating_add(1);
                        tracked.changed = true;
                        tracked.about = Some(text);
                    }
                }
            }
        }
        self.fresh = true;
    }

    /// The status to publish, if there is a newer one than the last taken.
    pub fn take(&mut self, store_id: &str) -> Option<TaskStatus> {
        if !std::mem::take(&mut self.fresh) {
            return None;
        }
        Some(TaskStatus {
            store_id: store_id.to_owned(),
            tasks: self
                .tasks
                .iter()
                .map(|(task, tracked)| TaskProgress {
                    task: *task,
                    engine: tracked.engine.clone(),
                    state: tracked.state,
                    working_s: tracked.working_s,
                    last_active_unix_ms: tracked.last_active_unix_ms,
                    read_unix_ms: tracked.read_unix_ms,
                    changes: tracked.changes,
                    changed: tracked.changed,
                    new_share: tracked.new_share,
                    signs: tracked.signs.iter().copied().collect(),
                    signs_seen: tracked.signs_seen.iter().copied().collect(),
                })
                .collect(),
        })
    }

    /// The task's progress with its words, for `get_progress`: asked for
    /// only when the lamp is about to say it.
    pub fn words(&self, task: u64) -> Option<Value> {
        let tracked = self.tasks.get(&task)?;
        Some(json!({
            "engine": tracked.engine,
            "state": tracked.state,
            "about": tracked.about,
            "changes": tracked.changes,
            "signs_seen": tracked.signs_seen,
            "last_active_unix_ms": tracked.last_active_unix_ms,
            "read_unix_ms": tracked.read_unix_ms,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signs(text: &str) -> Vec<Sign> {
        signs_in(text).into_iter().collect()
    }

    #[test]
    fn each_sign_fires_on_its_words_and_not_on_their_denial_or_promise() {
        use Sign::*;
        for (text, expected) in [
            ("All 42 tests pass now.", vec![TestsPass]),
            ("Ran cargo test: 118 passed; 0 failed", vec![TestsPass]),
            (
                "The build succeeded after the fix to config.rs",
                vec![BuildWorks],
            ),
            ("It compiles cleanly on Linux", vec![BuildWorks]),
            ("Fixed the off-by-one in the parser", vec![Fixed]),
            ("Found the root cause: a stale cache", vec![Found]),
            ("Implemented the retry and wrote the docs", vec![Done]),
            (
                "Opened a PR: https://github.com/o/r/pull/12",
                vec![PullRequest],
            ),
            ("Created a pull request for review", vec![PullRequest]),
            ("PR #731 is up", vec![PullRequest]),
            (
                "Tests now pass / build is green",
                vec![TestsPass, BuildWorks],
            ),
            // Denied, hoped for, still to come, or asked.
            ("The tests do not pass yet", vec![]),
            ("Not fixed: the race is still there", vec![]),
            ("Once the build succeeds I'll open a pull request", vec![]),
            ("I need to fix the parser", vec![]),
            ("Is it fixed?", vec![]),
            ("Reading src/main.rs and the tests", vec![]),
            ("Will be done in a minute", vec![]),
            // A curly apostrophe is a straight one.
            ("It isn\u{2019}t fixed", vec![]),
        ] {
            assert_eq!(signs(text), expected, "{text:?}");
        }
        // Words, not parts of them.
        assert_eq!(signs("refound unfixed abandoned"), vec![]);
    }

    #[test]
    fn how_much_of_a_line_is_new() {
        assert_eq!(new_share(None, "Reading the parser"), 1.0);
        assert_eq!(
            new_share(Some("Reading the parser"), "reading THE parser"),
            0.0
        );
        assert_eq!(new_share(Some("a b c d"), "a b e f"), 0.5);
        assert_eq!(new_share(Some("a b"), "  "), 0.0);
    }

    fn watched(task_id: u64, machine: &str, agent: &str, age: Option<u64>) -> Watched {
        Watched {
            task_id,
            machine_id: machine.into(),
            agent_id: agent.into(),
            engine: "claude".into(),
            accepted_ago_ms: age,
        }
    }
    fn listed(agent: &str, state: &str, about: Option<&str>) -> Listed {
        Listed {
            agent_id: agent.into(),
            state: state.into(),
            last_active_unix_ms: Some(1_791_964_800_000),
            about: about.map(str::to_owned),
        }
    }
    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";
    const OFFICE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HOME: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn one_read_a_machine_on_the_watchs_schedule() {
        let mut tracker = Tracker::default();
        // Handed over just now: not before half a minute.
        let young = vec![watched(1, OFFICE, A, Some(1_000))];
        assert_eq!(tracker.due(&young, 0), None);
        let young = vec![watched(1, OFFICE, A, Some(WATCH_EARLY_MS))];
        assert_eq!(tracker.due(&young, 0).as_deref(), Some(OFFICE));
        tracker.asked(OFFICE, 100);
        assert_eq!(tracker.due(&young, 100 + WATCH_EARLY_MS - 1), None);
        assert_eq!(
            tracker.due(&young, 100 + WATCH_EARLY_MS).as_deref(),
            Some(OFFICE)
        );
        // Two tasks on one machine are one read; after ten minutes, every two.
        let old = vec![
            watched(1, OFFICE, A, Some(WATCH_EARLY_FOR_MS)),
            watched(2, OFFICE, B, None),
        ];
        assert_eq!(tracker.due(&old, 100 + WATCH_EARLY_MS), None);
        assert_eq!(
            tracker.due(&old, 100 + WATCH_LATER_MS).as_deref(),
            Some(OFFICE)
        );
        // Another machine is read on its own; the one asked longest ago first.
        let both = vec![
            watched(1, OFFICE, A, None),
            watched(2, HOME, B, Some(WATCH_EARLY_MS)),
        ];
        assert_eq!(tracker.due(&both, 100).as_deref(), Some(HOME));
        tracker.asked(HOME, 200);
        assert_eq!(
            tracker.due(&both, 100 + WATCH_LATER_MS).as_deref(),
            Some(OFFICE)
        );
        // Nothing watched: nothing read.
        assert_eq!(tracker.due(&[], u64::MAX), None);
    }

    #[test]
    fn a_change_of_the_line_is_counted_measured_and_signed_and_the_words_stay_here() {
        let mut tracker = Tracker::default();
        let watched = vec![watched(3, OFFICE, A, Some(60_000))];
        // The first line read is where counting begins.
        tracker.read(
            OFFICE,
            Ok(vec![listed(
                A,
                "working",
                Some("Reading the parser; found the cause"),
            )]),
            &watched,
            1_000,
            900,
        );
        let first = tracker.take(&"0".repeat(32)).unwrap();
        first.check().unwrap();
        let task = &first.tasks[0];
        assert_eq!(
            (task.task, task.state, task.changes, task.changed),
            (3, AgentState::Working, 0, false)
        );
        assert_eq!(task.signs_seen, vec![Sign::Found]);
        assert!(task.signs.is_empty());
        assert_eq!(task.working_s, Some(60));
        // Nothing newer to publish until the next read.
        assert!(tracker.take(&"0".repeat(32)).is_none());
        // The same line again: read, not changed.
        tracker.read(
            OFFICE,
            Ok(vec![listed(
                A,
                "working",
                Some("Reading the parser; found the cause"),
            )]),
            &watched,
            2_000,
            800,
        );
        let same = tracker.take(&"0".repeat(32)).unwrap();
        assert_eq!(
            (
                same.tasks[0].changes,
                same.tasks[0].changed,
                same.tasks[0].read_unix_ms
            ),
            (0, false, 2_000)
        );
        // A new line: counted, how much is new, and the signs new in it.
        tracker.read(
            OFFICE,
            Ok(vec![listed(A, "working", Some("Fixed it; all tests pass"))]),
            &watched,
            3_000,
            700,
        );
        let changed = tracker.take(&"0".repeat(32)).unwrap();
        changed.check().unwrap();
        let task = &changed.tasks[0];
        assert_eq!((task.changes, task.changed), (1, true));
        assert_eq!(task.signs, vec![Sign::TestsPass, Sign::Fixed]);
        assert_eq!(
            task.signs_seen,
            vec![Sign::TestsPass, Sign::Fixed, Sign::Found]
        );
        assert!(task.new_share > 0.8, "{}", task.new_share);
        // No words on the bus.
        let text = serde_json::to_string(&changed).unwrap();
        assert!(
            !text.contains("tests pass") && !text.contains("Fixed"),
            "{text}"
        );
        // The words, here, for the lamp about to say them.
        let words = tracker.words(3).unwrap();
        assert_eq!(words["about"], "Fixed it; all tests pass");
        assert_eq!(words["state"], "working");
        assert!(tracker.words(4).is_none());
        // A failed read changes nothing and is counted.
        tracker.read(OFFICE, Err("timeout".into()), &watched, 4_000, 10_000);
        assert!(tracker.take(&"0".repeat(32)).is_none());
        assert_eq!(
            tracker.reads(),
            Reads {
                reads: 3,
                failures: 1,
                last_ms: 10_000,
                most_ms: 10_000
            }
        );
        // Not listed any more: missing, and its last line is kept.
        tracker.read(OFFICE, Ok(vec![]), &watched, 5_000, 600);
        let gone = tracker.take(&"0".repeat(32)).unwrap();
        assert_eq!(
            (
                gone.tasks[0].state,
                gone.tasks[0].changes,
                gone.tasks[0].changed
            ),
            (AgentState::Missing, 1, false)
        );
        // No longer watched: let go, and that is published.
        assert_eq!(tracker.due(&[], 0), None);
        assert!(tracker.take(&"0".repeat(32)).unwrap().tasks.is_empty());
        assert!(tracker.words(3).is_none());
    }

    #[test]
    fn a_read_of_one_machine_flags_only_its_own_tasks_as_changed() {
        let mut tracker = Tracker::default();
        let watched = vec![watched(1, OFFICE, A, None), watched(2, HOME, B, None)];
        for (machine, agent, about) in [(OFFICE, A, "one"), (HOME, B, "two")] {
            tracker.read(
                machine,
                Ok(vec![listed(agent, "working", Some(about))]),
                &watched,
                1,
                1,
            );
        }
        tracker.read(
            OFFICE,
            Ok(vec![listed(A, "working", Some("one more"))]),
            &watched,
            2,
            1,
        );
        tracker.read(
            HOME,
            Ok(vec![listed(B, "working", Some("two more"))]),
            &watched,
            3,
            1,
        );
        let status = tracker.take(&"0".repeat(32)).unwrap();
        assert_eq!(
            status
                .tasks
                .iter()
                .map(|t| (t.task, t.changes, t.changed))
                .collect::<Vec<_>>(),
            vec![(1, 1, false), (2, 1, true)]
        );
        // A state Harness may add later is not guessed.
        assert_eq!(agent_state("thinking"), AgentState::Unknown);
        assert_eq!(agent_state("needs-input"), AgentState::NeedsInput);
    }
}
