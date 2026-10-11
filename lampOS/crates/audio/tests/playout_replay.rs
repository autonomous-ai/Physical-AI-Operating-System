//! Recorded provider arrival schedules replayed through the start gate.
//!
//! The schedules are timings from device runs: when each provider audio event
//! arrived, measured from the reply's first, and how many 24 kHz frames it
//! carried. They hold no audio and no words.
//!
//! `play` is a model of the hand-over, not the coordinator and not the speaker
//! loop. It keeps their two rules: the queue gives up one block per 10 ms slot
//! of a clock that never stops and withholds its last block until the reply is
//! complete, and a slot with no speech in hand is filled with zeros. The first
//! test checks that model against the gaps the devices recorded.
use lamp_audio::{
    RENDER_SAMPLES,
    playout::{START_HORIZON_US, StartGate, StartNeed},
};

const SLOT_US: u64 = 10_000;
/// Provider event to the first of its audio in the reply queue. Recorded
/// values were 1.2 ms to 3.4 ms.
const ENQUEUE_US: u64 = 3_000;
const BLOCK: u64 = RENDER_SAMPLES as u64;

#[derive(Debug, PartialEq, Eq)]
struct Gap {
    start_us: u64,
    zero_frames: u64,
}

#[derive(Debug)]
struct Run {
    first_speech_us: u64,
    gaps: Vec<Gap>,
    end_us: u64,
}

impl Run {
    fn zero_ms(&self) -> u64 {
        self.gaps.iter().map(|gap| gap.zero_frames).sum::<u64>() / 24
    }
}

/// Speech frames handed to the speaker by `until_us`, and the run if the
/// reply was played to its end before then.
fn play_until(
    arrivals: &[(u64, u64)],
    horizon_us: u64,
    phase_us: u64,
    until_us: u64,
) -> (u64, Option<Run>) {
    let total: u64 = arrivals.iter().map(|(_, frames)| frames).sum();
    let mut gate = StartGate::new(horizon_us);
    let (mut queued, mut next, mut played) = (0_u64, 0_usize, 0_u64);
    let mut first_speech_us = None;
    let mut gaps: Vec<Gap> = Vec::new();
    let mut in_gap = false;
    let mut slot = phase_us;
    while played < total {
        if slot >= until_us {
            return (played, None);
        }
        while let Some((at_us, frames)) = arrivals.get(next).filter(|(at_us, _)| *at_us <= slot) {
            gate.audio_arrived(*at_us);
            queued += frames;
            next += 1;
        }
        // The generation's completion follows its last audio.
        let complete = next == arrivals.len();
        let ready = queued > BLOCK || (complete && queued > 0);
        if ready && gate.open(slot, queued as usize, complete) {
            let frames = queued.min(BLOCK);
            queued -= frames;
            played += frames;
            first_speech_us.get_or_insert(slot);
            in_gap = false;
        } else if first_speech_us.is_some() {
            if in_gap {
                gaps.last_mut().unwrap().zero_frames += BLOCK;
            } else {
                gaps.push(Gap {
                    start_us: slot,
                    zero_frames: BLOCK,
                });
                in_gap = true;
            }
        }
        slot += SLOT_US;
    }
    let run = Run {
        first_speech_us: first_speech_us.unwrap(),
        gaps,
        end_us: slot,
    };
    (played, Some(run))
}

fn play(arrivals: &[(u64, u64)], horizon_us: u64, phase_us: u64) -> Run {
    play_until(arrivals, horizon_us, phase_us, u64::MAX)
        .1
        .unwrap()
}

/// Provider events with the enqueue delay applied.
fn queued(events: &[(u64, u64)]) -> Vec<(u64, u64)> {
    events
        .iter()
        .map(|(at_us, frames)| (at_us + ENQUEUE_US, *frames))
        .collect()
}

fn phases() -> impl Iterator<Item = u64> {
    (0..SLOT_US).step_by(500)
}

struct Reply {
    run: &'static str,
    /// First speech accepted by ALSA, from the first provider audio event.
    first_write_us: u64,
    /// Zeros accepted while the reply was unfinished and its queue was empty.
    startup_zero_ms: u64,
    events: &'static [(u64, u64)],
}

/// Every reply with provider audio in the device traces available on
/// 2026-10-11: nine sessions, seventeen replies. The last three are that
/// day's Rust runs: the first pilot and two with noise suppression off. Three
/// further 10 ms gaps in the earlier runs happened with 16,080 to 60,960
/// frames queued. Those are not arrival gaps and are not listed.
const RECORDED: &[Reply] = &[
    Reply {
        run: "rubc3c4q/1",
        first_write_us: 177_280,
        startup_zero_ms: 0,
        events: &[
            (0, 1),
            (168_971, 4_800),
            (304_231, 3_840),
            (441_583, 6_720),
            (441_608, 1_920),
            (471_671, 5_760),
            (471_681, 3_840),
            (476_133, 3_840),
            (500_797, 3_840),
            (579_480, 14_400),
            (608_014, 7_680),
            (641_484, 8_640),
        ],
    },
    Reply {
        run: "rubc3c4q/2",
        first_write_us: 12_881,
        startup_zero_ms: 10,
        events: &[
            (0, 1),
            (6_396, 3_840),
            (176_125, 7_680),
            (306_935, 6_720),
            (333_694, 5_760),
            (341_232, 6_720),
            (355_185, 4_800),
            (443_320, 6_720),
            (469_048, 4_800),
            (494_928, 4_800),
        ],
    },
    Reply {
        run: "rubc3c4q/3",
        first_write_us: 11_573,
        startup_zero_ms: 110,
        events: &[
            (0, 1_440),
            (163_658, 4_800),
            (297_548, 3_840),
            (331_161, 5_760),
            (437_600, 5_760),
            (466_144, 6_720),
            (466_155, 5_760),
            (496_649, 4_800),
            (499_510, 4_800),
            (570_275, 6_720),
            (571_680, 3_840),
            (598_363, 5_760),
        ],
    },
    Reply {
        run: "rubc3c4q/4",
        first_write_us: 39_062,
        startup_zero_ms: 60,
        events: &[
            (0, 1),
            (26_249, 2_400),
            (191_047, 4_800),
            (327_572, 4_800),
            (334_408, 3_840),
            (385_119, 7_680),
            (386_527, 5_760),
            (506_618, 4_800),
            (506_640, 5_760),
        ],
    },
    Reply {
        run: "rubc3c4q/5",
        first_write_us: 9_542,
        startup_zero_ms: 320,
        events: &[
            (0, 1_920),
            (378_529, 8_640),
            (380_712, 7_680),
            (519_440, 16_320),
            (542_792, 7_680),
            (653_810, 7_680),
            (655_526, 11_520),
            (682_360, 9_600),
            (732_338, 5_760),
        ],
    },
    Reply {
        run: "fvackatg/1",
        first_write_us: 11_294,
        startup_zero_ms: 0,
        events: &[
            (0, 4_320),
            (171_990, 8_640),
            (335_346, 5_760),
            (348_013, 9_600),
            (348_883, 3_840),
        ],
    },
    Reply {
        run: "fvackatg/2",
        first_write_us: 14_433,
        startup_zero_ms: 0,
        events: &[
            (0, 1),
            (7_464, 3_120),
            (139_912, 4_800),
            (195_822, 5_760),
            (278_863, 4_800),
            (312_018, 12_480),
            (415_305, 8_640),
            (501_297, 10_560),
            (569_114, 3_840),
        ],
    },
    Reply {
        run: "5koru8e0/1",
        first_write_us: 9_578,
        startup_zero_ms: 90,
        events: &[
            (0, 2_160),
            (171_879, 3_840),
            (305_803, 3_840),
            (337_789, 4_800),
            (341_769, 3_840),
            (345_567, 3_840),
            (474_194, 5_760),
            (513_143, 14_400),
            (614_537, 17_280),
            (758_467, 17_280),
            (827_581, 7_680),
            (943_779, 11_520),
            (1_002_197, 7_680),
            (1_048_635, 3_840),
        ],
    },
    Reply {
        run: "l3yc4zsy/1",
        first_write_us: 200_087,
        startup_zero_ms: 0,
        events: &[
            (0, 1),
            (187_781, 5_040),
            (314_202, 4_800),
            (314_226, 3_840),
            (445_182, 3_840),
            (445_188, 960),
            (449_261, 5_760),
            (515_437, 12_480),
            (581_839, 5_760),
            (589_257, 7_680),
            (644_148, 8_640),
            (659_928, 7_680),
            (798_134, 7_680),
            (827_840, 9_600),
            (911_616, 7_680),
            (996_677, 7_680),
            (1_086_448, 8_640),
        ],
    },
    Reply {
        run: "j5_xqe32/1",
        first_write_us: 3_605,
        startup_zero_ms: 140,
        events: &[
            (0, 960),
            (168_788, 3_840),
            (305_188, 5_760),
            (437_543, 4_800),
            (437_552, 2_880),
            (470_963, 12_480),
            (576_843, 8_640),
            (586_613, 15_360),
            (635_103, 11_520),
            (648_057, 2_880),
        ],
    },
    Reply {
        run: "j5_xqe32/2",
        first_write_us: 142_165,
        startup_zero_ms: 0,
        events: &[
            (0, 1),
            (132_764, 4_800),
            (167_991, 3_840),
            (193_975, 3_840),
            (300_465, 4_800),
            (339_312, 9_600),
            (405_160, 7_680),
            (470_118, 7_680),
            (521_840, 10_560),
            (602_199, 8_640),
            (683_440, 7_680),
            (770_921, 9_600),
            (822_704, 4_800),
        ],
    },
    Reply {
        run: "j5_xqe32/3",
        first_write_us: 191_671,
        startup_zero_ms: 0,
        events: &[
            (0, 1),
            (179_998, 5_040),
            (304_502, 4_800),
            (338_089, 5_760),
            (441_247, 2_880),
            (472_568, 7_680),
            (473_504, 4_800),
            (505_525, 5_760),
            (577_509, 4_800),
            (583_636, 10_560),
            (604_461, 7_680),
            (676_085, 7_680),
            (735_309, 7_680),
            (755_852, 2_880),
        ],
    },
    Reply {
        run: "g2kp1mcp/1",
        first_write_us: 13_180,
        startup_zero_ms: 130,
        events: &[
            (0, 960),
            (169_849, 3_840),
            (307_997, 7_680),
            (443_780, 3_840),
            (443_788, 1_920),
            (448_419, 5_760),
            (449_694, 3_840),
            (470_910, 3_840),
            (512_474, 3_840),
            (580_056, 6_720),
            (593_995, 15_360),
            (623_425, 8_640),
            (715_196, 7_680),
            (795_034, 7_680),
        ],
    },
    Reply {
        run: "g2kp1mcp/2",
        first_write_us: 66_029,
        startup_zero_ms: 0,
        events: &[
            (0, 1),
            (52_710, 4_800),
            (230_007, 7_680),
            (269_549, 5_760),
            (355_174, 3_840),
            (387_078, 5_760),
            (391_261, 2_880),
        ],
    },
    Reply {
        run: "first-rust/1",
        first_write_us: 24_775,
        startup_zero_ms: 30,
        events: FIRST_RUST_EVENTS,
    },
    Reply {
        run: "ns-off/1",
        first_write_us: 398_754,
        startup_zero_ms: 0,
        events: &[
            (0, 1),
            (391_096, 7_440),
            (392_577, 3_840),
            (520_038, 7_680),
            (773_986, 7_680),
            (1_091_564, 3_840),
            (1_261_887, 8_640),
            (1_610_245, 3_840),
            (1_773_097, 5_760),
            (2_013_930, 4_800),
            (2_218_375, 4_800),
            (2_417_239, 8_640),
            (2_904_218, 11_520),
            (3_258_151, 9_600),
        ],
    },
    Reply {
        run: "ns-off/2",
        first_write_us: 12_267,
        startup_zero_ms: 260,
        events: &[
            (0, 1_200),
            (302_724, 5_760),
            (337_122, 8_640),
            (550_436, 2_880),
            (676_240, 8_640),
            (1_035_692, 8_640),
            (1_391_507, 6_720),
            (1_676_890, 10_560),
            (2_112_180, 8_640),
            (2_478_459, 7_680),
            (2_792_838, 8_640),
            (3_150_143, 1_920),
        ],
    },
];

const FIRST_RUST_EVENTS: &[(u64, u64)] = &[
    (0, 1),
    (14_558, 3_120),
    (171_828, 5_760),
    (275_060, 3_840),
    (435_788, 5_760),
    (674_810, 3_840),
    (839_944, 3_840),
    (997_141, 6_720),
];

/// The same reply as the coordinator enqueued it: 36 rows, each when its
/// frames reached the reply queue.
const FIRST_RUST_ENQUEUED: &[(u64, u64)] = &[
    (1_904, 1),
    (16_500, 960),
    (16_691, 960),
    (18_519, 960),
    (18_586, 240),
    (175_227, 960),
    (175_415, 960),
    (179_282, 960),
    (179_455, 960),
    (181_308, 960),
    (181_547, 960),
    (278_219, 960),
    (278_408, 960),
    (280_249, 960),
    (280_424, 960),
    (438_950, 960),
    (439_132, 960),
    (443_061, 960),
    (443_236, 960),
    (445_125, 960),
    (445_330, 960),
    (675_970, 960),
    (676_709, 960),
    (680_046, 960),
    (680_743, 960),
    (843_045, 960),
    (843_223, 960),
    (845_084, 960),
    (845_251, 960),
    (999_613, 960),
    (999_799, 960),
    (1_003_753, 960),
    (1_003_945, 960),
    (1_007_861, 960),
    (1_008_020, 960),
    (1_009_932, 960),
];
const FIRST_RUST_FIRST_WRITE_US: u64 = 24_775;

#[test]
fn without_a_hold_the_model_reproduces_the_gaps_the_devices_recorded() {
    for reply in RECORDED {
        let run = play(&queued(reply.events), 0, reply.first_write_us % SLOT_US);
        assert!(
            run.first_speech_us.abs_diff(reply.first_write_us) <= SLOT_US,
            "{}: first speech at {} us, recorded {} us",
            reply.run,
            run.first_speech_us,
            reply.first_write_us
        );
        assert!(
            run.zero_ms().abs_diff(reply.startup_zero_ms) <= 10,
            "{}: {} ms of zeros, recorded {} ms",
            reply.run,
            run.zero_ms(),
            reply.startup_zero_ms
        );
    }
    let recorded: u64 = RECORDED.iter().map(|reply| reply.startup_zero_ms).sum();
    assert_eq!(recorded, 1_150);
    assert_eq!(
        RECORDED
            .iter()
            .filter(|reply| reply.startup_zero_ms > 0)
            .count(),
        9
    );
}

#[test]
fn the_first_rust_pilot_gap_is_reproduced_from_its_enqueue_rows() {
    let run = play(FIRST_RUST_ENQUEUED, 0, FIRST_RUST_FIRST_WRITE_US % SLOT_US);
    assert_eq!(run.first_speech_us, FIRST_RUST_FIRST_WRITE_US);
    // Recorded: zeros from 154,618 us, 720 frames, speech again at 183,470 us.
    // Thirteen blocks of speech were out when the queue held one frame.
    assert_eq!(
        run.gaps,
        [Gap {
            start_us: FIRST_RUST_FIRST_WRITE_US + 13 * SLOT_US,
            zero_frames: 720,
        }]
    );
}

#[test]
fn the_horizon_plays_the_first_rust_pilot_reply_whole_at_every_clock_phase() {
    let recorded = play(FIRST_RUST_ENQUEUED, 0, FIRST_RUST_FIRST_WRITE_US % SLOT_US);
    for phase in (0..SLOT_US).step_by(250) {
        let run = play(FIRST_RUST_ENQUEUED, START_HORIZON_US, phase);
        assert_eq!(run.gaps, [], "phase {phase}");
        // 130 ms in hand 16.7 ms after the first audio: held to 70 ms after
        // it, then the next slot.
        assert!(
            (72_000..82_000).contains(&run.first_speech_us),
            "phase {phase}: {}",
            run.first_speech_us
        );
    }
    let held = play(
        FIRST_RUST_ENQUEUED,
        START_HORIZON_US,
        FIRST_RUST_FIRST_WRITE_US % SLOT_US,
    );
    assert_eq!(held.first_speech_us, recorded.first_speech_us + 50_000);
    assert_eq!(held.end_us, recorded.end_us + 20_000);
}

#[test]
fn the_smallest_horizon_that_covers_the_first_rust_pilot_reply_is_180_ms() {
    let worst = |horizon_us| {
        (0..SLOT_US)
            .step_by(250)
            .map(|phase| play(FIRST_RUST_ENQUEUED, horizon_us, phase).zero_ms())
            .max()
            .unwrap()
    };
    assert_eq!(
        [0, 100_000, 150_000, 160_000, 170_000, 180_000, 200_000].map(worst),
        [30, 30, 30, 20, 10, 0, 0]
    );
}

#[test]
fn the_horizon_removes_every_recorded_startup_gap_but_the_two_the_service_stalled() {
    let mut without = Vec::new();
    for reply in RECORDED {
        let arrivals = queued(reply.events);
        let worst_without = phases()
            .map(|phase| play(&arrivals, 0, phase).zero_ms())
            .max()
            .unwrap();
        if worst_without > 0 {
            without.push(reply.run);
        }
        let worst_with = phases()
            .map(|phase| play(&arrivals, START_HORIZON_US, phase).zero_ms())
            .max()
            .unwrap();
        // The second burst of these two came 378.5 ms after 80 ms of audio
        // and 302.7 ms after 50 ms. No start within the horizon covers that;
        // the gap is shorter.
        match reply.run {
            "rubc3c4q/5" => assert_eq!((worst_without, worst_with), (310, 190)),
            "ns-off/2" => assert_eq!((worst_without, worst_with), (270, 120)),
            _ => assert_eq!(worst_with, 0, "{}", reply.run),
        }
    }
    // Two of these eleven did not gap at the phase their device happened to
    // be in.
    assert_eq!(
        without,
        [
            "rubc3c4q/2",
            "rubc3c4q/3",
            "rubc3c4q/4",
            "rubc3c4q/5",
            "fvackatg/1",
            "fvackatg/2",
            "5koru8e0/1",
            "j5_xqe32/1",
            "g2kp1mcp/1",
            "first-rust/1",
            "ns-off/2",
        ]
    );
}

#[test]
fn a_190_ms_horizon_is_the_edge_and_180_ms_is_short() {
    let gapped = |horizon_us| {
        RECORDED
            .iter()
            .filter(|reply| {
                let arrivals = queued(reply.events);
                phases().any(|phase| play(&arrivals, horizon_us, phase).zero_ms() > 0)
            })
            .map(|reply| reply.run)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        gapped(180_000),
        [
            "rubc3c4q/4",
            "rubc3c4q/5",
            "fvackatg/1",
            "5koru8e0/1",
            "ns-off/2"
        ]
    );
    assert_eq!(gapped(190_000), ["rubc3c4q/4", "rubc3c4q/5", "ns-off/2"]);
    assert_eq!(gapped(START_HORIZON_US), ["rubc3c4q/5", "ns-off/2"]);
}

#[test]
fn the_hold_costs_the_start_what_the_gap_cost_the_middle() {
    let (mut added_ms, mut end_ms, mut latest_with, mut latest_without) = (0, 0, 0, 0);
    for reply in RECORDED {
        let arrivals = queued(reply.events);
        let phase = reply.first_write_us % SLOT_US;
        let without = play(&arrivals, 0, phase);
        let with = play(&arrivals, START_HORIZON_US, phase);
        let added = with.first_speech_us - without.first_speech_us;
        let removed = (without.zero_ms() - with.zero_ms()) * 1_000;
        // Every millisecond of hold is either a millisecond of zeros that no
        // longer falls inside the reply or a later end. Never both.
        assert_eq!(
            with.end_us - without.end_us,
            added - removed,
            "{}",
            reply.run
        );
        assert!(added <= 160_000, "{}: {added}", reply.run);
        // No start is later than the horizon plus the enqueue and one slot,
        // unless the reply had nothing to play until later than that anyway.
        assert!(
            with.first_speech_us
                <= without
                    .first_speech_us
                    .max(START_HORIZON_US + ENQUEUE_US + SLOT_US),
            "{}",
            reply.run
        );
        added_ms += added / 1_000;
        end_ms += (with.end_us - without.end_us) / 1_000;
        latest_with = latest_with.max(with.first_speech_us);
        latest_without = latest_without.max(without.first_speech_us);
    }
    // Over the seventeen replies: 64 ms later to the first speech on average,
    // 14 ms later to the end.
    assert_eq!((added_ms, end_ms), (1_080, 230));
    // The slowest start is one the service already produced without a hold.
    assert_eq!(latest_with, latest_without);
}

#[test]
fn a_stall_later_in_the_reply_is_a_gap_under_any_start_and_is_not_held_again() {
    // The first Rust pilot reply with everything from its fourth event on
    // arriving 400 ms late.
    let stalled: Vec<(u64, u64)> = FIRST_RUST_ENQUEUED
        .iter()
        .map(|(at_us, frames)| (at_us + if *at_us >= 278_219 { 400_000 } else { 0 }, *frames))
        .collect();
    let phase = FIRST_RUST_FIRST_WRITE_US % SLOT_US;
    let without = play(&stalled, 0, phase);
    let with = play(&stalled, START_HORIZON_US, phase);
    assert_eq!(
        without
            .gaps
            .iter()
            .map(|gap| gap.zero_frames)
            .collect::<Vec<_>>(),
        [720, 6_240]
    );
    assert_eq!(
        with.gaps,
        [Gap {
            start_us: 444_775,
            zero_frames: 5_760,
        }]
    );
    // Speech resumes in the first slot after the late audio, and the reply
    // ends when it would have ended anyway.
    let resumed = with.gaps[0].start_us + 5_760 / BLOCK * SLOT_US;
    assert!(resumed - 678_219 < SLOT_US);
    assert_eq!(with.end_us, without.end_us);
}

#[test]
fn a_cancellation_during_the_hold_finds_nothing_handed_to_the_speaker() {
    let phase = FIRST_RUST_FIRST_WRITE_US % SLOT_US;
    assert_eq!(
        play_until(FIRST_RUST_ENQUEUED, START_HORIZON_US, phase, 60_000).0,
        0
    );
    // Without the hold four blocks had already left the queue by then.
    assert_eq!(play_until(FIRST_RUST_ENQUEUED, 0, phase, 60_000).0, 960);
}

#[test]
fn each_recorded_reply_reports_the_horizon_it_needed() {
    let needed: Vec<u64> = RECORDED
        .iter()
        .map(|reply| {
            let mut need = StartNeed::default();
            for (at_us, frames) in reply.events {
                need.arrival(*at_us, *frames as usize);
            }
            need.horizon_us() / 1_000
        })
        .collect();
    // Thirteen of seventeen between 132 ms and 191 ms; one early; three
    // past 300 ms, two of them on the day's own runs.
    assert_eq!(
        needed,
        [
            168, 176, 163, 191, 378, 0, 139, 171, 187, 168, 132, 179, 169, 52, 171, 391, 302
        ]
    );
}
