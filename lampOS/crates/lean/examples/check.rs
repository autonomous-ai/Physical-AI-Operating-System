//! Finite, read-only arithmetic check. This example has no actuator connection.

use lamp_expression::Catalogue;
use lamp_lean::{
    Angles, Clip, Geometry, Measurement, Model, MovementCheck, OffsetFrame, PoseCheck,
};
use lamp_msgs::Voice;
use serde::Serialize;
use std::{
    env,
    ffi::OsString,
    fs::File,
    io::{self, Read, Write},
    path::Path,
    process::ExitCode,
};

const GEOMETRY_BYTES: usize = 65_536;
const CATALOGUE_BYTES: usize = 131_072;
const SAMPLE_EVERY_US: u64 = 20_000;
const USAGE: &str = "check <geometry.json> <catalogue.json> <count1,count2,count3,count4,count5>";
const MASKS: [(&str, [bool; 5]); 2] = [
    ("all_five_joints", [true; 5]),
    (
        "current_body_head_and_yaw",
        [true, false, false, true, true],
    ),
];

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Checked { result: MovementCheck },
    Error { error: String },
}

impl Outcome {
    fn within_margin(&self) -> bool {
        matches!(self, Self::Checked { result } if result.within_margin())
    }
}

#[derive(Debug, Serialize)]
struct BehaviourCheck {
    behaviour: String,
    voice_state: Voice,
    repeat: bool,
    hold: bool,
    applied_mask_name: &'static str,
    applied_mask: [bool; 5],
    #[serde(flatten)]
    outcome: Outcome,
}

#[derive(Debug, Serialize)]
struct Report {
    scope: &'static str,
    unqualified: [&'static str; 8],
    declared_measurement: Measurement,
    geometry_label: String,
    joint_order: [&'static str; 5],
    input_encoder_counts: [u16; 5],
    input_encoder_pose_rad: Angles,
    baseline: PoseCheck,
    sample_every_us: u64,
    checks: Vec<BehaviourCheck>,
    model_within_margin_all_checks: bool,
    actuation_qualified: bool,
    qualification_note: &'static str,
}

fn parse_counts(text: &str) -> Result<([u16; 5], Angles), String> {
    let mut counts = [0; 5];
    let mut fields = text.split(',');
    for count in &mut counts {
        let field = fields
            .next()
            .ok_or("expected exactly five comma-separated encoder counts")?;
        if field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("encoder counts must be unsigned decimal integers".into());
        }
        *count = field.parse().map_err(|_| "encoder count exceeds u16")?;
    }
    if fields.next().is_some() {
        return Err("expected exactly five comma-separated encoder counts".into());
    }
    let angles = Angles::from_encoder_counts(counts).map_err(|error| error.to_string())?;
    Ok((counts, angles))
}

fn bounded_text(reader: impl Read, limit: usize) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read: {error}"))?;
    if bytes.len() > limit {
        return Err(format!("input exceeds {limit} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| "input must be UTF-8 JSON".into())
}

fn read_file(path: &Path, limit: usize) -> Result<String, String> {
    // This command accepts supplied ordinary files, not streams or devices.
    // Inspect before opening so an accidentally supplied FIFO is not awaited.
    let metadata = path.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("input must be a regular file".into());
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("opened input is not a regular file".into());
    }
    bounded_text(file, limit)
}

fn check(
    geometry: Geometry,
    catalogue: &Catalogue,
    counts: [u16; 5],
    pose: Angles,
) -> Result<Report, String> {
    catalogue.check().map_err(|error| error.to_string())?;
    let declared_measurement = geometry.measurement;
    let geometry_label = geometry.label.clone();
    let model = Model::new(geometry).map_err(|error| error.to_string())?;
    let baseline = model.check_pose(pose);
    let mut checks = Vec::new();
    for behaviour in &catalogue.behaviours {
        let frames: Vec<_> = behaviour
            .keyframes
            .iter()
            .map(|frame| OffsetFrame {
                at_us: frame.at_us,
                offsets: frame.offsets,
            })
            .collect();
        for &voice_state in &behaviour.states {
            for (applied_mask_name, applied_mask) in MASKS {
                let clip = Clip {
                    posture: catalogue.postures.for_voice(voice_state),
                    frames: &frames,
                    weight: behaviour.weight,
                    limits: catalogue.limits,
                    applied: applied_mask,
                    repeat: behaviour.repeat,
                };
                let outcome = match model.check_clip(pose, clip, SAMPLE_EVERY_US) {
                    Ok(result) => Outcome::Checked { result },
                    Err(error) => Outcome::Error {
                        error: error.to_string(),
                    },
                };
                checks.push(BehaviourCheck {
                    behaviour: behaviour.name.clone(),
                    voice_state,
                    repeat: behaviour.repeat,
                    hold: behaviour.hold,
                    applied_mask_name,
                    applied_mask,
                    outcome,
                });
            }
        }
    }
    let model_within_margin_all_checks =
        baseline.within_margin() && checks.iter().all(|check| check.outcome.within_margin());
    Ok(Report {
        scope: "isolated clip from first through final keyframe; one closed cycle if repeating",
        unqualified: [
            "entry from the supplied baseline to the initial clip offset",
            "exit or release after the final clip offset",
            "interruption and return blends",
            "gaze and changing absolute posture",
            "simultaneous behaviours and changing weights or audio levels",
            "Body clamps, smoothing, gesture arbitration and servo tracking",
            "dynamic stability, contact, collision and external forces",
            "actual joint zeros, directions, masses and contact footprint",
        ],
        declared_measurement,
        geometry_label,
        joint_order: ["base_yaw", "shoulder", "elbow", "head_roll", "head_pitch"],
        input_encoder_counts: counts,
        input_encoder_pose_rad: pose,
        baseline,
        sample_every_us: SAMPLE_EVERY_US,
        checks,
        model_within_margin_all_checks,
        actuation_qualified: false,
        qualification_note: "Arithmetic only. Stand-in geometry never qualifies actuation. A measured label and a passing model are not actuator approval.",
    })
}

fn run(mut args: impl Iterator<Item = OsString>) -> Result<Report, String> {
    let geometry_path = args.next().ok_or(USAGE)?;
    let catalogue_path = args.next().ok_or(USAGE)?;
    let counts = args.next().ok_or(USAGE)?;
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let (counts, pose) = parse_counts(counts.to_str().ok_or("counts must be UTF-8 decimal text")?)?;
    let geometry_text = read_file(Path::new(&geometry_path), GEOMETRY_BYTES)
        .map_err(|error| format!("geometry: {error}"))?;
    let catalogue_text = read_file(Path::new(&catalogue_path), CATALOGUE_BYTES)
        .map_err(|error| format!("catalogue: {error}"))?;
    let geometry =
        Geometry::from_json(&geometry_text).map_err(|error| format!("geometry: {error}"))?;
    let catalogue =
        Catalogue::from_json(&catalogue_text).map_err(|error| format!("catalogue: {error}"))?;
    check(geometry, &catalogue, counts, pose)
}

fn main() -> ExitCode {
    match run(env::args_os().skip(1)) {
        Err(error) => {
            eprintln!("lamp-lean check: {error}\nUsage: {USAGE}");
            ExitCode::from(2)
        }
        Ok(report) => {
            let mut stdout = io::stdout().lock();
            let printed = serde_json::to_writer_pretty(&mut stdout, &report)
                .map_err(io::Error::other)
                .and_then(|()| writeln!(stdout));
            if let Err(error) = printed {
                eprintln!("lamp-lean check: stdout: {error}");
                return ExitCode::from(2);
            }
            ExitCode::from(u8::from(!report.model_within_margin_all_checks))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        Geometry::from_json(include_str!("../geometry.stand-in.json")).unwrap()
    }

    fn standing() -> ([u16; 5], Angles) {
        parse_counts("2048,2048,2048,2048,2048").unwrap()
    }

    #[test]
    fn exactly_five_unsigned_single_turn_counts_are_required() {
        let (counts, angles) = parse_counts("0,1024,2048,3072,4095").unwrap();
        assert_eq!(counts, [0, 1024, 2048, 3072, 4095]);
        assert_eq!(angles, Angles::from_encoder_counts(counts).unwrap());
        for text in [
            "",
            "1,2,3,4",
            "1,2,3,4,5,6",
            "1,2,,4,5",
            "-1,2,3,4,5",
            "+1,2,3,4,5",
            "1.0,2,3,4,5",
            "1,2,3,4,4096",
            "65536,2,3,4,5",
            " 1,2,3,4,5",
            "NaN,2,3,4,5",
        ] {
            assert!(parse_counts(text).is_err(), "{text}");
        }
    }

    #[test]
    fn bounded_reader_rejects_the_extra_byte_and_invalid_utf8() {
        assert_eq!(bounded_text(&b"12345678"[..], 8).unwrap(), "12345678");
        assert!(bounded_text(&b"123456789"[..], 8).is_err());
        // Even an endless reader is bounded by limit+1, not read to EOF.
        assert!(bounded_text(io::repeat(b'x'), 8).is_err());
        assert!(bounded_text(&[0xff][..], 8).is_err());
    }

    #[test]
    fn arithmetic_success_never_becomes_actuator_qualification() {
        let catalogue = Catalogue::builtin();
        let (counts, pose) = standing();
        for measurement in [Measurement::StandIn, Measurement::Measured] {
            let mut geometry = geometry();
            geometry.measurement = measurement;
            let report = check(geometry, &catalogue, counts, pose).unwrap();
            assert!(report.model_within_margin_all_checks);
            assert!(!report.actuation_qualified);
            assert_eq!(report.declared_measurement, measurement);
            assert_eq!(
                report.checks.len(),
                catalogue
                    .behaviours
                    .iter()
                    .map(|behaviour| behaviour.states.len() * MASKS.len())
                    .sum::<usize>()
            );
            let json = serde_json::to_value(&report).unwrap();
            assert_eq!(json["actuation_qualified"], false);
            assert_eq!(json["checks"][0]["status"], "checked");
        }
    }

    #[test]
    fn an_unclosed_repeat_is_an_explicit_error_under_both_masks() {
        let mut catalogue = Catalogue::builtin();
        let repeated = catalogue
            .behaviours
            .iter_mut()
            .find(|clip| clip.repeat)
            .unwrap();
        // Current Body ignores shoulder, but that does not make an unsupported
        // authored cycle pass the library's complete-input validation.
        repeated.keyframes.last_mut().unwrap().offsets.shoulder += 0.001;
        catalogue.check().unwrap();
        let (counts, pose) = standing();
        let report = check(geometry(), &catalogue, counts, pose).unwrap();
        assert!(!report.model_within_margin_all_checks);
        let errors: Vec<_> = report.checks.iter().filter(|check| check.repeat).collect();
        assert_eq!(errors.len(), 2);
        assert!(
            errors
                .iter()
                .all(|check| matches!(&check.outcome, Outcome::Error { .. }))
        );
    }

    #[test]
    fn outside_and_unresolved_arithmetic_do_not_report_success() {
        let catalogue = Catalogue::builtin();
        let (counts, pose) = standing();
        for x in [0.09, 0.13] {
            let mut geometry = geometry();
            geometry.base.com_m = [x, 0.0, 0.0];
            for link in &mut geometry.links {
                link.mass_kg = 0.0;
            }
            let report = check(geometry, &catalogue, counts, pose).unwrap();
            assert!(!report.model_within_margin_all_checks);
            assert!(!report.actuation_qualified);
            assert!(
                report
                    .checks
                    .iter()
                    .all(|check| !check.outcome.within_margin())
            );
        }
    }
}
