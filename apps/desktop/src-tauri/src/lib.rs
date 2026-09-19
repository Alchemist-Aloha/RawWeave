mod browser;
mod preview;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rawweave_core::NodeId;
use rawweave_graph::{
    DependencyReport, DependencyStatus, Graph, NodePackManifest, SubgraphDependency,
    WorkflowDefinition, WorkflowMetadata, WorkflowPort, WorkflowPortDirection,
};
use rawweave_image::Image;
use rawweave_node_api::{NodeDescriptor, ParameterValue};
use rawweave_project::{built_in_node_pack_manifests, EditorCore};
use rawweave_raw::{RawDecodeLimits, RawDecoder, RawFrame, RawloaderDecoder};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Debug)]
pub(crate) enum SourceAsset {
    Ordinary(Image),
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
pub struct AppState {
    pub editor: Arc<Mutex<EditorCore>>,
    pub preview: Arc<preview::PreviewManager>,
    pub(crate) source_image: Mutex<Option<SourceAsset>>,
    source_selection: Mutex<SourceSelectionIntent>,
    blueprint: Mutex<Option<WorkflowDefinition>>,
    blueprint_stack: Mutex<Vec<WorkflowDefinition>>,
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
) -> Result<WorkflowDefinition, String> {
    editor
        .load_blueprint(serialized.as_bytes())
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

pub(crate) fn build_ordinary_workflow(editor: &mut EditorCore) -> Result<(), String> {
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
        .add_node("input", "core.image-input")
        .map_err(|error| error.to_string())?;
    editor
        .add_node("output", "core.output")
        .map_err(|error| error.to_string())?;
    editor
        .connect("input", "image", "output", "image")
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn build_raw_workflow(editor: &mut EditorCore) -> Result<(), String> {
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

    for (node_id, type_id) in [
        ("raw-decode", "raw.decode"),
        ("black-level", "raw.black-level"),
        ("white-balance", "raw.white-balance"),
        ("highlight-reconstruction", "raw.highlight-reconstruction"),
        ("demosaic", "raw.demosaic"),
        ("camera-transform", "raw.camera-transform"),
        ("lens-correction", "raw.lens-correction"),
        ("display-transform", "raw.display-transform"),
    ] {
        editor
            .add_node(node_id, type_id)
            .map_err(|error| error.to_string())?;
    }
    for (from_node, from_port, to_node, to_port) in [
        ("raw-decode", "frame", "black-level", "frame"),
        ("black-level", "mosaic", "white-balance", "mosaic"),
        (
            "white-balance",
            "mosaic",
            "highlight-reconstruction",
            "mosaic",
        ),
        ("highlight-reconstruction", "mosaic", "demosaic", "mosaic"),
        ("demosaic", "scene", "camera-transform", "scene"),
        (
            "raw-decode",
            "camera_profile",
            "camera-transform",
            "camera_profile",
        ),
        ("camera-transform", "scene", "lens-correction", "scene"),
        (
            "raw-decode",
            "lens_profile",
            "lens-correction",
            "lens_profile",
        ),
        ("lens-correction", "scene", "display-transform", "scene"),
    ] {
        editor
            .connect(from_node, from_port, to_node, to_port)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn node_descriptors(state: State<'_, AppState>) -> Result<Vec<NodeDescriptor>, String> {
    Ok(lock_editor(&state.editor)?.node_descriptors())
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
    let blueprint = import_blueprint_definition(&editor, serialized)?;
    let dto = workflow_definition_dto(&blueprint);
    store_blueprint(state, blueprint)?;
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

fn load_workflow_state(state: &AppState, workflow: &str) -> Result<(), String> {
    lock_editor(&state.editor)?
        .load_workflow(workflow)
        .map_err(|error| error.to_string())?;
    clear_blueprint(state)?;
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
async fn request_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    request: preview::PreviewRequest,
) -> Result<preview::PreviewMetadata, String> {
    let editor = lock_editor(&state.editor)?.clone();
    let current_editor = Arc::clone(&state.editor);
    let source_image = state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())?
        .clone();
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
        SourceAsset::Raw { .. } => {
            Err("RAW input must be opened through the RAW source path".to_owned())
        }
    }
}

#[tauri::command]
fn open_image(
    state: State<'_, AppState>,
    path: String,
) -> Result<preview::OpenImageMetadata, String> {
    let (metadata, _) = open_image_state(&state, Path::new(&path), &RawloaderDecoder::default())?;
    Ok(metadata)
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

pub fn run() {
    let preview = Arc::new(preview::PreviewManager::default());
    let protocol_preview = Arc::clone(&preview);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("rawweave-preview", move |_ctx, request| {
            protocol_preview.response(&request)
        })
        .manage(AppState {
            editor: Arc::new(Mutex::new(EditorCore::default())),
            preview,
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
        })
        .invoke_handler(tauri::generate_handler![
            node_descriptors,
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
            open_image,
            request_preview,
            cancel_preview,
            release_preview,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running RawWeave");
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType, Encoder};
    use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};
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
            preview: Arc::new(preview::PreviewManager::default()),
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
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
            preview: Arc::new(preview::PreviewManager::default()),
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
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
            preview: Arc::clone(&preview),
            source_image: Mutex::new(Some(SourceAsset::Ordinary(Image::new(1, 1).unwrap()))),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
            blueprint: Mutex::new(None),
            blueprint_stack: Mutex::new(Vec::new()),
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
        };
        let error =
            preview::render_preview(&preview, &editor, &current_editor, None, request).unwrap_err();
        assert!(error.contains("source image unavailable"));
    }
}
