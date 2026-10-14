use lamp_controls::{input::Level, touch::*, touch_receiver::TouchReceiver};
use lamp_msgs::{ClockStamp, ControlFault, TouchEvent, TouchGesture, TouchIntent, TouchSource};

struct Rig {
    engine: TouchEngine,
    gpio_seq: u64,
    strip_seq: u64,
}
impl Rig {
    fn new(config: TouchConfig) -> Self {
        Self {
            engine: TouchEngine::new(config, "boot", "controls").unwrap(),
            gpio_seq: 0,
            strip_seq: 0,
        }
    }
    fn sample(&mut self, at: u64, primary: bool, reset: bool, head: u16) -> GpioSample {
        self.gpio_seq += 1;
        let config = self.engine.config();
        GpioSample {
            stamp: ClockStamp {
                clock_id: "boot".into(),
                at_us: at,
                sequence: self.gpio_seq,
            },
            session_id: "gpio".into(),
            primary: config.primary.as_ref().map(|_| primary),
            reset: config.reset.as_ref().map(|_| reset),
            head_pads: config
                .head_pads
                .iter()
                .enumerate()
                .map(|(i, name)| Level {
                    name: name.clone(),
                    active: head & (1 << i) != 0,
                })
                .collect(),
        }
    }
    fn gpio(&mut self, at: u64, primary: bool, reset: bool, head: u16) -> Vec<TouchEvent> {
        let sample = self.sample(at, primary, reset, head);
        self.engine.ingest_gpio(at, sample).unwrap();
        self.take(at)
    }
    fn take(&mut self, at: u64) -> Vec<TouchEvent> {
        let output = self.engine.tick(at);
        output.state.check().unwrap();
        for event in &output.events {
            event.check().unwrap();
        }
        output.events
    }
    fn strip(&mut self, at: u64, mask: u16) -> Vec<TouchEvent> {
        self.strip_seq += 1;
        self.engine
            .ingest_strip(
                at,
                StripSample {
                    stamp: ClockStamp {
                        clock_id: "boot".into(),
                        at_us: at,
                        sequence: self.strip_seq,
                    },
                    session_id: "strip".into(),
                    mask,
                },
            )
            .unwrap();
        self.take(at)
    }
    fn head_ready(&mut self) {
        for at in (0..=500_000).step_by(50_000) {
            assert!(self.gpio(at, false, false, 0).is_empty());
        }
    }
    fn hold(
        &mut self,
        from: u64,
        through: u64,
        primary: bool,
        reset: bool,
        head: u16,
    ) -> Vec<TouchEvent> {
        (from..=through)
            .step_by(50_000)
            .flat_map(|at| self.gpio(at, primary, reset, head))
            .collect()
    }
}
fn head() -> TouchConfig {
    TouchConfig {
        enabled: true,
        head_pads: vec!["pad37".into(), "pad96".into()],
        ..TouchConfig::default()
    }
}
fn buttons() -> TouchConfig {
    TouchConfig {
        enabled: true,
        primary: Some("primary99".into()),
        reset: Some("reset100".into()),
        ..TouchConfig::default()
    }
}
fn strip() -> TouchConfig {
    TouchConfig {
        enabled: true,
        strip_pads: Some((0..12).collect()),
        ..TouchConfig::default()
    }
}

#[test]
fn configuration_is_bounded_unique_explicit_and_strict() {
    assert!(TouchEngine::new(TouchConfig::default(), "boot", "controls").is_err());
    let mut config = buttons();
    config.reset = config.primary.clone();
    assert!(config.check().is_err());
    let mut config = head();
    config.head_pads = vec!["a".into(); 5];
    assert!(config.check().is_err());
    let mut config = strip();
    config.strip_pads = Some(vec![0; 12]);
    assert!(config.check().is_err());
    let mut config = strip();
    config.strip_pads = Some(vec![0, 1]);
    assert!(config.check().is_err());
    let mut value = serde_json::to_value(head()).unwrap();
    value["privacy"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TouchConfig>(value).is_err());
}

fn working_strip() -> TouchConfig {
    TouchConfig {
        strip_pads: Some(vec![0, 1, 2, 3, 5, 6, 7, 9, 10, 11]),
        ..strip()
    }
}

fn electrode_mask(pads: &[u8]) -> u16 {
    pads.iter().fold(0, |mask, pad| mask | (1 << pad))
}

#[test]
fn working_electrodes_accept_bounded_ordered_subsets_without_changing_full_layouts() {
    for pads in [
        vec![0, 5, 11],
        vec![11, 5, 0],
        (0..11).collect(),
        working_strip().strip_pads.unwrap(),
        vec![11, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    ] {
        let config = TouchConfig {
            strip_pads: Some(pads),
            ..strip()
        };
        config.check().unwrap();
        TouchEngine::new(config, "boot", "controls").unwrap();
    }
    for pads in [
        vec![],
        vec![0],
        vec![0, 1],
        (0..13).collect(),
        vec![0, 1, 12],
        vec![0, 1, 1],
        vec![0, 11, 5],
    ] {
        let config = TouchConfig {
            strip_pads: Some(pads),
            ..strip()
        };
        assert!(config.check().is_err());
        assert!(TouchEngine::new(config, "boot", "controls").is_err());
    }
}

#[test]
fn excluded_only_acquisitions_never_become_active_or_lifecycle_requests() {
    let excluded = electrode_mask(&[4, 8]);
    let mut rig = Rig::new(working_strip());
    let mut receiver = TouchReceiver::new("boot").unwrap();
    for at in (0..=2_100_000).step_by(50_000) {
        assert!(rig.strip(at, excluded).is_empty());
        let state = rig.engine.tick(at).state;
        let source = &state.sources[0];
        assert!(source.ready);
        assert_eq!(source.active_mask, Some(0));
        assert_eq!(source.active, Some(false));
        assert_eq!(source.held_us, None);
        assert_eq!(source.contact_id, None);
        assert_eq!(source.position, None);
        assert!(receiver.observe(at, &state).unwrap().is_empty());
    }
    assert!(rig.engine.tick(2_100_000).state.sources[0].armed);
    rig.strip(2_110_000, excluded | 7);
    rig.strip(2_115_000, excluded | 7);
    let events = rig.strip(2_125_000, excluded | 7);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].gesture, TouchGesture::Touch);
    assert_eq!(events[0].began_us, 2_110_000);
    assert_eq!(events[0].active_mask, 7);
    assert_eq!(events[0].when_asleep, Some(TouchIntent::Wake));
    let state = rig.engine.tick(2_125_000).state;
    assert_eq!(receiver.observe(2_125_000, &state).unwrap(), events);
}

#[test]
fn excluded_pad_noise_does_not_cancel_a_working_stationary_hold() {
    let mut rig = Rig::new(working_strip());
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    let mut events = rig.strip(20_000, 7);
    events.extend(rig.strip(25_000, 7 | (1 << 4)));
    events.extend(rig.strip(35_000, 7 | (1 << 8)));
    for (index, at) in (70_000..=2_070_000).step_by(50_000).enumerate() {
        let raw = 7 | if index % 2 == 0 { 1 << 4 } else { 1 << 8 };
        events.extend(rig.strip(at, raw));
        let state = rig.engine.tick(at).state;
        assert_eq!(state.sources[0].active_mask, Some(7));
        assert_eq!(state.sources[0].held_us, Some(at - 20_000));
    }
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].gesture, TouchGesture::Touch);
    assert_eq!(events[1].when_speaking, TouchIntent::Sleep);
    assert_eq!(events[1].observed_us, 2_020_000);
    assert_eq!(events[1].active_mask, 7);
    assert_eq!(events[0].contact_id, events[1].contact_id);
}

#[test]
fn excluded_electrodes_cannot_complete_the_three_working_pad_chord() {
    for working in [1, 3] {
        let mut rig = Rig::new(working_strip());
        let excluded = electrode_mask(&[4, 8]);
        rig.strip(0, excluded);
        rig.strip(10_000, excluded);
        for at in (20_000..=2_120_000).step_by(50_000) {
            assert!(rig.strip(at, excluded | working).is_empty());
        }
        let state = rig.engine.tick(2_120_000).state;
        assert_eq!(state.sources[0].active_mask, Some(working));
        assert_eq!(state.sources[0].active, Some(true));
        assert!(state.sources[0].position.is_some());
        assert!(state.latest.is_empty());
    }
}

#[test]
fn out_of_range_raw_bits_are_faults_even_when_no_working_electrode_is_active() {
    let mut rig = Rig::new(working_strip());
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    let raw = StripSample {
        stamp: ClockStamp {
            clock_id: "boot".into(),
            at_us: 20_000,
            sequence: 3,
        },
        session_id: "strip".into(),
        mask: (1 << 12) | (1 << 4),
    };
    assert_eq!(
        rig.engine.ingest_strip(20_000, raw).unwrap_err().fault,
        ControlFault::Invalid
    );
    let output = rig.engine.tick(20_000);
    output.state.check().unwrap();
    assert!(output.events.is_empty());
    assert_eq!(output.state.sources[0].active, None);
    assert_eq!(output.state.sources[0].active_mask, None);
    assert_eq!(output.state.sources[0].fault, Some(ControlFault::Invalid));
}

#[test]
fn excluded_only_bits_release_a_working_contact_before_its_sleep_deadline() {
    let excluded = electrode_mask(&[4, 8]);
    let mut rig = Rig::new(working_strip());
    rig.strip(0, excluded);
    rig.strip(10_000, excluded);
    rig.strip(20_000, excluded | 7);
    rig.strip(25_000, excluded | 7);
    let first = rig.strip(35_000, excluded | 7).pop().unwrap();
    for at in (70_000..=1_970_000).step_by(50_000) {
        assert!(rig.strip(at, excluded | 7).is_empty());
    }
    for at in [2_019_999, 2_020_000, 2_049_999, 2_050_000] {
        assert!(rig.strip(at, excluded).is_empty());
        let state = rig.engine.tick(at).state;
        assert_eq!(state.sources[0].active, Some(false));
        assert_eq!(state.sources[0].active_mask, Some(0));
        assert_eq!(state.sources[0].held_us, None);
        assert_eq!(state.sources[0].position, None);
    }
    assert_eq!(rig.engine.tick(2_050_000).state.sources[0].contact_id, None);
    rig.strip(2_060_000, excluded | 7);
    rig.strip(2_065_000, excluded | 7);
    let next = rig.strip(2_075_000, excluded | 7).pop().unwrap();
    assert_eq!(next.gesture, TouchGesture::Touch);
    assert_ne!(next.contact_id, first.contact_id);
    assert_eq!(next.began_us, 2_060_000);
}

#[test]
fn missing_working_electrodes_are_not_artificial_gaps_during_a_slide() {
    for reversed in [false, true] {
        let mut config = working_strip();
        if reversed {
            config.strip_pads.as_mut().unwrap().reverse();
        }
        let mut rig = Rig::new(config);
        rig.strip(0, 0);
        rig.strip(10_000, 0);
        let initial = electrode_mask(&[2, 3, 5]);
        rig.strip(20_000, initial);
        rig.strip(25_000, initial);
        let first = rig.strip(35_000, initial).pop().unwrap();
        let expected = if reversed { 23.0 / 33.0 } else { 10.0 / 33.0 };
        assert!((first.position.unwrap() - expected).abs() < 1e-6);
        let mut events = vec![first.clone()];
        let mut positions = vec![first.position.unwrap()];
        for (index, pads) in [[3, 5, 6], [5, 6, 7], [6, 7, 9], [7, 9, 10], [9, 10, 11]]
            .into_iter()
            .enumerate()
        {
            let at = 45_000 + index as u64 * 15_000;
            let mask = electrode_mask(&pads);
            events.extend(rig.strip(at, mask));
            events.extend(rig.strip(at + 5_000, mask));
            let state = rig.engine.tick(at + 5_000).state;
            assert_eq!(state.sources[0].active_mask, Some(mask));
            assert_eq!(state.sources[0].contact_id, Some(first.contact_id));
            positions.push(state.sources[0].position.unwrap());
        }
        assert!(
            positions
                .windows(2)
                .all(|p| if reversed { p[1] < p[0] } else { p[1] > p[0] })
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.gesture == TouchGesture::Touch)
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.gesture == TouchGesture::Slide)
                .count(),
            5
        );
        for event in events.iter().skip(1) {
            assert_eq!(event.contact_id, first.contact_id);
            assert_eq!(event.toward_pad, Some(11));
            assert_eq!(event.active_mask & electrode_mask(&[4, 8]), 0);
        }
    }
}

#[test]
fn working_subset_faults_remain_unknown_until_a_verified_release_baseline() {
    let mut rig = Rig::new(working_strip());
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.strip(20_000, 7);
    rig.strip(25_000, 7);
    rig.strip(35_000, 7);
    rig.engine.strip_fault(40_000, ControlFault::Missing);
    let fault = rig.engine.tick(40_000);
    assert!(fault.events.is_empty());
    assert!(fault.state.latest.is_empty());
    assert_eq!(fault.state.sources[0].active, None);
    assert_eq!(fault.state.sources[0].active_mask, None);
    assert_eq!(fault.state.sources[0].held_us, None);
    for at in (45_000..=2_095_000).step_by(50_000) {
        assert!(rig.strip(at, 7).is_empty());
        let state = rig.engine.tick(at).state;
        assert!(!state.sources[0].armed);
        assert_eq!(state.sources[0].active, Some(true));
        assert_eq!(state.sources[0].held_us, None);
    }
    let excluded = electrode_mask(&[4, 8]);
    rig.strip(2_100_000, excluded);
    rig.strip(2_110_000, excluded);
    rig.strip(2_120_000, 7 | excluded);
    rig.strip(2_125_000, 7 | excluded);
    let event = rig.strip(2_135_000, 7 | excluded).pop().unwrap();
    assert_eq!(event.gesture, TouchGesture::Touch);
    assert_eq!(event.began_us, 2_120_000);
}
#[test]
fn default_disabled_reports_shadow_without_eligible_events() {
    let mut config = buttons();
    config.enabled = false;
    let mut rig = Rig::new(config);
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, false, 0);
    let events = rig.gpio(20_000, false, false, 0);
    assert_eq!(events.len(), 1);
    assert!(!events[0].enabled);
    assert!(!events[0].fresh_at("boot", 20_000));
    assert!(!rig.engine.tick(20_000).state.enabled);
}
#[test]
fn primary_short_release_is_immediate_stop_with_no_double_tap_wait() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    assert!(rig.gpio(10_000, true, false, 0).is_empty());
    let e = rig.gpio(20_000, false, false, 0).pop().unwrap();
    assert_eq!(e.source, TouchSource::Primary);
    assert_eq!(e.gesture, TouchGesture::Tap);
    assert_eq!(
        (e.when_speaking, e.when_quiet),
        (TouchIntent::Stop, TouchIntent::Stop)
    );
    assert_eq!(e.stamp.at_us, 20_000);
    assert_eq!(e.released_us, Some(20_000));
}
#[test]
fn primary_hold_never_commits_before_release_and_never_factory_resets() {
    for duration in [2_000_000, 5_000_000] {
        let mut rig = Rig::new(buttons());
        rig.gpio(0, false, false, 0);
        assert!(
            rig.hold(50_000, duration + 50_000, true, false, 0)
                .is_empty()
        );
        let at = duration + 50_001;
        let events = rig.gpio(at, false, false, 0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].when_quiet, TouchIntent::SoftwareMute);
    }
}
#[test]
fn reset_short_release_does_nothing_and_exact_five_seconds_is_reset_only() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.hold(50_000, 5_000_000, false, true, 0);
    assert!(rig.gpio(5_049_999, false, false, 0).is_empty());
    rig.hold(5_099_999, 5_299_999, false, false, 0);
    assert!(rig.hold(5_349_999, 10_299_999, false, true, 0).is_empty());
    let e = rig.gpio(10_349_999, false, false, 0).pop().unwrap();
    assert_eq!(e.source, TouchSource::Reset);
    assert_eq!(e.when_speaking, TouchIntent::FactoryReset);
    assert_eq!(e.observed_us - e.began_us, RESET_HOLD_US);
}
#[test]
fn startup_held_buttons_and_orphan_release_cannot_act() {
    let mut rig = Rig::new(buttons());
    assert!(rig.hold(0, 5_000_000, true, true, 0).is_empty());
    assert!(rig.gpio(5_000_001, false, false, 0).is_empty());
    rig.gpio(5_020_000, true, false, 0);
    assert_eq!(rig.gpio(5_030_000, false, false, 0).len(), 1);
}
#[test]
fn same_polarity_button_debounce_cancels_bounce_hold_evidence() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, false, 0);
    assert_eq!(rig.gpio(20_000, false, false, 0).len(), 1);
    rig.gpio(30_000, true, false, 0);
    assert!(rig.hold(80_000, 2_080_000, true, false, 0).is_empty());
    assert!(rig.gpio(2_100_000, false, false, 0).is_empty());
}
#[test]
fn head_needs_startup_settle_and_a_released_baseline() {
    let mut rig = Rig::new(head());
    assert!(rig.hold(0, 1_000_000, false, false, 1).is_empty());
    assert!(!rig.engine.tick(1_000_000).state.sources[0].armed);
    rig.gpio(1_010_000, false, false, 0);
    rig.gpio(1_020_000, false, false, 1);
    let e = rig.gpio(1_040_000, false, false, 1).pop().unwrap();
    assert_eq!(e.when_speaking, TouchIntent::Stop);
    assert_eq!(e.when_quiet, TouchIntent::Attend);
}
#[test]
fn head_qualification_uses_acquisition_time_not_receipt_or_ticks() {
    let mut rig = Rig::new(head());
    rig.head_ready();
    rig.gpio(510_000, false, false, 1);
    assert!(rig.take(529_999).is_empty());
    let sample = rig.sample(529_999, false, false, 1);
    rig.engine.ingest_gpio(550_000, sample).unwrap();
    assert!(rig.take(550_000).is_empty());
    let sample = rig.sample(530_000, false, false, 1);
    rig.engine.ingest_gpio(550_001, sample).unwrap();
    let e = rig.take(550_001).pop().unwrap();
    assert_eq!(e.began_us, 510_000);
    assert_eq!(e.observed_us, 530_000);
    assert_eq!(e.stamp.at_us, 550_001);
}
#[test]
fn configured_head_order_controls_mask_and_active_names_not_input_vector_order() {
    let mut rig = Rig::new(head());
    rig.head_ready();
    let mut sample = rig.sample(510_000, false, false, 2);
    sample.head_pads.reverse();
    rig.engine.ingest_gpio(510_000, sample).unwrap();
    let mut sample = rig.sample(530_000, false, false, 2);
    sample.head_pads.reverse();
    rig.engine.ingest_gpio(530_000, sample).unwrap();
    let e = rig.take(530_000).pop().unwrap();
    assert_eq!(e.active_mask, 2);
    assert_eq!(e.active_names, ["pad96"]);
}
#[test]
fn stationary_head_stroke_occurs_once_at_actual_half_second() {
    let mut rig = Rig::new(head());
    rig.head_ready();
    let events = rig.hold(550_000, 2_550_000, false, false, 3);
    assert_eq!(
        events.iter().map(|e| e.gesture).collect::<Vec<_>>(),
        [TouchGesture::Touch, TouchGesture::Stroke]
    );
    assert_eq!(events[1].observed_us - events[1].began_us, HEAD_STROKE_US);
    assert_eq!(events[1].active_names, ["pad37", "pad96"]);
    assert!(
        rig.engine
            .tick(2_550_000)
            .state
            .latest
            .iter()
            .all(|e| e.gesture != TouchGesture::Stroke)
    );
}
#[test]
fn fast_mode_pulses_never_fabricate_a_resting_hand() {
    let mut rig = Rig::new(head());
    rig.head_ready();
    let mut events = Vec::new();
    for start in (550_000..=1_950_000).step_by(100_000) {
        events.extend(rig.gpio(start, false, false, 1));
        events.extend(rig.gpio(start + 20_000, false, false, 1));
        events.extend(rig.gpio(start + 50_000, false, false, 0));
    }
    assert_eq!(events.len(), 15);
    assert!(events.iter().all(|e| e.gesture == TouchGesture::Touch));
    assert!(events.windows(2).all(|e| e[0].contact_id < e[1].contact_id));
}
#[test]
fn authored_head_qualification_target_covers_first_and_repeated_sample_phases() {
    for cadence in [2_000, 5_000, 10_000, 20_000, 50_000] {
        let mut rig = Rig::new(head());
        rig.head_ready();
        for start in [550_000, 650_000] {
            let mut events = Vec::new();
            for at in (start..=start + 50_000).step_by(cadence as usize) {
                events.extend(rig.gpio(at, false, false, 1));
            }
            assert_eq!(events.len(), 1);
            let delay = events[0].observed_us - start;
            assert!((HEAD_QUALIFY_US..=HEAD_QUALIFY_US + cadence).contains(&delay));
            rig.gpio(start + 60_000, false, false, 0);
        }
    }
}
#[test]
fn acquisition_gap_disarms_hold_and_later_released_sample_recovers() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, true, 0);
    let sample = rig.sample(110_000, true, true, 0);
    assert_eq!(
        rig.engine.ingest_gpio(110_000, sample).unwrap_err().fault,
        ControlFault::Stale
    );
    assert!(rig.hold(160_000, 5_160_000, true, true, 0).is_empty());
    assert!(rig.gpio(5_170_000, false, false, 0).is_empty());
    rig.gpio(5_180_000, true, false, 0);
    assert_eq!(rig.gpio(5_190_000, false, false, 0).len(), 1);
}
#[test]
fn lost_acquisition_cannot_turn_hidden_release_repress_into_mute() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, false, 0);
    rig.gpio_seq += 1;
    let sample = rig.sample(20_000, true, false, 0);
    assert_eq!(
        rig.engine.ingest_gpio(20_000, sample).unwrap_err().fault,
        ControlFault::Overflow
    );
    assert!(rig.hold(70_000, 2_070_000, true, false, 0).is_empty());
    assert!(rig.gpio(2_080_000, false, false, 0).is_empty());
}
#[test]
fn source_ttl_withdraws_feedback_at_exact_boundary_without_synthetic_release() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, false, 0);
    let before = rig.engine.tick(109_999);
    assert_eq!(before.state.sources[0].held_us, Some(0));
    let expired = rig.engine.tick(110_000);
    assert!(expired.events.is_empty());
    for source in &expired.state.sources {
        assert_eq!(source.fault, Some(ControlFault::Stale));
        assert_eq!(source.active, None);
        assert_eq!(source.held_us, None);
    }
    expired.state.check().unwrap();
}
#[test]
fn malformed_future_duplicate_and_wrong_clock_inputs_clear_pending() {
    for fault in [
        ControlFault::Future,
        ControlFault::OutOfOrder,
        ControlFault::WrongClock,
        ControlFault::Missing,
        ControlFault::Invalid,
    ] {
        let mut rig = Rig::new(buttons());
        rig.gpio(0, false, false, 0);
        rig.gpio(10_000, true, false, 0);
        let mut sample = rig.sample(20_000, false, false, 0);
        match fault {
            ControlFault::Future => sample.stamp.at_us = 20_001,
            ControlFault::OutOfOrder => sample.stamp.sequence = 2,
            ControlFault::WrongClock => sample.stamp.clock_id = "other".into(),
            ControlFault::Missing => sample.reset = None,
            ControlFault::Invalid => sample.stamp.sequence = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            rig.engine.ingest_gpio(20_000, sample).unwrap_err().fault,
            fault
        );
        assert!(rig.take(20_000).is_empty());
    }
}
#[test]
fn reader_restart_consumes_first_sample_then_requires_release_and_rejects_old_reader() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, false, 0);
    let mut sample = rig.sample(20_000, false, false, 0);
    sample.session_id = "gpio2".into();
    sample.stamp.sequence = 1;
    assert_eq!(
        rig.engine
            .ingest_gpio(20_000, sample.clone())
            .unwrap_err()
            .fault,
        ControlFault::ReaderChanged
    );
    sample.stamp.at_us = 30_000;
    sample.stamp.sequence = 2;
    rig.engine.ingest_gpio(30_000, sample).unwrap();
    assert!(rig.take(30_000).is_empty());
    let sample = rig.sample(40_000, true, false, 0);
    assert_eq!(
        rig.engine.ingest_gpio(40_000, sample).unwrap_err().fault,
        ControlFault::ReaderChanged
    );
}
#[test]
fn backwards_clock_is_terminal_and_near_max_timestamps_do_not_wrap() {
    let mut rig = Rig::new(buttons());
    rig.gpio(u64::MAX - 10_000, false, false, 0);
    rig.gpio(u64::MAX - 1, true, false, 0);
    assert_eq!(rig.gpio(u64::MAX, false, false, 0).len(), 1);
    let state = rig.engine.tick(0).state;
    state.check().unwrap();
    assert!(
        state
            .sources
            .iter()
            .all(|s| s.fault == Some(ControlFault::ClockReset))
    );
    let sample = rig.sample(u64::MAX, false, false, 0);
    assert_eq!(
        rig.engine.ingest_gpio(u64::MAX, sample).unwrap_err().fault,
        ControlFault::ClockReset
    );
}
#[test]
fn gpio_and_strip_faults_do_not_withdraw_the_other_group() {
    let mut config = buttons();
    config.strip_pads = strip().strip_pads;
    let mut rig = Rig::new(config);
    rig.gpio(0, false, false, 0);
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.engine.strip_fault(20_000, ControlFault::Missing);
    rig.gpio(20_000, true, false, 0);
    let events = rig.gpio(30_000, false, false, 0);
    assert_eq!(events.len(), 1);
    rig.strip(30_000, 0);
    rig.strip(40_000, 0);
    rig.engine.gpio_fault(40_000, ControlFault::Missing);
    let state = rig.engine.tick(40_000).state;
    assert!(!state.sources[0].ready);
    assert!(state.sources[2].ready);
    assert!(state.sources[2].armed);
}
#[test]
fn strip_stationary_hold_requests_sleep_once_and_raw_release_is_not_a_tap() {
    let mut rig = Rig::new(strip());
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.strip(20_000, 7);
    let mut events = rig.strip(25_000, 7);
    events.extend(rig.strip(35_000, 7));
    for at in (85_000..=2_035_000).step_by(50_000) {
        events.extend(rig.strip(at, 7));
    }
    assert_eq!(
        events.iter().map(|e| e.when_quiet).collect::<Vec<_>>(),
        [TouchIntent::Attend, TouchIntent::Sleep]
    );
    assert_eq!(events[1].began_us, 20_000);
    assert!(rig.strip(2_040_000, 0).is_empty());
    assert!(rig.strip(2_070_000, 0).is_empty());
}
#[test]
fn strip_brief_qualified_release_and_reversed_mapping_are_preserved() {
    let mut config = strip();
    config.strip_pads.as_mut().unwrap().reverse();
    let mut rig = Rig::new(config);
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.strip(20_000, 7);
    rig.strip(25_000, 7);
    let e = rig.strip(35_000, 0).pop().unwrap();
    assert_eq!(e.released_us, Some(35_000));
    assert_eq!(e.active_mask, 0);
    assert_eq!(e.position, None);
    e.check().unwrap();
}
#[test]
fn strip_restart_and_expiry_cannot_reuse_old_hold() {
    let mut rig = Rig::new(strip());
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.strip(20_000, 7);
    rig.strip(25_000, 7);
    rig.strip(35_000, 7);
    let sample = StripSample {
        stamp: ClockStamp {
            clock_id: "boot".into(),
            at_us: 40_000,
            sequence: 1,
        },
        session_id: "strip2".into(),
        mask: 7,
    };
    assert_eq!(
        rig.engine.ingest_strip(40_000, sample).unwrap_err().fault,
        ControlFault::ReaderChanged
    );
    assert!(rig.engine.tick(40_000).state.latest.is_empty());
    for i in 2..50 {
        let at = 40_000 + (i - 1) * 50_000;
        rig.engine
            .ingest_strip(
                at,
                StripSample {
                    stamp: ClockStamp {
                        clock_id: "boot".into(),
                        at_us: at,
                        sequence: i,
                    },
                    session_id: "strip2".into(),
                    mask: 7,
                },
            )
            .unwrap();
        assert!(rig.take(at).is_empty());
    }
}
#[test]
fn output_queue_is_bounded_and_overflow_withdraws_undelivered_group() {
    let mut rig = Rig::new(buttons());
    let sample = rig.sample(0, false, false, 0);
    rig.engine.ingest_gpio(0, sample).unwrap();
    let mut overflow = None;
    for cycle in 0..65 {
        let start = cycle * 250_000 + 10_000;
        if cycle > 0 {
            for at in [
                start - 200_000,
                start - 150_000,
                start - 100_000,
                start - 50_000,
            ] {
                let s = rig.sample(at, false, false, 0);
                rig.engine.ingest_gpio(at, s).unwrap();
            }
        }
        let s = rig.sample(start, true, false, 0);
        rig.engine.ingest_gpio(start, s).unwrap();
        let s = rig.sample(start + 10_000, false, false, 0);
        if let Err(e) = rig.engine.ingest_gpio(start + 10_000, s) {
            overflow = Some(e.fault);
            break;
        }
    }
    assert_eq!(overflow, Some(ControlFault::Overflow));
    let output = rig.engine.tick(16_020_000);
    assert!(output.events.is_empty());
    assert!(output.state.latest.is_empty());
}
#[test]
fn reader_session_retirement_is_bounded_and_never_reopens_old_identity() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    for i in 1..=32 {
        let mut sample = rig.sample(i * 1_000, false, false, 0);
        sample.session_id = format!("reader{i}");
        sample.stamp.sequence = 1;
        assert_eq!(
            rig.engine.ingest_gpio(i * 1_000, sample).unwrap_err().fault,
            ControlFault::ReaderChanged
        );
    }
    let mut sample = rig.sample(33_000, false, false, 0);
    sample.session_id = "reader33".into();
    sample.stamp.sequence = 1;
    assert_eq!(
        rig.engine.ingest_gpio(33_000, sample).unwrap_err().fault,
        ControlFault::Overflow
    );
}
#[test]
fn pending_event_and_feedback_do_not_gain_new_age_on_heartbeat() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, false, 0);
    let original = rig.gpio(20_000, false, false, 0).pop().unwrap();
    for at in (70_000..=570_000).step_by(50_000) {
        rig.gpio(at, false, false, 0);
    }
    let state = rig.engine.tick(570_000).state;
    assert_eq!(state.latest, std::slice::from_ref(&original));
    assert!(!state.latest[0].fresh_at("boot", 570_000));
}

#[test]
fn strip_raw_handoff_is_active_even_while_filtered_geometry_is_empty() {
    let mut rig = Rig::new(strip());
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.strip(20_000, 1);
    rig.strip(25_000, 1);
    rig.strip(30_000, 1);
    rig.strip(40_000, 2);
    let source = &rig.engine.tick(40_000).state.sources[0];
    assert_eq!(source.active_mask, Some(2));
    assert_eq!(source.active, Some(true));
    assert!(source.contact_id.is_some());
}

#[test]
fn reversed_strip_order_names_actual_destination_electrode() {
    let mut config = strip();
    config.strip_pads.as_mut().unwrap().reverse();
    let mut rig = Rig::new(config);
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.strip(20_000, 7);
    rig.strip(25_000, 7);
    rig.strip(35_000, 7);
    let mut events = rig.strip(40_000, 14);
    events.extend(rig.strip(45_000, 14));
    let slide = events
        .iter()
        .find(|event| event.gesture == TouchGesture::Slide)
        .unwrap();
    assert_eq!(slide.toward_pad, Some(11));
    assert_eq!(slide.when_asleep, None);
}

#[test]
fn qualified_head_touch_offers_wake_without_changing_awake_pair() {
    let mut rig = Rig::new(head());
    rig.head_ready();
    rig.gpio(510_000, false, false, 1);
    let event = rig.gpio(530_000, false, false, 1).pop().unwrap();
    assert_eq!(event.intent(false, true), Some(TouchIntent::Wake));
    assert_eq!(event.intent(false, false), Some(TouchIntent::Attend));
    assert_eq!(event.intent(true, false), Some(TouchIntent::Stop));
}

#[test]
fn invalid_head_snapshot_cannot_omit_or_duplicate_configured_inputs() {
    for missing in [false, true] {
        let mut rig = Rig::new(head());
        rig.head_ready();
        rig.gpio(510_000, false, false, 1);
        let mut sample = rig.sample(530_000, false, false, 1);
        if missing {
            sample.head_pads.pop();
        } else {
            sample.head_pads[1] = sample.head_pads[0].clone();
        }
        assert!(rig.engine.ingest_gpio(530_000, sample).is_err());
        let output = rig.engine.tick(530_000);
        assert!(output.events.is_empty());
        assert_eq!(output.state.sources[0].active, None);
    }
}

#[test]
fn gpio_fault_preserves_independent_strip_request_identity() {
    let mut config = buttons();
    config.strip_pads = strip().strip_pads;
    let mut rig = Rig::new(config);
    rig.gpio(0, false, false, 0);
    rig.strip(0, 0);
    rig.strip(10_000, 0);
    rig.strip(20_000, 7);
    rig.strip(25_000, 7);
    let event = rig.strip(35_000, 7).pop().unwrap();
    rig.engine.gpio_fault(40_000, ControlFault::Missing);
    let output = rig.engine.tick(40_000);
    assert_eq!(output.state.latest, [event]);
    assert!(output.state.sources[2].ready);
}

#[test]
fn physical_button_events_preserve_stop_mute_and_reset_asleep() {
    let mut rig = Rig::new(buttons());
    rig.gpio(0, false, false, 0);
    rig.gpio(10_000, true, false, 0);
    let tap = rig.gpio(20_000, false, false, 0).pop().unwrap();
    assert_eq!(tap.intent(false, true), Some(TouchIntent::Stop));
    rig.hold(70_000, 270_000, false, false, 0);
    assert!(rig.hold(320_000, 5_320_000, true, true, 0).is_empty());
    let events = rig.gpio(5_320_001, false, false, 0);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].source, TouchSource::Primary);
    assert_eq!(
        events[0].intent(false, true),
        Some(TouchIntent::SoftwareMute)
    );
    assert_eq!(events[1].source, TouchSource::Reset);
    assert_eq!(
        events[1].intent(false, true),
        Some(TouchIntent::FactoryReset)
    );
    assert_eq!(events[1].released_us, Some(5_320_001));
}
