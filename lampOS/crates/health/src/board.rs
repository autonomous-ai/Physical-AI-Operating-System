//! The board the lamp runs on: its numbers, and whether any is too high.
//!
//! The numbers are read from the files the system keeps: `/proc` for load,
//! memory and uptime, `/sys/class/thermal` for each temperature and the
//! points at which the board acts on it, and the disk itself for how much
//! is free. Every place is a parameter ([`Places`]), so that a test can
//! point the reader at a tree of its own.
//!
//! Two things come of a reading. Its numbers are `board/state`
//! ([`Reading::state`]), which says nothing about whether they are good.
//! Its report is the board's own `Diagnostic` ([`Reading::report`]): the
//! board is one more part of the lamp, and says how it is like the others.
//!
//! Nothing here fails. A number that cannot be read is not there, and for
//! those the lamp cannot do without ([`Reading::unread`]) the report says
//! which.
use lamp_msgs::{BoardState, Diagnostic, KeyValue, Level};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

/// The name the board reports under.
pub const PART: &str = "board";

// The thresholds. Temperatures are in whole degrees Celsius, the rest are
// the fractions `board/state` carries.

/// A zone this close below its lowest passive trip point is a warning. At
/// that point the board begins to slow its processors down to cool, and
/// what the lamp says then comes late. It is nothing: the warning is for
/// a board that is slowing itself down, not for one that is near it. On
/// lamp-4ace on 2026-10-14 the processor zones read 63 to 66 °C with the
/// lamp at rest, against a first trip point of 70: a warning five under
/// that point would have been on and off all day, saying nothing a
/// person could act on.
pub const WARN_BELOW_PASSIVE_C: i64 = 0;
/// A zone this close below its critical trip point is an error. At that
/// point the board turns itself off.
pub const ERROR_BELOW_CRITICAL_C: i64 = 10;
/// A zone that names no trip point of its own is a warning from here.
pub const WARN_WITHOUT_TRIPS_C: i64 = 80;
/// And an error from here: where the board's processor zones have their
/// second passive trip point.
pub const ERROR_WITHOUT_TRIPS_C: i64 = 90;
/// A processor zone this hot is a warning, whatever trip points it names
/// (the first builder, 2026-10-16: lamp-4ace's processor zones read 86 to
/// 89 °C from 16:01 with the owner at the lamp, its first trip at 90, and at
/// 16:38 a voice worker held up past its bounds exited). Said from every
/// processor zone, judged or not ([`Places::judged`]).
pub const CPU_WARN_C: i64 = 85;
/// The zones that are the processors', by how their names begin.
pub const CPU_ZONES: &str = "cpu";
/// Less of the disk free than this is a warning: logs, recordings and
/// updates all need room, and there is still time to make some.
pub const DISK_FREE_WARN: f32 = 0.10;
/// Less than this is an error: the next thing written may not fit.
pub const DISK_FREE_ERROR: f32 = 0.03;
/// More of the memory in use than this is a warning: little is left
/// before the system has to end a program to find some.
pub const MEMORY_USED_WARN: f32 = 0.90;
/// A load for each processor above this is a warning: for a minute there
/// has been half as much work again as the processors can do, and
/// everything waits its turn.
pub const CPU_LOAD_WARN: f32 = 1.5;

/// No file read here is longer. `/proc/stat` is the longest: its lines
/// for the processors come first, and its line of interrupts can be long.
const MOST_FILE_BYTES: u64 = 256 * 1024;
const MOST_ZONES: usize = 64;
const MOST_TRIP_POINTS: usize = 16;
const MOST_ZONE_NAME_BYTES: usize = 48;
/// A reading outside these is not a temperature: below absolute zero, or
/// hotter than any board gets. Some sensors answer so when they are off.
const COLDEST_MILLI_C: i64 = -273_150;
const HOTTEST_MILLI_C: i64 = 1_000_000;

/// Where the board is read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// `/proc`: `loadavg`, `stat`, `meminfo`, `uptime`, `net/wireless`.
    pub proc: PathBuf,
    /// `/sys`: `class/thermal/thermal_zone*`.
    pub sys: PathBuf,
    /// `/run`: `systemd/timesync/synchronized`.
    pub run: PathBuf,
    /// Any path on the disk whose free room is asked for.
    pub disk: PathBuf,
    /// The zones whose own limits are judged, by how their names begin.
    /// With none named, every zone is. A board has zones for parts the
    /// lamp does not use, with limits of their own: on lamp-4ace the
    /// graphics zone slows itself down from 60 °C and reads 64 at rest,
    /// which is nothing to the lamp and would be a warning all day. Every
    /// zone's temperature is published whether it is judged or not.
    pub judged: Vec<String>,
    /// Who is asked whether the clock is set.
    pub clock: ClockSource,
}

/// Who says whether the clock is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClockSource {
    /// systemd-timesyncd's marker under `run`, as before; `None` where it
    /// does not run, which is the real board (health.md).
    #[default]
    Timesync,
    /// The kernel, as `timedatectl`'s `NTPSynchronized` does: whatever
    /// keeps the clock (timesyncd, chrony, ntpd) tells the kernel it is
    /// set. The marker still counts first (#830, `docs/offline.md`).
    Kernel,
}

impl std::str::FromStr for ClockSource {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "timesync" => Ok(Self::Timesync),
            "kernel" => Ok(Self::Kernel),
            _ => Err(()),
        }
    }
}

impl Default for Places {
    fn default() -> Self {
        Self {
            proc: "/proc".into(),
            sys: "/sys".into(),
            run: "/run".into(),
            disk: "/".into(),
            judged: Vec::new(),
            clock: ClockSource::Timesync,
        }
    }
}

/// One place the board measures its temperature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    /// The zone's `type`, which is its key in `board/state`.
    pub name: String,
    /// Thousandths of a degree Celsius, as the system gives it.
    pub milli_c: i64,
    /// Its passive trip points, the lowest first: where the board slows
    /// itself down to cool.
    pub passive_milli_c: Vec<i64>,
    /// Its critical trip point: where the board turns itself off.
    pub critical_milli_c: Option<i64>,
}

/// One group of processors and the fastest it may run now: below what the
/// hardware can do, the board is holding it back to cool (a thermal
/// cooling device lowers `scaling_max_freq`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuCap {
    /// `policyN` under `devices/system/cpu/cpufreq`.
    pub policy: String,
    /// `scaling_max_freq`, in kHz: the fastest it may run now.
    pub max_khz: u64,
    /// `cpuinfo_max_freq`, in kHz: the fastest it can run.
    pub hardware_khz: u64,
}

impl CpuCap {
    /// Held below what the hardware can do.
    pub fn throttled(&self) -> bool {
        self.max_khz < self.hardware_khz
    }
}

/// A number the lamp cannot do without, and why it could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unread {
    /// The number, by its name in `board/state`.
    pub what: &'static str,
    /// The file and what was the matter with it.
    pub why: String,
}

/// The board as it was read once.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub zones: Vec<Zone>,
    /// The load over the last minute, for each processor.
    pub cpu_load: Option<f32>,
    /// From 0 to 1.
    pub memory_used: Option<f32>,
    /// From 0 to 1.
    pub disk_free: Option<f32>,
    pub uptime_s: Option<u64>,
    pub wifi_dbm: Option<i32>,
    pub clock_synced: Option<bool>,
    /// Which of the four numbers above the Wi-Fi are not there.
    pub unread: Vec<Unread>,
    /// The zones whose own limits are judged, by how their names begin:
    /// every zone, with none named ([`Places::judged`]).
    pub judged: Vec<String>,
    /// Each processor group's cap; empty where the board has no cpufreq.
    pub cpu_caps: Vec<CpuCap>,
}

/// One thing that is not as it should be.
struct Finding {
    level: Level,
    message: String,
}

fn warn(message: String) -> Option<Finding> {
    Some(Finding {
        level: Level::Warn,
        message,
    })
}

fn error(message: String) -> Option<Finding> {
    Some(Finding {
        level: Level::Error,
        message,
    })
}

/// Thousandths of a degree as whole degrees, for a person.
fn degrees(milli_c: i64) -> String {
    format!("{:.0}", milli_c as f64 / 1_000.0)
}

fn percent(fraction: f32) -> String {
    format!("{:.0}%", fraction * 100.0)
}

impl Zone {
    pub fn celsius(&self) -> f32 {
        self.milli_c as f32 / 1_000.0
    }

    /// Whether the zone is too hot, by its own trip points if it names
    /// any, and by two fixed lines if it names none.
    fn finding(&self) -> Option<Finding> {
        let name = &self.name;
        let at = degrees(self.milli_c);
        if self.passive_milli_c.is_empty() && self.critical_milli_c.is_none() {
            return if self.milli_c >= ERROR_WITHOUT_TRIPS_C * 1_000 {
                error(format!(
                    "{name} is at {at} °C: too hot, and the zone names no limit of its own"
                ))
            } else if self.milli_c >= WARN_WITHOUT_TRIPS_C * 1_000 {
                warn(format!(
                    "{name} is at {at} °C: hot, and the zone names no limit of its own"
                ))
            } else {
                None
            };
        }
        if let Some(critical) = self.critical_milli_c
            && self.milli_c >= critical - ERROR_BELOW_CRITICAL_C * 1_000
        {
            return error(format!(
                "{name} is at {at} °C: the board turns itself off at {}",
                degrees(critical)
            ));
        }
        if let Some(&second) = self.passive_milli_c.get(1)
            && self.milli_c >= second
        {
            return error(format!(
                "{name} is at {at} °C: past {}, the second point at which the board slows itself down",
                degrees(second)
            ));
        }
        if let Some(&first) = self.passive_milli_c.first()
            && self.milli_c >= first - WARN_BELOW_PASSIVE_C * 1_000
        {
            return warn(format!(
                "{name} is at {at} °C: the board slows itself down from {}",
                degrees(first)
            ));
        }
        None
    }
}

impl Reading {
    /// The hottest zone; of two as hot, the one read first.
    pub fn hottest(&self) -> Option<&Zone> {
        self.zones.iter().reduce(|hottest, zone| {
            if zone.milli_c > hottest.milli_c {
                zone
            } else {
                hottest
            }
        })
    }

    /// The hottest processor zone ([`CPU_ZONES`]).
    pub fn hottest_cpu(&self) -> Option<&Zone> {
        self.zones
            .iter()
            .filter(|zone| zone.name.starts_with(CPU_ZONES))
            .reduce(|hottest, zone| {
                if zone.milli_c > hottest.milli_c {
                    zone
                } else {
                    hottest
                }
            })
    }

    /// The processor group held furthest below what it can do, if any is.
    pub fn throttled(&self) -> Option<&CpuCap> {
        self.cpu_caps
            .iter()
            .filter(|cap| cap.throttled())
            .min_by(|a, b| {
                let share = |cap: &CpuCap| cap.max_khz as f64 / cap.hardware_khz.max(1) as f64;
                share(a).total_cmp(&share(b))
            })
    }

    /// What the board says of its processors' heat: an error once they are
    /// held back to cool, a warning from [`CPU_WARN_C`].
    fn cpu_finding(&self) -> Option<Finding> {
        let zone = self.hottest_cpu();
        let hot = zone.map(|zone| format!("{} at {} °C", zone.name, degrees(zone.milli_c)));
        if let Some(cap) = self.throttled() {
            return error(format!(
                "the processors are held back to cool: {} may run at {} of {} MHz{}",
                cap.policy,
                cap.max_khz / 1_000,
                cap.hardware_khz / 1_000,
                hot.map(|hot| format!(", {hot}")).unwrap_or_default()
            ));
        }
        let zone = zone.filter(|zone| zone.milli_c >= CPU_WARN_C * 1_000)?;
        warn(match next_trip(zone) {
            Some(trip) => format!(
                "the processors are hot: {} °C at {}, {} below the board's trip at {}",
                degrees(zone.milli_c),
                zone.name,
                degrees(trip - zone.milli_c),
                degrees(trip)
            ),
            None => format!(
                "the processors are hot: {} °C at {}",
                degrees(zone.milli_c),
                zone.name
            ),
        })
    }

    /// The numbers as `board/state` carries them, if the four the lamp
    /// cannot do without are all there. A zone is there if it could be
    /// read; the Wi-Fi and the clock if they could be asked.
    pub fn state(&self) -> Option<BoardState> {
        let state = BoardState {
            temperature_c: self
                .zones
                .iter()
                .map(|zone| (zone.name.clone(), zone.celsius()))
                .collect(),
            cpu_load: self.cpu_load?,
            memory_used: self.memory_used?,
            disk_free: self.disk_free?,
            uptime_s: self.uptime_s?,
            wifi_dbm: self.wifi_dbm,
            clock_synced: self.clock_synced,
        };
        state.check().is_ok().then_some(state)
    }

    /// Everything that is not as it should be: the zones, the hottest
    /// first, then the disk, the memory, the load, the clock, and what
    /// could not be read.
    fn findings(&self) -> Vec<Finding> {
        let mut zones: Vec<&Zone> = self
            .zones
            .iter()
            .filter(|zone| {
                self.judged.is_empty()
                    || self
                        .judged
                        .iter()
                        .any(|begins| zone.name.starts_with(begins.as_str()))
            })
            .collect();
        zones.sort_by_key(|zone| std::cmp::Reverse(zone.milli_c));
        let mut found: Vec<Finding> = zones.iter().filter_map(|zone| zone.finding()).collect();
        found.extend(self.cpu_finding());
        found.extend(self.disk_free.and_then(|free| {
            if free < DISK_FREE_ERROR {
                error(format!(
                    "the disk is all but full: {} of it is free",
                    percent(free)
                ))
            } else if free < DISK_FREE_WARN {
                warn(format!(
                    "the disk is filling up: {} of it is free",
                    percent(free)
                ))
            } else {
                None
            }
        }));
        found.extend(
            self.memory_used
                .filter(|used| *used > MEMORY_USED_WARN)
                .and_then(|used| warn(format!("{} of the memory is in use", percent(used)))),
        );
        found.extend(
            self.cpu_load
                .filter(|load| *load > CPU_LOAD_WARN)
                .and_then(|load| {
                    warn(format!(
                    "the processors have more work than they can do: a load of {load:.2} for each"
                ))
                }),
        );
        if self.clock_synced == Some(false) {
            found.extend(warn("the clock is not set from the network".into()));
        }
        if !self.unread.is_empty() {
            let which: Vec<&str> = self.unread.iter().map(|unread| unread.what).collect();
            found.extend(warn(format!("could not be read: {}", which.join(", "))));
        }
        found
    }

    /// The board's own report: the worst thing found, in words, and the
    /// numbers as text. Of two findings as bad, the one found first.
    pub fn report(&self) -> Diagnostic {
        let worst = self.findings().into_iter().reduce(|worst, finding| {
            if finding.level.rank() > worst.level.rank() {
                finding
            } else {
                worst
            }
        });
        let (level, message) = worst.map_or((Level::Ok, String::new()), |finding| {
            (finding.level, finding.message)
        });
        let mut values = Vec::new();
        let mut value = |key: &str, value: String| {
            values.push(KeyValue {
                key: key.into(),
                value,
            });
        };
        if let Some(load) = self.cpu_load {
            value("cpu_load", format!("{load:.2}"));
        }
        if let Some(used) = self.memory_used {
            value("memory_used", format!("{used:.3}"));
        }
        if let Some(free) = self.disk_free {
            value("disk_free", format!("{free:.3}"));
        }
        if let Some(uptime) = self.uptime_s {
            value("uptime_s", uptime.to_string());
        }
        if let Some(zone) = self.hottest() {
            value("hottest_zone", zone.name.clone());
            value("hottest_c", format!("{:.1}", zone.celsius()));
        }
        // The processors' heat, for the self-test and the review page.
        if let Some(zone) = self.hottest_cpu() {
            value("cpu_zone", zone.name.clone());
            value("cpu_c", format!("{:.1}", zone.celsius()));
            if let Some(trip) = next_trip(zone) {
                value("cpu_next_trip_c", degrees(trip));
            }
        }
        if !self.cpu_caps.is_empty() {
            value("throttled", self.throttled().is_some().to_string());
            if let Some(cap) = self.throttled() {
                value("cpu_max_mhz", (cap.max_khz / 1_000).to_string());
                value("cpu_hardware_mhz", (cap.hardware_khz / 1_000).to_string());
            }
        }
        Diagnostic {
            level,
            name: PART.into(),
            message,
            values,
        }
    }
}

/// The zone's next trip point above it, passive or critical; with none
/// above, the highest it names.
fn next_trip(zone: &Zone) -> Option<i64> {
    let trips = zone
        .passive_milli_c
        .iter()
        .chain(zone.critical_milli_c.iter());
    trips
        .clone()
        .filter(|trip| **trip > zone.milli_c)
        .min()
        .or_else(|| trips.max())
        .copied()
}

/// What a file holds, or the file and what was the matter with it.
fn text(path: &Path) -> Result<String, String> {
    let named = |error: io::Error| format!("{}: {error}", path.display());
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(named)?
        .take(MOST_FILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(named)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The first number of `loadavg` over the processors that `stat` has a
/// line for (`cpu0`, `cpu1`, ...; the line `cpu` is all of them together).
fn cpu_load(proc: &Path) -> Result<f32, String> {
    let path = proc.join("loadavg");
    let load = text(&path)?
        .split_whitespace()
        .next()
        .and_then(|first| first.parse::<f32>().ok())
        .filter(|load| load.is_finite() && *load >= 0.0)
        .ok_or_else(|| format!("{}: does not begin with a load", path.display()))?;
    let path = proc.join("stat");
    let processors = text(&path)?
        .lines()
        .filter(|line| {
            line.strip_prefix("cpu")
                .and_then(|rest| rest.bytes().next())
                .is_some_and(|next| next.is_ascii_digit())
        })
        .count();
    if processors == 0 {
        return Err(format!("{}: has no line for a processor", path.display()));
    }
    Ok(load / processors as f32)
}

/// One less the share of `MemTotal` that `MemAvailable` is.
fn memory_used(proc: &Path) -> Result<f32, String> {
    let path = proc.join("meminfo");
    let text = text(&path)?;
    let kilobytes = |name: &str| {
        text.lines()
            .find_map(|line| {
                line.strip_prefix(name)?
                    .strip_prefix(':')?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .ok_or_else(|| format!("{}: has no {name}", path.display()))
    };
    let total = kilobytes("MemTotal")?;
    let available = kilobytes("MemAvailable")?;
    if total == 0 {
        return Err(format!("{}: MemTotal is 0", path.display()));
    }
    Ok((1.0 - available as f64 / total as f64).clamp(0.0, 1.0) as f32)
}

/// The blocks an ordinary program may still use, over all the disk's.
fn disk_free(disk: &Path) -> Result<f32, String> {
    let named = |why: String| format!("{}: {why}", disk.display());
    let held =
        rustix::fs::statvfs(disk).map_err(|error| named(io::Error::from(error).to_string()))?;
    if held.f_blocks == 0 {
        return Err(named("a disk of no blocks".into()));
    }
    Ok((held.f_bavail as f64 / held.f_blocks as f64).clamp(0.0, 1.0) as f32)
}

/// The first number of `uptime`, in whole seconds.
fn uptime_s(proc: &Path) -> Result<u64, String> {
    let path = proc.join("uptime");
    text(&path)?
        .split_whitespace()
        .next()
        .and_then(|first| first.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| seconds as u64)
        .ok_or_else(|| format!("{}: does not begin with a time", path.display()))
}

/// The level of the first device in `net/wireless` that gives one. A
/// device's line is its name and a colon, its status, then link, level and
/// noise, each perhaps with a full stop after it. A level that is not
/// between -150 and 0 is not in dBm, and is left out.
fn wifi_dbm(proc: &Path) -> Option<i32> {
    let text = text(&proc.join("net/wireless")).ok()?;
    text.lines().find_map(|line| {
        let (_, numbers) = line.split_once(':')?;
        let level = numbers
            .split_whitespace()
            .nth(2)?
            .trim_end_matches('.')
            .parse::<f32>()
            .ok()?;
        (-150.0..=0.0)
            .contains(&level)
            .then_some(level.round() as i32)
    })
}

/// Whether systemd's timesync says the clock is set: it keeps a directory,
/// and puts a file in it once it has set the clock. With no directory,
/// nobody can be asked.
fn clock_synced(run: &Path, source: ClockSource) -> Option<bool> {
    let timesync = run.join("systemd/timesync");
    if timesync.join("synchronized").exists() {
        Some(true)
    } else if let Some(set) = (source == ClockSource::Kernel)
        .then(kernel_clock_synced)
        .flatten()
    {
        Some(set)
    } else if timesync.is_dir() {
        Some(false)
    } else {
        None
    }
}

/// The kernel's answer ([`lamp_ipc::clock_synced`]).
pub fn kernel_clock_synced() -> Option<bool> {
    lamp_ipc::clock_synced()
}

/// A file of one whole number of thousandths of a degree.
fn millidegrees(path: &Path) -> Option<i64> {
    text(path)
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|milli_c| (COLDEST_MILLI_C..=HOTTEST_MILLI_C).contains(milli_c))
}

/// Every `thermal_zoneN` that has a `type` and a `temp`, by its number,
/// with the trip points it names.
fn zones(sys: &Path) -> Vec<Zone> {
    let thermal = sys.join("class/thermal");
    let Ok(entries) = fs::read_dir(&thermal) else {
        return Vec::new();
    };
    let mut numbered: Vec<(u32, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let number = entry
                .file_name()
                .to_str()?
                .strip_prefix("thermal_zone")?
                .parse()
                .ok()?;
            Some((number, entry.path()))
        })
        .collect();
    numbered.sort();
    numbered.truncate(MOST_ZONES);
    let mut zones: Vec<Zone> = Vec::new();
    for (number, zone) in numbered {
        let Some(kind) = text(&zone.join("type"))
            .ok()
            .map(|kind| kind.trim().to_owned())
            .filter(|kind| {
                !kind.is_empty()
                    && kind.len() <= MOST_ZONE_NAME_BYTES
                    && !kind.chars().any(char::is_control)
            })
        else {
            continue;
        };
        let Some(milli_c) = millidegrees(&zone.join("temp")) else {
            continue;
        };
        // Two zones of one type: the second is told apart by its number.
        let name = if zones.iter().any(|zone| zone.name == kind) {
            format!("{kind}_{number}")
        } else {
            kind
        };
        let mut passive = Vec::new();
        let mut critical: Option<i64> = None;
        for point in 0..MOST_TRIP_POINTS {
            let kind = text(&zone.join(format!("trip_point_{point}_type")));
            let at = millidegrees(&zone.join(format!("trip_point_{point}_temp")));
            match (kind.as_deref().map(str::trim), at) {
                // They are numbered from 0 without a hole.
                (Err(_), None) => break,
                (Ok("passive"), Some(at)) => passive.push(at),
                (Ok("critical"), Some(at)) => {
                    critical = Some(critical.map_or(at, |lowest| lowest.min(at)));
                }
                // An active or a hot point, or half of one.
                _ => {}
            }
        }
        passive.sort_unstable();
        zones.push(Zone {
            name,
            milli_c,
            passive_milli_c: passive,
            critical_milli_c: critical,
        });
    }
    zones
}

/// A number the lamp cannot do without: there, or noted as not read.
fn needed<T>(unread: &mut Vec<Unread>, what: &'static str, read: Result<T, String>) -> Option<T> {
    read.map_err(|why| unread.push(Unread { what, why })).ok()
}

/// Read the board once. Never fails: what could not be read is not in the
/// reading, and `unread` says which of the numbers the lamp cannot do
/// without those are.
pub fn read(places: &Places) -> Reading {
    let mut unread = Vec::new();
    let cpu_load = needed(&mut unread, "cpu_load", cpu_load(&places.proc));
    let memory_used = needed(&mut unread, "memory_used", memory_used(&places.proc));
    let disk_free = needed(&mut unread, "disk_free", disk_free(&places.disk));
    let uptime_s = needed(&mut unread, "uptime_s", uptime_s(&places.proc));
    Reading {
        zones: zones(&places.sys),
        cpu_load,
        memory_used,
        disk_free,
        uptime_s,
        wifi_dbm: wifi_dbm(&places.proc),
        clock_synced: clock_synced(&places.run, places.clock),
        unread,
        judged: places.judged.clone(),
        cpu_caps: cpu_caps(&places.sys),
    }
}

/// Each `policyN` under `devices/system/cpu/cpufreq` with both numbers, by
/// its number; one that cannot be read is left out.
fn cpu_caps(sys: &Path) -> Vec<CpuCap> {
    let folder = sys.join("devices/system/cpu/cpufreq");
    let Ok(entries) = fs::read_dir(&folder) else {
        return Vec::new();
    };
    let mut caps: Vec<(u32, CpuCap)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let number: u32 = name.strip_prefix("policy")?.parse().ok()?;
            let khz = |file: &str| -> Option<u64> {
                text(&entry.path().join(file)).ok()?.trim().parse().ok()
            };
            let cap = CpuCap {
                policy: name.clone(),
                max_khz: khz("scaling_max_freq")?,
                hardware_khz: khz("cpuinfo_max_freq").filter(|khz| *khz > 0)?,
            };
            Some((number, cap))
        })
        .take(MOST_ZONES)
        .collect();
    caps.sort_by_key(|(number, _)| *number);
    caps.into_iter().map(|(_, cap)| cap).collect()
}
