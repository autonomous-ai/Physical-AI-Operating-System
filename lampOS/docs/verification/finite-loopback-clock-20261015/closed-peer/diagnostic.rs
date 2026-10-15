use std::{io::{Read, Write}, os::unix::net::UnixStream, time::Duration};
fn main() {
    for close_peer in [false, true] {
        let (mut parent, mut child) = UnixStream::pair().unwrap();
        child.write_all(b"queued\n").unwrap();
        let child = if close_peer { drop(child); None } else { Some(child) };
        println!("closed={close_peer} read_timeout={:?}", parent.set_read_timeout(Some(Duration::from_secs(2))));
        println!("closed={close_peer} write_timeout={:?}", parent.set_write_timeout(Some(Duration::from_secs(2))));
        let mut bytes=[0;7];
        println!("closed={close_peer} read={:?}", parent.read_exact(&mut bytes).map(|()|bytes));
        drop(child);
    }
}
