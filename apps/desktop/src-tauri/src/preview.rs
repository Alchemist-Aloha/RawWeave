use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use png::{BitDepth, ColorType, Encoder};
use rawweave_image::Image;
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::EditorCore;
use rawweave_rendering::PreviewQuality;
use serde::{Deserialize, Serialize};
use tauri::http::{Request, Response};

const PREVIEW_MIME_TYPE: &str = "image/png";

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewQualityRequest {
    Draft,
    Preview,
    Final,
}

impl From<PreviewQualityRequest> for PreviewQuality {
    fn from(value: PreviewQualityRequest) -> Self {
        match value {
            PreviewQualityRequest::Draft => Self::Draft,
            PreviewQualityRequest::Preview => Self::Preview,
            PreviewQualityRequest::Final => Self::Final,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub request_id: String,
    pub revision: u64,
    pub node_id: String,
    pub output_port: String,
    pub quality: PreviewQualityRequest,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMetadata {
    pub request_id: String,
    pub revision: u64,
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub mime_type: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProgressEvent {
    pub request_id: String,
    pub revision: u64,
    pub progress: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewErrorEvent {
    pub request_id: String,
    pub revision: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCancelledEvent {
    pub request_id: String,
    pub revision: u64,
}

#[derive(Clone, Debug)]
struct StoredPreview {
    revision: u64,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub struct PreviewStore {
    previews: Mutex<HashMap<String, StoredPreview>>,
}

impl PreviewStore {
    pub fn insert(&self, path: String, revision: u64, bytes: Vec<u8>) -> Result<(), String> {
        self.previews
            .lock()
            .map_err(|_| "preview store is unavailable".to_owned())?
            .insert(path, StoredPreview { revision, bytes });
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.previews
            .lock()
            .ok()
            .and_then(|previews| previews.get(path).map(|preview| preview.bytes.clone()))
    }

    pub fn revision(&self, path: &str) -> Option<u64> {
        self.previews
            .lock()
            .ok()
            .and_then(|previews| previews.get(path).map(|preview| preview.revision))
    }

    pub fn remove(&self, path: &str) {
        if let Ok(mut previews) = self.previews.lock() {
            previews.remove(path);
        }
    }
}

#[derive(Default)]
pub struct PreviewManager {
    pub store: PreviewStore,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl PreviewManager {
    pub fn begin(&self, request_id: &str) -> Arc<AtomicBool> {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(token) = jobs.get(request_id) {
                return Arc::clone(token);
            }
            let token = Arc::new(AtomicBool::new(false));
            jobs.insert(request_id.to_owned(), Arc::clone(&token));
            return token;
        }
        Arc::new(AtomicBool::new(false))
    }

    pub fn is_cancelled(&self, request_id: &str) -> bool {
        self.jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(request_id).cloned())
            .is_some_and(|token| token.load(Ordering::Acquire))
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let cancelled = self
            .jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(request_id).cloned())
            .map(|token| {
                token.store(true, Ordering::Release);
                true
            })
            .unwrap_or(false);
        self.store.remove(&preview_path(request_id));
        cancelled
    }

    pub fn discard(&self, request_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(request_id);
        }
        self.store.remove(&preview_path(request_id));
    }

    pub fn finish_job(&self, request_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(request_id);
        }
    }

    pub fn response(&self, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        let path = request.uri().path();
        match self.store.get(path) {
            Some(bytes) => Response::builder()
                .status(200)
                .header("Content-Type", PREVIEW_MIME_TYPE)
                .header("Cache-Control", "no-store")
                .body(bytes)
                .expect("valid preview response"),
            None => Response::builder()
                .status(404)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(b"preview not found".to_vec())
                .expect("valid missing preview response"),
        }
    }
}

pub fn preview_path(request_id: &str) -> String {
    format!("/preview/{request_id}.png")
}

pub fn preview_url(request_id: &str) -> String {
    format!("rawweave-preview://localhost{}", preview_path(request_id))
}

pub fn encode_png(image: &Image) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(Cursor::new(&mut bytes), image.width(), image.height());
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("could not encode preview PNG: {error}"))?;
    let rgba = image
        .pixels()
        .iter()
        .flat_map(|pixel| pixel.map(channel_to_byte))
        .collect::<Vec<_>>();
    writer
        .write_image_data(&rgba)
        .map_err(|error| format!("could not write preview PNG: {error}"))?;
    writer
        .finish()
        .map_err(|error| format!("could not finish preview PNG: {error}"))?;
    Ok(bytes)
}

fn channel_to_byte(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn render_preview(
    manager: &PreviewManager,
    editor: &EditorCore,
    current_editor: &Arc<Mutex<EditorCore>>,
    source_image: Option<Image>,
    request: PreviewRequest,
) -> Result<PreviewMetadata, String> {
    manager.begin(&request.request_id);
    let result = (|| {
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        let current_revision = current_editor
            .lock()
            .map_err(|_| "editor state is unavailable".to_owned())?
            .graph()
            .revision();
        if request.revision != current_revision {
            return Err(format!(
                "stale preview request: graph revision is {current_revision}, requested {}",
                request.revision
            ));
        }
        let source_image = source_image.ok_or_else(|| {
            "preview source image unavailable; open an image before rendering".to_owned()
        })?;
        let context =
            EvaluationContext::with_source_image(source_image).with_quality(request.quality.into());
        let value = editor
            .evaluate(&request.node_id, &request.output_port, context)
            .map_err(|error| format!("preview evaluation failed: {error}"))?;
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        let Value::Image(image) = value else {
            return Err(format!(
                "preview output '{}:{}' is not an image",
                request.node_id, request.output_port
            ));
        };
        let latest_revision = current_editor
            .lock()
            .map_err(|_| "editor state is unavailable".to_owned())?
            .graph()
            .revision();
        if request.revision != latest_revision {
            return Err(format!(
                "stale preview result: graph revision is {latest_revision}, requested {}",
                request.revision
            ));
        }
        let bytes = encode_png(&image)?;
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        manager
            .store
            .insert(preview_path(&request.request_id), request.revision, bytes)?;
        if manager.is_cancelled(&request.request_id) {
            manager.store.remove(&preview_path(&request.request_id));
            return Err("preview cancelled".to_owned());
        }
        Ok(PreviewMetadata {
            request_id: request.request_id.clone(),
            revision: request.revision,
            url: preview_url(&request.request_id),
            width: image.width(),
            height: image.height(),
            mime_type: PREVIEW_MIME_TYPE,
        })
    })();
    match result {
        Ok(metadata) => {
            manager.finish_job(&request.request_id);
            Ok(metadata)
        }
        Err(error) => {
            manager.discard(&request.request_id);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawweave_image::Image;

    #[test]
    fn encodes_float_rgba_pixels_as_a_browser_png() {
        let image = Image::from_pixels(
            2,
            1,
            vec![[0.0, 0.5, 1.0, 1.0], [1.2, -0.1, f32::NAN, 0.25]],
        )
        .unwrap();
        let bytes = encode_png(&image).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut output = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut output).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(&output[..8], &[0, 128, 255, 255, 255, 0, 0, 64]);
    }

    #[test]
    fn stores_preview_bytes_with_the_graph_revision() {
        let store = PreviewStore::default();
        store
            .insert("/preview/request.png".to_owned(), 12, vec![1, 2, 3])
            .unwrap();
        assert_eq!(store.revision("/preview/request.png"), Some(12));
        assert_eq!(store.get("/preview/request.png"), Some(vec![1, 2, 3]));
        store.remove("/preview/request.png");
        assert!(store.get("/preview/request.png").is_none());
    }

    #[test]
    fn cancellation_marks_the_active_job_and_removes_its_stored_bytes() {
        let manager = PreviewManager::default();
        let token = manager.begin("request");
        manager
            .store
            .insert(preview_path("request"), 1, vec![1])
            .unwrap();

        assert!(manager.cancel("request"));
        assert!(token.load(Ordering::Acquire));
        assert!(manager.is_cancelled("request"));
        assert!(Arc::ptr_eq(&token, &manager.begin("request")));
        assert!(manager.store.get(&preview_path("request")).is_none());
        manager.finish_job("request");
        assert!(!manager.is_cancelled("request"));
    }

    #[test]
    fn serves_stored_png_bytes_over_the_preview_protocol() {
        let manager = PreviewManager::default();
        let path = preview_path("request");
        let bytes = vec![1, 2, 3];
        manager
            .store
            .insert(path.clone(), 12, bytes.clone())
            .unwrap();

        let request = Request::builder().uri(path).body(Vec::new()).unwrap();
        let response = manager.response(&request);

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            PREVIEW_MIME_TYPE
        );
        assert_eq!(response.body(), &bytes);
    }
}
