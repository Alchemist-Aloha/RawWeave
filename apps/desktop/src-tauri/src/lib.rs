mod preview;

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rawweave_image::Image;
use rawweave_node_api::{NodeDescriptor, ParameterValue};
use rawweave_project::EditorCore;
use rawweave_raw::{RawDecodeLimits, RawDecoder, RawFrame, RawloaderDecoder};
use serde::Serialize;
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

#[derive(Default)]
pub struct AppState {
    pub editor: Arc<Mutex<EditorCore>>,
    pub preview: Arc<preview::PreviewManager>,
    pub(crate) source_image: Mutex<Option<SourceAsset>>,
    source_selection: Mutex<SourceSelectionIntent>,
}

fn lock_editor(editor: &Arc<Mutex<EditorCore>>) -> Result<MutexGuard<'_, EditorCore>, String> {
    editor
        .lock()
        .map_err(|_| "editor state is unavailable".to_owned())
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

fn read_raw_file(path: &Path, limits: RawDecodeLimits) -> Result<Vec<u8>, String> {
    let file = File::open(path)
        .map_err(|error| format!("could not open RAW '{}': {error}", path.display()))?;
    read_bounded_raw(file, &path.display().to_string(), limits.max_input_bytes)
}

fn raw_metadata(frame: &RawFrame) -> OpenMetadataSummary {
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

fn rebuild_workflow_for_source(source: &SourceAsset, editor: &mut EditorCore) -> Result<(), String> {
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
        for port in node.descriptor.inputs.iter().chain(node.descriptor.outputs.iter()) {
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
        (false, false) => {
            Err("cannot determine whether loaded workflow expects a RAW or ordinary image source".to_owned())
        }
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
        WorkflowKind::Ordinary => Err("ordinary image workflow requires an ordinary image source".to_owned()),
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
    lock_editor(&state.editor)?
        .add_node(&node_id, &type_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_node(state: State<'_, AppState>, node_id: String) -> Result<(), String> {
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
    lock_editor(&state.editor)?
        .set_node_parameter(&node_id, &parameter_id, value)
        .map_err(|error| error.to_string())
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
        })
        .invoke_handler(tauri::generate_handler![
            node_descriptors,
            add_node,
            remove_node,
            connect_nodes,
            disconnect_nodes,
            set_node_parameter,
            save_workflow,
            load_workflow,
            open_image,
            request_preview,
            cancel_preview,
            release_preview,
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
        let path = std::env::temp_dir().join(format!(
            "rawweave-reattach-raw-{}.dng",
            std::process::id()
        ));
        std::fs::write(&path, b"deterministic raw fixture").unwrap();
        let decoder = DeterministicDecoder::new(DeterministicCorpus::bayer_12_bit());

        let mut loaded_editor = EditorCore::new_with_raw_decoder(decoder.clone());
        build_raw_workflow(&mut loaded_editor).unwrap();
        loaded_editor
            .set_node_parameter(
                "white-balance",
                "red_gain",
                ParameterValue::Float(1.75),
            )
            .unwrap();
        loaded_editor.add_node("custom", "raw.white-balance").unwrap();
        let workflow = loaded_editor.save_workflow().unwrap();
        let state = AppState {
            editor: Arc::new(Mutex::new(EditorCore::new_with_raw_decoder(decoder.clone()))),
            preview: Arc::new(preview::PreviewManager::default()),
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
        };

        load_workflow_state(&state, &workflow).unwrap();
        let (metadata, source) = open_image_state(&state, &path, &decoder).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            state.editor.lock().unwrap().save_workflow().unwrap(),
            workflow
        );
        assert_eq!(metadata.revision, state.editor.lock().unwrap().graph().revision());
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
        let rendered = preview::render_preview(
            &state.preview,
            &editor,
            &current_editor,
            source,
            request,
        )
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
            editor: Arc::new(Mutex::new(EditorCore::new_with_raw_decoder(decoder.clone()))),
            preview: Arc::new(preview::PreviewManager::default()),
            source_image: Mutex::new(None),
            source_selection: Mutex::new(SourceSelectionIntent::default()),
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
