use rawweave_color::{
    ColorError, ColorProfileRef, ColorTransformBackend, DisplayRGB, DisplayTransform,
    IccLittleCmsBackend, MatrixWorkingSpaceTransform, OcioBackend, PreviewSampling, SceneLinearRGB,
    SceneTransform, SrgbDisplayTransform, WorkingSpace,
};
use rawweave_image::Dimensions;

#[test]
fn reduced_rgb_sampling_survives_transforms_and_validated_wire_roundtrips() {
    let sampling = PreviewSampling {
        full_dimensions: Dimensions::new(9, 7),
        mip: 2,
    };
    let scene = SceneLinearRGB::new(
        Dimensions::new(3, 2),
        vec![[2.0, -0.25, 0.5]; 6],
        WorkingSpace::Srgb,
    )
    .unwrap()
    .with_sampling(Some(sampling))
    .unwrap();
    assert_eq!(
        scene.map_pixels(|pixel| pixel).unwrap().sampling(),
        Some(sampling)
    );
    let display = SrgbDisplayTransform.transform(&scene).unwrap();
    assert_eq!(display.sampling(), Some(sampling));
    assert_eq!(
        serde_json::from_str::<SceneLinearRGB>(&serde_json::to_string(&scene).unwrap()).unwrap(),
        scene
    );
    assert_eq!(
        serde_json::from_str::<DisplayRGB>(&serde_json::to_string(&display).unwrap()).unwrap(),
        display
    );
    let transformed = MatrixWorkingSpaceTransform::new(WorkingSpace::DisplayP3)
        .transform(&scene)
        .unwrap();
    assert_eq!(transformed.sampling(), Some(sampling));
    for invalid in [
        PreviewSampling {
            full_dimensions: Dimensions::new(1, 1),
            mip: 2,
        },
        PreviewSampling {
            full_dimensions: Dimensions::new(9, 7),
            mip: 7,
        },
    ] {
        assert!(scene.clone().with_sampling(Some(invalid)).is_err());
    }
    let mut wire = serde_json::to_value(&scene).unwrap();
    wire["sampling"]["mip"] = serde_json::json!(7);
    assert!(serde_json::from_value::<SceneLinearRGB>(wire).is_err());
    let full = SceneLinearRGB::from_pixels(1, 1, vec![[0.5; 3]]).unwrap();
    assert!(!serde_json::to_string(&full).unwrap().contains("sampling"));
}

#[test]
fn color_buffer_clones_share_pixels_without_changing_wire_format_or_hdr() {
    let scene = SceneLinearRGB::from_pixels(1, 1, vec![[2.0, 0.5, -0.25]]).unwrap();
    assert_eq!(scene.clone().pixels().as_ptr(), scene.pixels().as_ptr());
    assert_eq!(
        scene
            .with_working_space(WorkingSpace::CameraNative)
            .pixels()
            .as_ptr(),
        scene.pixels().as_ptr()
    );
    let changed = scene
        .map_pixels(|pixel| pixel.map(|channel| channel * 2.0))
        .unwrap();
    assert_ne!(changed.pixels().as_ptr(), scene.pixels().as_ptr());
    assert_eq!(scene.pixels(), &[[2.0, 0.5, -0.25]]);
    let display = SrgbDisplayTransform.transform(&scene).unwrap();
    assert_eq!(display.clone().pixels().as_ptr(), display.pixels().as_ptr());
    let json = serde_json::to_value(&scene).unwrap();
    assert!(json["pixels"].is_array());
    assert_eq!(
        serde_json::from_value::<SceneLinearRGB>(json).unwrap(),
        scene
    );
    let json = serde_json::to_value(&display).unwrap();
    assert!(json["pixels"].is_array());
    assert_eq!(serde_json::from_value::<DisplayRGB>(json).unwrap(), display);
}

#[test]
fn scene_linear_buffer_validates_dimensions_and_preserves_hdr_values() {
    let scene = SceneLinearRGB::new(
        Dimensions::new(2, 1),
        vec![[2.0, 0.5, 0.25], [0.0, 1.0, 4.0]],
        WorkingSpace::Srgb,
    )
    .unwrap();
    assert_eq!(scene.dimensions(), Dimensions::new(2, 1));
    assert_eq!(scene.pixel(1, 0), Some([0.0, 1.0, 4.0]));
}

#[test]
fn srgb_display_transform_maps_scene_linear_without_mutating_scene_values() {
    let scene = SceneLinearRGB::from_pixels(1, 1, vec![[0.25, 0.5, 2.0]]).unwrap();
    let display = SrgbDisplayTransform.transform(&scene).unwrap();
    assert_eq!(display.dimensions(), Dimensions::new(1, 1));
    assert!(display.pixel(0, 0).unwrap()[0] > 0.5);
    assert_eq!(scene.pixel(0, 0), Some([0.25, 0.5, 2.0]));
}

#[test]
fn display_rgb_reports_its_display_space() {
    let display = DisplayRGB::new(
        Dimensions::new(1, 1),
        vec![[0.0, 0.5, 1.0]],
        WorkingSpace::Srgb,
    )
    .unwrap();
    assert_eq!(display.working_space(), WorkingSpace::Srgb);
}

#[test]
fn matrix_transform_converts_non_srgb_working_space_before_relabeling() {
    let scene = SceneLinearRGB::new(
        Dimensions::new(1, 1),
        vec![[1.0, 0.0, 0.0]],
        WorkingSpace::DisplayP3,
    )
    .unwrap();
    let transformed = MatrixWorkingSpaceTransform::new(WorkingSpace::Srgb)
        .transform(&scene)
        .unwrap();

    assert_eq!(transformed.working_space(), WorkingSpace::Srgb);
    assert_ne!(transformed.pixel(0, 0), Some([1.0, 0.0, 0.0]));
    assert!(transformed.pixel(0, 0).unwrap()[0] > 0.8);
}

#[test]
fn display_transform_converts_declared_non_srgb_working_space() {
    let scene = SceneLinearRGB::new(
        Dimensions::new(1, 1),
        vec![[1.0, 0.0, 0.0]],
        WorkingSpace::DisplayP3,
    )
    .unwrap();
    let display = SrgbDisplayTransform.transform(&scene).unwrap();
    assert_ne!(display.pixel(0, 0), Some([1.0, 0.0, 0.0]));
}

#[test]
fn prophoto_d50_neutral_maps_to_d65_srgb_neutral() {
    let scene = SceneLinearRGB::new(
        Dimensions::new(1, 1),
        vec![[1.0, 1.0, 1.0]],
        WorkingSpace::ProPhoto,
    )
    .unwrap();
    let transformed = MatrixWorkingSpaceTransform::new(WorkingSpace::Srgb)
        .transform(&scene)
        .unwrap();
    let pixel = transformed.pixel(0, 0).unwrap();
    assert!(
        pixel
            .into_iter()
            .all(|channel| (channel - 1.0).abs() < 1e-3)
    );
}

#[test]
fn native_color_backends_expose_handles_and_fail_clearly_when_unavailable() {
    let ocio = OcioBackend::new("config.ocio");
    assert_eq!(ocio.backend(), ColorTransformBackend::Ocio);
    assert_eq!(ocio.config_handle(), "config.ocio");
    let error = ocio
        .transform(
            &SceneLinearRGB::from_pixels(1, 1, vec![[1.0, 0.0, 0.0]]).unwrap(),
            &ColorProfileRef::ocio("config.ocio", "scene_linear"),
            &ColorProfileRef::ocio("config.ocio", "sRGB"),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ColorError::BackendUnavailable {
            backend: ColorTransformBackend::Ocio,
            ..
        }
    ));

    let icc = IccLittleCmsBackend::new("display.icc");
    assert_eq!(icc.backend(), ColorTransformBackend::IccLittleCms);
    assert_eq!(icc.profile_handle(), "display.icc");
    assert!(matches!(
        icc.transform(
            &SceneLinearRGB::from_pixels(1, 1, vec![[1.0, 0.0, 0.0]]).unwrap(),
            &ColorProfileRef::icc("camera.icc"),
            &ColorProfileRef::icc("display.icc"),
        ),
        Err(ColorError::BackendUnavailable {
            backend: ColorTransformBackend::IccLittleCms,
            ..
        })
    ));
}

#[test]
fn color_serialization_preserves_working_space_and_pixels() {
    let scene = SceneLinearRGB::new(
        Dimensions::new(1, 1),
        vec![[2.0, 0.5, 0.25]],
        WorkingSpace::Custom("camera-linear".to_owned()),
    )
    .unwrap();
    let display = DisplayRGB::new(
        Dimensions::new(1, 1),
        vec![[0.8, 0.5, 0.25]],
        WorkingSpace::DisplayP3,
    )
    .unwrap();

    let scene_json = serde_json::to_string(&scene).unwrap();
    let display_json = serde_json::to_string(&display).unwrap();
    let restored_scene: SceneLinearRGB = serde_json::from_str(&scene_json).unwrap();
    let restored_display: DisplayRGB = serde_json::from_str(&display_json).unwrap();

    assert_eq!(restored_scene, scene);
    assert_eq!(restored_display, display);
}

#[test]
fn scene_linear_deserialization_rejects_mismatched_and_non_finite_samples() {
    let mismatched = r#"{
        "dimensions": {"width": 2, "height": 1},
        "pixels": [[1.0, 0.0, 0.0]],
        "working_space": "Srgb"
    }"#;
    let non_finite = r#"{
        "dimensions": {"width": 1, "height": 1},
        "pixels": [[1e39, 0.0, 0.0]],
        "working_space": "Srgb"
    }"#;

    assert!(serde_json::from_str::<SceneLinearRGB>(mismatched).is_err());
    assert!(serde_json::from_str::<SceneLinearRGB>(non_finite).is_err());
}

#[test]
fn display_rgb_deserialization_rejects_mismatched_and_non_finite_samples() {
    let mismatched = r#"{
        "dimensions": {"width": 2, "height": 1},
        "pixels": [[1.0, 0.0, 0.0]],
        "working_space": "Srgb"
    }"#;
    let non_finite = r#"{
        "dimensions": {"width": 1, "height": 1},
        "pixels": [[1e39, 0.0, 0.0]],
        "working_space": "Srgb"
    }"#;

    assert!(serde_json::from_str::<DisplayRGB>(mismatched).is_err());
    assert!(serde_json::from_str::<DisplayRGB>(non_finite).is_err());
}
