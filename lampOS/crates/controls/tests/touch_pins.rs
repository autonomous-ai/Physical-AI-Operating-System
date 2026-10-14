use lamp_controls::{
    engine::Config,
    input::{Level, Observation, PinReader},
    mpr121,
    pins::{Pin, PinsConfig, Pull},
    touch_pins::{HEAD_CANDIDATES, NoGpio, TouchPinsConfig},
};
use lamp_msgs::{ClockStamp, ControlFault};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

fn pin(name: &str, line: u32) -> Pin {
    Pin {
        name: name.into(),
        line,
        active_low: true,
        pull: Pull::Up,
    }
}

// An explicit host-only register profile; never opened or presented as per-unit
// calibration. Full register tests live in mpr121.rs.
fn base() -> mpr121::Config {
    serde_json::from_value(json!({
        "bus":0,"address":90,"every_ms":10,"pads":[11,10,9,8,7,6,5,4,3,2,1,0],
        "profile":{
            "touch_thresholds":([12;12]),"release_thresholds":([6;12]),
            "baseline":{
                "rising":{"mhd":1,"nhd":1,"ncl":14,"fdl":0},
                "falling":{"mhd":1,"nhd":1,"ncl":255,"fdl":2},
                "touched":{"nhd":0,"ncl":0,"fdl":0}
            },
            "ffi":34,"sfi":10,"esi_ms":1,"global_cdc":16,"global_cdt":1,
            "cdc":([0;12]),"cdt":([0;12]),"debounce_touch":2,"debounce_release":2,
            "ecr":143,"autoconfig":{"mode":"disabled"}
        }
    }))
    .unwrap()
}

fn wiring() -> TouchPinsConfig {
    TouchPinsConfig {
        chip: "/dev/gpiochip0".into(),
        buttons: vec![pin("primary", 99)],
        head_pads: vec![pin("head-one", 37), pin("head-two", 96)],
        every_ms: 10,
        reset: Some(pin("reset", 100)),
        base: Some(base()),
    }
}

fn config() -> Config {
    Config {
        enabled: true,
        buttons: vec!["primary".into()],
        head_pads: vec!["head-one".into(), "head-two".into()],
        base_selected_mask: Some(4095),
    }
}

fn observation() -> Observation {
    Observation {
        stamp: ClockStamp {
            clock_id: "test-clock".into(),
            at_us: 123_456,
            sequence: 7,
        },
        session_id: "test-reader".into(),
        // Conversion is by identity, not list position.
        buttons: vec![
            Level {
                name: "reset".into(),
                active: true,
            },
            Level {
                name: "primary".into(),
                active: false,
            },
        ],
        head_pads: vec![
            Level {
                name: "head-one".into(),
                active: true,
            },
            Level {
                name: "head-two".into(),
                active: false,
            },
        ],
        base_mask: None,
    }
}

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = if Path::new("/private/tmp").is_dir() {
            PathBuf::from("/private/tmp")
        } else {
            fs::canonicalize(std::env::temp_dir()).unwrap()
        };
        let path = parent.join(format!(
            "ctp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn explicit_roles_keep_reset_separate_and_strip_out_of_gpio_snapshots() {
    let wiring = wiring();
    let touch = wiring.check(&config()).unwrap();
    assert_eq!(touch.primary.as_deref(), Some("primary"));
    assert_eq!(touch.reset.as_deref(), Some("reset"));
    assert_eq!(touch.strip_pads, Some((0..12).rev().collect()));
    let (gpio, controls) = wiring.gpio_config().unwrap();
    gpio.check(&controls).unwrap();
    assert_eq!(
        gpio.buttons.iter().map(|p| p.line).collect::<Vec<_>>(),
        [99, 100]
    );
    assert!(!controls.enabled);
    assert_eq!(controls.base_selected_mask, None);
    assert_eq!(gpio.head_pads, wiring.head_pads);
}

#[test]
fn all_four_head_candidates_require_explicit_name_order_and_polarity() {
    let mut wiring = wiring();
    wiring.head_pads = HEAD_CANDIDATES
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let mut pin = pin(&format!("head-{i}"), line);
            pin.active_low = i % 2 == 0;
            pin.pull = if i % 2 == 0 { Pull::Up } else { Pull::Down };
            pin
        })
        .collect();
    let mut config = config();
    config.head_pads = wiring.head_pads.iter().map(|p| p.name.clone()).collect();
    wiring.check(&config).unwrap();
    wiring.head_pads.swap(0, 1);
    assert!(wiring.check(&config).is_err());
}

#[test]
fn privacy_chip_and_wrong_primary_reset_or_head_lines_are_rejected() {
    let original = serde_json::to_value(wiring()).unwrap();
    for (pointer, replacement) in [
        ("/chip", json!("/dev/gpiochip1")),
        ("/chip", json!("/dev/gpiochip2")),
        ("/chip", json!("/tmp/gpiochip0")),
        ("/buttons/0/line", json!(100)),
        ("/buttons/0/line", json!(98)),
        ("/reset/line", json!(99)),
        ("/reset/active_low", json!(false)),
        ("/head_pads/0/line", json!(99)),
        ("/head_pads/0/line", json!(100)),
        ("/head_pads/0/line", json!(0)),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let wiring: TouchPinsConfig = serde_json::from_value(value).unwrap();
        assert!(wiring.check(&config()).is_err(), "{pointer}");
    }
}

#[test]
fn duplicated_names_lines_and_unmatched_configured_inputs_are_rejected() {
    for change in 0..7 {
        let mut wiring = wiring();
        match change {
            0 => wiring.buttons.push(pin("other", 99)),
            1 => wiring.head_pads[1].line = wiring.head_pads[0].line,
            2 => wiring.head_pads[1].name = wiring.head_pads[0].name.clone(),
            3 => wiring.reset.as_mut().unwrap().name = "primary".into(),
            4 => wiring.buttons.clear(),
            5 => wiring.head_pads[0].name = "unknown".into(),
            _ => wiring.reset.as_mut().unwrap().name = "bad\nname".into(),
        }
        assert!(wiring.check(&config()).is_err(), "change {change}");
    }
}

#[test]
fn gpio_and_strip_poll_bounds_are_checked_independently() {
    for every in [2, 50] {
        let mut wiring = wiring();
        wiring.every_ms = every;
        wiring.check(&config()).unwrap();
    }
    for every in [0, 1, 51, u64::MAX] {
        let mut wiring = wiring();
        wiring.every_ms = every;
        assert!(wiring.check(&config()).is_err());
    }
    for every in [5, 50] {
        let mut wiring = wiring();
        wiring.base.as_mut().unwrap().every_ms = every;
        wiring.check(&config()).unwrap();
    }
    for every in [2, 51] {
        let mut wiring = wiring();
        wiring.base.as_mut().unwrap().every_ms = every;
        assert!(wiring.check(&config()).is_err());
    }
}

#[test]
fn strip_presence_working_mask_geometry_and_profile_must_agree() {
    let mut config = config();
    let mut wiring = wiring();
    config.base_selected_mask = None;
    assert!(wiring.check(&config).is_err());
    wiring.base = None;
    wiring.check(&config).unwrap();
    config.base_selected_mask = Some(4095);
    assert!(wiring.check(&config).is_err());
    wiring.base = Some(base());
    for pads in [
        vec![0, 1, 2, 3, 5, 6, 7, 9, 10, 11],
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10],
    ] {
        wiring.base.as_mut().unwrap().pads = pads;
        assert!(wiring.check(&config).is_err());
    }
    wiring.base = Some(base());
    wiring.base.as_mut().unwrap().profile.release_thresholds[4] = 12;
    assert!(wiring.check(&config).is_err());
    wiring.base = Some(base());
    config.base_selected_mask = Some(0x0eef);
    assert!(wiring.check(&config).is_err());
}

#[test]
fn sparse_working_electrodes_require_the_exact_configured_mask() {
    for pads in [
        vec![0, 5, 11],
        vec![11, 5, 0],
        vec![0, 1, 2, 3, 5, 6, 7, 9, 10, 11],
        vec![11, 10, 9, 7, 6, 5, 3, 2, 1, 0],
    ] {
        let mut wiring = wiring();
        wiring.base.as_mut().unwrap().pads = pads.clone();
        let selected = pads.iter().fold(0_u16, |mask, pad| mask | (1 << pad));
        let mut config = config();
        config.base_selected_mask = Some(selected);
        let touch = wiring.check(&config).unwrap();
        assert_eq!(touch.strip_pads, Some(pads));
        assert_eq!(touch.primary.as_deref(), Some("primary"));
        assert_eq!(touch.reset.as_deref(), Some("reset"));
        assert_eq!(touch.head_pads, config.head_pads);
        for mismatch in [None, Some(4095), Some(selected ^ 1)] {
            config.base_selected_mask = mismatch;
            assert!(wiring.check(&config).is_err());
        }
    }
}

#[test]
fn sparse_working_configuration_preserves_all_physical_threshold_indices() {
    let mut wiring = wiring();
    let base = wiring.base.as_mut().unwrap();
    base.pads = vec![11, 10, 9, 7, 6, 5, 3, 2, 1, 0];
    // These authored values exercise indexing only, not per-lamp calibration.
    base.profile.touch_thresholds = std::array::from_fn(|pad| 20 + pad as u8);
    base.profile.release_thresholds = std::array::from_fn(|pad| 8 + pad as u8);
    let original = base.profile.clone();
    let mut config = config();
    config.base_selected_mask = Some(0x0eef);
    wiring.check(&config).unwrap();
    let encoded = serde_json::to_vec(&wiring).unwrap();
    let decoded: TouchPinsConfig = serde_json::from_slice(&encoded).unwrap();
    decoded.check(&config).unwrap();
    assert_eq!(decoded.base.as_ref().unwrap().profile, original);
    assert_eq!(original.touch_thresholds[5], 25);
    assert_eq!(original.release_thresholds[9], 17);
    assert_eq!(original.touch_thresholds.len(), 12);
    assert_eq!(original.release_thresholds.len(), 12);
}

#[test]
fn sparse_invalid_or_ambiguous_orders_are_refused_before_wiring_is_accepted() {
    for pads in [
        vec![],
        vec![0, 1],
        (0..13).collect(),
        vec![0, 1, 12],
        vec![0, 1, 1],
        vec![0, 11, 5],
        vec![11, 0, 5],
    ] {
        let mut wiring = wiring();
        wiring.base.as_mut().unwrap().pads = pads.clone();
        let mut config = config();
        config.base_selected_mask = Some(
            pads.iter()
                .filter(|&&pad| pad < 12)
                .fold(0_u16, |mask, pad| mask | (1 << pad)),
        );
        assert!(wiring.check(&config).is_err(), "{pads:?}");
    }
}

#[test]
fn sparse_base_remains_optional_and_does_not_enable_shadow_or_gpio() {
    let mut wiring = wiring();
    wiring.buttons.clear();
    wiring.head_pads.clear();
    wiring.reset = None;
    wiring.base.as_mut().unwrap().pads = vec![0, 1, 2];
    let config = Config {
        enabled: false,
        buttons: vec![],
        head_pads: vec![],
        base_selected_mask: Some(7),
    };
    let touch = wiring.check(&config).unwrap();
    assert!(!touch.enabled);
    assert_eq!(touch.strip_pads, Some(vec![0, 1, 2]));
    assert!(touch.primary.is_none() && touch.reset.is_none() && touch.head_pads.is_empty());
    assert!(wiring.gpio_config().is_none());
    assert_eq!(NoGpio.read(123).unwrap(), None);
}

#[test]
fn base_only_configuration_does_not_invent_gpio_or_released_pads() {
    let mut wiring = wiring();
    wiring.buttons.clear();
    wiring.head_pads.clear();
    wiring.reset = None;
    let config = Config {
        enabled: false,
        buttons: vec![],
        head_pads: vec![],
        base_selected_mask: Some(4095),
    };
    let touch = wiring.check(&config).unwrap();
    assert!(touch.primary.is_none() && touch.reset.is_none() && touch.head_pads.is_empty());
    assert!(wiring.gpio_config().is_none());
    assert_eq!(NoGpio.read(1_000).unwrap(), None);
}

#[test]
fn complete_gpio_conversion_preserves_stamp_session_and_independent_roles() {
    let original = observation();
    let converted = wiring().gpio_sample(original.clone()).unwrap();
    assert_eq!(converted.stamp, original.stamp);
    assert_eq!(converted.session_id, original.session_id);
    assert_eq!(converted.primary, Some(false));
    assert_eq!(converted.reset, Some(true));
    assert_eq!(converted.head_pads, original.head_pads);
}

#[test]
fn partial_or_foreign_gpio_observations_are_not_filled_with_false_levels() {
    for change in 0..5 {
        let mut sample = observation();
        match change {
            0 => {
                sample.buttons.pop();
            }
            1 => sample.buttons[0].name = "unknown-reset".into(),
            2 => sample.buttons.push(Level {
                name: "extra".into(),
                active: false,
            }),
            3 => sample.base_mask = Some(0),
            _ => sample.buttons[1].name = sample.buttons[0].name.clone(),
        }
        let error = wiring().gpio_sample(sample).unwrap_err();
        assert!(matches!(
            error.fault,
            ControlFault::Missing | ControlFault::Invalid
        ));
    }
}

#[test]
fn new_wiring_rejects_unknown_fields_and_does_not_relax_legacy_schema() {
    let original = serde_json::to_value(wiring()).unwrap();
    for pointer in ["", "/buttons/0", "/reset", "/base", "/base/profile"] {
        let mut value = original.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(1));
        assert!(
            serde_json::from_value::<TouchPinsConfig>(value).is_err(),
            "{pointer}"
        );
    }
    assert!(serde_json::from_value::<PinsConfig>(original).is_err());
    let mut missing = serde_json::to_value(wiring()).unwrap();
    missing["base"]["profile"]
        .as_object_mut()
        .unwrap()
        .remove("touch_thresholds");
    assert!(serde_json::from_value::<TouchPinsConfig>(missing).is_err());
}

#[test]
fn file_size_bound_accepts_exact_limit_and_refuses_over_limit_or_corrupt_input() {
    let root = Root::new();
    let path = root.0.join("touch.json");
    let original = wiring();
    let mut bytes = serde_json::to_vec(&original).unwrap();
    bytes.resize(16_384, b' ');
    fs::write(&path, &bytes).unwrap();
    assert_eq!(TouchPinsConfig::load(&path).unwrap(), original);
    bytes.push(b' ');
    fs::write(&path, &bytes).unwrap();
    assert!(TouchPinsConfig::load(&path).is_err());
    fs::write(&path, b"{broken").unwrap();
    assert!(TouchPinsConfig::load(&path).is_err());
    assert!(TouchPinsConfig::load(&root.0.join("absent.json")).is_err());
}

#[test]
fn empty_configuration_does_not_claim_a_functional_touch_source() {
    let wiring = TouchPinsConfig {
        chip: "/dev/gpiochip0".into(),
        buttons: vec![],
        head_pads: vec![],
        every_ms: 10,
        reset: None,
        base: None,
    };
    assert!(wiring.check(&Config::default()).is_err());
}
