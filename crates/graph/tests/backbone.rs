use rawweave_core::NodeId;
use rawweave_core_image::register_nodes as register_image_nodes;
use rawweave_core_values::register_nodes as register_value_nodes;
use rawweave_graph::{Graph, GraphError};
use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, NodeRegistry, Value};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_image_nodes(&mut registry).unwrap();
    register_value_nodes(&mut registry).unwrap();
    registry
}

fn graph() -> Graph {
    Graph::new(registry())
}

#[test]
fn graph_adds_connects_disconnects_and_removes_nodes() {
    let mut graph = graph();
    graph
        .add_node(NodeId::from("input"), "core.image-input")
        .unwrap();
    graph
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();

    graph
        .connect(
            NodeId::from("input"),
            "image",
            NodeId::from("output"),
            "image",
        )
        .unwrap();
    assert_eq!(graph.edges().len(), 1);

    graph
        .disconnect(
            NodeId::from("input"),
            "image",
            NodeId::from("output"),
            "image",
        )
        .unwrap();
    assert!(graph.edges().is_empty());

    graph.remove_node(&NodeId::from("output")).unwrap();
    assert!(graph.node(&NodeId::from("output")).is_none());
}

#[test]
fn graph_rejects_type_mismatched_connections() {
    let mut graph = graph();
    graph
        .add_node(NodeId::from("constant"), "core.constant-float")
        .unwrap();
    graph
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();

    let error = graph
        .connect(
            NodeId::from("constant"),
            "value",
            NodeId::from("output"),
            "image",
        )
        .unwrap_err();
    assert!(matches!(error, GraphError::TypeMismatch { .. }));
}

#[test]
fn graph_rejects_cycles() {
    let mut graph = graph();
    graph.add_node(NodeId::from("left"), "core.invert").unwrap();
    graph
        .add_node(NodeId::from("right"), "core.invert")
        .unwrap();
    graph
        .connect(
            NodeId::from("left"),
            "image",
            NodeId::from("right"),
            "image",
        )
        .unwrap();

    let error = graph
        .connect(
            NodeId::from("right"),
            "image",
            NodeId::from("left"),
            "image",
        )
        .unwrap_err();
    assert!(matches!(error, GraphError::CycleDetected));
    assert_eq!(graph.edges().len(), 1);
}

#[test]
fn graph_round_trips_through_serde_without_semantic_changes() {
    let mut original = graph();
    original
        .add_node(NodeId::from("input"), "core.image-input")
        .unwrap();
    original
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();
    original
        .connect(
            NodeId::from("input"),
            "image",
            NodeId::from("output"),
            "image",
        )
        .unwrap();

    let json = original.to_json().unwrap();
    let restored = Graph::from_json(&json, registry()).unwrap();
    assert_eq!(
        restored.node(&NodeId::from("input")),
        original.node(&NodeId::from("input"))
    );
    assert_eq!(restored.edges(), original.edges());
    assert_eq!(restored.revision(), original.revision());
}

#[test]
fn evaluation_is_deterministic_and_propagates_image_values() {
    let mut graph = graph();
    graph
        .add_node(NodeId::from("input"), "core.image-input")
        .unwrap();
    graph
        .add_node(NodeId::from("amount"), "core.constant-float")
        .unwrap();
    graph
        .add_node(NodeId::from("exposure"), "core.exposure")
        .unwrap();
    graph
        .add_node(NodeId::from("invert"), "core.invert")
        .unwrap();
    graph
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();
    graph
        .set_parameter(&NodeId::from("amount"), "value", 1.0_f32.into())
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
            NodeId::from("amount"),
            "value",
            NodeId::from("exposure"),
            "exposure",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("exposure"),
            "image",
            NodeId::from("invert"),
            "image",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("invert"),
            "image",
            NodeId::from("output"),
            "image",
        )
        .unwrap();

    let image = Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap();
    let context = EvaluationContext::with_source_image(image);
    let first = graph
        .evaluate(&NodeId::from("output"), "image", &context)
        .unwrap();
    let second = graph
        .evaluate(&NodeId::from("output"), "image", &context)
        .unwrap();
    assert_eq!(first, second);
    let Value::Image(result) = first else {
        panic!("expected image output")
    };
    assert_eq!(result.pixel(0, 0), Some([0.5, 0.0, -0.5, 1.0]));
}

#[test]
fn missing_node_type_is_reported_when_evaluated() {
    let mut graph = graph();
    graph
        .add_node(NodeId::from("input"), "core.image-input")
        .unwrap();
    let json = graph
        .to_json()
        .unwrap()
        .replace("core.image-input", "missing.node");
    let restored = Graph::from_json(&json, registry()).unwrap();

    let error = restored
        .evaluate(
            &NodeId::from("input"),
            "image",
            &EvaluationContext::default(),
        )
        .unwrap_err();
    assert!(matches!(error, GraphError::UnknownNodeType { .. }));
}

#[test]
fn graph_rejects_non_finite_parameters_without_poisoning_json() {
    let mut graph = graph();
    graph
        .add_node(NodeId::from("exposure"), "core.exposure")
        .unwrap();
    let before = graph.to_json().unwrap();

    assert!(
        graph
            .set_parameter(&NodeId::from("exposure"), "exposure", f32::NAN.into(),)
            .is_err()
    );

    let json = graph.to_json().unwrap();
    assert_eq!(json, before);
    assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
}

#[test]
fn deserialized_graphs_reject_duplicate_incoming_edges() {
    let mut original = graph();
    original
        .add_node(NodeId::from("input"), "core.image-input")
        .unwrap();
    original
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();
    original
        .connect(
            NodeId::from("input"),
            "image",
            NodeId::from("output"),
            "image",
        )
        .unwrap();

    let mut document: serde_json::Value =
        serde_json::from_str(&original.to_json().unwrap()).unwrap();
    let duplicate = document["edges"][0].clone();
    document["edges"].as_array_mut().unwrap().push(duplicate);

    let error = Graph::from_json(&document.to_string(), registry()).unwrap_err();
    assert!(matches!(error, GraphError::InputAlreadyConnected { .. }));
}
