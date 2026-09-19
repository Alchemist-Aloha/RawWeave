use rawweave_batch::{
    BatchEngine, BatchError, BatchItem, BatchJob, BatchProcessor, CancellationToken,
    CheckpointPolicy, CheckpointResolution, CheckpointRuntime, ItemState, JobStore, OutputFormat,
    OutputRecipe, PinnedDependencies, PinnedWorkflow,
};
use rawweave_core::NodeId;
use rawweave_graph::{
    ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointPayload, GenerationMetadata, Graph,
    Provenance, WorkflowDefinition, WorkflowMetadata,
};
use rawweave_image::Image;
use rawweave_node_api::{
    EvaluationContext, EvaluationPolicy, Inputs, NodeDescriptor, NodeError, NodeInstance,
    NodeRegistry, NodeResult, Parameters, PortDescriptor,
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn artifact(dependency: &str) -> CheckpointArtifact {
    CheckpointArtifact::new(
        CheckpointPayload::Image(Image::from_pixels(1, 1, vec![[0.5, 0.5, 0.5, 1.0]]).unwrap()),
        dependency,
        Provenance::new(dependency, 1),
        GenerationMetadata::new(1),
    )
    .unwrap()
}

fn checkpoint(store: &ArtifactStore) -> Checkpoint {
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("fresh");
    checkpoint.commit(artifact("fresh"), store).unwrap();
    checkpoint.set_dependency_hash("stale");
    checkpoint
}

const MANUAL_TYPE: &str = "fixture.batch-manual";

struct BatchManualNode;

impl NodeInstance for BatchManualNode {
    fn evaluate(
        &self,
        _inputs: &Inputs,
        _parameters: &Parameters,
        _context: &EvaluationContext,
    ) -> Result<NodeResult, NodeError> {
        Err(NodeError::Message(
            "batch fixture is not evaluated directly".to_owned(),
        ))
    }
}

fn manual_workflow(store: &ArtifactStore, checkpoint: Checkpoint) -> WorkflowDefinition {
    let mut registry = NodeRegistry::default();
    let mut descriptor = NodeDescriptor::new(MANUAL_TYPE, "Batch Manual");
    descriptor.outputs = vec![PortDescriptor::output("image", "Image", "core.Image")];
    descriptor.evaluation_policy = EvaluationPolicy::ManualCheckpoint;
    registry
        .register_factory(descriptor, || Box::new(BatchManualNode))
        .unwrap();

    let mut graph = Graph::new(registry).with_artifact_store(store.clone());
    graph.add_node(NodeId::from("manual"), MANUAL_TYPE).unwrap();
    graph.register_checkpoint(checkpoint).unwrap();
    WorkflowDefinition::new(
        "test.batch-manual",
        "1.0.0",
        graph,
        WorkflowMetadata::new("Batch Manual"),
    )
    .unwrap()
}

fn artifact_with_value(dependency: &str, value: f32) -> CheckpointArtifact {
    CheckpointArtifact::new(
        CheckpointPayload::Image(
            Image::from_pixels(1, 1, vec![[value, value, value, 1.0]]).unwrap(),
        ),
        dependency,
        Provenance::new(dependency, 1),
        GenerationMetadata::new(2),
    )
    .unwrap()
}

fn stale_checkpoint(store: &ArtifactStore) -> Checkpoint {
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("fresh");
    checkpoint
        .commit(artifact_with_value("fresh", 0.25), store)
        .unwrap();
    checkpoint.set_dependency_hash("stale");
    checkpoint
}

fn missing_checkpoint() -> Checkpoint {
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("missing");
    checkpoint
}

fn fresh_checkpoint(store: &ArtifactStore) -> Checkpoint {
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("fresh");
    checkpoint
        .commit(artifact_with_value("fresh", 0.25), store)
        .unwrap();
    checkpoint
}

struct EngineCheckpointProcessor {
    generated: Arc<AtomicUsize>,
    processed: Arc<AtomicUsize>,
    generated_value: f32,
    cancel_after_generation: bool,
}

impl EngineCheckpointProcessor {
    fn committed_image(workflow: &PinnedWorkflow) -> Result<Image, BatchError> {
        let graph = &workflow.definition.graph;
        let checkpoint = graph
            .checkpoint(&NodeId::from("manual"))
            .map_err(|error| BatchError::Processor(error.to_string()))?
            .ok_or_else(|| {
                BatchError::Processor("manual checkpoint was not registered".to_owned())
            })?;
        let artifact = checkpoint
            .committed_artifact(&graph.artifact_store())?
            .ok_or_else(|| BatchError::Processor("manual checkpoint has no artifact".to_owned()))?;
        let CheckpointPayload::Image(image) = artifact.payload else {
            return Err(BatchError::Processor(
                "manual checkpoint artifact is not an image".to_owned(),
            ));
        };
        Ok(image)
    }
}

impl BatchProcessor for EngineCheckpointProcessor {
    fn process(
        &self,
        workflow: &PinnedWorkflow,
        _item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError> {
        if cancel.is_cancelled() {
            return Err(BatchError::Cancelled);
        }
        self.processed.fetch_add(1, Ordering::SeqCst);
        Self::committed_image(workflow)
    }

    fn generate_checkpoint(
        &self,
        _workflow: &PinnedWorkflow,
        _item: &BatchItem,
        checkpoint: &Checkpoint,
        cancel: &CancellationToken,
    ) -> Result<CheckpointArtifact, BatchError> {
        if cancel.is_cancelled() {
            return Err(BatchError::Cancelled);
        }
        self.generated.fetch_add(1, Ordering::SeqCst);
        let dependency = checkpoint
            .current_dependency_hash()
            .ok_or_else(|| {
                BatchError::CheckpointPolicy("checkpoint dependency is missing".to_owned())
            })?
            .to_owned();
        if self.cancel_after_generation {
            cancel.cancel();
        }
        Ok(artifact_with_value(&dependency, self.generated_value))
    }
}

fn run_engine(
    policy: CheckpointPolicy,
    store: &ArtifactStore,
    checkpoint: Checkpoint,
    processor: Arc<EngineCheckpointProcessor>,
    output_dir: &std::path::Path,
) -> rawweave_batch::BatchJob {
    let workflow = PinnedWorkflow::new(manual_workflow(store, checkpoint), 1).unwrap();
    let job = BatchJob::new(
        "checkpoint-engine-job",
        workflow,
        PinnedDependencies::default(),
        BTreeMap::new(),
        vec![OutputRecipe::new(OutputFormat::Png, output_dir)],
        policy,
        vec![BatchItem::new("item", "/synthetic/input.png", "item.png")],
    )
    .unwrap();
    let engine =
        BatchEngine::new_with_artifact_store(job, JobStore::memory(), processor, 1, store.clone())
            .unwrap();
    engine.start().unwrap();
    engine.wait().unwrap();
    engine.snapshot().unwrap()
}

#[test]
fn runtime_policies_choose_their_documented_checkpoint_action() {
    let store = ArtifactStore::memory();

    let mut stale = BTreeMap::from([("manual".to_owned(), checkpoint(&store))]);
    let mut runtime = CheckpointRuntime::new(CheckpointPolicy::UseCommitted, &mut stale, &store);
    assert!(matches!(
        runtime.resolve("manual").unwrap(),
        CheckpointResolution::UseCommitted { stale: true, .. }
    ));

    let mut missing = BTreeMap::from([("manual".to_owned(), Checkpoint::new("manual", 1))]);
    missing
        .get_mut("manual")
        .unwrap()
        .set_dependency_hash("missing");
    let mut runtime =
        CheckpointRuntime::new(CheckpointPolicy::GenerateIfMissing, &mut missing, &store);
    let token = match runtime.resolve("manual").unwrap() {
        CheckpointResolution::Generate { token } => token,
        CheckpointResolution::UseCommitted { .. } => panic!("missing checkpoint was reused"),
    };
    runtime.cancel_generation("manual", token).unwrap();

    let mut fresh = BTreeMap::from([("manual".to_owned(), {
        let mut checkpoint = Checkpoint::new("manual", 1);
        checkpoint.set_dependency_hash("fresh");
        checkpoint.commit(artifact("fresh"), &store).unwrap();
        checkpoint
    })]);
    let mut runtime = CheckpointRuntime::new(CheckpointPolicy::RegenerateAll, &mut fresh, &store);
    let token = match runtime.resolve("manual").unwrap() {
        CheckpointResolution::Generate { token } => token,
        CheckpointResolution::UseCommitted { .. } => panic!("regenerate-all reused a checkpoint"),
    };
    runtime.cancel_generation("manual", token).unwrap();

    let mut stale = BTreeMap::from([("manual".to_owned(), checkpoint(&store))]);
    let mut runtime = CheckpointRuntime::new(CheckpointPolicy::FailIfStale, &mut stale, &store);
    assert!(runtime.resolve("manual").is_err());
}

#[test]
fn use_committed_rejects_an_incompatible_artifact_instead_of_serving_it() {
    let store = ArtifactStore::memory();
    let mut checkpoint = Checkpoint::new("manual", 1);
    checkpoint.set_dependency_hash("fresh");
    checkpoint.commit(artifact("fresh"), &store).unwrap();

    let incompatible = CheckpointArtifact::new(
        CheckpointPayload::Image(Image::from_pixels(1, 1, vec![[0.9, 0.9, 0.9, 1.0]]).unwrap()),
        "fresh",
        Provenance::new("fresh", 2),
        GenerationMetadata::new(2),
    )
    .unwrap();
    store.put(&incompatible).unwrap();
    let mut document = serde_json::to_value(&checkpoint).unwrap();
    document["committed_artifact_id"] = serde_json::json!({ "Sha256": incompatible.id().as_str() });
    let checkpoint: Checkpoint = serde_json::from_value(document).unwrap();

    let mut checkpoints = BTreeMap::from([("manual".to_owned(), checkpoint)]);
    let mut runtime =
        CheckpointRuntime::new(CheckpointPolicy::UseCommitted, &mut checkpoints, &store);
    assert!(matches!(
        runtime.resolve("manual"),
        Err(BatchError::CheckpointPolicy(_))
    ));
}

#[test]
fn batch_engine_use_committed_completes_with_a_stale_artifact() {
    let directory = tempfile::tempdir().unwrap();
    let store = ArtifactStore::memory();
    let generated = Arc::new(AtomicUsize::new(0));
    let processed = Arc::new(AtomicUsize::new(0));
    let processor = Arc::new(EngineCheckpointProcessor {
        generated: Arc::clone(&generated),
        processed: Arc::clone(&processed),
        generated_value: 0.75,
        cancel_after_generation: false,
    });

    let snapshot = run_engine(
        CheckpointPolicy::UseCommitted,
        &store,
        stale_checkpoint(&store),
        processor,
        directory.path(),
    );
    let item = &snapshot.items[0];
    assert_eq!(item.state, ItemState::Completed);
    assert_eq!(item.outputs.len(), 1);
    assert_eq!(generated.load(Ordering::SeqCst), 0);
    assert_eq!(processed.load(Ordering::SeqCst), 1);
}

#[test]
fn batch_engine_generate_if_missing_generates_and_completes_the_item() {
    let directory = tempfile::tempdir().unwrap();
    let store = ArtifactStore::memory();
    let generated = Arc::new(AtomicUsize::new(0));
    let processed = Arc::new(AtomicUsize::new(0));
    let processor = Arc::new(EngineCheckpointProcessor {
        generated: Arc::clone(&generated),
        processed: Arc::clone(&processed),
        generated_value: 0.75,
        cancel_after_generation: false,
    });

    let snapshot = run_engine(
        CheckpointPolicy::GenerateIfMissing,
        &store,
        missing_checkpoint(),
        processor,
        directory.path(),
    );
    let item = &snapshot.items[0];
    assert_eq!(item.state, ItemState::Completed);
    assert_eq!(item.outputs.len(), 1);
    assert_eq!(generated.load(Ordering::SeqCst), 1);
    assert_eq!(processed.load(Ordering::SeqCst), 1);
}

#[test]
fn batch_cancellation_before_generation_commit_does_not_store_a_late_artifact() {
    let directory = tempfile::tempdir().unwrap();
    let store = ArtifactStore::memory();
    let generated = Arc::new(AtomicUsize::new(0));
    let processed = Arc::new(AtomicUsize::new(0));
    let processor = Arc::new(EngineCheckpointProcessor {
        generated: Arc::clone(&generated),
        processed: Arc::clone(&processed),
        generated_value: 0.75,
        cancel_after_generation: true,
    });

    let snapshot = run_engine(
        CheckpointPolicy::GenerateIfMissing,
        &store,
        missing_checkpoint(),
        processor,
        directory.path(),
    );
    assert_eq!(snapshot.items[0].state, ItemState::Cancelled);
    assert_eq!(generated.load(Ordering::SeqCst), 1);
    assert_eq!(processed.load(Ordering::SeqCst), 0);
    let generated_artifact = artifact_with_value("missing", 0.75);
    assert!(store.get(generated_artifact.id()).unwrap().is_none());
}

#[test]
fn batch_engine_regenerate_all_generates_and_completes_the_item() {
    let directory = tempfile::tempdir().unwrap();
    let store = ArtifactStore::memory();
    let generated = Arc::new(AtomicUsize::new(0));
    let processed = Arc::new(AtomicUsize::new(0));
    let processor = Arc::new(EngineCheckpointProcessor {
        generated: Arc::clone(&generated),
        processed: Arc::clone(&processed),
        generated_value: 0.75,
        cancel_after_generation: false,
    });

    let snapshot = run_engine(
        CheckpointPolicy::RegenerateAll,
        &store,
        fresh_checkpoint(&store),
        processor,
        directory.path(),
    );
    let item = &snapshot.items[0];
    assert_eq!(item.state, ItemState::Completed);
    assert_eq!(item.outputs.len(), 1);
    assert_eq!(generated.load(Ordering::SeqCst), 1);
    assert_eq!(processed.load(Ordering::SeqCst), 1);
}

#[test]
fn batch_engine_fail_if_stale_fails_the_item_without_processing_or_generation() {
    let directory = tempfile::tempdir().unwrap();
    let store = ArtifactStore::memory();
    let generated = Arc::new(AtomicUsize::new(0));
    let processed = Arc::new(AtomicUsize::new(0));
    let processor = Arc::new(EngineCheckpointProcessor {
        generated: Arc::clone(&generated),
        processed: Arc::clone(&processed),
        generated_value: 0.75,
        cancel_after_generation: false,
    });

    let snapshot = run_engine(
        CheckpointPolicy::FailIfStale,
        &store,
        stale_checkpoint(&store),
        processor,
        directory.path(),
    );
    let item = &snapshot.items[0];
    assert_eq!(item.state, ItemState::Failed);
    assert!(
        item.failure
            .as_deref()
            .is_some_and(|message| message.contains("stale"))
    );
    assert_eq!(item.outputs.len(), 0);
    assert_eq!(generated.load(Ordering::SeqCst), 0);
    assert_eq!(processed.load(Ordering::SeqCst), 0);
}
