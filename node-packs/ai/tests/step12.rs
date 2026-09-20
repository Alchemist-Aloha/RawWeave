use rawweave_ai_nodes::register_nodes;
use rawweave_image::{Dimensions, Image, Region};
use rawweave_node_api::{
    EvaluationContext, EvaluationPolicy, Inputs, NodeRegistry, Parameters, Value,
};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    registry
}

#[test]
fn registers_step12_ai_spatial_nodes_with_normal_graph_data_outputs() {
    let registry = registry();
    for type_id in [
        "ai.subject-segmentation",
        "ai.semantic-segmentation",
        "ai.prompt-segmentation",
        "ai.face-detection",
        "ai.skin-mask",
        "ai.sky-mask",
        "ai.foreground-mask",
        "ai.depth-estimation",
        "ai.scene-analysis",
    ] {
        assert!(registry.descriptor(type_id).is_some(), "missing {type_id}");
    }

    let semantic = registry.descriptor("ai.semantic-segmentation").unwrap();
    assert_eq!(
        semantic.output("label_map").unwrap().data_type,
        "core.LabelMap"
    );
    assert_eq!(
        semantic.output("confidence").unwrap().data_type,
        "core.ConfidenceMap"
    );
    assert_eq!(
        semantic.evaluation_policy,
        EvaluationPolicy::ManualCheckpoint
    );

    let prompt = registry.descriptor("ai.prompt-segmentation").unwrap();
    assert_eq!(prompt.output("mask").unwrap().data_type, "core.Mask");
    assert_eq!(prompt.input("prompt").unwrap().data_type, "value.String");
    assert_eq!(prompt.evaluation_policy, EvaluationPolicy::ManualCheckpoint);

    let depth = registry.descriptor("ai.depth-estimation").unwrap();
    assert_eq!(depth.output("depth").unwrap().data_type, "core.DepthMap");
    assert_eq!(depth.evaluation_policy, EvaluationPolicy::Automatic);
}

#[test]
fn automatic_ai_masks_and_depth_feed_ordinary_graph_values() {
    let image = Image::from_pixels_with_origin(
        Dimensions::new(2, 1),
        (8, 12),
        vec![[0.8, 0.45, 0.3, 1.0], [0.1, 0.2, 0.8, 1.0]],
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let context = EvaluationContext::with_source_image(image.clone())
        .with_requested_region(Region::new(8, 12, 2, 1));
    let inputs = [(String::from("image"), Value::Image(image))]
        .into_iter()
        .collect::<Inputs>();

    let skin = registry().instantiate("ai.skin-mask").unwrap();
    let skin_result = skin
        .evaluate(&inputs, &Parameters::new(), &context)
        .unwrap();
    let Value::Mask(skin_mask) = &skin_result.outputs["mask"] else {
        panic!("skin node must emit a core.Mask");
    };
    assert!(skin_mask.pixel_global(8, 12).unwrap() > skin_mask.pixel_global(9, 12).unwrap());

    let depth = registry().instantiate("ai.depth-estimation").unwrap();
    let depth_result = depth
        .evaluate(&inputs, &Parameters::new(), &context)
        .unwrap();
    let Value::DepthMap(depth_map) = &depth_result.outputs["depth"] else {
        panic!("depth node must emit a core.DepthMap");
    };
    assert_eq!(depth_map.origin(), (8, 12));
    assert_eq!(depth_map.values().len(), 2);
}
