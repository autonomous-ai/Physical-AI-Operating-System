//! What the kernel has said since boot about the lamp's storage: the SD/MMC
//! host's re-tunes that time out (#784) and block I/O errors on the card.
//! Read from `/dev/kmsg`, which every record since boot is in until the
//! kernel's ring buffer wraps; nothing is written to it.
use crate::Check;
use lamp_msgs::{PartResult, Verdict};
use std::time::Instant;

/// The kernel's ring of messages since boot.
pub const KMSG: &str = "/dev/kmsg";

/// A re-tune of an SD/MMC host that timed out (sunxi, #784).
pub const RETUNE: &str = "dma hold bit clear timeout";

/// What the kernel's storage messages since boot came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Storage {
    /// Records read.
    pub records: usize,
    /// Re-tunes that timed out.
    pub retunes: usize,
    /// I/O errors on an MMC block device.
    pub io_errors: usize,
    /// Records of this boot the ring no longer held: the first one kept was
    /// not the boot's first (sequence 0), or some were overwritten while
    /// they were read.
    pub lost: bool,
    /// The sequence number of the first record kept, when the records say.
    pub first_sequence: Option<u64>,
}

impl Storage {
    /// Counts one record's text: `pri,seq,usec,flags;message`, or the bare
    /// message. Continuation lines (starting with a space) are not records.
    pub fn count(&mut self, record: &str) {
        if record.starts_with(' ') {
            return;
        }
        let (prefix, message) = record.split_once(';').unwrap_or(("", record));
        if self.records == 0 {
            // `pri,seq,usec,flags`: the boot's first record has sequence 0.
            self.first_sequence = prefix.split(',').nth(1).and_then(|s| s.parse().ok());
            if self.first_sequence.is_some_and(|first| first > 0) {
                self.lost = true;
            }
        }
        self.records += 1;
        if message.contains(RETUNE) {
            self.retunes += 1;
        } else if message.contains("I/O error") && message.contains("mmcblk") {
            self.io_errors += 1;
        }
    }
}

/// The storage is healthy if the kernel reported no re-tune that timed out
/// and no I/O error on the card since boot. A fault in the records kept
/// fails whatever was lost; with records of the boot lost and no fault in
/// the rest, the boot as a whole is not checked.
pub fn storage(heard: &Result<Storage, String>) -> PartResult {
    let part = "storage";
    let storage = match heard {
        Ok(storage) => storage,
        Err(why) => {
            return PartResult::new(
                part,
                Verdict::NotChecked,
                &format!("the kernel's log cannot be read: {why}"),
            );
        }
    };
    if storage.records == 0 {
        return PartResult::new(part, Verdict::NotChecked, "the kernel's log was empty");
    }
    let result = if storage.io_errors > 0 {
        PartResult::new(
            part,
            Verdict::Fail,
            &format!("{} I/O errors on the card since boot", storage.io_errors),
        )
    } else if storage.retunes > 0 {
        PartResult::new(
            part,
            Verdict::Fail,
            &format!(
                "the SD/MMC host re-tuned and timed out {} times since boot (#784)",
                storage.retunes
            ),
        )
    } else if storage.lost {
        PartResult::new(
            part,
            Verdict::NotChecked,
            &format!(
                "no fault in the kernel's log, but it no longer holds all of this boot (first record kept: {})",
                storage
                    .first_sequence
                    .map_or("unknown".into(), |first| first.to_string())
            ),
        )
    } else {
        PartResult::new(part, Verdict::Pass, "")
    };
    let result = result
        .with("retunes", storage.retunes)
        .with("io_errors", storage.io_errors)
        .with("records", storage.records);
    if storage.lost {
        result.with("oldest_lost", "yes")
    } else {
        result
    }
}

/// Every record in the kernel's ring now.
#[cfg(target_os = "linux")]
pub fn read(path: &str) -> Result<Storage, String> {
    use std::{
        fs::OpenOptions,
        io::{ErrorKind, Read},
        os::unix::fs::OpenOptionsExt,
    };
    // O_NONBLOCK: a read at the end of the ring gives WouldBlock, not a wait.
    const O_NONBLOCK: i32 = 0o4000;
    let mut kmsg = OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut storage = Storage::default();
    // Each read gives one whole record; the kernel caps one at 8 KiB.
    let mut buffer = vec![0u8; 8192];
    loop {
        match kmsg.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                for line in String::from_utf8_lossy(&buffer[..n]).lines() {
                    storage.count(line);
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            // EPIPE: the ring overwrote records before they were read.
            Err(e) if e.raw_os_error() == Some(32) => storage.lost = true,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(storage)
}

/// Only Linux keeps `/dev/kmsg`.
#[cfg(not(target_os = "linux"))]
pub fn read(_path: &str) -> Result<Storage, String> {
    Err("only Linux has a kernel log to read".into())
}

/// Reads the kernel's log and judges the storage.
pub struct Kernel {
    pub path: String,
}

impl Check for Kernel {
    fn part(&self) -> &str {
        "storage"
    }

    fn run(&mut self, _deadline: Instant) -> PartResult {
        storage(&read(&self.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted(records: &[&str]) -> Storage {
        let mut storage = Storage::default();
        for record in records {
            storage.count(record);
        }
        storage
    }

    #[test]
    fn a_quiet_kernel_passes() {
        let quiet = counted(&[
            "6,0,0,-;Booting Linux on physical CPU 0x0",
            " SUBSYSTEM=platform",
            "6,1,15,-;mmc0: new high speed SDHC card at address aaaa",
        ]);
        assert_eq!(quiet.records, 2);
        let result = storage(&Ok(quiet));
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
    }

    #[test]
    fn a_log_that_lost_the_start_of_the_boot_is_not_checked_unless_a_fault_is_kept() {
        let late = counted(&[
            "6,10000,9000000,-;usb 3-1: new device",
            "6,10001,9000100,-;ok",
        ]);
        assert!(late.lost);
        assert_eq!(late.first_sequence, Some(10_000));
        let result = storage(&Ok(late));
        assert_eq!(result.verdict, Verdict::NotChecked, "{result:?}");
        assert!(result.reason.contains("10000"), "{}", result.reason);
        let overwritten = Storage {
            records: 1,
            lost: true,
            ..Storage::default()
        };
        assert_eq!(storage(&Ok(overwritten)).verdict, Verdict::NotChecked);
        let faulty = counted(&[
            "6,10000,1,-;x",
            "3,10001,2,-;sunxi_mmc: wait dma hold bit clear timeout",
        ]);
        assert_eq!(storage(&Ok(faulty)).verdict, Verdict::Fail);
    }

    #[test]
    fn a_retune_that_timed_out_fails_with_its_count() {
        let line = "3,812,7531000,-;sunxi_mmc_host-4021000.sdmmc: start tuning [ERR]: wait dma hold bit clear timeout";
        let heard = counted(&["6,1,0,-;Booting", line, line]);
        assert_eq!(heard.retunes, 2);
        let result = storage(&Ok(heard));
        assert_eq!(result.verdict, Verdict::Fail);
        assert!(result.reason.contains("2 times"), "{}", result.reason);
        assert!(
            result
                .values
                .iter()
                .any(|v| v.key == "retunes" && v.value == "2")
        );
    }

    #[test]
    fn an_io_error_on_the_card_fails_first() {
        let heard = counted(&[
            "3,9,1,-;blk_update_request: I/O error, dev mmcblk0, sector 1234",
            "3,10,2,-;wait dma hold bit clear timeout",
        ]);
        let result = storage(&Ok(heard));
        assert!(result.reason.contains("I/O errors"), "{}", result.reason);
    }

    #[test]
    fn a_log_that_cannot_be_read_or_is_empty_is_not_checked() {
        let denied = storage(&Err("Permission denied (os error 13)".into()));
        assert_eq!(denied.verdict, Verdict::NotChecked);
        assert!(denied.reason.contains("Permission denied"));
        let empty = storage(&Ok(Storage::default()));
        assert_eq!(empty.verdict, Verdict::NotChecked);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_missing_log_is_an_error_and_a_plain_file_is_read_whole() {
        assert!(read("/nonexistent/kmsg").is_err());
        let path = std::env::temp_dir().join(format!("lamp-selftest-kmsg-{}", std::process::id()));
        std::fs::write(
            &path,
            "6,1,0,-;Booting\n3,2,1,-;x: wait dma hold bit clear timeout\n",
        )
        .unwrap();
        let heard = read(path.to_str().unwrap()).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!((heard.records, heard.retunes), (2, 1));
    }
}
