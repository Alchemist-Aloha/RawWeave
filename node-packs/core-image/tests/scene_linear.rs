use rawweave_color::{PreviewSampling, SceneLinearRGB, WorkingSpace};
use rawweave_image::{ColorDomain, Dimensions, Mask, Region};
use rawweave_node_api::{EvaluationContext, Inputs, NodeRegistry, Parameters, Value};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    rawweave_core_image::register_nodes(&mut registry).unwrap();
    registry
}

fn scene() -> SceneLinearRGB {
    SceneLinearRGB::new(
        Dimensions::new(2, 1),
        vec![[-0.25, 2.0, 0.5], [0.75, 4.0, 1.5]],
        WorkingSpace::Rec2020,
    )
    .unwrap()
}

fn evaluate(id: &str, source: SceneLinearRGB, extras: Inputs, parameters: Parameters) -> Value {
    let mut inputs = extras;
    inputs.insert("scene".into(), Value::SceneLinearRGB(source));
    let result = registry()
        .instantiate(id)
        .unwrap()
        .evaluate(
            &inputs,
            &parameters,
            &EvaluationContext::default().with_requested_region(Region::new(1, 0, 1, 1)),
        )
        .unwrap();
    result.outputs[if id == "core.scene-linear-to-image" {
        "image"
    } else {
        "scene"
    }]
    .clone()
}

#[test]
fn levels_curves_and_invert_have_unbounded_scene_paths() {
    for (id, expected) in [
        ("core.levels", scene().pixels().to_vec()),
        ("core.curves", scene().pixels().to_vec()),
        (
            "core.invert",
            scene()
                .pixels()
                .iter()
                .map(|pixel| pixel.map(|channel| 1.0 - channel))
                .collect(),
        ),
    ] {
        let Value::SceneLinearRGB(result) = evaluate(id, scene(), Inputs::new(), Parameters::new())
        else {
            panic!()
        };
        assert_eq!(result.pixels(), expected, "{id}");
    }
    let Value::SceneLinearRGB(curved) = evaluate(
        "core.curves",
        scene(),
        Inputs::new(),
        [("gamma".into(), 2.0.into())].into(),
    ) else {
        panic!()
    };
    assert_eq!(curved.pixels()[0][0], -0.5);
    assert!(curved.pixels()[0][1] > 1.0);
    let Value::SceneLinearRGB(levels) = evaluate(
        "core.levels",
        scene(),
        Inputs::new(),
        [
            ("black_point".into(), 0.5.into()),
            ("white_point".into(), 1.5.into()),
        ]
        .into(),
    ) else {
        panic!()
    };
    assert_eq!(levels.pixels()[0], [-0.75, 1.5, 0.0]);
}

#[test]
fn scene_mask_sources_use_the_full_sampling_grid_and_declared_primaries() {
    let registry = registry();
    let source = SceneLinearRGB::new(
        Dimensions::new(1, 1),
        vec![[1.0, 0.0, 0.0]],
        WorkingSpace::Rec2020,
    )
    .unwrap()
    .with_sampling(Some(PreviewSampling {
        full_dimensions: Dimensions::new(2, 2),
        mip: 1,
    }))
    .unwrap();
    for id in [
        "core.mask-luminance",
        "core.mask-color-qualifier",
        "core.mask-linear-gradient",
        "core.mask-radial-gradient",
        "core.mask-painted",
    ] {
        let descriptor = registry.descriptor(id).unwrap();
        assert_eq!(
            descriptor.input("scene").unwrap().data_type,
            "color.SceneLinearRGB"
        );
        let inputs = [("scene".into(), Value::SceneLinearRGB(source.clone()))].into();
        let result = registry
            .instantiate(id)
            .unwrap()
            .evaluate(&inputs, &Parameters::new(), &EvaluationContext::default())
            .unwrap();
        let Value::Mask(mask) = &result.outputs["mask"] else {
            panic!()
        };
        assert_eq!(mask.dimensions(), Dimensions::new(2, 2), "{id}");
        if id == "core.mask-luminance" {
            assert!((mask.pixel_global(1, 1).unwrap() - 0.2627002).abs() < 1e-6);
        }
    }
}

#[test]
fn compatible_nodes_preserve_scene_type_space_and_unclipped_values() {
    for (id, parameters, expected) in [
        (
            "core.exposure",
            [("exposure".into(), 1.0.into())].into(),
            vec![[-0.5, 4.0, 1.0], [1.5, 8.0, 3.0]],
        ),
        (
            "core.local-exposure",
            [("exposure".into(), 0.0.into())].into(),
            scene().pixels().to_vec(),
        ),
        (
            "core.blur",
            [("radius".into(), 0.0.into())].into(),
            scene().pixels().to_vec(),
        ),
        (
            "core.resize",
            [("width".into(), 2.0.into()), ("height".into(), 1.0.into())].into(),
            scene().pixels().to_vec(),
        ),
        (
            "core.color-matrix",
            Parameters::new(),
            scene().pixels().to_vec(),
        ),
        ("core.output", Parameters::new(), scene().pixels().to_vec()),
    ] {
        let descriptor = registry().descriptor(id).unwrap().clone();
        assert_eq!(
            descriptor.input("scene").unwrap().data_type,
            "color.SceneLinearRGB"
        );
        assert_eq!(descriptor.output("image").unwrap().data_type, "core.Image");
        let Value::SceneLinearRGB(result) = evaluate(id, scene(), Inputs::new(), parameters) else {
            panic!("{id}")
        };
        assert_eq!(result.pixels(), expected, "{id}");
        assert_eq!(result.working_space(), WorkingSpace::Rec2020);
        assert_eq!(result.dimensions(), Dimensions::new(2, 1));
    }
}

#[test]
fn scene_blur_resize_and_rgb_matrix_have_defined_linear_results() {
    let Value::SceneLinearRGB(blur) = evaluate(
        "core.blur",
        scene(),
        Inputs::new(),
        [("radius".into(), 1.0.into())].into(),
    ) else {
        panic!()
    };
    assert!((blur.pixels()[0][1] - 8.0 / 3.0).abs() < 1e-6);
    let Value::SceneLinearRGB(resized) = evaluate(
        "core.resize",
        scene(),
        Inputs::new(),
        [("width".into(), 4.0.into()), ("height".into(), 1.0.into())].into(),
    ) else {
        panic!()
    };
    assert_eq!(
        resized.pixels(),
        &[
            scene().pixels()[0],
            scene().pixels()[0],
            scene().pixels()[1],
            scene().pixels()[1]
        ]
    );
    let Value::SceneLinearRGB(matrix) = evaluate(
        "core.color-matrix",
        scene(),
        Inputs::new(),
        [("m00".into(), 2.0.into()), ("offset_g".into(), 1.0.into())].into(),
    ) else {
        panic!()
    };
    assert_eq!(matrix.pixels()[0], [-0.5, 3.0, 0.5]);
}

#[test]
fn sampled_scenes_keep_sampling_and_local_masks_use_full_frame_coordinates() {
    let source = SceneLinearRGB::from_pixels(2, 1, vec![[2.0; 3]; 2])
        .unwrap()
        .with_sampling(Some(PreviewSampling {
            full_dimensions: Dimensions::new(4, 2),
            mip: 1,
        }))
        .unwrap();
    let mask = Mask::from_values_with_origin(Dimensions::new(1, 1), (2, 0), vec![1.0]).unwrap();
    let Value::SceneLinearRGB(result) = evaluate(
        "core.local-exposure",
        source.clone(),
        [("mask".into(), Value::Mask(mask))].into(),
        [("exposure".into(), 1.0.into())].into(),
    ) else {
        panic!()
    };
    assert_eq!(result.sampling(), source.sampling());
    assert_eq!(result.pixels(), &[[2.0; 3], [4.0; 3]]);
    for id in [
        "core.exposure",
        "core.blur",
        "core.color-matrix",
        "core.output",
    ] {
        let Value::SceneLinearRGB(result) = evaluate(
            id,
            source.clone(),
            Inputs::new(),
            [
                ("exposure".into(), 0.0.into()),
                ("radius".into(), 0.0.into()),
            ]
            .into(),
        ) else {
            panic!()
        };
        assert_eq!(result.sampling(), source.sampling(), "{id}");
    }
}

#[test]
fn conversion_adds_opaque_alpha_and_declares_linear_srgb_without_display_encoding() {
    let source = SceneLinearRGB::from_pixels(1, 1, vec![[-0.25, 2.0, 0.5]]).unwrap();
    let Value::Image(image) = evaluate(
        "core.scene-linear-to-image",
        source,
        Inputs::new(),
        Parameters::new(),
    ) else {
        panic!()
    };
    assert_eq!(image.pixel(0, 0), Some([-0.25, 2.0, 0.5, 1.0]));
    assert_eq!(image.color_metadata().domain, ColorDomain::LinearSrgb);
    assert!(!image.color_metadata().alpha_is_premultiplied);
}

#[test]
fn conversion_transforms_known_primaries_and_rejects_unknown_or_sampled_sources() {
    use rawweave_color::{MatrixWorkingSpaceTransform, SceneTransform};
    let source = scene();
    let expected = MatrixWorkingSpaceTransform::new(WorkingSpace::Srgb)
        .transform(&source)
        .unwrap();
    let Value::Image(image) = evaluate(
        "core.scene-linear-to-image",
        source,
        Inputs::new(),
        Parameters::new(),
    ) else {
        panic!()
    };
    assert_eq!(image.pixel(0, 0).unwrap()[..3], expected.pixels()[0]);
    let converter = registry()
        .instantiate("core.scene-linear-to-image")
        .unwrap();
    let sampled = SceneLinearRGB::from_pixels(2, 1, vec![[1.0; 3]; 2])
        .unwrap()
        .with_sampling(Some(PreviewSampling {
            full_dimensions: Dimensions::new(4, 2),
            mip: 1,
        }))
        .unwrap();
    for source in [
        scene().with_working_space(WorkingSpace::CameraNative),
        scene().with_working_space(WorkingSpace::Custom("unknown".into())),
        sampled,
    ] {
        assert!(
            converter
                .evaluate(
                    &[("scene".into(), Value::SceneLinearRGB(source))].into(),
                    &Parameters::new(),
                    &EvaluationContext::default()
                )
                .is_err()
        );
    }
}

#[test]
fn scene_paths_reject_ambiguous_inputs_alpha_loss_nonfinite_results_and_bad_parameters() {
    let registry = registry();
    for (id, parameters) in [
        ("core.exposure", [("exposure".into(), 1000.0.into())].into()),
        (
            "core.resize",
            [
                ("width".into(), 100000.0.into()),
                ("height".into(), 100000.0.into()),
            ]
            .into(),
        ),
        ("core.blur", [("radius".into(), 65.0.into())].into()),
        (
            "core.levels",
            [
                ("black_point".into(), (-f32::MAX).into()),
                ("white_point".into(), f32::MAX.into()),
            ]
            .into(),
        ),
        ("core.levels", [("gamma".into(), 0.0.into())].into()),
        ("core.curves", [("gamma".into(), f32::NAN.into())].into()),
        (
            "core.color-matrix",
            [("offset_a".into(), 1.0.into())].into(),
        ),
    ] {
        let inputs = [("scene".into(), Value::SceneLinearRGB(scene()))].into();
        assert!(
            registry
                .instantiate(id)
                .unwrap()
                .evaluate(&inputs, &parameters, &EvaluationContext::default())
                .is_err(),
            "{id}"
        );
    }
    let inputs = [
        ("scene".into(), Value::SceneLinearRGB(scene())),
        (
            "image".into(),
            Value::Image(rawweave_image::Image::from_pixels(1, 1, vec![[0.0; 4]]).unwrap()),
        ),
    ]
    .into();
    assert!(
        registry
            .instantiate("core.exposure")
            .unwrap()
            .evaluate(
                &inputs,
                &[("exposure".into(), 0.0.into())].into(),
                &EvaluationContext::default()
            )
            .is_err()
    );
}
