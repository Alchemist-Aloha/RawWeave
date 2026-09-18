use rawweave_image::Dimensions;
use rawweave_raw::{
    CameraMetadata, CameraProfile, CfaColor, CfaPattern, DeterministicCorpus, DeterministicDecoder,
    EmbeddedPreview, ExifMetadata, LensProfile, LensProfileProvider, LensProfileRegistry, Mosaic,
    Orientation, RawDecoder, RawFrame, parse_exif_metadata,
};

fn fixture_frame() -> RawFrame {
    let mosaic = Mosaic::new(
        Dimensions::new(4, 2),
        vec![64.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0],
        12,
        CfaPattern::new(
            2,
            2,
            vec![
                CfaColor::Red,
                CfaColor::Green,
                CfaColor::Green,
                CfaColor::Blue,
            ],
        )
        .unwrap(),
        Orientation::Rotate90,
    )
    .unwrap();
    RawFrame::new(
        mosaic,
        [64.0; 4],
        [4095.0; 4],
        CameraMetadata {
            make: "RawWeave".to_owned(),
            model: "Test Camera".to_owned(),
            lens: Some("Test Lens".to_owned()),
            iso: Some(400),
            aperture: Some(2.8),
            shutter_seconds: Some(1.0 / 125.0),
            focal_length_mm: Some(35.0),
            capture_time: Some("2026-09-18T12:00:00Z".to_owned()),
            orientation: Orientation::Rotate90,
            dimensions: Some(Dimensions::new(4, 2)),
        },
        CameraProfile::identity("RawWeave", "Test Camera"),
        Some(LensProfile::identity("Test Lens")),
        None,
        ExifMetadata::default(),
    )
    .unwrap()
}

#[test]
fn raw_frame_exposes_sensor_mosaic_levels_and_camera_metadata() {
    let frame = fixture_frame();

    assert_eq!(frame.sensor_dimensions(), Dimensions::new(4, 2));
    assert_eq!(frame.mosaic().bit_depth(), 12);
    assert_eq!(frame.mosaic().cfa().color_at(1, 1), Some(CfaColor::Blue));
    assert_eq!(frame.black_levels(), &[64.0; 4]);
    assert_eq!(frame.white_levels(), &[4095.0; 4]);
    assert_eq!(frame.camera().make, "RawWeave");
    assert_eq!(frame.camera().iso, Some(400));
    assert_eq!(frame.profile().model, "Test Camera");
    assert_eq!(frame.lens_profile().unwrap().name, "Test Lens");
}

#[test]
fn deterministic_test_decoder_returns_the_same_frame_for_any_input() {
    let expected = fixture_frame();
    let decoder = DeterministicDecoder::new(expected.clone());

    assert_eq!(decoder.decode(b"fixture-a").unwrap(), expected);
    assert_eq!(decoder.decode(b"fixture-b").unwrap(), expected);
}

#[test]
fn cfa_patterns_validate_sample_dimensions_and_support_multiple_colors() {
    let xtrans = CfaPattern::new(
        3,
        2,
        vec![
            CfaColor::Red,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Green,
            CfaColor::Blue,
            CfaColor::Red,
        ],
    )
    .unwrap();
    assert_eq!(xtrans.color_at(2, 4), Some(CfaColor::Blue));
    assert!(CfaPattern::new(2, 2, vec![CfaColor::Red]).is_err());
}

#[test]
fn crafted_tiff_exif_maps_capture_metadata_and_extracts_thumbnail() {
    let parsed = parse_exif_metadata(&crafted_tiff_exif()).unwrap();

    assert_eq!(parsed.camera.make, "Acme Camera");
    assert_eq!(parsed.camera.model, "Acme Model");
    assert_eq!(parsed.camera.lens.as_deref(), Some("Prime Lens"));
    assert_eq!(parsed.camera.iso, Some(400));
    assert_eq!(parsed.camera.aperture, Some(2.8));
    assert_eq!(parsed.camera.shutter_seconds, Some(1.0 / 125.0));
    assert_eq!(parsed.camera.focal_length_mm, Some(35.0));
    assert_eq!(
        parsed.camera.capture_time.as_deref(),
        Some("2026-09-18T12:34:56")
    );
    assert_eq!(parsed.camera.orientation, Orientation::Rotate270);
    assert_eq!(parsed.camera.dimensions, Some(Dimensions::new(600, 400)));
    assert_eq!(
        parsed.embedded_preview.as_deref(),
        Some(&[0xff, 0xd8, 0xff, 0xd9][..])
    );
    assert!(parsed.exif.tags.keys().any(|tag| tag.starts_with("Make@")));
}

#[test]
fn deterministic_corpus_covers_distinct_cameras_levels_patterns_and_clipped_highlights() {
    let corpus = DeterministicCorpus::all();
    assert!(corpus.len() >= 4);
    assert!(
        corpus
            .iter()
            .map(|frame| frame.camera().make.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 3
    );
    assert!(
        corpus
            .iter()
            .map(|frame| frame.camera().model.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 4
    );
    assert!(
        corpus
            .iter()
            .map(|frame| frame.black_levels().map(f32::to_bits))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 3
    );
    assert!(
        corpus
            .iter()
            .map(|frame| frame.white_levels().map(f32::to_bits))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 3
    );
    assert!(corpus.iter().any(|frame| frame.mosaic().cfa().width() == 6
        && frame.mosaic().cfa().height() == 6
        && frame.sensor_dimensions().width >= 6
        && frame.sensor_dimensions().height >= 6));
    assert!(corpus.iter().any(|frame| {
        frame
            .mosaic()
            .samples()
            .iter()
            .zip(frame.white_levels().iter().cycle())
            .any(|(sample, white)| sample > white)
    }));
}

#[test]
fn camera_profile_inverts_xyz_to_camera_and_rejects_singular_matrices() {
    let profile = CameraProfile::from_xyz_to_camera(
        "Test",
        "Matrix",
        [
            [2.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 0.0],
        ],
    )
    .unwrap();
    assert_eq!(
        profile.camera_to_xyz,
        [[0.5, 0.0, 0.0], [0.0, 0.25, 0.0], [0.0, 0.0, 0.2]]
    );
    let error = CameraProfile::from_xyz_to_camera(
        "Test",
        "Singular",
        [
            [1.0, 2.0, 3.0],
            [2.0, 4.0, 6.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
    )
    .unwrap_err();
    assert!(error.to_string().contains("camera color matrix"));
}

#[test]
fn built_in_lens_profiles_normalize_keys_and_mark_unknown_lenses_unavailable() {
    let registry = LensProfileRegistry::built_in();
    let camera = CameraMetadata {
        make: " RAWWEAVE ".to_owned(),
        model: "Test-Camera".to_owned(),
        lens: Some("Test Lens".to_owned()),
        ..CameraMetadata::default()
    };
    let profile = registry.profile_for(&camera).unwrap();
    assert_eq!(
        profile.provenance(),
        rawweave_raw::LensProfileProvenance::BuiltInCalibrated
    );
    assert!(!profile.is_identity());
    assert!(
        registry
            .profile_for(&CameraMetadata {
                make: "Unknown".to_owned(),
                model: "Body".to_owned(),
                lens: Some("Lens".to_owned()),
                ..CameraMetadata::default()
            })
            .is_none()
    );
    let unavailable = LensProfile::unavailable("Unknown Lens");
    assert_eq!(
        unavailable.provenance(),
        rawweave_raw::LensProfileProvenance::Unavailable
    );
    assert!(unavailable.is_identity());
}

#[test]
fn embedded_preview_preserves_availability_and_mime_type() {
    let unavailable = EmbeddedPreview::unavailable();
    assert_eq!(unavailable.bytes(), None);
    let available = EmbeddedPreview::new(Some(vec![0xff, 0xd8]), Some("image/jpeg"));
    assert_eq!(available.bytes(), Some(&[0xff, 0xd8][..]));
    assert_eq!(available.mime_type(), Some("image/jpeg"));
}

fn crafted_tiff_exif() -> Vec<u8> {
    let mut bytes = vec![0_u8; 360];
    bytes[0..8].copy_from_slice(b"II*\0\x08\0\0\0");
    put_u16(&mut bytes, 8, 6);
    put_entry(&mut bytes, 10, 0x010f, 2, 12, 200);
    put_entry(&mut bytes, 22, 0x0110, 2, 11, 212);
    put_entry(&mut bytes, 34, 0x0112, 3, 1, 8);
    put_entry(&mut bytes, 46, 0x0100, 4, 1, 600);
    put_entry(&mut bytes, 58, 0x0101, 4, 1, 400);
    put_entry(&mut bytes, 70, 0x8769, 4, 1, 226);
    put_u32(&mut bytes, 82, 98);

    put_u16(&mut bytes, 98, 2);
    put_entry(&mut bytes, 100, 0x0201, 4, 1, 140);
    put_entry(&mut bytes, 112, 0x0202, 4, 1, 4);
    put_u32(&mut bytes, 124, 0);

    put_u16(&mut bytes, 226, 6);
    put_entry(&mut bytes, 228, 0x829a, 5, 1, 304);
    put_entry(&mut bytes, 240, 0x829d, 5, 1, 312);
    put_entry(&mut bytes, 252, 0x8827, 3, 1, 400);
    put_entry(&mut bytes, 264, 0x9003, 2, 20, 320);
    put_entry(&mut bytes, 276, 0x920a, 5, 1, 340);
    put_entry(&mut bytes, 288, 0xa434, 2, 11, 348);
    put_u32(&mut bytes, 300, 0);

    bytes[140..144].copy_from_slice(&[0xff, 0xd8, 0xff, 0xd9]);
    bytes[200..212].copy_from_slice(b"Acme Camera\0");
    bytes[212..223].copy_from_slice(b"Acme Model\0");
    bytes[304..312].copy_from_slice(&[1, 0, 0, 0, 125, 0, 0, 0]);
    bytes[312..320].copy_from_slice(&[14, 0, 0, 0, 5, 0, 0, 0]);
    bytes[320..340].copy_from_slice(b"2026:09:18 12:34:56\0");
    bytes[340..348].copy_from_slice(&[35, 0, 0, 0, 1, 0, 0, 0]);
    bytes[348..359].copy_from_slice(b"Prime Lens\0");
    bytes
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_entry(bytes: &mut [u8], offset: usize, tag: u16, kind: u16, count: u32, value: u32) {
    put_u16(bytes, offset, tag);
    put_u16(bytes, offset + 2, kind);
    put_u32(bytes, offset + 4, count);
    put_u32(bytes, offset + 8, value);
}
