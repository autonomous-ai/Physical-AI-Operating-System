use lamp_controls::{
    mpr121::{self, Charge, Config, Signals},
    touch_calibrate::{self, Chip, Clock, Plan, Verdict, thresholds},
};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
};

const PROGRAM: &str = env!("CARGO_BIN_EXE_lamp-controls");

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

/// lamp-4ace's ten working electrodes, with its authored example profile.
fn base() -> Config {
    let pins: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture("strip-711-working-pins-example.json")).unwrap())
            .unwrap();
    serde_json::from_value(pins["base"].clone()).unwrap()
}

/// Time moves only when the calibration waits, and by 1 ms for each read.
struct FakeClock(Rc<Cell<u64>>);

impl Clock for FakeClock {
    fn now_us(&mut self) -> u64 {
        self.0.get()
    }
    fn sleep_us(&mut self, microseconds: u64) {
        self.0.set(self.0.get() + microseconds);
    }
}

/// Answers each read with the deltas `script` gives for the time of the read.
struct FakeChip<F: FnMut(u64) -> [i32; 12]> {
    clock: Rc<Cell<u64>>,
    script: F,
    reads: u64,
    fail_after: Option<u64>,
    charge: Charge,
}

impl<F: FnMut(u64) -> [i32; 12]> Chip for FakeChip<F> {
    fn signals(&mut self) -> Result<Signals, mpr121::Error> {
        self.reads += 1;
        if self.fail_after.is_some_and(|after| self.reads > after) {
            return Err(mpr121::Error::OverCurrent);
        }
        let deltas = (self.script)(self.clock.get());
        self.clock.set(self.clock.get() + 1_000);
        let mut signals = Signals {
            touched: 0,
            filtered: [0; 12],
            baseline: [512; 12],
        };
        for (electrode, delta) in deltas.into_iter().enumerate() {
            signals.filtered[electrode] = (512 - delta) as u16;
            if delta >= 6 {
                signals.touched |= 1 << electrode;
            }
        }
        Ok(signals)
    }
    fn charge(&mut self) -> Result<Charge, mpr121::Error> {
        Ok(self.charge)
    }
}

const PLAN: Plan = Plan {
    settle: 10,
    quiet: 5,
    swipe: 30,
};
const S: u64 = 1_000_000;

fn run<F: FnMut(u64) -> [i32; 12]>(
    script: F,
    fail_after: Option<u64>,
) -> Result<(touch_calibrate::Report, Vec<String>), mpr121::Error> {
    let base = base();
    let time = Rc::new(Cell::new(7 * S));
    let mut clock = FakeClock(time.clone());
    let mut chip = FakeChip {
        clock: time,
        script,
        reads: 0,
        fail_after,
        charge: Charge {
            cdc: [53, 57, 55, 51, 31, 50, 49, 42, 31, 41, 46, 47],
            cdt: [1; 12],
        },
    };
    let mut said = Vec::new();
    let report = touch_calibrate::measure(
        &mut chip,
        &mut clock,
        &base.pads,
        &base.profile,
        base.every_ms,
        PLAN,
        |step| said.push(step.to_owned()),
    )?;
    Ok((report, said))
}

/// Settle: the baseline is still far off on every electrode. Quiet: ±2 on
/// the working pads, ±5 on E6. Swipe: each working pad in turn, one second
/// each, three times over, peaking at 40, except E9 and E10 as given. The
/// excluded E4 and E8 swing wildly throughout and must be ignored.
fn script(e9: i32, e10: i32) -> impl FnMut(u64) -> [i32; 12] {
    move |t| lamp(t, e9, e10)
}

fn lamp(t: u64, e9: i32, e10: i32) -> [i32; 12] {
    let since = t - 7 * S;
    let mut deltas = [0; 12];
    let wobble = if (since / 10_000).is_multiple_of(2) {
        2
    } else {
        -2
    };
    for (electrode, delta) in deltas.iter_mut().enumerate() {
        *delta = match electrode {
            4 | 8 => 200,
            _ if since < 10 * S => 30,
            6 if since < 15 * S => wobble * 5 / 2,
            _ if since < 15 * S => wobble,
            _ => 0,
        };
    }
    if since >= 15 * S {
        let second = (since - 15 * S) / S;
        let pad = [0, 1, 2, 3, 5, 6, 7, 9, 10, 11][(second % 10) as usize];
        deltas[pad] = match pad {
            9 => e9,
            10 => e10,
            _ => 40,
        };
    }
    deltas
}

#[test]
fn the_rule_keeps_each_threshold_between_noise_and_touch() {
    assert_eq!(thresholds(0, 2), Err(Verdict::NotTouched));
    assert_eq!(thresholds(0, 3), Err(Verdict::TooCloseToNoise));
    assert_eq!(thresholds(0, 4), Ok((2, 1)));
    assert_eq!(thresholds(3, 40), Ok((20, 12)));
    assert_eq!(thresholds(10, 30), Ok((15, 11)));
    assert_eq!(thresholds(5, 13), Err(Verdict::TooCloseToNoise));
    assert_eq!(thresholds(5, 7), Err(Verdict::NotTouched));
    assert_eq!(thresholds(2, 1023), Ok((255, 153)));
    for noise in 0..=300 {
        for peak in 0..=1023 {
            if let Ok((touch, release)) = thresholds(noise, peak) {
                let (touch, release) = (u16::from(touch), u16::from(release));
                assert!(release < touch, "{noise} {peak}");
                assert!(release > noise, "{noise} {peak}");
                assert!(touch >= noise + 2 && touch <= peak / 2, "{noise} {peak}");
            }
        }
    }
}

#[test]
fn the_three_steps_measure_noise_and_peaks_of_working_pads_only() {
    // E9 peaks just under twice its floor and E10 is never touched.
    let (report, said) = run(script(7, 0), None).unwrap();
    assert_eq!(said.len(), 3);
    assert!(said[0].starts_with("hands away"), "{said:?}");
    assert!(said[1].contains("noise for 5 s"), "{said:?}");
    assert!(said[2].contains("swipe each working pad"), "{said:?}");
    // Every 10 ms: five and thirty seconds of readings, nothing from the settle.
    assert_eq!(report.quiet_readings, 500);
    assert_eq!(report.swipe_readings, 3_000);
    let electrodes: Vec<u8> = report.pads.iter().map(|pad| pad.electrode).collect();
    assert_eq!(electrodes, [0, 1, 2, 3, 5, 6, 7, 9, 10, 11]);
    for pad in &report.pads {
        let noise = if pad.electrode == 6 { 5 } else { 2 };
        assert_eq!(pad.noise, noise, "E{}", pad.electrode);
        assert_eq!(pad.baseline_quiet, 512);
        assert_eq!(
            pad.cdc,
            [53, 57, 55, 51, 31, 50, 49, 42, 31, 41, 46, 47][usize::from(pad.electrode)]
        );
        let expected = match pad.electrode {
            9 => (7, Verdict::TooCloseToNoise),
            10 => (0, Verdict::NotTouched),
            _ => (40, Verdict::Ok),
        };
        assert_eq!((pad.peak, pad.verdict), expected, "E{}", pad.electrode);
    }
    let e6 = &report.pads[5];
    assert_eq!((e6.touch_threshold, e6.release_threshold), (20, 12));
    // The old thresholds of a failed pad are reported, not invented.
    let e9 = &report.pads[7];
    assert_eq!(
        (e9.touch_threshold, e9.release_threshold),
        (
            base().profile.touch_thresholds[9],
            base().profile.release_thresholds[9]
        )
    );
    assert_eq!(report.swipe_touched_old[4], 0);
    assert!(report.swipe_touched_old[0] > 0);
    assert!(!report.all_ok());
    assert_eq!(report.apply(&base().profile), None);
}

#[test]
fn only_when_every_working_pad_is_clear_are_its_thresholds_saved() {
    let (report, _) = run(script(40, 36), None).unwrap();
    assert!(report.all_ok(), "{:?}", report.pads);
    let old = base().profile;
    let new = report.apply(&old).unwrap();
    for electrode in 0..12 {
        if electrode == 4 || electrode == 8 {
            assert_eq!(
                new.touch_thresholds[electrode],
                old.touch_thresholds[electrode]
            );
            assert_eq!(
                new.release_thresholds[electrode],
                old.release_thresholds[electrode]
            );
        } else {
            let pad = report
                .pads
                .iter()
                .find(|pad| usize::from(pad.electrode) == electrode)
                .unwrap();
            assert_eq!(new.touch_thresholds[electrode], pad.touch_threshold);
            assert_eq!(new.release_thresholds[electrode], pad.release_threshold);
        }
    }
    assert_eq!(new.touch_thresholds[10], 18);
    assert_eq!(new.release_thresholds[10], 10);
    // Nothing but the thresholds changes.
    assert_eq!(new.cdc, old.cdc);
    assert_eq!(new.baseline, old.baseline);
    assert_eq!(new.autoconfig, old.autoconfig);
    new.check().unwrap();
}

#[test]
fn a_chip_fault_during_the_swipe_ends_the_calibration_with_nothing_kept() {
    let error = run(script(40, 36), Some(2_000)).unwrap_err();
    assert!(matches!(error, mpr121::Error::OverCurrent), "{error}");
}

#[test]
fn each_step_lasts_one_to_six_hundred_seconds() {
    assert!(Plan::DEFAULT.check().is_ok());
    for (settle, quiet, swipe) in [(0, 5, 30), (10, 0, 30), (10, 5, 0), (601, 5, 30)] {
        let plan = Plan {
            settle,
            quiet,
            swipe,
        };
        assert!(plan.check().is_err(), "{plan:?}");
    }
}

fn program(args: &[&str]) -> std::process::Output {
    Command::new(PROGRAM).args(args).output().unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("controls-calibrate-{}-{name}", std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn the_command_needs_pins_and_a_new_output_and_writes_nothing_on_failure() {
    let pins = fixture("strip-711-working-pins-example.json");
    let pins = pins.to_str().unwrap();
    let output = scratch("new-pins.json");
    let out = output.to_str().unwrap();
    for args in [
        vec!["calibrate-touch", "--pins", pins],
        vec!["calibrate-touch", "--output", out],
        vec![
            "calibrate-touch",
            "--pins",
            pins,
            "--output",
            out,
            "--swipe-seconds",
            "0",
        ],
        vec![
            "calibrate-touch",
            "--pins",
            pins,
            "--output",
            out,
            "--quiet-seconds",
            "601",
        ],
        vec![
            "calibrate-touch",
            "--pins",
            pins,
            "--output",
            out,
            "--seconds",
            "5",
        ],
    ] {
        let result = program(&args);
        assert!(!result.status.success(), "{args:?}");
        assert!(result.stdout.is_empty(), "{args:?}");
        assert!(!output.exists(), "{args:?}");
    }

    // An existing output is refused before any hardware is opened.
    fs::write(&output, b"keep").unwrap();
    let result = program(&["calibrate-touch", "--pins", pins, "--output", out]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("already exists"));
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    fs::remove_file(&output).unwrap();

    // A pins file without a base strip has nothing to calibrate.
    let bare = scratch("bare-pins.json");
    fs::write(
        &bare,
        br#"{"chip":"/dev/gpiochip0","buttons":[],"head_pads":[],"every_ms":5}"#,
    )
    .unwrap();
    let result = program(&[
        "calibrate-touch",
        "--pins",
        bare.to_str().unwrap(),
        "--output",
        out,
    ]);
    fs::remove_file(&bare).unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("no base strip"));
    assert!(!output.exists());
}

#[cfg(not(target_os = "linux"))]
#[test]
fn off_linux_the_command_says_it_has_no_chip_and_writes_nothing() {
    let output = scratch("off-linux.json");
    let result = program(&[
        "calibrate-touch",
        "--pins",
        fixture("strip-711-working-pins-example.json")
            .to_str()
            .unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("which this system does not have"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}
