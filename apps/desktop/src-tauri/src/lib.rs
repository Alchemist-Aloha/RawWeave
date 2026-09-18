mod preview;

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
    Raw {
        bytes: Arc<Vec<u8>>,
        path: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceKind {
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
}

fn lock_editor(editor: &Arc<Mutex<EditorCore>>) -> Result<MutexGuard<'_, EditorCore>, String> {
    editor
        .lock()
        .map_err(|_| "editor state is unavailable".to_owned())
}

const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "dcr", "dng", "erf", "kdc", "mrw", "nef", "nrw",
    "orf", "pef", "raf", "raw", "rw2", "rwl", "srw", "x3f",
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

fn read_raw_file(path: &Path, limits: RawDecodeLimits) -> Result<Vec<u8>, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| format!("could not inspect RAW '{}': {error}", path.display()))?
        .len();
    let size = usize::try_from(size)
        .map_err(|_| format!("RAW '{}' exceeds the configured input limit", path.display()))?;
    if size > limits.max_input_bytes {
        return Err(format!(
            "RAW input is too large: {size} bytes exceeds limit {}",
            limits.max_input_bytes
        ));
    }
    std::fs::read(path).map_err(|error| format!("could not read RAW '{}': {error}", path.display()))
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
    if matches!(source, SourceAsset::Raw { .. }) {
        build_raw_workflow(editor)?;
        metadata.revision = editor.graph().revision();
    }
    Ok((source, metadata))
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
        editor.add_node(node_id, type_id).map_err(|error| error.to_string())?;
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
        (
            "highlight-reconstruction",
            "mosaic",
            "demosaic",
            "mosaic",
        ),
        ("demosaic", "scene", "camera-transform", "scene"),
        ("raw-decode", "camera_profile", "camera-transform", "camera_profile"),
        ("camera-transform", "scene", "lens-correction", "scene"),
        ("raw-decode", "lens_profile", "lens-correction", "lens_profile"),
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
    lock_editor(&state.editor)?
        .load_workflow(&workflow)
        .map_err(|error| error.to_string())
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
    let (source, metadata) = open_image_file_with_decoder(Path::new(path), &RawloaderDecoder::default())?;
    match source {
        SourceAsset::Ordinary(image) => Ok((image, metadata)),
        SourceAsset::Raw { .. } => Err("RAW input must be opened through the RAW source path".to_owned()),
    }
}

#[tauri::command]
fn open_image(
    state: State<'_, AppState>,
    path: String,
) -> Result<preview::OpenImageMetadata, String> {
    let (source, metadata) = {
        let mut editor = lock_editor(&state.editor)?;
        open_image_with_decoder(
            Path::new(&path),
            &RawloaderDecoder::default(),
            &mut editor,
        )?
    };
    *state
        .source_image
        .lock()
        .map_err(|_| "source image state is unavailable".to_owned())? = Some(source);
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
        .register_uri_scheme_protocol("rawweave-preview", move |_ctx, request| {
            protocol_preview.response(&request)
        })
        .manage(AppState {
            editor: Arc::new(Mutex::new(EditorCore::default())),
            preview,
            source_image: Mutex::new(None),
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
    use rawweave_raw::{DeterministicCorpus, DeterministicDecoder};
    use png::{BitDepth, ColorType, Encoder};
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
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                128, 128, 128, 255, 0, 0, 0, 255,
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
        let path = std::env::temp_dir().join(format!(
            "rawweave-open-raw-{}.dng",
            std::process::id()
        ));
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
        let path = std::env::temp_dir().join(format!(
            "rawweave-open-preview-{}.dng",
            std::process::id()
        ));
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

        let rendered = preview::render_preview(
            &manager,
            &editor,
            &current_editor,
            Some(source),
            request,
        )
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
}
