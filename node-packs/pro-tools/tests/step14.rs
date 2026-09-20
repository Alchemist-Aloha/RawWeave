use rawweave_image::{Image, Region};
use rawweave_node_api::{
    EvaluationContext, ExecutionCapability, Inputs, NodePack, NodeRegistry, Parameters, Value,
};
use rawweave_pro_tools::{ProToolsPack, descriptors, register_nodes};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    registry
}

fn input(image: Image) -> Inputs {
    [(String::from("image"), Value::Image(image))]
        .into_iter()
        .collect()
}

fn image() -> Image {
    Image::from_pixels_with_origin(
        rawweave_image::Dimensions::new(3, 3),
        (10, 20),
        vec![
            [0.0, 0.1, 0.2, 1.0],
            [0.2, 0.3, 0.4, 1.0],
            [0.4, 0.5, 0.6, 1.0],
            [0.1, 0.2, 0.3, 1.0],
            [0.3, 0.4, 0.5, 1.0],
            [0.5, 0.6, 0.7, 1.0],
            [0.2, 0.3, 0.4, 1.0],
            [0.4, 0.5, 0.6, 1.0],
            [0.6, 0.7, 0.8, 1.0],
        ],
        rawweave_image::PixelFormat::default(),
        rawweave_image::ColorMetadata::default(),
    )
    .unwrap()
}

#[test]
fn professional_pack_exposes_representative_detail_optical_color_creative_and_analysis_nodes() {
    let registry = registry();
    for type_id in [
        "pro.advanced-denoise",
        "pro.detail-separation",
        "pro.deconvolution",
        "pro.sharpen",
        "pro.local-contrast",
        "pro.defringe",
        "pro.chromatic-aberration",
        "pro.distortion-correction",
        "pro.vignetting",
        "pro.tone-map",
        "pro.color-zones",
        "pro.selective-color",
        "pro.channel-mixer",
        "pro.perceptual-saturation",
        "pro.gamut-compression",
        "pro.lut",
        "pro.film-curve",
        "pro.grain",
        "pro.halation",
        "pro.bloom",
        "pro.dye-layer",
        "pro.split-toning",
        "pro.histogram-statistics",
        "pro.clipping-analysis",
        "pro.noise-estimate",
        "pro.sharpness-estimate",
        "pro.dynamic-range-estimate",
    ] {
        assert!(registry.descriptor(type_id).is_some(), "missing {type_id}");
    }
    let descriptor = registry.descriptor("pro.sharpen").unwrap();
    assert!(descriptor.capabilities.contains(&ExecutionCapability::Cpu));
    assert!(
        descriptor
            .capabilities
            .contains(&ExecutionCapability::RegionAware)
    );
    assert_eq!(descriptor.inputs[0].data_type, "core.Image");
    assert_eq!(descriptor.outputs[0].data_type, "core.Image");
    assert_eq!(ProToolsPack.id(), "pro-tools");
    assert_eq!(descriptors().len(), registry.descriptors().len());
}

#[test]
fn channel_mixer_applies_a_bounded_matrix_and_preserves_global_origin() {
    let node = registry().instantiate("pro.channel-mixer").unwrap();
    let parameters = [
        ("m00".to_owned(), 0.0.into()),
        ("m01".to_owned(), 1.0.into()),
        ("m02".to_owned(), 0.0.into()),
        ("m10".to_owned(), 1.0.into()),
        ("m11".to_owned(), 0.0.into()),
        ("m12".to_owned(), 0.0.into()),
        ("m20".to_owned(), 0.0.into()),
        ("m21".to_owned(), 0.0.into()),
        ("m22".to_owned(), 1.0.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let result = node
        .evaluate(&input(image()), &parameters, &EvaluationContext::default())
        .unwrap();
    let Value::Image(actual) = &result.outputs["image"] else {
        panic!("channel mixer must emit an image");
    };
    assert_eq!(actual.global_region(), Region::new(10, 20, 3, 3));
    assert_eq!(actual.pixel(0, 0), Some([0.1, 0.0, 0.2, 1.0]));
}

#[test]
fn grain_is_deterministic_in_global_coordinates_and_leaves_alpha_unchanged() {
    let node = registry().instantiate("pro.grain").unwrap();
    let parameters = [
        ("amount".to_owned(), 0.25.into()),
        ("seed".to_owned(), 42_i64.into()),
        ("size".to_owned(), 1.0.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let context = EvaluationContext::default().with_requested_region(Region::new(11, 21, 1, 2));
    let first = node
        .evaluate(&input(image()), &parameters, &context)
        .unwrap();
    let second = node
        .evaluate(&input(image()), &parameters, &context)
        .unwrap();
    assert_eq!(first, second);
    let Value::Image(actual) = &first.outputs["image"] else {
        panic!("grain must emit an image");
    };
    assert_eq!(actual.global_region(), Region::new(11, 21, 1, 2));
    assert!(actual.pixels().iter().all(|pixel| pixel[3] == 1.0));
}

#[test]
fn region_requests_outside_the_image_return_an_empty_requested_region() {
    let node = registry().instantiate("pro.channel-mixer").unwrap();
    let context = EvaluationContext::default().with_requested_region(Region::new(100, 200, 5, 5));
    let result = node
        .evaluate(&input(image()), &Parameters::new(), &context)
        .unwrap();
    let Value::Image(actual) = &result.outputs["image"] else {
        panic!("channel mixer must emit an image");
    };
    assert_eq!(actual.global_region(), Region::new(100, 200, 0, 0));
    assert!(actual.pixels().is_empty());
}

#[test]
fn tone_mapping_compresses_highlights_without_parameter_overflow() {
    let node = registry().instantiate("pro.tone-map").unwrap();
    let source =
        Image::from_pixels(2, 1, vec![[4.0, 2.0, 1.0, 1.0], [0.25, 0.5, 0.75, 1.0]]).unwrap();
    let result = node
        .evaluate(
            &input(source),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap();
    let Value::Image(actual) = &result.outputs["image"] else {
        panic!("tone map must emit an image");
    };
    let pixel = actual.pixel(0, 0).unwrap();
    assert!(pixel[0] < 4.0 && pixel[0] > pixel[1]);
    assert!(pixel.iter().all(|value| value.is_finite()));
}

#[test]
fn clipping_analysis_emits_connectable_statistics_and_a_spatial_mask() {
    let node = registry().instantiate("pro.clipping-analysis").unwrap();
    let source =
        Image::from_pixels(2, 1, vec![[-0.1, 0.5, 1.2, 1.0], [0.2, 0.3, 0.4, 1.0]]).unwrap();
    let result = node
        .evaluate(
            &input(source),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap();
    assert!(matches!(result.outputs["low_clipped"], Value::Float(value) if value > 0.0));
    assert!(matches!(result.outputs["high_clipped"], Value::Float(value) if value > 0.0));
    let Value::Mask(mask) = &result.outputs["mask"] else {
        panic!("clipping analysis must emit a mask");
    };
    assert_eq!(mask.values(), vec![1.0, 0.0]);
}

#[test]
fn lut_rejects_unbounded_control_point_payloads() {
    let node = registry().instantiate("pro.lut").unwrap();
    let points = (0..5000)
        .map(|index| format!("{index},{}", index))
        .collect::<Vec<_>>()
        .join(";");
    let parameters = [("points".to_owned(), points.into())]
        .into_iter()
        .collect::<Parameters>();
    let result = node.evaluate(&input(image()), &parameters, &EvaluationContext::default());
    assert!(result.is_err());
}
