mod ai;
mod browser;
mod hosts;
mod preview;

use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use rawweave_ai_provider::{
    AiImage, AiInput, AiOperation, AiOutput, AiResult, AiWorkflow, ColorInterchange, PollPolicy,
    SubmitRequest, TaskProvenance, WorkflowBindings,
};
use rawweave_batch::{
    dry_run, BatchEngine, BatchJob, DryRunSubset, ImageFileProcessor, JobStore, PreflightOptions,
    MAX_BATCH_WORKERS,
};
use rawweave_core::NodeId;
use rawweave_graph::{
    hash_upstream_inputs, ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointAvailability,
    CheckpointPayload, CheckpointState, DependencyReport, DependencyStatus, EvaluationPolicy,
    ExternalToolMetadata, GenerationMetadata, GenerationToken, Graph, NodePackManifest, Provenance,
    SubgraphDependency, WorkflowDefinition, WorkflowMetadata, WorkflowPort, WorkflowPortDirection,
};
use rawweave_image::{
    ConfidenceMap, DepthMap, Dimensions, Image, LabelMap, Mask, MaskSet, Region, RegionSet,
};
use rawweave_node_api::{
    AlignmentState, EvaluationContext, ImageSet, ImageSetMember, ImageSetOrder,
    ImageSetSourceDescriptor, NodeDescriptor, ParameterValue, Value, MAX_IMAGE_SET_MEMBERS,
};
use rawweave_project::{built_in_node_pack_manifests, EditorCore};
use rawweave_raw::{RawDecodeLimits, RawDecoder, RawFrame, RawloaderDecoder};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Debug)]
pub(crate) enum SourceAsset {
    Ordinary(Image),
    ImageSet(Box<ImageSet>),
    Raw { bytes: Arc<Vec<u8>>, path: PathBuf },
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceKind {
    Ordinary,
    Raw,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SourceSelectionIntent {
    #[default]
    ReplaceWorkflow,
    AttachToLoadedWorkflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkflowKind {
    Ordinary,
    Raw,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenMetadataSummary {
    pub camera: String,
    pub lens: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter: Option<f32>,
    pub focal_length: Option<f32>,
    pub capture_time: Option<String>,
    pub orientation: String,
    pub dimensions: rawweave_image::Dimensions,
    pub exif: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenImageSetMemberDto {
    id: String,
    path: String,
    name: String,
    width: u32,
    height: u32,
    metadata: Option<OpenMetadataSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenImageSetDto {
    kind: &'static str,
    order: ImageSetOrder,
    revision: u64,
    members: Vec<OpenImageSetMemberDto>,
    shared_metadata: Option<OpenMetadataSummary>,
    alignment: OpenImageSetAlignmentDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
enum OpenImageSetAlignmentDto {
    Unaligned,
    Aligned { reference_member: String },
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMetadataDto {
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub recommended_input_type: Option<String>,
    pub minimum_app_version: Option<String>,
}

impl From<WorkflowMetadataDto> for WorkflowMetadata {
    fn from(metadata: WorkflowMetadataDto) -> Self {
        Self {
            name: metadata.name,
            author: metadata.author,
            description: metadata.description,
            thumbnail: metadata.thumbnail,
            tags: metadata.tags,
            license: metadata.license,
            recommended_input_type: metadata.recommended_input_type,
            minimum_app_version: metadata.minimum_app_version,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDependencyDto {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubgraphRequest {
    pub node_ids: Vec<String>,
    pub id: String,
    pub version: String,
    pub metadata: WorkflowMetadataDto,
    #[serde(default)]
    pub node_pack_dependencies: Vec<WorkflowDependencyDto>,
    #[serde(default)]
    pub subgraph_dependencies: Vec<WorkflowDependencyDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowGraphNodeDto {
    id: String,
    type_id: String,
    parameters: rawweave_node_api::Parameters,
    exposed_parameters: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowGraphEdgeDto {
    from_node: String,
    from_port: String,
    to_node: String,
    to_port: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowGraphDto {
    nodes: Vec<WorkflowGraphNodeDto>,
    edges: Vec<WorkflowGraphEdgeDto>,
    revision: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowIdentityDto {
    id: String,
    version: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMetadataResponseDto {
    name: String,
    author: Option<String>,
    description: Option<String>,
    thumbnail: Option<String>,
    tags: Vec<String>,
    license: Option<String>,
    recommended_input_type: Option<String>,
    minimum_app_version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPortDto {
    id: String,
    name: String,
    direction: WorkflowPortDirection,
    node_id: String,
    port_id: String,
    data_type: String,
    required: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowParameterDto {
    id: String,
    name: String,
    node_id: String,
    parameter_id: String,
    parameter_type: rawweave_node_api::ParameterType,
    default: ParameterValue,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinitionDto {
    pub identity: WorkflowIdentityDto,
    pub graph: WorkflowGraphDto,
    pub parameters: Vec<WorkflowParameterDto>,
    pub inputs: Vec<WorkflowPortDto>,
    pub outputs: Vec<WorkflowPortDto>,
    pub subgraph_dependencies: Vec<WorkflowDependencyDto>,
    pub node_pack_dependencies: Vec<WorkflowDependencyDto>,
    pub metadata: WorkflowMetadataResponseDto,
    pub nested_subgraphs: BTreeMap<String, WorkflowDefinitionDto>,
    pub hash: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyDiagnosticDto {
    id: String,
    required_version: String,
    available_version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum DependencyStatusDto {
    Available,
    Missing,
    VersionMismatch { required: String, available: String },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyReportDto {
    pub available: Vec<DependencyDiagnosticDto>,
    pub missing: Vec<DependencyDiagnosticDto>,
    pub mismatched: Vec<DependencyDiagnosticDto>,
    pub disabled_nodes: Vec<String>,
    pub statuses: BTreeMap<String, DependencyStatusDto>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableNodePackDto {
    package_id: String,
    version: String,
    #[serde(default)]
    nodes: Vec<AvailableNodeDto>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableNodeDto {
    type_id: String,
    version: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointStatusDto {
    pub node_id: String,
    pub output_port: String,
    pub state: CheckpointState,
    pub availability: CheckpointAvailability,
    pub current_dependency_hash: Option<String>,
    pub committed_dependency_hash: Option<String>,
    pub committed_artifact_id: Option<String>,
    pub generation: Option<GenerationMetadata>,
    pub provenance: Option<Provenance>,
    pub failure: Option<String>,
    pub progress: Option<f32>,
    pub can_use_committed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointProgressEvent {
    node_id: String,
    output_port: String,
    progress: f32,
    phase: String,
    message: Option<String>,
}

fn metadata_response(metadata: &WorkflowMetadata) -> WorkflowMetadataResponseDto {
    WorkflowMetadataResponseDto {
        name: metadata.name.clone(),
        author: metadata.author.clone(),
        description: metadata.description.clone(),
        thumbnail: metadata.thumbnail.clone(),
        tags: metadata.tags.clone(),
        license: metadata.license.clone(),
        recommended_input_type: metadata.recommended_input_type.clone(),
        minimum_app_version: metadata.minimum_app_version.clone(),
    }
}

fn graph_dto(graph: &Graph) -> WorkflowGraphDto {
    WorkflowGraphDto {
        nodes: graph
            .nodes()
            .values()
            .map(|node| WorkflowGraphNodeDto {
                id: node.id.to_string(),
                type_id: node.type_id.clone(),
                parameters: node.parameters.clone(),
                exposed_parameters: node.exposed_parameters.iter().cloned().collect(),
            })
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|edge| WorkflowGraphEdgeDto {
                from_node: edge.from_node.to_string(),
                from_port: edge.from_port.clone(),
                to_node: edge.to_node.to_string(),
                to_port: edge.to_port.clone(),
            })
            .collect(),
        revision: graph.revision(),
    }
}

fn workflow_definition_dto(definition: &WorkflowDefinition) -> WorkflowDefinitionDto {
    WorkflowDefinitionDto {
        identity: WorkflowIdentityDto {
            id: definition.identity().id.clone(),
            version: definition.version().to_owned(),
        },
        graph: graph_dto(definition.graph()),
        parameters: definition
            .parameters()
            .values()
            .map(|parameter| WorkflowParameterDto {
                id: parameter.id.clone(),
                name: parameter.name.clone(),
                node_id: parameter.node_id.to_string(),
                parameter_id: parameter.parameter_id.clone(),
                parameter_type: parameter.parameter_type,
                default: parameter.default.clone(),
            })
            .collect(),
        inputs: definition.inputs().iter().map(workflow_port_dto).collect(),
        outputs: definition.outputs().iter().map(workflow_port_dto).collect(),
        subgraph_dependencies: definition
            .subgraph_dependencies
            .iter()
            .map(workflow_dependency_dto)
            .collect(),
        node_pack_dependencies: definition
            .node_pack_dependencies
            .iter()
            .map(|dependency| WorkflowDependencyDto {
                id: dependency.id.clone(),
                version: dependency.version.clone(),
                hash: None,
            })
            .collect(),
        metadata: metadata_response(&definition.metadata),
        nested_subgraphs: definition
            .nested_subgraphs
            .iter()
            .map(|(id, nested)| (id.clone(), workflow_definition_dto(nested)))
            .collect(),
        hash: definition.hash(),
    }
}

fn workflow_port_dto(port: &WorkflowPort) -> WorkflowPortDto {
    WorkflowPortDto {
        id: port.id.clone(),
        name: port.name.clone(),
        direction: port.direction,
        node_id: port.node_id.to_string(),
        port_id: port.port_id.clone(),
        data_type: port.data_type.clone(),
        required: port.required,
    }
}

fn workflow_dependency_dto(dependency: &SubgraphDependency) -> WorkflowDependencyDto {
    WorkflowDependencyDto {
        id: dependency.id.clone(),
        version: dependency.version.clone(),
        hash: (!dependency.hash.is_empty()).then(|| dependency.hash.clone()),
    }
}

fn dependency_report_dto(report: DependencyReport) -> DependencyReportDto {
    let diagnostics = |items: Vec<rawweave_graph::DependencyDiagnostic>| {
        items
            .into_iter()
            .map(|item| DependencyDiagnosticDto {
                id: item.id,
                required_version: item.required_version,
                available_version: item.available_version,
            })
            .collect()
    };
    let ids = report
        .available
        .iter()
        .chain(report.missing.iter())
        .chain(report.mismatched.iter())
        .map(|diagnostic| diagnostic.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let statuses = ids
        .into_iter()
        .filter_map(|id| {
            let status = report.status(&id)?;
            let status = match status {
                DependencyStatus::Available => DependencyStatusDto::Available,
                DependencyStatus::Missing => DependencyStatusDto::Missing,
                DependencyStatus::VersionMismatch {
                    required,
                    available,
                } => DependencyStatusDto::VersionMismatch {
                    required,
                    available,
                },
            };
            Some((id, status))
        })
        .collect();
    DependencyReportDto {
        available: diagnostics(report.available),
        missing: diagnostics(report.missing),
        mismatched: diagnostics(report.mismatched),
        disabled_nodes: report.disabled_nodes,
        statuses,
    }
}

#[derive(Default)]
struct BatchManager {
    jobs: Mutex<BTreeMap<String, Arc<BatchEngine>>>,
}

impl BatchManager {
    fn insert(&self, job_id: String, engine: Arc<BatchEngine>) -> Result<(), String> {
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| "batch manager state is unavailable".to_owned())?;
        if jobs.contains_key(&job_id) {
            return Err(format!("batch job '{job_id}' is already loaded"));
        }
        jobs.insert(job_id, engine);
        Ok(())
    }

    fn get(&self, job_id: &str) -> Result<Arc<BatchEngine>, String> {
        self.jobs
            .lock()
            .map_err(|_| "batch manager state is unavailable".to_owned())?
            .get(job_id)
            .cloned()
            .ok_or_else(|| format!("batch job '{job_id}' is not loaded"))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CheckpointRecord {
    output_port: String,
    checkpoint: Checkpoint,
    #[serde(skip)]
    progress: Option<f32>,
}

#[derive(Clone, Debug)]
struct RemoteAiTask {
    generation: GenerationToken,
    provider_id: String,
    task_id: String,
}

struct CheckpointManager {
    checkpoints: Mutex<BTreeMap<String, CheckpointRecord>>,
    store: ArtifactStore,
    cancellation_requests: Mutex<BTreeMap<String, GenerationToken>>,
    remote_ai_tasks: Mutex<BTreeMap<String, RemoteAiTask>>,
    state_path: Option<PathBuf>,
}

const CHECKPOINT_STATE_MAX_BYTES: usize = 8 * 1024 * 1024;

static CHECKPOINT_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static CHECKPOINT_TARGET_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> =
    OnceLock::new();

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::memory()
    }
}

impl CheckpointManager {
    fn clear(&self) -> Result<(), String> {
        self.checkpoints
            .lock()
            .map_err(|_| "checkpoint state is unavailable".to_owned())?
            .clear();
        self.cancellation_requests
            .lock()
            .map_err(|_| "checkpoint cancellation state is unavailable".to_owned())?
            .clear();
        self.remote_ai_tasks
            .lock()
            .map_err(|_| "checkpoint remote task state is unavailable".to_owned())?
            .clear();
        self.persist()
    }

    fn memory() -> Self {
        Self {
            checkpoints: Mutex::new(BTreeMap::new()),
            store: ArtifactStore::memory(),
            cancellation_requests: Mutex::new(BTreeMap::new()),
            remote_ai_tasks: Mutex::new(BTreeMap::new()),
            state_path: None,
        }
    }

    fn persistent(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        let state_path = root.join("checkpoints.json");
        let checkpoints = load_checkpoint_records(&state_path)?;
        Ok(Self {
            checkpoints: Mutex::new(checkpoints),
            store: ArtifactStore::new(root.join("artifacts")),
            cancellation_requests: Mutex::new(BTreeMap::new()),
            remote_ai_tasks: Mutex::new(BTreeMap::new()),
            state_path: Some(state_path),
        })
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.state_path else {
            return Ok(());
        };
        let checkpoints = self
            .checkpoints
            .lock()
            .map_err(|_| "checkpoint state is unavailable".to_owned())?
            .clone();
        let bytes = serde_json::to_vec_pretty(&checkpoints)
            .map_err(|error| format!("could not serialize checkpoint state: {error}"))?;
        write_checkpoint_state(path, &bytes)
    }

    fn replace_from_graph(&self, editor: &EditorCore) -> Result<(), String> {
        let records = editor
            .graph()
            .nodes()
            .values()
            .filter(|node| node.descriptor.evaluation_policy == EvaluationPolicy::ManualCheckpoint)
            .filter_map(|node| {
                let checkpoint = editor.graph().checkpoint(&node.id).ok().flatten()?;
                let output_port = node.descriptor.outputs.first()?.id.clone();
                Some((
                    node.id.as_str().to_owned(),
                    CheckpointRecord {
                        output_port,
                        checkpoint,
                        progress: None,
                    },
                ))
            })
            .collect::<BTreeMap<_, _>>();
        *self
            .checkpoints
            .lock()
            .map_err(|_| "checkpoint state is unavailable".to_owned())? = records;
        self.cancellation_requests
            .lock()
            .map_err(|_| "checkpoint cancellation state is unavailable".to_owned())?
            .clear();
        self.remote_ai_tasks
            .lock()
            .map_err(|_| "checkpoint remote task state is unavailable".to_owned())?
            .clear();
        self.persist()
    }

    fn register_remote_ai_task(
        &self,
        node_id: &str,
        generation: GenerationToken,
        provider_id: String,
        task_id: String,
    ) -> Result<bool, String> {
        let cancellation_requested = self
            .cancellation_requests
            .lock()
            .map_err(|_| "checkpoint cancellation state is unavailable".to_owned())?
            .get(node_id)
            .is_some_and(|requested| *requested == generation);
        self.remote_ai_tasks
            .lock()
            .map_err(|_| "checkpoint remote task state is unavailable".to_owned())?
            .insert(
                node_id.to_owned(),
                RemoteAiTask {
                    generation,
                    provider_id,
                    task_id,
                },
            );
        Ok(cancellation_requested)
    }

    fn take_remote_ai_task(
        &self,
        node_id: &str,
        generation: GenerationToken,
    ) -> Result<Option<RemoteAiTask>, String> {
        let mut tasks = self
            .remote_ai_tasks
            .lock()
            .map_err(|_| "checkpoint remote task state is unavailable".to_owned())?;
        if tasks
            .get(node_id)
            .is_some_and(|task| task.generation == generation)
        {
            Ok(tasks.remove(node_id))
        } else {
            Ok(None)
        }
    }

    fn clear_remote_ai_task(
        &self,
        node_id: &str,
        generation: GenerationToken,
    ) -> Result<(), String> {
        let _ = self.take_remote_ai_task(node_id, generation)?;
        Ok(())
    }
}

fn load_checkpoint_records(path: &Path) -> Result<BTreeMap<String, CheckpointRecord>, String> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let metadata = std::fs::metadata(path).map_err(|error| {
        format!(
            "could not inspect checkpoint state '{}': {error}",
            path.display()
        )
    })?;
    if metadata.len() > CHECKPOINT_STATE_MAX_BYTES as u64 {
        return Err(format!(
            "checkpoint state '{}' exceeds the {CHECKPOINT_STATE_MAX_BYTES}-byte limit",
            path.display()
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len().try_into().unwrap_or(0));
    File::open(path)
        .map_err(|error| {
            format!(
                "could not open checkpoint state '{}': {error}",
                path.display()
            )
        })?
        .take((CHECKPOINT_STATE_MAX_BYTES as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!(
                "could not read checkpoint state '{}': {error}",
                path.display()
            )
        })?;
    if bytes.len() > CHECKPOINT_STATE_MAX_BYTES {
        return Err(format!(
            "checkpoint state '{}' exceeds the {CHECKPOINT_STATE_MAX_BYTES}-byte limit",
            path.display()
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "could not parse checkpoint state '{}': {error}",
            path.display()
        )
    })
}

fn checkpoint_temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("checkpoints.json");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = CHECKPOINT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".{file_name}.{}.{}.{}.tmp",
        std::process::id(),
        timestamp,
        sequence
    ))
}

fn checkpoint_target_lock(path: &Path) -> Arc<Mutex<()>> {
    let locks = CHECKPOINT_TARGET_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_owned(), Arc::downgrade(&lock));
    lock
}

#[cfg(unix)]
fn sync_checkpoint_parent_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_checkpoint_parent_directory(_path: &Path) {}

fn write_checkpoint_state(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "could not create checkpoint state directory '{}': {error}",
            parent.display()
        )
    })?;
    let target_lock = checkpoint_target_lock(path);
    let _target_guard = target_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temporary = checkpoint_temporary_path(path);
    let result: Result<(), String> = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| {
                format!("could not create checkpoint state temporary file: {error}")
            })?;
        file.write_all(bytes)
            .map_err(|error| format!("could not write checkpoint state: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("could not sync checkpoint state: {error}"))?;
        drop(file);
        std::fs::rename(&temporary, path)
            .map_err(|error| format!("could not install checkpoint state: {error}"))?;
        sync_checkpoint_parent_directory(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod checkpoint_persistence_tests {
    use super::{checkpoint_target_lock, write_checkpoint_state};
    use std::sync::{
        atomic::{AtomicBool, Ordering as AtomicOrdering},
        Arc, Barrier,
    };
    use std::thread;
    use std::time::Duration;

    #[test]
    fn checkpoint_writers_for_same_target_are_serialized() {
        let root = std::env::temp_dir().join(format!(
            "rawweave-checkpoint-serialized-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("checkpoints.json");
        let target_lock = checkpoint_target_lock(&path);
        let target_guard = target_lock.lock().unwrap();
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (finished_sender, finished_receiver) = std::sync::mpsc::channel();
        let writer_path = path.clone();
        let handle = thread::spawn(move || {
            started_sender.send(()).unwrap();
            finished_sender
                .send(write_checkpoint_state(&writer_path, b"{\"writer\":1}"))
                .unwrap();
        });

        started_receiver.recv().unwrap();
        assert!(finished_receiver
            .recv_timeout(Duration::from_millis(100))
            .is_err());
        drop(target_guard);
        assert!(finished_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_ok());
        handle.join().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_checkpoint_writers_install_complete_payloads() {
        let root = std::env::temp_dir().join(format!(
            "rawweave-checkpoint-concurrent-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("checkpoints.json");
        let writer_count = 16;
        let payloads = (0..writer_count)
            .map(|writer| {
                serde_json::to_vec(&serde_json::json!({
                    "writer": writer,
                    "payload": "x".repeat(512 * 1024),
                }))
                .unwrap()
            })
            .collect::<Vec<_>>();
        write_checkpoint_state(&path, &payloads[0]).unwrap();
        let reading = Arc::new(AtomicBool::new(true));
        let readers = (0..4)
            .map(|_| {
                let reading = Arc::clone(&reading);
                let path = path.clone();
                thread::spawn(move || {
                    while reading.load(AtomicOrdering::Acquire) {
                        let saved = std::fs::read(&path).unwrap();
                        let document: serde_json::Value = serde_json::from_slice(&saved).unwrap();
                        assert_eq!(
                            document
                                .get("payload")
                                .and_then(serde_json::Value::as_str)
                                .map(str::len),
                            Some(512 * 1024)
                        );
                    }
                })
            })
            .collect::<Vec<_>>();
        let barrier = Arc::new(Barrier::new(writer_count));
        let handles = payloads
            .into_iter()
            .map(|payload| {
                let barrier = Arc::clone(&barrier);
                let path = path.clone();
                thread::spawn(move || {
                    barrier.wait();
                    write_checkpoint_state(&path, &payload)
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let result = handle.join().unwrap();
            assert!(
                result.is_ok(),
                "concurrent checkpoint save failed: {result:?}"
            );
        }
        reading.store(false, AtomicOrdering::Release);
        for reader in readers {
            reader.join().unwrap();
        }
        let saved = std::fs::read(&path).unwrap();
        let document: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert!(document
            .get("writer")
            .and_then(serde_json::Value::as_u64)
            .is_some());
        assert_eq!(
            document
                .get("payload")
                .and_then(serde_json::Value::as_str)
                .map(str::len),
            Some(512 * 1024)
        );
        assert_eq!(
            std::fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBatchJobRequest {
    pub job: BatchJob,
    #[serde(default)]
    pub state_path: Option<PathBuf>,
    #[serde(default = "default_batch_workers")]
    pub max_workers: usize,
}

fn default_batch_workers() -> usize {
    4
}

fn validate_batch_workers(max_workers: usize) -> Result<(), String> {
    if max_workers == 0 {
        return Err("worker concurrency must be greater than zero".to_owned());
    }
    if max_workers > MAX_BATCH_WORKERS {
        return Err(format!(
            "worker concurrency cannot exceed the maximum of {MAX_BATCH_WORKERS}"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadBatchJobRequest {
    pub state_path: PathBuf,
    #[serde(default = "default_batch_workers")]
    pub max_workers: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchDryRunResponse {
    workflow_revision: u64,
    workflow_hash: String,
    item_ids: Vec<String>,
    recipes: Vec<rawweave_batch::OutputRecipe>,
}

#[derive(Default)]
pub struct AppState {
    pub editor: Arc<Mutex<EditorCore>>,
    pub(crate) hosts: Arc<Mutex<hosts::HostManager>>,
    pub preview: Arc<preview::PreviewManager>,
    pub(crate) source_image: Mutex<Option<SourceAsset>>,
    source_selection: Mutex<SourceSelectionIntent>,
    blueprint: Mutex<Option<WorkflowDefinition>>,
    blueprint_stack: Mutex<Vec<WorkflowDefinition>>,
    batch: Arc<BatchManager>,
    checkpoint: Arc<CheckpointManager>,
}

impl AppState {
    fn with_checkpoint_manager(
        checkpoint: Arc<CheckpointManager>,
        preview: Arc<preview::PreviewManager>,
    ) -> Self {
        Self {
            editor: Arc::new(Mutex::new(EditorCore::default())),
            hosts: Arc::new(Mutex::new(
                hosts::HostManager::load_default().unwrap_or_default(),
            )),
            preview,
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
            batch: Arc::new(BatchManager::default()),
            checkpoint,
        }
    }
}

fn lock_hosts(
    hosts: &Arc<Mutex<hosts::HostManager>>,
) -> Result<MutexGuard<'_, hosts::HostManager>, String> {
    hosts
        .lock()
        .map_err(|_| "external host state is unavailable".to_owned())
}

fn lock_editor(editor: &Arc<Mutex<EditorCore>>) -> Result<MutexGuard<'_, EditorCore>, String> {
    editor
        .lock()
        .map_err(|_| "editor state is unavailable".to_owned())
}

fn current_blueprint(state: &AppState) -> Result<WorkflowDefinition, String> {
    state
        .blueprint
        .lock()
        .map_err(|_| "blueprint state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "no blueprint is loaded".to_owned())
}

fn store_blueprint(state: &AppState, blueprint: WorkflowDefinition) -> Result<(), String> {
    *state
        .blueprint
        .lock()
        .map_err(|_| "blueprint state is unavailable".to_owned())? = Some(blueprint);
    Ok(())
}

fn clear_blueprint(state: &AppState) -> Result<(), String> {
    *state
        .blueprint
        .lock()
        .map_err(|_| "blueprint state is unavailable".to_owned())? = None;
    state
        .blueprint_stack
        .lock()
        .map_err(|_| "blueprint navigation state is unavailable".to_owned())?
        .clear();
    state.checkpoint.clear()?;
    Ok(())
}

fn export_blueprint_text(
    editor: &EditorCore,
    blueprint: &WorkflowDefinition,
) -> Result<String, String> {
    let bytes = editor
        .save_blueprint(blueprint)
        .map_err(|error| error.to_string())?;
    String::from_utf8(bytes)
        .map_err(|error| format!("blueprint export is not valid UTF-8: {error}"))
}

fn build_subgraph_definition(
    editor: &EditorCore,
    request: CreateSubgraphRequest,
) -> Result<WorkflowDefinition, String> {
    let selection = request
        .node_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut definition = editor
        .create_subgraph_from_selection(
            &selection,
            request.id,
            request.version,
            request.metadata.into(),
        )
        .map_err(|error| error.to_string())?;
    for dependency in request.node_pack_dependencies {
        definition
            .add_node_pack_dependency(dependency.id, dependency.version)
            .map_err(|error| error.to_string())?;
    }
    for dependency in request.subgraph_dependencies {
        definition
            .add_subgraph_dependency(SubgraphDependency::new(
                dependency.id,
                dependency.version,
                dependency.hash.unwrap_or_default(),
            ))
            .map_err(|error| error.to_string())?;
    }
    Ok(definition)
}

fn import_blueprint_definition(
    editor: &EditorCore,
    serialized: &str,
    store: &ArtifactStore,
) -> Result<WorkflowDefinition, String> {
    WorkflowDefinition::from_json_with_artifact_store(
        serialized,
        editor.graph().registry(),
        store.clone(),
    )
    .map_err(|error| error.to_string())
}

fn mutate_current_blueprint<F>(state: &AppState, mutate: F) -> Result<WorkflowDefinitionDto, String>
where
    F: FnOnce(&EditorCore, &mut WorkflowDefinition) -> Result<(), String>,
{
    let editor = lock_editor(&state.editor)?.clone();
    let mut blueprint = current_blueprint(state)?;
    mutate(&editor, &mut blueprint)?;
    blueprint.validate().map_err(|error| error.to_string())?;
    let dto = workflow_definition_dto(&blueprint);
    lock_editor(&state.editor)?
        .instantiate_blueprint(&blueprint)
        .map_err(|error| error.to_string())?;
    store_blueprint(state, blueprint)?;
    Ok(dto)
}

fn current_or_flat_blueprint(state: &AppState) -> Result<WorkflowDefinition, String> {
    if let Ok(blueprint) = current_blueprint(state) {
        return Ok(blueprint);
    }
    let editor = lock_editor(&state.editor)?;
    WorkflowDefinition::new(
        "workflow",
        "1.0.0",
        editor.graph().clone(),
        WorkflowMetadata::new("Workflow"),
    )
    .map_err(|error| error.to_string())
}

fn hash_checkpoint_input(bytes: &[u8]) -> String {
    let encoded = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hash_upstream_inputs(&BTreeMap::from([("bytes".to_owned(), encoded)]), 0)
}

fn checkpoint_source_hash(source: Option<&SourceAsset>) -> String {
    let mut inputs = BTreeMap::new();
    match source {
        Some(SourceAsset::Ordinary(image)) => {
            inputs.insert("kind".to_owned(), "ordinary".to_owned());
            inputs.insert("width".to_owned(), image.width().to_string());
            inputs.insert("height".to_owned(), image.height().to_string());
            inputs.insert(
                "pixels".to_owned(),
                image
                    .pixels()
                    .iter()
                    .flat_map(|pixel| pixel.iter())
                    .map(|channel| format!("{:08x}", channel.to_bits()))
                    .collect(),
            );
        }
        Some(SourceAsset::ImageSet(set)) => {
            inputs.insert("kind".to_owned(), "imageset".to_owned());
            let serialized = serde_json::to_vec(set).unwrap_or_default();
            inputs.insert("value".to_owned(), hash_checkpoint_input(&serialized));
        }
        Some(SourceAsset::Raw { bytes, path }) => {
            inputs.insert("kind".to_owned(), "raw".to_owned());
            inputs.insert("bytes".to_owned(), hash_checkpoint_input(bytes));
            inputs.insert("path".to_owned(), path.to_string_lossy().into_owned());
        }
        None => {
            inputs.insert("kind".to_owned(), "none".to_owned());
        }
    }
    hash_upstream_inputs(&inputs, 0)
}

fn checkpoint_dependencies(
    editor: &EditorCore,
    node_id: &str,
    output_port: &str,
    node_version: u32,
    source: Option<&SourceAsset>,
) -> Result<(String, BTreeMap<String, String>), String> {
    let workflow = editor
        .graph()
        .to_dependency_json()
        .map_err(|error| error.to_string())?;
    let upstream_hashes = BTreeMap::from([
        (
            "workflow".to_owned(),
            hash_checkpoint_input(workflow.as_bytes()),
        ),
        ("node".to_owned(), hash_checkpoint_input(node_id.as_bytes())),
        (
            "output".to_owned(),
            hash_checkpoint_input(output_port.as_bytes()),
        ),
        ("source".to_owned(), checkpoint_source_hash(source)),
    ]);
    Ok((
        hash_upstream_inputs(&upstream_hashes, node_version),
        upstream_hashes,
    ))
}

fn checkpoint_context(source: Option<&SourceAsset>) -> EvaluationContext {
    match source {
        Some(SourceAsset::Ordinary(image)) => EvaluationContext::with_source_image(image.clone()),
        Some(SourceAsset::ImageSet(set)) => {
            EvaluationContext::default().with_source_image_set(set.as_ref().clone())
        }
        Some(SourceAsset::Raw { bytes, path }) => EvaluationContext::default()
            .with_source_bytes(Arc::clone(bytes))
            .with_source_path(path),
        None => EvaluationContext::default(),
    }
}

fn checkpoint_payload(value: Value) -> Result<CheckpointPayload, String> {
    match value {
        Value::Image(image) => Ok(CheckpointPayload::Image(image)),
        Value::ImageSet(set) => Ok(CheckpointPayload::ImageSet(Box::new(set))),
        Value::Mask(mask) => Ok(CheckpointPayload::Mask(mask)),
        Value::MaskSet(set) => Ok(CheckpointPayload::MaskSet(set)),
        Value::LabelMap(map) => Ok(CheckpointPayload::LabelMap(map)),
        Value::ConfidenceMap(map) => Ok(CheckpointPayload::ConfidenceMap(map)),
        Value::DepthMap(map) => Ok(CheckpointPayload::DepthMap(map)),
        Value::RegionSet(set) => Ok(CheckpointPayload::RegionSet(set)),
        Value::Bytes(bytes) => Ok(CheckpointPayload::SpatialData(bytes)),
        value => Err(format!(
            "checkpoint output type '{}' is not persistable",
            value.data_type()
        )),
    }
}

#[derive(Debug)]
enum CheckpointGeneration {
    Local(CheckpointPayload),
    Provider(AiResult),
}

fn parameter_json(value: &ParameterValue) -> serde_json::Value {
    match value {
        ParameterValue::Float(value) => serde_json::json!(value),
        ParameterValue::Integer(value) => serde_json::json!(value),
        ParameterValue::Boolean(value) => serde_json::json!(value),
        ParameterValue::String(value) => serde_json::Value::String(value.clone()),
    }
}

fn ai_parameter_string(
    node: &rawweave_graph::GraphNode,
    parameter: &str,
) -> Result<String, String> {
    match node.parameters.get(parameter) {
        Some(ParameterValue::String(value)) if !value.trim().is_empty() => Ok(value.clone()),
        Some(_) => Err(format!(
            "AI node parameter '{parameter}' must be a non-empty string"
        )),
        None => Err(format!("AI node is missing parameter '{parameter}'")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AiCheckpointSpec {
    operation: AiOperation,
    output_type: &'static str,
    /// The stable PNG interpretation requested from providers that return bytes.
    encoding: &'static str,
}

fn ai_operation_for_type(type_id: &str) -> Result<AiOperation, String> {
    match type_id {
        "ai.img2img" => Ok(AiOperation::Img2Img),
        "ai.inpaint" => Ok(AiOperation::Inpaint),
        "ai.generative-fill" => Ok(AiOperation::GenerativeFill),
        "ai.upscale" => Ok(AiOperation::Upscale),
        // The provider-neutral protocol currently has no segmentation
        // operation. Spatial workflows are provider-defined image jobs and use
        // Img2Img while output_type/output_encoding select the typed artifact.
        "ai.subject-segmentation"
        | "ai.semantic-segmentation"
        | "ai.prompt-segmentation"
        | "ai.scene-analysis" => Ok(AiOperation::Img2Img),
        _ => Err(format!("unsupported AI checkpoint node '{type_id}'")),
    }
}

fn ai_checkpoint_spec(type_id: &str, output_port: &str) -> Result<AiCheckpointSpec, String> {
    let operation = ai_operation_for_type(type_id)?;
    let (output_type, encoding) = match (type_id, output_port) {
        ("ai.img2img" | "ai.inpaint" | "ai.generative-fill" | "ai.upscale", "image") => {
            ("core.Image", "png-image-v1")
        }
        ("ai.subject-segmentation", "mask") | ("ai.prompt-segmentation", "mask") => {
            ("core.Mask", "png-mask-v1")
        }
        ("ai.subject-segmentation", "mask_set") => ("core.MaskSet", "png-mask-set-v1"),
        ("ai.subject-segmentation", "confidence")
        | ("ai.prompt-segmentation", "confidence")
        | ("ai.semantic-segmentation", "confidence")
        | ("ai.scene-analysis", "confidence") => ("core.ConfidenceMap", "png-confidence-v1"),
        ("ai.semantic-segmentation", "label_map") | ("ai.scene-analysis", "label_map") => {
            ("core.LabelMap", "png-label-map-v1")
        }
        ("ai.scene-analysis", "regions") => ("core.RegionSet", "png-region-set-v1"),
        (type_id, output_port) => {
            return Err(format!(
                "unsupported AI checkpoint output '{type_id}:{output_port}'"
            ));
        }
    };
    Ok(AiCheckpointSpec {
        operation,
        output_type,
        encoding,
    })
}

fn spatial_encoding_for_output(output_type: &str) -> Option<&'static str> {
    Some(match output_type {
        "core.Mask" => "png-mask-v1",
        "core.MaskSet" => "png-mask-set-v1",
        "core.LabelMap" => "png-label-map-v1",
        "core.ConfidenceMap" => "png-confidence-v1",
        "core.DepthMap" => "png-depth-v1",
        "core.RegionSet" => "png-region-set-v1",
        _ => return None,
    })
}

fn source_ai_image(context: &EvaluationContext) -> Result<AiImage, String> {
    let Some(image) = context.source_image.as_ref() else {
        return Err("AI checkpoint generation requires an ordinary image source".to_owned());
    };
    ai_image_from_image(image)
}

fn ai_image_from_image(image: &Image) -> Result<AiImage, String> {
    AiImage::new(
        preview::encode_png(image)?,
        [image.width(), image.height()],
        ColorInterchange::png_srgb(),
    )
    .map_err(|error| error.to_string())
}

fn ai_mask_from_mask(mask: &Mask) -> Result<AiImage, String> {
    let image = Image::from_pixels(
        mask.width(),
        mask.height(),
        mask.values()
            .into_iter()
            .map(|value| [value, value, value, 1.0])
            .collect(),
    )
    .map_err(|error| format!("could not create AI mask image: {error}"))?;
    AiImage::new(
        preview::encode_png(&image)?,
        [mask.width(), mask.height()],
        ColorInterchange::png_srgb(),
    )
    .map_err(|error| error.to_string())
}

fn ai_input_from_value(value: Value, data_type: &str) -> Result<AiInput, String> {
    match (data_type, value) {
        ("core.Image", Value::Image(image)) => Ok(AiInput::Image(ai_image_from_image(&image)?)),
        ("core.Mask", Value::Mask(mask)) => Ok(AiInput::Mask(ai_mask_from_mask(&mask)?)),
        ("value.String", Value::String(value)) => Ok(AiInput::Text(value)),
        (expected, value) => Err(format!(
            "AI input expected '{expected}', got '{}'",
            value.data_type()
        )),
    }
}

fn connected_ai_input(
    editor: &EditorCore,
    node_id: &NodeId,
    port_id: &str,
    context: &EvaluationContext,
) -> Result<Option<Value>, String> {
    let Some(edge) = editor
        .graph()
        .edges()
        .iter()
        .find(|edge| edge.to_node == *node_id && edge.to_port == port_id)
    else {
        return Ok(None);
    };
    editor
        .graph()
        .evaluate(&edge.from_node, &edge.from_port, context)
        .map(Some)
        .map_err(|error| {
            format!(
                "could not evaluate connected AI input '{}:{}': {error}",
                edge.from_node, edge.from_port
            )
        })
}

#[cfg(test)]
fn build_ai_submit_request(
    editor: &EditorCore,
    node_id: &str,
    input_port: &str,
    context: &EvaluationContext,
    dependency_hash: &str,
) -> Result<SubmitRequest, String> {
    let output_port = editor
        .graph()
        .node(&NodeId::from(node_id))
        .and_then(|node| node.descriptor.outputs.first())
        .map(|port| port.id.as_str())
        .ok_or_else(|| format!("AI node '{node_id}' has no outputs"))?;
    build_ai_submit_request_for_output(
        editor,
        node_id,
        input_port,
        output_port,
        context,
        dependency_hash,
    )
}

fn build_ai_submit_request_for_output(
    editor: &EditorCore,
    node_id: &str,
    input_port: &str,
    output_port: &str,
    context: &EvaluationContext,
    dependency_hash: &str,
) -> Result<SubmitRequest, String> {
    let node = editor
        .graph()
        .node(&NodeId::from(node_id))
        .ok_or_else(|| format!("AI node '{node_id}' does not exist"))?;
    let spec = ai_checkpoint_spec(&node.type_id, output_port)?;
    let provider_id = ai_parameter_string(node, "provider_id")?;
    let workflow_id = ai_parameter_string(node, "workflow_id")?;
    let workflow_definition = ai_parameter_string(node, "workflow_definition")?;
    let workflow_definition: serde_json::Value = serde_json::from_str(&workflow_definition)
        .map_err(|error| format!("AI workflow definition is not valid JSON: {error}"))?;
    let workflow_version = workflow_definition
        .get("version")
        .and_then(serde_json::Value::as_str)
        .filter(|version| !version.trim().is_empty())
        .unwrap_or("1")
        .to_owned();
    let bindings = workflow_definition
        .get("bindings")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| format!("AI workflow bindings are invalid: {error}"))?
        .unwrap_or_else(WorkflowBindings::default);
    let model = node
        .parameters
        .get("model")
        .and_then(|value| match value {
            ParameterValue::String(value) if !value.trim().is_empty() => Some(value.clone()),
            _ => None,
        })
        .or_else(|| {
            workflow_definition
                .get("model")
                .and_then(serde_json::Value::as_str)
                .filter(|model| !model.trim().is_empty())
                .map(str::to_owned)
        });
    let workflow = AiWorkflow::new(workflow_id, workflow_version, workflow_definition, bindings);
    let provenance = TaskProvenance::new(node_id, node.descriptor.version, dependency_hash)
        .with_provider(provider_id.clone())
        .with_workflow_hash(
            workflow
                .content_hash()
                .map_err(|error| format!("could not hash AI workflow: {error}"))?,
        );
    let mut request = SubmitRequest::new(workflow, provenance)
        .with_parameter("operation", serde_json::to_value(spec.operation).unwrap())
        .with_parameter(
            "output_port",
            serde_json::Value::String(output_port.to_owned()),
        )
        .with_parameter(
            "output_type",
            serde_json::Value::String(spec.output_type.to_owned()),
        )
        .with_parameter(
            "output_encoding",
            serde_json::Value::String(spec.encoding.to_owned()),
        );
    for port in &node.descriptor.inputs {
        let connected = connected_ai_input(editor, &NodeId::from(node_id), &port.id, context)?;
        let input = match connected {
            Some(value) => Some(ai_input_from_value(value, &port.data_type)?),
            None if port.id == "prompt" => match node.parameters.get("prompt") {
                Some(ParameterValue::String(prompt)) => Some(AiInput::Text(prompt.clone())),
                Some(_) => return Err("AI prompt parameter must be a string".to_owned()),
                None if port.required => {
                    return Err(format!("required AI input '{}' is not connected", port.id));
                }
                None => None,
            },
            None if port.required => {
                return Err(format!("required AI input '{}' is not connected", port.id));
            }
            None if port.data_type == "core.Image" => {
                Some(AiInput::Image(source_ai_image(context)?))
            }
            None => None,
        };
        if let Some(input) = input {
            request = request.with_input(port.id.clone(), input);
        }
    }
    if !request.inputs.contains_key(input_port) {
        return Err(format!("AI input '{input_port}' is not available"));
    }
    for (parameter, value) in &node.parameters {
        if matches!(
            parameter.as_str(),
            "provider_id" | "workflow_id" | "workflow_definition"
        ) {
            continue;
        }
        request = request.with_parameter(parameter, parameter_json(value));
    }
    if let Some(model) = model {
        request = request.with_parameter("model", serde_json::Value::String(model));
    }
    if let Some(ParameterValue::String(prompt)) = node.parameters.get("prompt") {
        request = request.with_input("prompt", AiInput::Text(prompt.clone()));
    }
    Ok(request)
}

fn ai_mask_node_ids(editor: &EditorCore, node_id: &str) -> Vec<String> {
    let node_id = NodeId::from(node_id);
    editor
        .graph()
        .edges()
        .iter()
        .filter(|edge| edge.to_node == node_id && edge.to_port == "mask")
        .filter_map(|edge| {
            let node = editor.graph().node(&edge.from_node)?;
            node.type_id
                .starts_with("ai.")
                .then(|| edge.from_node.as_str().to_owned())
        })
        .collect()
}

const AI_SPATIAL_MAX_PIXELS: usize = 16_777_216;
const AI_PROVIDER_MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
const AI_SPATIAL_MAX_MASKS: usize = 1024;
const AI_SPATIAL_MAX_REGIONS: usize = 1_000_000;

#[derive(Debug, Deserialize)]
struct FloatPlaneDocument {
    #[serde(rename = "type", default)]
    kind: String,
    width: u32,
    height: u32,
    #[serde(default)]
    origin: [u32; 2],
    values: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct LabelPlaneDocument {
    #[serde(rename = "type", default)]
    kind: String,
    width: u32,
    height: u32,
    #[serde(default)]
    origin: [u32; 2],
    values: Vec<u16>,
    #[serde(default)]
    labels: BTreeMap<String, u16>,
}

#[derive(Debug, Deserialize)]
struct MaskSetDocument {
    #[serde(rename = "type", default)]
    kind: String,
    masks: Vec<FloatPlaneDocument>,
}

#[derive(Debug, Deserialize)]
struct RegionDocument {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
struct RegionSetDocument {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    regions: Vec<RegionDocument>,
}

fn provider_result_bytes(result: &AiResult) -> Result<(&[u8], &str), String> {
    match &result.output {
        AiOutput::Image(image) => Ok((&image.bytes, "image/png")),
        AiOutput::Bytes { bytes, media_type } => Ok((bytes, media_type.as_str())),
    }
}

fn spatial_dimensions(width: u32, height: u32) -> Result<Dimensions, String> {
    if width == 0 || height == 0 {
        return Err("AI spatial output dimensions must be non-zero".to_owned());
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "AI spatial output dimensions overflow".to_owned())?;
    if pixels > AI_SPATIAL_MAX_PIXELS as u64 {
        return Err(format!(
            "AI spatial output exceeds {AI_SPATIAL_MAX_PIXELS} pixels"
        ));
    }
    Ok(Dimensions::new(width, height))
}

fn check_document_kind(kind: &str, expected: &str) -> Result<(), String> {
    if !kind.is_empty() && kind != expected {
        return Err(format!(
            "AI structured output type '{kind}' does not match '{expected}'"
        ));
    }
    Ok(())
}

fn float_plane_payload(
    document: FloatPlaneDocument,
    expected_kind: &str,
) -> Result<CheckpointPayload, String> {
    check_document_kind(&document.kind, expected_kind)?;
    let dimensions = spatial_dimensions(document.width, document.height)?;
    let mask = Mask::from_values_with_origin(
        dimensions,
        (document.origin[0], document.origin[1]),
        document.values,
    )
    .map_err(|error| error.to_string())?;
    match expected_kind {
        "mask" => Ok(CheckpointPayload::Mask(mask)),
        "confidence_map" => Ok(CheckpointPayload::ConfidenceMap(ConfidenceMap::from_mask(
            mask,
        ))),
        "depth_map" => {
            let values = mask.values();
            let depth =
                DepthMap::from_values(dimensions, (document.origin[0], document.origin[1]), values)
                    .map_err(|error| error.to_string())?;
            Ok(CheckpointPayload::DepthMap(depth))
        }
        _ => Err(format!(
            "unsupported AI structured output type '{expected_kind}'"
        )),
    }
}

fn structured_provider_payload(
    bytes: &[u8],
    output_type: &str,
) -> Result<CheckpointPayload, String> {
    match output_type {
        "core.Mask" => {
            let document: FloatPlaneDocument = serde_json::from_slice(bytes)
                .map_err(|error| format!("could not decode AI structured mask: {error}"))?;
            float_plane_payload(document, "mask")
        }
        "core.ConfidenceMap" => {
            let document: FloatPlaneDocument = serde_json::from_slice(bytes).map_err(|error| {
                format!("could not decode AI structured confidence map: {error}")
            })?;
            float_plane_payload(document, "confidence_map")
        }
        "core.DepthMap" => {
            let document: FloatPlaneDocument = serde_json::from_slice(bytes)
                .map_err(|error| format!("could not decode AI structured depth map: {error}"))?;
            float_plane_payload(document, "depth_map")
        }
        "core.LabelMap" => {
            let document: LabelPlaneDocument = serde_json::from_slice(bytes)
                .map_err(|error| format!("could not decode AI structured label map: {error}"))?;
            check_document_kind(&document.kind, "label_map")?;
            let dimensions = spatial_dimensions(document.width, document.height)?;
            let map = LabelMap::from_values_with_labels(
                dimensions,
                (document.origin[0], document.origin[1]),
                document.values,
                document.labels,
            )
            .map_err(|error| error.to_string())?;
            Ok(CheckpointPayload::LabelMap(map))
        }
        "core.MaskSet" => {
            let document: MaskSetDocument = serde_json::from_slice(bytes)
                .map_err(|error| format!("could not decode AI structured mask set: {error}"))?;
            check_document_kind(&document.kind, "mask_set")?;
            if document.masks.is_empty() || document.masks.len() > AI_SPATIAL_MAX_MASKS {
                return Err(format!(
                    "AI structured mask set must contain between one and {AI_SPATIAL_MAX_MASKS} masks"
                ));
            }
            let mut masks = Vec::with_capacity(document.masks.len());
            for plane in document.masks {
                let payload = float_plane_payload(plane, "mask")?;
                let CheckpointPayload::Mask(mask) = payload else {
                    unreachable!("mask plane decoding is checked above")
                };
                masks.push(mask);
            }
            Ok(CheckpointPayload::MaskSet(MaskSet::new(masks)))
        }
        "core.RegionSet" => {
            let document: RegionSetDocument = serde_json::from_slice(bytes)
                .map_err(|error| format!("could not decode AI structured region set: {error}"))?;
            check_document_kind(&document.kind, "region_set")?;
            if document.regions.len() > AI_SPATIAL_MAX_REGIONS {
                return Err(format!(
                    "AI structured region set exceeds {AI_SPATIAL_MAX_REGIONS} regions"
                ));
            }
            let regions = document
                .regions
                .into_iter()
                .map(|region| {
                    if region.width == 0
                        || region.height == 0
                        || region.x.checked_add(region.width).is_none()
                        || region.y.checked_add(region.height).is_none()
                    {
                        return Err("AI structured region is empty or overflows".to_owned());
                    }
                    Ok(Region::new(region.x, region.y, region.width, region.height))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CheckpointPayload::RegionSet(RegionSet::new(regions)))
        }
        _ => Err(format!(
            "unsupported AI checkpoint output type '{output_type}'"
        )),
    }
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err("AI spatial byte output must use PNG encoding".to_owned());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    let _ = spatial_dimensions(width, height)?;
    Ok((width, height))
}

fn provider_png(result: &AiResult) -> Result<image::Rgba32FImage, String> {
    let (bytes, media_type) = provider_result_bytes(result)?;
    let media_type = media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase();
    if media_type != "image/png" {
        return Err(format!(
            "AI provider spatial byte output must use image/png, got '{media_type}'"
        ));
    }
    if bytes.is_empty() {
        return Err("AI provider returned no output bytes".to_owned());
    }
    if bytes.len() > AI_PROVIDER_MAX_RESULT_BYTES {
        return Err(format!(
            "AI provider output exceeds {AI_PROVIDER_MAX_RESULT_BYTES} bytes"
        ));
    }
    let (width, height) = png_dimensions(bytes)?;
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| format!("could not decode AI provider PNG: {error}"))?
        .to_rgba32f();
    if decoded.width() != width || decoded.height() != height {
        return Err("AI provider PNG dimensions changed while decoding".to_owned());
    }
    Ok(decoded)
}

fn provider_scalar(pixel: [f32; 4]) -> f32 {
    let [red, green, blue, alpha] = pixel;
    if alpha < 1.0 {
        alpha.clamp(0.0, 1.0)
    } else {
        (0.2126 * red + 0.7152 * green + 0.0722 * blue).clamp(0.0, 1.0)
    }
}

fn provider_plane(decoded: &image::Rgba32FImage) -> Vec<f32> {
    decoded
        .pixels()
        .map(|pixel| provider_scalar(pixel.0))
        .collect()
}

fn provider_regions(decoded: &image::Rgba32FImage) -> Vec<Region> {
    let mut min_x = decoded.width();
    let mut min_y = decoded.height();
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;
    for (index, pixel) in decoded.pixels().enumerate() {
        if provider_scalar(pixel.0) <= 0.0 {
            continue;
        }
        let x = index as u32 % decoded.width();
        let y = index as u32 / decoded.width();
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        found = true;
    }
    found
        .then(|| Region::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
        .into_iter()
        .collect()
}

fn provider_result_payload(
    result: &AiResult,
    output_type: &str,
) -> Result<CheckpointPayload, String> {
    let (bytes, media_type) = provider_result_bytes(result)?;
    let media_type = media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase();
    if media_type == "application/json" {
        if bytes.len() > AI_PROVIDER_MAX_RESULT_BYTES {
            return Err(format!(
                "AI provider output exceeds {AI_PROVIDER_MAX_RESULT_BYTES} bytes"
            ));
        }
        return structured_provider_payload(bytes, output_type);
    }
    if !matches!(
        output_type,
        "core.Image"
            | "core.Mask"
            | "core.MaskSet"
            | "core.LabelMap"
            | "core.ConfidenceMap"
            | "core.DepthMap"
            | "core.RegionSet"
    ) {
        return Err(format!(
            "unsupported AI checkpoint output type '{output_type}'"
        ));
    }
    let decoded = provider_png(result)?;
    let dimensions = spatial_dimensions(decoded.width(), decoded.height())?;
    let values = provider_plane(&decoded);
    match output_type {
        "core.Image" => {
            let image = Image::from_pixels(
                decoded.width(),
                decoded.height(),
                decoded.pixels().map(|pixel| pixel.0).collect(),
            )
            .map_err(|error| format!("could not create AI checkpoint image: {error}"))?;
            Ok(CheckpointPayload::Image(image))
        }
        "core.Mask" => Mask::from_values(dimensions, values)
            .map(CheckpointPayload::Mask)
            .map_err(|error| format!("could not create AI checkpoint mask: {error}")),
        "core.MaskSet" => Mask::from_values(dimensions, values)
            .map(|mask| CheckpointPayload::MaskSet(MaskSet::new(vec![mask])))
            .map_err(|error| format!("could not create AI checkpoint mask set: {error}")),
        "core.LabelMap" => {
            let labels = values
                .into_iter()
                .map(|value| (value * 255.0).round() as u16)
                .collect();
            LabelMap::from_values(dimensions, (0, 0), labels)
                .map(CheckpointPayload::LabelMap)
                .map_err(|error| format!("could not create AI checkpoint label map: {error}"))
        }
        "core.ConfidenceMap" => Mask::from_values(dimensions, values)
            .map(|mask| CheckpointPayload::ConfidenceMap(ConfidenceMap::from_mask(mask)))
            .map_err(|error| format!("could not create AI checkpoint confidence map: {error}")),
        "core.DepthMap" => DepthMap::from_values(dimensions, (0, 0), values)
            .map(CheckpointPayload::DepthMap)
            .map_err(|error| format!("could not create AI checkpoint depth map: {error}")),
        "core.RegionSet" => Ok(CheckpointPayload::RegionSet(RegionSet::new(
            provider_regions(&decoded),
        ))),
        _ => unreachable!("output type checked above"),
    }
}

struct ProviderArtifactContext<'a> {
    output_type: &'a str,
    mask_node_ids: &'a [String],
    model: Option<&'a str>,
}

fn provider_checkpoint_artifact(
    result: &AiResult,
    dependency_hash: &str,
    upstream_hashes: BTreeMap<String, String>,
    node_version: u32,
    generation_revision: u64,
    context: &ProviderArtifactContext<'_>,
) -> Result<CheckpointArtifact, String> {
    let provider_id = result
        .provenance
        .provider_id
        .clone()
        .unwrap_or_else(|| "unknown-ai-provider".to_owned());
    let mut parameters = BTreeMap::new();
    parameters.insert("task_id".to_owned(), result.task_id.clone());
    parameters.insert("output_type".to_owned(), context.output_type.to_owned());
    if let Some(encoding) = spatial_encoding_for_output(context.output_type) {
        parameters.insert("output_encoding".to_owned(), encoding.to_owned());
    }
    if !result.provenance.workflow_id.is_empty() {
        parameters.insert(
            "workflow_id".to_owned(),
            result.provenance.workflow_id.clone(),
        );
    }
    if !result.provenance.workflow_version.is_empty() {
        parameters.insert(
            "workflow_version".to_owned(),
            result.provenance.workflow_version.clone(),
        );
    }
    if let Some(workflow_hash) = result.provenance.workflow_hash.as_ref() {
        parameters.insert("workflow_hash".to_owned(), workflow_hash.clone());
    }
    if !context.mask_node_ids.is_empty() {
        parameters.insert(
            "mask_node_ids".to_owned(),
            serde_json::to_string(context.mask_node_ids).unwrap_or_else(|_| "[]".to_owned()),
        );
    }
    let external_tool = ExternalToolMetadata {
        id: provider_id,
        version: "unknown".to_owned(),
        model: context.model.map(str::to_owned),
        parameters,
    };
    CheckpointArtifact::new(
        provider_result_payload(result, context.output_type)?,
        dependency_hash,
        Provenance {
            dependency_hash: dependency_hash.to_owned(),
            upstream_hashes,
            node_version,
            external_tool: Some(external_tool),
        },
        GenerationMetadata {
            generation_revision,
            generated_at: Some(checkpoint_timestamp()),
            duration_millis: None,
            generator: Some("rawweave-ai-provider".to_owned()),
        },
    )
    .map_err(|error| error.to_string())
}

fn checkpoint_status_dto(
    checkpoint: &Checkpoint,
    store: &ArtifactStore,
) -> Result<CheckpointStatusDto, String> {
    let availability = checkpoint
        .availability_with_store(store)
        .map_err(|error| error.to_string())?;
    let artifact = checkpoint
        .committed_artifact(store)
        .map_err(|error| error.to_string())?;
    Ok(CheckpointStatusDto {
        node_id: checkpoint.node_id.clone(),
        output_port: String::new(),
        state: checkpoint.state(),
        availability,
        current_dependency_hash: checkpoint.current_dependency_hash().map(str::to_owned),
        committed_dependency_hash: checkpoint.committed_dependency_hash().map(str::to_owned),
        committed_artifact_id: checkpoint.committed_artifact_id().map(ToString::to_string),
        generation: checkpoint.generation.clone().or_else(|| {
            artifact
                .as_ref()
                .map(|artifact| artifact.generation.clone())
        }),
        provenance: artifact.map(|artifact| artifact.provenance),
        failure: checkpoint.failure.clone(),
        progress: None,
        can_use_committed: matches!(
            availability,
            CheckpointAvailability::Fresh | CheckpointAvailability::Stale
        ),
    })
}

fn checkpoint_record_status_dto(
    record: &CheckpointRecord,
    store: &ArtifactStore,
) -> Result<CheckpointStatusDto, String> {
    let mut status = checkpoint_status_dto(&record.checkpoint, store)?;
    status.output_port = record.output_port.clone();
    status.progress = record.progress;
    Ok(status)
}

fn ensure_checkpoint_record<'a>(
    records: &'a mut BTreeMap<String, CheckpointRecord>,
    editor: &EditorCore,
    node_id: &str,
    output_port: Option<&str>,
) -> Result<&'a mut CheckpointRecord, String> {
    let node = editor
        .graph()
        .node(&NodeId::from(node_id))
        .ok_or_else(|| format!("node '{node_id}' does not exist"))?;
    if node.descriptor.evaluation_policy != EvaluationPolicy::ManualCheckpoint {
        return Err(format!("node '{node_id}' is not a manual checkpoint"));
    }
    let selected_port = output_port
        .map(str::to_owned)
        .or_else(|| {
            records
                .get(node_id)
                .map(|record| record.output_port.clone())
        })
        .or_else(|| node.descriptor.outputs.first().map(|port| port.id.clone()))
        .ok_or_else(|| format!("manual checkpoint node '{node_id}' has no outputs"))?;
    if node.descriptor.output(&selected_port).is_none() {
        return Err(format!("output '{node_id}:{selected_port}' does not exist"));
    }
    let graph_checkpoint = editor
        .graph()
        .checkpoint(&NodeId::from(node_id))
        .map_err(|error| error.to_string())?
        .filter(|checkpoint| checkpoint.node_version == node.descriptor.version);
    let entry = records
        .entry(node_id.to_owned())
        .or_insert_with(|| CheckpointRecord {
            output_port: selected_port.clone(),
            checkpoint: graph_checkpoint
                .clone()
                .unwrap_or_else(|| Checkpoint::new(node_id, node.descriptor.version)),
            progress: None,
        });
    if entry.checkpoint.node_version != node.descriptor.version
        || entry.output_port != selected_port
    {
        *entry = CheckpointRecord {
            output_port: selected_port.clone(),
            checkpoint: Checkpoint::new(node_id, node.descriptor.version),
            progress: None,
        };
    } else {
        entry.output_port = selected_port;
    }
    Ok(entry)
}

fn emit_checkpoint_progress(
    app: &AppHandle,
    node_id: &str,
    output_port: &str,
    progress: f32,
    phase: &str,
    message: Option<String>,
) {
    let _ = app.emit(
        "checkpoint-progress",
        CheckpointProgressEvent {
            node_id: node_id.to_owned(),
            output_port: output_port.to_owned(),
            progress,
            phase: phase.to_owned(),
            message,
        },
    );
}

fn checkpoint_timestamp() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("unix-millis:{millis}")
}

fn available_manifests(packs: Vec<AvailableNodePackDto>) -> Vec<NodePackManifest> {
    packs
        .into_iter()
        .map(|pack| {
            pack.nodes.into_iter().fold(
                NodePackManifest::new(pack.package_id.clone(), pack.version.clone()),
                |manifest, node| {
                    manifest.with_node(rawweave_graph::NodeManifest::new(
                        node.type_id,
                        node.version,
                    ))
                },
            )
        })
        .collect()
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

fn read_bounded_raw<R: Read>(
    reader: R,
    label: &str,
    max_input_bytes: usize,
) -> Result<Vec<u8>, String> {
    let read_limit = max_input_bytes
        .checked_add(1)
        .and_then(|limit| u64::try_from(limit).ok())
        .ok_or_else(|| "RAW input limit cannot be represented safely".to_owned())?;
    let mut reader = reader.take(read_limit);
    let mut bytes = Vec::with_capacity(max_input_bytes.min(8192));
    let mut chunk = [0_u8; 8192];

    while bytes.len() < max_input_bytes {
        let remaining = max_input_bytes - bytes.len();
        let chunk_len = remaining.min(chunk.len());
        let read = reader
            .read(&mut chunk[..chunk_len])
            .map_err(|error| format!("could not read RAW '{label}': {error}"))?;
        if read == 0 {
            return Ok(bytes);
        }
        let required = bytes
            .len()
            .checked_add(read)
            .ok_or_else(|| "RAW input size overflowed while reading".to_owned())?;
        if required > bytes.capacity() {
            bytes.reserve_exact(required - bytes.capacity());
        }
        bytes.extend_from_slice(&chunk[..read]);
    }

    let mut extra = [0_u8; 1];
    if reader
        .read(&mut extra)
        .map_err(|error| format!("could not read RAW '{label}': {error}"))?
        != 0
    {
        return Err(format!(
            "RAW input is too large: exceeds limit {max_input_bytes}"
        ));
    }
    Ok(bytes)
}

pub(crate) fn read_raw_file(path: &Path, limits: RawDecodeLimits) -> Result<Vec<u8>, String> {
    let file = File::open(path)
        .map_err(|error| format!("could not open RAW '{}': {error}", path.display()))?;
    read_bounded_raw(file, &path.display().to_string(), limits.max_input_bytes)
}

pub(crate) fn raw_metadata(frame: &RawFrame) -> OpenMetadataSummary {
    let camera = [frame.camera().make.trim(), frame.camera().model.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    OpenMetadataSummary {
        camera: if camera.is_empty() {
            "Unavailable".to_owned()
        } else {
            camera
        },
        lens: frame.camera().lens.clone(),
        iso: frame.camera().iso,
        aperture: frame.camera().aperture,
        shutter: frame.camera().shutter_seconds,
        focal_length: frame.camera().focal_length_mm,
        capture_time: frame.camera().capture_time.clone(),
        orientation: format!("{:?}", frame.camera().orientation),
        dimensions: frame.sensor_dimensions(),
        exif: frame.exif().tags.clone(),
    }
}

#[derive(Clone, Debug)]
struct DecodedFrameDecoder {
    frame: RawFrame,
}

impl RawDecoder for DecodedFrameDecoder {
    fn decode(&self, _input: &[u8]) -> Result<RawFrame, rawweave_raw::RawError> {
        Ok(self.frame.clone())
    }
}

fn render_raw_frame_to_image(frame: RawFrame) -> Result<Image, String> {
    let mut editor = EditorCore::new_with_raw_decoder(DecodedFrameDecoder { frame });
    build_raw_workflow(&mut editor)?;
    let value = editor
        .evaluate(
            "display-transform",
            "display",
            EvaluationContext::default().with_source_bytes(Vec::new()),
        )
        .map_err(|error| format!("could not render RAW image set member: {error}"))?;
    let Value::DisplayRGB(display) = value else {
        return Err("RAW image set member did not produce display RGB output".to_owned());
    };
    let dimensions = display.dimensions();
    Image::from_pixels(
        dimensions.width,
        dimensions.height,
        display
            .pixels()
            .iter()
            .map(|[red, green, blue]| [*red, *green, *blue, 1.0])
            .collect(),
    )
    .map_err(|error| format!("could not create RAW image set member: {error}"))
}

fn open_raw_image_set_member_with_decoder(
    path: &Path,
    decoder: &dyn RawDecoder,
) -> Result<(Image, preview::OpenImageMetadata, rawweave_node_api::Metadata), String> {
    let bytes = read_raw_file(path, RawDecodeLimits::default())?;
    let frame = decoder
        .decode(&bytes)
        .map_err(|error| format!("could not decode RAW '{}': {error}", path.display()))?;
    let dimensions = frame.sensor_dimensions();
    let metadata = preview::OpenImageMetadata {
        kind: SourceKind::Raw,
        width: dimensions.width,
        height: dimensions.height,
        revision: 0,
        metadata: Some(raw_metadata(&frame)),
    };
    let member_metadata =
        rawweave_node_api::Metadata::from_sources(frame.camera(), frame.exif());
    let image = render_raw_frame_to_image(frame)?;
    Ok((image, metadata, member_metadata))
}

pub(crate) fn open_image_file_with_decoder(
    path: &Path,
    decoder: &dyn RawDecoder,
) -> Result<(SourceAsset, preview::OpenImageMetadata), String> {
    if is_raw_path(path) {
        let limits = RawDecodeLimits::default();
        let bytes = read_raw_file(path, limits)?;
        let frame = decoder
            .decode(&bytes)
            .map_err(|error| format!("could not decode RAW '{}': {error}", path.display()))?;
        let dimensions = frame.sensor_dimensions();
        return Ok((
            SourceAsset::Raw {
                bytes: Arc::new(bytes),
                path: path.to_owned(),
            },
            preview::OpenImageMetadata {
                kind: SourceKind::Raw,
                width: dimensions.width,
                height: dimensions.height,
                revision: 0,
                metadata: Some(raw_metadata(&frame)),
            },
        ));
    }

    let image = preview::decode_image_file(path)?;
    Ok((
        SourceAsset::Ordinary(image.clone()),
        preview::OpenImageMetadata {
            kind: SourceKind::Ordinary,
            width: image.width(),
            height: image.height(),
            revision: image.revision(),
            metadata: None,
        },
    ))
}

pub(crate) fn open_image_with_decoder(
    path: &Path,
    decoder: &dyn RawDecoder,
    editor: &mut EditorCore,
) -> Result<(SourceAsset, preview::OpenImageMetadata), String> {
    let (source, mut metadata) = open_image_file_with_decoder(path, decoder)?;
    rebuild_workflow_for_source(&source, editor)?;
    metadata.revision = editor.graph().revision();
    Ok((source, metadata))
}

fn rebuild_workflow_for_source(
    source: &SourceAsset,
    editor: &mut EditorCore,
) -> Result<(), String> {
    match source {
        SourceAsset::Raw { .. } => build_raw_workflow(editor),
        SourceAsset::Ordinary(_) => build_ordinary_workflow(editor),
        SourceAsset::ImageSet(_) => build_image_set_workflow(editor),
    }
}

fn infer_workflow_kind(editor: &EditorCore) -> Result<WorkflowKind, String> {
    let mut has_raw_requirement = false;
    let mut has_ordinary_requirement = false;
    for node in editor.graph().nodes().values() {
        if node.type_id.starts_with("raw.") {
            has_raw_requirement = true;
        }
        for port in node
            .descriptor
            .inputs
            .iter()
            .chain(node.descriptor.outputs.iter())
        {
            if port.data_type.starts_with("raw.") {
                has_raw_requirement = true;
            }
            if port.data_type == "core.Image" {
                has_ordinary_requirement = true;
            }
        }
    }

    match (has_raw_requirement, has_ordinary_requirement) {
        (true, false) => Ok(WorkflowKind::Raw),
        (false, true) => Ok(WorkflowKind::Ordinary),
        (true, true) => Err("loaded workflow mixes RAW and ordinary image requirements".to_owned()),
        (false, false) => Err(
            "cannot determine whether loaded workflow expects a RAW or ordinary image source"
                .to_owned(),
        ),
    }
}

fn ensure_source_compatible(editor: &EditorCore, source_kind: SourceKind) -> Result<(), String> {
    let workflow_kind = infer_workflow_kind(editor)?;
    let compatible = matches!(
        (workflow_kind, source_kind),
        (WorkflowKind::Raw, SourceKind::Raw) | (WorkflowKind::Ordinary, SourceKind::Ordinary)
    );
    if compatible {
        return Ok(());
    }

    match workflow_kind {
        WorkflowKind::Raw => Err("RAW workflow requires a RAW source".to_owned()),
        WorkflowKind::Ordinary => {
            Err("ordinary image workflow requires an ordinary image source".to_owned())
        }
    }
}

fn open_image_state(
    state: &AppState,
    path: &Path,
    decoder: &dyn RawDecoder,
) -> Result<(preview::OpenImageMetadata, SourceAsset), String> {
    let intent = *state
        .source_selection
        .lock()
        .map_err(|_| "source selection state is unavailable".to_owned())?;
    if intent == SourceSelectionIntent::ReplaceWorkflow {
        let (source, metadata) = {
            let mut editor = lock_editor(&state.editor)?;
            open_image_with_decoder(path, decoder, &mut editor)?
        };
        clear_blueprint(state)?;
        *state
            .source_image
            .lock()
            .map_err(|_| "source image state is unavailable".to_owned())? = Some(source.clone());
        return Ok((metadata, source));
    }

    let (source, mut metadata) = open_image_file_with_decoder(path, decoder)?;
    {
        let editor = lock_editor(&state.editor)?;
        ensure_source_compatible(&editor, metadata.kind)?;
        metadata.revision = editor.graph().revision();
    }
    *state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())? = Some(source.clone());
    *state
        .source_selection
        .lock()
        .map_err(|_| "source selection state is unavailable".to_owned())? =
        SourceSelectionIntent::ReplaceWorkflow;
    Ok((metadata, source))
}

fn open_image_set_files_with_decoder(
    paths: &[String],
    order: ImageSetOrder,
    decoder: &dyn RawDecoder,
) -> Result<(ImageSet, Vec<OpenImageSetMemberDto>), String> {
    if paths.is_empty() {
        return Err("image set must contain at least one file".to_owned());
    }
    if paths.len() > MAX_IMAGE_SET_MEMBERS {
        return Err(format!(
            "image set cannot contain more than {MAX_IMAGE_SET_MEMBERS} files"
        ));
    }

    let mut ids = std::collections::BTreeSet::new();
    let mut members = Vec::with_capacity(paths.len());
    let mut metadata = BTreeMap::new();
    for path in paths {
        if path.trim().is_empty() {
            return Err("image set member path cannot be empty".to_owned());
        }
        if !ids.insert(path.as_str()) {
            return Err(format!("image set member path '{path}' is duplicated"));
        }
        let path_ref = Path::new(path);
        let (image, summary, member_metadata) = if is_raw_path(path_ref) {
            open_raw_image_set_member_with_decoder(path_ref, decoder)?
        } else {
            let (source, summary) = open_image_file_with_decoder(path_ref, decoder)?;
            let image = match source {
                SourceAsset::Ordinary(image) => image,
                SourceAsset::ImageSet(_) => {
                    return Err(format!(
                        "image set member '{path}' decoded as another image set"
                    ));
                }
                SourceAsset::Raw { .. } => {
                    return Err(format!("RAW image set member '{path}' is not supported"));
                }
            };
            (image, summary, rawweave_node_api::Metadata::default())
        };
        metadata.insert(path.clone(), summary.metadata);
        members.push(
            ImageSetMember::new(path.clone(), image, member_metadata)
                .with_source(ImageSetSourceDescriptor::new(path.clone())),
        );
    }

    let set = ImageSet::new(order, members).map_err(|error| error.to_string())?;
    let result_members = set
        .members()
        .iter()
        .map(|member| {
            let path = member
                .source()
                .map(|source| source.path().to_owned())
                .unwrap_or_else(|| member.id.clone());
            let name = Path::new(&path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&path)
                .to_owned();
            OpenImageSetMemberDto {
                id: member.id.clone(),
                path,
                name,
                width: member.image.width(),
                height: member.image.height(),
                metadata: metadata.get(&member.id).cloned().flatten(),
            }
        })
        .collect();
    Ok((set, result_members))
}

fn open_image_set_alignment(alignment: &AlignmentState) -> OpenImageSetAlignmentDto {
    match alignment {
        AlignmentState::Unaligned => OpenImageSetAlignmentDto::Unaligned,
        AlignmentState::Aligned {
            reference_member, ..
        } => OpenImageSetAlignmentDto::Aligned {
            reference_member: reference_member.clone(),
        },
    }
}

fn open_image_set_state(
    state: &AppState,
    paths: &[String],
    order: ImageSetOrder,
    decoder: &dyn RawDecoder,
) -> Result<OpenImageSetDto, String> {
    let (set, members) = open_image_set_files_with_decoder(paths, order, decoder)?;
    let source = SourceAsset::ImageSet(Box::new(set.clone()));
    let revision = {
        let mut editor = lock_editor(&state.editor)?;
        rebuild_workflow_for_source(&source, &mut editor)?;
        editor.graph().revision()
    };
    clear_blueprint(state)?;
    *state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())? = Some(source);
    *state
        .source_selection
        .lock()
        .map_err(|_| "source selection state is unavailable".to_owned())? =
        SourceSelectionIntent::ReplaceWorkflow;
    Ok(OpenImageSetDto {
        kind: "imageset",
        order: set.order(),
        revision,
        members,
        shared_metadata: None,
        alignment: open_image_set_alignment(&set.alignment()),
    })
}

pub(crate) fn build_ordinary_workflow(editor: &mut EditorCore) -> Result<(), String> {
    editor.reset_ordinary_image_graph().map_err(|error| error.to_string())
}

pub(crate) fn build_image_set_workflow(editor: &mut EditorCore) -> Result<(), String> {
    let existing = editor
        .graph()
        .nodes()
        .keys()
        .map(|node_id| node_id.as_str().to_owned())
        .collect::<Vec<_>>();
    for node_id in existing {
        editor
            .remove_node(&node_id)
            .map_err(|error| error.to_string())?;
    }
    editor
        .add_node("imageset-input", "core.imageset-input")
        .map_err(|error| error.to_string())?;
    editor
        .add_node("imageset-select", "core.imageset-select")
        .map_err(|error| error.to_string())?;
    editor
        .add_node("output", "core.output")
        .map_err(|error| error.to_string())?;
    editor
        .connect("imageset-input", "images", "imageset-select", "images")
        .map_err(|error| error.to_string())?;
    editor
        .connect("imageset-select", "image", "output", "image")
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn build_raw_workflow(editor: &mut EditorCore) -> Result<(), String> {
    editor.reset_raw_image_graph().map_err(|error| error.to_string())
}

#[tauri::command]
fn node_descriptors(state: State<'_, AppState>) -> Result<Vec<NodeDescriptor>, String> {
    Ok(lock_editor(&state.editor)?.node_descriptors())
}

#[tauri::command]
fn add_external_host(
    state: State<'_, AppState>,
    config: hosts::ExternalHostConfigDto,
) -> Result<hosts::ExternalHostDto, String> {
    lock_hosts(&state.hosts)?
        .add(config)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_external_host(state: State<'_, AppState>, host_id: String) -> Result<(), String> {
    lock_hosts(&state.hosts)?
        .remove(&host_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_external_hosts(state: State<'_, AppState>) -> Result<Vec<hosts::ExternalHostDto>, String> {
    Ok(lock_hosts(&state.hosts)?.list())
}

#[tauri::command]
fn test_external_host(
    state: State<'_, AppState>,
    host_id: String,
) -> Result<hosts::ExternalHostDto, String> {
    lock_hosts(&state.hosts)?
        .test(&host_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn discover_external_host(
    state: State<'_, AppState>,
    host_id: String,
) -> Result<hosts::ExternalHostDto, String> {
    let mut hosts = lock_hosts(&state.hosts)?;
    let mut editor = lock_editor(&state.editor)?;
    hosts
        .discover(&host_id, &mut editor)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn discover_external_hosts(
    state: State<'_, AppState>,
) -> Result<Vec<hosts::ExternalHostDto>, String> {
    let mut hosts = lock_hosts(&state.hosts)?;
    let mut editor = lock_editor(&state.editor)?;
    hosts
        .discover_all(&mut editor)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn external_host_diagnostics(
    state: State<'_, AppState>,
    host_id: String,
) -> Result<hosts::ExternalHostDiagnosticsDto, String> {
    lock_hosts(&state.hosts)?
        .diagnostics(&host_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn add_node(state: State<'_, AppState>, node_id: String, type_id: String) -> Result<(), String> {
    if current_blueprint(&state).is_ok() {
        mutate_current_blueprint(&state, |_editor, blueprint| {
            blueprint
                .graph_mut()
                .add_node(NodeId::from(node_id), &type_id)
                .map_err(|error| error.to_string())
        })?;
        return Ok(());
    }
    lock_editor(&state.editor)?
        .add_node(&node_id, &type_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_node(state: State<'_, AppState>, node_id: String) -> Result<(), String> {
    if current_blueprint(&state).is_ok() {
        mutate_current_blueprint(&state, |_editor, blueprint| {
            blueprint
                .graph_mut()
                .remove_node(&NodeId::from(node_id))
                .map_err(|error| error.to_string())
        })?;
        return Ok(());
    }
    lock_editor(&state.editor)?
        .remove_node(&node_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn connect_nodes(
    state: State<'_, AppState>,
    from_node: String,
    from_port: String,
    to_node: String,
    to_port: String,
) -> Result<(), String> {
    if current_blueprint(&state).is_ok() {
        mutate_current_blueprint(&state, |_editor, blueprint| {
            blueprint
                .graph_mut()
                .connect(
                    NodeId::from(from_node),
                    &from_port,
                    NodeId::from(to_node),
                    &to_port,
                )
                .map_err(|error| error.to_string())
        })?;
        return Ok(());
    }
    lock_editor(&state.editor)?
        .connect(&from_node, &from_port, &to_node, &to_port)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn disconnect_nodes(
    state: State<'_, AppState>,
    from_node: String,
    from_port: String,
    to_node: String,
    to_port: String,
) -> Result<(), String> {
    if current_blueprint(&state).is_ok() {
        mutate_current_blueprint(&state, |_editor, blueprint| {
            blueprint
                .graph_mut()
                .disconnect(
                    NodeId::from(from_node),
                    &from_port,
                    NodeId::from(to_node),
                    &to_port,
                )
                .map_err(|error| error.to_string())
        })?;
        return Ok(());
    }
    lock_editor(&state.editor)?
        .disconnect(&from_node, &from_port, &to_node, &to_port)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_node_parameter(
    state: State<'_, AppState>,
    node_id: String,
    parameter_id: String,
    value: ParameterValue,
) -> Result<(), String> {
    if current_blueprint(&state).is_ok() {
        let workflow_parameter_id = format!("{node_id}:{parameter_id}");
        mutate_current_blueprint(&state, |_editor, blueprint| {
            if blueprint.parameters.contains_key(&workflow_parameter_id) {
                blueprint
                    .set_parameter(&workflow_parameter_id, value)
                    .map_err(|error| error.to_string())
            } else {
                blueprint
                    .graph_mut()
                    .set_parameter(&NodeId::from(node_id), &parameter_id, value)
                    .map_err(|error| error.to_string())
            }
        })?;
        return Ok(());
    }
    lock_editor(&state.editor)?
        .set_node_parameter(&node_id, &parameter_id, value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn expose_parameter(
    state: State<'_, AppState>,
    node_id: String,
    parameter_id: String,
) -> Result<(), String> {
    if current_blueprint(&state).is_ok() {
        let workflow_parameter_id = format!("{node_id}:{parameter_id}");
        mutate_current_blueprint(&state, |editor, blueprint| {
            editor
                .expose_blueprint_parameter(blueprint, &workflow_parameter_id)
                .map_err(|error| error.to_string())
        })?;
        return Ok(());
    }
    lock_editor(&state.editor)?
        .expose_parameter(&node_id, &parameter_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn unexpose_parameter(
    state: State<'_, AppState>,
    node_id: String,
    parameter_id: String,
) -> Result<(), String> {
    if current_blueprint(&state).is_ok() {
        let workflow_parameter_id = format!("{node_id}:{parameter_id}");
        mutate_current_blueprint(&state, |_editor, blueprint| {
            blueprint
                .hide_parameter(&workflow_parameter_id)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })?;
        return Ok(());
    }
    lock_editor(&state.editor)?
        .unexpose_parameter(&node_id, &parameter_id)
        .map_err(|error| error.to_string())
}

fn create_subgraph_command(
    state: &AppState,
    request: CreateSubgraphRequest,
) -> Result<WorkflowDefinitionDto, String> {
    let editor = lock_editor(&state.editor)?.clone();
    let child = build_subgraph_definition(&editor, request)?;
    let mut parent = current_or_flat_blueprint(state)?;
    parent
        .add_nested_subgraph(child.clone())
        .map_err(|error| error.to_string())?;
    let dto = workflow_definition_dto(&child);
    store_blueprint(state, parent)?;
    Ok(dto)
}

#[tauri::command]
fn create_subgraph(
    state: State<'_, AppState>,
    selection: Vec<String>,
    id: String,
    version: String,
    metadata: WorkflowMetadataDto,
    node_pack_dependencies: Option<Vec<WorkflowDependencyDto>>,
    subgraph_dependencies: Option<Vec<WorkflowDependencyDto>>,
) -> Result<WorkflowDefinitionDto, String> {
    create_subgraph_command(
        &state,
        CreateSubgraphRequest {
            node_ids: selection,
            id,
            version,
            metadata,
            node_pack_dependencies: node_pack_dependencies.unwrap_or_default(),
            subgraph_dependencies: subgraph_dependencies.unwrap_or_default(),
        },
    )
}

#[tauri::command]
fn create_subgraph_from_selection(
    state: State<'_, AppState>,
    node_ids: Vec<String>,
    id: String,
    version: String,
    metadata: WorkflowMetadataDto,
    node_pack_dependencies: Option<Vec<WorkflowDependencyDto>>,
    subgraph_dependencies: Option<Vec<WorkflowDependencyDto>>,
) -> Result<WorkflowDefinitionDto, String> {
    create_subgraph_command(
        &state,
        CreateSubgraphRequest {
            node_ids,
            id,
            version,
            metadata,
            node_pack_dependencies: node_pack_dependencies.unwrap_or_default(),
            subgraph_dependencies: subgraph_dependencies.unwrap_or_default(),
        },
    )
}

#[tauri::command]
fn expose_workflow_parameter(
    state: State<'_, AppState>,
    node_id: String,
    parameter_id: String,
) -> Result<WorkflowDefinitionDto, String> {
    let id = format!("{node_id}:{parameter_id}");
    mutate_current_blueprint(&state, |editor, blueprint| {
        editor
            .expose_blueprint_parameter(blueprint, &id)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
fn hide_workflow_parameter(
    state: State<'_, AppState>,
    node_id: String,
    parameter_id: String,
) -> Result<WorkflowDefinitionDto, String> {
    let id = format!("{node_id}:{parameter_id}");
    mutate_current_blueprint(&state, |_editor, blueprint| {
        blueprint
            .hide_parameter(&id)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
fn set_workflow_parameter(
    state: State<'_, AppState>,
    parameter_id: String,
    value: ParameterValue,
) -> Result<WorkflowDefinitionDto, String> {
    mutate_current_blueprint(&state, |_editor, blueprint| {
        blueprint
            .set_parameter(&parameter_id, value)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
fn expose_workflow_port(
    state: State<'_, AppState>,
    direction: WorkflowPortDirection,
    node_id: String,
    port_id: String,
) -> Result<WorkflowDefinitionDto, String> {
    mutate_current_blueprint(&state, |_editor, blueprint| match direction {
        WorkflowPortDirection::Input => blueprint
            .expose_input(&NodeId::from(node_id), &port_id)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        WorkflowPortDirection::Output => blueprint
            .expose_output(&NodeId::from(node_id), &port_id)
            .map(|_| ())
            .map_err(|error| error.to_string()),
    })
}

#[tauri::command]
fn expose_workflow_input(
    state: State<'_, AppState>,
    node_id: String,
    port_id: String,
) -> Result<WorkflowDefinitionDto, String> {
    expose_workflow_port(state, WorkflowPortDirection::Input, node_id, port_id)
}

#[tauri::command]
fn expose_workflow_output(
    state: State<'_, AppState>,
    node_id: String,
    port_id: String,
) -> Result<WorkflowDefinitionDto, String> {
    expose_workflow_port(state, WorkflowPortDirection::Output, node_id, port_id)
}

#[tauri::command]
fn hide_workflow_port(
    state: State<'_, AppState>,
    port_id: String,
) -> Result<WorkflowDefinitionDto, String> {
    mutate_current_blueprint(&state, |_editor, blueprint| {
        if blueprint.hide_port(&port_id) {
            Ok(())
        } else {
            Err(format!("workflow port '{port_id}' does not exist"))
        }
    })
}

fn load_blueprint_command(
    state: &AppState,
    serialized: &str,
) -> Result<WorkflowDefinitionDto, String> {
    let editor = lock_editor(&state.editor)?.clone();
    let blueprint = import_blueprint_definition(&editor, serialized, &state.checkpoint.store)?;
    let dto = workflow_definition_dto(&blueprint);
    store_blueprint(state, blueprint.clone())?;
    state.checkpoint.replace_from_graph(&editor)?;
    state
        .blueprint_stack
        .lock()
        .map_err(|_| "blueprint navigation state is unavailable".to_owned())?
        .clear();
    Ok(dto)
}

fn save_blueprint_state(state: &AppState) -> Result<String, String> {
    let editor = lock_editor(&state.editor)?.clone();
    let blueprint = current_or_flat_blueprint(state)?;
    let serialized = export_blueprint_text(&editor, &blueprint)?;
    store_blueprint(state, blueprint)?;
    Ok(serialized)
}

#[tauri::command]
fn save_blueprint(state: State<'_, AppState>) -> Result<String, String> {
    save_blueprint_state(&state)
}

#[tauri::command]
fn export_blueprint(state: State<'_, AppState>) -> Result<String, String> {
    save_blueprint_state(&state)
}

#[tauri::command]
fn load_blueprint(
    state: State<'_, AppState>,
    serialized: String,
) -> Result<WorkflowDefinitionDto, String> {
    load_blueprint_command(&state, &serialized)
}

#[tauri::command]
fn import_blueprint(
    state: State<'_, AppState>,
    serialized: String,
) -> Result<WorkflowDefinitionDto, String> {
    load_blueprint_command(&state, &serialized)
}

fn instantiate_loaded_blueprint(
    state: &AppState,
    serialized: Option<String>,
) -> Result<(), String> {
    if preview_diagnostics_enabled() {
        eprintln!("[preview] instantiate blueprint clears source");
    }
    if let Some(serialized) = serialized {
        load_blueprint_command(state, &serialized)?;
    } else if current_blueprint(state).is_err() {
        let blueprint = current_or_flat_blueprint(state)?;
        store_blueprint(state, blueprint)?;
    }
    let blueprint = current_blueprint(state)?;
    lock_editor(&state.editor)?
        .instantiate_blueprint(&blueprint)
        .map_err(|error| error.to_string())?;
    *state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())? = None;
    *state
        .source_selection
        .lock()
        .map_err(|_| "source selection state is unavailable".to_owned())? =
        SourceSelectionIntent::AttachToLoadedWorkflow;
    state.preview.cancel_all();
    Ok(())
}

#[tauri::command]
fn instantiate_blueprint(
    state: State<'_, AppState>,
    serialized: Option<String>,
) -> Result<(), String> {
    instantiate_loaded_blueprint(&state, serialized)
}

fn open_subgraph_state(state: &AppState, id: String) -> Result<WorkflowDefinitionDto, String> {
    let parent = current_blueprint(state)?;
    let nested = parent
        .nested_subgraph(&id)
        .cloned()
        .ok_or_else(|| format!("nested workflow '{id}' does not exist"))?;
    state
        .blueprint_stack
        .lock()
        .map_err(|_| "blueprint navigation state is unavailable".to_owned())?
        .push(parent);
    lock_editor(&state.editor)?
        .instantiate_blueprint(&nested)
        .map_err(|error| error.to_string())?;
    store_blueprint(state, nested.clone())?;
    Ok(workflow_definition_dto(&nested))
}

#[tauri::command]
fn open_subgraph(state: State<'_, AppState>, id: String) -> Result<WorkflowDefinitionDto, String> {
    open_subgraph_state(&state, id)
}

fn return_to_parent_state(state: &AppState) -> Result<WorkflowDefinitionDto, String> {
    let child = current_blueprint(state)?;
    let child_id = child.identity().id.clone();
    let mut parent = state
        .blueprint_stack
        .lock()
        .map_err(|_| "blueprint navigation state is unavailable".to_owned())?
        .pop()
        .ok_or_else(|| "already at the root workflow".to_owned())?;
    let nested = parent
        .open_subgraph_mut(&child_id)
        .ok_or_else(|| format!("nested workflow '{child_id}' does not exist"))?;
    *nested = child.clone();
    if let Some(dependency) = parent
        .subgraph_dependencies
        .iter_mut()
        .find(|dependency| dependency.id == child_id)
    {
        dependency.hash = child.hash();
    }
    let dto = workflow_definition_dto(&parent);
    lock_editor(&state.editor)?
        .instantiate_blueprint(&parent)
        .map_err(|error| error.to_string())?;
    store_blueprint(state, parent)?;
    Ok(dto)
}

#[tauri::command]
fn return_to_parent(state: State<'_, AppState>) -> Result<WorkflowDefinitionDto, String> {
    return_to_parent_state(&state)
}

fn workflow_hash_state(state: &AppState) -> Result<String, String> {
    let blueprint = current_or_flat_blueprint(state)?;
    Ok(blueprint.hash())
}

#[tauri::command]
fn workflow_hash(state: State<'_, AppState>) -> Result<String, String> {
    workflow_hash_state(&state)
}

fn dependency_report_for_state(
    state: &AppState,
    available_packs: Option<Vec<AvailableNodePackDto>>,
    available_subgraphs: Option<Vec<WorkflowDependencyDto>>,
) -> Result<DependencyReportDto, String> {
    let blueprint = current_or_flat_blueprint(state)?;
    let packs = available_packs
        .map(available_manifests)
        .unwrap_or_else(built_in_node_pack_manifests);
    let subgraphs = available_subgraphs
        .unwrap_or_default()
        .into_iter()
        .map(|dependency| {
            SubgraphDependency::new(
                dependency.id,
                dependency.version,
                dependency.hash.unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    Ok(dependency_report_dto(
        blueprint.diagnose_dependencies(&packs, &subgraphs),
    ))
}

#[tauri::command]
fn dependency_report(
    state: State<'_, AppState>,
    available_packs: Option<Vec<AvailableNodePackDto>>,
    available_subgraphs: Option<Vec<WorkflowDependencyDto>>,
) -> Result<DependencyReportDto, String> {
    dependency_report_for_state(&state, available_packs, available_subgraphs)
}

#[tauri::command]
fn dependency_status(state: State<'_, AppState>) -> Result<DependencyReportDto, String> {
    dependency_report_for_state(&state, None, None)
}

#[tauri::command]
fn workflow_dependency_report(
    state: State<'_, AppState>,
    available_packs: Option<Vec<AvailableNodePackDto>>,
    available_subgraphs: Option<Vec<WorkflowDependencyDto>>,
) -> Result<DependencyReportDto, String> {
    dependency_report_for_state(&state, available_packs, available_subgraphs)
}

#[tauri::command]
fn save_workflow(state: State<'_, AppState>) -> Result<String, String> {
    lock_editor(&state.editor)?
        .save_workflow()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn load_workflow(state: State<'_, AppState>, workflow: String) -> Result<(), String> {
    load_workflow_state(&state, &workflow)
}

#[tauri::command]
fn restore_workflow_history(state: State<'_, AppState>, workflow: String) -> Result<(), String> {
    restore_workflow_history_state(&state, &workflow)
}

fn load_workflow_state(state: &AppState, workflow: &str) -> Result<(), String> {
    load_workflow_state_with_source_policy(state, workflow, true)
}

fn restore_workflow_history_state(state: &AppState, workflow: &str) -> Result<(), String> {
    load_workflow_state_with_source_policy(state, workflow, false)
}

fn load_workflow_state_with_source_policy(
    state: &AppState,
    workflow: &str,
    clear_source: bool,
) -> Result<(), String> {
    if preview_diagnostics_enabled() {
        eprintln!("[preview] load workflow clear_source={clear_source}");
    }
    let artifact_store = state.checkpoint.store.clone();
    lock_editor(&state.editor)?
        .load_workflow_with_artifact_store(workflow, artifact_store)
        .map_err(|error| error.to_string())?;
    clear_blueprint(state)?;
    let editor = lock_editor(&state.editor)?.clone();
    state.checkpoint.replace_from_graph(&editor)?;
    if clear_source {
        *state
            .source_image
            .lock()
            .map_err(|_| "source image state is unavailable".to_owned())? = None;
        *state
            .source_selection
            .lock()
            .map_err(|_| "source selection state is unavailable".to_owned())? =
            SourceSelectionIntent::AttachToLoadedWorkflow;
    }
    state.preview.cancel_all();
    Ok(())
}

#[tauri::command]
async fn request_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    request: preview::PreviewRequest,
) -> Result<preview::PreviewMetadata, String> {
    if preview_diagnostics_enabled() {
        eprintln!(
            "[preview] request id={} revision={} node={} output={} region={:?} mip={}",
            request.request_id,
            request.revision,
            request.node_id,
            request.output_port,
            request.region,
            request.mip
        );
    }
    let editor = lock_editor(&state.editor)?.clone();
    let current_editor = Arc::clone(&state.editor);
    let source_image = state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())?
        .clone();
    if preview_diagnostics_enabled() {
        eprintln!("[preview] request source present={}", source_image.is_some());
    }
    let manager = Arc::clone(&state.preview);
    manager.begin(&request.request_id);
    let progress_app = app.clone();
    let progress_request = request.clone();
    let task = tauri::async_runtime::spawn_blocking(move || {
        let _ = progress_app.emit(
            "preview-progress",
            preview::PreviewProgressEvent {
                request_id: progress_request.request_id.clone(),
                revision: progress_request.revision,
                progress: 0.05,
            },
        );
        let result = preview::render_preview(
            &manager,
            &editor,
            &current_editor,
            source_image,
            progress_request.clone(),
        );
        if result.is_ok() {
            let _ = progress_app.emit(
                "preview-progress",
                preview::PreviewProgressEvent {
                    request_id: progress_request.request_id,
                    revision: progress_request.revision,
                    progress: 1.0,
                },
            );
        }
        result
    });
    let result = task
        .await
        .map_err(|error| format!("preview worker failed: {error}"))?;
    if preview_diagnostics_enabled() {
        match &result {
            Ok(metadata) => eprintln!("[preview] rendered id={} metadata={metadata:?}", request.request_id),
            Err(error) => eprintln!("[preview] render failed id={} error={error}", request.request_id),
        }
    }
    match &result {
        Ok(metadata) => {
            let _ = app.emit("preview-ready", metadata);
        }
        Err(message) if message == "preview cancelled" => {
            let _ = app.emit(
                "preview-cancelled",
                preview::PreviewCancelledEvent {
                    request_id: request.request_id.clone(),
                    revision: request.revision,
                },
            );
        }
        Err(message) => {
            let _ = app.emit(
                "preview-error",
                preview::PreviewErrorEvent {
                    request_id: request.request_id.clone(),
                    revision: request.revision,
                    message: message.clone(),
                },
            );
        }
    }
    result
}

#[cfg(test)]
fn open_image_file(path: &str) -> Result<(Image, preview::OpenImageMetadata), String> {
    let (source, metadata) =
        open_image_file_with_decoder(Path::new(path), &RawloaderDecoder::default())?;
    match source {
        SourceAsset::Ordinary(image) => Ok((image, metadata)),
        SourceAsset::ImageSet(_) => {
            Err("image-set input must be opened through the image-set source path".to_owned())
        }
        SourceAsset::Raw { .. } => {
            Err("RAW input must be opened through the RAW source path".to_owned())
        }
    }
}

#[tauri::command]
fn create_batch_job(
    state: State<'_, AppState>,
    request: CreateBatchJobRequest,
) -> Result<BatchJob, String> {
    validate_batch_workers(request.max_workers)?;
    let job_id = request.job.id.clone();
    let store = request
        .state_path
        .map(JobStore::new)
        .unwrap_or_else(JobStore::memory);
    let engine = BatchEngine::new(
        request.job,
        store,
        Arc::new(ImageFileProcessor),
        request.max_workers,
    )
    .map_err(|error| error.to_string())?;
    let engine = Arc::new(engine);
    let snapshot = engine.snapshot().map_err(|error| error.to_string())?;
    state.batch.insert(job_id, engine)?;
    Ok(snapshot)
}

#[tauri::command]
fn load_batch_job(
    state: State<'_, AppState>,
    request: LoadBatchJobRequest,
) -> Result<BatchJob, String> {
    validate_batch_workers(request.max_workers)?;
    let store = JobStore::new(request.state_path);
    let snapshot = store.load().map_err(|error| error.to_string())?;
    let job_id = snapshot.id.clone();
    let engine = BatchEngine::resume(store, Arc::new(ImageFileProcessor), request.max_workers)
        .map_err(|error| error.to_string())?;
    let engine = Arc::new(engine);
    let snapshot = engine.snapshot().map_err(|error| error.to_string())?;
    state.batch.insert(job_id, engine)?;
    Ok(snapshot)
}

#[tauri::command]
fn batch_preflight(
    state: State<'_, AppState>,
    job_id: String,
    options: Option<PreflightOptions>,
) -> Result<rawweave_batch::PreflightReport, String> {
    state
        .batch
        .get(&job_id)?
        .preflight(&options.unwrap_or_default())
        .map_err(|error| error.to_string())
}

fn batch_start_error(report: &rawweave_batch::PreflightReport) -> String {
    report
        .errors()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("; ")
}

#[tauri::command]
fn start_batch(state: State<'_, AppState>, job_id: String) -> Result<BatchJob, String> {
    let engine = state.batch.get(&job_id)?;
    let report = engine
        .preflight(&PreflightOptions::default())
        .map_err(|error| error.to_string())?;
    if report.has_errors() {
        return Err(batch_start_error(&report));
    }
    engine.start().map_err(|error| error.to_string())?;
    engine.snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn pause_batch(state: State<'_, AppState>, job_id: String) -> Result<BatchJob, String> {
    let engine = state.batch.get(&job_id)?;
    engine.pause().map_err(|error| error.to_string())?;
    engine.snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn resume_batch(state: State<'_, AppState>, job_id: String) -> Result<BatchJob, String> {
    let engine = state.batch.get(&job_id)?;
    engine.resume_run().map_err(|error| error.to_string())?;
    engine.snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_batch(state: State<'_, AppState>, job_id: String) -> Result<BatchJob, String> {
    let engine = state.batch.get(&job_id)?;
    engine.cancel().map_err(|error| error.to_string())?;
    engine.wait().map_err(|error| error.to_string())?;
    engine.snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
fn retry_failed_batch(state: State<'_, AppState>, job_id: String) -> Result<usize, String> {
    state
        .batch
        .get(&job_id)?
        .retry_failed()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn retry_selected_batch(
    state: State<'_, AppState>,
    job_id: String,
    item_ids: Vec<String>,
) -> Result<usize, String> {
    state
        .batch
        .get(&job_id)?
        .retry_selected(&item_ids)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn skip_batch_items(
    state: State<'_, AppState>,
    job_id: String,
    item_ids: Vec<String>,
) -> Result<usize, String> {
    state
        .batch
        .get(&job_id)?
        .skip(&item_ids)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_snapshot(state: State<'_, AppState>, job_id: String) -> Result<BatchJob, String> {
    state
        .batch
        .get(&job_id)?
        .snapshot()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_dry_run(
    state: State<'_, AppState>,
    job_id: String,
    subset: DryRunSubset,
) -> Result<BatchDryRunResponse, String> {
    let job = state
        .batch
        .get(&job_id)?
        .snapshot()
        .map_err(|error| error.to_string())?;
    let result = dry_run(&job, subset).map_err(|error| error.to_string())?;
    Ok(BatchDryRunResponse {
        workflow_revision: result.workflow_revision,
        workflow_hash: result.workflow_hash,
        item_ids: result.item_ids,
        recipes: result.recipes,
    })
}

#[tauri::command]
fn open_failed_batch_item(
    state: State<'_, AppState>,
    job_id: String,
    item_id: String,
) -> Result<rawweave_batch::BatchItem, String> {
    state
        .batch
        .get(&job_id)?
        .failed_item(&item_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_image(
    state: State<'_, AppState>,
    path: String,
) -> Result<preview::OpenImageMetadata, String> {
    let result = open_image_state(&state, Path::new(&path), &RawloaderDecoder::default())
        .map(|(metadata, _)| metadata);
    if preview_diagnostics_enabled() {
        match &result {
            Ok(metadata) => eprintln!("[preview] opened path={path:?} metadata={metadata:?}"),
            Err(error) => eprintln!("[preview] open failed path={path:?} error={error}"),
        }
    }
    result
}

#[tauri::command]
fn open_image_set(
    state: State<'_, AppState>,
    paths: Vec<String>,
    order: ImageSetOrder,
) -> Result<OpenImageSetDto, String> {
    open_image_set_state(&state, &paths, order, &RawloaderDecoder::default())
}

#[tauri::command]
fn cancel_preview(state: State<'_, AppState>, request_id: String) -> Result<(), String> {
    state.preview.cancel(&request_id);
    Ok(())
}

#[tauri::command]
fn release_preview(state: State<'_, AppState>, url: String) -> Result<(), String> {
    state.preview.release(&url)
}

fn checkpoint_status_for_node(
    state: &AppState,
    node_id: &str,
    output_port: Option<&str>,
) -> Result<CheckpointStatusDto, String> {
    let mut editor = lock_editor(&state.editor)?;
    let source = state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())?
        .clone();
    let manager = Arc::clone(&state.checkpoint);
    let (checkpoint, selected_port, progress) = {
        let mut records = manager
            .checkpoints
            .lock()
            .map_err(|_| "checkpoint state is unavailable".to_owned())?;
        let record = ensure_checkpoint_record(&mut records, &editor, node_id, output_port)?;
        let (dependency_hash, _) = checkpoint_dependencies(
            &editor,
            node_id,
            &record.output_port,
            record.checkpoint.node_version,
            source.as_ref(),
        )?;
        record.checkpoint.set_dependency_hash(dependency_hash);
        (
            record.checkpoint.clone(),
            record.output_port.clone(),
            record.progress,
        )
    };
    editor
        .register_checkpoint(checkpoint.clone())
        .map_err(|error| error.to_string())?;
    manager.persist()?;
    let mut status = checkpoint_status_dto(&checkpoint, &manager.store)?;
    status.output_port = selected_port;
    status.progress = progress;
    Ok(status)
}

#[tauri::command]
fn checkpoint_list(state: State<'_, AppState>) -> Result<Vec<CheckpointStatusDto>, String> {
    let editor = lock_editor(&state.editor)?.clone();
    let nodes = editor
        .graph()
        .nodes()
        .values()
        .filter(|node| node.descriptor.evaluation_policy == EvaluationPolicy::ManualCheckpoint)
        .map(|node| {
            (
                node.id.as_str().to_owned(),
                node.descriptor.outputs.first().map(|port| port.id.clone()),
            )
        })
        .collect::<Vec<_>>();
    nodes
        .into_iter()
        .map(|(node_id, output_port)| {
            checkpoint_status_for_node(state.inner(), &node_id, output_port.as_deref())
        })
        .collect()
}

#[tauri::command]
fn checkpoint_status(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<CheckpointStatusDto, String> {
    checkpoint_status_for_node(state.inner(), &node_id, None)
}

#[tauri::command]
async fn generate_checkpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    output_port: String,
) -> Result<CheckpointStatusDto, String> {
    let manager = Arc::clone(&state.checkpoint);
    let source = state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())?
        .clone();
    let (evaluation_editor, token, node_version, dependency_hash, upstream_hashes) = {
        let mut editor = lock_editor(&state.editor)?;
        let mut records = manager
            .checkpoints
            .lock()
            .map_err(|_| "checkpoint state is unavailable".to_owned())?;
        let (token, node_version, dependency_hash, upstream_hashes, checkpoint) = {
            let record =
                ensure_checkpoint_record(&mut records, &editor, &node_id, Some(&output_port))?;
            let (dependency_hash, upstream_hashes) = checkpoint_dependencies(
                &editor,
                &node_id,
                &record.output_port,
                record.checkpoint.node_version,
                source.as_ref(),
            )?;
            record
                .checkpoint
                .set_dependency_hash(dependency_hash.clone());
            let token = record
                .checkpoint
                .begin_generation_token()
                .map_err(|error| error.to_string())?;
            record.progress = Some(0.0);
            (
                token,
                record.checkpoint.node_version,
                dependency_hash,
                upstream_hashes,
                record.checkpoint.clone(),
            )
        };
        manager
            .cancellation_requests
            .lock()
            .map_err(|_| "checkpoint cancellation state is unavailable".to_owned())?
            .remove(&node_id);
        editor
            .register_checkpoint(checkpoint)
            .map_err(|error| error.to_string())?;
        drop(records);
        manager.persist()?;
        (
            editor.clone(),
            token,
            node_version,
            dependency_hash,
            upstream_hashes,
        )
    };
    emit_checkpoint_progress(&app, &node_id, &output_port, 0.05, "generating", None);

    let evaluation_node_id = NodeId::from(node_id.as_str());
    let is_ai_node = evaluation_editor
        .graph()
        .node(&evaluation_node_id)
        .is_some_and(|node| ai_checkpoint_spec(&node.type_id, &output_port).is_ok());
    let ai_input_port = evaluation_editor
        .graph()
        .node(&evaluation_node_id)
        .and_then(|node| {
            node.descriptor
                .inputs
                .iter()
                .find(|port| port.data_type == "core.Image")
                .map(|port| port.id.clone())
        })
        .unwrap_or_else(|| "image".to_owned());
    let evaluation_context = checkpoint_context(source.as_ref());
    let ai_request = is_ai_node.then(|| {
        build_ai_submit_request_for_output(
            &evaluation_editor,
            &node_id,
            &ai_input_port,
            &output_port,
            &evaluation_context,
            &dependency_hash,
        )
    });
    let requested_output_type = evaluation_editor
        .graph()
        .node(&evaluation_node_id)
        .and_then(|node| node.descriptor.output(&output_port))
        .map(|port| port.data_type.clone())
        .unwrap_or_default();
    let mask_node_ids = ai_mask_node_ids(&evaluation_editor, &node_id);
    let model = ai_request.as_ref().and_then(|request| {
        request
            .as_ref()
            .ok()
            .and_then(|request| request.parameters.get("model"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    });
    let ai_providers = is_ai_node.then(|| app.state::<ai::AiProviderManager>().inner().clone());
    let evaluation_source = source.clone();
    let evaluation_node = node_id.clone();
    let evaluation_output = output_port.clone();
    let worker_manager = Arc::clone(&manager);
    let evaluated = match tauri::async_runtime::spawn_blocking(move || {
        if let Some(request) = ai_request {
            let request = request?;
            let provider_id = request
                .provenance
                .provider_id
                .clone()
                .ok_or_else(|| "AI request has no provider id".to_owned())?;
            let providers =
                ai_providers.ok_or_else(|| "AI provider manager is unavailable".to_owned())?;
            let task = providers.submit(&provider_id, request)?;
            let cancellation_requested = worker_manager.register_remote_ai_task(
                &evaluation_node,
                token,
                provider_id.clone(),
                task.task_id.clone(),
            )?;
            if cancellation_requested {
                let _ = providers.cancel(&provider_id, &task.task_id);
            }
            let result = providers.wait(&provider_id, &task.task_id, PollPolicy::default());
            worker_manager.clear_remote_ai_task(&evaluation_node, token)?;
            Ok(CheckpointGeneration::Provider(result?))
        } else {
            evaluation_editor
                .graph()
                .evaluate_checkpoint_generation(
                    &NodeId::from(evaluation_node),
                    &evaluation_output,
                    &checkpoint_context(evaluation_source.as_ref()),
                )
                .map_err(|error| error.to_string())
                .and_then(checkpoint_payload)
                .map(CheckpointGeneration::Local)
        }
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(format!("checkpoint worker failed: {error}")),
    };

    let mut editor = lock_editor(&state.editor)?;
    let current_source = state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())?
        .clone();
    let mut records = manager
        .checkpoints
        .lock()
        .map_err(|_| "checkpoint state is unavailable".to_owned())?;
    let record = records
        .get_mut(&node_id)
        .ok_or_else(|| format!("checkpoint '{node_id}' is not registered"))?;
    let selected_port = record.output_port.clone();

    // A late result must never change the state of a newer attempt. The
    // active token is the authoritative ownership check; node ids alone are
    // insufficient when a user cancels and immediately regenerates.
    if record.checkpoint.active_generation_token() != Some(token) {
        let status = checkpoint_record_status_dto(record, &manager.store)?;
        drop(records);
        manager.persist()?;
        return Ok(status);
    }
    let cancellation_requested = manager
        .cancellation_requests
        .lock()
        .map_err(|_| "checkpoint cancellation state is unavailable".to_owned())?
        .get(&node_id)
        .is_some_and(|requested| *requested == token);
    if cancellation_requested || record.checkpoint.state() != CheckpointState::Generating {
        if cancellation_requested {
            manager
                .cancellation_requests
                .lock()
                .map_err(|_| "checkpoint cancellation state is unavailable".to_owned())?
                .remove(&node_id);
            record
                .checkpoint
                .cancel_generation(&token)
                .map_err(|error| error.to_string())?;
        }
        record.progress = None;
        let checkpoint = record.checkpoint.clone();
        let status = checkpoint_record_status_dto(record, &manager.store)?;
        editor
            .register_checkpoint(checkpoint)
            .map_err(|error| error.to_string())?;
        drop(records);
        manager.persist()?;
        emit_checkpoint_progress(&app, &node_id, &selected_port, 0.0, "cancelled", None);
        return Ok(status);
    }

    let generated = match evaluated {
        Ok(generated) => generated,
        Err(error) => {
            record
                .checkpoint
                .fail_generation(&token, &error)
                .map_err(|failure| failure.to_string())?;
            record.progress = None;
            let checkpoint = record.checkpoint.clone();
            let status = checkpoint_record_status_dto(record, &manager.store)?;
            editor
                .register_checkpoint(checkpoint)
                .map_err(|failure| failure.to_string())?;
            drop(records);
            manager.persist()?;
            emit_checkpoint_progress(&app, &node_id, &selected_port, 0.0, "failed", Some(error));
            return Ok(status);
        }
    };

    let (current_dependency_hash, _) = checkpoint_dependencies(
        &editor,
        &node_id,
        &selected_port,
        node_version,
        current_source.as_ref(),
    )?;
    record
        .checkpoint
        .set_dependency_hash(current_dependency_hash);
    record.progress = Some(90.0);
    emit_checkpoint_progress(&app, &node_id, &selected_port, 0.9, "committing", None);
    let generation = GenerationMetadata {
        generation_revision: token.generation_id(),
        generated_at: Some(checkpoint_timestamp()),
        duration_millis: None,
        generator: Some("rawweave-desktop".to_owned()),
    };
    let artifact_result = match generated {
        CheckpointGeneration::Local(payload) => CheckpointArtifact::new(
            payload,
            dependency_hash.clone(),
            Provenance {
                dependency_hash: dependency_hash.clone(),
                upstream_hashes,
                node_version,
                external_tool: None,
            },
            generation,
        )
        .map_err(|error| error.to_string()),
        CheckpointGeneration::Provider(result) => {
            let context = ProviderArtifactContext {
                output_type: &requested_output_type,
                mask_node_ids: &mask_node_ids,
                model: model.as_deref(),
            };
            provider_checkpoint_artifact(
                &result,
                &dependency_hash,
                upstream_hashes,
                node_version,
                token.generation_id(),
                &context,
            )
        }
    };
    let artifact = match artifact_result {
        Ok(artifact) => artifact,
        Err(error) => {
            record
                .checkpoint
                .fail_generation(&token, error.to_string())
                .map_err(|failure| failure.to_string())?;
            record.progress = None;
            let checkpoint = record.checkpoint.clone();
            let status = checkpoint_record_status_dto(record, &manager.store)?;
            editor
                .register_checkpoint(checkpoint)
                .map_err(|failure| failure.to_string())?;
            drop(records);
            manager.persist()?;
            emit_checkpoint_progress(&app, &node_id, &selected_port, 0.0, "failed", Some(error));
            return Ok(status);
        }
    };
    let commit_result = record
        .checkpoint
        .commit_generation(token, artifact, &manager.store);
    record.progress = None;
    let checkpoint = record.checkpoint.clone();
    let status = checkpoint_record_status_dto(record, &manager.store)?;
    editor
        .register_checkpoint(checkpoint)
        .map_err(|error| error.to_string())?;
    drop(records);
    manager.persist()?;
    match commit_result {
        Ok(()) => emit_checkpoint_progress(&app, &node_id, &selected_port, 1.0, "complete", None),
        Err(error) if status.state == CheckpointState::Stale => emit_checkpoint_progress(
            &app,
            &node_id,
            &selected_port,
            1.0,
            "complete",
            Some(error.to_string()),
        ),
        Err(error) => emit_checkpoint_progress(
            &app,
            &node_id,
            &selected_port,
            0.0,
            "failed",
            Some(error.to_string()),
        ),
    }
    Ok(status)
}

#[tauri::command]
fn cancel_checkpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
) -> Result<CheckpointStatusDto, String> {
    let mut editor = lock_editor(&state.editor)?;
    let source = state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())?
        .clone();
    let manager = Arc::clone(&state.checkpoint);
    let (checkpoint, status, output_port, was_generating, remote_task) = {
        let mut records = manager
            .checkpoints
            .lock()
            .map_err(|_| "checkpoint state is unavailable".to_owned())?;
        let record = ensure_checkpoint_record(&mut records, &editor, &node_id, None)?;
        let was_generating = record.checkpoint.state() == CheckpointState::Generating;
        let remote_task = if was_generating {
            let token = record
                .checkpoint
                .active_generation_token()
                .ok_or_else(|| "checkpoint generation token is unavailable".to_owned())?;
            manager
                .cancellation_requests
                .lock()
                .map_err(|_| "checkpoint cancellation state is unavailable".to_owned())?
                .insert(node_id.clone(), token);
            record
                .checkpoint
                .cancel_generation(&token)
                .map_err(|error| error.to_string())?;
            record.progress = None;
            manager.take_remote_ai_task(&node_id, token)?
        } else {
            let (dependency_hash, _) = checkpoint_dependencies(
                &editor,
                &node_id,
                &record.output_port,
                record.checkpoint.node_version,
                source.as_ref(),
            )?;
            record.checkpoint.set_dependency_hash(dependency_hash);
            None
        };
        let checkpoint = record.checkpoint.clone();
        let status = checkpoint_record_status_dto(record, &manager.store)?;
        let output_port = record.output_port.clone();
        (checkpoint, status, output_port, was_generating, remote_task)
    };
    editor
        .register_checkpoint(checkpoint)
        .map_err(|error| error.to_string())?;
    manager.persist()?;
    if let Some(remote_task) = remote_task {
        let providers = app.state::<ai::AiProviderManager>();
        let _ = providers.cancel(&remote_task.provider_id, &remote_task.task_id);
    }
    if was_generating {
        emit_checkpoint_progress(&app, &node_id, &output_port, 0.0, "cancelled", None);
    }
    Ok(status)
}

pub(crate) fn preview_diagnostics_enabled() -> bool {
    std::env::var_os("RAWWEAVE_PREVIEW_DIAGNOSTICS").is_some()
}

pub fn run() {
    let preview = Arc::new(preview::PreviewManager::default());
    let protocol_preview = Arc::clone(&preview);
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("rawweave-preview", move |_ctx, request| {
            let response = protocol_preview.response(&request);
            if preview_diagnostics_enabled() {
                eprintln!(
                    "[preview] fetch uri={} status={} bytes={}",
                    request.uri(),
                    response.status(),
                    response.body().len()
                );
            }
            response
        })
        .setup(move |app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let checkpoint = Arc::new(
                CheckpointManager::persistent(data_dir.join("checkpoints"))
                    .map_err(std::io::Error::other)?,
            );
            app.manage(AppState::with_checkpoint_manager(checkpoint, preview));
            let ai_providers = ai::AiProviderManager::persistent(
                app.path()
                    .app_config_dir()
                    .map_err(|error| std::io::Error::other(error.to_string()))?
                    .join("ai-providers.json"),
            )
            .map_err(std::io::Error::other)?;
            app.manage(ai_providers);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            node_descriptors,
            add_external_host,
            remove_external_host,
            list_external_hosts,
            test_external_host,
            discover_external_host,
            discover_external_hosts,
            external_host_diagnostics,
            add_node,
            remove_node,
            connect_nodes,
            disconnect_nodes,
            set_node_parameter,
            expose_parameter,
            unexpose_parameter,
            create_subgraph,
            create_subgraph_from_selection,
            expose_workflow_parameter,
            hide_workflow_parameter,
            set_workflow_parameter,
            expose_workflow_input,
            expose_workflow_output,
            hide_workflow_port,
            save_blueprint,
            export_blueprint,
            load_blueprint,
            import_blueprint,
            instantiate_blueprint,
            open_subgraph,
            return_to_parent,
            workflow_hash,
            dependency_report,
            dependency_status,
            workflow_dependency_report,
            save_workflow,
            load_workflow,
            restore_workflow_history,
            create_batch_job,
            load_batch_job,
            batch_preflight,
            start_batch,
            pause_batch,
            resume_batch,
            cancel_batch,
            retry_failed_batch,
            retry_selected_batch,
            skip_batch_items,
            batch_snapshot,
            batch_dry_run,
            open_failed_batch_item,
            open_image,
            open_image_set,
            request_preview,
            cancel_preview,
            release_preview,
            checkpoint_list,
            checkpoint_status,
            generate_checkpoint,
            cancel_checkpoint,
            ai::list_ai_providers,
            ai::add_ai_provider,
            ai::remove_ai_provider,
            ai::set_ai_provider_credential,
            ai::delete_ai_provider_credential,
            ai::test_ai_provider,
            ai::submit_ai_task,
            ai::ai_task_status,
            ai::ai_task_result,
            ai::wait_ai_task,
            ai::cancel_ai_task,
            browser::list_directory,
            browser::inspect_file,
            browser::set_file_marks,
            browser::rename_file,
            browser::move_file,
            browser::copy_file,
            browser::reveal_file,
            browser::trash_file,
            browser::save_session,
            browser::load_session,
        ]);
    #[cfg(feature = "wdio-e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .run(tauri::generate_context!())
        .expect("error while running RawWeave");
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType, Encoder};
    use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};

    #[test]
    fn app_state_uses_preview_manager_registered_for_uri_requests() {
        let protocol_preview = Arc::new(preview::PreviewManager::default());
        let state = AppState::with_checkpoint_manager(
            Arc::new(CheckpointManager::default()),
            Arc::clone(&protocol_preview),
        );
        assert!(Arc::ptr_eq(&state.preview, &protocol_preview));
        state
            .preview
            .store
            .insert(preview::preview_path("shared"), 1, vec![1, 2, 3])
            .unwrap();
        let request = tauri::http::Request::builder()
            .uri(preview::preview_url("shared"))
            .body(Vec::new())
            .unwrap();
        let response = protocol_preview.response(&request);
        assert_eq!(response.status(), tauri::http::StatusCode::OK);
        assert_eq!(response.body(), &[1, 2, 3]);
    }

    #[test]
    fn tauri_batch_request_rejects_worker_counts_above_the_engine_bound() {
        let error = validate_batch_workers(MAX_BATCH_WORKERS + 1)
            .expect_err("the Tauri request boundary must reject excessive workers");
        assert!(error.contains("maximum"));
    }
    use std::io::Cursor;

    #[test]
    fn step5_create_subgraph_returns_a_round_trippable_blueprint_dto() {
        let mut editor = EditorCore::default();
        editor.add_node("input", "core.image-input").unwrap();
        editor.add_node("exposure", "core.exposure").unwrap();
        editor.add_node("output", "core.output").unwrap();
        editor
            .connect("input", "image", "exposure", "image")
            .unwrap();
        editor
            .connect("exposure", "image", "output", "image")
            .unwrap();

        let request = CreateSubgraphRequest {
            node_ids: vec!["exposure".to_owned()],
            id: "looks.exposure".to_owned(),
            version: "1.0.0".to_owned(),
            metadata: WorkflowMetadataDto {
                name: "Exposure".to_owned(),
                ..WorkflowMetadataDto::default()
            },
            node_pack_dependencies: Vec::new(),
            subgraph_dependencies: Vec::new(),
        };
        let blueprint = build_subgraph_definition(&editor, request).unwrap();
        let dto = workflow_definition_dto(&blueprint);

        assert_eq!(dto.identity.id, "looks.exposure");
        assert_eq!(dto.inputs.len(), 1);
        assert_eq!(dto.outputs.len(), 1);
        assert_eq!(dto.hash.len(), 64);
        let exported = editor.save_blueprint(&blueprint).unwrap();
        let imported = editor.load_blueprint(&exported).unwrap();
        assert_eq!(imported.hash(), dto.hash);
    }

    #[test]
    fn step5_navigation_commands_restore_parent_state_after_child_mutation() {
        let state = AppState::default();
        {
            let mut editor = state.editor.lock().unwrap();
            editor.add_node("input", "core.image-input").unwrap();
            editor.add_node("exposure", "core.exposure").unwrap();
            editor.add_node("output", "core.output").unwrap();
            editor
                .connect("input", "image", "exposure", "image")
                .unwrap();
            editor
                .connect("exposure", "image", "output", "image")
                .unwrap();
        }

        let child = create_subgraph_command(
            &state,
            CreateSubgraphRequest {
                node_ids: vec!["exposure".to_owned()],
                id: "looks.exposure".to_owned(),
                version: "1.0.0".to_owned(),
                metadata: WorkflowMetadataDto {
                    name: "Exposure".to_owned(),
                    ..WorkflowMetadataDto::default()
                },
                node_pack_dependencies: vec![WorkflowDependencyDto {
                    id: "missing-pack".to_owned(),
                    version: "2.0.0".to_owned(),
                    hash: None,
                }],
                subgraph_dependencies: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(child.identity.id, "looks.exposure");

        let opened = open_subgraph_state(&state, child.identity.id.clone()).unwrap();
        assert_eq!(opened.graph.nodes.len(), 1);
        assert_eq!(state.editor.lock().unwrap().graph().nodes().len(), 1);

        let hidden = mutate_current_blueprint(&state, |_editor, blueprint| {
            assert!(blueprint.hide_port("input:exposure:image"));
            Ok(())
        })
        .unwrap();
        assert!(hidden.inputs.is_empty());

        let parent = return_to_parent_state(&state).unwrap();
        assert_eq!(parent.graph.nodes.len(), 3);
        assert!(parent.nested_subgraphs["looks.exposure"].inputs.is_empty());
        assert_eq!(state.editor.lock().unwrap().graph().nodes().len(), 3);

        let report = dependency_report_for_state(&state, None, None).unwrap();
        assert_eq!(report.missing[0].id, "looks.exposure");
    }

    #[test]
    fn step5_blueprint_commands_round_trip_and_reinstantiate_without_source_loss() {
        let state = AppState::default();
        {
            let mut editor = state.editor.lock().unwrap();
            editor.add_node("input", "core.image-input").unwrap();
            editor.add_node("output", "core.output").unwrap();
            editor.connect("input", "image", "output", "image").unwrap();
        }

        let hash = workflow_hash_state(&state).unwrap();
        let serialized = save_blueprint_state(&state).unwrap();
        let imported = load_blueprint_command(&state, &serialized).unwrap();
        assert_eq!(imported.identity.id, "workflow");
        assert_eq!(workflow_hash_state(&state).unwrap(), hash);

        state
            .source_image
            .lock()
            .unwrap()
            .replace(SourceAsset::Ordinary(Image::new(1, 1).unwrap()));
        state.preview.begin("instantiate-preview");
        {
            let mut editor = state.editor.lock().unwrap();
            editor.remove_node("input").unwrap();
        }

        instantiate_loaded_blueprint(&state, None).unwrap();

        assert_eq!(state.editor.lock().unwrap().graph().nodes().len(), 2);
        assert_eq!(workflow_hash_state(&state).unwrap(), hash);
        assert!(state.source_image.lock().unwrap().is_none());
        assert!(state.preview.is_cancelled("instantiate-preview"));
        assert_eq!(
            *state.source_selection.lock().unwrap(),
            SourceSelectionIntent::AttachToLoadedWorkflow
        );
    }

    #[test]
    fn step5_dependency_command_reports_version_mismatches_and_statuses() {
        let state = AppState::default();
        {
            let mut editor = state.editor.lock().unwrap();
            editor.add_node("exposure", "core.exposure").unwrap();
        }
        let child = create_subgraph_command(
            &state,
            CreateSubgraphRequest {
                node_ids: vec!["exposure".to_owned()],
                id: "looks.exposure".to_owned(),
                version: "1.0.0".to_owned(),
                metadata: WorkflowMetadataDto {
                    name: "Exposure".to_owned(),
                    ..WorkflowMetadataDto::default()
                },
                node_pack_dependencies: vec![WorkflowDependencyDto {
                    id: "looks-pack".to_owned(),
                    version: "2.0.0".to_owned(),
                    hash: None,
                }],
                subgraph_dependencies: Vec::new(),
            },
        )
        .unwrap();
        open_subgraph_state(&state, child.identity.id).unwrap();

        let report = dependency_report_for_state(
            &state,
            Some(vec![AvailableNodePackDto {
                package_id: "looks-pack".to_owned(),
                version: "1.0.0".to_owned(),
                nodes: vec![AvailableNodeDto {
                    type_id: "core.exposure".to_owned(),
                    version: 1,
                }],
            }]),
            None,
        )
        .unwrap();

        assert_eq!(report.mismatched.len(), 1);
        assert!(report.missing.is_empty());
        assert!(report.disabled_nodes.is_empty());
        assert!(matches!(
            report.statuses.get("looks-pack"),
            Some(DependencyStatusDto::VersionMismatch { required, available })
                if required == "2.0.0" && available == "1.0.0"
        ));
    }

    #[test]
    fn step5_dependency_status_uses_builtin_pack_manifests() {
        let state = AppState::default();
        {
            let mut editor = state.editor.lock().unwrap();
            editor.add_node("exposure", "core.exposure").unwrap();
        }
        let mut blueprint = current_or_flat_blueprint(&state).unwrap();
        blueprint
            .add_node_pack_dependency("core-image", env!("CARGO_PKG_VERSION"))
            .unwrap();
        *state.blueprint.lock().unwrap() = Some(blueprint);

        let report = dependency_report_for_state(&state, None, None).unwrap();

        assert!(report.missing.is_empty());
        assert!(report.mismatched.is_empty());
        assert!(report.disabled_nodes.is_empty());
        assert!(matches!(
            report.statuses.get("core-image"),
            Some(DependencyStatusDto::Available)
        ));
    }

    #[test]
    fn image_set_loader_preserves_canonical_order_and_source_references() {
        let first_path = std::env::temp_dir().join(format!(
            "rawweave-open-imageset-first-{}.png",
            std::process::id()
        ));
        let second_path = std::env::temp_dir().join(format!(
            "rawweave-open-imageset-second-{}.png",
            std::process::id()
        ));
        std::fs::write(
            &first_path,
            rgba_png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]),
        )
        .unwrap();
        std::fs::write(
            &second_path,
            rgba_png(2, 1, &[0, 0, 255, 255, 255, 255, 255, 255]),
        )
        .unwrap();
        let paths = vec![
            second_path.to_string_lossy().into_owned(),
            first_path.to_string_lossy().into_owned(),
        ];

        let result = open_image_set_files_with_decoder(
            &paths,
            rawweave_node_api::ImageSetOrder::Unordered,
            &RawloaderDecoder::default(),
        )
        .unwrap();
        let _ = std::fs::remove_file(&first_path);
        let _ = std::fs::remove_file(&second_path);

        let (set, members) = result;
        let mut expected_ids = vec![
            second_path.to_string_lossy().into_owned(),
            first_path.to_string_lossy().into_owned(),
        ];
        expected_ids.sort();
        assert_eq!(set.member_ids(), expected_ids);
        assert_eq!(
            set.members()[0].source().unwrap().path(),
            set.members()[0].id
        );
        assert_eq!(members[0].id, set.members()[0].id);
        assert_eq!(members[0].path, set.members()[0].source().unwrap().path);
    }

    #[test]
    fn image_set_loader_accepts_bounded_raw_members_and_renders_them_to_images() {
        let path = std::env::temp_dir().join(format!(
            "rawweave-open-imageset-raw-{}.dng",
            std::process::id()
        ));
        std::fs::write(&path, b"deterministic raw fixture").unwrap();
        let path_string = path.to_string_lossy().into_owned();
        let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());

        let result = open_image_set_files_with_decoder(
            std::slice::from_ref(&path_string),
            rawweave_node_api::ImageSetOrder::Ordered,
            &decoder,
        );
        let _ = std::fs::remove_file(&path);

        let (set, members) = result.unwrap();
        assert_eq!(set.member_ids(), vec![path_string.clone()]);
        assert_eq!(
            (
                set.members()[0].image.width(),
                set.members()[0].image.height()
            ),
            (4, 2)
        );
        assert_eq!(members[0].path, path_string);
        assert_eq!(members[0].metadata.as_ref().unwrap().camera, "Canon EOS R5");
        assert_eq!(set.members()[0].metadata.make, "Canon");
        assert_eq!(set.members()[0].metadata.model, "EOS R5");
    }

    #[test]
    fn opening_an_image_set_replaces_the_graph_and_retains_the_loaded_set() {
        let path = std::env::temp_dir().join(format!(
            "rawweave-open-imageset-state-{}.png",
            std::process::id()
        ));
        std::fs::write(&path, rgba_png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255])).unwrap();
        let path_string = path.to_string_lossy().into_owned();
        let state = AppState::default();

        let result = open_image_set_state(
            &state,
            std::slice::from_ref(&path_string),
            rawweave_node_api::ImageSetOrder::Ordered,
            &RawloaderDecoder::default(),
        )
        .unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(result.kind, "imageset");
        assert_eq!(result.members[0].path, path_string);
        assert!(matches!(
            result.alignment,
            OpenImageSetAlignmentDto::Unaligned
        ));
        let editor = state.editor.lock().unwrap();
        assert!(editor
            .graph()
            .nodes()
            .contains_key(&NodeId::from("imageset-input")));
        assert!(editor
            .graph()
            .nodes()
            .contains_key(&NodeId::from("imageset-select")));
        assert!(matches!(
            state.source_image.lock().unwrap().as_ref(),
            Some(SourceAsset::ImageSet(set)) if set.member_ids() == vec![path_string.clone()]
        ));
    }

    #[test]
    fn open_image_file_returns_source_dimensions_and_revision() {
        let path = std::env::temp_dir().join(format!(
            "rawweave-open-image-helper-{}.png",
            std::process::id()
        ));
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(Cursor::new(&mut bytes), 3, 2);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255, 128, 128, 128,
                255, 0, 0, 0, 255,
            ])
            .unwrap();
        writer.finish().unwrap();
        std::fs::write(&path, bytes).unwrap();

        let (image, metadata) = open_image_file(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!((image.width(), image.height()), (3, 2));
        assert_eq!((metadata.width, metadata.height), (3, 2));
        assert_eq!(metadata.revision, image.revision());
    }

    #[test]
    fn opens_a_raw_source_with_injected_decoder_and_structured_metadata() {
        let path =
            std::env::temp_dir().join(format!("rawweave-open-raw-{}.dng", std::process::id()));
        std::fs::write(&path, b"deterministic raw fixture").unwrap();
        let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());

        let (source, metadata) = open_image_file_with_decoder(&path, &decoder).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(matches!(source, SourceAsset::Raw { .. }));
        assert_eq!((metadata.width, metadata.height), (4, 2));
        let raw_metadata = metadata.metadata.expect("RAW metadata");
        assert_eq!(raw_metadata.camera, "Canon EOS R5");
        assert_eq!(raw_metadata.lens, None);
        assert_eq!(raw_metadata.orientation, "Normal");
        assert_eq!(raw_metadata.dimensions.width, 4);
        assert_eq!(raw_metadata.dimensions.height, 2);
    }

    #[test]
    fn deterministic_raw_open_builds_graph_and_renders_display_png() {
        let path =
            std::env::temp_dir().join(format!("rawweave-open-preview-{}.dng", std::process::id()));
        std::fs::write(&path, b"deterministic raw fixture").unwrap();
        let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());
        let mut editor = EditorCore::new_with_raw_decoder(decoder.clone());

        let (source, metadata) = open_image_with_decoder(&path, &decoder, &mut editor).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(matches!(source, SourceAsset::Raw { .. }));
        assert_eq!((metadata.width, metadata.height), (4, 2));
        assert_eq!(editor.graph().nodes().len(), 8);
        assert_eq!(metadata.revision, editor.graph().revision());

        let revision = editor.graph().revision();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let manager = preview::PreviewManager::default();
        let request = preview::PreviewRequest {
            request_id: "open-graph-preview".to_owned(),
            revision,
            node_id: "display-transform".to_owned(),
            output_port: "display".to_owned(),
            quality: preview::PreviewQualityRequest::Preview,
            region: preview::PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            },
            tile: preview::PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display: preview::MaskDisplayRequest::Grayscale,
        };

        let rendered =
            preview::render_preview(&manager, &editor, &current_editor, Some(source), request)
                .unwrap();
        assert_eq!((rendered.full_width, rendered.full_height), (4, 2));
        let bytes = manager
            .store
            .get(&preview::preview_path("open-graph-preview"))
            .unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn raw_open_builds_the_complete_graph_without_serializing_source_bytes() {
        let mut editor = EditorCore::new_with_raw_decoder(DeterministicDecoder::new(
            DeterministicCorpus::bayer_12_bit(),
        ));
        build_raw_workflow(&mut editor).unwrap();
        let serialized = editor.save_workflow().unwrap();

        for type_id in [
            "raw.decode",
            "raw.black-level",
            "raw.white-balance",
            "raw.highlight-reconstruction",
            "raw.demosaic",
            "raw.camera-transform",
            "raw.lens-correction",
            "raw.display-transform",
        ] {
            assert!(serialized.contains(type_id), "missing {type_id}");
        }
        assert!(!serialized.contains("deterministic raw fixture"));
        assert_eq!(editor.graph().nodes().len(), 8);
        assert_eq!(editor.graph().edges().len(), 9);
    }

    #[test]
    fn opening_an_ordinary_image_replaces_the_raw_graph_and_renders_the_standard_output() {
        let path =
            std::env::temp_dir().join(format!("rawweave-open-ordinary-{}.png", std::process::id()));
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(Cursor::new(&mut bytes), 2, 2);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ])
            .unwrap();
        writer.finish().unwrap();
        std::fs::write(&path, bytes).unwrap();

        let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());
        let mut editor = EditorCore::new_with_raw_decoder(decoder.clone());
        build_raw_workflow(&mut editor).unwrap();
        let (source, metadata) = open_image_with_decoder(&path, &decoder, &mut editor).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(matches!(source, SourceAsset::Ordinary(_)));
        assert_eq!((metadata.width, metadata.height), (2, 2));
        assert_eq!(editor.graph().nodes().len(), 2);
        assert_eq!(editor.graph().edges().len(), 1);
        assert!(editor
            .graph()
            .nodes()
            .values()
            .all(|node| !node.type_id.starts_with("raw.")));

        let revision = editor.graph().revision();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let manager = preview::PreviewManager::default();
        let request = preview::PreviewRequest {
            request_id: "ordinary-open-preview".to_owned(),
            revision,
            node_id: "output".to_owned(),
            output_port: "image".to_owned(),
            quality: preview::PreviewQualityRequest::Preview,
            region: preview::PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            tile: preview::PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display: preview::MaskDisplayRequest::Grayscale,
        };
        let rendered =
            preview::render_preview(&manager, &editor, &current_editor, Some(source), request)
                .unwrap();
        assert_eq!((rendered.full_width, rendered.full_height), (2, 2));
    }

    #[test]
    fn bounded_raw_read_rejects_bytes_added_after_the_initial_limit_without_retaining_them() {
        let error =
            read_bounded_raw(Cursor::new(vec![1, 2, 3, 4, 5]), "growing.dng", 4).unwrap_err();
        assert!(error.contains("too large"));

        let bytes = read_bounded_raw(Cursor::new(vec![1, 2, 3, 4]), "exact.dng", 4).unwrap();
        assert_eq!(bytes, vec![1, 2, 3, 4]);
        assert!(bytes.capacity() <= 4);
    }

    #[test]
    fn loaded_raw_workflow_reselection_attaches_source_without_mutating_graph() {
        let path =
            std::env::temp_dir().join(format!("rawweave-reattach-raw-{}.dng", std::process::id()));
        std::fs::write(&path, b"deterministic raw fixture").unwrap();
        let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());

        let mut loaded_editor = EditorCore::new_with_raw_decoder(decoder.clone());
        build_raw_workflow(&mut loaded_editor).unwrap();
        loaded_editor
            .set_node_parameter("white-balance", "red_gain", ParameterValue::Float(1.75))
            .unwrap();
        loaded_editor
            .add_node("custom", "raw.white-balance")
            .unwrap();
        let workflow = loaded_editor.save_workflow().unwrap();
        let state = AppState {
            editor: Arc::new(Mutex::new(EditorCore::new_with_raw_decoder(
                decoder.clone(),
            ))),
            hosts: Arc::new(Mutex::new(hosts::HostManager::memory())),
            preview: Arc::new(preview::PreviewManager::default()),
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
            batch: Arc::new(BatchManager::default()),
            checkpoint: Arc::new(CheckpointManager::default()),
        };

        load_workflow_state(&state, &workflow).unwrap();
        let (metadata, source) = open_image_state(&state, &path, &decoder).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            state.editor.lock().unwrap().save_workflow().unwrap(),
            workflow
        );
        assert_eq!(
            metadata.revision,
            state.editor.lock().unwrap().graph().revision()
        );
        assert!(matches!(source, SourceAsset::Raw { .. }));
        assert!(matches!(
            state.source_image.lock().unwrap().as_ref(),
            Some(SourceAsset::Raw { .. })
        ));
        assert_eq!(
            *state.source_selection.lock().unwrap(),
            SourceSelectionIntent::ReplaceWorkflow
        );

        let editor = state.editor.lock().unwrap().clone();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let request = preview::PreviewRequest {
            request_id: "reattach-preview".to_owned(),
            revision: editor.graph().revision(),
            node_id: "display-transform".to_owned(),
            output_port: "display".to_owned(),
            quality: preview::PreviewQualityRequest::Preview,
            region: preview::PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            },
            tile: preview::PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display: preview::MaskDisplayRequest::Grayscale,
        };
        let source = state.source_image.lock().unwrap().clone();
        let rendered =
            preview::render_preview(&state.preview, &editor, &current_editor, source, request)
                .unwrap();
        assert_eq!((rendered.full_width, rendered.full_height), (4, 2));
    }

    #[test]
    fn incompatible_reselection_preserves_loaded_graph_and_attach_intent() {
        let raw_path = std::env::temp_dir().join(format!(
            "rawweave-incompatible-raw-{}.dng",
            std::process::id()
        ));
        let ordinary_path = std::env::temp_dir().join(format!(
            "rawweave-incompatible-ordinary-{}.png",
            std::process::id()
        ));
        std::fs::write(&raw_path, b"deterministic raw fixture").unwrap();
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(Cursor::new(&mut bytes), 1, 1);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255, 0, 0, 255]).unwrap();
        writer.finish().unwrap();
        std::fs::write(&ordinary_path, bytes).unwrap();
        let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());

        let mut loaded_editor = EditorCore::new_with_raw_decoder(decoder.clone());
        build_raw_workflow(&mut loaded_editor).unwrap();
        loaded_editor
            .set_node_parameter("white-balance", "red_gain", ParameterValue::Float(1.5))
            .unwrap();
        let workflow = loaded_editor.save_workflow().unwrap();
        let state = AppState {
            editor: Arc::new(Mutex::new(EditorCore::new_with_raw_decoder(
                decoder.clone(),
            ))),
            hosts: Arc::new(Mutex::new(hosts::HostManager::memory())),
            preview: Arc::new(preview::PreviewManager::default()),
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
            batch: Arc::new(BatchManager::default()),
            checkpoint: Arc::new(CheckpointManager::default()),
        };
        load_workflow_state(&state, &workflow).unwrap();

        let error = open_image_state(&state, &ordinary_path, &decoder).unwrap_err();
        let _ = std::fs::remove_file(&raw_path);
        let _ = std::fs::remove_file(&ordinary_path);

        assert!(error.contains("RAW workflow requires a RAW source"));
        assert_eq!(
            state.editor.lock().unwrap().save_workflow().unwrap(),
            workflow
        );
        assert!(state.source_image.lock().unwrap().is_none());
        assert_eq!(
            *state.source_selection.lock().unwrap(),
            SourceSelectionIntent::AttachToLoadedWorkflow
        );
    }

    #[test]
    fn workflow_load_clears_source_and_cancels_previews_before_reselection() {
        let mut loaded_editor = EditorCore::default();
        loaded_editor.add_node("input", "core.image-input").unwrap();
        loaded_editor.add_node("output", "core.output").unwrap();
        loaded_editor
            .connect("input", "image", "output", "image")
            .unwrap();
        let workflow = loaded_editor.save_workflow().unwrap();
        let preview = Arc::new(preview::PreviewManager::default());
        preview.begin("load-preview");
        preview
            .store
            .insert(preview::preview_path("load-preview"), 1, vec![1, 2, 3])
            .unwrap();
        let state = AppState {
            editor: Arc::new(Mutex::new(EditorCore::default())),
            hosts: Arc::new(Mutex::new(hosts::HostManager::memory())),
            preview: Arc::clone(&preview),
            source_image: Mutex::new(Some(SourceAsset::Ordinary(Image::new(1, 1).unwrap()))),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
            batch: Arc::new(BatchManager::default()),
            checkpoint: Arc::new(CheckpointManager::default()),
        };

        load_workflow_state(&state, &workflow).unwrap();

        assert!(state.source_image.lock().unwrap().is_none());
        assert!(preview.is_cancelled("load-preview"));
        assert_eq!(preview.store.len(), 0);
        let editor = state.editor.lock().unwrap().clone();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let request = preview::PreviewRequest {
            request_id: "after-load".to_owned(),
            revision: editor.graph().revision(),
            node_id: "output".to_owned(),
            output_port: "image".to_owned(),
            quality: preview::PreviewQualityRequest::Preview,
            region: preview::PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            tile: preview::PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
            mask_display: preview::MaskDisplayRequest::Grayscale,
        };
        let error =
            preview::render_preview(&preview, &editor, &current_editor, None, request).unwrap_err();
        assert!(error.contains("source image unavailable"));
    }

    #[test]
    fn history_restore_preserves_runtime_source_and_source_selection() {
        let mut loaded_editor = EditorCore::default();
        loaded_editor.add_node("input", "core.image-input").unwrap();
        loaded_editor.add_node("output", "core.output").unwrap();
        loaded_editor
            .connect("input", "image", "output", "image")
            .unwrap();
        let workflow = loaded_editor.save_workflow().unwrap();
        let source =
            SourceAsset::Ordinary(Image::from_pixels(1, 1, vec![[0.25, 0.5, 0.75, 1.0]]).unwrap());
        let preview = Arc::new(preview::PreviewManager::default());
        preview.begin("history-restore-preview");
        let state = AppState {
            editor: Arc::new(Mutex::new(EditorCore::default())),
            hosts: Arc::new(Mutex::new(hosts::HostManager::memory())),
            preview: Arc::clone(&preview),
            source_image: Mutex::new(Some(source.clone())),
            source_selection: Mutex::new(SourceSelectionIntent::ReplaceWorkflow),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
            batch: Arc::new(BatchManager::default()),
            checkpoint: Arc::new(CheckpointManager::default()),
        };

        restore_workflow_history_state(&state, &workflow).unwrap();

        let source_image = state.source_image.lock().unwrap();
        let Some(SourceAsset::Ordinary(image)) = source_image.as_ref() else {
            panic!("history restore should retain the ordinary source");
        };
        assert_eq!(image.pixel(0, 0), Some([0.25, 0.5, 0.75, 1.0]));
        drop(source_image);
        assert_eq!(
            *state.source_selection.lock().unwrap(),
            SourceSelectionIntent::ReplaceWorkflow
        );
        assert!(preview.is_cancelled("history-restore-preview"));
        assert_eq!(
            state.editor.lock().unwrap().save_workflow().unwrap(),
            workflow
        );
    }

    #[test]
    fn step10_checkpoint_status_dto_preserves_stale_artifact_and_provenance() {
        let store = ArtifactStore::memory();
        let mut checkpoint = Checkpoint::new("manual", 1);
        checkpoint.set_dependency_hash("input-a");
        let artifact = CheckpointArtifact::new(
            CheckpointPayload::Image(Image::from_pixels(1, 1, vec![[0.5, 0.5, 0.5, 1.0]]).unwrap()),
            "input-a",
            Provenance::new("input-a", 1),
            GenerationMetadata::new(3),
        )
        .unwrap();
        let artifact_id = artifact.id().to_string();
        checkpoint.commit(artifact, &store).unwrap();
        checkpoint.set_dependency_hash("input-b");

        let status = checkpoint_status_dto(&checkpoint, &store).unwrap();

        assert_eq!(status.node_id, "manual");
        assert_eq!(status.state, rawweave_graph::CheckpointState::Stale);
        assert_eq!(
            status.committed_artifact_id.as_deref(),
            Some(artifact_id.as_str())
        );
        assert!(status.provenance.is_some());
    }

    #[test]
    fn step12_spatial_checkpoint_outputs_have_provider_operation_and_encoding_specs() {
        let cases = [
            (
                "ai.subject-segmentation",
                "mask",
                "core.Mask",
                "png-mask-v1",
            ),
            (
                "ai.subject-segmentation",
                "mask_set",
                "core.MaskSet",
                "png-mask-set-v1",
            ),
            (
                "ai.semantic-segmentation",
                "label_map",
                "core.LabelMap",
                "png-label-map-v1",
            ),
            (
                "ai.semantic-segmentation",
                "confidence",
                "core.ConfidenceMap",
                "png-confidence-v1",
            ),
            ("ai.prompt-segmentation", "mask", "core.Mask", "png-mask-v1"),
            (
                "ai.scene-analysis",
                "regions",
                "core.RegionSet",
                "png-region-set-v1",
            ),
        ];

        for (type_id, output_port, output_type, encoding) in cases {
            let spec = ai_checkpoint_spec(type_id, output_port).unwrap();
            assert_eq!(spec.operation, AiOperation::Img2Img);
            assert_eq!(spec.output_type, output_type);
            assert_eq!(spec.encoding, encoding);
        }
    }

    #[test]
    fn step12_provider_png_outputs_become_typed_spatial_payloads() {
        let result = rawweave_ai_provider::AiResult::from_bytes(
            "task-spatial",
            rgba_png(2, 1, &[255, 255, 255, 255, 0, 0, 0, 255]),
            "image/png",
        );

        assert!(matches!(
            provider_result_payload(&result, "core.MaskSet"),
            Ok(CheckpointPayload::MaskSet(set)) if set.len() == 1
        ));
        assert!(matches!(
            provider_result_payload(&result, "core.LabelMap"),
            Ok(CheckpointPayload::LabelMap(map))
                if map.dimensions() == rawweave_image::Dimensions::new(2, 1)
                    && map.values() == [255, 0]
        ));
        assert!(matches!(
            provider_result_payload(&result, "core.ConfidenceMap"),
            Ok(CheckpointPayload::ConfidenceMap(map)) if map.dimensions().width == 2
        ));
        assert!(matches!(
            provider_result_payload(&result, "core.DepthMap"),
            Ok(CheckpointPayload::DepthMap(map)) if map.values() == [1.0, 0.0]
        ));
        assert!(matches!(
            provider_result_payload(&result, "core.RegionSet"),
            Ok(CheckpointPayload::RegionSet(set)) if set.len() == 1
        ));
    }

    #[test]
    fn step12_provider_structured_outputs_preserve_typed_spatial_metadata() {
        let result = rawweave_ai_provider::AiResult::from_bytes(
            "task-structured-mask",
            serde_json::to_vec(&serde_json::json!({
                "type": "mask",
                "width": 2,
                "height": 1,
                "origin": [8, 12],
                "values": [0.25, 0.75]
            }))
            .unwrap(),
            "application/json; charset=utf-8",
        );
        let CheckpointPayload::Mask(mask) = provider_result_payload(&result, "core.Mask").unwrap()
        else {
            panic!("structured mask output must become a mask checkpoint payload");
        };
        assert_eq!(mask.dimensions(), Dimensions::new(2, 1));
        assert_eq!(mask.origin(), (8, 12));
        assert_eq!(mask.values(), [0.25, 0.75]);

        let result = rawweave_ai_provider::AiResult::from_bytes(
            "task-structured-labels",
            serde_json::to_vec(&serde_json::json!({
                "type": "label_map",
                "width": 2,
                "height": 1,
                "origin": [8, 12],
                "values": [3, 4],
                "labels": {"subject": 3, "background": 4}
            }))
            .unwrap(),
            "application/json",
        );
        let CheckpointPayload::LabelMap(map) =
            provider_result_payload(&result, "core.LabelMap").unwrap()
        else {
            panic!("structured label output must become a label-map checkpoint payload");
        };
        assert_eq!(map.origin(), (8, 12));
        assert_eq!(map.values(), [3, 4]);
        assert_eq!(map.label_value("subject"), Some(3));

        let result = rawweave_ai_provider::AiResult::from_bytes(
            "task-structured-regions",
            serde_json::to_vec(&serde_json::json!({
                "type": "region_set",
                "regions": [{"x": 8, "y": 12, "width": 2, "height": 1}]
            }))
            .unwrap(),
            "application/json",
        );
        let CheckpointPayload::RegionSet(regions) =
            provider_result_payload(&result, "core.RegionSet").unwrap()
        else {
            panic!("structured regions output must become a region-set checkpoint payload");
        };
        assert_eq!(regions.regions(), &[Region::new(8, 12, 2, 1)]);
    }

    #[test]
    fn step12_provider_structured_outputs_are_bounded_and_type_checked() {
        let oversized = rawweave_ai_provider::AiResult::from_bytes(
            "task-oversized-plane",
            serde_json::to_vec(&serde_json::json!({
                "type": "mask",
                "width": 4097,
                "height": 4096,
                "values": []
            }))
            .unwrap(),
            "application/json",
        );
        let error = provider_result_payload(&oversized, "core.Mask").unwrap_err();
        assert!(error.contains("AI spatial output exceeds"));

        let wrong_type = rawweave_ai_provider::AiResult::from_bytes(
            "task-wrong-type",
            serde_json::to_vec(&serde_json::json!({
                "type": "label_map",
                "width": 1,
                "height": 1,
                "values": [1]
            }))
            .unwrap(),
            "application/json",
        );
        let error = provider_result_payload(&wrong_type, "core.Mask").unwrap_err();
        assert!(error.contains("does not match 'mask'"));

        let empty_set = rawweave_ai_provider::AiResult::from_bytes(
            "task-empty-set",
            serde_json::to_vec(&serde_json::json!({
                "type": "mask_set",
                "masks": []
            }))
            .unwrap(),
            "application/json",
        );
        let error = provider_result_payload(&empty_set, "core.MaskSet").unwrap_err();
        assert!(error.contains("must contain between one and"));

        let overflowing_region = rawweave_ai_provider::AiResult::from_bytes(
            "task-overflowing-region",
            serde_json::to_vec(&serde_json::json!({
                "type": "region_set",
                "regions": [{"x": 4294967295u64, "y": 0, "width": 2, "height": 1}]
            }))
            .unwrap(),
            "application/json",
        );
        let error = provider_result_payload(&overflowing_region, "core.RegionSet").unwrap_err();
        assert!(error.contains("empty or overflows"));
    }

    #[test]
    fn provider_result_payload_rejects_non_png_spatial_bytes() {
        let result =
            rawweave_ai_provider::AiResult::from_bytes("task-jpeg", vec![0; 4], "image/jpeg");

        let error = provider_result_payload(&result, "core.Mask").unwrap_err();
        assert!(error.contains("must use image/png"));
    }

    #[test]
    fn ai_provider_result_becomes_a_durable_image_artifact_with_provider_provenance() {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(Cursor::new(&mut bytes), 1, 1);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[64, 128, 255, 255]).unwrap();
        writer.finish().unwrap();
        let result = rawweave_ai_provider::AiResult::from_bytes("task-1", bytes, "image/png")
            .with_provenance(
                rawweave_ai_provider::TaskProvenance::new("ai-node", 1, "snapshot-hash")
                    .with_provider("fixture-provider"),
            );

        let context = ProviderArtifactContext {
            output_type: "core.Image",
            mask_node_ids: &[],
            model: None,
        };
        let artifact = provider_checkpoint_artifact(
            &result,
            "snapshot-hash",
            BTreeMap::from([("source".to_owned(), "source-hash".to_owned())]),
            1,
            7,
            &context,
        )
        .unwrap();

        assert!(matches!(artifact.payload, CheckpointPayload::Image(_)));
        assert_eq!(artifact.dependency_hash, "snapshot-hash");
        assert_eq!(artifact.provenance.node_version, 1);
        assert_eq!(
            artifact
                .provenance
                .external_tool
                .as_ref()
                .map(|tool| tool.id.as_str()),
            Some("fixture-provider")
        );
        assert_eq!(artifact.generation.generation_revision, 7);
        let store = ArtifactStore::memory();
        store.put(&artifact).unwrap();
        assert!(store.get(artifact.id()).unwrap().is_some());
    }

    #[test]
    fn ai_checkpoint_request_routes_node_parameters_and_connected_image_to_provider() {
        let mut editor = EditorCore::default();
        editor.add_node("input", "core.image-input").unwrap();
        editor.add_node("ai", "ai.img2img").unwrap();
        editor.connect("input", "image", "ai", "image").unwrap();
        editor
            .set_node_parameter(
                "ai",
                "provider_id",
                ParameterValue::String("fixture".to_owned()),
            )
            .unwrap();
        editor
            .set_node_parameter(
                "ai",
                "workflow_id",
                ParameterValue::String("edit-v1".to_owned()),
            )
            .unwrap();
        editor
            .set_node_parameter(
                "ai",
                "workflow_definition",
                ParameterValue::String(r#"{"nodes":{}}"#.to_owned()),
            )
            .unwrap();
        editor
            .set_node_parameter(
                "ai",
                "prompt",
                ParameterValue::String("make it blue".to_owned()),
            )
            .unwrap();
        let source = Image::new(1, 1).unwrap();

        let context = EvaluationContext::with_source_image(source.clone());
        let request =
            build_ai_submit_request(&editor, "ai", "image", &context, "dependency-hash").unwrap();

        assert_eq!(request.provenance.provider_id.as_deref(), Some("fixture"));
        assert_eq!(request.workflow.id, "edit-v1");
        assert_eq!(
            request.parameters["prompt"],
            serde_json::json!("make it blue")
        );
        assert!(matches!(
            request.inputs.get("image"),
            Some(rawweave_ai_provider::AiInput::Image(_))
        ));
    }

    fn configured_ai_editor(type_id: &str) -> EditorCore {
        let mut editor = EditorCore::default();
        editor.add_node("input", "core.image-input").unwrap();
        editor.add_node("invert", "core.invert").unwrap();
        editor
            .add_node("mask", "core.mask-linear-gradient")
            .unwrap();
        editor.add_node("ai", type_id).unwrap();
        editor.connect("input", "image", "invert", "image").unwrap();
        editor.connect("invert", "image", "ai", "image").unwrap();
        editor.connect("input", "image", "mask", "image").unwrap();
        editor.connect("mask", "mask", "ai", "mask").unwrap();
        for (parameter, value) in [
            ("provider_id", ParameterValue::String("fixture".to_owned())),
            (
                "workflow_id",
                ParameterValue::String("workflow-id".to_owned()),
            ),
            (
                "workflow_definition",
                ParameterValue::String(
                    r#"{"version":"7","model":"model-a","nodes":{}}"#.to_owned(),
                ),
            ),
        ] {
            editor.set_node_parameter("ai", parameter, value).unwrap();
        }
        editor
    }

    #[test]
    fn ai_checkpoint_request_evaluates_connected_image_and_mask_under_context() {
        let editor = configured_ai_editor("ai.inpaint");
        let source =
            Image::from_pixels(2, 1, vec![[0.2, 0.3, 0.4, 1.0], [0.4, 0.5, 0.6, 1.0]]).unwrap();
        let context = EvaluationContext::with_source_image(source.clone());

        let request =
            build_ai_submit_request(&editor, "ai", "image", &context, "dependency").unwrap();
        let rawweave_ai_provider::AiInput::Image(image) = request.inputs.get("image").unwrap()
        else {
            panic!("connected image input must be sent as an image");
        };
        let decoded = image::load_from_memory(&image.bytes).unwrap().to_rgba32f();
        assert_eq!(decoded.get_pixel(0, 0).0[0], 0.8);
        assert_ne!(image.bytes, preview::encode_png(&source).unwrap());

        let rawweave_ai_provider::AiInput::Mask(mask) = request.inputs.get("mask").unwrap() else {
            panic!("connected mask input must be sent as a mask");
        };
        let decoded_mask = image::load_from_memory(&mask.bytes).unwrap().to_rgba32f();
        assert_eq!(decoded_mask.get_pixel(0, 0).0, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(decoded_mask.get_pixel(1, 0).0, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn ai_checkpoint_request_does_not_fall_back_to_source_for_unconnected_required_image() {
        let mut editor = EditorCore::default();
        editor.add_node("ai", "ai.img2img").unwrap();
        for (parameter, value) in [
            ("provider_id", ParameterValue::String("fixture".to_owned())),
            (
                "workflow_id",
                ParameterValue::String("workflow-id".to_owned()),
            ),
            (
                "workflow_definition",
                ParameterValue::String(r#"{"version":"1","nodes":{}}"#.to_owned()),
            ),
        ] {
            editor.set_node_parameter("ai", parameter, value).unwrap();
        }
        let source = SourceAsset::Ordinary(Image::new(1, 1).unwrap());
        let context = checkpoint_context(Some(&source));

        let error =
            build_ai_submit_request(&editor, "ai", "image", &context, "dependency").unwrap_err();
        assert!(error.contains("required AI input 'image' is not connected"));
    }

    fn rgba_png(width: u32, height: u32, bytes: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        let mut encoder = Encoder::new(Cursor::new(&mut encoded), width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(bytes).unwrap();
        writer.finish().unwrap();
        encoded
    }

    #[test]
    fn provider_result_payload_uses_requested_mask_type_and_deterministic_alpha_or_grayscale() {
        let result = rawweave_ai_provider::AiResult::from_bytes(
            "task-mask",
            rgba_png(2, 1, &[255, 255, 255, 128, 0, 0, 0, 255]),
            "image/png",
        );

        let payload = provider_result_payload(&result, "core.Mask").unwrap();
        let CheckpointPayload::Mask(mask) = payload else {
            panic!("core.Mask output must become a mask checkpoint payload");
        };
        assert_eq!(mask.values(), vec![128.0 / 255.0, 0.0]);
    }

    #[test]
    fn provider_result_payload_rejects_unknown_requested_output_type() {
        let result = rawweave_ai_provider::AiResult::from_bytes(
            "task-incompatible",
            rgba_png(1, 1, &[0, 0, 0, 255]),
            "image/png",
        );

        let error = provider_result_payload(&result, "core.Unknown").unwrap_err();
        assert!(error.contains("unsupported AI checkpoint output type 'core.Unknown'"));
    }

    #[test]
    fn provider_checkpoint_provenance_preserves_workflow_identity_hash_model_and_mask_nodes() {
        let mut provenance = rawweave_ai_provider::TaskProvenance::new("ai", 1, "dependency")
            .with_provider("provider-a")
            .with_workflow_hash("workflow-content-hash");
        provenance.workflow_id = "workflow-id".to_owned();
        provenance.workflow_version = "workflow-version".to_owned();
        let result = rawweave_ai_provider::AiResult::from_bytes(
            "task-provenance",
            rgba_png(1, 1, &[1, 2, 3, 255]),
            "image/png",
        )
        .with_provenance(provenance);

        let context = ProviderArtifactContext {
            output_type: "core.Image",
            mask_node_ids: &["mask-node".to_owned()],
            model: Some("model-a"),
        };
        let artifact =
            provider_checkpoint_artifact(&result, "dependency", BTreeMap::new(), 1, 2, &context)
                .unwrap();
        let tool = artifact.provenance.external_tool.unwrap();
        assert_eq!(tool.id, "provider-a");
        assert_eq!(tool.version, "unknown");
        assert_eq!(tool.model.as_deref(), Some("model-a"));
        assert_eq!(tool.parameters["task_id"], "task-provenance");
        assert_eq!(tool.parameters["workflow_id"], "workflow-id");
        assert_eq!(tool.parameters["workflow_version"], "workflow-version");
        assert_eq!(tool.parameters["workflow_hash"], "workflow-content-hash");
        assert_eq!(tool.parameters["mask_node_ids"], "[\"mask-node\"]");
    }
}
