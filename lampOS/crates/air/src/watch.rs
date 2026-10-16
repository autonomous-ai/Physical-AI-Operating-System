//! The reading loop, apart from the I2C adapter and the clock, so it can be
//! run against a sensor that never answers. See `docs/air.md`.
//!
//! Every look for the sensor is an I2C transaction, and the kernel logs each
//! one nobody acknowledges ("ACK not received"). A lamp without the sensor
//! (lamp-cb94) was looked at five times a second, about 300 kernel lines a
//! minute, so its journal kept only about 90 minutes. Now a sensor that does
//! not answer is looked for [`QUICK_LOOKS`] times at the reading pace, then
//! once every [`LOOK_EVERY`], and the log says so once.
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

/// The sensor, as the loop uses it.
pub trait Sensor {
    /// Opens the adapter and asks the sensor's name.
    fn find(&mut self) -> Result<String, String>;
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

/// Reads the sensor on I2C bus `bus_number` until `until` on `clock`.
pub fn run(
    sensor: &mut impl Sensor,
    clock: &mut impl Clock,
    say: &mut impl Say,
    bus_number: u32,
    until: Duration,
) {
    let mut found = false;
    // Looks in a row that found nothing, and when the next is due.
    let mut misses = 0u32;
    let mut look_at = Duration::ZERO;
    let mut read_at = Duration::ZERO;
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
                    say.log(&format!("the sensor answers: {name}"));
                    found = true;
                    misses = 0;
                    why = None;
                    read_at = now;
                }
                Err(error) => {
                    misses += 1;
                    let line = if misses < QUICK_LOOKS {
                        look_at = now + EVERY;
                        format!("the sensor does not answer: {error}")
                    } else {
                        look_at = now + LOOK_EVERY;
                        format!(
                            "no air sensor answers on I2C bus {bus_number}; looked for again every {} min",
                            LOOK_EVERY.as_secs() / 60
                        )
                    };
                    // Each line once: the first miss, and the moment it counts
                    // as absent, with the last error.
                    if misses == 1 || misses == QUICK_LOOKS {
                        say.log(&if misses == QUICK_LOOKS {
                            format!("{line} (last: {error})")
                        } else {
                            line.clone()
                        });
                    }
                    why = Some(line);
                }
            }
        }
        if found && now >= read_at {
            read_at = now + EVERY;
            match sensor.ready() {
                Ok(true) => match sensor.read() {
                    Ok(air) => {
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
                    let line = format!("the sensor stopped answering: {error}");
                    say.log(&line);
                    why = Some(line);
                    sensor.lose();
                    found = false;
                    // Looked for again at once, then as a sensor never found.
                    look_at = now;
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

    /// An I2C bus that acknowledges nothing unless the sensor is up: from
    /// `answers_from`, if ever, until `stops_at`. Each transaction is
    /// counted, and each one nobody acknowledged, as the kernel logs those.
    struct Bus {
        time: Time,
        answers_from: Option<Duration>,
        stops_at: Option<Duration>,
        transactions: u32,
        not_acknowledged: u32,
    }
    impl Bus {
        fn new(time: &Time, answers_from: Option<u64>, stops_at: Option<u64>) -> Self {
            Self {
                time: Rc::clone(time),
                answers_from: answers_from.map(Duration::from_secs),
                stops_at: stops_at.map(Duration::from_secs),
                transactions: 0,
                not_acknowledged: 0,
            }
        }
        fn answers(&mut self) -> Result<(), String> {
            self.transactions += 1;
            let now = self.time.get();
            if self.answers_from.is_some_and(|from| now >= from)
                && self.stops_at.is_none_or(|at| now < at)
            {
                Ok(())
            } else {
                self.not_acknowledged += 1;
                Err("Remote I/O error (os error 121)".into())
            }
        }
    }
    impl Sensor for Bus {
        fn find(&mut self) -> Result<String, String> {
            self.answers().map(|()| "SEN63C".into())
        }
        fn ready(&mut self) -> Result<bool, String> {
            self.answers().map(|()| true)
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

    /// The loop for `seconds` on a stepped clock against `bus`.
    fn run_for(seconds: u64, answers_from: Option<u64>, stops_at: Option<u64>) -> (Bus, Said) {
        let time: Time = Rc::default();
        let mut bus = Bus::new(&time, answers_from, stops_at);
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
            Duration::from_secs(seconds),
        );
        (bus, said)
    }

    /// Twelve on lamp-cb94: no sensor on the bus. An hour of it is a handful
    /// of transactions, not the 18,000 a look every 200 ms made, and the
    /// journal hears of it twice; the health report still comes each second.
    #[test]
    fn a_sensor_that_never_answers_is_looked_for_three_times_then_every_ten_minutes() {
        let (bus, said) = run_for(3_600, None, None);
        // At 0, 5 and 10 s, then 610, 1210, 1810, 2410 and 3010 s.
        assert_eq!((bus.transactions, bus.not_acknowledged), (8, 8));
        assert_eq!(said.airs, 0);
        assert_eq!(
            said.lines,
            [
                "the sensor does not answer: Remote I/O error (os error 121)",
                "no air sensor answers on I2C bus 0; looked for again every 10 min (last: Remote I/O error (os error 121))",
            ]
        );
        assert!(
            (3_500..=3_700).contains(&said.reports.len()),
            "{}",
            said.reports.len()
        );
        let (at, last) = said.reports.last().unwrap();
        assert!(*at > Duration::from_secs(3_598));
        assert_eq!(
            (last.level, last.message.as_str()),
            (
                Level::Warn,
                "no air sensor answers on I2C bus 0; looked for again every 10 min"
            )
        );
    }

    /// A sensor plugged in, or woken, later is found at the next look and
    /// read every five seconds from then on.
    #[test]
    fn a_sensor_that_answers_later_is_read_from_the_next_look() {
        let (bus, said) = run_for(1_900, Some(1_200), None);
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

    /// A sensor that stops answering is looked for again at once, then as
    /// one never found: no flood after it goes either.
    #[test]
    fn a_sensor_that_stops_answering_is_looked_for_at_once_then_backs_off() {
        let (bus, said) = run_for(3_600, Some(0), Some(60));
        // Its failed ready at 60 s, looks at 60, 65 and 70 s, then 670,
        // 1270, 1870, 2470 and 3070 s.
        assert_eq!(bus.not_acknowledged, 9);
        assert!(
            said.lines
                .iter()
                .any(|line| line.starts_with("the sensor stopped answering")),
            "{:?}",
            said.lines
        );
        assert_eq!(
            said.lines
                .iter()
                .filter(|line| line.starts_with("no air sensor answers"))
                .count(),
            1
        );
    }

    #[test]
    fn report_is_ok_or_a_warning_and_carries_the_last_air() {
        assert_eq!(report(None, None).level, Level::Ok);
        let warned = report(Some("no air sensor"), None);
        assert_eq!(
            (warned.level, warned.message.as_str()),
            (Level::Warn, "no air sensor")
        );
    }
}
