//! `lamp-body exit-report`: the last exit report the body printed
//! (`{"port":…,"body":{…}}`, as in `journalctl -u lamp-body`), as a few
//! plain lines for the morning report. Off the motion path: it only reads.

use crate::Joint;
use serde::Deserialize;
use std::fmt::Write;

/// The fields of [`crate::body::BodyReport`] the summary says, by the same names. A field
/// renamed there fails `a_joint_flapping_between_60_and_64_stops_on_heat`,
/// which reads a real report back through [`summary`].
#[derive(Deserialize)]
struct Report {
    stop: serde_json::Value,
    stood: bool,
    steps: u64,
    bad_temperatures: [u16; 5],
    /// Absent from reports before it was added.
    #[serde(default)]
    temperature_flaps: Option<[u16; 5]>,
    temperature_settles: Vec<(u8, u8, u8)>,
    peak_temperature: [u8; 3],
    arm_peak_temperature: [u8; 2],
    bad_reads: [u16; 5],
    goal_writes: u64,
}

#[derive(Deserialize)]
struct Exit {
    body: Option<Report>,
}

/// The summary of the last exit report in `text` (one line, or a whole
/// journal), or `None` if there is none.
pub fn summary(text: &str) -> Option<String> {
    let exit = text.lines().rev().find_map(|line| {
        let start = line.find("{\"port\":")?;
        serde_json::from_str::<Exit>(&line[start..]).ok()
    })?;
    let Some(body) = exit.body else {
        return Some("no body report: the port was not run\n".to_owned());
    };
    let mut out = format!("stop: {}\n", stop_line(&body.stop));
    let stood = if body.stood { "yes" } else { "no" };
    let _ = writeln!(out, "stood: {stood}, steps {}", body.steps);
    let all = Joint::ALL.map(Joint::name);
    let _ = writeln!(
        out,
        "bad_temperatures: {}",
        per_joint(&all, &body.bad_temperatures)
    );
    match body.temperature_flaps {
        Some(flaps) => {
            let _ = writeln!(out, "temperature_flaps: {}", per_joint(&all, &flaps));
        }
        None => out.push_str("temperature_flaps: not in this report\n"),
    }
    let settles: Vec<String> = body
        .temperature_settles
        .iter()
        .map(|(joint, from, to)| format!("{joint}:{from}->{to}"))
        .collect();
    let _ = writeln!(
        out,
        "temperature_settles: {} {}",
        settles.len(),
        settles.join(" ")
    );
    let commanded = crate::body::COMMANDED.map(Joint::name);
    let _ = writeln!(
        out,
        "peak_temperature: {}",
        per_joint(&commanded, &body.peak_temperature)
    );
    let arm = crate::body::ARM.map(Joint::name);
    let _ = writeln!(
        out,
        "arm_peak_temperature: {}",
        per_joint(&arm, &body.arm_peak_temperature)
    );
    let _ = writeln!(out, "bad_reads: {}", per_joint(&all, &body.bad_reads));
    let _ = writeln!(out, "goal_writes: {}", body.goal_writes);
    Some(out)
}

fn per_joint<T: std::fmt::Display>(names: &[&str], values: &[T]) -> String {
    names
        .iter()
        .zip(values)
        .map(|(name, value)| format!("{name} {value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `kind`, then `joint N (name)`, then the stop's other fields by name.
fn stop_line(stop: &serde_json::Value) -> String {
    let Some(fields) = stop.as_object() else {
        return stop.to_string();
    };
    let mut line = fields
        .get("kind")
        .and_then(|kind| kind.as_str())
        .unwrap_or("?")
        .to_owned();
    if let Some(id) = fields.get("joint").and_then(|joint| joint.as_u64()) {
        let name = u8::try_from(id)
            .ok()
            .and_then(|id| Joint::try_from(id).ok())
            .map_or("?", Joint::name);
        let _ = write!(line, " joint {id} ({name})");
    }
    for (key, value) in fields {
        if key != "kind" && key != "joint" {
            let _ = write!(line, " {key} {value}");
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::summary;

    /// lamp-4ace's 15:25:38 exit on 2026-10-17, as quoted on #705 at 16:23:
    /// the stop, steps, stood, `bad_temperatures`, the shoulder's ten settles
    /// and `arm_peak_temperature` are the lamp's; `peak_temperature` and
    /// `goal_writes` were not quoted and are made up. It predates
    /// `temperature_flaps`. A journal line before it is skipped.
    const LAMP_4ACE_1525: &str = concat!(
        "Oct 17 15:25:30 lamp-4ace lamp-body[811]: lamp-body: step 618580 a face is looked at\n",
        "Oct 17 15:25:38 lamp-4ace lamp-body[811]: {\"port\":{\"inspection\":null},\"body\":{",
        "\"steps\":618587,\"goal_writes\":618001,\"stood\":true,",
        "\"bad_reads\":[0,0,0,0,0],\"bad_temperatures\":[817,9354,2356,448,323],",
        "\"temperature_settles\":[[2,59,55],[2,61,55],[2,57,53],[2,56,51],[2,56,52],",
        "[2,57,52],[2,58,54],[2,54,50],[2,58,54],[2,59,55]],",
        "\"peak_temperature\":[50,48,47],\"arm_peak_temperature\":[62,58],",
        "\"peak_load\":[120,90,80],",
        "\"stop\":{\"kind\":\"bad_temperature\",\"joint\":2,\"temperature\":55,\"from\":60}}}\n",
        "Oct 17 15:25:53 lamp-4ace systemd[1]: lamp-body.service: Scheduled restart job.\n",
    );

    #[test]
    fn lamp_4ace_1525_reads_as_plain_lines() {
        assert_eq!(
            summary(LAMP_4ACE_1525).unwrap(),
            "stop: bad_temperature joint 2 (base_pitch) from 60 temperature 55\n\
             stood: yes, steps 618587\n\
             bad_temperatures: base_yaw 817, base_pitch 9354, elbow_pitch 2356, wrist_roll 448, wrist_pitch 323\n\
             temperature_flaps: not in this report\n\
             temperature_settles: 10 2:59->55 2:61->55 2:57->53 2:56->51 2:56->52 2:57->52 2:58->54 2:54->50 2:58->54 2:59->55\n\
             peak_temperature: wrist_pitch 50, wrist_roll 48, base_yaw 47\n\
             arm_peak_temperature: base_pitch 62, elbow_pitch 58\n\
             bad_reads: base_yaw 0, base_pitch 0, elbow_pitch 0, wrist_roll 0, wrist_pitch 0\n\
             goal_writes: 618001\n"
        );
    }

    #[test]
    fn the_last_report_wins_and_none_is_none() {
        let two = LAMP_4ACE_1525.replace("618587", "1") + LAMP_4ACE_1525;
        assert!(summary(&two).unwrap().contains("steps 618587"));
        assert_eq!(summary("lamp-body: step 1\n"), None);
        assert_eq!(
            summary("{\"port\":{},\"body\":null}").unwrap(),
            "no body report: the port was not run\n"
        );
    }
}
