use rawweave_image::Dimensions;
use rawweave_raw::{
    CameraMetadata, CameraProfile, CfaColor, CfaPattern, DeterministicCorpus, DeterministicDecoder,
    EmbeddedPreview, ExifMetadata, LensProfile, LensProfileProvider, LensProfileRegistry, Mosaic,
    Orientation, RawDecodeLimits, RawDecoder, RawError, RawFrame, RawloaderDecoder,
    parse_exif_metadata,
};
use serde_json::json;

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
                make: "Canon".to_owned(),
                model: "EOS R5".to_owned(),
                lens: Some("RF 50mm F1.2 L USM".to_owned()),
                ..CameraMetadata::default()
            })
            .is_none()
    );
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

#[test]
fn serde_rejects_empty_cfa_and_invalid_mosaic_dimensions() {
    let empty_cfa = json!({
        "width": 0,
        "height": 0,
        "colors": []
    });
    assert!(serde_json::from_value::<CfaPattern>(empty_cfa).is_err());

    let invalid_mosaic = json!({
        "dimensions": {"width": 0, "height": 2},
        "samples": [],
        "bit_depth": 12,
        "cfa": {"width": 1, "height": 1, "colors": ["Red"]},
        "orientation": "Normal"
    });
    assert!(serde_json::from_value::<Mosaic>(invalid_mosaic).is_err());
}

#[test]
fn camera_profile_legacy_payload_derives_camera_to_xyz_from_source_matrix() {
    let payload = json!({
        "make": "Legacy",
        "model": "Camera",
        "xyz_to_camera": [
            [2.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 0.0]
        ]
    });
    let profile: CameraProfile = serde_json::from_value(payload).unwrap();
    assert_eq!(
        profile.camera_to_xyz,
        [[0.5, 0.0, 0.0], [0.0, 0.25, 0.0], [0.0, 0.0, 0.2]]
    );
}

#[test]
fn raw_frame_legacy_preview_and_invalid_levels_are_migrated_or_rejected() {
    let mut legacy = serde_json::to_value(fixture_frame()).unwrap();
    legacy["embedded_preview"] = json!([0xff, 0xd8, 0xff, 0xd9]);
    let migrated: RawFrame = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        migrated.embedded_preview_bytes(),
        Some(&[0xff, 0xd8, 0xff, 0xd9][..])
    );

    let mut invalid = serde_json::to_value(fixture_frame()).unwrap();
    invalid["black_levels"] = json!([4096.0, 64.0, 64.0, 64.0]);
    assert!(serde_json::from_value::<RawFrame>(invalid).is_err());
}

#[test]
fn decoder_limits_reject_oversized_bytes_before_rawloader() {
    let limits = RawDecodeLimits {
        max_input_bytes: 3,
        max_samples: 8,
        max_pixels: 8,
        max_width: 8,
        max_height: 8,
    };
    let decoder = RawloaderDecoder::with_limits(limits);
    let error = decoder.decode_bytes(b"1234").unwrap_err();
    assert_eq!(error, RawError::InputTooLarge { actual: 4, max: 3 });

    let path = std::env::temp_dir().join(format!(
        "rawweave-limit-test-{}-{}.raw",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::write(&path, b"1234").unwrap();
    let file_error = decoder.decode_file(&path).unwrap_err();
    std::fs::remove_file(path).unwrap();
    assert_eq!(file_error, error);
}

#[test]
fn header_dimensions_reject_oversized_arri_mrw_and_x3f_before_rawloader() {
    let decoder = RawloaderDecoder::with_limits(RawDecodeLimits {
        max_input_bytes: 4096,
        max_samples: 8,
        max_pixels: 8,
        max_width: 8,
        max_height: 8,
    });

    for input in [
        crafted_arri_header(9, 1),
        crafted_mrw_header(9, 1),
        crafted_x3f_header(9, 1),
    ] {
        let error = decoder.decode(&input).unwrap_err();
        assert!(
            matches!(error, RawError::DimensionTooLarge { .. }),
            "expected a pre-decode dimension limit error, got {error:?}"
        );
    }
}

#[test]
fn header_dimensions_reject_oversized_sample_arithmetic_before_rawloader() {
    let decoder = RawloaderDecoder::with_limits(RawDecodeLimits {
        max_input_bytes: 4096,
        max_samples: 8,
        max_pixels: 64,
        max_width: 64,
        max_height: 64,
    });

    let error = decoder.decode(&crafted_arri_header(3, 3)).unwrap_err();
    assert_eq!(error, RawError::SampleCountTooLarge { actual: 9, max: 8 });
}

#[test]
fn malformed_or_truncated_header_probes_are_panic_free() {
    let decoder = RawloaderDecoder::default();
    for input in [
        b"ARRI".to_vec(),
        b"\0MRM".to_vec(),
        b"FOVb".to_vec(),
        crafted_mrw_header_without_dimensions(),
        crafted_x3f_header_without_image(),
    ] {
        let result = std::panic::catch_unwind(|| decoder.decode(&input));
        assert!(result.is_ok(), "header probe panicked for {input:?}");
        assert!(result.unwrap().is_err());
    }
}

#[test]
fn valid_size_header_probe_reaches_vendor_decoder() {
    let decoder = RawloaderDecoder::with_limits(RawDecodeLimits {
        max_input_bytes: 4096,
        max_samples: 64,
        max_pixels: 64,
        max_width: 64,
        max_height: 64,
    });

    for input in [
        crafted_arri_header(8, 1),
        crafted_mrw_header(8, 1),
        crafted_x3f_header(1, 1),
    ] {
        let error = decoder.decode(&input).unwrap_err();
        assert!(
            matches!(error, RawError::Decoder(_)),
            "expected vendor decoder error after a valid-size probe, got {error:?}"
        );
    }
}

fn crafted_arri_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0_u8; 32];
    bytes[0..4].copy_from_slice(b"ARRI");
    put_le_u32(&mut bytes, 8, 32);
    put_le_u32(&mut bytes, 20, width);
    put_le_u32(&mut bytes, 24, height);
    bytes
}

fn crafted_mrw_header(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0_u8; 64];
    put_be_u32(&mut bytes, 0, 0x004d_524d);
    put_be_u32(&mut bytes, 4, 48);
    put_be_u32(&mut bytes, 8, 0x0050_5244);
    put_be_u32(&mut bytes, 12, 40);
    put_be_u16(&mut bytes, 24, height);
    put_be_u16(&mut bytes, 26, width);
    bytes
}

fn crafted_mrw_header_without_dimensions() -> Vec<u8> {
    let mut bytes = crafted_mrw_header(1, 1);
    put_be_u32(&mut bytes, 12, 0);
    put_be_u16(&mut bytes, 24, 0);
    put_be_u16(&mut bytes, 26, 0);
    bytes
}

fn crafted_x3f_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = crafted_x3f_header_without_image();
    put_le_u32(&mut bytes, 72, 1);
    put_le_u32(&mut bytes, 76, 32);
    bytes[84..88].copy_from_slice(b"IMA2");
    put_le_u32(&mut bytes, 40, 1);
    put_le_u32(&mut bytes, 44, 35);
    put_le_u32(&mut bytes, 48, width);
    put_le_u32(&mut bytes, 52, height);
    bytes
}

fn crafted_x3f_header_without_image() -> Vec<u8> {
    let mut bytes = vec![0_u8; 128];
    bytes[0..4].copy_from_slice(b"FOVb");
    put_le_u32(&mut bytes, 68, 0x0002_0000);
    put_le_u32(&mut bytes, 124, 64);
    bytes
}

fn put_le_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_be_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn put_be_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
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
