use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use png::{BitDepth, ColorType, Encoder};
use rawweave_image::{Dimensions, Image, Region};
use rawweave_node_api::{EvaluationContext, Value};
use rawweave_project::EditorCore;
use rawweave_rendering::{PreviewQuality, TileCoord, TileRequest};
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

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRegionRequest {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl From<PreviewRegionRequest> for Region {
    fn from(value: PreviewRegionRequest) -> Self {
        Self::new(value.x, value.y, value.width, value.height)
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTileRequest {
    pub x: u32,
    pub y: u32,
}

impl From<PreviewTileRequest> for TileCoord {
    fn from(value: PreviewTileRequest) -> Self {
        Self::new(value.x, value.y)
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
    pub region: PreviewRegionRequest,
    pub tile: PreviewTileRequest,
    pub mip: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMetadata {
    pub request_id: String,
    pub revision: u64,
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub full_width: u32,
    pub full_height: u32,
    pub mime_type: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenImageMetadata {
    pub width: u32,
    pub height: u32,
    pub revision: u64,
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

pub struct PreviewStore {
    previews: Mutex<HashMap<String, StoredPreview>>,
    order: Mutex<VecDeque<String>>,
    max_entries: usize,
    max_bytes: usize,
    bytes: Mutex<usize>,
}

impl Default for PreviewStore {
    fn default() -> Self {
        Self::with_limits(32, 64 * 1024 * 1024)
    }
}

impl PreviewStore {
    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            previews: Mutex::new(HashMap::new()),
            order: Mutex::new(VecDeque::new()),
            max_entries,
            max_bytes,
            bytes: Mutex::new(0),
        }
    }

    pub fn len(&self) -> usize {
        self.previews.lock().map(|previews| previews.len()).unwrap_or(0)
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.lock().map(|bytes| *bytes).unwrap_or(0)
    }

    pub fn insert(&self, path: String, revision: u64, bytes: Vec<u8>) -> Result<(), String> {
        if self.max_entries == 0 || bytes.len() > self.max_bytes {
            return Err("preview exceeds the configured store budget".to_owned());
        }
        let mut previews = self
            .previews
            .lock()
            .map_err(|_| "preview store is unavailable".to_owned())?;
        let mut order = self
            .order
            .lock()
            .map_err(|_| "preview store is unavailable".to_owned())?;
        let mut total_bytes = self
            .bytes
            .lock()
            .map_err(|_| "preview store is unavailable".to_owned())?;
        if let Some(previous) = previews.remove(&path) {
            *total_bytes = total_bytes.saturating_sub(previous.bytes.len());
        }
        order.retain(|candidate| candidate != &path);
        *total_bytes = total_bytes.saturating_add(bytes.len());
        previews.insert(path.clone(), StoredPreview { revision, bytes });
        order.push_back(path);
        while previews.len() > self.max_entries || *total_bytes > self.max_bytes {
            let Some(oldest) = order.pop_front() else { break };
            if let Some(previous) = previews.remove(&oldest) {
                *total_bytes = total_bytes.saturating_sub(previous.bytes.len());
            }
        }
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        let bytes = self
            .previews
            .lock()
            .ok()
            .and_then(|previews| previews.get(path).map(|preview| preview.bytes.clone()));
        if bytes.is_some() {
            self.touch(path);
        }
        bytes
    }

    pub fn take(&self, path: &str) -> Option<Vec<u8>> {
        let bytes = self
            .previews
            .lock()
            .ok()
            .and_then(|mut previews| previews.remove(path).map(|preview| preview.bytes));
        if let Some(ref bytes) = bytes {
            if let Ok(mut total_bytes) = self.bytes.lock() {
                *total_bytes = total_bytes.saturating_sub(bytes.len());
            }
            if let Ok(mut order) = self.order.lock() {
                order.retain(|candidate| candidate != path);
            }
        }
        bytes
    }

    pub fn revision(&self, path: &str) -> Option<u64> {
        self.previews
            .lock()
            .ok()
            .and_then(|previews| previews.get(path).map(|preview| preview.revision))
    }

    pub fn remove(&self, path: &str) {
        let _ = self.take(path);
    }

    fn touch(&self, path: &str) {
        if let Ok(mut order) = self.order.lock() {
            order.retain(|candidate| candidate != path);
            order.push_back(path.to_owned());
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

    pub fn release(&self, url: &str) -> Result<(), String> {
        let path = url
            .strip_prefix("rawweave-preview://localhost")
            .unwrap_or(url);
        if !path.starts_with("/preview/") {
            return Err("invalid preview URL".to_owned());
        }
        self.store.remove(path);
        Ok(())
    }

    pub fn finish_job(&self, request_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(request_id);
        }
    }

    pub fn response(&self, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        let path = request.uri().path();
        match self.store.take(path) {
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

pub fn decode_image_file(path: impl AsRef<Path>) -> Result<Image, String> {
    let path = path.as_ref();
    let decoded = image::ImageReader::open(path)
        .map_err(|error| format!("could not open image '{}': {error}", path.display()))?
        .decode()
        .map_err(|error| format!("could not decode image '{}': {error}", path.display()))?;
    let rgba = decoded.to_rgba32f();
    Image::from_pixels(
        rgba.width(),
        rgba.height(),
        rgba.pixels().map(|pixel| pixel.0).collect(),
    )
    .map_err(|error| format!("could not create rawweave image: {error}"))
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

pub fn select_preview_image(
    image: &Image,
    requested_region: Region,
    mip: u8,
) -> Result<Image, String> {
    let region = image
        .global_region()
        .intersection(requested_region)
        .ok_or_else(|| "preview region is outside the evaluated image".to_owned())?;
    let scale = 1_u32.checked_shl(u32::from(mip)).unwrap_or(u32::MAX);
    let width = region.width.saturating_add(scale.saturating_sub(1)) / scale;
    let height = region.height.saturating_add(scale.saturating_sub(1)) / scale;
    let mut pixels = Vec::with_capacity((width as usize).saturating_mul(height as usize));
    for y in 0..height {
        for x in 0..width {
            let source_x = region.x + (x.saturating_mul(scale)).min(region.width - 1);
            let source_y = region.y + (y.saturating_mul(scale)).min(region.height - 1);
            pixels.push(
                image
                    .pixel_global(source_x, source_y)
                    .ok_or_else(|| "preview region pixel is unavailable".to_owned())?,
            );
        }
    }
    Image::from_pixels_with_origin(
        Dimensions::new(width, height),
        (region.x, region.y),
        pixels,
        image.pixel_format(),
        image.color_metadata(),
    )
    .map_err(|error| format!("could not create preview region: {error}"))
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
        let full_frame = editor
            .evaluate(
                &request.node_id,
                &request.output_port,
                EvaluationContext::with_source_image(source_image.clone()),
            )
            .map_err(|error| format!("preview full-frame evaluation failed: {error}"))?;
        let Value::Image(full_frame) = full_frame else {
            return Err(format!(
                "preview output '{}:{}' is not an image",
                request.node_id, request.output_port
            ));
        };
        let full_width = full_frame.width();
        let full_height = full_frame.height();
        let full_origin = full_frame.origin();
        let requested_region = Region::new(
            full_origin
                .0
                .checked_add(request.region.x)
                .ok_or_else(|| "preview region x overflowed the output origin".to_owned())?,
            full_origin
                .1
                .checked_add(request.region.y)
                .ok_or_else(|| "preview region y overflowed the output origin".to_owned())?,
            request.region.width,
            request.region.height,
        );
        if manager.is_cancelled(&request.request_id) {
            return Err("preview cancelled".to_owned());
        }
        let context = EvaluationContext::with_source_image(source_image).with_tile_request(
            TileRequest::new(
                requested_region,
                request.tile.into(),
                request.mip,
                request.quality.into(),
            ),
        );
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
        let image = select_preview_image(&image, requested_region, request.mip)?;
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
            full_width,
            full_height,
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
    fn evicts_least_recently_used_previews_by_count_and_bytes() {
        let store = PreviewStore::with_limits(2, 5);
        store.insert("/preview/a.png".to_owned(), 1, vec![1, 2, 3]).unwrap();
        store.insert("/preview/b.png".to_owned(), 1, vec![4, 5]).unwrap();
        assert_eq!(store.get("/preview/a.png"), Some(vec![1, 2, 3]));

        store.insert("/preview/c.png".to_owned(), 1, vec![6, 7]).unwrap();

        assert!(store.get("/preview/b.png").is_none());
        assert_eq!(store.get("/preview/a.png"), Some(vec![1, 2, 3]));
        assert_eq!(store.get("/preview/c.png"), Some(vec![6, 7]));
        assert_eq!(store.len(), 2);
        assert_eq!(store.byte_len(), 5);
    }

    #[test]
    fn taking_a_preview_releases_its_bytes_without_cloning_them_for_protocol_reads() {
        let store = PreviewStore::default();
        store
            .insert("/preview/request.png".to_owned(), 12, vec![1, 2, 3])
            .unwrap();

        assert_eq!(store.take("/preview/request.png"), Some(vec![1, 2, 3]));
        assert!(store.get("/preview/request.png").is_none());
        assert_eq!(store.byte_len(), 0);
    }

    #[test]
    fn releasing_a_preview_url_removes_its_stored_bytes() {
        let manager = PreviewManager::default();
        let path = preview_path("request");
        manager
            .store
            .insert(path.clone(), 12, vec![1, 2, 3])
            .unwrap();

        manager.release(&preview_url("request")).unwrap();

        assert!(manager.store.get(&path).is_none());
        assert_eq!(manager.store.byte_len(), 0);
        assert!(manager.release("not-a-preview-url").is_err());
    }

    #[test]
    fn selects_the_requested_region_and_mip_for_preview_encoding() {
        let image = Image::new(4, 3).unwrap();

        let region = select_preview_image(&image, rawweave_image::Region::new(1, 1, 2, 2), 1)
            .unwrap();

        assert_eq!(region.width(), 1);
        assert_eq!(region.height(), 1);
        assert_eq!(region.origin(), (1, 1));
    }

    #[test]
    fn opens_a_generated_png_and_renders_it_through_the_image_graph() {
        let path = std::env::temp_dir().join(format!(
            "rawweave-preview-open-render-{}.png",
            std::process::id()
        ));
        let mut png_bytes = Vec::new();
        let mut encoder = png::Encoder::new(Cursor::new(&mut png_bytes), 2, 2);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ])
            .unwrap();
        writer.finish().unwrap();
        std::fs::write(&path, png_bytes).unwrap();

        let source = decode_image_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((source.width(), source.height()), (2, 2));

        let mut editor = EditorCore::default();
        editor.add_node("input", "core.image-input").unwrap();
        editor.add_node("output", "core.output").unwrap();
        editor.connect("input", "image", "output", "image").unwrap();
        let current_editor = Arc::new(Mutex::new(editor.clone()));
        let manager = PreviewManager::default();
        let request = PreviewRequest {
            request_id: "open-render".to_owned(),
            revision: editor.graph().revision(),
            node_id: "output".to_owned(),
            output_port: "image".to_owned(),
            quality: PreviewQualityRequest::Preview,
            region: PreviewRegionRequest {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            tile: PreviewTileRequest { x: 0, y: 0 },
            mip: 0,
        };

        let metadata = render_preview(
            &manager,
            &editor,
            &current_editor,
            Some(source),
            request,
        )
        .unwrap();

        assert_eq!((metadata.width, metadata.height), (2, 2));
        assert_eq!((metadata.full_width, metadata.full_height), (2, 2));
        assert!(manager.store.get(&preview_path("open-render")).is_some());
    }

    #[test]
    fn reports_full_logical_dimensions_for_region_limited_intermediate_outputs() {
        let source = Image::new(4, 3).unwrap();
        let manager = PreviewManager::default();

        let cases = [
            ("exposure", "core.exposure", vec![("exposure", 1.0_f32)]),
            (
                "crop",
                "core.crop",
                vec![("x", 1.0), ("y", 1.0), ("width", 2.0), ("height", 2.0)],
            ),
            (
                "resize",
                "core.resize",
                vec![("width", 8.0), ("height", 6.0)],
            ),
        ];

        for (request_id, type_id, parameters) in cases {
            let mut editor = EditorCore::default();
            editor.add_node("input", "core.image-input").unwrap();
            editor.add_node("target", type_id).unwrap();
            editor
                .connect("input", "image", "target", "image")
                .unwrap();
            for (parameter_id, value) in parameters {
                editor
                    .set_node_parameter("target", parameter_id, value.into())
                    .unwrap();
            }
            let revision = editor.graph().revision();
            let request = PreviewRequest {
                request_id: request_id.to_owned(),
                revision,
                node_id: "target".to_owned(),
                output_port: "image".to_owned(),
                quality: PreviewQualityRequest::Preview,
                region: match request_id {
                    "resize" => PreviewRegionRequest {
                        x: 2,
                        y: 1,
                        width: 4,
                        height: 3,
                    },
                    _ => PreviewRegionRequest {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 2,
                    },
                },
                tile: PreviewTileRequest { x: 0, y: 0 },
                mip: 0,
            };
            let current_editor = Arc::new(Mutex::new(editor.clone()));
            let metadata = render_preview(
                &manager,
                &editor,
                &current_editor,
                Some(source.clone()),
                request,
            )
            .unwrap();

            match request_id {
                "exposure" => {
                    assert_eq!((metadata.full_width, metadata.full_height), (4, 3));
                    assert_eq!((metadata.width, metadata.height), (2, 2));
                }
                "crop" => {
                    assert_eq!((metadata.full_width, metadata.full_height), (2, 2));
                    assert_eq!((metadata.width, metadata.height), (2, 2));
                }
                "resize" => {
                    assert_eq!((metadata.full_width, metadata.full_height), (8, 6));
                    assert_eq!((metadata.width, metadata.height), (4, 3));
                }
                _ => unreachable!(),
            }
        }
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
