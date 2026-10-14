//! Ask the body for one small move the way Expression would, to see on a
//! lamp what the body makes of it: requests on `expression/body` every
//! 20 ms, each lasting a fifth of a second, that ease the head's pitch and
//! the base's turn out to an offset, hold it, and ease back.
//!
//! usage: expression_ask --bus-root DIR [--dip COUNTS] [--turn COUNTS]
//!        [--out-ms N] [--hold-ms N] [--back-ms N] [--times K] [--every-ms N]
//!
//! It moves nothing by itself: a body moves only if it runs with
//! `LAMP_EXPRESSION_BODY=on`, is in a mood other than rest, and finds the
//! requests sound. Prints one line of JSON when each move begins, is at
//! its farthest and is over, with the machine's clock, to set beside what
//! the joints did (`lamp-bus record … body/joint_states`).
use lamp_bus::Bus;
use lamp_msgs::{BodyOffsets, EXPRESSION_LEASE_US, ExpressionBody, ExpressionLease};
use std::{f64::consts::PI, path::PathBuf, thread, time::Duration};

const COUNTS_A_RADIAN: f64 = 4096.0 / (2.0 * PI);
const STEP_US: u64 = 20_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut root: Option<PathBuf> = None;
    let (mut dip, mut turn) = (0.0f64, 0.0f64);
    let (mut out_ms, mut hold_ms, mut back_ms, mut times, mut every_ms) =
        (400u64, 0u64, 400u64, 1u64, 3_000u64);
    let mut args = std::env::args().skip(1);
    while let Some(name) = args.next() {
        let value = args.next().ok_or("an option without its value")?;
        match name.as_str() {
            "--bus-root" => root = Some(value.into()),
            "--dip" => dip = value.parse()?,
            "--turn" => turn = value.parse()?,
            "--out-ms" => out_ms = value.parse()?,
            "--hold-ms" => hold_ms = value.parse()?,
            "--back-ms" => back_ms = value.parse()?,
            "--times" => times = value.parse()?,
            "--every-ms" => every_ms = value.parse()?,
            _ => return Err(format!("unknown option {name}").into()),
        }
    }
    if dip.abs() > 60.0 || turn.abs() > 70.0 || out_ms == 0 || back_ms == 0 || times > 20 {
        return Err(
            "a dip of at most 60 counts, a turn of at most 70, and at most 20 times".into(),
        );
    }
    let bus = Bus::new(&root.ok_or("--bus-root DIR is needed")?)?;
    let mut says = bus.publisher_declared(
        ExpressionBody::TOPIC,
        &ExpressionBody::declaration(),
        "expression-trial",
    )?;
    let clock_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .to_owned();
    let session_id = format!("trial-{}-{}", std::process::id(), lamp_ipc::monotonic_us());
    let (out_us, hold_us, back_us) = (out_ms * 1_000, hold_ms * 1_000, back_ms * 1_000);
    let lasts = out_us + hold_us + back_us;
    let mut sequence = 0u64;
    for time in 0..times {
        let began = lamp_ipc::monotonic_us();
        println!(
            "{}",
            serde_json::json!({"move": time + 1, "began_us": began, "farthest_us": began + out_us,
                "back_from_us": began + out_us + hold_us, "over_us": began + lasts, "dip": dip, "turn": turn})
        );
        // A quarter of a second of nothing after the move, so that the
        // last request says so and the body is not left with half of it.
        while lamp_ipc::monotonic_us() < began + lasts + 250_000 {
            let now = lamp_ipc::monotonic_us();
            let at = now - began;
            let ease = |share: f64| 0.5 - 0.5 * (PI * share.clamp(0.0, 1.0)).cos();
            let share = if at < out_us {
                ease(at as f64 / out_us as f64)
            } else if at < out_us + hold_us {
                1.0
            } else if at < lasts {
                1.0 - ease((at - out_us - hold_us) as f64 / back_us as f64)
            } else {
                0.0
            };
            sequence += 1;
            let request = ExpressionBody {
                lease: ExpressionLease {
                    clock_id: clock_id.clone(),
                    session_id: session_id.clone(),
                    voice_session_id: "trial".into(),
                    sequence,
                    turn: 1,
                    request: 1,
                    issued_us: now,
                    apply_us: now,
                    valid_until_us: now + EXPRESSION_LEASE_US,
                },
                offsets: BodyOffsets {
                    head_pitch: dip * share / COUNTS_A_RADIAN,
                    base_yaw: turn * share / COUNTS_A_RADIAN,
                    ..BodyOffsets::ZERO
                },
                hold: false,
                gaze: None,
            };
            request.check().map_err(|why| format!("{why:?}"))?;
            says.publish_json(now, &request)?;
            let spent = lamp_ipc::monotonic_us() - now;
            thread::sleep(Duration::from_micros(STEP_US.saturating_sub(spent)));
        }
        if time + 1 < times {
            thread::sleep(Duration::from_millis(
                every_ms.saturating_sub(lasts / 1_000 + 250),
            ));
        }
    }
    Ok(())
}
