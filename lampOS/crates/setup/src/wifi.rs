//! Joining the home's Wi-Fi during setup, as V1 does it (`docs/setup.md`):
//! the same failure reasons, the same 60 s bound, the same early stop after
//! two rejected keys, and the hotspot back on failure. V1's own image scripts
//! switch the radio (`device-sta-mode`, `device-ap-mode`); the one change is
//! that the password is written to the private configuration file here, never
//! put on a command line, and a failed join puts the previous file back.

use serde::Serialize;
use std::time::Duration;

/// V1's `setupInternetWait`: the join, the address and the Internet.
pub const JOIN_WAIT: Duration = Duration::from_secs(60);
/// V1's `wifiFailFastThreshold`: a key rejected this often will not pass.
pub const WRONG_KEY_STOP: u32 = 2;

/// Why a join failed. The names are V1's `setup_failure_reason` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    WrongPassword,
    AssociationRejected,
    SsidNotFound,
    NoDhcp,
    NoInternet,
    SsidTooLong,
    WifiConnectError,
    Unknown,
}

impl Reason {
    /// V1's text for the setup screen.
    pub fn detail(self) -> &'static str {
        match self {
            Self::WrongPassword => "wrong WiFi password: the router rejected the key",
            Self::AssociationRejected => "the router refused the connection",
            Self::SsidNotFound => {
                "WiFi network not found: out of range or on a channel the device cannot use"
            }
            Self::NoDhcp => "connected to WiFi but the router gave no IP address",
            Self::NoInternet => {
                "connected to WiFi but there is no internet (captive portal or firewall?)"
            }
            Self::SsidTooLong => "ssid too long: max 32 bytes (802.11 limit)",
            Self::WifiConnectError => "the WiFi could not be started",
            Self::Unknown => "no internet or SSID did not match within 60s",
        }
    }
}

/// What the join last saw (V1's `wifiObservation`).
#[derive(Clone, Debug, Default)]
pub struct Observation {
    /// wpa_supplicant's log lines since this attempt began.
    pub events: Vec<String>,
    /// `wpa_cli status`'s `wpa_state`.
    pub wpa_state: String,
    /// A station address, not the hotspot's.
    pub has_ip: bool,
    pub ssid_matched: bool,
}

/// The highest `auth_failures` wpa_supplicant reported with `WRONG_KEY`, or
/// the number of failed 4-way handshakes, whichever is more.
pub fn wrong_key_failures(events: &[String]) -> u32 {
    let mut reported = 0;
    let mut handshakes = 0;
    for event in events {
        if event.contains("reason=WRONG_KEY")
            && let Some(at) = event.find("auth_failures=")
        {
            let digits: String = event[at + "auth_failures=".len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(count) = digits.parse::<u32>() {
                reported = reported.max(count);
            }
        }
        if event.contains("4-Way Handshake failed") {
            handshakes += 1;
        }
    }
    reported.max(handshakes)
}

/// V1's `classifyWiFiFailure`: key evidence wins over state, since after a
/// wrong key wpa_supplicant rescans and its state reads like "not found".
pub fn classify(observation: &Observation) -> Reason {
    let joined = observation.events.join("\n");
    if wrong_key_failures(&observation.events) > 0 {
        Reason::WrongPassword
    } else if joined.contains("CTRL-EVENT-ASSOC-REJECT")
        || joined.contains("reason=AUTH_FAILED")
        || joined.contains("reason=CONN_FAILED")
    {
        Reason::AssociationRejected
    } else if observation.has_ip && observation.ssid_matched {
        Reason::NoInternet
    } else if observation.wpa_state == "COMPLETED" && !observation.has_ip {
        Reason::NoDhcp
    } else if joined.contains("CTRL-EVENT-NETWORK-NOT-FOUND")
        || (!observation.events.is_empty() && !joined.contains("Trying to associate"))
    {
        Reason::SsidNotFound
    } else {
        Reason::Unknown
    }
}

/// Why a network name, password or country is refused before anything is
/// written. A line break in either would add lines to wpa_supplicant's file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    SsidEmpty,
    SsidTooLong,
    SsidControl,
    PasswordLength,
    PasswordCharacters,
    Country,
}

/// wpa_supplicant's file for one network, as V1's `connect-wifi` writes it.
pub fn wpa_conf(country: &str, ssid: &str, password: &str) -> Result<String, Refused> {
    let ssid = ssid.trim();
    if ssid.is_empty() {
        return Err(Refused::SsidEmpty);
    }
    if ssid.len() > 32 {
        return Err(Refused::SsidTooLong);
    }
    if ssid.chars().any(char::is_control) {
        return Err(Refused::SsidControl);
    }
    if !password.is_empty() && !(8..=63).contains(&password.len()) {
        return Err(Refused::PasswordLength);
    }
    if password.chars().any(|c| !(' '..='~').contains(&c)) {
        return Err(Refused::PasswordCharacters);
    }
    if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(Refused::Country);
    }
    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let key = if password.is_empty() {
        "\tkey_mgmt=NONE\n".to_owned()
    } else {
        format!("\tpsk=\"{}\"\n", quote(password))
    };
    Ok(format!(
        "ctrl_interface=DIR=/run/wpa_supplicant\nupdate_config=1\ncountry={country}\n\
         fast_reauth=1\nap_scan=1\nnetwork={{\n\tssid=\"{}\"\n{key}\tscan_ssid=1\n}}\n",
        quote(ssid)
    ))
}

/// What a join needs from the lamp. Injected for tests.
pub trait Host {
    /// The current Wi-Fi file's bytes, if there is one.
    fn read_conf(&mut self) -> std::io::Result<Option<Vec<u8>>>;
    /// Replace the Wi-Fi file durably, private; `None` removes it.
    fn write_conf(&mut self, bytes: Option<&[u8]>) -> std::io::Result<()>;
    /// V1's `device-sta-mode` (the hotspot goes) and `device-ap-mode`.
    fn station(&mut self) -> std::io::Result<()>;
    fn hotspot(&mut self) -> std::io::Result<()>;
    /// V1's `CheckInternet`: ping, else a TLS reach.
    fn internet(&mut self) -> bool;
    /// The network joined now, its station address, wpa_supplicant's state.
    fn current_ssid(&mut self) -> Option<String>;
    fn has_station_ip(&mut self) -> bool;
    fn wpa_state(&mut self) -> String;
    /// wpa_supplicant's log since this attempt began.
    fn wpa_events(&mut self) -> Vec<String>;
    /// Monotonic milliseconds, and a wait.
    fn now_ms(&mut self) -> u64;
    fn sleep(&mut self, duration: Duration);
}

/// The outcome of one join.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Joined {
    Connected,
    Failed { reason: Reason },
}

/// Join `ssid` as V1 does, with the file written here. On failure the
/// previous file is put back, nothing of this attempt is kept, and the
/// hotspot comes back.
pub fn join(host: &mut impl Host, country: &str, ssid: &str, password: &str) -> Joined {
    let conf = match wpa_conf(country, ssid, password) {
        Ok(conf) => conf,
        Err(Refused::SsidTooLong) => {
            return Joined::Failed {
                reason: Reason::SsidTooLong,
            };
        }
        Err(_) => {
            return Joined::Failed {
                reason: Reason::WifiConnectError,
            };
        }
    };
    let ssid = ssid.trim();
    let previous = match host.read_conf() {
        Ok(previous) => previous,
        Err(_) => {
            return Joined::Failed {
                reason: Reason::WifiConnectError,
            };
        }
    };
    if host.write_conf(Some(conf.as_bytes())).is_err() || host.station().is_err() {
        return fail(host, previous.as_deref(), Reason::WifiConnectError);
    }
    let started = host.now_ms();
    let deadline = started + JOIN_WAIT.as_millis() as u64;
    let mut attempt = 0u32;
    while host.now_ms() < deadline {
        if host.internet() && host.current_ssid().as_deref() == Some(ssid) {
            return Joined::Connected;
        }
        if attempt % 3 == 2 && wrong_key_failures(&host.wpa_events()) >= WRONG_KEY_STOP {
            break;
        }
        attempt += 1;
        host.sleep(Duration::from_secs(1));
    }
    let observation = Observation {
        events: host.wpa_events(),
        wpa_state: host.wpa_state(),
        has_ip: host.has_station_ip(),
        ssid_matched: host.current_ssid().as_deref() == Some(ssid),
    };
    fail(host, previous.as_deref(), classify(&observation))
}

/// Nothing of a failed attempt is kept: the previous file goes back, and the
/// hotspot with it.
fn fail(host: &mut impl Host, previous: Option<&[u8]>, reason: Reason) -> Joined {
    let _ = host.write_conf(previous);
    let _ = host.hotspot();
    Joined::Failed { reason }
}

/// The lamp as V1's image leaves it: wpa_supplicant on `wlan0`, its file in
/// `/etc/wpa_supplicant`, and the image's two radio scripts.
pub struct ImageHost {
    pub conf: std::path::PathBuf,
    pub station_script: std::path::PathBuf,
    pub hotspot_script: std::path::PathBuf,
    pub interface: String,
    /// Reached over TCP when ping is blocked, as V1 falls back to TLS.
    pub reach: String,
    started: std::time::Instant,
    since_unix: u64,
}

impl ImageHost {
    pub fn new() -> Self {
        Self {
            conf: "/etc/wpa_supplicant/wpa_supplicant-wlan0.conf".into(),
            station_script: "/usr/local/bin/device-sta-mode".into(),
            hotspot_script: "/usr/local/bin/device-ap-mode".into(),
            interface: "wlan0".into(),
            reach: "generativelanguage.googleapis.com:443".into(),
            started: std::time::Instant::now(),
            since_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
        }
    }

    fn run(&self, program: &std::path::Path) -> std::io::Result<()> {
        let status = std::process::Command::new(program).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "{} ended with {status}",
                program.display()
            )))
        }
    }

    /// `wpa_cli status`, as its key=value lines.
    fn status(&self) -> Vec<(String, String)> {
        std::process::Command::new("wpa_cli")
            .args(["-i", &self.interface, "status"])
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn field(&self, key: &str) -> Option<String> {
        self.status()
            .into_iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    }
}

impl Default for ImageHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for ImageHost {
    fn read_conf(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        match std::fs::read(&self.conf) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write_conf(&mut self, bytes: Option<&[u8]>) -> std::io::Result<()> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let Some(bytes) = bytes else {
            return match std::fs::remove_file(&self.conf) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        };
        let temporary = self.conf.with_extension("conf.lamp-setup");
        let _ = std::fs::remove_file(&temporary);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &self.conf)?;
        if let Some(parent) = self.conf.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    }

    fn station(&mut self) -> std::io::Result<()> {
        // wpa_supplicant restarts here, so its log from now on is this join's.
        self.since_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        self.run(&self.station_script.clone())
    }

    fn hotspot(&mut self) -> std::io::Result<()> {
        self.run(&self.hotspot_script.clone())
    }

    fn internet(&mut self) -> bool {
        let ping = std::process::Command::new("ping")
            .args(["-c", "1", "-W", "2", "8.8.8.8"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        ping || std::net::ToSocketAddrs::to_socket_addrs(&self.reach.as_str())
            .ok()
            .and_then(|mut addresses| addresses.next())
            .is_some_and(|address| {
                std::net::TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok()
            })
    }

    fn current_ssid(&mut self) -> Option<String> {
        self.field("ssid")
    }

    fn has_station_ip(&mut self) -> bool {
        self.field("ip_address")
            .is_some_and(|address| !address.is_empty() && address != "192.168.100.1")
    }

    fn wpa_state(&mut self) -> String {
        self.field("wpa_state").unwrap_or_default()
    }

    fn wpa_events(&mut self) -> Vec<String> {
        std::process::Command::new("journalctl")
            .args(["-t", "wpa_supplicant", "--no-pager", "-o", "cat", "--since"])
            .arg(format!("@{}", self.since_unix))
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn now_ms(&mut self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
