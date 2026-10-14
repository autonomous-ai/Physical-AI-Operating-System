//! What time it is where the lamp stands, in words, for a new session's
//! instruction.
//!
//! The model has no clock. On lamp-4ace on 2026-10-13 it told two people who
//! asked how it was, at eleven in the morning, that it was "enjoying the quiet
//! evening on the desk". The zone comes from the system's own compiled zone
//! file (`/etc/localtime`, RFC 8536); nothing is spawned and no zone database
//! is carried here.
//!
//! A zone with summer time whose file stops listing its changes and leaves
//! them to a rule is not worked out: for a moment past the listed changes of
//! such a zone nothing is told, which is how the lamp was before.

const HEADER: usize = 44;

struct Block<'a> {
    wide: bool,
    times: &'a [u8],
    kinds: &'a [u8],
    infos: &'a [u8],
    end: usize,
}

fn block(file: &[u8], at: usize, wide: bool) -> Option<Block<'_>> {
    let head = file.get(at..at.checked_add(HEADER)?)?;
    if &head[..4] != b"TZif" {
        return None;
    }
    let count = |index: usize| {
        let bytes = head[20 + 4 * index..24 + 4 * index].try_into().ok()?;
        let count = u32::from_be_bytes(bytes) as usize;
        (count <= 100_000).then_some(count)
    };
    let (utc, standard, leaps, times, kinds, letters) = (
        count(0)?,
        count(1)?,
        count(2)?,
        count(3)?,
        count(4)?,
        count(5)?,
    );
    let width = if wide { 8 } else { 4 };
    let times_at = at + HEADER;
    let kinds_at = times_at + times * width;
    let infos_at = kinds_at + times;
    let end = infos_at + kinds * 6 + letters + leaps * (width + 4) + standard + utc;
    if end > file.len() {
        return None;
    }
    Some(Block {
        wide,
        times: &file[times_at..kinds_at],
        kinds: &file[kinds_at..infos_at],
        infos: &file[infos_at..infos_at + kinds * 6],
        end,
    })
}

impl Block<'_> {
    fn time(&self, index: usize) -> Option<i64> {
        if self.wide {
            let bytes = self.times.get(index * 8..index * 8 + 8)?;
            Some(i64::from_be_bytes(bytes.try_into().ok()?))
        } else {
            let bytes = self.times.get(index * 4..index * 4 + 4)?;
            Some(i64::from(i32::from_be_bytes(bytes.try_into().ok()?)))
        }
    }
}

/// Seconds east of UTC in the zone of this compiled zone file at `now`
/// (seconds since 1970 UTC).
pub fn offset_at(file: &[u8], now: i64) -> Option<i32> {
    let first = block(file, 0, false)?;
    let (table, rule) = if file.get(4).is_some_and(|version| *version >= b'2') {
        let second = block(file, first.end, true)?;
        let rule = file.get(second.end..)?;
        (second, rule)
    } else {
        (first, &[][..])
    };
    let mut kind = 0usize;
    let mut listed = false;
    for index in 0..table.kinds.len() {
        if table.time(index)? > now {
            listed = true;
            break;
        }
        kind = usize::from(table.kinds[index]);
    }
    // Past the listed changes a rule with a comma in it is summer time.
    if !listed && rule.contains(&b',') {
        return None;
    }
    let info = table.infos.get(kind * 6..kind * 6 + 4)?;
    let offset = i32::from_be_bytes(info.try_into().ok()?);
    (-86_400 < offset && offset < 86_400).then_some(offset)
}

/// Year, month and day of a count of days since 1970-01-01.
pub(crate) fn civil(days: i64) -> (i64, usize, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month as usize, day)
}

const DAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The moment `now` in the zone of this compiled zone file, in words: the
/// weekday, the date, the time on a twelve-hour clock and the part of the
/// day.
fn words(
    file: &[u8],
    now: i64,
) -> Option<(&'static str, i64, &'static str, i64, i64, i64, &'static str)> {
    let local = now.checked_add(i64::from(offset_at(file, now)?))?;
    let (days, seconds) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let (year, month, day) = civil(days);
    let (hour, minute) = (seconds / 3_600, seconds % 3_600 / 60);
    let part = match hour {
        5..=11 => "in the morning",
        12..=16 => "in the afternoon",
        17..=20 => "in the evening",
        _ => "at night",
    };
    let weekday = DAYS[days.rem_euclid(7) as usize];
    let month = MONTHS[month - 1];
    let hour = if hour % 12 == 0 { 12 } else { hour % 12 };
    Some((weekday, day, month, year, hour, minute, part))
}

/// What a session opened at `now` is told about the time, in the zone of
/// this compiled zone file.
pub fn told(file: &[u8], now: i64) -> Option<String> {
    let (weekday, day, month, year, hour, minute, part) = words(file, now)?;
    Some(format!(
        "\n\nThe time\nIt is {weekday} {day} {month} {year}, around {hour}:{minute:02} {part}. You have no clock, so this may be up to an hour old: if someone asks the time, give it roughly."
    ))
}

/// The same for a session that may ask the time ([`crate::tools::THE_TIME`]):
/// what it is told on opening is for the part of the day, not for telling.
pub fn told_with_clock(file: &[u8], now: i64) -> Option<String> {
    let (weekday, day, month, year, hour, minute, part) = words(file, now)?;
    Some(format!(
        "\n\nThe time\nYou woke on {weekday} {day} {month} {year}, around {hour}:{minute:02} {part}, and it may be much later now. If someone asks the time or the date, or how long until something, call the_time first and tell them what it gives."
    ))
}

/// The time at `now`, exactly, for the model that asked
/// ([`crate::tools::THE_TIME`]), with the words a person would say it in:
/// on lamp-4ace on 2026-10-14 the model, given "6:23 in the evening", said
/// "twenty-three past in the evening" and lost the hour.
pub fn said(file: &[u8], now: i64) -> Option<String> {
    let (weekday, day, month, year, hour, minute, part) = words(file, now)?;
    Some(format!(
        "It is {hour}:{minute:02} {part}, {weekday} {day} {month} {year}. Said aloud: {} {part}.",
        aloud(hour, minute)
    ))
}

const NUMBERS: [&str; 31] = [
    "",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "quarter",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "twenty-one",
    "twenty-two",
    "twenty-three",
    "twenty-four",
    "twenty-five",
    "twenty-six",
    "twenty-seven",
    "twenty-eight",
    "twenty-nine",
    "half",
];

/// A time on a twelve-hour clock as it is said: "six o'clock", "twenty-three
/// past six", "quarter to seven".
fn aloud(hour: i64, minute: i64) -> String {
    let next = hour % 12 + 1;
    match minute {
        0 => format!("{} o'clock", NUMBERS[hour as usize]),
        1..=30 => format!(
            "{} past {}",
            NUMBERS[minute as usize], NUMBERS[hour as usize]
        ),
        _ => format!(
            "{} to {}",
            NUMBERS[(60 - minute) as usize],
            NUMBERS[next as usize]
        ),
    }
}

/// How many seconds east of UTC this machine's local time is now.
pub fn offset_here() -> Option<i32> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let file = std::fs::read("/etc/localtime").ok()?;
    offset_at(&file, i64::try_from(now).ok()?)
}

/// The same for this machine, now.
pub fn told_here(clock: bool) -> Option<String> {
    let (file, now) = here()?;
    if clock {
        told_with_clock(&file, now)
    } else {
        told(&file, now)
    }
}

/// The time now where the lamp stands, exactly, in words.
pub fn now_here() -> Option<String> {
    let (file, now) = here()?;
    said(&file, now)
}

fn here() -> Option<(Vec<u8>, i64)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let file = std::fs::read("/etc/localtime").ok()?;
    Some((file, i64::try_from(now).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zone file: `changes` are (moment, kind), `offsets` the seconds east
    /// of each kind, `rule` the line after the second table.
    fn zone(version: u8, changes: &[(i64, u8)], offsets: &[i32], rule: &str) -> Vec<u8> {
        let table = |wide: bool| {
            let mut bytes = b"TZif".to_vec();
            bytes.push(version);
            bytes.extend([0; 15]);
            for count in [0, 0, 0, changes.len(), offsets.len(), 4] {
                bytes.extend((count as u32).to_be_bytes());
            }
            for (moment, _) in changes {
                if wide {
                    bytes.extend(moment.to_be_bytes());
                } else {
                    bytes.extend((*moment as i32).to_be_bytes());
                }
            }
            bytes.extend(changes.iter().map(|(_, kind)| *kind));
            for offset in offsets {
                bytes.extend(offset.to_be_bytes());
                bytes.extend([0, 0]);
            }
            bytes.extend(b"LMT\0");
            bytes
        };
        let mut file = table(false);
        if version >= b'2' {
            file.extend(table(true));
            file.extend(format!("\n{rule}\n").bytes());
        }
        file
    }

    // 2026-10-13 04:14:00 UTC, a Tuesday.
    const NOW: i64 = 1_791_864_840;

    #[test]
    fn it_reads_the_offset_now_from_a_zone_file_of_either_kind() {
        // Ho Chi Minh City as the lamp has it: its last change in 1975.
        let saigon = zone(
            b'2',
            &[(-2_004_073_600, 1), (171_820_800, 2)],
            &[25_590, 28_800, 25_200],
            "<+07>-7",
        );
        assert_eq!(offset_at(&saigon, NOW), Some(25_200));
        assert_eq!(offset_at(&saigon, 0), Some(28_800));
        assert_eq!(offset_at(&saigon, -3_000_000_000), Some(25_590));
        // An old file with one table and no rule, and one with no change at all.
        assert_eq!(
            offset_at(&zone(0, &[(100, 1)], &[0, 3_600], ""), NOW),
            Some(3_600)
        );
        assert_eq!(offset_at(&zone(b'2', &[], &[0], "UTC0"), NOW), Some(0));
        // Summer time: told from the listed changes, not from the rule.
        let listed = zone(
            b'3',
            &[(NOW - 10, 1), (NOW + 10, 0)],
            &[-18_000, -14_400],
            "EST5EDT,M3.2.0,M11.1.0",
        );
        assert_eq!(offset_at(&listed, NOW), Some(-14_400));
        assert_eq!(offset_at(&listed, NOW + 10), None);
        // Not a zone file, a cut one, an absurd one.
        assert_eq!(offset_at(b"", NOW), None);
        assert_eq!(offset_at(&saigon[..60], NOW), None);
        assert_eq!(offset_at(&zone(0, &[], &[400_000], ""), NOW), None);
        let mut wrong = saigon.clone();
        wrong[0] = b'X';
        assert_eq!(offset_at(&wrong, NOW), None);
    }

    #[test]
    fn it_tells_the_day_and_the_part_of_it() {
        let zone_at = |offset: i32| zone(b'2', &[], &[offset], "X");
        let said = |offset: i32, now: i64| told(&zone_at(offset), now).unwrap();
        assert!(
            said(25_200, NOW)
                .contains("It is Tuesday 13 October 2026, around 11:14 in the morning."),
            "{}",
            said(25_200, NOW)
        );
        assert!(said(0, NOW).contains("Tuesday 13 October 2026, around 4:14 at night."));
        // The day turns with the zone, and so do the month and the year.
        assert!(said(-36_000, NOW).contains("Monday 12 October 2026, around 6:14 in the evening."));
        assert!(said(0, 0).contains("Thursday 1 January 1970, around 12:00 at night."));
        assert!(said(-3_600, 0).contains("Wednesday 31 December 1969, around 11:00 at night."));
        assert!(
            said(0, 951_782_400 + 12 * 3_600)
                .contains("Tuesday 29 February 2000, around 12:00 in the afternoon.")
        );
        // No offset to be had, nothing told.
        assert_eq!(told(b"TZif", NOW), None);
    }

    #[test]
    fn the_model_that_asks_is_given_the_minute_and_no_hedge() {
        let saigon = zone(b'2', &[], &[25_200], "<+07>-7");
        assert_eq!(
            said(&saigon, NOW).unwrap(),
            "It is 11:14 in the morning, Tuesday 13 October 2026. Said aloud: fourteen past eleven in the morning."
        );
        // Thirty-three minutes on, as the session opened at 10:48 on
        // 2026-10-14 was asked at 11:17: the minute moves with the clock.
        assert_eq!(
            said(&saigon, NOW + 33 * 60).unwrap(),
            "It is 11:47 in the morning, Tuesday 13 October 2026. Said aloud: thirteen to twelve in the morning."
        );
        assert_eq!(said(b"TZif", NOW), None);
        // Told on opening when it may ask: the part of the day, and to ask.
        let opening = told_with_clock(&saigon, NOW).unwrap();
        assert!(
            opening.contains("You woke on Tuesday 13 October 2026, around 11:14 in the morning")
        );
        assert!(opening.contains("call the_time first") && !opening.contains("roughly"));
    }

    #[test]
    fn a_time_is_said_as_a_person_says_it_with_its_hour() {
        assert_eq!(aloud(6, 23), "twenty-three past six");
        assert_eq!(aloud(6, 0), "six o'clock");
        assert_eq!(aloud(6, 15), "quarter past six");
        assert_eq!(aloud(6, 30), "half past six");
        assert_eq!(aloud(6, 45), "quarter to seven");
        assert_eq!(aloud(12, 59), "one to one");
        assert_eq!(aloud(11, 31), "twenty-nine to twelve");
        assert_eq!(aloud(12, 1), "one past twelve");
        for hour in 1..=12 {
            for minute in 0..60 {
                let said = aloud(hour, minute);
                assert!(
                    !said.starts_with(' ') && !said.contains("  ") && !said.ends_with(' '),
                    "{said}"
                );
            }
        }
    }
}
