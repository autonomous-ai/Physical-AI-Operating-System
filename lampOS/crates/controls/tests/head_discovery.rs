use lamp_controls::head_discovery::Discovery;

fn feed(discovery: &mut Discovery, start: u64, end: u64, mask: u8) {
    for at in (start..=end).step_by(5_000) {
        discovery.ingest(at, mask).unwrap();
    }
}

#[test]
fn driven_high_and_low_candidates_need_two_complete_pulses_and_do_not_install_wiring() {
    let mut scan = Discovery::new(
        0,
        [(true, true), (false, false), (false, true), (false, true)],
    )
    .unwrap();
    feed(&mut scan, 0, 600_000, 1);
    for begin in [610_000, 800_000] {
        feed(&mut scan, begin, begin + 100_000, 2);
        feed(&mut scan, begin + 105_000, begin + 175_000, 1);
    }
    let report = scan.report();
    assert_eq!(report.lines[0].active_low_candidate, Some(true));
    assert_eq!(report.lines[1].active_low_candidate, Some(false));
    assert_eq!(report.lines[0].completed_pulses, 2);
    assert_eq!(report.lines[0].longest_pulse_us, 105_000);
    assert!(
        report.lines[2..]
            .iter()
            .all(|line| line.active_low_candidate.is_none())
    );
    assert!(!report.hardware_mapping_installed);
}

#[test]
fn startup_held_floating_chatter_and_unreleased_contact_do_not_qualify() {
    let mut scan =
        Discovery::new(0, [(true, true), (false, true), (true, true), (true, true)]).unwrap();
    feed(&mut scan, 0, 700_000, 0);
    assert!(
        scan.report()
            .lines
            .iter()
            .all(|line| line.completed_pulses == 0)
    );
    feed(&mut scan, 705_000, 800_000, 15);
    for begin in [810_000, 900_000] {
        feed(&mut scan, begin, begin + 10_000, 0);
        feed(&mut scan, begin + 15_000, begin + 75_000, 15);
    }
    feed(&mut scan, 980_000, 1_500_000, 0);
    assert!(
        scan.report()
            .lines
            .iter()
            .all(|line| line.completed_pulses == 0)
    );
}

#[test]
fn missing_samples_cannot_extend_a_pulse_and_recovery_needs_idle_again() {
    let mut scan = Discovery::new(0, [(true, true); 4]).unwrap();
    feed(&mut scan, 0, 600_000, 15);
    feed(&mut scan, 605_000, 620_000, 0);
    scan.ingest(800_000, 0).unwrap();
    feed(&mut scan, 805_000, 900_000, 15);
    assert_eq!(scan.report().gaps, 1);
    assert!(
        scan.report()
            .lines
            .iter()
            .all(|line| line.completed_pulses == 0)
    );
    assert!(scan.ingest(900_000, 15).is_err());
    assert!(scan.ingest(905_000, 16).is_err());
}
