use rawweave_core::NodeId;
use rawweave_core_image::register_nodes;
use rawweave_graph::Graph;
use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, NodeRegistry};

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

#[test]
fn evaluation_uses_revisioned_cache_and_invalidates_only_downstream_results() {
    let mut graph = graph();
    for (id, type_id) in [
        ("input", "core.image-input"),
        ("exposure", "core.exposure"),
        ("output", "core.output"),
        ("other-input", "core.image-input"),
        ("other-invert", "core.invert"),
    ] {
        graph.add_node(NodeId::from(id), type_id).unwrap();
    }
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
        .connect(
            NodeId::from("other-input"),
            "image",
            NodeId::from("other-invert"),
            "image",
        )
        .unwrap();

    let context = EvaluationContext::with_source_image(
        Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap(),
    );
    graph
        .evaluate(&NodeId::from("output"), "image", &context)
        .unwrap();
    let other_before = graph
        .evaluate(&NodeId::from("other-invert"), "image", &context)
        .unwrap();
    let rawweave_node_api::Value::Image(other_before) = other_before else {
        panic!("expected image output")
    };
    assert_eq!(graph.render_cache().lock().unwrap().len(), 5);

    let revision_before_edit = graph.graph_revision();
    graph
        .set_parameter(&NodeId::from("exposure"), "exposure", 1.0_f32.into())
        .unwrap();
    assert!(graph.graph_revision().value() > revision_before_edit.value());
    assert_eq!(graph.render_cache().lock().unwrap().len(), 3);

    let other_after = graph
        .evaluate(&NodeId::from("other-invert"), "image", &context)
        .unwrap();
    let rawweave_node_api::Value::Image(other_after) = other_after else {
        panic!("expected image output")
    };
    assert_eq!(other_before.revision(), other_after.revision());
    assert_eq!(other_before.backing_ptr(), other_after.backing_ptr());
    assert_eq!(graph.render_cache().lock().unwrap().len(), 3);

    let output = graph
        .evaluate(&NodeId::from("output"), "image", &context)
        .unwrap();
    let rawweave_node_api::Value::Image(output) = output else {
        panic!("expected image output")
    };
    assert_eq!(output.pixel(0, 0), Some([0.5, 1.0, 1.5, 1.0]));
    assert_eq!(graph.render_cache().lock().unwrap().len(), 5);
}
