use rawweave_color::{PreviewSampling, SceneLinearRGB, WorkingSpace};
use rawweave_image::{Dimensions, Image, Region};
use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    rawweave_pro_tools::register_nodes(&mut registry).unwrap();
    registry
}
fn source() -> SceneLinearRGB {
    SceneLinearRGB::new(
        Dimensions::new(3, 2),
        vec![
            [-0.2, 2.0, 0.4],
            [3.0, -0.1, 0.8],
            [0.5, 0.7, 4.0],
            [2.0, 1.5, 0.2],
            [0.0, 3.0, 0.1],
            [0.8, 0.2, 2.5],
        ],
        WorkingSpace::Srgb,
    )
    .unwrap()
}

#[test]
fn every_pro_alias_accepts_typed_scenes_and_preserves_the_image_contract() {
    let registry = registry();
    let scene = source();
    for descriptor in registry.descriptors() {
        assert_eq!(
            descriptor.input("scene").unwrap().data_type,
            "color.SceneLinearRGB",
            "{}",
            descriptor.type_id
        );
        assert_eq!(descriptor.input("image").unwrap().data_type, "core.Image");
        let node = registry.instantiate(&descriptor.type_id).unwrap();
        let parameters = descriptor
            .parameters
            .iter()
            .map(|parameter| (parameter.id.clone(), parameter.default.clone()))
            .collect::<Parameters>();
        let inputs = [("scene".into(), Value::SceneLinearRGB(scene.clone()))].into();
        let result = node
            .evaluate(
                &inputs,
                &parameters,
                &EvaluationContext::default().with_requested_region(Region::new(1, 0, 1, 1)),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", descriptor.type_id));
        for (id, value) in result.outputs {
            assert!(
                descriptor.output(&id).is_some(),
                "{}:{id}",
                descriptor.type_id
            );
            if let Value::SceneLinearRGB(output) = value {
                assert_eq!(
                    output.dimensions(),
                    scene.dimensions(),
                    "{}",
                    descriptor.type_id
                );
                assert_eq!(
                    output.working_space(),
                    scene.working_space(),
                    "{}",
                    descriptor.type_id
                );
                assert!(
                    output
                        .pixels()
                        .iter()
                        .flatten()
                        .all(|value| value.is_finite())
                );
            }
        }
    }
}

#[test]
fn shared_scene_kernels_match_existing_image_kernels_without_input_layout_copies() {
    let registry = registry();
    let scene = source();
    let image = Image::from_pixels(
        3,
        2,
        scene
            .pixels()
            .iter()
            .map(|[r, g, b]| [*r, *g, *b, 1.0])
            .collect(),
    )
    .unwrap();
    for descriptor in registry.descriptors() {
        // Bounded curves use an explicit unbounded extension for scene inputs.
        if matches!(
            descriptor.type_id.as_str(),
            "pro.lut"
                | "pro.lut-tools"
                | "pro.film-curve"
                | "pro.film-simulation"
                | "pro.tone-map"
                | "pro.advanced-tone-map"
        ) {
            continue;
        }
        let node = registry.instantiate(&descriptor.type_id).unwrap();
        let parameters = descriptor
            .parameters
            .iter()
            .map(|parameter| (parameter.id.clone(), parameter.default.clone()))
            .collect::<Parameters>();
        let ordinary = node
            .evaluate(
                &[("image".into(), Value::Image(image.clone()))].into(),
                &parameters,
                &EvaluationContext::default(),
            )
            .unwrap();
        let linear = node
            .evaluate(
                &[("scene".into(), Value::SceneLinearRGB(scene.clone()))].into(),
                &parameters,
                &EvaluationContext::default(),
            )
            .unwrap();
        for (id, value) in ordinary.outputs {
            if let Value::Image(image) = value {
                let scene_id = if id == "image" {
                    "scene".into()
                } else {
                    format!("{id}_scene")
                };
                let Value::SceneLinearRGB(output) = &linear.outputs[&scene_id] else {
                    panic!()
                };
                for (rgba, rgb) in image.pixels().iter().zip(output.pixels()) {
                    for channel in 0..3 {
                        assert!(
                            (rgba[channel] - rgb[channel]).abs() < 1e-5,
                            "{}",
                            descriptor.type_id
                        );
                    }
                }
            } else {
                assert_eq!(linear.outputs[&id], value, "{}:{id}", descriptor.type_id);
            }
        }
    }
}

#[test]
fn scene_curves_do_not_clip_negative_values_or_hdr_highlights() {
    let registry = registry();
    for id in [
        "pro.lut",
        "pro.lut-tools",
        "pro.film-curve",
        "pro.film-simulation",
    ] {
        let scene = source();
        let result = registry
            .instantiate(id)
            .unwrap()
            .evaluate(
                &[("scene".into(), Value::SceneLinearRGB(scene.clone()))].into(),
                &[("points".into(), "0,0;1,1".into())].into(),
                &EvaluationContext::default(),
            )
            .unwrap();
        let Value::SceneLinearRGB(output) = &result.outputs["scene"] else {
            panic!()
        };
        assert_eq!(output.pixels(), scene.pixels(), "{id}");
    }
    for operator in ["reinhard", "filmic", "aces"] {
        let scene = SceneLinearRGB::from_pixels(1, 1, vec![[-4.0, 4.0, 1.0e10]]).unwrap();
        let result = registry
            .instantiate("pro.tone-map")
            .unwrap()
            .evaluate(
                &[("scene".into(), Value::SceneLinearRGB(scene))].into(),
                &[("operator".into(), operator.into())].into(),
                &EvaluationContext::default(),
            )
            .unwrap();
        let Value::SceneLinearRGB(output) = &result.outputs["scene"] else {
            panic!()
        };
        assert!(output.pixels()[0][0] < 0.0, "{operator}");
        assert!(output.pixels()[0].iter().all(|v| v.is_finite()));
        if operator != "reinhard" {
            assert!(output.pixels()[0][2] > 1.0, "{operator}");
        }
    }
}

#[test]
fn scene_sampling_survives_pro_filters_and_clipping_masks_keep_full_bounds() {
    let scene = source()
        .with_sampling(Some(PreviewSampling {
            full_dimensions: Dimensions::new(6, 4),
            mip: 1,
        }))
        .unwrap();
    let registry = registry();
    for id in [
        "pro.sharpen",
        "pro.grain",
        "pro.distortion",
        "pro.channel-mixer",
    ] {
        let result = registry
            .instantiate(id)
            .unwrap()
            .evaluate(
                &[("scene".into(), Value::SceneLinearRGB(scene.clone()))].into(),
                &Parameters::new(),
                &EvaluationContext::default(),
            )
            .unwrap();
        let Value::SceneLinearRGB(output) = &result.outputs["scene"] else {
            panic!()
        };
        assert_eq!(output.sampling(), scene.sampling());
    }
    let result = registry
        .instantiate("pro.clipping")
        .unwrap()
        .evaluate(
            &[("scene".into(), Value::SceneLinearRGB(scene))].into(),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap();
    let Value::Mask(mask) = &result.outputs["mask"] else {
        panic!()
    };
    assert_eq!(mask.dimensions(), Dimensions::new(6, 4));
}

#[test]
fn scene_analysis_uses_declared_primaries_and_rejects_unknown_luma_definitions() {
    let registry = registry();
    let node = registry.instantiate("pro.histogram").unwrap();
    let scene = SceneLinearRGB::new(
        Dimensions::new(1, 1),
        vec![[1.0, 0.0, 0.0]],
        WorkingSpace::Rec2020,
    )
    .unwrap();
    let result = node
        .evaluate(
            &[("scene".into(), Value::SceneLinearRGB(scene.clone()))].into(),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap();
    assert!(matches!(result.outputs["mean"], Value::Float(value) if (value-0.2627002).abs()<1e-6));
    for space in [
        WorkingSpace::CameraNative,
        WorkingSpace::Custom("unknown".into()),
    ] {
        let inputs = [(
            "scene".into(),
            Value::SceneLinearRGB(scene.with_working_space(space)),
        )]
        .into();
        assert!(
            node.evaluate(&inputs, &Parameters::new(), &EvaluationContext::default())
                .is_err()
        );
        // Geometry/pointwise operations need no guessed primaries.
        assert!(
            registry
                .instantiate("pro.grain")
                .unwrap()
                .evaluate(&inputs, &Parameters::new(), &EvaluationContext::default())
                .is_ok()
        );
    }
}

#[test]
fn scene_hue_zones_retain_signed_and_tiny_values_at_neutral_settings() {
    let scene =
        SceneLinearRGB::from_pixels(2, 1, vec![[-4.0, -2.0, -3.0], [-1.0e-9, 2.0e-9, 1.0e-9]])
            .unwrap();
    let result = registry()
        .instantiate("pro.color-zones")
        .unwrap()
        .evaluate(
            &[("scene".into(), Value::SceneLinearRGB(scene.clone()))].into(),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap();
    let Value::SceneLinearRGB(output) = &result.outputs["scene"] else {
        panic!()
    };
    for (input, output) in scene.pixels().iter().zip(output.pixels()) {
        for channel in 0..3 {
            assert!(
                (input[channel] - output[channel]).abs() < 1e-6 * input[channel].abs().max(1e-9)
            );
        }
    }
}

#[test]
fn scene_input_errors_are_typed_and_do_not_fall_back_to_an_image() {
    let node = registry().instantiate("pro.sharpen").unwrap();
    for inputs in [
        [("scene".into(), Value::Float(1.0))].into(),
        [
            ("scene".into(), Value::SceneLinearRGB(source())),
            ("image".into(), Value::Image(Image::new(1, 1).unwrap())),
        ]
        .into(),
    ] {
        assert!(
            node.evaluate(&inputs, &Parameters::new(), &EvaluationContext::default())
                .is_err()
        );
    }
    let inputs: Inputs = [("scene".into(), Value::SceneLinearRGB(source()))].into();
    assert!(
        node.evaluate(
            &inputs,
            &[("radius".into(), 100000_i64.into())].into(),
            &EvaluationContext::default()
        )
        .is_err()
    );
}
