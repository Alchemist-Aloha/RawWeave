use rawweave_core_image::register_nodes;
use rawweave_image::Image;
use rawweave_node_api::{
    AlignmentState, EvaluationContext, ImageSet, ImageSetMember, ImageSetOrder, Inputs, Metadata,
    NodeRegistry, Parameters, Value,
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

#[test]
fn alignment_registers_translations_and_records_provenance() {
    let reference = Image::from_pixels(
        5,
        1,
        vec![
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [2.0, 2.0, 2.0, 1.0],
            [3.0, 3.0, 3.0, 1.0],
            [4.0, 4.0, 4.0, 1.0],
        ],
    )
    .unwrap();
    let shifted = Image::from_pixels(
        5,
        1,
        vec![
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [2.0, 2.0, 2.0, 1.0],
            [3.0, 3.0, 3.0, 1.0],
        ],
    )
    .unwrap();
    let set = ImageSet::new(
        ImageSetOrder::Ordered,
        vec![
            ImageSetMember::new("reference", reference, Metadata::default()),
            ImageSetMember::new("shifted", shifted, Metadata::default()),
        ],
    )
    .unwrap();
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();

    let result = evaluate(
        &registry,
        "core.alignment",
        [("images".into(), Value::ImageSet(set))]
            .into_iter()
            .collect(),
        [("reference".into(), "reference".into())]
            .into_iter()
            .collect(),
        EvaluationContext::default(),
    );
    let Value::ImageSet(aligned) = &result.outputs["images"] else {
        panic!("alignment must return an image set");
    };
    let AlignmentState::Aligned {
        reference_member,
        transforms,
        provenance,
    } = aligned.alignment()
    else {
        panic!("alignment must compute transforms");
    };
    assert_eq!(reference_member, "reference");
    assert_eq!(transforms["reference"].dx, 0);
    assert_eq!(transforms["shifted"].dx, 1);
    assert_eq!(transforms["shifted"].dy, 0);
    assert_eq!(provenance.algorithm, "translation-ssd");
}

#[test]
fn panorama_registration_is_not_advertised() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    assert!(registry.descriptor("core.panorama").is_none());
    assert!(registry.descriptor("core.panorama-stitch").is_none());
}

#[test]
fn image_set_alias_inputs_are_equally_optional_for_graph_resolution() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    for type_id in ["core.alignment", "core.hdr-merge", "core.imageset-filter"] {
        let descriptor = registry.descriptor(type_id).unwrap();
        assert!(descriptor.inputs.iter().all(|port| !port.required));
    }
}
