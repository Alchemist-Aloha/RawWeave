use rawweave_core_image::register_nodes;
use rawweave_image::Image;
use rawweave_node_api::{
    EvaluationContext, ImageSet, ImageSetMember, ImageSetOrder, Inputs, Metadata, NodeRegistry,
    Parameters, Value,
};

fn image(value: f32) -> Image {
    Image::from_pixels(2, 1, vec![[value, value, value, 1.0]; 2]).unwrap()
}

fn set() -> ImageSet {
    ImageSet::new(
        ImageSetOrder::Ordered,
        vec![
            ImageSetMember::new("a", image(1.0), Metadata::default()),
            ImageSetMember::new("b", image(3.0), Metadata::default()),
        ],
    )
    .unwrap()
}

fn evaluate(
    registry: &NodeRegistry,
    type_id: &str,
    inputs: Inputs,
    parameters: Parameters,
    context: EvaluationContext,
) -> rawweave_node_api::NodeResult {
    registry
        .instantiate(type_id)
        .unwrap()
        .evaluate(&inputs, &parameters, &context)
        .unwrap()
}

#[test]
fn imageset_nodes_are_registered_and_hdr_merge_is_collection_level() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    for type_id in [
        "core.imageset-input",
        "core.alignment",
        "core.exposure-set",
        "core.hdr-merge",
        "core.focus-stack",
        "core.imageset-select",
        "core.imageset-filter",
        "core.imageset-map",
        "core.imageset-group",
    ] {
        assert!(registry.descriptor(type_id).is_some(), "missing {type_id}");
    }

    let result = evaluate(
        &registry,
        "core.hdr-merge",
        [("images".into(), Value::ImageSet(set()))]
            .into_iter()
            .collect(),
        Parameters::new(),
        EvaluationContext::default(),
    );
    assert_eq!(
        result.outputs["image"],
        Value::Image(image(2.0)),
        "HDR should combine all members instead of evaluating one image repeatedly"
    );
}

#[test]
fn focus_stack_and_select_report_member_identity_on_failure() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    let selected = evaluate(
        &registry,
        "core.imageset-select",
        [("images".into(), Value::ImageSet(set()))]
            .into_iter()
            .collect(),
        [("index".into(), 1_i64.into())].into_iter().collect(),
        EvaluationContext::default(),
    );
    assert_eq!(selected.outputs["member_id"], Value::String("b".into()));

    let invalid = registry.instantiate("core.hdr-merge").unwrap().evaluate(
        &[("images".into(), Value::ImageSet(set()))]
            .into_iter()
            .collect(),
        &Parameters::new(),
        &EvaluationContext::default().with_parameter_override("images", "unused", 0_i64),
    );
    assert!(invalid.is_ok());
}
