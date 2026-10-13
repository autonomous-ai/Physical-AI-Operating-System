//! How the lamp is, for a person: what `lamp-health` prints, and the
//! status it ends with.
//!
//! It asks nobody. It reads the two values the bus keeps, `lamp/health`
//! and `board/state`, and shows what is in them. A value that has outlived
//! its time (fifteen seconds) is the same as none: whoever wrote it has
//! stopped. So what is shown is a picture of a moment, a few seconds old
//! at most, and the last line says how old.
use lamp_bus::Bus;
use lamp_msgs::{BoardState, Health, Invalid, Level};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fmt::Write,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// What `show` ends with for each level, so that a script can ask.
pub const STATUS_OK: u8 = 0;
pub const STATUS_WARN: u8 = 1;
pub const STATUS_ERROR: u8 = 2;
/// A part is stale or missing, or nobody is reporting the lamp's health
/// at all.
pub const STATUS_STALE: u8 = 3;
/// `show` was called wrongly. Not 1 or 2, which are levels.
pub const STATUS_USAGE: u8 = 64;

/// What is said when the bus keeps no health.
pub const NOBODY: &str = "Nobody is reporting the lamp's health: lamp-health serve is not running, or the bus is somewhere else.";

/// A value the bus keeps, and how long ago it was written.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept<T> {
    pub value: T,
    pub age: Duration,
}

/// What the bus keeps of the lamp's health and the board's numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub health: Option<Kept<Health>>,
    pub board: Option<Kept<BoardState>>,
    /// What was there and could not be made sense of, in words.
    pub troubles: Vec<String>,
}

fn kept<T: DeserializeOwned>(
    bus: &Bus,
    topic: &str,
    check: impl Fn(&T) -> Result<(), Invalid>,
    troubles: &mut Vec<String>,
) -> Option<Kept<T>> {
    let message = match bus.latest(topic) {
        Ok(Some(message)) => message,
        Ok(None) => return None,
        Err(error) => {
            troubles.push(format!("{topic} cannot be read: {error}"));
            return None;
        }
    };
    // Nobody has renewed it within its time: nobody is writing it.
    if message.expired {
        return None;
    }
    let value = match message.parse::<T>() {
        Ok(value) => value,
        Err(error) => {
            troubles.push(format!("{topic} holds something else: {error}"));
            return None;
        }
    };
    if let Err(error) = check(&value) {
        troubles.push(format!("{topic} holds what cannot be: {error}"));
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    Some(Kept {
        value,
        age: Duration::from_millis(now.saturating_sub(message.header.at_unix_ms)),
    })
}

/// Read the two kept values from `bus`.
pub fn look(bus: &Bus) -> Look {
    let mut troubles = Vec::new();
    Look {
        health: kept(bus, Health::TOPIC, Health::check, &mut troubles),
        board: kept(bus, BoardState::TOPIC, BoardState::check, &mut troubles),
        troubles,
    }
}

/// The status `show` ends with: 0 ok, 1 warn, 2 error, and 3 when a part
/// is stale or missing or there is no health to read.
pub fn status(look: &Look) -> u8 {
    match look.health.as_ref().map(|health| health.value.level) {
        Some(Level::Ok) => STATUS_OK,
        Some(Level::Warn) => STATUS_WARN,
        Some(Level::Error) => STATUS_ERROR,
        Some(Level::Stale) | None => STATUS_STALE,
    }
}

/// The two values as one JSON object on one line, each as it was
/// published, and `null` for one that is not there.
pub fn json(look: &Look) -> String {
    #[derive(Serialize)]
    struct Both<'a> {
        health: Option<&'a Health>,
        board: Option<&'a BoardState>,
    }
    serde_json::to_string(&Both {
        health: look.health.as_ref().map(|kept| &kept.value),
        board: look.board.as_ref().map(|kept| &kept.value),
    })
    // Two structures of numbers and text: there is nothing in them that
    // cannot be written.
    .unwrap_or_else(|_| "{\"health\":null,\"board\":null}".into())
}

/// Text from the bus as it is safe to print: anyone can write to the bus,
/// and a terminal obeys what it is sent.
fn plain(text: &str) -> String {
    text.chars()
        .map(|letter| if letter.is_control() { ' ' } else { letter })
        .collect()
}

/// A length of time in its two largest units: `2 d 10 h`, `3 h 12 min`,
/// `4 min 10 s`, `12 s`.
pub fn span(seconds: u64) -> String {
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3_600, seconds / 60);
    if days > 0 {
        format!("{days} d {} h", hours % 24)
    } else if hours > 0 {
        format!("{hours} h {} min", minutes % 60)
    } else if minutes > 0 {
        format!("{minutes} min {} s", seconds % 60)
    } else {
        format!("{seconds} s")
    }
}

/// How long ago: to a tenth of a second while that means something.
fn ago(age: Duration) -> String {
    if age < Duration::from_secs(100) {
        format!("{:.1} s ago", age.as_secs_f64())
    } else {
        format!("{} ago", span(age.as_secs()))
    }
}

fn percent(fraction: f32) -> String {
    format!("{:.0}%", fraction * 100.0)
}

/// The board's numbers on one line.
fn board_line(board: &BoardState) -> String {
    let mut line = String::from("board");
    // The hottest sensor; of two as hot, the first by name.
    let hottest = board
        .temperature_c
        .iter()
        .reduce(|hottest, zone| if zone.1 > hottest.1 { zone } else { hottest });
    if let Some((zone, celsius)) = hottest {
        let _ = write!(line, "  hottest {celsius:.0} °C ({})", plain(zone));
    }
    let _ = write!(
        line,
        "  load {:.2}  memory {} used  disk {} free  up {}",
        board.cpu_load,
        percent(board.memory_used),
        percent(board.disk_free),
        span(board.uptime_s)
    );
    if let Some(dbm) = board.wifi_dbm {
        let _ = write!(line, "  wifi {dbm} dBm");
    }
    match board.clock_synced {
        Some(true) => line.push_str("  clock set"),
        Some(false) => line.push_str("  clock not set"),
        None => {}
    }
    line
}

/// What a person is shown. `root` is where the bus was looked for, said
/// when nothing was found there.
pub fn text(look: &Look, root: &Path) -> String {
    let mut out = String::new();
    match &look.health {
        None => {
            let _ = writeln!(out, "{NOBODY}");
            let _ = writeln!(out, "Looked in {}.", root.display());
        }
        Some(Kept { value: health, .. }) => {
            let message = plain(&health.message);
            let _ = match health.level {
                Level::Ok => writeln!(out, "The lamp is well."),
                Level::Warn => writeln!(out, "The lamp has a warning: {message}"),
                Level::Error => writeln!(out, "The lamp is not well: {message}"),
                Level::Stale => writeln!(out, "Not all of the lamp is reporting: {message}"),
            };
            if !health.items.is_empty() || !health.missing.is_empty() {
                out.push('\n');
            }
            let wide = |texts: &mut dyn Iterator<Item = &str>| {
                texts.map(|text| text.chars().count()).max().unwrap_or(0)
            };
            let names = wide(&mut health.items.iter().map(|item| item.name.as_str()));
            let messages = wide(&mut health.items.iter().map(|item| item.message.as_str()));
            for item in &health.items {
                let _ = writeln!(
                    out,
                    "  {:<5}  {:<names$}  {:<messages$}  {}",
                    item.level.as_str(),
                    plain(&item.name),
                    plain(&item.message),
                    ago(Duration::from_millis(item.age_ms)),
                );
            }
            if !health.missing.is_empty() {
                let _ = writeln!(
                    out,
                    "  never reported: {}",
                    plain(&health.missing.join(", "))
                );
            }
        }
    }
    if look.health.is_some() || look.board.is_some() {
        out.push('\n');
    }
    match &look.board {
        Some(board) => {
            let _ = writeln!(out, "{}", board_line(&board.value));
        }
        None if look.health.is_some() => {
            let _ = writeln!(out, "board  no numbers: nobody is publishing board/state");
        }
        None => {}
    }
    // How old the picture is: the ages above are as they were then.
    match (&look.health, &look.board) {
        (Some(health), Some(board)) => {
            let _ = writeln!(
                out,
                "written {}; the board's numbers {}",
                ago(health.age),
                ago(board.age)
            );
        }
        (Some(kept), None) => {
            let _ = writeln!(out, "written {}", ago(kept.age));
        }
        (None, Some(kept)) => {
            let _ = writeln!(out, "the board's numbers written {}", ago(kept.age));
        }
        (None, None) => {}
    }
    // The caller ends the last line.
    out.truncate(out.trim_end().len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_msgs::HealthItem;

    fn item(level: Level, name: &str, message: &str, age_ms: u64) -> HealthItem {
        HealthItem {
            name: name.into(),
            level,
            message: message.into(),
            age_ms,
        }
    }

    fn health(level: Level, message: &str, items: Vec<HealthItem>, missing: &[&str]) -> Health {
        let health = Health {
            level,
            message: message.into(),
            items,
            missing: missing.iter().map(|name| (*name).to_owned()).collect(),
        };
        assert_eq!(health.check(), Ok(()));
        health
    }

    fn board() -> BoardState {
        BoardState {
            temperature_c: [
                ("cpul_thermal_zone", 64.5),
                ("cpub_thermal_zone", 66.0),
                ("skin_zone", 37.0),
            ]
            .into_iter()
            .map(|(zone, celsius)| (zone.to_owned(), celsius))
            .collect(),
            cpu_load: 0.31,
            memory_used: 0.44,
            disk_free: 0.13,
            uptime_s: 208_800,
            wifi_dbm: None,
            clock_synced: None,
        }
    }

    fn seen(health: Option<Health>, board: Option<BoardState>) -> Look {
        Look {
            health: health.map(|value| Kept {
                value,
                age: Duration::from_millis(4_310),
            }),
            board: board.map(|value| Kept {
                value,
                age: Duration::from_millis(1_240),
            }),
            troubles: Vec::new(),
        }
    }

    const ROOT: &str = "/run/lampos-bus";

    #[test]
    fn a_lamp_with_troubles_is_shown_worst_first_with_the_board_below() {
        let look = seen(
            Some(health(
                Level::Error,
                "voice: the link to the service is down",
                vec![
                    item(
                        Level::Error,
                        "voice",
                        "the link to the service is down",
                        300,
                    ),
                    item(Level::Warn, "body", "yaw is hot", 400),
                    item(Level::Stale, "eyes", "not heard from for 7 s", 7_300),
                    item(Level::Ok, "memory", "", 600),
                ],
                &["actions", "ring"],
            )),
            Some(board()),
        );
        assert_eq!(
            text(&look, Path::new(ROOT)),
            "The lamp is not well: voice: the link to the service is down\n\
             \n\
             \x20 error  voice   the link to the service is down  0.3 s ago\n\
             \x20 warn   body    yaw is hot                       0.4 s ago\n\
             \x20 stale  eyes    not heard from for 7 s           7.3 s ago\n\
             \x20 ok     memory                                   0.6 s ago\n\
             \x20 never reported: actions, ring\n\
             \n\
             board  hottest 66 °C (cpub_thermal_zone)  load 0.31  memory 44% used  disk 13% free  up 2 d 10 h\n\
             written 4.3 s ago; the board's numbers 1.2 s ago"
        );
        assert_eq!(status(&look), STATUS_ERROR);
    }

    #[test]
    fn each_level_has_its_first_line_and_its_status() {
        for (level, message, first, ends) in [
            (Level::Ok, "", "The lamp is well.", 0),
            (
                Level::Warn,
                "body: yaw is hot",
                "The lamp has a warning: body: yaw is hot",
                1,
            ),
            (
                Level::Error,
                "body: yaw is hot",
                "The lamp is not well: body: yaw is hot",
                2,
            ),
            (
                Level::Stale,
                "body: yaw is hot",
                "Not all of the lamp is reporting: body: yaw is hot",
                3,
            ),
        ] {
            let look = seen(
                Some(health(
                    level,
                    message,
                    vec![item(level, "body", "yaw is hot", 100)],
                    &[],
                )),
                None,
            );
            let shown = text(&look, Path::new(ROOT));
            assert_eq!(shown.lines().next(), Some(first));
            assert_eq!(status(&look), ends, "{level:?}");
            // No numbers of the board: said, and the lamp's health stands.
            assert!(
                shown.ends_with(
                    "\n\nboard  no numbers: nobody is publishing board/state\nwritten 4.3 s ago"
                ),
                "{shown}"
            );
        }
        // Only a missing part.
        let look = seen(
            Some(health(
                Level::Stale,
                "actions: has never reported",
                vec![item(Level::Ok, "board", "", 100)],
                &["actions"],
            )),
            None,
        );
        assert_eq!(status(&look), STATUS_STALE);
        assert!(
            text(&look, Path::new(ROOT)).starts_with(
                "Not all of the lamp is reporting: actions: has never reported\n\n\
                 \x20 ok     board    0.1 s ago\n\
                 \x20 never reported: actions\n"
            ),
            "{}",
            text(&look, Path::new(ROOT))
        );
        // A lamp with no parts at all is well, and says so in one line.
        let look = seen(Some(health(Level::Ok, "", Vec::new(), &[])), None);
        assert!(text(&look, Path::new(ROOT)).starts_with("The lamp is well.\n\nboard  no numbers"));
    }

    #[test]
    fn with_no_health_it_says_that_nobody_is_reporting_and_ends_with_3() {
        let look = seen(None, None);
        assert_eq!(
            text(&look, Path::new("/tmp/elsewhere")),
            "Nobody is reporting the lamp's health: lamp-health serve is not running, \
             or the bus is somewhere else.\nLooked in /tmp/elsewhere."
        );
        assert_eq!(status(&look), STATUS_STALE);
        assert_eq!(json(&look), "{\"health\":null,\"board\":null}");
        // The board's numbers alone are shown, and are not the lamp's
        // health.
        let look = seen(None, Some(board()));
        let shown = text(&look, Path::new(ROOT));
        assert!(shown.starts_with(NOBODY), "{shown}");
        assert!(
            shown.ends_with(
                "Looked in /run/lampos-bus.\n\nboard  hottest 66 °C (cpub_thermal_zone)  \
                 load 0.31  memory 44% used  disk 13% free  up 2 d 10 h\n\
                 the board's numbers written 1.2 s ago"
            ),
            "{shown}"
        );
        assert_eq!(status(&look), STATUS_STALE);
    }

    #[test]
    fn the_boards_line_has_what_the_board_could_be_asked() {
        let mut state = board();
        state.wifi_dbm = Some(-56);
        state.clock_synced = Some(true);
        assert_eq!(
            board_line(&state),
            "board  hottest 66 °C (cpub_thermal_zone)  load 0.31  memory 44% used  disk 13% free  \
             up 2 d 10 h  wifi -56 dBm  clock set"
        );
        state.clock_synced = Some(false);
        state.wifi_dbm = None;
        state.temperature_c.clear();
        state.uptime_s = 59;
        assert_eq!(
            board_line(&state),
            "board  load 0.31  memory 44% used  disk 13% free  up 59 s  clock not set"
        );
    }

    #[test]
    fn json_is_the_two_values_as_they_were_published_on_one_line() {
        let well = health(Level::Ok, "", vec![item(Level::Ok, "voice", "", 300)], &[]);
        let look = seen(Some(well.clone()), Some(board()));
        let printed = json(&look);
        assert!(!printed.contains('\n'));
        assert!(
            printed.starts_with("{\"health\":{\"level\":\"ok\","),
            "{printed}"
        );
        let read: serde_json::Value = serde_json::from_str(&printed).unwrap();
        assert_eq!(read.as_object().unwrap().len(), 2);
        assert_eq!(
            serde_json::from_value::<Health>(read["health"].clone()).unwrap(),
            well
        );
        assert_eq!(
            serde_json::from_value::<BoardState>(read["board"].clone()).unwrap(),
            board()
        );
        let look = seen(Some(well), None);
        assert!(json(&look).ends_with(",\"board\":null}"));
    }

    #[test]
    fn what_came_from_the_bus_is_printed_on_its_own_line_whatever_it_holds() {
        let look = seen(
            Some(health(
                Level::Warn,
                "bo\u{1b}[2Jdy: yaw\nis hot",
                vec![item(Level::Warn, "bo\u{1b}[2Jdy", "yaw\nis hot", 100)],
                &["act\rions"],
            )),
            None,
        );
        let shown = text(&look, Path::new(ROOT));
        assert!(!shown.contains(['\u{1b}', '\r']), "{shown:?}");
        assert_eq!(shown.lines().count(), 7, "{shown:?}");
        assert!(shown.contains("  warn   bo [2Jdy  yaw is hot  0.1 s ago\n"));
    }

    #[test]
    fn a_time_is_said_in_its_two_largest_units() {
        for (seconds, said) in [
            (0, "0 s"),
            (59, "59 s"),
            (60, "1 min 0 s"),
            (250, "4 min 10 s"),
            (3_599, "59 min 59 s"),
            (3_600, "1 h 0 min"),
            (11_520, "3 h 12 min"),
            (86_399, "23 h 59 min"),
            (86_400, "1 d 0 h"),
            (208_800, "2 d 10 h"),
        ] {
            assert_eq!(span(seconds), said);
        }
        assert_eq!(ago(Duration::from_millis(349)), "0.3 s ago");
        assert_eq!(ago(Duration::from_millis(99_900)), "99.9 s ago");
        assert_eq!(ago(Duration::from_secs(100)), "1 min 40 s ago");
    }
}
