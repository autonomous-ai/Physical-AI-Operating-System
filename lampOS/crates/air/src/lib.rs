//! The SEN63C's words, checked and read: particles, humidity, temperature
//! and carbon dioxide. No I/O here. See `docs/air.md`.
use lamp_msgs::{AirFeel, AirState};

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

/// A reply's words, and how many filler bytes were skipped to find them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filled {
    pub words: Vec<u16>,
    pub skipped: usize,
}

/// How many times a reply's length is read, for the filler: on lamp-4ace
/// up to two `0xff` came inside one word (the first builder, #705 17 Oct
/// 18:30).
pub const READ_TIMES: usize = 3;

/// `count` words from a reply with filler in it. On a bus faster than the
/// sensor (lamp-4ace's I2C bus 0 runs at 400 kHz, the SEN6x at 100 kHz and
/// stretches the clock), the master reads `0xff` while the sensor still
/// holds the line, so `0xff` bytes come between the real ones. Each word is
/// two bytes and a CRC that matches; a `0xff` is taken as filler first and
/// as data only where no decode with it skipped holds, backtracking over
/// every `0xff`, so a real `0xffff` ("not known yet") still decodes. Filler
/// first, since filler can make a word: `0xff00` is `ff 00 00` (its CRC is
/// `0x00`), and filler in `ff 00 ff 00` around a null word, with the inner
/// `ff` skipped, reads as it. Bytes after the last word are not read.
pub fn words_among_filler(reply: &[u8], count: usize) -> Result<Filled, String> {
    // A reply that reads straight is taken as it is, filler or not: a real
    // `0xff` in it is never skipped for the bytes after the answer, whatever
    // the sensor sends there (the reviewer on #1366: with `0x00` after it,
    // T `0x1BFF` then CO2 `0x0370` read 28,756 ppm with filler first).
    if let Some(answer) = reply.get(..count * 3)
        && let Ok(words) = words(answer)
    {
        return Ok(Filled { words, skipped: 0 });
    }
    let mut decode = Decode {
        reply,
        failed: std::collections::HashSet::new(),
        words: Vec::with_capacity(count),
        deepest: 0,
    };
    if let Some(end) = decode.next_word(0, count) {
        return Ok(Filled {
            words: decode.words,
            skipped: end - count * 3,
        });
    }
    if reply.len() < count * 3 {
        return Err(format!(
            "a reply of {} bytes is not {count} words",
            reply.len()
        ));
    }
    Err(format!(
        "word {} fails its CRC, with filler skipped too",
        decode.deepest
    ))
}

/// Most filler bytes skipped in one place: on lamp-4ace there were two.
const MOST_FILLER: usize = 8;

/// A decode in progress: the words so far, the most ever found, and each
/// (position, words still to find) that has no decode.
struct Decode<'a> {
    reply: &'a [u8],
    failed: std::collections::HashSet<(usize, usize)>,
    words: Vec<u16>,
    deepest: usize,
}

impl Decode<'_> {
    /// The rest of the words from `at`: the end of the last one, or `None`.
    fn next_word(&mut self, at: usize, left: usize) -> Option<usize> {
        self.deepest = self.deepest.max(self.words.len());
        if left == 0 {
            return Some(at);
        }
        if self.failed.contains(&(at, left)) {
            return None;
        }
        let end = self.next_byte(at, &mut [0; 3], 0, left);
        if end.is_none() {
            self.failed.insert((at, left));
        }
        end
    }

    /// Byte `n` of a word from `at`: past every `0xff` there first, then
    /// taking each of them as data, nearest last.
    fn next_byte(&mut self, at: usize, word: &mut [u8; 3], n: usize, left: usize) -> Option<usize> {
        if n == 3 {
            if crc8(&word[..2]) != word[2] {
                return None;
            }
            self.words.push(u16::from_be_bytes([word[0], word[1]]));
            let end = self.next_word(at, left - 1);
            if end.is_none() {
                self.words.pop();
            }
            return end;
        }
        let mut past = at;
        while self.reply.get(past) == Some(&0xFF) && past - at < MOST_FILLER {
            past += 1;
        }
        for start in (at..=past).rev() {
            let Some(&byte) = self.reply.get(start) else {
                continue;
            };
            word[n] = byte;
            if let Some(end) = self.next_byte(start + 1, word, n + 1, left) {
                return Some(end);
            }
        }
        None
    }
}

/// The command as the two bytes sent.
pub fn command(code: u16) -> [u8; 2] {
    code.to_be_bytes()
}

/// The seven measured words as a reading. 0xFFFF (unsigned) and 0x7FFF
/// (signed) are the sensor's "not known yet"; CO2 says it as either, and
/// as 0x7FFF for its first seconds after a start (lamp-4ace, 17 Oct 18:3x),
/// which as 32,767 ppm would make every start Stuffy.
pub fn reading(words: &[u16]) -> Result<AirState, String> {
    let [pm1, pm25, pm4, pm10, rh, t, co2] = words else {
        return Err(format!("{} words where seven are read", words.len()));
    };
    let unsigned = |w: u16, scale: f32| (w != 0xFFFF).then(|| f32::from(w) / scale);
    let signed = |w: u16, scale: f32| {
        let v = w as i16;
        (v != 0x7FFF).then(|| f32::from(v) / scale)
    };
    let co2_ppm = (*co2 != 0xFFFF && *co2 != 0x7FFF).then_some(*co2);
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

    /// lamp-4ace's SEN63C on 17 Oct at 18:30, on its 400 kHz bus: the
    /// product name (0xD014), with up to two `0xff` inside one word (the
    /// first builder, #705). "SEN63C" and then null words, nothing lost.
    #[test]
    fn the_product_name_is_read_through_the_buses_filler() {
        let read = [
            0x53, 0xff, 0x45, 0x83, 0xff, 0x4e, 0x36, 0xff, 0x06, 0xff, 0x33, 0x43, 0xff, 0x70,
            0xff, 0x00, 0xff, 0x00, 0xff, 0x81, 0x00, 0xff, 0x00, 0x81, 0xff, 0x00, 0x00, 0xff,
            0x81, 0x00, 0xff, 0x00, 0x81, 0xff, 0x00, 0x00, 0xff, 0x81, 0x00, 0xff, 0x00, 0x81,
            0xff, 0x00, 0xff, 0x00, 0x81, 0xff, 0x00, 0xff, 0x00, 0x81, 0xff, 0x00, 0xff, 0x00,
            0xff, 0x81, 0xff, 0x00,
        ];
        // The straight decode fails at word 1, as lamp-air did there.
        assert_eq!(words(&read[..48]).unwrap_err(), "word 0 fails its CRC");
        let filled = words_among_filler(&read, 12).unwrap();
        assert_eq!(text(&filled.words), "SEN63C");
        assert_eq!(filled.words[3..], [0; 9]);
        // Four words in, `ff 00 [ff] 00` is a word too (0xff00, CRC 0x00);
        // the filler around `00 00 81` is skipped first, so it is no value.
        assert_eq!(
            words_among_filler(&read, 4).unwrap().words,
            [0x5345, 0x4E36, 0x3343, 0x0000]
        );
        assert_eq!(filled.skipped, 22);
        // More words than the read holds: the one that fails is named.
        assert_eq!(
            words_among_filler(&read, 13).unwrap_err(),
            "word 12 fails its CRC, with filler skipped too"
        );
    }

    /// A real `0xffff` ("not known yet") is data, with filler around it or
    /// none, and a reply with no filler reads as `words` reads it.
    #[test]
    fn a_word_that_really_is_0xffff_is_read_as_data() {
        let unknown = [0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x7FFF, 0x7FFF, 0xFFFF];
        let straight = reply(&unknown);
        let mut read = straight.clone();
        read.extend([0xff; 42]);
        let filled = words_among_filler(&read, 7).unwrap();
        assert_eq!((filled.words.as_slice(), filled.skipped), (&unknown[..], 0));
        // Filler before each byte of the first word, and inside the others.
        let mut with_filler = Vec::new();
        for (i, byte) in straight.iter().enumerate() {
            if i < 3 || i % 4 == 1 {
                with_filler.push(0xff);
            }
            with_filler.push(*byte);
        }
        with_filler.extend([0xff; 21]);
        let filled = words_among_filler(&with_filler, 7).unwrap();
        assert_eq!(filled.words, unknown);
        assert!(filled.skipped > 0);
        // lamp-4ace's reading of 14 Oct, with filler and without.
        let measured = [888, 925, 925, 925, 3170, 6600, 1931];
        let mut read = Vec::new();
        for (i, byte) in reply(&measured).into_iter().enumerate() {
            if i % 2 == 0 {
                read.extend([0xff, 0xff]);
            }
            read.push(byte);
        }
        let filled = words_among_filler(&read, 7).unwrap();
        assert_eq!(
            (filled.words.as_slice(), filled.skipped),
            (&measured[..], 22)
        );
        assert_eq!(words_among_filler(&reply(&measured), 7).unwrap().skipped, 0);
        assert_eq!(
            words_among_filler(&[0x03, 0x7a, 0x55], 2).unwrap_err(),
            "a reply of 3 bytes is not 2 words"
        );
    }

    /// Seven words, `0xffff`, `0x7fff`, `0xff00` and `0x00ff` among them,
    /// with `0xff` put before a byte a third of the time (up to four, twice
    /// what lamp-4ace gave), always decode as they were sent.
    #[test]
    fn words_among_random_filler_decode_as_sent() {
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..3_000 {
            let sent: Vec<u16> = (0..7)
                .map(|_| match next() % 6 {
                    0 => 0xFFFF,
                    1 => 0x7FFF,
                    2 => 0xFF00,
                    3 => 0x00FF,
                    4 => next() as u16,
                    _ => (next() % 1_000) as u16,
                })
                .collect();
            let mut read = Vec::new();
            for byte in reply(&sent) {
                for _ in 0..4 {
                    if next() % 3 != 0 {
                        break;
                    }
                    read.push(0xff);
                }
                read.push(byte);
            }
            read.extend([0xff; 21]);
            assert_eq!(
                words_among_filler(&read, 7).unwrap().words,
                sent,
                "{read:02x?}"
            );
        }
    }

    /// The reviewer on #1366: with no filler, a real `0xff` is data. T
    /// `0x1BFF` (35.84 °C) and CO2 `0x0370` (880 ppm), then `0x00` after the
    /// answer, read as sent, not as T 35.0 °C and CO2 28,756 ppm.
    #[test]
    fn a_reply_without_filler_is_read_straight_whatever_follows_it() {
        let sent = [409, 481, 526, 549, 5_550, 0x1BFF, 0x0370];
        for after in [0x00, 0xff, 0x54] {
            let mut read = reply(&sent);
            read.extend([after; 42]);
            let filled = words_among_filler(&read, 7).unwrap();
            assert_eq!((filled.words.as_slice(), filled.skipped), (&sent[..], 0));
        }
        let air = reading(&sent).unwrap();
        assert_eq!((air.temperature, air.co2_ppm), (Some(35.835), Some(880)));
    }

    /// A read that is all filler (the line held high) is no words, and
    /// says so at once: the backtracking keeps what failed.
    #[test]
    fn a_read_of_only_filler_is_no_words_and_quick() {
        let began = std::time::Instant::now();
        let error = words_among_filler(&[0xff; 16 * 3 * READ_TIMES], 16).unwrap_err();
        assert_eq!(error, "word 0 fails its CRC, with filler skipped too");
        assert!(began.elapsed() < std::time::Duration::from_secs(1));
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

    /// lamp-4ace on 17 Oct at 18:3x, just after a start (the first
    /// builder, #705 18:32): CO2 `0x7FFF` while it warms up is no value,
    /// not 32,767 ppm and Stuffy; the rest is read.
    #[test]
    fn co2_warming_up_is_not_known_not_stuffy() {
        let warming = [409, 481, 526, 549, 5_550, 7_100, 0x7FFF];
        let air = reading(&warming).unwrap();
        assert_eq!(air.co2_ppm, None);
        assert_eq!(air.feel, AirFeel::Unknown);
        assert_eq!(
            (air.pm2_5, air.humidity, air.temperature),
            (Some(48.1), Some(55.5), Some(35.5))
        );
        assert_eq!(
            reading(&[0, 0, 0, 0, 0, 0, 493]).unwrap().feel,
            AirFeel::Fresh
        );
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
