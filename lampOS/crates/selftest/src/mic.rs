//! The voice microphone (Jieli): a second of its blocks on `mic2/audio`,
//! as `lamp-voice` gives them after its echo canceller, and their level;
//! whether it is on USB now (its sound card, `device_micro2`), and how many
//! times it fell off USB since boot, from the kernel's log. Only listened to
//! and read: the self-test never opens a microphone, which is the voice's
//! and behind its privacy switch.
use crate::{Check, kernel};
use lamp_bus::Bus;
use lamp_msgs::{Audio, PartResult, Verdict};
use std::time::{Duration, Instant};

/// The topic the voice microphone is heard on.
pub const TOPIC: &str = Audio::TOPICS[1];
/// How much of it is listened to.
pub const LISTEN: Duration = Duration::from_secs(1);
/// With nothing in this long, nothing is coming.
pub const FIRST_WITHIN: Duration = Duration::from_secs(2);
/// A microphone at or under this level gives nothing.
pub const SILENT_DB: f32 = crate::listen::SILENT_DB;

/// The voice microphone's sound card, by the name the lamp's udev rule
/// gives it.
pub const CARD: &str = "device_micro2";
/// Where the kernel lists its sound cards.
pub const SOUND: &str = "/sys/class/sound";

/// The voice microphone on USB: whether its card is there now, its port,
/// and how often the kernel saw it go since boot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usb {
    /// Its sound card is there now.
    pub present: bool,
    /// Its USB port (`3-1.1.1`), from its card now or from the kernel's
    /// record of a Jieli device being found there.
    pub port: Option<String>,
    /// `USB disconnect` records for that port since boot; `None` if the
    /// kernel's log could not be read.
    pub drops: Option<usize>,
}

/// The USB port a sound card's device path sits on: the last path
/// component before its interface (`…/3-1.1.1/3-1.1.1:1.1` gives `3-1.1.1`).
pub fn port_of(device: &str) -> Option<String> {
    let interface = device.rsplit('/').next()?;
    let (port, _) = interface.split_once(':')?;
    (!port.is_empty() && port.contains('-')).then(|| port.to_owned())
}

/// The kernel's USB records, counted: the ports a Jieli device was found
/// on, and each port's disconnects. A record is `pri,seq,usec,flags;text`,
/// and the text `usb PORT: Manufacturer: …` or `usb PORT: USB disconnect, …`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsbRecords {
    pub jieli_ports: Vec<String>,
    pub disconnects: Vec<String>,
}

impl UsbRecords {
    pub fn count(&mut self, record: &str) {
        let text = record.split_once(';').map_or(record, |(_, text)| text);
        let Some(rest) = text.strip_prefix("usb ") else {
            return;
        };
        let Some((port, said)) = rest.split_once(": ") else {
            return;
        };
        if said.starts_with("USB disconnect") {
            self.disconnects.push(port.to_owned());
        } else if said.starts_with("Manufacturer: Jieli")
            && !self.jieli_ports.iter().any(|p| p == port)
        {
            self.jieli_ports.push(port.to_owned());
        }
    }

    /// Disconnects on `port`.
    pub fn drops(&self, port: &str) -> usize {
        self.disconnects.iter().filter(|p| *p == port).count()
    }
}

/// Reads whether the card is there and its port (from `sound`), and the
/// drops since boot (from the kernel's log at `kmsg`).
pub fn usb(sound: &std::path::Path, kmsg: &str) -> Usb {
    let card_port = std::fs::read_dir(sound).ok().and_then(|cards| {
        cards.flatten().find_map(|card| {
            let id = std::fs::read_to_string(card.path().join("id")).ok()?;
            (id.trim() == CARD).then(|| {
                std::fs::canonicalize(card.path().join("device"))
                    .ok()
                    .and_then(|device| port_of(&device.to_string_lossy()))
            })
        })
    });
    let present = card_port.is_some();
    let mut records = UsbRecords::default();
    let read = kernel::each_record(kmsg, |line| records.count(line)).is_ok();
    let port = card_port
        .flatten()
        .or_else(|| records.jieli_ports.last().cloned());
    let drops = match (&port, read) {
        (Some(port), true) => Some(records.drops(port)),
        (None, true) => Some(0),
        (_, false) => None,
    };
    Usb {
        present,
        port,
        drops,
    }
}

/// What a second of listening came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    pub blocks: usize,
    /// Blocks missing between those heard, by their sequence.
    pub lost: u64,
    pub sum_squares: f64,
    pub samples: usize,
}

impl Heard {
    /// Adds one message's payload: the header line, then S16LE samples.
    pub fn add(&mut self, payload: &[u8]) -> Result<(), String> {
        let (header, samples) = Audio::split(payload).map_err(|e| e.to_string())?;
        if header.sample_format != Audio::S16LE {
            return Err(format!("samples are {}, not S16LE", header.sample_format));
        }
        self.blocks += 1;
        for pair in samples.chunks_exact(2) {
            let sample = f64::from(i16::from_le_bytes([pair[0], pair[1]]));
            self.sum_squares += sample * sample;
            self.samples += 1;
        }
        Ok(())
    }

    /// The level of everything heard, in dBFS.
    pub fn level_db(&self) -> f32 {
        if self.samples == 0 || self.sum_squares == 0.0 {
            return -120.0;
        }
        let rms = (self.sum_squares / self.samples as f64).sqrt() / 32_768.0;
        (20.0 * rms.log10()) as f32
    }
}

/// Judges what was heard, and says the microphone's USB. `declared`:
/// whether the voice ever said it would publish this microphone. Its card
/// gone with nothing heard fails it: the voice cannot open it (#1178, on
/// lamp-4ace twice on 17 Oct). Drops since boot are only said, never a
/// fail by themselves (the first builder, 17 Oct 13:18).
pub fn voice_mic(declared: bool, heard: &Result<Heard, String>, usb: Option<&Usb>) -> PartResult {
    let mut result = judged(declared, heard, usb);
    if let Some(usb) = usb {
        if let Some(port) = &usb.port {
            result = result.with("usb_port", port);
        }
        result = result.with("on_usb", usb.present);
        result = match usb.drops {
            Some(drops) => result.with("usb_drops", drops),
            None => result.with("usb_drops", "unknown: the kernel's log could not be read"),
        };
        if let Some(drops @ 1..) = usb.drops
            && result.verdict != Verdict::Fail
        {
            let off = format!(
                "off USB {drops} time{} since boot",
                if drops == 1 { "" } else { "s" }
            );
            result.reason = if result.reason.is_empty() {
                off
            } else {
                format!("{}; {off}", result.reason)
            };
        }
    }
    result
}

fn judged(declared: bool, heard: &Result<Heard, String>, usb: Option<&Usb>) -> PartResult {
    let part = "voice_mic";
    let heard = match heard {
        Err(why) => {
            return PartResult::new(
                part,
                Verdict::Fail,
                &format!("{TOPIC} cannot be read: {why}"),
            );
        }
        Ok(heard) => heard,
    };
    if heard.blocks == 0 {
        if let Some(usb) = usb.filter(|usb| !usb.present) {
            let drops = match usb.drops {
                Some(drops) => format!(
                    ", off USB {drops} time{} since boot",
                    if drops == 1 { "" } else { "s" }
                ),
                None => String::new(),
            };
            return PartResult::new(
                part,
                Verdict::Fail,
                &format!("not on USB now: no sound card {CARD}{drops}"),
            );
        }
        let why = if declared {
            "no sound for 2 s: the microphone is closed (privacy switch) or lamp-voice stopped giving it"
        } else {
            "nothing was ever said on mic2/audio: lamp-voice gives it only with LAMP_BUS=on and LAMP_BUS_AUDIO=on"
        };
        return PartResult::new(part, Verdict::NotChecked, why);
    }
    let level = heard.level_db();
    let result = if level <= SILENT_DB {
        PartResult::new(
            part,
            Verdict::Fail,
            &format!("silent at {level:.0} dBFS: the microphone gives nothing"),
        )
    } else {
        PartResult::new(part, Verdict::Pass, "")
    };
    result
        .with("level_db", format!("{level:.1}"))
        .with("blocks", heard.blocks)
        .with("lost", heard.lost)
}

/// Listens to the voice microphone for a second.
pub struct VoiceMic<'a> {
    pub bus: &'a Bus,
}

impl VoiceMic<'_> {
    fn listen(&self, deadline: Instant) -> Result<Heard, String> {
        let mut hears = self
            .bus
            .subscribe_typed(TOPIC, "selftest", Audio::TYPE)
            .map_err(|e| e.to_string())?;
        let begun = Instant::now();
        let mut heard = Heard::default();
        let mut last: Option<u64> = None;
        let mut first_at: Option<Instant> = None;
        loop {
            let now = Instant::now();
            let until = match first_at {
                Some(at) => at + LISTEN,
                None => begun + FIRST_WITHIN,
            }
            .min(deadline);
            if now >= until {
                break;
            }
            let Some(message) = hears.recv(until - now).map_err(|e| e.to_string())? else {
                continue;
            };
            if message.expired {
                continue;
            }
            first_at.get_or_insert_with(Instant::now);
            if let Ok((header, _)) = Audio::split(&message.payload) {
                if let Some(before) = last {
                    heard.lost += header.sequence.saturating_sub(before + 1);
                }
                last = Some(header.sequence);
            }
            heard.add(&message.payload)?;
        }
        Ok(heard)
    }
}

impl Check for VoiceMic<'_> {
    fn part(&self) -> &str {
        "voice_mic"
    }

    fn run(&mut self, deadline: Instant) -> PartResult {
        let declared = self.bus.declaration(TOPIC).ok().flatten().is_some();
        let heard = self.listen(deadline);
        let usb = cfg!(target_os = "linux").then(|| usb(std::path::Path::new(SOUND), kernel::KMSG));
        voice_mic(declared, &heard, usb.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(sequence: u64, amplitude: i16) -> Vec<u8> {
        let samples: Vec<u8> = (0..320)
            .flat_map(|n| (if n % 2 == 0 { amplitude } else { -amplitude }).to_le_bytes())
            .collect();
        Audio {
            sample_rate: 16_000,
            channels: 1,
            sample_format: Audio::S16LE.into(),
            sequence,
        }
        .block(&samples)
        .unwrap()
    }

    #[test]
    fn a_quiet_room_passes_with_its_level_and_digital_silence_fails() {
        let mut room = Heard::default();
        for n in 0..50 {
            room.add(&block(n, 30)).unwrap();
        }
        let result = voice_mic(true, &Ok(room), None);
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        assert!(
            result
                .values
                .iter()
                .any(|v| v.key == "level_db" && v.value == "-60.8"),
            "{result:?}"
        );
        let mut dead = Heard::default();
        dead.add(&block(0, 0)).unwrap();
        let silent = voice_mic(true, &Ok(dead), None);
        assert_eq!(silent.verdict, Verdict::Fail);
        assert!(silent.reason.contains("silent"));
    }

    #[test]
    fn no_sound_is_not_checked_and_says_whether_it_was_ever_given() {
        let closed = voice_mic(true, &Ok(Heard::default()), None);
        assert_eq!(closed.verdict, Verdict::NotChecked);
        assert!(closed.reason.contains("privacy"));
        let never = voice_mic(false, &Ok(Heard::default()), None);
        assert!(never.reason.contains("LAMP_BUS_AUDIO"), "{}", never.reason);
        assert_eq!(
            voice_mic(true, &Err("gone".into()), None).verdict,
            Verdict::Fail
        );
    }

    #[test]
    fn a_second_of_blocks_is_heard_from_the_bus_with_what_was_lost() {
        let root = std::path::PathBuf::from(format!("/tmp/ls-mic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bus = Bus::new(&root).unwrap();
        let mut voice = bus
            .publisher_declared(TOPIC, &Audio::declaration(), "voice")
            .unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let speaker = std::thread::spawn(move || {
            let mut sequence = 0;
            while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
                // Every tenth block is lost on the way.
                if sequence % 10 != 9 {
                    voice
                        .publish_bytes(sequence, Audio::ENCODING, &block(sequence, 300))
                        .unwrap();
                }
                sequence += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let result = VoiceMic { bus: &bus }.run(Instant::now() + Duration::from_secs(10));
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        speaker.join().unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(result.verdict, Verdict::Pass, "{result:?}");
        let value = |key: &str| {
            result
                .values
                .iter()
                .find(|v| v.key == key)
                .map(|v| v.value.parse::<u64>().unwrap())
                .unwrap()
        };
        assert!(value("blocks") >= 30, "{result:?}");
        assert!(value("lost") >= 3, "{result:?}");
    }

    #[test]
    fn a_cards_port_is_the_usb_path_before_its_interface() {
        let device = "/sys/devices/platform/soc@3000000/4200000.ehci1-controller/usb3/3-1/3-1.1/3-1.1.1/3-1.1.1:1.1";
        assert_eq!(port_of(device).as_deref(), Some("3-1.1.1"));
        assert_eq!(
            port_of("/sys/devices/platform/soc@3000000/soc@3000000:i2s4_mach"),
            None
        );
    }

    /// lamp-4ace on 17 Oct (#1178): the Jieli found at boot, then off USB
    /// twice. A disconnect on another port is not its own.
    #[test]
    fn the_kernels_records_give_the_jieli_port_and_its_disconnects() {
        let mut records = UsbRecords::default();
        for line in [
            "6,410,2100000,-;usb 3-1.1.1: new full-speed USB device number 4 using ehci-platform",
            "6,411,2200000,-;usb 3-1.1.1: Manufacturer: Jieli Technology",
            "6,412,2300000,-;usb 3-1.1.3: Manufacturer: C-Media Electronics Inc.",
            "6,9001,44960000000,-;usb 3-1.1.1: USB disconnect, device number 4",
            "6,9020,44965000000,-;usb 3-1.1.1: Manufacturer: Jieli Technology",
            "6,9500,46146000000,-;usb 3-1.1.1: USB disconnect, device number 9",
            "6,9600,46200000000,-;usb 3-1.1.3: USB disconnect, device number 5",
            " SUBSYSTEM=usb",
        ] {
            records.count(line);
        }
        assert_eq!(records.jieli_ports, vec!["3-1.1.1".to_owned()]);
        assert_eq!(records.drops("3-1.1.1"), 2);
        assert_eq!(records.drops("3-1.1.3"), 1);
    }

    /// Read from a sysfs and a kernel log laid out as lamp-75c9's: the
    /// card there gives the port; with the card gone, the kernel's Jieli
    /// record still names it.
    #[cfg(target_os = "linux")]
    #[test]
    fn its_card_and_drops_are_read_from_sysfs_and_the_kernels_log() {
        let root = std::env::temp_dir().join(format!("lamp-selftest-usb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let device = root.join("devices/usb3/3-1/3-1.1/3-1.1.1/3-1.1.1:1.1");
        std::fs::create_dir_all(&device).unwrap();
        let sound = root.join("sound");
        for (card, id) in [("card2", "device_cmedia"), ("card4", CARD)] {
            std::fs::create_dir_all(sound.join(card)).unwrap();
            std::fs::write(sound.join(card).join("id"), format!("{id}\n")).unwrap();
        }
        std::os::unix::fs::symlink(&device, sound.join("card4/device")).unwrap();
        let kmsg = root.join("kmsg");
        std::fs::write(
            &kmsg,
            "6,0,0,-;Booting\n6,1,1,-;usb 3-1.1.1: Manufacturer: Jieli Technology\n6,2,2,-;usb 3-1.1.1: USB disconnect, device number 4\n",
        )
        .unwrap();
        let kmsg = kmsg.to_str().unwrap();
        let there = usb(&sound, kmsg);
        assert_eq!(
            there,
            Usb {
                present: true,
                port: Some("3-1.1.1".into()),
                drops: Some(1),
            }
        );
        std::fs::remove_dir_all(sound.join("card4")).unwrap();
        let gone = usb(&sound, kmsg);
        assert_eq!(
            (gone.present, gone.port.as_deref(), gone.drops),
            (false, Some("3-1.1.1"), Some(1))
        );
        let unread = usb(&sound, "/nonexistent/kmsg");
        assert_eq!((unread.present, unread.drops), (false, None));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Off USB now and nothing heard: a fail, with its drops. On USB, heard
    /// and dropped twice: a pass that says so. Drops never fail it alone.
    #[test]
    fn a_microphone_off_usb_fails_and_its_drops_are_said() {
        let gone = Usb {
            present: false,
            port: Some("3-1.1.1".into()),
            drops: Some(2),
        };
        let off = voice_mic(true, &Ok(Heard::default()), Some(&gone));
        assert_eq!(off.verdict, Verdict::Fail);
        assert_eq!(
            off.reason,
            "not on USB now: no sound card device_micro2, off USB 2 times since boot"
        );
        let mut room = Heard::default();
        for sequence in 0..50 {
            room.add(&block(sequence, 300)).unwrap();
        }
        let back = Usb {
            present: true,
            ..gone.clone()
        };
        let heard = voice_mic(true, &Ok(room.clone()), Some(&back));
        assert_eq!(heard.verdict, Verdict::Pass);
        assert_eq!(heard.reason, "off USB 2 times since boot");
        let value = |result: &PartResult, key: &str| {
            result
                .values
                .iter()
                .find(|v| v.key == key)
                .map(|v| v.value.clone())
        };
        assert_eq!(value(&heard, "usb_drops").as_deref(), Some("2"));
        assert_eq!(value(&heard, "usb_port").as_deref(), Some("3-1.1.1"));
        assert_eq!(value(&heard, "on_usb").as_deref(), Some("true"));
        let steady = voice_mic(
            true,
            &Ok(room),
            Some(&Usb {
                drops: Some(0),
                ..back
            }),
        );
        assert_eq!(
            (steady.verdict, steady.reason.as_str()),
            (Verdict::Pass, "")
        );
        // On USB, the voice not giving it: still not checked, as before.
        let quiet = voice_mic(
            true,
            &Ok(Heard::default()),
            Some(&Usb {
                present: true,
                port: None,
                drops: Some(0),
            }),
        );
        assert_eq!(quiet.verdict, Verdict::NotChecked);
    }
}
