use rawweave_image::Dimensions;
use rawweave_raw::{
    CameraMetadata, CameraProfile, CfaColor, CfaPattern, DeterministicDecoder, ExifMetadata,
    LensProfile, Mosaic, Orientation, RawDecoder, RawFrame,
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
