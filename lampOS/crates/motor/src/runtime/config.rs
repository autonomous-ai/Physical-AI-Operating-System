//! Explicit, bounded provisioning. This is trusted evidence, not a safety proof.
use crate::{
    Joint,
    calibration::{CalibrationRecord, RegisterBindingRecord, UnitCalibration, UnitId},
    control::{QualifiedEnvelope, Timing},
    trajectory::{JointEnvelope, RawProfile},
    units::NormalizedPosition,
};
use lamp_interaction::{BootId, MonoTime};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub device: PathBuf,
    pub lock_directory: PathBuf,
    pub expected_device: Identity,
    pub unit: String,
    /// Software normalization/sign from the explicitly selected V1 source.
    pub calibration: [Calibration; 5],
    /// Separately observed per-unit EEPROM. Missing evidence has no fallback.
    pub register_binding: [RegisterBinding; 5],
    pub qualification: Qualification,
    pub timing: Bounds,
    pub placement_source: BootId,
    /// Finite worker lifetime; this never extends an authority/placement lease.
    pub maximum_runtime_us: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub filesystem_device: u64,
    pub inode: u64,
    pub character_device: u64,
    pub major: u32,
    pub minor: u32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calibration {
    pub joint: u8,
    pub id: u16,
    pub drive_mode: u8,
    pub homing_offset: i32,
    pub range_min: i32,
    pub range_max: i32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterBinding {
    pub joint: u8,
    pub minimum: i32,
    pub maximum: i32,
    pub homing_offset: i32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub transaction_us: u64,
    pub io_us: u64,
    pub readback_age_us: u64,
    pub placement_age_us: u64,
    pub interval_us: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Qualification {
    pub record: [u8; 32],
    pub valid_from_us: u64,
    pub expires_at_us: u64,
    pub joints: [Envelope; 5],
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub minimum: f64,
    pub maximum: f64,
    pub velocity: f64,
    pub acceleration: f64,
    pub raw_acceleration: u8,
    pub raw_goal_time: u16,
    pub raw_speed: u16,
    pub maximum_load_raw: u16,
    pub maximum_velocity_raw: u16,
    pub tracking_error: f64,
    pub settling_error: f64,
}
pub struct Validated {
    pub unit: UnitId,
    pub calibration: UnitCalibration,
    pub qualification: QualifiedEnvelope,
    pub timing: Timing,
}
impl Config {
    pub fn validate(&self) -> io::Result<Validated> {
        for path in [&self.device, &self.lock_directory] {
            bounded_path(path)?;
        }
        if !(1_000..=630_000_000).contains(&self.maximum_runtime_us)
            || self.expected_device.inode == 0
            || self.expected_device.character_device == 0
        {
            return Err(invalid("invalid lifetime or provisioned device identity"));
        }
        let unit = UnitId::new(&self.unit).map_err(invalid)?;
        let mut records = Vec::with_capacity(5);
        for c in self.calibration {
            records.push(CalibrationRecord {
                joint: Joint::try_from(c.joint).map_err(invalid)?,
                id: c.id,
                drive_mode: c.drive_mode,
                homing_offset: c.homing_offset,
                range_min: c.range_min,
                range_max: c.range_max,
            });
        }
        let registers = self
            .register_binding
            .map(|r| {
                Ok(RegisterBindingRecord {
                    joint: Joint::try_from(r.joint).map_err(invalid)?,
                    minimum: r.minimum,
                    maximum: r.maximum,
                    homing_offset: r.homing_offset,
                })
            })
            .into_iter()
            .collect::<io::Result<Vec<_>>>()?;
        let calibration = UnitCalibration::with_register_bindings(
            unit.clone(),
            records.try_into().expect("five records"),
            registers.try_into().expect("five register bindings"),
        )
        .map_err(invalid)?;
        let mut limits = Vec::with_capacity(5);
        for e in self.qualification.joints {
            limits.push(
                JointEnvelope::new(
                    NormalizedPosition::new(e.minimum).map_err(invalid)?,
                    NormalizedPosition::new(e.maximum).map_err(invalid)?,
                    e.velocity,
                    e.acceleration,
                    RawProfile::new(e.raw_acceleration, e.raw_goal_time, e.raw_speed)
                        .map_err(invalid)?,
                    e.maximum_load_raw,
                    e.maximum_velocity_raw,
                    e.tracking_error,
                    e.settling_error,
                )
                .map_err(invalid)?,
            );
        }
        let qualification = QualifiedEnvelope::new(
            self.qualification.record,
            calibration.clone(),
            limits.try_into().expect("five envelopes"),
            MonoTime::from_micros(self.qualification.valid_from_us),
            MonoTime::from_micros(self.qualification.expires_at_us),
        )
        .map_err(invalid)?;
        let b = self.timing;
        let timing = Timing::new(
            b.transaction_us,
            b.io_us,
            b.readback_age_us,
            b.placement_age_us,
            b.interval_us,
        )
        .map_err(invalid)?;
        Ok(Validated {
            unit,
            calibration,
            qualification,
            timing,
        })
    }
    /// A fresh bounded regular file, no symlink/FIFO or unbounded read. The
    /// supervisor must freeze provisioning before spawning the child.
    pub fn read(path: &Path) -> io::Result<Self> {
        bounded_path(path)?;
        use rustix::fs::{Mode, OFlags};
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK | OFlags::NOFOLLOW,
            Mode::empty(),
        )?;
        let file = File::from(fd);
        let meta = file.metadata()?;
        if !meta.is_file()
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o077 != 0
            || meta.nlink() != 1
            || meta.len() > 16_384
        {
            return Err(invalid(
                "provisioning requires a private single-link regular file <=16384 bytes",
            ));
        }
        let mut bytes = Vec::new();
        file.take(16_385).read_to_end(&mut bytes)?;
        if bytes.len() > 16_384 {
            return Err(invalid("provisioning file grew beyond limit"));
        }
        let config: Self = serde_json::from_slice(&bytes).map_err(invalid)?;
        config.validate()?;
        Ok(config)
    }
}
pub fn bounded_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.to_str().is_none()
        || path.components().count() > 64
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(invalid(
            "expected a bounded absolute path without parent traversal",
        ));
    }
    Ok(())
}
pub(crate) fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}
