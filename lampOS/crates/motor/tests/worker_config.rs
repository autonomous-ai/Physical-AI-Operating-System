use lamp_interaction::{BootId, MonoTime};
use lamp_motor::runtime::{channel::Channels, config::*, wire::Control};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
fn config() -> Config {
    Config {
        device: "/dev/explicit-test-only".into(),
        lock_directory: "/root/private-lock".into(),
        expected_device: Identity {
            filesystem_device: 1,
            inode: 2,
            character_device: 3,
            major: 4,
            minor: 5,
        },
        unit: "inert-unit".into(),
        calibration: std::array::from_fn(|i| Calibration {
            joint: i as u8 + 1,
            id: i as u16 + 1,
            drive_mode: 0,
            homing_offset: 0,
            range_min: 1000,
            range_max: 3000,
        }),
        qualification: Qualification {
            record: [7; 32],
            valid_from_us: 1,
            expires_at_us: 1_000_000,
            joints: [Envelope {
                minimum: -70.0,
                maximum: 70.0,
                velocity: 100.0,
                acceleration: 1000.0,
                raw_acceleration: 10,
                raw_goal_time: 0,
                raw_speed: 500,
                maximum_load_raw: 100,
                maximum_velocity_raw: 1000,
                tracking_error: 10.0,
                settling_error: 0.5,
            }; 5],
        },
        timing: Bounds {
            transaction_us: 40_000,
            io_us: 5_000,
            readback_age_us: 200_000,
            placement_age_us: 500_000,
            interval_us: 10_000,
        },
        placement_source: BootId::new([3; 16]).unwrap(),
        maximum_runtime_us: 600_000_000,
    }
}
#[test]
fn provisioning_rejects_hidden_defaults_unqualified_limits_and_narrowing() {
    let c = config();
    c.validate().unwrap();
    let mut value = serde_json::to_value(&c).unwrap();
    value["calibration"][0]["range_min"] = serde_json::json!(-1);
    assert!(
        serde_json::from_value::<Config>(value)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut bad = c.clone();
    bad.qualification.joints[0].raw_speed = 0;
    assert!(bad.validate().is_err());
    bad = c.clone();
    bad.qualification.record = [0; 32];
    assert!(bad.validate().is_err());
    bad = c.clone();
    bad.calibration[0].id = 257;
    assert!(bad.validate().is_err());
    bad = c.clone();
    bad.timing.io_us = 20_001;
    assert!(bad.validate().is_err());
    bad = c;
    bad.device = PathBuf::from("relative");
    assert!(bad.validate().is_err());
    assert!(serde_json::from_str::<Config>("{}").is_err());
    assert!(serde_json::from_str::<Control>(r#"{"kind":"stop","torque":false}"#).is_err());
}
#[test]
fn bounded_private_file_is_required_before_any_device_open() {
    let path = PathBuf::from("/tmp").join(format!(
        "motor-config-{}",
        lamp_motor::runtime::supervisor::boot_text(
            lamp_motor::runtime::supervisor::new_boot().unwrap()
        )
    ));
    let bytes = serde_json::to_vec(&config()).unwrap();
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    Config::read(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Config::read(&path).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&path, vec![b' '; 16_385]).unwrap();
    assert!(Config::read(&path).is_err());
    fs::remove_file(path).unwrap();
}
#[test]
fn transport_preserves_original_sequence_age_and_boot() {
    let parent = BootId::new([1; 16]).unwrap();
    let worker = BootId::new([2; 16]).unwrap();
    let (mut p, mut w) = Channels::pair(parent, worker).unwrap();
    p.control
        .send(&Control::Stop {}, MonoTime::from_micros(1))
        .unwrap();
    assert!(
        w.control
            .receive::<Control>(MonoTime::from_micros(100_002))
            .is_err()
    );
    let (mut p, mut w) = Channels::pair(parent, worker).unwrap();
    p.control
        .send(&Control::Stop {}, MonoTime::from_micros(2))
        .unwrap();
    assert!(
        w.control
            .receive::<Control>(MonoTime::from_micros(1))
            .is_err()
    );
}

#[test]
fn oversized_serialization_does_not_consume_the_next_channel_sequence() {
    let parent = BootId::new([1; 16]).unwrap();
    let child = BootId::new([2; 16]).unwrap();
    let (mut p, mut w) = Channels::pair(parent, child).unwrap();
    assert!(
        p.control
            .send(&"x".repeat(8193), MonoTime::from_micros(1))
            .is_err()
    );
    assert!(
        w.control
            .receive::<Control>(MonoTime::from_micros(1))
            .unwrap()
            .is_none()
    );
    p.control
        .send(&Control::Stop {}, MonoTime::from_micros(2))
        .unwrap();
    assert!(matches!(
        w.control
            .receive::<Control>(MonoTime::from_micros(2))
            .unwrap(),
        Some(Control::Stop {})
    ));
}
