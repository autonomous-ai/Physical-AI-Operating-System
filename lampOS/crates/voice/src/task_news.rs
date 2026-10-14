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
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, VecDeque},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

/// Where the coordinator reads the file of what was told.
pub const TOLD_ENVIRONMENT: &str = "LAMP_TASKS_TOLD";
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
/// Nothing said by or to the lamp for this long before news is told.
pub const QUIET_US: u64 = 15_000_000;
/// Without eyes that say someone is there, something said this recently
/// says it.
pub const HEARD_WITHIN_US: u64 = 120_000_000;

/// One piece of news, ready to tell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct News {
    /// The tasks part's store, task and observation: what is told once.
    pub key: String,
    /// The note the model is given.
    pub note: String,
}

pub struct TaskNews {
    offered: Receiver<News>,
    told: Sender<String>,
    waiting: VecDeque<News>,
    /// What the file says was told, as the reading thread last found it:
    /// news the person asked for and heard ([`crate::agent_tasks`]) is not
    /// told again.
    told_already: Arc<Mutex<BTreeSet<String>>>,
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

fn read_told(file: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(file)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

impl TaskNews {
    /// Reading starts on a thread of its own when the tasks part is set.
    pub fn from_environment() -> Option<Self> {
        let directory = std::env::var_os(crate::agent_tasks::ENVIRONMENT)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())?;
        let told_file = told_file();
        let (offer, offered) = mpsc::channel();
        let (told, heard_told) = mpsc::channel();
        let told_already = Arc::new(Mutex::new(BTreeSet::new()));
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
        self.waiting.front()
    }

    /// Take the news waiting, as it is being told: it is not offered again.
    pub fn tell(&mut self) -> Option<News> {
        let news = self.waiting.pop_front()?;
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
        std::thread::sleep(READ_EVERY);
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
    crate::tools::exchange(socket, line, ASK_DEADLINE).filter(|answer| answer["ok"] == json!(true))
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
                    news.push(News { key, note });
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
            if let Some(note) = note(&task, &observation["data"], age_ms) {
                news.push(News { key, note });
            }
        }
        match page["next_after_id"].as_u64() {
            Some(next) if next > after => after = next,
            _ => return news,
        }
    }
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
    let more = if cut || data["truncated"] == json!(true) {
        " …(it goes on)"
    } else {
        ""
    };
    let when = if age_ms <= JUST_MS {
        "just"
    } else {
        "a while ago, and you have not told them yet:"
    };
    let opening = "(Nobody has spoken. News from work you handed over for the person.";
    match data["state"].as_str()? {
        "idle" if !answer.is_empty() => Some(format!(
            "{opening} {who} {when} finished the task they gave it: \"{asked}\". What it came back with: \"{answer}{more}\". \
             Tell them now, the way a good chief of staff tells the person they work for: one or two natural spoken sentences, \
             starting with something like \"Oh, {who} just finished…\" (or \"earlier\" if it was a while ago), then the gist of the result in plain words. \
             Do not read out code, lists, file paths or links; if there is more than fits, say the rest is on their computer. No question at the end.)"
        )),
        "needs-input" => Some(format!(
            "{opening} {who} has stopped to ask them something about the task they gave it: \"{asked}\". What it says: \"{answer}{more}\". \
             Tell them briefly that {who} needs them, and ask its question in your own words, in one short sentence.)"
        )),
        "stopped" | "failed" => Some(format!(
            "{opening} {who} stopped without finishing the task they gave it: \"{asked}\". What it last said: \"{answer}{more}\". \
             Tell them in one short, plain sentence, without alarm.)"
        )),
        _ => None,
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
        let asking = json!({"state":"needs-input","answer":"Which folder?"});
        assert!(
            super::note(&task("claude", json!([])), &asking, 0)
                .unwrap()
                .contains("ask its question")
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
            long.contains("(it goes on)") && long.len() < 2_200,
            "{}",
            long.len()
        );
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
        };
        assert!(news.waiting().is_none());
        let one = News {
            key: "s:1:1".into(),
            note: "(…)".into(),
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
        };
        offer.send(two.clone()).unwrap();
        assert_eq!(news.waiting(), Some(&two));
        told_already.lock().unwrap().insert("s:2:1".into());
        assert!(news.waiting().is_none());
    }
}
