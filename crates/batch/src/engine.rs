use crate::checkpoint::{CheckpointResolution, CheckpointRuntime};
use crate::model::{
    BatchItem, BatchJob, BatchState, ItemState, OutputRecipe, PinnedDependencies, PinnedWorkflow,
};
use crate::persistence::JobStore;
use crate::preflight::{PreflightOptions, PreflightReport, preflight};
use crate::recipe::{record_for_path, write_output};
use crate::{BatchError, OutputRecord};
use rawweave_color::{DisplayRGB, SceneLinearRGB};
use rawweave_graph::{
    ArtifactStore, Checkpoint, EvaluationPolicy, GenerationToken, WorkflowDefinition, WorkflowPort,
};
use rawweave_image::{ColorDomain, Image, PixelFormat};
use rawweave_node_api::{EvaluationContext, NodeRegistry, ParameterValue, Value};
use rawweave_project::{built_in_node_pack_manifests, default_registry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

/// Cooperative cancellation shared with a decoder/processor.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// The execution boundary for a batch item. Implementations should decode and
/// evaluate one item at a time; the engine drops the returned image before it
/// advances to another item.
pub trait BatchProcessor: Send + Sync {
    fn validate(
        &self,
        workflow: &PinnedWorkflow,
        dependencies: &PinnedDependencies,
    ) -> Result<(), BatchError> {
        workflow.verify()?;
        let _ = dependencies;
        Ok(())
    }

    fn process(
        &self,
        workflow: &PinnedWorkflow,
        item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError>;

    /// Refresh any per-item dependency hashes before policy resolution. The
    /// default keeps processors that do not expose graph checkpoint inputs
    /// compatible with the runtime.
    fn refresh_checkpoint_dependencies(
        &self,
        _workflow: &PinnedWorkflow,
        _item: &BatchItem,
        _cancel: &CancellationToken,
    ) -> Result<(), BatchError> {
        Ok(())
    }

    /// Generate one explicit manual checkpoint. External/plugin-backed
    /// processors override this hook; the default fails with an actionable
    /// message instead of silently executing a manual node.
    fn generate_checkpoint(
        &self,
        _workflow: &PinnedWorkflow,
        _item: &BatchItem,
        checkpoint: &Checkpoint,
        _cancel: &CancellationToken,
    ) -> Result<rawweave_graph::CheckpointArtifact, BatchError> {
        Err(BatchError::CheckpointPolicy(format!(
            "checkpoint '{}' requires a generation-capable batch processor; use use_committed or generate it before the batch",
            checkpoint.node_id
        )))
    }
}

impl<F> BatchProcessor for F
where
    F: Fn(&PinnedWorkflow, &BatchItem, &CancellationToken) -> Result<Image, BatchError>
        + Send
        + Sync,
{
    fn process(
        &self,
        workflow: &PinnedWorkflow,
        item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError> {
        self(workflow, item, cancel)
    }
}

const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "dcr", "dng", "erf", "kdc", "mrw", "nef", "nrw", "orf", "pef",
    "raf", "raw", "rw2", "rwl", "srw", "x3f",
];

fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            RAW_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

/// Processor used by the desktop batch commands. It evaluates the pinned
/// workflow graph, rather than treating the batch as a source-file copier.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImageFileProcessor;

impl ImageFileProcessor {
    fn runtime_definition(
        &self,
        workflow: &PinnedWorkflow,
    ) -> Result<WorkflowDefinition, BatchError> {
        workflow.verify()?;
        let registry = default_registry();
        let mut definition = workflow.definition.clone();
        attach_registry(&mut definition, &registry);
        Ok(definition)
    }

    fn validate_definition(
        &self,
        workflow: &PinnedWorkflow,
        dependencies: &PinnedDependencies,
    ) -> Result<WorkflowDefinition, BatchError> {
        let definition = self.runtime_definition(workflow)?;
        validate_dependencies(&definition, dependencies)?;
        definition
            .validate()
            .map_err(|error| BatchError::Workflow(error.to_string()))?;
        Ok(definition)
    }
}

impl BatchProcessor for ImageFileProcessor {
    fn validate(
        &self,
        workflow: &PinnedWorkflow,
        dependencies: &PinnedDependencies,
    ) -> Result<(), BatchError> {
        self.validate_definition(workflow, dependencies).map(|_| ())
    }

    fn process(
        &self,
        workflow: &PinnedWorkflow,
        item: &BatchItem,
        cancel: &CancellationToken,
    ) -> Result<Image, BatchError> {
        if cancel.is_cancelled() {
            return Err(BatchError::Cancelled);
        }
        let definition = self.validate_definition(workflow, &PinnedDependencies::default())?;
        let mut context = match workflow_kind(&definition)? {
            WorkflowKind::Ordinary => EvaluationContext::with_source_image(decode_ordinary(item)?),
            WorkflowKind::Raw => EvaluationContext::default().with_source_path(&item.source_path),
        };
        for (id, value) in &item.overrides {
            let parameter = definition.parameters().get(id).ok_or_else(|| {
                BatchError::Processor(format!(
                    "workflow parameter override '{id}' is not declared"
                ))
            })?;
            validate_override(&definition, parameter, id, value)?;
            context = context.with_parameter_override(
                parameter.node_id.as_str(),
                &parameter.parameter_id,
                value.clone(),
            );
        }
        if cancel.is_cancelled() {
            return Err(BatchError::Cancelled);
        }
        let image = select_image_output(&definition, &context)?;
        if cancel.is_cancelled() {
            return Err(BatchError::Cancelled);
        }
        Ok(image)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkflowKind {
    Ordinary,
    Raw,
}

fn attach_registry(definition: &mut WorkflowDefinition, registry: &NodeRegistry) {
    definition.graph = definition.graph.clone().with_registry(registry.clone());
    for nested in definition.nested_subgraphs.values_mut() {
        attach_registry(nested, registry);
    }
}

fn attach_artifact_store(definition: &mut WorkflowDefinition, store: &ArtifactStore) {
    definition.graph = definition.graph.clone().with_artifact_store(store.clone());
    for nested in definition.nested_subgraphs.values_mut() {
        attach_artifact_store(nested, store);
    }
}

fn validate_dependencies(
    definition: &WorkflowDefinition,
    dependencies: &PinnedDependencies,
) -> Result<(), BatchError> {
    if let Some((id, version)) = dependencies.plugins.iter().next() {
        return Err(BatchError::Processor(format!(
            "missing plugin '{id}' version '{version}'"
        )));
    }
    if let Some((id, version)) = dependencies.external_providers.iter().next() {
        return Err(BatchError::Processor(format!(
            "missing external provider '{id}' version '{version}'"
        )));
    }

    let available_packs = built_in_node_pack_manifests();
    let mut required_packs = BTreeMap::new();
    for dependency in definition
        .node_pack_dependencies
        .iter()
        .chain(dependencies.node_packs.iter())
    {
        match required_packs.insert(dependency.id.clone(), dependency.version.clone()) {
            Some(previous) if previous != dependency.version => {
                return Err(BatchError::Processor(format!(
                    "pinned node pack '{}' has conflicting versions '{}' and '{}'",
                    dependency.id, previous, dependency.version
                )));
            }
            _ => {}
        }
    }
    for (id, required) in required_packs {
        let available = available_packs.iter().find(|pack| pack.package_id == id);
        match available {
            None => {
                return Err(BatchError::Processor(format!(
                    "missing node pack '{id}' version '{required}'"
                )));
            }
            Some(pack) if pack.version != required => {
                return Err(BatchError::Processor(format!(
                    "node pack '{id}' requires {required}, available {}",
                    pack.version
                )));
            }
            Some(_) => {}
        }
    }

    let mut available_subgraphs = BTreeMap::new();
    for (id, nested) in &definition.nested_subgraphs {
        available_subgraphs.insert(id.as_str(), nested.as_ref());
    }
    for dependency in definition
        .subgraph_dependencies
        .iter()
        .chain(dependencies.subgraphs.iter())
    {
        let Some(nested) = available_subgraphs.get(dependency.id.as_str()) else {
            return Err(BatchError::Processor(format!(
                "missing subgraph '{}' version '{}'",
                dependency.id, dependency.version
            )));
        };
        if nested.version() != dependency.version
            || (!dependency.hash.is_empty() && nested.hash() != dependency.hash)
        {
            return Err(BatchError::Processor(format!(
                "pinned subgraph '{}' does not match its version or hash",
                dependency.id
            )));
        }
    }
    Ok(())
}

fn validate_override(
    definition: &WorkflowDefinition,
    parameter: &rawweave_graph::WorkflowParameter,
    id: &str,
    value: &ParameterValue,
) -> Result<(), BatchError> {
    if parameter.parameter_type != value.parameter_type() {
        return Err(BatchError::Processor(format!(
            "workflow parameter override '{id}' has the wrong type"
        )));
    }
    let node = definition.graph.node(&parameter.node_id).ok_or_else(|| {
        BatchError::Processor(format!(
            "workflow parameter override '{id}' targets a missing node"
        ))
    })?;
    let descriptor = node
        .descriptor
        .parameter(&parameter.parameter_id)
        .ok_or_else(|| {
            BatchError::Processor(format!(
                "workflow parameter override '{id}' targets a missing parameter"
            ))
        })?;
    match value {
        ParameterValue::Float(number)
            if !number.is_finite()
                || descriptor.min.is_some_and(|minimum| *number < minimum)
                || descriptor.max.is_some_and(|maximum| *number > maximum) =>
        {
            return Err(BatchError::Processor(format!(
                "workflow parameter override '{id}' is outside its allowed range"
            )));
        }
        _ => {}
    }
    Ok(())
}

fn decode_ordinary(item: &BatchItem) -> Result<Image, BatchError> {
    if is_raw_path(&item.source_path) {
        return Err(BatchError::Processor(
            "RAW source requires a RAW workflow".to_owned(),
        ));
    }
    let decoded = image::ImageReader::open(&item.source_path)
        .map_err(|error| BatchError::Processor(format!("could not open source: {error}")))?
        .decode()
        .map_err(|error| BatchError::Processor(format!("could not decode source: {error}")))?
        .to_rgba32f();
    let (width, height) = decoded.dimensions();
    let pixels = decoded.pixels().map(|pixel| pixel.0).collect::<Vec<_>>();
    Image::from_pixels_with_metadata(
        width,
        height,
        pixels,
        PixelFormat::Rgba32Float,
        ColorDomain::Srgb,
    )
    .map_err(|error| BatchError::Processor(format!("decoded image is invalid: {error}")))
}

fn workflow_kind(definition: &WorkflowDefinition) -> Result<WorkflowKind, BatchError> {
    let mut has_raw = false;
    let mut has_ordinary = false;
    for node in definition.graph.nodes().values() {
        has_raw |= node.type_id.starts_with("raw.");
        for port in node
            .descriptor
            .inputs
            .iter()
            .chain(node.descriptor.outputs.iter())
        {
            has_raw |= port.data_type.starts_with("raw.");
            has_ordinary |= port.data_type == "core.Image";
        }
    }
    match (has_raw, has_ordinary) {
        (true, false) => Ok(WorkflowKind::Raw),
        (false, true) => Ok(WorkflowKind::Ordinary),
        (true, true) => Err(BatchError::Processor(
            "workflow mixes RAW and ordinary image requirements".to_owned(),
        )),
        (false, false) => Err(BatchError::Processor(
            "workflow has no supported image source".to_owned(),
        )),
    }
}

fn is_image_output(data_type: &str) -> bool {
    matches!(
        data_type,
        "core.Image" | "color.SceneLinearRGB" | "color.DisplayRGB"
    )
}

fn select_image_output(
    definition: &WorkflowDefinition,
    context: &EvaluationContext,
) -> Result<Image, BatchError> {
    let declared = definition
        .outputs()
        .iter()
        .filter(|port| is_image_output(&port.data_type))
        .collect::<Vec<_>>();
    if let Some(port) = declared.into_iter().next() {
        return evaluate_image_output(definition, port, context);
    }

    let mut candidates = Vec::<WorkflowPort>::new();
    for node in definition.graph.nodes().values() {
        let preferred_port = match node.type_id.as_str() {
            "core.output" => Some("image"),
            "raw.display-transform" => Some("display"),
            _ => None,
        };
        if let Some((port_id, port)) = preferred_port
            .and_then(|port_id| node.descriptor.output(port_id).map(|port| (port_id, port)))
        {
            candidates.push(WorkflowPort {
                id: format!("output:{}:{}", node.id, port_id),
                name: port.name.clone(),
                direction: rawweave_graph::WorkflowPortDirection::Output,
                node_id: node.id.clone(),
                port_id: port_id.to_owned(),
                data_type: port.data_type.clone(),
                required: false,
            });
        }
    }
    for node in definition.graph.nodes().values() {
        if node.type_id == "core.image-input"
            || definition
                .graph
                .edges()
                .iter()
                .any(|edge| edge.from_node == node.id)
        {
            continue;
        }
        for port in &node.descriptor.outputs {
            if is_image_output(&port.data_type) {
                candidates.push(WorkflowPort {
                    id: format!("output:{}:{}", node.id, port.id),
                    name: port.name.clone(),
                    direction: rawweave_graph::WorkflowPortDirection::Output,
                    node_id: node.id.clone(),
                    port_id: port.id.clone(),
                    data_type: port.data_type.clone(),
                    required: false,
                });
            }
        }
    }
    candidates
        .first()
        .map(|port| evaluate_image_output(definition, port, context))
        .unwrap_or_else(|| {
            Err(BatchError::UnsupportedWorkflowOutput(
                "workflow has no declared or appropriate image output".to_owned(),
            ))
        })
}

fn evaluate_image_output(
    definition: &WorkflowDefinition,
    port: &WorkflowPort,
    context: &EvaluationContext,
) -> Result<Image, BatchError> {
    let value = definition
        .graph
        .evaluate(&port.node_id, &port.port_id, context)
        .map_err(|error| {
            BatchError::Processor(format!("workflow output '{}': {error}", port.id))
        })?;
    match value {
        Value::Image(image) => Ok(image),
        Value::DisplayRGB(display) => display_to_image(&display),
        Value::SceneLinearRGB(scene) => scene_to_image(&scene),
        other => Err(BatchError::UnsupportedWorkflowOutput(format!(
            "workflow output '{}' produced {} instead of an image",
            port.id,
            other.data_type()
        ))),
    }
}

fn display_to_image(display: &DisplayRGB) -> Result<Image, BatchError> {
    let pixels = display
        .pixels()
        .iter()
        .map(|[red, green, blue]| [*red, *green, *blue, 1.0])
        .collect();
    Image::from_pixels_with_metadata(
        display.dimensions().width,
        display.dimensions().height,
        pixels,
        PixelFormat::Rgba32Float,
        ColorDomain::Srgb,
    )
    .map_err(|error| BatchError::Processor(format!("display output is invalid: {error}")))
}

fn scene_to_image(scene: &SceneLinearRGB) -> Result<Image, BatchError> {
    let pixels = scene
        .pixels()
        .iter()
        .map(|[red, green, blue]| [*red, *green, *blue, 1.0])
        .collect();
    Image::from_pixels_with_metadata(
        scene.dimensions().width,
        scene.dimensions().height,
        pixels,
        PixelFormat::Rgba32Float,
        ColorDomain::LinearSrgb,
    )
    .map_err(|error| BatchError::Processor(format!("scene output is invalid: {error}")))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DryRunResult {
    pub workflow_revision: u64,
    pub workflow_hash: String,
    pub item_ids: Vec<String>,
    pub recipes: Vec<OutputRecipe>,
}

impl DryRunResult {
    pub fn item_ids(&self) -> Vec<String> {
        self.item_ids.clone()
    }
}

pub fn dry_run(job: &BatchJob, subset: crate::DryRunSubset) -> Result<DryRunResult, BatchError> {
    job.validate()?;
    let items = job.selected_items(&subset)?;
    Ok(DryRunResult {
        workflow_revision: job.workflow.revision,
        workflow_hash: job.workflow.hash.clone(),
        item_ids: items.into_iter().map(|item| item.id.clone()).collect(),
        recipes: job.recipes.clone(),
    })
}

struct EngineState {
    running: bool,
    active_workers: usize,
}

struct EngineInner {
    job: Mutex<BatchJob>,
    store: JobStore,
    artifact_store: ArtifactStore,
    processor: Arc<dyn BatchProcessor>,
    max_workers: usize,
    cancel: CancellationToken,
    paused: AtomicBool,
    state: Mutex<EngineState>,
    wake: Condvar,
}

/// A restartable, bounded-concurrency batch runner.
pub struct BatchEngine {
    inner: Arc<EngineInner>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl BatchEngine {
    pub fn new<P>(
        job: BatchJob,
        store: JobStore,
        processor: Arc<P>,
        max_workers: usize,
    ) -> Result<Self, BatchError>
    where
        P: BatchProcessor + 'static,
    {
        let artifact_store = store.artifact_store();
        Self::new_with_artifact_store(job, store, processor, max_workers, artifact_store)
    }

    pub fn new_with_artifact_store<P>(
        mut job: BatchJob,
        store: JobStore,
        processor: Arc<P>,
        max_workers: usize,
        artifact_store: ArtifactStore,
    ) -> Result<Self, BatchError>
    where
        P: BatchProcessor + 'static,
    {
        if max_workers == 0 {
            return Err(BatchError::InvalidJob(
                "worker concurrency must be greater than zero".to_owned(),
            ));
        }
        attach_artifact_store(&mut job.workflow.definition, &artifact_store);
        job.validate()?;
        store.save(&job)?;
        Ok(Self {
            inner: Arc::new(EngineInner {
                job: Mutex::new(job),
                store,
                artifact_store,
                processor,
                max_workers,
                cancel: CancellationToken::new(),
                paused: AtomicBool::new(false),
                state: Mutex::new(EngineState {
                    running: false,
                    active_workers: 0,
                }),
                wake: Condvar::new(),
            }),
            workers: Mutex::new(Vec::new()),
        })
    }

    pub fn resume<P>(
        store: JobStore,
        processor: Arc<P>,
        max_workers: usize,
    ) -> Result<Self, BatchError>
    where
        P: BatchProcessor + 'static,
    {
        let mut job = store.load()?;
        for item in &mut job.items {
            if item.state == ItemState::Running {
                item.state = ItemState::Waiting;
            }
        }
        job.requeue_invalid_completed()?;
        job.refresh_state();
        Self::new(job, store, processor, max_workers)
    }

    pub fn store(&self) -> JobStore {
        self.inner.store.clone()
    }

    pub fn snapshot(&self) -> Result<BatchJob, BatchError> {
        self.inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))
            .map(|job| job.clone())
    }

    pub fn preflight(&self, options: &PreflightOptions) -> Result<PreflightReport, BatchError> {
        Ok(preflight(&self.snapshot()?, options))
    }

    pub fn start(&self) -> Result<(), BatchError> {
        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
            if state.running {
                return Err(BatchError::AlreadyRunning);
            }
            state.running = true;
        }
        self.inner.cancel.reset();
        self.inner.paused.store(false, Ordering::Release);

        let ids = match self.prepare_start() {
            Ok(ids) => ids,
            Err(error) => {
                if let Ok(mut state) = self.inner.state.lock() {
                    state.running = false;
                }
                return Err(error);
            }
        };
        if ids.is_empty() {
            if let Ok(mut state) = self.inner.state.lock() {
                state.running = false;
            }
            self.persist_refresh()?;
            return Ok(());
        }

        let queue = Arc::new(Mutex::new(VecDeque::from(ids)));
        let worker_count = self.inner.max_workers.min(
            queue
                .lock()
                .map_err(|_| BatchError::Persistence("work queue is poisoned".to_owned()))?
                .len(),
        );
        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
            state.active_workers = worker_count;
        }

        let mut handles = self
            .workers
            .lock()
            .map_err(|_| BatchError::Persistence("worker list is poisoned".to_owned()))?;
        handles.clear();
        for _ in 0..worker_count {
            let inner = Arc::clone(&self.inner);
            let queue = Arc::clone(&queue);
            handles.push(thread::spawn(move || worker_loop(inner, queue)));
        }
        Ok(())
    }

    fn prepare_start(&self) -> Result<Vec<String>, BatchError> {
        let mut job = self
            .inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
        job.requeue_invalid_completed()?;
        job.state = BatchState::Running;
        let ids = job
            .items
            .iter()
            .filter(|item| item.state == ItemState::Waiting)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        self.inner.store.save(&job)?;
        Ok(ids)
    }

    pub fn pause(&self) -> Result<(), BatchError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        if !state.running {
            return Err(BatchError::NotRunning);
        }
        drop(state);
        self.inner.paused.store(true, Ordering::Release);
        let mut job = self
            .inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
        job.state = BatchState::Paused;
        self.inner.store.save(&job)?;
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn resume_run(&self) -> Result<(), BatchError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        if !state.running {
            return Err(BatchError::NotRunning);
        }
        drop(state);
        self.inner.paused.store(false, Ordering::Release);
        let mut job = self
            .inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
        if job.state == BatchState::Paused {
            job.state = BatchState::Running;
        }
        self.inner.store.save(&job)?;
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn cancel(&self) -> Result<(), BatchError> {
        self.inner.cancel.cancel();
        self.inner.paused.store(false, Ordering::Release);
        {
            let mut job = self
                .inner
                .job
                .lock()
                .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
            let ids = job
                .items
                .iter()
                .filter(|item| item.state == ItemState::Waiting)
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                let _ = job.transition_item(&id, ItemState::Cancelled);
            }
            self.inner.store.save(&job)?;
        }
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn retry_failed(&self) -> Result<usize, BatchError> {
        self.ensure_stopped()?;
        let mut job = self.lock_job()?;
        let count = job.retry_failed()?;
        self.inner.store.save(&job)?;
        Ok(count)
    }

    pub fn retry_selected(&self, ids: &[String]) -> Result<usize, BatchError> {
        self.ensure_stopped()?;
        let mut job = self.lock_job()?;
        let count = job.retry_selected(ids)?;
        self.inner.store.save(&job)?;
        Ok(count)
    }

    pub fn skip(&self, ids: &[String]) -> Result<usize, BatchError> {
        self.ensure_stopped()?;
        let mut job = self.lock_job()?;
        let count = job.skip(ids)?;
        self.inner.store.save(&job)?;
        Ok(count)
    }

    pub fn failed_item(&self, id: &str) -> Result<BatchItem, BatchError> {
        let job = self.snapshot()?;
        let item = job
            .items
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| BatchError::UnknownItem(id.to_owned()))?;
        if item.state != ItemState::Failed {
            return Err(BatchError::InvalidJob(format!(
                "batch item '{id}' is not failed"
            )));
        }
        Ok(item)
    }

    pub fn wait(&self) -> Result<(), BatchError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        while state.running {
            state = self
                .inner
                .wake
                .wait(state)
                .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        }
        drop(state);
        let mut handles = self
            .workers
            .lock()
            .map_err(|_| BatchError::Persistence("worker list is poisoned".to_owned()))?;
        for handle in handles.drain(..) {
            let _ = handle.join();
        }
        Ok(())
    }

    fn ensure_stopped(&self) -> Result<(), BatchError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| BatchError::Persistence("engine state is poisoned".to_owned()))?;
        if state.running {
            Err(BatchError::AlreadyRunning)
        } else {
            Ok(())
        }
    }

    fn lock_job(&self) -> Result<std::sync::MutexGuard<'_, BatchJob>, BatchError> {
        self.inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))
    }

    fn persist_refresh(&self) -> Result<(), BatchError> {
        let mut job = self.lock_job()?;
        job.refresh_state();
        self.inner.store.save(&job)
    }
}

fn worker_loop(inner: Arc<EngineInner>, queue: Arc<Mutex<VecDeque<String>>>) {
    loop {
        if inner.cancel.is_cancelled() {
            break;
        }
        let id = {
            let mut queue_guard = match queue.lock() {
                Ok(queue_guard) => queue_guard,
                Err(_) => break,
            };
            queue_guard.pop_front()
        };
        let Some(id) = id else { break };

        let paused = inner.paused.load(Ordering::Acquire);
        if paused {
            let mut state = match inner.state.lock() {
                Ok(state) => state,
                Err(_) => break,
            };
            while inner.paused.load(Ordering::Acquire) && !inner.cancel.is_cancelled() {
                state = match inner.wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            drop(state);
        }
        if inner.cancel.is_cancelled() {
            break;
        }
        process_item(&inner, &id);
    }
    finish_worker(&inner);
}

fn checkpoint_key(prefix: &str, node_id: &str) -> String {
    if prefix.is_empty() {
        node_id.to_owned()
    } else {
        format!("{prefix}/{node_id}")
    }
}

fn collect_checkpoints(
    definition: &WorkflowDefinition,
    prefix: &str,
    checkpoints: &mut BTreeMap<String, Checkpoint>,
) -> Result<(), BatchError> {
    let manual_nodes = definition
        .graph
        .nodes()
        .values()
        .filter(|node| node.descriptor.evaluation_policy == EvaluationPolicy::ManualCheckpoint)
        .map(|node| (node.id.clone(), node.descriptor.version))
        .collect::<Vec<_>>();
    for (node_id, node_version) in manual_nodes {
        let key = checkpoint_key(prefix, node_id.as_str());
        let checkpoint = definition
            .graph
            .checkpoint(&node_id)
            .map_err(|error| BatchError::CheckpointPolicy(format!("{key}: {error}")))?
            .unwrap_or_else(|| Checkpoint::new(node_id.as_str(), node_version));
        checkpoints.insert(key, checkpoint);
    }
    for (nested_id, nested) in &definition.nested_subgraphs {
        let nested_prefix = checkpoint_key(prefix, nested_id);
        collect_checkpoints(nested, &nested_prefix, checkpoints)?;
    }
    Ok(())
}

fn apply_checkpoints(
    definition: &mut WorkflowDefinition,
    prefix: &str,
    checkpoints: &BTreeMap<String, Checkpoint>,
) -> Result<(), BatchError> {
    let manual_node_ids = definition
        .graph
        .nodes()
        .values()
        .filter(|node| node.descriptor.evaluation_policy == EvaluationPolicy::ManualCheckpoint)
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    for node_id in manual_node_ids {
        let key = checkpoint_key(prefix, node_id.as_str());
        if let Some(checkpoint) = checkpoints.get(&key) {
            definition
                .graph
                .register_checkpoint(checkpoint.clone())
                .map_err(|error| BatchError::CheckpointPolicy(format!("{key}: {error}")))?;
        }
    }
    for (nested_id, nested) in &mut definition.nested_subgraphs {
        let nested_prefix = checkpoint_key(prefix, nested_id);
        apply_checkpoints(nested, &nested_prefix, checkpoints)?;
    }
    Ok(())
}

fn checkpoint_policy_failure(item_id: &str, error: BatchError) -> BatchError {
    match error {
        BatchError::Cancelled => BatchError::Cancelled,
        error => BatchError::CheckpointPolicy(format!(
            "item '{item_id}' cannot satisfy its checkpoint policy: {error}; resolve the checkpoint or retry the item"
        )),
    }
}

fn process_with_checkpoint_policy(
    inner: &Arc<EngineInner>,
    workflow: &mut PinnedWorkflow,
    item: &BatchItem,
) -> Result<Image, BatchError> {
    inner
        .processor
        .refresh_checkpoint_dependencies(workflow, item, &inner.cancel)
        .map_err(|error| checkpoint_policy_failure(&item.id, error))?;

    let mut checkpoints = BTreeMap::new();
    collect_checkpoints(&workflow.definition, "", &mut checkpoints)
        .map_err(|error| checkpoint_policy_failure(&item.id, error))?;
    if checkpoints.is_empty() {
        return inner.processor.process(workflow, item, &inner.cancel);
    }
    let policy = {
        let job = inner
            .job
            .lock()
            .map_err(|_| BatchError::Persistence("batch job is poisoned".to_owned()))?;
        job.checkpoint_policy
    };
    if !policy.is_explicit_checkpoint_policy() {
        return Err(checkpoint_policy_failure(
            &item.id,
            BatchError::CheckpointPolicy(
                "workflow contains manual checkpoints; choose use_committed, generate_if_missing, regenerate_all, or fail_if_stale"
                    .to_owned(),
            ),
        ));
    }

    let keys = checkpoints.keys().cloned().collect::<Vec<_>>();
    {
        let mut runtime = CheckpointRuntime::new(policy, &mut checkpoints, &inner.artifact_store);
        let mut pending_generations = Vec::<(String, GenerationToken, Checkpoint)>::new();
        for key in keys {
            if inner.cancel.is_cancelled() {
                for (node_id, token, _) in pending_generations.drain(..) {
                    let _ = runtime.cancel_generation(&node_id, token);
                }
                return Err(BatchError::Cancelled);
            }
            match runtime
                .resolve(&key)
                .map_err(|error| checkpoint_policy_failure(&item.id, error))?
            {
                CheckpointResolution::UseCommitted { .. } => {}
                CheckpointResolution::Generate { token } => {
                    let checkpoint = runtime
                        .checkpoint(&key)
                        .map_err(|error| checkpoint_policy_failure(&item.id, error))?
                        .clone();
                    pending_generations.push((key, token, checkpoint));
                }
            }
        }

        for (node_id, token, checkpoint) in pending_generations {
            if inner.cancel.is_cancelled() {
                let _ = runtime.cancel_generation(&node_id, token);
                return Err(BatchError::Cancelled);
            }
            let generated =
                inner
                    .processor
                    .generate_checkpoint(workflow, item, &checkpoint, &inner.cancel);
            let artifact = match generated {
                Ok(artifact) => artifact,
                Err(BatchError::Cancelled) => {
                    let _ = runtime.cancel_generation(&node_id, token);
                    return Err(BatchError::Cancelled);
                }
                Err(error) => {
                    let _ = runtime.fail_generation(&node_id, token, error.to_string());
                    return Err(checkpoint_policy_failure(
                        &item.id,
                        BatchError::CheckpointPolicy(format!(
                            "checkpoint '{node_id}' generation failed: {error}"
                        )),
                    ));
                }
            };
            runtime
                .commit_generation(&node_id, token, artifact)
                .map_err(|error| checkpoint_policy_failure(&item.id, error))?;
        }
    }
    apply_checkpoints(&mut workflow.definition, "", &checkpoints)
        .map_err(|error| checkpoint_policy_failure(&item.id, error))?;
    inner.processor.process(workflow, item, &inner.cancel)
}

fn process_item(inner: &Arc<EngineInner>, id: &str) {
    let (mut workflow, dependencies, item, recipes) = {
        let mut job = match inner.job.lock() {
            Ok(job) => job,
            Err(_) => return,
        };
        let Some(item) = job.items.iter().find(|item| item.id == id).cloned() else {
            return;
        };
        if item.state != ItemState::Waiting {
            return;
        }
        if job.transition_item(id, ItemState::Running).is_err() {
            return;
        }
        if inner.store.save(&job).is_err() {
            return;
        }
        (
            job.workflow.clone(),
            job.dependencies.clone(),
            item,
            job.recipes.clone(),
        )
    };

    let result = validate_dependencies(&workflow.definition, &dependencies)
        .and_then(|()| process_with_checkpoint_policy(inner, &mut workflow, &item));
    let image = match result {
        Ok(image) if !inner.cancel.is_cancelled() => image,
        Ok(_) => {
            finish_cancelled(inner, id);
            return;
        }
        Err(BatchError::Cancelled) => {
            finish_cancelled(inner, id);
            return;
        }
        Err(error) => {
            finish_failed(inner, id, error.to_string());
            return;
        }
    };

    let mut outputs = Vec::<OutputRecord>::with_capacity(recipes.len());
    for (recipe_index, recipe) in recipes.iter().enumerate() {
        if inner.cancel.is_cancelled() {
            finish_cancelled(inner, id);
            return;
        }
        match write_output(&image, recipe, &item, recipe_index, None).and_then(record_for_path) {
            Ok(output) => outputs.push(output),
            Err(error) => {
                finish_failed(inner, id, error.to_string());
                return;
            }
        }
    }
    if inner.cancel.is_cancelled() {
        finish_cancelled(inner, id);
    } else {
        finish_completed(inner, id, outputs);
    }
}

fn finish_cancelled(inner: &Arc<EngineInner>, id: &str) {
    if let Ok(mut job) = inner.job.lock() {
        let _ = job.transition_item(id, ItemState::Cancelled);
        let _ = inner.store.save(&job);
    }
}

fn finish_failed(inner: &Arc<EngineInner>, id: &str, message: String) {
    if let Ok(mut job) = inner.job.lock() {
        let _ = job.fail_item(id, message);
        let _ = inner.store.save(&job);
    }
}

fn finish_completed(inner: &Arc<EngineInner>, id: &str, outputs: Vec<OutputRecord>) {
    if let Ok(mut job) = inner.job.lock() {
        let _ = job.complete_item(id, outputs);
        let _ = inner.store.save(&job);
    }
}

fn finish_worker(inner: &Arc<EngineInner>) {
    if let Ok(mut state) = inner.state.lock() {
        state.active_workers = state.active_workers.saturating_sub(1);
        if state.active_workers == 0 {
            state.running = false;
            if let Ok(mut job) = inner.job.lock() {
                job.refresh_state();
                let _ = inner.store.save(&job);
            }
            inner.wake.notify_all();
        }
    }
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, BatchError> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).map_err(|source| BatchError::Io {
        operation: "hash file",
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| BatchError::Io {
            operation: "read file while hashing",
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

use std::io::Read;
