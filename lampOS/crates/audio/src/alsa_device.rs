//! The ALSA calls behind [`crate::device`]. Each method is the call the
//! boundary made directly before the trait existed, and nothing more.
use crate::{
    CAPTURE_RATE, CAPTURE_SAMPLES,
    capture::{CaptureConfig, CaptureFormat, valid_format},
    device::{
        CaptureDevice, CaptureStatus, ConfigureError, DeviceFault, DeviceState, timestamp_us,
    },
};
use alsa::{
    Direction, ValueOr,
    pcm::{Access, Format, HwParams, PCM, State, TstampType},
};

fn fault(error: alsa::Error) -> DeviceFault {
    DeviceFault::new(error.func(), error.errno())
}

fn state(state: State) -> DeviceState {
    match state {
        State::Setup => DeviceState::Setup,
        State::Prepared => DeviceState::Prepared,
        State::Running => DeviceState::Running,
        State::XRun => DeviceState::XRun,
        State::Draining => DeviceState::Draining,
        State::Suspended => DeviceState::Suspended,
        State::Disconnected => DeviceState::Disconnected,
        _ => DeviceState::Other,
    }
}

pub struct AlsaCapture(PCM);

impl CaptureDevice for AlsaCapture {
    fn open(name: &str) -> Result<Self, DeviceFault> {
        PCM::new(name, Direction::Capture, true)
            .map(Self)
            .map_err(fault)
    }

    fn configure(&mut self, config: CaptureConfig) -> Result<CaptureFormat, ConfigureError> {
        let pcm = &self.0;
        let failed = |error: alsa::Error| ConfigureError::Fault(fault(error));
        let hardware = HwParams::any(pcm).map_err(failed)?;
        hardware.set_access(Access::RWInterleaved).map_err(failed)?;
        hardware.set_format(Format::s16()).map_err(failed)?;
        hardware.set_channels(1).map_err(failed)?;
        hardware
            .set_rate(CAPTURE_RATE, ValueOr::Nearest)
            .map_err(failed)?;
        hardware
            .set_period_size_near(CAPTURE_SAMPLES as i64, ValueOr::Nearest)
            .map_err(failed)?;
        let requested_buffer = config.requested_frames();
        hardware
            .set_buffer_size_near(requested_buffer)
            .map_err(failed)?;
        pcm.hw_params(&hardware).map_err(failed)?;
        let current = pcm.hw_params_current().map_err(failed)?;
        let mut format = CaptureFormat {
            rate: current.get_rate().map_err(failed)?,
            channels: current.get_channels().map_err(failed)?,
            period_frames: current.get_period_size().map_err(failed)?,
            buffer_frames: current.get_buffer_size().map_err(failed)?,
            monotonic_status_timestamps: false,
        };
        if current.get_format().map_err(failed)? != Format::s16()
            || current.get_access().map_err(failed)? != Access::RWInterleaved
            || !valid_format(format, requested_buffer)
        {
            return Err(ConfigureError::Unsupported(format));
        }
        let software = pcm.sw_params_current().map_err(failed)?;
        software.set_avail_min(1).map_err(failed)?;
        software.set_start_threshold(1).map_err(failed)?;
        // Timestamp support is optional; inability is reported in negotiated format.
        let timestamps = software.set_tstamp_type(TstampType::Monotonic).is_ok()
            && software.set_tstamp_mode(true).is_ok();
        if !timestamps {
            software.set_tstamp_mode(false).map_err(failed)?;
        }
        pcm.sw_params(&software).map_err(failed)?;
        let installed = pcm.sw_params_current().map_err(failed)?;
        format.monotonic_status_timestamps = installed.get_tstamp_mode().map_err(failed)?
            && installed.get_tstamp_type().map_err(failed)? == TstampType::Monotonic;
        Ok(format)
    }

    fn start(&mut self) -> Result<(), DeviceFault> {
        self.0.start().map_err(fault)
    }

    fn read(&mut self, into: &mut [i16]) -> Result<usize, DeviceFault> {
        self.0
            .io_i16()
            .and_then(|reader| reader.readi(into))
            .map_err(fault)
    }

    fn status(&mut self) -> Result<CaptureStatus, DeviceFault> {
        let status = self.0.status().map_err(fault)?;
        let stamp = status.get_htstamp();
        Ok(CaptureStatus {
            state: state(status.get_state()),
            monotonic_us: timestamp_us(stamp.tv_sec, stamp.tv_nsec),
            available: status.get_avail(),
            delayed: status.get_delay(),
        })
    }

    fn discard(&mut self) -> Result<(), DeviceFault> {
        self.0.drop().map_err(fault)
    }
}
