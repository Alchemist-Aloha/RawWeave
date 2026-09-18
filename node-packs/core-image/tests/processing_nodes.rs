use rawweave_image::{Image, Region};
use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};

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

struct NodeResultLike;
impl NodeResultLike {
    fn from_image(image: Image) -> rawweave_node_api::NodeResult {
        rawweave_node_api::NodeResult::single("image", Value::Image(image))
    }
}
