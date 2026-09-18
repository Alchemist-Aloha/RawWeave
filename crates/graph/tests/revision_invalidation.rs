use rawweave_core::NodeId;
use rawweave_core_image::register_nodes;
use rawweave_graph::Graph;
use rawweave_node_api::NodeRegistry;

fn graph() -> Graph {
    let mut registry = NodeRegistry::default();
    register_nodes(&mut registry).unwrap();
    Graph::new(registry)
}

#[test]
fn graph_reports_only_downstream_nodes_for_targeted_invalidation() {
    let mut graph = graph();
    graph
        .add_node(NodeId::from("input"), "core.image-input")
        .unwrap();
    graph
        .add_node(NodeId::from("invert"), "core.invert")
        .unwrap();
    graph
        .add_node(NodeId::from("output"), "core.output")
        .unwrap();
    graph
        .add_node(NodeId::from("other"), "core.invert")
        .unwrap();
    graph
        .connect(
            NodeId::from("input"),
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

    let invalidated = graph.downstream_nodes(&NodeId::from("input"));
    assert!(invalidated.contains(&NodeId::from("input")));
    assert!(invalidated.contains(&NodeId::from("invert")));
    assert!(invalidated.contains(&NodeId::from("output")));
    assert!(!invalidated.contains(&NodeId::from("other")));
}
