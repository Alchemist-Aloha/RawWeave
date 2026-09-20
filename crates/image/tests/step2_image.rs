use rawweave_image::{ColorDomain, Dimensions, Image, PixelFormat, Region};

#[test]
fn image_carries_format_domain_and_revision_safe_views() {
    let image = Image::from_pixels_with_metadata(
        2,
        2,
        vec![
            [1.0, 0.0, 0.0, 1.0],
            [2.0, 0.0, 0.0, 1.0],
            [3.0, 0.0, 0.0, 1.0],
            [4.0, 0.0, 0.0, 1.0],
        ],
        PixelFormat::Rgba32Float,
        ColorDomain::LinearSrgb,
    )
    .unwrap();

    assert_eq!(image.dimensions().width, 2);
    assert_eq!(image.dimensions().height, 2);
    assert_eq!(image.pixel_format(), PixelFormat::Rgba32Float);
    assert_eq!(image.color_domain(), ColorDomain::LinearSrgb);

    let view = image.view(Region::new(1, 0, 1, 2)).unwrap();
    assert_eq!(view.pixel(0, 0), Some([2.0, 0.0, 0.0, 1.0]));
    assert_eq!(view.pixel(0, 1), Some([4.0, 0.0, 0.0, 1.0]));
    assert_eq!(view.revision(), image.revision());
    assert_eq!(view.to_image().unwrap().dimensions().width, 1);
}

#[test]
fn image_views_reject_regions_outside_the_image() {
    let image = Image::new(4, 4).unwrap();
    assert!(image.view(Region::new(3, 3, 2, 1)).is_err());
}

#[test]
fn images_reject_origins_that_overflow_their_dimensions() {
    assert!(
        Image::from_pixels_with_origin(
            Dimensions::new(2, 1),
            (u32::MAX, 0),
            vec![[0.0; 4]; 2],
            PixelFormat::default(),
            rawweave_image::ColorMetadata::default(),
        )
        .is_err()
    );
}

#[test]
fn image_serialization_preserves_schema_metadata_origin_and_revision() {
    let image = Image::from_pixels_with_origin(
        Dimensions::new(1, 1),
        (4, 5),
        vec![[0.25, 0.5, 0.75, 1.0]],
        PixelFormat::Rgba16Float,
        rawweave_image::ColorMetadata {
            domain: ColorDomain::DisplayP3,
            alpha_is_premultiplied: true,
        },
    )
    .unwrap();
    let json = serde_json::to_string(&image).unwrap();
    let restored: Image = serde_json::from_str(&json).unwrap();

    assert_eq!(restored, image);
    assert_eq!(restored.origin(), image.origin());
    assert_eq!(restored.revision(), image.revision());
    assert_eq!(restored.pixel_format(), image.pixel_format());
    assert_eq!(restored.color_metadata(), image.color_metadata());
}

#[test]
fn image_deserialization_rejects_mismatched_pixel_buffers() {
    let json = r#"{
        "width": 2,
        "height": 1,
        "pixels": [[0.0, 0.0, 0.0, 1.0]]
    }"#;

    assert!(serde_json::from_str::<Image>(json).is_err());
}

#[test]
fn image_deserialization_rejects_origins_that_overflow_dimensions() {
    let json = r#"{
        "width": 1,
        "height": 1,
        "origin_x": 4294967295,
        "origin_y": 0,
        "pixels": [[0.0, 0.0, 0.0, 1.0]]
    }"#;

    assert!(serde_json::from_str::<Image>(json).is_err());
}
