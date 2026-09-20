use std::collections::BTreeMap;

use rawweave_core_image::register_nodes;
use rawweave_image::{Image, PixelFormat};
use rawweave_node_api::{
    AlignmentProvenance, AlignmentState, AlignmentTransform, EvaluationContext, ImageSet,
    ImageSetMember, ImageSetOrder, Inputs, Metadata, NodeRegistry, Parameters, Value,
};

fn image(value: f32) -> Image {
    Image::from_pixels(2, 1, vec![[value, value, value, 1.0]; 2]).unwrap()
}

fn metadata(iso: u32, aperture: f32, shutter_seconds: f32) -> Metadata {
    Metadata {
        iso: Some(iso),
        aperture: Some(aperture),
        shutter_seconds: Some(shutter_seconds),
        ..Metadata::default()
    }
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

fn aligned_set(members: Vec<ImageSetMember>) -> ImageSet {
    let ids = members
        .iter()
        .map(|member| member.id.clone())
        .collect::<Vec<_>>();
    let transforms = ids
        .iter()
        .map(|id| (id.clone(), AlignmentTransform::identity()))
        .collect::<BTreeMap<_, _>>();
    ImageSet::new(ImageSetOrder::Ordered, members)
        .unwrap()
        .with_alignment(AlignmentState::Aligned {
            reference_member: ids[0].clone(),
            transforms,
            provenance: AlignmentProvenance::new("test-alignment", 1, 0),
        })
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

    let set = aligned_set(vec![
        ImageSetMember::new("dark", image(0.25), metadata(100, 2.0, 0.01)),
        ImageSetMember::new("bright", image(1.0), metadata(400, 2.0, 0.01)),
    ]);
    let result = evaluate(
        &registry,
        "core.hdr-merge",
        [("images".into(), Value::ImageSet(set))]
            .into_iter()
            .collect(),
        Parameters::new(),
        EvaluationContext::default(),
    );
    let Value::Image(merged) = &result.outputs["image"] else {
        panic!("HDR should emit an image");
    };
    assert!((merged.pixel(0, 0).unwrap()[0] - 1.0).abs() < 0.01);
    let Value::ConfidenceMap(confidence) = &result.outputs["confidence"] else {
        panic!("HDR should expose confidence");
    };
    assert!(confidence.pixel(0, 0).unwrap() > 0.0);
    let Value::Mask(highlight_mask) = &result.outputs["highlight_mask"] else {
        panic!("HDR should expose a highlight mask");
    };
    assert!(highlight_mask.pixel(0, 0).unwrap() > 0.0);
    assert!(matches!(result.outputs["deghost_mask"], Value::Mask(_)));
    assert!(matches!(result.outputs["diagnostics"], Value::String(_)));
}

#[test]
fn hdr_merge_requires_alignment_and_reports_the_contract() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    let error = registry
        .instantiate("core.hdr-merge")
        .unwrap()
        .evaluate(
            &[("images".into(), Value::ImageSet(set()))]
                .into_iter()
                .collect(),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("aligned"), "unexpected error: {error}");
}

#[test]
fn hdr_merge_rejects_incompatible_formats_with_member_identity() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    let incompatible = Image::from_pixels_with_metadata(
        2,
        1,
        vec![[1.0, 1.0, 1.0, 1.0]; 2],
        PixelFormat::Rgba16Float,
        Default::default(),
    )
    .unwrap();
    let error = registry
        .instantiate("core.hdr-merge")
        .unwrap()
        .evaluate(
            &[(
                "images".into(),
                Value::ImageSet(aligned_set(vec![
                    ImageSetMember::new("reference", image(0.5), metadata(100, 2.0, 0.01)),
                    ImageSetMember::new("bad-format", incompatible, metadata(100, 2.0, 0.01)),
                ])),
            )]
            .into_iter()
            .collect(),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("bad-format"), "unexpected error: {error}");
    assert!(error.contains("format"), "unexpected error: {error}");
}

#[test]
fn hdr_merge_reports_the_member_with_invalid_capture_metadata() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    let error = registry
        .instantiate("core.hdr-merge")
        .unwrap()
        .evaluate(
            &[(
                "images".into(),
                Value::ImageSet(aligned_set(vec![
                    ImageSetMember::new("good", image(0.5), metadata(100, 2.0, 0.01)),
                    ImageSetMember::new("missing-iso", image(0.5), Metadata::default()),
                ])),
            )]
            .into_iter()
            .collect(),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing-iso"), "unexpected error: {error}");
    assert!(error.contains("ISO"), "unexpected error: {error}");
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
    assert!(invalid.is_err());
}

#[test]
fn focus_stack_requires_alignment_and_emits_blended_diagnostics() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    let reference = Image::from_pixels(
        3,
        1,
        vec![
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    )
    .unwrap();
    let focused = Image::from_pixels(
        3,
        1,
        vec![
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    )
    .unwrap();
    let result = evaluate(
        &registry,
        "core.focus-stack",
        [(
            "images".into(),
            Value::ImageSet(aligned_set(vec![
                ImageSetMember::new("reference", reference, Metadata::default()),
                ImageSetMember::new("focused", focused, Metadata::default()),
            ])),
        )]
        .into_iter()
        .collect(),
        Parameters::new(),
        EvaluationContext::default(),
    );
    let Value::Image(output) = &result.outputs["image"] else {
        panic!("focus stack should emit an image");
    };
    assert!(output.pixel(1, 0).unwrap()[0] > 0.0);
    assert!(output.pixel(1, 0).unwrap()[0] < 1.0);
    let Value::ConfidenceMap(confidence) = &result.outputs["confidence"] else {
        panic!("focus stack should expose confidence");
    };
    assert!(
        confidence
            .mask()
            .values()
            .iter()
            .all(|value| (0.0..=1.0).contains(value))
    );
    let Value::Mask(selection) = &result.outputs["selection_mask"] else {
        panic!("focus stack should expose a selection mask");
    };
    assert!(
        selection
            .values()
            .iter()
            .all(|value| (0.0..=1.0).contains(value))
    );
    assert!(matches!(result.outputs["diagnostics"], Value::String(_)));
}

#[test]
fn focus_stack_rejects_unaligned_sets() {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    let error = registry
        .instantiate("core.focus-stack")
        .unwrap()
        .evaluate(
            &[("images".into(), Value::ImageSet(set()))]
                .into_iter()
                .collect(),
            &Parameters::new(),
            &EvaluationContext::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("aligned"), "unexpected error: {error}");
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
