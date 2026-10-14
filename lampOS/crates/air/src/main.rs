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
    use lamp_air::{DATA_READY, PRODUCT, READ_VALUES, START, command, reading, text, words};
    use lamp_bus::{Bus, Publisher};
    use lamp_msgs::{AirState, Diagnostic, Level};
    use rustix::{
        fd::OwnedFd,
        fs::{Mode, OFlags, open},
        io::{read, write},
        ioctl::{IntegerSetter, Opcode, ioctl},
    };
    use std::{
        io,
        thread::sleep,
        time::{Duration, Instant},
    };

    const I2C_SLAVE: Opcode = 0x0703;
    /// The sensor answers a command after this long (the datasheet's
    /// execution times are 20 ms for reading values and names).
    const ANSWER: Duration = Duration::from_millis(25);
    const EVERY: Duration = Duration::from_secs(5);

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

        /// Send a command and read `count` words of its answer.
        fn ask(&self, code: u16, count: usize) -> Result<Vec<u16>, String> {
            write(&self.0, &command(code)).map_err(|e| format!("command {code:#06x}: {e}"))?;
            sleep(ANSWER);
            let mut reply = vec![0u8; count * 3];
            let got =
                read(&self.0, &mut reply).map_err(|e| format!("answer to {code:#06x}: {e}"))?;
            words(&reply[..got])
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

    pub fn run(arguments: &Arguments) -> io::Result<()> {
        let started = Instant::now();
        let at_us = || started.elapsed().as_micros() as u64;
        let bus = match &arguments.bus {
            Some(root) => Bus::new(root)?,
            None => Bus::from_environment()?,
        };
        let mut state: Publisher =
            bus.publisher_declared(AirState::TOPIC, &AirState::declaration(), "air")?;
        let mut health: Publisher =
            bus.publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), "air")?;
        let mut sensor: Option<Sensor> = None;
        let mut why: Option<String> = None;
        let mut not_ready = 0u32;
        let mut last: Option<AirState> = None;
        let mut reported = Instant::now() - Duration::from_secs(1);
        let mut read_at = Instant::now() - EVERY;
        log(&format!("started i2c_bus={}", arguments.bus_number));
        loop {
            if sensor.is_none() {
                match Sensor::open(arguments.bus_number)
                    .map_err(|e| e.to_string())
                    .and_then(|s| s.name().map(|n| (s, n)))
                {
                    Ok((opened, name)) => {
                        log(&format!("the sensor answers: {name}"));
                        why = None;
                        sensor = Some(opened);
                    }
                    Err(error) => {
                        let line = format!("the sensor does not answer: {error}");
                        if why.as_deref() != Some(line.as_str()) {
                            log(&line);
                        }
                        why = Some(line);
                    }
                }
            }
            if read_at.elapsed() >= EVERY {
                read_at = Instant::now();
                if let Some(s) = &sensor {
                    match s.ready() {
                        Ok(true) => match s.read() {
                            Ok(air) => {
                                not_ready = 0;
                                why = None;
                                let _ = state.publish_json(at_us(), &air);
                                if last.is_none_or(|l| l.feel != air.feel) {
                                    log(&format!(
                                        "the air feels {:?}: co2 {:?} ppm, pm2.5 {:?}",
                                        air.feel, air.co2_ppm, air.pm2_5
                                    ));
                                }
                                last = Some(air);
                            }
                            Err(e) => why = Some(format!("a reading failed: {e}")),
                        },
                        Ok(false) => {
                            not_ready += 1;
                            // Not measuring (after a power cut): start it, once in a while.
                            if not_ready % 6 == 3 {
                                log("no new reading for 15 s: starting the sensor's measurement");
                                if let Err(e) = s.start() {
                                    why = Some(e);
                                }
                            }
                        }
                        Err(e) => {
                            why = Some(format!("the sensor stopped answering: {e}"));
                            sensor = None;
                        }
                    }
                }
            }
            if reported.elapsed() >= Duration::from_millis(950) {
                reported = Instant::now();
                let (level, message) = match &why {
                    Some(w) => (Level::Warn, w.clone()),
                    None => (Level::Ok, String::new()),
                };
                let mut report = Diagnostic::new(level, "air", &message);
                if let Some(air) = &last {
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
                let _ = health.publish_json(at_us(), &report);
                let _ = state.alive();
            }
            sleep(Duration::from_millis(200));
        }
    }
}

fn main() -> ExitCode {
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
