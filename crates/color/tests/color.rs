use rawweave_color::{
    DisplayRGB, DisplayTransform, SceneLinearRGB, SrgbDisplayTransform, WorkingSpace,
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
