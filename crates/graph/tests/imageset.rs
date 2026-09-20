use rawweave_core::NodeId;
use rawweave_core_image::register_nodes;
use rawweave_graph::Graph;
use rawweave_image::Image;
use rawweave_node_api::{
    EvaluationContext, ImageSet, ImageSetMember, ImageSetOrder, Metadata, NodeRegistry, Value,
};

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    registry
}

fn set() -> ImageSet {
    ImageSet::new(
        ImageSetOrder::Ordered,
        vec![ImageSetMember::new(
            "frame-1",
            Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap(),
            Metadata::default(),
        )],
    )
    .unwrap()
}

#[test]
fn imageset_graph_values_round_trip_and_input_evaluates_without_queue_state() {
    let mut graph = Graph::new(registry());
    graph
        .add_node(NodeId::from("set-input"), "core.imageset-input")
        .unwrap();
    let json = graph.to_json().unwrap();
    let restored = Graph::from_json(&json, registry()).unwrap();
    let value = restored
        .evaluate(
            &NodeId::from("set-input"),
            "images",
            &EvaluationContext::default().with_source_image_set(set()),
        )
        .unwrap();
    assert!(matches!(value, Value::ImageSet(_)));
    assert!(!json.contains("queue"));
}
