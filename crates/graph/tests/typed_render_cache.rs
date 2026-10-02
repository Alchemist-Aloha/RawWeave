use rawweave_color::{DisplayRGB, SceneLinearRGB, WorkingSpace};
use rawweave_core::NodeId;
use rawweave_graph::Graph;
use rawweave_image::Dimensions;
use rawweave_node_api::{
    EvaluationContext, Inputs, NodeDescriptor, NodeError, NodeInstance, NodeRegistry, NodeResult,
    ParameterDescriptor, Parameters, PortDescriptor, Value,
};
use rawweave_rendering::{MemoryRenderCache, PreviewQuality};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct TypedSource(Arc<AtomicUsize>);
impl NodeInstance for TypedSource {
    fn evaluate(
        &self,
        _: &Inputs,
        parameters: &Parameters,
        context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let gain = match parameters.get("gain") {
            Some(rawweave_node_api::ParameterValue::Float(gain)) => *gain,
            _ => 1.0,
        };
        let source = context
            .source_bytes
            .as_ref()
            .and_then(|bytes| bytes.first())
            .copied()
            .unwrap_or(1) as f32;
        let scene = SceneLinearRGB::from_pixels(2, 1, vec![[source * gain, -0.5, 2.5]; 2]).unwrap();
        let display = DisplayRGB::new(
            Dimensions::new(2, 1),
            vec![[0.25, 0.5, 0.75]; 2],
            WorkingSpace::Srgb,
        )
        .unwrap();
        Ok(NodeResult::new(
            [
                ("scene".to_owned(), Value::SceneLinearRGB(scene)),
                ("display".to_owned(), Value::DisplayRGB(display)),
            ]
            .into_iter()
            .collect(),
        ))
    }
}
fn graph(calls: &Arc<AtomicUsize>) -> Graph {
    let mut registry = NodeRegistry::default();
    let mut descriptor = NodeDescriptor::new("test.typed-source", "Typed Source");
    descriptor.outputs = vec![
        PortDescriptor::output("scene", "Scene", "color.SceneLinearRGB"),
        PortDescriptor::output("display", "Display", "color.DisplayRGB"),
    ];
    descriptor
        .parameters
        .push(ParameterDescriptor::float("gain", "Gain", 1.0, None, None));
    let calls = Arc::clone(calls);
    registry
        .register_factory(descriptor, move || {
            Box::new(TypedSource(Arc::clone(&calls)))
        })
        .unwrap();
    let mut graph = Graph::new(registry);
    graph
        .add_node(NodeId::from("source"), "test.typed-source")
        .unwrap();
    graph
        .add_node(NodeId::from("other"), "test.typed-source")
        .unwrap();
    graph
}
#[test]
fn typed_multi_output_results_share_pixels_and_invalidate_by_dependency() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = graph(&calls);
    let node = NodeId::from("source");
    let context = EvaluationContext::default().with_source_bytes(vec![1]);
    let Value::SceneLinearRGB(first) = graph.evaluate(&node, "scene", &context).unwrap() else {
        panic!("expected scene")
    };
    assert_eq!(first.pixels()[0], [1.0, -0.5, 2.5]);
    assert!(matches!(
        graph.evaluate(&node, "display", &context).unwrap(),
        Value::DisplayRGB(_)
    ));
    let Value::SceneLinearRGB(repeated) = graph.evaluate(&node, "scene", &context).unwrap() else {
        panic!("expected scene")
    };
    assert_eq!(first.pixels().as_ptr(), repeated.pixels().as_ptr());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    graph
        .set_parameter(&NodeId::from("other"), "gain", 3.0_f32.into())
        .unwrap();
    graph.evaluate(&node, "scene", &context).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    graph.set_parameter(&node, "gain", 2.0_f32.into()).unwrap();
    let Value::SceneLinearRGB(edited) = graph.evaluate(&node, "scene", &context).unwrap() else {
        panic!("expected scene")
    };
    assert_eq!(edited.pixels()[0], [2.0, -0.5, 2.5]);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let changed = EvaluationContext::default().with_source_bytes(vec![2]);
    graph.evaluate(&node, "scene", &changed).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    graph
        .evaluate(&node, "scene", &changed.clone().with_mip_level(1))
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    graph
        .evaluate(&node, "scene", &changed.with_quality(PreviewQuality::Final))
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 5);
}
#[test]
fn typed_results_larger_than_the_payload_budget_are_not_retained() {
    let calls = Arc::new(AtomicUsize::new(0));
    let graph = graph(&calls).with_render_cache(MemoryRenderCache::with_limits(64, 1));
    for _ in 0..2 {
        graph
            .evaluate(
                &NodeId::from("source"),
                "scene",
                &EvaluationContext::default(),
            )
            .unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(graph.render_cache().lock().unwrap().is_empty());
}
