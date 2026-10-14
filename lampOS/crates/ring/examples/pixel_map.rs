//! Light a few pixels of the ring, dimly, so a photograph can show which
//! pixel is where: pixel 0 red, 1 orange and 2 yellow (the way they count),
//! 8 green, 16 blue, 24 white. A diagnostic for a person at the lamp, not a
//! lamp part: it writes the ring without the guard the voice's ring worker
//! uses, so it refuses unless it gets the ring's lock, and the voice (the
//! ring's owner) must be stopped first.
//!
//! `pixel_map SECONDS`: shows the map for that long (1 to 60), then black.

#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    use lamp_ring::{ChannelCeiling, PIXEL_COUNT, Rgb, SPI_SPEED_HZ, encode};
    use spidev::{SpiModeFlags, Spidev, SpidevOptions};
    use std::io::{self, Write};

    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .filter(|seconds| (1..=60).contains(seconds))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: pixel_map SECONDS (1 to 60)",
            )
        })?;
    let mut device = Spidev::open(lamp_ring::linux::DEVICE_PATH)?;
    device.inner().try_lock().map_err(|_| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            "the ring is in use: stop its owner first",
        )
    })?;
    device.configure(
        &SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(SPI_SPEED_HZ)
            .lsb_first(false)
            .mode(SpiModeFlags::SPI_MODE_0)
            .build(),
    )?;
    let ceiling = ChannelCeiling::new(40).map_err(io::Error::other)?;
    let mut pixels = [Rgb::BLACK; PIXEL_COUNT];
    pixels[0] = Rgb::new(40, 0, 0);
    pixels[1] = Rgb::new(40, 12, 0);
    pixels[2] = Rgb::new(30, 30, 0);
    pixels[8] = Rgb::new(0, 40, 0);
    pixels[16] = Rgb::new(0, 0, 40);
    pixels[24] = Rgb::new(25, 25, 25);
    // One write a frame, as the ring's own sink does: a split frame would be
    // read by the LEDs as two.
    let shown = device.write(encode(&pixels, ceiling).bytes())?;
    println!("pixel_map: wrote {shown} bytes; showing for {seconds} s");
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    let dark = device.write(encode(&[Rgb::BLACK; PIXEL_COUNT], ceiling).bytes())?;
    println!("pixel_map: wrote {dark} bytes of black");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("pixel_map: the ring is on Linux only");
    std::process::exit(2);
}
