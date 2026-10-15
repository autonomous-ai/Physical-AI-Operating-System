//! Every part's newest report, and how they become the lamp's health.
//!
//! A part says how it is: ok, warn or error, and a sentence. It never says
//! of itself that it has gone quiet, because a part that has gone quiet
//! says nothing. That is said here, of a part not heard from for longer
//! than the stale time, and of a part that is expected and has never
//! reported at all.
//!
//! [`health`] is the whole of the rule and touches nothing: the reports,
//! the time and what is expected go in, and a `Health` comes out.
use lamp_msgs::{Diagnostic, Health, HealthItem, Level, Online};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

/// No more parts than this are kept. A lamp has a dozen; a publisher that
/// makes up a new name with every message must not fill the memory, or
/// make a `Health` too large for the bus to carry.
pub const MOST_PARTS: usize = 64;
/// A part's name is no longer than this.
pub const MOST_NAME_BYTES: usize = 48;
/// A part's message is cut to this: with 64 parts of the longest name and
/// message, a `Health` is still well under the bus's 60,000 bytes.
pub const MOST_MESSAGE_BYTES: usize = 160;

/// One part's newest report, and when it came.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub level: Level,
    pub message: String,
    /// When it arrived, by this program's own clock.
    pub at: Instant,
    /// Its link to something on the internet, as its `link` fact says it
    /// (`ready`, `connecting`, `down`), with `down_for_s` (#830).
    pub link: Option<String>,
    pub down_for_s: Option<u64>,
}

/// What became of a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    /// It is its part's newest.
    Kept,
    /// Its name is not one a part can have.
    NotAName,
    /// It is from a part not heard before, and 64 are kept already.
    Full,
}

/// The newest report of every part that has reported.
#[derive(Debug, Default)]
pub struct Parts {
    kept: BTreeMap<String, Kept>,
}

impl Parts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep `report` as its part's newest, come at `at`.
    pub fn take(&mut self, report: &Diagnostic, at: Instant) -> Taken {
        if !is_name(&report.name) {
            return Taken::NotAName;
        }
        if self.kept.len() >= MOST_PARTS && !self.kept.contains_key(&report.name) {
            return Taken::Full;
        }
        self.kept.insert(
            report.name.clone(),
            Kept {
                level: report.level,
                message: one_line(&report.message),
                at,
                link: fact(report, "link").map(one_line),
                down_for_s: fact(report, "down_for_s").and_then(|value| value.parse().ok()),
            },
        );
        Taken::Kept
    }

    pub fn reports(&self) -> &BTreeMap<String, Kept> {
        &self.kept
    }
}

fn fact<'a>(report: &'a Diagnostic, key: &str) -> Option<&'a str> {
    report
        .values
        .iter()
        .find(|value| value.key == key)
        .map(|value| value.value.as_str())
}

/// The parts whose `link` says whether the lamp can reach the internet,
/// in the order they are believed. Today only the voice's link to its
/// speech service.
pub const ONLINE_FROM: [&str; 1] = ["voice"];

/// Whether the lamp is online at `now` (`lamp/online`): from the first
/// part in [`ONLINE_FROM`] with a fresh report that has a link. `ready`
/// and `connecting` (down under the voice's 5 s grace) are online, `down`
/// is not; with none of them fresh, nobody knows.
pub fn online(reports: &BTreeMap<String, Kept>, now: Instant, stale_after: Duration) -> Online {
    for name in ONLINE_FROM {
        let Some(kept) = reports.get(name) else {
            continue;
        };
        if now.saturating_duration_since(kept.at) >= stale_after {
            continue;
        }
        let online = match kept.link.as_deref() {
            Some("ready" | "connecting") => true,
            Some("down") => false,
            _ => continue,
        };
        return Online {
            online: Some(online),
            from: name.into(),
            down_for_s: if online { None } else { kept.down_for_s },
        };
    }
    Online {
        online: None,
        from: String::new(),
        down_for_s: None,
    }
}

/// Whether a part can be called this: something, on one line, of 48 bytes
/// at most, with no space before or after it.
pub fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MOST_NAME_BYTES
        && name.trim() == name
        && !name.chars().any(char::is_control)
}

/// A message as it is kept: on one line, and cut where it is too long.
fn one_line(message: &str) -> String {
    let line: String = message
        .trim()
        .chars()
        .map(|letter| if letter.is_control() { ' ' } else { letter })
        .collect();
    if line.len() <= MOST_MESSAGE_BYTES {
        return line;
    }
    // Room for the sign that something was cut, which is three bytes.
    let mut end = MOST_MESSAGE_BYTES - 3;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &line[..end])
}

fn millis(time: Duration) -> u64 {
    u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
}

/// What is said of an item that is not ok, for the lamp's one sentence.
fn said(item: &HealthItem) -> String {
    if item.message.is_empty() {
        format!(
            "{}: {}, and it does not say why",
            item.name,
            item.level.as_str()
        )
    } else {
        format!("{}: {}", item.name, item.message)
    }
}

/// How the whole lamp is at `now`, from each part's newest report.
///
/// - A part heard from within `stale_after` has the level and the message
///   it gave. One not heard from for longer is `stale`, and its message
///   says for how many whole seconds. One that had said `warn` or `error`
///   before it went quiet keeps that level, and what it said stays in
///   front of the seconds: a body that stopped itself and said why is not
///   made milder by its silence. A part that calls itself `stale` is
///   taken at its word.
/// - `missing` is every name in `expected` that has never reported, in the
///   order given, each once.
/// - The items are worst first (`Level::rank`), then by name. The lamp's
///   level is the worst item's, and `stale` at least when a part is
///   missing.
/// - The message is empty when the level is ok. Otherwise it is the name
///   and the message of the first item that is not ok, or, when only a
///   missing part makes the lamp not ok, that part's name and `has never
///   reported`.
///
/// What comes out passes `Health::check`.
pub fn health(
    reports: &BTreeMap<String, Kept>,
    now: Instant,
    expected: &[String],
    stale_after: Duration,
) -> Health {
    let limit = millis(stale_after);
    let mut items: Vec<HealthItem> = reports
        .iter()
        .map(|(name, kept)| {
            let age_ms = millis(now.saturating_duration_since(kept.at));
            let (level, message) = if age_ms > limit {
                // Quiet. It is stale at least, and a part that said
                // something was wrong before it went quiet is no better
                // for having gone quiet: it keeps the level it gave and
                // what it last said, which is often why it went quiet.
                let quiet = format!("not heard from for {} s", age_ms / 1_000);
                if kept.level.rank() > Level::Stale.rank() && !kept.message.is_empty() {
                    (kept.level, format!("{}; {quiet}", kept.message))
                } else if kept.level.rank() > Level::Stale.rank() {
                    (kept.level, quiet)
                } else {
                    (Level::Stale, quiet)
                }
            } else {
                (kept.level, kept.message.clone())
            };
            HealthItem {
                name: name.clone(),
                level,
                message,
                age_ms,
            }
        })
        .collect();
    items.sort_by(|a, b| {
        b.level
            .rank()
            .cmp(&a.level.rank())
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut missing: Vec<String> = Vec::new();
    for name in expected {
        if !reports.contains_key(name) && !missing.contains(name) {
            missing.push(name.clone());
        }
    }
    let worst = items.first().filter(|item| item.level != Level::Ok);
    let (level, message) = match (worst, missing.first()) {
        (Some(item), _) => (item.level, said(item)),
        (None, Some(name)) => (Level::Stale, format!("{name}: has never reported")),
        (None, None) => (Level::Ok, String::new()),
    };
    Health {
        level,
        message,
        items,
        missing,
    }
}

/// Whether two healths say the same but for the ages: each item's
/// `age_ms`, and the count of seconds in the message of a part that is
/// stale because it has not been heard from. The lamp's own message is not
/// compared: it follows from the items and the missing.
///
/// This is what decides whether a health is news. A part that stays quiet
/// is news once, when it goes stale, and not again with every second.
pub fn same_but_for_ages(a: &Health, b: &Health, stale_after: Duration) -> bool {
    let limit = millis(stale_after);
    let quiet = |item: &HealthItem| item.age_ms > limit;
    a.level == b.level
        && a.missing == b.missing
        && a.items.len() == b.items.len()
        && a.items.iter().zip(&b.items).all(|(a, b)| {
            a.name == b.name
                && a.level == b.level
                && quiet(a) == quiet(b)
                && (quiet(a) || a.message == b.message)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STALE: Duration = Duration::from_secs(5);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn report(level: Level, name: &str, message: &str) -> Diagnostic {
        Diagnostic {
            level,
            name: name.into(),
            message: message.into(),
            values: Vec::new(),
        }
    }

    /// The reports of a test: each a level, a name, a message and how
    /// long before `now` it came.
    fn lamp(reports: &[(Level, &str, &str, u64)]) -> (Parts, Instant) {
        // Far enough from the clock's start for every age to be taken off.
        let now = Instant::now() + Duration::from_secs(3_600);
        let mut parts = Parts::new();
        for (level, name, message, age_ms) in reports {
            assert_eq!(
                parts.take(&report(*level, name, message), now - ms(*age_ms)),
                Taken::Kept
            );
        }
        (parts, now)
    }

    fn of(reports: &[(Level, &str, &str, u64)], expected: &[&str]) -> Health {
        let (parts, now) = lamp(reports);
        let expected: Vec<String> = expected.iter().map(|name| (*name).to_owned()).collect();
        let health = health(parts.reports(), now, &expected, STALE);
        assert_eq!(health.check(), Ok(()), "{health:?}");
        health
    }

    fn shape(health: &Health) -> Vec<(&str, Level, &str)> {
        health
            .items
            .iter()
            .map(|item| (item.name.as_str(), item.level, item.message.as_str()))
            .collect()
    }

    #[test]
    fn a_lamp_whose_parts_are_all_well_is_ok_and_says_nothing() {
        let health = of(
            &[
                (Level::Ok, "voice", "", 300),
                (Level::Ok, "body", "", 400),
                (Level::Ok, "eyes", "", 100),
            ],
            &["voice", "body"],
        );
        assert_eq!((health.level, health.message.as_str()), (Level::Ok, ""));
        // Among parts as well as each other, by name.
        assert_eq!(
            shape(&health),
            [
                ("body", Level::Ok, ""),
                ("eyes", Level::Ok, ""),
                ("voice", Level::Ok, "")
            ]
        );
        assert_eq!(
            health
                .items
                .iter()
                .map(|item| item.age_ms)
                .collect::<Vec<_>>(),
            [400, 100, 300]
        );
        assert!(health.missing.is_empty());
        // Nothing at all has reported and nothing is expected: ok too.
        let nothing = of(&[], &[]);
        assert_eq!((nothing.level, nothing.message.as_str()), (Level::Ok, ""));
        assert!(nothing.items.is_empty() && nothing.missing.is_empty());
    }

    #[test]
    fn the_items_are_worst_first_then_by_name_and_the_lamp_is_as_bad_as_its_worst() {
        let health = of(
            &[
                (Level::Ok, "memory", "", 600),
                (Level::Warn, "body", "yaw is hot", 400),
                (Level::Ok, "actions", "", 200),
                (
                    Level::Error,
                    "voice",
                    "the link to the service is down",
                    300,
                ),
                (Level::Ok, "eyes", "", 7_300),
                (Level::Warn, "board", "the disk is filling up", 100),
                (Level::Error, "ring", "no light answers", 900),
            ],
            &[],
        );
        assert_eq!(
            shape(&health),
            [
                ("ring", Level::Error, "no light answers"),
                ("voice", Level::Error, "the link to the service is down"),
                ("board", Level::Warn, "the disk is filling up"),
                ("body", Level::Warn, "yaw is hot"),
                ("eyes", Level::Stale, "not heard from for 7 s"),
                ("actions", Level::Ok, ""),
                ("memory", Level::Ok, ""),
            ]
        );
        assert_eq!(health.level, Level::Error);
        // The first item that is not ok.
        assert_eq!(health.message, "ring: no light answers");
    }

    #[test]
    fn each_level_is_the_lamps_when_nothing_is_worse() {
        for (worst, others, message) in [
            (Level::Error, [Level::Warn, Level::Ok], "a: wrong"),
            (Level::Warn, [Level::Ok, Level::Ok], "a: wrong"),
            (Level::Stale, [Level::Ok, Level::Ok], "a: wrong"),
        ] {
            let health = of(
                &[
                    (others[0], "b", "also", 10),
                    (worst, "a", "wrong", 10),
                    (others[1], "c", "", 10),
                ],
                &[],
            );
            assert_eq!(health.level, worst);
            assert_eq!(health.message, message);
            assert_eq!(health.items[0].name, "a");
        }
        // Stale is worse than ok and not as bad as a warning.
        let health = of(
            &[
                (Level::Ok, "voice", "", 6_000),
                (Level::Warn, "body", "yaw is hot", 10),
            ],
            &[],
        );
        assert_eq!(health.level, Level::Warn);
        assert_eq!(health.message, "body: yaw is hot");
        assert_eq!(
            shape(&health)[1],
            ("voice", Level::Stale, "not heard from for 6 s")
        );
    }

    #[test]
    fn a_part_is_stale_once_it_has_been_quiet_for_longer_than_the_stale_time() {
        // To the millisecond: at the limit it is still what it said.
        let at = of(&[(Level::Warn, "body", "yaw is hot", 5_000)], &[]);
        assert_eq!(shape(&at), [("body", Level::Warn, "yaw is hot")]);
        assert_eq!(at.level, Level::Warn);
        let past = of(&[(Level::Ok, "body", "", 5_001)], &[]);
        assert_eq!(
            shape(&past),
            [("body", Level::Stale, "not heard from for 5 s")]
        );
        assert_eq!(
            (past.level, past.message.as_str()),
            (Level::Stale, "body: not heard from for 5 s")
        );
        assert_eq!(past.items[0].age_ms, 5_001);
        // A part that said something was wrong and then went quiet is no
        // better for it: it keeps its level, and what it said.
        let warned = of(&[(Level::Warn, "body", "yaw is hot", 5_001)], &[]);
        assert_eq!(
            shape(&warned),
            [("body", Level::Warn, "yaw is hot; not heard from for 5 s")]
        );
        assert_eq!(warned.message, "body: yaw is hot; not heard from for 5 s");
        let error = of(
            &[(Level::Error, "body", "stopped: elbow is too warm", 9_400)],
            &[],
        );
        assert_eq!(
            (error.level, error.message.as_str()),
            (
                Level::Error,
                "body: stopped: elbow is too warm; not heard from for 9 s"
            )
        );
        // With nothing said beside the level, the seconds alone.
        let silent = of(&[(Level::Error, "voice", "", 6_000)], &[]);
        assert_eq!(
            shape(&silent),
            [("voice", Level::Error, "not heard from for 6 s")]
        );
        // And it is still worse than a part that was well and went quiet.
        let both = of(
            &[
                (Level::Ok, "eyes", "", 8_000),
                (Level::Warn, "body", "yaw is hot", 8_000),
            ],
            &[],
        );
        assert_eq!(both.items[0].name, "body");
        assert_eq!(both.level, Level::Warn);
        // Whole seconds, not rounded up.
        let later = of(&[(Level::Ok, "eyes", "", 7_999)], &[]);
        assert_eq!(later.items[0].message, "not heard from for 7 s");
        let long = of(&[(Level::Ok, "eyes", "", 3_600_000)], &[]);
        assert_eq!(long.items[0].message, "not heard from for 3600 s");
        // Another stale time is another limit.
        let (parts, now) = lamp(&[(Level::Ok, "eyes", "", 150)]);
        let quick = health(parts.reports(), now, &[], ms(149));
        assert_eq!(quick.check(), Ok(()));
        assert_eq!(
            shape(&quick),
            [("eyes", Level::Stale, "not heard from for 0 s")]
        );
        let slow = health(parts.reports(), now, &[], ms(150));
        assert_eq!(shape(&slow), [("eyes", Level::Ok, "")]);
    }

    #[test]
    fn a_part_that_comes_back_from_stale_is_what_it_says_again() {
        let (mut parts, now) = lamp(&[
            (Level::Ok, "eyes", "", 9_000),
            (Level::Ok, "voice", "", 200),
        ]);
        let before = health(parts.reports(), now, &[], STALE);
        assert_eq!(before.check(), Ok(()));
        assert_eq!(
            (before.level, before.message.as_str()),
            (Level::Stale, "eyes: not heard from for 9 s")
        );
        // It reports again, and what is kept is the new report alone.
        let later = now + ms(250);
        assert_eq!(
            parts.take(&report(Level::Warn, "eyes", "the camera is slow"), later),
            Taken::Kept
        );
        let after = health(parts.reports(), later + ms(50), &[], STALE);
        assert_eq!(after.check(), Ok(()));
        assert_eq!(
            shape(&after),
            [
                ("eyes", Level::Warn, "the camera is slow"),
                ("voice", Level::Ok, "")
            ]
        );
        assert_eq!(after.items[0].age_ms, 50);
        assert_eq!(after.items[1].age_ms, 500);
        // And well again with its next.
        parts.take(&report(Level::Ok, "eyes", ""), later + ms(1_000));
        let well = health(parts.reports(), later + ms(1_000), &[], STALE);
        assert_eq!((well.level, well.message.as_str()), (Level::Ok, ""));
        // A report that seems to come from after `now` is of no age.
        let early = health(parts.reports(), later, &[], STALE);
        assert_eq!(early.items[0].age_ms, 0);
    }

    #[test]
    fn a_part_that_calls_itself_stale_is_taken_at_its_word() {
        let health = of(
            &[
                (Level::Stale, "voice", "the service has not answered", 300),
                (Level::Ok, "body", "", 300),
            ],
            &[],
        );
        assert_eq!(
            shape(&health),
            [
                ("voice", Level::Stale, "the service has not answered"),
                ("body", Level::Ok, "")
            ]
        );
        assert_eq!(
            (health.level, health.message.as_str()),
            (Level::Stale, "voice: the service has not answered")
        );
        // Until it goes quiet itself.
        let quiet = of(&[(Level::Stale, "voice", "its own words", 6_500)], &[]);
        assert_eq!(quiet.items[0].message, "not heard from for 6 s");
    }

    #[test]
    fn expected_parts_that_never_reported_are_missing_in_the_order_given() {
        let health = of(
            &[(Level::Ok, "voice", "", 100)],
            &["memory", "voice", "actions", "memory", "body"],
        );
        assert_eq!(health.missing, ["memory", "actions", "body"]);
        // A missing part alone makes the lamp stale, and is what is said.
        assert_eq!(
            (health.level, health.message.as_str()),
            (Level::Stale, "memory: has never reported")
        );
        // Nothing has reported at all.
        let empty = of(&[], &["voice"]);
        assert_eq!(
            (empty.level, empty.message.as_str()),
            (Level::Stale, "voice: has never reported")
        );
        assert!(empty.items.is_empty());
        // A part that is not ok is said before a missing one, whatever its
        // level: an item that went quiet, a warning, an error.
        for (level, age, message) in [
            (Level::Ok, 8_000, "voice: not heard from for 8 s"),
            (Level::Warn, 100, "voice: slow"),
            (Level::Error, 100, "voice: slow"),
        ] {
            let health = of(&[(level, "voice", "slow", age)], &["actions"]);
            assert_eq!(health.message, message);
            assert_eq!(health.missing, ["actions"]);
            assert!(health.level.rank() >= Level::Stale.rank());
        }
        // A part that reported once is never missing again, however long
        // ago: it is stale.
        let old = of(&[(Level::Ok, "actions", "", 900_000)], &["actions"]);
        assert!(old.missing.is_empty());
        assert_eq!(old.items[0].level, Level::Stale);
        // A part nobody expected is an item like any other.
        let extra = of(&[(Level::Ok, "ring", "", 100)], &[]);
        assert_eq!((extra.level, extra.items.len()), (Level::Ok, 1));
    }

    #[test]
    fn a_part_that_is_not_ok_and_says_nothing_is_still_named() {
        let health = of(
            &[(Level::Warn, "body", "", 100), (Level::Ok, "eyes", "", 100)],
            &[],
        );
        assert_eq!(health.message, "body: warn, and it does not say why");
        let health = of(&[(Level::Error, "body", "  \n", 100)], &[]);
        assert_eq!(health.message, "body: error, and it does not say why");
        // What a part that is well says stays with the part.
        let health = of(&[(Level::Ok, "body", "warming up", 100)], &[]);
        assert_eq!((health.level, health.message.as_str()), (Level::Ok, ""));
        assert_eq!(health.items[0].message, "warming up");
    }

    #[test]
    fn a_report_is_kept_by_its_name_and_the_newest_replaces_the_one_before() {
        let now = Instant::now();
        let mut parts = Parts::new();
        parts.take(&report(Level::Error, "voice", "down"), now);
        parts.take(&report(Level::Ok, "body", ""), now);
        parts.take(&report(Level::Ok, "voice", ""), now + ms(10));
        assert_eq!(parts.reports().len(), 2);
        assert_eq!(
            parts.reports()["voice"],
            Kept {
                level: Level::Ok,
                message: String::new(),
                at: now + ms(10),
                link: None,
                down_for_s: None,
            }
        );
    }

    #[test]
    fn online_is_the_voice_link_while_its_report_is_fresh() {
        let now = Instant::now();
        let stale = Duration::from_secs(5);
        let mut parts = Parts::new();
        let with = |link: &str, down: Option<&str>| {
            let mut report = report(Level::Ok, "voice", "");
            report.values.push(lamp_msgs::KeyValue {
                key: "link".into(),
                value: link.into(),
            });
            if let Some(down) = down {
                report.values.push(lamp_msgs::KeyValue {
                    key: "down_for_s".into(),
                    value: down.into(),
                });
            }
            report
        };
        // Nobody reporting a link: unknown.
        parts.take(&report(Level::Ok, "body", ""), now);
        assert_eq!(online(parts.reports(), now, stale).online, None);
        for (link, down, said, for_s) in [
            ("ready", None, Some(true), None),
            ("connecting", None, Some(true), None),
            ("down", Some("12"), Some(false), Some(12)),
            ("down", None, Some(false), None),
            ("something else", None, None, None),
        ] {
            parts.take(&with(link, down), now);
            let found = online(parts.reports(), now, stale);
            assert_eq!((found.online, found.down_for_s), (said, for_s), "{link}");
            assert!(found.check().is_ok());
        }
        // A voice that has stopped reporting says nothing any more.
        parts.take(&with("down", Some("3")), now);
        assert_eq!(online(parts.reports(), now + stale, stale).online, None);
    }

    #[test]
    fn a_name_is_something_short_on_one_line_and_a_message_is_kept_on_one() {
        for name in ["voice", "body", "Voice 2", "mắt", "voice/link", "a"] {
            assert!(is_name(name), "{name:?}");
        }
        assert!(is_name(&"n".repeat(MOST_NAME_BYTES)));
        for name in [
            "",
            " voice",
            "voice ",
            "vo\nice",
            "vo\tice",
            "\u{1b}[31mvoice",
            &"n".repeat(MOST_NAME_BYTES + 1),
        ] {
            assert!(!is_name(name), "{name:?}");
        }
        let now = Instant::now();
        let mut parts = Parts::new();
        assert_eq!(
            parts.take(&report(Level::Ok, "vo\nice", ""), now),
            Taken::NotAName
        );
        assert!(parts.reports().is_empty());
        // Line ends and the like become spaces, and the ends are trimmed.
        parts.take(
            &report(Level::Warn, "body", "  yaw is hot\nand slow\t\n"),
            now,
        );
        assert_eq!(parts.reports()["body"].message, "yaw is hot and slow");
        // A long message is cut on a whole character, and says that it is.
        parts.take(&report(Level::Warn, "body", &"nóng ".repeat(60)), now);
        let cut = &parts.reports()["body"].message;
        assert!(cut.len() <= MOST_MESSAGE_BYTES, "{}", cut.len());
        assert!(cut.len() > MOST_MESSAGE_BYTES - 8, "{}", cut.len());
        assert!(cut.starts_with("nóng nóng ") && cut.ends_with('…'));
        // One of exactly the most is kept whole.
        let most = "x".repeat(MOST_MESSAGE_BYTES);
        parts.take(&report(Level::Warn, "body", &most), now);
        assert_eq!(parts.reports()["body"].message, most);
    }

    #[test]
    fn no_more_than_sixty_four_parts_are_kept_and_those_kept_still_report() {
        let now = Instant::now();
        let mut parts = Parts::new();
        for number in 0..MOST_PARTS {
            let name = format!("part{number}");
            assert_eq!(
                parts.take(&report(Level::Ok, &name, ""), now),
                Taken::Kept,
                "{name}"
            );
        }
        assert_eq!(
            parts.take(&report(Level::Error, "one more", "x"), now),
            Taken::Full
        );
        assert_eq!(parts.reports().len(), MOST_PARTS);
        assert_eq!(
            parts.take(&report(Level::Warn, "part7", "hot"), now),
            Taken::Kept
        );
        let health = health(parts.reports(), now, &[], STALE);
        assert_eq!(health.check(), Ok(()));
        assert_eq!(health.message, "part7: hot");
        // The largest health there can be fits in one message of the bus:
        // every name and message as long as may be, and as many expected.
        let mut parts = Parts::new();
        let long = "\"".repeat(MOST_MESSAGE_BYTES);
        let mut expected = Vec::new();
        for number in 0..MOST_PARTS {
            let name = format!("{number:02}{}", "\\".repeat(MOST_NAME_BYTES - 2));
            parts.take(&report(Level::Error, &name, &long), now);
            expected.push(format!("m{number:02}{}", "\\".repeat(MOST_NAME_BYTES - 3)));
        }
        let largest = super::health(parts.reports(), now, &expected, STALE);
        assert_eq!(largest.missing.len(), MOST_PARTS);
        let bytes = serde_json::to_vec(&largest).unwrap().len();
        assert!(bytes < 50_000, "{bytes}");
    }

    #[test]
    fn two_healths_are_the_same_news_when_only_ages_differ() {
        let same = |a: &Health, b: &Health| same_but_for_ages(a, b, STALE);
        let reports = [
            (Level::Warn, "body", "yaw is hot", 400),
            (Level::Ok, "eyes", "", 7_300),
            (Level::Ok, "voice", "", 300),
        ];
        let first = of(&reports, &["actions"]);
        assert!(same(&first, &first));
        // A second later: every age is another, and so is the count in
        // the quiet part's message.
        let mut later = reports;
        for report in &mut later {
            report.3 += 1_000;
        }
        let second = of(&later, &["actions"]);
        assert_ne!(first, second);
        assert_eq!(second.items[1].message, "not heard from for 8 s");
        assert!(same(&first, &second));
        // A level, a message, a part more, a part missing: each is news.
        let mut level = reports;
        level[2].0 = Level::Warn;
        assert!(!same(&first, &of(&level, &["actions"])));
        let mut message = reports;
        message[0].2 = "yaw is very hot";
        assert!(!same(&first, &of(&message, &["actions"])));
        assert!(!same(&first, &of(&reports[..2], &["actions"])));
        assert!(!same(&first, &of(&reports, &[])));
        assert!(!same(&first, &of(&reports, &["actions", "memory"])));
        // A part going quiet is news, and so is its coming back as it was.
        let mut quiet = reports;
        quiet[2].3 = 5_001;
        assert!(!same(&first, &of(&quiet, &["actions"])));
        let mut back = reports;
        back[1].3 = 10;
        assert!(!same(&first, &of(&back, &["actions"])));
        // A part that calls itself stale has a message of its own, and a
        // change in that is news like any other.
        let own = of(
            &[(Level::Stale, "voice", "waiting for the service", 10)],
            &[],
        );
        let other = of(&[(Level::Stale, "voice", "waiting for a key", 10)], &[]);
        assert!(!same(&own, &other));
        // Once it has gone quiet as well, only its silence is said.
        let gone = of(
            &[(Level::Stale, "voice", "waiting for the service", 6_000)],
            &[],
        );
        let longer = of(&[(Level::Stale, "voice", "waiting for a key", 9_000)], &[]);
        assert!(same(&gone, &longer));
        assert!(!same(&own, &gone));
    }
}
