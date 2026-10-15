//! The setup server (#806): V1's setup API with V1's reply shapes, so V1's own
//! page works unchanged behind V1's nginx (where `os-server` listened), or a
//! small page of this crate's own when V1's is not on the lamp.
//!
//! It runs only while the lamp is not set up: the keeper starts it instead of
//! the lamp when the setup marker is missing (#702). Once the home's Wi-Fi is
//! joined and the person's settings are saved, it writes the marker, and the
//! keeper starts the lamp. It goes on answering for a while, so the page can
//! follow the lamp onto the home network and see that it is done.

use crate::wifi::{self, Host, Joined};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

fn default_interface() -> String {
    "wlan0".into()
}
fn default_hotspot() -> String {
    "192.168.100.1".into()
}
fn default_zoneinfo() -> PathBuf {
    "/usr/share/zoneinfo".into()
}
fn default_localtime() -> PathBuf {
    "/etc/localtime".into()
}
fn default_timezone_file() -> PathBuf {
    "/etc/timezone".into()
}

/// The server's written configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Where to answer: `127.0.0.1:5000` behind V1's nginx, or `0.0.0.0:80`
    /// with `page` when serving this crate's own page.
    pub listen: String,
    /// Written once setup is done; the keeper starts the lamp when it appears.
    pub marker: PathBuf,
    /// The voice's credential file, named by `provider.json`.
    pub credential: PathBuf,
    /// The person's name, for the voice to give the model.
    pub name: PathBuf,
    /// wpa_supplicant's two-letter country.
    pub country: String,
    #[serde(default = "default_interface")]
    pub interface: String,
    #[serde(default = "default_hotspot")]
    pub hotspot_address: String,
    /// Serve this crate's page at `/` and `/setup`, and 204 to the phones'
    /// sign-in checks, as V1's nginx does.
    #[serde(default)]
    pub page: bool,
    #[serde(default = "default_zoneinfo")]
    pub zoneinfo: PathBuf,
    #[serde(default = "default_localtime")]
    pub localtime: PathBuf,
    #[serde(default = "default_timezone_file")]
    pub timezone_file: PathBuf,
}

impl Config {
    pub fn check(&self) -> Result<(), String> {
        for path in [
            &self.marker,
            &self.credential,
            &self.name,
            &self.zoneinfo,
            &self.localtime,
            &self.timezone_file,
        ] {
            if !path.is_absolute() {
                return Err(format!("{} is not absolute", path.display()));
            }
        }
        wifi::wpa_conf(&self.country, "check", "").map_err(|_| "country is two capitals")?;
        self.listen
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "listen is an address and port")?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Connecting,
    Connected,
    Failed,
}

/// V1's `GET /api/device/setup/status` data, field for field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    pub phase: Phase,
    pub lan_ip: String,
    pub error: String,
    pub mac: String,
    /// Setup runs since start, so the page can tell its own verdict from a
    /// leftover one.
    pub run: u64,
    pub set_up_completed: bool,
    pub runtime_phase: String,
}

impl Status {
    pub fn new(mac: String) -> Self {
        Self {
            phase: Phase::Idle,
            lan_ip: String::new(),
            error: String::new(),
            mac,
            run: 0,
            set_up_completed: false,
            runtime_phase: String::new(),
        }
    }
}

/// V1's `SetupRequest`. V1's page also sends channel, voice and MQTT fields;
/// they are accepted and ignored. `name` and `timezone` are V2's own.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SetupRequest {
    #[serde(default)]
    pub ssid: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub llm_api_key: String,
    #[serde(default)]
    pub llm_base_url: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub timezone: String,
}

/// A request that passed every check, ready to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    pub ssid: String,
    pub password: String,
    pub credential: String,
    pub name: Option<String>,
    pub timezone: Option<String>,
}

/// Check a request before anything is touched; the message is for the page.
pub fn check(request: &SetupRequest, config: &Config) -> Result<Accepted, String> {
    wifi::wpa_conf(&config.country, &request.ssid, &request.password).map_err(|why| {
        match why {
            wifi::Refused::SsidEmpty => "choose a Wi-Fi network",
            wifi::Refused::SsidTooLong => "the Wi-Fi name is longer than 32 bytes",
            wifi::Refused::SsidControl => "the Wi-Fi name has a character the lamp cannot use",
            wifi::Refused::PasswordLength => "a Wi-Fi password has 8 to 63 characters",
            wifi::Refused::PasswordCharacters => {
                "the Wi-Fi password has a character the lamp cannot use"
            }
            wifi::Refused::Country => "the lamp's country is not set",
        }
        .to_owned()
    })?;
    let credential = request.llm_api_key.trim();
    if credential.is_empty()
        || credential.len() > 512
        || !credential.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err("the voice's key is missing or has a character it cannot have".into());
    }
    let name = request.name.trim();
    if name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err("a name has at most 64 characters".into());
    }
    let timezone = request.timezone.trim();
    if !timezone.is_empty() && !zone_known(timezone, &config.zoneinfo) {
        return Err("the time zone is not one the lamp knows".into());
    }
    Ok(Accepted {
        ssid: request.ssid.trim().to_owned(),
        password: request.password.clone(),
        credential: credential.to_owned(),
        name: (!name.is_empty()).then(|| name.to_owned()),
        timezone: (!timezone.is_empty()).then(|| timezone.to_owned()),
    })
}

/// An IANA name such as `Asia/Ho_Chi_Minh` with a file under the zone data.
pub fn zone_known(zone: &str, zoneinfo: &Path) -> bool {
    zone.len() <= 64
        && zone
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && zone
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b))
        && zoneinfo.join(zone).is_file()
}

/// What a finished setup writes. Injected for tests.
pub trait Save {
    fn credential(&mut self, key: &str) -> io::Result<()>;
    fn name(&mut self, name: &str) -> io::Result<()>;
    fn timezone(&mut self, zone: &str) -> io::Result<()>;
    fn marker(&mut self) -> io::Result<()>;
}

/// Write a private file durably: temporary file, sync, rename, directory sync.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("lamp-setup-tmp");
    let _ = fs::remove_file(&temporary);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

/// The files named in the configuration.
pub struct Files<'a>(pub &'a Config);

impl Save for Files<'_> {
    fn credential(&mut self, key: &str) -> io::Result<()> {
        write_private(&self.0.credential, key.as_bytes())
    }
    fn name(&mut self, name: &str) -> io::Result<()> {
        write_private(&self.0.name, format!("{name}\n").as_bytes())
    }
    fn timezone(&mut self, zone: &str) -> io::Result<()> {
        // V1's `timezone.go`: /etc/localtime links to the zone, /etc/timezone
        // names it. The link is made beside and renamed into place.
        let link = self.0.localtime.with_extension("lamp-setup-tmp");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(self.0.zoneinfo.join(zone), &link)?;
        fs::rename(&link, &self.0.localtime)?;
        let mut zone_line = zone.to_owned();
        zone_line.push('\n');
        let temporary = self.0.timezone_file.with_extension("lamp-setup-tmp");
        fs::write(&temporary, zone_line)?;
        fs::rename(&temporary, &self.0.timezone_file)
    }
    fn marker(&mut self) -> io::Result<()> {
        write_private(&self.0.marker, b"")
    }
}

/// One setup run, after V1's 2 s pause: join, then save, then the marker.
/// `station_ip` is the address the home network gave, polled every second
/// while joining so the page learns it before the hotspot is gone.
pub fn run(
    status: &Arc<Mutex<Status>>,
    accepted: &Accepted,
    country: &str,
    host: &mut impl Host,
    save: &mut impl Save,
    station_ip: impl Fn() -> Option<String> + Send + 'static,
) {
    {
        let mut status = status.lock().unwrap();
        status.run += 1;
        status.phase = Phase::Connecting;
        status.error.clear();
        status.lan_ip.clear();
    }
    let watching = Arc::new(Mutex::new(true));
    let watcher = {
        let status = Arc::clone(status);
        let watching = Arc::clone(&watching);
        thread::spawn(move || {
            while *watching.lock().unwrap() {
                if let Some(address) = station_ip() {
                    status.lock().unwrap().lan_ip = address;
                }
                thread::sleep(Duration::from_secs(1));
            }
        })
    };
    let joined = wifi::join(host, country, &accepted.ssid, &accepted.password);
    *watching.lock().unwrap() = false;
    let _ = watcher.join();
    let failure = match joined {
        Joined::Failed { reason } => Some(reason.detail().to_owned()),
        Joined::Connected => {
            let saved = save
                .credential(&accepted.credential)
                .and_then(|()| match &accepted.name {
                    Some(name) => save.name(name),
                    None => Ok(()),
                })
                .and_then(|()| match &accepted.timezone {
                    Some(zone) => save.timezone(zone),
                    None => Ok(()),
                })
                .and_then(|()| save.marker());
            match saved {
                Ok(()) => None,
                Err(_) => {
                    // Joined but not saved: back to the hotspot, so the
                    // person can try again.
                    let _ = host.hotspot();
                    Some("the lamp joined the Wi-Fi but could not save its settings".into())
                }
            }
        }
    };
    let mut status = status.lock().unwrap();
    match failure {
        None => {
            status.phase = Phase::Connected;
            status.set_up_completed = true;
            status.runtime_phase = "starting".into();
        }
        Some(error) => {
            status.phase = Phase::Failed;
            status.error = error;
        }
    }
}

/// V1's reply wrapper: `{"status":1,"data":…,"message":null}`.
pub fn success(data: Value) -> Value {
    json!({"status": 1, "data": data, "message": null})
}

pub fn failure(message: &str) -> Value {
    json!({"status": 0, "data": null, "message": message})
}

/// The station address the home network gave `interface`, if any: not the
/// hotspot's own.
pub fn station_ip(interface: &str, hotspot: &str) -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", interface])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .skip_while(|word| *word != "inet")
        .nth(1)
        .and_then(|cidr| cidr.split('/').next())
        .filter(|address| *address != hotspot)
        .map(String::from)
}
