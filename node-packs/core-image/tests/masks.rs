use rawweave_image::{Dimensions, Image, Mask, Region};
use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    rawweave_core_image::register_nodes(&mut registry).unwrap();
    registry
}

fn image_input(image: Image) -> Inputs {
    [("image".to_owned(), Value::Image(image))]
        .into_iter()
        .collect()
}

fn mask_input(name: &str, mask: Mask) -> Inputs {
    [(name.to_owned(), Value::Mask(mask))].into_iter().collect()
}

fn mask_output(result: rawweave_node_api::NodeResult) -> Mask {
    match result.outputs.get("mask") {
        Some(Value::Mask(mask)) => mask.clone(),
        other => panic!("expected mask output, got {other:?}"),
    }
}

#[test]
fn mask_pack_registers_all_spatial_nodes_and_local_exposure() {
    let registry = registry();
    for type_id in [
        "core.mask-painted",
        "core.mask-linear-gradient",
        "core.mask-radial-gradient",
        "core.mask-luminance",
        "core.mask-color-qualifier",
        "core.mask-invert",
        "core.mask-add",
        "core.mask-subtract",
        "core.mask-intersect",
        "core.mask-multiply",
        "core.mask-threshold",
        "core.mask-feather",
        "core.mask-blur",
        "core.mask-expand",
        "core.mask-contract",
        "core.local-exposure",
    ] {
        assert!(registry.descriptor(type_id).is_some(), "missing {type_id}");
    }
    let local = registry.descriptor("core.local-exposure").unwrap();
    assert_eq!(local.input("mask").unwrap().data_type, "core.Mask");
    assert!(!local.input("mask").unwrap().required);
}

#[test]
fn gradient_bounds_fail_with_the_pixel_budget_not_a_missing_dimensions_parameter() {
    // Declared bounds of 10^12 pixels cannot be honoured, and the error has to
    // say that. It used to read "parameter 'dimensions' is missing or has the
    // wrong type", which sent the reader after a parameter that the node does
    // not even declare.
    let node = registry().instantiate("core.mask-linear-gradient").unwrap();
    let parameters = [
        ("width".to_owned(), 1_000_000_i64.into()),
        ("height".to_owned(), 1_000_000_i64.into()),
    ]
    .into_iter()
    .collect::<Parameters>();

    let error = node
        .evaluate(&Inputs::new(), &parameters, &EvaluationContext::default())
        .expect_err("bounds above the pixel budget must be rejected");
    let message = error.to_string();
    assert!(
        message.contains("node budget"),
        "unexpected message: {message}"
    );
    assert!(
        !message.contains("parameter"),
        "misleading message: {message}"
    );
}

#[test]
fn gradients_and_mask_algebra_are_global_origin_aware() {
    let source = Image::from_pixels_with_origin(
        Dimensions::new(4, 2),
        (10, 20),
        vec![[0.0, 0.0, 0.0, 1.0]; 8],
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let context = EvaluationContext::with_source_image(source.clone())
        .with_requested_region(Region::new(11, 20, 2, 2));
    let gradient = registry().instantiate("core.mask-linear-gradient").unwrap();
    let params = [
        ("start_x".to_owned(), 10.0_f32.into()),
        ("start_y".to_owned(), 20.0_f32.into()),
        ("end_x".to_owned(), 13.0_f32.into()),
        ("end_y".to_owned(), 20.0_f32.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let mask = mask_output(
        gradient
            .evaluate(&Inputs::new(), &params, &context)
            .unwrap(),
    );
    assert_eq!(mask.origin(), (11, 20));
    assert!(mask.pixel_global(12, 20).unwrap() > mask.pixel_global(11, 20).unwrap());

    let invert = registry().instantiate("core.mask-invert").unwrap();
    let inverted = mask_output(
        invert
            .evaluate(
                &mask_input("mask", mask.clone()),
                &Parameters::new(),
                &context,
            )
            .unwrap(),
    );
    assert!(
        (inverted.pixel_global(11, 20).unwrap() + mask.pixel_global(11, 20).unwrap() - 1.0).abs()
            < 1e-6
    );

    let add = registry().instantiate("core.mask-add").unwrap();
    let inputs = [
        ("a".to_owned(), Value::Mask(mask.clone())),
        ("b".to_owned(), Value::Mask(inverted)),
    ]
    .into_iter()
    .collect();
    let added = mask_output(add.evaluate(&inputs, &Parameters::new(), &context).unwrap());
    assert_eq!(added.pixel_global(11, 20), Some(1.0));
}

#[test]
fn image_masks_and_masked_local_exposure_compose_through_common_values() {
    let image = Image::from_pixels_with_origin(
        Dimensions::new(2, 1),
        (5, 7),
        vec![[0.25, 0.5, 0.75, 1.0], [1.0, 0.5, 0.25, 1.0]],
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let context = EvaluationContext::with_source_image(image.clone());
    let luminance = registry().instantiate("core.mask-luminance").unwrap();
    let mask = mask_output(
        luminance
            .evaluate(&image_input(image.clone()), &Parameters::new(), &context)
            .unwrap(),
    );
    assert!(mask.pixel_global(5, 7).unwrap() < mask.pixel_global(6, 7).unwrap());

    let local = registry().instantiate("core.local-exposure").unwrap();
    let inputs = [
        ("image".to_owned(), Value::Image(image)),
        ("mask".to_owned(), Value::Mask(mask)),
    ]
    .into_iter()
    .collect();
    let parameters = [("exposure".to_owned(), 1.0_f32.into())]
        .into_iter()
        .collect::<Parameters>();
    let Value::Image(output) = local
        .evaluate(&inputs, &parameters, &context)
        .unwrap()
        .outputs["image"]
        .clone()
    else {
        panic!("expected image output");
    };
    assert!(output.pixel_global(6, 7).unwrap()[0] > output.pixel_global(5, 7).unwrap()[0]);
}

#[test]
fn blur_feather_expand_and_contract_keep_requested_region_origin() {
    let mask =
        Mask::from_values_with_origin(Dimensions::new(5, 1), (4, 9), vec![0.0, 0.0, 1.0, 0.0, 0.0])
            .unwrap();
    let context = EvaluationContext::default().with_requested_region(Region::new(5, 9, 3, 1));
    for type_id in [
        "core.mask-blur",
        "core.mask-feather",
        "core.mask-expand",
        "core.mask-contract",
    ] {
        let node = registry().instantiate(type_id).unwrap();
        let result = node
            .evaluate(
                &mask_input("mask", mask.clone()),
                &Parameters::new(),
                &context,
            )
            .unwrap();
        let output = mask_output(result);
        assert_eq!(output.origin(), (5, 9), "{type_id}");
        assert_eq!(output.dimensions(), Dimensions::new(3, 1), "{type_id}");
    }
}

#[test]
fn painted_mask_renders_every_serialized_stroke() {
    let context = EvaluationContext::default();
    let parameters = [
        ("width".to_owned(), 20_i64.into()),
        ("height".to_owned(), 1_i64.into()),
        ("size".to_owned(), 3.0_f32.into()),
        ("hardness".to_owned(), 1.0_f32.into()),
        ("opacity".to_owned(), 1.0_f32.into()),
        ("mode".to_owned(), "add".to_owned().into()),
        ("points".to_owned(), "2,0|17,0".to_owned().into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let node = registry().instantiate("core.mask-painted").unwrap();
    let mask = mask_output(
        node.evaluate(&Inputs::new(), &parameters, &context)
            .expect("multi-stroke points should evaluate"),
    );
    assert!(
        mask.pixel_global(2, 0).unwrap() > 0.0,
        "first stroke was dropped"
    );
    assert!(
        mask.pixel_global(17, 0).unwrap() > 0.0,
        "second stroke was dropped"
    );
    assert_eq!(mask.pixel_global(10, 0).unwrap(), 0.0);
}

#[test]
fn painted_mask_with_no_strokes_is_empty() {
    // Undoing the last stroke serializes `points` as an empty string. That is the
    // "nothing painted" state, and a large brush must not turn it into a blob at
    // the frame origin.
    let context = EvaluationContext::default();
    let parameters = [
        ("width".to_owned(), 20_i64.into()),
        ("height".to_owned(), 20_i64.into()),
        ("size".to_owned(), 8.0_f32.into()),
        ("hardness".to_owned(), 1.0_f32.into()),
        ("opacity".to_owned(), 1.0_f32.into()),
        ("mode".to_owned(), "add".to_owned().into()),
        ("points".to_owned(), String::new().into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let node = registry().instantiate("core.mask-painted").unwrap();
    let mask = mask_output(
        node.evaluate(&Inputs::new(), &parameters, &context)
            .expect("an empty stroke list should evaluate"),
    );
    for y in 0..20 {
        for x in 0..20 {
            assert_eq!(
                mask.pixel_global(x, y).unwrap(),
                0.0,
                "empty stroke list painted a pixel at ({x}, {y})"
            );
        }
    }
}

#[test]
fn painted_mask_without_a_points_parameter_keeps_the_legacy_brush_position() {
    // Graphs written before the multi-stroke `points` parameter must still paint
    // their single brush at x/y.
    let context = EvaluationContext::default();
    let parameters = [
        ("width".to_owned(), 20_i64.into()),
        ("height".to_owned(), 1_i64.into()),
        ("size".to_owned(), 3.0_f32.into()),
        ("hardness".to_owned(), 1.0_f32.into()),
        ("opacity".to_owned(), 1.0_f32.into()),
        ("mode".to_owned(), "add".to_owned().into()),
        ("x".to_owned(), 12.0_f32.into()),
        ("y".to_owned(), 0.0_f32.into()),
    ]
    .into_iter()
    .collect::<Parameters>();
    let node = registry().instantiate("core.mask-painted").unwrap();
    let mask = mask_output(
        node.evaluate(&Inputs::new(), &parameters, &context)
            .expect("legacy x/y parameters should evaluate"),
    );
    assert!(mask.pixel_global(12, 0).unwrap() > 0.0);
    assert_eq!(mask.pixel_global(2, 0).unwrap(), 0.0);
}
