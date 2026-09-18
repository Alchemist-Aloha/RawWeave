use rawweave_color::{
    ColorError, ColorProfileRef, ColorTransformBackend, DisplayRGB, DisplayTransform,
    IccLittleCmsBackend, MatrixWorkingSpaceTransform, OcioBackend, SceneLinearRGB, SceneTransform,
    SrgbDisplayTransform, WorkingSpace,
};
use rawweave_image::Dimensions;

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
