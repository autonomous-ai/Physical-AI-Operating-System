//! The executive as the voice meets it: a directory, a list, a socket.
use lamp_actions::{
    Executive,
    light::{BusLight, Light, STATE},
    server::{CALL_SOCKET, CATALOG_FILE, READY_FILE, serve},
    sound::{Mixer, Volume},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

/// A volume control the test can read while the executive holds it.
struct Shared(Arc<AtomicU8>);
impl Mixer for Shared {
    fn get(&mut self) -> std::io::Result<u8> {
        Ok(self.0.load(Ordering::Acquire))
    }
    fn set(&mut self, level: u8) -> std::io::Result<()> {
        self.0.store(level, Ordering::Release);
        Ok(())
    }
}
/// No ring owner at all.
struct NoLight;
impl Light for NoLight {
    fn there(&mut self) -> bool {
        false
    }
    fn shown(&mut self) -> Option<[u8; 3]> {
        None
    }
    fn ask(&mut self, _: [u8; 3]) -> bool {
        false
    }
}

fn directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "lamp-actions-{name}-{}-{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(started.elapsed() < Duration::from_secs(5), "never: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn ask(home: &Path, line: &[u8]) -> Value {
    let mut stream = UnixStream::connect(home.join(CALL_SOCKET)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(line).unwrap();
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).unwrap();
    serde_json::from_str(&answer).unwrap()
}
fn names(home: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(home.join(CATALOG_FILE)).unwrap_or_default();
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|file| {
            Some(
                file["tools"]
                    .as_array()?
                    .iter()
                    .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
                    .collect(),
            )
        })
        .unwrap_or_default()
}

/// The whole of what the voice does with it: reads the list, asks on the
/// socket in one line, gets one line back; and what it is told is what
/// happened to the control.
#[test]
fn the_list_is_there_a_call_changes_the_control_and_anything_else_is_refused_in_words() {
    let home = directory("calls");
    let level = Arc::new(AtomicU8::new(50));
    let stop = Arc::new(AtomicBool::new(false));
    let serving = {
        let (home, level, stop) = (home.clone(), level.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut executive = Executive {
                sound: Some(Volume::new(Shared(level), 100)),
                light: None::<NoLight>,
            };
            serve(&home, &mut executive, &stop)
        })
    };
    until("ready", || home.join(READY_FILE).exists());
    assert_eq!(
        names(&home),
        ["volume_up", "volume_down", "volume_mute", "volume_unmute"]
    );
    assert_eq!(
        ask(
            &home,
            b"{\"op\":\"call\",\"name\":\"volume_up\",\"args\":{}}\n"
        ),
        json!({"ok":true,"result":{"result":"My volume is now 70 out of 100; it was 50."}})
    );
    assert_eq!(level.load(Ordering::Acquire), 70);
    assert_eq!(
        ask(&home, b"{\"op\":\"call\",\"name\":\"volume_down\"}\n")["result"]["result"],
        "My volume is now 50 out of 100; it was 70."
    );
    // Not offered, not a call, not JSON, an argument a function does not
    // take: each gets a sentence, and nothing is changed.
    for refused in [
        b"{\"op\":\"call\",\"name\":\"light_on\",\"args\":{}}\n".as_slice(),
        b"{\"op\":\"call\",\"name\":\"open_the_door\",\"args\":{}}\n".as_slice(),
        b"{\"op\":\"call\",\"name\":\"volume_up\",\"args\":{\"by\":50}}\n".as_slice(),
        b"{\"op\":\"status\"}\n".as_slice(),
        b"hello\n".as_slice(),
    ] {
        assert_eq!(
            ask(&home, refused),
            json!({"ok":false,"error":"I can't do that."}),
            "{}",
            String::from_utf8_lossy(refused)
        );
    }
    assert_eq!(level.load(Ordering::Acquire), 50);
    // A caller that connects and says nothing costs the next one a second,
    // not the service.
    let silent = UnixStream::connect(home.join(CALL_SOCKET)).unwrap();
    let started = Instant::now();
    assert_eq!(
        ask(&home, b"{\"op\":\"call\",\"name\":\"volume_mute\"}\n")["result"]["result"],
        "My speaker is muted."
    );
    assert!(started.elapsed() < Duration::from_millis(2_500));
    drop(silent);
    stop.store(true, Ordering::Release);
    serving.join().unwrap().unwrap();
    // Stopped in order, it leaves nothing the voice would read.
    assert!(!home.join(CATALOG_FILE).exists() && !home.join(READY_FILE).exists());
    std::fs::remove_dir_all(home).unwrap();
}

/// The light's functions are offered only while the ring's owner says what
/// it shows. Here it starts after the executive, shows what it is asked,
/// and then goes: the list follows it both ways.
#[test]
fn the_light_is_offered_when_its_owner_appears_and_withdrawn_when_it_goes() {
    let home = directory("light");
    let root = directory("light-bus");
    let stop = Arc::new(AtomicBool::new(false));
    let serving = {
        let (home, root, stop) = (home.clone(), root.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut executive = Executive {
                sound: None::<Volume<Shared>>,
                light: Some(BusLight::new(lamp_bus::Bus::new(&root).unwrap()).unwrap()),
            };
            serve(&home, &mut executive, &stop)
        })
    };
    until("ready", || home.join(READY_FILE).exists());
    assert!(names(&home).is_empty());
    assert_eq!(
        ask(&home, b"{\"op\":\"call\",\"name\":\"light_on\"}\n"),
        json!({"ok":false,"error":"I can't do that."})
    );

    // The ring's owner: says what it shows once a second, and shows what
    // it is asked for.
    let owner_stop = Arc::new(AtomicBool::new(false));
    let owner = {
        let (root, owner_stop) = (root.clone(), owner_stop.clone());
        std::thread::spawn(move || {
            let bus = lamp_bus::Bus::new(&root).unwrap();
            let mut state = bus
                .publisher(STATE, lamp_bus::Kind::Latest, "ring")
                .unwrap();
            state.expires_after(Some(Duration::from_millis(600)));
            let mut shown = [0u8, 0, 0];
            let mut tick = 0u64;
            while !owner_stop.load(Ordering::Acquire) {
                if let Ok(Some(wanted)) = bus.latest(lamp_actions::light::WANTED)
                    && let Ok(wanted) = wanted.json()
                    && let Ok(rgb) = serde_json::from_value::<[u8; 3]>(wanted["rgb"].clone())
                {
                    shown = rgb;
                }
                tick += 1;
                state.publish_json(tick, &json!({"rgb": shown})).unwrap();
                std::thread::sleep(Duration::from_millis(20));
            }
        })
    };
    until("the light is offered", || names(&home).len() == 5);
    assert_eq!(
        ask(&home, b"{\"op\":\"call\",\"name\":\"light_on\"}\n"),
        json!({"ok":true,"result":{"result":"My light is on, warm white."}})
    );
    assert_eq!(
        ask(
            &home,
            b"{\"op\":\"call\",\"name\":\"light_colour\",\"args\":{\"colour\":\"blue\"}}\n"
        )["result"]["result"],
        "My light is blue now."
    );
    // One word out of ten, and nothing else.
    for refused in [
        b"{\"op\":\"call\",\"name\":\"light_colour\",\"args\":{\"colour\":\"beige\"}}\n".as_slice(),
        b"{\"op\":\"call\",\"name\":\"light_colour\",\"args\":{\"colour\":\"blue\",\"rgb\":[1,2,3]}}\n".as_slice(),
        b"{\"op\":\"call\",\"name\":\"light_colour\"}\n".as_slice(),
    ] {
        assert_eq!(
            ask(&home, refused)["error"],
            "That is not a colour my light can show."
        );
    }
    assert_eq!(
        ask(&home, b"{\"op\":\"call\",\"name\":\"light_dimmer\"}\n")["result"]["result"],
        "My light is half as bright."
    );
    // The owner goes. What it last said goes stale, and the list empties.
    owner_stop.store(true, Ordering::Release);
    owner.join().unwrap();
    until("the light is withdrawn", || names(&home).is_empty());
    assert_eq!(
        ask(&home, b"{\"op\":\"call\",\"name\":\"light_off\"}\n"),
        json!({"ok":false,"error":"I can't do that."})
    );
    stop.store(true, Ordering::Release);
    serving.join().unwrap().unwrap();
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

/// A second executive on the same directory is refused while the first
/// answers, and takes over a socket nobody answers on.
#[test]
fn one_executive_to_a_directory() {
    let home = directory("one");
    let stop = Arc::new(AtomicBool::new(false));
    let serving = {
        let (home, stop) = (home.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut executive = Executive {
                sound: None::<Volume<Shared>>,
                light: None::<NoLight>,
            };
            serve(&home, &mut executive, &stop)
        })
    };
    until("ready", || home.join(READY_FILE).exists());
    let mut second = Executive {
        sound: None::<Volume<Shared>>,
        light: None::<NoLight>,
    };
    let refused = serve(&home, &mut second, &AtomicBool::new(true)).unwrap_err();
    assert_eq!(refused.kind(), std::io::ErrorKind::AddrInUse);
    stop.store(true, Ordering::Release);
    serving.join().unwrap().unwrap();
    // A socket file left by a process that died: taken over.
    drop(std::os::unix::net::UnixListener::bind(home.join(CALL_SOCKET)).unwrap());
    serve(&home, &mut second, &AtomicBool::new(true)).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}
