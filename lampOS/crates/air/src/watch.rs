//! The reading loop, apart from the I2C adapter and the clock, so it can be
//! run against a sensor that never answers. See `docs/air.md`.
//!
//! Every look for the sensor is an I2C transaction, and the kernel logs each
//! one nobody acknowledges ("ACK not received"). A lamp without the sensor
//! (lamp-cb94) was looked at five times a second, about 300 kernel lines a
//! minute, so its journal kept only about 90 minutes. Now a sensor that does
//! not answer, or answers its name but gives no reading, is looked for
//! [`QUICK_LOOKS`] times at the reading pace, then once every [`LOOK_EVERY`],
//! and the journal hears of it twice. A bus that cannot be opened makes no
//! transaction, so it is looked at again at the reading pace.
use lamp_msgs::{AirState, Diagnostic, Level};
use std::time::Duration;

/// A reading every five seconds, as the sensor makes one a second.
pub const EVERY: Duration = Duration::from_secs(5);
/// Looks at the reading pace before a missing sensor counts as absent: a
/// sensor that is slow to wake after power-up is found at once.
pub const QUICK_LOOKS: u32 = 3;
/// How often an absent sensor is looked for again.
pub const LOOK_EVERY: Duration = Duration::from_secs(600);
/// The loop's own pace; it touches the sensor only when a look or a reading
/// is due.
pub const TICK: Duration = Duration::from_millis(200);
/// The health report's pace.
const REPORT_EVERY: Duration = Duration::from_millis(950);

/// What the report says while a sensor nobody has heard from is looked
/// for, in its first [`QUICK_LOOKS`]. The self-test reads these words
/// (`lamp-selftest`'s `air_silent`), so they are kept here, once.
pub const LOOKING: &str = "the sensor does not answer";
/// What it says once a sensor that never answered since `lamp-air` started
/// counts as absent: on a lamp without one fitted (lamp-06c1, lamp-cb94).
pub const ABSENT: &str = "no air sensor answers";
/// What it says instead once the sensor has answered since start, by name
/// or with a reading that failed its CRC, and then stopped giving readings:
/// a fitted sensor that is faulty or unseated (lamp-4ace), never taken for
/// an absent one (the Seventh and the reviewer on #1324).
pub const NO_READING: &str = "the air sensor answered but gives no reading";
/// What the health report says, at `ok`, for a sensor counted [`ABSENT`]:
/// the sensor is optional, and a part a lamp does not have is no warning,
/// so `lamp-health` says the lamp is well (the first builder on #705, 17 Oct
/// 15:36). The journal keeps the [`ABSENT`] line with the bus and error. A
/// fitted sensor that fails ([`NO_READING`], or a reading that failed)
/// stays a warning.
pub const NOT_FITTED: &str = "not fitted: no air sensor has answered since start";

/// Why a look found no sensor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// The bus could not be opened: no transaction was made, so the kernel
    /// logged nothing.
    NoBus(String),
    /// Nobody acknowledged, or the answer was wrong: a transaction made.
    NoAnswer(String),
}

/// The sensor, as the loop uses it.
pub trait Sensor {
    /// Opens the bus and asks the sensor's name.
    fn find(&mut self) -> Result<String, Missing>;
    fn ready(&mut self) -> Result<bool, String>;
    fn read(&mut self) -> Result<AirState, String>;
    /// Starts its measurement, as after a power cut.
    fn start(&mut self) -> Result<(), String>;
    /// Lets go of a sensor that stopped answering.
    fn lose(&mut self);
}

/// Time since the loop began, and waiting.
pub trait Clock {
    fn now(&mut self) -> Duration;
    fn sleep(&mut self, duration: Duration);
}

/// Where the loop says things: the air on `air/state`, its health on
/// `diagnostics`, and a line for the journal.
pub trait Say {
    fn air(&mut self, air: &AirState);
    fn health(&mut self, report: &Diagnostic);
    fn log(&mut self, line: &str);
}

/// The health report: ok, or a warning with why, and the last air read.
pub fn report(why: Option<&str>, last: Option<&AirState>) -> Diagnostic {
    let mut report = match why {
        Some(why) if why.starts_with(ABSENT) => Diagnostic::new(Level::Ok, "air", NOT_FITTED),
        Some(why) => Diagnostic::new(Level::Warn, "air", why),
        None => Diagnostic::new(Level::Ok, "air", ""),
    };
    if let Some(air) = last {
        report = report
            .with("feel", format!("{:?}", air.feel).to_lowercase())
            .with(
                "co2_ppm",
                air.co2_ppm.map_or("unknown".into(), |v| v.to_string()),
            )
            .with(
                "pm2_5",
                air.pm2_5.map_or("unknown".into(), |v| format!("{v:.1}")),
            );
    }
    report
}

/// A look or a reading that gave no reading, the `misses`th in a row: when
/// to look next, what the report says, and whether the journal hears it
/// (the first in a row, and when the sensor counts as absent).
fn missed(
    misses: &mut u32,
    now: Duration,
    what: &str,
    error: &str,
    bus_number: u32,
    ever_answered: bool,
) -> (Duration, String, bool) {
    *misses += 1;
    if *misses < QUICK_LOOKS {
        (now + EVERY, format!("{what}: {error}"), *misses == 1)
    } else {
        let gone = if ever_answered { NO_READING } else { ABSENT };
        (
            now + LOOK_EVERY,
            format!(
                "{gone} on I2C bus {bus_number} ({error}); looked for again every {} min",
                LOOK_EVERY.as_secs() / 60
            ),
            *misses == QUICK_LOOKS,
        )
    }
}

/// Reads the sensor on I2C bus `bus_number` until `until` on `clock`.
pub fn run(
    sensor: &mut impl Sensor,
    clock: &mut impl Clock,
    say: &mut impl Say,
    bus_number: u32,
    until: Duration,
) {
    run_answered(sensor, clock, say, bus_number, until, false);
}

/// [`run`], told whether the sensor already answered this boot, before
/// this `lamp-air` started: a restart (a crash, an update, a replugged
/// sensor) must not take a fitted sensor that only refuses for an absent
/// one (the reviewer on #1324). `main` knows it by a reading already kept
/// on `air/state`, which lives under `/run` and so clears at each boot.
pub fn run_answered(
    sensor: &mut impl Sensor,
    clock: &mut impl Clock,
    say: &mut impl Say,
    bus_number: u32,
    until: Duration,
    answered_before: bool,
) {
    let mut found = false;
    // Whether the sensor has answered since start: by name, or with words
    // that failed their CRC (something on the bus spoke). Never cleared.
    let mut ever_answered = answered_before;
    // Looks and readings in a row that gave no reading, counted out only by a
    // good reading, so a sensor that answers its name and then fails backs
    // off as one that never answers (eighth and the reviewer on #1163).
    let mut misses = 0u32;
    let mut look_at = Duration::ZERO;
    let mut read_at = Duration::ZERO;
    // Whether the journal has heard the sensor answer since it last did not.
    let mut answered = false;
    let mut reported_at: Option<Duration> = None;
    let mut why: Option<String> = None;
    let mut not_ready = 0u32;
    let mut last: Option<AirState> = None;
    loop {
        let now = clock.now();
        if now >= until {
            return;
        }
        if !found && now >= look_at {
            match sensor.find() {
                Ok(name) => {
                    ever_answered = true;
                    if !answered {
                        say.log(&format!("the sensor answers: {name}"));
                        answered = true;
                    }
                    found = true;
                    // Found again after a failed reading: the reading pace
                    // holds. Found the first time: read at once.
                    read_at = read_at.max(now);
                }
                Err(Missing::NoBus(error)) => {
                    // No transaction, nothing for the kernel to log: looked at
                    // again at the reading pace, so a bus that comes up later
                    // is used at once.
                    let line = format!(
                        "I2C bus {bus_number} cannot be opened ({error}); looked at again every {} s",
                        EVERY.as_secs()
                    );
                    if why.as_deref() != Some(line.as_str()) {
                        say.log(&line);
                    }
                    why = Some(line);
                    answered = false;
                    look_at = now + EVERY;
                }
                Err(Missing::NoAnswer(error)) => {
                    ever_answered |= error.contains("CRC");
                    let (at, line, tell) =
                        missed(&mut misses, now, LOOKING, &error, bus_number, ever_answered);
                    if tell {
                        say.log(&line);
                    }
                    why = Some(line);
                    answered = false;
                    look_at = at;
                }
            }
        }
        if found && now >= read_at {
            read_at = now + EVERY;
            match sensor.ready() {
                Ok(true) => match sensor.read() {
                    Ok(air) => {
                        misses = 0;
                        not_ready = 0;
                        why = None;
                        say.air(&air);
                        if last.as_ref().is_none_or(|l| l.feel != air.feel) {
                            say.log(&format!(
                                "the air feels {:?}: co2 {:?} ppm, pm2.5 {:?}",
                                air.feel, air.co2_ppm, air.pm2_5
                            ));
                        }
                        last = Some(air);
                    }
                    Err(error) => why = Some(format!("a reading failed: {error}")),
                },
                Ok(false) => {
                    not_ready += 1;
                    // Not measuring (after a power cut): start it, once in a while.
                    if not_ready % 6 == 3 {
                        say.log("no new reading for 15 s: starting the sensor's measurement");
                        if let Err(error) = sensor.start() {
                            why = Some(error);
                        }
                    }
                }
                Err(error) => {
                    sensor.lose();
                    found = false;
                    let (at, line, tell) = missed(
                        &mut misses,
                        now,
                        "the sensor stopped answering",
                        &error,
                        bus_number,
                        ever_answered,
                    );
                    if tell {
                        say.log(&line);
                    }
                    why = Some(line);
                    look_at = at;
                }
            }
        }
        if reported_at.is_none_or(|at| now.saturating_sub(at) >= REPORT_EVERY) {
            reported_at = Some(now);
            say.health(&report(why.as_deref(), last.as_ref()));
        }
        clock.sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_msgs::AirFeel;
    use std::{cell::Cell, rc::Rc};

    /// The time every stand-in reads: it moves only when the loop sleeps.
    type Time = Rc<Cell<Duration>>;

    struct Stepped(Time);
    impl Clock for Stepped {
        fn now(&mut self) -> Duration {
            self.0.get()
        }
        fn sleep(&mut self, duration: Duration) {
            self.0.set(self.0.get() + duration);
        }
    }

    const NAK: &str = "Remote I/O error (os error 121)";

    /// An I2C bus and the sensor on it. The bus can be opened from
    /// `bus_from`; the sensor acknowledges from `answers_from`, if ever,
    /// until `stops_at`; with `reads_fail` it answers its name and fails
    /// every reading (a half-seated plug); it has a new reading from
    /// `measuring_from`. Every transaction is counted, each one nobody
    /// acknowledged (the kernel logs those), and each start.
    #[derive(Default)]
    struct Bus {
        time: Time,
        bus_from: Duration,
        answers_from: Option<Duration>,
        stops_at: Option<Duration>,
        reads_fail: bool,
        /// Every look answers, but with words that fail their CRC.
        garbled: bool,
        measuring_from: Duration,
        transactions: u32,
        not_acknowledged: u32,
        starts: u32,
    }
    impl Bus {
        fn answers(&mut self) -> Result<(), String> {
            self.transactions += 1;
            let now = self.time.get();
            if self.answers_from.is_some_and(|from| now >= from)
                && self.stops_at.is_none_or(|at| now < at)
            {
                Ok(())
            } else {
                self.not_acknowledged += 1;
                Err(NAK.into())
            }
        }
    }
    impl Sensor for Bus {
        fn find(&mut self) -> Result<String, Missing> {
            if self.time.get() < self.bus_from {
                return Err(Missing::NoBus(
                    "No such file or directory (os error 2)".into(),
                ));
            }
            if self.garbled {
                self.transactions += 1;
                return Err(Missing::NoAnswer("word 0 fails its CRC".into()));
            }
            self.answers()
                .map(|()| "SEN63C".into())
                .map_err(Missing::NoAnswer)
        }
        fn ready(&mut self) -> Result<bool, String> {
            self.answers()?;
            if self.reads_fail {
                return Err("word 0 fails its CRC".into());
            }
            Ok(self.time.get() >= self.measuring_from)
        }
        fn read(&mut self) -> Result<AirState, String> {
            self.answers().map(|()| AirState {
                pm1_0: Some(1.0),
                pm2_5: Some(2.0),
                pm4_0: Some(3.0),
                pm10: Some(4.0),
                humidity: Some(40.0),
                temperature: Some(30.0),
                co2_ppm: Some(600),
                feel: AirFeel::Fresh,
            })
        }
        fn start(&mut self) -> Result<(), String> {
            self.starts += 1;
            self.answers()
        }
        fn lose(&mut self) {}
    }

    struct Said {
        time: Time,
        airs: u32,
        reports: Vec<(Duration, Diagnostic)>,
        lines: Vec<String>,
    }
    impl Say for Said {
        fn air(&mut self, _: &AirState) {
            self.airs += 1;
        }
        fn health(&mut self, report: &Diagnostic) {
            self.reports.push((self.time.get(), report.clone()));
        }
        fn log(&mut self, line: &str) {
            self.lines.push(line.to_owned());
        }
    }

    fn at(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    /// The loop for `seconds` on a stepped clock, the bus as `set` makes it.
    fn run_for(seconds: u64, set: impl FnOnce(&mut Bus)) -> (Bus, Said) {
        let time: Time = Rc::default();
        let mut bus = Bus {
            time: Rc::clone(&time),
            ..Bus::default()
        };
        set(&mut bus);
        let mut said = Said {
            time: Rc::clone(&time),
            airs: 0,
            reports: Vec::new(),
            lines: Vec::new(),
        };
        run(
            &mut bus,
            &mut Stepped(Rc::clone(&time)),
            &mut said,
            0,
            at(seconds),
        );
        (bus, said)
    }

    /// Twelve on lamp-cb94: no sensor on the bus. An hour of it is a handful
    /// of transactions, not the 18,000 a look every 200 ms made, and the
    /// journal hears of it twice; the health report still comes each second,
    /// with the error in it.
    #[test]
    fn a_sensor_that_never_answers_is_looked_for_three_times_then_every_ten_minutes() {
        let (bus, said) = run_for(3_600, |_| {});
        // At 0, 5 and 10 s, then 610, 1210, 1810, 2410 and 3010 s.
        assert_eq!((bus.transactions, bus.not_acknowledged), (8, 8));
        assert_eq!(said.airs, 0);
        let absent =
            format!("no air sensor answers on I2C bus 0 ({NAK}); looked for again every 10 min");
        assert_eq!(
            said.lines,
            [format!("the sensor does not answer: {NAK}"), absent.clone()]
        );
        assert!(
            (3_500..=3_700).contains(&said.reports.len()),
            "{}",
            said.reports.len()
        );
        let (when, last) = said.reports.last().unwrap();
        assert!(*when > at(3_598));
        assert_eq!((last.level, last.message.as_str()), (Level::Ok, NOT_FITTED));
    }

    /// Eighth and the reviewer on #1163: a sensor that answers its name and
    /// fails every reading was found again and failed again every tick,
    /// 600 journal lines a minute. It backs off as a missing one does.
    #[test]
    fn a_sensor_that_answers_its_name_but_never_reads_backs_off_too() {
        let (bus, said) = run_for(3_600, |bus| {
            bus.answers_from = Some(Duration::ZERO);
            bus.reads_fail = true;
        });
        // A name and a failed reading at 0, 5 and 10 s, then every 10 min.
        assert_eq!(bus.transactions, 16, "8 looks, each a find and a ready");
        assert_eq!(
            said.lines,
            [
                "the sensor answers: SEN63C".to_owned(),
                "the sensor stopped answering: word 0 fails its CRC".to_owned(),
                "the air sensor answered but gives no reading on I2C bus 0 (word 0 fails its CRC); looked for again every 10 min"
                    .to_owned(),
            ]
        );
        assert_eq!(said.airs, 0);
    }

    /// The reviewer's P2 on #1324: a sensor whose every look comes back
    /// with words that fail their CRC has answered (something on the bus
    /// spoke): it ends as one that gives no reading, a warning, never as
    /// not fitted.
    #[test]
    fn a_sensor_whose_every_look_fails_its_crc_has_answered_and_is_no_reading_not_absent() {
        let (_, said) = run_for(60, |bus| bus.garbled = true);
        let (_, last) = said.reports.last().unwrap();
        assert_eq!(last.level, Level::Warn, "{last:?}");
        assert!(last.message.starts_with(NO_READING), "{last:?}");
    }

    /// The reviewer's P3 on #1324: after a restart this boot, a sensor that
    /// answered before and now only refuses is still no reading, not absent.
    #[test]
    fn a_sensor_that_answered_before_a_restart_is_not_taken_for_absent() {
        let time: Time = Rc::default();
        let mut bus = Bus {
            time: Rc::clone(&time),
            ..Bus::default()
        };
        let mut said = Said {
            time: Rc::clone(&time),
            airs: 0,
            reports: Vec::new(),
            lines: Vec::new(),
        };
        run_answered(
            &mut bus,
            &mut Stepped(Rc::clone(&time)),
            &mut said,
            0,
            at(60),
            true,
        );
        let (_, last) = said.reports.last().unwrap();
        assert_eq!(last.level, Level::Warn, "{last:?}");
        assert!(last.message.starts_with(NO_READING), "{last:?}");
    }

    /// A sensor plugged in, or woken, later is found at the next look and
    /// read every five seconds from then on.
    #[test]
    fn a_sensor_that_answers_later_is_read_from_the_next_look() {
        let (bus, said) = run_for(1_900, |bus| bus.answers_from = Some(at(1_200)));
        // Missed at 0, 5, 10 and 610 s, found at 1210 s.
        assert_eq!(bus.not_acknowledged, 4);
        assert!(
            said.lines
                .contains(&"the sensor answers: SEN63C".to_owned())
        );
        // From 1210 s to 1900 s, a reading every 5 s.
        assert!((137..=139).contains(&said.airs), "{}", said.airs);
        let (_, last) = said.reports.last().unwrap();
        assert_eq!(last.level, Level::Ok);
        assert!(
            last.values
                .iter()
                .any(|value| value.key == "co2_ppm" && value.value == "600"),
            "{last:?}"
        );
    }

    /// A sensor that stops answering is looked for again 5 s later, then as
    /// one never found: no flood after it goes either.
    #[test]
    fn a_sensor_that_stops_answering_is_looked_for_again_then_backs_off() {
        let (bus, said) = run_for(3_600, |bus| {
            bus.answers_from = Some(Duration::ZERO);
            bus.stops_at = Some(at(60));
        });
        // Its failed ready at 60 s, looks at 65 and 70 s, then 670, 1270,
        // 1870, 2470 and 3070 s.
        assert_eq!(bus.not_acknowledged, 8);
        assert_eq!(
            said.lines
                .iter()
                .filter(|line| line.starts_with("the sensor stopped answering"))
                .count(),
            1
        );
        assert_eq!(
            said.lines
                .iter()
                .filter(|line| line.starts_with(NO_READING))
                .count(),
            1
        );
    }

    /// The reviewer on #1163: a bus that cannot be opened makes no
    /// transaction, so it is looked at every 5 s, and used as soon as it is
    /// there; the report and the journal say why, once.
    #[test]
    fn a_bus_that_cannot_be_opened_is_looked_at_every_five_seconds_and_used_once_there() {
        let (bus, said) = run_for(60, |bus| {
            bus.bus_from = at(20);
            bus.answers_from = Some(Duration::ZERO);
        });
        assert_eq!(bus.not_acknowledged, 0);
        let missing = "I2C bus 0 cannot be opened (No such file or directory (os error 2)); looked at again every 5 s";
        assert_eq!(
            said.lines[..2],
            [missing.to_owned(), "the sensor answers: SEN63C".to_owned()]
        );
        assert!(
            said.reports
                .iter()
                .any(|(when, report)| *when < at(20) && report.message == missing)
        );
        // Found at 20 s: read at 20, 25, ..., 55 s.
        assert_eq!(said.airs, 8);
    }

    /// The reviewer on #1163: a sensor that is not measuring, as after a
    /// power cut, is started on the third reading with nothing new (15 s),
    /// and again every 30 s until it measures.
    #[test]
    fn a_sensor_with_nothing_new_is_started_after_fifteen_seconds() {
        let (bus, said) = run_for(60, |bus| {
            bus.answers_from = Some(Duration::ZERO);
            bus.measuring_from = at(50);
        });
        // Not ready at 0, 5, ..., 45 s: the 3rd (10 s) and the 9th (40 s).
        assert_eq!(bus.starts, 2);
        assert_eq!(
            said.lines
                .iter()
                .filter(|line| line.starts_with("no new reading for 15 s"))
                .count(),
            2
        );
        // Measuring from 50 s: read at 50 and 55 s.
        assert_eq!(said.airs, 2);
    }

    #[test]
    fn report_is_ok_or_a_warning_and_carries_the_last_air() {
        assert_eq!(report(None, None).level, Level::Ok);
        let warned = report(Some("a reading failed: word 0 fails its CRC"), None);
        assert_eq!(
            (warned.level, warned.message.as_str()),
            (Level::Warn, "a reading failed: word 0 fails its CRC")
        );
        // Absent is no warning: the sensor is optional (the first builder,
        // 17 Oct 15:36). Answered and silent stays one.
        let absent = report(
            Some(&format!(
                "{ABSENT} on I2C bus 0 (x); looked for again every 10 min"
            )),
            None,
        );
        assert_eq!(
            (absent.level, absent.message.as_str()),
            (Level::Ok, NOT_FITTED)
        );
        let silent = report(Some(&format!("{NO_READING} on I2C bus 0 (x)")), None);
        assert_eq!(silent.level, Level::Warn);
    }
}
