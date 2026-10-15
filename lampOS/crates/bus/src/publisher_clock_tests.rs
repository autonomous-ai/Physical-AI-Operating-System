//! Cadence boundaries still perform real filesystem work and socket delivery.
use super::*;
use crate::{Kind, Subscriber};
use std::sync::{Arc, Mutex, atomic::AtomicUsize};

struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = PathBuf::from(format!(
            "/tmp/bpc-{}-{}",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }

    fn bus(&self) -> Bus {
        Bus::new(self.0.join("b")).unwrap()
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Clock {
    at: Arc<Mutex<Instant>>,
    reads: Arc<AtomicUsize>,
}

impl Clock {
    fn new(at: Instant) -> Self {
        Self {
            at: Arc::new(Mutex::new(at)),
            reads: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn reader(&self) -> impl Fn() -> Instant + Send + Sync + 'static + use<> {
        let clock = self.clone();
        move || {
            clock.reads.fetch_add(1, Ordering::Relaxed);
            *clock.at.lock().unwrap()
        }
    }

    fn set(&self, at: Instant) {
        *self.at.lock().unwrap() = at;
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }
}

fn take(subscriber: &mut Subscriber, seq: u64, value: u64) -> crate::Message {
    // A successful local datagram send has already queued these bytes.
    let message = subscriber.recv(Duration::ZERO).unwrap().unwrap();
    assert_eq!(message.header.seq, seq);
    assert_eq!(message.json().unwrap(), serde_json::json!(value));
    message
}

#[test]
fn undeclared_construction_and_invalid_names_or_declarations_do_not_read_the_clock() {
    let root = Root::new();
    let bus = root.bus();
    let clock = Clock::new(Instant::now());
    for (topic, source) in [("bad.topic", "source"), ("cadence", "BadSource")] {
        assert_eq!(
            bus.publisher_with_cadence_clock(topic, Kind::Event, source, clock.reader())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(clock.reads(), 0);
    }
    for declaration in [
        Declaration::new("bad_type", 1, Kind::Event),
        Declaration::new("fixture/Cadence", 1, Kind::Event).every(Duration::ZERO),
    ] {
        assert_eq!(
            bus.publisher_declared_with_cadence_clock(
                "cadence",
                &declaration,
                "source",
                clock.reader(),
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(clock.reads(), 0);
    }
    let publisher = bus
        .publisher_with_cadence_clock("cadence", Kind::Event, "source", clock.reader())
        .unwrap();
    assert_eq!(clock.reads(), 0);
    assert!(publisher.scanned.is_none());
    assert!(publisher.shown.is_none());
    assert!(!bus.root().exists());
}

#[test]
fn a_real_declaration_write_failure_precedes_the_first_cadence_sample() {
    let root = Root::new();
    let bus = root.bus();
    fs::write(bus.root(), b"blocked").unwrap();
    let clock = Clock::new(Instant::now());
    assert!(
        bus.publisher_declared_with_cadence_clock(
            "cadence",
            &Declaration::new("fixture/Cadence", 1, Kind::Event),
            "source",
            clock.reader(),
        )
        .is_err()
    );
    assert_eq!(clock.reads(), 0);
    assert_eq!(fs::read(bus.root()).unwrap(), b"blocked");
}

#[test]
fn declaration_and_first_delivery_take_separate_samples_and_send_the_real_header() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("cadence", "receiver").unwrap();
    let start = Instant::now();
    let clock = Clock::new(start);
    let declaration = Declaration::new("fixture/Cadence", 1, Kind::Event);
    let mut publisher = bus
        .publisher_declared_with_cadence_clock("cadence", &declaration, "source", clock.reader())
        .unwrap();
    assert_eq!(clock.reads(), 1);
    assert_eq!(publisher.shown, Some(start));
    assert!(publisher.scanned.is_none());
    assert!(publisher.entry.is_file());
    assert_eq!(bus.declaration("cadence").unwrap(), Some(declaration));

    let delivered = start + Duration::from_millis(17);
    clock.set(delivered);
    assert_eq!(publisher.publish_json(912_345, &7).unwrap().delivered, 1);
    assert_eq!(clock.reads(), 2);
    assert_eq!(publisher.scanned, Some(delivered));
    assert_eq!(publisher.shown, Some(start));
    let message = take(&mut subscriber, 1, 7);
    assert_eq!(message.header.at_us, 912_345);
    assert!(message.header.at_unix_ms > 0);
    assert_eq!(message.header.type_name.as_deref(), Some("fixture/Cadence"));
    assert_eq!(message.header.kind, Kind::Event);
    assert_eq!(message.header.source, "source");
    assert_eq!(message.header.boot, publisher.boot());
}

#[test]
fn invalid_messages_do_not_read_cadence_or_advance_accepted_counts() {
    let root = Root::new();
    let bus = root.bus();
    let clock = Clock::new(Instant::now());
    let mut publisher = bus
        .publisher_with_cadence_clock("cadence", Kind::Event, "source", clock.reader())
        .unwrap();
    assert!(publisher.publish_bytes(1, wire::NONE, b"payload").is_err());
    assert!(
        publisher
            .publish_bytes(1, "bad encoding", b"payload")
            .is_err()
    );
    assert!(
        publisher
            .publish_bytes(1, "bytes", &vec![0; wire::MOST_BYTES])
            .is_err()
    );
    assert!(
        publisher
            .publish(1, wire::JSON, |_| Err(io::Error::other("payload failed")))
            .is_err()
    );
    assert_eq!(clock.reads(), 0);
    assert_eq!(publisher.totals(), Totals::default());
    assert_eq!(publisher.seq, 0);
    assert!(publisher.scanned.is_none());
    assert!(publisher.shown.is_none());
    assert!(!bus.root().exists());
    assert_eq!(publisher.publish_json(2, &2).unwrap(), Published::default());
    assert_eq!(clock.reads(), 1);
    assert_eq!(publisher.totals().published, 1);
    assert_eq!(publisher.seq, 1);
}

#[test]
fn frozen_reads_hold_scans_and_presence_until_the_exact_default_boundaries() {
    let root = Root::new();
    let bus = root.bus();
    let mut first = bus.subscribe("cadence", "first").unwrap();
    let start = Instant::now();
    let clock = Clock::new(start);
    let mut publisher = bus
        .publisher_with_cadence_clock("cadence", Kind::Event, "source", clock.reader())
        .unwrap();
    assert_eq!(publisher.publish_json(1, &1).unwrap().delivered, 1);
    take(&mut first, 1, 1);
    assert_eq!(publisher.scanned, Some(start));
    assert_eq!(publisher.shown, Some(start));
    let mut second = bus.subscribe("cadence", "second").unwrap();
    fs::remove_file(&publisher.entry).unwrap();
    for value in 2..=3 {
        publisher.alive().unwrap();
        assert_eq!(publisher.publish_json(value, &value).unwrap().delivered, 1);
        take(&mut first, value, value);
        assert!(second.recv(Duration::ZERO).unwrap().is_none());
        assert!(!publisher.entry.exists());
        assert_eq!(publisher.scanned, Some(start));
        assert_eq!(publisher.shown, Some(start));
    }
    clock.set(start + Duration::from_micros(199_999));
    assert_eq!(publisher.publish_json(4, &4).unwrap().delivered, 1);
    take(&mut first, 4, 4);
    assert!(second.recv(Duration::ZERO).unwrap().is_none());
    assert_eq!(publisher.scanned, Some(start));

    let scan_due = start + Duration::from_millis(200);
    clock.set(scan_due);
    assert_eq!(publisher.publish_json(5, &5).unwrap().delivered, 2);
    take(&mut first, 5, 5);
    take(&mut second, 5, 5);
    assert_eq!(publisher.scanned, Some(scan_due));
    assert_eq!(publisher.shown, Some(start));
    assert!(!publisher.entry.exists());

    clock.set(start + Duration::from_micros(999_999));
    publisher.alive().unwrap();
    assert!(!publisher.entry.exists());
    let show_due = start + Duration::from_secs(1);
    clock.set(show_due);
    publisher.alive().unwrap();
    assert!(publisher.entry.is_file());
    assert_eq!(publisher.shown, Some(show_due));
    assert_eq!(publisher.scanned, Some(scan_due));
    assert_eq!(clock.reads(), 9);
}

#[test]
fn a_root_obstruction_is_discovered_at_the_real_rescan_boundary_and_recovers() {
    let root = Root::new();
    let bus = root.bus();
    let mut subscriber = bus.subscribe("cadence", "first").unwrap();
    let start = Instant::now();
    let clock = Clock::new(start);
    let mut publisher = bus
        .publisher_with_cadence_clock("cadence", Kind::Event, "source", clock.reader())
        .unwrap();
    assert_eq!(publisher.publish_json(1, &1).unwrap().delivered, 1);
    take(&mut subscriber, 1, 1);
    drop(subscriber);
    let saved = root.0.join("saved");
    fs::rename(bus.root(), &saved).unwrap();
    fs::write(bus.root(), b"blocked").unwrap();

    clock.set(start + Duration::from_micros(199_999));
    assert_eq!(
        publisher.publish_json(2, &2).unwrap(),
        Published {
            delivered: 0,
            dropped: 1,
            removed: 0
        }
    );
    assert_eq!(publisher.scanned, Some(start));
    assert!(!publisher.failing);
    assert_eq!(fs::read(bus.root()).unwrap(), b"blocked");

    clock.set(start + Duration::from_millis(200));
    assert!(publisher.publish_json(3, &3).is_err());
    assert_eq!(publisher.scanned, Some(start + Duration::from_millis(200)));
    assert!(publisher.failing);
    clock.set(start + Duration::from_millis(400));
    assert_eq!(publisher.publish_json(4, &4).unwrap().dropped, 1);
    assert!(publisher.failing);
    assert_eq!(fs::read(bus.root()).unwrap(), b"blocked");

    fs::remove_file(bus.root()).unwrap();
    fs::rename(&saved, bus.root()).unwrap();
    let mut restored = bus.subscribe("cadence", "restored").unwrap();
    clock.set(start + Duration::from_millis(600));
    assert_eq!(publisher.publish_json(5, &5).unwrap().delivered, 1);
    take(&mut restored, 5, 5);
    assert!(!publisher.failing);
    assert_eq!(
        publisher.totals(),
        Totals {
            published: 5,
            delivered: 2,
            dropped: 3
        }
    );

    drop(restored);
    fs::rename(bus.root(), &saved).unwrap();
    fs::write(bus.root(), b"blocked again").unwrap();
    clock.set(start + Duration::from_millis(800));
    assert!(publisher.publish_json(6, &6).is_err());
    assert!(publisher.failing);
    assert_eq!(fs::read(bus.root()).unwrap(), b"blocked again");
}

#[test]
fn a_presence_obstruction_waits_until_one_second_and_unlisted_recovery_keeps_its_time() {
    let root = Root::new();
    let bus = root.bus();
    let start = Instant::now();
    let clock = Clock::new(start);
    let mut publisher = bus
        .publisher_declared_with_cadence_clock(
            "cadence",
            &Declaration::new("fixture/Cadence", 1, Kind::Event),
            "source",
            clock.reader(),
        )
        .unwrap();
    fs::remove_file(&publisher.entry).unwrap();
    fs::remove_dir(&publisher.pubs).unwrap();
    fs::write(&publisher.pubs, b"blocked").unwrap();
    clock.set(start + Duration::from_micros(999_999));
    publisher.alive().unwrap();
    assert_eq!(publisher.shown, Some(start));
    assert!(!publisher.unlisted);
    assert_eq!(fs::read(&publisher.pubs).unwrap(), b"blocked");

    let due = start + Duration::from_secs(1);
    clock.set(due);
    assert!(publisher.alive().is_err());
    assert_eq!(publisher.shown, Some(due));
    assert!(publisher.unlisted);
    assert_eq!(fs::read(&publisher.pubs).unwrap(), b"blocked");
    fs::remove_file(&publisher.pubs).unwrap();
    fs::create_dir(&publisher.pubs).unwrap();
    // The original unlisted path attempts a real refresh at this same instant.
    publisher.alive().unwrap();
    assert!(publisher.entry.is_file());
    assert!(!publisher.unlisted);
    assert_eq!(publisher.shown, Some(due));
    assert_eq!(clock.reads(), 4);
    assert_eq!(publisher.totals(), Totals::default());
}

#[test]
fn the_real_mtime_lease_remains_independent_of_a_frozen_publisher_clock() {
    let root = Root::new();
    let bus = root.bus();
    let start = Instant::now();
    let clock = Clock::new(start);
    let mut publisher = bus
        .publisher_declared_with_cadence_clock(
            "cadence",
            &Declaration::new("fixture/Cadence", 1, Kind::Event),
            "source",
            clock.reader(),
        )
        .unwrap();
    fs::File::open(&publisher.entry)
        .unwrap()
        .set_modified(UNIX_EPOCH)
        .unwrap();
    assert!(bus.publishers("cadence").unwrap().is_empty());
    assert!(!publisher.entry.exists());
    publisher.alive().unwrap();
    assert!(!publisher.entry.exists());
    clock.set(start + Duration::from_secs(1));
    publisher.alive().unwrap();
    assert!(fs::metadata(&publisher.entry).unwrap().modified().unwrap() > UNIX_EPOCH);
    assert_eq!(publisher.shown, Some(start + Duration::from_secs(1)));
}

#[test]
fn ordinary_constructors_keep_host_samples_and_publishers_remain_send_and_sync() {
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<Publisher>();
    let root = Root::new();
    let bus = root.bus();
    let before = Instant::now();
    let declared = bus
        .publisher_declared(
            "declared",
            &Declaration::new("fixture/Cadence", 1, Kind::Event),
            "source",
        )
        .unwrap();
    let after = Instant::now();
    assert!(declared.shown.is_some_and(|at| before <= at && at <= after));
    assert!(declared.scanned.is_none());
    let mut plain = bus.publisher("plain", Kind::Event, "source").unwrap();
    assert!(plain.scanned.is_none());
    assert!(plain.shown.is_none());
    let before = Instant::now();
    assert_eq!(plain.publish_json(1, &1).unwrap(), Published::default());
    let after = Instant::now();
    assert!(plain.scanned.is_some_and(|at| before <= at && at <= after));
    assert_eq!(plain.scanned, plain.shown);
}
