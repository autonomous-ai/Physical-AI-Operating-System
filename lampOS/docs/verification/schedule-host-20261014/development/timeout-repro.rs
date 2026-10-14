use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

fn outcome(label: &str, value: io::Result<()>) -> bool {
    match value {
        Ok(()) => {
            println!("{label}: ok");
            true
        }
        Err(error) => {
            println!("{label}: kind={:?} raw={:?} error={error}", error.kind(), error.raw_os_error());
            false
        }
    }
}

fn main() -> io::Result<()> {
    println!("platform={}/{}", std::env::consts::OS, std::env::consts::ARCH);
    let durations = [
        Duration::from_secs(2),
        Duration::new(1, 999_999_999),
        Duration::new(1, 999_123_456),
        Duration::from_secs(1),
        Duration::new(0, 999_999_999),
        Duration::from_millis(50),
        Duration::from_nanos(1),
    ];
    for closed in [false, true] {
        for duration in durations {
            let (mut receiver, mut peer) = UnixStream::pair()?;
            peer.write_all(&[0x61; 4096])?;
            let mut peer = Some(peer);
            if closed {
                drop(peer.take());
            }
            let label = format!("closed={closed} timeout_ns={} before_read", duration.as_nanos());
            outcome(&label, receiver.set_read_timeout(Some(duration)));
            let mut total = 0;
            for chunk in 0..4 {
                let mut bytes = [0; 1024];
                let count = receiver.read(&mut bytes)?;
                total += count;
                println!("closed={closed} timeout_ns={} chunk={chunk} read={count} total={total}", duration.as_nanos());
                outcome(
                    &format!("closed={closed} timeout_ns={} after_chunk={chunk}", duration.as_nanos()),
                    receiver.set_read_timeout(Some(duration)),
                );
            }
            assert_eq!(total, 4096);
            drop(peer);
        }
    }
    Ok(())
}
