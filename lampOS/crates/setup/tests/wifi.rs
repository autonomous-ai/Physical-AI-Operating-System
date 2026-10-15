use lamp_setup::wifi::{
    Host, Joined, Observation, Reason, Refused, WRONG_KEY_STOP, classify, decode_wpa, join,
    wpa_conf, wrong_key_failures,
};
use std::{io, time::Duration};

// V1's real wpa_supplicant lines (intern-v2-d94b, 2026-09-28,
// `system/network/setup_failure_test.go`): a wrong password, then the right one.
fn wrong_password() -> Vec<String> {
    [
        "Successfully initialized wpa_supplicant",
        "wlan0: Trying to associate with 34:3a:20:95:78:93 (SSID='Glinks' freq=5745 MHz)",
        "wlan0: Associated with 34:3a:20:95:78:93",
        "wlan0: WPA: 4-Way Handshake failed - pre-shared key may be incorrect",
        r#"wlan0: CTRL-EVENT-SSID-TEMP-DISABLED id=0 ssid="Glinks" auth_failures=1 duration=10 reason=WRONG_KEY"#,
        "wlan0: CTRL-EVENT-DISCONNECTED bssid=34:3a:20:95:78:93 reason=3",
        "wlan0: WPA: 4-Way Handshake failed - pre-shared key may be incorrect",
        r#"wlan0: CTRL-EVENT-SSID-TEMP-DISABLED id=0 ssid="Glinks" auth_failures=2 duration=30 reason=WRONG_KEY"#,
    ]
    .map(String::from)
    .to_vec()
}

fn connected() -> Vec<String> {
    [
        "wlan0: Trying to associate with 34:3a:20:95:78:93 (SSID='Glinks' freq=5745 MHz)",
        "wlan0: Associated with 34:3a:20:95:78:93",
        "wlan0: WPA: Key negotiation completed with 34:3a:20:95:78:93 [PTK=CCMP GTK=CCMP]",
        "wlan0: CTRL-EVENT-CONNECTED - Connection to 34:3a:20:95:78:93 completed [id=0 id_str=]",
    ]
    .map(String::from)
    .to_vec()
}

fn seen(events: Vec<String>, state: &str, has_ip: bool, ssid_matched: bool) -> Observation {
    Observation {
        events,
        wpa_state: state.into(),
        has_ip,
        ssid_matched,
    }
}

#[test]
fn every_v1_failure_case_gets_v1s_reason() {
    let lines = |lines: &[&str]| lines.iter().map(|line| line.to_string()).collect();
    for (observation, want) in [
        (
            seen(wrong_password(), "SCANNING", false, false),
            Reason::WrongPassword,
        ),
        (
            seen(wrong_password()[..5].to_vec(), "SCANNING", false, false),
            Reason::WrongPassword,
        ),
        (
            seen(
                lines(&[
                    "wlan0: Trying to associate with aa (SSID='Home')",
                    "wlan0: CTRL-EVENT-ASSOC-REJECT bssid=aa status_code=17",
                ]),
                "",
                false,
                false,
            ),
            Reason::AssociationRejected,
        ),
        (
            seen(
                lines(&[
                    "Successfully initialized wpa_supplicant",
                    "wlan0: CTRL-EVENT-NETWORK-NOT-FOUND",
                ]),
                "SCANNING",
                false,
                false,
            ),
            Reason::SsidNotFound,
        ),
        (
            seen(
                lines(&["Successfully initialized wpa_supplicant"]),
                "SCANNING",
                false,
                false,
            ),
            Reason::SsidNotFound,
        ),
        (seen(connected(), "COMPLETED", false, false), Reason::NoDhcp),
        (
            seen(connected(), "COMPLETED", true, true),
            Reason::NoInternet,
        ),
        (Observation::default(), Reason::Unknown),
    ] {
        assert_eq!(classify(&observation), want, "{:?}", observation.events);
    }
}

#[test]
fn wpa_cli_names_are_read_back_to_the_bytes_they_stand_for() {
    // Reviewer, #817: "Nhà Lan" comes back from `wpa_cli status` escaped.
    assert_eq!(decode_wpa(r"Nh\xc3\xa0 Lan"), "Nhà Lan");
    assert_eq!(decode_wpa(r#"My \"Home\" \\ net"#), r#"My "Home" \ net"#);
    assert_eq!(decode_wpa("Home"), "Home");
    assert_eq!(decode_wpa(r"tab\there"), "tab\there");
    // A stray backslash that escapes nothing is kept.
    assert_eq!(decode_wpa(r"a\qb"), r"a\qb");
}

#[test]
fn two_rejected_keys_reach_the_early_stop_and_one_does_not() {
    assert!(wrong_key_failures(&wrong_password()[..5]) < WRONG_KEY_STOP);
    assert!(wrong_key_failures(&wrong_password()) >= WRONG_KEY_STOP);
    assert_eq!(wrong_key_failures(&connected()), 0);
}

#[test]
fn the_file_is_v1s_with_the_name_in_hex_and_nothing_can_add_a_line_to_it() {
    assert_eq!(
        wpa_conf("US", "Home", "password1").unwrap(),
        "ctrl_interface=DIR=/run/wpa_supplicant\nupdate_config=1\ncountry=US\nfast_reauth=1\n\
         ap_scan=1\nnetwork={\n\tssid=486f6d65\n\tpsk=\"password1\"\n\tscan_ssid=1\n}\n"
    );
    assert!(
        wpa_conf("VN", "Cafe", "")
            .unwrap()
            .contains("\tkey_mgmt=NONE\n")
    );
    // wpa_supplicant reads between the first and last quote as it is: no
    // escapes. The name is hex, so its quotes and backslashes are just bytes.
    let quoted = wpa_conf("US", r#"My "Home" \ net"#, r#"pa"ss\word"#).unwrap();
    assert!(
        quoted.contains("\tssid=4d792022486f6d6522205c206e6574\n"),
        "{quoted}"
    );
    assert!(quoted.contains(r#"psk="pa"ss\word""#), "{quoted}");
    // A 63-character password with a backslash stays 63 characters.
    let long = format!("{}\\", "p".repeat(62));
    assert!(wpa_conf("US", "Home", &long).is_ok());
    // Accented names are bytes too.
    assert!(
        wpa_conf("VN", "Nhà Lan", "password1")
            .unwrap()
            .contains("ssid=4e68c3a0204c616e")
    );
    for (ssid, password, why) in [
        ("", "password1", Refused::SsidEmpty),
        ("   ", "password1", Refused::SsidEmpty),
        ("Home\nnetwork={", "password1", Refused::SsidControl),
        (&"x".repeat(33), "password1", Refused::SsidTooLong),
        ("Home", "short", Refused::PasswordLength),
        ("Home", &"p".repeat(64), Refused::PasswordLength),
        ("Home", "pass\nword1", Refused::PasswordCharacters),
        ("Home", "mật khẩu dài", Refused::PasswordCharacters),
    ] {
        assert_eq!(
            wpa_conf("US", ssid, password),
            Err(why),
            "{ssid:?} {password:?}"
        );
    }
    assert_eq!(wpa_conf("us", "Home", "password1"), Err(Refused::Country));
    // 32 bytes is the 802.11 limit, counted in bytes.
    assert!(wpa_conf("US", &"é".repeat(16), "password1").is_ok());
    assert_eq!(
        wpa_conf("US", &"é".repeat(17), "password1"),
        Err(Refused::SsidTooLong)
    );
}

/// A lamp whose network comes up after `connect_after` seconds, or never.
#[derive(Default)]
struct Lamp {
    conf: Option<Vec<u8>>,
    written: Vec<Option<Vec<u8>>>,
    calls: Vec<&'static str>,
    now_ms: u64,
    joined_ssid: Option<String>,
    connect_after_s: Option<u64>,
    events: Vec<String>,
    events_after_s: u64,
    station_fails: bool,
}

impl Host for Lamp {
    fn read_conf(&mut self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.conf.clone())
    }
    fn write_conf(&mut self, bytes: Option<&[u8]>) -> io::Result<()> {
        self.conf = bytes.map(<[u8]>::to_vec);
        self.written.push(self.conf.clone());
        Ok(())
    }
    fn station(&mut self) -> io::Result<()> {
        self.calls.push("station");
        if self.station_fails {
            return Err(io::Error::other("device-sta-mode failed"));
        }
        Ok(())
    }
    fn hotspot(&mut self) -> io::Result<()> {
        self.calls.push("hotspot");
        Ok(())
    }
    fn internet(&mut self) -> bool {
        self.connect_after_s
            .is_some_and(|after| self.now_ms >= after * 1_000)
    }
    fn current_ssid(&mut self) -> Option<String> {
        self.internet().then(|| self.joined_ssid.clone()).flatten()
    }
    fn has_station_ip(&mut self) -> bool {
        self.internet()
    }
    fn wpa_state(&mut self) -> String {
        "SCANNING".into()
    }
    fn wpa_events(&mut self) -> Vec<String> {
        if self.now_ms >= self.events_after_s * 1_000 {
            self.events.clone()
        } else {
            vec![]
        }
    }
    fn now_ms(&mut self) -> u64 {
        self.now_ms
    }
    fn sleep(&mut self, duration: Duration) {
        self.now_ms += duration.as_millis() as u64;
    }
}

const OLD: &[u8] = b"country=US\nctrl_interface=DIR=/run/wpa_supplicant\nupdate_config=1\n";

#[test]
fn a_good_password_joins_and_keeps_the_new_file() {
    let mut lamp = Lamp {
        conf: Some(OLD.to_vec()),
        joined_ssid: Some("Home".into()),
        connect_after_s: Some(9),
        ..Lamp::default()
    };
    assert_eq!(
        join(&mut lamp, "US", " Home ", "password1"),
        Joined::Connected
    );
    assert_eq!(lamp.calls, ["station"]);
    assert!(
        String::from_utf8(lamp.conf.unwrap())
            .unwrap()
            .contains("psk=\"password1\"")
    );
    assert_eq!(lamp.now_ms, 9_000);
}

#[test]
fn a_wrong_password_stops_early_puts_the_old_file_back_and_the_hotspot_up() {
    let mut lamp = Lamp {
        conf: Some(OLD.to_vec()),
        events: wrong_password(),
        events_after_s: 4,
        ..Lamp::default()
    };
    assert_eq!(
        join(&mut lamp, "US", "Glinks", "password1"),
        Joined::Failed {
            reason: Reason::WrongPassword
        }
    );
    // Checked on every third second: the two rejections seen at 4 s end it at 5 s.
    assert_eq!(lamp.now_ms, 5_000);
    assert_eq!(lamp.conf.as_deref(), Some(OLD));
    assert_eq!(lamp.calls, ["station", "hotspot"]);
}

#[test]
fn a_network_never_found_waits_the_full_minute_then_says_so() {
    let mut lamp = Lamp {
        events: vec!["Successfully initialized wpa_supplicant".into()],
        ..Lamp::default()
    };
    assert_eq!(
        join(&mut lamp, "US", "Away", "password1"),
        Joined::Failed {
            reason: Reason::SsidNotFound
        }
    );
    assert_eq!(lamp.now_ms, 60_000);
    // There was no file before, so there is none after.
    assert_eq!(lamp.conf, None);
    assert_eq!(lamp.written.last(), Some(&None));
}

#[test]
fn a_joined_network_with_another_name_is_not_this_join() {
    let mut lamp = Lamp {
        joined_ssid: Some("Neighbour".into()),
        connect_after_s: Some(1),
        ..Lamp::default()
    };
    assert!(matches!(
        join(&mut lamp, "US", "Home", "password1"),
        Joined::Failed { .. }
    ));
    assert_eq!(lamp.now_ms, 60_000);
}

#[test]
fn a_refused_name_touches_nothing_and_a_radio_failure_restores_everything() {
    let mut lamp = Lamp {
        conf: Some(OLD.to_vec()),
        ..Lamp::default()
    };
    assert_eq!(
        join(&mut lamp, "US", &"x".repeat(33), "password1"),
        Joined::Failed {
            reason: Reason::SsidTooLong
        }
    );
    assert_eq!(
        join(&mut lamp, "US", "Home\n}", "password1"),
        Joined::Failed {
            reason: Reason::WifiConnectError
        }
    );
    assert!(lamp.calls.is_empty() && lamp.written.is_empty());

    let mut lamp = Lamp {
        conf: Some(OLD.to_vec()),
        station_fails: true,
        ..Lamp::default()
    };
    assert_eq!(
        join(&mut lamp, "US", "Home", "password1"),
        Joined::Failed {
            reason: Reason::WifiConnectError
        }
    );
    assert_eq!(lamp.conf.as_deref(), Some(OLD));
    assert_eq!(lamp.calls, ["station", "hotspot"]);
}
