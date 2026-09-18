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
