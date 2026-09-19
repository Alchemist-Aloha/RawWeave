use std::collections::BTreeSet;

use rawweave_core::NodeId;
use rawweave_core_image::register_nodes as register_image_nodes;
use rawweave_core_values::register_nodes as register_value_nodes;
use rawweave_graph::{
    DependencyStatus, Graph, NodeManifest, NodePackManifest, PlatformRequirement, TemplateMetadata,
    WorkflowDefinition, WorkflowError, WorkflowMetadata, WorkflowParameter, WorkflowPort,
    WorkflowPortDirection,
};
use rawweave_node_api::{NodeRegistry, ParameterType};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_image_nodes(&mut registry).unwrap();
    register_value_nodes(&mut registry).unwrap();
    registry
}

fn source_graph() -> Graph {
    let mut graph = Graph::new(registry());
    graph
        .add_node(NodeId::from("input"), "core.image-input")
        .unwrap();
    graph
        .add_node(NodeId::from("exposure"), "core.exposure")
        .unwrap();
    graph
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();
    graph
        .connect(
            NodeId::from("input"),
            "image",
            NodeId::from("exposure"),
            "image",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("exposure"),
            "image",
            NodeId::from("output"),
            "image",
        )
        .unwrap();
    graph
}

#[test]
fn selected_graph_becomes_a_reusable_definition_with_boundary_ports() {
    let graph = source_graph();
    let selected = BTreeSet::from([NodeId::from("exposure")]);

    let definition = WorkflowDefinition::from_selection(
        &graph,
        &selected,
        "looks.exposure",
        "1.0.0",
        WorkflowMetadata::new("Exposure Look"),
    )
    .unwrap();

    assert_eq!(definition.identity().id, "looks.exposure");
    assert_eq!(definition.version(), "1.0.0");
    assert_eq!(definition.graph().nodes().len(), 1);
    assert_eq!(definition.ports().len(), 2);
    assert!(definition.ports().iter().any(|port| {
        port.direction == WorkflowPortDirection::Input
            && port.node_id == NodeId::from("exposure")
            && port.port_id == "image"
    }));
    assert!(definition.ports().iter().any(|port| {
        port.direction == WorkflowPortDirection::Output
            && port.node_id == NodeId::from("exposure")
            && port.port_id == "image"
    }));
}

#[test]
fn workflow_parameters_ports_and_nested_subgraphs_round_trip_with_stable_hash() {
    let mut child_graph = Graph::new(registry());
    child_graph
        .add_node(NodeId::from("amount"), "core.constant-float")
        .unwrap();
    let mut child = WorkflowDefinition::new(
        "looks.child",
        "1.0.0",
        child_graph,
        WorkflowMetadata::new("Child"),
    )
    .unwrap();
    child
        .add_parameter(WorkflowParameter::new(
            "amount",
            "Amount",
            NodeId::from("amount"),
            "value",
            ParameterType::Float,
            1.0_f32.into(),
        ))
        .unwrap();

    let mut definition = WorkflowDefinition::new(
        "looks.parent",
        "2.1.0",
        source_graph(),
        WorkflowMetadata::new("Parent"),
    )
    .unwrap();
    definition
        .expose_parameter(&NodeId::from("exposure"), "exposure")
        .unwrap();
    definition
        .expose_output(&NodeId::from("output"), "image")
        .unwrap();
    definition.add_nested_subgraph(child).unwrap();

    let hash = definition.hash();
    let json = definition.to_json().unwrap();
    let restored = WorkflowDefinition::from_json(&json, registry()).unwrap();

    assert_eq!(restored.hash(), hash);
    assert_eq!(restored.parameters().len(), 1);
    assert_eq!(restored.outputs().len(), 1);
    assert!(restored.nested_subgraph("looks.child").is_some());
    assert_eq!(
        restored.graph().nodes().len(),
        definition.graph().nodes().len()
    );
}

#[test]
fn dependency_diagnostics_report_missing_mismatch_and_disabled_nodes() {
    let mut definition = WorkflowDefinition::new(
        "looks.raw",
        "1.0.0",
        source_graph(),
        WorkflowMetadata::new("RAW Look"),
    )
    .unwrap();
    definition
        .add_node_pack_dependency("pack.raw", "2.0.0")
        .unwrap();
    definition
        .add_node_pack_dependency("pack.optional", "1.0.0")
        .unwrap();

    let available = [NodePackManifest::new("pack.raw", "1.0.0")
        .with_node(NodeManifest::new("core.image-input", 1))
        .with_platform_requirement(PlatformRequirement::new("linux", "x86_64"))];
    let report = definition.diagnose_dependencies(&available, &[]);

    assert!(
        report
            .available
            .iter()
            .any(|dependency| dependency.id == "pack.raw")
    );
    assert!(
        report
            .missing
            .iter()
            .any(|dependency| dependency.id == "pack.optional")
    );
    assert!(
        report
            .mismatched
            .iter()
            .any(|dependency| dependency.id == "pack.raw")
    );
    assert!(report.disabled_nodes.iter().any(|node| node == "exposure"));
    assert!(matches!(
        report.status("pack.raw"),
        Some(DependencyStatus::VersionMismatch { .. })
    ));
}

#[test]
fn template_and_node_pack_manifests_are_serializable() {
    let template = TemplateMetadata::new("RAW Portrait")
        .with_author("RawWeave")
        .with_version("1.0.0")
        .with_description("A portrait development workflow")
        .with_thumbnail("thumb.png")
        .with_tags(["portrait", "raw"])
        .with_license("MIT")
        .with_recommended_input_type("raw.Frame")
        .with_minimum_app_version("0.1.0");
    let manifest = NodePackManifest::new("pack.raw", "1.0.0")
        .with_node(NodeManifest::new("raw.decode", 1))
        .with_workflow_template(template.clone())
        .with_dependency("pack.core", "1.0.0")
        .with_platform_requirement(PlatformRequirement::new("linux", "x86_64"));

    let json = serde_json::to_string(&manifest).unwrap();
    let restored: NodePackManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.package_id, "pack.raw");
    assert_eq!(restored.workflow_templates[0], template);
    assert_eq!(restored.nodes[0].type_id, "raw.decode");
    assert_eq!(restored.dependencies[0].id, "pack.core");
}

#[test]
fn invalid_workflow_import_is_rejected_instead_of_silently_substituted() {
    let definition = WorkflowDefinition::new(
        "looks.invalid",
        "1.0.0",
        source_graph(),
        WorkflowMetadata::new("Invalid"),
    )
    .unwrap();
    let json = definition
        .to_json()
        .unwrap()
        .replace("core.output", "missing.output");

    let error = WorkflowDefinition::from_json(&json, registry()).unwrap_err();
    assert!(matches!(error, WorkflowError::Graph(_)));
}

#[test]
fn workflow_hash_is_canonical_for_dependency_order_and_uses_a_full_digest() {
    let mut first = WorkflowDefinition::new(
        "looks.canonical",
        "1.0.0",
        source_graph(),
        WorkflowMetadata::new("Canonical"),
    )
    .unwrap();
    first.add_node_pack_dependency("pack.a", "1.0.0").unwrap();
    first.add_node_pack_dependency("pack.b", "1.0.0").unwrap();

    let mut second = WorkflowDefinition::new(
        "looks.canonical",
        "1.0.0",
        source_graph(),
        WorkflowMetadata::new("Canonical"),
    )
    .unwrap();
    second.add_node_pack_dependency("pack.b", "1.0.0").unwrap();
    second.add_node_pack_dependency("pack.a", "1.0.0").unwrap();

    assert_eq!(first.hash(), second.hash());
    assert_eq!(first.hash().len(), 64);
}

#[test]
fn changing_an_exposed_parameter_updates_its_public_default() {
    let mut definition = WorkflowDefinition::new(
        "looks.parameter",
        "1.0.0",
        source_graph(),
        WorkflowMetadata::new("Parameter"),
    )
    .unwrap();
    definition
        .expose_parameter(&NodeId::from("exposure"), "exposure")
        .unwrap();
    definition
        .set_parameter("exposure:exposure", 2.0_f32.into())
        .unwrap();

    assert_eq!(
        definition.parameters()["exposure:exposure"].default,
        2.0_f32.into()
    );
}

#[test]
fn workflow_import_rejects_forged_identity_and_node_descriptor() {
    let definition = WorkflowDefinition::new(
        "looks.valid",
        "1.0.0",
        source_graph(),
        WorkflowMetadata::new("Valid"),
    )
    .unwrap();
    let mut document: serde_json::Value =
        serde_json::from_str(&definition.to_json().unwrap()).unwrap();
    document["identity"]["id"] = serde_json::Value::String(String::new());
    let error = WorkflowDefinition::from_json(&document.to_string(), registry()).unwrap_err();
    assert!(matches!(error, WorkflowError::InvalidIdentity(_)));

    let mut document: serde_json::Value =
        serde_json::from_str(&definition.to_json().unwrap()).unwrap();
    document["graph"]["nodes"]["exposure"]["descriptor"]["name"] =
        serde_json::Value::String("forged".to_owned());
    let error = WorkflowDefinition::from_json(&document.to_string(), registry()).unwrap_err();
    assert!(matches!(
        error,
        WorkflowError::NodeDescriptorMismatch { .. }
    ));
}

#[allow(dead_code)]
fn _assert_port_shape(port: &WorkflowPort) {
    assert!(!port.id.is_empty());
}
