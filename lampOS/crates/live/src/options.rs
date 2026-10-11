//! Explicit local experiment settings. AEC is mandatory. Turn-taking changes
//! prospective admission only, not provider settings, gains, or hard authority.
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf, str::FromStr};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoiseSuppression {
    #[default]
    On,
    Off,
}
impl NoiseSuppression {
    pub fn enabled(self) -> bool {
        self == Self::On
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }
    pub fn software_processing(self) -> SoftwareProcessing {
        SoftwareProcessing {
            aec: SoftwareAec::SonoraAec3,
            noise_suppression: self.enabled(),
        }
    }
}
impl FromStr for NoiseSuppression {
    type Err = io::Error;
    fn from_str(value: &str) -> io::Result<Self> {
        match value {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "noise suppression must be exactly on or off; AEC remains enabled",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SoftwareAec {
    SonoraAec3,
}

/// Immutable provenance for the actual capture pipeline. Missing provenance in
/// a generic recorder or historical file does not imply either mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SoftwareProcessing {
    aec: SoftwareAec,
    noise_suppression: bool,
}

/// Admission experiment only; neither mode identifies a person or addressee.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnTaking {
    #[default]
    VadOnly,
    V1Baseline,
}
impl FromStr for TurnTaking {
    type Err = io::Error;
    fn from_str(value: &str) -> io::Result<Self> {
        match value {
            "vad-only" => Ok(Self::VadOnly),
            "v1-baseline" => Ok(Self::V1Baseline),
            _ => Err(invalid_options()),
        }
    }
}

/// Endpoint experiment. `smart-turn` confirms V1-baseline pauses with the
/// separate `lamp-endpoint-worker` process and its pinned model, both
/// installed beside this executable, and keeps the silence window as the
/// fallback for every pause without a usable verdict. It is never a default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Endpoint {
    #[default]
    Silence,
    SmartTurn,
    /// `smart-turn` plus a recoverable early end of input: a confident verdict
    /// ends input at 500 ms while reply output waits for the baseline quiet.
    /// Recovering a continuation depends on the provider settling the request
    /// it interrupts (docs/endpoint-detection.md).
    SmartTurnEarly,
}
impl FromStr for Endpoint {
    type Err = io::Error;
    fn from_str(value: &str) -> io::Result<Self> {
        match value {
            "silence" => Ok(Self::Silence),
            "smart-turn" => Ok(Self::SmartTurn),
            "smart-turn-early" => Ok(Self::SmartTurnEarly),
            _ => Err(invalid_options()),
        }
    }
}

/// Experimental coordinator horizon; zero preserves immediate dispatch.
/// This is not an ALSA queue size or a promise of gap-free playback.
///
/// The continuity start is the other choice, not an addition to a horizon:
/// its own cover takes the horizon's place. Neither is selected by default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayoutStart {
    horizon_ms: u16,
    continuity: bool,
}
impl PlayoutStart {
    pub fn new(milliseconds: u16) -> io::Result<Self> {
        if milliseconds > 200 {
            return Err(invalid_options());
        }
        Ok(Self {
            horizon_ms: milliseconds,
            continuity: false,
        })
    }
    /// The offline-evaluated continuity start. Not a measured device setting.
    pub const fn continuity() -> Self {
        Self {
            horizon_ms: 0,
            continuity: true,
        }
    }
    pub fn policy(self) -> Option<lamp_audio::playout::Continuity> {
        self.continuity
            .then_some(lamp_audio::playout::Continuity::EVALUATED)
    }
    /// The plain horizon, or the continuity start's cover.
    pub fn as_micros(self) -> u64 {
        self.gate().horizon_us()
    }
    pub fn gate(self) -> lamp_audio::playout::StartGate {
        match self.policy() {
            Some(policy) => lamp_audio::playout::StartGate::continuous(policy),
            None => lamp_audio::playout::StartGate::new(u64::from(self.horizon_ms) * 1_000),
        }
    }
}
impl FromStr for PlayoutStart {
    type Err = io::Error;
    fn from_str(value: &str) -> io::Result<Self> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid_options());
        }
        Self::new(value.parse().map_err(|_| invalid_options())?)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectedOptions {
    pub playout_start: PlayoutStart,
    pub turn_taking: TurnTaking,
    pub endpoint: Endpoint,
    pub diagnostics: bool,
    pub noise_suppression: NoiseSuppression,
    /// Explicit qualification opt-in. None leaves the ring device unopened.
    pub ring_channel_ceiling: Option<u16>,
}
impl DirectedOptions {
    pub fn parse(arguments: &[String]) -> io::Result<Self> {
        let mut options = Self::default();
        let mut noise_seen = false;
        let mut turn_seen = false;
        let mut endpoint_seen = false;
        let mut playout_seen = false;
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--diagnostics" if !options.diagnostics => options.diagnostics = true,
                "--playout-start-ms" if !playout_seen => {
                    options.playout_start = required_value(&mut arguments)?.parse()?;
                    playout_seen = true;
                }
                "--playout-continuity" if !playout_seen => {
                    options.playout_start = PlayoutStart::continuity();
                    playout_seen = true;
                }
                "--turn-taking" if !turn_seen => {
                    options.turn_taking = required_value(&mut arguments)?.parse()?;
                    turn_seen = true;
                }
                "--endpoint" if !endpoint_seen => {
                    options.endpoint = required_value(&mut arguments)?.parse()?;
                    endpoint_seen = true;
                }
                "--noise-suppression" if !noise_seen => {
                    options.noise_suppression = required_value(&mut arguments)?.parse()?;
                    noise_seen = true;
                }
                "--ring-channel-ceiling" if options.ring_channel_ceiling.is_none() => {
                    let value = required_value(&mut arguments)?
                        .parse::<u16>()
                        .map_err(|_| invalid_options())?;
                    lamp_ring::ChannelCeiling::new(value).map_err(io::Error::other)?;
                    options.ring_channel_ceiling = Some(value);
                }
                _ => return Err(invalid_options()),
            }
        }
        // Confirmation is defined against the V1 baseline's silence fallback.
        if options.endpoint != Endpoint::Silence && options.turn_taking != TurnTaking::V1Baseline {
            return Err(invalid_options());
        }
        Ok(options)
    }
}

/// Optional observation of a finite conversation. Cues contain lifecycle
/// metadata only; enabling them does not enable microphone PCM diagnostics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionOptions {
    pub directed: DirectedOptions,
    pub cue_socket: Option<PathBuf>,
    /// Opt-in direct camera context; absent means no camera process or device.
    pub camera_config: Option<PathBuf>,
}
impl SessionOptions {
    pub fn validate_serve(&self) -> io::Result<()> {
        if self.directed.diagnostics {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "serve rejects unbounded raw audio diagnostics; use a finite directed session",
            ));
        }
        Ok(())
    }

    pub fn parse(arguments: &[String]) -> io::Result<Self> {
        let mut remaining = Vec::new();
        let mut cue_socket = None;
        let mut camera_config = None;
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            if matches!(argument.as_str(), "--cue-socket" | "--camera-config") {
                let slot = if argument == "--cue-socket" {
                    &mut cue_socket
                } else {
                    &mut camera_config
                };
                if slot.is_some() {
                    return Err(invalid_options());
                }
                let path = PathBuf::from(required_value(&mut arguments)?);
                if !path.is_absolute() {
                    return Err(invalid_options());
                }
                *slot = Some(path);
            } else {
                remaining.push(argument.clone());
            }
        }
        Ok(Self {
            directed: DirectedOptions::parse(&remaining)?,
            cue_socket,
            camera_config,
        })
    }
}

/// Internal child CLI: only named options can identify mode or diagnostic path.
/// Parsing finishes before IPC binding, recorder creation or device opening.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AudioWorkerOptions {
    pub diagnostic_directory: Option<PathBuf>,
    pub noise_suppression: NoiseSuppression,
}
impl AudioWorkerOptions {
    /// The exact argv tail used by the supervisor. Values remain individual
    /// arguments (including paths with spaces); no shell quoting is involved.
    pub fn child_arguments(&self, device: &str, is_capture: bool) -> io::Result<Vec<String>> {
        if device.is_empty() || (!is_capture && self.noise_suppression != NoiseSuppression::On) {
            return Err(invalid_options());
        }
        let mut arguments = vec![device.to_owned()];
        if is_capture {
            arguments.extend([
                "--noise-suppression".to_owned(),
                self.noise_suppression.as_str().to_owned(),
            ]);
        }
        if let Some(path) = &self.diagnostic_directory {
            if !path.is_absolute() {
                return Err(invalid_options());
            }
            arguments.extend([
                "--diagnostics".to_owned(),
                path.to_str().ok_or_else(invalid_options)?.to_owned(),
            ]);
        }
        Ok(arguments)
    }

    pub fn parse(arguments: &[String], is_capture: bool) -> io::Result<Self> {
        let mut options = Self::default();
        let mut noise_seen = false;
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--diagnostics" if options.diagnostic_directory.is_none() => {
                    let path = PathBuf::from(required_value(&mut arguments)?);
                    if !path.is_absolute() {
                        return Err(invalid_options());
                    }
                    options.diagnostic_directory = Some(path);
                }
                "--noise-suppression" if is_capture && !noise_seen => {
                    options.noise_suppression = required_value(&mut arguments)?.parse()?;
                    noise_seen = true;
                }
                _ => return Err(invalid_options()),
            }
        }
        Ok(options)
    }
}

fn required_value<'a>(arguments: &mut impl Iterator<Item = &'a String>) -> io::Result<&'a str> {
    arguments
        .next()
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(invalid_options)
}
fn invalid_options() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "missing, duplicate or unknown audio experiment option",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }
    #[test]
    fn service_rejects_raw_recording_but_preserves_explicit_audio_camera_ring_options() {
        assert!(SessionOptions::default().validate_serve().is_ok());
        assert!(
            SessionOptions::parse(&args(&["--diagnostics"]))
                .unwrap()
                .validate_serve()
                .is_err()
        );
        let options = SessionOptions::parse(&args(&[
            "--camera-config",
            "/tmp/camera.json",
            "--ring-channel-ceiling",
            "24",
            "--turn-taking",
            "v1-baseline",
            "--playout-start-ms",
            "200",
        ]))
        .unwrap();
        assert!(options.validate_serve().is_ok());
        assert_eq!(options.directed.ring_channel_ceiling, Some(24));
    }
    #[test]
    fn startup_buffer_defaults_to_zero_and_bounds_cli_and_programmatic_callers() {
        assert_eq!(
            SessionOptions::default().directed.playout_start.as_micros(),
            0
        );
        for ms in [0_u16, 1, 120, 160, 200] {
            let parsed = SessionOptions::parse(&args(&[
                "--playout-start-ms",
                &ms.to_string(),
                "--turn-taking",
                "v1-baseline",
                "--noise-suppression",
                "off",
            ]))
            .unwrap();
            assert_eq!(
                parsed.directed.playout_start.as_micros(),
                u64::from(ms) * 1_000
            );
            assert_eq!(parsed.directed.turn_taking, TurnTaking::V1Baseline);
            assert!(!parsed.directed.diagnostics);
        }
        assert!(PlayoutStart::new(201).is_err());
        assert!(PlayoutStart::new(u16::MAX).is_err());
        for flags in [
            vec!["--playout-start-ms"],
            vec!["--playout-start-ms", "201"],
            vec!["--playout-start-ms", "65536"],
            vec!["--playout-start-ms", "-1"],
            vec!["--playout-start-ms", "+1"],
            vec!["--playout-start-ms", "0.2"],
            vec!["--playout-start-ms", "--diagnostics"],
            vec!["--playout-start-ms", "0", "--playout-start-ms", "200"],
        ] {
            assert!(SessionOptions::parse(&args(&flags)).is_err(), "{flags:?}");
        }
        assert!(AudioWorkerOptions::parse(&args(&["--playout-start-ms", "200"]), true).is_err());
        assert!(AudioWorkerOptions::parse(&args(&["--playout-start-ms", "200"]), false).is_err());
    }

    #[test]
    fn smart_turn_endpoint_is_explicit_and_requires_the_v1_baseline() {
        assert_eq!(
            SessionOptions::default().directed.endpoint,
            Endpoint::Silence
        );
        let options = SessionOptions::parse(&args(&[
            "--turn-taking",
            "v1-baseline",
            "--endpoint",
            "smart-turn",
        ]))
        .unwrap();
        assert_eq!(options.directed.endpoint, Endpoint::SmartTurn);
        let early = SessionOptions::parse(&args(&[
            "--turn-taking",
            "v1-baseline",
            "--endpoint",
            "smart-turn-early",
        ]))
        .unwrap();
        assert_eq!(early.directed.endpoint, Endpoint::SmartTurnEarly);
        assert!(
            SessionOptions::parse(&args(&["--endpoint", "smart-turn-early"])).is_err(),
            "early end requires the V1 baseline"
        );
        assert_eq!(
            serde_json::to_value(options.directed.endpoint).unwrap(),
            "smart-turn"
        );
        for values in [
            vec!["--endpoint", "smart-turn"],
            vec!["--turn-taking", "vad-only", "--endpoint", "smart-turn"],
            vec!["--turn-taking", "v1-baseline", "--endpoint"],
            vec!["--turn-taking", "v1-baseline", "--endpoint", "Smart-Turn"],
            vec![
                "--turn-taking",
                "v1-baseline",
                "--endpoint",
                "silence",
                "--endpoint",
                "smart-turn",
            ],
        ] {
            assert!(SessionOptions::parse(&args(&values)).is_err(), "{values:?}");
        }
    }

    #[test]
    fn continuity_is_explicit_and_takes_the_place_of_a_horizon() {
        use lamp_audio::playout::Continuity;
        let default = SessionOptions::default().directed.playout_start;
        assert_eq!(
            (default.policy(), default.gate().continuity()),
            (None, None)
        );
        for ms in [0_u16, 200] {
            let plain = PlayoutStart::new(ms).unwrap();
            assert_eq!(plain.policy(), None);
            assert_eq!(plain.gate().horizon_us(), u64::from(ms) * 1_000);
        }
        let parsed = SessionOptions::parse(&args(&[
            "--playout-continuity",
            "--turn-taking",
            "v1-baseline",
        ]))
        .unwrap()
        .directed;
        assert_eq!(parsed.playout_start, PlayoutStart::continuity());
        assert_eq!(parsed.playout_start.policy(), Some(Continuity::EVALUATED));
        assert_eq!(
            parsed.playout_start.gate().continuity(),
            Some(Continuity::EVALUATED)
        );
        // The trace's horizon field reports the cover that replaced it.
        assert_eq!(
            parsed.playout_start.as_micros(),
            Continuity::EVALUATED.cover_us
        );
        assert_eq!(parsed.turn_taking, TurnTaking::V1Baseline);
        for flags in [
            vec!["--playout-continuity", "--playout-continuity"],
            vec!["--playout-continuity", "--playout-start-ms", "200"],
            vec!["--playout-start-ms", "0", "--playout-continuity"],
            vec!["--playout-continuity", "on"],
            vec!["--playout-continuity=on"],
        ] {
            assert!(SessionOptions::parse(&args(&flags)).is_err(), "{flags:?}");
        }
        assert!(AudioWorkerOptions::parse(&args(&["--playout-continuity"]), true).is_err());
        assert!(AudioWorkerOptions::parse(&args(&["--playout-continuity"]), false).is_err());
    }

    #[test]
    fn turn_taking_is_explicit_and_default_remains_vad_only() {
        assert_eq!(
            SessionOptions::default().directed.turn_taking,
            TurnTaking::VadOnly
        );
        let options = SessionOptions::parse(&args(&[
            "--turn-taking",
            "v1-baseline",
            "--noise-suppression",
            "off",
            "--cue-socket",
            "/tmp/test-only.sock",
        ]))
        .unwrap();
        assert_eq!(options.directed.turn_taking, TurnTaking::V1Baseline);
        assert_eq!(options.directed.noise_suppression, NoiseSuppression::Off);
        assert!(!options.directed.diagnostics);
        assert_eq!(options.directed.ring_channel_ceiling, None);
        assert_eq!(
            serde_json::to_value(options.directed.turn_taking).unwrap(),
            "v1-baseline"
        );
        for values in [
            vec!["--turn-taking"],
            vec!["--turn-taking", "unknown"],
            vec!["--turn-taking", "V1-baseline"],
            vec!["--turn-taking", "vad-only", "--turn-taking", "v1-baseline"],
        ] {
            assert!(SessionOptions::parse(&args(&values)).is_err());
        }
    }
    #[test]
    fn default_is_the_existing_aec_plus_noise_suppression_pipeline() {
        let options = DirectedOptions::parse(&[]).unwrap();
        assert_eq!(options, DirectedOptions::default());
        assert!(options.noise_suppression.enabled());
        assert!(!options.diagnostics);
        assert_eq!(options.ring_channel_ceiling, None);
        assert_eq!(
            serde_json::to_value(options.noise_suppression.software_processing()).unwrap(),
            serde_json::json!({"aec":"sonora_aec3","noise_suppression":true})
        );
    }
    #[test]
    fn session_cues_are_optional_metadata_without_enabling_audio_recording() {
        assert_eq!(
            SessionOptions::parse(&[]).unwrap(),
            SessionOptions::default()
        );
        let options = SessionOptions::parse(&args(&[
            "--cue-socket",
            "/tmp/private session/cue.sock",
            "--noise-suppression",
            "off",
            "--ring-channel-ceiling",
            "24",
        ]))
        .unwrap();
        assert_eq!(
            options.cue_socket,
            Some(PathBuf::from("/tmp/private session/cue.sock"))
        );
        assert_eq!(options.directed.noise_suppression, NoiseSuppression::Off);
        assert_eq!(options.directed.ring_channel_ceiling, Some(24));
        assert!(!options.directed.diagnostics);
    }
    #[test]
    fn session_cues_reject_missing_duplicate_relative_and_flag_values() {
        for values in [
            vec!["--cue-socket"],
            vec!["--cue-socket", ""],
            vec!["--cue-socket", "relative.sock"],
            vec!["--cue-socket", "--diagnostics"],
            vec!["--cue-socket", "/tmp/a", "--cue-socket", "/tmp/b"],
            vec!["--cue-socket", "/tmp/a", "--unknown"],
            vec!["--cue-socket", "/tmp/a", "--noise-suppression", "yes"],
        ] {
            assert!(SessionOptions::parse(&args(&values)).is_err(), "{values:?}");
        }
    }
    #[test]
    fn ring_is_explicit_bounded_and_independent_of_audio_treatment() {
        for ceiling in [0, 24, 120] {
            let options = DirectedOptions::parse(&args(&[
                "--ring-channel-ceiling",
                &ceiling.to_string(),
                "--noise-suppression",
                "off",
                "--diagnostics",
            ]))
            .unwrap();
            assert_eq!(options.ring_channel_ceiling, Some(ceiling));
            assert_eq!(options.noise_suppression, NoiseSuppression::Off);
            assert!(options.diagnostics);
        }
        for flags in [
            vec!["--ring-channel-ceiling"],
            vec!["--ring-channel-ceiling", "121"],
            vec!["--ring-channel-ceiling", "65536"],
            vec!["--ring-channel-ceiling", "-1"],
            vec![
                "--ring-channel-ceiling",
                "24",
                "--ring-channel-ceiling",
                "24",
            ],
            vec!["--ring-channel-ceiling", "--diagnostics"],
        ] {
            assert!(DirectedOptions::parse(&args(&flags)).is_err());
        }
    }
    #[test]
    fn both_modes_are_orthogonal_to_diagnostics_in_either_order() {
        for mode in [NoiseSuppression::On, NoiseSuppression::Off] {
            for flags in [
                vec!["--noise-suppression", mode.as_str(), "--diagnostics"],
                vec!["--diagnostics", "--noise-suppression", mode.as_str()],
            ] {
                assert_eq!(
                    DirectedOptions::parse(&args(&flags)).unwrap(),
                    DirectedOptions {
                        playout_start: PlayoutStart::default(),
                        turn_taking: TurnTaking::VadOnly,
                        endpoint: Endpoint::Silence,
                        diagnostics: true,
                        noise_suppression: mode,
                        ring_channel_ceiling: None,
                    }
                );
            }
            let only =
                DirectedOptions::parse(&args(&["--noise-suppression", mode.as_str()])).unwrap();
            assert!(!only.diagnostics);
            assert_eq!(only.noise_suppression, mode);
            assert_eq!(
                serde_json::to_value(mode.software_processing()).unwrap(),
                serde_json::json!({"aec":"sonora_aec3","noise_suppression":mode.enabled()})
            );
        }
    }
    #[test]
    fn invalid_or_ambiguous_flags_fail_before_any_startup_work() {
        for flags in [
            vec!["--noise-suppression"],
            vec!["--noise-suppression", "false"],
            vec!["--noise-suppression", "OFF"],
            vec!["--noise-suppression", "--diagnostics"],
            vec!["--noise-suppression", "off", "--noise-suppression", "on"],
            vec!["--diagnostics", "--diagnostics"],
            vec!["--aec", "off"],
            vec!["off"],
            vec!["/tmp/audio"],
            vec!["--diagnostics", "/tmp/audio"],
        ] {
            assert!(DirectedOptions::parse(&args(&flags)).is_err(), "{flags:?}");
        }
    }
    #[test]
    fn child_options_are_explicit_and_render_cannot_claim_capture_processing() {
        for mode in [NoiseSuppression::On, NoiseSuppression::Off] {
            let parsed = AudioWorkerOptions::parse(
                &args(&[
                    "--noise-suppression",
                    mode.as_str(),
                    "--diagnostics",
                    "/tmp/new-capture",
                ]),
                true,
            )
            .unwrap();
            assert_eq!(parsed.noise_suppression, mode);
            assert_eq!(
                parsed.diagnostic_directory,
                Some(PathBuf::from("/tmp/new-capture"))
            );
        }
        assert_eq!(
            AudioWorkerOptions::parse(&[], true)
                .unwrap()
                .noise_suppression,
            NoiseSuppression::On
        );
        assert!(AudioWorkerOptions::parse(&args(&["--noise-suppression", "off"]), false).is_err());
        for flags in [
            vec!["off"],
            vec!["/tmp/new-capture"],
            vec!["--diagnostics"],
            vec!["--diagnostics", "relative"],
            vec!["--diagnostics", "--noise-suppression", "off"],
            vec!["--diagnostics", "/tmp/a", "--diagnostics", "/tmp/b"],
        ] {
            assert!(
                AudioWorkerOptions::parse(&args(&flags), true).is_err(),
                "{flags:?}"
            );
        }
    }

    #[test]
    fn supervisor_argv_preserves_requested_capture_mode_and_separate_render_path() {
        for mode in [NoiseSuppression::On, NoiseSuppression::Off] {
            for directory in [None, Some(PathBuf::from("/tmp/private run/capture-audio"))] {
                let requested = AudioWorkerOptions {
                    diagnostic_directory: directory,
                    noise_suppression: mode,
                };
                let arguments = requested
                    .child_arguments("plug:device_micro2", true)
                    .unwrap();
                assert_eq!(arguments[0], "plug:device_micro2");
                let actual = AudioWorkerOptions::parse(&arguments[1..], true).unwrap();
                assert_eq!(actual, requested);
            }
        }
        let render = AudioWorkerOptions {
            diagnostic_directory: Some(PathBuf::from("/tmp/private run/render-audio")),
            ..AudioWorkerOptions::default()
        };
        let arguments = render
            .child_arguments("plug:device_speaker", false)
            .unwrap();
        assert_eq!(
            arguments,
            args(&[
                "plug:device_speaker",
                "--diagnostics",
                "/tmp/private run/render-audio"
            ])
        );
        assert_eq!(
            AudioWorkerOptions::parse(&arguments[1..], false).unwrap(),
            render
        );
    }
}
