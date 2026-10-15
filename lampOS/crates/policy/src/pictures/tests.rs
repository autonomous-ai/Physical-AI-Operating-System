use super::*;
use crate::attention::Room;

/// A JPEG of `width` by `height`, red rising across and blue rising down.
pub fn jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut rgb = Vec::new();
    for y in 0..u32::from(height) {
        for x in 0..u32::from(width) {
            rgb.extend([(x % 256) as u8, 90, (y % 256) as u8]);
        }
    }
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 80)
        .encode(&rgb, width, height, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    out
}

fn decoded_size(jpeg: &[u8]) -> (usize, usize) {
    let mut decoder = zune_jpeg::JpegDecoder::new(jpeg);
    decoder.decode().unwrap();
    decoder.dimensions().unwrap()
}

/// The eyes' rule, as the voice keeps it: no file, a stale one, a large
/// one or one that is not a JPEG is not sent; a fresh one is, with its age.
#[test]
fn a_frame_is_taken_only_as_the_voice_takes_one() {
    let dir = std::env::temp_dir().join(format!("policy-pictures-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sight.jpg");
    let now = SystemTime::now();
    assert_eq!(fresh_picture(&path, now).unwrap_err(), "no_frame");

    let frame = jpeg(320, 240);
    fs::write(&path, &frame).unwrap();
    let (taken, age) = fresh_picture(&path, SystemTime::now()).unwrap();
    assert_eq!(taken, frame);
    assert!(age <= FRESH);
    assert_eq!(
        fresh_picture(&path, SystemTime::now() + Duration::from_millis(1_500)).unwrap_err(),
        "stale"
    );

    fs::write(&path, vec![0xff; (MOST_BYTES + 1) as usize]).unwrap();
    assert_eq!(
        fresh_picture(&path, SystemTime::now()).unwrap_err(),
        "too_large"
    );
    fs::write(&path, b"not a picture").unwrap();
    assert_eq!(
        fresh_picture(&path, SystemTime::now()).unwrap_err(),
        "not_a_jpeg"
    );
    fs::remove_dir_all(&dir).unwrap();
}

/// TypeSafe takes 256 to 1,568 pixels a side and 78,400 in all: the eyes'
/// 320 by 240 is doubled; a frame that fits is sent as it is; one far too
/// small or too large is not sent.
#[test]
fn a_frame_is_made_as_large_as_the_service_takes() {
    let (sent, size) = for_jev(&jpeg(320, 240)).unwrap();
    assert_eq!(size, (640, 480));
    assert_eq!(decoded_size(&sent), (640, 480));
    let fits = jpeg(640, 480);
    assert_eq!(for_jev(&fits).unwrap(), (fits.clone(), (640, 480)));
    assert_eq!(for_jev(&jpeg(20, 20)).unwrap_err(), "too_small");
    assert_eq!(for_jev(&jpeg(1_200, 200)).unwrap_err(), "too_large");
    assert_eq!(for_jev(b"\xff\xd8 not really").unwrap_err(), "not_a_jpeg");
}

/// With a picture the state is the same text with `picture` last, holding
/// the key, and the questions are the seven and the picture's five.
#[test]
fn the_state_with_a_picture_names_it_last_and_five_questions_are_added() {
    let context = Context {
        heard: "Hey Lamp, what time is it?".into(),
        room: Room::Quiet,
        lamp_is_speaking: false,
        lamp_last_said: None,
        seconds_since_lamp_spoke: None,
        name_heard: None,
    };
    let plain = attention::state_text(&context);
    let pictured = attention::state_text_with_picture(&context, KEY);
    assert_eq!(
        pictured,
        format!("{},\"picture\":\"{KEY}\"}}", &plain[..plain.len() - 1])
    );
    let value: Value = serde_json::from_str(&pictured).unwrap();
    assert_eq!(value["picture"], KEY);
    let asked = attention::questions_with_picture();
    assert_eq!(asked.len(), 12);
    assert_eq!(asked[..7], attention::questions()[..]);
    let names: Vec<&str> = asked[7..].iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, attention::picture_questions().collect::<Vec<_>>());
    for (_, question) in &asked[7..] {
        let crate::decide::Question::Noul { instructions } = question else {
            panic!("{question:?}");
        };
        assert!(instructions.contains("state.picture"), "{instructions}");
        assert!(!instructions.contains(KEY));
    }
}
