//! The pinned model through the real decoder, detector and analysis path.
mod common;

use lamp_perception::{
    analyze::{AnalysisError, Analyzer, INPUT_WIDTH},
    image::{ExposureVerdict, ImageError},
    model::{self, ModelError, YUNET},
    observation::{Attention, Presence, UnknownReason},
};
use std::{fs, os::unix::fs::PermissionsExt, time::Instant};

fn analyzer() -> Analyzer {
    let dir = common::installed_model();
    let bytes = model::read_verified(&dir.0, &YUNET).unwrap();
    Analyzer::new(bytes, &YUNET, INPUT_WIDTH)
}

#[test]
fn the_vendored_model_verifies_and_any_altered_copy_is_refused() {
    let original = fs::read(model::vendored_directory().join(YUNET.file)).unwrap();
    assert_eq!(model::sha256_hex(&original), YUNET.sha256);
    let dir = common::Private::new("model");
    let path = dir.join(YUNET.file);
    // A source checkout's own permissions are not part of this check.
    let write = |bytes: &[u8]| {
        let _ = fs::remove_file(&path);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    };
    write(&original);
    assert_eq!(model::read_verified(&dir.0, &YUNET).unwrap(), original);
    let mut flipped = original.clone();
    flipped[1000] ^= 1;
    write(&flipped);
    assert!(matches!(
        model::read_verified(&dir.0, &YUNET),
        Err(ModelError::Digest { .. })
    ));
    write(&original[..original.len() - 1]);
    assert!(matches!(
        model::read_verified(&dir.0, &YUNET),
        Err(ModelError::Size { .. })
    ));
    write(&original);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o664)).unwrap();
    assert!(matches!(
        model::read_verified(&dir.0, &YUNET),
        Err(ModelError::Writable(_))
    ));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::hard_link(&path, dir.join("second-name")).unwrap();
    assert!(matches!(
        model::read_verified(&dir.0, &YUNET),
        Err(ModelError::HardLinked(2))
    ));
    fs::remove_file(dir.join("second-name")).unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(model::vendored_directory().join(YUNET.file), &path).unwrap();
    assert!(matches!(
        model::read_verified(&dir.0, &YUNET),
        Err(ModelError::Symlink)
    ));
    fs::remove_file(&path).unwrap();
    assert!(matches!(
        model::read_verified(&dir.0, &YUNET),
        Err(ModelError::Missing(_))
    ));
}

#[test]
fn install_verifies_the_source_and_never_replaces_a_different_file() {
    let dir = common::Private::new("install");
    let target = dir.join("models");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    let source = model::vendored_directory().join(YUNET.file);
    let installed = model::install(&source, &target, &YUNET).unwrap();
    assert_eq!(
        fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
        0o644
    );
    // Installing the same bytes again is a verified no-op.
    model::install(&source, &target, &YUNET).unwrap();
    let wrong = dir.join("wrong.onnx");
    let mut bytes = fs::read(&source).unwrap();
    bytes[10] ^= 1;
    fs::write(&wrong, &bytes).unwrap();
    assert!(matches!(
        model::install(&wrong, &dir.join("unused"), &YUNET),
        Err(ModelError::Digest { .. })
    ));
    // A different file already at the target is refused, not overwritten.
    fs::remove_file(&installed).unwrap();
    fs::write(&installed, &bytes).unwrap();
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(model::install(&source, &target, &YUNET).is_err());
    assert_eq!(fs::read(&installed).unwrap(), bytes);
    assert!(
        fs::read_dir(&target).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("partial"))
    );
}

#[test]
fn a_drawn_face_is_found_where_it_was_drawn_with_ordered_landmarks() {
    let mut analyzer = analyzer();
    for (cx, cy) in [(320.0, 360.0), (960.0, 330.0)] {
        let analysis = analyzer.analyze(&common::face_jpeg(cx, cy, 150.0)).unwrap();
        assert_eq!(analysis.exposure.verdict, ExposureVerdict::Usable);
        let Presence::Faces {
            count,
            best_per_mille,
        } = analysis.presence
        else {
            panic!("{:?}", analysis.presence)
        };
        assert_eq!(count, 1);
        assert!(best_per_mille >= 600, "{best_per_mille}");
        let face = &analysis.faces[0];
        let [x, y, w, h] = face.bounds.map(f32::from);
        // Centre within 5% of the frame of where the oval was drawn.
        let (mx, my) = (
            (x + w / 2.0) / 10_000.0 * 1280.0,
            (y + h / 2.0) / 10_000.0 * 720.0,
        );
        assert!(
            (mx - cx).abs() < 64.0 && (my - cy).abs() < 36.0,
            "{mx},{my} vs {cx},{cy}"
        );
        // Subject's right eye appears left of the left eye; eyes above nose above mouth.
        let [right_eye, left_eye, nose, right_mouth, left_mouth] = face.landmarks;
        assert!(right_eye[0] < left_eye[0] && right_mouth[0] < left_mouth[0]);
        assert!(right_eye[1] < nose[1] && nose[1] < right_mouth[1]);
        assert!(
            matches!(analysis.attention, Attention::Toward { face: 0, .. }),
            "{:?}",
            analysis.attention
        );
        assert_eq!(analysis.model.id, "yunet-2023mar");
        assert_eq!(
            (analysis.model.input_width, analysis.model.input_height),
            (320, 192)
        );
    }
}

#[test]
fn darkness_and_glare_are_unknown_and_skip_inference() {
    let mut analyzer = analyzer();
    for (background, reason) in [
        ([4, 4, 6], UnknownReason::TooDark),
        ([255, 255, 255], UnknownReason::Overexposed),
    ] {
        let frame = common::jpeg(&common::scene(1280, 720, background, None), 1280, 720);
        let analysis = analyzer.analyze(&frame).unwrap();
        assert_eq!(analysis.presence, Presence::Unknown { reason });
        assert_eq!(analysis.attention, Attention::Unknown { reason });
        assert!(analysis.faces.is_empty());
        assert_eq!(analysis.inference_us, 0);
        assert_eq!(analysis.best_score, None);
    }
}

#[test]
fn an_empty_usable_scene_reports_no_face_detected_not_absence() {
    let mut analyzer = analyzer();
    let frame = common::jpeg(&common::scene(1280, 720, [110, 120, 130], None), 1280, 720);
    let analysis = analyzer.analyze(&frame).unwrap();
    assert_eq!(analysis.presence, Presence::NoFaceDetected);
    assert_eq!(
        analysis.attention,
        Attention::Unknown {
            reason: UnknownReason::NoFace
        }
    );
}

#[test]
fn a_small_distant_face_is_detected_but_its_attention_is_unknown() {
    let mut analyzer = analyzer();
    let analysis = analyzer
        .analyze(&common::face_jpeg(640.0, 360.0, 40.0))
        .unwrap();
    if let Presence::Faces { .. } = analysis.presence {
        assert_eq!(
            analysis.attention,
            Attention::Unknown {
                reason: UnknownReason::FaceTooSmall
            }
        );
    } else {
        // Too small to detect at all is also an honest outcome.
        assert_eq!(analysis.presence, Presence::NoFaceDetected);
    }
}

#[test]
fn corrupt_and_oversized_images_are_decode_errors() {
    let mut analyzer = analyzer();
    let frame = common::face_jpeg(640.0, 360.0, 150.0);
    assert!(matches!(
        analyzer.analyze(&frame[..frame.len() / 3]),
        Err(AnalysisError::Decode(_))
    ));
    assert!(matches!(
        analyzer.analyze(b"not a jpeg at all"),
        Err(AnalysisError::Decode(_))
    ));
    let wide = common::jpeg(
        &common::scene(1920, 1080, [100, 100, 100], None),
        1920,
        1080,
    );
    assert!(matches!(
        analyzer.analyze(&wide),
        Err(AnalysisError::Decode(
            ImageError::TooLarge { .. } | ImageError::Decode(_)
        ))
    ));
}

#[test]
fn a_different_frame_size_rebuilds_the_plan() {
    let mut analyzer = analyzer();
    let small = common::jpeg(
        &common::scene(640, 480, [110, 120, 130], Some((320.0, 240.0, 90.0))),
        640,
        480,
    );
    let analysis = analyzer.analyze(&small).unwrap();
    assert_eq!(
        (analysis.model.input_width, analysis.model.input_height),
        (320, 256)
    );
    assert!(
        matches!(analysis.presence, Presence::Faces { .. }),
        "{:?}",
        analysis.presence
    );
    let large = analyzer
        .analyze(&common::face_jpeg(640.0, 360.0, 150.0))
        .unwrap();
    assert_eq!(
        (large.model.input_width, large.model.input_height),
        (320, 192)
    );
}

/// Host timing only: decode plus inference for a 1280x720 frame on this
/// machine. Lamp's ARM CPU will be slower; this is not a device measurement.
#[test]
fn measured_host_analysis_time() {
    let dir = common::installed_model();
    let started = Instant::now();
    let bytes = model::read_verified(&dir.0, &YUNET).unwrap();
    let verified_us = started.elapsed().as_micros();
    let mut analyzer = Analyzer::new(bytes, &YUNET, INPUT_WIDTH);
    analyzer.prepare(1280, 720).unwrap();
    println!(
        "MEASURED perception_model_load verify_us={verified_us} verify_and_plan_us={} (debug={})",
        started.elapsed().as_micros(),
        cfg!(debug_assertions)
    );
    let frame = common::face_jpeg(640.0, 360.0, 150.0);
    let mut decode = Vec::new();
    let mut inference = Vec::new();
    let mut total = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let analysis = analyzer.analyze(&frame).unwrap();
        total.push(started.elapsed().as_micros() as u64);
        decode.push(u64::from(analysis.decode_us));
        inference.push(u64::from(analysis.inference_us));
    }
    for values in [&mut decode, &mut inference, &mut total] {
        values.sort_unstable();
    }
    let p = |v: &Vec<u64>, q: usize| v[(v.len() * q).div_ceil(100) - 1];
    println!(
        "MEASURED perception_host n=20 frame=1280x720 decode_us p50={} p95={} inference_us p50={} p95={} total_us p50={} p95={} (debug={})",
        p(&decode, 50),
        p(&decode, 95),
        p(&inference, 50),
        p(&inference, 95),
        p(&total, 50),
        p(&total, 95),
        cfg!(debug_assertions)
    );
    // A sanity bound only, generous enough for unoptimized test builds.
    assert!(p(&total, 50) < 5_000_000);
}

/// Remove every DHT segment before the scan, as many UVC cameras send MJPEG.
fn strip_huffman_tables(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes[..2].to_vec();
    let mut i = 2;
    while bytes[i] == 0xff {
        let marker = bytes[i + 1];
        if marker == 0xda {
            out.extend_from_slice(&bytes[i..]);
            return out;
        }
        let length = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if marker != 0xc4 {
            out.extend_from_slice(&bytes[i..i + 2 + length]);
        }
        i += 2 + length;
    }
    panic!("no scan");
}

#[test]
fn mjpeg_without_huffman_tables_decodes_to_identical_pixels() {
    use lamp_perception::image::{decode_jpeg, with_standard_huffman_tables};
    // jpeg-encoder writes the Annex K tables, so stripping and reinserting
    // them must reproduce the original pixels exactly.
    let frame = common::face_jpeg(640.0, 360.0, 150.0);
    let stripped = strip_huffman_tables(&frame);
    assert!(stripped.len() < frame.len());
    assert!(matches!(
        with_standard_huffman_tables(&frame).unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    let original = decode_jpeg(&frame).unwrap();
    let repaired = decode_jpeg(&stripped).unwrap();
    assert_eq!((repaired.width, repaired.height), (1280, 720));
    assert!(
        original.pixels == repaired.pixels,
        "standard tables differ from the encoder's"
    );
    let mut analyzer = analyzer();
    assert!(matches!(
        analyzer.analyze(&stripped).unwrap().presence,
        Presence::Faces { .. }
    ));
}

/// Transport truncation (no end-of-image marker) is refused before decoding.
/// Known limit: a scan cut short but still followed by an end marker decodes
/// partly, with the missing area mid-grey. Driver-flagged corrupt buffers are
/// already terminal in the camera backend; such frames should not reach here.
#[test]
fn a_frame_without_its_end_marker_is_refused() {
    use lamp_perception::image::decode_jpeg;
    let frame = common::face_jpeg(640.0, 360.0, 150.0);
    for cut in [frame.len() / 3, frame.len() - 1] {
        assert!(decode_jpeg(&frame[..cut]).is_err(), "cut at {cut}");
    }
}
