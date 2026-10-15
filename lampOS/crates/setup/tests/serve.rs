use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use lamp_setup::{
    http::{Shared, router},
    scan,
    serve::{Accepted, Config, Files, Phase, Save, SetupRequest, Status, check, run, zone_known},
    wifi::Host,
};
use serde_json::{Value, json};
use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "lamp-setup-serve-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("zoneinfo/Asia")).unwrap();
        fs::create_dir_all(path.join("zoneinfo/America")).unwrap();
        fs::write(path.join("zoneinfo/Asia/Ho_Chi_Minh"), b"TZif").unwrap();
        fs::write(path.join("zoneinfo/America/New_York"), b"TZif").unwrap();
        fs::create_dir_all(path.join("etc")).unwrap();
        Self(path)
    }
    fn config(&self, page: bool) -> Config {
        Config {
            listen: "127.0.0.1:5000".into(),
            marker: self.0.join("etc/setup-done"),
            credential: self.0.join("etc/credential"),
            name: self.0.join("etc/name"),
            country: "VN".into(),
            interface: "wlan0".into(),
            hotspot_address: "192.168.100.1".into(),
            page,
            zoneinfo: self.0.join("zoneinfo"),
            localtime: self.0.join("etc/localtime"),
            timezone_file: self.0.join("etc/timezone"),
            web_start: vec![],
            web_stop: vec![],
        }
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn request(value: Value) -> SetupRequest {
    serde_json::from_value(value).unwrap()
}

fn good() -> Value {
    json!({"ssid": "Home", "password": "password1", "llm_api_key": "AIza-key_1",
           "device_id": "d1", "name": " Lan ", "timezone": "Asia/Ho_Chi_Minh",
           // V1's page sends these too; they are accepted and ignored.
           "channel": "telegram", "mqtt_port": 1883})
}

#[test]
fn a_request_is_checked_before_anything_is_touched() {
    let root = Root::new();
    let config = root.config(false);
    assert_eq!(
        check(&request(good()), &config).unwrap(),
        Accepted {
            ssid: "Home".into(),
            password: "password1".into(),
            credential: "AIza-key_1".into(),
            name: Some("Lan".into()),
            timezone: Some("Asia/Ho_Chi_Minh".into()),
        }
    );
    let without = check(
        &request(json!({"ssid": "Cafe", "password": "", "llm_api_key": "k"})),
        &config,
    )
    .unwrap();
    assert_eq!((without.name, without.timezone), (None, None));
    for (field, value) in [
        ("ssid", json!("")),
        ("ssid", json!("Home\nnetwork={")),
        ("password", json!("short")),
        ("llm_api_key", json!("")),
        ("llm_api_key", json!("two words")),
        ("name", json!("x".repeat(65))),
        ("name", json!("La\nn")),
        ("timezone", json!("../etc/passwd")),
        ("timezone", json!("Asia/Nowhere")),
        ("timezone", json!("Asia/./Ho_Chi_Minh")),
    ] {
        let mut body = good();
        body[field] = value.clone();
        assert!(check(&request(body), &config).is_err(), "{field} {value}");
    }
    assert!(zone_known("America/New_York", &config.zoneinfo));
    assert!(
        !zone_known("America", &config.zoneinfo),
        "a directory is no zone"
    );
}

/// A lamp whose join ends after `seconds` of fake time as `connects` says.
struct Lamp {
    connects: bool,
    conf: Option<Vec<u8>>,
    calls: Vec<&'static str>,
    now_ms: u64,
}
impl Lamp {
    fn new(connects: bool) -> Self {
        Self {
            connects,
            conf: None,
            calls: vec![],
            now_ms: 0,
        }
    }
}
impl Host for Lamp {
    fn read_conf(&mut self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.conf.clone())
    }
    fn write_conf(&mut self, bytes: Option<&[u8]>) -> io::Result<()> {
        self.conf = bytes.map(<[u8]>::to_vec);
        Ok(())
    }
    fn station(&mut self) -> io::Result<()> {
        self.calls.push("station");
        Ok(())
    }
    fn hotspot(&mut self) -> io::Result<()> {
        self.calls.push("hotspot");
        Ok(())
    }
    fn internet(&mut self) -> bool {
        self.connects && self.now_ms >= 3_000
    }
    fn current_ssid(&mut self) -> Option<String> {
        self.internet().then(|| "Home".into())
    }
    fn has_station_ip(&mut self) -> bool {
        false
    }
    fn wpa_state(&mut self) -> String {
        "SCANNING".into()
    }
    fn wpa_events(&mut self) -> Vec<String> {
        if self.connects {
            vec![]
        } else {
            vec![
                "wlan0: WPA: 4-Way Handshake failed - pre-shared key may be incorrect".into(),
                "wlan0: WPA: 4-Way Handshake failed - pre-shared key may be incorrect".into(),
            ]
        }
    }
    fn now_ms(&mut self) -> u64 {
        self.now_ms
    }
    fn sleep(&mut self, duration: Duration) {
        self.now_ms += duration.as_millis() as u64;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[derive(Default)]
struct Saved {
    calls: Vec<String>,
    fail_on: Option<&'static str>,
}
impl Save for Saved {
    fn credential(&mut self, key: &str) -> io::Result<()> {
        self.record("credential", key)
    }
    fn name(&mut self, name: &str) -> io::Result<()> {
        self.record("name", name)
    }
    fn timezone(&mut self, zone: &str) -> io::Result<()> {
        self.record("timezone", zone)
    }
    fn marker(&mut self) -> io::Result<()> {
        self.record("marker", "")
    }
}
impl Saved {
    fn record(&mut self, what: &'static str, value: &str) -> io::Result<()> {
        if self.fail_on == Some(what) {
            return Err(io::Error::other("disk full"));
        }
        self.calls.push(format!("{what}={value}"));
        Ok(())
    }
}

fn accepted() -> Accepted {
    check(&request(good()), &Root::new().config(false)).unwrap()
}

#[test]
fn a_joined_lamp_saves_the_settings_then_the_marker_and_says_it_is_set_up() {
    let status = Arc::new(Mutex::new(Status::new("lamp-4ace".into())));
    let mut saved = Saved::default();
    run(
        &status,
        &accepted(),
        "VN",
        &mut Lamp::new(true),
        &mut saved,
        || Some("192.168.1.23".into()),
    );
    let status = status.lock().unwrap().clone();
    assert_eq!(status.phase, Phase::Connected);
    assert!(status.set_up_completed);
    assert_eq!(status.run, 1);
    // V1's page keeps its "Preparing your robot's voice…" screen.
    assert_eq!(status.runtime_phase, "preparing");
    assert_eq!(status.lan_ip, "192.168.1.23", "published while joining");
    assert_eq!(
        saved.calls,
        [
            "credential=AIza-key_1",
            "name=Lan",
            "timezone=Asia/Ho_Chi_Minh",
            "marker="
        ]
    );
}

#[test]
fn a_wrong_password_saves_nothing_and_names_the_reason() {
    let status = Arc::new(Mutex::new(Status::new("lamp-4ace".into())));
    let mut saved = Saved::default();
    let mut lamp = Lamp::new(false);
    run(&status, &accepted(), "VN", &mut lamp, &mut saved, || None);
    let status = status.lock().unwrap().clone();
    assert_eq!(status.phase, Phase::Failed);
    assert!(!status.set_up_completed);
    assert!(
        status.error.starts_with("wrong WiFi password"),
        "{}",
        status.error
    );
    assert!(saved.calls.is_empty());
    assert_eq!(lamp.calls, ["station", "hotspot"]);
}

#[test]
fn settings_that_cannot_be_saved_leave_no_marker_and_bring_the_hotspot_back() {
    let status = Arc::new(Mutex::new(Status::new("lamp-4ace".into())));
    let mut saved = Saved {
        fail_on: Some("timezone"),
        ..Saved::default()
    };
    let mut lamp = Lamp::new(true);
    run(&status, &accepted(), "VN", &mut lamp, &mut saved, || None);
    let status = status.lock().unwrap().clone();
    assert_eq!(status.phase, Phase::Failed);
    assert!(!status.set_up_completed);
    assert!(!saved.calls.iter().any(|call| call.starts_with("marker")));
    assert_eq!(lamp.calls, ["station", "hotspot"]);
}

#[test]
fn the_files_are_private_and_the_zone_is_linked_as_v1_does() {
    let root = Root::new();
    let config = root.config(false);
    let mut files = Files(&config);
    files.credential("AIza-key_1").unwrap();
    files.name("Lan").unwrap();
    files.timezone("Asia/Ho_Chi_Minh").unwrap();
    files.marker().unwrap();
    for path in [&config.credential, &config.name, &config.marker] {
        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{}", path.display());
    }
    assert_eq!(fs::read(&config.credential).unwrap(), b"AIza-key_1");
    assert_eq!(fs::read(&config.name).unwrap(), b"Lan\n");
    assert_eq!(
        fs::read_link(&config.localtime).unwrap(),
        config.zoneinfo.join("Asia/Ho_Chi_Minh")
    );
    assert_eq!(
        fs::read(&config.timezone_file).unwrap(),
        b"Asia/Ho_Chi_Minh\n"
    );
    // A second zone replaces the first.
    files.timezone("America/New_York").unwrap();
    assert_eq!(
        fs::read_link(&config.localtime).unwrap(),
        config.zoneinfo.join("America/New_York")
    );
}

/// The router, its status, the runs it was asked to start, and its files.
type App = (
    axum::Router,
    Arc<Mutex<Status>>,
    Arc<Mutex<Vec<Accepted>>>,
    Root,
);

fn app(page: bool) -> App {
    let root = Root::new();
    let status = Arc::new(Mutex::new(Status::new("lamp-4ace".into())));
    let started = Arc::new(Mutex::new(Vec::new()));
    let shared = Arc::new(Shared {
        config: root.config(page),
        status: Arc::clone(&status),
        start: Box::new({
            let started = Arc::clone(&started);
            move |accepted| started.lock().unwrap().push(accepted)
        }),
        scan: Box::new(|| {
            Ok(scan::parse(
                "BSS 34:3a:20:95:78:93(on wlan0)\n\tsignal: -41.00 dBm\n\tSSID: Home\n\tDS Parameter set: channel 6\n",
            ))
        }),
    });
    (router(shared), status, started, root)
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let code = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test(flavor = "current_thread")]
async fn v1s_routes_answer_in_v1s_shapes() {
    let (app, status, started, _root) = app(false);
    let (code, body) = call(&app, "GET", "/api/device/setup/status", Value::Null).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        body,
        json!({"status": 1, "message": null, "data": {"phase": "idle", "lan_ip": "", "error": "",
               "mac": "lamp-4ace", "run": 0, "set_up_completed": false, "runtime_phase": ""}})
    );
    let (code, body) = call(&app, "GET", "/api/network", Value::Null).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        body["data"],
        json!([{"bssid": "34:3a:20:95:78:93", "ssid": "Home", "mode": "STA", "channel": 6,
                "rate": "", "signal": -41, "linkRate": 0, "security": ""}])
    );
    let (code, body) = call(
        &app,
        "POST",
        "/api/device/setup",
        json!({"ssid": "Home", "password": "short", "llm_api_key": "k"}),
    )
    .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert_eq!(body["status"], 0);
    assert!(body["message"].as_str().unwrap().contains("8 to 63"));
    assert!(started.lock().unwrap().is_empty());

    let (code, body) = call(&app, "POST", "/api/device/setup", good()).await;
    assert_eq!(
        (code, body),
        (
            StatusCode::OK,
            json!({"status": 1, "data": true, "message": null})
        )
    );
    assert_eq!(started.lock().unwrap().len(), 1);
    assert_eq!(status.lock().unwrap().phase, Phase::Connecting);
    let (code, _) = call(&app, "POST", "/api/device/setup", good()).await;
    assert_eq!(code, StatusCode::CONFLICT, "one run at a time");

    status.lock().unwrap().phase = Phase::Connected;
    status.lock().unwrap().set_up_completed = true;
    let (code, _) = call(&app, "POST", "/api/device/setup", good()).await;
    assert_eq!(
        code,
        StatusCode::CONFLICT,
        "a set-up lamp is not set up again here"
    );
    assert_eq!(started.lock().unwrap().len(), 1);
    // No page without `page`.
    let (code, _) = call(&app, "GET", "/setup", Value::Null).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "current_thread")]
async fn the_own_page_and_the_phones_checks_are_served_only_with_page() {
    let (app, _, _, _root) = app(true);
    for path in ["/", "/setup"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        assert!(String::from_utf8_lossy(&html).contains("/api/device/setup/status"));
    }
    for path in [
        "/generate_204",
        "/hotspot-detect.html",
        "/ncsi.txt",
        "/connecttest.txt",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "{path}");
    }
}

#[test]
fn the_scan_reads_v1s_fields_and_iws_escapes() {
    let output = "BSS 34:3a:20:95:78:93(on wlan0) -- associated\n\
                  \tfreq: 2437\n\tsignal: -48.00 dBm\n\tSSID: Caf\\xc3\\xa9\\ Home\n\
                  \tDS Parameter set: channel 6\n\tHESSID: 00:00:00:00:00:00\n\
                  BSS 11:22:33:44:55:66(on wlan0)\n\tsignal: -80.00 dBm\n\tSSID: \n\
                  BSS aa:bb:cc:dd:ee:ff(on wlan0)\n\tsignal: -67.50 dBm\n\tSSID: Neighbour\n\tSSID: second\n";
    let list = scan::parse(output);
    assert_eq!(list.len(), 2, "a hidden network has no name to choose");
    assert_eq!(list[0].ssid, "Café Home");
    assert_eq!((list[0].signal, list[0].channel), (-48, 6));
    assert_eq!(list[1].ssid, "Neighbour", "the first SSID line wins");
    assert_eq!(list[1].signal, -67);
}
