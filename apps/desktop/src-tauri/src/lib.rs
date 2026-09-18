mod preview;

use std::sync::{Arc, Mutex, MutexGuard};

use rawweave_image::Image;
use rawweave_node_api::{NodeDescriptor, ParameterValue};
use rawweave_project::EditorCore;
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct AppState {
    pub editor: Arc<Mutex<EditorCore>>,
    pub preview: Arc<preview::PreviewManager>,
    pub source_image: Mutex<Option<Image>>,
}

fn lock_editor(editor: &Arc<Mutex<EditorCore>>) -> Result<MutexGuard<'_, EditorCore>, String> {
    editor
        .lock()
        .map_err(|_| "editor state is unavailable".to_owned())
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

#[tauri::command]
fn cancel_preview(state: State<'_, AppState>, request_id: String) -> Result<(), String> {
    state.preview.cancel(&request_id);
    Ok(())
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
            request_preview,
            cancel_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RawWeave");
}
