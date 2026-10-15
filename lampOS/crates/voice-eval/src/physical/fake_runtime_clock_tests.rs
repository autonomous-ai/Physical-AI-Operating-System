use super::*;
use crate::stimulus::{ClipTiming, TimingSource};
use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Private(PathBuf);
impl Private {
    fn new() -> Self {
        // Keep the path below Unix datagram socket name limits on macOS.
        let path = PathBuf::from(format!(
            "/tmp/lve-fake-clock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn arguments(output: &Path, seconds: u16, options: &[(&str, &Path)]) -> Vec<String> {
    let mut arguments = vec![
        "--fake-scenario".into(),
        "fixed-reply-echo-only".into(),
        "--fake-profile".into(),
        "v2-directed-current".into(),
    ];
    for (flag, path) in options {
        arguments.extend([(*flag).to_owned(), path.display().to_string()]);
    }
    arguments.extend([
        "directed-fixture".into(),
        "unused.wav".into(),
        "0".repeat(64),
        seconds.to_string(),
        output.display().to_string(),
    ]);
    arguments
}

fn records(output: &Path) -> Vec<Value> {
    let bytes = fs::read(output.join("events.jsonl")).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn a_pure_clock_keeps_tick_boundaries_and_frozen_polls_distinct() {
    let dir = Private::new();
    let output = dir.join("out");
    let base = 1_000_000;
    let clock = Cell::new(base);
    let reads = Cell::new(0);
    let mut polls = Vec::new();
    let schedule = [9_999, 10_000, 10_000, 2_000_000, 2_010_000];
    let result = run_with_clock_and_poll(
        &arguments(&output, 0, &[]),
        || {
            reads.set(reads.get() + 1);
            MonoTime::from_micros(clock.get())
        },
        |progress| {
            assert_eq!(reads.get(), 1 + progress.poll);
            assert_eq!(progress.at.as_micros(), clock.get());
            assert_eq!(progress.room_inputs, 0);
            assert!(!output.join("events.jsonl").exists());
            polls.push(progress);
            if !progress.ended {
                let next = schedule.get(polls.len() - 1).expect("bounded polls");
                clock.set(base + next);
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(result, 0);
    assert_eq!(polls.len(), 6);
    assert_eq!(
        polls.iter().map(|poll| poll.runtime_us).collect::<Vec<_>>(),
        [0, 0, 10_000, 10_000, 2_000_000, 2_010_000]
    );
    assert_eq!(polls[2].trace_records, polls[3].trace_records);
    assert_eq!(polls[3].poll, polls[2].poll + 1);
    assert_eq!(polls[3].stdout_lines, 0);
    assert_eq!(polls[4].stdout_lines, 1);
    assert!(!polls[4].ended);
    assert!(polls[5].ended);
    let trace = records(&output);
    assert_eq!(trace.len(), polls[5].trace_records);
    assert_eq!(trace[0]["kind"], "run_start");
    assert_eq!(trace[0]["at_us"], base);
    assert_eq!(trace.last().unwrap()["kind"], "run_end");
    assert_eq!(trace.last().unwrap()["at_us"], base + 2_010_000);
}

#[test]
fn completed_polls_prove_room_consumption_and_keep_cue_fault_timing() {
    let dir = Private::new();
    let output = dir.join("out");
    let room_path = dir.join("room.sock");
    let cue_path = dir.join("cue.sock");
    let cue_socket = UnixDatagram::bind(&cue_path).unwrap();
    cue_socket.set_nonblocking(true).unwrap();
    let room_sender = UnixDatagram::unbound().unwrap();
    let mut runtime_arguments = vec!["--fake-cue-fault".into(), "stale-completion".into()];
    runtime_arguments.extend(arguments(&output, 8, &[("--fake-room", &room_path)]));
    runtime_arguments.extend(["--cue-socket".into(), cue_path.display().to_string()]);
    let speech = SceneTiming {
        scene: "clock-input".into(),
        duration_ms: 200,
        clips: vec![ClipTiming {
            utterance: "clock-input".into(),
            text: "A clocked question".into(),
            voice: "test".into(),
            gain_db: 0.0,
            start_ms: 0,
            end_ms: 200,
        }],
        speech_span_ms: Some((0, 200)),
        voices: 1,
        noise: false,
        source: TimingSource::TextEstimate,
    };
    let bytes = serde_json::to_vec(&speech).unwrap();
    let base = 10_000_000;
    let clock = Cell::new(base);
    let reads = Cell::new(0);
    let mut injected = false;
    let mut consumed_at = None;
    let mut delivered = Vec::new();
    let mut final_poll = None;
    let result = run_with_clock_and_poll(
        &runtime_arguments,
        || {
            reads.set(reads.get() + 1);
            MonoTime::from_micros(clock.get())
        },
        |progress| {
            assert!(progress.poll <= 6_000, "bounded driver");
            // One base read, then separate tick, CueSink and fault-relay reads.
            assert_eq!(reads.get(), 1 + 3 * progress.poll);
            assert_eq!(progress.at.as_micros(), clock.get());
            let mut buffer = [0_u8; 513];
            loop {
                match cue_socket.recv(&mut buffer) {
                    Ok(count) => {
                        let cue: Value = serde_json::from_slice(&buffer[..count]).unwrap();
                        delivered.push((progress.at.as_micros(), cue));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => panic!("read completed cue poll: {error}"),
                }
            }
            if progress.stdout_lines == 1 && !injected {
                assert_eq!(progress.runtime_us, 2_000_000);
                assert_eq!(progress.room_inputs, 0);
                assert!(
                    delivered
                        .iter()
                        .any(|(_, cue)| cue["kind"] == "listening_ready")
                );
                assert_eq!(
                    room_sender.send_to(&bytes, &room_path).unwrap(),
                    bytes.len()
                );
                injected = true;
                // Sending is not consumption. Poll again without advancing time.
                return Ok(());
            }
            if progress.room_inputs == 1 && consumed_at.is_none() {
                consumed_at = Some(progress.at.as_micros());
                assert_eq!(progress.runtime_us, 2_000_000);
                assert_eq!(progress.trace_records, 4);
                assert!(
                    !delivered
                        .iter()
                        .any(|(_, cue)| cue["kind"] == "input_admitted")
                );
            }
            if progress.ended {
                assert!(!output.join("events.jsonl").exists());
                final_poll = Some(progress);
            } else {
                clock.set(clock.get() + 2_000);
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(result, 0);
    let consumed_at = consumed_at.expect("room input acknowledged");
    assert_eq!(consumed_at, base + 2_000_000);
    let final_poll = final_poll.unwrap();
    assert_eq!(final_poll.runtime_us, 10_000_000);
    assert_eq!(final_poll.stdout_lines, 1);
    assert_eq!(final_poll.room_inputs, 1);
    assert!(!room_path.exists());
    let trace = records(&output);
    assert_eq!(trace.len(), final_poll.trace_records);
    let admitted: Vec<_> = trace
        .iter()
        .filter(|event| event["kind"] == "input_admitted")
        .collect();
    assert_eq!(admitted.len(), 1);
    assert!(admitted[0]["at_us"].as_u64().unwrap() > consumed_at);
    assert_eq!(admitted[0]["owner"]["boot"], json!(BOOT_BYTES));
    assert!(
        trace
            .iter()
            .any(|event| event["kind"] == "transcript" && event["text"] == "A clocked question")
    );
    assert_eq!(trace.last().unwrap()["kind"], "run_end");
    let completed: Vec<_> = delivered
        .iter()
        .filter(|(_, cue)| cue["kind"] == "turn_completed")
        .collect();
    assert_eq!(completed.len(), 1);
    let (received_us, cue) = completed[0];
    assert_eq!(*received_us, cue["sent_us"].as_u64().unwrap() + 150_000);
    assert_eq!(
        cue["expires_us"].as_u64().unwrap(),
        cue["event_us"].as_u64().unwrap() + 100_000
    );
    assert!(*received_us > cue["expires_us"].as_u64().unwrap());
    assert_eq!(delivered.last().unwrap().1["kind"], "run_end");
}

#[test]
fn controlled_serve_is_refused_before_clock_hooks_or_output() {
    let dir = Private::new();
    let output = dir.join("out");
    let room = dir.join("room.sock");
    let arguments = [
        "--fake-room".into(),
        room.display().to_string(),
        "serve".into(),
        "unused.json".into(),
        output.display().to_string(),
    ];
    let error = run_with_clock_and_poll(
        &arguments,
        || panic!("serve must not sample the controlled clock"),
        |_| panic!("serve must not invoke the finite poll hook"),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "a controlled fake clock supports only finite directed runs"
    );
    assert!(!output.exists());
    assert!(!room.exists());
}

#[test]
fn a_failed_poll_hook_stops_before_another_tick_or_final_trace() {
    let dir = Private::new();
    let output = dir.join("out");
    let reads = Cell::new(0);
    let error = run_with_clock_and_poll(
        &arguments(&output, 0, &[]),
        || {
            reads.set(reads.get() + 1);
            MonoTime::from_micros(1_000_000)
        },
        |progress| {
            assert_eq!(progress.poll, 1);
            assert_eq!(progress.runtime_us, 0);
            Err(invalid("fixture coordinator stopped"))
        },
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "fixture coordinator stopped");
    assert_eq!(reads.get(), 2);
    assert!(!output.join("events.jsonl").exists());
}
