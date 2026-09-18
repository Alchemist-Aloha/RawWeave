use rawweave_image::{Image, Region};
use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};
use rawweave_rendering::{GpuContext, RenderContext};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    rawweave_core_image::register_nodes(&mut registry).unwrap();
    registry
}

fn source() -> Image {
    Image::from_pixels(
        3,
        2,
        vec![
            [0.0, 0.0, 0.0, 1.0],
            [0.5, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.5, 0.0, 1.0],
            [0.5, 0.5, 0.0, 1.0],
            [1.0, 0.5, 0.0, 1.0],
        ],
    )
    .unwrap()
}

fn image_input(image: Image) -> Inputs {
    [("image".to_owned(), Value::Image(image))]
        .into_iter()
        .collect()
}

#[test]
fn step2_nodes_are_registered_with_execution_capabilities() {
    let registry = registry();
    for type_id in [
        "core.resize",
        "core.crop",
        "core.blur",
        "core.levels",
        "core.curves",
        "core.color-matrix",
    ] {
        let descriptor = registry
            .descriptor(type_id)
            .unwrap_or_else(|| panic!("{type_id}"));
        assert!(
            descriptor
                .capabilities
                .contains(&rawweave_node_api::ExecutionCapability::Cpu)
        );
        assert!(
            descriptor
                .capabilities
                .contains(&rawweave_node_api::ExecutionCapability::RegionAware)
        );
    }
}

#[test]
fn crop_and_resize_produce_expected_pixels_and_honor_regions() {
    let crop = registry().instantiate("core.crop").unwrap();
    let crop_parameters = [
        ("x".to_owned(), 1.0.into()),
        ("y".to_owned(), 0.0.into()),
        ("width".to_owned(), 2.0.into()),
        ("height".to_owned(), 2.0.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let crop_result = crop
        .evaluate(
            &image_input(source()),
            &crop_parameters,
            &EvaluationContext::default(),
        )
        .unwrap();
    let Value::Image(cropped) = crop_result.outputs["image"].clone() else {
        panic!("expected image")
    };
    assert_eq!(cropped.dimensions().width, 2);
    assert_eq!(cropped.pixel(0, 0), Some([0.5, 0.0, 0.0, 1.0]));

    let resize = registry().instantiate("core.resize").unwrap();
    let resize_parameters = [
        ("width".to_owned(), 6.0.into()),
        ("height".to_owned(), 4.0.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let context = EvaluationContext::default().with_requested_region(Region::new(2, 1, 2, 2));
    let resize_result = resize
        .evaluate(&image_input(source()), &resize_parameters, &context)
        .unwrap();
    let Value::Image(resized) = resize_result.outputs["image"].clone() else {
        panic!("expected image")
    };
    assert_eq!(resized.dimensions(), rawweave_image::Dimensions::new(2, 2));
    assert_eq!(resized.pixel(0, 0), Some([0.5, 0.0, 0.0, 1.0]));
}

#[test]
fn requested_regions_keep_global_coordinates_through_chained_operations() {
    let crop = registry().instantiate("core.crop").unwrap();
    let crop_parameters = [
        ("x".to_owned(), 1.0.into()),
        ("y".to_owned(), 0.0.into()),
        ("width".to_owned(), 2.0.into()),
        ("height".to_owned(), 2.0.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let cropped = crop
        .evaluate(
            &image_input(source()),
            &crop_parameters,
            &EvaluationContext::default().with_requested_region(Region::new(2, 1, 1, 1)),
        )
        .unwrap();
    let Value::Image(cropped) = cropped.outputs["image"].clone() else {
        panic!("expected image")
    };
    assert_eq!(cropped.global_region(), Region::new(2, 1, 1, 1));
    assert_eq!(cropped.pixel(0, 0), Some([1.0, 0.5, 0.0, 1.0]));

    let invert = registry().instantiate("core.invert").unwrap();
    let inverted = invert
        .evaluate(
            &image_input(cropped),
            &Parameters::new(),
            &EvaluationContext::default().with_requested_region(Region::new(2, 1, 1, 1)),
        )
        .unwrap();
    let Value::Image(inverted) = inverted.outputs["image"].clone() else {
        panic!("expected image")
    };
    assert_eq!(inverted.global_region(), Region::new(2, 1, 1, 1));
    assert_eq!(inverted.pixel(0, 0), Some([0.0, 0.5, 1.0, 1.0]));
}

#[test]
fn tile_requests_reach_nodes_and_preserve_mip_quality() {
    let request = rawweave_rendering::TileRequest::new(
        Region::new(1, 1, 1, 1),
        rawweave_rendering::TileCoord::new(3, 4),
        2,
        rawweave_rendering::PreviewQuality::Draft,
    );
    let context = EvaluationContext::default().with_tile_request(request);
    assert_eq!(context.requested_region(), Some(request.region));
    assert_eq!(context.tile(), request.tile);
    assert_eq!(context.mip_level(), request.mip_level);
    assert_eq!(context.quality(), request.quality);
}

#[test]
fn full_frame_capability_is_used_without_region_requests() {
    let registry = registry();
    let descriptor = registry.descriptor("core.blur").unwrap();
    assert_eq!(
        descriptor.select_capability(&EvaluationContext::default()),
        Some(rawweave_node_api::ExecutionCapability::FullFrame)
    );
    assert_eq!(
        descriptor.select_capability(
            &EvaluationContext::default().with_requested_region(Region::new(1, 1, 1, 1))
        ),
        Some(rawweave_node_api::ExecutionCapability::RegionAware)
    );
}

#[test]
fn blur_samples_nonzero_origin_images_using_global_coordinates() {
    let blur = registry().instantiate("core.blur").unwrap();
    let source = Image::from_pixels_with_origin(
        rawweave_image::Dimensions::new(2, 1),
        (10, 20),
        vec![[0.25, 0.5, 0.75, 1.0], [0.5, 0.25, 0.0, 1.0]],
        rawweave_image::PixelFormat::default(),
        rawweave_image::ColorMetadata::default(),
    )
    .unwrap();
    let parameters = [("radius".to_owned(), 0.0.into())]
        .into_iter()
        .collect::<Parameters>();

    let result = blur
        .evaluate(
            &image_input(source.clone()),
            &parameters,
            &EvaluationContext::default(),
        )
        .unwrap();
    let Value::Image(actual) = result.outputs["image"].clone() else {
        panic!("expected image")
    };

    assert_eq!(actual.global_region(), source.global_region());
    assert_eq!(actual.pixels(), source.pixels());
}

#[test]
fn levels_curves_and_color_matrix_are_deterministic() {
    let inputs = image_input(source());
    let parameters = [
        ("black_point".to_owned(), 0.0.into()),
        ("white_point".to_owned(), 1.0.into()),
        ("gamma".to_owned(), 2.0.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let levels = registry().instantiate("core.levels").unwrap();
    let first = levels
        .evaluate(&inputs, &parameters, &EvaluationContext::default())
        .unwrap();
    let second = levels
        .evaluate(&inputs, &parameters, &EvaluationContext::default())
        .unwrap();
    assert_eq!(first, second);

    let curves = registry().instantiate("core.curves").unwrap();
    let curve_parameters = [("gamma".to_owned(), 0.5.into())]
        .into_iter()
        .collect::<Parameters>();
    let curves_result = curves
        .evaluate(&inputs, &curve_parameters, &EvaluationContext::default())
        .unwrap();
    assert_ne!(curves_result, first);

    let matrix = registry().instantiate("core.color-matrix").unwrap();
    let identity = matrix
        .evaluate(&inputs, &Parameters::new(), &EvaluationContext::default())
        .unwrap();
    assert_eq!(identity, NodeResultLike::from_image(source()));
}

#[test]
fn color_matrix_uses_gpu_when_the_context_provides_one() {
    let Some(gpu) = GpuContext::initialize_or_cpu() else {
        return;
    };
    let matrix = registry().instantiate("core.color-matrix").unwrap();
    let parameters = [
        ("m00".to_owned(), 0.5.into()),
        ("m11".to_owned(), 0.75.into()),
        ("offset_b".to_owned(), 0.1.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let context =
        EvaluationContext::default().with_render_context(RenderContext::new().with_gpu(gpu));
    let result = matrix
        .evaluate(&image_input(source()), &parameters, &context)
        .unwrap();
    let Value::Image(actual) = result.outputs["image"].clone() else {
        panic!("expected image")
    };
    let expected = source()
        .map_pixels(|[red, green, blue, alpha]| [0.5 * red, 0.75 * green, blue + 0.1, alpha]);
    for (actual, expected) in actual.pixels().iter().zip(expected.pixels()) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() <= 1e-5, "{actual} != {expected}");
        }
    }
}

struct NodeResultLike;
impl NodeResultLike {
    fn from_image(image: Image) -> rawweave_node_api::NodeResult {
        rawweave_node_api::NodeResult::single("image", Value::Image(image))
    }
}
