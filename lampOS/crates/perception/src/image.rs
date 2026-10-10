//! Bounded JPEG decoding, exposure measurement and detector input.
use serde::{Deserialize, Serialize};
use std::fmt;
use zune_jpeg::{
    JpegDecoder,
    zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions},
};

/// The camera contract's largest mode; larger images are refused before decoding.
pub const MAX_WIDTH: u32 = 1280;
pub const MAX_HEIGHT: u32 = 720;
/// YuNet's input stride multiple.
pub const INPUT_MULTIPLE: u32 = 32;

#[derive(Debug)]
pub enum ImageError {
    Decode(String),
    TooLarge { width: u32, height: u32 },
    Empty,
    UnexpectedLayout,
}
impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(f, "JPEG decode: {error}"),
            Self::TooLarge { width, height } => {
                write!(f, "image {width}x{height} exceeds {MAX_WIDTH}x{MAX_HEIGHT}")
            }
            Self::Empty => f.write_str("empty image"),
            Self::UnexpectedLayout => f.write_str("decoder returned an unexpected pixel layout"),
        }
    }
}
impl std::error::Error for ImageError {}

/// Packed 8-bit RGB.
#[derive(Clone, Debug)]
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// JPEG Annex K.3 Huffman tables, as one DHT segment. Motion-JPEG from USB
/// cameras commonly omits DHT and relies on these (the AVI1 convention).
const STANDARD_DHT: [u8; 420] = standard_dht();

const fn standard_dht() -> [u8; 420] {
    const DC_LUMA_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
    const DC_CHROMA_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
    const DC_VALUES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const AC_LUMA_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
    const AC_LUMA_VALUES: [u8; 162] = [
        0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61,
        0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52,
        0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25,
        0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45,
        0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64,
        0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83,
        0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
        0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6,
        0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3,
        0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8,
        0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ];
    const AC_CHROMA_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
    const AC_CHROMA_VALUES: [u8; 162] = [
        0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61,
        0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33,
        0x52, 0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18,
        0x19, 0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44,
        0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63,
        0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a,
        0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
        0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4,
        0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca,
        0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7,
        0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ];
    let mut out = [0_u8; 420];
    // Marker, then length 418 (everything after the marker).
    out[0] = 0xff;
    out[1] = 0xc4;
    out[2] = (418 >> 8) as u8;
    out[3] = (418 & 0xff) as u8;
    let mut at = 4;
    let tables: [(u8, &[u8; 16], &[u8]); 4] = [
        (0x00, &DC_LUMA_BITS, &DC_VALUES),
        (0x10, &AC_LUMA_BITS, &AC_LUMA_VALUES),
        (0x01, &DC_CHROMA_BITS, &DC_VALUES),
        (0x11, &AC_CHROMA_BITS, &AC_CHROMA_VALUES),
    ];
    let mut t = 0;
    while t < tables.len() {
        let (class, bits, values) = tables[t];
        out[at] = class;
        at += 1;
        let mut i = 0;
        while i < 16 {
            out[at] = bits[i];
            at += 1;
            i += 1;
        }
        let mut v = 0;
        while v < values.len() {
            out[at] = values[v];
            at += 1;
            v += 1;
        }
        t += 1;
    }
    out
}

/// Insert the standard Huffman tables before the first scan when a JPEG has
/// none. Other inputs are returned unchanged. Header markers are walked with
/// their declared lengths; a malformed header is an error, not repaired.
pub fn with_standard_huffman_tables(
    bytes: &[u8],
) -> Result<std::borrow::Cow<'_, [u8]>, ImageError> {
    let malformed = || ImageError::Decode("malformed JPEG header".into());
    if bytes.len() < 4 || bytes[..2] != [0xff, 0xd8] {
        return Err(malformed());
    }
    let mut at = 2;
    loop {
        if at + 4 > bytes.len() || bytes[at] != 0xff {
            return Err(malformed());
        }
        let marker = bytes[at + 1];
        match marker {
            0xc4 => return Ok(std::borrow::Cow::Borrowed(bytes)),
            0xda => {
                let mut fixed = Vec::with_capacity(bytes.len() + STANDARD_DHT.len());
                fixed.extend_from_slice(&bytes[..at]);
                fixed.extend_from_slice(&STANDARD_DHT);
                fixed.extend_from_slice(&bytes[at..]);
                return Ok(std::borrow::Cow::Owned(fixed));
            }
            // Fill bytes before a marker.
            0xff => at += 1,
            _ => {
                let length = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
                if length < 2 {
                    return Err(malformed());
                }
                at += 2 + length;
            }
        }
    }
}

/// Decode a baseline or progressive JPEG (including UVC MJPEG without
/// Huffman tables) to RGB in strict mode: data ending inside the scan is an
/// error, not a partly grey image. The end-of-image marker is required.
/// Dimensions above the camera contract are refused before any pixel buffer
/// is allocated.
pub fn decode_jpeg(bytes: &[u8]) -> Result<Rgb, ImageError> {
    if bytes.len() < 4 || bytes[bytes.len() - 2..] != [0xff, 0xd9] {
        return Err(ImageError::Decode("missing end-of-image marker".into()));
    }
    let bytes = with_standard_huffman_tables(bytes)?;
    let options = DecoderOptions::default()
        .set_strict_mode(true)
        .set_max_width(MAX_WIDTH as usize)
        .set_max_height(MAX_HEIGHT as usize)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(&bytes[..]), options);
    decoder
        .decode_headers()
        .map_err(|e| ImageError::Decode(format!("{e:?}")))?;
    let info = decoder.info().ok_or(ImageError::UnexpectedLayout)?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if width == 0 || height == 0 {
        return Err(ImageError::Empty);
    }
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(ImageError::TooLarge { width, height });
    }
    let size = decoder
        .output_buffer_size()
        .ok_or(ImageError::UnexpectedLayout)?;
    if size != width as usize * height as usize * 3 {
        return Err(ImageError::UnexpectedLayout);
    }
    let mut pixels = vec![0; size];
    decoder
        .decode_into(&mut pixels)
        .map_err(|e| ImageError::Decode(format!("{e:?}")))?;
    Ok(Rgb {
        width,
        height,
        pixels,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureVerdict {
    Usable,
    TooDark,
    Overexposed,
}

/// Whole-frame brightness summary. A dark or washed-out frame makes visual
/// facts unknown; it is never evidence that nobody is there.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exposure {
    pub mean_luma: u8,
    /// Sampled pixels with luma below `DARK_LUMA`, per mille.
    pub dark_per_mille: u16,
    /// Sampled pixels with luma above `BRIGHT_LUMA`, per mille.
    pub bright_per_mille: u16,
    pub verdict: ExposureVerdict,
}

pub const DARK_LUMA: u8 = 24;
pub const BRIGHT_LUMA: u8 = 245;
/// Unqualified starting thresholds; adjust only from recorded desk frames.
pub const TOO_DARK_MEAN: u8 = 32;
pub const TOO_DARK_FRACTION: u16 = 850;
pub const OVEREXPOSED_FRACTION: u16 = 850;

/// BT.601 luma of every second pixel in both directions.
pub fn measure_exposure(image: &Rgb) -> Exposure {
    let (mut sum, mut count, mut dark, mut bright) = (0_u64, 0_u64, 0_u64, 0_u64);
    let width = image.width as usize;
    for y in (0..image.height as usize).step_by(2) {
        let row = &image.pixels[y * width * 3..(y + 1) * width * 3];
        for pixel in row.chunks_exact(3).step_by(2) {
            let luma =
                (77 * u32::from(pixel[0]) + 150 * u32::from(pixel[1]) + 29 * u32::from(pixel[2]))
                    >> 8;
            sum += u64::from(luma);
            count += 1;
            dark += u64::from(luma < u32::from(DARK_LUMA));
            bright += u64::from(luma > u32::from(BRIGHT_LUMA));
        }
    }
    let count = count.max(1);
    let mean_luma = (sum / count) as u8;
    let dark_per_mille = (dark * 1000 / count) as u16;
    let bright_per_mille = (bright * 1000 / count) as u16;
    let verdict = if mean_luma < TOO_DARK_MEAN && dark_per_mille >= TOO_DARK_FRACTION {
        ExposureVerdict::TooDark
    } else if bright_per_mille >= OVEREXPOSED_FRACTION {
        ExposureVerdict::Overexposed
    } else {
        ExposureVerdict::Usable
    };
    Exposure {
        mean_luma,
        dark_per_mille,
        bright_per_mille,
        verdict,
    }
}

/// Detector input: planar BGR float 0..255, `width` x padded `height`, with
/// the image area-averaged into the top-left `content` region and zeros below.
#[derive(Clone, Debug)]
pub struct DetectorInput {
    pub width: u32,
    pub height: u32,
    pub content_width: u32,
    pub content_height: u32,
    /// Input pixels per source pixel.
    pub scale: f32,
    pub data: Vec<f32>,
}

/// The padded input size for a source size: `width` wide, aspect kept, the
/// height rounded up to a multiple of 32.
pub fn input_size(source_width: u32, source_height: u32, width: u32) -> (u32, u32, u32) {
    let content_height = ((u64::from(source_height) * u64::from(width)
        + u64::from(source_width) / 2)
        / u64::from(source_width))
    .max(1) as u32;
    let height = content_height.div_ceil(INPUT_MULTIPLE) * INPUT_MULTIPLE;
    (width, height, content_height)
}

/// The detector width for a source: the requested width, never wider than
/// the source, rounded down to a multiple of 32 (at least 32).
pub fn detector_width(source_width: u32, requested: u32) -> u32 {
    (requested.min(source_width) / INPUT_MULTIPLE * INPUT_MULTIPLE).max(INPUT_MULTIPLE)
}

/// Area-average `image` down to the detector width and lay it out as YuNet
/// expects: content in the top-left, zero padding below.
pub fn detector_input(image: &Rgb, width: u32) -> DetectorInput {
    let width = detector_width(image.width, width);
    let (width, height, content_height) = input_size(image.width, image.height, width);
    let content_width = width;
    let plane = (width * height) as usize;
    let mut data = vec![0.0_f32; plane * 3];
    let spans = |out: u32, source: u32| -> Vec<(usize, usize)> {
        (0..out)
            .map(|o| {
                let start = (u64::from(o) * u64::from(source) / u64::from(out)) as usize;
                let end = ((u64::from(o + 1) * u64::from(source) / u64::from(out)) as usize)
                    .max(start + 1);
                (start, end)
            })
            .collect()
    };
    let columns = spans(content_width, image.width);
    let rows = spans(content_height, image.height);
    let stride = image.width as usize * 3;
    for (oy, &(y0, y1)) in rows.iter().enumerate() {
        for (ox, &(x0, x1)) in columns.iter().enumerate() {
            let (mut r, mut g, mut b) = (0_u32, 0_u32, 0_u32);
            for y in y0..y1 {
                let row = &image.pixels[y * stride + x0 * 3..y * stride + x1 * 3];
                for pixel in row.chunks_exact(3) {
                    r += u32::from(pixel[0]);
                    g += u32::from(pixel[1]);
                    b += u32::from(pixel[2]);
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as f32;
            let index = oy * width as usize + ox;
            data[index] = b as f32 / count;
            data[plane + index] = g as f32 / count;
            data[2 * plane + index] = r as f32 / count;
        }
    }
    DetectorInput {
        width,
        height,
        content_width,
        content_height,
        scale: content_width as f32 / image.width as f32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> Rgb {
        Rgb {
            width,
            height,
            pixels: rgb
                .iter()
                .copied()
                .cycle()
                .take((width * height * 3) as usize)
                .collect(),
        }
    }

    #[test]
    fn camera_frames_map_to_a_32_multiple_input() {
        assert_eq!(input_size(1280, 720, 320), (320, 192, 180));
        assert_eq!(input_size(640, 480, 320), (320, 256, 240));
        assert_eq!(input_size(320, 320, 320), (320, 320, 320));
    }

    #[test]
    fn area_average_keeps_channel_order_and_pads_with_zeros() {
        let input = detector_input(&solid(1280, 720, [200, 100, 50]), 320);
        assert_eq!(
            (input.width, input.height, input.content_height),
            (320, 192, 180)
        );
        let plane = (320 * 192) as usize;
        // Planar BGR: blue first.
        assert_eq!(input.data[0], 50.0);
        assert_eq!(input.data[plane], 100.0);
        assert_eq!(input.data[2 * plane], 200.0);
        // Rows 180.. are padding.
        assert_eq!(input.data[180 * 320], 0.0);
        assert!((input.scale - 0.25).abs() < 1e-6);
    }

    #[test]
    fn small_images_are_not_upsampled_and_widths_stay_multiples_of_32() {
        let input = detector_input(&solid(160, 120, [10, 10, 10]), 320);
        assert_eq!((input.width, input.content_width), (160, 160));
        assert!((input.scale - 1.0).abs() < 1e-6);
        let input = detector_input(&solid(100, 75, [10, 10, 10]), 320);
        assert_eq!(
            (input.width, input.height, input.content_height),
            (96, 96, 72)
        );
        assert_eq!(detector_width(1280, 320), 320);
        assert_eq!(detector_width(1280, 330), 320);
    }

    #[test]
    fn exposure_separates_dark_bright_and_usable_frames() {
        assert_eq!(
            measure_exposure(&solid(64, 48, [5, 5, 5])).verdict,
            ExposureVerdict::TooDark
        );
        assert_eq!(
            measure_exposure(&solid(64, 48, [255, 255, 255])).verdict,
            ExposureVerdict::Overexposed
        );
        let usable = measure_exposure(&solid(64, 48, [120, 110, 100]));
        assert_eq!(usable.verdict, ExposureVerdict::Usable);
        assert_eq!(usable.mean_luma, 111);
        assert_eq!((usable.dark_per_mille, usable.bright_per_mille), (0, 0));
    }
}
