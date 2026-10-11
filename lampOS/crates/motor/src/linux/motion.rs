//! Opt-in Linux adapter for the finite, guarded position writer.
//!
//! Opening configures a tty but sends no protocol bytes. The opt-in supervised
//! motor worker uses this adapter. Its supervisor must exclude pre-existing handles and
//! privileged competing writers: TIOCEXCL cannot evict them. It must also qualify
//! USB open/close effects; absence of a modem-control ioctl is not that evidence.

use super::{
    CleanupReport, DeviceIdentity, Diagnostic, InspectionConfig, InspectionResult, Owner,
    SerialSettings, empty_result, invalid, micros,
};
use crate::{
    JOINT_COUNT,
    calibration::UnitCalibration,
    control::{
        self, BusPort, Clock, MotionId, MotorControl, Placement, QualifiedEnvelope, Tick, Timing,
    },
    units::NormalizedPosition,
};
use lamp_interaction::{BootId, OutputPermit, Snapshot};
use rustix::termios::{self, QueueSelector};
use std::{fmt, io, path::PathBuf, time::Instant};

/// Host serial evidence only. Neither identity labels nor exclusivity settings
/// certify mechanical placement, adapter effects, or exclusion of old handles.
#[derive(Clone, Debug)]
pub struct OpenReport {
    pub requested_device: PathBuf,
    pub resolved_device: Option<PathBuf>,
    pub device_identity: Option<DeviceIdentity>,
    pub prior_settings: Option<SerialSettings>,
    pub negotiated_settings: Option<SerialSettings>,
    pub tty_exclusive_applied: bool,
    pub elapsed_us: u64,
}

impl OpenReport {
    fn from_inspection(report: &InspectionResult, origin: Instant) -> Self {
        Self {
            requested_device: report.requested_device.clone(),
            resolved_device: report.resolved_device.clone(),
            device_identity: report.device_identity,
            prior_settings: report.prior_settings.clone(),
            negotiated_settings: report.negotiated_settings.clone(),
            tty_exclusive_applied: report.tty_exclusive_applied,
            elapsed_us: micros(origin),
        }
    }
}

#[derive(Debug)]
pub struct OpenError {
    pub failure: Diagnostic,
    pub serial: OpenReport,
    pub cleanup: CleanupReport,
}
impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "motor open {}: {}",
            self.failure.operation, self.failure.detail
        )
    }
}
impl std::error::Error for OpenError {}

/// Explicit cleanup evidence. `local` does not assert a physical halt; `serial`
/// reports settings restoration (with HUPCL still cleared), lock release/close.
#[derive(Debug)]
pub struct ShutdownReport {
    pub local: control::StopReport,
    pub serial: CleanupReport,
}

/// Non-cloneable bus owner. Its port and fd cannot escape to bypass the guard.
/// Caller services priority control before each tick; no background work or
/// gesture queue can delay `cancel`, `control_lost`, or `shutdown`.
pub struct LinuxMotor<C: Clock> {
    control: MotorControl<MotionPort, C>,
}

impl<C: Clock> LinuxMotor<C> {
    /// Explicit opt-in only. Starts idle without reads, goals, pose or torque.
    /// `clock` must use the controller's OS monotonic domain. `cancelled` must be
    /// nonblocking; setup observes its 200 ms deadline between bounded syscalls,
    /// which still requires a supervisor if the kernel does not return.
    ///
    /// `expected_device` is the exact inode/rdev evidence from the provisioned
    /// device path, not a USB authentication token. Reprovision after replacement.
    #[allow(clippy::too_many_arguments)] // Explicit ownership and timing, no default device/config.
    pub fn open(
        config: &InspectionConfig,
        expected_device: DeviceIdentity,
        clock: C,
        controller_boot: BootId,
        worker_boot: BootId,
        calibration: UnitCalibration,
        timing: Timing,
        mut cancelled: impl FnMut() -> io::Result<bool>,
    ) -> Result<(Self, OpenReport), Box<OpenError>> {
        let origin = Instant::now();
        let mut report = empty_result(config);
        let mut owner = Owner::default();
        let prepared = (|| {
            calibration.bind(&config.unit_id).map_err(|_| {
                Diagnostic::new(
                    "calibration_identity",
                    invalid("calibration belongs to another unit"),
                )
            })?;
            owner.prepare(
                config,
                Some(expected_device),
                &mut report,
                origin,
                &mut cancelled,
            )?;
            if report.device_identity != Some(expected_device) {
                return Err(Diagnostic::new(
                    "expected_device_identity",
                    invalid("opened device identity differs from provisioned identity"),
                ));
            }
            super::checkpoint(origin, &mut cancelled)
                .map_err(|error| Diagnostic::new("final_open_checkpoint", error))
        })();
        if let Err(failure) = prepared {
            let cleanup = owner.finish(origin, true);
            return Err(Box::new(OpenError {
                failure,
                serial: OpenReport::from_inspection(&report, origin),
                cleanup,
            }));
        }
        let serial = OpenReport::from_inspection(&report, origin);
        // The same immutable calibration and unit passed the check above. The
        // constructor has no other failure path and starts without device I/O.
        let control = MotorControl::new(
            MotionPort { owner, origin },
            clock,
            controller_boot,
            worker_boot,
            &config.unit_id,
            calibration,
            timing,
        )
        .expect("calibration identity was checked before opening");
        Ok((Self { control }, serial))
    }

    pub fn install(&mut self, snapshot: Snapshot) -> Result<(), control::Error> {
        self.control.install(snapshot)
    }
    pub fn qualify(&mut self, evidence: QualifiedEnvelope) -> Result<(), control::Error> {
        self.control.qualify(evidence)
    }
    pub fn inspect(&mut self) -> Result<(), control::Error> {
        self.control.inspect()
    }
    pub fn start_segment(
        &mut self,
        permit: OutputPermit,
        placement: Placement,
        target: [NormalizedPosition; JOINT_COUNT],
        duration_us: u64,
    ) -> Result<MotionId, control::Error> {
        self.control
            .start_segment(permit, placement, target, duration_us)
    }
    pub fn tick(&mut self) -> Result<Tick, control::Error> {
        self.control.tick()
    }
    pub fn cancel(&mut self, motion: MotionId) -> Result<control::StopReport, control::Error> {
        self.control.cancel(motion)
    }
    /// Inhibit local work and discard output before separately closing the tty.
    pub fn inhibit(&mut self) -> control::StopReport {
        self.control.shutdown()
    }

    pub fn control_lost(&mut self) -> control::StopReport {
        self.control.control_lost()
    }
    pub fn stop_report(&self) -> Option<control::StopReport> {
        self.control.stop_report()
    }

    /// Clear logical work and discard unsent bytes first, then restore and close
    /// the owned tty. No park, torque, retry, or new goal is part of cleanup.
    pub fn shutdown(mut self) -> ShutdownReport {
        let local = self.control.shutdown();
        let port = self.control.port_mut();
        let serial = port.owner.finish(port.origin, false);
        ShutdownReport { local, serial }
    }
}

struct MotionPort {
    owner: Owner,
    origin: Instant,
}
impl BusPort for MotionPort {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        rustix::io::read(self.owner.fd.as_ref().ok_or_else(closed)?, output).map_err(Into::into)
    }
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        rustix::io::write(self.owner.fd.as_ref().ok_or_else(closed)?, input).map_err(Into::into)
    }
    fn discard_output(&mut self) -> io::Result<()> {
        termios::tcflush(
            self.owner.fd.as_ref().ok_or_else(closed)?,
            QueueSelector::OFlush,
        )
        .map_err(Into::into)
    }
}
fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "owned motor tty is closed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_port_has_no_reopen_or_fallback_path() {
        let mut port = MotionPort {
            owner: Owner::default(),
            origin: Instant::now(),
        };
        let mut bytes = [0; 8];
        assert_eq!(
            port.read(&mut bytes).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            port.write(&bytes).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            port.discard_output().unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert!(port.owner.finish(port.origin, true).errors.is_empty());
        assert!(!port.owner.finish(port.origin, true).tty_close_attempted);
    }
}
