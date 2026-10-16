//! News from the agents the lamp handed work to (`docs/tasks.md`, "Telling
//! the person").
//!
//! The lamp is the person's chief of staff: it hands work to Claude Code or
//! Codex on another computer ([`crate::agent_tasks`]) and tells the person
//! when the work comes back, unasked, the way someone at the next desk
//! would: "Oh, Claude Code just finished the names you asked for." The
//! tasks part watches each task handed over (unless `lamp-tasks serve --no-watch`)
//! and keeps what its agent came back with once it has finished, stopped,
//! or stopped to ask something. A thread of the voice's own reads that
//! every ten seconds and offers each piece of news once; the coordinator
//! tells it at a good moment (someone there, nothing being said, the room
//! not busy) as a turn the lamp begins itself, and marks it told. What was
//! told is kept in a file, so a restart does not tell it again.
//!
//! On whenever the tasks part is ([`crate::agent_tasks::ENVIRONMENT`]).
//! `LAMP_TASKS_TOLD` names the file of what was told, [`DEFAULT_TOLD`]
//! unless set.
//!
//! **Progress** while an agent works (#732, #752) is told too, only with
//! [`PROGRESS_ENVIRONMENT`] `on`: the policy decides on `policy/decisions`
//! whether a change of the agent's progress line is worth telling now
//! (`lamp-policy --progress --act progress`), and a `say_now` that is fresh,
//! of this store and about the newest change on `tasks/status` is fetched
//! (`get_progress`) and offered here like any other news, under the same
//! gates.
use lamp_bus::{Bus, Subscriber};
use lamp_msgs::{AgentState, Decision, Sign, TaskStatus};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Where the coordinator reads the file of what was told.
pub const TOLD_ENVIRONMENT: &str = "LAMP_TASKS_TOLD";
/// `skill`: news of work finished, stopped or asking waits for a skill's
/// `from: tasks` ask, at most [`crate::say::SKILL_WAITS_US`], and is then
/// told with its ticket, or by the voice itself (#939, (c)). Progress is
/// the Policy's to time already, and is told as it was.
pub const BY_SKILL_ENVIRONMENT: &str = "LAMP_TASK_NEWS";

/// Whether news waits for a skill ([`BY_SKILL_ENVIRONMENT`]).
pub fn by_skill() -> bool {
    std::env::var(BY_SKILL_ENVIRONMENT).as_deref() == Ok("skill")
}
pub const DEFAULT_TOLD: &str = "/var/lib/lampOS/voice/tasks-told";
/// How often the tasks part is read.
const READ_EVERY: Duration = Duration::from_secs(10);
/// Each request to the tasks part, connecting included.
const ASK_DEADLINE: Duration = Duration::from_millis(1_500);
/// News older than this is not told unasked: the day it belonged to is over.
const OLDEST_MS: u64 = 12 * 3_600_000;
/// Within this the work has "just" finished; after it, it finished earlier.
const JUST_MS: u64 = 10 * 60_000;
/// The most of an agent's answer put before the model.
pub(crate) const MOST_ANSWER_CHARS: usize = 1_200;
/// The most of the task's own words.
pub(crate) const MOST_TASK_CHARS: usize = 300;
/// When a note is too long, the task's words are cut to this many first:
/// the person knows their task from a few words, and the gist is in the
/// answer. With a task of 300 characters the note kept 163 bytes of a
/// long answer; on lamp-4ace on 2026-10-15 at 08:02 the lamp then told a
/// 300-word story as only "the rest is waiting on your computer" (#757).
const TASK_CHARS_KEPT_FIRST: usize = 100;
/// Nothing said by or to the lamp for this long before news is told.
pub const QUIET_US: u64 = 15_000_000;
/// Without eyes that say someone is there, something said this recently
/// says it.
pub const HEARD_WITHIN_US: u64 = 120_000_000;
/// Progress news is told only with this set to `on`, read at start (#752).
/// Off unless set: without it nothing more is subscribed to, asked or kept.
pub const PROGRESS_ENVIRONMENT: &str = "LAMP_PROGRESS_NEWS";
/// A decision to say progress is let go this long after it was made: told
/// later, it would be old news.
pub const PROGRESS_FRESH_MS: u64 = 60_000;
/// The most of the agent's progress line put before the model.
pub(crate) const MOST_ABOUT_CHARS: usize = 400;
/// How long the progress reader waits for a decision before it looks again
/// whether it is to stop.
const PROGRESS_WAIT: Duration = Duration::from_millis(200);

/// One piece of news, ready to tell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct News {
    /// The tasks part's store, task and observation (or `hand-over`, or
    /// `progress-<run>-<change>`): what is told once.
    pub key: String,
    /// The note the model is given.
    pub note: String,
    /// Let go if not told by then: progress is told fresh or not at all.
    pub until: Option<Instant>,
    /// For progress, which change of which task it tells: withdrawn once
    /// `tasks/status` no longer shows that change of that task at work.
    pub progress: Option<ProgressOf>,
}

/// The piece of progress a note tells: a change is named by its count and
/// the run of the tasks part that counted it, since counts begin again at
/// each run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressOf {
    pub store: String,
    pub task: u64,
    pub change: u32,
    pub run: u64,
}

/// Whether `of` is still the newest change of a task whose agent works,
/// as the newest `tasks/status` says: what a progress note may still say.
fn still_current(status: Option<&TaskStatus>, of: &ProgressOf) -> bool {
    status.is_some_and(|status| {
        status.store_id == of.store
            && status.run == of.run
            && status.tasks.iter().any(|entry| {
                entry.task == of.task
                    && entry.state == AgentState::Working
                    && entry.changes == of.change
            })
    })
}

pub struct TaskNews {
    offered: Receiver<News>,
    told: Sender<String>,
    waiting: VecDeque<News>,
    /// What the file says was told, as the reading thread last found it:
    /// news the person asked for and heard ([`crate::agent_tasks`]) is not
    /// told again.
    told_already: Arc<Mutex<BTreeSet<String>>>,
    /// Set when this is let go, so that the progress reader ends.
    stopping: Arc<AtomicBool>,
    /// The newest `tasks/status` the progress reader has: a queued note is
    /// checked against it, never waited on.
    status: Arc<Mutex<Option<TaskStatus>>>,
}

impl Drop for TaskNews {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
    }
}

/// Whether progress news is on: `on`, and nothing else.
pub fn progress_on(value: Option<&str>) -> bool {
    value == Some("on")
}

/// The file of what was told.
pub fn told_file() -> PathBuf {
    std::env::var_os(TOLD_ENVIRONMENT)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_TOLD))
}

/// Keep `key` as told.
pub fn keep_told(file: &Path, key: &str) {
    let kept = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut file| writeln!(file, "{key}"));
    if let Err(error) = kept {
        eprintln!(
            "lamp-voice: task news told, not kept in {}: {error}",
            file.display()
        );
    }
}

/// Whether `key` is in the file of what was told: news told unasked, or
/// heard when the person asked.
pub(crate) fn was_told(file: &Path, key: &str) -> bool {
    read_told(file).contains(key)
}

fn read_told(file: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(file)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

impl TaskNews {
    /// Reading starts on a thread of its own when the tasks part is set;
    /// progress too, with [`PROGRESS_ENVIRONMENT`] `on`, on the lamp's bus.
    pub fn from_environment() -> Option<Self> {
        let directory = std::env::var_os(crate::agent_tasks::ENVIRONMENT)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())?;
        let bus = if progress_on(std::env::var(PROGRESS_ENVIRONMENT).ok().as_deref()) {
            match Bus::from_environment() {
                Ok(bus) => Some(bus),
                Err(error) => {
                    eprintln!(
                        "lamp-voice: progress news is on and the bus cannot be used: {error}"
                    );
                    None
                }
            }
        } else {
            None
        };
        Self::start(&directory, told_file(), bus.as_ref())
    }

    /// Read the tasks part in `directory` and keep what is told in
    /// `told_file`; with a `bus`, take progress decisions from it too.
    /// Without one nothing is subscribed to and no progress is asked for.
    pub fn start(directory: &Path, told_file: PathBuf, bus: Option<&Bus>) -> Option<Self> {
        let directory = directory.to_path_buf();
        let (offer, offered) = mpsc::channel();
        let (told, heard_told) = mpsc::channel();
        let told_already = Arc::new(Mutex::new(BTreeSet::new()));
        let stopping = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(None));
        if let Some(bus) = bus {
            // Subscribed before this returns: nothing published after it is
            // missed, and `tasks/status`'s kept value comes first.
            let made = bus
                .subscribe_typed(Decision::TOPIC, "voice", Decision::TYPE)
                .and_then(|decisions| {
                    Ok((
                        decisions,
                        bus.subscribe_typed(TaskStatus::TOPIC, "voice", TaskStatus::TYPE)?,
                    ))
                });
            match made {
                Ok((decisions, statuses)) => {
                    let reader = ProgressReader {
                        socket: directory.join("request.sock"),
                        offer: offer.clone(),
                        told: told_already.clone(),
                        stopping: stopping.clone(),
                        status: status.clone(),
                    };
                    std::thread::Builder::new()
                        .name("lamp-voice-progress-news".into())
                        .spawn(move || reader.run(decisions, statuses))
                        .ok()?;
                }
                Err(error) => {
                    eprintln!("lamp-voice: progress news cannot subscribe: {error}");
                }
            }
        }
        let shared = told_already.clone();
        std::thread::Builder::new()
            .name("lamp-voice-task-news".into())
            .spawn(move || read_forever(&directory, &told_file, &offer, &heard_told, &shared))
            .ok()?;
        Some(Self {
            offered,
            told,
            waiting: VecDeque::new(),
            told_already,
            stopping,
            status,
        })
    }

    /// The news waiting to be told, if any. Call every tick: it never blocks.
    pub fn waiting(&mut self) -> Option<&News> {
        while let Ok(news) = self.offered.try_recv() {
            if !self.waiting.iter().any(|waiting| waiting.key == news.key) {
                self.waiting.push_back(news);
            }
        }
        if let Ok(told) = self.told_already.try_lock() {
            self.waiting.retain(|news| !told.contains(&news.key));
        }
        // Progress not told in time is old news, and progress whose task
        // has finished, gone or moved on would say what is no longer so:
        // let go. The status is looked at only if it is free this tick.
        let now = Instant::now();
        self.waiting
            .retain(|news| news.until.is_none_or(|until| now < until));
        if self.waiting.iter().any(|news| news.progress.is_some())
            && let Ok(status) = self.status.try_lock()
        {
            self.waiting.retain(|news| {
                news.progress
                    .as_ref()
                    .is_none_or(|of| still_current(status.as_ref(), of))
            });
        }
        self.waiting.front()
    }

    /// Take the news waiting, as it is being told: it is not offered again.
    /// Progress is looked at once more first: one past its time, or whose
    /// task finished, went or moved on while the lamp waited for its moment
    /// (`coordinator`: quiet, someone there, a provider ready), is let go
    /// and nothing is told this tick; one whose status is being written
    /// just now is kept for the next tick. Never waits.
    pub fn tell(&mut self) -> Option<News> {
        let front = self.waiting.front()?;
        let past = front.until.is_some_and(|until| Instant::now() >= until);
        let moved_on = match (&front.progress, past) {
            (Some(of), false) => match self.status.try_lock() {
                Ok(status) => !still_current(status.as_ref(), of),
                Err(_) => return None,
            },
            _ => false,
        };
        let news = self.waiting.pop_front()?;
        if past || moved_on {
            return None;
        }
        let _ = self.told.send(news.key.clone());
        Some(news)
    }
}

fn read_forever(
    directory: &Path,
    told_file: &Path,
    offer: &Sender<News>,
    told: &Receiver<String>,
    shared: &Mutex<BTreeSet<String>>,
) {
    let socket = directory.join("request.sock");
    let mut offered = BTreeSet::new();
    loop {
        while let Ok(key) = told.try_recv() {
            keep_told(told_file, &key);
        }
        // Read again each time: the provider worker keeps there what the
        // person asked for and heard.
        let told_keys = read_told(told_file);
        if let Ok(mut shared) = shared.lock() {
            shared.clone_from(&told_keys);
        }
        if let Some(now) = boot_clock() {
            for news in read_once(&socket, &now, &told_keys, &offered) {
                offered.insert(news.key.clone());
                if offer.send(news).is_err() {
                    return;
                }
            }
        }
        // Wait for the next read, keeping what is told the moment it is
        // told: on lamp-4ace on 2026-10-15 a build whose provider ended
        // just after news was told told the same news three times, each
        // time before this thread's next pass had kept it.
        let next = std::time::Instant::now() + READ_EVERY;
        loop {
            let left = next.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                break;
            }
            match told.recv_timeout(left) {
                Ok(key) => keep_told(told_file, &key),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// The boot and its clock as the tasks part keeps them (`CLOCK_BOOTTIME`,
/// which `/proc/uptime` reads).
#[derive(Clone, Debug)]
struct BootClock {
    boot_id: String,
    ms: u64,
}

fn boot_clock() -> Option<BootClock> {
    let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let uptime = std::fs::read_to_string("/proc/uptime").ok()?;
    let seconds: f64 = uptime.split_whitespace().next()?.parse().ok()?;
    Some(BootClock {
        boot_id: boot_id.trim().to_owned(),
        ms: (seconds * 1_000.0) as u64,
    })
}

fn ask(socket: &Path, line: &Value) -> Option<Value> {
    crate::tools::exchange_up_to(
        socket,
        line,
        ASK_DEADLINE,
        crate::tools::MAX_TASKS_ANSWER_BYTES,
    )
    .filter(|answer| answer["ok"] == json!(true))
}

/// Every piece of news there is now that has not been told or offered.
fn read_once(
    socket: &Path,
    now: &BootClock,
    told: &BTreeSet<String>,
    offered: &BTreeSet<String>,
) -> Vec<News> {
    let mut news = Vec::new();
    let mut after = 0;
    loop {
        let Some(page) = ask(socket, &json!({"op":"list","after_id":after})) else {
            return news;
        };
        let store = page["store_id"].as_str().unwrap_or_default().to_owned();
        for item in page["items"].as_array().into_iter().flatten() {
            let Some(id) = item["id"].as_u64() else {
                continue;
            };
            // Work the person said yes to that never reached its agent is
            // news too: the lamp said it was handing it over.
            if let Some(failed) = failed_hand_over(item, now) {
                let key = format!("{store}:{id}:hand-over");
                if !told.contains(&key)
                    && !offered.contains(&key)
                    && let Some(task) = ask(socket, &json!({"op":"get_task","id":id}))
                    && let Some(note) = note_failed(&task, failed)
                {
                    news.push(News {
                        key,
                        note,
                        until: None,
                        progress: None,
                    });
                }
                continue;
            }
            if item["observations"].as_u64().unwrap_or(0) == 0 {
                continue;
            }
            let Some(task) = ask(socket, &json!({"op":"get_task","id":id})) else {
                continue;
            };
            let Some((observation_id, age_ms)) = newest_settled(&task, now) else {
                continue;
            };
            let key = format!("{store}:{id}:{observation_id}");
            if told.contains(&key) || offered.contains(&key) || age_ms > OLDEST_MS {
                continue;
            }
            let Some(observation) = ask(
                socket,
                &json!({"op":"get_observation","id":id,"observation_id":observation_id}),
            ) else {
                continue;
            };
            let mut data = observation["data"].clone();
            // The watch follows an idle answer and keeps the end the agent
            // went on to; Harness gives that answer with the earlier one at
            // its front (lamp-75c9, 2026-10-16, "…once it finishes.done"),
            // which was told already: only what came after is news.
            if let Some(earlier) = earlier_idle(&task, observation_id).and_then(|earlier_id| {
                ask(
                    socket,
                    &json!({"op":"get_observation","id":id,"observation_id":earlier_id}),
                )
            }) && let Some(rest) = went_on(
                earlier["data"]["answer"].as_str().unwrap_or_default(),
                data["answer"].as_str().unwrap_or_default(),
            ) {
                data["answer"] = json!(rest);
            }
            if let Some(note) = note(&task, &data, age_ms) {
                news.push(News {
                    key,
                    note,
                    until: None,
                    progress: None,
                });
            }
        }
        match page["next_after_id"].as_u64() {
            Some(next) if next > after => after = next,
            _ => return news,
        }
    }
}

/// The newest observation before `observation_id` in which the task's agent
/// had finished with an answer: what a later answer may begin with.
fn earlier_idle(task: &Value, observation_id: u64) -> Option<u64> {
    task["observations"]
        .as_array()?
        .iter()
        .filter(|observation| observation["state"].as_str() == Some("idle"))
        .filter_map(|observation| observation["id"].as_u64())
        .filter(|id| *id < observation_id)
        .max()
}

/// What `answer` says after `earlier`, when it begins with it: the part of
/// the agent's answer that is new. None when it does not, or adds nothing.
fn went_on<'a>(earlier: &str, answer: &'a str) -> Option<&'a str> {
    let earlier = earlier.trim();
    let rest = answer.trim_start().strip_prefix(earlier)?.trim();
    (!earlier.is_empty() && !rest.is_empty()).then_some(rest)
}

/// The task's newest observation in which its agent had finished, stopped,
/// or stopped to ask, and how long ago it was seen: only one seen in this
/// boot can be dated.
fn newest_settled(task: &Value, now: &BootClock) -> Option<(u64, u64)> {
    task["observations"]
        .as_array()?
        .iter()
        .filter(|observation| {
            matches!(
                observation["state"].as_str(),
                Some("idle" | "needs-input" | "stopped" | "failed")
            )
        })
        .filter_map(|observation| {
            let seen = &observation["observed"];
            (seen["boot_id"].as_str() == Some(now.boot_id.as_str())).then_some(())?;
            Some((
                observation["id"].as_u64()?,
                now.ms.saturating_sub(seen["monotonic_ms"].as_u64()?),
            ))
        })
        .max_by_key(|(id, _)| *id)
}

/// Whether a task the person said yes to failed to reach its agent within
/// the last [`JUST_MS`] of this boot: `false` when it surely did not start,
/// `true` when it may have.
fn failed_hand_over(item: &Value, now: &BootClock) -> Option<bool> {
    let dispatch = &item["dispatch"];
    let unsure = match dispatch["state"].as_str()? {
        "not_sent" => false,
        "uncertain" => true,
        _ => return None,
    };
    if dispatch["confirmed"].is_null() {
        return None;
    }
    let finished = &dispatch["finished"];
    (finished["boot_id"].as_str() == Some(now.boot_id.as_str())
        && now.ms.saturating_sub(finished["monotonic_ms"].as_u64()?) <= JUST_MS)
        .then_some(unsure)
}

fn note_failed(task: &Value, unsure: bool) -> Option<String> {
    let who = who(task);
    let (asked, _) = quoted(task["text"].as_str()?, MOST_TASK_CHARS);
    let what = if unsure {
        format!("may not have reached {who}: you cannot tell whether it started")
    } else {
        format!("did not reach {who}: it could not be started just now")
    };
    Some(format!(
        "(Nobody has spoken. The work the person asked you to hand over, \"{asked}\", {what}. \
         Tell them in one plain, light sentence, with no technical words, and offer to try again.)"
    ))
}

/// Who the work went to, as the person calls them.
pub(crate) fn who(task: &Value) -> &'static str {
    match task["target"]["engine"].as_str() {
        Some("codex") => "Codex",
        Some("opencode") => "OpenCode",
        _ => "Claude Code",
    }
}

/// At most `most` characters of `text` on one line, with no double quotes
/// to end the note's quotation early.
pub(crate) fn quoted(text: &str, most: usize) -> (String, bool) {
    let flat: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'");
    let cut = flat.chars().count() > most;
    (flat.chars().take(most).collect(), cut)
}

/// What the model is told, for the agent's state; none when there is
/// nothing to tell.
fn note(task: &Value, data: &Value, age_ms: u64) -> Option<String> {
    let who = who(task);
    let (asked, _) = quoted(task["text"].as_str()?, MOST_TASK_CHARS);
    let (answer, cut) = quoted(
        data["answer"].as_str().unwrap_or_default(),
        MOST_ANSWER_CHARS,
    );
    let cut = cut || data["truncated"] == json!(true);
    let when = if age_ms <= JUST_MS {
        "just"
    } else {
        "a while ago, and you have not told them yet:"
    };
    let opening = "(Nobody has spoken. News from work you handed over for the person.";
    let state = match data["state"].as_str()? {
        "idle" if !answer.is_empty() => "idle",
        state @ ("needs-input" | "stopped" | "failed") => state,
        _ => return None,
    };
    let say = |asked: &str, answer: &str, more: &str| match state {
        // Cut: the gist first, from what is kept, then where the rest is.
        "idle" if !more.is_empty() => format!(
            "{opening} {who} {when} finished the task they gave it: \"{asked}\". Its answer is long; it begins: \"{answer}{more}\". \
             Tell them now in one or two natural spoken sentences, as a good chief of staff would: \"Oh, {who} just finished…\" (\"earlier\" if it was a while ago), \
             then its gist in one plain line from that beginning (a story: who it is about and what happens; code: what it does now), \
             then that the rest is on their computer. Never only that. No code, lists, file paths or links, and no question.)"
        ),
        "idle" => format!(
            "{opening} {who} {when} finished the task they gave it: \"{asked}\". What it came back with: \"{answer}{more}\". \
             Tell them now, the way a good chief of staff tells the person they work for: one or two natural spoken sentences, \
             starting with something like \"Oh, {who} just finished…\" (or \"earlier\" if it was a while ago), then the gist of the result in plain words. \
             Do not read out code, lists, file paths or links; if there is more than fits, say the rest is on their computer. No question at the end.)"
        ),
        "needs-input" if answer.is_empty() && more.is_empty() => format!(
            "{opening} {who} has stopped and is waiting for them about the task they gave it: \"{asked}\". {})",
            waiting(who, "")
        ),
        "needs-input" => format!(
            "{opening} {who} has stopped and is waiting for them about the task they gave it: \"{asked}\". What it last said: \"{answer}{more}\". {})",
            waiting(who, answer)
        ),
        _ => format!(
            "{opening} {who} stopped without finishing the task they gave it: \"{asked}\". What it last said: \"{answer}{more}\". \
             Tell them in one short, plain sentence, without alarm.)"
        ),
    };
    fitted(&asked, &answer, cut, say)
}

/// What the model is told to say while the agent waits for the person
/// (`needs-input`), in both the news and `check_agent_task`. Harness
/// 0.4.6's `show` carries no question while it waits (probed 2026-10-15,
/// #770): `answer` is empty or the previous answer. So the model is never
/// asked to voice a question it was not given.
pub(crate) fn waiting(who: &str, answer: &str) -> String {
    let tell = format!(
        "Tell them once, in one short sentence, that {who} is waiting for them on their computer and that they can answer it there."
    );
    if answer.is_empty() {
        format!(
            "{tell} It did not say what it is asking: do not ask a question for it or guess one."
        )
    } else {
        format!(
            "{tell} If what it last said is a question to them, you may pass it on in your own words; otherwise do not ask a question for it or guess one."
        )
    }
}

/// A progress decision that is to be told: what it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Wanted {
    store: String,
    task: u64,
    change: u32,
    /// The run of the tasks part that counted the change.
    run: u64,
    /// Seconds the agent has been at it, as `tasks/status` says.
    working_s: Option<u64>,
}

impl Wanted {
    /// What is kept as told. The run is in it: a restart of the tasks part
    /// keeps the store and the task numbers and counts changes from
    /// nothing, and its change 1 is not the last run's (#765 review,
    /// 2026-10-15).
    fn key(&self) -> String {
        format!(
            "{}:{}:progress-{}-{}",
            self.store, self.task, self.run, self.change
        )
    }
}

/// Whether `decision`, made `age_ms` ago, is progress to tell: `progress`,
/// `say_now`, acting (not shadow), at most [`PROGRESS_FRESH_MS`] old, of
/// the store and the run `tasks/status` is of, and about that task's
/// newest change while its agent still works. Its `task` is
/// `<store>:<task>:<change>@<run>`. Why not, in one word, if not.
fn wanted(
    decision: &Decision,
    age_ms: u64,
    status: Option<&TaskStatus>,
) -> Result<Wanted, &'static str> {
    if decision.about != "progress" {
        return Err("not_progress");
    }
    if decision.choice != "say_now" {
        return Err("wait");
    }
    if decision.shadow {
        return Err("shadow");
    }
    let parts: Vec<&str> = decision
        .task
        .as_deref()
        .unwrap_or_default()
        .split(':')
        .collect();
    let [store, task, change_of_run] = parts[..] else {
        return Err("no_task");
    };
    let Some((change, run)) = change_of_run.split_once('@') else {
        return Err("no_task");
    };
    let (Ok(task), Ok(change), Ok(run)) = (
        task.parse::<u64>(),
        change.parse::<u32>(),
        run.parse::<u64>(),
    ) else {
        return Err("no_task");
    };
    if age_ms > PROGRESS_FRESH_MS {
        return Err("stale");
    }
    let Some(status) = status else {
        return Err("no_status");
    };
    if status.store_id != store {
        return Err("other_store");
    }
    if status.run != run {
        return Err("other_run");
    }
    let Some(entry) = status.tasks.iter().find(|entry| entry.task == task) else {
        return Err("not_watched");
    };
    if entry.changes > change {
        return Err("older_change");
    }
    if entry.state != AgentState::Working {
        return Err("not_working");
    }
    Ok(Wanted {
        store: store.to_owned(),
        task,
        change,
        run,
        working_s: entry.working_s,
    })
}

/// A sign in the words the person would use.
fn sign_words(sign: Sign) -> &'static str {
    match sign {
        Sign::TestsPass => "the tests pass",
        Sign::BuildWorks => "the build works",
        Sign::Fixed => "something was fixed",
        Sign::Found => "something was found",
        Sign::Done => "a part was done",
        Sign::PullRequest => "a pull request was opened",
    }
}

/// What the model is told of progress: who, the task's words, the agent's
/// progress line, the signs not told yet in words and how long it has been
/// at it, cut to one written turn ([`fitted`]).
fn note_progress(
    task: &Value,
    about: &str,
    signs: &BTreeSet<Sign>,
    working_s: Option<u64>,
) -> Option<String> {
    let who = who(task);
    let (asked, _) = quoted(task["text"].as_str()?, MOST_TASK_CHARS);
    let (about, cut) = quoted(about, MOST_ABOUT_CHARS);
    if about.is_empty() {
        return None;
    }
    let signs = if signs.is_empty() {
        String::new()
    } else {
        let words: Vec<&str> = signs.iter().map(|sign| sign_words(*sign)).collect();
        format!(" Signs of a step forward in it: {}.", words.join(", "))
    };
    let time = match working_s.map(|s| (s + 30) / 60) {
        None => String::new(),
        Some(0 | 1) => " It has been at it for about a minute.".to_owned(),
        Some(minutes) => format!(" It has been at it for about {minutes} minutes."),
    };
    let say = |asked: &str, about: &str, more: &str| {
        format!(
            "(Nobody has spoken. Progress on work you handed over for the person: {who} is still working on the task they gave it: \"{asked}\". \
             Its latest progress line: \"{about}{more}\".{signs}{time} \
             Tell them how it is going in one short, natural sentence, the way a good chief of staff mentions it in passing, \
             starting with something like \"Oh, {who} has…\". Do not read the line out word for word, and no code, file paths or links. \
             No question at the end.)"
        )
    };
    fitted(&asked, &about, cut, say)
}

/// Reads progress decisions and offers each one to tell as news.
struct ProgressReader {
    socket: PathBuf,
    offer: Sender<News>,
    told: Arc<Mutex<BTreeSet<String>>>,
    stopping: Arc<AtomicBool>,
    /// Where the newest status is kept for [`TaskNews`].
    status: Arc<Mutex<Option<TaskStatus>>>,
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

impl ProgressReader {
    fn run(self, mut decisions: Subscriber, mut statuses: Subscriber) {
        let mut status: Option<TaskStatus> = None;
        // The signs already put in a note, for each store and task: a note
        // names only those not told yet.
        let mut mentioned: BTreeMap<(String, u64), BTreeSet<Sign>> = BTreeMap::new();
        while !self.stopping.load(Ordering::Relaxed) {
            let received = decisions.recv(PROGRESS_WAIT);
            // The newest status, taken after the wait: a decision is made
            // after the status it is about was published.
            while let Ok(Some(message)) = statuses.recv(Duration::ZERO) {
                if let Ok(newer) = message.parse::<TaskStatus>()
                    && newer.check().is_ok()
                {
                    // Kept for the notes already queued as well: a task that
                    // finishes or moves on withdraws them (#765 review).
                    if let Ok(mut shared) = self.status.lock() {
                        *shared = Some(newer.clone());
                    }
                    status = Some(newer);
                }
            }
            let message = match received {
                Ok(Some(message)) => message,
                Ok(None) => continue,
                Err(_) => {
                    std::thread::sleep(PROGRESS_WAIT);
                    continue;
                }
            };
            let Ok(decision) = message.parse::<Decision>() else {
                continue;
            };
            let age_ms = unix_ms().saturating_sub(message.header.at_unix_ms);
            // The one time it is let go, counted from when it was made: the
            // words fetched below take time of their own.
            let expires =
                Instant::now() + Duration::from_millis(PROGRESS_FRESH_MS.saturating_sub(age_ms));
            let wanted = match wanted(&decision, age_ms, status.as_ref()) {
                Ok(wanted) => wanted,
                Err(why) => {
                    if decision.about == "progress" && decision.choice == "say_now" {
                        eprintln!("lamp-voice: progress to say let go ({why})");
                    }
                    continue;
                }
            };
            if self
                .told
                .lock()
                .is_ok_and(|told| told.contains(&wanted.key()))
            {
                continue;
            }
            let said = mentioned
                .entry((wanted.store.clone(), wanted.task))
                .or_default();
            match self.news(&wanted, said, expires) {
                Ok(news) => {
                    eprintln!("lamp-voice: progress to say offered ({})", news.key);
                    if self.offer.send(news).is_err() {
                        return;
                    }
                }
                Err(why) => eprintln!("lamp-voice: progress to say let go ({why})"),
            }
        }
    }

    /// The words, fetched now that the lamp will say them: the progress
    /// line (`get_progress`) and the task (`get_task`), both of the store
    /// decided about, the agent still at work, and exactly the change
    /// decided about. A snapshot of anything else is let go: the task
    /// finished, or moved on, while the decision travelled. So is one that
    /// comes back at or past `expires`.
    fn news(
        &self,
        wanted: &Wanted,
        said: &mut BTreeSet<Sign>,
        expires: Instant,
    ) -> Result<News, &'static str> {
        let progress = ask(&self.socket, &json!({"op":"get_progress","id":wanted.task}))
            .ok_or("no_progress")?;
        let task =
            ask(&self.socket, &json!({"op":"get_task","id":wanted.task})).ok_or("no_task")?;
        if progress["store_id"].as_str() != Some(wanted.store.as_str())
            || task["store_id"].as_str() != Some(wanted.store.as_str())
        {
            return Err("other_store");
        }
        if Instant::now() >= expires {
            return Err("stale");
        }
        let progress = &progress["progress"];
        if progress["run"].as_u64() != Some(wanted.run) {
            return Err("other_run");
        }
        if progress["state"].as_str() != Some("working") {
            return Err("not_working");
        }
        if progress["changes"].as_u64() != Some(u64::from(wanted.change)) {
            return Err("other_change");
        }
        let seen: BTreeSet<Sign> =
            serde_json::from_value(progress["signs_seen"].clone()).unwrap_or_default();
        let new: BTreeSet<Sign> = seen.difference(said).copied().collect();
        let note = note_progress(
            &task,
            progress["about"].as_str().unwrap_or_default(),
            &new,
            wanted.working_s,
        )
        .ok_or("nothing_to_say")?;
        said.extend(new);
        Ok(News {
            key: wanted.key(),
            note,
            until: Some(expires),
            progress: Some(ProgressOf {
                store: wanted.store.clone(),
                task: wanted.task,
                change: wanted.change,
                run: wanted.run,
            }),
        })
    }
}

/// Said after an answer that was cut.
const GOES_ON: &str = " …(it goes on)";

/// The note `say` makes, cut to what one written turn may hold
/// (`lamp_gemini::wire::MAX_NOTE_BYTES`). The transport refuses a longer
/// one as `InvalidConfiguration`, and the provider worker ends on that
/// refusal: on lamp-4ace on 2026-10-15 the voice ended so 4 ms after
/// committing news, three times in three.
/// The task's words are cut to [`TASK_CHARS_KEPT_FIRST`] first, then the
/// agent's answer, then the rest of the task's words, by whole characters,
/// and a cut answer is said to go on. A note that fits is exactly what
/// `say` makes.
fn fitted(
    asked: &str,
    answer: &str,
    cut: bool,
    say: impl Fn(&str, &str, &str) -> String,
) -> Option<String> {
    let (asked_chars, answer_chars) = (asked.chars().count(), answer.chars().count());
    let (mut asked_kept, mut answer_kept) = (asked_chars, answer_chars);
    loop {
        let more = if cut || answer_kept < answer_chars {
            GOES_ON
        } else {
            ""
        };
        let text = say(
            &asked.chars().take(asked_kept).collect::<String>(),
            &answer.chars().take(answer_kept).collect::<String>(),
            more,
        );
        // A character is at least one byte: leaving out this many is enough
        // for what is over, or more than enough.
        let over = text.len().saturating_sub(lamp_gemini::wire::MAX_NOTE_BYTES);
        if over == 0 {
            return Some(text);
        }
        if asked_kept > TASK_CHARS_KEPT_FIRST {
            asked_kept = asked_kept.saturating_sub(over).max(TASK_CHARS_KEPT_FIRST);
        } else if answer_kept > 0 {
            answer_kept = answer_kept.saturating_sub(over);
        } else if asked_kept > 0 {
            asked_kept = asked_kept.saturating_sub(over);
        } else {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        os::unix::net::UnixListener,
    };

    fn task(engine: &str, observations: Value) -> Value {
        json!({"ok":true,"text":"Suggest three \"friendly\" names for a desk lamp",
            "target":{"engine":engine,"machine_id":"c533fbfda3de5f8f1fdb6619ae2e30b5","project":"lamp-tasks"},
            "observations":observations})
    }
    fn seen(id: u64, state: &str, boot: &str, ms: u64) -> Value {
        json!({"id":id,"state":state,"observed":{"boot_id":boot,"monotonic_ms":ms}})
    }
    const NOW: u64 = 50_000_000;
    fn now() -> BootClock {
        BootClock {
            boot_id: "boot-a".into(),
            ms: NOW,
        }
    }

    #[test]
    fn the_newest_finished_look_in_this_boot_is_the_news_and_work_in_progress_is_not() {
        let observations = json!([
            seen(1, "working", "boot-a", NOW - 9_000),
            seen(2, "idle", "boot-a", NOW - 5_000),
            seen(3, "working", "boot-a", NOW - 1_000),
        ]);
        assert_eq!(
            newest_settled(&task("claude", observations), &now()),
            Some((2, 5_000))
        );
        // A look from before the lamp last started cannot be dated: not news.
        let earlier = json!([seen(1, "idle", "boot-b", 10)]);
        assert_eq!(newest_settled(&task("claude", earlier), &now()), None);
        let asking = json!([
            seen(1, "idle", "boot-a", 10),
            seen(4, "needs-input", "boot-a", NOW)
        ]);
        assert_eq!(newest_settled(&task("codex", asking), &now()), Some((4, 0)));
    }

    #[test]
    fn an_answer_that_went_on_is_told_from_where_the_earlier_one_ended() {
        let earlier = "The sleep is running in the background, and I'll reply once it finishes.";
        assert_eq!(
            went_on(
                earlier,
                "The sleep is running in the background, and I'll reply once it finishes.done"
            ),
            Some("done")
        );
        assert_eq!(
            went_on(earlier, &format!("{earlier}\n\nAll done.")),
            Some("All done.")
        );
        // Not the same beginning, the same answer, or nothing earlier: as is.
        assert_eq!(went_on(earlier, "Done: slept 90 s."), None);
        assert_eq!(went_on(earlier, earlier), None);
        assert_eq!(went_on("", "done"), None);
        // Only an earlier finished answer is looked at, the newest before.
        let observations = json!([
            seen(1, "idle", "boot-a", 10),
            seen(2, "needs-input", "boot-a", 20),
            seen(3, "idle", "boot-a", 30),
            seen(5, "idle", "boot-a", 50),
        ]);
        let task = task("claude", observations);
        assert_eq!(earlier_idle(&task, 5), Some(3));
        assert_eq!(earlier_idle(&task, 3), Some(1));
        assert_eq!(earlier_idle(&task, 1), None);
    }

    #[test]
    fn the_note_names_the_agent_quotes_safely_and_says_how_fresh_it_is() {
        let finished = json!({"state":"idle","answer":"Here are three:\n\n1. **Lumi**\n2. \"Sunny\"\n3. Archie","truncated":false});
        let note = note(&task("claude", json!([])), &finished, 60_000).unwrap();
        assert!(
            note.contains("Claude Code just finished the task"),
            "{note}"
        );
        assert!(
            note.contains("\"Suggest three 'friendly' names for a desk lamp\""),
            "{note}"
        );
        assert!(
            note.contains("1. **Lumi** 2. 'Sunny' 3. Archie\""),
            "{note}"
        );
        assert!(!note.contains("(it goes on)"));
        let later = super::note(&task("codex", json!([])), &finished, JUST_MS + 1).unwrap();
        assert!(later.contains("Codex a while ago"), "{later}");
        // Asking, and stopped.
        // Asking with words: waiting on their computer, by name; what it
        // last said may be passed on only if it is a question.
        let asking = json!({"state":"needs-input","answer":"Which folder?"});
        let waits = super::note(&task("claude", json!([])), &asking, 0).unwrap();
        assert!(
            waits.contains("Claude Code is waiting for them on their computer")
                && waits.contains("What it last said: \"Which folder?\"")
                && waits.contains("you may pass it on")
                && !waits.contains("ask its question"),
            "{waits}"
        );
        let stopped = json!({"state":"stopped","answer":"I could not reach the repository.","truncated":true});
        let told = super::note(&task("claude", json!([])), &stopped, 0).unwrap();
        assert!(told.contains("stopped without finishing") && told.contains("(it goes on)"));
        // An idle agent with nothing to say, or one at work: nothing to tell.
        assert_eq!(
            super::note(
                &task("claude", json!([])),
                &json!({"state":"idle","answer":" "}),
                0
            ),
            None
        );
        assert_eq!(
            super::note(
                &task("claude", json!([])),
                &json!({"state":"working","answer":"x"}),
                0
            ),
            None
        );
        // A long answer is cut and says so.
        let long = json!({"state":"idle","answer":"word ".repeat(400)});
        let long = super::note(&task("claude", json!([])), &long, 0).unwrap();
        assert!(
            long.contains("(it goes on)") && long.len() <= lamp_gemini::wire::MAX_NOTE_BYTES,
            "{}",
            long.len()
        );
    }

    /// Every note is one written turn, which the transport takes only up
    /// to `MAX_NOTE_BYTES`; a longer one is refused as
    /// `InvalidConfiguration`, which ends the provider worker, as the voice
    /// ended 4 ms after committing news on lamp-4ace on 2026-10-15. Long answers
    /// and long tasks, in letters of one byte or of several, fit and are
    /// taken by the transport; a note that fits is as it was.
    #[test]
    fn every_note_fits_in_one_written_turn() {
        let english = "Here is what I found and changed, file by file. ".repeat(40);
        let vietnamese = "Đây là những gì tôi đã tìm thấy và sửa, từng tệp một. ".repeat(40);
        // What the transport does with a note one byte over.
        let over = "x".repeat(lamp_gemini::wire::MAX_NOTE_BYTES + 1);
        assert!(matches!(
            lamp_gemini::wire::encode_written_turn(&over, None),
            Err(lamp_gemini::Error::InvalidConfiguration)
        ));
        let asks = [
            "Suggest three names for a desk lamp".to_owned(),
            "Đề xuất ba cái tên thân thiện cho một chiếc đèn bàn. ".repeat(10),
        ];
        for asked in &asks {
            for answer in [&english, &vietnamese] {
                for state in ["idle", "needs-input", "stopped"] {
                    for age_ms in [0, JUST_MS + 1] {
                        let task = json!({"text": asked, "target": {"engine": "claude"}});
                        let data = json!({"state": state, "answer": answer});
                        let told = note(&task, &data, age_ms).unwrap();
                        assert!(
                            told.len() <= lamp_gemini::wire::MAX_NOTE_BYTES,
                            "{state}: {} bytes",
                            told.len()
                        );
                        lamp_gemini::wire::encode_written_turn(&told, None).unwrap();
                        assert!(told.contains("(it goes on)"), "{told}");
                        assert!(told.ends_with(".)"), "{told}");
                    }
                }
            }
        }
        // As on lamp-4ace: a task of about a hundred characters and an
        // answer of six hundred, well within what is taken of each. The
        // note of both whole is over the bound: the answer is cut to fit.
        let asked = "Ask Claude Code on my office computer to look at why the build is slow and tell me what it finds";
        let answer =
            "The build spends most of its time compiling the same dependencies again. ".repeat(8);
        assert!(answer.chars().count() < MOST_ANSWER_CHARS);
        let told = note(
            &json!({"text": asked, "target": {"engine": "claude"}}),
            &json!({"state": "idle", "answer": answer}),
            0,
        )
        .unwrap();
        lamp_gemini::wire::encode_written_turn(&told, None).unwrap();
        assert!(told.contains("(it goes on)"), "{told}");
        assert!(told.contains("\"Ask Claude Code on my office computer to look at why the build is slow and tell me what it finds\""));
        // A note that fits is exactly as before.
        let finished = json!({"state":"idle","answer":"Lumi, Sunny and Archie.","truncated":false});
        assert_eq!(
            note(&task("claude", json!([])), &finished, 0).unwrap(),
            "(Nobody has spoken. News from work you handed over for the person. Claude Code just finished the task they gave it: \
             \"Suggest three 'friendly' names for a desk lamp\". What it came back with: \"Lumi, Sunny and Archie.\". \
             Tell them now, the way a good chief of staff tells the person they work for: one or two natural spoken sentences, \
             starting with something like \"Oh, Claude Code just finished…\" (or \"earlier\" if it was a while ago), then the gist of the result in plain words. \
             Do not read out code, lists, file paths or links; if there is more than fits, say the rest is on their computer. No question at the end.)"
        );
    }

    /// Harness 0.4.6 in auto mode, waiting on a question (#770): `show`
    /// says `needs-input` with no question text. The person is told the
    /// agent, by name, is waiting for them on their computer, and the
    /// model is never asked to voice a question it was not given.
    #[test]
    fn an_agent_waiting_with_no_question_is_told_as_waiting_on_their_computer() {
        let waiting = json!({"state":"needs-input","answer":"","truncated":false});
        for (engine, who) in [
            ("claude", "Claude Code"),
            ("codex", "Codex"),
            ("opencode", "OpenCode"),
        ] {
            let told = note(&task(engine, json!([])), &waiting, 0).unwrap();
            assert_eq!(
                told,
                format!(
                    "(Nobody has spoken. News from work you handed over for the person. {who} has stopped and is waiting for them about the task they gave it: \
                     \"Suggest three 'friendly' names for a desk lamp\". \
                     Tell them once, in one short sentence, that {who} is waiting for them on their computer and that they can answer it there. \
                     It did not say what it is asking: do not ask a question for it or guess one.)"
                )
            );
            assert!(!told.contains("What it last said") && !told.contains("ask its question"));
        }
    }

    /// The other states' notes are byte for byte what they were before
    /// #770 (idle is pinned in `every_note_fits_in_one_written_turn`).
    #[test]
    fn a_stop_or_a_failure_is_told_as_before() {
        for state in ["stopped", "failed"] {
            let data = json!({"state":state,"answer":"I could not reach the repository.","truncated":false});
            assert_eq!(
                note(&task("claude", json!([])), &data, 0).unwrap(),
                "(Nobody has spoken. News from work you handed over for the person. Claude Code stopped without finishing the task they gave it: \
                 \"Suggest three 'friendly' names for a desk lamp\". What it last said: \"I could not reach the repository.\". \
                 Tell them in one short, plain sentence, without alarm.)"
            );
        }
    }

    /// On lamp-4ace on 2026-10-15 at 08:02 a 300-word story was told as only
    /// "the rest is waiting on your computer" (#757): the note kept 163 bytes
    /// of it after a long task, and asked for the gist only in passing. A
    /// cut answer now keeps the task's words to a hundred characters first,
    /// keeps more of the answer, and asks for the gist in one line before
    /// where the rest is.
    #[test]
    fn a_long_result_keeps_enough_for_a_gist_and_asks_for_it_first() {
        let story =
            "Every night, when the office emptied, the desk lamp hummed a little tune to itself. "
                .repeat(20);
        let long_task =
            "Write me a short story of about three hundred words about a desk lamp that \
                         learns to sing at night when everyone has gone home, with a happy ending, \
                         something gentle I could read to my daughter before bed, and keep the \
                         words simple enough for a six year old to follow along"
                .to_owned();
        assert!(long_task.chars().count() > 250);
        for asked in [long_task.as_str(), "Write a story about a lamp"] {
            for age_ms in [0, JUST_MS + 1] {
                let told = note(
                    &json!({"text": asked, "target": {"engine": "claude"}}),
                    &json!({"state": "idle", "answer": story}),
                    age_ms,
                )
                .unwrap();
                lamp_gemini::wire::encode_written_turn(&told, None).unwrap();
                // The gist first, in one line, then where the rest is.
                assert!(
                    told.contains("then its gist in one plain line from that beginning")
                        && told
                            .contains("then that the rest is on their computer. Never only that."),
                    "{told}"
                );
                // At least 300 characters of the story to make it from,
                // even after a long task.
                let kept = told
                    .split("it begins: \"")
                    .nth(1)
                    .and_then(|rest| rest.split(" …(it goes on)").next())
                    .unwrap();
                assert!(
                    kept.chars().count() >= 300,
                    "{} kept: {told}",
                    kept.chars().count()
                );
                // The task's words are cut to a hundred characters, no fewer.
                let quoted_task = told
                    .split("finished the task they gave it: \"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .unwrap();
                assert_eq!(
                    quoted_task.chars().count(),
                    asked.chars().count().min(TASK_CHARS_KEPT_FIRST),
                    "{told}"
                );
            }
        }
        // A question or a stop that is cut is as it was: the task first,
        // then the answer, by the same rule.
        let asking = note(
            &json!({"text": long_task, "target": {"engine": "codex"}}),
            &json!({"state": "needs-input", "answer": story}),
            0,
        )
        .unwrap();
        assert!(
            asking.contains("is waiting for them on their computer"),
            "{asking}"
        );
        assert!(!asking.contains("gist"), "{asking}");
    }

    #[test]
    fn each_piece_of_news_is_offered_once_and_told_news_is_kept() {
        let root = std::env::temp_dir().join(format!("task-news-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let socket = root.join("request.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        // The tasks part: two tasks, one with news, one still at work.
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let asked: Value = serde_json::from_str(&line).unwrap();
                let answer = match (asked["op"].as_str().unwrap(), asked["id"].as_u64()) {
                    ("list", _) => json!({"ok":true,"store_id":"s1","items":[
                        {"id":1,"observations":1},{"id":2,"observations":1},{"id":3,"observations":0}]}),
                    ("get_task", Some(1)) => {
                        task("claude", json!([seen(1, "idle", "boot-a", NOW - 1_000)]))
                    }
                    ("get_task", Some(2)) => {
                        task("claude", json!([seen(2, "working", "boot-a", NOW - 1_000)]))
                    }
                    ("get_observation", Some(1)) => {
                        json!({"ok":true,"data":{"state":"idle","answer":"Lumi, Sunny and Archie.","truncated":false}})
                    }
                    _ => json!({"ok":false}),
                };
                writeln!(stream, "{answer}").unwrap();
            }
        });
        let told = BTreeSet::new();
        let mut offered = BTreeSet::new();
        let news = read_once(&socket, &now(), &told, &offered);
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].key, "s1:1:1");
        assert!(news[0].note.contains("Lumi, Sunny and Archie."));
        offered.insert(news[0].key.clone());
        assert!(read_once(&socket, &now(), &told, &offered).is_empty());
        // Told before a restart: not told again.
        let told = BTreeSet::from(["s1:1:1".to_owned()]);
        assert!(read_once(&socket, &now(), &told, &BTreeSet::new()).is_empty());
        // Too old to tell unasked.
        let late = BootClock {
            boot_id: "boot-a".into(),
            ms: NOW + OLDEST_MS,
        };
        assert!(read_once(&socket, &late, &BTreeSet::new(), &BTreeSet::new()).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn work_said_yes_to_that_never_reached_its_agent_is_told_soon_after() {
        let item = |state: &str, confirmed: bool, boot: &str, ms: u64| {
            json!({"id":3,"observations":0,"dispatch":{"state":state,
                "confirmed": if confirmed { json!({"boot_id":boot}) } else { Value::Null },
                "finished":{"boot_id":boot,"monotonic_ms":ms}}})
        };
        assert_eq!(
            failed_hand_over(&item("not_sent", true, "boot-a", NOW - 5_000), &now()),
            Some(false)
        );
        assert_eq!(
            failed_hand_over(&item("uncertain", true, "boot-a", NOW - 5_000), &now()),
            Some(true)
        );
        // Never said yes to, long ago, another boot, or started: not news.
        assert_eq!(
            failed_hand_over(&item("not_sent", false, "boot-a", NOW), &now()),
            None
        );
        assert_eq!(
            failed_hand_over(&item("not_sent", true, "boot-a", NOW - JUST_MS - 1), &now()),
            None
        );
        assert_eq!(
            failed_hand_over(&item("not_sent", true, "boot-b", NOW), &now()),
            None
        );
        assert_eq!(
            failed_hand_over(&item("accepted", true, "boot-a", NOW), &now()),
            None
        );
        let told = note_failed(&task("claude", json!([])), false).unwrap();
        assert!(
            told.contains("did not reach Claude Code") && told.contains("offer to try again"),
            "{told}"
        );
        assert!(
            note_failed(&task("codex", json!([])), true)
                .unwrap()
                .contains("may not have reached Codex")
        );
    }

    /// #560, V1's third lesson: Harness accepted work and its agent never
    /// answered (V1 held the run 15 minutes and said nothing). Here work the
    /// person said yes to was accepted half an hour ago, and every look since
    /// finds its agent `offline`: the person should hear that it isn't
    /// answering, in plain words, as a failed hand-over is told. Today an
    /// accepted agent that goes silent is watched for a day and never told.
    #[test]
    #[ignore = "#560: accepted work whose agent never answers is never told"]
    fn work_whose_agent_never_answers_after_it_was_accepted_is_told() {
        let root = std::env::temp_dir().join(format!("task-news-silent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let socket = root.join("request.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        const ACCEPTED: u64 = NOW - 30 * 60_000;
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let asked: Value = serde_json::from_str(&line).unwrap();
                let answer = match (asked["op"].as_str().unwrap(), asked["id"].as_u64()) {
                    ("list", _) => json!({"ok":true,"store_id":"s1","items":[
                        {"id":4,"observations":3,"dispatch":{"state":"accepted",
                            "confirmed":{"boot_id":"boot-a"},
                            "finished":{"boot_id":"boot-a","monotonic_ms":ACCEPTED}}}]}),
                    ("get_task", Some(4)) => task(
                        "claude",
                        json!([
                            seen(1, "offline", "boot-a", ACCEPTED + 30_000),
                            seen(2, "offline", "boot-a", ACCEPTED + 600_000),
                            seen(3, "offline", "boot-a", NOW - 60_000),
                        ]),
                    ),
                    _ => json!({"ok":false}),
                };
                writeln!(stream, "{answer}").unwrap();
            }
        });
        let news = read_once(&socket, &now(), &BTreeSet::new(), &BTreeSet::new());
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            news.len(),
            1,
            "half an hour without an answer from the agent is news"
        );
        assert!(news[0].note.contains("Claude Code"), "{}", news[0].note);
    }

    const RUN: u64 = 1_791_964_000_000;

    fn status(store: &str, changes: u32, state: AgentState) -> TaskStatus {
        TaskStatus {
            store_id: store.into(),
            run: RUN,
            unread: vec![],
            settled: Vec::new(),
            tasks: vec![lamp_msgs::TaskProgress {
                task: 3,
                engine: "claude".into(),
                state,
                working_s: Some(125),
                last_active_unix_ms: None,
                read_unix_ms: 0,
                changes,
                changed: changes > 0,
                new_share: 0.5,
                signs: vec![],
                signs_seen: vec![],
            }],
        }
    }
    fn decided(choice: &str, shadow: bool, task: &str) -> Decision {
        Decision {
            about: "progress".into(),
            choice: choice.into(),
            probability: 0.9,
            answers: Default::default(),
            by: "stand-in".into(),
            took_ms: 1,
            shadow,
            request: None,
            task: Some(task.into()),
            arrival: None,
            ask: None,
        }
    }
    const STORE: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn only_a_fresh_acting_say_now_about_the_newest_change_of_this_store_is_told() {
        let key = format!("{STORE}:3:4@{RUN}");
        let now = status(STORE, 4, AgentState::Working);
        let good = wanted(&decided("say_now", false, &key), 1_000, Some(&now)).unwrap();
        assert_eq!(good.key(), format!("{STORE}:3:progress-{RUN}-4"));
        assert_eq!(good.run, RUN);
        assert_eq!(good.working_s, Some(125));
        // A status that lags behind the decision is no reason to drop it.
        let behind = status(STORE, 3, AgentState::Working);
        assert!(wanted(&decided("say_now", false, &key), 0, Some(&behind)).is_ok());
        let attention = Decision {
            about: "attention".into(),
            ..decided("say_now", false, &key)
        };
        for (decision, age, status, why) in [
            (attention, 0, Some(&now), "not_progress"),
            (decided("wait", false, &key), 0, Some(&now), "wait"),
            (decided("say_now", true, &key), 0, Some(&now), "shadow"),
            (decided("say_now", false, "3:4"), 0, Some(&now), "no_task"),
            // Without its run a count names no change.
            (
                decided("say_now", false, &format!("{STORE}:3:4")),
                0,
                Some(&now),
                "no_task",
            ),
            // Another run of the tasks part: its count 4 is another change.
            (
                decided("say_now", false, &format!("{STORE}:3:4@{}", RUN + 1)),
                0,
                Some(&now),
                "other_run",
            ),
            (
                decided("say_now", false, &key),
                PROGRESS_FRESH_MS + 1,
                Some(&now),
                "stale",
            ),
            (decided("say_now", false, &key), 0, None, "no_status"),
            (
                decided("say_now", false, &key),
                0,
                Some(&status(&"f".repeat(32), 4, AgentState::Working)),
                "other_store",
            ),
            (
                decided("say_now", false, &key),
                0,
                Some(&status(STORE, 5, AgentState::Working)),
                "older_change",
            ),
            (
                decided("say_now", false, &key),
                0,
                Some(&status(STORE, 4, AgentState::Idle)),
                "not_working",
            ),
            (
                decided("say_now", false, &format!("{STORE}:9:4@{RUN}")),
                0,
                Some(&now),
                "not_watched",
            ),
        ] {
            assert_eq!(wanted(&decision, age, status), Err(why));
        }
        assert!(
            wanted(
                &decided("say_now", false, &key),
                PROGRESS_FRESH_MS,
                Some(&now)
            )
            .is_ok()
        );
    }

    /// A progress note is one written turn too: long tasks and long lines,
    /// in letters of one byte or of several, fit and are taken.
    #[test]
    fn a_progress_note_says_who_what_how_far_and_fits_in_one_written_turn() {
        let signs = BTreeSet::from([Sign::TestsPass, Sign::PullRequest]);
        let short = note_progress(
            &task("claude", json!([])),
            "Fixed the tokenizer / all 118 tests pass",
            &signs,
            Some(600),
        )
        .unwrap();
        assert_eq!(
            short,
            "(Nobody has spoken. Progress on work you handed over for the person: Claude Code is still working on the task they gave it: \
             \"Suggest three 'friendly' names for a desk lamp\". Its latest progress line: \"Fixed the tokenizer / all 118 tests pass\". \
             Signs of a step forward in it: the tests pass, a pull request was opened. It has been at it for about 10 minutes. \
             Tell them how it is going in one short, natural sentence, the way a good chief of staff mentions it in passing, \
             starting with something like \"Oh, Claude Code has…\". Do not read the line out word for word, and no code, file paths or links. \
             No question at the end.)"
        );
        // No signs, a minute, Codex; nothing to say without a line.
        let bare = note_progress(
            &task("codex", json!([])),
            "Reading the parser",
            &BTreeSet::new(),
            Some(40),
        )
        .unwrap();
        assert!(
            bare.contains("Codex is still working")
                && bare.contains("about a minute.")
                && !bare.contains("Signs")
        );
        assert!(note_progress(&task("claude", json!([])), "  ", &signs, None).is_none());
        let english =
            "Read src/parser.rs and the failing test, fixed the end index, ran the suite. "
                .repeat(20);
        let vietnamese =
            "Đã đọc tệp phân tích cú pháp, sửa chỉ số cuối và chạy lại toàn bộ kiểm thử. "
                .repeat(20);
        let asks = [
            "Fix the quoted string bug in the parser".to_owned(),
            "Sửa lỗi chuỗi trong ngoặc kép ở bộ phân tích cú pháp, rồi chạy lại kiểm thử. "
                .repeat(10),
        ];
        for asked in &asks {
            for about in [&english, &vietnamese] {
                let told = note_progress(
                    &json!({"text": asked, "target": {"engine": "claude"}}),
                    about,
                    &BTreeSet::from([
                        Sign::TestsPass,
                        Sign::BuildWorks,
                        Sign::Fixed,
                        Sign::Found,
                        Sign::Done,
                        Sign::PullRequest,
                    ]),
                    Some(86_400),
                )
                .unwrap();
                assert!(
                    told.len() <= lamp_gemini::wire::MAX_NOTE_BYTES,
                    "{} bytes",
                    told.len()
                );
                lamp_gemini::wire::encode_written_turn(&told, None).unwrap();
                assert!(
                    told.contains("(it goes on)") && told.ends_with("No question at the end.)"),
                    "{told}"
                );
            }
        }
    }

    /// #765 review: words that come back at or past the decision's time
    /// are let go, however long the fetching took.
    #[test]
    fn words_that_come_back_past_the_decisions_time_are_let_go() {
        let root = std::env::temp_dir().join(format!("task-news-late-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let socket = root.join("request.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let answer = json!({"ok":true,"store_id":STORE,"text":"Fix the parser",
                    "target":{"engine":"claude"},"progress":{"state":"working","run":RUN,"changes":4,
                    "about":"All tests pass","signs_seen":[]}});
                writeln!(stream, "{answer}").unwrap();
            }
        });
        let (offer, _offered) = mpsc::channel();
        let reader = ProgressReader {
            socket,
            offer,
            told: Arc::new(Mutex::new(BTreeSet::new())),
            stopping: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(None)),
        };
        let wanted = Wanted {
            store: STORE.into(),
            task: 3,
            change: 4,
            run: RUN,
            working_s: None,
        };
        let mut said = BTreeSet::new();
        assert_eq!(
            reader.news(&wanted, &mut said, Instant::now()).unwrap_err(),
            "stale"
        );
        let fresh = Instant::now() + Duration::from_secs(60);
        let news = reader.news(&wanted, &mut said, fresh).unwrap();
        assert_eq!(news.until, Some(fresh));
        assert_eq!(
            news.progress,
            Some(ProgressOf {
                store: STORE.into(),
                task: 3,
                change: 4,
                run: RUN,
            })
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn progress_not_told_in_time_is_let_go() {
        let (offer, offered) = mpsc::channel();
        let (told, _heard) = mpsc::channel();
        let mut news = TaskNews {
            offered,
            told,
            waiting: VecDeque::new(),
            told_already: Arc::new(Mutex::new(BTreeSet::new())),
            stopping: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(None)),
        };
        let soon = News {
            key: "s:3:progress-1".into(),
            note: "(…)".into(),
            until: Some(Instant::now() + Duration::from_millis(50)),
            progress: None,
        };
        offer.send(soon.clone()).unwrap();
        assert_eq!(news.waiting(), Some(&soon));
        std::thread::sleep(Duration::from_millis(60));
        assert!(news.waiting().is_none());
    }

    #[test]
    fn news_waits_once_and_telling_takes_it() {
        let (offer, offered) = mpsc::channel();
        let (told, heard) = mpsc::channel();
        let told_already = Arc::new(Mutex::new(BTreeSet::new()));
        let mut news = TaskNews {
            offered,
            told,
            waiting: VecDeque::new(),
            told_already: told_already.clone(),
            stopping: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(None)),
        };
        assert!(news.waiting().is_none());
        let one = News {
            key: "s:1:1".into(),
            note: "(…)".into(),
            until: None,
            progress: None,
        };
        offer.send(one.clone()).unwrap();
        offer.send(one.clone()).unwrap();
        assert_eq!(news.waiting(), Some(&one));
        assert_eq!(news.tell(), Some(one));
        assert_eq!(heard.try_recv().unwrap(), "s:1:1");
        assert!(news.waiting().is_none());
        // Asked for and heard before it was told: it is not told.
        let two = News {
            key: "s:2:1".into(),
            note: "(…)".into(),
            until: None,
            progress: None,
        };
        offer.send(two.clone()).unwrap();
        assert_eq!(news.waiting(), Some(&two));
        told_already.lock().unwrap().insert("s:2:1".into());
        assert!(news.waiting().is_none());
    }
}
