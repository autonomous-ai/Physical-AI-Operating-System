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

/// `iw dev <interface> scan` on the lamp.
pub fn scan(interface: &str) -> std::io::Result<Vec<Network>> {
    let output = std::process::Command::new("iw")
        .args(["dev", interface, "scan"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("iw scan failed"));
    }
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}
