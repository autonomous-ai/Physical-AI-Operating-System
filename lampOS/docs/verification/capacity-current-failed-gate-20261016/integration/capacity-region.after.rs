    // ---- Speaking speed through the output capacity contract ------------
    //
    // Fixture time drives modeled playback and the scripted service; IPC and
    // authority freshness stay on the host clock. The harness plays the
    // coordinator exactly as `provider-flow-control.md` specifies:
    // every received packet is counted, and capacity is granted from the free
    // space of a reply buffer that drains at 24,000 samples per second.

    const LARGEST: usize = lamp_gemini::MAX_OUTPUT_SAMPLES;
    const REPLY_BUFFER: usize = crate::provider_flow::MAX_REPLY_SAMPLES;

    /// The coordinator's reply buffer and speaker, reduced to their arithmetic.
    /// Playback advances by fixture time only: time spent held or with
    /// nothing to play is never made up afterwards.
    struct Speaker {
        /// Samples the buffer may hold. The real coordinator has 30 s; a test
        /// may give it less so a short answer still exhausts capacity.
        room: usize,
        received: u64,
        played: u64,
        /// When playback was last advanced; set by the first audio.
        at: Option<tokio::time::Instant>,
        /// Microsecond-samples not yet amounting to a whole sample.
        fraction: u64,
        held: Duration,
        /// Samples of playing time lost with an empty buffer.
        starved: u64,
        /// The largest backlog ever held.
        peak: usize,
    }
    impl Speaker {
        fn new(room: usize) -> Self {
            Self {
                room,
                received: 0,
                played: 0,
                at: None,
                fraction: 0,
                held: Duration::ZERO,
                starved: 0,
                peak: 0,
            }
        }
        fn hear(&mut self, samples: usize) {
            self.at.get_or_insert_with(tokio::time::Instant::now);
            self.received += samples as u64;
        }
        /// Advance playback to now, unless it is on `hold`, and return the
        /// backlog.
        fn backlog(&mut self, hold: bool) -> usize {
            if let Some(last) = self.at {
                let now = tokio::time::Instant::now();
                self.at = Some(now);
                let elapsed = now - last;
                if hold {
                    self.held += elapsed;
                } else {
                    self.fraction += elapsed.as_micros() as u64 * 24_000;
                    let due = self.fraction / 1_000_000;
                    self.fraction %= 1_000_000;
                    let waiting = self.received - self.played;
                    self.played += due.min(waiting);
                    self.starved += due.saturating_sub(waiting);
                }
            }
            let backlog = (self.received - self.played) as usize;
            self.peak = self.peak.max(backlog);
            backlog
        }
        /// Cancel what is queued, as the coordinator does on a new admission.
        fn flush(&mut self) {
            self.played = self.received;
        }
    }
    impl LiveRig {
        /// One coordinator tick: heartbeat, then capacity from the speaker's
        /// free space. A smaller `room` is expressed as space already taken.
        fn grant(&mut self, speaker: &mut Speaker, hold: bool) {
            self.publish();
            let backlog = speaker.backlog(hold);
            assert!(backlog <= speaker.room, "reply buffer overran: {backlog}");
            self.output_flow
                .replenish(
                    REPLY_BUFFER - speaker.room + backlog,
                    &mut self.parent.control,
                )
                .unwrap();
        }
        /// Every packet that has arrived, each counted against capacity. An
        /// excess over what was granted fails here.
        fn arrived(&mut self) -> Vec<Wire> {
            let mut all = Vec::new();
            while let Some(wire) = self.parent.data.receive::<Wire>().unwrap() {
                self.output_flow.received().unwrap();
                all.push(wire);
            }
            all
        }
    }
    struct Playback {
        packets: u64,
        frames: usize,
        /// First tick to the idle completion arriving.
        completed: Duration,
        /// First tick to the last sample leaving the speaker.
        finished: Duration,
        held: Duration,
        starved: Duration,
        peak: usize,
    }
    impl LiveRig {
        /// Take one answer of `total` samples at speaking speed, to the end of
        /// its playback. Playback stops once, for `pause`, after `pause_after`
        /// samples have been played.
        async fn play_at_speaking_speed(
            &mut self,
            request: u64,
            room: usize,
            total: u64,
            pause_after: u64,
            pause: Duration,
        ) -> Playback {
            let mut speaker = Speaker::new(room);
            let started = tokio::time::Instant::now();
            let (mut packets, mut frames) = (0u64, 0usize);
            let mut completed = None;
            // When the last sample left the speaker, and the playing time
            // lost to an empty buffer up to then.
            let mut finished = None;
            loop {
                let hold = speaker.played >= pause_after && speaker.held < pause;
                self.grant(&mut speaker, hold);
                for wire in self.arrived() {
                    assert!(completed.is_none(), "output after completion: {wire:?}");
                    match wire {
                        Wire::Output(ProviderOutput::Audio {
                            request: owner,
                            source_offset,
                            samples,
                            ..
                        }) => {
                            assert_eq!(owner, request);
                            // Every block carries its message index: nothing
                            // lost, repeated or reordered.
                            let expected = (frames / LARGEST) as i16;
                            assert_eq!(source_offset, frames % LARGEST);
                            assert!(samples.iter().all(|sample| *sample == expected));
                            frames += samples.len();
                            packets += 1;
                            speaker.hear(samples.len());
                        }
                        Wire::Output(ProviderOutput::GenerationComplete { .. }) => {}
                        Wire::Output(ProviderOutput::TurnComplete { idle: true, .. }) => {
                            completed = Some(started.elapsed());
                        }
                        other => panic!("unexpected at speaking speed: {other:?}"),
                    }
                }
                if finished.is_none() && speaker.played == total {
                    finished = Some((started.elapsed(), speaker.starved));
                }
                if let (Some(completed), Some((finished, starved))) = (completed, finished) {
                    return Playback {
                        packets,
                        frames,
                        completed,
                        finished,
                        held: speaker.held,
                        starved: Duration::from_micros(starved * 1_000_000 / 24_000),
                        peak: speaker.peak,
                    };
                }
                assert!(!self.worker.is_finished(), "worker ended mid-answer");
                assert!(
                    started.elapsed() < Duration::from_secs(total / 24_000 + 120),
                    "answer never finished"
                );
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    }
    /// How long the scripted network stalls inside a long answer, and how many
    /// largest messages then arrive with no gap between them.
    const STALL: Duration = Duration::from_secs(3);
    const BURST: u64 = 5;

    /// The service side of one long answer: largest messages at ten times
    /// speaking speed, a network stall a third of the way in followed by a
    /// ten-second burst, pings answered throughout, and idle completion
    /// withheld for the answer's own duration, as the service assumes
    /// playback. Returns how long the writing took, and the service: dropping
    /// it would be a connection loss.
    fn long_answer(
        mut service: lamp_gemini::testing::FakeService,
        seconds: u64,
    ) -> JoinHandle<(Duration, lamp_gemini::testing::FakeService)> {
        tokio::spawn(async move {
            let started = tokio::time::Instant::now();
            let messages = seconds / 2;
            let stall_at = messages / 3;
            for index in 0..messages {
                if index == stall_at {
                    service.rest(STALL).await;
                }
                service.send(provider_audio(index as i16, LARGEST)).await;
                if !(stall_at..stall_at + BURST - 1).contains(&index) {
                    service.rest(Duration::from_millis(200)).await;
                }
            }
            let written = started.elapsed();
            service
                .send(json!({"serverContent":{"generationComplete":true}}))
                .await;
            service
                .rest(Duration::from_secs(seconds).saturating_sub(started.elapsed()))
                .await;
            service.send(idle_complete()).await;
            (written, service)
        })
    }
    /// A `seconds` long answer through a reply buffer of `room` samples, with
    /// playback held for `pause` once five seconds have been played.
    async fn speaking_speed(seconds: u64, room: usize, pause: Duration) {
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let request = rig.speak(&mut dial, false, true).await;
        let script = long_answer(dial.service, seconds);
        let total = seconds * 24_000;
        let played = rig
            .play_at_speaking_speed(request, room, total, 5 * 24_000, pause)
            .await;
        let (written, _service) = script.await.unwrap();
        assert_eq!(played.frames as u64, total);
        assert_eq!(played.packets, seconds * 25);
        println!(
            "FIXTURE speaking_speed seconds={seconds} reply_buffer_s={:.1} hold_ms={} playback_finished_ms={} completion_received_ms={} starved_ms={} service_wrote_in_ms={} packets={} peak_backlog_s={:.2} (scripted service through output capacity to a local IPC reader and a modeled speaker, fixture clock)",
            room as f64 / 24_000.0,
            played.held.as_millis(),
            played.finished.as_millis(),
            played.completed.as_millis(),
            played.starved.as_millis(),
            written.as_millis(),
            played.packets,
            played.peak as f64 / 24_000.0
        );
        // Playback was held for the whole pause and never ran dry: the stall
        // and the pause were both absorbed by what was already buffered.
        assert!(played.held >= pause, "{:?}", played.held);
        assert!(
            played.starved < Duration::from_millis(100),
            "{:?}",
            played.starved
        );
        // So the answer took its own length plus the pause, and no longer.
        let expected = Duration::from_secs(seconds) + played.held + played.starved;
        assert!(
            played.finished >= expected && played.finished <= expected + Duration::from_secs(1),
            "{:?} against {expected:?}",
            played.finished
        );
        // Its completion needs capacity like any packet and still arrives.
        assert!(played.completed <= played.finished + Duration::from_secs(1));
        // Capacity really was exhausted: the buffer filled and stayed bounded.
        assert!(played.peak <= room);
        if total as usize > room {
            assert!(played.peak + 2 * PACKET_SAMPLES >= room, "{}", played.peak);
        }
        // The service was not slowed to playback speed.
        assert!(
            written <= STALL + Duration::from_secs(seconds / 4 + 3),
            "{written:?}"
        );
        rig.parent.control.send(Control::Stop).unwrap();
        assert!(rig.exit().await.0.is_ok());
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity() {
        // A small buffer so a short answer exhausts capacity for most of its
        // length. Playback also stops for four seconds with the buffer full.
        speaking_speed(20, 5 * 24_000, Duration::from_secs(4)).await;
    }

    #[tokio::test]
    #[ignore = "one minute of real time; run explicitly"]
    async fn sixty_second_answer_follows_output_capacity() {
        speaking_speed(60, REPLY_BUFFER, Duration::from_secs(10)).await;
    }

    #[tokio::test]
    #[ignore = "three minutes of real time; run explicitly"]
    async fn three_minute_answer_follows_output_capacity() {
        speaking_speed(180, REPLY_BUFFER, Duration::from_secs(20)).await;
    }

    #[tokio::test]
    async fn interrupting_a_capacity_paced_answer_is_not_delayed_by_its_queued_audio() {
        let answer: Vec<_> = (0..20)
            .map(|index| provider_audio(index, LARGEST))
            .collect();
        let mut rig = LiveRig::start(quick_policy());
        let mut dial = rig.ready().await;
        let old = rig.speak(&mut dial, false, true).await;
        // Forty seconds generated at once into a two-second reply buffer: the
        // worker holds its full backlog and the transport holds the rest.
        let mut speaker = Speaker::new(2 * 24_000);
        // One coordinator tick while only the old answer may arrive.
        let hear = |rig: &mut LiveRig, speaker: &mut Speaker| {
            rig.grant(speaker, false);
            for wire in rig.arrived() {
                let Wire::Output(ProviderOutput::Audio {
                    request, samples, ..
                }) = wire
                else {
                    panic!("audio only");
                };
                assert_eq!(request, old);
                speaker.hear(samples.len());
            }
        };
        for message in answer {
            dial.service.send(message).await;
            hear(&mut rig, &mut speaker);
        }
        let started = Instant::now();
        while speaker.played < 2 * 24_000 {
            hear(&mut rig, &mut speaker);
            assert!(
                started.elapsed() < Duration::from_secs(8),
                "playback stalled"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        // The person interrupts. The coordinator drops what it had queued.
        let owner = rig
            .owner
            .admit(real_time(), AdmittedInput::Interruption)
            .unwrap();
        let new = owner.turn();
        speaker.flush();
        rig.grant(&mut speaker, false);
        let interrupted_at = Instant::now();
        let privacy_generation = rig.snapshot.microphone_generation();
        rig.parent
            .data
            .send(ProviderInput::Start {
                session: 1,
                request: new,
                privacy_generation,
                continues: false,
                continues_cut_off: false,
            })
            .unwrap();
        rig.parent
            .data
            .send(ProviderInput::Audio {
                request: new,
                privacy_generation,
                sequence: 1,
                read_completed_at_us: monotonic_us(),
                samples: vec![3; 160],
            })
            .unwrap();
        rig.owner.input_ended(real_time(), owner).unwrap();
        rig.publish();
        rig.parent
            .data
            .send(ProviderInput::End {
                request: new,
                privacy_generation: rig.snapshot.microphone_generation(),
                sound: false,
            })
            .unwrap();
        // The scripted service confirms the interruption and answers. Packets
        // keep being counted and capacity granted while it does.
        let mut stale = 0u64;
        let mut first_new = None;
        let mut step = 0;
        while first_new.is_none() {
            rig.grant(&mut speaker, false);
            for wire in rig.arrived() {
                match wire {
                    Wire::Output(ProviderOutput::Audio {
                        request, samples, ..
                    }) if request == new => {
                        assert!(samples.iter().all(|sample| *sample == 99));
                        first_new.get_or_insert(interrupted_at.elapsed());
                    }
                    // Already sent before the interruption: at most the two
                    // packets of outstanding capacity. Discarded on arrival.
                    Wire::Output(ProviderOutput::Audio { .. }) => stale += 1,
                    Wire::Output(_) => {}
                    Wire::Status(status) => panic!("unexpected status: {status:?}"),
                }
            }
            if let Some(message) = dial.service.try_receive(Duration::from_millis(2)).await {
                let input = &message["realtimeInput"];
                let expected = ["activityStart", "audio", "activityEnd"][step];
                assert!(input.get(expected).is_some(), "{message}");
                step += 1;
                if step == 2 {
                    dial.service
                        .send(json!({"serverContent":{"interrupted":true}}))
                        .await;
                    dial.service.send(idle_complete()).await;
                } else if step == 3 {
                    dial.service.send(provider_audio(99, 960)).await;
                }
            }
            assert!(
                interrupted_at.elapsed() < Duration::from_secs(5),
                "new answer delayed"
            );
        }
        let first_new = first_new.unwrap();
        println!(
            "MEASURED interruption_under_capacity new_answer_first_packet_ms={} stale_packets_after_interruption={stale} (admission to first packet of the new answer at a local IPC reader, scripted service, host)",
            first_new.as_millis()
        );
        // The cancelled answer still had about 36 s queued, 15 s of it in this
        // worker. At playback pace that would have come first.
        assert!(first_new < Duration::from_secs(1), "{first_new:?}");
        assert!(stale <= 2, "{stale} stale packets");
    }

