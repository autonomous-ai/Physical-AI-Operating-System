//! The I2C bus's clock, lowered to what the sensor takes. The SEN6x take
//! the standard mode only (100 kHz), and the board's bus 0 runs at 400 kHz.
//! It keeps the bus within the sensor's limit; it is not shown to be what
//! made lamp-4ace's sensor fail on 17 Oct, which read at 400 kHz too once
//! it measured (the first builder on #705, 20:2x).
//!
//! The Allwinner controller (`SUNXI TWI`) takes a new rate at run time,
//! through `device/freq`, and says the rate it runs at in `device/info`
//! (`twi->freqency = N`, the kernel's spelling). The device tree's
//! `clock-frequency` is only the rate it booted with. V1's HAL does the same
//! (`hal/drivers/environment/i2c.py`, `limit_sunxi_bus_clock`). Run as root,
//! by `lamp-air-clock.service`, before the air and the controls start; a
//! reboot puts the bus back at its device tree's rate. See `docs/air.md`.
use std::{fs, path::Path};

/// Where the adapters are.
pub const ADAPTERS: &str = "/sys/class/i2c-adapter";

/// The fastest the SEN6x take (Sensirion's datasheet: I2C standard mode).
pub const MOST_HZ: u32 = 100_000;

/// How long [`wait`] gives the adapter to appear, in one-second looks: a
/// controller whose driver binds after this unit runs (a module udev loads
/// late) is still caught.
pub const WAIT_LOOKS: u32 = 10;

/// Whether bus `bus`'s adapter is under `adapters`, looking up to `looks`
/// times with `pause` between.
pub fn wait(adapters: &Path, bus: u32, looks: u32, mut pause: impl FnMut()) -> bool {
    let adapter = adapters.join(format!("i2c-{bus}"));
    for look in 0..looks {
        if adapter.exists() {
            return true;
        }
        if look + 1 < looks {
            pause();
        }
    }
    false
}

/// What was done.
#[derive(Debug, PartialEq, Eq)]
pub enum Done {
    /// Not an Allwinner controller: its rate is not set here.
    Other(String),
    /// Already at most the rate: left as it is.
    Already(u32),
    /// Lowered from the first to the second, as read back.
    Lowered(u32, u32),
}

/// The rate `info` says the controller runs at.
fn rate(info: &str) -> Option<u32> {
    info.lines()
        .find_map(|line| line.trim().strip_prefix("twi->freqency"))
        .and_then(|rest| rest.trim().strip_prefix('='))
        .and_then(|value| value.trim().parse().ok())
        .filter(|hz| *hz > 0)
}

/// Bus `bus`'s clock at most `most` Hz, under `adapters` (`ADAPTERS` on a
/// lamp). A missing bus, an unreadable rate, a refused write or a rate still
/// above `most` afterwards is an error; a rate already at most `most` is
/// never written, so it is never raised.
pub fn limit(adapters: &Path, bus: u32, most: u32) -> Result<Done, String> {
    limit_by(adapters, bus, most, |freq, text| fs::write(freq, text))
}

/// [`limit`], with the write to `device/freq` given, so a test can be the
/// kernel that takes it.
fn limit_by(
    adapters: &Path,
    bus: u32,
    most: u32,
    mut write: impl FnMut(&Path, &str) -> std::io::Result<()>,
) -> Result<Done, String> {
    let adapter = adapters.join(format!("i2c-{bus}"));
    let name = fs::read_to_string(adapter.join("name"))
        .map_err(|e| format!("I2C bus {bus}: no adapter ({e})"))?;
    let name = name.trim();
    if !name.starts_with("SUNXI TWI") {
        return Ok(Done::Other(name.to_owned()));
    }
    let info = adapter.join("device/info");
    let read = || {
        fs::read_to_string(&info)
            .ok()
            .as_deref()
            .and_then(rate)
            .ok_or_else(|| format!("I2C bus {bus}: no rate in {}", info.display()))
    };
    let before = read()?;
    if before <= most {
        return Ok(Done::Already(before));
    }
    write(&adapter.join("device/freq"), &format!("{most}\n"))
        .map_err(|e| format!("I2C bus {bus}: {most} Hz was not taken: {e}"))?;
    let after = read()?;
    if after > most {
        return Err(format!(
            "I2C bus {bus}: still {after} Hz after asking for {most} Hz"
        ));
    }
    Ok(Done::Lowered(before, after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// An adapter as lamp-75c9's bus 0 has it (17 Oct): its `freq` is a
    /// plain file here, so a test sees what was written to it.
    fn adapter(test: &str, name: &str, info: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("lamp-air-clock-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("i2c-0/device")).unwrap();
        fs::write(root.join("i2c-0/name"), format!("{name}\n")).unwrap();
        fs::write(root.join("i2c-0/device/info"), info).unwrap();
        root
    }

    const SUNXI: &str = "SUNXI TWI(0x0000000002510000)";
    const AT_400: &str =
        "twi->bus_num = 0\ntwi->name = 2510000.twi\ntwi->irq = 423\ntwi->freqency = 400000\n";

    #[test]
    fn the_rate_is_read_as_the_kernel_spells_it() {
        assert_eq!(rate(AT_400), Some(400_000));
        assert_eq!(rate("twi->freqency=100000"), Some(100_000));
        for unknown in [
            "",
            "twi->freqency = 0\n",
            "twi->freqency = fast\n",
            "twi->frequency = 1\n",
        ] {
            assert_eq!(rate(unknown), None, "{unknown:?}");
        }
    }

    /// The controller is a file here: the write lands, and the rate read
    /// back is what `info` says. A kernel that took it changes `info`; one
    /// that didn't leaves it at 400000, and that is said, not passed.
    #[test]
    fn a_fast_bus_is_lowered_and_read_back() {
        let root = adapter("fast", SUNXI, AT_400);
        let err = limit(&root, 0, MOST_HZ).unwrap_err();
        assert!(err.contains("still 400000 Hz"), "{err}");
        assert_eq!(
            fs::read_to_string(root.join("i2c-0/device/freq")).unwrap(),
            "100000\n"
        );
        // As the kernel does: the write changes the rate `info` says.
        let info = root.join("i2c-0/device/info");
        let mut written = Vec::new();
        let done = limit_by(&root, 0, MOST_HZ, |freq, text| {
            written.push((freq.to_owned(), text.to_owned()));
            fs::write(&info, "twi->freqency = 100000\n")
        });
        assert_eq!(done, Ok(Done::Lowered(400_000, 100_000)));
        assert_eq!(
            written,
            [(root.join("i2c-0/device/freq"), "100000\n".to_owned())]
        );
        assert_eq!(limit(&root, 0, MOST_HZ), Ok(Done::Already(100_000)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_bus_already_slow_enough_is_never_written() {
        for (test, hz) in [("slow", 50_000u32), ("standard", 100_000)] {
            let root = adapter(test, SUNXI, &format!("twi->freqency = {hz}\n"));
            assert_eq!(limit(&root, 0, MOST_HZ), Ok(Done::Already(hz)));
            assert!(!root.join("i2c-0/device/freq").exists(), "{hz}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn another_controller_or_no_bus_is_left_alone() {
        let root = adapter("other", "bcm2835 I2C adapter", AT_400);
        assert_eq!(
            limit(&root, 0, MOST_HZ),
            Ok(Done::Other("bcm2835 I2C adapter".into()))
        );
        assert!(!root.join("i2c-0/device/freq").exists());
        let err = limit(&root, 3, MOST_HZ).unwrap_err();
        assert!(err.starts_with("I2C bus 3: no adapter"), "{err}");
        fs::remove_dir_all(root).unwrap();
    }

    /// An adapter that appears late is waited for, up to the looks given;
    /// one that never does is said after the last look, with no pause after.
    #[test]
    fn a_late_adapter_is_waited_for() {
        let root = adapter("late", SUNXI, AT_400);
        let mut pauses = 0;
        assert!(wait(&root, 0, WAIT_LOOKS, || pauses += 1));
        assert_eq!(pauses, 0);
        assert!(!wait(&root, 3, WAIT_LOOKS, || pauses += 1));
        assert_eq!(pauses, WAIT_LOOKS - 1);
        let mut pauses = 0;
        let late = root.join("i2c-5");
        assert!(wait(&root, 5, WAIT_LOOKS, || {
            pauses += 1;
            if pauses == 3 {
                fs::create_dir(&late).unwrap();
            }
        }));
        assert_eq!(pauses, 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unknown_rate_is_an_error_before_any_write() {
        let root = adapter("unknown", SUNXI, "twi->freqency = 0\n");
        let err = limit(&root, 0, MOST_HZ).unwrap_err();
        assert!(err.contains("no rate in"), "{err}");
        assert!(!root.join("i2c-0/device/freq").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_refused_write_is_an_error() {
        let root = adapter("refused", SUNXI, AT_400);
        // A folder where the file would be: the write fails, as a missing
        // permission would.
        fs::create_dir(root.join("i2c-0/device/freq")).unwrap();
        let err = limit(&root, 0, MOST_HZ).unwrap_err();
        assert!(err.contains("100000 Hz was not taken"), "{err}");
        fs::remove_dir_all(root).unwrap();
    }
}
