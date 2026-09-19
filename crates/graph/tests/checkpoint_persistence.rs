use rawweave_core::NodeId;
use rawweave_core_image::register_nodes as register_image_nodes;
use rawweave_graph::{
    ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointAvailability, CheckpointError,
    CheckpointPayload, GenerationMetadata, Graph, GraphError, Provenance, WorkflowDefinition,
    WorkflowMetadata,
};
use rawweave_image::Image;
use rawweave_node_api::{
    EvaluationContext, EvaluationPolicy, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodeRegistry, NodeResult, Parameters, PortDescriptor, Value,
};

const MANUAL_TYPE: &str = "fixture.persisted-manual";

struct ManualIdentity;

impl NodeInstance for ManualIdentity {
    fn evaluate(
        &self,
        inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        let Value::Image(image) = inputs
            .get("image")
            .ok_or_else(|| NodeError::MissingInput("image".to_owned()))?
        else {
            return Err(NodeError::InvalidParameter("image".to_owned()));
        };
        Ok(NodeResult::single("image", Value::Image(image.clone())))
    }
}

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::default();
    register_image_nodes(&mut registry).unwrap();
    let mut descriptor = NodeDescriptor::new(MANUAL_TYPE, "Persisted Manual");
    descriptor.inputs = vec![PortDescriptor::input("image", "Image", "core.Image", true)];
    descriptor.outputs = vec![PortDescriptor::output("image", "Image", "core.Image")];
    descriptor.evaluation_policy = EvaluationPolicy::ManualCheckpoint;
    registry
        .register_factory(descriptor, || Box::new(ManualIdentity))
        .unwrap();
    registry
}

fn image(value: f32) -> Image {
    Image::from_pixels(1, 1, vec![[value, value, value, 1.0]]).unwrap()
}

fn graph(store: ArtifactStore) -> Graph {
    let mut graph = Graph::new(registry()).with_artifact_store(store);
    graph
        .add_node(NodeId::from("source"), "core.image-input")
        .unwrap();
    graph.add_node(NodeId::from("manual"), MANUAL_TYPE).unwrap();
    graph
        .connect(
            NodeId::from("source"),
            "image",
            NodeId::from("manual"),
            "image",
        )
        .unwrap();
    graph
        .register_checkpoint(Checkpoint::new("manual", 1))
        .unwrap();
    graph
}

fn commit(graph: &Graph, value: f32) {
    let context = EvaluationContext::with_source_image(image(0.25));
    assert!(
        graph
            .evaluate(&NodeId::from("manual"), "image", &context)
            .is_err()
    );
    let checkpoint = graph.checkpoint(&NodeId::from("manual")).unwrap().unwrap();
    let dependency = checkpoint.current_dependency_hash().unwrap().to_owned();
    let artifact = CheckpointArtifact::new(
        CheckpointPayload::Image(image(value)),
        dependency.clone(),
        Provenance::new(dependency, 1),
        GenerationMetadata::new(1),
    )
    .unwrap();
    graph
        .commit_checkpoint(&NodeId::from("manual"), artifact)
        .unwrap();
}

#[test]
fn graph_workflow_round_trip_restores_checkpoint_state_from_file_store() {
    let directory = std::env::temp_dir().join(format!(
        "rawweave-checkpoint-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let store = ArtifactStore::new(directory.join("artifacts"));
    let graph = graph(store.clone());
    commit(&graph, 0.75);

    let serialized = graph.to_json().unwrap();
    assert!(serialized.contains("checkpoints"));
    assert!(!serialized.contains("\"payload\""));
    let restored =
        Graph::from_json_with_artifact_store(&serialized, registry(), store.clone()).unwrap();
    let checkpoint = restored
        .checkpoint(&NodeId::from("manual"))
        .unwrap()
        .unwrap();
    assert_eq!(
        checkpoint.availability_with_store(&store).unwrap(),
        CheckpointAvailability::Fresh
    );
    let value = restored
        .evaluate(
            &NodeId::from("manual"),
            "image",
            &EvaluationContext::with_source_image(image(0.25)),
        )
        .unwrap();
    assert_eq!(value, Value::Image(image(0.75)));

    let workflow = WorkflowDefinition::new(
        "persisted.workflow",
        "1.0.0",
        graph,
        WorkflowMetadata::new("Persisted"),
    )
    .unwrap();
    let workflow_json = workflow.to_json().unwrap();
    let restored_workflow = WorkflowDefinition::from_json_with_artifact_store(
        &workflow_json,
        registry(),
        store.clone(),
    )
    .unwrap();
    let checkpoint = restored_workflow
        .graph
        .checkpoint(&NodeId::from("manual"))
        .unwrap()
        .unwrap();
    assert_eq!(
        checkpoint.availability_with_store(&store).unwrap(),
        CheckpointAvailability::Fresh
    );
    assert_eq!(
        restored_workflow
            .graph
            .evaluate(
                &NodeId::from("manual"),
                "image",
                &EvaluationContext::with_source_image(image(0.25)),
            )
            .unwrap(),
        Value::Image(image(0.75))
    );
}

#[test]
fn missing_and_corrupt_checkpoint_artifacts_are_rejected_on_restore() {
    let directory = std::env::temp_dir().join(format!(
        "rawweave-checkpoint-invalid-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(directory.join("artifacts")).unwrap();
    let store = ArtifactStore::new(directory.join("artifacts"));
    let graph = graph(store.clone());
    commit(&graph, 0.75);
    let serialized = graph.to_json().unwrap();
    let artifact_id = graph
        .checkpoint(&NodeId::from("manual"))
        .unwrap()
        .unwrap()
        .committed_artifact_id()
        .unwrap()
        .as_str()
        .to_owned();
    let artifact_path = directory
        .join("artifacts")
        .join(format!("{artifact_id}.json"));

    std::fs::remove_file(&artifact_path).unwrap();
    assert!(matches!(
        Graph::from_json_with_artifact_store(&serialized, registry(), store.clone()),
        Err(GraphError::Checkpoint(CheckpointError::NoCommittedArtifact))
    ));

    commit(&graph, 0.75);
    std::fs::write(&artifact_path, b"not valid json").unwrap();
    assert!(matches!(
        Graph::from_json_with_artifact_store(&serialized, registry(), store),
        Err(GraphError::Checkpoint(CheckpointError::Serialization(_)))
    ));
}
