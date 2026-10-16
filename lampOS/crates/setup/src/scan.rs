//! The home networks the lamp can see, as V1 lists them (`parseIWScan` in
//! `system/network/service.go`): `iw dev wlan0 scan`, one entry per BSS with a
//! name, its first `SSID:` line, its signal in dBm and its channel.

use serde::Serialize;

/// V1's `domain.Network`, field for field, so V1's page reads it unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Network {
    pub bssid: String,
    pub ssid: String,
    pub mode: String,
    pub channel: u32,
    pub rate: String,
    pub signal: i32,
    #[serde(rename = "linkRate")]
    pub link_rate: u32,
    pub security: String,
}

/// iw and wpa_cli write `\xNN` for a byte and `\ ` for a space.
pub fn decode_ssid(text: &str) -> String {
    if !text.contains('\\') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' && at + 1 < bytes.len() {
            if bytes[at + 1] == b'x'
                && at + 3 < bytes.len()
                && let Ok(byte) = u8::from_str_radix(&text[at + 2..at + 4], 16)
            {
                out.push(byte);
                at += 4;
                continue;
            }
            if bytes[at + 1] == b' ' {
                out.push(b' ');
                at += 2;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_mac(text: &str) -> Option<&str> {
    let mac = text.split(['(', ' ']).next()?;
    (mac.len() == 17 && mac.bytes().all(|b| b.is_ascii_hexdigit() || b == b':')).then_some(mac)
}

pub fn parse(output: &str) -> Vec<Network> {
    let mut list = Vec::new();
    let mut current: Option<Network> = None;
    let push = |network: Option<Network>, list: &mut Vec<Network>| {
        if let Some(network) = network
            && !network.ssid.is_empty()
        {
            list.push(network);
        }
    };
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("BSS ")
            && let Some(mac) = hex_mac(rest)
        {
            push(current.take(), &mut list);
            current = Some(Network {
                bssid: mac.to_owned(),
                ssid: String::new(),
                mode: "STA".into(),
                channel: 0,
                rate: String::new(),
                signal: 0,
                link_rate: 0,
                security: String::new(),
            });
            continue;
        }
        let Some(network) = current.as_mut() else {
            continue;
        };
        if let Some(ssid) = line.strip_prefix("SSID: ") {
            if network.ssid.is_empty() {
                network.ssid = decode_ssid(ssid.trim());
            }
        } else if let Some(signal) = line.strip_prefix("signal: ") {
            let value = signal.split_whitespace().next().unwrap_or("0");
            network.signal = value.parse::<f64>().map_or(0, |dbm| dbm as i32);
        } else if let Some(channel) = line.strip_prefix("DS Parameter set: channel ") {
            network.channel = channel.trim().parse().unwrap_or(0);
        }
    }
    push(current, &mut list);
    list
}

/// How long a page's scan may take before it is given up.
pub const SCAN_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// `iw dev <interface> scan` on the lamp, given up after [`SCAN_LIMIT`].
pub fn scan(interface: &str) -> std::io::Result<Vec<Network>> {
    scan_within(interface, SCAN_LIMIT)
}

/// `iw dev <interface> scan`, given up and killed after `limit`: a stuck
/// `iw` holds neither the hotspot nor the one scan at a time for long
/// (the fifth on #1045). Its output is read as it comes, so a long list
/// never fills the pipe and stalls it.
pub fn scan_within(interface: &str, limit: std::time::Duration) -> std::io::Result<Vec<Network>> {
    run_within(
        std::process::Command::new("iw").args(["dev", interface, "scan"]),
        limit,
    )
    .map(|text| parse(&text))
}

/// Run a command for its standard output, killed after `limit`.
fn run_within(
    command: &mut std::process::Command,
    limit: std::time::Duration,
) -> std::io::Result<String> {
    use std::io::Read;
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stdout.read_to_end(&mut text);
        text
    });
    let until = std::time::Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("iw scan took longer than {} ms", limit.as_millis()),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let text = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(std::io::Error::other("iw scan failed"));
    }
    Ok(String::from_utf8_lossy(&text).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_command_that_hangs_is_killed_at_its_limit() {
        let started = Instant::now();
        let error = run_within(
            std::process::Command::new("sleep").arg("30"),
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_long_output_is_read_whole_without_stalling_the_command() {
        // 256 KiB, more than a pipe holds.
        let text = run_within(
            std::process::Command::new("sh").args(["-c", "head -c 262144 /dev/zero | tr '\\0' x"]),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(text.len(), 262_144);
        assert!(
            run_within(
                &mut std::process::Command::new("false"),
                Duration::from_secs(5)
            )
            .is_err()
        );
    }
}
