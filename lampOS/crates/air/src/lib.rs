//! The SEN63C's words, checked and read: particles, humidity, temperature
//! and carbon dioxide. No I/O here. See `docs/air.md`.
use lamp_msgs::{AirFeel, AirState};

pub mod clock;
pub mod watch;

/// The sensor's I2C address.
pub const ADDRESS: u16 = 0x6B;
/// Start continuous measurement.
pub const START: u16 = 0x0021;
/// Is a new reading ready: one word, its low byte 1 when it is.
pub const DATA_READY: u16 = 0x0202;
/// Read the measured values: seven words (SEN63C).
pub const READ_VALUES: u16 = 0x0471;
/// Product name: up to 16 words of text.
pub const PRODUCT: u16 = 0xD014;

/// Sensirion's CRC-8: polynomial 0x31, start 0xFF, over each two-byte word.
pub fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0xFFu8;
    for &byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// The words of a reply: each two bytes and a CRC. A wrong CRC or a short
/// reply is an error, never a value.
pub fn words(reply: &[u8]) -> Result<Vec<u16>, String> {
    if reply.is_empty() || !reply.len().is_multiple_of(3) {
        return Err(format!(
            "a reply of {} bytes is not whole words",
            reply.len()
        ));
    }
    reply
        .chunks_exact(3)
        .enumerate()
        .map(|(i, word)| {
            if crc8(&word[..2]) == word[2] {
                Ok(u16::from_be_bytes([word[0], word[1]]))
            } else {
                Err(format!("word {i} fails its CRC"))
            }
        })
        .collect()
}

/// The command as the two bytes sent.
pub fn command(code: u16) -> [u8; 2] {
    code.to_be_bytes()
}

/// The seven measured words as a reading. 0xFFFF (unsigned) and 0x7FFF
/// (signed) are the sensor's "not known yet".
pub fn reading(words: &[u16]) -> Result<AirState, String> {
    let [pm1, pm25, pm4, pm10, rh, t, co2] = words else {
        return Err(format!("{} words where seven are read", words.len()));
    };
    let unsigned = |w: u16, scale: f32| (w != 0xFFFF).then(|| f32::from(w) / scale);
    let signed = |w: u16, scale: f32| {
        let v = w as i16;
        (v != 0x7FFF).then(|| f32::from(v) / scale)
    };
    let co2_ppm = (*co2 != 0xFFFF).then_some(*co2);
    Ok(AirState {
        pm1_0: unsigned(*pm1, 10.0),
        pm2_5: unsigned(*pm25, 10.0),
        pm4_0: unsigned(*pm4, 10.0),
        pm10: unsigned(*pm10, 10.0),
        humidity: signed(*rh, 100.0),
        temperature: signed(*t, 200.0),
        co2_ppm,
        feel: AirFeel::of_co2(co2_ppm),
    })
}

/// The product name in a reply's words: text up to its first zero byte.
pub fn text(words: &[u16]) -> String {
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(words: &[u16]) -> Vec<u8> {
        words
            .iter()
            .flat_map(|w| {
                let b = w.to_be_bytes();
                [b[0], b[1], crc8(&b)]
            })
            .collect()
    }

    #[test]
    fn the_crc_is_sensirions() {
        // The datasheet's example: 0xBEEF gives 0x92.
        assert_eq!(crc8(&[0xBE, 0xEF]), 0x92);
    }

    #[test]
    fn a_reading_from_lamp_4ace_reads_as_it_did_there() {
        // The words read on lamp-4ace on 2026-10-14 at 17:36.
        let measured = [888, 925, 925, 925, 3170, 6600, 1931];
        let air = reading(&words(&reply(&measured)).unwrap()).unwrap();
        assert_eq!(air.pm2_5, Some(92.5));
        assert_eq!(air.humidity, Some(31.7));
        assert_eq!(air.temperature, Some(33.0));
        assert_eq!(air.co2_ppm, Some(1_931));
        assert_eq!(air.feel, AirFeel::Stuffy);
        assert_eq!(air.check(), Ok(()));
    }

    #[test]
    fn not_known_yet_is_absent_and_a_bad_crc_is_no_value() {
        let unknown = [0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x7FFF, 0x7FFF, 0xFFFF];
        let air = reading(&words(&reply(&unknown)).unwrap()).unwrap();
        assert_eq!(
            (air.pm2_5, air.temperature, air.co2_ppm),
            (None, None, None)
        );
        assert_eq!(air.feel, AirFeel::Unknown);
        let mut broken = reply(&[888, 925]);
        broken[2] ^= 1;
        assert!(words(&broken).unwrap_err().contains("CRC"));
        assert!(words(&[1, 2]).is_err());
        assert!(reading(&[1, 2, 3]).is_err());
    }

    #[test]
    fn a_cold_reading_keeps_its_sign_and_the_name_is_text() {
        let cold = [10, 20, 30, 40, 5_000, (-1_000i16) as u16, 420];
        let air = reading(&cold).unwrap();
        assert_eq!(air.temperature, Some(-5.0));
        assert_eq!(air.feel, AirFeel::Fresh);
        let name = [
            u16::from_be_bytes(*b"SE"),
            u16::from_be_bytes(*b"N6"),
            u16::from_be_bytes(*b"3C"),
            0,
        ];
        assert_eq!(text(&name), "SEN63C");
        assert_eq!(command(READ_VALUES), [0x04, 0x71]);
    }
}
