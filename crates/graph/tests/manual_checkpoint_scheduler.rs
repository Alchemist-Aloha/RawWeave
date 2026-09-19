use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use rawweave_core::NodeId;
use rawweave_core_image::register_nodes as register_image_nodes;
use rawweave_graph::{
    ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointState, GenerationMetadata, Graph,
    GraphError, Provenance,
};
use rawweave_image::Image;
use rawweave_node_api::{
    EvaluationContext, EvaluationPolicy, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodeRegistry, NodeResult, ParameterDescriptor, Parameters, PortDescriptor, Value,
};

const MANUAL_TYPE: &str = "fixture.manual-transform";

#[derive(Clone)]
struct FixtureCounter(Arc<AtomicUsize>);

struct ManualTransform {
    counter: FixtureCounter,
}

impl NodeInstance for ManualTransform {
    fn evaluate(
        &self,
        inputs: &Inputs,
        parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        self.counter.0.fetch_add(1, Ordering::SeqCst);
        let Value::Image(image) = inputs
            .get("image")
            .ok_or_else(|| NodeError::MissingInput("image".to_owned()))?
        else {
            return Err(NodeError::InvalidParameter("image".to_owned()));
        };
        let strength = parameters
            .get("strength")
            .and_then(|value| value.as_float())
            .ok_or_else(|| NodeError::InvalidParameter("strength".to_owned()))?;
        let pixels = image
            .pixels()
            .iter()
            .map(|pixel| {
                [
                    pixel[0] * strength,
                    pixel[1] * strength,
                    pixel[2] * strength,
                    pixel[3],
                ]
            })
            .collect();
        let image = Image::from_pixels_with_color_metadata(
            image.width(),
            image.height(),
            pixels,
            image.pixel_format(),
            image.color_metadata(),
        )
        .map_err(|error| NodeError::Message(error.to_string()))?;
        Ok(NodeResult::new(
            [
                ("image".to_owned(), Value::Image(image.clone())),
                ("preview".to_owned(), Value::Image(image)),
            ]
            .into_iter()
            .collect(),
        ))
    }
}

fn registry(counter: FixtureCounter) -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_image_nodes(&mut registry).unwrap();
    let mut descriptor = NodeDescriptor::new(MANUAL_TYPE, "Fixture Manual Transform");
    descriptor.inputs = vec![PortDescriptor::input("image", "Image", "core.Image", true)];
    descriptor.outputs = vec![
        PortDescriptor::output("image", "Image", "core.Image"),
        PortDescriptor::output("preview", "Preview", "core.Image"),
    ];
    descriptor.parameters = vec![ParameterDescriptor::float(
        "strength",
        "Strength",
        1.0,
        Some(0.0),
        Some(4.0),
    )];
    descriptor.evaluation_policy = EvaluationPolicy::ManualCheckpoint;
    registry
        .register_factory(descriptor, move || {
            Box::new(ManualTransform {
                counter: counter.clone(),
            })
        })
        .unwrap();
    registry
}

fn image(value: f32) -> Image {
    Image::from_pixels(1, 1, vec![[value, value, value, 1.0]]).unwrap()
}

fn graph(counter: FixtureCounter) -> Graph {
    Graph::new(registry(counter)).with_artifact_store(ArtifactStore::memory())
}

fn add_pipeline(graph: &mut Graph) {
    graph
        .add_node(NodeId::from("source"), "core.image-input")
        .unwrap();
    graph.add_node(NodeId::from("manual"), MANUAL_TYPE).unwrap();
    graph
        .add_node(NodeId::from("downstream"), "core.invert")
        .unwrap();
    graph
        .connect(
            NodeId::from("source"),
            "image",
            NodeId::from("manual"),
            "image",
        )
        .unwrap();
    graph
        .connect(
            NodeId::from("manual"),
            "image",
            NodeId::from("downstream"),
            "image",
        )
        .unwrap();
    graph
        .register_checkpoint(Checkpoint::new("manual", 1))
        .unwrap();
}

fn current_dependency(graph: &Graph) -> String {
    graph
        .checkpoint(&NodeId::from("manual"))
        .unwrap()
        .unwrap()
        .current_dependency_hash()
        .unwrap()
        .to_owned()
}

fn commit_image(graph: &Graph, dependency: String, value: f32, generation: u64) {
    let artifact = CheckpointArtifact::new(
        rawweave_graph::CheckpointPayload::Image(image(value)),
        dependency,
        Provenance::new(current_dependency(graph), 1),
        GenerationMetadata::new(generation),
    )
    .unwrap();
    graph
        .commit_checkpoint(&NodeId::from("manual"), artifact)
        .unwrap();
}

#[test]
fn normal_evaluation_skips_manual_fixture_serves_stale_output_and_accepts_regeneration() {
    let counter = FixtureCounter(Arc::new(AtomicUsize::new(0)));
    let mut graph = graph(counter.clone());
    add_pipeline(&mut graph);
    let first_context = EvaluationContext::with_source_image(image(0.2));

    let missing = graph.evaluate(&NodeId::from("downstream"), "image", &first_context);
    assert!(matches!(missing, Err(GraphError::Checkpoint(_))));
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);

    let first_dependency = current_dependency(&graph);
    commit_image(&graph, first_dependency, 0.8, 1);
    let first = graph
        .evaluate(&NodeId::from("downstream"), "image", &first_context)
        .unwrap();
    let Value::Image(first) = first else {
        panic!("expected image output");
    };
    assert_eq!(
        first.pixel(0, 0),
        Some([0.19999999, 0.19999999, 0.19999999, 1.0])
    );
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);

    let changed_context = EvaluationContext::with_source_image(image(0.4));
    let stale = graph
        .evaluate(&NodeId::from("downstream"), "image", &changed_context)
        .unwrap();
    let Value::Image(stale) = stale else {
        panic!("expected image output");
    };
    assert_eq!(
        stale.pixel(0, 0),
        Some([0.19999999, 0.19999999, 0.19999999, 1.0])
    );
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
    assert_eq!(
        graph
            .checkpoint(&NodeId::from("manual"))
            .unwrap()
            .unwrap()
            .state(),
        CheckpointState::Stale
    );

    let regenerated_dependency = current_dependency(&graph);
    commit_image(&graph, regenerated_dependency, 0.6, 2);
    assert_eq!(
        graph
            .checkpoint(&NodeId::from("manual"))
            .unwrap()
            .unwrap()
            .state(),
        CheckpointState::Current
    );
    let regenerated = graph
        .evaluate(&NodeId::from("downstream"), "image", &changed_context)
        .unwrap();
    let Value::Image(regenerated) = regenerated else {
        panic!("expected image output");
    };
    assert_eq!(
        regenerated.pixel(0, 0),
        Some([0.39999998, 0.39999998, 0.39999998, 1.0])
    );
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
}

#[test]
fn checkpoint_dependency_hash_is_stable_and_covers_transitive_content_parameters_context_and_output()
 {
    let counter_a = FixtureCounter(Arc::new(AtomicUsize::new(0)));
    let mut first = graph(counter_a);
    add_pipeline(&mut first);
    let context = EvaluationContext::with_source_image(image(0.2))
        .with_source_bytes(vec![1, 2, 3])
        .with_external_input("seed", Value::Integer(7))
        .with_asset("model", vec![4, 5, 6]);
    assert!(
        first
            .evaluate(&NodeId::from("manual"), "image", &context)
            .is_err()
    );
    let first_hash = current_dependency(&first);

    let counter_b = FixtureCounter(Arc::new(AtomicUsize::new(0)));
    let mut second = graph(counter_b);
    add_pipeline(&mut second);
    assert!(
        second
            .evaluate(&NodeId::from("manual"), "image", &context)
            .is_err()
    );
    assert_eq!(first_hash, current_dependency(&second));

    second
        .set_parameter(&NodeId::from("manual"), "strength", 2.0_f32.into())
        .unwrap();
    assert!(
        second
            .evaluate(&NodeId::from("manual"), "image", &context)
            .is_err()
    );
    assert_ne!(first_hash, current_dependency(&second));
}
