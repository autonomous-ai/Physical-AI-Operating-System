//! `lamp-air`: read the air sensor (SEN63C) every five seconds and say on
//! the bus what the air is like. See `docs/air.md`.
use std::{io::Write, process::ExitCode};

const USAGE: &str = "usage: lamp-air [--bus-number N] [--bus-root ABSOLUTE_DIR]\n       lamp-air once [--bus-number N]";
const HELP: &str =
    "Reads the lamp's air sensor (SEN63C) and says what the air is like. See docs/air.md.

  --bus-number  the I2C bus the sensor is on (0)
  --bus-root    the bus's directory (LAMP_BUS_ROOT, or /run/lampos-bus)

It publishes air/state every five seconds (particles, humidity and temperature at the
sensor, carbon dioxide, and how stuffy the air would feel) and how it is on diagnostics.
lamp-air once prints the sensor's name and one reading as JSON, and touches no bus.";

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn log(line: &str) {
    let _ = writeln!(std::io::stderr(), "lamp-air: {line}");
}

#[derive(Debug, PartialEq)]
struct Arguments {
    once: bool,
    bus_number: u32,
    bus: Option<std::path::PathBuf>,
}

fn parse(given: &[String]) -> Result<Option<Arguments>, String> {
    let (once, rest) = match given {
        [first, rest @ ..] if first == "once" => (true, rest),
        _ => (false, given),
    };
    let (mut bus_number, mut bus) = (None, None);
    let mut items = rest.iter();
    while let Some(option) = items.next() {
        if option == "--help" || option == "-h" {
            return Ok(None);
        }
        let value = items
            .next()
            .ok_or_else(|| format!("{option} needs a value"))?;
        let twice = match option.as_str() {
            "--bus-number" => bus_number
                .replace(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|n| *n < 64)
                        .ok_or("--bus-number is from 0 to 63")?,
                )
                .is_some(),
            "--bus-root" if !once => {
                let path = std::path::PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("--bus-root must be an absolute path".into());
                }
                bus.replace(path).is_some()
            }
            other => return Err(format!("unknown option {other}")),
        };
        if twice {
            return Err(format!("{option} was given twice"));
        }
    }
    Ok(Some(Arguments {
        once,
        bus_number: bus_number.unwrap_or(0),
        bus,
    }))
}

#[cfg(target_os = "linux")]
mod serve {
    use super::{Arguments, log};
    use lamp_air::{
        DATA_READY, PRODUCT, READ_TIMES, READ_VALUES, START, command, reading, text, watch,
        words_among_filler,
    };
    use lamp_bus::Publisher;
    use lamp_msgs::{AirState, Diagnostic};
    use rustix::{
        fd::OwnedFd,
        fs::{Mode, OFlags, open},
        io::{read, write},
        ioctl::{IntegerSetter, Opcode, ioctl},
    };
    use std::{
        io,
        sync::atomic::{AtomicBool, Ordering},
        thread::sleep,
        time::{Duration, Instant},
    };

    const I2C_SLAVE: Opcode = 0x0703;
    /// Filler was said in the journal: once a run is enough.
    static FILLER_SAID: AtomicBool = AtomicBool::new(false);
    /// The sensor answers a command after this long (the datasheet's
    /// execution times are 20 ms for reading values and names).
    const ANSWER: Duration = Duration::from_millis(25);

    /// One I2C adapter with the sensor's address set.
    pub struct Sensor(OwnedFd);

    /// The only unsafe call here: `I2C_SLAVE` takes the 7-bit address as a
    /// plain integer argument, by value (linux/i2c-dev.h); no pointer is
    /// passed and nothing is read back.
    #[allow(unsafe_code)]
    fn set_address(fd: &OwnedFd, address: u16) -> io::Result<()> {
        // SAFETY: I2C_SLAVE's argument is an integer by value, as above.
        unsafe {
            ioctl(
                fd,
                IntegerSetter::<I2C_SLAVE>::new_usize(usize::from(address)),
            )
        }
        .map_err(io::Error::from)
    }

    impl Sensor {
        pub fn open(bus: u32) -> io::Result<Self> {
            let fd = open(
                format!("/dev/i2c-{bus}"),
                OFlags::RDWR | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            set_address(&fd, lamp_air::ADDRESS)?;
            Ok(Self(fd))
        }

        /// Send a command and read `count` words of its answer, through
        /// any filler a bus faster than the sensor puts in it: the read is
        /// `READ_TIMES` the answer's length. Filler is said once in the
        /// journal.
        fn ask(&self, code: u16, count: usize) -> Result<Vec<u16>, String> {
            write(&self.0, &command(code)).map_err(|e| format!("command {code:#06x}: {e}"))?;
            sleep(ANSWER);
            let mut reply = vec![0u8; count * 3 * READ_TIMES];
            let got =
                read(&self.0, &mut reply).map_err(|e| format!("answer to {code:#06x}: {e}"))?;
            let filled = words_among_filler(&reply[..got], count)?;
            if filled.skipped > 0 && !FILLER_SAID.swap(true, Ordering::Relaxed) {
                log(&format!(
                    "the air sensor's answer to {code:#06x} had {} filler bytes (0xff) between its words, skipped by their CRCs",
                    filled.skipped
                ));
            }
            Ok(filled.words)
        }

        pub fn name(&self) -> Result<String, String> {
            self.ask(PRODUCT, 16).map(|w| text(&w))
        }

        pub fn ready(&self) -> Result<bool, String> {
            self.ask(DATA_READY, 1).map(|w| w[0] & 0x00FF == 1)
        }

        pub fn read(&self) -> Result<AirState, String> {
            reading(&self.ask(READ_VALUES, 7)?)
        }

        pub fn start(&self) -> Result<(), String> {
            write(&self.0, &command(START)).map_err(|e| format!("start: {e}"))?;
            sleep(Duration::from_millis(60));
            Ok(())
        }
    }

    pub fn once(arguments: &Arguments) -> Result<(), String> {
        let sensor = Sensor::open(arguments.bus_number).map_err(|e| e.to_string())?;
        let name = sensor.name()?;
        let air = sensor.read()?;
        println!(
            "{}",
            serde_json::json!({ "product": name, "ready": sensor.ready()?, "air": air })
        );
        Ok(())
    }

    /// The sensor on its bus, once found.
    struct I2c {
        bus: u32,
        sensor: Option<Sensor>,
    }

    impl I2c {
        fn found(&self) -> Result<&Sensor, String> {
            self.sensor.as_ref().ok_or_else(|| "not found".into())
        }
    }

    impl watch::Sensor for I2c {
        fn find(&mut self) -> Result<String, watch::Missing> {
            // Opening the bus and setting the address make no transaction.
            let sensor =
                Sensor::open(self.bus).map_err(|e| watch::Missing::NoBus(e.to_string()))?;
            let name = match sensor.name() {
                Ok(name) => name,
                // Something answered, but its name failed its CRC: start
                // its measurement anyway and try the values. A guard: on
                // lamp-4ace on 17 Oct the name failed all day, and whether
                // a start would have brought the values back is not shown
                // (the first builder, #705 20:44). Nobody acknowledging is
                // never started, so a lamp without the sensor makes no more
                // transactions.
                Err(error) if error.contains("CRC") => {
                    // A start refused (one already measuring may refuse
                    // it) still leaves the values to try; the loop starts
                    // it again after 15 s with no reading.
                    match sensor.start() {
                        Ok(()) => format!(
                            "an air sensor whose name failed ({error}); its measurement started"
                        ),
                        Err(start) => format!(
                            "an air sensor whose name failed ({error}); its start failed too ({start}), so its values are tried as it is"
                        ),
                    }
                }
                Err(error) => return Err(watch::Missing::NoAnswer(error)),
            };
            self.sensor = Some(sensor);
            Ok(name)
        }
        fn ready(&mut self) -> Result<bool, String> {
            self.found()?.ready()
        }
        fn read(&mut self) -> Result<AirState, String> {
            self.found()?.read()
        }
        fn start(&mut self) -> Result<(), String> {
            self.found()?.start()
        }
        fn lose(&mut self) {
            self.sensor = None;
        }
    }

    struct Wall(Instant);

    impl watch::Clock for Wall {
        fn now(&mut self) -> Duration {
            self.0.elapsed()
        }
        fn sleep(&mut self, duration: Duration) {
            sleep(duration);
        }
    }

    /// `air/state`, `diagnostics` and the journal.
    struct Bus {
        started: Instant,
        state: Publisher,
        health: Publisher,
    }

    impl watch::Say for Bus {
        fn air(&mut self, air: &AirState) {
            let _ = self
                .state
                .publish_json(self.started.elapsed().as_micros() as u64, air);
        }
        fn health(&mut self, report: &Diagnostic) {
            let _ = self
                .health
                .publish_json(self.started.elapsed().as_micros() as u64, report);
            let _ = self.state.alive();
        }
        fn log(&mut self, line: &str) {
            log(line);
        }
    }

    pub fn run(arguments: &Arguments) -> io::Result<()> {
        let bus = match &arguments.bus {
            Some(root) => lamp_bus::Bus::new(root)?,
            None => lamp_bus::Bus::from_environment()?,
        };
        let mut say = Bus {
            started: Instant::now(),
            state: bus.publisher_declared(AirState::TOPIC, &AirState::declaration(), "air")?,
            health: bus.publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), "air")?,
        };
        log(&format!("started i2c_bus={}", arguments.bus_number));
        // A reading already kept this boot: the sensor answered before this
        // start, and is not to be taken for an absent one (#1324).
        let answered_before = matches!(bus.latest(AirState::TOPIC), Ok(Some(_)));
        watch::run_answered(
            &mut I2c {
                bus: arguments.bus_number,
                sensor: None,
            },
            &mut Wall(Instant::now()),
            &mut say,
            arguments.bus_number,
            Duration::MAX,
            answered_before,
        );
        Ok(())
    }
}

fn main() -> ExitCode {
    // Its log lines never block it or end it, and its last ones reach the
    // journal (`lamp_ipc::log_relay`).
    let _log = lamp_ipc::log_relay::guard();
    let given: Vec<String> = std::env::args().skip(1).collect();
    let arguments = match parse(&given) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(why) => {
            eprintln!("lamp-air: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    #[cfg(target_os = "linux")]
    {
        let result = if arguments.once {
            serve::once(&arguments).map_err(|e| e.to_string())
        } else {
            serve::run(&arguments).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("lamp-air: {error}");
                ExitCode::FAILURE
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!(
            "lamp-air: the sensor on I2C bus {} is read on the lamp's Linux",
            arguments.bus_number
        );
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(text: &str) -> Result<Option<Arguments>, String> {
        parse(
            &text
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn the_options_are_read_and_the_rest_have_their_defaults() {
        assert_eq!(
            of("").unwrap().unwrap(),
            Arguments {
                once: false,
                bus_number: 0,
                bus: None
            }
        );
        assert_eq!(
            of("--bus-number 3 --bus-root /run/lampos-bus")
                .unwrap()
                .unwrap(),
            Arguments {
                once: false,
                bus_number: 3,
                bus: Some("/run/lampos-bus".into())
            }
        );
        assert_eq!(
            of("once --bus-number 0").unwrap().unwrap(),
            Arguments {
                once: true,
                bus_number: 0,
                bus: None
            }
        );
        assert_eq!(of("--help").unwrap(), None);
        for refused in [
            "--bus-number 99",
            "--bus-root relative",
            "once --bus-root /x",
            "--loud 1",
            "--bus-number",
        ] {
            assert!(of(refused).is_err(), "{refused:?}");
        }
    }
}
