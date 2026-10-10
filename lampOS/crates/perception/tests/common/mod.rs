//! Synthetic scenes for perception tests. No photograph of a person is used:
//! the "face" is a drawing (skin oval, hair, eyes, brows, nose, mouth) that the
//! pinned detector happens to recognise. It exercises decoding, detection and
//! coordinate mapping; it says nothing about accuracy on real people.
#![allow(dead_code)]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Private(pub PathBuf);
impl Private {
    pub fn new(label: &str) -> Self {
        let path = PathBuf::from(format!(
            "/tmp/lpt-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A flat background with an optional drawn face centred at (cx, cy) with
/// half-width `rx` (the oval is 4/3 as tall).
pub fn scene(
    width: u32,
    height: u32,
    background: [u8; 3],
    face: Option<(f32, f32, f32)>,
) -> Vec<u8> {
    let mut pixels = vec![0_u8; (width * height * 3) as usize];
    for y in 0..height {
        for x in 0..width {
            let (fx, fy) = (x as f32, y as f32);
            let mut c = background;
            if let Some((cx, cy, rx)) = face {
                let k = rx / 150.0;
                let ry = rx * 4.0 / 3.0;
                let d = ((fx - cx) / rx).powi(2) + ((fy - cy) / ry).powi(2);
                if d < 1.0 {
                    c = [224, 172, 140];
                }
                if d < 1.05 && fy < cy - 120.0 * k {
                    c = [50, 35, 25];
                }
                for ex in [cx - 55.0 * k, cx + 55.0 * k] {
                    let ey = cy - 40.0 * k;
                    if ((fx - ex) / (22.0 * k)).powi(2) + ((fy - ey) / (11.0 * k)).powi(2) < 1.0 {
                        c = [245, 245, 245];
                    }
                    if ((fx - ex) / (9.0 * k)).powi(2) + ((fy - ey) / (9.0 * k)).powi(2) < 1.0 {
                        c = [30, 20, 15];
                    }
                    if ((fx - ex) / (30.0 * k)).powi(2)
                        + ((fy - (cy - 75.0 * k)) / (5.0 * k)).powi(2)
                        < 1.0
                    {
                        c = [60, 40, 30];
                    }
                }
                if (fx - cx).abs() < 8.0 * k && fy > cy - 30.0 * k && fy < cy + 40.0 * k {
                    c = [195, 140, 112];
                }
                if ((fx - cx) / (45.0 * k)).powi(2) + ((fy - (cy + 85.0 * k)) / (10.0 * k)).powi(2)
                    < 1.0
                {
                    c = [160, 60, 60];
                }
            }
            let i = ((y * width + x) * 3) as usize;
            pixels[i..i + 3].copy_from_slice(&c);
        }
    }
    pixels
}

pub fn jpeg(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 85)
        .encode(
            pixels,
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .unwrap();
    out
}

pub fn face_jpeg(cx: f32, cy: f32, rx: f32) -> Vec<u8> {
    jpeg(
        &scene(1280, 720, [110, 120, 130], Some((cx, cy, rx))),
        1280,
        720,
    )
}
