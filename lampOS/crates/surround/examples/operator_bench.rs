//! Host-only HTTP admission/status measurements, not lamp or browser timings.
//!
//! Run: cargo run --offline -p lamp-surround --example operator_bench
//! The local target is 100 ms for status and durable admission. This example
//! reports samples against that target without asserting a timing threshold.
//! Router/authentication/atomic persistence are real; maintenance is a fixture.
//! A scripted JoinWifi body exercises redaction, but no adapter is constructed.
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use lamp_surround::{
    atomic::{self, DirectoryLock},
    config::Secret,
    operator::{self, Clock, Kind, Operation, Phase, Shared, Submission},
};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, mpsc as blocking},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tower::ServiceExt;

const WARM_SAMPLES: usize = 100;
const SLOW_STATUS_SAMPLES: usize = 100;
const SLOW_AFTER: usize = WARM_SAMPLES + 2;
const SLOW_MS: u64 = 500;
const TARGET_MS: f64 = 100.0;
const JOURNAL_BOUND: usize = 4096;
const LOGIN_SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const FIXTURE_PASSWORD: &str = "fixture-only-never-persist-this-password";

struct Response {
    elapsed: Duration,
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

struct Completion {
    id: String,
    dependency_elapsed: Option<Duration>,
}

fn request(method: &str, path: &str, cookie: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8089")
        .header("origin", "http://127.0.0.1:8089");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |value| {
            Body::from(serde_json::to_vec(&value).unwrap())
        }))
        .unwrap()
}

async fn measure(router: &Router, request: Request<Body>) -> Result<Response, Box<dyn Error>> {
    // Start after constructing the request, end after collecting the response
    // body. Application JSON parsing is included; benchmark parsing is not.
    let started = Instant::now();
    let response = router.clone().oneshot(request).await?;
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, 65_536).await?;
    let elapsed = started.elapsed();
    Ok(Response {
        elapsed,
        status: parts.status,
        headers: parts.headers,
        body: serde_json::from_slice(&bytes)?,
    })
}

fn save(path: &Path, operation: &Operation) -> io::Result<()> {
    let bytes = serde_json::to_vec(operation).map_err(io::Error::other)?;
    atomic::write(path, &bytes, JOURNAL_BOUND)
}

fn fixture_worker(
    path: PathBuf,
    _lock: DirectoryLock,
    shared: Shared,
    clock: Clock,
    mut submissions: mpsc::Receiver<Submission>,
    completed: mpsc::Sender<Completion>,
    release_slow: blocking::Receiver<()>,
) -> io::Result<()> {
    let mut count = 0;
    while let Some(submission) = submissions.blocking_recv() {
        count += 1;
        assert_eq!(submission.command.kind(), Kind::JoinWifi);
        let mut operation = Operation {
            schema_version: 1,
            id: submission.id,
            kind: submission.command.kind(),
            phase: Phase::Working,
            updated_us: clock(),
            error: None,
        };
        // This redacted record contains no command body, SSID or password.
        // Atomic write includes file fsync, rename and parent-directory fsync.
        save(&path, &operation)?;
        shared.view.write().unwrap().operation = Some(operation.clone());
        submission
            .admitted
            .send(Ok(()))
            .map_err(|_| io::Error::other("HTTP admission receiver disappeared"))?;
        drop(submission.command);

        let dependency_elapsed = if count == SLOW_AFTER {
            let started = Instant::now();
            // Dedicated worker thread: deliberately blocking here cannot
            // occupy the current-thread HTTP executor. The barrier extends
            // the minimum 500 ms only if status sampling itself takes longer.
            thread::sleep(Duration::from_millis(SLOW_MS));
            release_slow
                .recv()
                .map_err(|_| io::Error::other("slow fixture sampling was abandoned"))?;
            Some(started.elapsed())
        } else {
            None
        };
        operation.phase = Phase::Complete;
        operation.updated_us = clock();
        save(&path, &operation)?;
        shared.view.write().unwrap().operation = Some(operation.clone());
        shared.finished();
        completed
            .blocking_send(Completion {
                id: operation.id,
                dependency_elapsed,
            })
            .map_err(|_| io::Error::other("completion receiver disappeared"))?;
    }
    assert_eq!(count, SLOW_AFTER);
    Ok(())
}

fn statistics(samples: &[Duration]) -> Value {
    assert!(!samples.is_empty());
    let mut ms: Vec<_> = samples
        .iter()
        .map(|sample| sample.as_secs_f64() * 1000.0)
        .collect();
    ms.sort_by(f64::total_cmp);
    let middle = ms.len() / 2;
    let median = if ms.len().is_multiple_of(2) {
        (ms[middle - 1] + ms[middle]) / 2.0
    } else {
        ms[middle]
    };
    json!({
        "samples": ms.len(),
        "median_ms": median,
        "max_ms": ms.last().unwrap(),
        "above_target_samples": ms.iter().filter(|sample| **sample > TARGET_MS).count(),
    })
}

async fn status(router: &Router, cookie: &str) -> Result<Response, Box<dyn Error>> {
    let response = measure(router, request("GET", "/api/status", Some(cookie), None)).await?;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], 1);
    Ok(response)
}

async fn admit(
    router: &Router,
    cookie: &str,
    journal: &Path,
) -> Result<(Duration, String), Box<dyn Error>> {
    let response = measure(
        router,
        request(
            "POST",
            "/api/operation",
            Some(cookie),
            Some(json!({
                "operation": "join_wifi",
                "ssid": "benchmark-fixture",
                "password": FIXTURE_PASSWORD,
            })),
        ),
    )
    .await?;
    assert_eq!(response.status, StatusCode::ACCEPTED);
    assert_eq!(response.body["status"], 1);
    let id = response.body["data"]["id"].as_str().unwrap().to_owned();
    // Read-back is outside the timed HTTP interval. The worker may already
    // have persisted Complete; either phase must refer to this exact intent.
    let bytes = atomic::read(journal, JOURNAL_BOUND)?.expect("ack requires durable intent");
    let operation: Operation = serde_json::from_slice(&bytes)?;
    assert_eq!(operation.id, id);
    assert_eq!(operation.kind, Kind::JoinWifi);
    let text = String::from_utf8(bytes)?;
    for private in [LOGIN_SECRET, FIXTURE_PASSWORD, "benchmark-fixture"] {
        assert!(!text.contains(private));
        assert!(!response.body.to_string().contains(private));
    }
    Ok((response.elapsed, id))
}

async fn completion(completed: &mut mpsc::Receiver<Completion>, expected_id: &str) -> Completion {
    let result = completed.recv().await.expect("fixture worker completed");
    assert_eq!(result.id, expected_id);
    result
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::Builder::new()
        .prefix("lamp-operator-bench-")
        .tempdir()?;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700))?;
    let root = temporary.path().canonicalize()?;
    let lock = DirectoryLock::acquire(&root)?;
    let journal = root.join("fixture-operation.json");
    let started = Instant::now();
    let clock: Clock = Arc::new(move || u64::try_from(started.elapsed().as_micros()).unwrap());
    let shared = Shared::new(true);
    let (submit, submissions) = mpsc::channel(1);
    let (done, mut completed) = mpsc::channel(1);
    let (release, release_slow) = blocking::sync_channel(1);
    let app = operator::router(
        Secret::new(LOGIN_SECRET.as_bytes().to_vec())?,
        "http://127.0.0.1:8089",
        shared.clone(),
        submit,
        clock.clone(),
    )?;
    let worker_path = journal.clone();
    let worker = thread::Builder::new()
        .name("operator-bench-fixture".into())
        .spawn(move || {
            fixture_worker(
                worker_path,
                lock,
                shared,
                clock,
                submissions,
                done,
                release_slow,
            )
        })?;

    let login = measure(
        &app,
        request(
            "POST",
            "/api/login",
            None,
            Some(json!({"secret": LOGIN_SECRET})),
        ),
    )
    .await?;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.headers["set-cookie"]
        .to_str()?
        .split(';')
        .next()
        .unwrap()
        .to_owned();

    let first_status = status(&app, &cookie).await?.elapsed;
    let mut warm_status = Vec::with_capacity(WARM_SAMPLES);
    for _ in 0..WARM_SAMPLES {
        warm_status.push(status(&app, &cookie).await?.elapsed);
    }

    let (first_admission, id) = admit(&app, &cookie, &journal).await?;
    completion(&mut completed, &id).await;
    let mut warm_admission = Vec::with_capacity(WARM_SAMPLES);
    for _ in 0..WARM_SAMPLES {
        let (elapsed, id) = admit(&app, &cookie, &journal).await?;
        warm_admission.push(elapsed);
        completion(&mut completed, &id).await;
    }

    let (slow_admission, id) = admit(&app, &cookie, &journal).await?;
    let mut slow_status = Vec::with_capacity(SLOW_STATUS_SAMPLES);
    for _ in 0..SLOW_STATUS_SAMPLES {
        let response = status(&app, &cookie).await?;
        assert_eq!(response.body["data"]["operation"]["id"], id);
        assert_eq!(response.body["data"]["operation"]["phase"], "working");
        slow_status.push(response.elapsed);
    }
    release.send(())?;
    let slow = completion(&mut completed, &id).await;
    drop(app);
    worker
        .join()
        .map_err(|_| io::Error::other("fixture worker panicked"))??;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "measurement_host": {"os": std::env::consts::OS, "architecture": std::env::consts::ARCH},
            "boundary": "in-process Router dispatch through response-body collection; request construction and benchmark JSON parsing excluded",
            "excluded": ["TCP/Unix sockets", "browser rendering", "physical lamp", "providers", "network adapters", "production maintenance effects"],
            "storage": "private 0700 temporary directory on this host's default temporary filesystem; file and directory fsync included in admission",
            "worker": "scripted fixture on a separate thread; redacted durable Operation before acknowledgement; no network action",
            "target_ms": TARGET_MS,
            "login": statistics(&[login.elapsed]),
            "authenticated_status": {"first": statistics(&[first_status]), "warm": statistics(&warm_status)},
            "durable_admission": {"first": statistics(&[first_admission]), "warm": statistics(&warm_admission)},
            "slow_dependency": {
                "authored_minimum_ms": SLOW_MS,
                "completion_barrier": "also waits for all concurrent-with-work status samples; may extend the minimum on a slow host",
                "measured_fixture_dependency_ms": slow.dependency_elapsed.unwrap().as_secs_f64() * 1000.0,
                "admission": statistics(&[slow_admission]),
                "status_while_operation_working": statistics(&slow_status),
            },
            "durable_redacted_journal_readbacks": SLOW_AFTER,
            "timing_threshold_asserted": false,
        }))?
    );
    Ok(())
}
